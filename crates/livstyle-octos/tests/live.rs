//! Spike 2 against a running `octos serve`. Skips when nothing is listening.
//!
//! ```sh
//! LIVSTYLE_OCTOS_URL=ws://127.0.0.1:50080/api/ui-protocol/ws \
//! LIVSTYLE_OCTOS_TOKEN=123456 \
//! cargo test -p livstyle-octos --test live -- --nocapture
//! ```

use livstyle_octos::{OctosClient, ServerCaps};

#[test]
fn spike2_against_local_serve() {
    let url = std::env::var("LIVSTYLE_OCTOS_URL")
        .unwrap_or_else(|_| "ws://127.0.0.1:50080/api/ui-protocol/ws".into());
    let token = std::env::var("LIVSTYLE_OCTOS_TOKEN").unwrap_or_else(|_| "123456".into());
    let client = match OctosClient::connect(&url, &token) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("skipping: {url} ({e})");
            return;
        }
    };
    let hello = client.hello().expect("client_hello");
    eprintln!(
        "server {:?} methods {:?}",
        hello.server_version, hello.methods
    );
    eprintln!("features {:?}", hello.features);
    let caps = ServerCaps::from_hello(&hello);
    assert!(caps.steer, "turn/steer missing: {}", hello.raw);
    assert!(caps.interrupt, "turn/interrupt missing");
    assert!(caps.task_cancel, "task/cancel missing");
    assert!(caps.restart_from_node, "task/restart_from_node missing");
    assert!(caps.steer_dropped, "steer_dropped not negotiated");

    // `{profile}:local:{chat}` so SessionKey::profile_id() is `livstyle`.
    // A two-part id is not profile-scoped; hydrate then looks up a different
    // SessionManager and returns unknown_session.
    let session = format!("livstyle:local:spike2-{}", std::process::id());
    let opened = client.open_session(&session, "livstyle", "/tmp");
    match opened {
        Ok(v) => {
            eprintln!("opened {v}");
            let confirmed = v
                .pointer("/opened/session_id")
                .and_then(|id| id.as_str())
                .unwrap_or(session.as_str());
            let hydrated = client.hydrate(confirmed).expect("session/hydrate");
            eprintln!(
                "hydrate keys {:?}",
                hydrated.as_object().map(|o| o.keys().collect::<Vec<_>>())
            );
            let turn = client.turn_start(&session, "018f0000-0000-7000-8000-000000000001", "ping");
            eprintln!("turn/start: {turn:?}");
        }
        Err(e) => {
            let msg = e.to_string();
            eprintln!("session/open: {msg}");
            // This serve has a profile and no LLM. Opening must fail here,
            // before any turn, and say what is missing.
            assert!(msg.contains("\"missing\":\"llm\""), "{msg}");
        }
    }
}
