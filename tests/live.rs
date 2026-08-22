//! An edit is allowed to change the factory. It is not allowed to change the
//! past.
//!
//! Prototype 1 puts a barrier, a harvest, a recompile and a reseed in the
//! middle of a run. Every one of those is a chance to lose a batch, hand a
//! round-robin turn to the wrong machine, or quietly restart a cycle that was
//! half finished -- and none of it would look wrong on screen. A factory that
//! silently drops four hundred ore every time you place a bay is a factory
//! whose numbers mean nothing.
//!
//! So the load-bearing test is the one where the edit does nothing at all.
//! Retune a node to exactly what it already is at tick k: the plant is the
//! same plant, the source is byte-identical, and the only difference between
//! the two runs is that one of them went through the whole edit machinery. If
//! the states still agree at every probe afterwards, the machinery carries
//! everything and invents nothing.

use std::collections::HashMap;
use temporal_rooms::graph::{Graph, Kind, Node};
use temporal_rooms::json;
use temporal_rooms::live::{self, Carry, Command, Edit, Log};
use temporal_rooms::model::*;
use temporal_rooms::{dsl, snap};

const CONFIGS: &[&str] = &[
    "configs/01-spec.factory",
    "configs/02-balanced.factory",
    "configs/04-science.factory",
    "configs/05-coupled.factory",
    "configs/06-cycle.factory",
    "configs/07-transport.factory",
    "configs/08-policy.factory",
    "configs/09-population.factory",
    "configs/10-billion.factory",
    "configs/11-railchain.factory",
    "configs/12-tradeloop.factory",
    "configs/15-continent.factory",
];

fn log_of(path: &str) -> Log {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let prog = dsl::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"));
    let mut g = Graph::from_program(&prog);
    g.apply_positions(&src);
    Log::new(g)
}

fn sig(log: &Log, t: Tick) -> Vec<u8> {
    live::carry_at(log, t).unwrap_or_else(|e| panic!("t={t}: {}", e.msg)).signature()
}

/// A node retuned to exactly what it already is: a command that is a genuine
/// command -- it forces the barrier, the recompile and the reseed -- and a
/// genuine no-op.
fn same_again(g: &Graph, at: Tick) -> Command {
    let n = g.nodes.iter().find(|n| n.kind.is_machine()).expect("a plant with no machines");
    Command { at, edit: Edit::Retune(n.clone()) }
}

#[test]
fn an_edit_that_changes_nothing_changes_nothing() {
    const PROBES: &[Tick] = &[1, 500, 3_000, 9_000, 40_000];
    for path in CONFIGS {
        let plain = log_of(path);
        for &cut in &[1u64, 137, 900, 5_000] {
            let mut edited = plain.clone();
            edited.commands.push(same_again(&plain.base, cut));
            // The plant is the same plant, stated as bytes rather than hoped.
            assert_eq!(
                plain.graph_at(cut).unwrap().emit(),
                edited.graph_at(cut).unwrap().emit(),
                "{path}: the no-op edit was not a no-op"
            );
            for &t in PROBES.iter().filter(|&&t| t >= cut) {
                assert_eq!(
                    sig(&plain, t),
                    sig(&edited, t),
                    "{path}: an edit at t={cut} changed the state at t={t}"
                );
            }
        }
    }
}

/// Several no-op edits in a row, at ticks that straddle everything
/// interesting. Each one is another barrier, another harvest and another
/// reseed, and the errors a carry can make are exactly the kind that
/// accumulate.
#[test]
fn edits_do_not_accumulate_error() {
    for path in CONFIGS {
        let plain = log_of(path);
        let mut edited = plain.clone();
        for &at in &[1u64, 7, 60, 137, 601, 1_000, 2_500, 6_000, 12_000] {
            edited.commands.push(same_again(&plain.base, at));
        }
        for &t in &[12_000u64, 20_000, 45_000] {
            assert_eq!(sig(&plain, t), sig(&edited, t), "{path}: nine no-op edits drifted by t={t}");
        }
    }
}

/// The same state, reached by one long run and by a run that stopped at every
/// boundary to hand its state to the next epoch.
///
/// This is the incremental path the server takes when a timeline is being
/// scrubbed, and the property the networking proof will need: a snapshot at a
/// boundary plus the rest of the log is the same thing as the whole log.
#[test]
fn a_carry_is_worth_the_ticks_it_replaces() {
    for path in CONFIGS {
        let mut log = log_of(path);
        for &at in &[500u64, 4_000] {
            log.commands.push(same_again(&log.base.clone(), at));
        }
        for &t in &[4_000u64, 9_000, 30_000] {
            let whole = sig(&log, t);
            let mid = live::carry_at(&log, 4_000).unwrap();
            let jumped = live::with_state_from(&log, t, Some((4_000, &mid)), |a| {
                Carry::take(a.room, a.prog, a.bp, t)
            })
            .unwrap()
            .signature();
            assert_eq!(whole, jumped, "{path}: resuming from t=4,000 did not reach t={t}");
        }
    }
}

/// A carry that has been through JSON is the same carry. This is the
/// serialisation a joining client would be handed, so a field that quietly
/// fails to survive it is a desynchronisation waiting for its first player.
#[test]
fn a_carry_survives_the_wire() {
    for path in CONFIGS {
        let log = log_of(path);
        for &t in &[0u64, 137, 6_000] {
            let c = live::carry_at(&log, t).unwrap();
            let back = Carry::from_json(&json::parse(&c.to_json().to_string()).unwrap()).unwrap();
            assert_eq!(c.signature(), back.signature(), "{path}: a carry did not survive JSON at t={t}");
            assert_eq!(c.now, back.now);
            assert_eq!(c.cycles, back.cycles, "{path}: cycle counts did not survive JSON");
            assert_eq!(c.produced, back.produced);
            assert_eq!(c.consumed, back.consumed);
        }
    }
}

/// A log that has been through JSON is the same log.
#[test]
fn a_log_survives_the_wire() {
    let mut log = log_of("configs/11-railchain.factory");
    let n = log.base.nodes[0].clone();
    log.commands.push(Command { at: 100, edit: Edit::Item("Widget".into()) });
    log.commands.push(Command { at: 200, edit: Edit::Retune(n.clone()) });
    log.commands.push(Command { at: 200, edit: Edit::Remove("nothing".into()) });
    log.commands.push(Command {
        at: 300,
        edit: Edit::Wire { from: "a".into(), to: "b".into(), item: Some("Ore".into()) },
    });
    log.commands.push(Command { at: 400, edit: Edit::Unwire { from: "a".into(), to: "b".into() } });
    log.commands.push(Command { at: 500, edit: Edit::Place(n) });
    let back = Log::from_json(&json::parse(&log.to_json().to_string()).unwrap()).unwrap();
    assert_eq!(log, back);
}

// ============================================ edits that are not no-ops

fn gears() -> Log {
    log_of("configs/p1-gears.factory")
}

fn node<'a>(g: &'a Graph, name: &str) -> &'a Node {
    g.node(name).unwrap_or_else(|| panic!("no `{name}` in the plant"))
}

fn delivered(log: &Log, t: Tick, item: &str) -> u64 {
    live::carry_at(log, t).unwrap().consumed.get(item).copied().unwrap_or(0)
}

/// Buying machines at tick k must make more of the thing, must make no
/// difference at all before tick k, and must make its difference *at* tick k
/// rather than at the tick after it.
#[test]
fn an_upgrade_changes_the_future_and_not_the_past() {
    let plain = gears();
    let mut faster = plain.clone();
    let mut rail = node(&plain.base, "Rail").clone();
    rail.count = 6;
    faster.commands.push(Command { at: 20_000, edit: Edit::Retune(rail) });

    for &t in &[1u64, 5_000, 19_999] {
        assert_eq!(sig(&plain, t), sig(&faster, t), "the upgrade leaked backwards to t={t}");
    }
    // An edit is applied at its own tick, not after it: the five new vehicles
    // exist at 20,000, and a plant that showed them at 20,001 would be a plant
    // whose command log did not mean what it said.
    assert_ne!(
        sig(&plain, 20_000),
        sig(&faster, 20_000),
        "the edit at t=20,000 had not happened at t=20,000"
    );
}

/// The upgrade that is worth buying makes more of the thing.
#[test]
fn buying_the_right_upgrade_buys_something() {
    let plain = gears();
    let mut better = plain.clone();
    let mut press = node(&plain.base, "GearPress").clone();
    press.count = 2;
    better.commands.push(Command { at: 20_000, edit: Edit::Retune(press) });
    let before = delivered(&plain, 120_000, "Gear");
    let after = delivered(&better, 120_000, "Gear");
    assert!(
        after > before,
        "a second gear press delivered {after} gears where one delivered {before}"
    );
}

/// And the upgrade that is not worth buying buys *nothing at all*, which is
/// the entire point of the scenario and the reason it exists.
///
/// `configs/p1-gears.factory` is underbuilt in two places. The rail is the
/// bottleneck a player notices first, because it is the one with a queue in
/// front of it; the gear press is the bottleneck that is actually binding.
/// Six rail vehicles where there was one deliver exactly as many gears as one
/// did -- to the unit -- and the plant gives no credit for the money.
///
/// This test was written to assert the opposite and failed, which is a fair
/// description of how the mistake feels to make.
#[test]
fn buying_the_wrong_upgrade_buys_nothing() {
    let plain = gears();
    let mut faster = plain.clone();
    let mut rail = node(&plain.base, "Rail").clone();
    rail.count = 6;
    faster.commands.push(Command { at: 20_000, edit: Edit::Retune(rail) });
    assert_eq!(
        delivered(&faster, 120_000, "Gear"),
        delivered(&plain, 120_000, "Gear"),
        "six rail vehicles were supposed to be a waste of money"
    );
    // The money did buy *something*: the ore now waits at the far end instead
    // of the near one. It just does not become a gear any faster.
    let stuck = live::carry_at(&faster, 120_000).unwrap();
    assert!(
        stuck.qty[&("OreYard".to_string(), "IronOre".to_string())] > 0,
        "the ore did not even make it across"
    );
}

/// The vehicles that were already in the air when the fleet grew are still in
/// the air afterwards, arriving when they were always going to arrive.
#[test]
fn an_upgrade_does_not_teleport_what_is_already_moving() {
    let plain = gears();
    let cut = 6_000;
    let before = live::carry_at(&plain, cut).unwrap();
    let flying = before.classes["Rail"].clone();

    let mut bigger = plain.clone();
    let mut rail = node(&plain.base, "Rail").clone();
    rail.count += 4;
    bigger.commands.push(Command { at: cut, edit: Edit::Retune(rail) });
    let after = live::carry_at(&bigger, cut).unwrap();
    let grown = &after.classes["Rail"];

    assert_eq!(grown.total(), flying.total() + 4, "the new vehicles did not arrive");
    // Every leg that was in the air is still in the air, landing when it was
    // always going to land. The new vehicles may add legs of their own -- they
    // are idle at a loading bay with ore in it, so of course they load -- but
    // they cannot land before they could have departed.
    for &(at, n) in &flying.working {
        let still = grown.working.iter().find(|w| w.0 == at).map(|w| w.1).unwrap_or(0);
        assert!(still >= n, "a loaded vehicle due at t={at} lost {n} of its load");
    }
    for &(at, n) in &flying.returning {
        let still = grown.returning.iter().find(|w| w.0 == at).map(|w| w.1).unwrap_or(0);
        assert!(still >= n, "a vehicle due home at t={at} stopped being due home");
    }
    let leg = node(&plain.base, "Rail").duration;
    for &(at, _) in &grown.working {
        assert!(
            at >= cut + leg || flying.working.iter().any(|w| w.0 == at),
            "a vehicle arrives at t={at}, which is sooner than one could leave at t={cut}"
        );
    }
}

/// Scaling a line down while it is loaded costs something, and the game says
/// what.
#[test]
fn taking_machines_out_of_service_is_reported() {
    let plain = gears();
    let cut = 6_000;
    let mut smaller = plain.clone();
    let mut smelter = node(&plain.base, "Smelter").clone();
    smelter.count = 1;
    smaller.commands.push(Command { at: cut, edit: Edit::Retune(smelter) });

    let before = live::carry_at(&plain, cut).unwrap().classes["Smelter"].clone();
    assert!(before.total() > 1, "the test plant no longer has smelters to take away");
    let scrapped = live::with_state(&smaller, cut, |a| a.scrapped.to_vec()).unwrap();
    let after = live::carry_at(&smaller, cut).unwrap().classes["Smelter"].clone();
    assert_eq!(after.total(), 1);
    if before.done + before.working_total() > 0 {
        assert!(
            scrapped.iter().any(|s| s.what == "Smelter"),
            "smelters were taken out of service mid-cycle and nobody was told"
        );
    }
}

/// Demolishing a bay is allowed to destroy what is in it, and is required to
/// say so.
#[test]
fn demolishing_a_full_bay_is_reported() {
    let plain = gears();
    let cut = 30_000;
    let held: Qty = live::carry_at(&plain, cut)
        .unwrap()
        .qty
        .iter()
        .filter(|((bay, _), _)| bay == "GearBay")
        .map(|(_, q)| *q)
        .sum();
    assert!(held > 0, "the test plant has nothing in GearBay at t={cut}");

    // The whole gear works comes down at once. Three removals at one tick are
    // one plant, so the half-demolished states in between are never compiled
    // and never have to be legal.
    let mut razed = plain.clone();
    for name in ["Delivery", "GearBay", "GearPress"] {
        razed.commands.push(Command { at: cut, edit: Edit::Remove(name.into()) });
    }
    let scrapped = live::with_state(&razed, cut + 1, |a| a.scrapped.to_vec()).unwrap();
    assert!(
        scrapped.iter().any(|s| s.what == "GearBay" && s.detail.contains("scrapped")),
        "demolishing a full bay reported {scrapped:?}"
    );
    let after = live::carry_at(&razed, cut + 1).unwrap();
    assert!(!after.qty.keys().any(|(bay, _)| bay == "GearBay"), "a demolished bay still holds stock");
    // And the plant that is left still runs: plates simply pile up now.
    assert!(
        after.qty[&("PlateBay".to_string(), "IronPlate".to_string())] > 0,
        "the surviving plant stopped"
    );
}

/// An edit can produce a plant that does not compile -- and when it does, the
/// log has to say which command did it, because "line 0 of a file nobody
/// wrote" is not an answer a player can act on.
///
/// Taking out a bay that a machine deposits into is the ordinary way to hit
/// this: the v3 rule that a machine's products need somewhere to go does not
/// stop applying because the plant is running.
#[test]
fn an_edit_that_breaks_the_plant_says_which_edit() {
    let mut razed = gears();
    razed.commands.push(Command { at: 30_000, edit: Edit::Remove("PlateBay".into()) });
    let fault = live::carry_at(&razed, 30_001).expect_err("a smelter with no output bay compiled");
    assert_eq!(fault.at, Some(30_000), "the fault blamed the wrong tick: {fault:?}");
    assert!(fault.msg.contains("Smelter"), "the fault did not name the machine: {}", fault.msg);
    // And the plant is unharmed at every tick before the edit.
    assert!(live::carry_at(&razed, 29_999).is_ok());
}

/// A machine placed mid-run starts idle and asking, exactly as one placed at
/// t=0 does -- and the bay it is wired to keeps everything it had.
#[test]
fn a_machine_placed_mid_run_starts_asking() {
    let plain = gears();
    let cut = 25_000;
    let mut built = plain.clone();
    let mut extra = node(&plain.base, "Smelter").clone();
    extra.name = "Smelter2".into();
    extra.count = 2;
    built.commands.push(Command { at: cut, edit: Edit::Place(extra) });
    built.commands.push(Command {
        at: cut,
        edit: Edit::Wire { from: "OreYard".into(), to: "Smelter2".into(), item: None },
    });
    built.commands.push(Command {
        at: cut,
        edit: Edit::Wire { from: "Smelter2".into(), to: "PlateBay".into(), item: None },
    });

    let before = live::carry_at(&plain, cut).unwrap();
    let after = live::carry_at(&built, cut).unwrap();
    assert_eq!(after.classes["Smelter2"].total(), 2, "the new smelters are not there");
    // Idle and asking, at the tick they were placed: the only bay that changes
    // is the one they withdraw from, and it changes by exactly the two batches
    // they took. Nothing else in the plant notices yet.
    let batch = node(&plain.base, "Smelter").inputs[0].qty;
    for (key, q) in &before.qty {
        let now = after.qty.get(key).copied().unwrap_or(0);
        if key.0 == "OreYard" {
            assert_eq!(now, q - 2 * batch, "the new smelters did not take one batch each");
        } else {
            assert_eq!(now, *q, "placing a machine disturbed {}", key.0);
        }
    }
    // Three commands at one tick are one recompile, not three.
    assert_eq!(built.boundaries(cut), vec![0, cut]);
    assert!(delivered(&built, 200_000, "Gear") >= delivered(&plain, 200_000, "Gear"));
}

/// The log is the document. Whatever a client believes the plant looks like
/// after a series of edits, the log has to agree, or two clients holding the
/// same log are looking at different factories.
#[test]
fn the_log_reconstructs_the_document() {
    let mut log = gears();
    let mut press = node(&log.base, "GearPress").clone();
    press.count = 3;
    let mut extra = node(&log.base, "Smelter").clone();
    extra.name = "Smelter2".into();

    log.commands.push(Command { at: 10, edit: Edit::Retune(press.clone()) });
    log.commands.push(Command { at: 20, edit: Edit::Place(extra.clone()) });
    log.commands.push(Command {
        at: 20,
        edit: Edit::Wire { from: "OreYard".into(), to: "Smelter2".into(), item: None },
    });
    log.commands.push(Command { at: 30, edit: Edit::Remove("Smelter2".into()) });

    let g = log.graph_at(1_000).unwrap();
    assert_eq!(node(&g, "GearPress").count, 3);
    assert!(g.node("Smelter2").is_none(), "a removed node came back");
    assert!(!g.edges.iter().any(|e| e.to == "Smelter2"), "a removed node kept its wires");
    // And at a tick before the edits, none of it has happened.
    assert_eq!(log.graph_at(5).unwrap(), log.base);
}

/// Structural refusals are the same refusals on every machine that replays the
/// log, which is the only reason it is safe for a client to send one.
#[test]
fn illegal_edits_are_refused_the_same_way_everywhere() {
    let log = gears();
    let g = log.base.clone();
    let bad: Vec<(Edit, &str)> = vec![
        (Edit::Wire { from: "OreYard".into(), to: "PlateBay".into(), item: None }, "two storages"),
        (Edit::Wire { from: "Smelter".into(), to: "GearPress".into(), item: None }, "two machines"),
        (Edit::Wire { from: "OreYard".into(), to: "Nobody".into(), item: None }, "no `Nobody`"),
        (Edit::Remove("Nobody".into()), "no `Nobody` to remove"),
        (Edit::Place(node(&g, "Smelter").clone()), "already here"),
        (Edit::Unwire { from: "OreYard".into(), to: "PlateBay".into() }, "not wired"),
    ];
    for (edit, wanted) in bad {
        let mut doc = g.clone();
        let err = edit.apply(&mut doc).expect_err(&format!("{edit:?} was allowed"));
        assert!(err.contains(wanted), "refused `{edit:?}` with `{err}`, wanted `{wanted}`");
        assert_eq!(doc, g, "a refused edit changed the document anyway");
    }

    // And a retune may not change what a building is.
    let mut bay = node(&g, "Smelter").clone();
    bay.kind = Kind::Storage;
    let err = Edit::Retune(bay).apply(&mut g.clone()).unwrap_err();
    assert!(err.contains("different building"), "{err}");
}

/// A log whose plant does not compile says which command broke it, not merely
/// that something did.
#[test]
fn a_broken_edit_names_its_own_tick() {
    let mut log = gears();
    let mut broken = node(&log.base, "Smelter").clone();
    broken.inputs.clear();
    broken.outputs.clear();
    log.commands.push(Command { at: 4_000, edit: Edit::Retune(broken) });
    let fault = live::carry_at(&log, 5_000).expect_err("a machine with no recipe compiled");
    assert_eq!(fault.at, Some(4_000), "the fault blamed the wrong tick: {fault:?}");
}

/// Everything above compares carries. This checks the carry against the thing
/// it claims to be a view of: the snapshot the workbench draws.
#[test]
fn the_carry_and_the_snapshot_agree() {
    for path in CONFIGS {
        let log = log_of(path);
        for &t in &[137u64, 6_000] {
            let (held, pops) = live::with_state(&log, t, |a| {
                let j = snap::render(a.prog, a.bp, a.plan, a.room, t);
                let mut held: HashMap<(String, String), Qty> = HashMap::new();
                for s in j.at("storages").as_arr() {
                    for h in s.at("held").as_arr() {
                        let q = h.at("qty").as_u64().unwrap();
                        if q > 0 {
                            held.insert(
                                (
                                    s.at("name").as_str().unwrap().to_string(),
                                    h.at("item").as_str().unwrap().to_string(),
                                ),
                                q,
                            );
                        }
                    }
                }
                let pops: HashMap<String, u64> = j
                    .at("classes")
                    .as_arr()
                    .iter()
                    .map(|c| {
                        (c.at("name").as_str().unwrap().to_string(), c.at("count").as_u64().unwrap())
                    })
                    .collect();
                (held, pops)
            })
            .unwrap();
            let carry = live::carry_at(&log, t).unwrap();
            assert_eq!(carry.qty, held, "{path}: the carry and the snapshot disagree at t={t}");
            for (name, p) in &carry.classes {
                assert_eq!(
                    p.total(),
                    pops[name],
                    "{path}: `{name}` has a population the snapshot does not report"
                );
            }
        }
    }
}

// ================================ a plant halfway through being built

/// Placing a machine and not yet wiring it up is not an error in the
/// *document*. It is what a factory looks like while somebody is building one,
/// and the thing they are going to wire up next has to be on the canvas.
///
/// This is a regression test for a real bug. Prototype 0's browser applied its
/// own edits, so a half-built plant drew fine next to a red error. Prototype 1
/// made the server authoritative -- and placing a processor stopped working
/// entirely, because the plant did not compile, so no document came back, so
/// nothing appeared and there was nothing to wire.
#[test]
fn a_plant_that_does_not_compile_still_has_a_document() {
    let mut log = gears();
    let mut extra = node(&log.base, "Smelter").clone();
    extra.name = "Smelter2".into();
    log.commands.push(Command { at: 0, edit: Edit::Place(extra) });

    // The document folds perfectly well; it is the *plant* that does not run.
    let doc = log.graph_at(0).expect("an unwired machine broke the document");
    assert!(doc.node("Smelter2").is_some());

    let fault = live::carry_at(&log, 100).expect_err("an unwired smelter compiled");
    assert!(!fault.refused, "an unfinished plant is not a refused command: {fault:?}");
    let carried = fault.graph.as_ref().expect("the fault carried no document to draw");
    assert!(
        carried.node("Smelter2").is_some(),
        "the machine that was just placed is not in the document the view is given"
    );
    assert_eq!(carried.nodes.len(), log.base.nodes.len() + 1);

    // And wiring it up finishes the job.
    log.commands.push(Command {
        at: 0,
        edit: Edit::Wire { from: "OreYard".into(), to: "Smelter2".into(), item: None },
    });
    log.commands.push(Command {
        at: 0,
        edit: Edit::Wire { from: "Smelter2".into(), to: "PlateBay".into(), item: None },
    });
    let ok = live::carry_at(&log, 4_000).expect("the wired-up plant still would not run");
    assert!(ok.classes.contains_key("Smelter2"));
}

/// And the other kind of failure is still the other kind: a command that can
/// never work says so, and hands back the document as it stood before it.
#[test]
fn a_command_that_can_never_work_is_marked_refused() {
    let mut log = gears();
    log.commands.push(Command {
        at: 0,
        edit: Edit::Wire { from: "OreBay".into(), to: "PlateBay".into(), item: None },
    });
    let fault = live::carry_at(&log, 100).expect_err("two bays were wired together");
    assert!(fault.refused, "a structurally impossible wire was not marked refused");
    let carried = fault.graph.as_ref().expect("the fault carried no document");
    assert_eq!(carried, &log.base, "a refused command changed the document anyway");
}
