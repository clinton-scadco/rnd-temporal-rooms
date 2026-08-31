//! `room serve`: two browsers, one factory.
//!
//! ```text
//!   GET  /                      the game
//!   GET  /api/catalogue         what may be placed, and what it does
//!   GET  /api/parts             the components a machine is designed from
//!   GET  /api/kit               experiment 08's meshes, for the 3D view
//!   GET  /api/rooms             rooms that are open, for the lobby
//!   POST /api/host              open one, and get a code
//!   POST /api/join              join one, with the code
//!   POST /api/start             begin the clock. There is no matching stop.
//!   POST /api/cmd               one intention; accepted or refused
//!   POST /api/presence          a cursor. Lossy on purpose
//!   GET  /api/state             one player's whole view of one frame
//!   POST /api/form              a design, built as a plant, for the 3D view
//!   POST /api/inside            what every component in it is doing, and why
//!   GET  /api/goals             every template, for the developer panel
//! ```
//!
//! # What the server is, and is not
//!
//! It is a third small HTTP server in `std`, for the same reason the machine
//! designer got the second one: this experiment must be possible to throw
//! away. It binds to loopback, speaks HTTP/1.1 with `Connection: close`, and
//! holds the only thing in this project that is genuinely *stateful* -- the
//! rooms, their clocks, and one reconstruction per player.
//!
//! That statefulness is the difference from every server above it. Prototype
//! 0 and 1 could be stateless because `state(log, T)` is a pure function and
//! the browser could hold the log. A multiplayer room cannot: two browsers
//! cannot both hold the authority, and the whole question of the experiment is
//! what happens when they try.
//!
//! # Polling
//!
//! The client polls `/api/state` a few times a second and posts intentions as
//! they happen. No sockets, no server push, no frame synchronisation. That is
//! enough because nothing in the protocol is timing-sensitive: a command is
//! stamped by the host when it arrives, and a client that polls late gets a
//! later tick rather than a different history.

use super::room::Room;
use super::world::PlayerId;
use crate::json::{self, Json};
use crate::machine::design::Design;
use std::collections::HashMap;
use std::io::{BufRead, BufReader, Read, Write};
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;

const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../../web/room/index.html")),
    ("/room.css", "text/css; charset=utf-8", include_str!("../../web/room/room.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../../web/room/app.js")),
    ("/net.js", "text/javascript; charset=utf-8", include_str!("../../web/room/net.js")),
    ("/world.js", "text/javascript; charset=utf-8", include_str!("../../web/room/world.js")),
    ("/bench.js", "text/javascript; charset=utf-8", include_str!("../../web/room/bench.js")),
    ("/panels.js", "text/javascript; charset=utf-8", include_str!("../../web/room/panels.js")),
    // Experiment 10's renderer, unchanged and unforked. The machine designer's
    // window is the machine designer's window; this prototype only decides
    // which design goes in it.
    // Served at the path it lives at, so that `../machine/form.js` resolves to
    // the same file whether it is being loaded by a browser over HTTP or by
    // the front end's own test harness off the disk.
    (
        "/machine/form.js",
        "text/javascript; charset=utf-8",
        include_str!("../../web/machine/form.js"),
    ),
];

static ROOMS: Mutex<Option<HashMap<String, Room>>> = Mutex::new(None);

fn with_rooms<R>(f: impl FnOnce(&mut HashMap<String, Room>) -> R) -> R {
    let mut g = ROOMS.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(HashMap::new))
}

// -------------------------------------------------------------------- serve

pub fn serve(port: u16) -> std::io::Result<()> {
    let listener = bind(port)?;
    let addr = listener.local_addr()?;
    println!("prototype 2 is at   http://{addr}/");
    println!("one browser hosts, the other joins with the code.");
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

impl Req {
    fn q(&self, k: &str) -> String {
        self.query.get(k).cloned().unwrap_or_default()
    }
    fn json(&self) -> Json {
        json::parse(&self.body).unwrap_or(Json::Null)
    }
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

const MIME: &str = "application/json; charset=utf-8";

fn route(req: &Req) -> (&'static str, &'static str, String) {
    if req.method == "GET" {
        if let Some((_, m, body)) = ASSETS.iter().find(|(p, _, _)| *p == req.path) {
            return ("200 OK", m, body.to_string());
        }
        match req.path.as_str() {
            "/api/catalogue" => return ok(super::kit::catalogue()),
            "/api/parts" => {
                return ok(Json::obj()
                    .set("ok", true)
                    .set("parts", Design::catalogue())
                    .set("portKinds", crate::machine::design::port_kinds())
                    .set("substances", crate::machine::design::substances()))
            }
            "/api/kit" => return ok(crate::machine::form::kit_json()),
            "/api/goals" => return ok(goals()),
            "/api/rooms" => return ok(rooms()),
            "/api/state" => {
                let (code, player) = (req.q("code"), req.q("player"));
                return answer(with_rooms(|rs| match rs.get_mut(&code) {
                    Some(r) => r.view(player.parse().unwrap_or(0)),
                    None => Err(format!("there is no room {code}")),
                }));
            }
            _ => {}
        }
    }
    if req.method == "POST" {
        match req.path.as_str() {
            "/api/host" => return answer(host(&req.json())),
            "/api/join" => return answer(join(&req.json())),
            "/api/start" => return answer(start(&req.json())),
            "/api/cmd" => return answer(command(&req.json())),
            "/api/presence" => return answer(presence(&req.json())),
            "/api/form" => return answer(form(req)),
            "/api/inside" => return answer(inside(req)),
            _ => {}
        }
    }
    ("404 Not Found", MIME, err("no such route").to_string())
}

fn ok(j: Json) -> (&'static str, &'static str, String) {
    ("200 OK", MIME, j.to_string())
}

fn answer(r: Result<Json, String>) -> (&'static str, &'static str, String) {
    match r {
        Ok(j) => ("200 OK", MIME, j.to_string()),
        Err(e) => ("200 OK", MIME, err(&e).to_string()),
    }
}

fn err(msg: &str) -> Json {
    Json::obj().set("ok", false).set("error", msg)
}

// ------------------------------------------------------------------- routes

/// Open a room. The seed decides the code and the goal together, so a room
/// somebody wants to play again is one number long.
fn host(j: &Json) -> Result<Json, String> {
    let seed = j.at("seed").as_u64().unwrap_or_else(fresh_seed);
    let template = j.at("template").as_str().filter(|t| !t.is_empty());
    let name = j.at("name").as_str().unwrap_or("host").to_string();
    with_rooms(|rs| {
        let mut room = Room::open(seed, template);
        if rs.contains_key(&room.code) {
            return Err(format!("room {} is already open", room.code));
        }
        let code = room.code.clone();
        // The host is a player like any other: their browser is a client of
        // the authority, even though the authority is in the same process.
        //
        // The clock is *not* started here. Section 19 of the brief asks for
        // the objective to be on screen before anybody builds, and that is the
        // only pause the game has: once `/api/start` is called there is no
        // matching stop.
        let id = room.join(&name)?;
        let goal = room.goal.clone();
        let progress = room.host.progress();
        rs.insert(code.clone(), room);
        Ok(Json::obj()
            .set("ok", true)
            .set("code", code)
            .set("player", id as i64)
            .set("seed", Json::big(seed as u128))
            .set("goal", goal.to_json(&progress)))
    })
}

fn join(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_uppercase();
    let name = j.at("name").as_str().unwrap_or("player").to_string();
    with_rooms(|rs| {
        let room = rs.get_mut(&code).ok_or(format!("there is no room {code}"))?;
        let id = room.join(&name)?;
        Ok(Json::obj()
            .set("ok", true)
            .set("code", code.clone())
            .set("player", id as i64)
            .set("joinedAt", room.player(id).map(|p| Json::Int(p.joined as i128))))
    })
}

fn start(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_uppercase();
    with_rooms(|rs| {
        let room = rs.get_mut(&code).ok_or(format!("there is no room {code}"))?;
        room.start();
        Ok(Json::obj().set("ok", true).set("tick", room.now()))
    })
}

/// One intention.
///
/// The answer is deliberately small: whether it was accepted, and the
/// canonical `(tick, sequence)` if it was. Everything else the client needs
/// arrives on the next poll, from the same reconstruction every other client
/// is being shown.
fn command(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_uppercase();
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let act = super::cmd::Cmd::from_json(
        &Json::obj()
            .set("type", j.at("type").clone())
            .set("payload", j.at("payload").clone()),
    )
    .map(|c| c.act)?;
    with_rooms(|rs| {
        let room = rs.get_mut(&code).ok_or(format!("there is no room {code}"))?;
        match room.submit(player, act) {
            Ok(c) => Ok(Json::obj().set("ok", true).set("command", c.to_json())),
            // A refusal is an answer, not an error: the client is told which
            // command and why, and the room carries on exactly as it was.
            Err(e) => Ok(Json::obj().set("ok", false).set("refused", true).set("error", e)),
        }
    })
}

/// A cursor, a selection, and which window somebody is looking at.
///
/// None of it is ordered, none of it is hashed, and losing a packet of it
/// costs nothing at all.
fn presence(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_uppercase();
    let id: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    with_rooms(|rs| {
        let room = rs.get_mut(&code).ok_or(format!("there is no room {code}"))?;
        let Some(p) = room.players.iter_mut().find(|p| p.id == id) else {
            return Err("you are not in this room".into());
        };
        let c = j.at("cursor");
        p.cursor = match (c.at("x").as_f64(), c.at("y").as_f64()) {
            (Some(x), Some(y)) => Some((x, y)),
            _ => None,
        };
        p.selection = j.at("selection").as_u64();
        p.editing = j.at("editing").as_u64();
        if let Some(v) = j.at("view").as_str() {
            p.view = v.to_string();
        }
        Ok(Json::obj().set("ok", true))
    })
}

/// A design, built as a plant, for the 3D window.
///
/// The document comes from the room rather than from the browser: a client
/// that could post any design here would be drawing something nobody else can
/// see. `draft` asks for the one being edited; without it you get the design
/// that is actually running.
///
/// The style is not a choice here, the way it is in the designer. The room's
/// 3D window is an *editor*: the only reason to open a machine is to look at
/// its components and place another one, and `works` and `hall` both answer
/// that by putting walls and a roof between the player and the thing they
/// came in to work on. So a room always builds the yard -- a slab, and the
/// plant standing on it in the open -- and the enclosure a machine would
/// otherwise wear is a question for the designer, where nobody is editing.
fn form(req: &Req) -> Result<Json, String> {
    let j = req.json();
    let code = j.at("code").as_str().unwrap_or_default().to_uppercase();
    let id = j.at("id").as_u64().unwrap_or(0);
    let want_draft = j.at("draft").as_bool().unwrap_or(false);
    let grade = req.q("grade");
    let seed = req.q("seed").parse().unwrap_or(0);
    let design = with_rooms(|rs| {
        let room = rs.get(&code).ok_or(format!("there is no room {code}"))?;
        let i = room.host.world.get(id).ok_or("there is no such machine")?;
        let d = if want_draft { i.draft.clone().or_else(|| i.design.clone()) } else { i.design.clone() };
        d.ok_or_else(|| "that installation has no design".to_string())
    })?;
    let ask = crate::machine::form::Ask {
        style: crate::machine::form::Style::Yard,
        world: seed,
        grade: crate::machine::form::Grade::by_tag(&grade).unwrap_or_default(),
    };
    let scene = crate::machine::form::build(&design, ask)?;
    Ok(scene
        .to_json()
        .set("ok", true)
        .set("design", design.to_json())
        .set("source", design.emit()))
}

/// What every component inside one machine is doing, and why it is not doing
/// more.
///
/// The phase is the room's clock read through the machine's own orbit: a
/// designer tick is a game second, and a settled machine at second 4,000 is
/// indistinguishable from itself at `transient + 4000 % period`. So the
/// diagnostics are the machine's, at the moment the room is at, without the
/// inner simulation ever being stepped by the outer one.
///
/// What this deliberately does **not** show is the machine starved by the
/// *world*. Inside, it is running on its design's own supply; outside, the
/// world's inspector says whether the bay it draws from is empty. Two
/// altitudes, two answers, and the outer one is the authority on whether the
/// machine turned at all.
fn inside(req: &Req) -> Result<Json, String> {
    let j = req.json();
    let code = j.at("code").as_str().unwrap_or_default().to_uppercase();
    let id = j.at("id").as_u64().unwrap_or(0);
    let want_draft = j.at("draft").as_bool().unwrap_or(false);
    let (design, now) = with_rooms(|rs| {
        let room = rs.get(&code).ok_or(format!("there is no room {code}"))?;
        let i = room.host.world.get(id).ok_or("there is no such machine")?;
        let d = if want_draft {
            i.draft.clone().or_else(|| i.design.clone())
        } else {
            i.design.clone()
        };
        Ok::<_, String>((d.ok_or_else(|| "that installation has no design".to_string())?, room.now()))
    })?;
    let c = crate::machine::orbit::compile(&design)?;
    let r = crate::machine::eval::report(&design, &c);
    let phase = c.equivalent_tick(now / super::DESIGN_TICK);
    let m = c.state_at(&design, phase)?;
    Ok(crate::machine::snap::render(&design, &m, &r)
        .set("ok", true)
        .set("phase", phase as i64)
        .set("period", c.period as i64)
        .set("transient", c.transient as i64))
}

fn rooms() -> Json {
    with_rooms(|rs| {
        let mut open: Vec<Json> = rs
            .values()
            .map(|r| {
                Json::obj()
                    .set("code", r.code.clone())
                    .set("goal", r.goal.title.clone())
                    .set("brief", r.goal.brief())
                    .set("players", r.players.len() as i64)
                    .set("tick", r.now())
                    .set("seconds", super::as_secs(r.now()))
            })
            .collect();
        open.sort_by_key(|j| j.at("code").as_str().unwrap_or_default().to_string());
        Json::obj().set("ok", true).set("rooms", Json::Arr(open))
    })
}

/// Every template, with the range each of its numbers is drawn from -- for a
/// developer who wants to force one rather than roll for it.
fn goals() -> Json {
    Json::obj().set("ok", true).set(
        "templates",
        Json::Arr(
            super::goal::TEMPLATES
                .iter()
                .map(|t| {
                    let sample = super::goal::Goal::of_seed(1, Some(t.id));
                    Json::obj()
                        .set("id", t.id)
                        .set("family", t.family.word())
                        .set("title", t.title)
                        .set("note", t.note)
                        .set("example", sample.brief())
                })
                .collect(),
        ),
    )
}

/// A seed nobody chose. The clock is the only entropy this program has, and
/// the only place it is allowed to be used: everything downstream of the seed
/// is a pure function of it.
fn fresh_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    super::hash64(&t.as_nanos().to_le_bytes())
}

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
