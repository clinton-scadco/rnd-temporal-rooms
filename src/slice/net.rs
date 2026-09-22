//! `slice serve`: three centuries of one valley, in a browser.
//!
//! ```text
//!   GET  /                    the slice
//!   GET  /api/regions         the map: three regions, two fractures, the lanes
//!   GET  /api/land            fifteen features, three faces each, with colours
//!   GET  /api/catalogue       what may be placed here, and what the book's
//!                             version of it would cost in this century
//!   GET  /api/parts           the components, priced in the century you are in
//!   GET  /api/kit             experiment 08's meshes, for the 3D view
//!   GET  /api/slice           the slice frame: regions, fractures, provenance
//!   GET  /api/state           one region's frame -- Prototype 2's contract
//!   POST /api/enter           join, and get a player id
//!   POST /api/start           begin the clock. There is no matching stop
//!   POST /api/travel          stand in another century
//!   POST /api/cmd             one intention, in one region
//!   POST /api/route           open, retune or close a supply relationship
//!   POST /api/gate            put an interface on a fracture, or take it down
//!   POST /api/presence        a cursor. Lossy on purpose
//!   POST /api/form            a design, built as a plant, for the 3D view
//!   POST /api/inside          what every component in it is doing, and why
//! ```
//!
//! # It is still the same client
//!
//! `/api/state`, `/api/cmd`, `/api/presence`, `/api/form` and `/api/inside`
//! answer exactly what Prototype 2's server answered, with `code` naming a
//! region of the slice rather than a hosted game -- so `web/room/`'s world
//! view, machine bench and panels are served here unchanged and unforked, for
//! the third time. Prototype 3 made that claim about a campaign; this makes it
//! about a *century*, which is a harder one, because walking through a fracture
//! changes what may be built and what it costs and the room view does not have
//! to know.
//!
//! The one thing this front end adds below the shell is a canvas: `terrain.js`
//! paints [`land`](super::land) *underneath* the plot, using the projection
//! `world.js` exports and touching nothing. Crossing a fracture repaints the
//! valley -- the wood becomes a street, the ore body becomes a foundry
//! somebody else built -- while the same unforked renderer draws the factory on
//! top of it.
//!
//! # Two things the palette does that camp's does not
//!
//! A component is never hidden and never locked. It is *priced*, per century,
//! in gears that somebody later has to make:
//!
//! ```text
//!   motor in 2037     native
//!   motor in 1890     1,600 gears, out of 2037
//!   mains in 1890     no crate of one would help
//! ```
//!
//! and the palette says which, because the entire argument of this experiment
//! is that the wall is logistical rather than magical. A player who can see
//! that a motor costs 1,600 gears has a supply problem. A player who sees a
//! greyed-out box has a tech tree.
//!
//! # One slice per process
//!
//! As with the campaign: there is one, it is made on the first request, and
//! `POST /api/enter` puts you in it.

use super::run::Slice;
use super::{gate, land, phase, region};
use crate::http::{self, Req};
use crate::json::Json;
use crate::machine::design::Design;
use crate::mp::world::PlayerId;
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../../web/slice/index.html")),
    ("/slice.css", "text/css; charset=utf-8", include_str!("../../web/slice/slice.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../../web/slice/app.js")),
    ("/map.js", "text/javascript; charset=utf-8", include_str!("../../web/slice/map.js")),
    ("/shell.js", "text/javascript; charset=utf-8", include_str!("../../web/slice/shell.js")),
    ("/terrain.js", "text/javascript; charset=utf-8", include_str!("../../web/slice/terrain.js")),
    // Prototype 2's client, unchanged and unforked, at the paths its own
    // imports resolve to.
    ("/room/net.js", "text/javascript; charset=utf-8", include_str!("../../web/room/net.js")),
    ("/room/world.js", "text/javascript; charset=utf-8", include_str!("../../web/room/world.js")),
    ("/room/bench.js", "text/javascript; charset=utf-8", include_str!("../../web/room/bench.js")),
    ("/room/panels.js", "text/javascript; charset=utf-8", include_str!("../../web/room/panels.js")),
    ("/room/room.css", "text/css; charset=utf-8", include_str!("../../web/room/room.css")),
    (
        "/machine/form.js",
        "text/javascript; charset=utf-8",
        include_str!("../../web/machine/form.js"),
    ),
];

static SLICE: Mutex<Option<Slice>> = Mutex::new(None);

fn with_slice<R>(f: impl FnOnce(&mut Slice) -> R) -> R {
    let mut g = SLICE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| Slice::open(fresh_seed())))
}

/// Throw the world away and make another one. Only `POST /api/enter` with a
/// seed does this, and only before anybody has joined.
fn reopen(seed: u64) {
    let mut g = SLICE.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(Slice::open(seed));
}

// -------------------------------------------------------------------- serve

pub fn serve(host: &str, port: u16) -> std::io::Result<()> {
    let listener = bind(host, port)?;
    let addr = listener.local_addr()?;
    beat();
    println!("experiment 15 is at   http://{addr}/");
    if addr.ip().is_unspecified() {
        println!("bound to every interface: other machines on this network can join.");
    }
    println!("three centuries of one valley, on one clock. everybody joins the same slice.");
    println!("ctrl-c to stop.");
    for stream in listener.incoming() {
        match stream {
            Ok(s) => {
                std::thread::spawn(move || {
                    if let Err(e) = handle(s) {
                        if !http::hung_up(e.kind()) {
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

/// The slice's own thread.
///
/// Three regions advance whether anybody is polling or not, and so do the
/// fractures: an interface that only asked whether it was holding when a
/// browser happened to look would be a fracture whose power bill depended on
/// somebody's tab being in the foreground.
fn beat() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(crate::mp::HEARTBEAT_MS));
        let mut g = SLICE.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(s) = g.as_mut() {
            s.heartbeat();
        }
    });
}

fn bind(host: &str, port: u16) -> std::io::Result<TcpListener> {
    let mut last = None;
    for p in port..port + 8 {
        match TcpListener::bind((host, p)) {
            Ok(l) => return Ok(l),
            Err(e) => last = Some(e),
        }
    }
    Err(last.unwrap())
}

fn handle(stream: TcpStream) -> std::io::Result<()> {
    let Some(req) = http::accept(&stream)? else { return Ok(()) };
    let (status, mime, payload) = route(&req);
    http::reply(&stream, status, mime, &payload)
}

const MIME: &str = "application/json; charset=utf-8";

fn route(req: &Req) -> (&'static str, &'static str, String) {
    if req.method == "GET" {
        if let Some((_, m, body)) = ASSETS.iter().find(|(p, _, _)| *p == req.path) {
            return ("200 OK", m, body.to_string());
        }
        match req.path.as_str() {
            "/api/regions" => return ok(regions()),
            "/api/land" => return ok(land::to_json().set("ok", true)),
            "/api/kit" => return ok(crate::machine::form::kit_json()),
            "/api/reference" => return ok(crate::mp::net::reference()),
            "/api/catalogue" => return ok(catalogue(&req.q("code").to_lowercase())),
            "/api/parts" => return ok(parts(&req.q("code").to_lowercase())),
            "/api/slice" => {
                let player = req.q("player").parse().unwrap_or(0);
                return answer(with_slice(|s| s.to_json(player)));
            }
            "/api/state" => {
                let (code, player) = (req.q("code").to_lowercase(), req.q("player"));
                return answer(with_slice(|s| s.look(player.parse().unwrap_or(0), &code)));
            }
            _ => {}
        }
    }
    if req.method == "POST" {
        match req.path.as_str() {
            "/api/enter" => return answer(enter(&req.json())),
            "/api/start" => {
                return answer(with_slice(|s| {
                    s.start();
                    Ok(Json::obj().set("ok", true).set("tick", s.now()))
                }))
            }
            "/api/travel" => return answer(travel(&req.json())),
            "/api/cmd" => return answer(command(&req.json())),
            "/api/route" => return answer(route_cmd(&req.json())),
            "/api/gate" => return answer(gate_cmd(&req.json())),
            "/api/presence" => return answer(presence(&req.json())),
            "/api/form" => return answer(form(req)),
            "/api/inside" => return answer(inside(&req.json())),
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

/// The map: three regions, two fractures, eight lanes and three fleets, none of
/// which change.
fn regions() -> Json {
    Json::obj()
        .set("ok", true)
        .set(
            "regions",
            Json::Arr(region::REGIONS.iter().map(region::Region::to_json).collect()),
        )
        .set("phases", phase::phases())
        .set("fractures", Json::Arr(gate::FRACTURES.iter().map(|f| f.to_json()).collect()))
        .set("lanes", Json::Arr(gate::LANES.iter().map(|l| l.to_json()).collect()))
        .set("fleets", Json::Arr(gate::FLEETS.iter().map(gate::Fleet::to_json).collect()))
        .set("plot", land::PLOT as i64)
}

/// Which century a request is about, defaulting to the earliest.
///
/// A palette is a fact about *where you are standing*, which is the whole
/// difference between this server and Prototype 3's: the same button means
/// three different prices depending on which side of a fracture the browser is
/// on.
fn phase_of(code: &str) -> phase::Phase {
    region::region(code).map(|(_, r)| r.phase).unwrap_or(phase::Phase::P1890)
}

/// The palette, priced for the century the player is standing in.
///
/// Nothing is hidden and almost nothing is locked. A prototype carries what its
/// *stock* design would cost to stand up here, because that is the question a
/// player actually has -- "if I place this and build the usual thing in it,
/// what do I owe?" -- and it carries a refusal only where no quantity of gears
/// would help.
fn catalogue(code: &str) -> Json {
    let p = phase_of(code);
    let base = crate::mp::kit::catalogue();
    let free = with_slice(|s| s.free_machinery(code));
    let protos: Vec<Json> = base
        .at("protos")
        .as_arr()
        .iter()
        .map(|j| {
            let tag = j.at("tag").as_str().unwrap_or_default().to_string();
            let Ok(d) = crate::mp::world::stock_design(&tag) else {
                // A chassis with no stock design costs nothing to put down, and
                // what goes in it is the bench's problem.
                return j.clone().set("stockCost", 0).set("illegal", false);
            };
            let illegal = phase::legal(&d, p).err();
            let cost = phase::design_cost(&d, p);
            j.clone()
                .set("stockCost", Json::big(cost as u128))
                .set("illegal", illegal.is_some())
                .set("why", illegal)
                .set("short", (cost as i64 - free).max(0))
                .set(
                    "imports",
                    Json::Arr(
                        phase::imported(&d, p)
                            .iter()
                            .map(|(unit, kind, c)| {
                                Json::obj()
                                    .set("unit", unit.clone())
                                    .set("part", kind.tag())
                                    .set("title", kind.title())
                                    .set("gears", Json::big(*c as u128))
                            })
                            .collect(),
                    ),
                )
        })
        .collect();
    base.clone()
        .set("protos", Json::Arr(protos))
        .set("phase", p.tag())
        .set("plot", land::PLOT as i64)
        .set("freeMachinery", free)
}

/// The components, priced in the century the player is standing in.
///
/// This is the panel the experiment is really about. Open the bench in 1890 and
/// the motor in the palette says *1,600 gears, out of 2037*; open the same
/// bench in 2037 and it says nothing at all, because there it is just a motor.
fn parts(code: &str) -> Json {
    let p = phase_of(code);
    let free = with_slice(|s| s.free_machinery(code));
    let list: Vec<Json> = Design::catalogue()
        .as_arr()
        .iter()
        .map(|j| {
            let tag = j.at("kind").as_str().unwrap_or_default();
            let Some(k) = crate::machine::parts::by_tag(tag) else { return j.clone() };
            let m = crate::machine::design::Tune::default_for(k).mat;
            let cost = phase::import_cost(k, m, p);
            let a = phase::arrival(tag);
            j.clone()
                .set("native", p.builds_on(k, m))
                .set("gears", Json::big(cost as u128))
                .set("crateable", phase::crateable(k))
                .set("arrives", a.map(|a| Json::Str(a.at.tag().to_string())))
                .set("madeIn", phase::made_in(k, m).map(|f| Json::Str(f.tag().to_string())))
                .set("why", a.map(|a| Json::Str(a.why.to_string())))
                // The frame is the other half, and it is the half a player can
                // do something about this afternoon: a crusher in 1890 is free
                // the moment somebody writes `frame iron` on it.
                .set(
                    "frames",
                    Json::Arr(
                        crate::machine::parts::phys(k)
                            .mats
                            .iter()
                            .map(|mat| {
                                Json::obj()
                                    .set("tag", mat.tag())
                                    .set("title", mat.title())
                                    .set("here", phase::frame_ok(k, *mat, p))
                            })
                            .collect(),
                    ),
                )
        })
        .collect();
    Json::obj()
        .set("ok", true)
        .set("parts", Json::Arr(list))
        .set("portKinds", crate::machine::design::port_kinds())
        .set("substances", crate::machine::design::substances())
        .set("phase", p.tag())
        .set("freeMachinery", free)
}

/// Arrive, or come back.
fn enter(j: &Json) -> Result<Json, String> {
    let name = j.at("name").as_str().unwrap_or("player").to_string();
    let key = j.at("key").as_str().unwrap_or_default().to_string();
    let back = j.at("back").as_bool().unwrap_or(false);
    if back {
        return with_slice(|s| {
            if !s.seated(&key) {
                return Err("your seat in this slice is not there any more".into());
            }
            let (id, rejoined) = s.join_as("", &key)?;
            Ok(seat(s, id, rejoined))
        });
    }
    if let Some(seed) = j.at("seed").as_u64() {
        let empty = with_slice(|s| s.cast.is_empty() && !s.started);
        if empty {
            reopen(seed);
        }
    }
    with_slice(|s| {
        let (id, rejoined) = s.join_as(&name, &key)?;
        Ok(seat(s, id, rejoined))
    })
}

/// Everything a browser needs to be in the slice, however it got there --
/// including which century it was standing in when it was last here.
fn seat(s: &mut Slice, id: PlayerId, rejoined: bool) -> Json {
    let here = s.who(id).map(|w| w.at).unwrap_or(0);
    Json::obj()
        .set("ok", true)
        .set("player", id as i64)
        .set("name", s.who(id).map(|w| Json::Str(w.name.clone())))
        .set("rejoined", rejoined)
        .set("code", s.code.clone())
        .set("seed", Json::big(s.seed as u128))
        .set("started", s.started)
        .set("at", region::REGIONS[here].tag)
}

/// Walk into another century. One string, exactly as Prototype 3's walk
/// between rooms was -- the regions have all been running the whole time.
fn travel(j: &Json) -> Result<Json, String> {
    let id: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let to = j.at("region").as_str().unwrap_or_default().to_lowercase();
    with_slice(|s| {
        s.travel(id, &to)?;
        Ok(Json::obj()
            .set("ok", true)
            .set("at", to.clone())
            .set("phase", s.phase_of(&to).map(|p| Json::Str(p.tag().to_string()))))
    })
}

/// One intention, in one century. Refused answers are answers, not errors.
fn command(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let act = crate::mp::cmd::Cmd::from_json(
        &Json::obj()
            .set("type", j.at("type").clone())
            .set("payload", j.at("payload").clone()),
    )
    .map(|c| c.act)?;
    with_slice(|s| match s.submit(player, &code, act) {
        Ok(cmd) => Ok(Json::obj().set("ok", true).set("command", cmd.to_json())),
        Err(e) => Ok(Json::obj().set("ok", false).set("refused", true).set("error", e)),
    })
}

fn route_cmd(j: &Json) -> Result<Json, String> {
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let what = j.at("do").as_str().unwrap_or("open");
    with_slice(|s| match what {
        "open" => {
            let id = s.open_route(
                player,
                j.at("from").as_str().unwrap_or_default(),
                j.at("to").as_str().unwrap_or_default(),
                j.at("item").as_str().unwrap_or_default(),
                j.at("fleet").as_str().unwrap_or("wagon"),
                j.at("cap").as_u64(),
            )?;
            Ok(Json::obj().set("ok", true).set("route", id as i64))
        }
        "close" => {
            s.close_route(j.at("route").as_u64().unwrap_or(0) as u32)?;
            Ok(Json::obj().set("ok", true))
        }
        "cap" => {
            s.retune_route(
                j.at("route").as_u64().unwrap_or(0) as u32,
                j.at("cap").as_u64().unwrap_or(1),
            )?;
            Ok(Json::obj().set("ok", true))
        }
        other => Err(format!("`{other}` is not something to do to a route")),
    })
}

/// Put an interface on a fracture, or take one down.
///
/// The one command in this experiment that has no counterpart in any earlier
/// prototype, and it is deliberately the smallest: opening a fracture is not a
/// purchase and not a research. It is a declaration, and what decides whether
/// it *holds* is somebody's grid, every five simulated seconds, for as long as
/// it is open.
fn gate_cmd(j: &Json) -> Result<Json, String> {
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let what = j.at("do").as_str().unwrap_or("open");
    let tag = j.at("fracture").as_str().unwrap_or_default().to_lowercase();
    with_slice(|s| match what {
        "open" => {
            let i = s.open_gate(player, &tag)?;
            Ok(Json::obj().set("ok", true).set("fracture", gate::FRACTURES[i].tag))
        }
        "close" => {
            s.close_gate(&tag)?;
            Ok(Json::obj().set("ok", true))
        }
        other => Err(format!("`{other}` is not something to do to a fracture")),
    })
}

/// A cursor, a selection, and which window somebody is looking at.
fn presence(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let id: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    with_slice(|s| {
        let y = s.yard_mut(&code).ok_or(format!("there is no region called {code}"))?;
        let Some(p) = y.room.players.iter_mut().find(|p| p.id == id) else {
            return Err("you are not in this slice".into());
        };
        let cur = j.at("cursor");
        p.cursor = match (cur.at("x").as_f64(), cur.at("y").as_f64()) {
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
fn form(req: &Req) -> Result<Json, String> {
    let j = req.json();
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let id = j.at("id").as_u64().unwrap_or(0);
    let want_draft = j.at("draft").as_bool().unwrap_or(false);
    let grade = req.q("grade");
    let seed = req.q("seed").parse().unwrap_or(0);
    let design = with_slice(|s| -> Result<Design, String> {
        let y = s.yard(&code).ok_or(format!("there is no region called {code}"))?;
        let i = y.room.host.world.get(id).ok_or("there is no such machine")?;
        let d = if want_draft {
            i.draft.clone().or_else(|| i.design.clone())
        } else {
            i.design.clone()
        };
        Ok(d.unwrap_or_else(|| {
            let mut d = Design::empty();
            d.name = i.proto.title.to_string();
            d
        }))
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

/// What every component inside one machine is doing -- and, because this is the
/// experiment it is, what the machine as a whole owes in imported plant.
fn inside(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let id = j.at("id").as_u64().unwrap_or(0);
    let want_draft = j.at("draft").as_bool().unwrap_or(false);
    let (design, now, p) = with_slice(|s| {
        let now = s.now();
        let p = s.phase_of(&code).unwrap_or(phase::Phase::P1890);
        let y = s.yard(&code).ok_or(format!("there is no region called {code}"))?;
        let i = y.room.host.world.get(id).ok_or("there is no such machine")?;
        let d = if want_draft {
            i.draft.clone().or_else(|| i.design.clone())
        } else {
            i.design.clone()
        };
        Ok::<_, String>((d, now, p))
    })?;
    let Some(design) = design else {
        return Ok(Json::obj()
            .set("ok", true)
            .set("empty", true)
            .set("units", Json::Arr(Vec::new()))
            .set("phase", 0)
            .set("period", 0)
            .set("transient", 0)
            .set("crated", 0));
    };
    let bill = phase::imported(&design, p);
    let c = crate::machine::orbit::compile(&design)?;
    let r = crate::machine::eval::report(&design, &c);
    let at = c.equivalent_tick(now / crate::mp::DESIGN_TICK);
    let m = c.state_at(&design, at)?;
    Ok(crate::machine::snap::render(&design, &m, &r)
        .set("ok", true)
        .set("phase", at as i64)
        .set("period", c.period as i64)
        .set("transient", c.transient as i64)
        .set("century", p.tag())
        .set("crated", Json::big(phase::design_cost(&design, p) as u128))
        .set(
            "imports",
            Json::Arr(
                bill.iter()
                    .map(|(unit, kind, cost)| {
                        Json::obj()
                            .set("unit", unit.clone())
                            .set("part", kind.tag())
                            .set("title", kind.title())
                            .set("gears", Json::big(*cost as u128))
                    })
                    .collect(),
            ),
        ))
}

/// A seed nobody chose. The clock is the only entropy this program has.
fn fresh_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    crate::mp::hash64(&t.as_nanos().to_le_bytes())
}
