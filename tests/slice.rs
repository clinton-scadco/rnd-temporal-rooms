//! Experiment 15: three centuries of one valley, and the properties that make
//! them one factory problem rather than three games in a trench coat.
//!
//! ```text
//!   the land       one rectangle, three histories, and a number for the change
//!   the phases     what a century can make, and what a crate of it costs
//!   the fractures  what holds one open, and what it refuses when nothing does
//!   the provenance carrying is not making, and the queue that proves it
//!   the slice      the whole thing, played, with every replica compared
//!   the view       the JSON a front end that does not exist yet would read
//! ```
//!
//! The group that is the experiment is the fourth. Everything else here is a
//! guard rail around the claim in the brief -- *technology is constrained by
//! industrial capability and logistics, not by a magical era lock* -- and the
//! way to keep that claim honest in a test is to check both halves: that a
//! motor in 1890 is refused when nobody has shipped one, and that it is
//! *allowed* the moment somebody has.

use temporal_rooms::machine::design::{Design, Tune};
use temporal_rooms::machine::era::{Era, Mat};
use temporal_rooms::machine::parts;
use temporal_rooms::mp::cmd::Act;
use temporal_rooms::mp::goal::{Goal, Shape};
use temporal_rooms::slice::gate::{self, Crossing, Ledger, Stamped};
use temporal_rooms::slice::land::{self, Face, Layer};
use temporal_rooms::slice::phase::{self, Phase, PHASES};
use temporal_rooms::slice::play::{self, Play, HYDRO, WATERMILL};
use temporal_rooms::slice::region::{region, REGIONS};
use temporal_rooms::slice::run::Slice;

// ==================================================================== the land

/// Every region is the same rectangle, and every feature on it is at the same
/// coordinates in all three centuries.
///
/// The load-bearing test of the whole experiment. If this fails, the three
/// regions are three maps that happen to look alike, and nothing else here
/// means anything.
#[test]
fn one_rectangle_three_histories() {
    for r in REGIONS {
        let (w, _) = r.furnish();
        assert_eq!(w.plot(), land::PLOT, "{}: a plot of its own", r.tag);
    }
    // A feature is one row with three faces, so there is no way for a deposit to
    // be in one place in 1890 and another in 2037 -- but the *test* has to be
    // written against the resolved worlds rather than against the table, or it
    // is checking that a struct has three fields.
    for f in land::LAND {
        for p in PHASES {
            let Some((item, q)) = f.face(p).yields() else { continue };
            let Some((_, r)) = REGIONS.iter().enumerate().find(|(_, r)| r.phase == p) else {
                continue;
            };
            let (w, _) = r.furnish();
            let found = w
                .deposits
                .iter()
                .find(|d| d.x == f.x && d.y == f.y && d.item == item)
                .unwrap_or_else(|| panic!("{}: {} is not at {},{} in {p}", f.tag, item, f.x, f.y));
            assert_eq!(found.w, f.w, "{}: a different width in {p}", f.tag);
            assert_eq!(found.h, f.h, "{}: a different height in {p}", f.tag);
            assert_eq!(found.yields, q, "{}: a different yield in {p}", f.tag);
        }
    }
}

/// The test the brief actually asks for: a rich deposit in 1890, and in 2037
/// the same ground exhausted with a factory standing on part of it.
#[test]
fn the_ore_body_is_taken_and_then_built_over() {
    let reach = land::feature("kestrel").expect("Kestrel Reach");
    let spoil = land::feature("kestrel-spoil").expect("Kestrel Spoil");

    // 1890: rich, and both halves of it are open ground.
    assert!(matches!(reach.face(Phase::P1890), Face::Ore(q) if q >= 400));
    assert!(matches!(spoil.face(Phase::P1890), Face::Ore(_)));
    assert!(!reach.face(Phase::P1890).taken());

    // 2037: one half is under a works and cannot be built on; the other half is
    // open, still named, and worth almost nothing.
    assert!(reach.face(Phase::P2037).taken(), "nobody built on Kestrel Reach");
    assert_eq!(reach.face(Phase::P2037).yields(), None, "the body is still being worked");
    match spoil.face(Phase::P2037) {
        Face::Spent(item) => assert_eq!(item, "IronOre"),
        other => panic!("Kestrel Spoil should be a spent ore body, not {other:?}"),
    }
    let spent = spoil.face(Phase::P2037).yields().expect("a spent body still shows");
    assert!(spent.1 < 20, "a spent body worth {} a second is not spent", spent.1);

    // And the refusal a player meets says both of those things, in one sentence,
    // naming the century where the same tiles are open.
    let (_, district) = region("district").expect("the district");
    let why = district
        .built_over(reach.x, reach.y, 4, 4)
        .expect("building on a standing works should be refused");
    assert!(why.contains(reach.name), "the refusal does not say what is there: {why}");
    assert!(why.contains("1890"), "the refusal does not say when it was open: {why}");
    assert!(why.contains("iron ore"), "the refusal does not say what is under it: {why}");

    // The same rectangle in 1890 is free.
    let (_, valley) = region("valley").expect("the valley");
    assert!(valley.built_over(reach.x, reach.y, 4, 4).is_none());
}

/// Crossing a fracture changes the world, in more than one of the ways the
/// brief lists, and by enough of the plot to be noticed.
///
/// A number rather than an opinion, in the same spirit as experiment 09's
/// palette metric: "the world changes clearly" is a claim somebody can be
/// wrong about.
#[test]
fn a_crossing_changes_the_world_measurably() {
    let deep = land::changes(Phase::P1890, Phase::P2037);
    assert!(deep.pct() >= 15, "the deep fracture changes only {}% of the plot", deep.pct());
    // Vegetation, roads, buildings, resource state and infrastructure: five of
    // the six layers, which is what section 6 of the brief lists.
    for want in [Layer::Resource, Layer::Vegetation, Layer::Route, Layer::Building, Layer::Supply] {
        assert!(
            deep.by_layer.iter().any(|(l, n)| *l == want && *n > 0),
            "nothing changed in the {} layer",
            want.tag()
        );
    }
    // And the near corridor changes less, because thirty-three years is less
    // than a hundred and forty-seven.
    let near = land::changes(Phase::P2037, Phase::P2070);
    assert!(near.pct() < deep.pct(), "a shorter crossing changed more of the world");

    // The glyph maps are different fields, not the same field drawn twice.
    let a = land::glyphs(Phase::P1890);
    let b = land::glyphs(Phase::P2037);
    assert_ne!(a, b);
    assert_eq!(a.lines().count(), land::PLOT as usize);
    assert!(a.lines().all(|l| l.chars().count() == land::PLOT as usize));
}

// ================================================================= the phases

/// A phase is a date laid over experiment 14's table, so the components it
/// lacks are few, dated, and each one has a reason written down.
#[test]
fn the_centuries_differ_in_power_and_not_in_crushers() {
    // Every row of the arrivals table names a real component, once.
    for a in phase::ARRIVES {
        assert!(parts::by_tag(a.part).is_some(), "{} is not a component", a.part);
        assert!(!a.why.is_empty(), "{} arrives for no stated reason", a.part);
        assert_eq!(
            phase::ARRIVES.iter().filter(|b| b.part == a.part).count(),
            1,
            "{} is dated twice",
            a.part
        );
    }
    // Most of the catalogue is the same in every century, which is the claim.
    let shared = parts::KINDS.iter().filter(|k| Phase::P1890.builds(**k)).count();
    assert!(
        shared * 10 >= parts::KINDS.len() * 7,
        "only {shared} of {} components are as old as the catalogue",
        parts::KINDS.len()
    );
    // And there is exactly one crusher, in every century.
    let crusher = parts::by_tag("crusher").expect("a crusher");
    for p in PHASES {
        assert!(p.builds(crusher), "{p} has no crusher");
    }
    // What 1890 lacks is the electrical era and the two 2070 processes, and
    // nothing else.
    for k in parts::KINDS {
        if Phase::P1890.builds(k) {
            continue;
        }
        let a = phase::arrival(k.tag()).expect("something undated is missing from 1890");
        // Three kinds of absence, and no fourth. The electrical era; the two
        // 2070 processes; and the two components the catalogue calls `Era::Any`
        // because a wire is a wire and an element is an element -- both of which
        // presuppose a national supply, which is a fact about the century.
        assert!(
            k.era() == Era::Electric
                || a.at == Phase::P2070
                || matches!(k.tag(), "mains" | "heater"),
            "{} is missing from 1890 and is not electrical, a 2070 process, or a grid fitting",
            k.tag()
        );
    }
}

/// The material rule bites where experiment 14 made the frame a decision, and
/// nowhere else.
///
/// This is the test that would have caught the reading of `ONLY_STEEL` that
/// made an extraction head cost 9,600 gears in 1890.
#[test]
fn a_frame_is_a_phase_problem_only_where_it_is_a_decision() {
    for k in parts::KINDS {
        let offers = parts::phys(k).mats;
        for m in offers {
            let ok = phase::frame_ok(k, *m, Phase::P1890);
            if offers.len() < 2 {
                assert!(ok, "{} has no frame decision and is refused a {m} one", k.tag());
            } else {
                assert_eq!(
                    ok,
                    Phase::P1890.mats().contains(m),
                    "{} on {m} in 1890",
                    k.tag()
                );
            }
        }
    }
    // Concretely: a head is native to 1890 and costs nothing at all.
    let head = temporal_rooms::mp::world::head_design("IronOre").expect("an ore head");
    assert_eq!(phase::design_cost(&head, Phase::P1890), 0, "a head is not a 2037 machine");
    assert!(phase::native(&head, Phase::P1890).is_ok());
    // And a crusher on cast iron is native to 1890 where a steel one is not.
    let crusher = parts::by_tag("crusher").expect("a crusher");
    assert!(Phase::P1890.builds_on(crusher, Mat::CastIron));
    assert!(!Phase::P1890.builds_on(crusher, Mat::Steel));
}

/// The two designs this experiment added are what they claim to be: one native
/// to 1890 and one that has to be imported into it.
#[test]
fn the_water_mill_is_local_and_the_hydro_station_is_not() {
    let mill = Design::parse(WATERMILL).expect("the water powder line parses");
    assert!(mill.check().is_empty(), "the water powder line does not compile: {:?}", mill.check());
    assert_eq!(
        phase::design_cost(&mill, Phase::P1890),
        0,
        "the first era's own answer should cost it nothing"
    );
    // It is a water-era machine, and it says so by what is in it.
    assert!(mill.eras().contains(&Era::Water), "a water powder line with no water in it");
    // Every frame in it that 1890 had to choose is cast iron or timber.
    for u in &mill.units {
        assert!(
            phase::frame_ok(u.kind, u.tune.mat, Phase::P1890),
            "{} is on a frame 1890 cannot make",
            u.name
        );
    }

    let hydro = Design::parse(HYDRO).expect("the hydro station parses");
    assert!(hydro.check().is_empty(), "the hydro station does not compile: {:?}", hydro.check());
    // Two generators, and they are the only thing in it that is not local.
    let bill = phase::imported(&hydro, Phase::P1890);
    assert_eq!(bill.len(), 2, "the hydro station should import exactly two components");
    for (_, kind, cost) in &bill {
        assert_eq!(kind.tag(), "generator");
        assert_eq!(*cost, 1_600);
    }
    assert_eq!(phase::design_cost(&hydro, Phase::P1890), 3_200);
    // And in 2070 the same document is free, because 2070 makes generators.
    assert_eq!(phase::design_cost(&hydro, Phase::P2070), 0);
}

/// A crate is priced on distance in fractures, and exactly one thing cannot be
/// crated at all.
#[test]
fn a_lathe_costs_twice_as_much_in_1890_as_in_2037() {
    let lathe = parts::by_tag("lathe").expect("a lathe");
    let m = Tune::default_for(lathe).mat;
    let near = phase::import_cost(lathe, m, Phase::P2037);
    let far = phase::import_cost(lathe, m, Phase::P1890);
    assert!(near > 0 && far == near * 2, "{far} should be twice {near}");
    assert_eq!(phase::import_cost(lathe, m, Phase::P2070), 0);

    // The one wall that is a wall, and it is about the century rather than the
    // component: importing a grid connection is importing a wire with nothing on
    // the end of it.
    let mains = parts::by_tag("mains").expect("a grid connection");
    assert!(!phase::crateable(mains));
    assert_eq!(phase::import_cost(mains, Tune::default_for(mains).mat, Phase::P1890), 0);
    let mut d = Design::empty();
    d.units.push(temporal_rooms::machine::design::Unit {
        name: "M1".into(),
        kind: mains,
        x: 0,
        y: 0,
        z: 0,
        face: None,
        tune: Tune::default_for(mains),
    });
    assert!(phase::legal(&d, Phase::P1890).is_err(), "a mains in 1890 should be illegal");
    assert!(phase::legal(&d, Phase::P2037).is_ok());
    // Everything else that is missing is merely expensive.
    for k in parts::KINDS {
        if k.tag() == "mains" {
            continue;
        }
        let mut one = Design::empty();
        one.units.push(temporal_rooms::machine::design::Unit {
            name: "U1".into(),
            kind: k,
            x: 0,
            y: 0,
            z: 0,
            face: None,
            tune: Tune::default_for(k),
        });
        assert!(phase::legal(&one, Phase::P1890).is_ok(), "{} is walled off", k.tag());
    }
}

// ============================================================== the fractures

/// The rule the brief states, asked about all nine ordered pairs of phases
/// rather than about the two joins this map happens to have.
#[test]
fn same_phase_is_ordinary_and_every_other_crossing_is_not() {
    for a in PHASES {
        for b in PHASES {
            let (Some((_, ra)), Some((_, rb))) = (
                REGIONS.iter().enumerate().find(|(_, r)| r.phase == a),
                REGIONS.iter().enumerate().find(|(_, r)| r.phase == b),
            ) else {
                continue;
            };
            let c = gate::crossing(ra.tag, rb.tag);
            if a == b {
                assert_eq!(c, Crossing::Same, "{a} to {b} should need no interface");
                assert_eq!(a.displacement(b), 0);
            } else {
                assert_ne!(c, Crossing::Same, "{a} to {b} is not ordinary logistics");
                assert!(a.displacement(b) > 0);
            }
        }
    }
    // A lane that crosses nothing is a lane no interface is asked about, which
    // is the floor of the rule rather than a case this map exercises.
    for l in gate::LANES {
        let c = gate::crossing(l.from, l.to);
        assert_ne!(c, Crossing::None, "{} -> {} crosses nothing at all", l.from, l.to);
        assert_eq!(l.fracture().is_some(), c != Crossing::Same);
    }
}

/// What an interface draws and what it will pass are both functions of how far
/// out of its time the load is.
#[test]
fn a_wider_crossing_costs_more_and_carries_less() {
    let near = gate::draw(33);
    let deep = gate::draw(147);
    let through = gate::draw(180);
    assert!(near < deep && deep < through, "{near} {deep} {through}");
    assert!(gate::gauge(33) > gate::gauge(147));
    assert!(gate::gauge(147) > gate::gauge(180));
    // And the number the whole mechanic turns on: processing 1890 ore in 2037
    // and sending concentrate on is cheaper than sending the ore on raw.
    assert!(
        gate::draw(33) < gate::draw(180),
        "there would be no reason to process anything locally"
    );
    for f in gate::FRACTURES {
        assert!(f.gap() > 0, "{}: a fracture between two of the same century", f.tag);
        assert!(region(f.early).is_some() && region(f.late).is_some());
        assert!(region(f.held_by).is_some(), "{} is held open by nobody", f.tag);
    }
}

/// A fleet belongs to a century, and 1890 has carts.
#[test]
fn a_train_cannot_be_fielded_in_1890() {
    let mut s = Slice::open(5);
    s.start_manual();
    let ada = s.join("Ada").expect("a seat");
    let e = s
        .open_route(ada, "valley", "district", "IronOre", "train", None)
        .expect_err("a train out of 1890");
    assert!(e.contains("1890"), "{e}");
    assert!(e.contains("2037"), "the refusal should say when a train turns up: {e}");
    // And the cart it does have works.
    assert!(s.open_route(ada, "valley", "district", "IronOre", "tramway", None).is_ok());
    // Every fleet is fieldable by the phase it is dated to and by every later
    // one, which is what "modern logistics" is spent on here.
    for f in gate::FLEETS {
        for r in REGIONS {
            let ok = s.open_route(ada, r.tag, "district", "Coal", f.tag, None).is_ok()
                || gate::lane(r.tag, "district", "Coal").is_none();
            assert!(ok || r.phase < f.from, "{} could not field a {}", r.tag, f.tag);
        }
    }
}

/// An interface passes nothing while the grid behind it is short, and what was
/// already inside it still lands.
#[test]
fn a_dark_fracture_carries_nothing_and_loses_nothing() {
    let mut l = Ledger::new(0);
    // The corridor comes with an interface; the deep one does not.
    assert!(l.gate(0).is_none(), "the deep fracture should start bare");
    assert!(l.gate(1).is_some(), "the corridor should come already built");
    l.open_gate("deep", 0).expect("an interface on the deep fracture");
    assert!(l.open_gate("deep", 0).is_err(), "two interfaces on one fracture");

    // Nobody delivering anything: dark, and it says who was short.
    l.power(0, |_| 0);
    let g = l.gate(0).expect("the interface");
    assert!(!g.lit);
    let why = g.why_dark().expect("a dark interface explains itself");
    assert!(why.contains("dark") && why.contains("147"), "{why}");
    assert_eq!(g.want_mw, gate::draw(147));

    // Enough, and it holds.
    l.power(gate::SETTLE, |_| 10_000);
    assert!(l.gate(0).expect("the interface").lit);

    // A load already in the air lands whether or not the fracture is holding at
    // the moment it does.
    l.flight.push(gate::Load {
        route: 1,
        from: "valley",
        to: "district",
        item: "IronOre",
        at: gate::SETTLE,
        parcels: vec![Stamped { origin: Phase::P1890, qty: 1_000 }],
    });
    l.power(gate::SETTLE * 2, |_| 0);
    assert!(!l.gate(0).expect("the interface").lit);
    let landed = l.arrivals(gate::SETTLE * 2);
    assert_eq!(landed.len(), 1);
    assert_eq!(landed[0].qty(), 1_000);
}

// ============================================================= the provenance

/// Carrying is not making: a parcel keeps the century it came out of, however
/// many regions it passes through, and only a *different item* gets a new
/// stamp.
///
/// The queue rather than the world, so that the mechanic is checked rather than
/// the playthrough that exercises it.
#[test]
fn material_carried_through_a_century_keeps_its_own() {
    let mut b = gate::Bin::default();
    b.put(Stamped { origin: Phase::P1890, qty: 300 });
    b.put(Stamped { origin: Phase::P1890, qty: 200 });
    assert_eq!(b.held.len(), 1, "runs of one origin should merge");
    assert_eq!(b.total(), 500);
    b.put(Stamped { origin: Phase::P2070, qty: 100 });

    // Oldest first, and the origins come back with the quantities.
    let out = b.take(400);
    assert_eq!(out, vec![Stamped { origin: Phase::P1890, qty: 400 }]);
    let rest = b.take(1_000);
    assert_eq!(
        rest,
        vec![
            Stamped { origin: Phase::P1890, qty: 100 },
            Stamped { origin: Phase::P2070, qty: 100 },
        ]
    );
    assert_eq!(b.total(), 0);
    // And what it was ever sent is remembered even once it is empty, because
    // "what is this factory made of" is a question about history.
    assert_eq!(b.landed.get(&Phase::P1890).copied(), Some(500));
    assert_eq!(b.landed.get(&Phase::P2070).copied(), Some(100));

    // Displacement is measured against where it is going, not where it has been.
    let s = Stamped { origin: Phase::P1890, qty: 1 };
    assert_eq!(gate::displacement(s, Phase::P2037), 147);
    assert_eq!(gate::displacement(s, Phase::P2070), 180);
    assert_eq!(gate::displacement(s, Phase::P1890), 0);
}

/// A region's exports are drawn out of what it imported, so nothing is created
/// and nothing is laundered.
#[test]
fn a_region_ships_what_it_was_sent_before_what_it_made() {
    let mut l = Ledger::new(0);
    l.open_gate("deep", 0).expect("an interface");
    l.power(0, |_| 10_000);
    let id = l
        .open("district", "valley", "Gear", "train", Some(10_000), 0)
        .expect("a machinery run");

    // Ten thousand gears land in the district out of 2070. Put straight into the
    // bin the way an arrival would: `landed` also credits the route a load came
    // in on, and the route it came in on is not this one.
    l.bins
        .entry(("district".into(), "Gear".into()))
        .or_default()
        .put(Stamped { origin: Phase::P2070, qty: 10_000 });
    let mut counter = 0u64;
    let mut settle = |l: &mut Ledger, t, n: u64| {
        counter += n;
        l.power(t, |_| 10_000);
        l.dispatch(t, |tag, item| {
            if (tag, item) == ("district", "Gear") {
                counter
            } else {
                0
            }
        })
    };
    settle(&mut l, gate::SETTLE, 0);
    settle(&mut l, gate::SETTLE * 2, 6_000);

    // What is waiting to leave the district is 2070 machinery, because that is
    // what the district was sent. It did not become 2037 machinery by being
    // carried through 2037.
    let r = l.route(id).expect("the route");
    let waiting: Vec<Stamped> = r.hold.clone();
    assert!(!waiting.is_empty(), "nothing was loaded");
    for s in &waiting {
        assert_eq!(s.origin, Phase::P2070, "gears out of 2070 became something else");
    }
    assert_eq!(waiting.iter().map(|s| s.qty).sum::<u64>(), 6_000);

    // Ship more than was ever sent, and the surplus -- and only the surplus --
    // is the district's own.
    settle(&mut l, gate::SETTLE * 3, 9_000);
    let r = l.route(id).expect("the route");
    let by_origin = |p: Phase| -> u64 {
        r.hold.iter().filter(|s| s.origin == p).map(|s| s.qty).sum()
    };
    assert_eq!(by_origin(Phase::P2070), 10_000, "all the imports should go first");
    assert_eq!(by_origin(Phase::P2037), 5_000, "the rest is the district's own");
}

// ================================================================== the slice

/// Every region's objective is a template that exists, is authored rather than
/// rolled, and cannot turn up in a single-room game.
#[test]
fn every_region_names_a_goal_that_exists_and_is_not_rollable() {
    for r in REGIONS {
        let g = Goal::of_seed(1, Some(r.template));
        assert_eq!(g.template, r.template, "{}: the template is missing", r.tag);
        assert!(!g.brief().is_empty());
        // Authored, not rolled: the same region twice is the same problem.
        assert_eq!(g.shape, Goal::of_seed(9_999, Some(r.template)).shape);
        assert!(
            !temporal_rooms::mp::goal::template(r.template).expect("the template").rollable(),
            "{} could turn up on its own, without the other two centuries",
            r.tag
        );
    }
    // And the district's objective is one it *cannot* meet alone, which is the
    // whole reason the fracture is not scenery.
    let (_, district) = region("district").expect("the district");
    let g = Goal::of_seed(1, Some(district.template));
    let Shape::Sustain { per_sec, .. } = g.shape else { panic!("the district wants a rate") };
    assert!(
        district.yields("IronOre") < per_sec,
        "the district can feed its own crushing line, so 1890 is decoration"
    );
}

/// Every region furnishes itself on the shared plot without anything
/// overlapping, hanging off the edge, or standing where somebody else built.
#[test]
fn every_region_fits_on_the_shared_plot() {
    for r in REGIONS {
        let (w, ports) = r.furnish();
        assert_eq!(w.installs.len(), r.kit.len(), "{}: something would not fit", r.tag);
        for i in &w.installs {
            let (x0, y0, x1, y1) = i.bounds();
            assert!(x1 <= land::PLOT && y1 <= land::PLOT, "{}: {} hangs off", r.tag, i.name);
            assert!(
                r.built_over(x0, y0, x1 - x0, y1 - y0).is_none(),
                "{}: {} stands on something that was already there",
                r.tag,
                i.name
            );
        }
        for (k, a) in w.installs.iter().enumerate() {
            for b in w.installs.iter().skip(k + 1) {
                assert!(!a.overlaps(b), "{}: {} overlaps {}", r.tag, a.name, b.name);
            }
        }
        // A yard for everything that can arrive, a dock for everything that can
        // leave, and every one of them named by a lane.
        for item in ports.incoming.keys() {
            assert!(
                gate::LANES.iter().any(|l| l.to == r.tag && l.item == item),
                "{} can unload {item} and nothing carries any",
                r.tag
            );
        }
        for l in gate::LANES.iter().filter(|l| l.to == r.tag) {
            assert!(
                ports.incoming.contains_key(l.item),
                "{} is sent {} and has nowhere to put it",
                r.tag,
                l.item
            );
        }
        for l in gate::LANES.iter().filter(|l| l.from == r.tag) {
            assert!(
                ports.outgoing.contains_key(l.item),
                "{} is asked for {} and has no dock for it",
                r.tag,
                l.item
            );
        }
    }
}

/// The machinery ledger is a subtraction over state that already exists, so
/// taking a machine down gives its crates back.
#[test]
fn deleting_an_imported_machine_frees_its_machinery() {
    let mut s = Slice::open(11);
    s.start_manual();
    let ada = s.join("Ada").expect("a seat");
    let hydro = Design::parse(HYDRO).expect("the hydro station");
    let cost = phase::design_cost(&hydro, Phase::P1890);
    assert!(cost > 0);

    // Nothing has been shipped, so nothing may be stood up -- and the refusal
    // is a price rather than a locked node.
    let place = |x, y, d: Option<Design>| Act::PlaceMachine {
        proto: "turbinehall".into(),
        x,
        y,
        face: 0,
        item: None,
        design: d,
    };
    let e = s
        .submit(ada, "valley", place(0, 58, Some(hydro.clone())))
        .expect_err("a hydro station with no crates");
    assert!(e.contains("imported machinery"), "{e}");
    assert!(e.contains("3,200"), "the refusal should name the shortfall: {e}");

    // Pretend a train got through. The ledger is the only thing that has to
    // agree, because everything else is derived from it.
    s.ledger.landed(
        0,
        &[Stamped { origin: Phase::P2070, qty: 20_000 }],
        0,
        0,
    );
    // `landed` with an unknown route credits nothing, so put it where a landing
    // would: the region's own bin.
    s.ledger
        .bins
        .entry(("valley".into(), "Gear".into()))
        .or_default()
        .put(Stamped { origin: Phase::P2070, qty: 20_000 });
    assert_eq!(s.free_machinery("valley"), 20_000);

    s.submit(ada, "valley", place(0, 58, Some(hydro.clone())))
        .expect("the crates have landed");
    assert_eq!(s.free_machinery("valley"), 20_000 - cost as i64);

    // And taking it down gives them back, because the sum is one term shorter.
    let id = s
        .yard("valley")
        .and_then(|y| y.room.host.world.installs.last().map(|i| i.id))
        .expect("the station");
    s.submit(ada, "valley", Act::DeleteMachine { id }).expect("it is not a fixture");
    assert_eq!(s.free_machinery("valley"), 20_000);

    // Gears the valley made itself would not be machinery, which is why the
    // ledger asks about the origin rather than the item.
    assert_eq!(s.ledger.from_later("valley", "Gear", Phase::P2070), 0);
    assert_eq!(s.ledger.from_later("valley", "Gear", Phase::P1890), 20_000);
}

/// A region's fixtures are what the region is.
#[test]
fn a_dock_cannot_be_bulldozed() {
    let mut s = Slice::open(13);
    s.start_manual();
    let ada = s.join("Ada").expect("a seat");
    let fixtures = s.yard("valley").map(|y| y.ports.fixtures.clone()).unwrap_or_default();
    assert!(!fixtures.is_empty());
    for id in fixtures {
        let e = s
            .submit(ada, "valley", Act::DeleteMachine { id })
            .or_else(|_| s.submit(ada, "valley", Act::DeleteStorage { id }))
            .err()
            .expect("a fixture should not be removable");
        assert!(e.contains("cannot be removed"), "{e}");
    }
}

/// Nobody invents an arrival.
#[test]
fn nobody_can_deliver_to_themselves() {
    let mut s = Slice::open(17);
    s.start_manual();
    let ada = s.join("Ada").expect("a seat");
    let e = s
        .submit(
            ada,
            "district",
            Act::Deliver { to: 1, item: "IronOre".into(), qty: 9_000, from: "valley".into() },
        )
        .expect_err("a player-issued arrival");
    assert!(e.contains("not something a player does"), "{e}");
}

/// The whole slice, played: every region meets an objective it could not have
/// met alone, the ore in 2037 came out of 1890, the machinery in 1890 came out
/// of 2070, and every replica of every region agreed with its host throughout.
///
/// The acceptance test of the experiment. It runs the same script `slice play`
/// narrates, because a playthrough only the binary could run would be a
/// demonstration rather than a proof.
#[test]
fn the_slice_can_be_played_end_to_end() {
    let mut p = Play::quiet(3);
    let finished = play::run(&mut p);
    assert!(p.bad.is_empty(), "{} things went wrong: {:#?}", p.bad.len(), p.bad);
    assert!(finished, "the slice could not be played to the end");

    // Every region finished.
    for r in REGIONS {
        assert!(
            p.s.yard(r.tag).and_then(|y| y.done_at()).is_some(),
            "{} never met its objective",
            r.title
        );
    }
    assert!(p.s.complete());

    // The criterion: ore mined in 1890, crushed in 2037.
    let ore = p
        .s
        .ledger
        .bin("district", "IronOre")
        .and_then(|b| b.landed.get(&Phase::P1890).copied())
        .unwrap_or(0);
    assert!(ore > 0, "no 1890 ore reached 2037");
    let conc = p.s.yard("district").map(|y| y.shipped("Concentrate")).unwrap_or(0);
    assert!(conc > 0, "2037 crushed nothing");

    // The other half of the criterion: machinery, backwards, and still stamped
    // with the century that made it.
    let gears = p.s.ledger.bin("valley", "Gear").expect("gears reached 1890");
    assert_eq!(
        gears.landed.keys().copied().collect::<Vec<_>>(),
        vec![Phase::P2070],
        "the machinery in 1890 should all be 2070's, and nothing else's"
    );
    assert!(p.s.machinery("valley").1 > 0, "1890 stood nothing imported up");

    // Both fractures ended up holding, and the deep one widened when 2070
    // machinery started crossing it.
    for g in &p.s.ledger.gates {
        assert!(g.lit, "the {} fracture finished dark", g.fracture().tag);
        assert!(g.carried > 0, "the {} fracture carried nothing", g.fracture().tag);
    }
    let deep = p.s.ledger.gate(0).expect("the deep interface");
    assert!(
        deep.worst > gate::FRACTURES[0].gap(),
        "2070 machinery crossed the deep fracture without widening what it holds"
    );
    assert_eq!(deep.want_mw, gate::draw(deep.worst));

    // And the proof that has nothing to do with time travel: three regions,
    // two players, one clock, and every reconstruction identical.
    assert!(p.s.agrees(), "a replica disagreed with its host");
    assert!(p.checks > 50, "only {} synchronisation checks", p.checks);
}

/// A region nobody is standing in keeps running, and the slice advances the
/// same way whether it is polled once a minute or twelve times a second.
#[test]
fn a_region_runs_the_same_whether_anybody_is_watching() {
    let hash = |step: u64| -> (Option<u64>, u64) {
        let mut s = Slice::open(23);
        s.start_manual();
        let ada = s.join("Ada").expect("a seat");
        // One region gets built; the other two are left alone entirely.
        let ore = s
            .yard("valley")
            .and_then(|y| y.room.host.world.nth_ground("IronOre", 0).map(|d| (d.x, d.y)))
            .expect("Kestrel Reach");
        let head = s
            .submit(
                ada,
                "valley",
                Act::PlaceMachine {
                    proto: "head".into(),
                    x: ore.0,
                    y: ore.1,
                    face: 0,
                    item: None,
                    design: temporal_rooms::mp::world::head_design("IronOre").ok(),
                },
            )
            .map(|_| {
                s.yard("valley").and_then(|y| y.room.host.world.installs.last().map(|i| i.id))
            })
            .ok()
            .flatten()
            .expect("a head");
        let dock = s.yard("valley").and_then(|y| y.ports.outgoing.get("IronOre").copied()).unwrap();
        let bay = s
            .submit(ada, "valley", Act::PlaceStorage { proto: "yard".into(), x: 0, y: 58, face: 0 })
            .map(|_| {
                s.yard("valley").and_then(|y| y.room.host.world.installs.last().map(|i| i.id))
            })
            .ok()
            .flatten()
            .expect("a yard");
        s.submit(ada, "valley", Act::CreateConnection { from: head, to: bay, item: "IronOre".into() })
            .expect("a wire");
        s.submit(ada, "valley", Act::CreateConnection { from: bay, to: dock, item: "IronOre".into() })
            .expect("a wire");
        let mut t = 0;
        while t < 240 {
            t += step;
            s.set_now(temporal_rooms::mp::secs(t));
            s.advance().expect("the slice runs");
        }
        let probe = s.yard("valley").map(|y| y.room.host.probe()).unwrap_or(0);
        (
            s.yard("valley").and_then(|y| y.room.host.check(probe)),
            s.yard("valley").map(|y| y.shipped("IronOre")).unwrap_or(0),
        )
    };
    let fine = hash(1);
    let coarse = hash(60);
    assert_eq!(fine.1, coarse.1, "a coarsely polled valley mined a different amount");
    assert_eq!(fine.0, coarse.0, "a coarsely polled valley is a different factory");
    assert!(fine.1 > 0, "the valley mined nothing at all");
}

/// The slice serialises: every region, every fracture, the land, the ledger and
/// the provenance, in one object a front end could read.
///
/// There is no browser for this experiment, which makes the JSON exactly the
/// kind of public surface that rots quietly. So it is called against a slice
/// that has actually run, and the things it is *for* are checked rather than
/// its length.
#[test]
fn the_slice_serialises_what_a_front_end_would_need() {
    let mut p = Play::quiet(3);
    play::run(&mut p);
    let ada = p.s.cast.first().map(|c| c.id).expect("a seat");
    let j = p.s.to_json(ada).expect("the slice serialises");

    assert_eq!(j.at("ok").as_bool(), Some(true));
    assert_eq!(j.at("regions").as_arr().len(), REGIONS.len());
    assert_eq!(j.at("phases").as_arr().len(), PHASES.len());
    assert_eq!(j.at("crossings").as_arr().len(), gate::FRACTURES.len());
    assert_eq!(j.at("land").at("features").as_arr().len(), land::LAND.len());
    assert_eq!(j.at("land").at("plot").as_u64(), Some(land::PLOT as u64));

    for r in j.at("regions").as_arr() {
        let tag = r.at("tag").as_str().unwrap_or_default().to_string();
        assert!(region(&tag).is_some(), "an unknown region in the view: {tag}");
        assert!(r.at("phase").as_str().is_some(), "{tag} has no century");
        assert!(r.at("done").as_bool() == Some(true), "{tag} is not finished");
        assert!(!r.at("ground").as_arr().is_empty(), "{tag} has nothing in the ground");
        // The machinery ledger is what the valley's whole story is told in.
        let m = r.at("machinery");
        assert!(m.at("landed").as_u64().is_some(), "{tag} has no machinery figure");
    }

    // Each crossing carries the metric that makes "the world changes" checkable.
    for c in j.at("crossings").as_arr() {
        let ch = c.at("changes");
        assert!(ch.at("tiles").as_u64().unwrap_or(0) > 0);
        assert!(!ch.at("byLayer").as_arr().is_empty());
    }

    // And the provenance is in it, because "what is this made of" is the one
    // question this experiment exists to be able to answer.
    let prov = j.at("shipping").at("provenance").as_arr().to_vec();
    assert!(!prov.is_empty(), "nothing crossed, or nothing recorded that it had");
    assert!(
        prov.iter().any(|e| {
            e.at("region").as_str() == Some("valley")
                && e.at("item").as_str() == Some("Gear")
                && e.at("sentFrom")
                    .as_arr()
                    .iter()
                    .any(|f| f.at("phase").as_str() == Some("2070"))
        }),
        "the view cannot say that 1890's machinery came out of 2070"
    );
}
