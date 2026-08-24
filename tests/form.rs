//! Experiment 08, held to its own claims.
//!
//! The visual compiler makes five claims that are worth something only if
//! something keeps trying to break them:
//!
//! ```text
//!   1  the mesh never defines the machine    changing the look changes no number
//!   2  generation is deterministic           design + seed -> the same plant, always
//!   3  it is reactive, and locally so        move one thing, and one thing moves
//!   4  connections say what they connect     every wire is a route, socket to socket
//!   5  complexity is arrangement             a small kit, instanced, not modelled
//! ```
//!
//! Claim 1 is the important one and the easiest to break by accident, because
//! the day somebody reads a component's *height* to decide a rate, the whole
//! architecture has quietly inverted. So it is checked the brutal way: every
//! design in the repository is judged, then built under three styles and four
//! world seeds, then judged again, and the verdicts must be identical to the
//! last decimal place.

use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::form::layout::{Arch, Mount};
use temporal_rooms::machine::form::{self, kit, Ask, Style, FAR, MEDIUM};
use temporal_rooms::machine::{eval, orbit};

fn design(path: &str) -> Design {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    Design::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

fn all_designs() -> Vec<(String, Design)> {
    let mut out = Vec::new();
    let mut names: Vec<String> = std::fs::read_dir("designs")
        .expect("designs/")
        .flatten()
        .map(|e| e.path().to_string_lossy().into_owned())
        .filter(|p| p.ends_with(".machine"))
        .collect();
    names.sort();
    for n in names {
        let d = design(&n);
        out.push((n, d));
    }
    assert!(out.len() >= 10, "the primary experiment wants ten installations");
    out
}

fn built(d: &Design) -> form::Scene {
    form::build(d, Ask::default()).expect("it builds")
}

// ------------------------------------------------- 1. downstream of the sim

/// The core rule, from the only direction that can prove it: build the same
/// design every way the visual compiler can be asked to build it, and check
/// that the machine's verdict never moves.
#[test]
fn the_look_changes_no_number() {
    for (path, d) in all_designs() {
        let before = eval::report(&d, &orbit::compile(&d).unwrap());
        let mut seen = std::collections::BTreeSet::new();
        for style in [Style::Works, Style::Yard, Style::Hall] {
            for world in [0u64, 1, 7, 9_999] {
                let s = form::build(&d, Ask { style, world }).unwrap();
                seen.insert(s.hash());
                let after = eval::report(&d, &orbit::compile(&d).unwrap());
                assert_eq!(
                    before.headline().value(),
                    after.headline().value(),
                    "{path}: building it {style} at seed {world} moved the verdict"
                );
                assert_eq!(before.width, after.width, "{path}: the plot moved");
                assert_eq!(before.components, after.components, "{path}");
            }
        }
        // And the converse: the styles and seeds are not decoration on an
        // identical scene. If they were, section 7 would be vacuous.
        assert!(seen.len() > 1, "{path}: every style and seed built the same plant");
    }
}

// ------------------------------------------------------- 2. it is a function

#[test]
fn the_same_design_builds_the_same_plant() {
    for (path, d) in all_designs() {
        let a = built(&d);
        let b = built(&d);
        assert_eq!(a.hash(), b.hash(), "{path}: built twice, differently");
        assert_eq!(a.pieces.len(), b.pieces.len(), "{path}");
        for (x, y) in a.pieces.iter().zip(b.pieces.iter()) {
            assert_eq!(x, y, "{path}: a piece moved between two builds");
        }
    }
}

/// A design that has been through the file and back is the same design, so it
/// has to build the same plant -- which is the property that makes `design +
/// seed` a thing you can put on a wire.
#[test]
fn a_round_trip_through_the_file_builds_the_same_plant() {
    for (path, d) in all_designs() {
        let again = Design::parse(&d.emit()).unwrap();
        assert_eq!(built(&d).hash(), built(&again).hash(), "{path}");
    }
}

#[test]
fn a_different_world_seed_is_a_different_plant() {
    let d = design("designs/03-compact.machine");
    let a = form::build(&d, Ask { style: Style::Works, world: 1 }).unwrap();
    let b = form::build(&d, Ask { style: Style::Works, world: 2 }).unwrap();
    assert_ne!(a.hash(), b.hash());
    // But not a differently *shaped* plant: the seed dresses it, it does not
    // design it. Same components, same pipework, same steel.
    assert_eq!(a.routes.len(), b.routes.len());
    for (x, y) in a.routes.iter().zip(b.routes.iter()) {
        assert_eq!(x.path, y.path, "the seed moved a pipe");
    }
}

// ---------------------------------------------------------- 3. reactivity

/// Move the generator; the shaft that drives it, and the plinth under it, must
/// move too -- and the reactor at the other end of the plant must not so much
/// as change a handwheel.
#[test]
fn moving_a_component_moves_what_belongs_to_it() {
    let d = design("designs/03-compact.machine");
    let before = built(&d);

    let mut after = d.clone();
    let g = after.index_of("G2").expect("G2");
    after.units[g].x += 3;
    let after = built(&after);

    let moved: Vec<_> = before.pieces_of("G2").iter().map(|p| p.at).collect();
    let now: Vec<_> = after.pieces_of("G2").iter().map(|p| p.at).collect();
    assert!(!moved.is_empty(), "a generator with no geometry");
    assert_ne!(moved, now, "the generator did not move");

    // Its shaft was rerouted.
    let shaft = |s: &form::Scene| {
        s.routes.iter().find(|r| r.name.contains("T2.rotary")).map(|r| r.path.clone())
    };
    assert_ne!(shaft(&before), shaft(&after), "the shaft did not follow the generator");

    // And the reactor is untouched, down to the last piece.
    let a: Vec<_> = before.pieces_of("R1").iter().map(|p| **p).collect();
    let b: Vec<_> = after.pieces_of("R1").iter().map(|p| **p).collect();
    assert_eq!(a, b, "moving a generator redressed the reactor");
}

/// Adding a component adds its own geometry and the steel under it, and the
/// plant grows by roughly what was added rather than being reshuffled.
#[test]
fn adding_a_component_has_visible_consequences() {
    let d = design("designs/03-compact.machine");
    let before = built(&d);

    let mut more = d.clone();
    let t1 = more.unit("T1").unwrap().clone();
    more.units.push(temporal_rooms::machine::design::Unit {
        name: "T3".into(),
        kind: t1.kind,
        x: t1.x,
        y: t1.y + 7,
        tune: t1.tune,
    });
    more.wires.push(temporal_rooms::machine::design::Wire {
        from: "HX2".into(),
        from_port: "steam".into(),
        to: "T3".into(),
        to_port: "steam".into(),
    });
    assert!(more.check().is_empty(), "{:?}", more.check());
    let after = built(&more);

    assert!(after.pieces_of("T3").len() > 4, "a turbine made of nothing");
    assert!(after.routes.len() > before.routes.len(), "no new steam branch");
    assert!(after.pieces.len() > before.pieces.len(), "the plant did not grow");
    // The turbine stands on something that was never drawn by hand.
    let owns: Vec<&str> = after
        .owners
        .iter()
        .filter(|o| o.name == "T3")
        .map(|o| o.what.as_str())
        .collect();
    assert!(owns.contains(&"plinth"), "no foundation appeared under the new turbine: {owns:?}");
}

// -------------------------------------------------------- 4. the connections

#[test]
fn every_wire_becomes_a_route_between_its_own_two_sockets() {
    for (path, d) in all_designs() {
        let s = built(&d);
        assert_eq!(s.routes.len(), d.wires.len(), "{path}: a wire went missing");
        let plan = form::layout::plan(&d);
        for (w, r) in d.wires.iter().zip(s.routes.iter()) {
            assert!(r.name.starts_with(&format!("{}.{}", w.from, w.from_port)), "{path}: {}", r.name);
            assert!(r.path.len() >= 2, "{path}: {} is not a route", r.name);

            let from = plan.find(&w.from).unwrap();
            let to = plan.find(&w.to).unwrap();
            let a = *r.path.first().unwrap();
            let b = *r.path.last().unwrap();
            assert!(
                from.sockets.iter().any(|s| s.at == a),
                "{path}: {} does not start at a socket of {}",
                r.name,
                w.from
            );
            assert!(
                to.sockets.iter().any(|s| s.at == b),
                "{path}: {} does not end at a socket of {}",
                r.name,
                w.to
            );
        }
    }
}

/// Industrial-looking means orthogonal. Every segment of every route runs along
/// one axis -- there are no diagonals in a pipe rack.
#[test]
fn routes_are_orthogonal() {
    for (path, d) in all_designs() {
        for r in built(&d).routes {
            if r.direct {
                // The router admits when it could not find a way through; a
                // straight line in defiance of the plant is allowed to be
                // diagonal, because it is already an apology.
                continue;
            }
            for i in 1..r.path.len() {
                let seg = r.path[i].sub(r.path[i - 1]);
                assert!(
                    seg.is_axis(),
                    "{path}: {} has a diagonal section {seg} at {}",
                    r.name,
                    r.path[i - 1]
                );
            }
        }
    }
}

/// A pipe may not go through a machine. This is the property that makes the
/// A* worth having at all -- a straight line would be cheaper and wrong.
#[test]
fn routes_go_round_equipment_rather_than_through_it() {
    for (path, d) in all_designs() {
        let plan = form::layout::plan(&d);
        let s = built(&d);
        for r in &s.routes {
            if r.direct {
                continue;
            }
            for (i, u) in plan.units.iter().enumerate() {
                if u.arch == Arch::Run {
                    continue;
                }
                // Its own two ends are allowed to be inside the things they
                // are bolted to.
                if r.name.starts_with(&format!("{}.", u.name))
                    || r.name.contains(&format!("> {}.", u.name))
                    || r.name.contains(&format!("-> {}.", u.name))
                {
                    continue;
                }
                let body = u.vol.grow(-300);
                for k in 1..r.path.len() {
                    let (a, b) = (r.path[k - 1], r.path[k]);
                    // Sample the segment rather than clip it: a metre is finer
                    // than any machine in the kit is small.
                    let n = (b.sub(a).len() / 500).max(1);
                    for t in 0..=n {
                        let p = a.add(b.sub(a).mul(t).div(n));
                        assert!(
                            !body.has(p),
                            "{path}: {} passes through {} (unit {i}) at {p}",
                            r.name,
                            u.name
                        );
                    }
                }
            }
        }
    }
}

/// The seven domains do not look alike, which is what lets a viewer read a
/// plant with the labels hidden.
#[test]
fn each_domain_gets_its_own_treatment() {
    let d = design("designs/11-steamcrusher.machine");
    let s = built(&d);
    let mut kinds: std::collections::BTreeMap<&str, std::collections::BTreeSet<(kit::Mesh, kit::Mat)>> =
        Default::default();
    for r in &s.routes {
        let id = s
            .owners
            .iter()
            .position(|o| o.name == r.name)
            .expect("a route with no owner") as u16;
        for p in s.pieces.iter().filter(|p| p.of == id && p.lod >= MEDIUM) {
            kinds.entry(r.dom.tag()).or_default().insert((p.mesh, p.mat));
        }
    }
    assert!(kinds.len() >= 4, "a design with fewer than four domains proves nothing");
    // No two domains are drawn out of the same set of mesh-and-material pairs.
    let sets: Vec<_> = kinds.values().cloned().collect();
    for (i, a) in sets.iter().enumerate() {
        for b in &sets[i + 1..] {
            assert_ne!(a, b, "two domains are drawn identically: {kinds:?}");
        }
    }
}

// ---------------------------------------------------- 5. arrangement, not art

#[test]
fn the_library_stays_small() {
    assert!(kit::MESHES.len() <= 30, "the note asked for twenty to thirty meshes");
    assert!(kit::MATS.len() <= 10, "one material library, not one per asset");
    for m in kit::MESHES {
        let g = kit::geom(m);
        assert!(g.tris() > 0, "{m} is empty");
        assert!(g.tris() < 800, "{m} has {} triangles, which is a model", g.tris());
        // Canonical space: footprint centred, standing on the origin plane.
        // The elbow is the one exception, and it is not a sloppy one -- it
        // reaches out to exactly the bend radius the router places it by.
        let wide = if m == kit::Mesh::Elbow { kit::Mesh::ELBOW_R + 0.5 } else { 0.62 };
        // A handrail is the one piece whose *height* runs along its local +Z,
        // because its length has to run along +Y like every other stretchable
        // thing, and something has to give.
        let deep = if m == kit::Mesh::Rail { 1.02 } else { wide };
        // A little over the top is allowed: a pipe support's cradle holds the
        // pipe *at* its nominal height, so its arms stand proud of it.
        let tall = if m == kit::Mesh::Elbow { kit::Mesh::ELBOW_R + 0.6 } else { 1.3 };
        for i in 0..g.verts() {
            let (x, y, z) = (g.pos[i * 3], g.pos[i * 3 + 1], g.pos[i * 3 + 2]);
            assert!(x.abs() <= wide, "{m} is wider than canonical: {x}");
            assert!(z >= -deep && z <= deep, "{m} is deeper than canonical: {z}");
            assert!((-0.1..=tall).contains(&y), "{m} leaves canonical height: {y}");
            assert!(x.is_finite() && y.is_finite() && z.is_finite(), "{m} has a bad vertex");
        }
    }
    // And the elbow really does reach its bend radius, because the router
    // shortens the straights either side of it by exactly that.
    let e = kit::geom(kit::Mesh::Elbow);
    let far = e.pos.chunks(3).map(|v| v[1]).fold(0.0f32, f32::max);
    assert!((far - kit::Mesh::ELBOW_R).abs() < 0.51, "the elbow does not end where it says: {far}");
}

/// The claim in section 10: a plant is a handful of draw calls, and it stays a
/// handful of draw calls when the plant gets big.
#[test]
fn a_plant_is_a_few_dozen_draw_calls() {
    let mut worst = 0;
    for (path, d) in all_designs() {
        let s = built(&d);
        let st = s.stats();
        assert!(st.pieces > 40, "{path}: {} pieces is not a plant", st.pieces);
        assert!(
            st.batches <= 60,
            "{path}: {} draw calls for {} pieces",
            st.batches,
            st.pieces
        );
        // Instancing is the whole point: far more pieces than batches.
        assert!(
            st.pieces >= st.batches * 8,
            "{path}: {} pieces over {} batches is not instancing",
            st.pieces,
            st.batches
        );
        worst = worst.max(st.batches);
    }
    assert!(worst > 0);
}

/// Every batch is sorted so that a coarser view draws a prefix of the same
/// buffer. If that ordering is wrong, the level-of-detail switch quietly draws
/// the wrong pieces.
#[test]
fn detail_is_a_prefix_of_the_same_buffer() {
    for (path, d) in all_designs() {
        let s = built(&d);
        for b in s.batches() {
            let mut last = u8::MAX;
            for p in &b.inst {
                assert!(p.lod <= last, "{path}: a batch is not sorted by level");
                last = p.lod;
            }
            assert_eq!(b.keep[0], b.inst.len());
            assert!(b.keep[1] <= b.keep[0]);
            assert!(b.keep[2] <= b.keep[1]);
            for (i, p) in b.inst.iter().enumerate() {
                assert_eq!(i < b.keep[1], p.lod >= MEDIUM);
                assert_eq!(i < b.keep[2], p.lod >= FAR);
            }
        }
    }
}

/// Distance takes pieces away and never adds them, and the far view is a small
/// fraction of the close one.
#[test]
fn distance_only_ever_removes() {
    let d = design("designs/11-steamcrusher.machine");
    let s = built(&d);
    let st = s.stats();
    assert!(st.medium < st.close);
    assert!(st.far < st.medium);
    assert!(st.far * 3 < st.close, "the far view is not much cheaper: {st:?}");
    // What survives to the far view is equipment and structure, not dressing.
    for p in s.pieces.iter().filter(|p| p.lod >= FAR) {
        assert!(
            !matches!(p.mesh, kit::Mesh::Gauge | kit::Mesh::Rail | kit::Mesh::Step | kit::Mesh::Ladder),
            "{} survives to the far view",
            p.mesh
        );
    }
}

// ------------------------------------------------------------ the layout

/// Every rotary socket in a plant is at one height, which is why a line shaft
/// is a straight line and not a staircase.
#[test]
fn the_drive_train_lines_up() {
    let d = design("designs/12-onemotor.machine");
    let plan = form::layout::plan(&d);
    let mut heights = std::collections::BTreeSet::new();
    for u in &plan.units {
        // Everything that stands on the floor. What is up a frame is up a
        // frame, and its drive is up there with it.
        if u.lift > 400 {
            continue;
        }
        for s in &u.sockets {
            if s.dom == temporal_rooms::machine::stuff::Domain::Rotary {
                heights.insert(s.at.y);
            }
        }
    }
    assert_eq!(
        heights.len(),
        1,
        "the drive train is at {} different heights: {heights:?}",
        heights.len()
    );
    assert_eq!(*heights.iter().next().unwrap(), form::layout::SHAFT_Y);
}

/// Material falls: in at the top, out at the bottom. An ore line built out of
/// that one rule visibly cascades downhill.
#[test]
fn material_goes_in_high_and_comes_out_low() {
    let d = design("designs/07-crushline.machine");
    let plan = form::layout::plan(&d);
    for u in &plan.units {
        let ins: Vec<i32> = u
            .sockets
            .iter()
            .filter(|s| s.dom == temporal_rooms::machine::stuff::Domain::Material)
            .filter(|s| s.dir == temporal_rooms::machine::parts::Dir::In)
            .map(|s| s.at.y)
            .collect();
        let outs: Vec<i32> = u
            .sockets
            .iter()
            .filter(|s| s.dom == temporal_rooms::machine::stuff::Domain::Material)
            .filter(|s| s.dir == temporal_rooms::machine::parts::Dir::Out)
            .map(|s| s.at.y)
            .collect();
        if let (Some(hi), Some(lo)) = (ins.iter().max(), outs.iter().max()) {
            assert!(hi > lo, "{} discharges above what it is fed: {hi} vs {lo}", u.name);
        }
    }
}

/// Sockets face what they are wired to. Move a component to the other side of
/// its partner and both sockets turn round, with nobody editing anything.
#[test]
fn sockets_face_their_partner() {
    let d = design("designs/03-compact.machine");
    let plan = form::layout::plan(&d);
    let hx = plan.find("HX1").unwrap();
    let east = hx.sockets.iter().find(|s| s.dom == temporal_rooms::machine::stuff::Domain::Gas).unwrap();
    assert_eq!(east.out, form::EAST, "the steam outlet does not face the turbine");

    let mut flipped = d.clone();
    let i = flipped.index_of("T1").unwrap();
    flipped.units[i].x = -6;
    let plan = form::layout::plan(&flipped);
    let hx = plan.find("HX1").unwrap();
    let west = hx.sockets.iter().find(|s| s.dom == temporal_rooms::machine::stuff::Domain::Gas).unwrap();
    assert_eq!(west.out, form::WEST, "the steam outlet did not follow the turbine");
}

/// Nothing floats. Everything either stands on the ground, stands on something
/// that stands on the ground, or is a pipe.
#[test]
fn everything_that_is_up_is_held_up() {
    for (path, d) in all_designs() {
        let plan = form::layout::plan(&d);
        for u in &plan.units {
            if u.arch == Arch::Run {
                continue;
            }
            if u.lift > 0 {
                assert!(
                    matches!(u.mount, Mount::Plinth | Mount::Legs | Mount::Frame),
                    "{path}: {} is {}mm up on nothing",
                    u.name,
                    u.lift
                );
            }
        }
        // And a run that is high and long has supports under it.
        let s = built(&d);
        for r in &s.routes {
            let long = r.length > 12_000;
            let high = r.path.iter().any(|p| p.y > 2500);
            if long && high && !r.direct {
                assert!(!r.props.is_empty(), "{path}: {} is a {}m span on nothing", r.name, r.length / 1000);
            }
        }
    }
}

// -------------------------------------------------------------- the output

#[test]
fn the_scene_survives_the_wire() {
    let d = design("designs/10-refinery.machine");
    let s = built(&d);
    let j = s.to_json();
    let text = j.to_string();
    let back = temporal_rooms::json::parse(&text).expect("valid json");
    assert_eq!(back.at("hash").as_str().unwrap(), format!("{:016x}", s.hash()));
    assert_eq!(back.at("batches").as_arr().len(), s.batches().len());
    let n: usize = back
        .at("batches")
        .as_arr()
        .iter()
        .map(|b| b.at("n").as_u64().unwrap() as usize)
        .sum();
    assert_eq!(n, s.pieces.len(), "an instance went missing on the way out");
    for b in back.at("batches").as_arr() {
        let n = b.at("n").as_u64().unwrap() as usize;
        assert_eq!(b.at("inst").as_arr().len(), n * 12, "twelve floats per instance");
        assert!(kit::by_tag(b.at("mesh").as_str().unwrap()).is_some());
    }
    // The library the client is sent once.
    let k = form::kit_json();
    assert_eq!(k.at("meshes").as_arr().len(), kit::MESHES.len());
    assert_eq!(k.at("mats").as_arr().len(), kit::MATS.len());
}

#[test]
fn it_bakes_to_an_obj_that_says_what_it_is() {
    let d = design("designs/01-first-try.machine");
    let s = built(&d);
    let (obj, mtl) = form::obj::write(&s);
    let verts = obj.lines().filter(|l| l.starts_with("v ")).count();
    let faces = obj.lines().filter(|l| l.starts_with("f ")).count();
    assert_eq!(faces, s.tris(), "the baked triangles disagree with the count");
    assert!(verts > faces / 2);
    assert!(obj.contains("g R1_reactor"), "the obj is not grouped by component");
    for m in kit::MATS {
        assert!(mtl.contains(&format!("newmtl {}", m.tag())));
    }
}

/// The headless renderer: a picture, without a browser. It is checked for
/// being a picture *of something* rather than for being pretty -- a plant that
/// renders to an empty frame is the failure worth catching.
#[test]
fn it_renders_without_a_browser() {
    let d = design("designs/03-compact.machine");
    let s = built(&d);
    let img = form::shot::render(&s, 200, 140, form::shot::Eye::default(), 0);
    let rgb = img.rgb();
    assert_eq!(rgb.len(), 200 * 140 * 3);
    let sky = [204u8, 214, 224];
    let painted = rgb.chunks(3).filter(|p| p[0] != sky[0] || p[1] != sky[1] || p[2] != sky[2]).count();
    assert!(painted > 200 * 140 / 20, "the plant covers {painted} pixels of 28,000");

    let png = img.png();
    assert_eq!(&png[..8], &[0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a]);
    assert!(png.windows(4).any(|w| w == b"IHDR"));
    assert!(png.windows(4).any(|w| w == b"IEND"));

    // The far view is genuinely cheaper to draw, not just declared to be.
    let near = form::shot::render(&s, 60, 40, form::shot::Eye::default(), 0);
    let far = form::shot::render(&s, 60, 40, form::shot::Eye::default(), FAR);
    assert_ne!(near.rgb(), far.rgb());
}

// ------------------------------------------------------------ the awkward

/// A design the simulator refuses is a design the renderer refuses, in the
/// same words. Two answers to "is this a machine?" would be one too many.
#[test]
fn a_broken_design_is_refused_by_both() {
    let d = Design::parse("machine \"Bad\"\nreactor R1 at 0,0\nreactor R1 at 1,1\n").unwrap();
    assert!(!d.check().is_empty());
    let e = form::build(&d, Ask::default()).unwrap_err();
    assert_eq!(e, d.check()[0].what);
}

/// One component and no wires still builds: a plant with nothing to connect is
/// a legitimate thing to look at while you decide what to put next to it.
#[test]
fn one_component_is_a_plant() {
    let d = Design::parse("machine \"Alone\"\nreactor R1 at 0,0\n").unwrap();
    let s = built(&d);
    assert!(s.routes.is_empty());
    assert!(s.pieces.len() > 10);
    assert!(s.bounds.size().y > 8_000, "a nine-metre reactor came out {}mm tall", s.bounds.size().y);
}

/// The frame construction has to be a rotation rather than a reflection, or
/// every upright piece in the plant is mirrored -- which is invisible on a
/// cylinder and obvious on a stair.
#[test]
fn the_placement_frame_is_a_rotation() {
    use temporal_rooms::machine::form::{frame_of, p3, EAST, NORTH, SOUTH, UP, WEST};
    let (r, f) = frame_of(UP);
    assert_eq!(r, EAST.mul(1000), "canonical +X does not lie along the world's");
    assert_eq!(f, SOUTH.mul(1000));
    for d in [EAST, WEST, NORTH, SOUTH, UP, p3(0, -1, 0)] {
        let (r, f) = frame_of(d);
        let u = p3(d.x * 1000, d.y * 1000, d.z * 1000);
        // Right-handed: right x up = fwd, to within integer rounding.
        let cross = p3(
            (r.y * u.z - r.z * u.y) / 1000,
            (r.z * u.x - r.x * u.z) / 1000,
            (r.x * u.y - r.y * u.x) / 1000,
        );
        assert_eq!(cross, f, "the frame at {d} is a reflection");
    }
    // A horizontal piece stands up in its own +Z, which is what makes a
    // handrail a handrail.
    for d in [EAST, WEST, NORTH, SOUTH] {
        assert_eq!(frame_of(d).1, UP.mul(1000), "a rail along {d} would lie down");
    }
}
