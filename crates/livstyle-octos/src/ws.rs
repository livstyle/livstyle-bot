//! Minimal WebSocket client: one HTTP/1.1 upgrade, then masked text frames.
//! Enough for OUP, which is JSON text in both directions.

use std::io::{Read, Write};
use std::net::TcpStream;
use std::time::Duration;

use livstyle_http::Url;
use sha1::{Digest, Sha1};

const GUID: &str = "258EAFA5-E914-47DA-95CA-C5AB0DC85B11";

#[derive(Debug)]
pub struct Socket {
    stream: TcpStream,
}

impl Socket {
    pub fn connect(url: &Url, token: &str, timeout: Duration) -> Result<Self, String> {
        if !url.is_loopback() {
            return Err(format!("refusing non-loopback OUP endpoint {}", url.host));
        }
        let addr = (url.host.as_str(), url.port)
            .to_socket_addrs_first()
            .map_err(|e| e.to_string())?;
        let mut stream = TcpStream::connect_timeout(&addr, timeout).map_err(|e| e.to_string())?;
        stream
            .set_read_timeout(Some(timeout))
            .map_err(|e| e.to_string())?;
        stream.set_nodelay(true).ok();

        let mut key_raw = [
            0x6c, 0x69, 0x76, 0x73, 0x74, 0x79, 0x6c, 0x65, 0, 0, 0, 0, 0, 0, 0, 0,
        ];
        key_raw[8..12].copy_from_slice(&std::process::id().to_be_bytes());
        let key = b64(&key_raw);
        let host = format!("{}:{}", url.host, url.port);
        let req = format!(
            "GET {path} HTTP/1.1\r\n\
             Host: {host}\r\n\
             Upgrade: websocket\r\n\
             Connection: Upgrade\r\n\
             Sec-WebSocket-Key: {key}\r\n\
             Sec-WebSocket-Version: 13\r\n\
             Authorization: Bearer {token}\r\n\
             \r\n",
            path = url.path,
        );
        stream
            .write_all(req.as_bytes())
            .map_err(|e| e.to_string())?;
        let head = read_headers(&mut stream)?;
        let status = head.lines().next().unwrap_or("");
        if !status.contains(" 101 ") {
            return Err(format!("websocket upgrade refused: {status}"));
        }
        let accept = header(&head, "sec-websocket-accept").unwrap_or("");
        let mut hasher = Sha1::new();
        hasher.update(key.as_bytes());
        hasher.update(GUID.as_bytes());
        let expect = b64(&hasher.finalize());
        if accept != expect {
            return Err("websocket accept mismatch".into());
        }
        Ok(Self { stream })
    }

    pub fn try_clone(&self) -> Result<Self, String> {
        Ok(Self {
            stream: self.stream.try_clone().map_err(|e| e.to_string())?,
        })
    }

    pub fn shutdown(&self) {
        use std::net::Shutdown;
        let _ = self.stream.shutdown(Shutdown::Both);
    }

    pub fn set_timeout(&self, timeout: Option<Duration>) -> Result<(), String> {
        self.stream
            .set_read_timeout(timeout)
            .map_err(|e| e.to_string())
    }

    pub fn send_text(&mut self, text: &str) -> Result<(), String> {
        let payload = text.as_bytes();
        let mut frame = Vec::with_capacity(payload.len() + 14);
        frame.push(0x81);
        let mask = [0x3c, 0xa1, 0x07, 0x5e];
        match payload.len() {
            n if n < 126 => frame.push(0x80 | n as u8),
            n if n <= u16::MAX as usize => {
                frame.push(0x80 | 126);
                frame.extend((n as u16).to_be_bytes());
            }
            n => {
                frame.push(0x80 | 127);
                frame.extend((n as u64).to_be_bytes());
            }
        }
        frame.extend(mask);
        frame.extend(payload.iter().enumerate().map(|(i, b)| b ^ mask[i % 4]));
        self.stream.write_all(&frame).map_err(|e| e.to_string())?;
        self.stream.flush().map_err(|e| e.to_string())
    }

    pub fn send_pong(&mut self, payload: &[u8]) -> Result<(), String> {
        let mut frame = vec![0x8A, 0x80 | payload.len() as u8, 0, 0, 0, 0];
        frame.extend_from_slice(payload);
        self.stream.write_all(&frame).map_err(|e| e.to_string())
    }

    /// Next data message. Control frames are handled here; close returns Ok(None).
    pub fn recv(&mut self) -> Result<Option<Vec<u8>>, String> {
        loop {
            let (opcode, payload) = self.read_frame()?;
            match opcode {
                0x1 | 0x2 => return Ok(Some(payload)),
                0x8 => return Ok(None),
                0x9 => self.send_pong(&payload)?,
                0xA => {}
                other => return Err(format!("unsupported websocket opcode {other:#x}")),
            }
        }
    }

    fn read_frame(&mut self) -> Result<(u8, Vec<u8>), String> {
        let mut hdr = [0u8; 2];
        self.stream
            .read_exact(&mut hdr)
            .map_err(|e| e.to_string())?;
        let opcode = hdr[0] & 0x0f;
        let masked = hdr[1] & 0x80 != 0;
        let mut len = (hdr[1] & 0x7f) as u64;
        if len == 126 {
            let mut b = [0u8; 2];
            self.stream.read_exact(&mut b).map_err(|e| e.to_string())?;
            len = u16::from_be_bytes(b) as u64;
        } else if len == 127 {
            let mut b = [0u8; 8];
            self.stream.read_exact(&mut b).map_err(|e| e.to_string())?;
            len = u64::from_be_bytes(b);
        }
        if len > 8 * 1024 * 1024 {
            return Err(format!("websocket frame is {len} bytes"));
        }
        let mut mask = [0u8; 4];
        if masked {
            self.stream
                .read_exact(&mut mask)
                .map_err(|e| e.to_string())?;
        }
        let mut payload = vec![0u8; len as usize];
        self.stream
            .read_exact(&mut payload)
            .map_err(|e| e.to_string())?;
        if masked {
            for (i, b) in payload.iter_mut().enumerate() {
                *b ^= mask[i % 4];
            }
        }
        Ok((opcode, payload))
    }
}

fn read_headers(stream: &mut TcpStream) -> Result<String, String> {
    let mut buf = Vec::new();
    let mut byte = [0u8; 1];
    while !buf.ends_with(b"\r\n\r\n") {
        if buf.len() > 16 * 1024 {
            return Err("websocket handshake headers too large".into());
        }
        stream.read_exact(&mut byte).map_err(|e| e.to_string())?;
        buf.push(byte[0]);
    }
    String::from_utf8(buf).map_err(|e| e.to_string())
}

fn b64(bytes: &[u8]) -> String {
    const T: &[u8] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut out = String::new();
    let mut i = 0;
    while i + 3 <= bytes.len() {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8) | bytes[i + 2] as u32;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push(T[(n & 63) as usize] as char);
        i += 3;
    }
    let rest = bytes.len() - i;
    if rest == 1 {
        let n = (bytes[i] as u32) << 16;
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push('=');
        out.push('=');
    } else if rest == 2 {
        let n = ((bytes[i] as u32) << 16) | ((bytes[i + 1] as u32) << 8);
        out.push(T[((n >> 18) & 63) as usize] as char);
        out.push(T[((n >> 12) & 63) as usize] as char);
        out.push(T[((n >> 6) & 63) as usize] as char);
        out.push('=');
    }
    out
}

fn header<'a>(head: &'a str, name: &str) -> Option<&'a str> {
    head.lines().find_map(|line| {
        let (k, v) = line.split_once(':')?;
        k.trim().eq_ignore_ascii_case(name).then(|| v.trim())
    })
}

trait FirstAddr {
    fn to_socket_addrs_first(&self) -> std::io::Result<std::net::SocketAddr>;
}

impl FirstAddr for (&str, u16) {
    fn to_socket_addrs_first(&self) -> std::io::Result<std::net::SocketAddr> {
        use std::net::ToSocketAddrs;
        self.to_socket_addrs()?
            .next()
            .ok_or_else(|| std::io::Error::new(std::io::ErrorKind::NotFound, "no address"))
    }
}

/// `Url::is_loopback` is crate-private in livstyle-http; re-check here.
trait Loopback {
    fn is_loopback(&self) -> bool;
}

impl Loopback for Url {
    fn is_loopback(&self) -> bool {
        self.host.eq_ignore_ascii_case("localhost")
            || self
                .host
                .parse::<std::net::IpAddr>()
                .is_ok_and(|ip| ip.is_loopback())
    }
}
