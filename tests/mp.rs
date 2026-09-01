//! Prototype 2: two players, one factory, and the properties that make it one
//! factory rather than two.
//!
//! The tests fall into three groups, and only the third one is new:
//!
//! ```text
//!   the compiler   a game document lowers to a plant the solver will run
//!   the machine    a design lowers to a recipe, exactly, and the same way twice
//!   the room       three reconstructions of one command stream agree
//! ```
//!
//! The third group is the experiment. Everything in it is written against
//! [`Room`] with a hand-driven clock, because a real-time system tested
//! against a real clock is a test that passes on a fast machine -- and what is
//! being proved has nothing to do with wall time.

use temporal_rooms::live::{self, Log};
use temporal_rooms::machine::design::Design;
use temporal_rooms::mp::cmd::{Act, Cmd};
use temporal_rooms::mp::goal::{self, Goal, Shape, TEMPLATES};
use temporal_rooms::mp::kit::{proto, Role, PROTOS};
use temporal_rooms::mp::room::{Room, Sim};
use temporal_rooms::mp::world::{stock_design, Id, World};
use temporal_rooms::mp::{lower, secs, Rng};
use temporal_rooms::json;

// ============================================================== the machines

/// Every machine in the catalogue compiles to a recipe, and the recipe is a
/// fact about the design rather than a number anybody typed.
#[test]
fn every_catalogue_machine_lowers() {
    for p in PROTOS.iter().filter(|p| p.role == Role::Machine) {
        let d = stock_design(p.tag).unwrap_or_else(|e| panic!("{}: {e}", p.tag));
        let m = lower::lower(&d).unwrap_or_else(|e| panic!("{}: {e}", p.tag));
        assert!(m.settled, "{}: its orbit never closed", p.tag);
        assert!(!m.gives.is_empty(), "{} makes nothing", p.tag);
        assert!(m.cycle > 0, "{} has a zero-length cycle", p.tag);
        assert!(
            m.takes.iter().all(|(_, q)| *q > 0) && m.gives.iter().all(|(_, q)| *q > 0),
            "{} has a zero quantity in its recipe",
            p.tag
        );
        // The world footprint the palette advertises has to be the one the
        // design actually occupies, or a player is shown a shape that will not
        // fit where they were told it would.
        assert_eq!((m.w, m.h), (p.w, p.h), "{}: the catalogue's footprint is wrong", p.tag);
    }
}

/// The same design lowers the same way, every time, on every replica.
#[test]
fn lowering_is_a_function_of_the_design() {
    for p in PROTOS.iter().filter(|p| p.role == Role::Machine) {
        let a = lower::lower(&stock_design(p.tag).unwrap()).unwrap();
        let b = lower::lower(&Design::parse(&stock_design(p.tag).unwrap().emit()).unwrap()).unwrap();
        assert_eq!(a, b, "{} lowered differently after a round trip through its own file", p.tag);
    }
}

/// The primitive cycle is a reduction, not a rounding: the rate is preserved
/// exactly.
#[test]
fn the_primitive_cycle_keeps_the_rate() {
    for p in PROTOS.iter().filter(|p| p.role == Role::Machine) {
        let d = stock_design(p.tag).unwrap();
        let m = lower::lower(&d).unwrap();
        let c = temporal_rooms::machine::orbit::compile(&d).unwrap();
        let r = temporal_rooms::machine::eval::report(&d, &c);
        for s in &r.gives {
            let item = lower::item_of(&s.what);
            let per_orbit = s.rate.num;
            let orbit_ticks = s.rate.den;
            let got = m.gives.iter().filter(|(i, _)| i == item).map(|(_, q)| *q as u128).sum::<u128>();
            // qty / cycle == per_orbit / orbit, in integers.
            assert_eq!(
                got * orbit_ticks * 60,
                per_orbit * m.cycle as u128,
                "{}: {item} came out at a different rate",
                p.tag
            );
        }
    }
}

// =============================================================== the world

fn tiny_world() -> (World, Vec<Id>) {
    let mut w = World::new("Test");
    let mine = w.place(proto("oremine").unwrap(), 0, 0, 0, None, None, 0, 0).unwrap();
    let bay = w.place(proto("bay").unwrap(), 10, 0, 0, None, None, 0, 0).unwrap();
    let depot = w
        .place(proto("depot").unwrap(), 20, 0, 0, Some("IronOre".into()), None, 0, 0)
        .unwrap();
    (w, vec![mine, bay, depot])
}

/// A document that is wired up compiles to a plant the solver runs.
#[test]
fn a_wired_world_runs() {
    let (mut w, ids) = tiny_world();
    w.connect(ids[0], ids[1], "IronOre").unwrap();
    w.connect(ids[1], ids[2], "IronOre").unwrap();
    let b = w.compile();
    assert!(b.runnable);
    assert!(b.idle.is_empty(), "something was left out: {:?}", b.idle);
    let log = Log::new(b.graph.clone());
    let c = live::carry_at(&log, secs(60)).expect("the plant would not run");
    assert!(c.consumed.get("IronOre").copied().unwrap_or(0) > 0, "nothing was delivered");
}

/// A half-built factory is not a compile error. It is a factory that is half
/// built, and the parts that are finished keep running.
#[test]
fn an_unwired_machine_is_left_out_rather_than_fatal() {
    let (mut w, ids) = tiny_world();
    w.connect(ids[0], ids[1], "IronOre").unwrap();
    w.connect(ids[1], ids[2], "IronOre").unwrap();
    let press = w
        .place(
            proto("stamping").unwrap(),
            40,
            0,
            0,
            None,
            Some(stock_design("stamping").unwrap()),
            0,
            0,
        )
        .unwrap();
    let b = w.compile();
    assert!(b.runnable, "one unwired press stopped the whole factory");
    assert!(b.why_idle(press).is_some(), "the press should have been left out and told why");
    assert!(live::carry_at(&Log::new(b.graph.clone()), secs(30)).is_ok());
}

/// Two bays feeding one machine the same item is the one thing the language
/// below cannot arbitrate, so it is refused where the player can see it.
#[test]
fn one_bay_per_item_per_machine() {
    let (mut w, ids) = tiny_world();
    let press = w
        .place(
            proto("stamping").unwrap(),
            40,
            0,
            0,
            None,
            Some(stock_design("stamping").unwrap()),
            0,
            0,
        )
        .unwrap();
    let b1 = w.place(proto("bay").unwrap(), 50, 0, 0, None, None, 0, 0).unwrap();
    let b2 = w.place(proto("bay").unwrap(), 60, 0, 0, None, None, 0, 0).unwrap();
    w.connect(b1, press, "IronBillet").unwrap();
    let e = w.connect(b2, press, "IronBillet").unwrap_err();
    assert!(e.contains("already has a bay"), "{e}");
    let _ = ids;
}

/// Nothing may be placed on top of anything else, and nothing may leave the
/// plot.
#[test]
fn placement_is_refused_rather_than_fudged() {
    let (mut w, _) = tiny_world();
    assert!(w.place(proto("bay").unwrap(), 10, 0, 0, None, None, 0, 0).is_err());
    assert!(w.place(proto("bay").unwrap(), -2, 0, 0, None, None, 0, 0).is_err());
    assert!(w.place(proto("bay").unwrap(), 999, 0, 0, None, None, 0, 0).is_err());
    assert!(w.place(proto("bay").unwrap(), 40, 40, 0, None, None, 0, 0).is_ok());
}

/// A transport's latency comes from where its two ends are, and from nothing
/// else.
#[test]
fn distance_is_derived_from_the_layout() {
    let mut w = World::new("Test");
    let a = w.place(proto("bay").unwrap(), 0, 0, 0, None, None, 0, 0).unwrap();
    let near = w.place(proto("bay").unwrap(), 12, 0, 0, None, None, 0, 0).unwrap();
    let far = w.place(proto("bay").unwrap(), 100, 0, 0, None, None, 0, 0).unwrap();
    let mine = w.place(proto("oremine").unwrap(), 0, 20, 0, None, None, 0, 0).unwrap();
    w.connect(mine, a, "IronOre").unwrap();
    let h1 = w.link(proto("belt").unwrap(), a, near, "IronOre", 0, 0).unwrap();
    let h2 = w.link(proto("belt").unwrap(), a, far, "IronOre", 0, 0).unwrap();
    let span = |id: Id| w.span(w.haul(id).unwrap());
    assert!(span(h1) < span(h2), "the far bay was not further away");
    let b = w.compile();
    let node = |name: &str| b.graph.nodes.iter().find(|n| n.name == name).unwrap().clone();
    let (n1, n2) = (node(&w.haul(h1).unwrap().name), node(&w.haul(h2).unwrap().name));
    assert!(n1.duration < n2.duration, "the longer belt was not slower");
    assert!(n1.geometry.is_some(), "the latency was not derived from a distance");
}

/// The document survives the wire. A joining client is handed exactly this.
#[test]
fn a_world_survives_json() {
    let (mut w, ids) = tiny_world();
    w.connect(ids[0], ids[1], "IronOre").unwrap();
    w.connect(ids[1], ids[2], "IronOre").unwrap();
    w.place(
        proto("crusher").unwrap(),
        40,
        20,
        1,
        None,
        Some(stock_design("crusher").unwrap()),
        0,
        0,
    )
    .unwrap();
    let build = w.compile();
    let text = w.to_json(&build, true).to_string();
    let back = World::from_json(&json::parse(&text).unwrap()).unwrap();
    assert_eq!(w.signature(), back.signature(), "a world did not survive JSON");
    assert_eq!(w.compile().graph.emit(), back.compile().graph.emit());
    // A *frame* leaves the designs out, and must therefore be refused as a
    // snapshot rather than quietly rebuilt with the catalogue's designs.
    let frame = w.to_json(&build, false).to_string();
    assert!(
        World::from_json(&json::parse(&frame).unwrap()).is_err(),
        "a frame was accepted as a snapshot"
    );
}

// ================================================================== goals

/// A goal is a pure function of its seed, and every template produces one.
#[test]
fn goals_are_a_function_of_the_seed() {
    for seed in [0u64, 1, 7, 42, 9_999, u64::MAX / 3] {
        let a = Goal::of_seed(seed, None);
        let b = Goal::of_seed(seed, None);
        assert_eq!(a.brief(), b.brief());
        assert_eq!(a.template, b.template);
    }
    for t in TEMPLATES {
        let g = Goal::of_seed(3, Some(t.id));
        assert_eq!(g.template, t.id);
        assert!(!g.brief().is_empty());
        assert!(!g.starting_kit().is_empty(), "{}: a room with nothing in it", t.id);
    }
}

/// Nothing is drawn from an unconstrained range, and every number a template
/// produces is inside the one it declared.
#[test]
fn goal_numbers_stay_inside_their_ranges() {
    for t in TEMPLATES {
        for seed in 0..64u64 {
            let g = Goal::of_seed(seed, Some(t.id));
            let sane = |n: u64| n > 0 && n < 1_000_000;
            match &g.shape {
                Shape::Deliver { qty, .. } => assert!(sane(*qty)),
                Shape::DeliverPair { a, b } => assert!(sane(a.1) && sane(b.1)),
                Shape::Sustain { per_sec, secs: s, .. } => assert!(sane(*per_sec) && *s <= 120),
                Shape::SustainPair { a, b, secs: s } => {
                    assert!(sane(a.1) && sane(b.1) && *s <= 120)
                }
                Shape::Frugal { qty, cap_qty, .. } => assert!(sane(*qty) && sane(*cap_qty)),
                Shape::Compact { per_sec, tiles, .. } => {
                    assert!(sane(*per_sec) && *tiles > 100)
                }
                Shape::CleanPower { mw, max_waste_pct, .. } => {
                    assert!(sane(*mw) && *max_waste_pct <= 100)
                }
                // Prototype 3's authored rooms take no numbers from the seed
                // at all, so there is nothing here to be outside a range --
                // only an ordering to be sensible about.
                Shape::Peak { base, peak, spill, hold, every, secs: s } => {
                    assert!(sane(*base) && sane(*peak) && sane(*spill));
                    assert!(base <= spill && spill < peak, "{}: a peak below its spill", t.id);
                    assert!(*hold >= 1 && *hold < *every);
                    assert!(*every > 0 && *every <= *s && *s <= 120);
                }
                Shape::Both(..) => {}
            }
        }
    }
}

/// A rate is measured over a window on the lattice, so a room asked at tick
/// 12,345 and one asked at 12,400 agree about it.
#[test]
fn a_rate_is_measured_on_the_lattice() {
    let mut acct = goal::Acct::default();
    let mut shipped = std::collections::BTreeMap::new();
    for s in 1..=90u64 {
        shipped.insert("Gear".to_string(), s * 10);
        acct.count(secs(s), &shipped, &std::collections::BTreeMap::new(), 0, 0, 0);
    }
    let g = Goal::of_seed(1, Some("steady-gears"));
    let p = goal::evaluate(&g, &acct);
    assert!(!p.warming, "ninety seconds of history was not enough for the window");
    assert_eq!(p.lines[0].have.round(), 10.0, "ten a second was not measured as ten a second");
}

// ================================================================== the room

fn wired_room(seed: u64) -> (Room, u32, u32) {
    let mut r = Room::open(seed, Some("first-gears"));
    r.start_manual();
    let a = r.join("Ada").unwrap();
    r.set_now(secs(2));
    let b = r.join("Bee").unwrap();

    let tag = |r: &Room, t: &str, n: usize| -> Id {
        r.host.world.installs.iter().filter(|i| i.proto.tag == t).nth(n).map(|i| i.id).unwrap()
    };
    let caster = tag(&r, "billetcaster", 0);
    let coal = tag(&r, "coalpit", 0);
    let depot = tag(&r, "depot", 0);
    let bays: Vec<Id> =
        r.host.world.installs.iter().filter(|i| i.proto.tag == "bay").map(|i| i.id).collect();

    r.set_now(secs(4));
    r.submit(a, Act::PlaceMachine {
        proto: "machining".into(),
        x: 40,
        y: 6,
        face: 0,
        item: None,
        design: None,
    })
    .unwrap();
    let cell = r.host.world.installs.last().unwrap().id;
    r.submit(a, Act::PlaceStorage { proto: "bay".into(), x: 60, y: 6, face: 0 }).unwrap();
    let gearbay = r.host.world.installs.last().unwrap().id;
    r.submit(a, Act::PlaceMachine {
        proto: "steamplant".into(),
        x: 40,
        y: 26,
        face: 0,
        item: None,
        design: None,
    })
    .unwrap();
    let plant = r.host.world.installs.last().unwrap().id;
    r.submit(a, Act::PlaceStorage { proto: "yard".into(), x: 60, y: 26, face: 0 }).unwrap();
    let powerbay = r.host.world.installs.last().unwrap().id;

    let water = tag(&r, "waterpump", 0);
    let mut wire = |from: Id, to: Id, item: &str| {
        r.submit(b, Act::CreateConnection { from, to, item: item.into() }).unwrap();
    };
    wire(caster, bays[0], "IronBillet");
    wire(coal, bays[1], "Coal");
    wire(water, bays[2], "Water");
    wire(bays[0], cell, "IronBillet");
    wire(cell, gearbay, "Gear");
    wire(gearbay, depot, "Gear");
    wire(bays[1], plant, "Coal");
    wire(bays[2], plant, "Water");
    wire(plant, powerbay, "Power");
    wire(powerbay, cell, "Power");
    assert!(r.host.world.compile().idle.is_empty(), "the test factory does not run");
    (r, a, b)
}

/// The proof, in the smallest form it comes in: three reconstructions of one
/// command stream, advanced at *different rates*, agreeing at every second.
///
/// The different rates are the point. An earlier version of the room closed
/// its books at the end of whatever interval a client happened to ask about,
/// which made a replica polled every two seconds disagree with one polled
/// every four -- the same room, hashed with a different amount of the future
/// in it. Nothing about that is visible in a test where everybody polls
/// together.
#[test]
fn replicas_agree_however_often_they_ask() {
    let (mut r, a, b) = wired_room(5);
    for k in 1..=60u64 {
        r.set_now(secs(20 + k * 3));
        r.sync(a).unwrap();
        if k % 5 == 0 {
            r.sync(b).unwrap();
        }
    }
    r.sync(b).unwrap();
    let t = r
        .host
        .probe()
        .min(r.player(a).unwrap().sim.probe())
        .min(r.player(b).unwrap().sim.probe());
    let hs = r.hashes(t);
    assert!(t > 0, "nothing was ever checked");
    assert!(
        hs.iter().all(|(_, h)| h.is_some() && *h == hs[0].1),
        "the replicas disagree at tick {t}: {hs:?}"
    );
    assert_eq!(r.player(a).unwrap().mismatches, 0);
    assert_eq!(r.player(b).unwrap().mismatches, 0);
    assert_eq!(r.player(a).unwrap().resyncs, 0);
}

/// A player who arrives at tick 40,000 reconstructs the room from a snapshot
/// and a tail of commands, and agrees with everybody who was there.
#[test]
fn a_late_joiner_reproduces_the_room() {
    let (mut r, a, _) = wired_room(9);
    for k in 1..=40u64 {
        r.set_now(secs(20 + k * 5));
        r.sync(a).unwrap();
    }
    let late = r.join("Cy").unwrap();
    assert!(r.player(late).unwrap().joined > secs(200), "the room was not old enough");
    for k in 1..=10u64 {
        r.set_now(secs(230 + k * 4));
        r.sync(a).unwrap();
        r.sync(late).unwrap();
    }
    let t = r.host.probe().min(r.player(late).unwrap().sim.probe());
    assert_eq!(
        r.host.check(t),
        r.player(late).unwrap().sim.check(t),
        "the late joiner rebuilt a different room"
    );
    assert_eq!(r.player(late).unwrap().mismatches, 0);
    // And the books came with it: a joiner that started counting from zero
    // would disagree about everything the room had already delivered.
    assert_eq!(
        r.host.acct.shipped,
        r.player(late).unwrap().sim.acct.shipped,
        "the joiner's books do not match"
    );
}

/// Joining does not stop, pause or rewind the host.
#[test]
fn joining_does_not_disturb_the_host() {
    let (mut r, a, _) = wired_room(4);
    r.set_now(secs(120));
    r.sync(a).unwrap();
    let before = (r.host.now, r.host.probe(), r.host.acct.shipped.clone());
    let _ = r.join("Cy").unwrap();
    assert_eq!(r.host.now, before.0, "the host's clock moved for a joiner");
    assert_eq!(r.host.probe(), before.1);
    assert_eq!(r.host.acct.shipped, before.2);
}

/// Editing a machine does not touch the machine that is running.
#[test]
fn a_draft_is_not_the_machine() {
    let (mut r, a, _) = wired_room(6);
    let cell = r
        .host
        .world
        .installs
        .iter()
        .find(|i| i.proto.tag == "machining")
        .map(|i| i.id)
        .unwrap();
    r.set_now(secs(30));
    r.sync(a).unwrap();
    let before = r.host.world.get(cell).unwrap().lowered.clone();
    r.submit(a, Act::OpenDesign { id: cell }).unwrap();
    r.set_now(secs(32));
    r.submit(a, Act::PlaceComponent {
        id: cell,
        kind: "motor".into(),
        x: 0,
        y: 10,
        z: 0,
        face: None,
    })
    .unwrap();
    assert_eq!(
        before,
        r.host.world.get(cell).unwrap().lowered,
        "the running machine changed while a draft was open"
    );
    assert!(r.host.world.get(cell).unwrap().draft.is_some());

    // And the commit is one command, at one tick.
    let draft = r.host.world.get(cell).unwrap().draft.clone().unwrap();
    r.set_now(secs(40));
    let c = r.submit(a, Act::CommitMachineDesign { id: cell, design: draft }).unwrap();
    assert_eq!(c.tick, secs(40));
    assert_ne!(before, r.host.world.get(cell).unwrap().lowered, "the commit changed nothing");
    assert!(r.host.world.get(cell).unwrap().draft.is_none(), "the draft outlived the commit");
}

/// One editor at a time, and the lock is in the document so that everybody can
/// see whose it is.
#[test]
fn a_machine_has_one_editor() {
    let (mut r, a, b) = wired_room(8);
    let cell = r
        .host
        .world
        .installs
        .iter()
        .find(|i| i.proto.tag == "machining")
        .map(|i| i.id)
        .unwrap();
    r.set_now(secs(20));
    r.submit(a, Act::OpenDesign { id: cell }).unwrap();
    assert!(r.submit(b, Act::OpenDesign { id: cell }).is_err());
    assert!(r.submit(b, Act::DeleteMachine { id: cell }).is_err());
    assert!(r
        .submit(b, Act::PlaceComponent {
            id: cell,
            kind: "motor".into(),
            x: 0,
            y: 12,
            z: 0,
            face: None
        })
        .is_err());
    r.set_now(secs(22));
    r.submit(a, Act::CloseDesign { id: cell, keep: false }).unwrap();
    assert!(r.submit(b, Act::OpenDesign { id: cell }).is_ok());
}

/// A deleted thing leaves a ghost, and restoring it is a new placement rather
/// than a rewind.
#[test]
fn a_ghost_restores_forward() {
    let (mut r, a, _) = wired_room(3);
    let bay = r
        .host
        .world
        .installs
        .iter()
        .find(|i| i.proto.tag == "yard")
        .map(|i| i.id)
        .unwrap();
    r.set_now(secs(30));
    r.submit(a, Act::DeleteStorage { id: bay }).unwrap();
    assert_eq!(r.host.ghosts.len(), 1);
    let g = r.host.ghosts[0].clone();
    r.set_now(secs(35));
    r.submit(a, g.restore()).unwrap();
    let back = r.host.world.installs.last().unwrap();
    assert_eq!((back.x, back.y), (g.x, g.y), "the restore did not go back where it was");
    assert_ne!(back.id, bay, "a restore is a new object, not the old one");
    assert_eq!(back.placed, secs(35), "the restore pretended the gap had not happened");
}

/// Commands are ordered by (tick, sequence), and by nothing else.
#[test]
fn ordering_is_tick_then_sequence() {
    let (mut r, a, b) = wired_room(2);
    r.set_now(secs(50));
    r.submit(a, Act::PlaceStorage { proto: "bay".into(), x: 90, y: 40, face: 0 }).unwrap();
    r.submit(b, Act::PlaceStorage { proto: "bay".into(), x: 90, y: 50, face: 0 }).unwrap();
    let n = r.log.len();
    let (x, y) = (&r.log[n - 2], &r.log[n - 1]);
    assert_eq!(x.tick, y.tick, "two commands in one instant landed at different ticks");
    assert!(x.seq < y.seq);
    assert!(r.log.windows(2).all(|w| w[0].key() < w[1].key()), "the log is not in order");
}

/// A refused command never enters the log, so replaying the log can never
/// reproduce it.
#[test]
fn a_refusal_leaves_no_trace() {
    let (mut r, a, _) = wired_room(12);
    r.set_now(secs(20));
    let before = (r.log.len(), r.seq, r.host.world.signature());
    assert!(r.submit(a, Act::PlaceStorage { proto: "bay".into(), x: -5, y: 0, face: 0 }).is_err());
    assert!(r.submit(a, Act::DeleteMachine { id: 99_999 }).is_err());
    assert!(r
        .submit(a, Act::CreateConnection { from: 1, to: 1, item: "Coal".into() })
        .is_err());
    assert_eq!(r.log.len(), before.0);
    assert_eq!(r.seq, before.1, "a refused command consumed a sequence number");
    assert_eq!(r.host.world.signature(), before.2, "a refused command changed the world");
}

/// Every command survives the wire, in both directions.
#[test]
fn commands_survive_json() {
    let acts = vec![
        Act::PlaceMachine {
            proto: "stamping".into(),
            x: 3,
            y: 4,
            face: 2,
            item: None,
            design: Some(stock_design("stamping").unwrap()),
        },
        Act::PlaceStorage { proto: "bay".into(), x: 1, y: 2, face: 1 },
        Act::DeleteMachine { id: 7 },
        Act::DeleteStorage { id: 8 },
        Act::CreateConnection { from: 1, to: 2, item: "Gear".into() },
        Act::DeleteConnection { from: 1, to: 2, item: "Gear".into() },
        Act::CreateWorldLink { proto: "rail".into(), from: 1, to: 2, item: "Coal".into() },
        Act::DeleteWorldLink { id: 9 },
        Act::OpenDesign { id: 3 },
        Act::CloseDesign { id: 3, keep: false },
        Act::PlaceComponent { id: 3, kind: "motor".into(), x: 1, y: 2, z: 3, face: Some(1) },
        Act::DeleteComponent { id: 3, unit: "MO1".into() },
        Act::TuneComponent {
            id: 3,
            unit: "R1".into(),
            field: "throttle".into(),
            value: "40".into(),
        },
        Act::ConnectComponent {
            id: 3,
            from: "A".into(),
            from_port: "out".into(),
            to: "B".into(),
            to_port: "in".into(),
        },
        Act::DisconnectComponent {
            id: 3,
            from: "A".into(),
            from_port: "out".into(),
            to: "B".into(),
            to_port: "in".into(),
        },
        Act::CommitMachineDesign { id: 3, design: stock_design("machining").unwrap() },
    ];
    for act in acts {
        let verb = act.verb();
        let c = Cmd { room: "ABCD12".into(), tick: 600, seq: 3, player: 2, act };
        let text = c.to_json().to_string();
        let back = Cmd::from_json(&json::parse(&text).unwrap())
            .unwrap_or_else(|e| panic!("{verb}: {e}"));
        assert_eq!(text, back.to_json().to_string(), "{verb} did not survive JSON");
    }
}

/// A snapshot rebuilds a room that agrees with the one it came from -- and it
/// goes through JSON on the way, because a snapshot that is really a clone
/// proves nothing about a snapshot that is really a socket.
#[test]
fn a_snapshot_is_the_room() {
    let (mut r, a, _) = wired_room(13);
    for k in 1..=30u64 {
        r.set_now(secs(20 + k * 4));
        r.sync(a).unwrap();
    }
    let text = r.host.snapshot().to_string();
    let mut copy = Sim::of_snapshot(&json::parse(&text).unwrap()).unwrap();
    // Both are advanced past a further lattice point, because a hash is only
    // ever taken at one: the copy was handed a carry from between two.
    let t = r.host.now + secs(9);
    copy.advance(t).unwrap();
    r.host.advance(t).unwrap();
    let at = r.host.probe();
    assert_eq!(copy.check(at), r.host.check(at), "a snapshot did not rebuild the room");
    assert_eq!(copy.world.signature(), r.host.world.signature());
}

/// Completion is recorded once, at the lattice point it became true, and the
/// room does not stop.
#[test]
fn a_finished_goal_does_not_stop_the_room() {
    let mut r = Room::open(21, Some("billet-stock"));
    r.start_manual();
    let a = r.join("Ada").unwrap();
    let caster = r
        .host
        .world
        .installs
        .iter()
        .find(|i| i.proto.tag == "billetcaster")
        .map(|i| i.id)
        .unwrap();
    let bay = r.host.world.installs.iter().find(|i| i.proto.tag == "bay").map(|i| i.id).unwrap();
    let depot = r.host.world.installs.iter().find(|i| i.proto.tag == "depot").map(|i| i.id).unwrap();
    r.set_now(secs(2));
    r.submit(a, Act::CreateConnection { from: caster, to: bay, item: "IronBillet".into() })
        .unwrap();
    r.submit(a, Act::CreateConnection { from: bay, to: depot, item: "IronBillet".into() })
        .unwrap();
    for k in 1..=90u64 {
        r.set_now(secs(4 + k * 8));
        r.sync(a).unwrap();
    }
    let p = r.host.progress();
    assert!(p.met, "the simplest goal in the catalogue was not met: {:?}", p.lines);
    let done = p.done.expect("nothing was recorded");
    assert_eq!(done.at % 60, 0, "completion was recorded off the lattice");
    let at = done.at;
    // Keep going. The room is still running, and the answer does not move.
    for k in 1..=10u64 {
        r.set_now(secs(730 + k * 10));
        r.sync(a).unwrap();
    }
    assert_eq!(r.host.progress().done.unwrap().at, at, "completion moved");
    assert!(r.now() > at, "the room stopped when the goal was met");
}

/// The room code and the goal are both functions of the seed, so a room worth
/// playing again is one number long.
#[test]
fn a_seed_is_a_room() {
    let a = Room::open(1234, None);
    let b = Room::open(1234, None);
    assert_eq!(a.code, b.code);
    assert_eq!(a.goal.brief(), b.goal.brief());
    assert_eq!(a.host.world.signature(), b.host.world.signature());
    let c = Room::open(1235, None);
    assert_ne!(a.code, c.code);
}

/// The random numbers are the same random numbers everywhere.
#[test]
fn the_generator_is_the_generator() {
    let mut r = Rng(7);
    let got: Vec<u64> = (0..4).map(|_| r.between(10, 20)).collect();
    let mut again = Rng(7);
    let same: Vec<u64> = (0..4).map(|_| again.between(10, 20)).collect();
    assert_eq!(got, same);
    assert!(got.iter().all(|n| (10..=20).contains(n)));
}
