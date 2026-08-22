//! Why is this not running?
//!
//! Factory games live or die on whether a player can find out why their
//! magnificent industrial spaghetti has stopped. The usual answer is a status
//! word floating over a machine -- `STARVED` -- which tells you the thing you
//! could already see and not one thing you did not know.
//!
//! What a player actually needs is the *sentence after* the status word:
//!
//! ```text
//!   Smelter                       Smelter
//!   STARVED                       BLOCKED
//!   needs 25 IronOre              holding 25 IronPlate
//!   OreYard holds 0               PlateBay is 100% full
//!   Rail is 100% busy             GearPress wants 0.25/tick of the 0.50 arriving
//!   next delivery lands t=14,200  and is itself 100% busy
//! ```
//!
//! Every number in those two boxes was already in the simulation state before
//! this module existed. The bay contents are in `Pop::qty`; the machine
//! populations are the four buckets; the arrival tick of the next train is the
//! deadline v3 has stored for every batch in the air since the day transports
//! became channels. Nothing here computes any physics -- it *reads* -- which
//! is why it was worth writing now rather than after a rendering pass that
//! would have had to invent somewhere to put it.
//!
//! # The one judgement it makes
//!
//! A class with ten thousand members is rarely in one state: some are working,
//! some are starved, some are blocked. So the diagnosis picks the condition
//! that most of the *non-working* members are in, and says how many are in it.
//! That is a summary and it is labelled as one. The four exact counts travel
//! beside it, because a summary that replaced the numbers would be the third
//! opinion this codebase keeps refusing to have.

use crate::json::Json;
use crate::model::*;
use crate::pop::ClassPop;
use crate::rooms::{Plan, Room};
use crate::snap;

/// What is wrong with a class, in one word.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Condition {
    /// Nothing is wrong: every member is mid-cycle.
    Running,
    /// Waiting for ingredients that are not in its bay.
    Starved,
    /// Finished, holding the output, with nowhere to put it.
    Blocked,
    /// A transport whose vehicles are waiting at the loading end for cargo.
    WaitingToLoad,
    /// A transport whose vehicles have arrived and cannot unload.
    WaitingToUnload,
    /// Starved, and what it is starved of arrives by transport that is
    /// already running flat out. The distinction matters because the fix is a
    /// different purchase.
    TransportLimited,
    /// Starved of something whose source is empty and has nothing refilling
    /// it: an exhausted deposit, not a slow one.
    Exhausted,
    /// Nothing is pending anywhere and nothing ever will be.
    Stopped,
}

impl Condition {
    pub fn label(self) -> &'static str {
        match self {
            Condition::Running => "RUNNING",
            Condition::Starved => "STARVED",
            Condition::Blocked => "BLOCKED",
            Condition::WaitingToLoad => "WAITING TO LOAD",
            Condition::WaitingToUnload => "WAITING TO UNLOAD",
            Condition::TransportLimited => "TRANSPORT LIMITED",
            Condition::Exhausted => "EXHAUSTED",
            Condition::Stopped => "STOPPED",
        }
    }
}

/// Sustained appetite or output of a class, in items per tick, exact as a
/// ratio and rounded only for display.
fn rate(count: u64, qty: Qty, per: Tick) -> f64 {
    if per == 0 {
        return 0.0;
    }
    count as f64 * qty as f64 / per as f64
}

fn pct(a: u64, b: u64) -> f64 {
    if b == 0 {
        0.0
    } else {
        100.0 * a as f64 / b as f64
    }
}

/// How busy a class is: the share of its members that are neither waiting for
/// input nor stuck holding output.
pub fn utilisation(p: &ClassPop) -> f64 {
    let total = p.total();
    if total == 0 {
        return 0.0;
    }
    (p.working_total() + p.returning.iter().map(|r| r.1).sum::<u64>()) as f64 / total as f64
}

/// The earliest moment anything lands in `storage`, and what is bringing it.
fn next_delivery(
    bp: &Blueprint,
    plan: &Plan,
    room: &Room,
    pops: &[ClassPop],
    storage: usize,
) -> Option<(Tick, String)> {
    let mut best: Option<(Tick, String)> = None;
    for &c in &bp.storages[storage].givers {
        let a = &bp.actors[c as usize];
        if a.is_link() {
            let tr = snap::traffic(room, plan, c, a);
            for f in tr.flights.iter().filter(|f| f.loaded) {
                if best.as_ref().is_none_or(|(t, _)| f.arrive < *t) {
                    best = Some((f.arrive, a.name.clone()));
                }
            }
        } else if let Some(&(at, _)) = pops[c as usize].working.first() {
            {
                if best.as_ref().is_none_or(|(t, _)| at < *t) {
                    best = Some((at, a.name.clone()));
                }
            }
        }
    }
    best
}

/// A machine class, and why it is in the state it is in.
pub fn diagnose(
    prog: &Program,
    bp: &Blueprint,
    plan: &Plan,
    room: &Room,
    pops: &[ClassPop],
    class: usize,
    t: Tick,
) -> Json {
    let a = &bp.actors[class];
    let p = &pops[class];
    let total = p.total().max(1);
    let busy = p.working_total();
    let home: u64 = p.returning.iter().map(|r| r.1).sum();
    let idle = p.starved;
    let blocked = p.done;

    // Whichever way most of the stopped members are stopped. A class where
    // nothing is stopped is running, whatever else is true of it.
    let stuck = idle + blocked;
    let mut cond = if stuck == 0 {
        Condition::Running
    } else if idle >= blocked {
        if a.is_link() {
            Condition::WaitingToLoad
        } else {
            Condition::Starved
        }
    } else if a.is_link() {
        Condition::WaitingToUnload
    } else {
        Condition::Blocked
    };

    // ---- what it is short of, and where that was supposed to come from
    let mut needs: Vec<Json> = Vec::new();
    let mut upstream: Vec<Json> = Vec::new();
    let mut soonest: Option<(Tick, String)> = None;
    if matches!(cond, Condition::Starved | Condition::WaitingToLoad) {
        for &s in &a.in_stores {
            let sd = &bp.storages[s as usize];
            for want in &a.inputs {
                if !sd.slots.contains(&want.item) {
                    continue;
                }
                let have = room.storage_qty(s as usize, want.item);
                needs.push(
                    Json::obj()
                        .set("item", prog.item_name(want.item).to_string())
                        .set("perCycle", Json::big(want.qty as u128))
                        .set("bay", sd.name.clone())
                        .set("available", Json::big(have as u128))
                        .set("short", have < want.qty),
                );
            }
            if let Some((at, who)) = next_delivery(bp, plan, room, pops, s as usize) {
                if soonest.as_ref().is_none_or(|(b, _)| at < *b) {
                    soonest = Some((at, who));
                }
            }
            // Everyone who fills this bay, and whether they are managing it.
            for &g in &sd.givers {
                let up = &bp.actors[g as usize];
                let upop = &pops[g as usize];
                let u = utilisation(upop);
                let starved_up = upop.starved;
                upstream.push(
                    Json::obj()
                        .set("name", up.name.clone())
                        .set("kind", up.kind.label())
                        .set("bay", sd.name.clone())
                        .set("utilisation", u)
                        .set("idle", Json::big(starved_up as u128))
                        .set("blocked", Json::big(upop.done as u128))
                        .set(
                            "rate",
                            rate(
                                up.count,
                                up.outputs.iter().find(|o| sd.slots.contains(&o.item)).map_or(0, |o| o.qty),
                                up.cycle(),
                            ),
                        ),
                );
                // Starved behind a transport that is already flat out is a
                // different problem from starved behind a slow machine, and it
                // costs a different thing to fix.
                if up.is_link() && upop.starved == 0 && cond == Condition::Starved {
                    cond = Condition::TransportLimited;
                }
            }
            // A bay with nothing in it, nobody filling it and no delivery on
            // the way is not slow. It is finished.
            if sd.givers.is_empty() && room.storage_used(s as usize) == 0 {
                cond = Condition::Exhausted;
            }
        }
    }

    // ---- what it is holding, and who was supposed to take it
    let mut holding: Vec<Json> = Vec::new();
    let mut downstream: Vec<Json> = Vec::new();
    if matches!(cond, Condition::Blocked | Condition::WaitingToUnload) {
        for &s in &a.out_stores {
            let sd = &bp.storages[s as usize];
            let used = room.storage_used(s as usize);
            for made in &a.outputs {
                if !sd.slots.contains(&made.item) {
                    continue;
                }
                holding.push(
                    Json::obj()
                        .set("item", prog.item_name(made.item).to_string())
                        .set("qty", Json::big((made.qty * blocked.max(1)) as u128))
                        .set("bay", sd.name.clone())
                        .set("used", Json::big(used as u128))
                        .set("capacity", Json::big(sd.capacity as u128))
                        .set("full", pct(used, sd.capacity)),
                );
            }
            for &k in &sd.takers {
                let down = &bp.actors[k as usize];
                let dpop = &pops[k as usize];
                downstream.push(
                    Json::obj()
                        .set("name", down.name.clone())
                        .set("kind", down.kind.label())
                        .set("bay", sd.name.clone())
                        .set("utilisation", utilisation(dpop))
                        .set("idle", Json::big(dpop.starved as u128))
                        .set("blocked", Json::big(dpop.done as u128))
                        .set(
                            "rate",
                            rate(
                                down.count,
                                down.inputs.iter().find(|i| sd.slots.contains(&i.item)).map_or(0, |i| i.qty),
                                down.cycle(),
                            ),
                        ),
                );
            }
        }
    }

    if cond != Condition::Running && busy == 0 && home == 0 && room.pops.iter().all(|p| p.frozen()) {
        cond = Condition::Stopped;
    }

    Json::obj()
        .set("state", cond.label())
        .set("headline", headline(cond, a, idle, blocked, total))
        .set("utilisation", utilisation(p))
        .set("busy", Json::big(busy as u128))
        .set("homebound", Json::big(home as u128))
        .set("idle", Json::big(idle as u128))
        .set("blockedCount", Json::big(blocked as u128))
        .set("needs", Json::Arr(needs))
        .set("holding", Json::Arr(holding))
        .set("upstream", Json::Arr(upstream))
        .set("downstream", Json::Arr(downstream))
        .set("nextDelivery", soonest.as_ref().map(|(at, _)| Json::Int(*at as i128)))
        .set("nextDeliveryIn", soonest.as_ref().map(|(at, _)| Json::Int(at.saturating_sub(t) as i128)))
        .set("nextDeliveryBy", soonest.map(|(_, who)| who))
}

fn headline(c: Condition, a: &ActorDef, idle: u64, blocked: u64, total: u64) -> String {
    let some = |n: u64| {
        if n == total {
            "every one".to_string()
        } else {
            format!("{n} of {total}")
        }
    };
    match c {
        Condition::Running => "everything is mid-cycle".into(),
        Condition::Starved | Condition::Exhausted => {
            format!("{} waiting for ingredients", some(idle))
        }
        Condition::TransportLimited => {
            format!("{} waiting for a delivery that is already running flat out", some(idle))
        }
        Condition::Blocked => format!("{} finished, with nowhere to put the output", some(blocked)),
        Condition::WaitingToLoad => format!("{} parked at the loading bay with nothing to carry", some(idle)),
        Condition::WaitingToUnload => {
            format!("{} arrived and cannot unload", some(blocked))
        }
        Condition::Stopped => format!("`{}` has nothing left to do, ever", a.name),
    }
}

// ============================================================= the plant

/// What is holding the whole plant back.
///
/// The definition is deliberately mechanical rather than clever: a class is a
/// **constraint** when it never waits -- no member of it is ever idle or
/// blocked at this instant -- and something that draws on it *is* waiting. A
/// machine that is flat out while its customer starves is, by construction,
/// the thing its customer is waiting for.
///
/// This finds the honest bottleneck and not the loud one. A plant whose ore
/// bay is overflowing looks like it has a transport problem; if the smelters
/// behind that transport are themselves blocked, the transport is not the
/// constraint and buying more of it changes nothing.
pub fn constraints(bp: &Blueprint, room: &Room, pops: &[ClassPop]) -> Json {
    let _ = room;
    let flat_out = |c: usize| {
        let p = &pops[c];
        p.total() > 0 && p.starved == 0 && p.done == 0
    };
    let mut found: Vec<Json> = Vec::new();
    for (c, a) in bp.actors.iter().enumerate() {
        if !flat_out(c) {
            continue;
        }
        // Everyone downstream: whoever takes from a bay this class fills.
        let mut waiting: Vec<String> = Vec::new();
        for &s in &a.out_stores {
            for &k in &bp.storages[s as usize].takers {
                if k as usize != c && pops[k as usize].starved > 0 {
                    waiting.push(bp.actors[k as usize].name.clone());
                }
            }
        }
        if waiting.is_empty() {
            continue;
        }
        let (num, den) = if a.is_link() {
            a.throughput()
        } else {
            (a.count as u128 * a.outputs.iter().map(|o| o.qty).sum::<Qty>() as u128, a.cycle().max(1) as u128)
        };
        found.push(
            Json::obj()
                .set("name", a.name.clone())
                .set("kind", a.kind.label())
                .set("rate", if den == 0 { 0.0 } else { num as f64 / den as f64 })
                .set("starving", Json::arr(waiting)),
        );
    }
    Json::Arr(found)
}
