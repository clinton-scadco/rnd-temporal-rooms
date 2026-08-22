//! The machine at tick *T*, in the shape a renderer needs.
//!
//! Same rule as the workbench, and it is the rule this experiment is least
//! willing to bend:
//!
//! ```text
//!   Simulation  ->  State(t)  ->  RenderSnapshot  ->  Renderer
//! ```
//!
//! Nothing downstream of here computes any physics. The canvas does not know
//! that a heat pipe leaks 2%; it knows that this wire carried 392 units last
//! tick and that its rate is 400, which is enough to draw it thick. Everything
//! the inspector says -- including the sentences that explain *why* a component
//! is stopped -- is composed here, out of state that was already there.
//!
//! The explanations are the point of the experiment as much as the simulation
//! is. A design tool where the player can see that the machine is bad but not
//! why is a puzzle with the solution torn out.

use super::design::Design;
use super::eval::Report;
use super::parts::{self, Dir, Kind};
use super::sim::{Machine, Status};
use crate::json::Json;

/// One decimal place of a per-mille figure, as a percentage.
fn pct(per_mille: u32) -> f64 {
    per_mille as f64 / 10.0
}

pub fn render(d: &Design, m: &Machine, r: &Report) -> Json {
    let units: Vec<Json> = (0..m.len()).map(|i| unit(d, m, i)).collect();
    let wires: Vec<Json> = m
        .links
        .iter()
        .enumerate()
        .map(|(i, l)| {
            let p = &parts::part(m.kinds[l.from]).ports[l.from_port];
            Json::obj()
                .set("from", m.names[l.from].clone())
                .set("fromPort", p.name)
                .set("to", m.names[l.to].clone())
                .set("toPort", parts::part(m.kinds[l.to]).ports[l.to_port].name)
                .set("type", p.kind.tag())
                .set("flow", m.flow[i] as i64)
                .set("rate", p.rate as i64)
                .set("gap", d.units[l.from].gap_to(&d.units[l.to]) as i64)
        })
        .collect();

    Json::obj()
        .set("tick", m.tick as i64)
        .set("units", Json::Arr(units))
        .set("wires", Json::Arr(wires))
        .set("now", now(m))
        .set("holding", holding(d, m))
        .set("report", r.to_json())
}

/// What the machine did during the tick just simulated, as opposed to what it
/// does on average. The difference between these two is the entire reason the
/// orbit is worth keeping.
fn now(m: &Machine) -> Json {
    let d = m.last;
    let n = m.len().max(1) as u64;
    Json::obj()
        .set("power", d.power as i64)
        .set("fuel", d.fuel as i64)
        .set("water", d.water as i64)
        .set("heatWasted", d.heat_wasted as i64)
        .set("steamVented", d.steam_vented as i64)
        .set("utilisation", Json::Real(d.util_sum as f64 / n as f64 / 10.0))
}

fn unit(d: &Design, m: &Machine, i: usize) -> Json {
    let kind = m.kinds[i];
    let part = parts::part(kind);
    let s = &m.st[i];
    let u = &d.units[i];

    let ports: Vec<Json> = part
        .ports
        .iter()
        .enumerate()
        .map(|(p, q)| {
            Json::obj()
                .set("name", q.name)
                .set("type", q.kind.tag())
                .set("dir", if q.dir == Dir::In { "in" } else { "out" })
                .set("rate", q.rate as i64)
                .set("cap", q.cap as i64)
                .set("level", s.buf[p] as i64)
                .set("got", s.got[p] as i64)
                .set("sent", s.sent[p] as i64)
                .set("used", s.used[p] as i64)
                .set("made", s.made[p] as i64)
                .set("wired", !(m.feeders(i, p).is_empty() && m.drains(i, p).is_empty()))
                .set("external", q.external)
        })
        .collect();

    let mut detail = Json::obj();
    match kind {
        Kind::Reactor => {
            detail = detail
                .set("throttle", u.tune.throttle as i64)
                .set("age", s.age as i64)
                .set("warmup", parts::WARMUP as i64);
        }
        Kind::Turbine => {
            detail = detail
                .set("spin", s.spin as i64)
                .set("spinMax", parts::SPIN_MAX as i64)
                .set("threshold", parts::TURBINE_MIN as i64);
        }
        Kind::Tank => {
            detail = detail
                .set("level", s.buf[0] as i64)
                .set("cap", part.ports[0].cap as i64)
                .set("pulse", u.tune.pulse)
                .set("high", u.tune.high as i64)
                .set("low", u.tune.low as i64)
                .set("draining", s.draining);
        }
        _ => {}
    }

    Json::obj()
        .set("name", m.names[i].clone())
        .set("kind", part.tag)
        .set("title", part.title)
        .set("blurb", part.blurb)
        .set("x", u.x as i64)
        .set("y", u.y as i64)
        .set("w", part.w as i64)
        .set("h", part.h as i64)
        .set("status", s.status.tag())
        .set("well", s.status.well())
        .set("util", Json::Real(pct(s.util)))
        .set("waste", s.waste as i64)
        .set("ports", Json::Arr(ports))
        .set("detail", detail)
        .set("why", Json::arr(why(d, m, i)))
}

/// Why this component is doing what it is doing, in the fewest sentences that
/// are all true.
pub fn why(d: &Design, m: &Machine, i: usize) -> Vec<String> {
    let kind = m.kinds[i];
    let part = parts::part(kind);
    let s = &m.st[i];
    let t = d.units[i].tune;
    let mut out = Vec::new();

    match kind {
        Kind::Reactor => {
            let thr = t.throttle.clamp(parts::MIN_THROTTLE, 100) as u64;
            let full = parts::REACTOR_HEAT * thr / 100;
            out.push(format!("throttle {}% — {full} heat/tick when hot", t.throttle));
            out.push(format!("fuel: {}/tick, warm or not", parts::REACTOR_FUEL * thr / 100));
            if s.age < parts::WARMUP {
                out.push(format!(
                    "warming: {}/{} — making {} of {full}",
                    s.age, parts::WARMUP, s.made[0]
                ));
            }
            if s.waste > 0 {
                out.push(format!("unused heat: {}/tick, vented", s.waste));
                let taken = s.made[0];
                let want = ((thr * taken + full.max(1) - 1) / full.max(1)).max(parts::MIN_THROTTLE as u64);
                if want < thr {
                    out.push(format!(
                        "nobody downstream can take it — throttle {want}% would waste nothing"
                    ));
                }
            }
        }
        Kind::HeatPipe | Kind::SteamPipe => {
            out.push(format!(
                "carrying {}/tick of {}",
                s.made[1], part.ports[1].rate
            ));
            if s.waste > 0 {
                out.push(format!("lost to the room: {}/tick", s.waste));
            }
            if s.status == Status::Blocked {
                out.push(format!(
                    "the far end is full — {} held back this tick",
                    s.buf[0]
                ));
            }
            if s.status == Status::Idle {
                out.push("nothing is arriving".into());
            }
        }
        Kind::Pump => {
            out.push(format!("drawing {}/tick of {}", s.made[0], part.ports[0].rate));
            if s.status == Status::Blocked {
                out.push("its tank is full — nothing is taking the water".into());
            }
        }
        Kind::Exchanger => {
            let hp = &part.ports[0];
            let wp = &part.ports[1];
            out.push(format!("needs: {} heat + {} water/tick", hp.rate, wp.rate));
            out.push(format!(
                "arriving: {} heat, {} water",
                s.got[0], s.got[1]
            ));
            match s.status {
                Status::Blocked => {
                    out.push("steam output buffer full".into());
                    out.push(format!("unused heat: {}/tick", s.got[0].saturating_sub(s.used[0])));
                }
                Status::Starved => {
                    let short_heat = s.used[0] < hp.rate;
                    let short_water = s.used[1] < wp.rate;
                    if short_heat && short_water {
                        out.push(format!(
                            "short of both: {} of {} heat, {} of {} water",
                            s.used[0], hp.rate, s.used[1], wp.rate
                        ));
                    } else if short_heat {
                        out.push(format!("short of heat: {} of {}", s.used[0], hp.rate));
                    } else if short_water {
                        out.push(format!("short of water: {} of {}", s.used[1], wp.rate));
                    }
                }
                _ => {}
            }
            out.push(format!("making {} steam/tick", s.made[2]));
        }
        Kind::Tank => {
            let cap = part.ports[0].cap;
            out.push(format!("holding {} of {cap}", s.buf[0]));
            if t.pulse {
                out.push(format!(
                    "pulse: fill to {}, empty to {} — currently {}",
                    t.high,
                    t.low,
                    if s.draining { "emptying" } else { "filling" }
                ));
            } else {
                out.push("passing steam straight through".into());
            }
            out.push(format!("releasing {}/tick", s.made[1]));
        }
        Kind::Turbine => {
            out.push(format!("needs: {} steam/tick", part.ports[0].rate));
            out.push(format!("available: {} steam/tick", s.got[0]));
            out.push(format!("spin: {}/{}", s.spin, parts::SPIN_MAX));
            if s.status == Status::Stalled {
                out.push(format!(
                    "below the {} steam/tick it needs to turn over at all",
                    parts::TURBINE_MIN
                ));
                out.push("a Steam Buffer in pulse mode can push a trickle over the line".into());
            }
            if s.waste > 0 {
                out.push(format!("steam condensed and lost: {}/tick", s.waste));
            }
            if s.status == Status::Blocked {
                out.push("the shaft has nowhere to go — its generator is full".into());
            }
            out.push(format!("rotary out: {}/tick", s.made[1]));
        }
        Kind::Generator => {
            out.push(format!("needs: {} rotary/tick", part.ports[0].rate));
            out.push(format!("available: {} rotary/tick", s.got[0]));
            out.push(format!("output: {} MW", s.made[1]));
        }
    }
    out.push(format!("utilisation: {:.1}%", pct(s.util)));
    out
}

/// What is holding the machine back, worst first.
///
/// Rank by what the player can actually do something about: a stalled turbine
/// is producing nothing and is one tank away from producing something; a
/// venting reactor is burning fuel for nothing; a starved component is a
/// symptom of one of those two somewhere upstream.
fn holding(d: &Design, m: &Machine) -> Json {
    let mut rows: Vec<(u8, Json)> = Vec::new();
    for i in 0..m.len() {
        let s = &m.st[i];
        let rank = match s.status {
            Status::Stalled => 0,
            Status::Venting => 1,
            Status::Blocked => 2,
            Status::Starved => 3,
            Status::Idle if m.kinds[i] != Kind::Tank => 4,
            _ => continue,
        };
        let line = why(d, m, i)
            .into_iter()
            .nth(if s.status == Status::Stalled { 3 } else { 1 })
            .unwrap_or_default();
        rows.push((
            rank,
            Json::obj()
                .set("name", m.names[i].clone())
                .set("kind", parts::part(m.kinds[i]).tag)
                .set("title", parts::part(m.kinds[i]).title)
                .set("status", s.status.tag())
                .set("util", Json::Real(pct(s.util)))
                .set("why", line),
        ));
    }
    rows.sort_by_key(|(r, _)| *r);
    Json::Arr(rows.into_iter().map(|(_, j)| j).collect())
}
