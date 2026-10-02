use temporal_rooms::machine::design::Design;
use temporal_rooms::machine::form::{self, kit, Ask};
use temporal_rooms::slice::{geography, phase::PHASES, region::REGIONS};

#[test]
fn every_world_tile_has_exactly_one_present_region() {
    for y in 0..geography::HEIGHT {
        for x in 0..geography::WIDTH {
            assert_eq!(PHASES.iter().filter(|p|geography::contains(**p,x,y,1,1)).count(),1);
        }
    }
    assert!(geography::phase_at(-1,0).is_none());
    assert!(geography::phase_at(0,geography::HEIGHT).is_none());
    assert_ne!(geography::edge(1,0),geography::edge(1,30));
    let worlds:Vec<_>=REGIONS.iter().map(|r|r.furnish().0).collect();
    for (n,a) in worlds.iter().enumerate() {
        for b in worlds.iter().skip(n+1) {
            for i in &a.installs { for j in &b.installs { assert!(!i.overlaps(j)); } }
        }
    }
}

#[test]
fn construction_material_changes_geometry_without_moving_interfaces() {
    let mut scenes=Vec::new();
    let mut plans=Vec::new();
    for material in ["wood","iron","steel"] {
        let d=Design::parse(&format!("machine Study\nbrief power\nhopper P at 0,0 frame {material}\n")).unwrap();
        let scene=form::build(&d,Ask::default()).unwrap();
        assert_eq!(form::layout::plan(&d).units[0].tile,(0,0,d.units[0].w(),d.units[0].h()));
        plans.push(form::layout::plan(&d));
        scenes.push(scene);
    }
    assert!(scenes[0].pieces.iter().any(|p|p.mat==kit::Mat::Wood));
    assert!(scenes[1].pieces.iter().any(|p|p.mat==kit::Mat::CastIron));
    assert!(!scenes[2].pieces.iter().any(|p|matches!(p.mat,kit::Mat::Wood|kit::Mat::CastIron)));
    for pair in scenes.windows(2) {
        assert_ne!(pair[0].pieces,pair[1].pieces);
    }
    for pair in plans.windows(2) {
        let a=&pair[0].units[0];let b=&pair[1].units[0];
        assert_eq!(a.vol,b.vol);
        assert_eq!(a.sockets.iter().map(|s|s.at).collect::<Vec<_>>(),b.sockets.iter().map(|s|s.at).collect::<Vec<_>>());
    }
}

#[test]
fn the_authored_factory_handles_a_real_local_disturbance() {
    let mut p=temporal_rooms::slice::play::Play::quiet(719);
    assert!(temporal_rooms::slice::play::run(&mut p),"{:?}",p.bad);
    assert!(p.s.disturbance.resolved>0,"the campaign never encountered a disturbance");
    assert!(p.s.yard("district").unwrap().room.log.iter().any(|c|
        c.player==0 && matches!(c.act,temporal_rooms::mp::cmd::Act::DeleteConnection{..})),
        "damage must affect a real connection through the deterministic log");
    for player in p.s.cast.iter().map(|c|c.id).collect::<Vec<_>>() { p.s.sync_all(player).unwrap(); }
}

#[test]
fn water_drive_belts_survive_draft_commands_and_replication() {
    use temporal_rooms::mp::{cmd::{self, Act}, room::Room};
    let design=Design::parse(include_str!("../designs/19-waterline.machine")).unwrap();
    let mut room=Room::open(123,Some("first-gears"));
    room.start_manual();
    let player=room.join("Engineer").unwrap();
    room.submit(player,Act::PlaceMachine{proto:"powderline".into(),x:40,y:40,face:0,item:None,design:None}).unwrap();
    let id=room.host.world.installs.last().unwrap().id;
    for act in cmd::draw(id,&design) {room.submit(player,act).unwrap();}
    let installed=room.host.world.get(id).unwrap().design.as_ref().unwrap();
    assert_eq!(installed.wires.iter().filter(|w|w.belt).count(),design.wires.iter().filter(|w|w.belt).count());
    let wire=installed.wires.iter().find(|w|w.belt).unwrap().clone();
    room.submit(player,Act::OpenDesign{id}).unwrap();
    room.submit(player,Act::SetDriveBelt{id,from:wire.from.clone(),from_port:wire.from_port.clone(),to:wire.to.clone(),to_port:wire.to_port.clone(),belt:false}).unwrap();
    assert!(room.host.world.get(id).unwrap().design.as_ref().unwrap().wires.iter().any(|w|w.belt));
    room.sync(player).unwrap();
    assert!(room.host.world.get(id).unwrap().draft.is_some());
}

#[test]
fn a_designed_machine_cannot_straddle_history_patches() {
    use temporal_rooms::{mp::cmd::Act, slice::run::Slice};
    let mut valley=Slice::open(124);
    valley.start_manual();
    let player=valley.join("Engineer").unwrap();
    let design=Design::parse(include_str!("../designs/19-waterline.machine")).unwrap();
    let refusal=valley.submit(player,"valley",Act::PlaceMachine {
        proto:"steamplant".into(),x:69,y:40,face:0,item:None,design:Some(design)
    }).unwrap_err();
    assert!(refusal.contains("region boundary"),"{refusal}");
}
