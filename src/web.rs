//! `trooms serve`: the workbench, over a socket.
//!
//! An HTTP server in `std` is a couple of hundred lines, and the alternative
//! was a dependency tree larger than this entire crate to move some JSON
//! between a solver and a canvas. It binds to loopback only, speaks HTTP/1.1
//! with `Connection: close`, and answers four kinds of question:
//!
//! ```text
//!   GET  /                     the workbench itself
//!   GET  /api/configs          the plants on disk
//!   POST /api/state?t=N        the graph, compiled, run to N, rendered
//!   POST /api/trace?t=N        the scheduler's own log of getting there
//!   POST /api/save?name=X      a sketch, written to sketches/
//! ```
//!
//! The browser sends the whole document every time and the server holds no
//! session. That is not laziness -- it is the same property the rest of the
//! crate turns on: state at tick *T* is a pure function of the plant and *T*,
//! so there is nothing to keep in sync, nothing to invalidate, and a reload
//! cannot desynchronise from a simulation it does not own.
//!
//! What *is* cached is the compiled plan and a Room already advanced to some
//! tick, because dragging a timeline asks the same plant the same question
//! five hundred times a second and the honest answer is usually "a bit further
//! than last time". A forward seek advances the Room it has; a backward seek
//! builds a new one.

use crate::dsl;
use crate::graph::Graph;
use crate::json::{self, Json};
use crate::model::{Blueprint, Program, Tick};
use crate::pop::Pop;
use crate::rooms::{self, Plan, Room};
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
];

// ------------------------------------------------------------------- server

pub fn serve(port: u16) -> std::io::Result<()> {
    let listener = bind(port)?;
    let addr = listener.local_addr()?;
    println!("the workbench is at  http://{addr}/");
    println!("plants come from     ./configs, sketches are saved to ./sketches");
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
    }
    if req.method == "POST" {
        match req.path.as_str() {
            "/api/open" => return ("200 OK", json_mime, open_source(&req.body).to_string()),
            "/api/state" => {
                let t = req.query.get("t").and_then(|s| s.parse().ok()).unwrap_or(0);
                return ("200 OK", json_mime, state(&req.body, t).to_string());
            }
            "/api/trace" => {
                let t = req.query.get("t").and_then(|s| s.parse().ok()).unwrap_or(0);
                return ("200 OK", json_mime, trace(&req.body, t).to_string());
            }
            "/api/save" => {
                let name = req.query.get("name").cloned().unwrap_or_default();
                return ("200 OK", json_mime, save(&name, &req.body).to_string());
            }
            _ => {}
        }
    }
    ("404 Not Found", json_mime, err("no such route").to_string())
}

fn err(msg: &str) -> Json {
    Json::obj().set("ok", false).set("error", msg)
}

/// A DSL error, and the node it is probably about. The generated source is
/// one declaration per line, so the line number is enough to name a node --
/// which is what puts a red ring on the canvas rather than a line number in a
/// panel nobody is looking at.
fn dsl_error(e: &dsl::DslError, src: &str) -> Json {
    let line = src.lines().nth(e.line.saturating_sub(1)).unwrap_or("");
    let node = line
        .split_whitespace()
        .find(|w| {
            !matches!(
                *w,
                "shared" | "source" | "storage" | "process" | "sink" | "link" | "wire"
            )
        })
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string())
        .filter(|w| !w.is_empty());
    Json::obj()
        .set("ok", false)
        .set("error", e.msg.clone())
        .set("line", e.line)
        .set("node", node)
        .set("source", src.to_string())
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
    names.sort();
    sketches.sort();
    Json::obj()
        .set("ok", true)
        .set("configs", Json::arr(names))
        .set("sketches", Json::arr(sketches))
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
        Err(e) => return dsl_error(&e, src),
    };
    let mut g = Graph::from_program(&prog);
    g.apply_positions(src);
    Json::obj().set("ok", true).set("graph", g.to_json()).set("source", src.to_string())
}

fn compile(body: &str) -> Result<(Graph, String), Json> {
    let j = json::parse(body).map_err(|e| err(&format!("malformed request: {e}")))?;
    let g = Graph::from_json(j.at("graph")).map_err(|e| err(&e))?;
    let src = g.emit();
    Ok((g, src))
}

fn state(body: &str, t: Tick) -> Json {
    let (_, src) = match compile(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    with_plant(&src, t, |prog, bp, plan, room| {
        Json::obj()
            .set("ok", true)
            .set("source", src.clone())
            .set("plant", snap::plant(prog, bp, plan, room))
            .set("snapshot", snap::render(prog, bp, plan, room, t))
    })
}

fn trace(body: &str, t: Tick) -> Json {
    let (_, src) = match compile(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    let prog = match dsl::parse(&src) {
        Ok(p) => p,
        Err(e) => return dsl_error(&e, &src),
    };
    let d = prog.deploys[0];
    let bp = &prog.blueprints[d.blueprint as usize];
    let plan = rooms::plan(bp);
    let mut room = Room::new(&plan, prog.items.len());
    room.trace = Some(Vec::new());
    room.run_until(t);
    Json::obj().set("ok", true).set("timetable", snap::timetable(&room))
}

fn save(name: &str, body: &str) -> Json {
    let (_, src) = match compile(body) {
        Ok(v) => v,
        Err(e) => return e,
    };
    // Compiling before writing means a sketch on disk is always a plant the
    // harness can run.
    if let Err(e) = dsl::parse(&src) {
        return dsl_error(&e, &src);
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
    let stem: String = name
        .trim_end_matches(".factory")
        .chars()
        .filter(|c| c.is_ascii_alphanumeric() || *c == '-' || *c == '_' || *c == '.')
        .collect();
    if stem.is_empty() || stem.contains("..") {
        return Err(format!("`{name}` is not a usable plant name"));
    }
    Ok(format!("{stem}.factory"))
}

// --------------------------------------------------------------- the cache

/// A compiled plant, kept between requests.
///
/// `Room<'a>` borrows its `Plan`, so a cache holding both would be
/// self-referential. The plan is therefore leaked into `'static` and memoised
/// by source text: scrubbing a timeline reuses one plan for the whole session,
/// and only genuinely editing the plant costs another. A `Plan` is a handful
/// of blueprints of a few dozen nodes -- the billions live in counts -- so the
/// bound is a few kilobytes per distinct plant a session compiles.
struct Cache {
    plans: HashMap<String, &'static Plan>,
    progs: HashMap<String, &'static Program>,
    /// The plant the live Room belongs to, and how far it has run.
    live: Option<(String, Tick, Room<'static>)>,
}

static CACHE: Mutex<Option<Cache>> = Mutex::new(None);

/// Answer a question about a plant at tick `t`, reusing whatever is already
/// compiled and however far it has already run.
fn with_plant(
    src: &str,
    t: Tick,
    f: impl FnOnce(&Program, &Blueprint, &Plan, &Room) -> Json,
) -> Json {
    let mut guard = match CACHE.lock() {
        Ok(g) => g,
        // A panic in another request must not take the tool down with it.
        Err(p) => p.into_inner(),
    };
    let cache = guard.get_or_insert_with(|| Cache {
        plans: HashMap::new(),
        progs: HashMap::new(),
        live: None,
    });

    if !cache.plans.contains_key(src) {
        let prog = match dsl::parse(src) {
            Ok(p) => p,
            Err(e) => return dsl_error(&e, src),
        };
        if prog.deploys.is_empty() {
            return err("the plant is never deployed");
        }
        let prog: &'static Program = Box::leak(Box::new(prog));
        let d = prog.deploys[0];
        let bp = &prog.blueprints[d.blueprint as usize];
        let plan: &'static Plan = Box::leak(Box::new(rooms::plan(bp)));
        cache.plans.insert(src.to_string(), plan);
        cache.progs.insert(src.to_string(), prog);
        cache.live = None;
    }
    let plan = cache.plans[src];
    let prog = cache.progs[src];
    let bp = &prog.blueprints[prog.deploys[0].blueprint as usize];

    // Reuse the running Room when this is the same plant and the question is
    // about its future; rebuild when it is about its past.
    let reuse = match &cache.live {
        Some((s, tick, _)) => s == src && *tick <= t,
        None => false,
    };
    if !reuse {
        cache.live = Some((src.to_string(), 0, Room::new(plan, prog.items.len())));
    }
    let (_, tick, room) = cache.live.as_mut().unwrap();
    room.run_until(t);
    *tick = t;
    f(prog, bp, plan, room)
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
