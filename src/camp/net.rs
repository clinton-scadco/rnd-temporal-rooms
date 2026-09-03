//! `camp serve`: five rooms, two browsers, one clock.
//!
//! ```text
//!   GET  /                    the campaign
//!   GET  /api/sites           the map, the lanes and the fleets
//!   GET  /api/catalogue       what may be placed, and what is still locked
//!   GET  /api/parts           the components, and which twelve are not yours yet
//!   GET  /api/kit             experiment 08's meshes, for the 3D view
//!   GET  /api/camp            the campaign frame: rooms, tech, shelf, shipping
//!   GET  /api/state           one room's frame -- Prototype 2's contract, verbatim
//!   POST /api/enter           join, and get a player id
//!   POST /api/start           begin the clock. There is no matching stop
//!   POST /api/travel          stand somewhere else
//!   POST /api/cmd             one intention, in one room
//!   POST /api/route           open, retune or close a supply relationship
//!   POST /api/shelf           save, copy, rename, forget, or place from it
//!   POST /api/presence        a cursor. Lossy on purpose
//!   POST /api/form            a design, built as a plant, for the 3D view
//!   POST /api/inside          what every component in it is doing, and why
//! ```
//!
//! # It is the same client
//!
//! `/api/state`, `/api/cmd`, `/api/presence`, `/api/form` and `/api/inside`
//! answer exactly what Prototype 2's server answered, with `code` naming a
//! room of the campaign rather than a hosted game. That is why `web/room/`'s
//! world view, machine bench and panels are served here unchanged and
//! unforked, the same way experiment 10's renderer was served unchanged by
//! Prototype 2. The campaign front end is a *shell* around them: a map, a
//! library, a tech list and a shipping board.
//!
//! A prototype that had to rewrite the previous prototype's front end to reuse
//! its ideas would have been evidence that the ideas were not separable.
//!
//! # One campaign per process
//!
//! Prototype 2's server held a map of rooms because hosting one was the
//! gesture that started a game. A campaign is not that gesture: it is a world
//! that exists, that everybody joins, and that keeps running. So there is one,
//! it is made on the first request, and `POST /api/enter` puts you in it.

use super::run::Camp;
use super::{ship, site};
use crate::http::{self, Req};
use crate::json::Json;
use crate::machine::design::Design;
use crate::mp::world::PlayerId;
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::Duration;

const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../../web/camp/index.html")),
    ("/camp.css", "text/css; charset=utf-8", include_str!("../../web/camp/camp.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../../web/camp/app.js")),
    ("/map.js", "text/javascript; charset=utf-8", include_str!("../../web/camp/map.js")),
    ("/shell.js", "text/javascript; charset=utf-8", include_str!("../../web/camp/shell.js")),
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

static CAMP: Mutex<Option<Camp>> = Mutex::new(None);

fn with_camp<R>(f: impl FnOnce(&mut Camp) -> R) -> R {
    let mut g = CAMP.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| Camp::open(fresh_seed())))
}

/// Throw the world away and make another one. Only `POST /api/enter` with a
/// seed does this, and only before anybody has joined.
fn reopen(seed: u64) {
    let mut g = CAMP.lock().unwrap_or_else(|e| e.into_inner());
    *g = Some(Camp::open(seed));
}

// -------------------------------------------------------------------- serve

pub fn serve(host: &str, port: u16) -> std::io::Result<()> {
    let listener = bind(host, port)?;
    let addr = listener.local_addr()?;
    beat();
    println!("prototype 3 is at   http://{addr}/");
    if addr.ip().is_unspecified() {
        println!("bound to every interface: other machines on this network can join.");
    }
    println!("five rooms, one clock. everybody joins the same campaign.");
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

/// The campaign's own thread.
///
/// Prototype 2's server got one of these and this one did not, which is the
/// whole reason the freeze survived being fixed. See [`Camp::heartbeat`] for
/// what it is carrying and why a poll is the wrong thing to carry it.
///
/// It deliberately does *not* use `with_camp`, which would make a campaign on
/// its first tick out of a seed nobody chose. A server nobody has entered yet
/// has nothing to beat, and should still be handing the first `/api/enter`
/// with a seed on it the campaign that seed asks for.
fn beat() {
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(crate::mp::HEARTBEAT_MS));
        let mut g = CAMP.lock().unwrap_or_else(|e| e.into_inner());
        if let Some(c) = g.as_mut() {
            c.heartbeat();
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
            "/api/sites" => return ok(sites()),
            "/api/kit" => return ok(crate::machine::form::kit_json()),
            "/api/catalogue" => return ok(catalogue()),
            // See `mp::net::reference`: the designs the catalogue's numbers
            // came from, for the front end's own harness. Nothing in the game
            // places one.
            "/api/reference" => return ok(crate::mp::net::reference()),
            "/api/parts" => return ok(parts()),
            "/api/camp" => {
                let player = req.q("player").parse().unwrap_or(0);
                return answer(with_camp(|c| c.to_json(player)));
            }
            "/api/state" => {
                let (code, player) = (req.q("code").to_lowercase(), req.q("player"));
                return answer(with_camp(|c| c.look(player.parse().unwrap_or(0), &code)));
            }
            _ => {}
        }
    }
    if req.method == "POST" {
        match req.path.as_str() {
            "/api/enter" => return answer(enter(&req.json())),
            "/api/start" => {
                return answer(with_camp(|c| {
                    c.start();
                    Ok(Json::obj().set("ok", true).set("tick", c.now()))
                }))
            }
            "/api/travel" => return answer(travel(&req.json())),
            "/api/cmd" => return answer(command(&req.json())),
            "/api/route" => return answer(route_cmd(&req.json())),
            "/api/shelf" => return answer(shelf_cmd(&req.json())),
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

/// The map: five rooms, seven lanes and three fleets, none of which change.
fn sites() -> Json {
    Json::obj()
        .set("ok", true)
        .set("sites", Json::Arr(site::SITES.iter().map(site::Site::to_json).collect()))
        .set(
            "lanes",
            Json::Arr(
                ship::LANES
                    .iter()
                    .map(|l| {
                        Json::obj()
                            .set("from", l.from)
                            .set("to", l.to)
                            .set("item", l.item)
                            .set("leagues", l.leagues as i64)
                            .set("why", l.why)
                    })
                    .collect(),
            ),
        )
        .set("fleets", Json::Arr(ship::FLEETS.iter().map(ship::Fleet::to_json).collect()))
}

/// The palette, with a lock on everything whose components have not arrived.
///
/// A prototype is not hidden when it is locked. A player who can see that the
/// Stamping Line exists, and that the press is what is missing, has a reason
/// to want the press -- and a palette that concealed it would be a progression
/// system nobody could look forward to.
fn catalogue() -> Json {
    let base = crate::mp::kit::catalogue();
    with_camp(|c| {
        let protos: Vec<Json> = base
            .at("protos")
            .as_arr()
            .iter()
            .map(|p| {
                let tag = p.at("tag").as_str().unwrap_or_default().to_string();
                let missing = c.tech.missing_for(&tag);
                p.clone()
                    .set("locked", !missing.is_empty())
                    .set(
                        "needs",
                        Json::Arr(
                            missing
                                .iter()
                                .map(|m| {
                                    Json::obj().set("part", *m).set(
                                        "title",
                                        super::tech::unlock(m).map(|u| u.title).unwrap_or(m),
                                    )
                                })
                                .collect(),
                        ),
                    )
            })
            .collect();
        base.clone().set("protos", Json::Arr(protos))
    })
}

/// The components, with the twelve marked.
fn parts() -> Json {
    with_camp(|c| {
        let list: Vec<Json> = Design::catalogue()
            .as_arr()
            .iter()
            .map(|p| {
                let tag = p.at("kind").as_str().unwrap_or_default();
                p.clone().set("locked", !c.tech.has(tag)).set(
                    "opens",
                    super::tech::unlock(tag).map(|u| u.opens.to_string()),
                )
            })
            .collect();
        Json::obj()
            .set("ok", true)
            .set("parts", Json::Arr(list))
            .set("portKinds", crate::machine::design::port_kinds())
            .set("substances", crate::machine::design::substances())
            .set("tech", c.tech.to_json())
    })
}

/// Arrive, or come back.
///
/// The same route for both, because the client cannot tell which it is doing:
/// a browser that was refreshed knows only its own token. Whether that is a
/// seat or a stranger is the campaign's question, and the answer comes back as
/// `rejoined` so the screen can say so -- and so it can put the player back in
/// the room they were standing in rather than on the doorstep of the first one.
///
/// `back` is a client saying it is only interested in a seat it already has.
/// Without it a token left over from a campaign that has since been thrown
/// away would take a *new* seat in whatever campaign is running now, which is
/// a phantom player arriving from a tab nobody opened.
fn enter(j: &Json) -> Result<Json, String> {
    let name = j.at("name").as_str().unwrap_or("player").to_string();
    let key = j.at("key").as_str().unwrap_or_default().to_string();
    let back = j.at("back").as_bool().unwrap_or(false);
    if back {
        return with_camp(|c| {
            if !c.seated(&key) {
                return Err("your seat in this campaign is not there any more".into());
            }
            let (id, rejoined) = c.join_as("", &key)?;
            Ok(seat(c, id, rejoined))
        });
    }
    if let Some(seed) = j.at("seed").as_u64() {
        // A named seed is a request for a specific campaign, and it is only
        // honoured before anybody has joined the one that is running.
        let empty = with_camp(|c| c.cast.is_empty() && !c.started);
        if empty {
            reopen(seed);
        }
    }
    with_camp(|c| {
        let (id, rejoined) = c.join_as(&name, &key)?;
        Ok(seat(c, id, rejoined))
    })
}

/// Everything a browser needs to be in the campaign, however it got there.
///
/// `at` used to be `SITES[0]` unconditionally, which was fine while every
/// arrival was a first arrival. It is not fine for somebody coming back: a
/// refresh in Iron Valley should not put you down in Coal Basin.
fn seat(c: &mut Camp, id: PlayerId, rejoined: bool) -> Json {
    let here = c.who(id).map(|w| w.at).unwrap_or(0);
    Json::obj()
        .set("ok", true)
        .set("player", id as i64)
        .set("name", c.who(id).map(|w| Json::Str(w.name.clone())))
        .set("rejoined", rejoined)
        .set("code", c.code.clone())
        .set("seed", Json::big(c.seed as u128))
        .set("started", c.started)
        .set("at", site::SITES[here].tag)
}

fn travel(j: &Json) -> Result<Json, String> {
    let id: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let to = j.at("site").as_str().unwrap_or_default().to_lowercase();
    with_camp(|c| {
        c.travel(id, &to)?;
        Ok(Json::obj().set("ok", true).set("at", to.clone()))
    })
}

/// One intention, in one room. Refused answers are answers, not errors.
fn command(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let act = crate::mp::cmd::Cmd::from_json(
        &Json::obj()
            .set("type", j.at("type").clone())
            .set("payload", j.at("payload").clone()),
    )
    .map(|c| c.act)?;
    with_camp(|c| match c.submit(player, &code, act) {
        Ok(cmd) => Ok(Json::obj().set("ok", true).set("command", cmd.to_json())),
        Err(e) => Ok(Json::obj().set("ok", false).set("refused", true).set("error", e)),
    })
}

fn route_cmd(j: &Json) -> Result<Json, String> {
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let what = j.at("do").as_str().unwrap_or("open");
    with_camp(|c| match what {
        "open" => {
            let id = c.open_route(
                player,
                j.at("from").as_str().unwrap_or_default(),
                j.at("to").as_str().unwrap_or_default(),
                j.at("item").as_str().unwrap_or_default(),
                j.at("fleet").as_str().unwrap_or("train"),
                j.at("cap").as_u64(),
            )?;
            Ok(Json::obj().set("ok", true).set("route", id as i64))
        }
        "close" => {
            c.close_route(j.at("route").as_u64().unwrap_or(0) as u32)?;
            Ok(Json::obj().set("ok", true))
        }
        "cap" => {
            c.retune_route(
                j.at("route").as_u64().unwrap_or(0) as u32,
                j.at("cap").as_u64().unwrap_or(1),
            )?;
            Ok(Json::obj().set("ok", true))
        }
        other => Err(format!("`{other}` is not something to do to a route")),
    })
}

fn shelf_cmd(j: &Json) -> Result<Json, String> {
    let player: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    let what = j.at("do").as_str().unwrap_or("save");
    let name = j.at("name").as_str().unwrap_or_default().to_string();
    let id = j.at("design").as_u64().unwrap_or(0) as u32;
    with_camp(|c| match what {
        "save" => {
            let saved = c.keep(
                player,
                &j.at("code").as_str().unwrap_or_default().to_lowercase(),
                j.at("id").as_u64().unwrap_or(0),
                &name,
                j.at("draft").as_bool().unwrap_or(false),
            )?;
            Ok(Json::obj().set("ok", true).set("design", saved as i64))
        }
        "copy" => {
            let saved = c.copy(player, id, &name)?;
            Ok(Json::obj().set("ok", true).set("design", saved as i64))
        }
        "rename" => {
            c.shelf.rename(id, &name)?;
            Ok(Json::obj().set("ok", true))
        }
        "forget" => {
            let gone = c.shelf.forget(id)?;
            Ok(Json::obj().set("ok", true).set("name", gone))
        }
        "place" => {
            let cmd = c.place_saved(
                player,
                &j.at("code").as_str().unwrap_or_default().to_lowercase(),
                id,
                j.at("x").as_i128().unwrap_or(0) as i32,
                j.at("y").as_i128().unwrap_or(0) as i32,
                j.at("face").as_u64().unwrap_or(0) as u8,
            );
            match cmd {
                Ok(cmd) => Ok(Json::obj().set("ok", true).set("command", cmd.to_json())),
                Err(e) => Ok(Json::obj().set("ok", false).set("refused", true).set("error", e)),
            }
        }
        other => Err(format!("`{other}` is not something to do to a design")),
    })
}

/// A cursor, a selection, and which window somebody is looking at. Lossy, and
/// nothing downstream is allowed to remember it.
fn presence(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let id: PlayerId = j.at("player").as_u64().unwrap_or(0) as PlayerId;
    with_camp(|c| {
        let y = c.yard_mut(&code).ok_or(format!("there is no room called {code}"))?;
        let Some(p) = y.room.players.iter_mut().find(|p| p.id == id) else {
            return Err("you are not in this campaign".into());
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

/// A design, built as a plant, for the 3D window. The document comes from the
/// campaign rather than from the browser.
fn form(req: &Req) -> Result<Json, String> {
    let j = req.json();
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let id = j.at("id").as_u64().unwrap_or(0);
    let saved = j.at("design").as_u64().unwrap_or(0) as u32;
    let want_draft = j.at("draft").as_bool().unwrap_or(false);
    let grade = req.q("grade");
    let seed = req.q("seed").parse().unwrap_or(0);
    let design = with_camp(|c| -> Result<Design, String> {
        if saved > 0 {
            return c.shelf.get(saved).map(|s| s.design.clone()).ok_or("that design is not on the shelf".into());
        }
        let y = c.yard(&code).ok_or(format!("there is no room called {code}"))?;
        let i = y.room.host.world.get(id).ok_or("there is no such machine")?;
        let d = if want_draft { i.draft.clone().or_else(|| i.design.clone()) } else { i.design.clone() };
        // An empty chassis builds an empty scene rather than refusing to be
        // looked at. It used to error, which meant the one machine you most
        // needed to open -- the one with nothing in it -- was the one you
        // could not.
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

/// What every component inside one machine is doing, at the phase the
/// campaign's clock puts its orbit in.
fn inside(j: &Json) -> Result<Json, String> {
    let code = j.at("code").as_str().unwrap_or_default().to_lowercase();
    let id = j.at("id").as_u64().unwrap_or(0);
    let want_draft = j.at("draft").as_bool().unwrap_or(false);
    let (design, now) = with_camp(|c| {
        let now = c.now();
        let y = c.yard(&code).ok_or(format!("there is no room called {code}"))?;
        let i = y.room.host.world.get(id).ok_or("there is no such machine")?;
        let d = if want_draft {
            i.draft.clone().or_else(|| i.design.clone())
        } else {
            i.design.clone()
        };
        Ok::<_, String>((d, now))
    })?;
    // Nothing designed yet: an answer rather than an error, because the panel
    // beside an empty drawing board should say it is empty.
    let Some(design) = design else {
        return Ok(Json::obj()
            .set("ok", true)
            .set("empty", true)
            .set("units", Json::Arr(Vec::new()))
            .set("phase", 0)
            .set("period", 0)
            .set("transient", 0));
    };
    let c = crate::machine::orbit::compile(&design)?;
    let r = crate::machine::eval::report(&design, &c);
    let phase = c.equivalent_tick(now / crate::mp::DESIGN_TICK);
    let m = c.state_at(&design, phase)?;
    Ok(crate::machine::snap::render(&design, &m, &r)
        .set("ok", true)
        .set("phase", phase as i64)
        .set("period", c.period as i64)
        .set("transient", c.transient as i64))
}

/// A seed nobody chose. The clock is the only entropy this program has.
fn fresh_seed() -> u64 {
    use std::time::{SystemTime, UNIX_EPOCH};
    let t = SystemTime::now().duration_since(UNIX_EPOCH).unwrap_or_default();
    crate::mp::hash64(&t.as_nanos().to_le_bytes())
}
