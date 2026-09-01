//! The five rooms, written out by hand and deliberately nasty.
//!
//! ```text
//!   Coal Basin ─── coal ───┬──────────────► Power Station
//!    (basin)               │                  (station)
//!        │                 │                      │
//!        ├──── coal ──► Iron Valley               │ power
//!        │              (valley)                  ▼
//!        │                 │              Manufacturing
//!        │                 │ concentrate    (works)
//!        │                 │                      │ gears
//!        └──── coal ──►  Final Works ◄────────────┘
//!                         (final)
//! ```
//!
//! # Five problems, not one problem five times
//!
//! Section 5 of the brief is the single most important instruction in it: do
//! not turn "produce 100 gears" into "produce 500 gears" and call it a second
//! room. So each of these five is a *different* question, and each question is
//! one the engine underneath can actually be asked:
//!
//! ```text
//!   Coal Basin      a platform too small for the plant it needs
//!   Iron Valley     all the land in the world and a coal seam that will not
//!                   keep one boiler alight
//!   Power Station   every lump of fuel is ninety seconds away, in trainloads
//!   Manufacturing   no coal, no water, no grid: two supply chains, both live
//!   Final Works     a load that will not sit still
//! ```
//!
//! Four of those are answered by *designing something different*, and the
//! fifth -- Power Station -- by sizing a yard and a fleet, which is the same
//! kind of thinking one altitude up. None of them is answered by placing more
//! of what worked last time, which was the failure mode worth designing
//! against.
//!
//! # What was tried and thrown away
//!
//! The brief asks for a room whose problem is scarce water, answered by
//! recycling steam. It is not here, and the reason is worth writing down: in
//! this machine model the water cost of a megawatt is fixed. Every stock plant
//! lands within two percent of `1.48 water/MW`, because the chain from
//! exchanger to turbine to generator has no slack in it, and a turbine
//! *consumes* its steam rather than exhausting it, so there is nothing
//! downstream for a condenser to catch. A room whose problem was water would
//! have been a room with one answer, arrived at by arithmetic rather than by
//! design.
//!
//! So the water room became a fuel-logistics room, and the condenser earns its
//! place elsewhere -- as an unlock that pays off inside the refinery chain,
//! where the light fraction really does come off as vapour. That is the same
//! honest substitution the goal module already made once, and it is better
//! than shipping a room that cannot be solved.

use super::tech;
use crate::json::Json;
use crate::model::Qty;
use crate::mp::kit::{proto, Role};
use crate::mp::world::{Id, World};
use std::collections::BTreeMap;

/// What a fixture is for, beyond standing there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// Part of the room: a seam, an intake, a caster, a grid.
    Plain,
    /// Where loads from another room land. Always a yard, and never deletable.
    In(&'static str),
    /// Where loads leave from, and where the room's own objective is counted.
    /// Those are the same act on purpose -- shipping the thing you were asked
    /// for *is* how it reaches the next room.
    Out(&'static str),
}

/// One thing the room comes with.
pub struct Fixture {
    pub proto: &'static str,
    pub x: i32,
    pub y: i32,
    /// A depot's item.
    pub item: Option<&'static str>,
    /// This one's rate or capacity, when the catalogue's is not the point.
    pub rated: Option<Qty>,
    pub port: Port,
}

const fn fix(proto: &'static str, x: i32, y: i32, rated: Option<Qty>) -> Fixture {
    Fixture { proto, x, y, item: None, rated, port: Port::Plain }
}

const fn ship(proto: &'static str, x: i32, y: i32, item: &'static str) -> Fixture {
    Fixture { proto, x, y, item: Some(item), rated: None, port: Port::Out(item) }
}

const fn land(x: i32, y: i32, item: &'static str, cap: Qty) -> Fixture {
    // The item is on the yard as well as on the port: the compiler downstream
    // has to know this bay is supplied, or every machine drawing from it would
    // be refused for being fed by nobody.
    Fixture { proto: "yard", x, y, item: Some(item), rated: Some(cap), port: Port::In(item) }
}

/// One room in the campaign.
pub struct Site {
    pub tag: &'static str,
    pub title: &'static str,
    /// The goal template this room's objective is. It has to be a template
    /// rather than a `Shape` because a snapshot carries a seed and a template
    /// id and nothing else, and a joining player rebuilds the objective from
    /// those two numbers alone.
    pub template: &'static str,
    /// The side of the plot, in tiles.
    pub plot: i32,
    /// The constraint, in the sentence a player is shown before they enter.
    pub problem: &'static str,
    /// What is worth knowing once they are inside.
    pub note: &'static str,
    /// Rooms that must be finished before this one opens.
    pub needs: &'static [&'static str],
    /// Components finishing it hands over.
    pub gives: &'static [&'static str],
    pub kit: &'static [Fixture],
    /// Where it sits on the campaign map, in map units.
    pub mx: i32,
    pub my: i32,
}

/// A yard large enough that a room can be left alone for a while, which is
/// the entire reason inter-room transport exists.
const DEPOT: Qty = 240_000;

pub static SITES: &[Site] = &[
    Site {
        tag: "basin",
        title: "Coal Basin",
        template: "site-basin",
        plot: 40,
        problem: "A constrained footprint, and more fuel and water than anyone could burn.",
        note: "Four compact plants make the megawatts. Fitting them, their bays and their \
               fuel inside four hundred and eighty tiles is the problem -- and a machine's \
               footprint is its design's footprint, so the way to win space here is to open \
               one up.",
        needs: &[],
        gives: &["motor", "gearbox", "shaft"],
        kit: &[
            // Two seams, because a source feeds exactly one bay: a room that
            // had to choose between running and exporting would be a room with
            // one decision in it, and that decision is not this room's problem.
            // The second is the larger, because four other rooms burn what
            // comes off it and this one only burns a hundred and sixty.
            fix("coalpit", 2, 2, Some(400)),
            fix("coalpit", 2, 7, Some(900)),
            fix("waterpump", 2, 12, Some(1_600)),
            fix("grid", 30, 2, None),
            ship("depot", 30, 8, "Coal"),
        ],
        mx: 0,
        my: 1,
    },
    Site {
        tag: "valley",
        title: "Iron Valley",
        template: "site-valley",
        plot: 112,
        problem: "Cheap land in every direction, and a coal seam worth thirty-five a second.",
        note: "Ore is not the constraint here and space certainly is not. Electricity is: \
               the seam will not keep one boiler alight, so the rest of the fuel comes in \
               on a train from the Basin, and the crushing line runs on whatever that pays for.",
        needs: &["basin"],
        gives: &["separator", "preheater", "condenser"],
        kit: &[
            fix("oremine", 4, 6, Some(120)),
            fix("oremine", 4, 20, Some(120)),
            fix("coalpit", 4, 34, Some(35)),
            fix("waterpump", 4, 46, Some(400)),
            land(4, 58, "Coal", DEPOT),
            ship("depot", 100, 10, "OrePowder"),
            ship("depot", 100, 24, "Concentrate"),
        ],
        mx: 1,
        my: 0,
    },
    Site {
        tag: "station",
        title: "Power Station",
        template: "site-station",
        plot: 80,
        problem: "Water to spare, and every lump of coal a minute away in trainloads.",
        note: "Nothing here is short of anything, on average. The question is what happens \
               between trains: a yard too small is a plant that stops, and a plant that stops \
               is three hundred megawatts that were not delivered at the second they were counted.",
        needs: &["basin"],
        gives: &["furnace", "rollmill", "press", "crank"],
        kit: &[
            fix("waterpump", 4, 8, Some(1_200)),
            land(4, 20, "Coal", DEPOT),
            fix("grid", 68, 10, None),
        ],
        mx: 2,
        my: 2,
    },
    Site {
        tag: "works",
        title: "Manufacturing",
        template: "site-works",
        plot: 72,
        problem: "No coal, no water, no grid. Everything it runs on spent a minute on a train.",
        note: "The billet is local and nothing else is. A stamping line wants 121 MW and \
               twelve coal a second, and both of them arrive in lumps from two different rooms \
               that are also being asked for things.",
        needs: &["station"],
        gives: &["lathe"],
        kit: &[
            fix("billetcaster", 4, 8, Some(120)),
            land(4, 20, "Coal", DEPOT),
            land(4, 34, "Power", DEPOT),
            ship("depot", 60, 12, "Gear"),
        ],
        mx: 3,
        my: 1,
    },
    Site {
        tag: "final",
        title: "Final Works",
        template: "site-final",
        plot: 88,
        problem: "A load that will not sit still, on top of a line that must not stop.",
        note: "Four plants held wide open will not pass this: the surge is required and so is \
               the quiet between them. And the concentrate on the order is the one thing \
               Iron Valley was never asked for -- that line has to be gone back to and rebuilt.",
        needs: &["valley", "works"],
        gives: &["column"],
        kit: &[
            fix("waterpump", 4, 8, Some(800)),
            land(4, 20, "Coal", DEPOT),
            land(4, 34, "Gear", DEPOT),
            land(4, 48, "Concentrate", DEPOT),
            fix("grid", 74, 8, None),
            ship("depot", 74, 22, "Gear"),
            ship("depot", 74, 36, "Concentrate"),
        ],
        mx: 4,
        my: 1,
    },
];

pub fn site(tag: &str) -> Option<(usize, &'static Site)> {
    SITES.iter().enumerate().find(|(_, s)| s.tag == tag)
}

/// Where the loads land and where they leave from, once the room has been
/// built.
#[derive(Clone, Debug, Default)]
pub struct Ports {
    /// item -> the yard an arrival is unloaded into.
    pub incoming: BTreeMap<String, Id>,
    /// item -> the depot a departure is counted at.
    pub outgoing: BTreeMap<String, Id>,
    /// Everything the room came with. None of it may be deleted: a room's
    /// fixtures are what the room *is*, and a player who could bulldoze the
    /// coal seam and rebuild it at full rate would have deleted the problem
    /// rather than solved it.
    pub fixtures: Vec<Id>,
}

impl Site {
    /// The plot as it stands at tick zero.
    pub fn furnish(&self) -> (World, Ports) {
        // The tag rather than the title: a world's name becomes the blueprint
        // name in the source the solver is handed, and `blueprint Coal Basin
        // {` is two identifiers and a parse error. Prototype 2 never noticed,
        // because it only ever had one room and called it `Room`.
        let mut w = World::new(self.tag);
        w.plot = self.plot;
        let mut ports = Ports::default();
        for f in self.kit {
            let Some(p) = proto(f.proto) else { continue };
            let item = f.item.map(str::to_string);
            let Ok(id) = w.place(p, f.x, f.y, 0, item, None, 0, 0) else { continue };
            w.rate(id, f.rated);
            ports.fixtures.push(id);
            match f.port {
                Port::Plain => {
                    // A grid connection is a delivery point like any other, and
                    // a room whose objective is megawatts counts them there.
                    if p.role == Role::Sink {
                        ports.outgoing.entry("Power".to_string()).or_insert(id);
                    }
                }
                Port::In(item) => {
                    ports.incoming.insert(item.to_string(), id);
                }
                Port::Out(item) => {
                    ports.outgoing.insert(item.to_string(), id);
                }
            }
        }
        (w, ports)
    }

    /// Which of the twelve this room hands over, resolved.
    pub fn unlocks(&self) -> Vec<&'static tech::Unlock> {
        self.gives.iter().filter_map(|p| tech::unlock(p)).collect()
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tag", self.tag)
            .set("title", self.title)
            .set("template", self.template)
            .set("plot", self.plot as i64)
            .set("problem", self.problem)
            .set("note", self.note)
            .set("needs", Json::arr(self.needs.iter().map(|s| s.to_string()).collect::<Vec<_>>()))
            .set(
                "gives",
                Json::Arr(
                    self.unlocks()
                        .iter()
                        .map(|u| {
                            Json::obj().set("part", u.part).set("title", u.title).set("opens", u.opens)
                        })
                        .collect(),
                ),
            )
            .set("x", self.mx as i64)
            .set("y", self.my as i64)
    }
}
