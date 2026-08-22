//! `trooms serve`: the workbench, over a socket.
//!
//! An HTTP server in `std` is a couple of hundred lines, and the alternative
//! was a dependency tree larger than this entire crate to move some JSON
//! between a solver and a canvas. It binds to loopback only, speaks HTTP/1.1
//! with `Connection: close`, and answers four kinds of question:
//!
//! ```text
//!   GET  /                     the workbench itself
//!   GET  /api/configs          the plants and scenarios on disk
//!   GET  /api/scenario?name=X  a scenario, and the plant it is posed about
//!   POST /api/state?t=N        the log, compiled, run to N, rendered
//!   POST /api/trace?t=N        the scheduler's own log of getting there
//!   POST /api/verify?t=N       the same tick, reached two ways, compared
//!   POST /api/save?name=X      a sketch, written to sketches/
//! ```
//!
//! The browser sends the whole *command log* every time and the server holds
//! no session. That is not laziness -- it is the same property the rest of the
//! crate turns on, and Prototype 1 did not weaken it: state at tick *T* is a
//! pure function of the log and *T*, so there is nothing to keep in sync,
//! nothing to invalidate, and a reload cannot desynchronise from a simulation
//! it does not own.
//!
//! What *is* cached is one [`Carry`]: the plant's state at the last tick
//! anyone asked about. Dragging a timeline asks the same plant the same
//! question five hundred times a second and the honest answer is usually "a
//! bit further than last time", so a forward seek resumes from the carry and a
//! backward seek starts again.
//!
//! Prototype 0 cached a compiled `Plan` and a live `Room` instead, which meant
//! leaking both into `'static` to escape a self-referential borrow. A carry is
//! plain owned data, so that machinery is gone -- and the thing that replaced
//! it is the same object the networking proof needs.

use crate::dsl;
use crate::graph::Graph;
use crate::json::{self, Json};
use crate::live::{self, Carry, Log};
use crate::model::Tick;
use crate::pop::Pop;
use crate::rooms::{self, Room};
use crate::scenario::{self, Scenario};
use crate::snap;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

// ------------------------------------------------------------------ assets

const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../web/index.html")),
    ("/app.css", "text/css; charset=utf-8", include_str!("../web/app.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../web/app.js")),
    ("/doc.js", "text/javascript; charset=utf-8", include_str!("../web/doc.js")),
    ("/canvas.js", "text/javascript; charset=utf-8", include_str!("../web/canvas.js")),
    ("/render.js", "text/javascript; charset=utf-8", include_str!("../web/render.js")),
    ("/panels.js", "text/javascript; charset=utf-8", include_str!("../web/panels.js")),
    ("/play.js", "text/javascript; charset=utf-8", include_str!("../web/play.js")),
];

// ------------------------------------------------------------------- server

pub fn serve(port: u16) -> std::io::Result<()> {
    let listener = bind(port)?;
    let addr = listener.local_addr()?;
    println!("the workbench is at  http://{addr}/");
    println!("plants come from     ./configs, scenarios from ./scenarios");
    println!("sketches are saved to ./sketches");
    println!("ctrl-c to stop.");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle(s) {
                        // A browser closing a tab mid-response is not news.
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
    let json_mime = "application/json; charset=utf-8";
    if req.method == "GET" {
        if let Some((_, mime, body)) = ASSETS.iter().find(|(p, _, _)| *p == req.path) {
            return ("200 OK", mime, body.to_string());
        }
        if req.path == "/api/configs" {
            return ("200 OK", json_mime, configs().to_string());
        }
        if req.path == "/api/config" {
            let name = req.query.get("name").cloned().unwrap_or_default();
            return match read_config(&name) {
                Ok(src) => ("200 OK", json_mime, open_source(&src).to_string()),
                Err(e) => ("404 Not Found", json_mime, err(&e).to_string()),
            };
        }
        if req.path == "/api/scenario" {
            let name = req.query.get("name").cloned().unwrap_or_default();
            return match open_scenario(&name) {
                Ok(j) => ("200 OK", json_mime, j.to_string()),
                Err(e) => ("404 Not Found", json_mime, err(&e).to_string()),
            };
        }
    }
    if req.method == "POST" {
        let t = || req.query.get("t").and_then(|s| s.parse().ok()).unwrap_or(0);
        match req.path.as_str() {
            "/api/open" => return ("200 OK", json_mime, open_source(&req.body).to_string()),
            "/api/state" => {
                let sc = req.query.get("scenario").cloned().unwrap_or_default();
                return ("200 OK", json_mime, state(&req.body, t(), &sc).to_string());
            }
            "/api/trace" => return ("200 OK", json_mime, trace(&req.body, t()).to_string()),
            "/api/verify" => return ("200 OK", json_mime, verify(&req.body, t()).to_string()),
            "/api/save" => {
                let name = req.query.get("name").cloned().unwrap_or_default();
                return ("200 OK", json_mime, save(&name, &req.body, t()).to_string());
            }
            _ => {}
        }
    }
    ("404 Not Found", json_mime, err("no such route").to_string())
}

fn err(msg: &str) -> Json {
    Json::obj().set("ok", false).set("error", msg)
}

// -------------------------------------------------------------- the routes

fn configs() -> Json {
    let mut names: Vec<String> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("configs") {
        for e in dir.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "factory") {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    names.push(n.to_string());
                }
            }
        }
    }
    let mut sketches: Vec<String> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("sketches") {
        for e in dir.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "factory") {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    sketches.push(n.to_string());
                }
            }
        }
    }
    let mut scenarios: Vec<String> = Vec::new();
    if let Ok(dir) = std::fs::read_dir("scenarios") {
        for e in dir.flatten() {
            let p = e.path();
            if p.extension().is_some_and(|x| x == "scenario") {
                if let Some(n) = p.file_name().and_then(|n| n.to_str()) {
                    scenarios.push(n.to_string());
                }
            }
        }
    }
    names.sort();
    sketches.sort();
    scenarios.sort();
    Json::obj()
        .set("ok", true)
        .set("configs", Json::arr(names))
        .set("sketches", Json::arr(sketches))
        .set("scenarios", Json::arr(scenarios))
}

/// A scenario, and the plant it is posed about, in one answer -- because a
/// client that fetched them separately could be shown a brief for a factory it
/// had not loaded yet.
fn open_scenario(name: &str) -> Result<Json, String> {
    let file = safe_named(name, "scenario")?;
    let src = std::fs::read_to_string(format!("scenarios/{file}"))
        .map_err(|_| format!("no scenario called `{name}`"))?;
    let sc = scenario::parse(&src).map_err(|e| format!("{file}: {e}"))?;
    let plant = read_config(&sc.plant)?;
    let opened = open_source(&plant);
    if opened.at("ok").as_bool() != Some(true) {
        return Ok(opened);
    }
    Ok(Json::obj()
        .set("ok", true)
        .set("scenario", sc.to_json())
        .set("graph", opened.at("graph").clone())
        .set("source", opened.at("source").clone()))
}

fn load_scenario(name: &str) -> Result<Scenario, String> {
    let file = safe_named(name, "scenario")?;
    let src = std::fs::read_to_string(format!("scenarios/{file}"))
        .map_err(|_| format!("no scenario called `{name}`"))?;
    scenario::parse(&src).map_err(|e| format!("{file}: {e}"))
}

fn read_config(name: &str) -> Result<String, String> {
    let name = safe_name(name)?;
    for dir in ["configs", "sketches"] {
        let path = format!("{dir}/{name}");
        if let Ok(s) = std::fs::read_to_string(&path) {
            return Ok(s);
        }
    }
    Err(format!("no plant called `{name}`"))
}

/// Source in, document out. Opening a hand-written `.factory` on the canvas is
/// the same operation as opening one the canvas wrote.
fn open_source(src: &str) -> Json {
    let prog = match dsl::parse(src) {
        Ok(p) => p,
        Err(e) => return live::Fault::of_dsl(&e, src).to_json(),
    };
    let mut g = Graph::from_program(&prog);
    g.apply_positions(src);
    Json::obj().set("ok", true).set("graph", g.to_json()).set("source", src.to_string())
}

/// The document the browser sent.
///
/// A client that has not caught up with Prototype 1 -- or a test, or a curl --
/// may still post a bare `graph`, which is a log with nothing having happened
/// to it yet.
fn incoming(body: &str) -> Result<Log, Json> {
    let j = json::parse(body).map_err(|e| err(&format!("malformed request: {e}")))?;
    if !j.at("log").is_null() {
        return Log::from_json(j.at("log")).map_err(|e| err(&e));
    }
    Ok(Log::new(Graph::from_json(j.at("graph")).map_err(|e| err(&e))?))
}

fn state(body: &str, t: Tick, scenario_name: &str) -> Json {
    let log = match incoming(body) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let mut out = with_log(&log, t);
    if out.at("ok").as_bool() != Some(true) || scenario_name.is_empty() {
        return out;
    }
    out = match load_scenario(scenario_name) {
        Ok(sc) => match scenario::evaluate(&sc, &log, t) {
            Ok(j) => out.set("play", j),
            Err(f) => return f.to_json(),
        },
        Err(e) => out.set("play", err(&e)),
    };
    out
}

fn trace(body: &str, t: Tick) -> Json {
    let log = match incoming(body) {
        Ok(l) => l,
        Err(e) => return e,
    };
    match live::timetable(&log, t) {
        Ok(tt) => Json::obj().set("ok", true).set("timetable", tt),
        Err(f) => f.to_json(),
    }
}

/// The same tick, reached two ways, compared.
///
/// This is the networking proof rehearsed against itself. One run starts at
/// tick 0 and plays the whole log; the other takes the canonical snapshot at
/// the halfway point, throws the first half away, and replays the rest. If the
/// signatures differ, a joining client would have desynchronised -- and this
/// is the cheapest possible place to find that out.
fn verify(body: &str, t: Tick) -> Json {
    let log = match incoming(body) {
        Ok(l) => l,
        Err(e) => return e,
    };
    let whole = match live::carry_at(&log, t) {
        Ok(c) => c,
        Err(f) => return f.to_json(),
    };
    let half = t / 2;
    let mid = match live::carry_at(&log, half) {
        Ok(c) => c,
        Err(f) => return f.to_json(),
    };
    let joined =
        match live::with_state_from(&log, t, Some((half, &mid)), |a| Carry::take(a.room, a.prog, a.bp, t))
        {
            Ok(c) => c,
            Err(f) => return f.to_json(),
        };
    let a = whole.signature();
    let b = joined.signature();
    Json::obj()
        .set("ok", true)
        .set("tick", t)
        .set("joinedAt", half)
        .set("matches", a == b)
        .set("bytes", a.len())
        .set("digest", digest(&a))
        .set("joinedDigest", digest(&b))
        .set("commands", log.commands.iter().filter(|c| c.at <= t).count())
}

/// A short, stable fingerprint of a signature, so two of them can be compared
/// by eye in a panel.
fn digest(v: &[u8]) -> String {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in v {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    format!("{h:016x}")
}

fn save(name: &str, body: &str, t: Tick) -> Json {
    let log = match incoming(body) {
        Ok(l) => l,
        Err(e) => return e,
    };
    // What gets written is the plant as it stands at `t`: a sketch is a
    // factory, not a history of one. The history is the log, and the log is
    // the browser's to keep.
    let src = match log.graph_at(t) {
        Ok(g) => g.emit(),
        Err(f) => return f.to_json(),
    };
    // Compiling before writing means a sketch on disk is always a plant the
    // harness can run.
    if let Err(e) = dsl::parse(&src) {
        return live::Fault::of_dsl(&e, &src).to_json();
    }
    let name = match safe_name(name) {
        Ok(n) => n,
        Err(e) => return err(&e),
    };
    if std::fs::create_dir_all("sketches").is_err() {
        return err("cannot create ./sketches");
    }
    let path = format!("sketches/{name}");
    match std::fs::write(&path, &src) {
        Ok(()) => Json::obj().set("ok", true).set("path", path),
        Err(e) => err(&format!("cannot write {path}: {e}")),
    }
}

/// `configs/` and `sketches/` are the only places this server reads or writes,
/// and a name is a file name -- never a path.
fn safe_name(name: &str) -> Result<String, String> {
    safe_named(name, "factory")
}

fn safe_named(name: &str, ext: &str) -> Result<String, String> {
    let stem: String = name
        .trim_end_matches(&format!(".{ext}"))
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if stem.is_empty() || stem.contains("..") {
        return Err(format!("`{name}` is not a usable name"));
    }
    Ok(format!("{stem}.{ext}"))
}

// --------------------------------------------------------------- the cache

/// The plant's state at the last tick anybody asked about.
///
/// One entry, not a map. A session is one person dragging one timeline, and
/// the question they ask five hundred times a second is always about the tick
/// just after the last one -- so the only cache worth having is the answer to
/// that.
///
/// `key` is the identity of the log *up to* `at`, which is what makes appended
/// commands safe: a purchase made at tick 40,000 does not invalidate a state
/// cached at 12,000, because the log up to 12,000 has not changed.
struct Cache {
    key: String,
    at: Tick,
    carry: Carry,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// The whole answer about a log at tick `t`, resuming from the cache when the
/// cache is about this plant and about this plant's past.
fn with_log(log: &Log, t: Tick) -> Json {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        // A panic in another request must not take the tool down with it.
        Err(p) => p.into_inner(),
    };
    let resume = guard.as_ref().and_then(|c| {
        (c.at <= t && c.key == log.key(c.at)).then(|| (c.at, c.carry.clone()))
    });
    let answered = live::with_state_from(log, t, resume.as_ref().map(|(a, c)| (*a, c)), |a| {
        (
            Json::obj()
                .set("ok", true)
                .set("source", a.source.to_string())
                .set("graph", a.graph.to_json())
                .set("plant", snap::plant(a.prog, a.bp, a.plan, a.room))
                .set("snapshot", snap::render(a.prog, a.bp, a.plan, a.room, t))
                .set(
                    "scrapped",
                    Json::Arr(
                        a.scrapped
                            .iter()
                            .map(|s| {
                                Json::obj()
                                    .set("what", s.what.clone())
                                    .set("detail", s.detail.clone())
                            })
                            .collect(),
                    ),
                )
                .set("resumedFrom", resume.as_ref().map(|(at, _)| Json::Int(*at as i128))),
            Carry::take(a.room, a.prog, a.bp, t),
        )
    });
    match answered {
        Ok((j, carry)) => {
            *guard = Some(Cache { key: log.key(t), at: t, carry });
            j
        }
        Err(f) => {
            // A plant that does not compile leaves the cache alone: the state
            // it holds is still a true statement about an earlier tick.
            f.to_json()
        }
    }
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

/// A deterministic trace of one plant, self-contained: everything a viewer
/// needs to render it at any of the sampled ticks with no simulator present.
///
/// This is the same snapshot the workbench asks for, taken at a schedule of
/// ticks and written to one file -- which is all "render the factory at an
/// arbitrary tick" needs once the answers are already known.
pub fn export(path: &str, ticks: &[Tick]) -> Result<String, String> {
    let src = std::fs::read_to_string(path).map_err(|e| format!("cannot read {path}: {e}"))?;
    let prog = dsl::parse(&src).map_err(|e| format!("{path}: {e}"))?;
    let d = prog.deploys[0];
    let bp = &prog.blueprints[d.blueprint as usize];
    let plan = rooms::plan(bp);
    let n_items = prog.items.len();

    let mut g = Graph::from_program(&prog);
    g.apply_positions(&src);

    let mut frames: Vec<Json> = Vec::new();
    let mut room = Room::new(&plan, n_items);
    let mut sorted = ticks.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    for &t in &sorted {
        room.run_until(t);
        frames.push(snap::render(&prog, bp, &plan, &room, t));
    }

    // The timetable is a separate run because it needs the trace switched on
    // from the start.
    let mut traced = Room::new(&plan, n_items);
    traced.trace = Some(Vec::new());
    traced.run_until(*sorted.last().unwrap_or(&0));

    // And the monolithic solver, on the same probes, so the trace carries its
    // own cross-validation rather than asking to be trusted.
    let mut agree = true;
    for &t in &sorted {
        let mut check = Room::new(&plan, n_items);
        check.run_until(t);
        let mut mono = Pop::new(bp, n_items);
        mono.run_until(t);
        if check.signature(bp) != mono.signature() || check.counters() != mono.c {
            agree = false;
        }
    }

    let doc = Json::obj()
        .set("plant", snap::plant(&prog, bp, &plan, &room))
        .set("graph", g.to_json())
        .set("source", src)
        .set("frames", Json::Arr(frames))
        .set("timetable", snap::timetable(&traced))
        .set("probes", Json::arr(sorted.iter().map(|&t| t as i64).collect::<Vec<_>>()))
        .set("verified", agree);
    Ok(doc.to_string())
}
