//! Spike 1 (PRD §17): exercise the Cursor SDK Bridge end to end. Each check
//! prints PASS/FAIL; the command fails if any check fails.

use std::path::PathBuf;
use std::time::{Duration, Instant};

use livstyle_cursor::{
    Bridge, BridgeConfig, CursorClient, CursorError, RunEvent, RunStatus, locate_bridge,
    runtime_caps,
};

struct Report {
    failed: usize,
}

impl Report {
    fn check(&mut self, name: &str, result: Result<String, String>) -> bool {
        match result {
            Ok(detail) => {
                println!("PASS {name}: {detail}");
                true
            }
            Err(e) => {
                self.failed += 1;
                println!("FAIL {name}: {e}");
                false
            }
        }
    }
}

fn scratch_workspace() -> Result<PathBuf, String> {
    let dir = std::env::temp_dir().join(format!("livstyle-smoke-{}", std::process::id()));
    std::fs::create_dir_all(&dir).map_err(|e| e.to_string())?;
    std::fs::write(
        dir.join("README.md"),
        "Scratch workspace for the livstyle Cursor smoke test. Nothing here matters.\n",
    )
    .map_err(|e| e.to_string())?;
    Ok(dir)
}

pub fn run(model: Option<&str>, offline: bool) -> Result<(), String> {
    let key = if offline {
        None
    } else {
        crate::auth::cursor_key()?
    };
    if !offline && key.is_none() {
        return Err(
            "no Cursor API key; run `livstyle auth cursor` first, or pass --offline".into(),
        );
    }
    let workspace = scratch_workspace()?;
    let config = BridgeConfig {
        binary: locate_bridge().map_err(|e| e.to_string())?,
        workspace: workspace.clone(),
        state_root: Some(workspace.join(".bridge-state")),
    };
    let mut report = Report { failed: 0 };

    let started = Instant::now();
    let mut bridge = Bridge::spawn(&config, key.as_ref()).map_err(|e| e.to_string())?;
    report.check(
        "spawn",
        Ok(format!("ready in {} ms", started.elapsed().as_millis())),
    );
    let client = CursorClient::new(bridge.endpoint().clone(), key.clone());

    report.check(
        "ping",
        client
            .ping()
            .map(|_| "ok".into())
            .map_err(|e| e.to_string()),
    );
    let version = client.version();
    let caps_ok = report.check(
        "version",
        version
            .as_ref()
            .map(|v| {
                format!(
                    "{} {} caps={}",
                    v.bridge_version,
                    v.protocol_version,
                    v.capabilities.join(",")
                )
            })
            .map_err(|e| e.to_string()),
    );
    if caps_ok {
        let caps = runtime_caps(version.as_ref().unwrap());
        println!(
            "     steering caps: steer={} interrupt={}",
            caps.steer, caps.interrupt
        );
    }

    if offline {
        bridge.shutdown();
        return finish(report);
    }

    report.check(
        "me",
        client
            .me()
            .map(|u| format!("key {:?}", u.api_key_name))
            .map_err(|e| e.to_string()),
    );
    let models = client.list_models().map_err(|e| e.to_string());
    let model_id = match (&models, model) {
        (_, Some(m)) => Some(m.to_owned()),
        (Ok(list), None) => list.first().map(|m| m.id.clone()),
        _ => None,
    };
    report.check(
        "list_models",
        models
            .as_ref()
            .map(|l| format!("{} models, using {:?}", l.len(), model_id))
            .map_err(Clone::clone),
    );
    let Some(model_id) = model_id else {
        bridge.shutdown();
        return finish(report);
    };

    let agent = client.create_agent(&model_id, &workspace, Some("livstyle-smoke"));
    let agent_ok = report.check(
        "create_agent",
        agent
            .as_ref()
            .map(|a| a.agent_id.clone())
            .map_err(|e| e.to_string()),
    );
    if !agent_ok {
        bridge.shutdown();
        return finish(report);
    }
    let agent = agent.unwrap();

    // Turn 1: short answer, streamed to completion.
    let turn1 = (|| -> Result<(String, String), CursorError> {
        let mut stream = client.send(
            &agent.agent_id,
            "Reply with exactly the text: livstyle-ok",
            None,
        )?;
        let mut kinds = Vec::new();
        let mut summary = None;
        for event in stream.by_ref() {
            match event? {
                RunEvent::Message { kind, .. } => kinds.push(kind),
                RunEvent::Result(r) => summary = Some(r),
                _ => {}
            }
        }
        let summary = summary.ok_or_else(|| CursorError::Protocol("no result".into()))?;
        if summary.status != RunStatus::Finished {
            return Err(CursorError::Protocol(format!(
                "status {:?}: {}",
                summary.status,
                stream.last_status_message().unwrap_or("")
            )));
        }
        kinds.dedup();
        Ok((
            summary.run_id.clone(),
            format!(
                "text {:?}, message kinds {}",
                summary.text.unwrap_or_default().trim(),
                kinds.join(",")
            ),
        ))
    })();
    let run1 = turn1.as_ref().ok().map(|(id, _)| id.clone());
    report.check(
        "send_stream",
        turn1.map(|(_, d)| d).map_err(|e| e.to_string()),
    );

    // Turn 2: cancel from another connection as soon as the run id is known.
    let turn2 = (|| -> Result<String, CursorError> {
        let mut stream = client.send(
            &agent.agent_id,
            "Count slowly from 1 to 200, one number per line, explaining each number in a sentence.",
            None,
        )?;
        let mut canceller = None;
        let mut status = None;
        let started = Instant::now();
        while let Some(event) = stream.next() {
            let event = event?;
            if canceller.is_none() {
                if let Some(run_id) = stream.run_id().map(str::to_owned) {
                    let c = CursorClient::new(bridge.endpoint().clone(), key.clone());
                    let agent_id = agent.agent_id.clone();
                    canceller = Some(std::thread::spawn(move || {
                        c.cancel_run(&run_id, Some(&agent_id))
                    }));
                }
            }
            if let RunEvent::Result(r) = event {
                status = Some(r.status);
            }
        }
        if let Some(handle) = canceller {
            handle.join().expect("cancel thread")?;
        } else {
            return Err(CursorError::Protocol("run id never seen".into()));
        }
        match status {
            Some(RunStatus::Cancelled) => Ok(format!(
                "cancelled after {} ms",
                started.elapsed().as_millis()
            )),
            other => Err(CursorError::Protocol(format!(
                "expected Cancelled, got {other:?}"
            ))),
        }
    })();
    report.check("cancel_run", turn2.map_err(|e| e.to_string()));

    // Durable replay and resume from an offset.
    if let Some(run_id) = run1 {
        let replay = (|| -> Result<String, CursorError> {
            let mut first = client.observe_run(&run_id, None)?;
            let mut total = 0usize;
            let mut midpoint = None;
            while let Some(event) = first.next() {
                event?;
                total += 1;
                if total == 2 {
                    midpoint = first.resume_offset().map(str::to_owned);
                }
            }
            let Some(offset) = midpoint else {
                return Err(CursorError::Protocol(format!(
                    "only {total} events, no offset to resume from"
                )));
            };
            let resumed = client
                .observe_run(&run_id, Some(&offset))?
                .filter(|e| e.is_ok())
                .count();
            if resumed >= total {
                return Err(CursorError::Protocol(format!(
                    "resume after offset returned {resumed} of {total} events"
                )));
            }
            Ok(format!("{total} events, {resumed} after offset"))
        })();
        report.check("observe_resume", replay.map_err(|e| e.to_string()));
    }

    report.check(
        "close_agent",
        client
            .close_agent(&agent.agent_id)
            .map(|_| "ok".into())
            .map_err(|e| e.to_string()),
    );
    bridge.shutdown();
    std::thread::sleep(Duration::from_millis(100));
    report.check(
        "shutdown",
        if bridge.is_running() {
            Err("still running".into())
        } else {
            Ok("exited".into())
        },
    );
    finish(report)
}

fn finish(report: Report) -> Result<(), String> {
    if report.failed == 0 {
        Ok(())
    } else {
        Err(format!("{} check(s) failed", report.failed))
    }
}
