use temporal_rooms::machine::{design::Design, parts::Kind, stuff::Domain, form::{body, layout::{self, Arch},
    kit::{Mat, Mesh}, seed::Seed, Grade, FAR}};

#[test]
fn detailed_machines_have_seated_supports_and_connected_casings() {
    let mut checked = [0; 3];
    let seed = Seed::of("assembly", 0, "yard", 0);
    for file in std::fs::read_dir("designs").unwrap().flatten() {
        if file.path().extension().is_none_or(|ext| ext != "machine") { continue; }
        let d = Design::parse(&std::fs::read_to_string(file.path()).unwrap()).unwrap();
        let plan = layout::plan(&d);
        for u in &plan.units {
            let mut pieces = Vec::new();
            body::dress(u, &plan, &seed, Grade::Full, 0, &mut pieces);
            let barrel = pieces.iter().find(|p| matches!(p.mesh, Mesh::Cyl | Mesh::Fins | Mesh::Volute) && p.lod == FAR && p.mat != Mat::Dark);
            match u.arch {
                Arch::Can => {
                    let barrel = barrel.unwrap();
                    if let Some(port) = u.sockets.iter().find(|s| matches!(s.dom, Domain::Rotary | Domain::Mech)) {
                        assert_eq!(barrel.at.y, port.at.y, "{}: drive leaves below the barrel axis", u.name);
                    }
                    let feet: Vec<_> = pieces.iter().filter(|p| p.mesh == Mesh::Box && p.mat == Mat::Dark && p.at.y == u.vol.lo.y + 160).collect();
                    assert_eq!(feet.len(), 4, "{}: four feet on the bedplate", u.name);
                    for foot in feet {
                        assert!(foot.at.y + foot.size.y < barrel.at.y, "{}: foot penetrates past the shaft axis", u.name);
                    }
                    checked[0] += 1;
                }
                Arch::Shell => {
                    let barrel = barrel.unwrap();
                    let underside = barrel.at.y - barrel.size.x / 2;
                    for saddle in pieces.iter().filter(|p| p.mesh == Mesh::Saddle) {
                        let seat = saddle.at.y + saddle.size.y * 62 / 100;
                        assert!((seat - underside).abs() <= 2, "{}: saddle cuts through or misses the shell", u.name);
                    }
                    checked[1] += 1;
                }
                Arch::Turbine => {
                    let barrel = barrel.unwrap();
                    let taper = pieces.iter().find(|p| p.mesh == Mesh::Cone && p.lod == FAR).unwrap();
                    assert_eq!(taper.dir, barrel.dir, "{}: taper points back into the casing", u.name);
                    assert_eq!(taper.at, barrel.at.add(barrel.dir.mul(barrel.size.y)), "{}: casing halves do not meet", u.name);
                    checked[2] += 1;
                }
                _ => {}
            }
            // These three archetypes use unit directions for their bodies;
            // their non-unit cylinder spans are the added connection necks.
            if matches!(u.arch, Arch::Can | Arch::Shell | Arch::Turbine) {
                for neck in pieces.iter().filter(|p| p.mesh == Mesh::Cyl && p.dir.len() > 1) {
                    assert!(u.sockets.iter().any(|s| s.at == neck.at.add(neck.dir)), "{}: neck misses every interface", u.name);
                    assert_eq!(neck.size.y, neck.dir.len());
                }
            }
        }
    }
    assert!(checked.iter().all(|&n| n > 0), "missing assembly coverage: {checked:?}");
}

#[test]
fn pumps_have_a_compact_process_casing_and_the_right_drive() {
    let seed = Seed::of("pump-assembly", 0, "yard", 0);
    let mut checked = [0; 2];
    for file in std::fs::read_dir("designs").unwrap().flatten() {
        if file.path().extension().is_none_or(|ext| ext != "machine") { continue; }
        let d = Design::parse(&std::fs::read_to_string(file.path()).unwrap()).unwrap();
        let plan = layout::plan(&d);
        for u in plan.units.iter().filter(|u| matches!(u.kind, Kind::Pump | Kind::MechPump)) {
            let mut pieces = Vec::new();
            body::dress(u, &plan, &seed, Grade::Full, 0, &mut pieces);
            let case = pieces.iter().find(|p| p.mesh == Mesh::Volute && p.lod == FAR).unwrap();
            assert!(case.size.y < case.size.x, "{}: pump still looks like a long can", u.name);
            assert!(case.vol().hi.y <= u.vol.hi.y, "{}: new casing exceeds the logical equipment height", u.name);
            let drive = pieces.iter().find(|p| p.lod == FAR && p.at != case.at).unwrap();
            assert!(drive.size.x < case.size.x * 3 / 4, "{}: process casing must dominate the drive", u.name);
            assert_eq!(drive.at.y, case.at.y, "{}: coupling shafts do not align", u.name);
            assert_eq!(drive.dir, case.dir);
            assert!(pieces.iter().any(|p| p.mesh == Mesh::Coupling), "{}: drive is disconnected", u.name);
            for socket in u.sockets.iter().filter(|s| s.dom == Domain::Fluid) {
                let neck = pieces.iter().find(|p| p.mesh == Mesh::Cyl && p.dir.len() > 1 && p.at.add(p.dir) == socket.at)
                    .unwrap_or_else(|| panic!("{}: fluid socket has no casing connection", u.name));
                assert!(case.vol().has(neck.at), "{}: fluid was connected to the drive instead of the process casing", u.name);
            }
            if u.kind == Kind::MechPump {
                assert_eq!(drive.mesh, Mesh::Cyl, "a driven pump must not acquire an electric motor");
                assert!(!pieces.iter().any(|p| p.mesh == Mesh::Fins || p.mesh == Mesh::Cowl));
                let socket = u.sockets.iter().find(|s| s.dom == Domain::Rotary).unwrap();
                assert_eq!(case.dir, socket.out.neg());
                assert_eq!(case.at.y, socket.at.y);
                checked[1] += 1;
            } else {
                assert_eq!(drive.mesh, Mesh::Fins);
                checked[0] += 1;
            }
        }
    }
    assert!(checked.iter().all(|&n| n > 0), "missing pump variant coverage: {checked:?}");
}
