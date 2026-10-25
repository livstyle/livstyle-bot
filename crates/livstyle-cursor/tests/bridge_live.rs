//! Spike 1 checks that need the real bridge binary but no Cursor API key.
//! Skipped (with a message) when the pinned bridge is not installed.

use livstyle_cursor::{
    Bridge, BridgeConfig, CursorClient, CursorError, Endpoint, locate_bridge, runtime_caps,
};

fn spawn() -> Option<Bridge> {
    let binary = match locate_bridge() {
        Ok(b) => b,
        Err(e) => {
            eprintln!("skipping: {e}");
            return None;
        }
    };
    let workspace =
        std::env::temp_dir().join(format!("livstyle-bridge-test-{}", std::process::id()));
    std::fs::create_dir_all(&workspace).unwrap();
    let config = BridgeConfig {
        binary,
        workspace: workspace.clone(),
        state_root: Some(workspace.join("state")),
    };
    Some(Bridge::spawn(&config, None).expect("bridge starts"))
}

#[test]
fn handshake_version_auth_and_shutdown() {
    let Some(mut bridge) = spawn() else { return };
    let client = CursorClient::new(bridge.endpoint().clone(), None);

    client.ping().unwrap();
    let version = client.version().unwrap();
    assert_eq!(version.protocol_version, "sdk.v1");
    assert!(version.has("agent.send"));
    assert!(runtime_caps(&version).interrupt);

    let url = bridge.ready_info().endpoint_url().unwrap();
    let wrong = CursorClient::new(Endpoint::attach(&url, "not-the-token").unwrap(), None);
    assert!(matches!(
        wrong.ping(),
        Err(CursorError::BridgeUnauthenticated)
    ));

    // No key configured: refused locally, before any network call.
    assert!(matches!(client.me(), Err(CursorError::Auth { .. })));

    assert!(bridge.is_running());
    bridge.shutdown();
    assert!(!bridge.is_running());
}
