//! Experiments 06 and 07, held to the same standard as the rest of the crate.
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
//! Experiment 07 put two more claims underneath it. A quantity on a wire is now
//! a substance with properties rather than a number, so *nothing may be created*
//! has to be checked per substance and not per total; and a component is now a
//! row in a table, so the table itself is something that can be wrong -- a row
//! in the wrong place is a power cable that carries ore.
//!
//! The rest is the boring, essential kind: a file that round-trips, a rule that
//! refuses what it says it refuses, and components that each do the one thing
//! the table says they do.

use temporal_rooms::json;
use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::parts::{self, Kind};
use temporal_rooms::machine::sim::{Machine, Status, Tick, Totals};
use temporal_rooms::machine::stuff::{Subst, FORM_GEAR, SIZE_POWDER};
use temporal_rooms::machine::{eval, orbit, snap};

/// Experiment 06's six, which are still judged by experiment 06's brief and
/// must still get experiment 06's numbers.
const DESIGNS: &[&str] = &[
    "designs/01-first-try.machine",
    "designs/02-more-of-everything.machine",
    "designs/03-compact.machine",
    "designs/04-stalled.machine",
    "designs/05-pulsed.machine",
    "designs/06-radial.machine",
];

/// Experiment 07's, one or two per brief.
const KIT: &[&str] = &[
    "designs/07-crushline.machine",
    "designs/08-stamping.machine",
    "designs/09-machining.machine",
    "designs/10-refinery.machine",
    "designs/11-steamcrusher.machine",
    "designs/12-onemotor.machine",
];

/// Everything on disk, which is what the general claims are made about.
fn every() -> Vec<&'static str> {
    DESIGNS.iter().chain(KIT.iter()).copied().collect()
}

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
    for path in every() {
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
    for path in every() {
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
    for path in every() {
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
    assert_eq!(b.water() - a.water(), c.orbit.water());
    assert_eq!(b.fuel() - a.fuel(), c.orbit.fuel());
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
    for path in every() {
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
    for path in every() {
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
    for path in every() {
        let d = load(path);
        let re = Design::parse(&d.emit()).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(d.emit(), re.emit(), "{path}: the file is not stable");
        assert_eq!(plod(&d, 1_500).1, plod(&re, 1_500).1, "{path}: it is not the same machine");
    }
}

#[test]
fn a_design_survives_the_wire() {
    for path in every() {
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
    // Experiment 06 refused this because `power` is a boundary port. Experiment
    // 07 allows a boundary port to be wired -- a generator may run a motor
    // inside the same machine -- so the only thing left wrong with it is that
    // electricity is not gas, and that is what it should say.
    refuses(
        "machine \"x\"\ngenerator G1 at 0,0\nturbine T1 at 4,0\nwire G1.power -> T1.steam\n",
        "carries electrical",
    );
    assert!(
        Design::parse("machine \"x\"\ngenerator G1 at 0,0\nmotor MO1 at 4,0\nwire G1.power -> MO1.power\n")
            .unwrap()
            .check()
            .is_empty(),
        "a generator powering a motor inside the same machine is a design"
    );

    // The new document rules, each of which is a number a player can type.
    refuses("machine \"x\"\ngearbox GB1 at 0,0 ratio 40\n", "outside -8..8");
    refuses("machine \"x\"\ncolumn CO1 at 0,0 stages 9\n", "stages");
    refuses("machine \"x\"\npump W1 at 0,0 draws ore\n", "inlet");
    refuses(
        "machine \"x\"\ncrusher C1 at 0,0\nmill MI1 at 5,0\nwire C1.out -> MI1.drive\n",
        "carries material",
    );
}

/// The part table is indexed by the enum's own discriminant, which is fast and
/// silently wrong the first time a row is inserted in the wrong place.
#[test]
fn the_parts_table_is_in_order() {
    for (i, &kind) in parts::KINDS.iter().enumerate() {
        assert_eq!(kind as usize, i, "KINDS is not in enum order at {i}");
        let p = parts::part(kind);
        assert_eq!(p.kind, kind, "the table is out of order at {}", p.tag);
        assert!(!p.tag.is_empty() && !p.blurb.is_empty(), "{} says nothing", p.tag);
        assert!(!p.ports.is_empty(), "{} has no ports", p.tag);
        assert_eq!(parts::by_tag(p.tag), Some(kind), "{} cannot be looked up", p.tag);
        // Every recipe has to point at ports this component actually has, in
        // the right direction, or it would panic the first time one is placed.
        if let Some(r) = p.recipe {
            assert!(r.rate > 0, "{} makes nothing", p.tag);
            assert!(r.floor <= r.rate, "{}: a floor above the rate never runs", p.tag);
            for d in r.draws {
                assert!(d.port < p.ports.len(), "{}: a draw off the end", p.tag);
                assert_eq!(
                    p.ports[d.port].dir,
                    parts::Dir::In,
                    "{}: drawing from an output",
                    p.tag
                );
                assert!(d.qty > 0, "{}: a draw of nothing", p.tag);
            }
            for m in r.makes {
                assert!(m.port < p.ports.len(), "{}: a make off the end", p.tag);
                assert_eq!(
                    p.ports[m.port].dir,
                    parts::Dir::Out,
                    "{}: making into an input",
                    p.tag
                );
                assert!(m.from == parts::MADE || m.from < r.draws.len(), "{}: a make from nowhere", p.tag);
            }
            // A batch has to fit: the rate must be reachable without the port
            // being the thing that stops it on the first tick.
            for d in r.draws {
                assert!(
                    p.ports[d.port].cap >= d.qty * r.rate,
                    "{}: {} cannot hold one tick of what it draws",
                    p.tag,
                    p.ports[d.port].name
                );
            }
        }
    }
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
    assert_eq!(tot.fuel(), parts::REACTOR_FUEL as u128 * parts::WARMUP as u128);
    let (m, _) = plod(&d, parts::WARMUP + 5);
    assert_eq!(m.st[0].status, Status::Venting);
    assert_eq!(m.st[0].made[0] + m.st[0].waste, parts::REACTOR_HEAT);

    // A pump with nowhere to send water fills its own tank and stops, so the
    // water it has drawn is bounded by its capacity rather than by the clock.
    let d = Design::parse("machine \"w\"\npump W1 at 0,0\n").unwrap();
    let (m, tot) = plod(&d, 500);
    assert_eq!(m.st[0].status, Status::Blocked);
    assert_eq!(tot.water(), parts::part(Kind::Pump).ports[0].cap as u128);

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
    for path in every() {
        let d = load(path);
        let mut m = Machine::new(&d).unwrap();
        let held = |m: &Machine, i: usize| -> u64 { m.st[i].buf.iter().map(|b| b.qty).sum() };
        let mut before: Vec<u64> = (0..m.len()).map(|i| held(&m, i)).collect();
        for t in 0..600 {
            m.step();
            for i in 0..m.len() {
                let after: u64 = held(&m, i);
                let arrived: u64 = m.st[i].got.iter().sum();
                let left: u64 = m.st[i].sent.iter().sum();
                let shipped: u64 = m.st[i].shipped.iter().sum();
                // A component that transforms one stream into another is
                // allowed to change the total; the ones that only move things
                // are not.
                if matches!(
                    m.kinds[i],
                    Kind::HeatPipe
                        | Kind::SteamPipe
                        | Kind::FluidPipe
                        | Kind::Chute
                        | Kind::Shaft
                        | Kind::Cable
                        | Kind::Tank
                        | Kind::Drum
                        | Kind::Hopper
                        | Kind::Flywheel
                        | Kind::Valve
                        | Kind::Clutch
                ) {
                    let lost = m.st[i].waste;
                    assert_eq!(
                        before[i] + arrived,
                        after + left + lost + shipped,
                        "{path}: {} at t={t}",
                        m.names[i]
                    );
                }
                before[i] = after;
            }
        }
    }
}

/// The same claim, one level up and per substance, which is the version
/// experiment 07 actually needs: a machine may turn ore into powder, but the
/// powder has to have been ore.
///
/// Iron ore does not appear from nowhere, and neither does the water in a pipe.
/// What crosses the boundary in must account for everything sitting inside plus
/// everything that crossed out plus everything thrown away -- counted per
/// substance, because a design that quietly turned tailings into concentrate
/// would balance perfectly by weight.
#[test]
fn matter_is_conserved_per_substance() {
    for path in every() {
        let d = load(path);
        let mut m = Machine::new(&d).unwrap();
        let mut took: Vec<(Subst, u64)> = Vec::new();
        let mut gave: Vec<(Subst, u64)> = Vec::new();
        let mut lost: Vec<(Subst, u64)> = Vec::new();
        let add = |v: &mut Vec<(Subst, u64)>, s: Subst, n: u64| {
            match v.iter_mut().find(|(k, _)| *k == s) {
                Some((_, q)) => *q += n,
                None => v.push((s, n)),
            }
        };
        for _ in 0..800 {
            let delta = m.step();
            for (s, n) in &delta.took {
                add(&mut took, s.subst, *n);
            }
            for (s, n) in &delta.gave {
                add(&mut gave, s.subst, *n);
            }
            for (s, n) in &delta.lost {
                add(&mut lost, s.subst, *n);
            }
        }
        // What is still inside, by substance.
        let mut inside: Vec<(Subst, u64)> = Vec::new();
        for i in 0..m.len() {
            for b in &m.st[i].buf {
                if b.qty > 0 {
                    add(&mut inside, b.stuff.subst, b.qty);
                }
            }
        }
        // Only the substances nothing in the kit consumes. A crusher changes
        // an ore's size, a mill changes it again, a separator splits it in two
        // and a furnace changes its temperature -- and not one of them changes
        // how much iron ore there is. Coal, water and crude are deliberately
        // not on this list: a burner turns coal into heat, a turbine turns
        // steam into rotation, and a column turns crude into three other
        // things. Those are conversions, and a conversion is allowed to be the
        // end of a substance.
        for subst in [Subst::Ore, Subst::Iron] {
            let of = |v: &Vec<(Subst, u64)>| {
                v.iter().find(|(k, _)| *k == subst).map(|(_, q)| *q).unwrap_or(0)
            };
            assert_eq!(
                of(&took),
                of(&inside) + of(&gave) + of(&lost),
                "{path}: {subst} does not add up -- {} drawn, {} inside, {} shipped, {} thrown away",
                of(&took),
                of(&inside),
                of(&gave),
                of(&lost)
            );
        }
    }
}

// -------------------------------------------------------------- reporting

#[test]
fn the_brief_is_judged_the_same_way_twice() {
    for path in every() {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        let r = eval::report(&d, &c);
        // The verdict and the numbers behind it have to agree, for whichever
        // of the four briefs this design was written against.
        let all_met = r.scored.iter().all(|s| s.met);
        let one_source = !d.brief.one_source() || r.sources == 1;
        assert_eq!(r.met(), all_met && one_source && r.settled, "{path}");
        // Every target is judged against its own number, and nothing else.
        for s in &r.scored {
            assert_eq!(
                s.met,
                s.got.num >= s.need * s.got.den,
                "{path}: {} says {} of {}",
                s.label,
                s.got.value(),
                s.need
            );
        }
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
    for path in every() {
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
    assert!(why.contains("Gas Buffer"), "and so is the way out: {why}");
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

// ------------------------------------------------------- experiment 07

/// The claim the whole experiment rests on: one set of components, four
/// different questions, and an answer to each.
///
/// If this ever fails because a brief has no design that meets it, the
/// vocabulary has stopped being a vocabulary.
#[test]
fn every_brief_has_an_answer() {
    let mut met: Vec<eval::Brief> = Vec::new();
    for path in every() {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        if eval::report(&d, &c).met() && !met.contains(&d.brief) {
            met.push(d.brief);
        }
    }
    for b in eval::BRIEFS {
        assert!(met.contains(&b), "nothing on disk answers the {} brief", b.tag());
    }
}

/// And that the answers are not the same machine twice. Two designs against the
/// same brief that share a component list are one design with the furniture
/// moved.
#[test]
fn one_brief_has_two_genuinely_different_answers() {
    let a = load("designs/07-crushline.machine");
    let b = load("designs/11-steamcrusher.machine");
    assert_eq!(a.brief, b.brief);
    let ra = eval::report(&a, &orbit::compile(&a).unwrap());
    let rb = eval::report(&b, &orbit::compile(&b).unwrap());
    assert!(ra.met() && rb.met(), "both meet the brief");
    // One draws from the grid and burns nothing; the other burns fuel and
    // touches no grid at all. Neither dominates.
    assert!(ra.grid.value() > 0.0 && rb.grid.value() == 0.0);
    assert!(ra.fuel.value() == 0.0 && rb.fuel.value() > 0.0);
    let kinds = |d: &Design| {
        let mut v: Vec<&str> =
            d.units.iter().map(|u| u.kind.tag()).collect::<std::collections::BTreeSet<_>>()
                .into_iter().collect();
        v.sort();
        v
    };
    assert_ne!(kinds(&a), kinds(&b), "and they are not the same parts list");
}

/// A recipe is a transformation of properties, not of items. The outer game
/// still has one thing called Iron Ore all the way down this chain.
#[test]
fn the_line_changes_properties_and_not_names() {
    let d = load("designs/07-crushline.machine");
    let c = orbit::compile(&d).unwrap();
    let m = c.state_at(&d, 4_000).unwrap();

    let out_of = |name: &str, port: &str| {
        let i = m.index_of(name).unwrap();
        let pi = parts::part(m.kinds[i]).port_index(port).unwrap();
        m.st[i].buf[pi].stuff
    };
    let inlet = out_of("I1", "out");
    let crushed = out_of("C2", "out");
    let powder = out_of("MI1", "out");
    let rich = out_of("S1", "rich");
    let tails = out_of("S1", "tails");

    // One substance from end to end.
    for s in [inlet, crushed, powder, rich, tails] {
        assert_eq!(s.subst, Subst::Ore, "the whole line is iron ore");
    }
    // And a different one of its properties at every stage.
    assert!(crushed.q.size > inlet.q.size, "a crusher makes it finer");
    assert_eq!(powder.q.size, SIZE_POWDER, "a mill finishes the job");
    assert!(rich.q.purity > powder.q.purity, "a separator concentrates it");
    assert!(tails.q.purity < powder.q.purity, "and what is left is poorer");
    assert!(rich.q.purity >= 80, "which is what the brief asked for");
}

/// The refusals, which are the mechanic rather than the error case. Each of
/// these is a design that is wired correctly, is short of nothing, and produces
/// nothing -- and each must say which property was wrong.
#[test]
fn a_component_refuses_the_wrong_property_and_says_which() {
    // A crusher driven straight off a motor: the right amount of rotary,
    // turning far too fast.
    let fast = "machine \"fast\"\nbrief crush\n\
                mains M1 at 0,0\nmotor MO1 at 3,0\ncrusher C1 at 6,0\n\
                inlet I1 at 6,5 draws ore\noutlet O1 at 11,0\n\
                wire M1.power -> MO1.power\nwire MO1.rotary -> C1.drive\n\
                wire I1.out -> C1.in\nwire C1.out -> O1.solid\n";
    let d = Design::parse(fast).unwrap();
    assert!(d.check().is_empty(), "{:?}", d.check());
    let c = orbit::compile(&d).unwrap();
    let m = c.state_at(&d, 500).unwrap();
    let i = m.index_of("C1").unwrap();
    assert_eq!(m.st[i].status, Status::Refused, "a crusher will not be spun fast");
    assert_eq!(m.st[i].made[2], 0, "and it makes nothing at all, not less");
    let why = snap::why(&d, &m, i).join(" | ");
    assert!(why.contains("speed"), "it names the property: {why}");
    assert!(why.contains("gearbox"), "and the way out: {why}");

    // The same machine with a gearbox in the middle. One component, and the
    // whole lesson of the crush brief stated as a diff.
    let geared = "machine \"geared\"\nbrief crush\n\
                  mains M1 at 0,0\nmotor MO1 at 3,0\ngearbox GB1 at 6,0\n\
                  crusher C1 at 9,0\ninlet I1 at 9,5 draws ore\noutlet O1 at 14,0\n\
                  wire M1.power -> MO1.power\nwire MO1.rotary -> GB1.in\n\
                  wire GB1.out -> C1.drive\nwire I1.out -> C1.in\n\
                  wire C1.out -> O1.solid\n";
    let d2 = Design::parse(geared).unwrap();
    assert!(d2.check().is_empty(), "{:?}", d2.check());
    let c2 = orbit::compile(&d2).unwrap();
    let r2 = eval::report(&d2, &c2);
    assert!(
        r2.gives.iter().any(|s| s.what.subst == Subst::Ore && s.rate.value() > 0.0),
        "geared down, the same machine crushes ore"
    );
    let m2 = c2.state_at(&d2, 500).unwrap();
    let i2 = m2.index_of("C1").unwrap();
    assert_ne!(m2.st[i2].status, Status::Refused, "and nothing is refused any more");
}

/// A rolling mill will not touch cold metal, which is a property arriving
/// wrong rather than an amount arriving short.
#[test]
fn cold_metal_is_refused_rather_than_rolled() {
    let src = "machine \"cold\"\nbrief gears\n\
               inlet I1 at 0,0 draws iron\nmains M1 at 0,4\nmotor MO1 at 3,4\n\
               rollmill R1 at 4,0\n\
               wire I1.out -> R1.in\nwire M1.power -> MO1.power\n\
               wire MO1.rotary -> R1.drive\n";
    let d = Design::parse(src).unwrap();
    assert!(d.check().is_empty(), "{:?}", d.check());
    let c = orbit::compile(&d).unwrap();
    let m = c.state_at(&d, 400).unwrap();
    let i = m.index_of("R1").unwrap();
    assert_eq!(m.st[i].status, Status::Refused);
    let why = snap::why(&d, &m, i).join(" | ");
    assert!(why.contains("hotter"), "it says what is wrong: {why}");
    assert!(why.contains("furnace"), "and what fixes it: {why}");
}

/// A phase change is a change of domain. Iron past its melting point leaves a
/// furnace through a different port, on a wire a rolling mill cannot accept.
#[test]
fn melting_moves_a_material_into_the_fluid_domain() {
    let furnace = parts::part(Kind::Furnace);
    let solid = furnace.port_index("out").unwrap();
    let molten = furnace.port_index("molten").unwrap();
    assert_eq!(furnace.ports[solid].dom.tag(), "material");
    assert_eq!(furnace.ports[molten].dom.tag(), "fluid");

    // One pass through a furnace leaves iron hot and solid; two passes take it
    // past melting, and it comes out of the other port.
    let src = "machine \"melt\"\nbrief gears\n\
               inlet I1 at 0,0 draws iron\n\
               reactor R1 at 0,4\n\
               furnace F1 at 4,0\nfurnace F2 at 9,0\n\
               wire I1.out -> F1.in\nwire R1.heat -> F1.heat\nwire R1.heat -> F2.heat\n\
               wire F1.out -> F2.in\n";
    let d = Design::parse(src).unwrap();
    assert!(d.check().is_empty(), "{:?}", d.check());
    let c = orbit::compile(&d).unwrap();
    let m = c.state_at(&d, 900).unwrap();
    let f1 = m.index_of("F1").unwrap();
    let f2 = m.index_of("F2").unwrap();
    assert!(m.st[f1].buf[solid].qty > 0, "one pass: hot, and still solid");
    assert_eq!(m.st[f1].buf[molten].qty, 0);
    assert!(m.st[f2].buf[molten].qty > 0, "two passes: molten, and a fluid");
    assert!(
        m.st[f2].buf[molten].stuff.q.temp >= Subst::Iron.melt(),
        "past iron's melting point"
    );
    // And the wire that would carry it is refused by anything that wants a solid.
    let bad = src.to_string() + "rollmill RM1 at 14,0\nwire F2.molten -> RM1.in\n";
    let faults = Design::parse(&bad).unwrap().check();
    assert!(
        faults.iter().any(|f| f.what.contains("carries fluid")),
        "molten iron will not plug into a rolling mill: {:?}",
        faults.iter().map(|f| &f.what).collect::<Vec<_>>()
    );
}

/// A press does not run slowly. Below its floor it does nothing at all, and
/// the strokes that reached it are gone rather than queued.
#[test]
fn a_press_below_its_floor_makes_nothing() {
    let d = load("designs/12-onemotor.machine");
    let c = orbit::compile(&d).unwrap();
    let r = eval::report(&d, &c);
    let full = load("designs/08-stamping.machine");
    let rf = eval::report(&full, &orbit::compile(&full).unwrap());
    // Half the drive is not half the gears.
    assert!(
        r.headline().value() * 2.0 < rf.headline().value(),
        "18 against 49, not 25 against 49"
    );
    assert!(!r.met(), "and it misses the brief");
    assert!(r.wasted.value() > 0.0, "with strokes falling on nothing");

    // Somewhere in the orbit the press is stopped for want of strokes, and says
    // so in the words that explain the mechanic.
    let mut found = String::new();
    for t in c.transient..(c.transient + c.period) {
        let m = c.state_at(&d, t).unwrap();
        let i = m.index_of("P1").unwrap();
        if m.st[i].status == Status::Stalled {
            found = snap::why(&d, &m, i).join(" | ");
            break;
        }
    }
    assert!(found.contains("does not run slowly"), "it explains itself: {found}");
    assert!(found.contains("flywheel"), "and names a way out: {found}");
}

/// Two substances will not share a port. A pipe full of water does not accept
/// crude, and the component says which port is holding what.
#[test]
fn one_port_holds_one_substance() {
    let src = "machine \"mixed\"\nbrief distil\n\
               pump P1 at 0,0\npump P2 at 0,4 draws crude\n\
               fluidpipe FP1 at 4,2\n\
               wire P1.water -> FP1.in\nwire P2.water -> FP1.in\n";
    let d = Design::parse(src).unwrap();
    assert!(d.check().is_empty(), "the document is legal; the physics is not");
    let mut m = Machine::new(&d).unwrap();
    for _ in 0..50 {
        m.step();
    }
    let fp = m.index_of("FP1").unwrap();
    let held = m.st[fp].buf[0].stuff.subst;
    assert!(held == Subst::Water || held == Subst::Crude, "it holds one of them");
    // And the other one never got in: the pipe's contents are all one substance.
    let all_one = m.st[fp].buf.iter().filter(|b| b.qty > 0).all(|b| b.stuff.subst == held);
    assert!(all_one, "a pipe carries one thing at a time");
}

/// A gearbox trades speed for the ability to turn something heavy, both ways,
/// and never leaves the band range.
#[test]
fn a_gearbox_is_a_ratio_in_both_directions() {
    use temporal_rooms::machine::sim::geared;
    assert_eq!(geared(6, 4), 1, "geared down");
    assert_eq!(geared(6, 2), 3);
    assert_eq!(geared(6, 1), 6, "straight through");
    assert_eq!(geared(3, -2), 6, "geared up");
    assert_eq!(geared(6, -8), 9, "and clamped to the top band");
    assert_eq!(geared(0, -8), 0);
}

/// The blend rule, which decides what a buffer holds when two lots of the same
/// substance arrive. It has to be weighted, deterministic, and stay inside the
/// set of values a state key can be equal to.
#[test]
fn blending_is_weighted_and_stays_whole() {
    use temporal_rooms::machine::stuff::{Qual, Stuff};
    let cold = Stuff::with(Subst::Water, Qual { temp: 0, ..Default::default() });
    let hot = Stuff::with(Subst::Water, Qual { temp: 8, ..Default::default() });
    assert_eq!(Stuff::blend(cold, 1, hot, 1).q.temp, 4, "half and half");
    assert_eq!(Stuff::blend(cold, 3, hot, 1).q.temp, 2, "three to one");
    assert_eq!(Stuff::blend(cold, 0, hot, 5), hot, "nothing plus something");
    assert_eq!(Stuff::blend(hot, 5, cold, 0), hot, "something plus nothing");
    // Deterministic, whichever way round it is asked, when the amounts match.
    assert_eq!(
        Stuff::blend(cold, 2, hot, 2).q.temp,
        Stuff::blend(hot, 2, cold, 2).q.temp
    );
}

/// The experiment's own acceptance test, as a number rather than an opinion.
///
/// It does not assert that everything is reused -- ten components are not used
/// by any shipped design and that is a finding, not a bug. What it asserts is
/// the thing the note asked for: that the infrastructure a machine is *built*
/// out of, as opposed to the process step it exists to perform, turns up across
/// more than one brief.
#[test]
fn the_infrastructure_primitives_span_more_than_one_brief() {
    let designs: Vec<Design> = every().iter().map(|p| load(p)).collect();
    let uses = eval::reuse(&designs);
    for tag in ["reactor", "pump", "exchanger", "turbine", "shaft", "motor", "outlet"] {
        let u = uses
            .iter()
            .find(|u| u.kind.tag() == tag)
            .unwrap_or_else(|| panic!("no such component: {tag}"));
        assert!(
            u.designs >= 2,
            "{tag} appears in {} designs, so it has not been shown to be a primitive",
            u.designs
        );
    }
    let across: usize = uses.iter().filter(|u| u.briefs.len() > 1).count();
    assert!(across >= 8, "only {across} components appear in more than one brief");
}

/// Gears are gears whichever machine made them, and the brief says so by
/// property rather than by name.
#[test]
fn two_machines_make_the_same_gear() {
    for path in ["designs/08-stamping.machine", "designs/09-machining.machine"] {
        let d = load(path);
        let c = orbit::compile(&d).unwrap();
        let r = eval::report(&d, &c);
        assert!(r.met(), "{path} meets the gear brief");
        let gear = r
            .gives
            .iter()
            .find(|s| s.what.subst == Subst::Iron && s.what.q.form == FORM_GEAR)
            .unwrap_or_else(|| panic!("{path} shipped no gears"));
        assert!(gear.rate.value() >= 20.0, "{path}: {} a tick", gear.rate.value());
    }
}
