//! Speaks OUP against a tiny in-process WebSocket so the client does not need
//! a running `octos serve`.

use std::io::{Read, Write};
use std::net::TcpListener;
use std::time::Duration;

use livstyle_octos::OctosClient;
use serde_json::{Value, json};
use sha1::{Digest, Sha1};

fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | bytes[i + 2] as u32;
        for shift in [18, 12, 6, 0] {
            out.push(T[((n >> shift) & 63) as usize] as char);
        }
        i += 3;
    }
    let rest = bytes.len() - i;
    if rest == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push_str("==");
    } else if rest == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

fn send_text(sock: &mut impl Write, text: &str) {
    let n = text.len();
    if n < 126 {
        sock.write_all(&[0x81, n as u8]).unwrap();
    } else {
        sock.write_all(&[0x81, 126]).unwrap();
        sock.write_all(&(n as u16).to_be_bytes()).unwrap();
    }
    sock.write_all(text.as_bytes()).unwrap();
}

fn serve(body: Value) -> String {
    let listener = TcpListener::bind("127.0.0.1:0").unwrap();
    let port = listener.local_addr().unwrap().port();
    std::thread::spawn(move || {
        let (mut sock, _) = listener.accept().unwrap();
        sock.set_read_timeout(Some(Duration::from_secs(5))).unwrap();
        let mut buf = Vec::new();
        let mut byte = [0u8; 1];
        while !buf.ends_with(b"\r\n\r\n") {
            sock.read_exact(&mut byte).unwrap();
            buf.push(byte[0]);
        }
        let head = String::from_utf8(buf).unwrap();
        let key = head
            .lines()
            .find_map(|l| {
                l.split_once(':').and_then(|(k, v)| {
                    k.eq_ignore_ascii_case("sec-websocket-key")
                        .then(|| v.trim())
                })
            })
            .unwrap();
        let mut hasher = Sha1::new();
        hasher.update(key.as_bytes());
        hasher.update(b"258EAFA5-E914-47DA-95CA-C5AB0DC85B11");
        let accept = b64(&hasher.finalize());
        write!(
            sock,
            "HTTP/1.1 101 Switching Protocols\r\nUpgrade: websocket\r\nConnection: Upgrade\r\nSec-WebSocket-Accept: {accept}\r\n\r\n"
        )
        .unwrap();
        // One masked client frame, then one unmasked reply.
        let mut hdr = [0u8; 2];
        sock.read_exact(&mut hdr).unwrap();
        let mut len = (hdr[1] & 0x7f) as usize;
        if len == 126 {
            let mut ext = [0u8; 2];
            sock.read_exact(&mut ext).unwrap();
            len = u16::from_be_bytes(ext) as usize;
        }
        let mut mask = [0u8; 4];
        sock.read_exact(&mut mask).unwrap();
        let mut payload = vec![0u8; len];
        sock.read_exact(&mut payload).unwrap();
        for (i, b) in payload.iter_mut().enumerate() {
            *b ^= mask[i % 4];
        }
        let req: Value = serde_json::from_slice(&payload).unwrap();
        let reply = json!({
            "jsonrpc": "2.0",
            "id": req["id"],
            "result": body,
        });
        send_text(&mut sock, &reply.to_string());
        send_text(
            &mut sock,
            r#"{"jsonrpc":"2.0","method":"turn/steer_dropped","params":{"inputs":["kept"]}}"#,
        );
        std::thread::sleep(Duration::from_millis(80));
    });
    format!("ws://127.0.0.1:{port}/api/ui-protocol/ws")
}

#[test]
fn round_trip_hello_and_notification() {
    let url = serve(json!({
        "type": "server_hello",
        "server_version": "2.0.3-rc.13",
        "supported_features": ["event.turn_steer_dropped.v1"],
        "supported_methods": ["turn/steer", "turn/interrupt", "task/cancel", "task/restart_from_node"],
    }));
    let client = OctosClient::connect(&url, "123456").unwrap();
    let hello = client.hello().unwrap();
    assert_eq!(hello.server_version, "2.0.3-rc.13");
    let caps = livstyle_octos::ServerCaps::from_hello(&hello);
    assert!(caps.steer && caps.interrupt && caps.restart_from_node && caps.steer_dropped);
    std::thread::sleep(Duration::from_millis(100));
    let notes = client.drain();
    assert_eq!(notes[0]["method"], "turn/steer_dropped");
}

#[test]
fn refuses_remote_endpoint() {
    let err = match OctosClient::connect("ws://example.com/api/ui-protocol/ws", "t") {
        Err(e) => e,
        Ok(_) => panic!("remote endpoint must be refused"),
    };
    assert!(err.to_string().contains("loopback"), "{err}");
}
