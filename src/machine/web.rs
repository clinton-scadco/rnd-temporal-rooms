//! `machine serve`: the designer, over a socket.
//!
//! This is a second small HTTP server rather than a route added to the
//! workbench's, on purpose. Experiment 06 is supposed to be possible to throw
//! away, and a prototype that has already grown roots into the tool everything
//! else depends on is not. The hundred lines of `std` here are the price of
//! that, and they are the cheapest part of the experiment.
//!
//! ```text
//!   GET  /                    the designer
//!   GET  /api/catalogue       the eight components, their ports and numbers
//!   GET  /api/designs         what is on disk
//!   GET  /api/design?name=X   one of them, as a document and as source
//!   POST /api/open            source in, document out
//!   POST /api/state?t=N       a document, run to N, rendered
//!   POST /api/compile         the macro-machine: transient, orbit, waveform
//!   POST /api/verify?t=N      the compiled answer against a straight run
//!   POST /api/save?name=X     write it to designs/
//! ```
//!
//! The browser posts the whole document every time and the server holds no
//! session, for the same reason the workbench does not: state is a pure
//! function of `(design, t)`, so there is nothing to keep in sync and a reload
//! cannot desynchronise from a simulation it does not own.
//!
//! What *is* cached is one compiled machine and one stepped-to position in it,
//! because dragging a timeline asks the same question five hundred times a
//! second and the honest answer is usually "a bit further than last time".

use super::design::Design;
use super::eval;
use super::orbit::{self, Compiled};
use super::sim::{Machine, Tick};
use super::snap;
use crate::json::{self, Json};
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../../web/machine/index.html")),
    ("/machine.css", "text/css; charset=utf-8", include_str!("../../web/machine/machine.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../../web/machine/app.js")),
    ("/doc.js", "text/javascript; charset=utf-8", include_str!("../../web/machine/doc.js")),
    ("/canvas.js", "text/javascript; charset=utf-8", include_str!("../../web/machine/canvas.js")),
    ("/render.js", "text/javascript; charset=utf-8", include_str!("../../web/machine/render.js")),
    ("/panels.js", "text/javascript; charset=utf-8", include_str!("../../web/machine/panels.js")),
];

/// Where designs are read from and written to. The only directory this server
/// touches.
const DIR: &str = "designs";

// -------------------------------------------------------------------- serve

pub fn serve(port: u16) -> std::io::Result<()> {
    let listener = bind(port)?;
    let addr = listener.local_addr()?;
    println!("the machine designer is at  http://{addr}/");
    println!("designs come from and go to ./{DIR}");
    println!("ctrl-c to stop.");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle(s) {
                        if e.kind() != std::io::ErrorKind::BrokenPipe {
                            eprintln!("request failed: {e}");
                        }
                    }
                });
            }
            Err(e) => eprintln!("accept failed: {e}"),
        }
    }
    Ok(())
}

fn bind(port: u16) -> std::io::Result<TcpListener> {
    let mut last = None;
    for p in port..port + 8 {
        match TcpListener::bind(("127.0.0.1", p)) {
            Ok(l) => return Ok(l),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap())
}

struct Req {
    method: String,
    path: String,
    query: HashMap<String, String>,
    body: String,
}

fn handle(stream: TcpStream) -> std::io::Result<()> {
    let mut reader = BufReader::new(stream.try_clone()?);
    let mut line = String::new();
    if reader.read_line(&mut line)? == 0 {
        return Ok(());
    }
    let mut parts = line.split_whitespace();
    let method = parts.next().unwrap_or("GET").to_string();
    let target = parts.next().unwrap_or("/").to_string();

    let mut len = 0usize;
    loop {
        let mut h = String::new();
        if reader.read_line(&mut h)? == 0 || h.trim().is_empty() {
            break;
        }
        if let Some(v) = h.to_ascii_lowercase().strip_prefix("content-length:") {
            len = v.trim().parse().unwrap_or(0);
        }
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

    let req = Req { method, path, query, body: String::from_utf8_lossy(&body).into_owned() };
    let (status, mime, payload) = route(&req);
    let mut out = stream;
    let head = format!(
        "HTTP/1.1 {status}\r\nContent-Type: {mime}\r\nContent-Length: {}\r\n\
         Cache-Control: no-store\r\nConnection: close\r\n\r\n",
        payload.len()
    );
    out.write_all(head.as_bytes())?;
    out.write_all(payload.as_bytes())?;
    out.flush()
}

fn route(req: &Req) -> (&'static str, &'static str, String) {
    let mime = "application/json; charset=utf-8";
    if req.method == "GET" {
        if let Some((_, m, body)) = ASSETS.iter().find(|(p, _, _)| *p == req.path) {
            return ("200 OK", m, body.to_string());
        }
        match req.path.as_str() {
            "/api/catalogue" => {
                return (
                    "200 OK",
                    mime,
                    Json::obj()
                        .set("ok", true)
                        .set("parts", Design::catalogue())
                        .set("portKinds", super::design::port_kinds())
                        .set("constants", eval::constants())
                        .to_string(),
                )
            }
            "/api/designs" => return ("200 OK", mime, designs().to_string()),
            "/api/design" => {
                let name = req.query.get("name").cloned().unwrap_or_default();
                return match read_design(&name) {
                    Ok(src) => ("200 OK", mime, opened(&src).to_string()),
                    Err(e) => ("404 Not Found", mime, err(&e).to_string()),
                };
            }
            _ => {}
        }
    }
    if req.method == "POST" {
        let t = || req.query.get("t").and_then(|s| s.parse().ok()).unwrap_or(0);
        match req.path.as_str() {
            "/api/open" => return ("200 OK", mime, opened(&req.body).to_string()),
            "/api/state" => return ("200 OK", mime, state(&req.body, t()).to_string()),
            "/api/compile" => return ("200 OK", mime, compiled(&req.body).to_string()),
            "/api/verify" => return ("200 OK", mime, verified(&req.body, t()).to_string()),
            "/api/save" => {
                let name = req.query.get("name").cloned().unwrap_or_default();
                return ("200 OK", mime, save(&name, &req.body).to_string());
            }
            _ => {}
        }
    }
    ("404 Not Found", mime, err("no such route").to_string())
}

fn err(msg: &str) -> Json {
    Json::obj().set("ok", false).set("error", msg)
}

// ------------------------------------------------------------------- routes

fn designs() -> Json {
    let mut names: Vec<String> = Vec::new();
    if let Ok(dir) = std::fs::read_dir(DIR) {
        for e in dir.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "machine") {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    names.push(n.to_string());
                }
            }
        }
    }
    names.sort();
    Json::obj().set("ok", true).set("designs", Json::arr(names))
}

fn read_design(name: &str) -> Result<String, String> {
    let name = safe_name(name)?;
    std::fs::read_to_string(format!("{DIR}/{name}"))
        .map_err(|_| format!("no design called `{name}`"))
}

/// Source in, document out. Opening a hand-written `.machine` is the same
/// operation as opening one the canvas wrote.
fn opened(src: &str) -> Json {
    match Design::parse(src) {
        Ok(d) => Json::obj()
            .set("ok", true)
            .set("design", d.to_json())
            .set("source", src.to_string()),
        Err(e) => err(&e),
    }
}

fn incoming(body: &str) -> Result<Design, Json> {
    let j = json::parse(body).map_err(|e| err(&format!("malformed request: {e}")))?;
    let d = if j.at("design").is_null() { j } else { j.at("design").clone() };
    Design::from_json(&d).map_err(|e| err(&e))
}

/// The document, checked, compiled, run to `t`, and rendered.
fn state(body: &str, t: Tick) -> Json {
    let d = match incoming(body) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let faults = d.check();
    if !faults.is_empty() {
        // A document that cannot be simulated is still a document you can look
        // at -- a component you have just placed and not yet wired is exactly
        // that, and it has to appear on the canvas or there is no wiring it.
        return Json::obj()
            .set("ok", false)
            .set("error", faults[0].what.clone())
            .set("design", d.to_json())
            .set("source", d.emit())
            .set(
                "faults",
                Json::Arr(
                    faults
                        .iter()
                        .map(|f| {
                            Json::obj()
                                .set("what", f.what.clone())
                                .set("unit", f.unit.clone())
                        })
                        .collect(),
                ),
            );
    }
    at(&d, t, |c, m| {
        let r = eval::report(&d, c);
        Json::obj()
            .set("ok", true)
            .set("design", d.to_json())
            .set("source", d.emit())
            .set("snapshot", snap::render(&d, m, &r))
            .set("macro", eval::macro_machine(&d, c, &r))
            .set("equivalentTick", c.equivalent_tick(t) as i64)
            .set("totals", totals_json(&c.totals_at(t)))
    })
}

fn totals_json(t: &super::sim::Totals) -> Json {
    Json::obj()
        .set("ticks", Json::big(t.ticks))
        .set("power", Json::big(t.power))
        .set("fuel", Json::big(t.fuel))
        .set("water", Json::big(t.water))
        .set("heatWasted", Json::big(t.heat_wasted))
        .set("steamVented", Json::big(t.steam_vented))
}

/// The compiled macro-machine, and the picture of its orbit.
fn compiled(body: &str) -> Json {
    let d = match incoming(body) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let c = match orbit::compile(&d) {
        Ok(c) => c,
        Err(e) => return err(&e),
    };
    let r = eval::report(&d, &c);
    let (wave, stride) = c.waveform(560);
    Json::obj()
        .set("ok", true)
        .set("macro", eval::macro_machine(&d, &c, &r))
        .set("report", r.to_json())
        .set("transient", c.transient as i64)
        .set("period", c.period as i64)
        .set("searched", c.searched as i64)
        .set("settled", c.settled())
        .set("stride", stride as i64)
        .set("wave", Json::arr(wave.iter().map(|&v| v as i64).collect::<Vec<_>>()))
}

/// The compiled answer against the thing it summarises.
fn verified(body: &str, t: Tick) -> Json {
    let d = match incoming(body) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let t = t.max(1).min(200_000);
    let probes: Vec<Tick> = [1, t / 8, t / 4, t / 2, t * 3 / 4, t]
        .iter()
        .copied()
        .filter(|&x| x > 0)
        .collect();
    match orbit::verify(&d, &probes) {
        Ok(checks) => {
            let all = checks.iter().all(|c| c.agrees);
            Json::obj()
                .set("ok", true)
                .set("agrees", all)
                .set(
                    "checks",
                    Json::Arr(
                        checks
                            .iter()
                            .map(|c| {
                                Json::obj()
                                    .set("tick", c.tick as i64)
                                    .set("agrees", c.agrees)
                                    .set("simulated", Json::big(c.simulated.power))
                                    .set("compiled", Json::big(c.compiled.power))
                            })
                            .collect(),
                    ),
                )
        }
        Err(e) => err(&e),
    }
}

fn save(name: &str, body: &str) -> Json {
    let d = match incoming(body) {
        Ok(d) => d,
        Err(e) => return e,
    };
    let src = d.emit();
    // Parsing before writing means a design on disk is always one the CLI can
    // run.
    if let Err(e) = Design::parse(&src) {
        return err(&format!("the design did not survive being written down: {e}"));
    }
    let name = match safe_name(name) {
        Ok(n) => n,
        Err(e) => return err(&e),
    };
    if std::fs::create_dir_all(DIR).is_err() {
        return err(&format!("cannot create ./{DIR}"));
    }
    let path = format!("{DIR}/{name}");
    match std::fs::write(&path, &src) {
        Ok(()) => Json::obj().set("ok", true).set("path", path),
        Err(e) => err(&format!("cannot write {path}: {e}")),
    }
}

/// `designs/` is the only place this server reads or writes, and a name is a
/// file name -- never a path.
fn safe_name(name: &str) -> Result<String, String> {
    let stem: String = name
        .trim_end_matches(".machine")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if stem.is_empty() || stem.contains("..") {
        return Err(format!("`{name}` is not a usable name"));
    }
    Ok(format!("{stem}.machine"))
}

// -------------------------------------------------------------------- cache

/// One compiled machine, and one position inside it.
///
/// `key` is the design's own source text, which is the cheapest exact identity
/// a document has -- and unlike a hash it can be compared without wondering
/// about collisions.
struct Cache {
    key: String,
    compiled: Compiled,
    at: Tick,
    machine: Machine,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Answer a question about `d` at tick `t`, reusing whatever the last question
/// left behind.
fn at(d: &Design, t: Tick, f: impl FnOnce(&Compiled, &Machine) -> Json) -> Json {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        // A panic in another request must not take the tool down with it.
        Err(p) => p.into_inner(),
    };
    let key = d.emit();
    let fresh = guard.as_ref().map(|c| c.key != key).unwrap_or(true);
    if fresh {
        let compiled = match orbit::compile(d) {
            Ok(c) => c,
            Err(e) => return err(&e),
        };
        let machine = match Machine::new(d) {
            Ok(m) => m,
            Err(e) => return err(&e),
        };
        *guard = Some(Cache { key, compiled, at: 0, machine });
    }
    let c = guard.as_mut().unwrap();

    // The whole trick, in three lines: the tick the player asked for is
    // indistinguishable from a tick inside the transient or the first orbit, so
    // that is the one actually simulated -- and if the last question left us
    // behind it, walk forward rather than starting again.
    let want = c.compiled.equivalent_tick(t);
    if want < c.at {
        c.machine = match Machine::new(d) {
            Ok(m) => m,
            Err(e) => return err(&e),
        };
        c.at = 0;
    }
    while c.at < want {
        c.machine.step();
        c.at += 1;
    }
    c.machine.tick = t;
    f(&c.compiled, &c.machine)
}

/// Percent-decoding, for query strings.
fn percent(s: &str) -> String {
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
