//! The door every server in this project stands behind.
//!
//! Three of the experiments here ended up with a small HTTP server in `std`,
//! for the same reason each time: the alternative was a dependency tree larger
//! than the crate to move some JSON between a solver and a canvas. Keeping
//! them separate was deliberate -- a prototype that cannot be thrown away is
//! not a prototype -- and what was *not* deliberate was three copies of the
//! part that reads the socket.
//!
//! That only became visible when one of the copies turned out to be wrong. A
//! play session left six lines of `stream did not contain valid UTF-8` in a
//! log; the fix went into Prototype 2's server; and Prototype 3 -- the one
//! actually being played -- went on printing them, because it had its own.
//!
//! So the *routing* stays where it belongs, one server per experiment, and the
//! door is here. Nothing in this module knows what any of them serve.

use crate::json::{self, Json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::TcpStream;
use std::time::Duration;

/// One request, as far as any of the servers care about it.
pub struct Req {
    pub method: String,
    pub path: String,
    pub query: HashMap<String, String>,
    pub body: String,
}

impl Req {
    /// One query parameter, or the empty string. Absent and blank are the same
    /// answer to every caller here.
    pub fn q(&self, k: &str) -> String {
        self.query.get(k).cloned().unwrap_or_default()
    }

    /// The body, as JSON. A body that is not JSON is `Null`, which every route
    /// then reads its fields out of and gets nothing -- an answer, rather than
    /// a failed request nobody can act on.
    pub fn json(&self) -> Json {
        json::parse(&self.body).unwrap_or(Json::Null)
    }
}

/// A socket that went away. Nobody is waiting for the answer, so nobody needs
/// to read about it either: browsers abandon requests constantly -- a
/// navigation, a reload, a poll that outlived the page that asked for it --
/// and every one of them used to print a line that looked exactly like a bug.
pub fn hung_up(k: std::io::ErrorKind) -> bool {
    use std::io::ErrorKind::*;
    matches!(k, BrokenPipe | ConnectionReset | ConnectionAborted | TimedOut | WouldBlock)
}

/// A request head, or a reason there is not one.
enum Head {
    /// The request line and its headers, one string each.
    Req(Vec<String>),
    /// Bytes arrived, and they were not HTTP.
    Garbage,
    /// The connection opened and closed without saying anything.
    Empty,
}

const MAX_HEAD: usize = 16 * 1024;
const MAX_HEADERS: usize = 96;
const MAX_BODY: usize = 8 * 1024 * 1024;

/// How long a socket may say nothing before its thread is given back.
const IDLE: Duration = Duration::from_secs(30);

/// The head, read as bytes.
///
/// `BufRead::read_line` is shorter and is wrong, which took a play session and
/// a packet capture to find out. It requires UTF-8, and the first thing that
/// arrives on this port when a browser decides to try `https://` -- which
/// Chrome does on its own, for anything typed without a scheme -- is a TLS
/// ClientHello, whose random block is not UTF-8 and never will be. That failed
/// the whole request with `stream did not contain valid UTF-8`, dropped the
/// socket with no answer on it, and printed a line indistinguishable from a
/// real protocol fault. Six of them in a log is a mystery; six of them
/// explained is a browser being helpful.
///
/// So: bytes, a look at the first one before anything else, and a `400` for
/// whatever is not a request.
fn read_head(reader: &mut impl BufRead) -> std::io::Result<Head> {
    // One byte settles it. A TLS record begins `0x16`; an HTTP request begins
    // with a method, which is upper-case ASCII. Deciding here rather than at
    // the first newline matters because a ClientHello contains no newline at
    // all -- the old code sat on the socket until the browser gave up.
    match reader.fill_buf()?.first() {
        None => return Ok(Head::Empty),
        Some(b) if !b.is_ascii_uppercase() => return Ok(Head::Garbage),
        Some(_) => {}
    }
    let mut head: Vec<String> = Vec::new();
    let mut read = 0usize;
    loop {
        let mut raw = Vec::new();
        let n = reader.by_ref().take((MAX_HEAD - read) as u64).read_until(b'\n', &mut raw)?;
        if n == 0 {
            // EOF part-way through a head is a client that hung up, not a
            // request we can answer.
            return Ok(if head.is_empty() { Head::Empty } else { Head::Garbage });
        }
        read += n;
        while matches!(raw.last(), Some(b'\r' | b'\n')) {
            raw.pop();
        }
        if raw.is_empty() {
            return Ok(Head::Req(head));
        }
        match std::str::from_utf8(&raw) {
            Ok(line) => head.push(line.to_string()),
            Err(_) => return Ok(Head::Garbage),
        }
        if head.len() > MAX_HEADERS || read >= MAX_HEAD {
            return Ok(Head::Garbage);
        }
    }
}

/// A short answer for something that was not a request, so that even a browser
/// guessing at the wrong protocol is told rather than dropped.
pub fn refuse(stream: &TcpStream, status: &str, why: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: text/plain; charset=utf-8\r\n\
         Content-Length: {}\r\nConnection: close\r\n\r\n",
        why.len()
    );
    let mut out = stream;
    out.write_all(head.as_bytes())?;
    out.write_all(why.as_bytes())?;
    out.flush()
}

/// One answer, with its body.
pub fn reply(stream: &TcpStream, status: &str, mime: &str, payload: &str) -> std::io::Result<()> {
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    let mut out = stream;
    out.write_all(head.as_bytes())?;
    out.write_all(payload.as_bytes())?;
    out.flush()
}

/// One request off one socket, or `None` if there was not one to have.
///
/// `None` means the socket has already been dealt with -- answered with a
/// `400`, or found empty -- and the caller has nothing left to route. Every
/// path that returns it has either written something or established that
/// nobody is listening.
pub fn accept(stream: &TcpStream) -> std::io::Result<Option<Req>> {
    // A client that opens a socket and then says nothing must not hold a
    // thread for the rest of the session.
    let _ = stream.set_read_timeout(Some(IDLE));
    let mut reader = BufReader::new(stream.try_clone()?);
    let head = match read_head(&mut reader)? {
        Head::Empty => return Ok(None),
        Head::Garbage => {
            refuse(stream, "400 Bad Request", "this port speaks http, not https\n")?;
            return Ok(None);
        }
        Head::Req(h) => h,
    };
    let Some(start) = head.first() else {
        refuse(stream, "400 Bad Request", "no request line\n")?;
        return Ok(None);
    };
    let mut parts = start.split_whitespace();
    let (Some(method), Some(target)) = (parts.next(), parts.next()) else {
        refuse(stream, "400 Bad Request", "no request line\n")?;
        return Ok(None);
    };
    let (method, target) = (method.to_string(), target.to_string());

    let mut len = 0usize;
    for h in &head[1..] {
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
    }
    if len > MAX_BODY {
        refuse(stream, "413 Payload Too Large", "that is more than this room will hold\n")?;
        return Ok(None);
    }
    let mut body = vec![0u8; len];
    if len > 0 {
        reader.read_exact(&mut body)?;
    }
    let (path, qs) = match target.split_once('?') {
        Some((p, q)) => (p.to_string(), q.to_string()),
        None => (target.clone(), String::new()),
    };
    let query = qs
        .split('&')
        .filter(|s| !s.is_empty())
        .map(|kv| match kv.split_once('=') {
            Some((k, v)) => (k.to_string(), percent(v)),
            None => (kv.to_string(), String::new()),
        })
        .collect();
    Ok(Some(Req { method, path, query, body: String::from_utf8_lossy(&body).into_owned() }))
}

/// `%20` and `+`, undone. Anything that is not a valid escape is the character
/// it already was, because a query somebody typed by hand should not fail to
/// parse over a stray percent sign.
pub fn percent(s: &str) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        match b[i] {
            b'%' if i + 2 < b.len() => {
                let hex = std::str::from_utf8(&b[i + 1..i + 3]).unwrap_or("");
                match u8::from_str_radix(hex, 16) {
                    Ok(v) => {
                        out.push(v);
                        i += 3;
                    }
                    Err(_) => {
                        out.push(b[i]);
                        i += 1;
                    }
                }
            }
            b'+' => {
                out.push(b' ');
                i += 1;
            }
            c => {
                out.push(c);
                i += 1;
            }
        }
    }
    String::from_utf8_lossy(&out).into_owned()
}
