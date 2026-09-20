//! Experiment 14: era machines and physical operating limits.
//!
//! The note behind this experiment states its own acceptance test, twice, and
//! both of them are here:
//!
//! > Can two valid machines perform the same function but have genuinely
//! > different layouts, power systems, thermal behaviour and failure modes?
//!
//! > Does upgrading from a water-wheel/line-shaft factory to electric motors
//! > feel like a change in industrial architecture, rather than a stat increase?
//!
//! Neither of those is a thing a test can feel, so what is asserted here is the
//! part underneath them that is countable: that the three shipped answers to one
//! brief share only their ore path, run on three different energy domains, stop
//! for three different reasons, and are not three spellings of one machine with
//! a multiplier on it.
//!
//! The other half of the file is the guard rail. Experiment 14 adds physics to
//! a crate that had thirteen experiments and eighteen designs measured without
//! it, and an experiment about thermal limits that silently re-scored all of
//! them would have proved nothing except that it had changed the subject. So:
//! every component that existed before this experiment settles inside its
//! operating range on the frame it comes on, and `the_older_designs_are_the_same
//! _machines` says what that buys.

use std::collections::BTreeSet;
use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::era::{Band, Era, Mat};
use temporal_rooms::machine::eval::{self, Brief};
use temporal_rooms::machine::parts::{self, Kind};
use temporal_rooms::machine::sim::{Machine, Status};
use temporal_rooms::machine::{orbit, snap};

/// The components that existed before experiment 14, minus the six of them
/// that were transport and went when the family did.
const BEFORE: usize = 32;

fn parse(src: &str) -> Design {
    let d = Design::parse(src).unwrap_or_else(|e| panic!("{e}"));
    let faults = d.check();
    assert!(faults.is_empty(), "{}", faults[0].what);
    d
}

fn load(path: &str) -> Design {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    parse(&src)
}

fn run(d: &Design, ticks: u64) -> Machine {
    let mut m = Machine::new(d).unwrap_or_else(|e| panic!("{e}"));
    for _ in 0..ticks {
        m.step();
    }
    m
}

/// How much one named component put out of one named port over a window.
///
/// A window rather than a tick, because a machine that trips, cools, restarts
/// and trips again has no representative tick: the whole shape of the failure
/// is that it is intermittent.
fn output(d: &Design, unit: &str, port: usize, ticks: u64) -> u64 {
    let mut m = Machine::new(d).unwrap();
    let i = at(&m, unit);
    let mut total = 0;
    for _ in 0..ticks {
        m.step();
        total += m.st[i].made[port];
    }
    total
}

fn at(m: &Machine, name: &str) -> usize {
    m.index_of(name).unwrap_or_else(|| panic!("no component called {name}"))
}

// ------------------------------------------------- the thing being claimed

/// The experiment's own acceptance test, in the only form a test can take it.
///
/// Three designs answer the `line` brief. All three meet it. What has to be
/// true for the experiment to have worked is that they are not the same machine
/// three times: different prime movers, different energy domains, different
/// costs, and a parts list that overlaps only where it should -- on the ore
/// path, which is the half that genuinely does not change between centuries.
#[test]
fn one_brief_three_machines() {
    let designs: Vec<(&str, Design)> = vec![
        ("water", load("designs/19-waterline.machine")),
        ("steam", load("designs/20-steamline.machine")),
        ("electric", load("designs/21-electricline.machine")),
    ];

    for (name, d) in &designs {
        assert_eq!(d.brief, Brief::Line, "{name}");
        let c = orbit::compile(d).unwrap();
        let r = eval::report(d, &c);
        assert!(r.met(), "{name} does not meet the brief: {:?}", r.failings);
    }

    // The prime movers are three different components in three different
    // domains, which is what "different power system" has to mean if it means
    // anything.
    let has = |d: &Design, k: Kind| d.count_of(k) > 0;
    assert!(has(&designs[0].1, Kind::WaterWheel));
    assert!(has(&designs[1].1, Kind::SteamEngine));
    assert!(has(&designs[2].1, Kind::Motor) && has(&designs[2].1, Kind::Mains));
    assert!(!has(&designs[0].1, Kind::Mains), "the first era has no grid");
    assert!(!has(&designs[1].1, Kind::Mains), "the second era has no grid");
    assert!(!has(&designs[2].1, Kind::WaterWheel), "the third era has no river");

    // The costs land in different columns. A water plant pays in river, a steam
    // plant pays in coal, an electric plant pays in metered power -- and each
    // pays nothing at all in the other two.
    let cost = |d: &Design| {
        let c = orbit::compile(d).unwrap();
        let r = eval::report(d, &c);
        (r.water.value(), r.fuel.value(), r.grid.value())
    };
    let (w_water, w_fuel, w_grid) = cost(&designs[0].1);
    let (s_water, s_fuel, s_grid) = cost(&designs[1].1);
    let (e_water, e_fuel, e_grid) = cost(&designs[2].1);
    assert!(w_water > 100.0 && w_fuel == 0.0 && w_grid == 0.0, "water pays in river");
    assert!(s_fuel > 0.0 && s_grid == 0.0 && s_water < w_water, "steam pays in coal");
    assert!(e_grid > 0.0 && e_fuel == 0.0 && e_water == 0.0, "electric pays in metering");

    // And the parts lists overlap only on the ore path.
    let kinds = |d: &Design| -> BTreeSet<Kind> { d.units.iter().map(|u| u.kind).collect() };
    let ore: BTreeSet<Kind> =
        [Kind::Inlet, Kind::Crusher, Kind::Outlet].into_iter().collect();
    for (i, (an, a)) in designs.iter().enumerate() {
        for (bn, b) in designs.iter().skip(i + 1) {
            let ka = kinds(a);
            let kb = kinds(b);
            let shared: BTreeSet<Kind> = ka.intersection(&kb).copied().collect();
            assert!(
                shared.is_superset(&ore),
                "{an} and {bn} should both have the whole ore path"
            );
            let drive: Vec<Kind> = shared.difference(&ore).copied().collect();
            assert!(
                drive.len() <= 2,
                "{an} and {bn} share too much of their drive train: {drive:?}"
            );
            assert!(ka != kb, "{an} and {bn} are the same parts list");
        }
    }
}

/// The failure this experiment is written against.
///
/// If the eras had been done the easy way there would be a `crusher`, a
/// `crusher2` and a `crusher3`, or a crusher with a tier on it. There is one
/// crusher, one mill and one press, and the way to notice a second one
/// appearing is to refuse to let any component's name be another's with
/// something stuck on the end.
#[test]
fn there_is_exactly_one_of_each_machine() {
    for &a in parts::KINDS.iter() {
        for &b in parts::KINDS.iter() {
            if a == b {
                continue;
            }
            let (x, y) = (a.tag(), b.tag());
            assert!(
                !y.starts_with(x),
                "`{y}` looks like a second `{x}` -- that is the Mk2 this experiment refuses"
            );
        }
    }
    // And the process family, which is where a tier would show up first, is
    // era-neutral with exactly one exception. A crusher, a mill, a press and a
    // rolling mill are the same objects in any century and are filed under
    // none. A CNC lathe is not: it is a twentieth-century machine that does not
    // have an eighteenth-century equivalent, and pretending otherwise to make a
    // rule come out tidy would be the same dishonesty as a Crusher Mk2 in the
    // other direction.
    for &k in parts::KINDS.iter() {
        if k.family() == parts::Family::Process && k != Kind::Lathe {
            assert_eq!(
                parts::phys(k).era,
                Era::Any,
                "{} is filed under a century, and process machinery should not be",
                k.tag()
            );
        }
    }
    assert_eq!(parts::phys(Kind::Lathe).era, Era::Electric, "and the exception is the lathe");
}

// --------------------------------------------------------- the first era

/// A crusher shakes at 7 and a timber wheel carries 4, so the obvious drive
/// train -- bolt the crusher straight to the wheel -- pulls the mill apart.
/// The belt is what makes the first era buildable, and what it does is
/// *topological*: it breaks the rigid cluster in two.
///
/// It used to be a five-tile component and is now one word at the end of a
/// wire, and this test is the argument for the change in its shortest form.
/// The two documents below differ by four characters.
#[test]
fn a_timber_drive_will_not_carry_a_crusher() {
    let src = |belt: &str| {
        format!(
            r#"
machine "Drive"
brief line
pump       P1  at 0,0   draws water
waterwheel W1  at 4,0
crusher    C1  at 4,6   frame iron
inlet      I1  at 9,6   draws ore
outlet     O1  at 0,6
wire P1.water -> W1.water
wire W1.rotary -> C1.drive{belt}
wire I1.out -> C1.in
wire C1.out -> O1.solid
"#
        )
    };

    let bolted = parse(&src(""));
    let m = run(&bolted, 200);
    let w1 = at(&m, "W1");
    assert_eq!(m.shake[w1], 7, "the crusher's shake reaches the wheel");
    assert_eq!(m.st[w1].status, Status::Shaking, "and the wheel driving it stops");
    // The crusher itself is fine -- cast iron carries exactly 7 -- and that is
    // the point worth being precise about. It is not the crusher that fails,
    // it is everything the crusher is bolted to, and the crusher then starves
    // because the drive it was waiting on has stopped.
    assert_eq!(m.shake[at(&m, "C1")], 7);
    assert_eq!(m.st[at(&m, "C1")].status, Status::Starved);
    assert_eq!(m.st[at(&m, "C1")].made[2], 0, "and the machine crushes nothing at all");

    let why = snap::why(&bolted, &m, w1).join(" ");
    assert!(why.contains("SHAKING"), "it says so: {why}");
    assert!(why.contains("belt"), "and names the way out: {why}");

    // The same machine with the drive wire marked `belt`, and nothing else
    // changed at all -- not a component, not a tile, not a frame.
    let belted = parse(&src("  belt"));
    assert_eq!(belted.units.len(), bolted.units.len(), "a belt is not a component");
    let m2 = run(&belted, 200);
    for i in 0..m2.len() {
        assert_ne!(m2.st[i].status, Status::Shaking, "{} still shaking", m2.names[i]);
    }
    assert_eq!(m2.shake[at(&m2, "W1")], 2, "the wheel now only carries itself");
    assert_eq!(m2.shake[at(&m2, "C1")], 7, "and the crusher only carries itself");
    assert_eq!(m2.st[at(&m2, "C1")].status, Status::Running, "and it runs");
    assert!(m2.st[at(&m2, "C1")].made[2] > 0, "and ore comes out of it");
}

/// A frame is a choice with a wrong answer, and the document says so before
/// anything is simulated.
#[test]
fn a_frame_has_to_be_something_the_component_comes_in() {
    let d = Design::parse(
        "machine \"Timber Press\"\nbrief gears\npress PR1 at 0,0 frame wood\n",
    )
    .unwrap();
    let faults = d.check();
    assert_eq!(faults.len(), 1);
    assert!(faults[0].what.contains("wood"), "{}", faults[0].what);
    assert!(faults[0].what.contains("steel"), "{}", faults[0].what);
}

// -------------------------------------------------------- the second era

/// The centre of the experiment: an engine that cannot be built without a
/// cooling decision.
///
/// The same design twice, differing by one wire. Without it the engine reaches
/// its ceiling, trips, cools, restarts and trips again, and the plant runs at a
/// fraction of what it should. With it the engine sits in the middle of its
/// range and the plant runs.
#[test]
fn an_engine_with_nowhere_to_put_its_heat_cooks() {
    // The engine is deliberately given enough to do. An engine at thirty
    // percent of rating settles at 86 degrees and is a perfectly happy engine;
    // the three pumps and the crusher here take everything it makes, and a
    // fully loaded engine is the one with the thermal problem.
    const PLANT: &str = r#"
machine "Engine"
brief line
inlet       F1  at 0,0   draws coal
burner      BU1 at 4,0
pump        P1  at 0,4   draws water
exchanger   HX1 at 4,4
exchanger   HX2 at 8,4
steamengine SE1 at 4,8
skip        SK1 at 0,8
mechpump    MP1 at 9,8   frame iron
mechpump    MP2 at 12,8  frame iron
mechpump    MP3 at 9,11  frame iron
mechpump    MP4 at 12,11 frame iron
skip        SK2 at 15,8
gearbox     GB1 at 4,11  ratio 2
crusher     C1  at 0,11
inlet       I1  at 0,15  draws ore
outlet      O1  at 4,15
wire F1.out -> BU1.fuel
wire BU1.heat -> HX1.heat
wire BU1.heat -> HX2.heat
wire P1.water -> HX1.water
wire P1.water -> HX2.water
wire HX1.steam -> SE1.steam
wire HX2.steam -> SE1.steam
wire SE1.condensate -> SK1.liquid
wire SE1.exhaust -> SK1.vapour
wire SE1.rotary -> MP1.drive
wire SE1.rotary -> MP2.drive
wire SE1.rotary -> MP3.drive
wire SE1.rotary -> MP4.drive
wire MP1.water -> SK2.liquid
wire MP2.water -> SK2.liquid
wire MP3.water -> SK2.liquid
wire MP4.water -> SK2.liquid
wire SE1.rotary -> GB1.in
wire GB1.out -> C1.drive
wire I1.out -> C1.in
wire C1.out -> O1.solid
"#;

    let bare = parse(PLANT);
    let m = run(&bare, 1_500);
    let se = at(&m, "SE1");
    assert_eq!(m.clear[se], 0, "this engine has been wedged against its gearbox");
    let ph = parts::phys(Kind::SteamEngine);
    assert!(
        ph.settles_at(Mat::CastIron, 0) > ph.ceiling(Mat::CastIron),
        "on air alone this engine is above its own ceiling before anything is run"
    );

    // It trips, cools, restarts and trips again, so what is asserted is the
    // *cycle* rather than the state at one arbitrary tick. Which half of the
    // cycle tick 1500 lands in is a fact about the arithmetic of the whole
    // plant, and pinning it here made this test fail the first time anything
    // upstream of the engine changed by one unit.
    let mut tripped = 0;
    let mut m2 = Machine::new(&bare).unwrap();
    let mut worst = 0;
    for _ in 0..1_500 {
        m2.step();
        if m2.st[se].tripped {
            tripped += 1;
        }
        worst = worst.max(m2.temp(se));
    }
    assert!(tripped > 0, "an uncooled engine trips on temperature");
    assert!(
        tripped * 4 > 1_500,
        "an uncooled engine should spend most of its life tripped, and this one spent {tripped} ticks of fifteen hundred"
    );
    assert!(
        worst >= ph.ceiling(Mat::CastIron),
        "and it should have reached its own ceiling to get there: {worst}"
    );

    // And at whichever point it is tripped, it says so in the words a player
    // can act on.
    let hot = (0..1_500)
        .scan(Machine::new(&bare).unwrap(), |mm, _| {
            mm.step();
            Some(mm.clone())
        })
        .find(|mm| mm.st[se].status == Status::Overheated)
        .expect("it trips");
    assert_eq!(hot.duty(se), 0);
    assert_eq!(hot.st[se].status, Status::Overheated);
    let why = snap::why(&bare, &hot, se).join(" ");
    assert!(why.contains("OVERHEATED"), "it says so: {why}");
    assert!(why.contains("waste port is not wired"), "and what is missing: {why}");

    // One more wire: a radiator on the waste port. Same plant, same layout,
    // same clearance -- one component and one connection between a machine
    // that runs and a machine that cooks.
    let cooled = parse(&format!(
        "{PLANT}radiator RD1 at 9,14\nwire SE1.waste -> RD1.heat\n"
    ));
    let m3 = run(&cooled, 1_500);
    let se3 = at(&m3, "SE1");
    assert_eq!(m3.clear[se3], 0, "and it is still wedged in");
    assert!(!m3.st[se3].tripped, "but with a radiator it stays in range");
    assert_eq!(m3.band(se3), Band::Normal, "at {} degrees", m3.temp(se3));
    let cold = output(&cooled, "C1", 2, 3_000);
    let cooked = output(&bare, "C1", 2, 3_000);
    assert!(
        cold > cooked,
        "and the plant crushes more: {cold} against {cooked} over three thousand ticks"
    );
}

/// Waste heat is worth something, and it is deliberately not worth very much.
///
/// A jacket will take it. An exchanger will not, and that is the one number in
/// this experiment chosen to close a loophole rather than to model anything: an
/// engine whose own waste heat could raise the steam that drives it is a
/// perpetual motion machine with a nice diagram.
#[test]
fn waste_heat_is_low_grade_on_purpose() {
    use temporal_rooms::machine::era;
    assert_eq!(era::grade(0), 0, "a cold body has nothing to give");
    assert_eq!(era::grade(era::GRADE_MIN), 1);
    assert_eq!(era::grade(10_000), era::WASTE_GRADE_MAX, "and it never gets better");

    // What each of the two consumers will accept, read off the table rather
    // than asserted by hand.
    let need_of = |k: Kind, port: &str| -> u8 {
        let p = parts::part(k);
        let i = p.port_index(port).unwrap();
        let r = p.recipe.unwrap();
        let d = r.draws.iter().find(|d| d.port == i).unwrap();
        d.need
            .iter()
            .filter_map(|n| match n {
                parts::Need::MinTemp(t) => Some(*t),
                _ => None,
            })
            .max()
            .unwrap_or(0)
    };
    assert!(
        need_of(Kind::Jacket, "heat") <= era::WASTE_GRADE_MAX,
        "a jacket takes what an engine gives off"
    );
    assert!(
        need_of(Kind::Exchanger, "heat") > era::WASTE_GRADE_MAX,
        "and an exchanger does not, which is what stops the loop closing on itself"
    );

    // In the shipped steam plant, the jacket is running on it.
    let d = load("designs/20-steamline.machine");
    let m = run(&d, 2_500);
    let jk = at(&m, "JK1");
    assert!(m.st[jk].made[2] > 0, "the jacket is returning warmed feedwater");
    assert!(
        m.st[jk].buf[2].stuff.q.temp > m.st[at(&m, "SE1")].buf[2].stuff.q.temp,
        "and it is warmer than the condensate that went in"
    );
}

// --------------------------------------------------------- the whole model

/// Temperature is exact; behaviour is not. Between two thresholds nothing
/// about a component changes, which is the whole reason the bands exist.
#[test]
fn behaviour_changes_only_at_thresholds() {
    let ph = parts::phys(Kind::SteamEngine);
    let m = Mat::CastIron;
    let top = ph.ceiling(m);
    let mut seen: Vec<(Band, u64)> = Vec::new();
    for t in 0..=top + 20 {
        let b = ph.band(t, m);
        if seen.last().map(|(x, _)| *x) != Some(b) {
            seen.push((b, b.duty()));
        }
    }
    let bands: Vec<Band> = seen.iter().map(|(b, _)| *b).collect();
    assert_eq!(
        bands,
        vec![Band::Cold, Band::Normal, Band::Warm, Band::Hot, Band::Overheated],
        "five bands, in order, and each of them entered exactly once"
    );
    let duties: Vec<u64> = seen.iter().map(|(_, d)| *d).collect();
    assert_eq!(duties, vec![600, 1000, 950, 750, 0], "and these are the numbers");

    // A component with no operating floor has no cold band at all: a crusher is
    // not worse in the morning.
    let cr = parts::phys(Kind::Crusher);
    assert_eq!(cr.lo, 0);
    assert_eq!(cr.band(0, Mat::Steel), Band::Normal);
}

/// A frame is three decisions at once, and every one of them has a use.
#[test]
fn a_material_is_a_real_trade() {
    let mats = [Mat::Wood, Mat::CastIron, Mat::Steel];
    for w in mats.windows(2) {
        assert!(w[0].cond() < w[1].cond(), "better material conducts better");
        assert!(w[0].ceiling() < w[1].ceiling(), "and stands more heat");
        assert!(w[0].tol() < w[1].tol(), "and carries more shake");
    }
    // Which is why the choice is only interesting because the catalogue says
    // which components may have it. Everything is not available in everything.
    let choices: usize =
        parts::KINDS.iter().filter(|&&k| parts::phys(k).mats.len() > 1).count();
    assert!(choices >= 12, "only {choices} components offer a frame choice");
    assert!(
        choices < parts::KINDS.len(),
        "and not everything does -- a timber column is not a trade-off"
    );
    for &k in parts::KINDS.iter() {
        let ph = parts::phys(k);
        assert_eq!(ph.mats[0], ph.mat, "{}: the default is not first in the list", k.tag());
    }
}

/// Spacing is the cooling option that costs plot and nothing else, which is why
/// it is the one that makes the tile grid mean something it did not mean before.
#[test]
fn air_around_a_machine_is_cooling() {
    let ph = parts::phys(Kind::SteamEngine);
    let m = Mat::CastIron;
    let tight = ph.settles_at(m, 0);
    let roomy = ph.settles_at(m, temporal_rooms::machine::era::CLEAR_MAX);
    assert!(roomy * 2 <= tight + 2, "four clear tiles is worth about a halving");
    assert!(
        ph.cooled_at(m, 0) < ph.settles_at(m, 0),
        "and a wired waste port is worth more than any amount of air"
    );

    // Measured rather than asserted: one motor doing exactly the same work,
    // with its neighbours moved away from it and nothing else changed.
    let plan = |gap: i32| {
        format!(
            "machine \"Spacing\"\nbrief line\n\
             mains M1 at 0,0\n\
             motor MO1 at {},0\n\
             generator G1 at {},0\n\
             wire M1.power -> MO1.power\n\
             wire MO1.rotary -> G1.rotary\n",
            2 + gap,
            4 + 2 * gap
        )
    };
    let packed = run(&parse(&plan(0)), 800);
    let spaced = run(&parse(&plan(4)), 800);
    let mo = |m: &Machine| m.index_of("MO1").unwrap();
    assert_eq!(m_util(&packed, mo(&packed)), m_util(&spaced, mo(&spaced)), "same work");
    assert!(m_util(&packed, mo(&packed)) > 0, "and it is actually working");
    assert!(
        packed.temp(mo(&packed)) > spaced.temp(mo(&spaced)),
        "packed {} should be hotter than spaced {}",
        packed.temp(mo(&packed)),
        spaced.temp(mo(&spaced))
    );
}

fn m_util(m: &Machine, i: usize) -> u32 {
    m.st[i].util
}

// ------------------------------------------------------- the guard rail

/// The promise experiment 14 made to the thirteen before it.
///
/// Every component that existed before this experiment settles inside its
/// operating range on the frame it comes on, with no clearance at all and at
/// full output. So `duty` is one thousand per mille for all of them in every
/// design written before this file existed, and the arithmetic of experiments
/// 06 to 13 is bit-for-bit what it was. The thermal model is not switched off
/// for them -- it is running, and it is telling the truth, and the truth is
/// that an idealised steel crusher in still air runs warm rather than hot.
#[test]
fn the_older_designs_are_the_same_machines() {
    for &k in parts::KINDS.iter().take(BEFORE) {
        let ph = parts::phys(k);
        if !ph.thermal() {
            continue;
        }
        let settles = ph.settles_at(ph.mat, 0);
        assert!(
            settles <= ph.hi,
            "{} settles at {settles} against a range that ends at {} -- \
             that would re-score every design that uses one",
            k.tag(),
            ph.hi
        );
        assert_eq!(ph.band(settles, ph.mat), Band::Normal, "{}", k.tag());
    }

    // And nothing in the eighteen older designs is shaking or derated.
    for path in std::fs::read_dir("designs").unwrap().flatten() {
        let p = path.path();
        if p.extension().is_none_or(|x| x != "machine") {
            continue;
        }
        let name = p.to_string_lossy().into_owned();
        let d = load(&name);
        if d.brief == Brief::Line {
            continue;
        }
        let m = run(&d, 2_000);
        for i in 0..m.len() {
            assert_eq!(
                m.duty(i),
                1000,
                "{name}: {} is derated, and no design older than experiment 14 should be",
                m.names[i]
            );
            assert_ne!(m.st[i].status, Status::Shaking, "{name}: {}", m.names[i]);
            assert_ne!(m.st[i].status, Status::Overheated, "{name}: {}", m.names[i]);
        }
    }
}

/// A body temperature is state, and `orbit` compiles a design by watching its
/// state repeat, so a body temperature has to be the kind of thing that can
/// repeat. It is an integer, it is bounded, and every design on disk still
/// settles into an exact periodic orbit.
#[test]
fn a_warm_machine_still_settles() {
    for path in std::fs::read_dir("designs").unwrap().flatten() {
        let p = path.path();
        if p.extension().is_none_or(|x| x != "machine") {
            continue;
        }
        let name = p.to_string_lossy().into_owned();
        let d = load(&name);
        let c = orbit::compile(&d).unwrap();
        assert!(c.settled(), "{name} never repeated itself");
        assert!(
            c.transient + c.period < orbit::SEARCH,
            "{name}: {} + {}",
            c.transient,
            c.period
        );
    }
}

/// The physical table is indexed by the enum's own discriminant, which is fast,
/// free, and silently wrong the first time somebody inserts a row in the middle.
#[test]
fn the_physical_table_is_in_order() {
    assert_eq!(parts::KINDS.len(), 37);
    for (i, &k) in parts::KINDS.iter().enumerate() {
        assert_eq!(k as usize, i);
        let ph = parts::phys(k);
        assert_eq!(ph.kind, k, "the physical table is out of order at {}", k.tag());
        if ph.thermal() {
            assert!(ph.lo <= ph.hi, "{}: an operating range that is not one", k.tag());
            assert!(ph.hi < ph.max, "{}: a range above its own ceiling", k.tag());
        }
        assert!(ph.vib <= 9, "{}", k.tag());
        assert!(ph.torque <= 9, "{}", k.tag());
    }
    // Only the components that do work carry a body, and there is one with a
    // waste port, because there is one component whose waste heat is a decision.
    let waste: Vec<&str> =
        parts::KINDS.iter().filter(|k| k.waste_port().is_some()).map(|k| k.tag()).collect();
    assert_eq!(waste, vec!["steamengine"]);
}
