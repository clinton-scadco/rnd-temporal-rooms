//! Experiment 16: combat as an active disturbance, and the properties that make
//! it one rather than a second simulation bolted on top of the first.
//!
//! ```text
//!   compression    a cohort is one record, whatever it stands for
//!   the boundary   only what the fight can reach wakes; only what it changed
//!                  is recompiled; everything else is bit-identical to peace
//!   collapse       every woken sector finds an orbit again
//!   the log        commands in, events never out
//!   reconstruction live == replayed == resumed from a mid-fight checkpoint
//! ```

use temporal_rooms::combat::field::{self, STRUCTURES};
use temporal_rooms::combat::fight::{Band, Fight};
use temporal_rooms::combat::run::{self, Cmd, Command, Encounter};
use temporal_rooms::combat::TICK_RATE;
use temporal_rooms::json;
use temporal_rooms::model::Tick;

const SEED: u64 = 16;

/// One wave, run until the domain settles.
fn fought(n: u64, seed: u64) -> Encounter {
    let mut e = Encounter::new(seed);
    e.advance_to(2 * TICK_RATE);
    e.apply(Cmd::Wave { nominal: n }).unwrap();
    let stop = e.now + 20 * 60 * TICK_RATE;
    while e.now < stop {
        e.advance_to(e.now + TICK_RATE);
        if !e.open {
            break;
        }
    }
    assert!(!e.open, "a wave of {n} never settled");
    e
}

fn sector(e: &Encounter, name: &str) -> usize {
    e.sectors.iter().position(|s| s.spec.name == name).unwrap()
}

// ================================================================ compression

/// Five orders of magnitude of attackers, and the state that stands for them
/// barely moves. The success criterion in the brief, as an assertion.
#[test]
fn cohorts_do_not_grow_with_the_wave() {
    let small = fought(10_000, SEED);
    let huge = fought(1_000_000, SEED);
    let (a, b) = (&small.fight.stats, &huge.fight.stats);
    assert!(b.peak_cohorts <= 32, "peak cohorts {}", b.peak_cohorts);
    assert!(b.peak_cohorts <= a.peak_cohorts * 2, "{} vs {}", b.peak_cohorts, a.peak_cohorts);
    assert!(b.events <= a.events * 2, "events {} vs {}", b.events, a.events);
    assert!(b.visits <= a.visits * 2, "visits {} vs {}", b.visits, a.visits);
    assert!(huge.fight.spawned == 1_000_000);
}

/// A split is a record, not an attacker, and identical states are one record.
#[test]
fn a_volley_splits_a_cohort_and_the_pieces_merge() {
    let e = fought(3_000, SEED);
    let s = &e.fight.stats;
    assert!(s.splits > 50, "splits {}", s.splits);
    assert!(s.merges > 50, "merges {}", s.merges);
    // Every attacker is accounted for exactly once.
    assert_eq!(e.fight.killed + e.fight.leaked + e.fight.alive_count(), 3_000);
}

// =============================================================== the boundary

/// The default wave destroys the conveyor and nothing else in the factory.
/// Quarry and Works never hear about it; the Yard wakes and is not changed;
/// Smelting is recompiled -- and only Smelting.
#[test]
fn only_the_disturbed_region_is_invalidated() {
    let e = fought(3_000, SEED);
    let belt = run::structure("Ore conveyor").unwrap();
    assert_eq!(e.fight.structures[belt].band, Band::Destroyed, "the test needs a torn-up conveyor");
    let mut calm = Encounter::new(SEED);
    calm.advance_to(e.now);
    let same = e.same_sectors(&calm, e.now);
    for name in ["Quarry", "Works"] {
        let i = sector(&e, name);
        assert_eq!(e.sectors[i].woke, 0, "{name} woke");
        assert_eq!(e.sectors[i].stepped, 0, "{name} was stepped");
        assert_eq!(e.sectors[i].recompiles, 0, "{name} was recompiled");
        assert!(same[i], "{name} differs from a world with no fight");
    }
    let y = sector(&e, "Yard");
    assert_eq!(e.sectors[y].woke, 1);
    assert_eq!(e.sectors[y].recompiles, 0);
    assert!(same[y], "the Yard woke, was stepped, and must still be exactly itself");
    let s = sector(&e, "Smelting");
    assert!(e.sectors[s].recompiles >= 1);
    assert!(e.sectors[s].graph.node("Conveyor").is_none());
    assert!(!same[s]);
    // The hopper kept what had arrived before the belt went.
    assert!(e.sectors[s].graph.node("Hopper").unwrap().holds.contains(&"Ore".to_string()));
}

/// A repair after the fight puts the plant back as built.
#[test]
fn a_repair_restores_the_topology() {
    let mut e = fought(3_000, SEED);
    let belt = run::structure("Ore conveyor").unwrap();
    e.apply(Cmd::Repair { structure: belt }).unwrap();
    let s = sector(&e, "Smelting");
    assert_eq!(e.sectors[s].graph.emit(), e.sectors[s].base.emit());
    assert!(e.apply(Cmd::Repair { structure: belt }).is_err(), "repairing something intact");
}

// ================================================================== collapse

#[test]
fn every_sector_collapses_back_into_an_orbit() {
    for n in [100, 3_000, 100_000] {
        let e = fought(n, SEED);
        assert_eq!(e.closed_count(), field::SECTORS.len(), "wave {n}");
    }
}

// ==================================================================== the log

/// What is written down does not grow with the fight.
#[test]
fn the_log_holds_commands_not_events() {
    let small = fought(100, SEED);
    let huge = fought(100_000, SEED);
    let (a, b) = (small.log_json().to_string(), huge.log_json().to_string());
    assert_eq!(small.log.len(), 1);
    assert!(b.len() <= a.len() + 4, "{} vs {}", b.len(), a.len());
    assert!(huge.fight.stats.events > 1_000);
    for c in &huge.log {
        let back = Command::from_json(&json::parse(&c.to_json().to_string()).unwrap()).unwrap();
        assert_eq!(&back, c);
    }
}

/// A checkpoint is state, and it round-trips.
#[test]
fn a_fight_round_trips_through_json() {
    let mut e = Encounter::new(SEED);
    e.advance_to(TICK_RATE);
    e.apply(Cmd::Wave { nominal: 3_000 }).unwrap();
    e.advance_to(30 * TICK_RATE);
    let j = e.fight.to_json();
    let back = Fight::from_json(&json::parse(&j.to_string()).unwrap()).unwrap();
    assert_eq!(back.to_json().to_string(), j.to_string());
    assert!(!back.cohorts.is_empty() && !back.volleys.is_empty());
}

// ============================================================ reconstruction

#[test]
fn live_replayed_and_resumed_agree() {
    let mut e = Encounter::new(SEED);
    e.advance_to(2 * TICK_RATE);
    e.apply(Cmd::Wave { nominal: 3_200 }).unwrap();
    // A live clock advances in uneven steps; a replay jumps to each command.
    let mut t: Tick = e.now;
    let mut k = 0;
    while t < 70 * TICK_RATE {
        k += 1;
        t += 7 + (k * 13) % 29;
        e.advance_to(t);
    }
    let hold = run::battery("Mortars").unwrap();
    e.apply(Cmd::Hold { battery: hold, hold: true }).unwrap();
    e.advance_to(e.now + 5 * TICK_RATE);
    e.apply(Cmd::Hold { battery: hold, hold: false }).unwrap();
    while e.open {
        e.advance_to(e.now + TICK_RATE);
    }
    for (s, def) in STRUCTURES.iter().enumerate() {
        if def.tie.is_some() && e.fight.hp_at(s, e.now) < def.hp * 1000 {
            e.apply(Cmd::Repair { structure: s }).unwrap();
        }
    }
    e.advance_to(e.now + 10 * TICK_RATE);
    let end = e.now;
    let live = e.hash();

    assert_eq!(Encounter::replay(SEED, &e.log, end).hash(), live, "replayed from the seed");
    let mid = e
        .checkpoints
        .iter()
        .find(|c| c.tick > 20 * TICK_RATE && c.tick < 60 * TICK_RATE)
        .expect("a checkpoint inside the fight");
    let r = Encounter::resume(SEED, mid, &e.log, end).unwrap();
    assert_eq!(r.hash(), live, "resumed from t={}", mid.tick);
    assert!(!mid.json.contains("impact"), "a checkpoint holds no events");

    assert_ne!(Encounter::replay(SEED + 1, &e.log, end).hash(), live, "another seed is another fight");
}
