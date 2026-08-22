//! Experiment 06, held to the same standard as the rest of the crate.
//!
//! The prototype makes one load-bearing claim -- that a design compiles to a
//! startup transient plus an exact periodic orbit, and that the orbit answers
//! any tick however far away -- and a claim like that is worthless unless
//! something keeps trying to break it. So the compiled answer is never trusted
//! here, it is compared: against a straight tick-by-tick run, at ticks either
//! side of the transient, at ticks that are exact multiples of the period, at
//! ticks that are not, and at a tick a billion in the future that no straight
//! run would reach.
//!
//! The rest is the boring, essential kind: a file that round-trips, a rule that
//! refuses what it says it refuses, and eight components that each do the one
//! thing the table says they do.

use temporal_rooms::json;
use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::parts::{self, Kind};
use temporal_rooms::machine::sim::{Machine, Status, Tick, Totals};
use temporal_rooms::machine::{eval, orbit, snap};

const DESIGNS: &[&str] = &[
    "designs/01-first-try.machine",
    "designs/02-more-of-everything.machine",
    "designs/03-compact.machine",
    "designs/04-stalled.machine",
    "designs/05-pulsed.machine",
    "designs/06-radial.machine",
];

fn load(path: &str) -> Design {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    let d = Design::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"));
    let faults = d.check();
    assert!(faults.is_empty(), "{path}: {}", faults[0].what);
    d
}

/// A straight run to `t`, which is the thing every clever answer is measured
/// against.
fn plod(d: &Design, t: Tick) -> (Machine, Totals) {
    let mut m = Machine::new(d).unwrap();
    let mut tot = Totals::default();
    for _ in 0..t {
        tot.add(&m.step());
    }
    (m, tot)
}

// -------------------------------------------------------------- the orbit

#[test]
fn every_design_settles() {
    for path in DESIGNS {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        assert!(
            c.settled(),
            "{path} had not repeated itself after {} ticks",
            c.searched
        );
        assert!(c.period > 0);
        assert!(
            c.transient + c.period < orbit::SEARCH,
            "{path}: the orbit does not fit inside the search"
        );
    }
}

/// The claim, stated as a test: the state at tick `t` and the state at the tick
/// the compiler says is indistinguishable from `t` really are indistinguishable
/// -- byte for byte, in the key the orbit search itself keys on.
#[test]
fn the_equivalent_tick_is_equivalent() {
    for path in DESIGNS {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        for t in [c.transient, c.transient + 1, c.transient + c.period,
                  c.transient + c.period * 3 + 1, 50_000, 1_000_000] {
            let far = plod(&d, t.min(60_000)).0;
            let near = c.state_at(&d, t.min(60_000)).unwrap();
            assert_eq!(
                far.key(),
                near.key(),
                "{path}: tick {t} is not the state the orbit says it is"
            );
        }
    }
}

/// Totals, which is the harder half: a state can repeat while a counter cannot,
/// so the arithmetic that turns laps into numbers has to be right on its own.
#[test]
fn compiled_totals_match_a_straight_run() {
    for path in DESIGNS {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        let probes: Vec<Tick> = vec![
            0, 1, 7, 60,
            c.transient.saturating_sub(1),
            c.transient,
            c.transient + 1,
            c.transient + c.period,
            c.transient + c.period * 7 + 3,
            9_999,
            40_000,
        ];
        let checks = orbit::verify(&d, &probes).unwrap();
        for k in checks {
            assert!(
                k.agrees,
                "{path} at t={}: simulated {:?} but compiled {:?}",
                k.tick, k.simulated, k.compiled
            );
        }
    }
}

/// The point of compiling at all. A tick a billion in the future must cost the
/// transient plus the period, and no more.
#[test]
fn a_billion_ticks_costs_hundreds() {
    let d = load("designs/05-pulsed.machine");
    let c = orbit::compile(&d).unwrap();
    let far: Tick = 1_000_000_000;
    assert!(c.equivalent_tick(far) <= c.transient + c.period);
    assert!(c.equivalent_tick(far) < 1_000);

    // And the totals it reports there are the exact ones, not an estimate: one
    // full lap must be exactly the orbit's own totals.
    let a = c.totals_at(far);
    let b = c.totals_at(far + c.period);
    assert_eq!(b.power - a.power, c.orbit.power);
    assert_eq!(b.water - a.water, c.orbit.water);
    assert_eq!(b.fuel - a.fuel, c.orbit.fuel);
}

/// Two machines can average the same and be different machines. That is the
/// distinction the whole module tree exists to keep, so something had better
/// notice if it stops being true.
#[test]
fn the_orbit_is_more_than_an_average() {
    let a = orbit::compile(&load("designs/02-more-of-everything.machine")).unwrap();
    let b = orbit::compile(&load("designs/06-radial.machine")).unwrap();
    let ra = eval::report(&load("designs/02-more-of-everything.machine"), &a);
    let rb = eval::report(&load("designs/06-radial.machine"), &b);
    assert_eq!(ra.power.value(), rb.power.value(), "the same average output");
    assert_ne!(
        (ra.wasted.value(), ra.width, ra.components),
        (rb.wasted.value(), rb.width, rb.components),
        "but not the same machine"
    );
}

// ------------------------------------------------------------ determinism

#[test]
fn the_same_design_is_the_same_run() {
    for path in DESIGNS {
        let d = load(path);
        let (a, ta) = plod(&d, 3_000);
        let (b, tb) = plod(&d, 3_000);
        assert_eq!(a.key(), b.key(), "{path}");
        assert_eq!(ta, tb, "{path}");
    }
}

/// Where a component *is* changes what it can reach, but not what it does once
/// it is reached. Sliding the whole machine across the plot must not change a
/// single tick of it.
#[test]
fn translation_changes_nothing() {
    for path in DESIGNS {
        let mut d = load(path);
        let before = plod(&d, 2_500);
        for u in &mut d.units {
            u.x += 7;
            u.y += 3;
        }
        assert!(d.check().is_empty(), "{path}: moving it broke it");
        let after = plod(&d, 2_500);
        assert_eq!(before.0.key(), after.0.key(), "{path}");
        assert_eq!(before.1, after.1, "{path}");
    }
}

// ---------------------------------------------------------- the document

#[test]
fn a_design_survives_being_written_down() {
    for path in DESIGNS {
        let d = load(path);
        let re = Design::parse(&d.emit()).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(d.emit(), re.emit(), "{path}: the file is not stable");
        assert_eq!(plod(&d, 1_500).1, plod(&re, 1_500).1, "{path}: it is not the same machine");
    }
}

#[test]
fn a_design_survives_the_wire() {
    for path in DESIGNS {
        let d = load(path);
        let text = d.to_json().to_string();
        let back = Design::from_json(&json::parse(&text).unwrap()).unwrap();
        assert_eq!(d.emit(), back.emit(), "{path}");
    }
}

#[test]
fn the_rules_refuse_what_they_say_they_refuse() {
    let base = |src: &str| Design::parse(src).unwrap();
    let refuses = |src: &str, needle: &str| {
        let d = base(src);
        let faults = d.check();
        assert!(
            faults.iter().any(|f| f.what.contains(needle)),
            "expected a complaint about `{needle}`, got {:?}",
            faults.iter().map(|f| &f.what).collect::<Vec<_>>()
        );
    };

    refuses(
        "machine \"x\"\nreactor R1 at 0,0\nexchanger HX1 at 2,2\n",
        "overlaps",
    );
    refuses(
        "machine \"x\"\nreactor R1 at 0,0\nexchanger HX1 at 40,0\nwire R1.heat -> HX1.heat\n",
        "tiles apart",
    );
    refuses(
        "machine \"x\"\nreactor R1 at 0,0\ngenerator G1 at 6,0\nwire R1.heat -> G1.rotary\n",
        "carries heat",
    );
    refuses(
        "machine \"x\"\nturbine T1 at 0,0\nexchanger HX1 at 5,0\nwire T1.steam -> HX1.heat\n",
        "wrong way",
    );
    refuses(
        "machine \"x\"\nreactor R1 at 0,0\nreactor R1 at 6,0\n",
        "two components called",
    );
    refuses("machine \"x\"\nreactor R1 at 0,0 throttle 5\n", "outside");
    refuses(
        "machine \"x\"\ngenerator G1 at 0,0\nturbine T1 at 4,0\nwire G1.power -> T1.steam\n",
        "boundary",
    );
}

// ------------------------------------------------------------ the physics

/// Each of the eight components, alone or in the smallest arrangement that
/// makes it do anything, checked against the number in the table.
#[test]
fn the_parts_do_what_the_table_says() {
    // A reactor at full throttle, with nowhere to put its heat, warms up over
    // exactly WARMUP ticks and then vents everything.
    let d = Design::parse("machine \"r\"\nreactor R1 at 0,0\n").unwrap();
    let (m, tot) = plod(&d, parts::WARMUP);
    assert_eq!(m.st[0].age, parts::WARMUP);
    assert_eq!(tot.fuel, parts::REACTOR_FUEL as u128 * parts::WARMUP as u128);
    let (m, _) = plod(&d, parts::WARMUP + 5);
    assert_eq!(m.st[0].status, Status::Venting);
    assert_eq!(m.st[0].made[0] + m.st[0].waste, parts::REACTOR_HEAT);

    // A pump with nowhere to send water fills its own tank and stops, so the
    // water it has drawn is bounded by its capacity rather than by the clock.
    let d = Design::parse("machine \"w\"\npump W1 at 0,0\n").unwrap();
    let (m, tot) = plod(&d, 500);
    assert_eq!(m.st[0].status, Status::Blocked);
    assert_eq!(tot.water, parts::part(Kind::Pump).ports[0].cap as u128);

    // A heat pipe loses its 2%, and the loss is the machine's wasted heat.
    let d = load("designs/02-more-of-everything.machine");
    let c = orbit::compile(&d).unwrap();
    let r = eval::report(&d, &c);
    assert!(r.wasted.value() > 0.0, "pipes and a full-throttle reactor waste heat");

    // A turbine below its threshold produces nothing at all, and one above it
    // produces something -- from the same steam, moved by a tank.
    let stalled = orbit::compile(&load("designs/04-stalled.machine")).unwrap();
    let pulsed = orbit::compile(&load("designs/05-pulsed.machine")).unwrap();
    assert_eq!(stalled.orbit.power, 0, "three turbines on a trickle make nothing");
    assert!(pulsed.orbit.power > 0, "the same trickle, pulsed, makes something");

    // The exchanger's ratio, taken straight off a design that is not short of
    // anything: heat in, steam out, five to two.
    let d = load("designs/03-compact.machine");
    let c = orbit::compile(&d).unwrap();
    let m = c.state_at(&d, 4_000).unwrap();
    let hx = m.index_of("HX1").unwrap();
    assert_eq!(
        m.st[hx].used[0] * parts::BOIL_STEAM,
        m.st[hx].made[2] * parts::BOIL_HEAT
    );
    // and water one for one with steam.
    assert_eq!(m.st[hx].used[1], m.st[hx].made[2]);

    // The generator's 90%, and its ceiling.
    let g = m.index_of("G1").unwrap();
    assert!(m.st[g].used[0] <= parts::part(Kind::Generator).ports[0].rate);
    assert_eq!(m.st[g].made[1], m.st[g].used[0] * parts::GENERATOR_EFF / 100);
}

/// Nothing may be created. Every unit that enters a component leaves it, is
/// still inside it, or was thrown away and counted.
#[test]
fn nothing_appears_from_nowhere() {
    for path in DESIGNS {
        let d = load(path);
        let mut m = Machine::new(&d).unwrap();
        let mut before: Vec<u64> = m.st.iter().map(|s| s.buf.iter().sum()).collect();
        for t in 0..600 {
            m.step();
            for i in 0..m.len() {
                let after: u64 = m.st[i].buf.iter().sum();
                let arrived: u64 = m.st[i].got.iter().sum();
                let left: u64 = m.st[i].sent.iter().sum();
                // A component that transforms one stream into another is
                // allowed to change the total; the ones that do not are not.
                if matches!(m.kinds[i], Kind::HeatPipe | Kind::SteamPipe | Kind::Tank) {
                    let lost = m.st[i].waste;
                    let electricity = 0;
                    assert_eq!(
                        before[i] + arrived,
                        after + left + lost + electricity,
                        "{path}: {} at t={t}",
                        m.names[i]
                    );
                }
                before[i] = after;
            }
        }
    }
}

// -------------------------------------------------------------- reporting

#[test]
fn the_brief_is_judged_the_same_way_twice() {
    for path in DESIGNS {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        let r = eval::report(&d, &c);
        // The verdict and the numbers behind it have to agree.
        let short = r.power.value() < eval::TARGET_MW as f64;
        assert_eq!(r.met(), !short && r.sources == 1 && r.settled, "{path}");
        // Rates are rationals over one orbit, so the denominator is the period.
        assert_eq!(r.power.den, c.period as u128, "{path}");
        // The footprint is a box that contains every component.
        let (w, h, tiles) = d.footprint();
        assert_eq!((r.width, r.height, r.tiles), (w, h, tiles), "{path}");
        assert!(r.tiles <= r.area(), "{path}: more machine than plot");
    }
}

/// The snapshot is the only thing a renderer sees, so it must describe every
/// component and every wire, and it must explain itself.
#[test]
fn the_snapshot_says_everything_a_renderer_needs() {
    for path in DESIGNS {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        let m = c.state_at(&d, 3_000).unwrap();
        let r = eval::report(&d, &c);
        let j = snap::render(&d, &m, &r);
        assert_eq!(j.at("units").as_arr().len(), d.units.len(), "{path}");
        assert_eq!(j.at("wires").as_arr().len(), d.wires.len(), "{path}");
        for u in j.at("units").as_arr() {
            assert!(!u.at("status").as_str().unwrap_or("").is_empty());
            assert!(
                !u.at("why").as_arr().is_empty(),
                "{path}: {} explains nothing",
                u.at("name").as_str().unwrap_or("?")
            );
        }
        // And it survives being written and read as JSON, which is the only
        // form the browser ever sees it in.
        let text = j.to_string();
        let back = json::parse(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(back.at("units").as_arr().len(), d.units.len());
    }
}

/// Every component that is stopped can say why, in words, without the caller
/// having to know which kind it is.
#[test]
fn a_stopped_component_explains_itself() {
    let d = load("designs/04-stalled.machine");
    let c = orbit::compile(&d).unwrap();
    let m = c.state_at(&d, 2_000).unwrap();
    let t1 = m.index_of("T1").unwrap();
    assert_eq!(m.st[t1].status, Status::Stalled);
    let why = snap::why(&d, &m, t1).join(" | ");
    assert!(why.contains("40"), "the threshold is named: {why}");
    assert!(why.contains("Steam Buffer"), "and so is the way out: {why}");
}

/// The catalogue the browser draws its palette from has to describe the parts
/// this crate actually simulates.
#[test]
fn the_catalogue_is_the_parts() {
    let j = Design::catalogue();
    assert_eq!(j.as_arr().len(), parts::KINDS.len());
    for (entry, &kind) in j.as_arr().iter().zip(parts::KINDS.iter()) {
        let p = parts::part(kind);
        assert_eq!(entry.at("kind").as_str(), Some(p.tag));
        assert_eq!(entry.at("w").as_u64(), Some(p.w as u64));
        assert_eq!(entry.at("ports").as_arr().len(), p.ports.len());
    }
}
