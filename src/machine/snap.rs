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
//! that a heat pipe leaks 2%, that a crusher will not take a drive turning at
//! speed 6, or that iron melts at band 7; it knows that this wire carried 392
//! units of *iron ore, crushed, 40% pure* last tick and that its rate is 400,
//! which is enough to draw it thick and label it.
//!
//! The explanations are the point of the experiment as much as the simulation
//! is, and experiment 07 leans on them harder than 06 did. With forty-four
//! components and five properties there is now a whole class of design that is
//! wired correctly, is not short of anything, and still produces nothing --
//! because the ore arriving at the mill is lumps, or the metal arriving at the
//! rolling mill has gone cold. A tool that could not say which would be a
//! puzzle with the solution torn out.

use super::design::Design;
use super::era;
use super::eval::Report;
use super::parts::{self, Dir, Kind, Need};
use super::sim::{need_of, Machine, Status, Stop};
use super::stuff::{Subst, TEMP_NAMES};
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
                .set("type", p.dom.tag())
                .set("flow", m.flow[i] as i64)
                .set("rate", l.carries(p.rate) as i64)
                // What the run itself is: how long it is, what it takes on the
                // way, and whether it is slack. This is the whole of the
                // transport family as the panel now sees it -- three numbers
                // on a connection rather than a component to click on.
                .set("gap", l.gap as i64)
                .set("reach", parts::span_of(p.dom, d.units[l.from].falls_to(&d.units[l.to]), l.belt).reach as i64)
                .set("lossPct", l.loss_pct as i64)
                .set("lost", m.lost[i] as i64)
                .set("belt", l.belt)
                .set("carrying", if m.flow[i] > 0 { m.carried[i].label() } else { String::new() })
                .set("stuff", m.carried[i].to_json())
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
    let d = &m.last;
    let n = m.len().max(1) as u64;
    let flow = |f: &super::sim::Flow| {
        Json::Arr(
            f.iter()
                .map(|(s, q)| Json::obj().set("what", s.label()).set("qty", *q as i64))
                .collect(),
        )
    };
    Json::obj()
        .set("power", d.power as i64)
        .set("heatWasted", d.heat_wasted as i64)
        .set("took", flow(&d.took))
        .set("gave", flow(&d.gave))
        .set("lost", flow(&d.lost))
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
            let b = &s.buf[p];
            Json::obj()
                .set("name", q.name)
                .set("type", q.dom.tag())
                .set("dir", if q.dir == Dir::In { "in" } else { "out" })
                .set("rate", q.rate as i64)
                .set("cap", q.cap as i64)
                .set("level", b.qty as i64)
                .set("holding", if b.qty > 0 { b.stuff.label() } else { String::new() })
                .set("stuff", b.stuff.to_json())
                .set("got", s.got[p] as i64)
                .set("sent", s.sent[p] as i64)
                .set("used", s.used[p] as i64)
                .set("made", s.made[p] as i64)
                .set("shipped", s.shipped[p] as i64)
                .set("wired", !(m.feeders(i, p).is_empty() && m.drains(i, p).is_empty()))
                .set("external", q.external)
        })
        .collect();

    let mut detail = Json::obj().set("family", part.family.tag());
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
        Kind::Tank | Kind::Drum | Kind::Flywheel | Kind::Hopper => {
            detail = detail
                .set("level", s.buf[0].qty as i64)
                .set("cap", part.ports[0].cap as i64)
                .set("pulse", u.tune.pulse)
                .set("high", u.tune.high as i64)
                .set("low", u.tune.low as i64)
                .set("draining", s.draining);
        }
        Kind::Pump | Kind::Inlet => {
            detail = detail.set("draws", u.tune.subst.tag()).set("drawsTitle", u.tune.subst.title());
        }
        Kind::Gearbox => {
            detail = detail
                .set("ratio", u.tune.ratio as i64)
                .set("inSpeed", s.buf[0].stuff.q.speed as i64)
                .set("outSpeed", super::sim::geared(s.buf[0].stuff.q.speed, u.tune.ratio) as i64);
        }
        Kind::Valve | Kind::Clutch => {
            detail = detail.set("limit", u.tune.limit as i64).set("engaged", s.draining);
        }
        Kind::Column => {
            let (l, mid, h, heat) = parts::column_split(u.tune.stages);
            detail = detail
                .set("stages", u.tune.stages as i64)
                .set("light", l as i64)
                .set("middle", mid as i64)
                .set("heavy", h as i64)
                .set("heatPerBatch", heat as i64);
        }
        _ => {}
    }

    Json::obj()
        .set("name", m.names[i].clone())
        .set("kind", part.tag)
        .set("title", part.title)
        .set("blurb", part.blurb)
        .set("family", part.family.tag())
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
        .set("body", body(m, i))
        .set("why", Json::arr(why(d, m, i)))
}

/// Experiment 14: the component as a physical object rather than as a
/// transformation -- what it is made of, how hot it is, and how hard the drive
/// it is bolted to is shaking it.
///
/// Every component has one of these, including the ones with no body
/// temperature at all, because "this has no thermal behaviour" is a thing a
/// panel should be able to say plainly rather than by leaving a gap.
fn body(m: &Machine, i: usize) -> Json {
    let ph = parts::phys(m.kinds[i]);
    let mat = m.mats[i];
    let temp = m.temp(i);
    Json::obj()
        .set("era", ph.era.tag())
        .set("material", mat.tag())
        .set("materialTitle", mat.title())
        .set("clear", m.clear[i] as i64)
        .set("thermal", ph.thermal())
        .set("temp", temp as i64)
        .set("band", m.band(i).tag())
        .set("duty", m.duty(i) as i64)
        .set("tripped", m.st[i].tripped)
        .set("lo", ph.lo as i64)
        .set("hi", ph.hi as i64)
        .set("ceiling", ph.ceiling(mat) as i64)
        .set("grade", era::grade(temp) as i64)
        .set("shed", m.st[i].body as i64)
        .set("shakes", ph.vib as i64)
        .set("shaken", m.shake[i] as i64)
        .set("tolerates", mat.tol() as i64)
        .set(
            "shakenBy",
            match m.shaker(i) {
                Some(j) if j != i => Json::Str(m.names[j].clone()),
                _ => Json::Null,
            },
        )
}

// ------------------------------------------------------------ the sentences

fn held(m: &Machine, i: usize, port: usize) -> String {
    let b = &m.st[i].buf[port];
    if b.qty == 0 {
        "nothing".to_string()
    } else {
        b.stuff.label()
    }
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
            out.push(format!(
                "grade: {} — hot enough for a furnace",
                TEMP_NAMES[parts::REACTOR_TEMP as usize]
            ));
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
                let want = ((thr * taken + full.max(1) - 1) / full.max(1))
                    .max(parts::MIN_THROTTLE as u64);
                if want < thr {
                    out.push(format!(
                        "nobody downstream can take it — throttle {want}% would waste nothing"
                    ));
                }
            }
        }
        Kind::Pump | Kind::Inlet => {
            out.push(format!("drawing {} from outside", t.subst.title()));
            out.push(format!("{}/tick of {}", s.made[0], part.ports[0].rate));
            if t.subst.hardness() > 0 {
                out.push(format!("hardness {} — a crusher rates 8", t.subst.hardness()));
            }
            if s.status == Status::Blocked {
                out.push("its own buffer is full — nothing is taking it".into());
            }
        }
        Kind::Mains => {
            out.push(format!("up to {} MW/tick from the grid", part.ports[0].rate));
            out.push(format!("drawing {} MW this tick", s.made[0]));
            out.push("every unit is a cost on the scoreboard".into());
        }
        Kind::Hopper | Kind::Tank | Kind::Drum | Kind::Flywheel => {
            let cap = part.ports[0].cap;
            out.push(format!("holding {} of {cap}", s.buf[0].qty));
            if s.buf[0].qty > 0 {
                out.push(format!("which is {}", s.buf[0].stuff.label()));
            }
            if t.pulse {
                out.push(format!(
                    "pulse: fill to {}, empty to {} — currently {}",
                    t.high,
                    t.low,
                    if s.draining { "emptying" } else { "filling" }
                ));
            } else {
                out.push("passing straight through".into());
            }
            out.push(format!("releasing {}/tick", s.made[1]));
        }
        Kind::Outlet => {
            out.push(format!("shipping {}/tick out of the machine", s.waste));
            for (p, q) in part.ports.iter().enumerate() {
                if s.used[p] > 0 {
                    out.push(format!("{}: {} of {}", q.name, s.used[p], held(m, i, p)));
                }
            }
            out.push("everything that arrives here is counted as product".into());
        }
        Kind::Skip | Kind::Radiator => {
            out.push(format!("taking away {}/tick", s.waste));
            out.push("everything that arrives here is counted as waste".into());
        }
        Kind::Valve => {
            out.push(format!("set to {} of {}/tick", t.limit, part.ports[0].rate));
            out.push(format!("passing {}/tick", s.made[1]));
        }
        Kind::Clutch => {
            out.push(format!("engages at {} rotary", t.limit));
            out.push(if s.draining {
                format!("engaged — passing {}/tick", s.made[1])
            } else {
                format!("waiting: {} of {} gathered", s.buf[0].qty, t.limit)
            });
            out.push("a stuttering drive can still turn something heavy this way".into());
        }
        Kind::Gearbox => {
            let inn = s.buf[0].stuff.q.speed;
            let outn = super::sim::geared(inn, t.ratio);
            out.push(format!(
                "ratio {} — {}",
                t.ratio,
                if t.ratio >= 2 {
                    "slower and heavier"
                } else if t.ratio <= -2 {
                    "faster and lighter"
                } else {
                    "straight through"
                }
            ));
            out.push(format!("speed {inn} in, speed {outn} out"));
            out.push(format!("passing {}/tick, losing {}", s.made[1], s.waste));
        }
        Kind::Turbine => {
            out.push(format!("needs: {} gas/tick", part.ports[0].rate));
            out.push(format!("available: {} this tick", s.got[0]));
            out.push(format!("spin: {}/{}", s.spin, parts::SPIN_MAX));
            if s.status == Status::Stalled {
                out.push(format!(
                    "below the {} gas/tick it needs to turn over at all",
                    parts::TURBINE_MIN
                ));
                out.push("a Gas Buffer in pulse mode can push a trickle over the line".into());
            }
            if s.waste > 0 {
                out.push(format!("gas condensed and lost: {}/tick", s.waste));
            }
            if s.status == Status::Blocked {
                out.push("the drive has nowhere to go — its generator is full".into());
            }
            out.push(format!("rotary out: {}/tick at speed {}", s.made[1], parts::DRIVE_SPEED));
        }
        Kind::Generator => {
            out.push(format!("needs: {} rotary/tick", part.ports[0].rate));
            out.push(format!("available: {} rotary/tick", s.got[0]));
            out.push(format!(
                "and a drive turning at speed {} or more",
                parts::GENERATOR_MIN_SPEED
            ));
            if s.status == Status::Refused {
                out.push(format!(
                    "REFUSED — the drive turns at speed {}, and this wants {}",
                    s.buf[0].stuff.q.speed,
                    parts::GENERATOR_MIN_SPEED
                ));
                out.push("a gearbox with a negative ratio gears up".into());
            }
            out.push(format!("output: {} MW", s.made[1]));
            out.push(format!("shipped out of the machine: {} MW", s.shipped[1]));
        }
        Kind::Furnace => {
            let feed = s.buf[1].stuff;
            out.push(format!("feed: {}", held(m, i, 1)));
            out.push(format!(
                "heat: {} — a furnace wants {} or better",
                held(m, i, 0),
                TEMP_NAMES[5]
            ));
            out.push(format!(
                "lifts what it heats {} bands, to {}",
                parts::FURNACE_LIFT,
                TEMP_NAMES[((feed.q.temp + parts::FURNACE_LIFT).min(9)) as usize]
            ));
            if feed.subst.melt() > 0 {
                out.push(format!(
                    "{} melts at {} — past that it leaves as a fluid",
                    feed.name(),
                    TEMP_NAMES[feed.subst.melt() as usize]
                ));
            }
            if s.made[3] > 0 {
                out.push(format!("molten out: {}/tick", s.made[3]));
            } else {
                out.push(format!("hot solid out: {}/tick", s.made[2]));
            }
            if s.status == Status::Refused {
                out.push("the heat arriving is too low a grade to do anything".into());
            }
        }
        Kind::Column => {
            let (l, mid, h, heat) = parts::column_split(t.stages);
            out.push(format!("{} stages: {l} light, {mid} middle, {h} heavy per 10 fed", t.stages));
            out.push(format!("and {heat} heat per batch of 10"));
            out.push(format!("feed: {}", held(m, i, 0)));
            if s.buf[0].qty > 0 && s.buf[0].stuff.q.temp < parts::COLUMN_FEED_TEMP {
                out.push(format!(
                    "too cold to separate — wants {} or hotter, put a preheater in front",
                    TEMP_NAMES[parts::COLUMN_FEED_TEMP as usize]
                ));
            }
            if s.buf[0].qty > 0 && s.buf[0].stuff.subst != Subst::Crude {
                out.push(format!("this column takes Crude, and that is {}", s.buf[0].stuff.name()));
            }
            out.push(format!(
                "making {} light, {} middle, {} heavy per tick",
                s.made[2], s.made[3], s.made[4]
            ));
            if s.made[2] > 0 {
                out.push("the light fraction leaves as vapour — it needs a condenser".into());
            }
        }
        _ => out.extend(recipe_why(m, i)),
    }
    out.extend(chain_why(m, i));
    out.extend(body_why(m, i));
    out.push(format!("utilisation: {:.1}%", pct(s.util)));
    out
}

/// Who else this component is waiting on, and who is actually at fault.
///
/// The complaint this answers is a real one and it is about legibility rather
/// than physics: a plant of twenty components produces fifteen lines of STARVED
/// and BLOCKED, every one of them true, and not one of them says which
/// component to go and look at. STARVED means somebody upstream is not
/// supplying and BLOCKED means somebody downstream is not taking, so both of
/// them are *directions*, and following them is something the panel can do
/// rather than something the player should have to.
///
/// Three sentences at most, and usually none: a component that is running is
/// not waiting on anybody.
fn chain_why(m: &Machine, i: usize) -> Vec<String> {
    let part = parts::part(m.kinds[i]);
    let s = &m.st[i];
    let mut out = Vec::new();

    // The one that is never obvious and is always somebody's first mistake:
    // a component that cannot run because nothing was ever wired to it.
    for p in m.needed_inputs(i) {
        if m.feeders(i, p).is_empty() {
            out.push(format!(
                "nothing is wired to {} — it needs {} and will never run without it",
                part.ports[p].name, part.ports[p].dom
            ));
        }
    }
    if !out.is_empty() {
        return out;
    }

    // Name the neighbour, and what it actually delivered or took. "short of
    // drive" is true; "SH1 delivered 46 of the 50 this wants" is actionable.
    let flows = |wires: &[usize]| -> (u64, Vec<String>) {
        let total: u64 = wires.iter().map(|&w| m.flow[w]).sum();
        let who: Vec<String> = wires
            .iter()
            .map(|&w| {
                let l = m.links[w];
                m.names[if l.to == i { l.from } else { l.to }].clone()
            })
            .collect();
        (total, who)
    };
    let say = |who: &[String]| -> String {
        match who.len() {
            0 => String::new(),
            1 => who[0].clone(),
            2 => format!("{} and {}", who[0], who[1]),
            n => format!("{} and {} others", who[0], n - 1),
        }
    };

    match s.status {
        Status::Starved => {
            if let Stop::Short(p) = s.stop {
                let (got, who) = flows(m.feeders(i, p));
                if !who.is_empty() {
                    out.push(format!(
                        "{} arrived from {} last tick — that is what is holding this back",
                        got,
                        say(&who)
                    ));
                }
            }
        }
        Status::Blocked => {
            // Whichever output is actually full, rather than all of them.
            let p = match s.stop {
                Stop::Full(p) => Some(p),
                _ => (0..part.ports.len()).find(|&p| {
                    part.ports[p].dir == Dir::Out
                        && !m.drains(i, p).is_empty()
                        && s.buf[p].qty >= part.ports[p].cap
                }),
            };
            if let Some(p) = p {
                let (took, who) = flows(m.drains(i, p));
                if who.is_empty() {
                    out.push(format!(
                        "{} is full and nothing is wired to it — it has nowhere to go",
                        part.ports[p].name
                    ));
                } else {
                    out.push(format!(
                        "{} took {} of it last tick, and {} is full",
                        say(&who),
                        took,
                        part.ports[p].name
                    ));
                }
            }
        }
        _ => {}
    }

    // And then the end of the trail, which is the sentence worth reading.
    if let Some(r) = m.root_cause(i) {
        let title = parts::part(m.kinds[r]).title;
        let st = m.st[r].status;
        out.push(if st.well() {
            format!(
                "follow the wire back and it ends at {} ({title}), running flat out — \
                 there is simply less of it than this wants",
                m.names[r]
            )
        } else {
            format!(
                "follow the wire back and it ends at {} ({title}), which is {} — fix that first",
                m.names[r],
                st.tag()
            )
        });
    }
    out
}

/// What experiment 14 has to say about this component, if anything.
///
/// Deliberately silent for the great majority of the catalogue. An outlet has
/// no body temperature and no vibration and is bolted to nothing, and printing
/// "temperature: 0, NORMAL" underneath it would be three experiments' worth of
/// panel noise in exchange for no information at all.
fn body_why(m: &Machine, i: usize) -> Vec<String> {
    let ph = parts::phys(m.kinds[i]);
    let mat = m.mats[i];
    let mut out = Vec::new();

    if m.shake[i] > 0 {
        let by = match m.shaker(i) {
            Some(j) if j != i => format!("{} on the same drive", m.names[j]),
            _ => "itself".to_string(),
        };
        if m.shake[i] > mat.tol() {
            out.push(format!(
                "SHAKING — {} shakes at {} and {} carries {}",
                by,
                m.shake[i],
                mat.title().to_lowercase(),
                mat.tol()
            ));
            out.push(
                "put it on a stiffer frame, or make the drive wire a belt -- \
                 a belt passes torque and does not pass shake"
                    .to_string(),
            );
        } else if m.shake[i] >= mat.tol() {
            out.push(format!(
                "shaken at {} by {}, and {} carries exactly {}",
                m.shake[i],
                by,
                mat.title().to_lowercase(),
                mat.tol()
            ));
        }
    }

    if !ph.thermal() {
        return out;
    }

    let temp = m.temp(i);
    let band = m.band(i);
    out.push(format!(
        "body: {temp} degrees, {band} -- range {}..{}, trips at {}",
        ph.lo,
        ph.hi,
        ph.ceiling(mat)
    ));
    if m.st[i].tripped {
        out.push(format!(
            "OVERHEATED — stopped until it is back under {}, and it is {temp}",
            ph.hi
        ));
    } else if m.duty(i) < 1000 {
        out.push(format!("{} -- {}% of rating", band.note(), m.duty(i) / 10));
    }

    let cooling = m.clear[i];
    out.push(format!(
        "{} with {} clear, shedding {}/tick",
        mat.title().to_lowercase(),
        match cooling {
            0 => "nothing".to_string(),
            1 => "one tile".to_string(),
            n => format!("{n} tiles"),
        },
        m.st[i].body
    ));

    let wired = m
        .kinds[i]
        .waste_port()
        .map(|p| !m.drains(i, p).is_empty())
        .unwrap_or(false);
    if m.kinds[i].waste_port().is_some() && !wired {
        out.push(format!(
            "the waste port is not wired -- on air alone this settles at {}",
            ph.settles_at(mat, cooling)
        ));
    } else if wired {
        out.push(format!(
            "waste heat leaving at grade {}, and it settles at {}",
            era::grade(temp),
            ph.cooled_at(mat, cooling)
        ));
    }
    if band == super::era::Band::Warm || band == super::era::Band::Hot {
        out.push(format!(
            "a clear tile either side would take it to {}",
            ph.cooled_at(mat, (cooling + 1).min(era::CLEAR_MAX))
        ));
    }
    out
}

/// The explanation for the fifteen components that are a row in the part table.
///
/// One function, because they are one function: what it wants, what arrived,
/// which of the two stopped it, and what came out.
fn recipe_why(m: &Machine, i: usize) -> Vec<String> {
    let kind = m.kinds[i];
    let part = parts::part(kind);
    let s = &m.st[i];
    let mut out = Vec::new();
    let Some(r) = part.recipe else {
        return out;
    };

    for dr in r.draws {
        let port = &part.ports[dr.port];
        let mut line = format!("needs {} {}/tick", dr.qty * r.rate, port.name);
        if !dr.need.is_empty() {
            let all: Vec<String> = dr.need.iter().map(|n| n.wants()).collect();
            line.push_str(&format!(" — {}", all.join(", ")));
        }
        out.push(line);
    }
    for dr in r.draws {
        // What arrived *and* what is left, because for anything consumed in the
        // same tick it was delivered the buffer is empty by the time this is
        // read, and "drive: nothing" on a component running at 100% is a
        // sentence that makes a player distrust the whole panel.
        let b = &s.buf[dr.port];
        out.push(format!(
            "{}: {} arrived, {}",
            part.ports[dr.port].name,
            s.got[dr.port],
            if b.qty > 0 {
                format!("holding {} of {}", b.qty, b.stuff.label())
            } else {
                "nothing held".to_string()
            }
        ));
    }

    match s.stop {
        Stop::Unmet(di, ni) => {
            let stuff = s.buf[r.draws[di].port].stuff;
            if let Some(need) = need_of(kind, di, ni) {
                if let Some(msg) = need.unmet(&stuff) {
                    out.push(format!("REFUSED — {msg}"));
                }
                out.push(hint_for(need, kind));
            }
        }
        Stop::Wrong(p) => {
            out.push(format!(
                "{} already holds {} — two substances will not share a port",
                part.ports[p].name,
                held(m, i, p)
            ));
        }
        Stop::Full(p) => {
            out.push(format!(
                "{} is full: {} of {} — nothing downstream is taking it",
                part.ports[p].name,
                s.buf[p].qty,
                part.ports[p].cap
            ));
        }
        Stop::Below(floor) => {
            let per = r.draws.first().map(|d| d.qty).unwrap_or(1);
            out.push(format!(
                "STALLED — this one does not run slowly. Below {} {}/tick it does nothing at all.",
                floor * per,
                part.ports[r.draws.first().map(|d| d.port).unwrap_or(0)].name
            ));
            out.push(
                "and a stroke that arrives unused does not queue -- it has happened".into(),
            );
            out.push("a flywheel upstream can gather a trickle into bursts big enough".into());
        }
        Stop::Short(p) => {
            out.push(format!(
                "short of {}: {} held, and a full tick wants {}",
                part.ports[p].name,
                s.buf[p].qty,
                r.draws
                    .iter()
                    .find(|d| d.port == p)
                    .map(|d| d.qty * r.rate)
                    .unwrap_or(0)
            ));
        }
        Stop::None => {}
    }

    for mk in r.makes {
        let port = &part.ports[mk.port];
        let what = if s.buf[mk.port].qty > 0 {
            s.buf[mk.port].stuff.label()
        } else {
            String::new()
        };
        let mut line = format!("{}: {}/tick", port.name, s.made[mk.port]);
        if !what.is_empty() {
            line.push_str(&format!(" of {what}"));
        }
        out.push(line);
    }
    out
}

/// The sentence that turns a refusal into a lesson. This is the whole
/// difference between a tool that says NO and a tool that teaches a mechanic.
fn hint_for(need: &Need, kind: Kind) -> String {
    match need {
        Need::MaxSpeed(_) => "put a gearbox in between and gear it down".to_string(),
        Need::MinSpeed(_) => "a gearbox with a negative ratio gears up".to_string(),
        Need::MinTemp(_) if kind == Kind::RollMill => {
            "run the billet through a furnace first".to_string()
        }
        Need::MinTemp(_) => "a preheater or a furnace upstream would fix it".to_string(),
        Need::MaxTemp(_) => "let it cool, or condense it first".to_string(),
        Need::MinSize(_) | Need::Size(_) => {
            "crush it, then mill it, before it comes here".to_string()
        }
        Need::MaxSize(_) => "this has already been through — send it on instead".to_string(),
        Need::Form(_) => "something upstream has to shape it first".to_string(),
        Need::MaxHardness(_) => "nothing in this kit will break that".to_string(),
        Need::MinPurity(_) => "a separator upstream would raise it".to_string(),
        Need::OneOf(_) => "check what the inlet feeding this is set to draw".to_string(),
    }
}

/// What is holding the machine back, worst first.
///
/// Rank by what the player can actually do something about: a refused component
/// is one property away from working and is the most fixable thing in the
/// design; a stalled turbine is producing nothing and is one tank away from
/// producing something; a venting reactor is burning fuel for nothing; a
/// starved component is a symptom of one of those somewhere upstream.
///
/// Since the legibility pass that followed experiment 14, that last clause is
/// not just a comment. Every row knows whether it is a *cause* or a *symptom* —
/// a symptom being a starved or blocked component whose trail ends somewhere
/// else in this same list — and causes are listed first whatever their
/// severity. A plant with one broken burner and fourteen starved machines
/// downstream of it used to be fifteen rows of equal weight, which is fifteen
/// places to look and one that is right.
fn holding(d: &Design, m: &Machine) -> Json {
    let mut rows: Vec<(u8, bool, Json)> = Vec::new();
    for i in 0..m.len() {
        let s = &m.st[i];
        let rank = match s.status {
            // Experiment 14's two both outrank everything: a shaking component
            // is wrong in the document and will never work, and an overheated
            // one will not restart on its own.
            Status::Shaking => 0,
            Status::Overheated => 0,
            Status::Refused => 1,
            Status::Stalled => 2,
            Status::Venting => 3,
            Status::Blocked => 4,
            Status::Starved => 5,
            Status::Idle if !matches!(m.kinds[i], Kind::Tank | Kind::Drum | Kind::Flywheel) => 6,
            _ => continue,
        };
        // Whether this row is somebody else's fault, and whose.
        let root = m.root_cause(i);
        let symptom = matches!(s.status, Status::Starved | Status::Blocked) && root.is_some();
        let lines = why(d, m, i);
        // The line worth putting on the row is the one that names the problem.
        let line = lines
            .iter()
            .find(|l| {
                l.starts_with("REFUSED")
                    || l.starts_with("SHAKING")
                    || l.starts_with("OVERHEATED")
                    || l.starts_with("nothing is wired")
                    || l.contains("follow it back")
                    || l.contains("short of")
                    || l.contains("is full")
                    || l.contains("below the")
                    || l.contains("too cold")
                    || l.contains("unused heat")
                    || l.contains("will not share")
            })
            .cloned()
            .or_else(|| lines.get(1).cloned())
            .unwrap_or_default();
        rows.push((
            rank,
            symptom,
            Json::obj()
                .set("name", m.names[i].clone())
                .set("kind", parts::part(m.kinds[i]).tag)
                .set("title", parts::part(m.kinds[i]).title)
                .set("status", s.status.tag())
                .set("util", Json::Real(pct(s.util)))
                .set("cause", !symptom)
                .set(
                    "because",
                    match root {
                        Some(r) if symptom => Json::Str(m.names[r].clone()),
                        _ => Json::Null,
                    },
                )
                .set("why", line),
        ));
    }
    // Causes before symptoms, and then worst first within each half.
    rows.sort_by_key(|(r, symptom, _)| (*symptom, *r));
    Json::Arr(rows.into_iter().map(|(_, _, j)| j).collect())
}

