//! Experiment 16: an active disturbance, caused by what crosses a temporal
//! boundary, and the properties that make it one rather than a second
//! simulation bolted on top of the first.
//!
//! ```text
//!   causation      nothing crosses, nothing happens; the first tear is solved
//!   the levers     laundering starves a store; an anchor keeps the gantry lit
//!   the bleed      an 1890 tear darkens the interface and silences guns
//!   compression    a cohort is one record, whatever it stands for
//!   the boundary   only what a rupture can reach wakes; the rest is
//!                  bit-identical to a world where nothing crossed
//!   collapse       every woken sector finds an orbit again
//!   the log        commands in, events never out
//!   reconstruction live == replayed == resumed from a mid-disturbance checkpoint
//! ```

use temporal_rooms::combat::field::{self, SITES, STRUCTURES};
use temporal_rooms::combat::fight::Fight;
use temporal_rooms::combat::run::{self, Cmd, Command, Encounter};
use temporal_rooms::combat::TICK_RATE;
use temporal_rooms::json;
use temporal_rooms::model::Tick;

const SEED: u64 = 16;
const DEEP: usize = 0;
const NEAR: usize = 1;

fn secs(s: Tick) -> Tick {
    s * TICK_RATE
}

/// Play a script of `(second, command)`, then run until the district is quiet
/// and the domain has closed.
fn played(script: &[(Tick, Cmd)]) -> Encounter {
    let mut e = Encounter::new(SEED);
    for (t, c) in script {
        e.advance_to(secs(*t));
        e.apply(c.clone()).unwrap();
    }
    let stop = e.now + 30 * 60 * TICK_RATE;
    while e.now < stop {
        e.advance_to(e.now + TICK_RATE);
        if !e.open && e.fight.quiet() {
            break;
        }
    }
    assert!(!e.open, "the district never settled");
    e
}

/// Both lanes open for `secs` seconds at `rate` (the near lane at half).
fn shipped(rate: u64, secs: Tick, extra: &[(Tick, Cmd)]) -> Encounter {
    let mut s = vec![
        (2, Cmd::Stream { lane: DEEP, rate }),
        (2, Cmd::Stream { lane: NEAR, rate: (rate / 2).max(1) }),
    ];
    s.extend(extra.iter().cloned());
    s.push((2 + secs, Cmd::Stream { lane: DEEP, rate: 0 }));
    s.push((2 + secs, Cmd::Stream { lane: NEAR, rate: 0 }));
    s.sort_by_key(|(t, _)| *t);
    played(&s)
}

fn anchored(rate: u64, secs: Tick) -> Encounter {
    shipped(
        rate,
        secs,
        &[(1, Cmd::Anchor { anchor: 0, on: true }), (1, Cmd::Anchor { anchor: 1, on: true })],
    )
}

fn sector(e: &Encounter, name: &str) -> usize {
    e.sectors.iter().position(|s| s.spec.name == name).unwrap()
}

// ================================================================== causation

#[test]
fn nothing_crossing_means_nothing_happens() {
    let mut e = Encounter::new(SEED);
    e.advance_to(secs(600));
    assert_eq!(e.fight.stats.events, 0);
    assert!(e.episodes.is_empty() && e.sectors.iter().all(|s| s.woke == 0));
}

#[test]
fn a_trickle_strains_but_does_not_tear() {
    let e = shipped(5, 60, &[]);
    assert_eq!(e.fight.stats.tears, 0);
    assert!(e.fight.total(0) > 0, "the gantry carries what came through it");
    assert!(e.fight.total(1) > 0, "the ore yard carries what was stacked in it");
}

#[test]
fn the_tear_is_where_the_anachronism_is() {
    let e = shipped(40, 60, &[]);
    let ep = &e.episodes[0];
    assert_eq!(ep.tore[0], "Gantry", "the door tears first: it has the lowest threshold");
    assert!(e.fight.spawned > 0);
    // An echo is made of 1890 and a glint of 2070, and each went looking for
    // its own century's store.
    assert!(e.notes.iter().any(|(_, n)| n.contains("reach Ore yard") || n.contains("fade")));
}

// ================================================================= the levers

#[test]
fn laundering_starves_the_ore_yard() {
    let plain = shipped(40, 40, &[]);
    let washed = shipped(40, 40, &[(1, Cmd::Launder { on: true })]);
    let yard = 1;
    assert!(plain.fight.strain_at(yard, plain.now)[0] > 0 || plain.fight.stats.tears > 0);
    assert_eq!(washed.fight.strain_at(yard, washed.now)[0], 0, "no 1890 ore was ever stacked");
    assert!(
        !washed.episodes.iter().any(|ep| ep.tore.contains(&"Ore yard")),
        "a yard with nothing out of its time in it cannot tear"
    );
}

#[test]
fn an_unanchored_gantry_goes_dark_and_an_anchored_one_does_not() {
    // Run with the lanes open and watch the interface.
    let watch = |anchor: bool| {
        let mut e = Encounter::new(SEED);
        if anchor {
            e.advance_to(secs(1));
            e.apply(Cmd::Anchor { anchor: 0, on: true }).unwrap();
        }
        e.advance_to(secs(2));
        e.apply(Cmd::Stream { lane: DEEP, rate: 40 }).unwrap();
        let mut dark = 0;
        let mut silenced = false;
        for _ in 0..90 {
            e.advance_to(e.now + TICK_RATE);
            dark += e.fight.dark() as u32;
            silenced |= (0..field::BATTERIES.len()).any(|b| !e.fight.powered(b));
        }
        (dark, silenced, e)
    };
    let (dark, silenced, e) = watch(false);
    assert!(dark > 0, "an 1890 tear on the gantry takes its grid away");
    assert!(silenced, "and the guns standing in the bleed with it");
    let i = sector(&e, "Interface");
    assert!(e.sectors[i].edits.iter().any(|(_, w)| w.contains("remove Haul")), "the haul road stops when it is dark");
    let (dark, _, e) = watch(true);
    assert_eq!(dark, 0, "the north anchor pins the gantry to 2037");
    assert!(e.fight.stats.tears > 0, "a pinned gantry still tears; it just does not bleed");
}

#[test]
fn the_grid_pays_for_holding_time_still() {
    let mut f = Fight::new();
    f.stream(DEEP, 10);
    f.stream(NEAR, 10);
    assert_eq!(f.grid(), (151, 1000), "both fractures fit on the grid");
    f.anchor(0, true);
    f.anchor(1, true);
    f.set_launder(true);
    let (demand, factor) = f.grid();
    assert_eq!(demand, 151 + 50 + 20);
    assert!(factor < 1000, "anchors and laundering come out of the lanes' share");
    assert_eq!(f.flow(DEEP), 10 * factor);
}

// ================================================================ compression

/// Four orders of magnitude of what crosses, and the state that stands for
/// what it manifests barely moves. The brief's success criterion, as an
/// assertion -- anchored, so the gantry cannot throttle itself and every
/// tonne asked for arrives.
#[test]
fn cohorts_do_not_grow_with_the_crossing() {
    let small = anchored(400, 75);
    let huge = anchored(40_000, 75);
    let (a, b) = (&small.fight.stats, &huge.fight.stats);
    assert!(huge.fight.spawned > 100 * small.fight.spawned.max(1), "{} vs {}", huge.fight.spawned, small.fight.spawned);
    assert!(b.peak_cohorts <= 40, "peak cohorts {}", b.peak_cohorts);
    assert!(b.events <= a.events * 3, "events {} vs {}", b.events, a.events);
    assert!(b.visits <= a.visits * 3, "visits {} vs {}", b.visits, a.visits);
}

#[test]
fn everything_manifested_is_accounted_for() {
    let e = shipped(40, 60, &[]);
    let f = &e.fight;
    assert!(f.stats.splits > 5 && f.stats.merges > 5, "splits {} merges {}", f.stats.splits, f.stats.merges);
    assert_eq!(f.killed + f.faded + f.alive_count(), f.spawned);
}

#[test]
fn an_unanchored_fracture_limits_itself() {
    let mut e = Encounter::new(SEED);
    e.advance_to(secs(2));
    e.apply(Cmd::Stream { lane: DEEP, rate: 10_000 }).unwrap();
    e.advance_to(secs(62));
    let crossed = e.fight.crossed_at(DEEP, e.now) / 1000;
    assert!(crossed < 10_000 * 60 / 4, "{crossed} t crossed of 600,000 asked");
}

// =============================================================== the boundary

#[test]
fn only_what_a_rupture_can_reach_wakes() {
    let e = shipped(40, 60, &[]);
    let mut calm = Encounter::new(SEED);
    calm.advance_to(e.now);
    let same = e.same_sectors(&calm, e.now);
    let foundry = sector(&e, "Foundry");
    assert_eq!(e.sectors[foundry].woke, 0, "the foundry is out of every reach");
    assert_eq!(e.sectors[foundry].stepped, 0);
    assert_eq!(e.sectors[foundry].recompiles, 0);
    assert!(same[foundry], "the foundry differs from a world where nothing crossed");
    for (i, s) in e.sectors.iter().enumerate() {
        if s.woke > 0 {
            let reached = (0..SITES.len()).any(|k| field::reaches(k, i));
            assert!(reached, "{} woke but nothing can reach it", s.spec.name);
        }
    }
    let iface = sector(&e, "Interface");
    assert!(e.sectors[iface].woke >= 1);
}

#[test]
fn every_sector_collapses_back_into_an_orbit() {
    for rate in [40, 400] {
        let e = shipped(rate, 60, &[]);
        assert_eq!(e.closed_count(), field::SECTORS.len(), "at {rate} t/s");
    }
}

#[test]
fn a_repair_restores_the_topology() {
    let mut e = Encounter::new(SEED);
    e.advance_to(secs(2));
    e.apply(Cmd::Stream { lane: DEEP, rate: 40 }).unwrap();
    while !e.fight.dark() {
        e.advance_to(e.now + 10);
    }
    let i = sector(&e, "Interface");
    assert!(e.sectors[i].graph.node("Haul").is_none(), "dark means no haul road");
    assert!(e.sectors[i].graph.node("OreYard").unwrap().holds.contains(&"Ore".to_string()));
    e.apply(Cmd::Anchor { anchor: 0, on: true }).unwrap();
    assert!(!e.fight.dark(), "the anchor relights it at once");
    assert_eq!(e.sectors[i].graph.emit(), e.sectors[i].base.emit(), "and the haul road is back as built");
    assert!(e.apply(Cmd::Repair { structure: run::structure("Crusher house").unwrap() }).is_err(), "repairing something intact");
}

// ==================================================================== the log

#[test]
fn the_log_holds_commands_not_events() {
    let small = shipped(40, 30, &[]);
    let huge = anchored(40_000, 30);
    let (a, b) = (small.log_json().to_string(), huge.log_json().to_string());
    assert_eq!(small.log.len(), 4);
    assert!(b.len() <= a.len() + 120, "{} vs {}", b.len(), a.len());
    assert!(huge.fight.stats.events > 100);
    for c in &huge.log {
        let back = Command::from_json(&json::parse(&c.to_json().to_string()).unwrap()).unwrap();
        assert_eq!(&back, c);
    }
}

#[test]
fn a_disturbance_round_trips_through_json() {
    let mut e = Encounter::new(SEED);
    e.advance_to(secs(2));
    e.apply(Cmd::Stream { lane: DEEP, rate: 40 }).unwrap();
    e.apply(Cmd::Stream { lane: NEAR, rate: 20 }).unwrap();
    while e.fight.cohorts.is_empty() || e.fight.volleys.is_empty() {
        e.advance_to(e.now + 5);
        assert!(e.now < secs(120), "nothing manifested");
    }
    let j = e.fight.to_json();
    let back = Fight::from_json(&json::parse(&j.to_string()).unwrap()).unwrap();
    assert_eq!(back.to_json().to_string(), j.to_string());
    assert!(back.sites.iter().any(|s| s.rupture.is_some()));
}

// ============================================================ reconstruction

#[test]
fn live_replayed_and_resumed_agree() {
    let mut e = Encounter::new(SEED);
    e.advance_to(secs(2));
    e.apply(Cmd::Stream { lane: DEEP, rate: 60 }).unwrap();
    e.apply(Cmd::Stream { lane: NEAR, rate: 25 }).unwrap();
    // A live clock advances in uneven steps; a replay jumps to each command.
    let mut t: Tick = e.now;
    let mut k = 0;
    while t < secs(50) {
        k += 1;
        t += 7 + (k * 13) % 29;
        e.advance_to(t);
    }
    e.apply(Cmd::Anchor { anchor: 1, on: true }).unwrap();
    let hold = run::battery("Street mortars").unwrap();
    e.apply(Cmd::Hold { battery: hold, hold: true }).unwrap();
    e.advance_to(e.now + secs(5));
    e.apply(Cmd::Hold { battery: hold, hold: false }).unwrap();
    e.apply(Cmd::Launder { on: true }).unwrap();
    e.advance_to(secs(80));
    e.apply(Cmd::Stream { lane: DEEP, rate: 0 }).unwrap();
    e.apply(Cmd::Stream { lane: NEAR, rate: 0 }).unwrap();
    while e.open || !e.fight.quiet() {
        e.advance_to(e.now + TICK_RATE);
    }
    for (s, def) in STRUCTURES.iter().enumerate() {
        if e.fight.hp_at(s, e.now) < def.hp * 1000 {
            e.apply(Cmd::Repair { structure: s }).unwrap();
        }
    }
    e.advance_to(e.now + secs(10));
    let end = e.now;
    let live = e.hash();

    assert_eq!(Encounter::replay(SEED, &e.log, end).hash(), live, "replayed from the log");
    let mid = e
        .checkpoints
        .iter()
        .find(|c| c.tick > secs(20) && c.tick < secs(60))
        .expect("a checkpoint inside the disturbance");
    let r = Encounter::resume(SEED, mid, &e.log, end).unwrap();
    assert_eq!(r.hash(), live, "resumed from t={}", mid.tick);
    assert!(!mid.json.contains("impact"), "a checkpoint holds no events");

    let mut other = e.log.clone();
    other.retain(|c| !matches!(c.cmd, Cmd::Launder { .. }));
    assert_ne!(Encounter::replay(SEED, &other, end).hash(), live, "another log is another afternoon");
}
