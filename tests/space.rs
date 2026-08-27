//! Experiment 10, held to its own claims.
//!
//! The note that asked for experiment 10 finished with the question the whole
//! thing is for:
//!
//! > Can a player manipulate this industrial scene directly, and does the
//! > procedural system reliably turn their functional 3D decisions into
//! > geometry that makes physical visual sense?
//!
//! That splits into five claims, and this file is the five of them:
//!
//! ```text
//!   1  the document has three axes and a rotation, and they are the player's
//!   2  a port is an interface: it has a face, a class, a stub and an axis
//!   3  the router keeps its own rules, or says it could not
//!   4  space is scarce, and the plant says who is standing in whose way
//!   5  building upwards has consequences the player did not have to draw
//! ```
//!
//! Claim 3 is the load-bearing one, and it is the one worth stating carefully.
//! Experiment 08's router could not fail: when A* found nothing it drew a
//! straight line through the plant and moved on. So "every wire has a route"
//! was true and meaningless. Now the rules are real -- a minimum straight off
//! every flange, a minimum straight between every pair of bends, a bend radius
//! that fits -- and the honest consequence is that a run can be refused. What
//! is checked here is that the rules hold *on every run that was laid*, and
//! that a refused one is drawn as nothing at all rather than as an apology.

use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::form::layout::{Arch, Press, Side};
use temporal_rooms::machine::form::route::Tier;
use temporal_rooms::machine::form::space::Verdict;
use temporal_rooms::machine::form::{self, Ask, Style, GRADES};
use temporal_rooms::machine::parts::Kind;
use temporal_rooms::machine::stuff::Domain;
use temporal_rooms::machine::{eval, orbit};

fn design(path: &str) -> Design {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Design::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn all_designs() -> Vec<(String, Design)> {
    let mut names: Vec<String> = std::fs::read_dir("designs")
        .expect("designs/")
        .flatten()
        .map(|e| e.path().to_string_lossy().into_owned())
        .filter(|p| p.ends_with(".machine"))
        .collect();
    names.sort();
    names.into_iter().map(|n| (n.clone(), design(&n))).collect()
}

fn built(d: &Design) -> form::Scene {
    form::build(d, Ask { style: Style::Yard, world: 0, grade: form::Grade::Full })
        .expect("it builds")
}

// ------------------------------------------- 1. the document has three axes

/// `at x,y,z` and `face` survive the file, and mean what they say.
#[test]
fn the_third_tile_is_part_of_the_document() {
    let d = design("designs/17-stacked.machine");
    assert!(d.units.iter().any(|u| u.z > 0), "the stacked design does not stack anything");
    assert!(d.units.iter().any(|u| u.face.is_some()), "and it turns nothing");

    // Through the file and back, twice, because the second pass is the one
    // that proves the emitter and the parser agree rather than merely both
    // being wrong.
    let once = Design::parse(&d.emit()).expect("it re-reads");
    let twice = Design::parse(&once.emit()).expect("and again");
    assert_eq!(d.emit(), once.emit(), "the file is not a fixed point");
    assert_eq!(once.emit(), twice.emit());
    for (a, b) in d.units.iter().zip(once.units.iter()) {
        assert_eq!((a.x, a.y, a.z, a.face), (b.x, b.y, b.z, b.face), "{} moved", a.name);
    }

    // And through the wire, which is the other way the document travels.
    let json = Design::from_json(&d.to_json()).expect("it survives JSON");
    for (a, b) in d.units.iter().zip(json.units.iter()) {
        assert_eq!((a.x, a.y, a.z, a.face), (b.x, b.y, b.z, b.face), "{} moved", a.name);
    }
}

/// Sharing a footprint is a stack; sharing it at the same height is not.
///
/// This is the rule that makes the third axis worth having, and the one the
/// browser keeps its own copy of, so it is worth being exact about.
#[test]
fn two_components_may_share_tiles_if_they_do_not_share_a_height() {
    let mut d = design("designs/03-compact.machine");
    let (hx1, hx2) = (d.index_of("HX1").unwrap(), d.index_of("HX2").unwrap());
    let (x, y) = (d.units[hx1].x, d.units[hx1].y);

    // Straight on top of it: a collision, in two dimensions and in three.
    d.units[hx2].x = x;
    d.units[hx2].y = y;
    d.units[hx2].z = 0;
    assert!(
        d.check().iter().any(|f| f.what.contains("overlaps")),
        "two components on the same tiles at the same height is not a fault"
    );

    // Lifted clear of it: a design.
    d.units[hx2].z = d.units[hx1].tall();
    assert!(
        !d.check().iter().any(|f| f.what.contains("overlaps")),
        "stacking is refused, which is the one thing experiment 10 exists to allow: {:?}",
        d.check().iter().map(|f| f.what.clone()).collect::<Vec<_>>()
    );

    // And it is still close enough to be wired, because reach counts upwards.
    assert!(d.check().is_empty(), "{:?}", d.check().iter().map(|f| &f.what).collect::<Vec<_>>());
    assert!(form::build(&d, Ask::default()).is_ok(), "and it builds");
}

/// Turning a component turns its footprint. Only an authored turn: an inferred
/// one is the visual pipeline's opinion about which way a machine points, and
/// a document that reshaped itself because somebody drew a wire on the far
/// side would be a document editing itself.
#[test]
fn an_authored_turn_turns_the_footprint() {
    let mut d = design("designs/03-compact.machine");
    let i = d.index_of("HX1").unwrap();
    let (w, h) = (d.units[i].w(), d.units[i].h());
    d.units[i].face = Some(1);
    assert_eq!((d.units[i].w(), d.units[i].h()), (h, w), "a quarter turn did not turn it");
    d.units[i].face = Some(2);
    assert_eq!((d.units[i].w(), d.units[i].h()), (w, h), "a half turn is not a quarter turn");
    d.units[i].face = None;
    assert_eq!((d.units[i].w(), d.units[i].h()), (w, h));
}

// ------------------------------------------------- 2. a port is an interface

/// Every socket in every design carries the five things the note asked a port
/// to carry, and each of them is consistent with the machine it is on.
#[test]
fn every_port_carries_its_interface() {
    for (path, d) in all_designs() {
        let plan = form::layout::plan(&d);
        for u in &plan.units {
            for s in &u.sockets {
                assert!(s.out.is_axis(), "{path}: {}'s socket faces {}", u.name, s.out);
                assert!(s.bore >= 160, "{path}: {} has a {}mm bore", u.name, s.bore);
                assert!(
                    s.stub >= 400 && s.stub <= 1600,
                    "{path}: {}'s stub is {}mm",
                    u.name,
                    s.stub
                );
                assert_eq!(
                    s.class,
                    Press::of(s.dom, temporal_rooms::machine::parts::part(u.kind).ports[s.port].rate),
                    "{path}: {}'s class disagrees with its own rate",
                    u.name
                );
                assert_eq!(
                    s.layer,
                    form::layout::Layer::of(s.dom),
                    "{path}: {} is not on its domain's storey",
                    u.name
                );
                // A rotary port is a shaft, and a shaft has an axis. Nothing
                // else does, because nothing else has to line up.
                assert_eq!(
                    s.axis.is_some(),
                    matches!(s.dom, Domain::Rotary | Domain::Mech),
                    "{path}: {}'s {} port and its axis disagree",
                    u.name,
                    s.dom
                );
                if let Some(axis) = s.axis {
                    assert_eq!(
                        axis,
                        form::layout::face(u.yaw),
                        "{path}: {}'s shaft is off its own axis",
                        u.name
                    );
                }
                // And every socket is on a face the archetype allows.
                let allowed = form::layout::nozzle(u.arch, s.dom, s.dir);
                assert!(
                    allowed.iter().any(|side| side.world(u.yaw) == s.out),
                    "{path}: {}'s {} port is on a face a {} does not have one on",
                    u.name,
                    s.dom,
                    u.arch.tag()
                );
            }
        }
    }
}

/// The one nozzle rule the note called out by name: a shaft leaves the end of
/// the machine, so two machines coupled together are two machines pointing the
/// same way.
#[test]
fn a_shaft_leaves_the_end_of_the_barrel() {
    for (path, d) in all_designs() {
        let plan = form::layout::plan(&d);
        for u in plan.units.iter().filter(|u| {
            matches!(u.arch, Arch::Can | Arch::Wheel | Arch::Skid | Arch::Turbine | Arch::Portal)
        }) {
            for s in u.sockets.iter().filter(|s| matches!(s.dom, Domain::Rotary | Domain::Mech)) {
                let along = form::layout::face(u.yaw);
                assert!(
                    s.out == along || s.out == along.neg(),
                    "{path}: {}'s shaft leaves by {} and the machine faces {along}",
                    u.name,
                    s.out
                );
            }
        }
    }
}

// ------------------------------------------- 3. the router keeps its own rules

/// Every run that was laid obeys the note's own list, on every design there is.
///
/// The two that matter are the first two. A pipe that turns the instant it
/// leaves a flange looks like a mistake because it is one, and a pipe that
/// bends twice in a metre is a staircase -- which is exactly what experiment
/// 08's cell-by-cell A* produced, and exactly what walking straight sections
/// makes unrepresentable.
#[test]
fn a_laid_run_obeys_the_rules_it_was_laid_under() {
    for (path, d) in all_designs() {
        let plan = form::layout::plan(&d);
        let s = built(&d);
        for r in s.routes.iter().filter(|r| r.laid()) {
            let rule = form::route::rules(r.dom, r.bore);

            // Orthogonal, still and always.
            for i in 1..r.path.len() {
                assert!(
                    r.path[i].sub(r.path[i - 1]).is_axis(),
                    "{path}: {} has a diagonal section",
                    r.name
                );
            }

            // Off the flange, along the flange, for as far as the port asked.
            //
            // Only where there is a bend to keep clear of. A run with two
            // points in it is a coupling -- two flanges facing each other on
            // one axis -- and the rule it has to keep is that it is straight,
            // which it is by having two points in it.
            if r.path.len() == 2 {
                continue;
            }
            for (end, near) in [(0usize, 1usize), (r.path.len() - 1, r.path.len() - 2)] {
                let socket = socket_at(&plan, r.path[end]);
                let Some(stub) = socket else { continue };
                // Halved on a squeezed route, which is the one thing the tight
                // tier is actually for.
                let stub = if r.tier == Tier::Clean { stub } else { stub / 2 };
                let d0 = r.path[near].sub(r.path[end]);
                assert!(
                    d0.len() + 1 >= stub,
                    "{path}: {} bends {}mm off a flange that wanted {}mm of straight",
                    r.name,
                    d0.len(),
                    stub
                );
            }

            // And between two bends, for as far as the domain asked. The first
            // and last sections are the flange stubs, which have their own
            // rule above and are allowed to be shorter than this one.
            for i in 2..r.path.len().saturating_sub(1) {
                let len = r.path[i].sub(r.path[i - 1]).len();
                let least = rule.least(r.tier);
                assert!(
                    len + 1 >= least,
                    "{path}: {} has a {len}mm section between two bends, and a {} run                      of this domain has a {least}mm minimum",
                    r.name,
                    r.tier.tag()
                );
                // Which is the rule that matters, because it is the one that
                // pays for the elbows: a section with a bend on each end has
                // to be able to give up the radius twice.
                assert!(
                    len > rule.bend * 2,
                    "{path}: {} has a {len}mm section and a {}mm bend radius, so the                      elbows on its two ends overlap",
                    r.name,
                    rule.bend
                );
            }
        }
    }
}

/// The stub a flange at this point asked for, if there is a flange at it.
fn socket_at(plan: &form::layout::Plan, at: form::P3) -> Option<form::Mm> {
    plan.units
        .iter()
        .flat_map(|u| u.sockets.iter())
        .find(|s| s.at == at)
        .map(|s| s.stub)
}

/// A run the router refused is drawn as nothing at all.
///
/// This is the note's own request -- *"No valid route found." That is better
/// than generating nonsense* -- and the only way to check it is from the other
/// end: build a design whose connection genuinely cannot be made, and assert
/// that the plant has a hole in it rather than a pipe through a machine.
///
/// The design below is a chute wedged between the machine feeding it and the
/// machine it feeds, with both of its two allowed faces pressed against them.
/// A chute is the right thing to wedge because it is an `Inline` archetype:
/// its ports are pinned to one face each, so unlike a vessel it has nowhere
/// else to put them and no way out.
///
/// If a later change to the router makes this route, the fix is not to loosen
/// this test -- it is to find a new arrangement that cannot be routed, because
/// a router that never refuses is experiment 08's router with better manners.
#[test]
fn a_refused_run_is_drawn_as_nothing() {
    let src = "\
machine \"Jammed\"
brief power
reactor   R1  at 0,0
exchanger HX1 at 5,0
turbine   T1  at 9,0
generator G1  at 12,0
pump      W1  at 0,5
inlet     I1  at 5,5
chute     CH1 at 7,5
crusher   C1  at 10,5
wire R1.heat -> HX1.heat
wire W1.water -> HX1.water
wire HX1.steam -> T1.steam
wire T1.rotary -> G1.rotary
wire I1.out -> CH1.in
wire CH1.out -> C1.in
";
    let d = Design::parse(src).expect("it parses");
    assert!(d.check().is_empty(), "{:?}", d.check().iter().map(|f| &f.what).collect::<Vec<_>>());

    let s = built(&d);
    let lost: Vec<&form::route::Run> = s.routes.iter().filter(|r| r.tier == Tier::Lost).collect();
    assert!(
        !lost.is_empty(),
        "nothing in a deliberately jammed plant was refused -- this test needs a \
         tighter arrangement, not a looser assertion"
    );

    for r in lost {
        assert!(r.path.is_empty(), "{}: refused, and given a path anyway", r.name);
        assert_eq!(r.length, 0, "{}: refused, and given a length", r.name);
        assert_eq!(r.bends, 0);
        assert!(r.props.is_empty(), "{}: refused, and given supports", r.name);
        assert!(
            s.pieces_of(&r.name).is_empty(),
            "{}: refused, and drawn with {} pieces anyway",
            r.name,
            s.pieces_of(&r.name).len()
        );
        assert!(
            s.issues.iter().any(|i| i.rule == "no route" && i.bad && i.of == r.name),
            "{}: refused, and nobody was told",
            r.name
        );
    }
    assert_eq!(s.stats().lost, s.routes.iter().filter(|r| !r.laid()).count());
}

/// And the same invariant across every design there is: whatever is not laid
/// is not drawn.
#[test]
fn nothing_unlaid_is_ever_drawn() {
    for (path, d) in all_designs() {
        let s = built(&d);
        for r in s.routes.iter().filter(|r| !r.laid()) {
            assert!(s.pieces_of(&r.name).is_empty(), "{path}: {} was drawn anyway", r.name);
        }
    }
}

/// Almost every connection in the repository is laid under the full rules.
///
/// Not a property of the router so much as a property of the repository, and
/// worth pinning: rules are only worth having if a plant built by somebody who
/// had never heard of them still satisfies them. Every design here was drawn
/// against experiment 08, which had no rules at all, and all but three of its
/// connections still satisfy every one of them.
///
/// The exceptions are the interesting part, and all three are the same thing:
/// a drive shaft with a dog-leg in it. Two are squeezed through and separately
/// reported as misaligned; the third -- `MO2.rotary -> SH1.in` in
/// `07-crushline` -- is refused outright, because two motors cannot both be
/// bolted to the same end of the same shaft. Experiment 08 drew all three. See
/// the comment at the top of that design.
#[test]
fn the_repository_routes() {
    let (mut runs, mut tight) = (0usize, 0usize);
    let mut lost: Vec<(String, Domain)> = Vec::new();
    for (_, d) in all_designs() {
        let s = built(&d);
        let st = s.stats();
        runs += st.runs;
        tight += st.tight;
        for r in s.routes.iter().filter(|r| r.tier == Tier::Lost) {
            // Every refusal is reported. That is the contract: the plant may
            // have a hole in it, and the player may not have to go looking.
            assert!(
                s.issues.iter().any(|i| i.rule == "no route" && i.of == r.name),
                "{} was refused in silence",
                r.name
            );
            lost.push((r.name.clone(), r.dom));
        }
    }
    assert!(
        lost.iter().all(|(_, dom)| matches!(dom, Domain::Rotary | Domain::Mech)),
        "something other than a drive shaft was refused: {lost:?}"
    );
    assert!(
        lost.len() <= 1,
        "{} connections were refused, and the repository is meant to have one: {lost:?}",
        lost.len()
    );
    assert!(
        tight * 20 <= runs,
        "{tight} of {runs} connections had to be squeezed, which is more than a twentieth          of the repository and therefore a rule that does not fit the plant"
    );
}

// ------------------------------------------------------- 4. space is scarce

/// The spatial pass reads the plant and never writes it.
///
/// The same claim experiment 09 made about paint, one level up: a verdict is a
/// reading, so asking for it may not change the thing being read.
#[test]
fn judging_a_plant_does_not_move_it() {
    for (path, d) in all_designs() {
        let a = built(&d);
        let b = built(&d);
        assert_eq!(a.hash(), b.hash(), "{path}: the same design built two different plants");
        assert_eq!(
            a.issues.len(),
            b.issues.len(),
            "{path}: and had two different opinions about it"
        );
        for (x, y) in a.issues.iter().zip(b.issues.iter()) {
            assert_eq!((x.rule, &x.of), (y.rule, &y.of), "{path}: the report reshuffled itself");
        }
    }
}

/// A verdict is the worst thing said about a component, and nothing is said
/// about a component that nothing was said about.
#[test]
fn a_verdict_agrees_with_the_issues_behind_it() {
    for (path, d) in all_designs() {
        let s = built(&d);
        for u in &s.units {
            let mine: Vec<&form::space::Issue> =
                s.issues.iter().filter(|i| i.of == u.name).collect();
            let worst = if mine.iter().any(|i| i.bad) {
                Verdict::Bad
            } else if !mine.is_empty() {
                Verdict::Watch
            } else {
                Verdict::Clear
            };
            // A component can be dragged down by a connection at its far end,
            // so a verdict may be worse than its own issues but never better.
            assert!(
                u.verdict >= worst,
                "{path}: {} is {} and has {} issues against it",
                u.name,
                u.verdict.tag(),
                mine.len()
            );
        }
    }
}

/// Putting a machine inside another one is seen, said, and coloured.
///
/// The document refuses this in *tiles*, and for everything with a body inside
/// its own tile box that is the whole story. The case the tiles cannot see is
/// a transport component: a heat pipe does not stand on its tiles, it lives at
/// the rack elevation its domain belongs to -- four and a quarter metres up,
/// wherever its tiles are. So a heat pipe on the ground and a drum on the
/// second storey can be a legal document and two objects in the same place,
/// and this is the pass that notices.
#[test]
fn a_machine_inside_another_one_is_red() {
    let src = "\
machine \"Through The Floor\"
brief power
reactor   R1  at 0,0
heatpipe  HP1 at 5,0
exchanger HX1 at 9,0
turbine   T1  at 13,0
generator G1  at 16,0
pump      W1  at 9,4
drum      DR1 at 5,0,2
wire R1.heat -> HP1.in
wire HP1.out -> HX1.heat
wire W1.water -> HX1.water
wire HX1.steam -> T1.steam
wire T1.rotary -> G1.rotary
";
    let d = Design::parse(src).expect("it parses");
    assert!(
        d.check().is_empty(),
        "the tile grid should be happy with this: {:?}",
        d.check().iter().map(|f| &f.what).collect::<Vec<_>>()
    );

    let s = built(&d);
    assert!(
        s.issues.iter().any(|i| i.rule == "collision" && i.bad),
        "a drum was parked on top of a heat main and nobody minded: {:?}",
        s.issues.iter().map(|i| i.rule).collect::<Vec<_>>()
    );
    let hp = s.units.iter().find(|u| u.name == "HP1").unwrap();
    let dr = s.units.iter().find(|u| u.name == "DR1").unwrap();
    assert_eq!(hp.verdict, Verdict::Bad, "and the pipe is not red");
    assert_eq!(dr.verdict, Verdict::Bad, "nor is the drum");
}

/// A tower on a mezzanine is not a design. This is the note's "big vessels
/// need foundation contact", and it is the rule that stops the whole game
/// being solved by stacking.
#[test]
fn a_big_vessel_needs_the_ground() {
    let mut d = design("designs/01-first-try.machine");
    let r = d.index_of("R1").unwrap();
    assert_eq!(d.units[r].kind, Kind::Reactor);
    d.units[r].z = 4;
    let s = built(&d);
    assert!(
        s.issues.iter().any(|i| i.rule == "foundation" && i.bad),
        "a nine-metre reactor was hoisted eight metres into the air without comment"
    );
}

/// Shafts need alignment, and the rule is stricter than the one for pipes --
/// which is the whole reason a rotary port has an axis and a fluid one does
/// not.
#[test]
fn a_shaft_that_does_not_line_up_is_said_so() {
    let mut d = design("designs/01-first-try.machine");
    let g = d.index_of("G1").unwrap();
    // Slide the generator sideways out of the turbine's axis, but leave it
    // close enough to be wired.
    d.units[g].y += 3;
    assert!(d.check().is_empty(), "{:?}", d.check().iter().map(|f| &f.what).collect::<Vec<_>>());
    let s = built(&d);
    assert!(
        s.issues.iter().any(|i| i.rule == "shaft alignment" || i.rule == "no route"),
        "a drive shaft with a dog-leg in it was accepted without comment: {:?}",
        s.issues.iter().map(|i| i.rule).collect::<Vec<_>>()
    );
}

// -------------------------------------- 5. building upwards has consequences

/// Nothing stands on nothing. A component the player lifted gets a floor, and
/// the floor gets columns that reach the ground.
#[test]
fn a_storey_the_player_built_on_gets_a_floor() {
    let d = design("designs/17-stacked.machine");
    let s = built(&d);
    let deck: Vec<&form::Piece> = s
        .owners
        .iter()
        .enumerate()
        .filter(|(_, o)| o.what == "deck")
        .flat_map(|(i, _)| s.pieces.iter().filter(move |p| p.of == i as u16))
        .collect();
    assert!(!deck.is_empty(), "a mezzanine was built and nothing was put under it");

    // Whatever is up there is standing on the deck, and the deck reaches the
    // ground: the lowest piece of it is at or below the slab.
    let low = deck.iter().map(|p| p.vol().lo.y).min().unwrap();
    assert!(low <= 0, "the floor's columns stop {low}mm in the air");

    // And a way up, which is the note's second structural rule.
    let steps = deck.iter().filter(|p| p.mesh == form::kit::Mesh::Step).count();
    assert!(steps >= 4, "a floor six metres up with {steps} steps onto it");
}

/// A component on a deck is measured from its deck. The alternative -- which
/// is what the first version did -- declares every machine upstairs to be out
/// of reach and gives each of them a private staircase down to the yard.
#[test]
fn a_machine_upstairs_is_not_a_machine_out_of_reach() {
    let flat = built(&design("designs/03-compact.machine"));
    let up = built(&design("designs/17-stacked.machine"));
    let stairs = |s: &form::Scene| {
        s.pieces.iter().filter(|p| p.mesh == form::kit::Mesh::Step).count()
    };
    // The stacked plant has one more flight than the flat one -- the one onto
    // the deck -- and not one per machine standing on it.
    assert!(
        stairs(&up) < stairs(&flat) * 3,
        "the stacked plant has {} steps in it against the flat one's {}",
        stairs(&up),
        stairs(&flat)
    );
}

/// The whole of experiment 10, as one number: the same brief, the same power,
/// on less ground.
#[test]
fn stacking_buys_footprint() {
    let flat = design("designs/03-compact.machine");
    let up = design("designs/17-stacked.machine");
    let (fw, fh, _) = flat.footprint();
    let (uw, uh, _) = up.footprint();
    assert!(
        uw * uh < fw * fh,
        "the stacked design uses {}x{} and the flat one {}x{}",
        uw,
        uh,
        fw,
        fh
    );

    let power = |d: &Design| {
        let c = orbit::compile(d).expect("it compiles");
        eval::report(d, &c).met()
    };
    assert!(power(&flat) && power(&up), "and both of them answer the brief");
}

// ------------------------------------------------------------ and the rule

/// None of this reaches the simulator. The core rule of the whole module tree,
/// restated for the two things experiment 10 added: a component's storey and
/// its rotation are placement, and placement is not physics.
#[test]
fn neither_height_nor_rotation_changes_what_a_machine_does() {
    let d = design("designs/03-compact.machine");
    let want = {
        let c = orbit::compile(&d).expect("it compiles");
        eval::report(&d, &c).text()
    };

    let mut moved = d.clone();
    for u in moved.units.iter_mut() {
        // Turn everything a quarter, which changes every footprint and every
        // nozzle in the plant and must change nothing on the scoreboard.
        u.face = Some(1);
    }
    // A turn can make two components overlap, which is a document fault rather
    // than a simulation one -- so this only asserts on the designs where the
    // turn is legal.
    if moved.check().is_empty() {
        let c = orbit::compile(&moved).expect("it still compiles");
        assert_eq!(eval::report(&moved, &c).text(), want, "turning the plant changed what it does");
    }

    let mut lifted = d.clone();
    let i = lifted.index_of("G1").unwrap();
    lifted.units[i].z = 2;
    assert!(lifted.check().is_empty(), "lifting the generator is a legal document");
    let c = orbit::compile(&lifted).expect("it still compiles");
    assert_eq!(eval::report(&lifted, &c).text(), want, "lifting a generator changed what it makes");

    // And the plant is a different plant, or the lift did nothing at all.
    assert_ne!(built(&d).hash(), built(&lifted).hash(), "the lift moved no geometry");
}

/// Every grade still builds every design, including the ones with a storey on
/// them. Experiment 09's four pictures are four pictures of whatever the
/// player built, not of what they built in 2024.
#[test]
fn every_grade_still_builds_every_design() {
    for (path, d) in all_designs() {
        for g in GRADES {
            let s = form::build(&d, Ask { style: Style::Works, world: 0, grade: g })
                .unwrap_or_else(|e| panic!("{path} at {g}: {e}"));
            assert!(s.pieces.len() > 4, "{path} at {g}: {} pieces", s.pieces.len());
        }
    }
}

/// A face is a face whichever way the machine is pointing: the six sides of a
/// turned box are the six sides of a box.
#[test]
fn the_six_sides_stay_six_sides() {
    for yaw in 0..4u8 {
        let dirs: Vec<form::P3> = form::layout::SIDES.iter().map(|s| s.world(yaw)).collect();
        for (i, a) in dirs.iter().enumerate() {
            assert!(a.is_axis(), "side {i} at yaw {yaw} points at {a}");
            for (j, b) in dirs.iter().enumerate() {
                if i != j {
                    assert_ne!(a, b, "two sides of a box point the same way at yaw {yaw}");
                }
            }
        }
        assert_eq!(Side::Top.world(yaw), form::UP, "the top of a turned box is not up");
        assert_eq!(Side::Front.world(yaw), form::layout::face(yaw));
    }
}
