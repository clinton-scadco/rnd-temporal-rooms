//! Prototype 3: five rooms, one clock, and the properties that make them one
//! world rather than five games.
//!
//! ```text
//!   the map      five authored rooms, and a ladder that can actually be climbed
//!   the shelf    a design is kept, copied, and remembers where it came from
//!   the trains   what leaves one room arrives in another, and nothing is made
//!   the world    a room nobody is standing in keeps running, exactly
//! ```
//!
//! The last group is the experiment. A campaign is only worth building if
//! leaving a factory is not the same as pausing it, and the way to prove that
//! here is the way Prototype 2 proved synchronisation: run the same command
//! stream twice, in different shapes, and compare the canonical hashes.

use temporal_rooms::camp::run::Camp;
use temporal_rooms::camp::ship::{self, Ledger};
use temporal_rooms::camp::site::{self, SITES};
use temporal_rooms::camp::tech::{self, Tech};
use temporal_rooms::machine::parts;
use temporal_rooms::mp::cmd::Act;
use temporal_rooms::mp::goal::{Goal, Shape};
use temporal_rooms::mp::kit::{Role, PROTOS};
use temporal_rooms::mp::world::{stock_design, Id};
use temporal_rooms::mp::{secs, world::World};

// =================================================================== the map

/// Every room's objective is a template, because a template id is the only
/// thing a snapshot carries -- and a joining player who could not rebuild the
/// objective would be playing a different game in the same room.
#[test]
fn every_room_names_a_goal_that_exists() {
    for s in SITES {
        let g = Goal::of_seed(1, Some(s.template));
        assert_eq!(g.template, s.template, "{}: the template is missing", s.tag);
        assert!(!g.brief().is_empty(), "{}: an objective with nothing in it", s.tag);
        // Authored, not rolled: the same room twice is the same problem.
        let again = Goal::of_seed(999, Some(s.template));
        assert_eq!(g.shape, again.shape, "{}: its numbers came off a seed", s.tag);
    }
}

/// Every room furnishes itself on its own plot, without anything overlapping
/// anything else or hanging off the edge.
#[test]
fn every_room_fits_on_its_own_plot() {
    for s in SITES {
        let (w, ports) = s.furnish();
        assert_eq!(w.installs.len(), s.kit.len(), "{}: something would not fit", s.tag);
        assert_eq!(w.plot(), s.plot);
        for i in &w.installs {
            let (_, _, x1, y1) = i.bounds();
            assert!(x1 <= s.plot && y1 <= s.plot, "{}: {} hangs off the plot", s.tag, i.name);
        }
        for (a, b) in pairs(&w) {
            assert!(!a.0, "{}: {} overlaps {}", s.tag, b.0, b.1);
        }
        assert_eq!(ports.fixtures.len(), s.kit.len());
    }
}

fn pairs(w: &World) -> Vec<((bool,), (String, String))> {
    let mut out = Vec::new();
    for (i, a) in w.installs.iter().enumerate() {
        for b in w.installs.iter().skip(i + 1) {
            out.push(((a.overlaps(b),), (a.name.clone(), b.name.clone())));
        }
    }
    out
}

/// A room's imports and exports are the ones the map has lanes for. A yard
/// nothing can ever be delivered to is scenery.
#[test]
fn every_port_has_a_lane_and_every_lane_has_its_ports() {
    for s in SITES {
        let (_, ports) = s.furnish();
        for item in ports.incoming.keys() {
            assert!(
                ship::LANES.iter().any(|l| l.to == s.tag && l.item == *item),
                "{}: nothing on the map can deliver {item}",
                s.tag
            );
        }
    }
    for l in ship::LANES {
        let (_, from) = site::site(l.from).unwrap_or_else(|| panic!("no room `{}`", l.from));
        let (_, to) = site::site(l.to).unwrap_or_else(|| panic!("no room `{}`", l.to));
        assert!(
            from.furnish().1.outgoing.contains_key(l.item),
            "{} has no way to ship {}",
            from.tag,
            l.item
        );
        assert!(
            to.furnish().1.incoming.contains_key(l.item),
            "{} has nowhere to unload {}",
            to.tag,
            l.item
        );
    }
}

/// The dependency graph is a graph and not a knot: every room is reachable
/// from the one you start in.
#[test]
fn the_map_can_be_walked_from_the_beginning() {
    let mut open: Vec<&str> = SITES.iter().filter(|s| s.needs.is_empty()).map(|s| s.tag).collect();
    assert_eq!(open.len(), 1, "a campaign should begin in exactly one room");
    let mut moved = true;
    while moved {
        moved = false;
        for s in SITES {
            if open.contains(&s.tag) {
                continue;
            }
            if s.needs.iter().all(|n| open.contains(n)) {
                open.push(s.tag);
                moved = true;
            }
        }
    }
    assert_eq!(open.len(), SITES.len(), "some room can never be opened: {open:?}");
}

// ================================================================== the tech

/// Twelve unlockable components, every one of them a real part, and none of
/// them in the starting kit.
#[test]
fn the_twelve_are_twelve_real_components() {
    assert_eq!(tech::UNLOCKS.len(), 12);
    let start = tech::starting();
    assert_eq!(start.len() + tech::UNLOCKS.len(), parts::KINDS.len());
    for u in tech::UNLOCKS {
        assert!(parts::by_tag(u.part).is_some(), "`{}` is not a component", u.part);
        assert!(!start.contains(&u.part), "`{}` is unlocked and also free", u.part);
        assert!(!u.opens.is_empty());
    }
}

/// Every unlock is handed over by exactly one room, and the rooms hand over
/// all twelve between them.
#[test]
fn every_unlock_comes_from_exactly_one_room() {
    for u in tech::UNLOCKS {
        let from: Vec<&str> =
            SITES.iter().filter(|s| s.gives.contains(&u.part)).map(|s| s.tag).collect();
        assert_eq!(from.len(), 1, "`{}` is handed over by {from:?}", u.part);
    }
    let given: usize = SITES.iter().map(|s| s.gives.len()).sum();
    assert_eq!(given, tech::UNLOCKS.len());
}

/// The ladder can be climbed. Walking the rooms in dependency order, every
/// room's objective must be reachable with the components handed over *before*
/// it -- which is the property that makes a progression a progression rather
/// than a lock with its own key inside.
#[test]
fn no_room_needs_a_component_it_hands_over_itself() {
    // What each room's intended answer is built from. Authored here rather than
    // derived, because "what a room can be solved with" is a design claim and
    // deriving it from the catalogue would only prove the catalogue agrees
    // with itself.
    let wants: &[(&str, &[&str])] = &[
        ("basin", &["steamplant"]),
        ("valley", &["powderline", "steamplant"]),
        ("station", &["steamplant"]),
        ("works", &["stamping"]),
        ("final", &["steamplant", "pulseplant"]),
    ];
    let mut have = Tech::new();
    for s in SITES {
        let (_, need) = wants.iter().find(|(tag, _)| *tag == s.tag).expect("a room's answer");
        for proto in *need {
            assert!(
                have.allows_proto(proto).is_ok(),
                "{} cannot be solved yet: {} needs {:?}",
                s.tag,
                proto,
                have.missing_for(proto)
            );
        }
        for part in s.gives {
            have.learn(part);
        }
    }
    // And by the end, everything in the catalogue is placeable.
    for p in PROTOS.iter().filter(|p| p.role == Role::Machine) {
        assert!(have.allows_proto(p.tag).is_ok(), "{} is never unlocked", p.tag);
    }
}

/// A design containing a locked component is refused, and the refusal names
/// the component rather than the design.
#[test]
fn a_locked_component_is_refused_by_name() {
    let tech = Tech::new();
    let stamping = stock_design("stamping").expect("the stock stamping line");
    let e = tech.allows(&stamping).expect_err("a press should not be free");
    assert!(e.contains("Furnace Chamber") || e.contains("Stamping Press"), "{e}");
    assert!(tech.allows(&stock_design("steamplant").unwrap()).is_ok());
}

// ================================================================= the shelf

/// A copy is a copy: the parent survives it, and the child remembers.
#[test]
fn the_shelf_derives_rather_than_mutates() {
    let mut c = Camp::open(5);
    c.start_manual();
    let ada = c.join("Ada").unwrap();
    let plant = stock_design("steamplant").unwrap();
    let mk1 = c.shelf.save("Mk1", "steamplant", plant, None, "basin", 0, ada).unwrap();
    let mk2 = c.copy(ada, mk1, "Mk2").unwrap();
    assert_ne!(mk1, mk2);
    assert_eq!(c.shelf.get(mk2).unwrap().from, Some(mk1));
    assert_eq!(c.shelf.get(mk2).unwrap().proto, "steamplant");
    // The parent is untouched, and still called what it was called.
    assert_eq!(c.shelf.get(mk1).unwrap().name, "Mk1");
    assert_eq!(c.shelf.get(mk1).unwrap().from, None);
    // Two under one name is refused; the same design under two is not.
    assert!(c.copy(ada, mk1, "Mk2").is_err());
    assert!(c.copy(ada, mk1, "Mk3").is_ok());
    // Throwing the parent away does not take the child with it.
    c.shelf.forget(mk1).unwrap();
    assert!(c.shelf.get(mk1).is_none());
    assert_eq!(c.shelf.get(mk2).unwrap().from, Some(mk1));
}

// ================================================================ the trains

/// A route only ships what the origin actually shipped, and only as fast as
/// the fleet and the cap allow.
#[test]
fn a_route_carries_what_was_shipped_and_no_more() {
    let mut l = Ledger::default();
    let id = l.open("basin", "valley", "Coal", "train", Some(100), 0).unwrap();
    let mut sent = 0u64;
    // The first settlement only reads the counter; nothing has been shipped
    // since a moment that existed.
    l.dispatch(ship::SETTLE, |_, _| sent);
    let mut carried = 0u64;
    for k in 2..80u64 {
        let t = ship::SETTLE * k;
        // The origin ships 400 a second, and the cap says 100.
        sent += 400 * 5;
        l.dispatch(t, |_, _| sent);
        for load in l.arrivals(t) {
            carried += load.qty;
            l.landed(load.route, load.qty, 0, t);
        }
    }
    let r = l.route(id).unwrap();
    let seconds = 78 * 5;
    assert!(carried > 0, "nothing was ever carried");
    assert!(
        carried <= 100 * seconds,
        "the cap was ignored: {carried} in {seconds}s at 100/s"
    );
    assert!(r.moved == carried, "the ledger and the arrivals disagree");
    // What is in the air plus what landed plus what is waiting is exactly what
    // the cap let out of the yard. Nothing was created on the way.
    let air: u64 = l.flight.iter().map(|f| f.qty).sum();
    assert_eq!(r.moved + air + r.hold, 100 * seconds);
}

/// Two routes out of one room share one yard rather than each getting a copy
/// of it.
#[test]
fn two_routes_out_of_one_room_split_one_yard() {
    let mut l = Ledger::default();
    l.open("basin", "valley", "Coal", "train", Some(1_000), 0).unwrap();
    l.open("basin", "station", "Coal", "train", Some(1_000), 0).unwrap();
    let mut sent = 0u64;
    l.dispatch(ship::SETTLE, |_, _| sent);
    sent += 1_000; // one settlement's worth, and less than either cap
    l.dispatch(ship::SETTLE * 2, |_, _| sent);
    let held: u64 = l.routes.iter().map(|r| r.hold).sum();
    let air: u64 = l.flight.iter().map(|f| f.qty).sum();
    assert_eq!(held + air, 1_000, "the yard was shipped twice");
}

/// A load in the air outlives the contract behind it.
#[test]
fn closing_a_route_does_not_recall_its_trains() {
    let mut l = Ledger::default();
    let id = l.open("basin", "valley", "Coal", "train", Some(10_000), 0).unwrap();
    let mut sent = 0u64;
    l.dispatch(ship::SETTLE, |_, _| sent);
    sent += 50_000;
    l.dispatch(ship::SETTLE * 2, |_, _| sent);
    assert!(!l.flight.is_empty(), "nothing left the yard");
    let air: u64 = l.flight.iter().map(|f| f.qty).sum();
    l.close(id).unwrap();
    assert!(l.routes.is_empty());
    assert_eq!(l.flight.iter().map(|f| f.qty).sum::<u64>(), air);
    let landed: u64 = l.arrivals(ship::SETTLE * 200).iter().map(|f| f.qty).sum();
    assert_eq!(landed, air, "a load evaporated with its contract");
}

// ================================================================= the world

/// An arrival is a command, and a command is a thing every replica applies.
///
/// The campaign is run twice over the same script: once polled every second,
/// once left alone for a minute at a time. The trains have to leave at the
/// same seconds and land at the same seconds either way, or "come back in
/// twenty minutes" is a promise the simulation cannot keep.
#[test]
fn a_room_runs_the_same_whether_anybody_is_watching() {
    let fine = run_basin(1);
    let coarse = run_basin(60);
    assert_eq!(fine.0, coarse.0, "the two runs disagree about the canonical hash");
    assert_eq!(fine.1, coarse.1, "the two runs disagree about what was delivered");
    assert!(fine.1 > 0, "nothing was delivered in either run");
}

/// Build Coal Basin's factory, run it for four minutes polling every `step`
/// seconds, and report the canonical hash and the deliveries.
fn run_basin(step: u64) -> (Option<u64>, u64) {
    let mut c = Camp::open(7);
    c.start_manual();
    let ada = c.join("Ada").unwrap();
    let fixture = |c: &Camp, tag: &str, n: usize| -> Id {
        c.yard("basin")
            .and_then(|y| {
                y.room.host.world.installs.iter().filter(|i| i.proto.tag == tag).nth(n).map(|i| i.id)
            })
            .unwrap_or(0)
    };
    c.set_now(secs(2));
    let seam = fixture(&c, "coalpit", 0);
    let intake = fixture(&c, "waterpump", 0);
    let grid = fixture(&c, "grid", 0);
    let place = |c: &mut Camp, proto: &str, x: i32, y: i32| -> Id {
        let act = if proto == "bay" {
            Act::PlaceStorage { proto: proto.into(), x, y, face: 0 }
        } else {
            Act::PlaceMachine { proto: proto.into(), x, y, face: 0, item: None, design: None }
        };
        c.submit(ada, "basin", act).expect("a placement");
        c.yard("basin").and_then(|y| y.room.host.world.installs.last().map(|i| i.id)).unwrap_or(0)
    };
    let bay_c = place(&mut c, "bay", 8, 2);
    let bay_w = place(&mut c, "bay", 8, 8);
    let bay_p = place(&mut c, "bay", 24, 2);
    let wire = |c: &mut Camp, from: Id, to: Id, item: &str| {
        c.submit(ada, "basin", Act::CreateConnection { from, to, item: item.into() })
            .expect("a wire");
    };
    wire(&mut c, seam, bay_c, "Coal");
    wire(&mut c, intake, bay_w, "Water");
    wire(&mut c, bay_p, grid, "Power");
    for k in 0..3 {
        let plant = place(&mut c, "steamplant", 14, 2 + k * 3);
        wire(&mut c, bay_c, plant, "Coal");
        wire(&mut c, bay_w, plant, "Water");
        wire(&mut c, plant, bay_p, "Power");
    }
    let mut t = 2;
    while t < 240 {
        t += step;
        c.set_now(secs(t.min(240)));
        c.advance().expect("the campaign runs");
    }
    c.set_now(secs(240));
    c.advance().expect("the campaign runs");
    let y = c.yard("basin").unwrap();
    (y.room.host.check(secs(240)), y.shipped("Power"))
}

/// Nothing may be built in a room that has not opened, and no room opens
/// before the one it depends on has finished.
#[test]
fn a_shut_room_is_shut_to_everybody() {
    let mut c = Camp::open(9);
    c.start_manual();
    c.set_now(secs(1));
    let ada = c.join("Ada").unwrap();
    for s in SITES.iter().filter(|s| !s.needs.is_empty()) {
        assert!(c.travel(ada, s.tag).is_err(), "{} was open at the start", s.tag);
        let e = c
            .submit(ada, s.tag, Act::PlaceStorage { proto: "bay".into(), x: 2, y: 2, face: 0 })
            .expect_err("a shut room should refuse a placement");
        assert!(e.contains("not open yet"), "{e}");
    }
    assert!(c.travel(ada, "basin").is_ok());
}

/// A room's fixtures are what the room is. Deleting the seam would delete the
/// problem.
#[test]
fn a_fixture_cannot_be_bulldozed() {
    let mut c = Camp::open(9);
    c.start_manual();
    c.set_now(secs(1));
    let ada = c.join("Ada").unwrap();
    let fixtures = c.yard("basin").map(|y| y.ports.fixtures.clone()).unwrap_or_default();
    assert!(!fixtures.is_empty());
    for id in fixtures {
        let e = c
            .submit(ada, "basin", Act::DeleteMachine { id })
            .or_else(|_| c.submit(ada, "basin", Act::DeleteStorage { id }))
            .expect_err("a fixture should not be deletable");
        assert!(e.contains("cannot be removed") || e.contains("came with"), "{e}");
    }
}

/// Only the shipping office issues arrivals. A player who could would be a
/// player who could conjure thirty thousand coal.
#[test]
fn nobody_can_deliver_to_themselves() {
    let mut c = Camp::open(9);
    c.start_manual();
    c.set_now(secs(1));
    let ada = c.join("Ada").unwrap();
    let bay = c
        .submit(ada, "basin", Act::PlaceStorage { proto: "bay".into(), x: 8, y: 2, face: 0 })
        .map(|_| c.yard("basin").unwrap().room.host.world.installs.last().unwrap().id)
        .unwrap();
    let e = c
        .submit(
            ada,
            "basin",
            Act::Deliver { to: bay, item: "Coal".into(), qty: 30_000, from: "nowhere".into() },
        )
        .expect_err("a player should not be able to unload a train");
    assert!(e.contains("not something a player does"), "{e}");
}

/// The five rooms all advance on one clock, including the four nobody has
/// walked into.
#[test]
fn every_room_advances_whether_or_not_it_is_occupied() {
    let mut c = Camp::open(13);
    c.start_manual();
    c.join("Ada").unwrap();
    c.set_now(secs(300));
    c.advance().expect("the campaign runs");
    for s in SITES {
        let y = c.yard(s.tag).expect("a room");
        assert_eq!(y.room.host.now, secs(300), "{} was left behind", s.tag);
        assert!(y.room.host.probe() > 0, "{} never took a canonical sample", s.tag);
    }
}

/// A `Peak` goal is the one shape a flat factory cannot answer, and the
/// arithmetic that decides it has to say so.
#[test]
fn a_flat_load_fails_the_unstable_one() {
    use temporal_rooms::mp::goal::{Acct, Line};
    let shape = match Goal::of_seed(1, Some("site-final")).shape {
        Shape::Both(a, _) => *a,
        other => panic!("Final Works stopped being two problems: {other:?}"),
    };
    let Shape::Peak { base, peak, spill, every, secs: window, .. } = shape.clone() else {
        panic!("the first half of Final Works stopped being a Peak");
    };
    let judge = |per_second: &dyn Fn(u64) -> u64| -> Vec<Line> {
        let mut acct = Acct::default();
        let mut total = 0u64;
        for s in 0..=window + 5 {
            total += per_second(s);
            acct.count(
                secs(s),
                &[("Power".to_string(), total)].into_iter().collect(),
                &Default::default(),
                0,
                0,
                0,
            );
        }
        let goal = Goal { shape: shape.clone(), ..Goal::of_seed(1, Some("site-final")) };
        temporal_rooms::mp::goal::evaluate(&goal, &acct).lines
    };
    // Four plants held wide open: over the peak, and over it every second.
    let flat = judge(&|_| peak + 40);
    assert!(flat.iter().any(|l| !l.met), "a flat factory passed the unstable load");
    assert!(
        flat.iter().filter(|l| l.met).count() >= 2,
        "it should fail on the spill alone: {flat:?}"
    );
    // A floor, and a surge inside every window: the answer the room wants.
    let pulsed = judge(&|s| if s % (every - 2) == 0 { peak + 100 } else { base + 20 });
    assert!(
        pulsed.iter().all(|l| l.met),
        "a plant that idles and surges was refused: {pulsed:?}"
    );
    assert!(base < spill && spill < peak);
}
