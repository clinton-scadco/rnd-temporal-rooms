//! The load-bearing claim of this crate is that the closed form and the event
//! simulator are the *same function*, computed two different ways. These tests
//! try hard to break that.

use temporal_rooms::analytic::{self, Rat};
use temporal_rooms::dsl;
use temporal_rooms::model::*;
use temporal_rooms::sim::{CountersBig, World};

fn load(path: &str) -> Program {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    dsl::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn configs() -> Vec<Program> {
    vec![
        load("configs/01-spec.factory"),
        load("configs/02-balanced.factory"),
        load("configs/03-megafactory.factory"),
    ]
}

/// Dense sweep of tick values: every tick up to 400, then coarse and absurd ones.
fn probe_ticks() -> Vec<Tick> {
    let mut v: Vec<Tick> = (0..400).collect();
    v.extend((400..20_000).step_by(97));
    v.extend([
        50_000,
        123_457,
        1_000_000,
        999_999_999,
        1_000_000_000_000,
        u64::MAX / 4,
    ]);
    v
}

#[test]
fn closed_form_matches_event_sim_everywhere() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        let n_items = prog.items.len();
        let cf = analytic::orbit(bp, n_items, 20_000_000);
        assert!(cf.found, "{}: no orbit found", bp.name);

        for t in probe_ticks() {
            let ana = cf.eval(bp, n_items, t);
            // Simulating to u64::MAX/4 directly is the thing we are avoiding, so
            // only cross-check by simulation where simulation is affordable.
            if t > 200_000 {
                continue;
            }
            let mut w = World::new(bp, n_items, 1, 0);
            w.run_until(t);
            assert_eq!(
                CountersBig::from_narrow(&w.c),
                ana,
                "{} disagrees at t={t}",
                bp.name
            );
        }
    }
}

/// Beyond the horizon we can afford to simulate, the closed form must still be
/// self-consistent: advancing by a whole orbit must add exactly `delta`.
#[test]
fn closed_form_is_periodic_at_astronomical_ticks() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        let n_items = prog.items.len();
        let cf = analytic::orbit(bp, n_items, 20_000_000);
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
                "{}: item {} is not periodic at t={t}",
                bp.name,
                prog.items[i]
            );
        }
    }
}

/// Mass balance: an item is either still in a storage, or it was consumed.
/// Items inside a working machine have been consumed and not yet produced, so
/// `inventory == produced - consumed` holds at *every* instant.
#[test]
fn mass_is_conserved() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        let n_items = prog.items.len();
        for t in (0..6_000).step_by(37) {
            let mut w = World::new(bp, n_items, 3, 7);
            w.run_until(t);
            for item in 0..n_items as ItemId {
                let inventory: u64 = (0..3)
                    .flat_map(|inst| {
                        (0..bp.storages.len()).map(move |s| (inst, s))
                    })
                    .map(|(inst, s)| w.storage_qty(inst, s, item))
                    .sum();
                let net = w.c.produced[item as usize] - w.c.consumed[item as usize];
                assert_eq!(
                    inventory, net,
                    "{} leaks {} at t={t}",
                    bp.name, prog.items[item as usize]
                );
            }
        }
    }
}

#[test]
fn capacity_is_never_exceeded() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        let n_items = prog.items.len();
        let mut w = World::new(bp, n_items, 4, 7);
        for t in (0..4_000).step_by(13) {
            w.run_until(t);
            for inst in 0..4 {
                for (s, sd) in bp.storages.iter().enumerate() {
                    let used = w.storage_used(inst, s);
                    assert!(
                        used <= sd.capacity,
                        "{}: {} holds {} > capacity {} at t={t}",
                        bp.name,
                        sd.name,
                        used,
                        sd.capacity
                    );
                    let sum: u64 = sd.slots.iter().map(|&i| w.storage_qty(inst, s, i)).sum();
                    assert_eq!(sum, used, "{}: {} occupancy desync", bp.name, sd.name);
                }
            }
        }
    }
}

/// The whole scaling argument: archetype aggregation must reproduce a fully
/// materialised deployment exactly, for every stagger and every size.
#[test]
fn archetypes_reproduce_materialised_deployments() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        let n_items = prog.items.len();
        let cf = analytic::orbit(bp, n_items, 20_000_000);
        for &stagger in &[0u64, 1, 7, 13, 60, 97] {
            for &count in &[1u64, 2, 59, 60, 61, 137, 541] {
                for &t in &[0u64, 137, 1_000, 2_500] {
                    let d = Deploy { blueprint: 0, count, stagger };
                    let mut w = World::new(bp, n_items, count, stagger);
                    w.run_until(t);
                    let (totals, _) = analytic::deployment_totals(bp, n_items, &cf, &d, t);
                    assert_eq!(
                        CountersBig::from_narrow(&w.c),
                        totals,
                        "{}: count={count} stagger={stagger} t={t}",
                        bp.name
                    );
                }
            }
        }
    }
}

#[test]
fn archetype_counts_partition_the_deployment() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        for &stagger in &[0u64, 1, 7, 11, 540] {
            for &count in &[1u64, 5, 999, 66_666_667, u32::MAX as u64] {
                let arch = analytic::archetypes(bp, count, stagger);
                let total: u64 = arch.iter().map(|a| a.count).sum();
                assert_eq!(total, count, "archetypes lose instances");
                assert!(arch.len() as u64 <= bp.base_period.max(1));
                for a in &arch {
                    assert!(a.offset < bp.base_period.max(1));
                }
            }
        }
    }
}

/// T3 is a fluid approximation, so it may disagree about *which* machine does
/// the work -- but it must get aggregate item throughput right on a plant that
/// reaches a genuine steady state.
#[test]
fn rate_algebra_matches_orbit_on_aggregate_throughput() {
    for prog in configs() {
        let bp = &prog.blueprints[0];
        let n_items = prog.items.len();
        let cf = analytic::orbit(bp, n_items, 20_000_000);
        let rr = analytic::rates(bp, n_items);
        if cf.frozen {
            assert!(rr.terminal, "{}: orbit froze but rate algebra disagrees", bp.name);
            continue;
        }
        assert!(!rr.terminal, "{}: plant runs but rate algebra calls it terminal", bp.name);
        for i in 0..n_items {
            assert_eq!(
                cf.steady_output_per_tick(i),
                rr.produced_per_tick[i],
                "{}: throughput of {} disagrees",
                bp.name,
                prog.items[i]
            );
        }
    }
}

#[test]
fn reference_plant_deadlocks_exactly_as_predicted() {
    let prog = load("configs/01-spec.factory");
    let bp = &prog.blueprints[0];
    let n_items = prog.items.len();
    let cf = analytic::orbit(bp, n_items, 1_000_000);

    assert!(cf.frozen, "the reference plant is supposed to deadlock");
    assert_eq!(cf.t0, 2060, "deadlock time");
    assert_eq!(cf.base.produced[0], 1000, "ore ever produced");
    assert_eq!(cf.base.produced[1], 1000, "plate ever produced");

    // Storage ends full of plate; nothing can move.
    let (w, _) = cf.world_at(bp, n_items, 10_000_000);
    assert_eq!(w.storage_qty(0, 0, 1), 1000);
    assert_eq!(w.storage_qty(0, 0, 0), 0);

    // And it stays that way forever.
    let a = cf.eval(bp, n_items, 3_000);
    let b = cf.eval(bp, n_items, u64::MAX / 2);
    assert_eq!(a, b);
}

#[test]
fn rationals_are_exact() {
    let third = Rat::new(1, 3);
    let sum = third.add(third).add(third);
    assert_eq!(sum, Rat::new(1, 1));
    assert_eq!(Rat::new(100, 60), Rat::new(5, 3));
    assert!(Rat::new(1, 60).lt(Rat::new(1, 20)));
    assert_eq!(Rat::new(5, 3).mul(Rat::new(3, 5)), Rat::new(1, 1));
}

#[test]
fn dsl_rejects_broken_programs() {
    let cases: &[(&str, &str)] = &[
        ("blueprint B { source S { produces 1 A } }", "cycle takes"),
        ("blueprint B { source S { produces 1 A every 0 ticks } }", "zero-tick"),
        ("blueprint B { storage T { capacity 0 } source S { produces 1 A every 5 ticks } wire S -> T }", "zero capacity"),
        ("blueprint B { source S { produces 1 A every 5 ticks } }", "nowhere to put"),
        ("blueprint B { source S { produces 1 A every 5 ticks } storage T { capacity 5 } wire S -> Nope }", "unknown node"),
        ("blueprint B { source S { produces 1 A every 5 ticks } source S2 { produces 1 A every 5 ticks } wire S -> S2 }", "two machines"),
        ("item A\nblueprint B { source S { produces 1 Zz every 5 ticks } storage T { capacity 5 } wire S -> T }", "unknown item"),
        ("deploy 5 x Ghost", "unknown blueprint"),
    ];
    for (src, want) in cases {
        match dsl::parse(src) {
            Ok(_) => panic!("expected `{want}` error, but this parsed: {src}"),
            Err(e) => assert!(
                e.msg.contains(want),
                "expected `{want}`, got `{}` for: {src}",
                e.msg
            ),
        }
    }
}

#[test]
fn dsl_accepts_both_replication_spellings() {
    let a = dsl::parse(
        "blueprint B { storage T { capacity 50 } process P x3 { consumes 1 A takes 2 ticks produces 1 C } wire T -> P -> T }",
    )
    .unwrap();
    let b = dsl::parse(
        "blueprint B { storage T { capacity 50 } process P x 3 { consumes 1 A takes 2 ticks produces 1 C } wire T -> P -> T }",
    )
    .unwrap();
    assert_eq!(a.blueprints[0].actors.len(), 3);
    assert_eq!(b.blueprints[0].actors.len(), 3);
}

/// A source feeding a storage with no drain must stall at exactly the tick the
/// buffer fills, and never overfill it.
#[test]
fn backpressure_stalls_a_source_precisely() {
    let prog = dsl::parse(
        "blueprint B { source S { produces 30 A every 10 ticks } storage T { capacity 100 } wire S -> T }",
    )
    .unwrap();
    let bp = &prog.blueprints[0];
    let n_items = prog.items.len();
    let cf = analytic::orbit(bp, n_items, 100_000);

    // 30 units per 10 ticks into a 100-unit buffer: deposits at t=10,20,30 fill
    // it to 90, and the deposit at t=40 cannot fit.
    let (w, _) = cf.world_at(bp, n_items, 1_000);
    assert_eq!(w.storage_used(0, 0), 90);
    assert!(cf.frozen);
    assert_eq!(cf.base.produced[0], 90);
}
