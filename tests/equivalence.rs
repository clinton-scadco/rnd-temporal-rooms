//! The load-bearing claim of v1 was that the closed form and the event
//! simulator are the same function computed two ways. v2 adds a second, larger
//! claim: that the *lumped population* solver is also that same function, even
//! though it never represents an individual machine.
//!
//! These tests try hard to break both.

use temporal_rooms::analytic::{self, Rat};
use temporal_rooms::domains;
use temporal_rooms::dsl;
use temporal_rooms::model::*;
use temporal_rooms::pop;
use temporal_rooms::sim::{CountersBig, World};

const CONFIGS: &[&str] = &[
    "configs/01-spec.factory",
    "configs/02-balanced.factory",
    "configs/03-megafactory.factory",
    "configs/04-science.factory",
    "configs/05-coupled.factory",
    "configs/06-cycle.factory",
    "configs/07-transport.factory",
    "configs/08-policy.factory",
    "configs/09-population.factory",
    "configs/11-railchain.factory",
    "configs/12-tradeloop.factory",
    "configs/14-privatebay.factory",
];

fn load(path: &str) -> Program {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    dsl::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn programs() -> Vec<(&'static str, Program)> {
    CONFIGS.iter().map(|&p| (p, load(p))).collect()
}

/// The blueprint a program actually deploys. v3 rewrites the deployment axis
/// when lines share infrastructure, so this is no longer always the first one.
fn deployed(prog: &Program) -> &Blueprint {
    &prog.blueprints[prog.deploys[0].blueprint as usize]
}

/// Dense sweep: every tick up to 400, then coarse and absurd ones.
fn probe_ticks() -> Vec<Tick> {
    let mut v: Vec<Tick> = (0..400).collect();
    v.extend((400..20_000).step_by(97));
    v.extend([50_000, 123_457, 1_000_000, 999_999_999, 1_000_000_000_000, u64::MAX / 4]);
    v
}

// ===================================================== the v2 claim

/// The whole point. For every configuration, at every affordable tick, the
/// population solver must produce the same counters as walking every machine.
#[test]
fn population_form_matches_machine_by_machine() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        for t in (0..4_000).step_by(13) {
            let mut w = World::new(bp, n_items, 1, 0);
            w.run_until(t);
            let mut p = pop::Pop::new(bp, n_items);
            p.run_until(t);
            assert_eq!(
                CountersBig::from_narrow(&w.c),
                CountersBig::from_narrow(&p.c),
                "{path}: T1 and T5 disagree at t={t}"
            );
        }
    }
}

/// The population histogram is not merely counter-compatible: at every instant
/// it must be the exact multiset of machine states the simulator holds.
#[test]
fn population_histogram_is_the_real_state() {
    use temporal_rooms::sim::{S_DONE, S_STARVED, S_WORKING};
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        if bp.machines > 50_000 {
            continue;
        }
        let n_items = prog.items.len();
        for t in (0..2_000).step_by(29) {
            let mut w = World::new(bp, n_items, 1, 0);
            w.run_until(t);
            let mut p = pop::Pop::new(bp, n_items);
            p.run_until(t);

            for (ci, ad) in bp.actors.iter().enumerate() {
                let h = w.class_histogram(0, ci);
                let cp = &p.classes[ci];
                assert_eq!(
                    h[S_STARVED as usize], cp.starved,
                    "{path}: {} starved count differs at t={t}", ad.name
                );
                assert_eq!(
                    h[S_DONE as usize], cp.done,
                    "{path}: {} blocked count differs at t={t}", ad.name
                );
                assert_eq!(
                    h[S_WORKING as usize],
                    cp.working_total(),
                    "{path}: {} working count differs at t={t}", ad.name
                );
            }

            for (s, sd) in bp.storages.iter().enumerate() {
                for &it in &sd.slots {
                    assert_eq!(
                        w.storage_qty(0, s, it),
                        p.storage_qty(s, it),
                        "{path}: {} holds a different amount of {} at t={t}",
                        sd.name,
                        prog.items[it as usize]
                    );
                }
            }
        }
    }
}

/// Compression has to be real, not incidental. The number of occupied cells
/// must not grow with the population -- that is the entire thesis.
#[test]
fn compression_is_independent_of_population() {
    let small = load("configs/09-population.factory");
    let big = load("configs/10-billion.factory");
    let (a, b) = (&small.blueprints[0], &big.blueprints[0]);

    let fa = pop::orbit(a, small.items.len(), 20_000_000);
    let fb = pop::orbit(b, big.items.len(), 20_000_000);
    assert!(fa.found && fb.found, "both population orbits must be found");

    assert!(
        fb.population / fa.population > 50_000,
        "the big configuration should be ~100,000x the machines"
    );
    assert_eq!(
        fa.max_distinct_states, fb.max_distinct_states,
        "cell count must not depend on how many machines each cell stands for"
    );
    assert_eq!(fa.period, fb.period, "the same plant should have the same orbit");
}

/// Scaling every population by k must scale every extensive counter by exactly
/// k. If the lumping were approximate this is where it would show.
#[test]
fn scaling_the_population_scales_the_answer() {
    let small = load("configs/09-population.factory");
    let big = load("configs/10-billion.factory");
    let k = 100_000u128;
    let (a, b) = (&small.blueprints[0], &big.blueprints[0]);

    for t in [0, 1, 7, 137, 1_000, 4_321, 100_000] {
        let mut pa = pop::Pop::new(a, small.items.len());
        pa.run_until(t);
        let mut pb = pop::Pop::new(b, big.items.len());
        pb.run_until(t);
        for i in 0..small.items.len() {
            assert_eq!(
                pa.c.produced[i] as u128 * k,
                pb.c.produced[i] as u128,
                "produced {} does not scale at t={t}",
                small.items[i]
            );
        }
    }
}

// ================================================ orbits and closed forms

#[test]
fn closed_form_matches_event_sim_everywhere() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        let cf = pop::orbit(bp, n_items, 20_000_000);
        assert!(cf.found, "{path}: no orbit found");

        for t in probe_ticks() {
            // Simulating to u64::MAX/4 is the thing we are avoiding, so only
            // cross-check by simulation where simulation is affordable.
            if t > 20_000 {
                continue;
            }
            let ana = cf.eval(bp, n_items, t);
            let mut w = World::new(bp, n_items, 1, 0);
            w.run_until(t);
            assert_eq!(
                CountersBig::from_narrow(&w.c),
                ana,
                "{path}: closed form disagrees at t={t}"
            );
        }
    }
}

/// Beyond the horizon we can afford to simulate, the closed form must still be
/// self-consistent: advancing by a whole orbit adds exactly `delta`.
#[test]
fn closed_form_is_periodic_at_astronomical_ticks() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        let cf = pop::orbit(bp, n_items, 20_000_000);
        if cf.frozen {
            continue;
        }
        let t = 1_000_000_000_000_000_000u64;
        let a = cf.eval(bp, n_items, t);
        let b = cf.eval(bp, n_items, t + cf.period);
        for i in 0..n_items {
            assert_eq!(
                b.produced[i] - a.produced[i],
                cf.delta.produced[i] as u128,
                "{path}: {} is not periodic at t={t}",
                prog.items[i]
            );
        }
    }
}

/// T2 walks machines, T5 walks histograms. Where both can run they must find
/// the same orbit and the same steady rates.
#[test]
fn machine_orbit_and_population_orbit_agree() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        if bp.machines > 50_000 {
            continue;
        }
        let n_items = prog.items.len();
        let t2 = analytic::orbit(bp, n_items, 20_000_000);
        let t5 = pop::orbit(bp, n_items, 20_000_000);
        assert!(t2.found && t5.found, "{path}: both tiers must find an orbit");
        assert_eq!(t2.frozen, t5.frozen, "{path}: disagree on whether the plant freezes");
        if t2.frozen {
            continue;
        }
        assert_eq!(t2.period, t5.period, "{path}: different orbit periods");
        assert_eq!(t2.delta, t5.delta, "{path}: different per-orbit deltas");
    }
}

// ================================================ conservation and sanity

/// Mass balance: an item is either still in a storage, or it was consumed.
/// Items inside a working machine have been consumed and not yet produced, so
/// `inventory == initial + produced - consumed` holds at *every* instant.
#[test]
fn mass_is_conserved() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        if bp.machines > 50_000 {
            continue;
        }
        let seeded = |item: ItemId| -> u64 {
            bp.storages
                .iter()
                .flat_map(|s| s.initial.iter())
                .filter(|s| s.item == item)
                .map(|s| s.qty)
                .sum()
        };
        for t in (0..4_000).step_by(37) {
            let mut w = World::new(bp, n_items, 3, 7);
            w.run_until(t);
            for item in 0..n_items as ItemId {
                let inventory: u64 = (0..3)
                    .flat_map(|inst| (0..bp.storages.len()).map(move |s| (inst, s)))
                    .map(|(inst, s)| w.storage_qty(inst, s, item))
                    .sum();
                let net = seeded(item) * 3 + w.c.produced[item as usize]
                    - w.c.consumed[item as usize];
                assert_eq!(
                    inventory, net,
                    "{path} leaks {} at t={t}",
                    prog.items[item as usize]
                );
            }
        }
    }
}

/// No storage may ever exceed its declared capacity, in either engine.
#[test]
fn capacity_is_never_exceeded() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        for t in (0..3_000).step_by(23) {
            let mut p = pop::Pop::new(bp, n_items);
            p.run_until(t);
            for (s, sd) in bp.storages.iter().enumerate() {
                assert!(
                    p.storage_used(s) <= sd.capacity,
                    "{path}: {} over capacity at t={t}",
                    sd.name
                );
            }
        }
    }
}

/// Machines must never be created or destroyed: every class always accounts
/// for exactly its declared population.
#[test]
fn population_is_conserved() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        for t in (0..3_000).step_by(31) {
            let mut p = pop::Pop::new(bp, n_items);
            p.run_until(t);
            for (ci, ad) in bp.actors.iter().enumerate() {
                assert_eq!(
                    p.classes[ci].total(),
                    ad.count,
                    "{path}: {} lost machines at t={t}",
                    ad.name
                );
            }
        }
    }
}

// ======================================================= arbitration

/// Within a class, service must be even. A class is by definition the set of
/// machines the arbiter refuses to distinguish, so no member may pull ahead of
/// another by more than the one cycle integrality forces.
#[test]
fn classes_share_work_evenly() {
    for (path, prog) in programs() {
        let bp = deployed(&prog);
        let n_items = prog.items.len();
        if bp.machines > 50_000 {
            continue;
        }
        let mut w = World::new_tracked(bp, n_items, 1, 0);
        w.run_until(4_000);
        for ad in &bp.actors {
            if ad.count < 2 {
                continue;
            }
            let counts: Vec<u32> = (ad.machine_offset..ad.machine_offset + ad.count)
                .map(|m| w.member_cycles(0, m))
                .collect();
            let lo = *counts.iter().min().unwrap();
            let hi = *counts.iter().max().unwrap();
            assert!(
                hi - lo <= 1,
                "{path}: {} split work {lo}..{hi} -- members of one class must be interchangeable",
                ad.name
            );
        }
    }
}

/// Aggregate answers must not depend on which member of a class was picked --
/// that is what makes the lumping legitimate. Reversing the wiring order
/// permutes machine indices without changing the plant, and both engines must
/// be indifferent.
#[test]
fn aggregates_ignore_machine_identity() {
    let prog = load("configs/09-population.factory");
    let bp = deployed(&prog);
    let n_items = prog.items.len();

    let mut shuffled = bp.clone();
    for s in &mut shuffled.storages {
        s.takers.reverse();
        s.givers.reverse();
    }
    // Reversing a *round-robin* order is a relabelling of an unordered set, so
    // the totals cannot move. Under `index` it would be a different policy, and
    // this test would rightly fail.
    for t in [500, 1_000, 2_500] {
        let mut a = pop::Pop::new(bp, n_items);
        a.run_until(t);
        let mut b = pop::Pop::new(&shuffled, n_items);
        b.run_until(t);
        assert_eq!(
            a.c.produced, b.c.produced,
            "reordering interchangeable classes changed the answer at t={t}"
        );
    }
}

/// Under `index` the first-declared consumer is fed first and the last can
/// starve outright; under `round_robin` the same plant shares. Both must be
/// exact, and they must differ -- otherwise the policy is not doing anything.
#[test]
fn contention_policy_changes_the_outcome() {
    let prog = load("configs/08-policy.factory");
    let bp = deployed(&prog);
    let n_items = prog.items.len();

    let mut duties = Vec::new();
    for pol in [Policy::Index, Policy::RoundRobin] {
        let mut b = bp.clone();
        for s in &mut b.storages {
            s.policy = pol;
        }
        let f = pop::orbit(&b, n_items, 20_000_000);
        assert!(f.found && !f.frozen, "policy {:?} should still run", pol);

        // And the population form must still match the simulator under it.
        for t in [137, 900, 3_000] {
            let mut w = World::new(&b, n_items, 1, 0);
            w.run_until(t);
            assert_eq!(
                CountersBig::from_narrow(&w.c),
                f.eval(&b, n_items, t),
                "policy {:?}: T1 and T5 disagree at t={t}",
                pol
            );
        }
        duties.push(
            (0..b.actors.len())
                .map(|a| Rat::new(f.delta.cycles[a] as u128, f.period as u128))
                .collect::<Vec<_>>(),
        );
    }

    let shops: Vec<usize> = bp
        .actors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.name.starts_with("Shop"))
        .map(|(i, _)| i)
        .collect();
    assert_eq!(shops.len(), 3);

    // index: someone is starved out completely.
    assert!(
        shops.iter().any(|&i| duties[0][i].is_zero()),
        "under index one shop should never run at all"
    );
    // round_robin: nobody is, and they all get the same.
    assert!(
        shops.iter().all(|&i| !duties[1][i].is_zero()),
        "under round_robin every shop should run"
    );
    assert!(
        shops.windows(2).all(|w| duties[1][w[0]] == duties[1][w[1]]),
        "under round_robin identical shops should get identical shares"
    );
    // Throughput is conserved: the policy decides who, not how much.
    let total = |d: &Vec<Rat>| shops.iter().fold(Rat::zero(), |acc, &i| acc.add(d[i]));
    assert_eq!(total(&duties[0]), total(&duties[1]), "policy changed total throughput");
}

// ========================================================== structure

/// A cycle that nothing seeds is dead, and the rate algebra must say so before
/// anything is simulated.
#[test]
fn an_unseeded_cycle_is_reported_dead() {
    let src = std::fs::read_to_string("configs/06-cycle.factory").unwrap();
    let seeded = dsl::parse(&src).unwrap();
    let r = analytic::rates(&seeded.blueprints[0], seeded.items.len());
    assert!(r.unattainable.is_empty(), "the seeded loop should be alive");
    assert!(!r.terminal, "the seeded loop should be sustainable");

    let starved = dsl::parse(&src.replace("initial 40 Catalyst", "")).unwrap();
    let bp = &starved.blueprints[0];
    let r = analytic::rates(bp, starved.items.len());
    let cat = starved.items.iter().position(|i| i == "Catalyst").unwrap() as ItemId;
    assert!(
        r.unattainable.contains(&cat),
        "an unseeded catalyst loop must be reported unattainable"
    );

    // And the simulator must agree by simply never running the reactor.
    let mut p = pop::Pop::new(bp, starved.items.len());
    p.run_until(10_000);
    let reactor = bp.actors.iter().position(|a| a.name == "Reactor").unwrap();
    assert_eq!(p.c.cycles[reactor], 0, "an unseeded reactor must never cycle");
}

/// Cutting the transports must split the rail configuration in two, and each
/// side must be able to run alone for the length of a trip.
#[test]
fn transport_creates_causal_domains() {
    let prog = load("configs/07-transport.factory");
    let bp = deployed(&prog);
    let rep = domains::analyse(bp);
    assert_eq!(rep.hard.len(), 1, "the plant is connected");
    assert_eq!(rep.transit.len(), 2, "cutting the trains should split it in two");

    let receiving: Vec<&domains::Domain> =
        rep.transit.iter().filter(|d| !d.inbound.is_empty()).collect();
    assert_eq!(receiving.len(), 1, "exactly one side receives");
    assert_eq!(
        receiving[0].independent_for(),
        Some(3_000),
        "it should be advanceable alone for one trip"
    );
    let sending: Vec<&domains::Domain> =
        rep.transit.iter().filter(|d| d.inbound.is_empty()).collect();
    assert_eq!(sending[0].independent_for(), None, "the mine hears from nobody");
}

/// The plants with feedback should be recognised as having it, and the plants
/// without should not.
#[test]
fn feedback_is_detected() {
    let cyc = load("configs/06-cycle.factory");
    assert!(
        !domains::analyse(&cyc.blueprints[0]).feedback_classes.is_empty(),
        "the catalyst loop is feedback"
    );
    let line = load("configs/02-balanced.factory");
    assert!(
        domains::analyse(&line.blueprints[0]).feedback_classes.is_empty(),
        "a straight line is not feedback"
    );
}

// ============================================================== the DSL

#[test]
fn replication_is_a_population_not_a_node_list() {
    let prog = load("configs/09-population.factory");
    let bp = deployed(&prog);
    assert_eq!(bp.actors.len(), 6, "six classes, however many machines");
    assert_eq!(bp.machines, 10_085);
    // The point: analysis walks classes, not machines.
    assert!(bp.nodes() < 20, "the blueprint must stay small");
}

#[test]
fn item_qualified_wires_keep_bays_separate() {
    let prog = load("configs/06-cycle.factory");
    let bp = deployed(&prog);
    let cat = prog.items.iter().position(|i| i == "Catalyst").unwrap() as ItemId;
    let prod = prog.items.iter().position(|i| i == "Product").unwrap() as ItemId;
    let catbay = bp.storages.iter().position(|s| s.name == "CatBay").unwrap();
    assert!(bp.slot_of(catbay, cat).is_some(), "CatBay holds catalyst");
    assert!(
        bp.slot_of(catbay, prod).is_none(),
        "CatBay must not be able to hold product"
    );
}

#[test]
fn bad_programs_are_rejected() {
    let cases: &[(&str, &str)] = &[
        ("storage with no capacity", "blueprint B { storage S { } }"),
        (
            "link that goes nowhere",
            "blueprint B { storage S { capacity 10 } link L { moves 1 X takes 5 ticks } \
             wire S -> L -> S }",
        ),
        (
            "priority naming an unwired machine",
            "blueprint B { storage S { capacity 10 priority Ghost } \
             source M { produces 1 X every 5 ticks } wire M -> S }",
        ),
        (
            "over-seeded storage",
            "blueprint B { storage S { capacity 10 initial 99 X } \
             source M { produces 1 X every 5 ticks } wire M -> S }",
        ),
        (
            "item-qualified wire that leaves an ingredient with no source",
            "blueprint B { storage S { capacity 10 } source M { produces 1 X every 5 ticks } \
             process P { consumes 1 Y takes 5 ticks produces 1 X } \
             wire M -> S  wire S -> P { X }  wire P -> S }",
        ),
    ];
    for (what, src) in cases {
        assert!(dsl::parse(src).is_err(), "should have been rejected: {what}");
    }
}
