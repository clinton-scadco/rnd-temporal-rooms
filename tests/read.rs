//! Experiment 09, held to its own claims.
//!
//! The readability pass makes a much narrower set of claims than experiment 08
//! did, and every one of them is about what did *not* change:
//!
//! ```text
//!   1  B is A, repainted             the same pieces, in the same places
//!   2  C and D add, and never move   every grade is the same machine
//!   3  the material language is a language, not a coat of paint
//!   4  the vocabulary lands on the pipe it belongs to
//!   5  none of it reaches the simulator
//! ```
//!
//! Claim 1 is the one the whole comparison rests on. The note that asked for
//! experiment 09 was explicit -- *no geometry changes, just improve the
//! material assignment rules* -- and a comparison between two pictures that
//! quietly differ in geometry as well as in paint would prove nothing at all
//! about paint. So it is checked piece for piece, on every design in the
//! repository, in both directions: nothing moved, and something was repainted.

use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::form::kit::{Mat, Mesh};
use temporal_rooms::machine::form::{self, Ask, Grade, Owns, Style, GRADES, MEDIUM};
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

/// Outdoors, because a readability pass on a building is a readability pass on
/// a wall.
fn at(d: &Design, g: Grade) -> form::Scene {
    form::build(d, Ask { style: Style::Yard, world: 0, grade: g }).expect("it builds")
}

// -------------------------------------------------- 1. B is A, repainted

/// The claim the note actually made, checked the only way it can be: piece for
/// piece, on every design there is.
#[test]
fn the_material_pass_moves_no_geometry() {
    for (path, d) in all_designs() {
        let a = at(&d, Grade::Grey);
        let b = at(&d, Grade::Paint);
        assert_eq!(a.pieces.len(), b.pieces.len(), "{path}: the material pass added a piece");
        let mut repainted = 0;
        for (x, y) in a.pieces.iter().zip(b.pieces.iter()) {
            assert_eq!(x.mesh, y.mesh, "{path}: a mesh changed in the material pass");
            assert_eq!(x.at, y.at, "{path}: a piece moved in the material pass");
            assert_eq!(x.size, y.size, "{path}: a piece resized in the material pass");
            assert_eq!(x.dir, y.dir, "{path}: a piece turned in the material pass");
            assert_eq!(x.spin, y.spin, "{path}: a piece span in the material pass");
            assert_eq!(x.lod, y.lod, "{path}: a piece changed level in the material pass");
            assert_eq!(x.of, y.of, "{path}: a piece changed owner in the material pass");
            if x.mat != y.mat {
                repainted += 1;
            }
        }
        // And the converse, or the pass would be vacuous.
        assert!(
            repainted > a.pieces.len() / 20,
            "{path}: the material pass repainted {repainted} of {} pieces",
            a.pieces.len()
        );
    }
}

// ------------------------------------------- 2. every grade is one machine

/// The grades add and refine. What they may never do is lay the plant out
/// differently -- same components, same volumes, same routes, same bends. If
/// this ever fails, the four pictures have stopped being four pictures of one
/// thing and the comparison is worthless.
#[test]
fn every_grade_is_the_same_machine() {
    for (path, d) in all_designs() {
        let base = at(&d, Grade::Grey);
        for g in GRADES {
            let s = at(&d, g);
            assert_eq!(s.routes.len(), base.routes.len(), "{path} at {g}: a run appeared");
            for (x, y) in s.routes.iter().zip(base.routes.iter()) {
                assert_eq!(x.path, y.path, "{path} at {g}: {} was rerouted", x.name);
                assert_eq!(x.bore, y.bore, "{path} at {g}: {} changed size", x.name);
            }
            let units = |sc: &form::Scene| {
                sc.owners.iter().filter(|o| o.class == Owns::Unit).map(|o| o.name.clone()).collect::<Vec<_>>()
            };
            assert_eq!(units(&s), units(&base), "{path} at {g}: the components changed");
            assert!(s.pieces.len() >= base.pieces.len(), "{path} at {g}: the plant lost pieces");
        }
        // Detail is added, not swapped: each grade is at least as full as the
        // one before it.
        let counts: Vec<usize> = GRADES.iter().map(|&g| at(&d, g).pieces.len()).collect();
        assert!(counts[1] == counts[0], "{path}: B is not A repainted");
        assert!(counts[2] > counts[1], "{path}: C added nothing");
        assert!(counts[3] > counts[2], "{path}: D added nothing");
    }
}

/// The silhouette is what a plant is recognised by from across a yard, and the
/// readability pass is not allowed to change it much. Articulation hangs
/// things on a machine; it does not make a different machine.
#[test]
fn articulation_does_not_reshape_the_plant() {
    for (path, d) in all_designs() {
        let a = at(&d, Grade::Grey);
        let full = at(&d, Grade::Full);
        let (x, y) = (a.bounds.size(), full.bounds.size());
        for (n, (before, after)) in [("width", (x.x, y.x)), ("depth", (x.z, y.z))].iter() {
            // Growth is the failure. Shrinking is not, and is expected: from
            // grade C a stair comes down whichever side of its platform has
            // room, and a plant whose access stairs stop landing in the yard
            // has a smaller footprint than one whose stairs do not.
            assert!(
                after - before < 1_500,
                "{path}: the plant's {n} grew {}mm between A and D",
                after - before
            );
        }
        // Height is allowed a davit and a stack, but not a storey.
        assert!(y.y - x.y < 3_000, "{path}: the plant grew {}mm taller", y.y - x.y);
    }
}

// ------------------------------------------------- 3. it is a language

/// A material language means every material is *for* something. A material
/// used on one piece in a whole repository is not a language, it is a
/// decoration; and one used on everything is not a distinction.
#[test]
fn every_material_is_for_something() {
    let mut uses: std::collections::BTreeMap<Mat, usize> = Default::default();
    let mut total = 0usize;
    for (_, d) in all_designs() {
        for p in &at(&d, Grade::Full).pieces {
            *uses.entry(p.mat).or_default() += 1;
            total += 1;
        }
    }
    for m in [Mat::Paint, Mat::Steel, Mat::Dark, Mat::Galv, Mat::Concrete, Mat::Lag, Mat::Warn, Mat::Water] {
        let n = *uses.get(&m).unwrap_or(&0);
        assert!(n > 20, "{m} is used {n} times across every design there is");
        assert!(
            n * 2 < total,
            "{m} is over half the plant: a distinction that is everywhere is a colour"
        );
    }
}

/// What a component is made of says what it is for. A tank is not the colour a
/// turbine is, a heat exchanger is not the colour either of them is, and the
/// structure is not the colour of any of it.
#[test]
fn the_palette_separates_what_a_thing_is_for() {
    let d = design("designs/11-steamcrusher.machine");
    let s = at(&d, Grade::Full);
    // The body material of each kind: the commonest material among the pieces
    // that survive to the middle distance, which is what a viewer sees.
    let mut body: std::collections::BTreeMap<String, std::collections::BTreeMap<Mat, usize>> =
        Default::default();
    for p in s.pieces.iter().filter(|p| p.lod >= MEDIUM) {
        let o = s.owner(p.of);
        if o.class != Owns::Unit {
            continue;
        }
        *body.entry(o.what.clone()).or_default().entry(p.mat).or_default() += 1;
    }
    let commonest = |k: &str| -> Mat {
        *body
            .get(k)
            .unwrap_or_else(|| panic!("{k} is not in this design"))
            .iter()
            .max_by_key(|(m, n)| (**n, std::cmp::Reverse(**m)))
            .map(|(m, _)| m)
            .unwrap()
    };
    assert_ne!(commonest("turbine"), commonest("exchanger"), "a turbine is painted like a heat exchanger");
    assert_ne!(commonest("reactor"), commonest("turbine"), "a reactor is painted like a turbine");

    // And the whole plant is more legible than it was: at grade A several
    // kinds are drawn out of an identical set of pieces in identical
    // materials, and at grade D fewer are.
    let grey = at(&d, Grade::Grey).legible();
    let full = s.legible();
    assert_eq!(grey.1, full.1, "the two grades disagree about how many kinds there are");
    assert!(full.0 >= grey.0, "the readability pass made the plant less legible: {grey:?} -> {full:?}");
    assert!(full.0 * 4 >= full.1 * 3, "only {} of {} kinds are distinguishable", full.0, full.1);
}

/// The one thing in the palette that is decided by the *machine* rather than
/// by the piece: a water line and an oil line are the same pipe in different
/// paint, and which one it is comes from tracing the line back to its source.
#[test]
fn service_decides_the_colour_of_a_fluid_line() {
    let refinery = design("designs/10-refinery.machine");
    let s = at(&refinery, Grade::Full);
    let fluids: Vec<&form::route::Run> =
        s.routes.iter().filter(|r| r.dom == Domain::Fluid).collect();
    assert!(!fluids.is_empty(), "a refinery with no fluid in it");
    let oil = fluids.iter().filter(|r| r.serve == temporal_rooms::machine::stuff::Subst::Crude).count();
    assert!(oil > 0, "nothing in this refinery is carrying crude: {:?}", fluids.iter().map(|r| (r.name.clone(), r.serve)).collect::<Vec<_>>());

    // The colour follows: an oil line is drawn in oil and a water line is not.
    let mats_of = |want: temporal_rooms::machine::stuff::Subst| {
        let ids: Vec<u16> = s
            .owners
            .iter()
            .enumerate()
            .filter(|(_, o)| {
                o.class == Owns::Run && s.routes.iter().any(|r| r.name == o.name && r.serve == want)
            })
            .map(|(i, _)| i as u16)
            .collect();
        s.pieces
            .iter()
            .filter(|p| ids.contains(&p.of) && p.mesh == Mesh::Cyl)
            .map(|p| p.mat)
            .collect::<std::collections::BTreeSet<_>>()
    };
    assert!(
        mats_of(temporal_rooms::machine::stuff::Subst::Crude).contains(&Mat::Oil),
        "a crude line is not drawn as one"
    );
}

// ------------------------------------------- 4. the vocabulary lands on it

/// Every flange, valve, clamp, reducer and collar belongs to a run, and every
/// one of them is *on* that run. A fitting floating half a metre off its own
/// pipe is the failure this whole section is exposed to, and it is the sort
/// that a screenshot from the right angle will hide for a month.
#[test]
fn the_connection_vocabulary_sits_on_its_own_run() {
    for (path, d) in all_designs() {
        let s = at(&d, Grade::Full);
        for (i, o) in s.owners.iter().enumerate() {
            if o.class != Owns::Run {
                continue;
            }
            let Some(run) = s.routes.iter().find(|r| r.name == o.name) else { continue };
            for p in s.pieces.iter().filter(|p| p.of == i as u16) {
                let d0 = to_path(&run.path, p.at);
                assert!(
                    d0 <= run.bore * 3 + 400,
                    "{path}: a {} on {} is {}mm off its own pipe",
                    p.mesh,
                    run.name,
                    d0
                );
            }
        }
    }
}

/// The distance from a point to the run's polyline, in millimetres.
fn to_path(path: &[form::P3], p: form::P3) -> form::Mm {
    let mut best = form::Mm::MAX;
    for i in 1..path.len() {
        let (a, b) = (path[i - 1], path[i]);
        let d = b.sub(a);
        let len = d.len().max(1);
        // The projection, clamped to the segment. Integer, and therefore off
        // by a millimetre, which is well inside the tolerance above.
        let t = ((p.sub(a).x as i64 * d.x as i64
            + p.sub(a).y as i64 * d.y as i64
            + p.sub(a).z as i64 * d.z as i64)
            / (len as i64 * len as i64).max(1)) as form::Mm;
        let t = t.clamp(0, 1);
        let on = if t == 0 { a } else { b };
        // Sample the segment, because a clamped integer projection is a coarse
        // instrument on a forty-metre run.
        let n = (len / 250).max(1);
        for k in 0..=n {
            let q = a.add(d.mul(k).div(n));
            best = best.min(q.sub(p).len());
        }
        best = best.min(on.sub(p).len());
    }
    best
}

/// Nothing hangs in the air. Every support, pad, rack and trestle the extra
/// grades add starts at the ground or on something that does.
#[test]
fn the_added_steel_stands_on_something() {
    for (path, d) in all_designs() {
        let s = at(&d, Grade::Full);
        for p in s.pieces.iter() {
            let o = s.owner(p.of);
            if o.class != Owns::Frame || p.mesh != Mesh::Beam {
                continue;
            }
            // A column points up; anything else is a beam or a brace, and is
            // held up by the columns.
            if p.dir != form::UP {
                continue;
            }
            assert!(p.at.y <= 200, "{path}: a column of {} starts {}mm up", o.name, p.at.y);
        }
    }
}

// ------------------------------------------------ 5. still downstream

/// Experiment 08's core rule, re-checked against the axis experiment 09 added.
/// Four grades is four more ways to accidentally let the picture decide
/// something, and this is the test that would notice.
#[test]
fn the_grade_changes_no_number() {
    for (path, d) in all_designs() {
        let before = eval::report(&d, &orbit::compile(&d).unwrap());
        let mut seen = std::collections::BTreeSet::new();
        for g in GRADES {
            let s = at(&d, g);
            seen.insert(s.hash());
            let after = eval::report(&d, &orbit::compile(&d).unwrap());
            assert_eq!(
                before.headline().value(),
                after.headline().value(),
                "{path}: building it at grade {g} moved the verdict"
            );
        }
        assert_eq!(seen.len(), GRADES.len(), "{path}: two grades built the same plant");
    }
}

/// And it is still a function: the same design at the same grade is the same
/// plant, to the last bit of the hash.
#[test]
fn a_grade_is_deterministic() {
    let d = design("designs/10-refinery.machine");
    for g in GRADES {
        assert_eq!(at(&d, g).hash(), at(&d, g).hash(), "grade {g} is not a function");
    }
}

// ------------------------------------------------------ the measurements

/// The readability metric is measured off the pixels, so it has to survive a
/// render: a picture of a plant with nothing in it would score zero and prove
/// the metric works on nothing at all.
#[test]
fn the_palette_is_measured_off_the_picture() {
    let d = design("designs/15-turbinehall.machine");
    let grey = at(&d, Grade::Grey);
    let full = at(&d, Grade::Full);
    let frame = grey.bounds.join(full.bounds);
    let eye = form::shot::Eye::default();
    let a = form::shot::render_in(&grey, frame, 320, 220, eye, 0).palette();
    let b = form::shot::render_in(&full, frame, 320, 220, eye, 0).palette();

    assert!(a.ink > 5, "the baseline covers {}% of the frame", a.ink);
    // The same machine from the same camera covers the same frame, whatever it
    // is painted -- which is the check that the two pictures are comparable.
    assert!((a.ink as i64 - b.ink as i64).abs() <= 3, "{a:?} vs {b:?}");
    assert!(b.tones >= a.tones, "the readability pass lost tones: {a:?} -> {b:?}");
    assert!(b.chroma >= a.chroma, "the readability pass lost colour: {a:?} -> {b:?}");
}

/// The kit stayed a kit. Experiment 09 was allowed to add to the library and
/// not to abandon the argument for having one.
#[test]
fn the_library_is_still_small() {
    use temporal_rooms::machine::form::kit;
    assert!(kit::MESHES.len() <= 30, "the note asked for twenty to thirty meshes");
    assert!(kit::MATS.len() <= 14, "one material library, not one per asset");
    // Every material in the library has a distinct colour, or it is not a
    // material, it is a synonym.
    let mut seen: std::collections::BTreeSet<[u8; 3]> = Default::default();
    for m in kit::MATS {
        let (c, _, _) = m.look();
        assert!(seen.insert(c), "{m} is the same colour as something else");
    }
    // And the four added meshes are canonical, like the twenty-five before
    // them: the whole library is checked in `tests/form.rs`, so this only has
    // to catch one that is empty.
    for m in [Mesh::Reducer, Mesh::Clamp, Mesh::Cowl, Mesh::Saddle] {
        assert!(kit::geom(m).tris() > 8, "{m} is not a mesh");
    }
}

/// Grade A does not drift.
///
/// These eight numbers pin the grey build, so that a change made for the sake
/// of grade B, C or D cannot quietly reach back and alter the baseline the
/// comparison is measured against. Four new meshes, four new materials, a
/// repaint, a vocabulary and a set of articulated archetypes were all added
/// without moving one of them.
///
/// They moved once, deliberately, and it is worth writing down why: the
/// readability pass painted grade C and D in a material language, and the
/// first thing the paint did was show up five pieces of geometry that had
/// been wrong since experiment 08 and were invisible in grey.
///
/// ```text
///   the dome was wound inside out            tanks had no tops
///   the pipe support overshot its own box    posts came up through pipes
///   the handrail was spun towards a machine  walkways were railed lying down
///   both stair stringers rounded to zero     flights drawn as a single wire
///   the corners were decided twice           two fifths of them had a gap
///   a tee's branch stopped inside its run    tees were pipes with a collar
///   an inline valve was always due east      a barrel across a straight
/// ```
///
/// None of that is a grade: a plant in grey had every one of those faults too,
/// which is why fixing them moves this test. The baseline is the plant built
/// without the material language, not the plant built with the bugs.
///
/// Experiment 10 moved it again, and for a bigger reason than a list of bugs:
/// every port in the plant is on a different face, because a port is now an
/// interface rather than a coordinate, and every line between them is laid by
/// a router that walks straight sections rather than cells. The plant these
/// numbers pin is the same plant in the same materials; it is not the same
/// pipework, and it was never going to be.
#[test]
fn grade_a_is_experiment_08_exactly() {
    for (name, want) in [
        ("01-first-try", 0xa06a_6ad4u32),
        ("03-compact", 0x5796_f30d),
        ("07-crushline", 0x5d4d_4093),
        ("09-machining", 0x2a0d_2586),
        ("10-refinery", 0x4299_59f8),
        ("11-steamcrusher", 0x44a1_290b),
        ("13-longreach", 0xc5e3_1d20),
        ("15-turbinehall", 0x536e_50ef),
    ] {
        let d = design(&format!("designs/{name}.machine"));
        let s = form::build(&d, Ask { style: Style::Works, world: 0, grade: Grade::Grey })
            .expect("it builds");
        assert_eq!(
            s.hash() as u32,
            want,
            "{name}: the baseline moved -- {:08x}, and it was pinned at {want:08x}",
            s.hash() as u32
        );
    }
}
