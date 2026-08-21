//! The workbench is not allowed to change the physics.
//!
//! Prototype 0 puts two new things between a player and the solver: a document
//! that a mouse edits, and a snapshot that a canvas draws. Both are conversions,
//! and a conversion is exactly the kind of thing that quietly permutes a list
//! and produces a plant that is nearly the one somebody built.
//!
//! So the document is required to be a *faithful* view: take any of the fifteen
//! configurations, read it into the document, write it back out, and the plant
//! that comes back must be the same plant -- the same storage indices, the same
//! class indices, the same arbitration queues, and the same state at every tick
//! probed, checked with the same signatures v3 uses to compare a decomposed
//! Room against a monolithic one.

use temporal_rooms::graph::Graph;
use temporal_rooms::json;
use temporal_rooms::model::*;
use temporal_rooms::pop::Pop;
use temporal_rooms::rooms::{self, Room};
use temporal_rooms::snap;
use temporal_rooms::{dsl, web};

const CONFIGS: &[&str] = &[
    "configs/01-spec.factory",
    "configs/02-balanced.factory",
    "configs/03-megafactory.factory",
    "configs/04-science.factory",
    "configs/05-coupled.factory",
    "configs/06-cycle.factory",
    "configs/07-transport.factory",
    "configs/08-policy.factory",
    "configs/09-population.factory",
    "configs/10-billion.factory",
    "configs/11-railchain.factory",
    "configs/12-tradeloop.factory",
    "configs/13-orefield.factory",
    "configs/14-privatebay.factory",
    "configs/15-continent.factory",
];

const PROBES: &[Tick] = &[1, 17, 60, 137, 999, 2_000, 6_000, 20_000];

fn load(path: &str) -> Program {
    let src = std::fs::read_to_string(path).unwrap_or_else(|e| panic!("{path}: {e}"));
    dsl::parse(&src).unwrap_or_else(|e| panic!("{path}: {e}"))
}

/// The blueprint the solver actually runs, which for a deployment of lines
/// that share infrastructure is the lowered one.
fn effective(prog: &Program) -> &Blueprint {
    &prog.blueprints[prog.deploys[0].blueprint as usize]
}

/// Every structural fact about a plant, in one string. Comparing these
/// compares indices and orderings, not just contents -- which is the part a
/// round trip is most likely to get subtly wrong.
fn fingerprint(bp: &Blueprint, items: &[String]) -> String {
    let mut s = String::new();
    let name = |i: ItemId| items[i as usize].as_str();
    let stacks = |v: &[Stack]| -> String {
        v.iter().map(|k| format!("{}x{}", k.qty, name(k.item))).collect::<Vec<_>>().join("+")
    };
    s.push_str(&format!("blueprint machines={} period={}\n", bp.machines, bp.base_period));
    for (i, a) in bp.actors.iter().enumerate() {
        s.push_str(&format!(
            "class {i} {} {} x{} dur={} ret={} geo={:?} shared={} off={} in={:?} out={:?} \
             consumes[{}] produces[{}]\n",
            a.name,
            a.kind.label(),
            a.count,
            a.duration,
            a.return_latency,
            a.geometry.map(|g| (g.base, g.distance, g.speed)),
            a.shared,
            a.machine_offset,
            a.in_stores,
            a.out_stores,
            stacks(&a.inputs),
            stacks(&a.outputs),
        ));
    }
    for (i, d) in bp.storages.iter().enumerate() {
        s.push_str(&format!(
            "store {i} {} cap={} shared={} policy={} slots={:?} initial[{}] \
             clients={:?} order={:?} takers={:?} givers={:?} qoff={}\n",
            d.name,
            d.capacity,
            d.shared,
            d.policy.label(),
            d.slots.iter().map(|&x| name(x)).collect::<Vec<_>>(),
            stacks(&d.initial),
            d.clients,
            d.order,
            d.takers,
            d.givers,
            d.qty_offset,
        ));
    }
    s
}

// ============================================================ the round trip

#[test]
fn every_config_survives_the_document() {
    for path in CONFIGS {
        let src = std::fs::read_to_string(path).unwrap();
        let before = load(path);

        let mut doc = Graph::from_program(&before);
        doc.apply_positions(&src);
        let emitted = doc.emit();
        let after = dsl::parse(&emitted)
            .unwrap_or_else(|e| panic!("{path}: the emitted source does not parse: {e}\n{emitted}"));

        assert_eq!(before.items, after.items, "{path}: item ids moved");
        assert_eq!(
            fingerprint(effective(&before), &before.items),
            fingerprint(effective(&after), &after.items),
            "{path}: the plant changed shape on the way through the document",
        );
    }
}

#[test]
fn the_round_trip_plant_behaves_identically() {
    for path in CONFIGS {
        let before = load(path);
        let emitted = Graph::from_program(&before).emit();
        let after = dsl::parse(&emitted).unwrap();

        let (a, b) = (effective(&before), effective(&after));
        let n = before.items.len();
        for &t in PROBES {
            let mut pa = Pop::new(a, n);
            pa.run_until(t);
            let mut pb = Pop::new(b, n);
            pb.run_until(t);
            assert_eq!(pa.signature(), pb.signature(), "{path}: states differ at t={t}");
            assert_eq!(pa.c, pb.c, "{path}: counters differ at t={t}");

            // And the same again through the decomposition, because a permuted
            // storage list would change which region a bay lands in.
            let (plan_a, plan_b) = (rooms::plan(a), rooms::plan(b));
            assert_eq!(
                plan_a.graph.regions.len(),
                plan_b.graph.regions.len(),
                "{path}: the region count changed"
            );
            let mut ra = Room::new(&plan_a, n);
            ra.run_until(t);
            let mut rb = Room::new(&plan_b, n);
            rb.run_until(t);
            assert_eq!(ra.signature(a), rb.signature(b), "{path}: room states differ at t={t}");
        }
    }
}

#[test]
fn the_document_survives_json() {
    for path in CONFIGS {
        let prog = load(path);
        let doc = Graph::from_program(&prog);
        let text = doc.to_json().to_string();
        let parsed = json::parse(&text).unwrap_or_else(|e| panic!("{path}: {e}"));
        let back = Graph::from_json(&parsed).unwrap_or_else(|e| panic!("{path}: {e}"));
        assert_eq!(doc, back, "{path}: the document did not survive the wire");
        // And the plant it emits is still the same plant.
        assert_eq!(doc.emit(), back.emit(), "{path}: emitted source differs after a round trip");
    }
}

#[test]
fn positions_round_trip_through_the_source() {
    let prog = load("configs/11-railchain.factory");
    let mut doc = Graph::from_program(&prog);
    doc.nodes[0].x = 1234.0;
    doc.nodes[0].y = -56.0;
    let src = doc.emit();
    let reparsed = dsl::parse(&src).expect("a document with positions is still a valid plant");
    let mut back = Graph::from_program(&reparsed);
    back.apply_positions(&src);
    assert_eq!(back.nodes[0].x, 1234.0);
    assert_eq!(back.nodes[0].y, -56.0);
}

// ================================================================ snapshots

fn snapshot_at(path: &str, t: Tick) -> json::Json {
    let prog = load(path);
    let bp = effective(&prog);
    let plan = rooms::plan(bp);
    let mut room = Room::new(&plan, prog.items.len());
    room.run_until(t);
    let text = snap::render(&prog, bp, &plan, &room, t).to_string();
    json::parse(&text).expect("a snapshot is valid JSON")
}

#[test]
fn a_snapshot_is_the_same_snapshot_however_it_was_reached() {
    // Reached in one jump, and reached by walking. The renderer must not be
    // able to tell, or "seek to any tick" is a different simulation from
    // "play from the start".
    let prog = load("configs/11-railchain.factory");
    let bp = effective(&prog);
    let plan = rooms::plan(bp);
    let n = prog.items.len();

    let mut walked = Room::new(&plan, n);
    for t in (0..=8_000).step_by(250) {
        walked.run_until(t);
    }
    let a = snap::render(&prog, bp, &plan, &walked, 8_000).to_string();

    let mut jumped = Room::new(&plan, n);
    jumped.run_until(8_000);
    let b = snap::render(&prog, bp, &plan, &jumped, 8_000).to_string();

    // The two Rooms genuinely got there differently -- otherwise this test
    // would pass for the wrong reason.
    assert!(
        walked.steps > jumped.steps,
        "both runs took {} advances; the walk was not a walk",
        jumped.steps
    );
    assert_eq!(a, b, "the snapshot depends on how the Room got there");
}

#[test]
fn nothing_in_the_air_has_already_landed() {
    // The renderer interpolates a vehicle between the tick it left and the
    // tick it lands. That is only meaningful if every leg it is handed
    // actually straddles the snapshot's tick.
    for path in ["configs/07-transport.factory", "configs/11-railchain.factory", "configs/15-continent.factory"] {
        for &t in &[1u64, 137, 2_000, 8_000, 20_000] {
            let snap = snapshot_at(path, t);
            let links = snap.at("links").as_arr();
            assert!(!links.is_empty(), "{path} has transports and the snapshot shows none");
            for link in links {
                let name = link.at("name").as_str().unwrap_or("?");
                let mut vehicles = link.at("waitingToLoad").as_u64().unwrap()
                    + link.at("waitingToUnload").as_u64().unwrap();
                for f in link.at("flights").as_arr() {
                    let (d, a) = (f.at("depart").as_u64().unwrap(), f.at("arrive").as_u64().unwrap());
                    assert!(
                        d <= t && t < a,
                        "{path} t={t}: {name} has a leg {d}..{a} that does not straddle the tick"
                    );
                    vehicles += f.at("n").as_u64().unwrap();
                }
                // Every vehicle is in exactly one of the four places.
                let declared = link.at("vehicles").as_u64().unwrap();
                assert_eq!(
                    vehicles, declared,
                    "{path} t={t}: {name} has {vehicles} vehicles accounted for, not {declared}"
                );
            }
        }
    }
}

#[test]
fn a_snapshot_reports_the_state_the_solver_reports() {
    // The numbers on the screen are the numbers in the population buckets, not
    // a rounded or aggregated retelling of them.
    let path = "configs/11-railchain.factory";
    let t = 8_000;
    let prog = load(path);
    let bp = effective(&prog);
    let mut mono = Pop::new(bp, prog.items.len());
    mono.run_until(t);

    let shot = snapshot_at(path, t);
    for (c, a) in bp.actors.iter().enumerate() {
        let seen = shot
            .at("classes")
            .as_arr()
            .iter()
            .find(|j| j.at("name").as_str() == Some(a.name.as_str()))
            .unwrap_or_else(|| panic!("{} is missing from the snapshot", a.name));
        let cp = &mono.classes[c];
        assert_eq!(seen.at("idle").as_u64(), Some(cp.starved), "{}: idle", a.name);
        assert_eq!(seen.at("blocked").as_u64(), Some(cp.done), "{}: blocked", a.name);
        assert_eq!(seen.at("busy").as_u64(), Some(cp.working_total()), "{}: busy", a.name);
        assert_eq!(seen.at("cycles").as_u64(), Some(mono.c.cycles[c]), "{}: cycles", a.name);
    }
    for (s, sd) in bp.storages.iter().enumerate() {
        let seen = shot
            .at("storages")
            .as_arr()
            .iter()
            .find(|j| j.at("name").as_str() == Some(sd.name.as_str()))
            .unwrap();
        assert_eq!(seen.at("used").as_u64(), Some(mono.storage_used(s)), "{}: used", sd.name);
    }
}

#[test]
fn a_plant_built_one_node_at_a_time_runs() {
    // What the canvas does, in the order a pair of hands does it: place five
    // things, wire them up, compile, run. If this ever stops working the
    // builder is decoration.
    use temporal_rooms::graph::{Amount, Edge, Kind, Node};

    let mut g = Graph { name: "Built".into(), items: vec!["Ore".into(), "Plate".into()], ..Graph::default() };
    let amount = |item: &str, qty| Amount { item: item.into(), qty };

    let mut miner = Node::new("Miner", Kind::Source);
    miner.count = 4;
    miner.duration = 30;
    miner.outputs = vec![amount("Ore", 50)];

    let mut bay = Node::new("OreBay", Kind::Storage);
    bay.capacity = 5_000;

    let mut smelter = Node::new("Smelter", Kind::Process);
    smelter.count = 10_000;
    smelter.duration = 40;
    smelter.inputs = vec![amount("Ore", 10)];
    smelter.outputs = vec![amount("Plate", 10)];

    let mut out = Node::new("PlateBay", Kind::Storage);
    out.capacity = 5_000;

    let mut depot = Node::new("Depot", Kind::Sink);
    depot.duration = 60;
    depot.inputs = vec![amount("Plate", 100)];

    g.nodes = vec![miner, bay, smelter, out, depot];
    for (from, to) in [("Miner", "OreBay"), ("OreBay", "Smelter"), ("Smelter", "PlateBay"), ("PlateBay", "Depot")] {
        g.edges.push(Edge { from: from.into(), to: to.into(), item: None });
    }

    let src = g.emit();
    let prog = dsl::parse(&src).unwrap_or_else(|e| panic!("the built plant does not compile: {e}
{src}"));
    let bp = effective(&prog);
    assert_eq!(bp.machines, 4 + 10_000 + 1);

    let plan = rooms::plan(bp);
    let mut room = Room::new(&plan, prog.items.len());
    room.run_until(2_000);
    let shot = json::parse(&snap::render(&prog, bp, &plan, &room, 2_000).to_string()).unwrap();

    // Four miners at 50 ore per 30 ticks cannot keep ten thousand smelters fed,
    // so most of them are waiting -- which is a fact about the plant that was
    // built, and exactly the sort of thing the inspector is for.
    let smelter = shot
        .at("classes")
        .as_arr()
        .iter()
        .find(|c| c.at("name").as_str() == Some("Smelter"))
        .expect("the smelter is in the snapshot");
    assert_eq!(smelter.at("count").as_u64(), Some(10_000));
    assert!(smelter.at("idle").as_u64().unwrap() > 9_000, "a starved plant should look starved");
    assert!(
        shot.at("items").as_arr().iter().any(|i| i.at("produced").as_u64().unwrap_or(0) > 0),
        "nothing was ever produced"
    );
}

// ================================================================== the wire

#[test]
fn the_exported_trace_carries_its_own_proof() {
    let text = web::export("configs/11-railchain.factory", &[0, 1_000, 4_000, 8_000])
        .expect("export works");
    let doc = json::parse(&text).expect("the trace is valid JSON");
    assert_eq!(doc.at("verified").as_bool(), Some(true), "the exported trace failed its own check");
    assert_eq!(doc.at("frames").as_arr().len(), 4);
    assert!(!doc.at("timetable").at("advances").as_arr().is_empty(), "no advances recorded");
    // A trace has to be openable by something with no simulator in it, so the
    // document it carries must stand on its own.
    let g = Graph::from_json(doc.at("graph")).expect("the trace carries a usable document");
    assert!(!g.nodes.is_empty());
}

#[test]
fn the_scheduler_log_adds_up() {
    let prog = load("configs/15-continent.factory");
    let bp = effective(&prog);
    let plan = rooms::plan(bp);
    let mut room = Room::new(&plan, prog.items.len());
    room.trace = Some(Vec::new());
    room.run_until(20_000);
    let log = room.trace.clone().unwrap();
    assert_eq!(log.len() as u64, room.steps, "the log missed an advance");

    // A region's advances are contiguous and monotone: each one starts where
    // that region's last one stopped. This is the property that makes the
    // timetable a picture of the run rather than a decoration.
    let mut clock = vec![0u64; plan.regions()];
    for a in &log {
        assert_eq!(a.from, clock[a.region], "region {} jumped a gap", a.region);
        assert!(a.to > a.from, "region {} advanced by nothing", a.region);
        clock[a.region] = a.to;
    }
    for (r, &c) in clock.iter().enumerate() {
        assert_eq!(c, 20_000, "region {r} did not finish at the horizon");
    }
}

#[test]
fn a_population_of_storages_is_refused_rather_than_unusable() {
    // Prototype 0 tried to draw one and found the construct was dead: a wire
    // names the group, so `storage Bay x3` wires all three at once, which the
    // one-bay rule then refuses -- and the instance names it invented began
    // with the comment character, so the error's own suggested fix could not
    // be typed. It is now refused where it is written.
    let src = "item Ore
               blueprint P {
                 source Miner { produces 10 Ore every 60 ticks }
                 storage Bay x3 { capacity 1000 }
                 wire Miner -> Bay
               }
               deploy 1 x P
";
    let e = dsl::parse(src).expect_err("a population of bays is not a thing");
    assert!(e.msg.contains("cannot be wired"), "unhelpful message: {}", e.msg);
    assert!(e.line > 0, "the error should point at the declaration");

    // One bay each is the same plant, spelled honestly, and it compiles.
    let ok = "item Ore
              blueprint P {
                source Miner { produces 10 Ore every 60 ticks }
                storage Bay1 { capacity 1000 }
                wire Miner -> Bay1
              }
              deploy 1 x P
";
    assert!(dsl::parse(ok).is_ok());
}

#[test]
fn json_survives_the_awkward_values() {
    // Counters on a billion-machine plant leave the range a JS number holds
    // exactly, and the wire format has to say so rather than round.
    let big = json::Json::big((1u128 << 60) + 7);
    assert_eq!(big.to_string(), "\"1152921504606846983\"");
    assert_eq!(big.as_u64(), Some(1_152_921_504_606_846_983));
    assert_eq!(json::Json::big(42).to_string(), "42");

    let round = |s: &str| json::parse(s).unwrap().to_string();
    assert_eq!(round(r#"{"a":[1,-2,3.5,true,null,"x\ny"]}"#), r#"{"a":[1,-2,3.5,true,null,"x\ny"]}"#);
    assert_eq!(json::parse(r#""é""#).unwrap().as_str(), Some("é"));
    assert_eq!(json::parse(r#""🚀""#).unwrap().as_str(), Some("🚀"));
    // A node name with a closing script tag in it must not escape the page an
    // exported trace is embedded in.
    let tag = json::Json::from("</script>").to_string();
    assert_eq!(tag, r##""\u003c/script\u003e""##);
    assert_eq!(json::parse(&tag).unwrap().as_str(), Some("</script>"));
    assert!(json::parse("{oops}").is_err());
}
