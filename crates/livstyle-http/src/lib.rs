//! Minimal blocking HTTP/1.1 client restricted to loopback hosts.
//!
//! Local services (the Cursor SDK bridge, Ollama / llama-server judges) only
//! speak plain HTTP on loopback; refusing other hosts keeps credentials such
//! as the bridge bearer token from ever leaving the machine.

use std::io::{self, BufRead, BufReader, Read, Write};
use std::net::{IpAddr, TcpStream, ToSocketAddrs};
use std::time::Duration;

use thiserror::Error;

#[derive(Debug, Error)]
pub enum HttpError {
    #[error("invalid url {0:?}")]
    InvalidUrl(String),
    #[error("refusing non-loopback host {0:?}")]
    NotLoopback(String),
    #[error("io: {0}")]
    Io(#[from] io::Error),
    #[error("malformed response: {0}")]
    Malformed(String),
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Url {
    pub host: String,
    pub port: u16,
    pub path: String,
}

impl Url {
    pub fn parse(url: &str) -> Result<Self, HttpError> {
        let invalid = || HttpError::InvalidUrl(url.to_owned());
        let rest = url.strip_prefix("http://").ok_or_else(invalid)?;
        let (authority, path) = match rest.find('/') {
            Some(i) => (&rest[..i], &rest[i..]),
            None => (rest, "/"),
        };
        let (host, port) = if let Some(v6) = authority.strip_prefix('[') {
            let (h, p) = v6.split_once(']').ok_or_else(invalid)?;
            (h.to_owned(), p.strip_prefix(':').unwrap_or("80"))
        } else {
            match authority.rsplit_once(':') {
                Some((h, p)) => (h.to_owned(), p),
                None => (authority.to_owned(), "80"),
            }
        };
        if host.is_empty() {
            return Err(invalid());
        }
        Ok(Self {
            host,
            port: port.parse().map_err(|_| invalid())?,
            path: path.to_owned(),
        })
    }

    /// Joins `path` onto this base URL's path.
    pub fn join(&self, path: &str) -> Self {
        let base = self.path.trim_end_matches('/');
        Self {
            host: self.host.clone(),
            port: self.port,
            path: format!("{base}/{}", path.trim_start_matches('/')),
        }
    }

    fn is_loopback(&self) -> bool {
        if self.host.eq_ignore_ascii_case("localhost") {
            return true;
        }
        self.host.parse::<IpAddr>().is_ok_and(|ip| ip.is_loopback())
    }
}

#[derive(Debug, Clone)]
pub struct Request<'a> {
    pub method: &'a str,
    pub url: &'a Url,
    pub headers: Vec<(&'a str, String)>,
    pub body: &'a [u8],
    pub connect_timeout: Duration,
    /// `None` waits forever, which streaming RPCs need between keepalives.
    pub read_timeout: Option<Duration>,
}

impl<'a> Request<'a> {
    pub fn post(url: &'a Url, body: &'a [u8]) -> Self {
        Self {
            method: "POST",
            url,
            headers: Vec::new(),
            body,
            connect_timeout: Duration::from_secs(5),
            read_timeout: Some(Duration::from_secs(60)),
        }
    }

    pub fn get(url: &'a Url) -> Self {
        Self {
            method: "GET",
            ..Self::post(url, &[])
        }
    }

    pub fn header(mut self, name: &'a str, value: impl Into<String>) -> Self {
        self.headers.push((name, value.into()));
        self
    }

    pub fn read_timeout(mut self, timeout: Option<Duration>) -> Self {
        self.read_timeout = timeout;
        self
    }
}

pub struct Response {
    pub status: u16,
    pub headers: Vec<(String, String)>,
    body: Body,
}

impl std::fmt::Debug for Response {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.debug_struct("Response")
            .field("status", &self.status)
            .field("headers", &self.headers)
            .finish_non_exhaustive()
    }
}

impl Response {
    pub fn header(&self, name: &str) -> Option<&str> {
        self.headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.as_str())
    }

    pub fn is_success(&self) -> bool {
        (200..300).contains(&self.status)
    }

    /// Streaming body reader (chunked encoding already decoded).
    pub fn into_reader(self) -> impl Read {
        self.body
    }

    pub fn read_all(self) -> Result<Vec<u8>, HttpError> {
        let mut out = Vec::new();
        self.into_reader().read_to_end(&mut out)?;
        Ok(out)
    }
}

pub fn send(request: Request<'_>) -> Result<Response, HttpError> {
    let url = request.url;
    if !url.is_loopback() {
        return Err(HttpError::NotLoopback(url.host.clone()));
    }
    let addr = (url.host.as_str(), url.port)
        .to_socket_addrs()?
        .next()
        .ok_or_else(|| HttpError::InvalidUrl(url.host.clone()))?;
    let mut stream = TcpStream::connect_timeout(&addr, request.connect_timeout)?;
    stream.set_read_timeout(request.read_timeout)?;
    stream.set_nodelay(true)?;

    let host = if url.host.contains(':') {
        format!("[{}]:{}", url.host, url.port)
    } else {
        format!("{}:{}", url.host, url.port)
    };
    let mut head = format!(
        "{} {} HTTP/1.1\r\nHost: {host}\r\nConnection: close\r\nContent-Length: {}\r\n",
        request.method,
        url.path,
        request.body.len()
    );
    for (name, value) in &request.headers {
        head.push_str(name);
        head.push_str(": ");
        head.push_str(value);
        head.push_str("\r\n");
    }
    head.push_str("\r\n");
    stream.write_all(head.as_bytes())?;
    stream.write_all(request.body)?;
    stream.flush()?;

    let mut reader = BufReader::new(stream);
    let status_line = read_line(&mut reader)?;
    let status = status_line
        .split_whitespace()
        .nth(1)
        .and_then(|s| s.parse().ok())
        .ok_or_else(|| HttpError::Malformed(format!("status line {status_line:?}")))?;

    let mut headers = Vec::new();
    loop {
        let line = read_line(&mut reader)?;
        if line.is_empty() {
            break;
        }
        let (k, v) = line
            .split_once(':')
            .ok_or_else(|| HttpError::Malformed(format!("header {line:?}")))?;
        headers.push((k.trim().to_owned(), v.trim().to_owned()));
    }

    let find = |name: &str| {
        headers
            .iter()
            .find(|(k, _)| k.eq_ignore_ascii_case(name))
            .map(|(_, v)| v.clone())
    };
    let body =
        if find("transfer-encoding").is_some_and(|v| v.to_ascii_lowercase().contains("chunked")) {
            Body::Chunked {
                reader,
                remaining: 0,
                done: false,
            }
        } else if let Some(len) = find("content-length") {
            let len: u64 = len
                .parse()
                .map_err(|_| HttpError::Malformed(format!("content-length {len:?}")))?;
            Body::Fixed(reader.take(len))
        } else {
            Body::UntilClose(reader)
        };
    Ok(Response {
        status,
        headers,
        body,
    })
}

fn read_line(reader: &mut impl BufRead) -> Result<String, HttpError> {
    let mut line = String::new();
    let n = reader.read_line(&mut line)?;
    if n == 0 {
        return Err(HttpError::Malformed("unexpected end of stream".into()));
    }
    Ok(line.trim_end_matches(['\r', '\n']).to_owned())
}

enum Body {
    Fixed(io::Take<BufReader<TcpStream>>),
    UntilClose(BufReader<TcpStream>),
    Chunked {
        reader: BufReader<TcpStream>,
        remaining: u64,
        done: bool,
    },
}

impl Read for Body {
    fn read(&mut self, buf: &mut [u8]) -> io::Result<usize> {
        match self {
            Body::Fixed(r) => r.read(buf),
            Body::UntilClose(r) => r.read(buf),
            Body::Chunked {
                reader,
                remaining,
                done,
            } => read_chunked(reader, remaining, done, buf),
        }
    }
}

fn read_chunked(
    reader: &mut impl BufRead,
    remaining: &mut u64,
    done: &mut bool,
    buf: &mut [u8],
) -> io::Result<usize> {
    if *done || buf.is_empty() {
        return Ok(0);
    }
    if *remaining == 0 {
        let mut size_line = String::new();
        if reader.read_line(&mut size_line)? == 0 {
            return Err(io::Error::new(
                io::ErrorKind::UnexpectedEof,
                "chunked body truncated",
            ));
        }
        let size_hex = size_line.trim().split(';').next().unwrap_or("");
        let size = u64::from_str_radix(size_hex, 16).map_err(|_| {
            io::Error::new(
                io::ErrorKind::InvalidData,
                format!("chunk size {size_line:?}"),
            )
        })?;
        if size == 0 {
            // Trailers until an empty line.
            loop {
                let mut line = String::new();
                if reader.read_line(&mut line)? == 0 || line.trim().is_empty() {
                    break;
                }
            }
            *done = true;
            return Ok(0);
        }
        *remaining = size;
    }
    let want = buf.len().min(*remaining as usize);
    let n = reader.read(&mut buf[..want])?;
    if n == 0 {
        return Err(io::Error::new(
            io::ErrorKind::UnexpectedEof,
            "chunk truncated",
        ));
    }
    *remaining -= n as u64;
    if *remaining == 0 {
        let mut crlf = [0u8; 2];
        reader.read_exact(&mut crlf)?;
    }
    Ok(n)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Cursor;
    use std::net::TcpListener;
    use std::thread;

    #[test]
    fn parses_urls() {
        let u = Url::parse("http://127.0.0.1:39217").unwrap();
        assert_eq!(
            (u.host.as_str(), u.port, u.path.as_str()),
            ("127.0.0.1", 39217, "/")
        );
        let j = Url::parse("http://localhost:11434/v1/")
            .unwrap()
            .join("/models");
        assert_eq!(j.path, "/v1/models");
        assert_eq!(Url::parse("http://[::1]:80/x").unwrap().host, "::1");
        assert!(Url::parse("https://example.com").is_err());
    }

    #[test]
    fn refuses_remote_hosts() {
        let u = Url::parse("http://10.0.0.5:80/").unwrap();
        assert!(matches!(
            send(Request::get(&u)),
            Err(HttpError::NotLoopback(_))
        ));
    }

    #[test]
    fn decodes_chunked() {
        let raw = b"4\r\nWiki\r\n5;ext=1\r\npedia\r\n0\r\nX-Trailer: a\r\n\r\n";
        let mut reader = BufReader::new(Cursor::new(&raw[..]));
        let (mut remaining, mut done) = (0, false);
        let mut out = Vec::new();
        let mut buf = [0u8; 3];
        loop {
            let n = read_chunked(&mut reader, &mut remaining, &mut done, &mut buf).unwrap();
            if n == 0 {
                break;
            }
            out.extend_from_slice(&buf[..n]);
        }
        assert_eq!(out, b"Wikipedia");
    }

    #[test]
    fn round_trip_against_local_server() {
        let listener = TcpListener::bind("127.0.0.1:0").unwrap();
        let port = listener.local_addr().unwrap().port();
        let server = thread::spawn(move || {
            let (mut s, _) = listener.accept().unwrap();
            let mut req = [0u8; 1024];
            let n = s.read(&mut req).unwrap();
            let text = String::from_utf8_lossy(&req[..n]).to_string();
            s.write_all(
                b"HTTP/1.1 200 OK\r\nTransfer-Encoding: chunked\r\n\r\n2\r\nok\r\n0\r\n\r\n",
            )
            .unwrap();
            text
        });
        let url = Url::parse(&format!("http://127.0.0.1:{port}/sdk.v1.X/Ping")).unwrap();
        let resp = send(Request::post(&url, b"{}").header("Authorization", "Bearer t")).unwrap();
        assert_eq!(resp.status, 200);
        assert_eq!(resp.read_all().unwrap(), b"ok");
        let seen = server.join().unwrap();
        assert!(seen.starts_with("POST /sdk.v1.X/Ping HTTP/1.1"));
        assert!(seen.contains("Authorization: Bearer t"));
    }
}
