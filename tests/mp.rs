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
use temporal_rooms::mp::cmd::{Act, Cmd, Effect};
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

/// A head on a patch of ground, which is now the only way to get material out
/// of the world at all.
fn head(w: &mut World, tag: &'static str, x: i32, y: i32, yields: u64) -> Id {
    let item = proto(tag).unwrap().extracts().expect("not an extraction head");
    w.seam(item, x, y, 8, 6, yields);
    w.place(proto(tag).unwrap(), x, y, 0, None, Some(stock_design(tag).unwrap()), 0, 0)
        .unwrap_or_else(|e| panic!("a {tag} on its own ground: {e}"))
}

fn tiny_world() -> (World, Vec<Id>) {
    let mut w = World::new("Test");
    let mine = head(&mut w, "oremine", 0, 0, 100);
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

/// A whole factory with no bays in it at all.
///
/// Three sources, two machines and a depot, wired to each other directly. This
/// is experiment 13's first change and the play session's second note: routing
/// every flow through a declared bay was logically immaculate and felt like
/// operating warehouse middleware. The buffers have not gone away -- the
/// solver still needs somewhere to put a cycle of output while the far end is
/// busy -- they have stopped being the player's problem.
fn bayless_world() -> (World, Vec<Id>) {
    let mut w = World::new("Test");
    let mut machine = |tag: &'static str, x: i32, y: i32| {
        w.place(proto(tag).unwrap(), x, y, 0, None, Some(stock_design(tag).unwrap()), 0, 0).unwrap()
    };
    let plant = machine("steamplant", 0, 0);
    let cell = machine("machining", 20, 0);
    let coal = head(&mut w, "coalpit", 0, 20, 100);
    let water = head(&mut w, "waterpump", 20, 20, 400);
    let caster = head(&mut w, "billetcaster", 40, 20, 100);
    let depot = w
        .place(proto("depot").unwrap(), 60, 20, 0, Some("Gear".into()), None, 0, 0)
        .unwrap();

    // Every one of these used to be a refusal.
    w.connect(coal, plant, "Coal").expect("coal into the plant");
    w.connect(water, plant, "Water").expect("water into the plant");
    w.connect(plant, cell, "Power").expect("power into the cell");
    w.connect(caster, cell, "IronBillet").expect("billet into the cell");
    w.connect(cell, depot, "Gear").expect("gears out to the depot");
    (w, vec![plant, cell, coal, water, caster, depot])
}

#[test]
fn a_factory_needs_no_bays_at_all() {
    let (w, ids) = bayless_world();
    assert!(
        !w.installs.iter().any(|i| i.is_storage()),
        "the bayless factory has a bay in it after all"
    );

    // One derived buffer per direct connection, each named after the
    // connection that implied it and holding what crosses it.
    let bridges = w.bridges();
    assert_eq!(bridges.len(), 5, "a direct connection did not imply a buffer");
    for b in &bridges {
        assert!(b.capacity > 0, "{} holds nothing", b.name);
        assert!(w.get(b.id).is_none(), "a derived buffer took an identity");
    }

    let b = w.compile();
    assert!(b.idle.is_empty(), "the bayless factory would not commission: {:?}", b.idle);
    assert!(b.runnable);
    for br in &bridges {
        let node = b.graph.nodes.iter().find(|n| n.name == br.name);
        assert!(node.is_some(), "{} is not in the plant", br.name);
        assert!(node.unwrap().holds.iter().any(|h| *h == br.item));
    }

    // And it runs, and gears come out of the far end.
    let c = live::carry_at(&Log::new(b.graph.clone()), secs(120)).expect("the plant would not run");
    assert!(
        c.consumed.get("Gear").copied().unwrap_or(0) > 0,
        "nothing came out of the far end: {:?}",
        c.consumed
    );
    let _ = ids;
}

/// Electricity does not queue in a shed.
///
/// The note that made this change unavoidable. Power had to be wired into a
/// bay and out again, which is not a thing a bay is, and the player had to do
/// it for every consumer in the room.
#[test]
fn power_does_not_go_through_a_shed() {
    let (w, ids) = bayless_world();
    let (plant, cell) = (ids[0], ids[1]);
    assert!(
        w.get(plant).unwrap().ports().iter().any(|p| p.out && p.item == "Power"),
        "the steam plant has no power output port"
    );
    assert!(
        w.get(cell).unwrap().ports().iter().any(|p| !p.out && p.item == "Power"),
        "the machining cell has no power input port"
    );
    assert_eq!(domain_tag("Power"), "electrical", "power stopped being electrical");
    // The connection is between the two machines, and nothing else is.
    let power = w.bridges().into_iter().find(|b| b.item == "Power").expect("no power buffer");
    assert_eq!((power.from, power.to), (plant, cell));
    let b = w.compile();
    assert!(
        b.graph.nodes.iter().any(|n| n.name == power.name),
        "the power connection has nothing between its ends"
    );
}

/// Two suppliers of one item into one machine is the one thing the language
/// below cannot arbitrate, so it is refused where the player can see it.
///
/// The rule survived experiment 13; what it is stated about did not. It used
/// to be "one *bay* per item per machine", because a bay was the only thing
/// that could supply anything. Now a supplier is whatever has that output
/// port, and the refusal has to cover both.
#[test]
fn one_supplier_per_item_per_machine() {
    let (mut w, ids) = bayless_world();
    let cell = ids[1];
    let bay = w.place(proto("bay").unwrap(), 80, 0, 0, None, None, 0, 0).unwrap();
    let e = w.connect(bay, cell, "IronBillet").unwrap_err();
    assert!(e.contains("already has somewhere to draw"), "{e}");

    // A second caster on its own stock is a second supplier, and is refused
    // the same way.
    let spare = head(&mut w, "billetcaster", 80, 40, 100);
    let e = w.connect(spare, cell, "IronBillet").unwrap_err();
    assert!(e.contains("already has somewhere to draw"), "{e}");

    // The other half of the same rule: one destination per output.
    let e = w.connect(cell, bay, "Gear").unwrap_err();
    assert!(e.contains("already goes somewhere"), "{e}");
}

/// A port is a fact about the design inside the machine, so a connection that
/// names an item neither end handles is refused by name.
#[test]
fn a_connection_is_refused_by_its_ports() {
    let mut w = World::new("Test");
    let cell = w
        .place(
            proto("machining").unwrap(),
            0,
            0,
            0,
            None,
            Some(stock_design("machining").unwrap()),
            0,
            0,
        )
        .unwrap();
    let press = w
        .place(
            proto("stamping").unwrap(),
            20,
            0,
            0,
            None,
            Some(stock_design("stamping").unwrap()),
            0,
            0,
        )
        .unwrap();
    let e = w.connect(cell, press, "Coal").unwrap_err();
    assert!(e.contains("no coal output"), "{e}");
    // Two consumers of the same thing have nothing to say to each other: the
    // cell makes gears, and the press does not take them.
    let e = w.connect(cell, press, "Gear").unwrap_err();
    assert!(e.contains("no gears input"), "{e}");
    // And a connection to itself goes nowhere.
    assert!(w.connect(cell, cell, "Gear").is_err());
}

/// A placed machine is an empty chassis, not somebody else's answer.
///
/// Note 7 of the play session, in full: "pre built machines takes the fun out
/// of the game entirely." A prototype used to hand over a finished machine and
/// the tech tree handed over more of them. Now a prototype is a footprint and
/// a name, and what goes inside it is the player's.
#[test]
fn a_placed_machine_is_empty_until_it_is_designed() {
    let mut w = World::new("Test");
    let id = w
        .place(proto("machining").unwrap(), 0, 0, 0, None, None, 0, 0)
        .expect("an empty chassis can be placed");
    let i = w.get(id).unwrap();
    assert!(i.design.is_none(), "a chassis arrived with somebody's design in it");
    assert!(i.lowered.is_none(), "and with a recipe");
    assert!(i.wants().is_empty() && i.makes().is_empty(), "and with ports");
    // It wears the catalogue's box until there is a design to take its size
    // from, and it stands there not running.
    let p = proto("machining").unwrap();
    assert_eq!(i.size(), (p.w, p.h));
    let b = w.compile();
    let why = b.why_idle(id).expect("an empty chassis should be left out, and told why");
    assert!(why.contains("designed"), "{why}");
}

/// The worked example is still there, and has to be asked for by name.
///
/// Experiment 13 allows tutorial examples and asks that they not be the normal
/// way forward. So the command says which it is, and the room's log therefore
/// says which machines somebody designed and which they copied out of the book.
#[test]
fn the_worked_example_is_asked_for_by_name() {
    let (mut r, a, _) = wired_room(33);
    r.set_now(secs(20));
    let empty = |example: bool| Act::PlaceMachine {
        proto: "machining".into(),
        x: if example { 70 } else { 84 },
        y: 60,
        face: 0,
        item: None,
        design: None,
        example,
    };
    r.submit(a, empty(false)).unwrap();
    let chassis = r.host.world.installs.last().unwrap();
    assert!(chassis.design.is_none(), "a plain placement filled itself in");

    r.submit(a, empty(true)).unwrap();
    let worked = r.host.world.installs.last().unwrap();
    assert!(worked.design.is_some(), "the worked example did not arrive");
    assert!(worked.makes().iter().any(|i| i == "Gear"), "and it does not make gears");
    assert_eq!(
        worked.design.as_ref().map(|d| d.emit()),
        Some(stock_design("machining").unwrap().emit()),
        "the example is not the catalogue's"
    );

    // The distinction survives the wire, because it is what the log says
    // happened.
    let last = r.log.last().unwrap().to_json();
    assert_eq!(last.at("payload").at("example").as_bool(), Some(true));
}

/// An empty chassis opens on an empty drawing board.
///
/// Without this a machine you had not designed yet could not be designed at
/// all, which would make the chassis a very elaborate way of placing nothing.
#[test]
fn an_empty_chassis_can_be_designed() {
    let (mut r, a, _) = wired_room(35);
    r.set_now(secs(20));
    r.submit(a, Act::PlaceMachine {
        proto: "machining".into(),
        x: 70,
        y: 60,
        face: 0,
        item: None,
        design: None,
        example: false,
    })
    .unwrap();
    let id = r.host.world.installs.last().unwrap().id;
    r.submit(a, Act::OpenDesign { id }).expect("an empty chassis opens");
    let i = r.host.world.get(id).unwrap();
    assert!(i.draft.is_some(), "there is nothing to draw on");
    assert_eq!(i.editor, Some(a));
    assert!(i.draft.as_ref().unwrap().units.is_empty(), "the board was not empty");

    // And something can be put on it, which is the whole point.
    r.submit(a, Act::PlaceComponent {
        id,
        kind: "motor".into(),
        x: 0,
        y: 0,
        z: 0,
        face: None,
    })
    .expect("a component goes into the empty draft");
    assert_eq!(r.host.world.get(id).unwrap().draft.as_ref().unwrap().units.len(), 1);
}

/// The world offers an opportunity, and never an output.
///
/// Note 1 of the play session: an ore mine that produced ore because the
/// catalogue said so was the last magical object left in the game. Now the
/// room has ground in it and the machine standing on that ground is the
/// player's, design and all.
#[test]
fn a_head_has_to_stand_on_something() {
    let mut w = World::new("Test");
    // Nothing in the ground: a head is refused, by name, where it can be seen.
    let e = w
        .place(
            proto("oremine").unwrap(),
            0,
            0,
            0,
            None,
            Some(stock_design("oremine").unwrap()),
            0,
            0,
        )
        .unwrap_err();
    assert!(e.contains("ore body"), "{e}");

    // The wrong ground is no ground at all.
    w.seam("Coal", 0, 0, 8, 6, 200);
    let e = w
        .place(
            proto("oremine").unwrap(),
            0,
            0,
            0,
            None,
            Some(stock_design("oremine").unwrap()),
            0,
            0,
        )
        .unwrap_err();
    assert!(e.contains("ore body"), "{e}");

    // The right ground, and it stands. A head only has to *touch* the seam:
    // "on or beside it", not centred on it to the tile.
    let pit = w
        .place(
            proto("coalpit").unwrap(),
            6,
            4,
            0,
            None,
            Some(stock_design("coalpit").unwrap()),
            0,
            0,
        )
        .expect("a coal head on a coal seam");
    assert!(w.under(pit).is_some(), "the head does not know what it is standing on");
    assert_eq!(w.under(pit).unwrap().item, "Coal");
}

/// The ground has the last word, and a head is not it.
#[test]
fn the_ground_caps_the_head() {
    let rate = |w: &World, id: Id| -> u64 {
        let (_, gives, cycle) = w.get(id).unwrap().recipe();
        gives.iter().find(|a| a.item == "Coal").map(|a| a.qty).unwrap_or(0) * 60 / cycle
    };

    // A seam richer than the head: the head's design is the constraint.
    let mut rich = World::new("Rich");
    let a = head(&mut rich, "coalpit", 0, 0, 5_000);
    assert_eq!(rate(&rich, a), 400, "a head on a rich seam is not running flat out");

    // A seam thinner than the head: the ground is the constraint.
    let mut thin = World::new("Thin");
    let b = head(&mut thin, "coalpit", 0, 0, 35);
    assert_eq!(rate(&thin, b), 35, "a thin seam did not hold the head back");

    // And the thin one still turns every second rather than in lumps -- an
    // earlier version stretched the cycle instead of scaling the amounts, and
    // the campaign lost a ninth of its gears to machines idling between
    // deliveries.
    let (_, _, cycle) = thin.get(b).unwrap().recipe();
    assert_eq!(cycle, 60, "a capped head stopped turning once a second");
}

/// A seam is a budget, not a socket: what one head cannot lift, two standing
/// beside each other can, for the price of the floor they take.
#[test]
fn a_seam_is_shared_first_come_first_served() {
    let mut w = World::new("Test");
    w.seam("Coal", 0, 0, 12, 6, 900);
    let put = |w: &mut World, x: i32| -> Id {
        w.place(
            proto("coalpit").unwrap(),
            x,
            0,
            0,
            None,
            Some(stock_design("coalpit").unwrap()),
            0,
            0,
        )
        .unwrap_or_else(|e| panic!("a head at {x}: {e}"))
    };
    let one = put(&mut w, 0);
    let two = put(&mut w, 2);
    let three = put(&mut w, 4);
    // Four hundred, four hundred, and the hundred that is left.
    assert_eq!(w.get(one).unwrap().rated, Some(400));
    assert_eq!(w.get(two).unwrap().rated, Some(400));
    assert_eq!(w.get(three).unwrap().rated, Some(100));

    // A fourth is refused rather than left standing there turning at nothing.
    let e = w
        .place(
            proto("coalpit").unwrap(),
            6,
            0,
            0,
            None,
            Some(stock_design("coalpit").unwrap()),
            0,
            0,
        )
        .unwrap_err();
    assert!(e.contains("spoken for"), "{e}");

    // Taking one down gives its share back to what is still standing.
    w.remove(two).unwrap();
    assert_eq!(w.get(one).unwrap().rated, Some(400));
    assert_eq!(w.get(three).unwrap().rated, Some(400), "the seam was not shared out again");
}

/// A head draws the substance it is standing on out of the *ground*, and never
/// out of a bay.
///
/// Inside the machine an inlet is a boundary flow like any other -- it has to
/// be, or the design would not balance. Outside it, the boundary it crosses is
/// the deposit. The first version of this asked a bay for the coal a coal head
/// was standing on, and every head in the game was idle for want of what it
/// was producing.
#[test]
fn a_head_does_not_ask_a_bay_for_what_it_is_digging() {
    let mut w = World::new("Test");
    let pit = head(&mut w, "coalpit", 0, 0, 400);
    assert!(!w.get(pit).unwrap().wants().iter().any(|i| i == "Coal"));
    assert!(w.get(pit).unwrap().makes().iter().any(|i| i == "Coal"));
    let bay = w.place(proto("bay").unwrap(), 20, 0, 0, None, None, 0, 0).unwrap();
    w.connect(pit, bay, "Coal").unwrap();
    let b = w.compile();
    assert!(
        b.idle.iter().all(|(id, _)| *id != pit),
        "the head was left out: {:?}",
        b.idle
    );
}

/// Ground is part of the document, so it is part of the hash and it survives
/// the wire. Two replicas that disagreed about what was underneath them would
/// agree about every command and build different factories.
#[test]
fn ground_survives_the_wire() {
    let (w, _) = tiny_world();
    assert!(!w.deposits.is_empty());
    let build = w.compile();
    let back = World::from_json(&w.to_json(&build, true)).expect("a world came back");
    assert_eq!(back.deposits, w.deposits, "the ground did not survive");
    assert_eq!(back.signature(), w.signature());

    // And the head that stands on it came back able to run.
    let mine = back.installs.iter().find(|i| i.proto.tag == "oremine").expect("the head");
    assert!(mine.lowered.is_some(), "a head came back without its recipe");
    assert!(mine.makes().iter().any(|i| i == "IronOre"));

    // Terrain is in the signature: change it and the hash changes.
    let mut moved = w.clone();
    moved.deposits[0].yields += 1;
    assert_ne!(moved.signature(), w.signature(), "ground is not in the signature");
}

/// Two bays are still joined by a transport rather than by a wire: a belt has
/// a length, a fleet and a latency, and a wire between two sheds would have
/// none of them.
#[test]
fn two_bays_are_joined_by_a_transport() {
    let mut w = World::new("Test");
    let a = w.place(proto("bay").unwrap(), 0, 0, 0, None, None, 0, 0).unwrap();
    let b = w.place(proto("bay").unwrap(), 20, 0, 0, None, None, 0, 0).unwrap();
    let e = w.connect(a, b, "IronOre").unwrap_err();
    assert!(e.contains("transport"), "{e}");
    assert!(w.link(proto("belt").unwrap(), a, b, "IronOre", 0, 0).is_ok());
}

/// Every item belongs to exactly one domain, which is what makes matching the
/// item enough to match the domain -- and therefore what keeps electricity out
/// of a pipe without a second check anywhere.
#[test]
fn an_item_belongs_to_one_domain() {
    use temporal_rooms::mp::lower::ITEMS;
    for item in ITEMS {
        assert!(!domain_tag(item).is_empty(), "{item} has no domain");
    }
    assert_eq!(domain_tag("Water"), "fluid");
    assert_eq!(domain_tag("Crude"), "fluid");
    assert_eq!(domain_tag("Coal"), "material");
    assert_eq!(domain_tag("Gear"), "material");
    assert_eq!(domain_tag("Heat"), "heat");
    assert_eq!(domain_tag("Torque"), "rotary");
    assert_eq!(domain_tag("Stroke"), "mech");
}

fn domain_tag(item: &str) -> &'static str {
    temporal_rooms::mp::lower::domain_of(item).tag()
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
    let mine = head(&mut w, "oremine", 0, 20, 100);
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

/// A head on the room's own ground, placed as a command like anything else.
fn sink_head(r: &mut Room, who: u32, tag: &'static str) -> Id {
    let item = proto(tag).unwrap().extracts().expect("not an extraction head");
    let (x, y) = r
        .host
        .world
        .nth_ground(item, 0)
        .map(|d| (d.x, d.y))
        .unwrap_or_else(|| panic!("the room has no {item} in the ground"));
    r.submit(
        who,
        Act::PlaceMachine { proto: tag.into(), x, y, face: 0, item: None, design: None, example: true },
    )
        .unwrap_or_else(|e| panic!("a {tag} on its own ground: {e}"));
    r.host.world.installs.last().unwrap().id
}

fn wired_room(seed: u64) -> (Room, u32, u32) {
    let mut r = Room::open(seed, Some("first-gears"));
    r.start_manual();
    let a = r.join("Ada").unwrap();
    r.set_now(secs(2));
    let b = r.join("Bee").unwrap();

    let tag = |r: &Room, t: &str, n: usize| -> Id {
        r.host.world.installs.iter().filter(|i| i.proto.tag == t).nth(n).map(|i| i.id).unwrap()
    };
    // A room comes with ground, not with working mines, so the first thing
    // anybody does is put a head on each seam.
    let caster = sink_head(&mut r, a, "billetcaster");
    let coal = sink_head(&mut r, a, "coalpit");
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
        example: true,
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
        example: true,
    })
    .unwrap();
    let plant = r.host.world.installs.last().unwrap().id;
    r.submit(a, Act::PlaceStorage { proto: "yard".into(), x: 60, y: 26, face: 0 }).unwrap();
    let powerbay = r.host.world.installs.last().unwrap().id;

    let water = sink_head(&mut r, b, "waterpump");
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

/// A browser that stops asking is carried anyway.
///
/// This is the fix for the freeze the play session kept hitting, and it is
/// worth being clear about which end of it was broken. A replica used to move
/// only when the browser that owned it polled. A tab in the background is
/// throttled to a `setTimeout` a minute; a laptop that was shut sends nothing
/// at all. So the replica stopped where it was, and then one poll had to carry
/// it a minute of ticks in a single call, holding the room's lock -- and the
/// player who *froze* was the other one, the one who never went anywhere.
///
/// So the room beats on its own. Ada polls; Bee's browser has gone quiet; and
/// Bee's replica is at the same tick as the host anyway, having been carried
/// there in beat-sized pieces rather than one lump.
#[test]
fn a_quiet_browser_is_carried_anyway() {
    let (mut r, a, b) = wired_room(7);
    // Two minutes of room, and one of the two browsers asks for nothing at all.
    for k in 1..=40u64 {
        r.set_now(secs(20 + k * 3));
        r.heartbeat();
        r.sync(a).unwrap();
    }
    let now = r.now();
    assert_eq!(r.player(b).unwrap().sim.now, now, "the quiet replica was left behind");
    assert_eq!(r.host.now, now, "the host was left behind");

    // And it is not merely *at* the right tick, it got there the right way:
    // the same commands, the same lattice, the same hash as everybody else.
    let t = r.host.probe().min(r.player(b).unwrap().sim.probe());
    let hs = r.hashes(t);
    assert!(t > 0, "nothing was ever checked");
    assert!(
        hs.iter().all(|(_, h)| h.is_some() && *h == hs[0].1),
        "beating the room apart from its replicas broke the agreement at {t}: {hs:?}"
    );
    assert_eq!(r.player(b).unwrap().mismatches, 0);
    assert_eq!(r.player(b).unwrap().resyncs, 0);

    // The poll that eventually arrives is a read, not a catch-up: there is
    // nothing left for it to simulate.
    let before = r.player(b).unwrap().sim.now;
    r.view(b).unwrap();
    assert_eq!(r.player(b).unwrap().sim.now, before, "the poll still had work to do");
}

/// A beat and a poll are the same arithmetic, so it must not matter which one
/// a room gets. Two rooms, one command stream, one beaten and one polled, and
/// the same hash at the end of both.
#[test]
fn beating_a_room_is_polling_it() {
    let (mut beaten, ba, _) = wired_room(17);
    let (mut polled, pa, _) = wired_room(17);
    for k in 1..=30u64 {
        let t = secs(20 + k * 7);
        beaten.set_now(t);
        beaten.heartbeat();
        polled.set_now(t);
        polled.sync(pa).unwrap();
    }
    let t = beaten.host.probe().min(polled.host.probe());
    assert!(t > 0, "nothing was ever checked");
    assert_eq!(beaten.host.check(t), polled.host.check(t), "a beat is not a poll");
    assert_eq!(
        beaten.player(ba).unwrap().sim.check(t),
        polled.player(pa).unwrap().sim.check(t),
        "a beaten replica is not a polled one"
    );
}

/// Who is *watching* is not who is up to date any more.
///
/// With the beat running, every replica sits at the current tick whether its
/// browser is there or not -- so `behind`, which the header used to dim a
/// player by, is always zero and says nothing. `away` is the number that still
/// means something: not where their copy of the room is, but when their screen
/// last collected one.
#[test]
fn away_is_not_behind() {
    let (mut r, a, b) = wired_room(19);
    for k in 1..=20u64 {
        r.set_now(secs(20 + k * 3));
        r.heartbeat();
        r.view(a).unwrap();
    }
    let now = r.now();
    let (ada, bee) = (r.player(a).unwrap(), r.player(b).unwrap());
    assert_eq!(now.saturating_sub(bee.sim.now), 0, "the quiet one is behind after all");
    assert_eq!(now.saturating_sub(ada.last_seen), 0, "the watching one is marked away");
    assert!(
        now.saturating_sub(bee.last_seen) > secs(30),
        "a browser that has not asked for a minute is not marked away"
    );

    // And coming back clears it, without touching the seat.
    r.view(b).unwrap();
    assert_eq!(r.now().saturating_sub(r.player(b).unwrap().last_seen), 0, "coming back did not count");
    assert_eq!(r.player(b).unwrap().resyncs, 0, "being away was treated as a divergence");
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

/// A browser that was refreshed comes back to its own seat.
///
/// This is the property that a play session found the hard way: the seat is
/// everything a player owns, and a reload that took a *second* seat left the
/// first one holding the factory while the screen in front of the person who
/// built it could no longer touch it.
#[test]
fn a_refreshed_browser_keeps_its_seat() {
    let (mut r, a, _) = wired_room(11);
    r.set_now(secs(60));
    r.sync(a).unwrap();

    let (cy, fresh) = r.join_as("Cy", "seat-cy").unwrap();
    assert!(!fresh, "the first arrival was not an arrival");
    let seats = r.players.len();
    // Cy builds something, so the seat is worth coming back to.
    r.set_now(secs(64));
    r.submit(cy, Act::PlaceStorage { proto: "bay".into(), x: 70, y: 20, face: 0 }).unwrap();
    let built = r.host.world.installs.last().unwrap().id;

    // The browser is refreshed: same token, and no memory of the name or the
    // id, because both were in the page that went away.
    r.set_now(secs(70));
    let (again, rejoined) = r.join_as("", "seat-cy").unwrap();
    assert!(rejoined, "the room did not recognise the token");
    assert_eq!(again, cy, "the same browser was given a different seat");
    assert_eq!(r.players.len(), seats, "coming back added a seat");
    assert_eq!(r.player(cy).unwrap().name, "Cy", "the name was forgotten");
    assert!(
        r.host.world.installs.iter().any(|i| i.id == built),
        "what the seat had built did not survive the reload"
    );

    // The replica is new, and agrees. A rejoin is counted as a rejoin: it is
    // not evidence of divergence, and the experiment's numbers must not say
    // that it is.
    assert_eq!(r.player(cy).unwrap().rejoins, 1);
    assert_eq!(r.player(cy).unwrap().resyncs, 0, "a reload was counted as a correction");
    assert_eq!(r.player(cy).unwrap().mismatches, 0);
    for k in 1..=6u64 {
        r.set_now(secs(70 + k * 5));
        r.sync(cy).unwrap();
    }
    let t = r.host.probe().min(r.player(cy).unwrap().sim.probe());
    assert_eq!(
        r.host.check(t),
        r.player(cy).unwrap().sim.check(t),
        "the rebuilt replica is a different room"
    );
    assert_eq!(r.player(cy).unwrap().mismatches, 0);

    // A different browser is a different person, however it introduces itself.
    let (other, was_there) = r.join_as("Cy", "seat-someone-else").unwrap();
    assert!(!was_there);
    assert_ne!(other, cy, "a token is the identity, and a name is not");
    // And no token at all never matches a seat, which is what keeps the CLI
    // harness and every test above this one getting a fresh one.
    let (anon_a, _) = r.join_as("nobody", "").unwrap();
    let (anon_b, seen) = r.join_as("nobody", "").unwrap();
    assert!(!seen, "an empty token matched a seat");
    assert_ne!(anon_a, anon_b);
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

    // And the host's own reconstruction did not move either.
    //
    // This is the half that was missing, and it was expensive. The host is a
    // `Sim`, `Sim::apply` used to stamp its sequence before finding out
    // whether the command applied, and `submit_for` rolls the room's counter
    // back on a refusal. So one refusal left the two disagreeing by one, the
    // next real command reused the number, and every replica built from a
    // snapshot after that quietly skipped it -- with nothing anywhere
    // reporting a fault, because as far as the room could tell that replica
    // was up to date.
    assert_eq!(
        r.host.seq, r.seq,
        "a refused command moved the host's sequence past the log's"
    );

    // The proof that it matters: somebody joins after the refusals, somebody
    // builds, and the new arrival can see it.
    let cy = r.join("Cy").unwrap();
    r.set_now(secs(24));
    r.submit(a, Act::PlaceStorage { proto: "bay".into(), x: 78, y: 40, face: 0 }).unwrap();
    let built = r.host.world.installs.last().unwrap().id;
    r.sync(cy).unwrap();
    assert!(
        r.player(cy).unwrap().sim.world.get(built).is_some(),
        "a player who joined after a refusal never saw the next command"
    );
    assert_eq!(r.player(cy).unwrap().resyncs, 0, "and did not need correcting to get it");
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
            example: false,
        },
        Act::PlaceStorage { proto: "bay".into(), x: 1, y: 2, face: 1 },
        // A chassis: the same command with nothing inside it, which is what a
        // player gets by default since experiment 13.
        Act::PlaceMachine {
            proto: "machining".into(),
            x: 9,
            y: 9,
            face: 0,
            item: None,
            design: None,
            example: false,
        },
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
    let bay = r.host.world.installs.iter().find(|i| i.proto.tag == "bay").map(|i| i.id).unwrap();
    let depot = r.host.world.installs.iter().find(|i| i.proto.tag == "depot").map(|i| i.id).unwrap();
    let caster = sink_head(&mut r, a, "billetcaster");
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

/// A ghost is a tombstone: restoring brings the wiring back with the building.
///
/// Note 12 from the play session. Deleting and restoring only the building was
/// technically consistent and experientially obnoxious -- the machine came
/// back, none of its connections did, and the player had to remember what it
/// had been joined to and draw all of it again.
#[test]
fn restoring_brings_the_wiring_back() {
    let (mut r, a, _) = wired_room(23);
    r.set_now(secs(30));
    let cell = r
        .host
        .world
        .installs
        .iter()
        .find(|i| i.proto.tag == "machining")
        .map(|i| i.id)
        .unwrap();
    let before: Vec<(Id, Id, String)> = r
        .host
        .world
        .conns
        .iter()
        .filter(|c| c.from == cell || c.to == cell)
        .map(|c| (c.from, c.to, c.item.clone()))
        .collect();
    assert!(before.len() >= 3, "the test cell was not wired enough to prove anything");
    let design = r.host.world.get(cell).unwrap().design.clone();
    assert!(design.is_some(), "the test cell has no design to lose");

    r.submit(a, Act::DeleteMachine { id: cell }).unwrap();
    assert!(r.host.world.conns.iter().all(|c| c.from != cell && c.to != cell));

    // The ghost is the tombstone, and it knows what died with it.
    let ghost = r.host.ghosts.last().cloned().expect("no ghost");
    assert_eq!(ghost.conns.len(), before.len(), "the ghost forgot the wiring");

    r.set_now(secs(34));
    r.submit(a, ghost.restore()).unwrap();
    let back = r.host.world.installs.last().unwrap();
    let now = back.id;
    assert_ne!(now, cell, "a restore is a new placement, not a rollback");
    // Compared by what it emits: a design has no `PartialEq`, and the thing
    // that matters is that the machine came back as the machine somebody built
    // rather than as the catalogue's.
    assert_eq!(
        back.design.as_ref().map(|d| d.emit()),
        design.as_ref().map(|d| d.emit()),
        "the design did not come back with it"
    );

    // Every connection, rewritten onto the new identity.
    let after: Vec<(Id, Id, String)> = r
        .host
        .world
        .conns
        .iter()
        .filter(|c| c.from == now || c.to == now)
        .map(|c| (c.from, c.to, c.item.clone()))
        .collect();
    assert_eq!(after.len(), before.len(), "the wiring did not come back: {after:?}");
    for (from, to, item) in &before {
        let want = (if *from == cell { now } else { *from }, if *to == cell { now } else { *to }, item.clone());
        assert!(after.contains(&want), "{want:?} was not restored");
    }

    // And it says so, once, in the feed.
    assert!(
        r.events.iter().any(|e| e.verb == "restore" && e.what.contains("all")),
        "a whole restore was not reported: {:?}",
        r.events.iter().map(|e| &e.what).collect::<Vec<_>>()
    );
}

/// Half a restore is reported as half a restore.
///
/// "Do not silently do half the job": if the room has moved on and a
/// connection cannot go back, the building still returns and the player is
/// told which wire did not.
#[test]
fn a_restore_that_cannot_finish_says_so() {
    let (mut r, a, _) = wired_room(27);
    r.set_now(secs(30));
    let cell = r
        .host
        .world
        .installs
        .iter()
        .find(|i| i.proto.tag == "machining")
        .map(|i| i.id)
        .unwrap();
    let billet = r
        .host
        .world
        .conns
        .iter()
        .find(|c| c.to == cell && c.item == "IronBillet")
        .map(|c| c.from)
        .expect("the cell draws billet from somewhere");

    r.submit(a, Act::DeleteMachine { id: cell }).unwrap();
    let ghost = r.host.ghosts.last().cloned().expect("no ghost");

    // While it is a ghost, the room moves on: the bay it drew its billet from
    // is taken down. There is nothing left to reconnect that end to.
    r.set_now(secs(32));
    r.submit(a, Act::DeleteStorage { id: billet }).unwrap();

    // The restore still happens, and it is honest about what it could not do.
    r.set_now(secs(36));
    let (_, effects) = r.submit_for(a, ghost.restore()).unwrap();
    let restored = effects
        .iter()
        .find_map(|e| match e {
            Effect::Restored { wanted, made, failed, .. } => Some((*wanted, *made, failed.clone())),
            _ => None,
        })
        .expect("no restore effect");
    let (wanted, made, failed) = restored;
    assert!(made < wanted, "everything went back, so nothing was in the way");
    assert!(!failed.is_empty(), "a connection was lost without a word");
    assert!(
        failed.iter().any(|f| f.to_lowercase().contains("billet")),
        "the failure did not name the connection: {failed:?}"
    );
    assert!(
        r.host.world.installs.iter().any(|i| i.x == ghost.x && i.y == ghost.y),
        "the building itself did not come back"
    );
    assert!(
        r.events.iter().any(|e| e.verb == "restore" && e.what.contains(&format!("{made} of {wanted}"))),
        "the feed did not report a partial restore"
    );
}

/// A finished room says whether it is still doing what finished it.
///
/// Note 9. The play session passed a room on a power requirement, unplugged one
/// of the stations that had passed it, watched the number fall, and could not
/// tell from the screen whether anything was wrong. Both answers were true --
/// the room stays passed, the requirement is false right now -- and the panel
/// only had room for one of them.
#[test]
fn a_finished_room_still_says_what_it_is_doing() {
    use temporal_rooms::mp::goal::Kind;
    let mut r = Room::open(21, Some("billet-stock"));
    r.start_manual();
    let a = r.join("Ada").unwrap();
    let tag = |r: &Room, t: &str| -> Id {
        r.host.world.installs.iter().find(|i| i.proto.tag == t).map(|i| i.id).unwrap()
    };
    let bay = tag(&r, "bay");
    let depot = tag(&r, "depot");
    let caster = sink_head(&mut r, a, "billetcaster");
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
    assert!(p.met, "the simplest goal in the catalogue was not met");

    // A delivery goal is a pile: it only grows, so it is an achievement and
    // stays true whatever happens next.
    assert!(
        p.lines.iter().all(|l| l.kind == Kind::Achievement),
        "a delivery goal has a live requirement in it"
    );
    assert!(p.holding(), "an achievement stopped holding");
    assert!(p.slipped().is_empty());

    // Unplug it. The room stays finished -- that is the rule -- and the pile
    // stays delivered, because nothing in the game un-ships anything.
    let done_at = p.done_at;
    r.submit(a, Act::DeleteConnection { from: caster, to: bay, item: "IronBillet".into() })
        .unwrap();
    for k in 1..=20u64 {
        r.set_now(secs(740 + k * 8));
        r.sync(a).unwrap();
    }
    let after = r.host.progress();
    assert_eq!(after.done_at, done_at, "completion moved");
    assert!(after.met, "a delivered pile stopped being delivered");
}

/// A rate is a fact about now, and says so.
#[test]
fn a_held_rate_is_a_live_requirement() {
    use temporal_rooms::mp::goal::Kind;
    let (mut r, a, _) = wired_room(29);
    for k in 1..=30u64 {
        r.set_now(secs(20 + k * 4));
        r.sync(a).unwrap();
    }
    // Whatever this room's goal asks, every windowed line in it is live and
    // every pile is not -- and the two are never the same line.
    let p = r.host.progress();
    assert!(!p.lines.is_empty());
    for l in &p.lines {
        let windowed = l.what.contains("held for") || l.what.contains("second")
            || l.unit == "MW" || l.what.contains("footprint") || l.what.contains("wasted")
            || l.what.contains("surge") || l.what.contains("floor");
        if windowed {
            assert_eq!(l.kind, Kind::State, "`{}` is not live", l.what);
        }
    }
    // `holding` is about the live half only, so a goal made entirely of piles
    // is always holding and a goal with a rate in it can stop.
    assert_eq!(
        p.holding(),
        p.lines.iter().filter(|l| l.kind == Kind::State).all(|l| l.met),
        "holding disagrees with the live lines it is made of"
    );
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
