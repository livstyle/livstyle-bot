use rusqlite::Connection;

use crate::{Result, StoreError};

pub const SCHEMA_VERSION: u32 = 1;

const V1: &str = "
CREATE TABLE tasks (
    id          TEXT PRIMARY KEY,
    version     INTEGER NOT NULL,
    status      TEXT NOT NULL,
    doc         TEXT NOT NULL,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL
);

CREATE TABLE plans (
    task_id     TEXT NOT NULL REFERENCES tasks(id),
    revision    INTEGER NOT NULL,
    doc         TEXT NOT NULL,
    reason      TEXT NOT NULL,
    created_ms  INTEGER NOT NULL,
    PRIMARY KEY (task_id, revision)
);

CREATE TABLE interventions (
    id          TEXT PRIMARY KEY,
    task_id     TEXT NOT NULL REFERENCES tasks(id),
    seq         INTEGER NOT NULL,
    input       TEXT NOT NULL,
    status      TEXT NOT NULL,
    judgment    TEXT,
    resolution  TEXT,
    created_ms  INTEGER NOT NULL,
    updated_ms  INTEGER NOT NULL,
    UNIQUE (task_id, seq)
);

CREATE TABLE operations (
    id            TEXT PRIMARY KEY,
    task_id       TEXT NOT NULL REFERENCES tasks(id),
    op_type       TEXT NOT NULL,
    params_digest TEXT NOT NULL,
    device_id     TEXT NOT NULL,
    state         TEXT NOT NULL,
    fencing_token INTEGER,
    detail        TEXT NOT NULL,
    updated_ms    INTEGER NOT NULL
);
CREATE INDEX operations_state ON operations(state);

CREATE TABLE runtime_mappings (
    task_id       TEXT NOT NULL REFERENCES tasks(id),
    runtime       TEXT NOT NULL,
    kind          TEXT NOT NULL,
    external_id   TEXT NOT NULL,
    resume_offset TEXT,
    updated_ms    INTEGER NOT NULL,
    PRIMARY KEY (task_id, runtime, kind)
);

CREATE TABLE events (
    seq      INTEGER PRIMARY KEY AUTOINCREMENT,
    task_id  TEXT NOT NULL,
    kind     TEXT NOT NULL,
    payload  TEXT NOT NULL,
    at_ms    INTEGER NOT NULL
);
CREATE INDEX events_task ON events(task_id, seq);

CREATE TABLE audit (
    seq        INTEGER PRIMARY KEY,
    at_ms      INTEGER NOT NULL,
    actor      TEXT NOT NULL,
    action     TEXT NOT NULL,
    subject    TEXT NOT NULL,
    payload    TEXT NOT NULL,
    prev_hash  TEXT NOT NULL,
    hash       TEXT NOT NULL
);
CREATE TRIGGER audit_no_update BEFORE UPDATE ON audit
BEGIN SELECT RAISE(ABORT, 'audit is append-only'); END;
CREATE TRIGGER audit_no_delete BEFORE DELETE ON audit
BEGIN SELECT RAISE(ABORT, 'audit is append-only'); END;
";

const MIGRATIONS: &[&str] = &[V1];

pub(crate) fn migrate(conn: &mut Connection) -> Result<()> {
    let found: u32 = conn.pragma_query_value(None, "user_version", |r| r.get(0))?;
    if found > SCHEMA_VERSION {
        return Err(StoreError::FutureSchema { found });
    }
    for (i, sql) in MIGRATIONS.iter().enumerate().skip(found as usize) {
        let tx = conn.transaction()?;
        tx.execute_batch(sql)?;
        tx.pragma_update(None, "user_version", (i + 1) as u32)?;
        tx.commit()?;
    }
    Ok(())
}
