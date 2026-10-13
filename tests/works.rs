use temporal_rooms::{live::{self, Log}, machine::design::Design, mp::{cmd::{Act, Cmd}, goal::Goal, kit::proto, lower, room::Sim, secs, world::World}, slice::{landscape, phase::{Phase, PHASES}, region::REGIONS}};

fn design(src: &str) -> Design { Design::parse(src).unwrap() }
fn put(w: &mut World, x: i32, y: i32, src: &str) -> u64 {
    w.place(proto("works").unwrap(),x,y,0,None,Some(design(src)),0,1).unwrap()
}
const MINE: &str = "machine Mine\ndrill D at 0,0\noutlet O at 4,0\nwire D.out -> O.solid\n";
const STORE: &str = "machine Store\ninlet I at 0,0\nbin B at 4,0\noutlet O at 9,0\nwire I.out -> B.in\nwire B.out -> O.solid\n";
const SHIP: &str = "machine Dispatch\ninlet I at 0,0\ndispatch S at 4,0\nwire I.out -> S.solid\n";

#[test]
fn playable_terrain_has_one_position_per_feature() {
    for f in landscape::LAND {
        assert_eq!(PHASES.iter().filter(|p|f.face(**p)!=temporal_rooms::slice::land::Face::Bare).count(),1,"{}",f.tag);
    }
    let water: Vec<_>=REGIONS.iter().map(|r|r.ground().into_iter().find(|(i,_,_)|*i=="Water").unwrap().1).collect();
    assert!(water[0].w>water[0].h);
    assert!(water[1].h>water[1].w);
    assert_ne!((water[1].x-72,water[1].y),(water[0].x,water[0].y));
    assert_ne!((water[2].x-144,water[2].y),(water[0].x,water[0].y));
}

#[test]
fn a_blank_plot_gets_its_purpose_from_its_design() {
    let mut w=World::new("Works");
    w.seam("IronOre",0,0,8,6,70);
    let id=w.place(proto("works").unwrap(),0,0,0,None,None,0,1).unwrap();
    assert!(w.get(id).unwrap().design.is_none());
    let cmd=Cmd {room:"Works".into(),tick:0,seq:1,player:1,act:Act::CommitMachineDesign{id,design:design(MINE)}};
    temporal_rooms::mp::cmd::apply(&mut w,&cmd).unwrap();
    let i=w.get(id).unwrap();
    assert!(i.digs());
    assert_eq!(i.title(),"Extraction Works");
    assert!(i.wants().is_empty());
    assert_eq!(i.recipe().1[0].qty,70);
    assert_eq!(lower::lower(&design(MINE)).unwrap().rate_of("IronOre"),100.0);
    assert!(temporal_rooms::slice::phase::legal(&design(MINE),Phase::P1890).is_ok());
    let store=put(&mut w,20,0,STORE);
    assert!(w.get(store).unwrap().is_storage());
    assert_eq!(w.get(store).unwrap().capacity(),20_000);
    let ship=put(&mut w,30,0,SHIP);
    assert!(w.get(ship).unwrap().is_dispatch());
    w.connect(id,store,"IronOre").unwrap();
    w.connect(store,ship,"IronOre").unwrap();
    let c=live::carry_at(&Log::new(w.compile().graph),secs(60)).unwrap();
    assert!(c.consumed["IronOre"]>3000);
}

#[test]
fn an_ordinary_belt_intake_does_not_mine_the_ground() {
    let mut w=World::new("Works");
    w.seam("IronOre",0,0,8,6,1000);
    let id=put(&mut w,0,0,"machine Intake\ninlet I at 0,0\noutlet O at 4,0\nwire I.out -> O.solid\n");
    assert!(!w.get(id).unwrap().digs());
    assert_eq!(w.get(id).unwrap().wants(),vec!["IronOre"]);
}

#[test]
fn mining_and_processing_can_share_the_same_plot() {
    let d=design("machine Crusher\ndrill D at 0,0\nmains P at 0,5\nmotor M at 4,5\ngearbox G at 7,5 ratio 3\ncrusher C at 4,0\noutlet O at 9,0\nwire D.out -> C.in\nwire P.power -> M.power\nwire M.rotary -> G.in\nwire G.out -> C.drive\nwire C.out -> O.solid\n");
    let mut w=World::new("Works");w.seam("IronOre",0,0,8,6,50);
    let id=w.place(proto("works").unwrap(),0,0,0,None,Some(d),0,1).unwrap();
    let i=w.get(id).unwrap();
    assert!(!i.makes().is_empty());
    assert!(!i.wants().contains(&"IronOre".to_string()));
    assert!(i.recipe().1[0].qty>0);
}

#[test]
fn mixed_storage_exposes_only_connected_and_filtered_routes() {
    let d=design("machine Mixed\ninlet I at 0,0\ninlet C at 0,3 draws coal\nbin B at 4,0\nsorter O at 9,0\nsorter K at 9,3 draws coal\nwire I.out -> B.in\nwire C.out -> B.in\nwire B.out -> O.solid\nwire B.out -> K.solid\n");
    let m=lower::lower(&d).unwrap();
    assert!(m.facility.storage);
    assert_eq!(m.facility.capacity,20_000);
    assert_eq!(m.facility.outputs,vec![("Coal".into(),100),("IronOre".into(),100)]);
    let mut w=World::new("Works");
    w.seam("IronOre",0,0,8,6,100);w.seam("Coal",0,12,8,6,100);
    let ore=put(&mut w,0,0,MINE);
    let coal=put(&mut w,0,12,&MINE.replace("drill D at 0,0","drill D at 0,0 draws coal"));
    let store=w.place(proto("works").unwrap(),20,0,0,None,Some(d),0,1).unwrap();
    w.connect(ore,store,"IronOre").unwrap();w.connect(coal,store,"Coal").unwrap();
    let water=w.place(proto("head").unwrap(),40,0,0,None,None,0,1);
    assert!(water.is_err());
    let c=live::carry_at(&Log::new(w.compile().graph),secs(30)).unwrap();
    let name=&w.get(store).unwrap().name;
    assert!(c.qty[&(name.clone(),"IronOre".into())]>2000);
    assert!(c.qty[&(name.clone(),"Coal".into())]>2000);
    let ship=put(&mut w,30,0,SHIP);
    assert!(w.connect(store,ship,"Water").is_err());
}

#[test]
fn liquid_tanks_get_their_capacity_and_ports_from_the_design() {
    let d=design("machine Tank\npump I at 0,0\nstoragetank T at 3,0\noutlet O at 7,0\nwire I.water -> T.in\nwire T.out -> O.liquid\n");
    let m=lower::lower(&d).unwrap();
    assert_eq!(m.facility.capacity,12_000);
    assert_eq!(m.facility.inputs,vec![("Water".into(),200)]);
    assert_eq!(m.facility.outputs,vec![("Water".into(),200)]);
}

#[test]
fn a_shipping_dock_can_draw_from_its_own_designed_storage() {
    let store=STORE.replace("outlet O at 9,0","dispatch O at 9,0");
    let mut w=World::new("Works");w.seam("IronOre",0,0,8,6,100);
    let mine=put(&mut w,0,0,MINE);let depot=put(&mut w,20,0,&store);
    w.connect(mine,depot,"IronOre").unwrap();
    let mut sim=Sim::new(Goal::of_seed(1,Some("first-gears")),w);
    sim.advance(secs(60)).unwrap();
    assert!(sim.acct.shipped.get("IronOre").copied().unwrap_or(0)>5000);
}

#[test]
fn storage_and_its_inventory_survive_commands_and_a_snapshot() {
    let mut w=World::new("Works");w.seam("IronOre",0,0,8,6,100);
    let mine=put(&mut w,0,0,MINE);let store=put(&mut w,20,0,STORE);
    w.connect(mine,store,"IronOre").unwrap();
    let goal=Goal::of_seed(1,Some("first-gears"));
    let mut host=Sim::new(goal.clone(),w.clone());let mut peer=Sim::new(goal,w);
    host.advance(secs(30)).unwrap();peer.advance(secs(30)).unwrap();
    assert_eq!(host.hash_at(secs(30)),peer.hash_at(secs(30)));
    let name=host.world.get(store).unwrap().name.clone();
    let held=host.carry.qty[&(name.clone(),"IronOre".into())];
    let revised=design(&STORE.replace("outlet O at 9,0","outlet OU1 at 10,0").replace("B.out -> O.solid","B.out -> OU1.solid"));
    let edits=vec![Act::OpenDesign{id:store},Act::DeleteComponent{id:store,unit:"O".into()},
        Act::PlaceComponent{id:store,kind:"outlet".into(),x:10,y:0,z:0,face:None},
        Act::ConnectComponent{id:store,from:"B".into(),from_port:"out".into(),to:"OU1".into(),to_port:"solid".into(),belt:false},
        Act::CommitMachineDesign{id:store,design:revised}];
    for (seq,act) in edits.into_iter().enumerate() {
        let cmd=Cmd {room:"Works".into(),tick:secs(30),seq:seq as u64+1,player:1,act};
        host.apply(&cmd).unwrap();peer.apply(&Cmd::from_json(&cmd.to_json()).unwrap()).unwrap();
    }
    assert_eq!(host.carry.qty[&(name,"IronOre".into())],held);
    let mut late=Sim::of_snapshot(&host.snapshot()).unwrap();
    host.advance(secs(60)).unwrap();peer.advance(secs(60)).unwrap();late.advance(secs(60)).unwrap();
    assert_eq!(host.hash_at(secs(60)),peer.hash_at(secs(60)));
    assert_eq!(host.hash_at(secs(60)),late.hash_at(secs(60)));
}

#[test]
fn adding_loading_interfaces_changes_real_world_throughput() {
    let store="machine Store\ninlet I1 at 0,0\ninlet I2 at 0,3\ninlet I3 at 0,6\ninlet I4 at 0,9\nbin B1 at 4,0\nbin B2 at 4,6\noutlet O at 9,3\nwire I1.out -> B1.in\nwire I2.out -> B1.in\nwire I3.out -> B2.in\nwire I4.out -> B2.in\nwire B1.out -> O.solid\nwire B2.out -> O.solid\n";
    let faster=store.to_string()+"outlet P at 9,6\nwire B2.out -> P.solid\n";
    let rate=|src:&str| {
        let mut w=World::new("Works");w.seam("IronOre",0,0,15,15,1000);
        let mines:Vec<_>=(0..4).map(|n|put(&mut w,0,n*3,MINE)).collect();
        let store=put(&mut w,20,0,src);
        for mine in mines { w.connect(mine,store,"IronOre").unwrap(); }
        let depot=w.place(proto("depot").unwrap(),35,0,0,Some("IronOre".into()),None,0,1).unwrap();
        w.connect(store,depot,"IronOre").unwrap();
        live::carry_at(&Log::new(w.compile().graph),secs(120)).unwrap().consumed["IronOre"]
    };
    assert!(rate(&faster)>rate(store)*18/10);
}

#[test]
fn delivered_grades_survive_designs_commands_and_shipping() {
    for item in lower::ITEMS.iter().filter(|item|temporal_rooms::machine::stuff::Stuff::for_item(item).is_some()) {
        let stuff=temporal_rooms::machine::stuff::Stuff::for_item(item).unwrap();
        assert_eq!(lower::item_of(&stuff),*item);
    }
    let d=design("machine GearStore\ninlet I at 0,0 item Gear\nbin B at 4,0\ndispatch O at 9,0\nwire I.out -> B.in\nwire B.out -> O.solid\n");
    assert_eq!(Design::from_json(&d.to_json()).unwrap().emit(),d.emit());
    assert_eq!(Design::parse(&d.emit()).unwrap().emit(),d.emit());
    let m=lower::lower(&d).unwrap();
    assert_eq!(m.facility.inputs,vec![("Gear".into(),100)]);
    assert_eq!(m.facility.shipping,vec![("Gear".into(),200)]);
    let mut w=World::new("Works");let id=w.place(proto("works").unwrap(),0,0,0,None,None,0,1).unwrap();
    for (seq,act) in temporal_rooms::mp::cmd::draw(id,&d).into_iter().enumerate() {
        let cmd=Cmd{room:"Works".into(),tick:0,seq:seq as u64+1,player:1,act};
        temporal_rooms::mp::cmd::apply(&mut w,&Cmd::from_json(&cmd.to_json()).unwrap()).unwrap();
    }
    let mut sim=Sim::new(Goal::of_seed(1,Some("first-gears")),w);
    sim.apply(&Cmd{room:"Works".into(),tick:secs(1),seq:100,player:0,act:Act::Deliver{to:id,item:"Gear".into(),qty:2000,from:"QA".into()}}).unwrap();
    sim.advance(secs(15)).unwrap();
    assert_eq!(sim.acct.shipped.get("Gear").copied().unwrap_or(0),2000);
}

#[test]
fn a_sorter_distinguishes_two_grades_of_the_same_substance() {
    let d=design("machine Grades\ninlet A at 0,0 item OrePowder\ninlet B at 0,3 item Concentrate\nbin C at 4,0\nsorter D at 9,0 item Concentrate\nwire A.out -> C.in\nwire B.out -> C.in\nwire C.out -> D.solid\n");
    let m=lower::lower(&d).unwrap();
    assert_eq!(m.facility.outputs,vec![("Concentrate".into(),100)]);
    let filtered=design("machine Filter\ninlet A at 0,0 item OrePowder\nsorter B at 4,0 item Concentrate\nwire A.out -> B.solid\n");
    assert!(lower::lower(&filtered).unwrap().gives.is_empty());
    assert!(Design::parse("machine Bad\ndrill D at 0,0 item Gear\n").unwrap().check().len()>0);
}

#[test]
fn a_redesign_cannot_silently_remove_connected_loading_interfaces() {
    let mut w=World::new("Works");w.seam("IronOre",0,0,8,6,100);
    let mine=put(&mut w,0,0,MINE);let store=put(&mut w,20,0,STORE);w.connect(mine,store,"IronOre").unwrap();
    let before=w.signature();
    let no_intake=design("machine Closed\nbin B at 4,0\noutlet O at 9,0\nwire B.out -> O.solid\n");
    let err=temporal_rooms::mp::cmd::apply(&mut w,&Cmd{room:"Works".into(),tick:0,seq:1,player:1,act:Act::CommitMachineDesign{id:store,design:no_intake}}).unwrap_err();
    assert!(err.contains("Disconnect"));assert_eq!(w.signature(),before);
}

#[test]
fn mixed_deliveries_share_capacity_and_loaded_storage_cannot_be_erased_by_commit() {
    let d=design("machine Mixed\ninlet I at 0,0\ninlet C at 0,3 draws coal\nbin B at 4,0\nwire I.out -> B.in\nwire C.out -> B.in\n");
    let mut w=World::new("Works");let id=w.place(proto("works").unwrap(),0,0,0,None,Some(d),0,1).unwrap();
    let name=w.get(id).unwrap().name.clone();let mut sim=Sim::new(Goal::of_seed(1,Some("first-gears")),w);
    for (seq,item) in ["IronOre","Coal"].iter().enumerate() {
        sim.apply(&Cmd{room:"Works".into(),tick:secs(1),seq:seq as u64+1,player:0,act:Act::Deliver{to:id,item:item.to_string(),qty:15_000,from:"QA".into()}}).unwrap();
    }
    assert_eq!(sim.carry.qty[&(name.clone(),"IronOre".into())],15_000);
    assert_eq!(sim.carry.qty[&(name.clone(),"Coal".into())],5000);
    let before=sim.world.signature();
    assert!(sim.apply(&Cmd{room:"Works".into(),tick:secs(1),seq:3,player:1,act:Act::CommitMachineDesign{id,design:design(MINE)}}).unwrap_err().msg.contains("Unload"));
    assert_eq!(sim.world.signature(),before);
    assert!(sim.apply(&Cmd{room:"Works".into(),tick:secs(1),seq:4,player:0,act:Act::Deliver{to:id,item:"Water".into(),qty:1,from:"QA".into()}}).is_err());
}

#[test]
fn a_running_miner_is_not_reported_as_an_idle_outlet() {
    use temporal_rooms::machine::{orbit,eval,snap,sim::Machine};
    let d=design(MINE);let c=orbit::compile(&d).unwrap();
    let viewed=c.state_at(&d,30).unwrap();let mut actual=Machine::new(&d).unwrap();
    for _ in 0..30 {actual.step();}
    assert_eq!(viewed.key(),actual.key());
    let j=snap::render(&d,&viewed,&eval::report(&d,&c));
    let outlet=j.at("units").as_arr().iter().find(|u|u.at("name").as_str()==Some("O")).unwrap();
    assert_eq!(outlet.at("status").as_str(),Some("RUNNING"));
    assert!(!j.at("holding").as_arr().iter().any(|u|u.at("name").as_str()==Some("O")));
}
