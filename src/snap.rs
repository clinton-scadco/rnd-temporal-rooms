//! The factory at tick *T*, in the shape a renderer needs.
//!
//! The rule the workbench is built on is that the view owns no factory state.
//! It does not step anything, integrate anything or remember anything it was
//! not told; it asks
//!
//! > what does this plant look like at tick 182,400?
//!
//! and draws the answer. Between two answers it may interpolate, and this
//! module's job is to hand it enough to do that honestly.
//!
//! The load-bearing part is transport. A train's position is never simulated
//! and never stored -- there is no xy anywhere in this crate -- but v3 already
//! keeps, for every batch in the air, the tick it lands. Subtract the leg's
//! latency and you have the tick it left:
//!
//! ```text
//! departure = arrival - latency
//! progress  = (render_time - departure) / (arrival - departure)
//! ```
//!
//! So the renderer gets a list of `(departure, arrival, vehicles)` and draws
//! each one wherever that implies. Nothing new is computed to move a train
//! across a screen; the numbers were already in the population buckets.
//!
//! The other half is `x 1,000,000`. A class is one number and a distribution
//! over a handful of states, so the snapshot ships the distribution rather than
//! the machines. Whether the client draws that as one industrial installation
//! with a utilisation bar or as five thousand individual furnaces is a question
//! about the camera, not about the simulation.

use crate::domains::Channel;
use crate::json::Json;
use crate::model::*;
use crate::pop::ClassPop;
use crate::rooms::{Plan, Room};

/// A vehicle leg in the air at the moment of the snapshot.
pub struct Flight {
    pub depart: Tick,
    pub arrive: Tick,
    pub vehicles: u64,
    /// False for the empty trip home.
    pub loaded: bool,
}

/// The four places a transport's vehicles can be, gathered from whichever
/// region holds each end.
pub struct Traffic {
    pub waiting_to_load: u64,
    pub waiting_to_unload: u64,
    pub flights: Vec<Flight>,
}

/// Where a class lives, and what it is doing.
fn class_in<'a>(
    room: &'a Room<'_>,
    plan: &Plan,
    region: usize,
    class: u16,
) -> Option<&'a ClassPop> {
    let local = plan.class_down[region][class as usize];
    if local == u16::MAX {
        None
    } else {
        Some(&room.pops[region].classes[local as usize])
    }
}

/// Vehicles of one transport class, put back together across the region
/// boundary the channel runs over.
///
/// The four buckets a class already had are exactly the four places a vehicle
/// can be, which is why lifting a transport out of two regions needed no new
/// state in the first place:
///
/// ```text
/// starved    waiting to load       lives in the sending region
/// working    loaded, in transit    lives in the receiving region
/// done       waiting to unload     lives in the receiving region
/// returning  empty, going home     lives in the sending region
/// ```
pub fn traffic(room: &Room, plan: &Plan, class: u16, a: &ActorDef) -> Traffic {
    let g = &plan.graph;
    let ch = g.channels.iter().find(|c| c.class == class);
    let (src, dst) = match ch {
        Some(c) => (c.src_region, c.dst_region),
        // Not lifted: both ends are in the one region that holds the class.
        None => {
            let r = g.of_class[class as usize];
            (r, r)
        }
    };
    let mut t = Traffic { waiting_to_load: 0, waiting_to_unload: 0, flights: Vec::new() };
    if let Some(sending) = class_in(room, plan, src, class) {
        t.waiting_to_load = sending.starved;
        for &(home, n) in &sending.returning {
            t.flights.push(Flight {
                depart: home.saturating_sub(a.return_latency),
                arrive: home,
                vehicles: n,
                loaded: false,
            });
        }
    }
    if let Some(receiving) = class_in(room, plan, dst, class) {
        t.waiting_to_unload = receiving.done;
        for &(land, n) in &receiving.working {
            t.flights.push(Flight {
                depart: land.saturating_sub(a.duration),
                arrive: land,
                vehicles: n,
                loaded: true,
            });
        }
    }
    t.flights.sort_by_key(|f| (f.arrive, f.loaded));
    t
}

// =============================================================== the snapshot

/// Everything the view is allowed to know at tick `t`.
pub fn render(prog: &Program, bp: &Blueprint, plan: &Plan, room: &Room, t: Tick) -> Json {
    let g = &plan.graph;
    let counters = room.counters();
    let item = |i: ItemId| prog.items[i as usize].clone();

    // ---- storages
    let storages: Vec<Json> = bp
        .storages
        .iter()
        .enumerate()
        .map(|(s, sd)| {
            let held: Vec<Json> = sd
                .slots
                .iter()
                .map(|&it| {
                    Json::obj()
                        .set("item", item(it))
                        .set("qty", Json::big(room.storage_qty(s, it) as u128))
                })
                .collect();
            Json::obj()
                .set("name", sd.name.clone())
                .set("region", region_of(g.of_storage[s]))
                .set("capacity", Json::big(sd.capacity as u128))
                .set("used", Json::big(room.storage_used(s) as u128))
                .set("policy", sd.policy.label())
                .set("shared", sd.shared)
                .set("held", Json::Arr(held))
        })
        .collect();

    // ---- classes
    let merged = room.classes();
    let classes: Vec<Json> = bp
        .actors
        .iter()
        .enumerate()
        .map(|(c, a)| {
            let pop = &merged[c];
            let working: Vec<Json> = pop
                .working
                .iter()
                .map(|&(at, n)| {
                    Json::obj()
                        .set("at", at)
                        .set("left", at.saturating_sub(t))
                        .set("n", Json::big(n as u128))
                })
                .collect();
            let returning: Vec<Json> = pop
                .returning
                .iter()
                .map(|&(at, n)| {
                    Json::obj()
                        .set("at", at)
                        .set("left", at.saturating_sub(t))
                        .set("n", Json::big(n as u128))
                })
                .collect();
            Json::obj()
                .set("name", a.name.clone())
                .set("kind", a.kind.label())
                .set("region", region_of(g.of_class[c]))
                .set("count", Json::big(a.count as u128))
                .set("duration", a.duration)
                .set("returns", a.return_latency)
                .set("idle", Json::big(pop.starved as u128))
                .set("blocked", Json::big(pop.done as u128))
                .set("busy", Json::big(pop.working_total() as u128))
                .set("working", Json::Arr(working))
                .set("returning", Json::Arr(returning))
                .set("cycles", Json::big(counters.cycles[c] as u128))
                .set("states", pop.distinct_states())
                .set("inputs", stacks(prog, &a.inputs))
                .set("outputs", stacks(prog, &a.outputs))
                .set("shared", a.shared)
        })
        .collect();

    // ---- transports, with what is in the air right now
    let links: Vec<Json> = bp
        .actors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.is_link())
        .map(|(c, a)| {
            let tr = traffic(room, plan, c as u16, a);
            let flights: Vec<Json> = tr
                .flights
                .iter()
                .map(|f| {
                    Json::obj()
                        .set("depart", f.depart)
                        .set("arrive", f.arrive)
                        .set("n", Json::big(f.vehicles as u128))
                        .set("loaded", f.loaded)
                })
                .collect();
            let (num, den) = a.throughput();
            let ch: Option<&Channel> = plan.graph.channels.iter().find(|x| x.class == c as u16);
            Json::obj()
                .set("name", a.name.clone())
                .set("class", c)
                .set("from", a.primary_in().map(|s| bp.storages[s as usize].name.clone()))
                .set("to", a.primary_out().map(|s| bp.storages[s as usize].name.clone()))
                .set("item", a.inputs.first().map(|s| item(s.item)))
                .set("batch", Json::big(a.inputs.first().map_or(0, |s| s.qty) as u128))
                .set("vehicles", Json::big(a.count as u128))
                .set("latency", a.duration)
                .set("returns", a.return_latency)
                .set("rate", if den == 0 { 0.0 } else { num as f64 / den as f64 })
                .set("channel", ch.is_some())
                .set("srcRegion", ch.map(|c| c.src_region))
                .set("dstRegion", ch.map(|c| c.dst_region))
                .set("waitingToLoad", Json::big(tr.waiting_to_load as u128))
                .set("waitingToUnload", Json::big(tr.waiting_to_unload as u128))
                .set("flights", Json::Arr(flights))
        })
        .collect();

    // ---- regions
    let regions: Vec<Json> = g
        .regions
        .iter()
        .enumerate()
        .map(|(r, reg)| {
            Json::obj()
                .set("index", r)
                .set("clock", room.clock(r))
                .set(
                    "slack",
                    match reg.slack(&g.channels) {
                        Some(s) => Json::Int(s as i128),
                        None => Json::Null,
                    },
                )
                .set("mode", room.modes[r].label())
                .set("machines", Json::big(reg.machines as u128))
                .set("capacity", Json::big(reg.capacity as u128))
                .set(
                    "storages",
                    Json::arr(
                        reg.storages
                            .iter()
                            .map(|&s| bp.storages[s as usize].name.clone())
                            .collect::<Vec<_>>(),
                    ),
                )
                .set(
                    "classes",
                    Json::arr(
                        reg.classes
                            .iter()
                            .map(|&c| bp.actors[c as usize].name.clone())
                            .collect::<Vec<_>>(),
                    ),
                )
        })
        .collect();

    let produced: Vec<Json> = prog
        .items
        .iter()
        .enumerate()
        .map(|(i, name)| {
            Json::obj()
                .set("item", name.clone())
                .set("produced", Json::big(counters.produced[i] as u128))
                .set("consumed", Json::big(counters.consumed[i] as u128))
        })
        .collect();

    // The earliest thing that will happen anywhere. The client may interpolate
    // freely up to here and must ask again after it.
    let next = room.pops.iter().filter_map(|p| p.next_time()).min();

    Json::obj()
        .set("tick", t)
        .set("nextEvent", next.map(|n| Json::Int(n as i128)))
        .set("storages", Json::Arr(storages))
        .set("classes", Json::Arr(classes))
        .set("links", Json::Arr(links))
        .set("regions", Json::Arr(regions))
        .set("items", Json::Arr(produced))
}

/// A class lifted out of two regions belongs to neither, and `of_class` says
/// so with a sentinel. A view that rendered `18446744073709551615` as a region
/// number would be reporting the sentinel as a fact.
fn region_of(r: usize) -> Json {
    if r == usize::MAX {
        Json::Null
    } else {
        Json::Int(r as i128)
    }
}

fn stacks(prog: &Program, v: &[Stack]) -> Json {
    Json::arr(
        v.iter()
            .map(|s| {
                Json::obj()
                    .set("item", prog.items[s.item as usize].clone())
                    .set("qty", Json::big(s.qty as u128))
            })
            .collect::<Vec<_>>(),
    )
}

/// The plant's shape, which changes only when the source does.
pub fn plant(prog: &Program, bp: &Blueprint, plan: &Plan, room: &Room) -> Json {
    let g = &plan.graph;
    let mut cells = 0usize;
    for p in &room.pops {
        cells += p.distinct_states();
    }
    Json::obj()
        .set("name", bp.name.clone())
        .set("objects", Json::big(prog.total_objects()))
        .set("machines", Json::big(bp.machines as u128))
        .set("classes", bp.actors.len())
        .set("storages", bp.storages.len())
        .set("cells", cells)
        .set("regions", g.regions.len())
        .set("fused", g.fused)
        .set("basePeriod", bp.base_period)
        .set(
            "minSlack",
            match g.min_slack() {
                Some(s) => Json::Int(s as i128),
                None => Json::Null,
            },
        )
        .set("items", Json::arr(prog.items.clone()))
        .set(
            "channels",
            Json::Arr(
                g.channels
                    .iter()
                    .map(|c| {
                        Json::obj()
                            .set("name", bp.actors[c.class as usize].name.clone())
                            .set("src", c.src_region)
                            .set("dst", c.dst_region)
                            .set("latency", c.latency)
                            .set("returns", c.return_latency)
                            .set("from", bp.storages[c.from_store as usize].name.clone())
                            .set("to", bp.storages[c.to_store as usize].name.clone())
                    })
                    .collect::<Vec<_>>(),
            ),
        )
}

/// The scheduler's own log: which region ran alone, from when, to when.
pub fn timetable(room: &Room) -> Json {
    // A picture of a run needs enough bars to read, not every bar there was.
    const DRAWN: usize = 4_000;
    let all = room.trace.as_deref().unwrap_or(&[]);
    let advances: Vec<Json> = all
        .iter()
        .take(DRAWN)
        .map(|a| {
            Json::obj()
                .set("region", a.region)
                .set("from", a.from)
                .set("to", a.to)
                .set("blocked", a.blocked)
        })
        .collect();
    Json::obj()
        .set("advances", Json::Arr(advances))
        .set("recorded", all.len())
        .set("truncated", all.len() > DRAWN)
        .set("steps", Json::big(room.steps as u128))
        .set("messages", Json::big(room.messages as u128))
        .set("rendezvous", Json::big(room.rendezvous as u128))
        .set("maxSkew", room.max_skew)
        .set("maxAdvance", room.max_advance)
        .set("meanAdvance", room.mean_advance())
        .set("skewClocks", Json::arr(room.skew_clocks.iter().map(|&t| t as i64).collect::<Vec<_>>()))
}
