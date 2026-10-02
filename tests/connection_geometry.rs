use temporal_rooms::machine::{design::Design, form::{self, layout, route, kit::{Mat, Mesh}, Ask, Grade, Owns, P3, p3, FAR}};

fn build(body: &str) -> (Design, form::Scene) {
    let d = Design::parse(&format!("machine \"Connection regression\"\nbrief power\n{body}\n")).unwrap();
    let s = form::build(&d, Ask { style: form::Style::Yard, world: 0, grade: Grade::Full }).unwrap();
    (d, s)
}
fn on(a: P3, b: P3, p: P3) -> bool { p.taxi(a) + p.taxi(b) == a.taxi(b) }

#[test]
fn risers_never_extrapolate_supports_or_leave_them_in_rounded_corners() {
    for body in [
        "pump P1 at 0,0 face east\npreheater H1 at 6,0,3 face east\nwire P1.water -> H1.in",
        "reactor R1 at 0,0\nexchanger H1 at 6,3 face east\nwire R1.heat -> H1.heat",
        "mains E1 at 0,0\nmotor M1 at 7,4\nwire E1.power -> M1.power",
    ] {
        let (_, scene) = build(body);
        for r in &scene.routes {
            let elbows = route::elbows_of(r);
            for &p in &r.props {
                let (i, segment) = r.path.windows(2).enumerate().find(|(_, ab)| on(ab[0], ab[1], p)).expect("support is on the actual finite route");
                assert_eq!(segment[0].y, segment[1].y, "a ground post needs a horizontal seat");
                if elbows[i] { assert!(p.sub(segment[0]).len() >= route::bend_of(r)); }
                if elbows[i + 1] { assert!(p.sub(segment[1]).len() >= route::bend_of(r)); }
            }
        }
        // A tall post must not scale its cradle away from the pipe. Each
        // cradle is independently sized and reaches a real pipe underside.
        for p in scene.pieces.iter().filter(|p| p.mesh == Mesh::Support) {
            assert_eq!(p.size.y, 150);
            let seat = p.at.y + p.size.y * 94 / 100;
            assert!(scene.routes.iter().any(|r| r.props.iter().any(|at| {
                at.x == p.at.x && at.z == p.at.z && (seat - (at.y - route::outer(r.dom, r.bore) / 2)).abs() <= 2
            })), "cradle does not meet any pipe: {p:?}");
        }
    }
}

#[test]
fn raised_bin_frame_reaches_the_bin_from_its_deck() {
    let (d, scene) = build("inlet I1 at 0,0,3 draws ore\ncrusher C1 at 4,0\nwire I1.out -> C1.in");
    let plan = layout::plan(&d);
    let bin = plan.find("I1").unwrap();
    let columns: Vec<_> = scene.pieces.iter().filter(|p| scene.owner(p.of).name == "I1" && scene.owner(p.of).what == "legs" && p.mesh == Mesh::Beam && p.dir == form::UP).collect();
    assert_eq!(columns.len(), 4);
    for p in columns { assert_eq!(p.at.y, bin.base); assert_eq!(p.at.y + p.size.y, bin.vol.lo.y); }
    assert!(scene.pieces.iter().any(|p| p.mesh == Mesh::DuctBend && scene.owner(p.of).class == Owns::Run), "falling chute has no corner");
}

#[test]
fn motor_and_inline_castings_follow_the_solved_port_axis() {
    for body in [
        "motor M1 at 0,0 face east\ngearbox G1 at 5,1 face east\nwire M1.rotary -> G1.in",
        "valve V1 at 0,0 face east\nvalve V2 at 5,0 face east\nwire V1.out -> V2.in",
    ] {
        let (d, scene) = build(body); let plan = layout::plan(&d);
        for u in plan.units.iter().filter(|u| matches!(u.arch, layout::Arch::Can | layout::Arch::Inline)) {
            let port = u.sockets.iter().find(|s| s.connected).unwrap();
            let casing = scene.pieces_of(&u.name).into_iter().find(|p| matches!(p.mesh, Mesh::Fins | Mesh::Cyl) && (p.lod == FAR || u.arch == layout::Arch::Inline)).unwrap();
            assert_eq!(casing.at.y, port.at.y);
            if port.out.x != 0 { assert_eq!(casing.at.z, port.at.z); }
            if port.out.z != 0 { assert_eq!(casing.at.x, port.at.x); }
            if u.arch == layout::Arch::Can {
                for p in scene.pieces_of(&u.name).into_iter().filter(|p| p.mesh == Mesh::Panel) {
                    if port.out.x != 0 { assert_eq!(p.at.z, port.at.z, "terminal box floats away from shifted motor"); }
                    if port.out.z != 0 { assert_eq!(p.at.x, port.at.x, "terminal box floats away from shifted motor"); }
                }
            }
        }
    }
}

#[test]
fn shared_ports_have_one_trunk_and_a_tee_at_the_actual_split_or_merge() {
    for body in [
        "pump P1 at 0,3\npreheater H1 at 5,0 face east\npreheater H2 at 5,6 face east\nwire P1.water -> H1.in\nwire P1.water -> H2.in",
        "pump P1 at 0,0\npump P2 at 0,6\npreheater H1 at 6,3 face east\nwire P1.water -> H1.in\nwire P2.water -> H1.in",
    ] {
        let (_, scene) = build(body);
        let physical = route::physical_runs(&scene.routes);
        assert!(physical[1].length < scene.routes[1].length, "second branch still duplicates the trunk");
        let tees: Vec<_> = scene.pieces.iter().filter(|p| p.mesh == Mesh::Tee).collect();
        assert_eq!(tees.len(), 1);
        let tee = tees[0];
        let d = tee.dir; let len = d.len();
        let centre = tee.at.add(p3(d.x * tee.size.y / (2 * len), d.y * tee.size.y / (2 * len), d.z * tee.size.y / (2 * len)));
        assert!(physical[1].path.first() == Some(&centre) || physical[1].path.last() == Some(&centre), "tee is not at the branch junction");
    }
}

#[test]
fn belts_have_pulleys_and_separate_working_and_return_strands() {
    let (_, scene) = build("motor M1 at 0,0 face east\ngearbox G1 at 5,6 face east\nwire M1.rotary -> G1.in belt");
    let r = &scene.routes[0]; assert!(r.belt && r.laid());
    let pieces = scene.pieces_of(&r.name);
    assert_eq!(pieces.iter().filter(|p| p.mesh == Mesh::Box && p.mat == Mat::Rubber && p.size.z == 45).count(), 4);
    assert_eq!(pieces.iter().filter(|p| p.mesh == Mesh::Band && p.mat == Mat::Rubber).count(), 2);
    assert!(r.props.is_empty(), "belt generated unused pipe posts");
    assert!(!scene.issues.iter().any(|i| i.rule == "shaft alignment"));
}

#[test]
fn unused_turbine_drive_ports_do_not_render_a_second_open_pipe() {
    let (d, scene) = build("exchanger H1 at 0,0 face east\nturbine T1 at 5,3 face east\nwire H1.steam -> T1.steam");
    let plan = layout::plan(&d);
    let turbine = plan.find("T1").unwrap();
    let ports = turbine.sockets.iter().filter(|s| s.connected).count();
    assert_eq!(scene.pieces_of("T1").iter().filter(|p| p.mesh == Mesh::Nozzle).count(), ports);
    assert!(!scene.pieces_of("T1").iter().any(|p| p.mesh == Mesh::Cyl && p.mat == Mat::Steel && p.dir.is_axis() && p.dir.y == 0), "unused output shaft remains");
}

#[test]
fn mixed_assembly_stairs_keep_clear_of_the_pipes() {
    let (_, scene) = build("reactor R1 at 0,0\npump P1 at 0,4\nexchanger H1 at 5,0 face east\nturbine T1 at 9,0 face east\ngenerator G1 at 13,0 face east\nwire R1.heat -> H1.heat\nwire P1.water -> H1.water\nwire H1.steam -> T1.steam\nwire T1.rotary -> G1.rotary");
    for step in scene.pieces.iter().filter(|p| p.mesh == Mesh::Step) {
        for r in &scene.routes {
            for ab in r.path.windows(2) {
                assert!(!form::Vol::new(ab[0].min(ab[1]), ab[0].max(ab[1])).grow(route::outer(r.dom, r.bore) / 2).hits(step.vol()), "stair clips {}", r.name);
            }
        }
    }
}

#[test]
fn reactor_lifting_arm_is_mounted_to_the_shell() {
    let (_, scene) = build("reactor R1 at 0,0");
    let arm = scene.pieces_of("R1").into_iter().find(|p| p.mesh == Mesh::Beam && p.mat == Mat::Dark && p.dir == form::UP).expect("lifting arm upright");
    let shell = scene.pieces_of("R1").into_iter().find(|p| p.mesh == Mesh::Cyl && p.lod == FAR && p.mat != Mat::Dark).expect("reactor shell");
    assert!(arm.at.y < shell.at.y + shell.size.y, "upright starts above the shell");
}

#[test]
fn a_third_branch_does_not_duplicate_the_second_branches_trunk() {
    let (_, scene) = build("pump P1 at 0,3\npreheater H1 at 5,0 face east\npreheater H2 at 5,6 face east\npreheater H3 at 8,7 face east\nwire P1.water -> H1.in\nwire P1.water -> H2.in\nwire P1.water -> H3.in");
    let runs = route::physical_runs(&scene.routes);
    for (i, r) in runs.iter().enumerate() {
        for parent in &runs[..i] {
            for ab in r.path.windows(2) {
                for cd in parent.path.windows(2) {
                    let d = ab[1].sub(ab[0]);
                    if d.y != 0 { continue; }
                    let same_line = if d.x != 0 { ab[0].y == cd[0].y && ab[0].z == cd[0].z && cd[0].z == cd[1].z } else { ab[0].y == cd[0].y && ab[0].x == cd[0].x && cd[0].x == cd[1].x };
                    if !same_line { continue; }
                    let (a,b,c,d) = if d.x != 0 { (ab[0].x,ab[1].x,cd[0].x,cd[1].x) } else { (ab[0].z,ab[1].z,cd[0].z,cd[1].z) };
                    assert!(a.min(b).max(c.min(d)) >= a.max(b).min(c.max(d)), "shared trunk drawn twice");
                }
            }
        }
    }
}
