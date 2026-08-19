//! The v3 claim: a plant split into regions that advance on their own clocks,
//! exchanging nothing but timestamped events, is the *same function* as the
//! monolithic solver -- not an approximation of it, and not merely equal in
//! throughput.
//!
//! These tests compare the two at the level of the whole state, byte for byte,
//! including things nobody would think to check by hand: the live round-robin
//! pointers, how many vehicles are halfway home, and which tick each of them
//! arrives.

use temporal_rooms::domains;
use temporal_rooms::dsl;
use temporal_rooms::model::*;
use temporal_rooms::pop;
use temporal_rooms::rooms::{self, Room};
use temporal_rooms::sim::{CountersBig, World};

const CONFIGS: &[&str] = &[
    "configs/01-spec.factory",
    "configs/04-science.factory",
    "configs/05-coupled.factory",
    "configs/06-cycle.factory",
    "configs/07-transport.factory",
    "configs/08-policy.factory",
    "configs/11-railchain.factory",
    "configs/12-tradeloop.factory",
    "configs/14-privatebay.factory",
];

fn load(path: &str) -> Program {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    dsl::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn deployed(prog: &Program) -> &Blueprint {
    &prog.blueprints[prog.deploys[0].blueprint as usize]
}

fn probes() -> Vec<Tick> {
    let mut v: Vec<Tick> = (1..200).collect();
    v.extend((200..12_000).step_by(53));
    v.extend([20_000, 33_333, 60_000]);
    v
}

// ================================================== the primary v3 proof

/// For every configuration, at every affordable tick, the decomposed plant and
/// the monolithic one must be in *the same state* -- not just agree on totals.
#[test]
fn decomposed_state_equals_monolithic_state() {
    for path in CONFIGS {
        let prog = load(path);
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        let plan = rooms::plan(bp);

        let mut room = Room::new(&plan, n_items);
        let mut mono = pop::Pop::new(bp, n_items);
        for t in probes() {
            room.run_until(t);
            mono.run_until(t);
            assert_eq!(
                room.counters(),
                mono.c,
                "{path}: counters diverged at t={t} with {} regions",
                plan.regions()
            );
            assert_eq!(
                room.signature(bp),
                mono.signature(),
                "{path}: state diverged at t={t} with {} regions",
                plan.regions()
            );
        }
    }
}

/// The same, but starting the decomposition afresh at each tick, so a Room
/// that only happens to agree because it is being stepped in lockstep with the
/// thing it is checked against cannot hide.
#[test]
fn a_fresh_decomposition_lands_in_the_same_place() {
    for path in CONFIGS {
        let prog = load(path);
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        let plan = rooms::plan(bp);
        for t in [1u64, 7, 60, 137, 900, 1_500, 3_000, 7_777, 20_000] {
            let mut room = Room::new(&plan, n_items);
            room.run_until(t);
            let mut mono = pop::Pop::new(bp, n_items);
            mono.run_until(t);
            assert_eq!(room.signature(bp), mono.signature(), "{path}: fresh room at t={t}");
        }
    }
}

/// And the decomposition must agree with the simulator that walks every
/// machine individually, which is the only thing here that is nobody's claim
/// about anything.
#[test]
fn decomposed_agrees_with_machine_by_machine() {
    for path in CONFIGS {
        let prog = load(path);
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        let plan = rooms::plan(bp);
        for t in [1u64, 40, 137, 601, 1_500, 4_000, 12_000] {
            let mut room = Room::new(&plan, n_items);
            room.run_until(t);
            let mut w = World::new(bp, n_items, 1, 0);
            w.run_until(t);
            assert_eq!(
                CountersBig::from_narrow(&room.counters()),
                CountersBig::from_narrow(&w.c),
                "{path}: decomposed vs T1 at t={t}"
            );
        }
    }
}

// ================================================== the causal argument

/// The regions really do sit at different ticks. If they never drifted, the
/// decomposition would be a lockstep tick loop wearing a hat.
#[test]
fn regions_really_do_drift_apart() {
    let prog = load("configs/11-railchain.factory");
    let bp = deployed(&prog);
    let plan = rooms::plan(bp);
    assert_eq!(plan.regions(), 3, "mine, smelting and works");

    let mut room = Room::new(&plan, prog.items.len());
    room.run_until(40_000);
    assert!(room.max_skew >= 900, "clocks barely drifted: {}", room.max_skew);
    assert!(
        room.max_advance >= 900,
        "no region ever got to run alone for a whole trip: {}",
        room.max_advance
    );
    assert!(
        room.mean_advance() > 100.0,
        "regions are synchronising far more often than the physics requires: {}",
        room.mean_advance()
    );
}

/// A region may never be handed an event it should already have processed.
/// This is the property that makes the decomposition a distributed system
/// rather than a data structure, so it is asserted inside `Pop::deliver` and
/// exercised here on every configuration that has a channel at all.
#[test]
fn every_message_lands_in_the_receivers_future() {
    for path in CONFIGS {
        let prog = load(path);
        let bp = deployed(&prog);
        let plan = rooms::plan(bp);
        if plan.graph.channels.is_empty() {
            continue;
        }
        let mut room = Room::new(&plan, prog.items.len());
        room.run_until(30_000); // panics inside `deliver` if one ever does not
        assert!(room.messages > 0, "{path}: a plant with channels sent nothing");
    }
}

/// Slack is a property of both directions. Material going one way buys the
/// receiver its window; vehicles coming back buy the sender one.
#[test]
fn slack_is_bidirectional() {
    let prog = load("configs/11-railchain.factory");
    let bp = deployed(&prog);
    let g = domains::regions(bp);

    let mine = &g.regions[0];
    assert!(mine.inbound.is_empty(), "the mine receives nothing");
    assert_eq!(
        mine.slack(&g.channels),
        Some(1_400),
        "its slack is the trip home, not infinity"
    );

    // v2 asked only about arrivals and therefore called this region
    // independent forever.
    let v2 = domains::analyse(bp);
    let sending: Vec<&domains::Domain> =
        v2.transit.iter().filter(|d| d.inbound.is_empty()).collect();
    assert_eq!(sending[0].independent_for(), None, "which is what v2 concluded");
}

/// A link whose vehicle teleports home is a zero-cost channel running
/// backwards through the transport. Where that closes a loop, the regions on
/// it can never differ by a tick and are fused rather than scheduled.
#[test]
fn a_teleporting_vehicle_pins_its_two_ends_together() {
    let prog = load("configs/12-tradeloop.factory");
    let bp = deployed(&prog);
    assert_eq!(domains::regions(bp).regions.len(), 2, "with trips home: two regions");

    let mut flat = bp.clone();
    for a in &mut flat.actors {
        a.return_latency = 0;
    }
    let g = domains::regions(&flat);
    assert_eq!(g.regions.len(), 1, "without them: one, because the loop costs nothing");
    assert_eq!(g.fused, 1, "and the fusion is reported rather than silent");

    // Config 7's link also teleports, but its region graph has no cycle, so
    // nothing needs fusing -- the sending side simply gets no slack.
    let seven = load("configs/07-transport.factory");
    let g7 = domains::regions(deployed(&seven));
    assert_eq!(g7.regions.len(), 2);
    assert_eq!(g7.fused, 0);
    assert_eq!(g7.min_slack(), Some(0), "the mine can never lead the yard");
}

/// The regions partition the plant: every storage and every non-lifted class
/// belongs to exactly one, and a lifted transport belongs to none.
#[test]
fn regions_partition_the_plant() {
    for path in CONFIGS {
        let prog = load(path);
        let bp = deployed(&prog);
        let plan = rooms::plan(bp);
        let g = &plan.graph;

        let mut seen_store = vec![0usize; bp.storages.len()];
        let mut seen_class = vec![0usize; bp.actors.len()];
        for reg in &g.regions {
            for &s in &reg.storages {
                seen_store[s as usize] += 1;
            }
            for &c in &reg.classes {
                seen_class[c as usize] += 1;
            }
        }
        assert!(seen_store.iter().all(|&n| n == 1), "{path}: a storage is in two regions");
        for (c, &n) in seen_class.iter().enumerate() {
            let lifted = g.channels.iter().any(|ch| ch.class == c as u16);
            assert_eq!(n, if lifted { 0 } else { 1 }, "{path}: class {c} placed {n} times");
        }
        // A region's own blueprint has to be self-contained: every storage a
        // class names is one this region owns.
        for (r, rbp) in plan.bps.iter().enumerate() {
            for a in &rbp.actors {
                for &s in a.in_stores.iter().chain(a.out_stores.iter()) {
                    assert!(
                        (s as usize) < rbp.storages.len(),
                        "{path}: region {r} reaches outside itself"
                    );
                }
            }
        }
    }
}

// ================================================== transport as physics

/// Throughput is derived from the vehicles, the batch and the round trip. It
/// is not a second number that can disagree with the first three.
#[test]
fn throughput_is_vehicles_times_batch_over_the_round_trip() {
    let prog = load("configs/11-railchain.factory");
    let bp = deployed(&prog);
    let ore = bp.actors.iter().find(|a| a.name == "OreTrain").unwrap();
    assert_eq!(ore.duration, 1_400, "200 + 2400/2");
    assert_eq!(ore.return_latency, 1_400, "a distance is symmetric");
    assert_eq!(ore.cycle(), 2_800);
    let (num, den) = ore.throughput();
    assert_eq!((num, den), (8 * 6_000, 2_800));
}

/// The trip home is real time in the ground-truth simulator too, not just in
/// the lumped one -- otherwise the equivalence would be between two identical
/// mistakes.
#[test]
fn the_trip_home_costs_time_in_both_engines() {
    let prog = load("configs/11-railchain.factory");
    let bp = deployed(&prog);
    let n_items = prog.items.len();
    for t in [500u64, 1_401, 2_801, 5_000, 20_000] {
        let mut w = World::new(bp, n_items, 1, 0);
        w.run_until(t);
        let mut p = pop::Pop::new(bp, n_items);
        p.run_until(t);
        assert_eq!(
            CountersBig::from_narrow(&w.c),
            CountersBig::from_narrow(&p.c),
            "T1 and T5 disagree about the return leg at t={t}"
        );
    }
    // A train cannot complete more than one round trip per 2800 ticks.
    let ore = bp.actors.iter().position(|a| a.name == "OreTrain").unwrap();
    let mut p = pop::Pop::new(bp, n_items);
    p.run_until(28_000);
    assert!(
        p.c.cycles[ore] <= 8 * (28_000 / 2_800),
        "vehicles ran more trips than the round trip allows"
    );
}

/// Material reaches a machine through the logistics graph. Reaching into every
/// bay that happens to hold what you want is no longer expressible, which is
/// what retires v2's one unproven assumption.
#[test]
fn a_machine_may_not_draw_one_item_from_two_bays() {
    let src = "
        item IronOre
        blueprint Ambiguous {
            source  MinerA { produces 100 IronOre every 10 ticks }
            source  MinerB { produces 100 IronOre every 10 ticks }
            storage BayA { capacity 1000 }
            storage BayB { capacity 1000 }
            sink    Furnace { consumes 10 IronOre every 5 ticks }
            wire MinerA -> BayA -> Furnace
            wire MinerB -> BayB -> Furnace
        }
        deploy 1 x Ambiguous
    ";
    let err = dsl::parse(src).expect_err("two ore bays feeding one furnace should be refused");
    assert!(err.msg.contains("BayA") && err.msg.contains("BayB"), "unhelpful: {err}");

    // The same plant is fine once the two bays meet through a link, because
    // then the order of arrival is physics rather than array order.
    let routed = "
        item IronOre
        blueprint Routed {
            source  MinerA { produces 100 IronOre every 10 ticks }
            source  MinerB { produces 100 IronOre every 10 ticks }
            storage BayA { capacity 1000 }
            storage BayB { capacity 1000 policy round_robin }
            link    Feed { moves 50 IronOre takes 20 ticks returns 20 ticks }
            sink    Furnace { consumes 10 IronOre every 5 ticks }
            wire MinerA -> BayA -> Feed -> BayB
            wire MinerB -> BayB -> Furnace
        }
        deploy 1 x Routed
    ";
    dsl::parse(routed).expect("routing the two bays together is the way to say it");
}

// ================================================== deployments that share

/// The higher-level lumping: a deployment of lines that share *all* of their
/// storage is one population, and the claim is checked against the same plant
/// written out line by line.
#[test]
fn a_fully_shared_deployment_is_one_population() {
    let prog = load("configs/13-orefield.factory");
    let org = prog.deploys[0].origin.expect("this deployment shares infrastructure");
    assert!(org.collapsed, "everything is shared, so it should have collapsed");
    assert_eq!(org.lines, 250_000_000);

    let one = &prog.blueprints[org.blueprint as usize];
    let n_items = prog.items.len();
    for n in [1u64, 2, 4, 7, 16] {
        let wide = one.spread(n);
        let tall = one.collapse(n);
        for t in [1u64, 20, 41, 137, 400, 1_111, 4_000] {
            let mut a = pop::Pop::new(&wide, n_items);
            a.run_until(t);
            let mut b = pop::Pop::new(&tall, n_items);
            b.run_until(t);
            assert_eq!(a.c.produced, b.c.produced, "{n} lines at t={t}");
            assert_eq!(a.c.consumed, b.c.consumed, "{n} lines at t={t}");
            let mut k = 0usize;
            for (i, orig) in one.actors.iter().enumerate() {
                let reps = if orig.shared { 1 } else { n as usize };
                let sum: u64 = (0..reps).map(|j| a.c.cycles[k + j]).sum();
                assert_eq!(sum, b.c.cycles[i], "class {i} at {n} lines, t={t}");
                k += reps;
            }
        }
        // And the wide form itself is checked against machine-by-machine, so
        // the ground truth of the comparison is not another claim.
        let mut w = World::new(&wide, n_items, 1, 0);
        w.run_until(2_000);
        let mut a = pop::Pop::new(&wide, n_items);
        a.run_until(2_000);
        assert_eq!(a.c, w.c, "the wide form itself is wrong at {n} lines");
    }
}

/// And a line that keeps a bay of its own cannot be lumped away, because that
/// bay is exactly the state that tells one line from another.
#[test]
fn a_private_bay_blocks_the_collapse() {
    let prog = load("configs/14-privatebay.factory");
    let org = prog.deploys[0].origin.expect("this deployment shares infrastructure");
    assert!(!org.collapsed, "a private bay must stop the collapse");
    let bp = deployed(&prog);
    assert_eq!(bp.storages.iter().filter(|s| !s.shared).count(), 16, "one bay per line");

    // Collapsing anyway must be visibly wrong, or the guard is guarding
    // nothing.
    let one = &prog.blueprints[org.blueprint as usize];
    let n_items = prog.items.len();
    let wide = one.spread(4);
    let tall = one.collapse(4);
    let mut a = pop::Pop::new(&wide, n_items);
    a.run_until(20_000);
    let mut b = pop::Pop::new(&tall, n_items);
    b.run_until(20_000);
    assert_ne!(
        a.c.produced, b.c.produced,
        "if these agreed, the private bay would not matter and the guard would be pointless"
    );

    // Too many lines to write out and not collapsible: say so rather than
    // quietly producing the wrong answer.
    let src = std::fs::read_to_string("configs/14-privatebay.factory").unwrap();
    let big = src.replace("deploy 16 x", "deploy 4096 x");
    let err = dsl::parse(&big).expect_err("4096 private lines should be refused");
    assert!(err.msg.contains("PlateBay"), "unhelpful: {err}");
}

/// A shared machine has no private line to live in, so it may not be wired to
/// private storage.
#[test]
fn a_shared_machine_needs_shared_storage() {
    let src = "
        item IronOre
        blueprint Bad {
            shared source Field x2 { produces 100 IronOre every 10 ticks }
            storage Bay { capacity 1000 }
            sink    Furnace { consumes 10 IronOre every 5 ticks }
            wire Field -> Bay -> Furnace
        }
        deploy 4 x Bad
    ";
    let err = dsl::parse(src).expect_err("shared machine on a private bay");
    assert!(err.msg.contains("Field") && err.msg.contains("Bay"), "unhelpful: {err}");
}

// ================================================== execution modes

/// A region that hears from nobody is solved by its closed form, and one that
/// does not is stepped. Both are exact; the ladder is about price.
#[test]
fn a_region_that_hears_from_nobody_uses_its_closed_form() {
    let alone = load("configs/09-population.factory");
    let plan = rooms::plan(deployed(&alone));
    let room = Room::new(&plan, alone.items.len());
    assert_eq!(plan.regions(), 1);
    assert_eq!(room.modes[0], rooms::Mode::Closed);

    let split = load("configs/11-railchain.factory");
    let plan = rooms::plan(deployed(&split));
    let room = Room::new(&plan, split.items.len());
    assert!(
        room.modes.iter().all(|&m| m == rooms::Mode::Population),
        "every region here has a neighbour to listen to"
    );
}
