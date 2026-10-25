use livstyle_octos::{OctosClient, ServerCaps};

pub fn probe(url: &str, token: &str) -> Result<(), String> {
    let client = OctosClient::connect(url, token).map_err(|e| e.to_string())?;
    let hello = client.hello().map_err(|e| e.to_string())?;
    let caps = ServerCaps::from_hello(&hello);
    println!("protocol: {}", hello.server_version);
    println!(
        "steer={} interrupt={} task_cancel={} restart_from_node={} steer_dropped={}",
        caps.steer, caps.interrupt, caps.task_cancel, caps.restart_from_node, caps.steer_dropped
    );
    let session = format!("livstyle:probe-{}", std::process::id());
    match client.open_session(&session, "livstyle", "/tmp") {
        Ok(opened) => println!("session/open: {opened}"),
        Err(e) => {
            let msg = e.to_string();
            if msg.contains("\"missing\":\"llm\"") {
                println!("session/open: refused, profile has no LLM (expected on this serve)");
                println!("turns stay on Cursor until an LLM is selected for the livstyle profile");
            } else {
                return Err(msg);
            }
        }
    }
    Ok(())
}
