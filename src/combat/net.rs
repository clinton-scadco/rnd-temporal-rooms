//! `combat serve`: one encounter, in a browser.
//!
//! ```text
//!   GET  /                the encounter
//!   GET  /api/layout      the district: land, roads, sites, lanes, anchors,
//!                         batteries, sectors. Fixed
//!   GET  /api/frame       everything a view draws at the current tick
//!   GET  /api/log         what is written down: the seed, the commands, checkpoints
//!   POST /api/stream      { lane, rate }   tonnes a second through a fracture
//!   POST /api/anchor      { a, on }        power a phase anchor, or not
//!   POST /api/launder     { on }           crush 1890 ore before it is stacked
//!   POST /api/repair      { s }            put a structure back
//!   POST /api/hold        { b, on }        hold a battery's fire, or release it
//!   POST /api/speed       { x, paused }    how fast the clock runs. Not logged:
//!                                          the clock's rate is not the fight
//!   POST /api/reset       { seed }         a fresh encounter
//!   POST /api/verify      replay the log from nothing and resume it from the
//!                         oldest checkpoint held, and compare both with now
//! ```
//!
//! A frame carries state, not pictures: a site's strain and the rate it is
//! changing at, a cohort's road and the tick it set off, a volley's four
//! numbers, a battery's two headings and the ticks between them. The page polls a few times a second and interpolates every
//! one of those at sixty frames a second on its own -- which is the
//! experiment's rendering claim, made by the renderer rather than about it.

use super::field;
use super::run::{self, Cmd, Encounter};
use super::{commas, TICK_RATE};
use crate::http::{self, Req};
use crate::json::Json;
use std::net::{TcpListener, TcpStream};
use std::sync::Mutex;
use std::time::{Duration, Instant};

const ASSETS: &[(&str, &str, &str)] = &[
    ("/", "text/html; charset=utf-8", include_str!("../../web/combat/index.html")),
    ("/combat.css", "text/css; charset=utf-8", include_str!("../../web/combat/combat.css")),
    ("/app.js", "text/javascript; charset=utf-8", include_str!("../../web/combat/app.js")),
    ("/draw.js", "text/javascript; charset=utf-8", include_str!("../../web/combat/draw.js")),
];

struct Live {
    enc: Encounter,
    speed: u64,
    paused: bool,
    last: Instant,
    /// Fractional ticks owed by the wall clock.
    owed: f64,
    /// (when, events so far): one second of history, for events per second.
    window: Vec<(Instant, u64)>,
}

impl Live {
    fn new(seed: u64) -> Live {
        Live {
            enc: Encounter::new(seed),
            speed: 1,
            paused: false,
            last: Instant::now(),
            owed: 0.0,
            window: Vec::new(),
        }
    }

    fn beat(&mut self) {
        let now = Instant::now();
        let dt = now.duration_since(self.last).as_secs_f64();
        self.last = now;
        if !self.paused {
            self.owed += dt * TICK_RATE as f64 * self.speed as f64;
            // A stalled process does not get to fast-forward through a fight
            // it was not there for.
            self.owed = self.owed.min(TICK_RATE as f64 * self.speed as f64);
            let whole = self.owed.floor();
            self.owed -= whole;
            let t = self.enc.now + whole as u64;
            self.enc.advance_to(t);
        }
        self.window.push((now, self.enc.fight.stats.events));
        let cut = now - Duration::from_secs(1);
        self.window.retain(|w| w.0 >= cut);
    }

    fn events_per_sec(&self) -> f64 {
        match (self.window.first(), self.window.last()) {
            (Some(a), Some(b)) if b.0 > a.0 => {
                (b.1 - a.1) as f64 / b.0.duration_since(a.0).as_secs_f64()
            }
            _ => 0.0,
        }
    }
}

static LIVE: Mutex<Option<Live>> = Mutex::new(None);

fn with<R>(f: impl FnOnce(&mut Live) -> R) -> R {
    let mut g = LIVE.lock().unwrap_or_else(|e| e.into_inner());
    f(g.get_or_insert_with(|| Live::new(16)))
}

pub fn serve(host: &str, port: u16) -> std::io::Result<()> {
    let listener = bind(host, port)?;
    let addr = listener.local_addr()?;
    std::thread::spawn(|| loop {
        std::thread::sleep(Duration::from_millis(16));
        with(|l| l.beat());
    });
    println!("experiment 16 is at   http://{addr}/");
    println!("one district in 2037, two fractures into it, and whatever comes through.");
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
            "/api/layout" => return ok(field::to_json()),
            "/api/frame" => return ok(with(frame)),
            "/api/log" => return ok(with(|l| log(&l.enc))),
            _ => {}
        }
    }
    if req.method == "POST" {
        let j = req.json();
        let r = match req.path.as_str() {
            "/api/stream" => command(|| {
                let rate = j
                    .at("rate")
                    .as_u64()
                    .or_else(|| j.at("rate").as_str().and_then(|s| s.replace(',', "").parse().ok()))
                    .ok_or("how many tonnes a second?")?;
                Ok(Cmd::Stream { lane: run::lane(j.at("lane").as_str().unwrap_or(""))?, rate })
            }),
            "/api/anchor" => command(|| {
                Ok(Cmd::Anchor {
                    anchor: run::anchor(j.at("a").as_str().unwrap_or(""))?,
                    on: j.at("on").as_bool().unwrap_or(true),
                })
            }),
            "/api/launder" => command(|| Ok(Cmd::Launder { on: j.at("on").as_bool().unwrap_or(true) })),
            "/api/repair" => command(|| {
                Ok(Cmd::Repair { structure: run::structure(j.at("s").as_str().unwrap_or(""))? })
            }),
            "/api/hold" => command(|| {
                Ok(Cmd::Hold {
                    battery: run::battery(j.at("b").as_str().unwrap_or(""))?,
                    hold: j.at("on").as_bool().unwrap_or(true),
                })
            }),
            "/api/speed" => Ok(with(|l| {
                if let Some(x) = j.at("x").as_u64() {
                    l.speed = x.clamp(1, 16);
                }
                if let Some(p) = j.at("paused").as_bool() {
                    l.paused = p;
                }
                Json::obj().set("ok", true).set("speed", l.speed).set("paused", l.paused)
            })),
            "/api/reset" => Ok(with(|l| {
                let seed = j.at("seed").as_u64().unwrap_or(l.enc.seed);
                *l = Live::new(seed);
                Json::obj().set("ok", true).set("seed", seed)
            })),
            "/api/verify" => Ok(with(|l| verify(&l.enc))),
            _ => return ("404 Not Found", MIME, err("no such route").to_string()),
        };
        return match r {
            Ok(j) => ok(j),
            Err(e) => ok(err(&e)),
        };
    }
    ("404 Not Found", MIME, err("no such route").to_string())
}

fn command(make: impl FnOnce() -> Result<Cmd, String>) -> Result<Json, String> {
    let cmd = make()?;
    with(|l| {
        l.enc.apply(cmd)?;
        Ok(Json::obj().set("ok", true).set("tick", l.enc.now).set("commands", l.enc.log.len()))
    })
}

fn ok(j: Json) -> (&'static str, &'static str, String) {
    ("200 OK", MIME, j.to_string())
}

fn err(msg: &str) -> Json {
    Json::obj().set("ok", false).set("error", msg)
}

fn frame(l: &mut Live) -> Json {
    let eps = l.events_per_sec();
    let over = l.enc.overlay().set("eventsPerSec", eps);
    l.enc
        .frame()
        .set("overlay", over)
        .set("speed", l.speed)
        .set("paused", l.paused)
        .set("seed", Json::big(l.enc.seed as u128))
}

fn log(e: &Encounter) -> Json {
    let latest = e.checkpoints.last();
    Json::obj()
        .set("ok", true)
        .set("log", e.log_json())
        .set("bytes", e.log_json().to_string().len())
        .set("derived", e.fight.stats.events)
        .set(
            "checkpoints",
            Json::Arr(
                e.checkpoints
                    .iter()
                    .map(|c| {
                        Json::obj()
                            .set("tick", c.tick)
                            .set("bytes", c.json.len())
                            .set("hash", format!("{:016x}", c.hash))
                    })
                    .collect(),
            ),
        )
        .set("latest", latest.map(|c| c.json.clone()))
}

/// Three reconstructions of now, compared.
fn verify(e: &Encounter) -> Json {
    let t0 = Instant::now();
    let live = e.hash();
    let replay = Encounter::replay(e.seed, &e.log, e.now).hash();
    let from = e.checkpoints.first();
    let resumed = from.map(|cp| Encounter::resume(e.seed, cp, &e.log, e.now).map(|r| r.hash()));
    let ms = t0.elapsed().as_secs_f64() * 1000.0;
    let resumed_ok = match &resumed {
        None => true,
        Some(Ok(h)) => *h == live,
        Some(Err(_)) => false,
    };
    Json::obj()
        .set("ok", true)
        .set("tick", e.now)
        .set("live", format!("{live:016x}"))
        .set("replay", format!("{replay:016x}"))
        .set(
            "resumed",
            match &resumed {
                None => Json::Null,
                Some(Ok(h)) => Json::from(format!("{h:016x}")),
                Some(Err(m)) => Json::from(format!("error: {m}")),
            },
        )
        .set("from", from.map(|c| c.tick))
        .set("match", replay == live && resumed_ok)
        .set("commands", e.log.len())
        .set("derived", commas(e.fight.stats.events as u128))
        .set("ms", ms)
}
