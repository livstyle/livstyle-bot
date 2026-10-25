//! SQLite persistence (ADR-011). One database per livstyle profile, WAL mode,
//! schema versioned through `PRAGMA user_version`.
//!
//! Domain objects are stored as JSON documents next to the columns needed for
//! querying and concurrency control, so the schema does not have to change
//! every time a domain struct gains a field.

mod schema;

use std::path::Path;
use std::time::{SystemTime, UNIX_EPOCH};

use rusqlite::{Connection, OptionalExtension, Transaction, params};
use serde::Serialize;
use serde::de::DeserializeOwned;
use serde_json::Value;
use sha2::{Digest, Sha256};
use thiserror::Error;

pub use schema::SCHEMA_VERSION;

#[derive(Debug, Error)]
pub enum StoreError {
    #[error("sqlite: {0}")]
    Sqlite(#[from] rusqlite::Error),
    #[error("json: {0}")]
    Json(#[from] serde_json::Error),
    #[error("{kind} {id:?} not found")]
    NotFound { kind: &'static str, id: String },
    #[error("{kind} {id:?} already exists")]
    Exists { kind: &'static str, id: String },
    /// Optimistic concurrency failure: someone else updated the row first.
    #[error("{kind} {id:?} is at version {actual}, expected {expected}")]
    Conflict {
        kind: &'static str,
        id: String,
        expected: u64,
        actual: u64,
    },
    #[error("database schema version {found} is newer than this build ({SCHEMA_VERSION})")]
    FutureSchema { found: u32 },
    #[error("audit chain broken at seq {seq}")]
    AuditChainBroken { seq: u64 },
    #[error("operation {id:?} is {state:?}, cannot move to {next:?}")]
    BadOperationState {
        id: String,
        state: String,
        next: String,
    },
}

pub type Result<T> = std::result::Result<T, StoreError>;

fn now_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// A versioned JSON document.
#[derive(Debug, Clone, PartialEq)]
pub struct Versioned<T> {
    pub version: u64,
    pub value: T,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Event {
    pub seq: u64,
    pub task_id: String,
    pub kind: String,
    pub payload: Value,
    pub at_ms: i64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct AuditEntry {
    pub seq: u64,
    pub at_ms: i64,
    pub actor: String,
    pub action: String,
    pub subject: String,
    pub payload: Value,
    pub hash: String,
}

/// Lifecycle of a hardware operation record. `Dispatched` is the only state
/// in which the device may have been touched without a recorded outcome.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum OperationState {
    Prepared,
    Authorized,
    Dispatched,
    Succeeded,
    Failed,
    Cancelled,
    /// The device may or may not have been modified. Never auto-retried.
    OutcomeUnknown,
}

impl OperationState {
    fn as_str(self) -> &'static str {
        match self {
            Self::Prepared => "prepared",
            Self::Authorized => "authorized",
            Self::Dispatched => "dispatched",
            Self::Succeeded => "succeeded",
            Self::Failed => "failed",
            Self::Cancelled => "cancelled",
            Self::OutcomeUnknown => "outcome_unknown",
        }
    }

    fn parse(s: &str) -> Self {
        match s {
            "prepared" => Self::Prepared,
            "authorized" => Self::Authorized,
            "dispatched" => Self::Dispatched,
            "succeeded" => Self::Succeeded,
            "failed" => Self::Failed,
            "cancelled" => Self::Cancelled,
            _ => Self::OutcomeUnknown,
        }
    }

    fn can_move_to(self, next: Self) -> bool {
        use OperationState::*;
        matches!(
            (self, next),
            (Prepared, Authorized | Cancelled)
                | (Authorized, Dispatched | Cancelled)
                | (Dispatched, Succeeded | Failed | Cancelled | OutcomeUnknown)
                // Reconciliation (e.g. read-back verify) can settle an unknown outcome.
                | (OutcomeUnknown, Succeeded | Failed)
        )
    }

    pub fn is_terminal(self) -> bool {
        matches!(self, Self::Succeeded | Self::Failed | Self::Cancelled)
    }
}

#[derive(Debug, Clone, PartialEq)]
pub struct OperationRecord {
    pub id: String,
    pub task_id: String,
    pub op_type: String,
    pub params_digest: String,
    pub device_id: String,
    pub state: OperationState,
    pub fencing_token: Option<u64>,
    pub detail: Value,
    pub updated_ms: i64,
}

/// Mapping from a livstyle task to an external runtime object (Octos session,
/// Cursor agent/run) so a restart can re-attach instead of starting over.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct RuntimeMapping {
    pub task_id: String,
    pub runtime: String,
    pub kind: String,
    pub external_id: String,
    /// Last durable stream offset seen, for `ObserveRun`-style resume.
    pub resume_offset: Option<String>,
}

pub struct Store {
    conn: Connection,
}

impl Store {
    pub fn open(path: impl AsRef<Path>) -> Result<Self> {
        let conn = Connection::open(path)?;
        conn.pragma_update(None, "journal_mode", "WAL")?;
        conn.pragma_update(None, "synchronous", "FULL")?;
        Self::init(conn)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::init(Connection::open_in_memory()?)
    }

    fn init(mut conn: Connection) -> Result<Self> {
        conn.pragma_update(None, "foreign_keys", "ON")?;
        conn.busy_timeout(std::time::Duration::from_secs(5))?;
        schema::migrate(&mut conn)?;
        Ok(Self { conn })
    }

    fn tx(&mut self) -> Result<Transaction<'_>> {
        Ok(self.conn.transaction()?)
    }

    // ---- tasks -------------------------------------------------------------

    pub fn create_task<T: Serialize>(&mut self, id: &str, status: &str, doc: &T) -> Result<()> {
        let json = serde_json::to_string(doc)?;
        let tx = self.tx()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO tasks (id, version, status, doc, created_ms, updated_ms)
             VALUES (?1, 1, ?2, ?3, ?4, ?4)",
            params![id, status, json, now_ms()],
        )?;
        if inserted == 0 {
            return Err(StoreError::Exists {
                kind: "task",
                id: id.into(),
            });
        }
        append_event(
            &tx,
            id,
            "task.created",
            &serde_json::json!({ "status": status }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Removes a task and the rows that reference it. The audit log stays.
    pub fn delete_task(&mut self, id: &str) -> Result<()> {
        let tx = self.tx()?;
        tx.execute("DELETE FROM interventions WHERE task_id = ?1", [id])?;
        tx.execute("DELETE FROM plans WHERE task_id = ?1", [id])?;
        tx.execute("DELETE FROM operations WHERE task_id = ?1", [id])?;
        tx.execute("DELETE FROM runtime_mappings WHERE task_id = ?1", [id])?;
        tx.execute("DELETE FROM events WHERE task_id = ?1", [id])?;
        let removed = tx.execute("DELETE FROM tasks WHERE id = ?1", [id])?;
        if removed == 0 {
            return Err(StoreError::NotFound {
                kind: "task",
                id: id.into(),
            });
        }
        tx.commit()?;
        Ok(())
    }

    pub fn task<T: DeserializeOwned>(&self, id: &str) -> Result<Versioned<T>> {
        let row: Option<(i64, String)> = self
            .conn
            .query_row("SELECT version, doc FROM tasks WHERE id = ?1", [id], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })
            .optional()?;
        let (version, doc) = row.ok_or_else(|| StoreError::NotFound {
            kind: "task",
            id: id.into(),
        })?;
        Ok(Versioned {
            version: version as u64,
            value: serde_json::from_str(&doc)?,
        })
    }

    /// Compare-and-swap update. Returns the new version.
    pub fn update_task<T: Serialize>(
        &mut self,
        id: &str,
        expected_version: u64,
        status: &str,
        doc: &T,
        event: Option<(&str, &Value)>,
    ) -> Result<u64> {
        let json = serde_json::to_string(doc)?;
        let tx = self.tx()?;
        let changed = tx.execute(
            "UPDATE tasks SET version = version + 1, status = ?3, doc = ?4, updated_ms = ?5
             WHERE id = ?1 AND version = ?2",
            params![id, expected_version as i64, status, json, now_ms()],
        )?;
        if changed == 0 {
            let actual: Option<i64> = tx
                .query_row("SELECT version FROM tasks WHERE id = ?1", [id], |r| {
                    r.get(0)
                })
                .optional()?;
            return Err(match actual {
                None => StoreError::NotFound {
                    kind: "task",
                    id: id.into(),
                },
                Some(actual) => StoreError::Conflict {
                    kind: "task",
                    id: id.into(),
                    expected: expected_version,
                    actual: actual as u64,
                },
            });
        }
        if let Some((kind, payload)) = event {
            append_event(&tx, id, kind, payload)?;
        }
        tx.commit()?;
        Ok(expected_version + 1)
    }

    /// Task ids with the given statuses, oldest first.
    pub fn tasks_with_status(&self, statuses: &[&str]) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id, status FROM tasks ORDER BY created_ms, id")?;
        let rows = stmt.query_map([], |r| Ok((r.get::<_, String>(0)?, r.get::<_, String>(1)?)))?;
        let mut out = Vec::new();
        for row in rows {
            let (id, status) = row?;
            if statuses.contains(&status.as_str()) {
                out.push(id);
            }
        }
        Ok(out)
    }

    // ---- plans -------------------------------------------------------------

    /// Stores a new plan revision; revisions are append-only.
    pub fn save_plan<T: Serialize>(
        &mut self,
        task_id: &str,
        plan: &T,
        reason: &str,
    ) -> Result<u64> {
        let json = serde_json::to_string(plan)?;
        let tx = self.tx()?;
        let next: i64 = tx.query_row(
            "SELECT COALESCE(MAX(revision), 0) + 1 FROM plans WHERE task_id = ?1",
            [task_id],
            |r| r.get(0),
        )?;
        tx.execute(
            "INSERT INTO plans (task_id, revision, doc, reason, created_ms) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![task_id, next, json, reason, now_ms()],
        )?;
        append_event(
            &tx,
            task_id,
            "plan.updated",
            &serde_json::json!({ "revision": next, "reason": reason }),
        )?;
        tx.commit()?;
        Ok(next as u64)
    }

    pub fn latest_plan<T: DeserializeOwned>(&self, task_id: &str) -> Result<Option<Versioned<T>>> {
        let row: Option<(i64, String)> = self
            .conn
            .query_row(
                "SELECT revision, doc FROM plans WHERE task_id = ?1 ORDER BY revision DESC LIMIT 1",
                [task_id],
                |r| Ok((r.get(0)?, r.get(1)?)),
            )
            .optional()?;
        row.map(|(rev, doc)| {
            Ok(Versioned {
                version: rev as u64,
                value: serde_json::from_str(&doc)?,
            })
        })
        .transpose()
    }

    // ---- interventions -----------------------------------------------------

    /// Persists a user input *before* it is judged (PRD §8.2: nothing typed
    /// mid-run may be lost). Returns the per-task sequence number.
    pub fn record_intervention(&mut self, id: &str, task_id: &str, input: &str) -> Result<u64> {
        let tx = self.tx()?;
        let seq: i64 = tx.query_row(
            "SELECT COALESCE(MAX(seq), 0) + 1 FROM interventions WHERE task_id = ?1",
            [task_id],
            |r| r.get(0),
        )?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO interventions (id, task_id, seq, input, status, created_ms, updated_ms)
             VALUES (?1, ?2, ?3, ?4, 'received', ?5, ?5)",
            params![id, task_id, seq, input, now_ms()],
        )?;
        if inserted == 0 {
            return Err(StoreError::Exists {
                kind: "intervention",
                id: id.into(),
            });
        }
        append_event(
            &tx,
            task_id,
            "intervention.received",
            &serde_json::json!({ "id": id, "seq": seq }),
        )?;
        tx.commit()?;
        Ok(seq as u64)
    }

    pub fn update_intervention<J: Serialize, R: Serialize>(
        &mut self,
        id: &str,
        status: &str,
        judgment: Option<&J>,
        resolution: Option<&R>,
    ) -> Result<()> {
        let judgment = judgment.map(serde_json::to_string).transpose()?;
        let resolution = resolution.map(serde_json::to_string).transpose()?;
        let tx = self.tx()?;
        let task_id: Option<String> = tx
            .query_row(
                "SELECT task_id FROM interventions WHERE id = ?1",
                [id],
                |r| r.get(0),
            )
            .optional()?;
        let task_id = task_id.ok_or_else(|| StoreError::NotFound {
            kind: "intervention",
            id: id.into(),
        })?;
        tx.execute(
            "UPDATE interventions SET status = ?2,
                 judgment = COALESCE(?3, judgment), resolution = COALESCE(?4, resolution), updated_ms = ?5
             WHERE id = ?1",
            params![id, status, judgment, resolution, now_ms()],
        )?;
        append_event(
            &tx,
            &task_id,
            "intervention.updated",
            &serde_json::json!({ "id": id, "status": status }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Interventions not yet settled, in arrival order: what a restarted
    /// process must re-judge or re-deliver.
    /// Newest first, including ones already settled. Used by the UI projection.
    pub fn interventions(&self, task_id: &str) -> Result<Vec<(String, u64, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, input, status FROM interventions
             WHERE task_id = ?1 ORDER BY seq DESC LIMIT 20",
        )?;
        let rows = stmt.query_map([task_id], |r| {
            Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn task_ids(&self) -> Result<Vec<String>> {
        let mut stmt = self
            .conn
            .prepare("SELECT id FROM tasks ORDER BY created_ms")?;
        let rows = stmt.query_map([], |r| r.get(0))?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    pub fn pending_interventions(
        &self,
        task_id: &str,
    ) -> Result<Vec<(String, u64, String, String)>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, seq, input, status FROM interventions
             WHERE task_id = ?1 AND status NOT IN ('applied', 'rejected', 'superseded')
             ORDER BY seq",
        )?;
        let rows = stmt.query_map([task_id], |r| {
            Ok((r.get(0)?, r.get::<_, i64>(1)? as u64, r.get(2)?, r.get(3)?))
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- hardware operations ----------------------------------------------

    pub fn record_operation(
        &mut self,
        id: &str,
        task_id: &str,
        op_type: &str,
        params_digest: &str,
        device_id: &str,
    ) -> Result<()> {
        let tx = self.tx()?;
        let inserted = tx.execute(
            "INSERT OR IGNORE INTO operations (id, task_id, op_type, params_digest, device_id, state, detail, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, 'prepared', 'null', ?6)",
            params![id, task_id, op_type, params_digest, device_id, now_ms()],
        )?;
        if inserted == 0 {
            return Err(StoreError::Exists {
                kind: "operation",
                id: id.into(),
            });
        }
        append_audit(
            &tx,
            "supervisor",
            "operation.prepared",
            id,
            &serde_json::json!({ "op_type": op_type, "params_digest": params_digest, "device": device_id }),
        )?;
        tx.commit()?;
        Ok(())
    }

    /// Moves an operation along its lifecycle; illegal transitions are refused
    /// so a crash-recovered `OutcomeUnknown` cannot silently become `Dispatched`.
    pub fn transition_operation(
        &mut self,
        id: &str,
        next: OperationState,
        fencing_token: Option<u64>,
        detail: &Value,
    ) -> Result<()> {
        let tx = self.tx()?;
        let current: Option<String> = tx
            .query_row("SELECT state FROM operations WHERE id = ?1", [id], |r| {
                r.get(0)
            })
            .optional()?;
        let current = current.ok_or_else(|| StoreError::NotFound {
            kind: "operation",
            id: id.into(),
        })?;
        let state = OperationState::parse(&current);
        if !state.can_move_to(next) {
            return Err(StoreError::BadOperationState {
                id: id.into(),
                state: current,
                next: next.as_str().into(),
            });
        }
        tx.execute(
            "UPDATE operations SET state = ?2, fencing_token = COALESCE(?3, fencing_token), detail = ?4, updated_ms = ?5
             WHERE id = ?1",
            params![
                id,
                next.as_str(),
                fencing_token.map(|t| t as i64),
                serde_json::to_string(detail)?,
                now_ms()
            ],
        )?;
        append_audit(
            &tx,
            "supervisor",
            &format!("operation.{}", next.as_str()),
            id,
            &serde_json::json!({ "from": current, "fencing_token": fencing_token, "detail": detail }),
        )?;
        tx.commit()?;
        Ok(())
    }

    pub fn operation(&self, id: &str) -> Result<OperationRecord> {
        self.conn
            .query_row(
                "SELECT id, task_id, op_type, params_digest, device_id, state, fencing_token, detail, updated_ms
                 FROM operations WHERE id = ?1",
                [id],
                operation_row,
            )
            .optional()?
            .ok_or_else(|| StoreError::NotFound { kind: "operation", id: id.into() })
    }

    /// Crash recovery (PRD §10.5): any operation left `Dispatched` may have
    /// touched the device. Mark it `OutcomeUnknown` and return it so the
    /// supervisor can ask the user to reconcile. Never re-dispatched.
    pub fn recover_inflight_operations(&mut self) -> Result<Vec<OperationRecord>> {
        let ids: Vec<String> = {
            let mut stmt = self.conn.prepare(
                "SELECT id FROM operations WHERE state = 'dispatched' ORDER BY updated_ms",
            )?;
            stmt.query_map([], |r| r.get(0))?
                .collect::<std::result::Result<_, _>>()?
        };
        let mut out = Vec::new();
        for id in ids {
            self.transition_operation(
                &id,
                OperationState::OutcomeUnknown,
                None,
                &serde_json::json!({ "reason": "process restarted while dispatched" }),
            )?;
            out.push(self.operation(&id)?);
        }
        Ok(out)
    }

    // ---- runtime mappings --------------------------------------------------

    pub fn upsert_runtime_mapping(&mut self, m: &RuntimeMapping) -> Result<()> {
        self.conn.execute(
            "INSERT INTO runtime_mappings (task_id, runtime, kind, external_id, resume_offset, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6)
             ON CONFLICT (task_id, runtime, kind) DO UPDATE SET
                 external_id = excluded.external_id,
                 resume_offset = excluded.resume_offset,
                 updated_ms = excluded.updated_ms",
            params![m.task_id, m.runtime, m.kind, m.external_id, m.resume_offset, now_ms()],
        )?;
        Ok(())
    }

    pub fn runtime_mappings(&self, task_id: &str) -> Result<Vec<RuntimeMapping>> {
        let mut stmt = self.conn.prepare(
            "SELECT task_id, runtime, kind, external_id, resume_offset FROM runtime_mappings
             WHERE task_id = ?1 ORDER BY runtime, kind",
        )?;
        let rows = stmt.query_map([task_id], |r| {
            Ok(RuntimeMapping {
                task_id: r.get(0)?,
                runtime: r.get(1)?,
                kind: r.get(2)?,
                external_id: r.get(3)?,
                resume_offset: r.get(4)?,
            })
        })?;
        Ok(rows.collect::<std::result::Result<_, _>>()?)
    }

    // ---- events and audit --------------------------------------------------

    pub fn events_after(&self, task_id: &str, after_seq: u64) -> Result<Vec<Event>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, task_id, kind, payload, at_ms FROM events
             WHERE task_id = ?1 AND seq > ?2 ORDER BY seq",
        )?;
        let rows = stmt.query_map(params![task_id, after_seq as i64], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, String>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, i64>(4)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, task_id, kind, payload, at_ms) = row?;
            out.push(Event {
                seq: seq as u64,
                task_id,
                kind,
                payload: serde_json::from_str(&payload)?,
                at_ms,
            });
        }
        Ok(out)
    }

    pub fn audit(
        &mut self,
        actor: &str,
        action: &str,
        subject: &str,
        payload: &Value,
    ) -> Result<u64> {
        let tx = self.tx()?;
        let seq = append_audit(&tx, actor, action, subject, payload)?;
        tx.commit()?;
        Ok(seq)
    }

    pub fn audit_entries(&self) -> Result<Vec<AuditEntry>> {
        let mut stmt = self.conn.prepare(
            "SELECT seq, at_ms, actor, action, subject, payload, hash FROM audit ORDER BY seq",
        )?;
        let rows = stmt.query_map([], |r| {
            Ok((
                r.get::<_, i64>(0)?,
                r.get::<_, i64>(1)?,
                r.get::<_, String>(2)?,
                r.get::<_, String>(3)?,
                r.get::<_, String>(4)?,
                r.get::<_, String>(5)?,
                r.get::<_, String>(6)?,
            ))
        })?;
        let mut out = Vec::new();
        for row in rows {
            let (seq, at_ms, actor, action, subject, payload, hash) = row?;
            out.push(AuditEntry {
                seq: seq as u64,
                at_ms,
                actor,
                action,
                subject,
                payload: serde_json::from_str(&payload)?,
                hash,
            });
        }
        Ok(out)
    }

    /// Recomputes the hash chain; detects edited or deleted audit rows.
    pub fn verify_audit_chain(&self) -> Result<usize> {
        let mut stmt = self
            .conn
            .prepare("SELECT seq, at_ms, actor, action, subject, payload, prev_hash, hash FROM audit ORDER BY seq")?;
        let mut rows = stmt.query([])?;
        let mut prev = String::new();
        let mut expected_seq = 1u64;
        let mut n = 0;
        while let Some(r) = rows.next()? {
            let seq = r.get::<_, i64>(0)? as u64;
            let prev_hash: String = r.get(6)?;
            let hash: String = r.get(7)?;
            let computed = audit_hash(
                &prev,
                seq,
                r.get(1)?,
                &r.get::<_, String>(2)?,
                &r.get::<_, String>(3)?,
                &r.get::<_, String>(4)?,
                &r.get::<_, String>(5)?,
            );
            if seq != expected_seq || prev_hash != prev || hash != computed {
                return Err(StoreError::AuditChainBroken { seq });
            }
            prev = hash;
            expected_seq += 1;
            n += 1;
        }
        Ok(n)
    }

    #[cfg(test)]
    pub(crate) fn conn(&self) -> &Connection {
        &self.conn
    }
}

fn operation_row(r: &rusqlite::Row<'_>) -> rusqlite::Result<OperationRecord> {
    let detail: String = r.get(7)?;
    Ok(OperationRecord {
        id: r.get(0)?,
        task_id: r.get(1)?,
        op_type: r.get(2)?,
        params_digest: r.get(3)?,
        device_id: r.get(4)?,
        state: OperationState::parse(&r.get::<_, String>(5)?),
        fencing_token: r.get::<_, Option<i64>>(6)?.map(|t| t as u64),
        detail: serde_json::from_str(&detail).unwrap_or(Value::Null),
        updated_ms: r.get(8)?,
    })
}

fn append_event(tx: &Transaction<'_>, task_id: &str, kind: &str, payload: &Value) -> Result<u64> {
    tx.execute(
        "INSERT INTO events (task_id, kind, payload, at_ms) VALUES (?1, ?2, ?3, ?4)",
        params![task_id, kind, serde_json::to_string(payload)?, now_ms()],
    )?;
    Ok(tx.last_insert_rowid() as u64)
}

fn audit_hash(
    prev: &str,
    seq: u64,
    at_ms: i64,
    actor: &str,
    action: &str,
    subject: &str,
    payload: &str,
) -> String {
    let mut h = Sha256::new();
    for part in [
        prev,
        &seq.to_string(),
        &at_ms.to_string(),
        actor,
        action,
        subject,
        payload,
    ] {
        h.update((part.len() as u64).to_be_bytes());
        h.update(part.as_bytes());
    }
    hex::encode(h.finalize())
}

fn append_audit(
    tx: &Transaction<'_>,
    actor: &str,
    action: &str,
    subject: &str,
    payload: &Value,
) -> Result<u64> {
    let (seq, prev): (i64, String) = tx.query_row(
        "SELECT COALESCE(MAX(seq), 0) + 1,
                COALESCE((SELECT hash FROM audit ORDER BY seq DESC LIMIT 1), '')
         FROM audit",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    let at = now_ms();
    let payload = serde_json::to_string(payload)?;
    let hash = audit_hash(&prev, seq as u64, at, actor, action, subject, &payload);
    tx.execute(
        "INSERT INTO audit (seq, at_ms, actor, action, subject, payload, prev_hash, hash)
         VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
        params![seq, at, actor, action, subject, payload, prev, hash],
    )?;
    Ok(seq as u64)
}

#[cfg(test)]
mod tests;
