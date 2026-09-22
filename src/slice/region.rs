//! Three regions, one plot, and the problem each of them is.
//!
//! ```text
//!   valley    1890   ore a shovel deep, a river, and no way to sell any of it
//!   district  2037   a grid, a caster, and nothing left in the ground
//!   zone      2070   two processes nobody else has, and no resources at all
//! ```
//!
//! # Every region is the same rectangle
//!
//! They all have [`land::PLOT`] for a plot, and every deposit in every one of
//! them comes out of [`super::land::seams`] at coordinates authored once. That
//! is not a convenience, it is the experiment: the Kestrel ore body is at
//! `8,6` in 1890 and Kestrel Foundry is at `8,6` in 2037, and neither of them
//! can be moved without moving the other, because they are one row of one
//! table.
//!
//! # What a region *is*, and what it is not
//!
//! A region is an [`mp::room::Room`](crate::mp::room::Room), unchanged: same
//! clock, same command log, same `(tick, sequence)` ordering, same
//! host-plus-one-replica-per-player reconstruction, same canonical hash. Three
//! regions is three of those, exactly as Prototype 3's five sites were five.
//!
//! What this file adds to a room is four things, and each of them is a fact the
//! room underneath cannot know:
//!
//! ```text
//!   a phase        which century the plant in it is being built in
//!   ground         resolved out of the shared land, per phase
//!   ports          a yard for what arrives, a depot for what leaves
//!   built over     the tiles somebody else already used, and the sentence why
//! ```
//!
//! The last one is the smallest and does the most work. In 2037 the tiles the
//! Kestrel ore body occupies are refused, and the refusal reads *Kestrel
//! Foundry has stood on this since 1951 -- in 1890 it is open ground with four
//! hundred a second under it*. That sentence is the whole of the brief's
//! instruction to "prove the player understands: this is the same place at a
//! different time", and it costs one lookup in [`super::land`].
//!
//! # The three problems, and why they are three problems
//!
//! ```text
//!   valley    all the ore in the world and carts to move it with
//!   district  every machine you want, and nothing to put in one
//!   zone      the best process on the map, standing on bare concrete
//! ```
//!
//! None of them is answered by building more of what answered the last one,
//! which was the instruction that mattered most in Prototype 3's brief and
//! matters here for a second reason: the three regions have to *need* each
//! other, or the fractures between them are scenery.

use super::land::{self, PLOT};
use super::phase::Phase;
use crate::json::Json;
use crate::model::Qty;
use crate::mp::kit::{proto, Role};
use crate::mp::world::{Id, World};
use std::collections::BTreeMap;

/// What a fixture is for, beyond standing there.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Port {
    /// Part of the region: a grid connection.
    Plain,
    /// Where loads from another region land. Always a yard, never deletable.
    In(&'static str),
    /// Where loads leave from, and where the region's own objective is counted.
    Out(&'static str),
}

/// One thing a region comes with.
pub struct Fixture {
    pub proto: &'static str,
    pub x: i32,
    pub y: i32,
    pub item: Option<&'static str>,
    pub rated: Option<Qty>,
    pub port: Port,
}

/// A yard big enough that a region can be left alone for a while.
const DEPOT: Qty = 240_000;

const fn fix(proto: &'static str, x: i32, y: i32) -> Fixture {
    Fixture { proto, x, y, item: None, rated: None, port: Port::Plain }
}

const fn ship(x: i32, y: i32, item: &'static str) -> Fixture {
    Fixture { proto: "depot", x, y, item: Some(item), rated: None, port: Port::Out(item) }
}

const fn land_at(x: i32, y: i32, item: &'static str) -> Fixture {
    // The item goes on the yard as well as on the port: the compiler
    // downstream has to know this bay is supplied, or every machine drawing
    // from it is refused for being fed by nobody.
    Fixture { proto: "yard", x, y, item: Some(item), rated: Some(DEPOT), port: Port::In(item) }
}

/// One region of the slice.
pub struct Region {
    pub tag: &'static str,
    pub title: &'static str,
    pub phase: Phase,
    /// The goal template its objective is. A template rather than a shape,
    /// because a snapshot carries a seed and a template id and nothing else.
    pub template: &'static str,
    /// The constraint, in the sentence shown before anybody goes there.
    pub problem: &'static str,
    /// What is worth knowing once they are there -- including, where it is not
    /// obvious, how to start.
    pub note: &'static str,
    pub kit: &'static [Fixture],
    /// Where it sits on the slice's own map, top to bottom.
    pub my: i32,
}

/// Every fixture in every region stands on the flat ground below the mill race,
/// which is empty in all three phases.
///
/// That is a deliberate choice rather than a coincidence: a region's own
/// buildings must not be the thing that makes one phase's plot different from
/// another's, or the comparison this experiment is built on gets muddied by its
/// own furniture.
pub static REGIONS: &[Region] = &[
    Region {
        tag: "valley",
        title: "1890 Mining Valley",
        phase: Phase::P1890,
        template: "slice-valley",
        problem: "Eight hundred a second of iron ore at the surface, and carts to move it with.",
        note: "Nothing here is short of ore and nothing here is short of water. What it is \
               short of is *everything that turns one into the other*: no grid, no steel, \
               and no electrical component of any kind. Heads and bays will fill the ore \
               order on their own. The powder half of it needs a mill, a mill wants speed 4, \
               a river turns at speed 1 -- and `designs/23-watermill.machine` is what the \
               first era does about that. If you would rather have motors, they are \
               available: they come back through the fracture in crates, and somebody in \
               2037 has to make them and somebody has to keep the fracture lit.",
        kit: &[
            ship(4, 48, "IronOre"),
            ship(12, 48, "Coal"),
            ship(20, 48, "OrePowder"),
            // Where machinery from the future is unloaded. A region that could
            // not receive a crate could not be electrified at all.
            land_at(32, 48, "Gear"),
        ],
        my: 0,
    },
    Region {
        tag: "district",
        title: "2037 Industrial District",
        phase: Phase::P2037,
        template: "slice-district",
        problem: "Every machine you could want, and an ore body four generations went through.",
        note: "Kestrel Spoil is eight a second and a crushing line wants ninety-three, so \
               the ore is coming out of 1890 or it is not coming. This region also pays for \
               both fractures out of its own grid connection -- 98 MW for the deep one, 53 \
               for the corridor to 2070 -- and the coal to make that with is on the far side \
               of the deep one. The way in is the forty-five a second still in Blackband and \
               the twelve hundred in the race: one compact plant and one water-driven \
               generator will hold the deep fracture open, and after that the coal arrives.",
        kit: &[
            land_at(4, 48, "IronOre"),
            land_at(14, 48, "Coal"),
            land_at(24, 48, "OrePowder"),
            land_at(34, 48, "Gear"),
            ship(44, 48, "Concentrate"),
            ship(50, 48, "IronBillet"),
            ship(56, 48, "Gear"),
            // Power goals are counted here, and so is every megawatt an
            // interface is holding a fracture open with.
            fix("grid", 64, 48),
        ],
        my: 1,
    },
    Region {
        tag: "zone",
        title: "2070 Manufacturing Zone",
        phase: Phase::P2070,
        template: "slice-zone",
        problem: "The two best processes on the map, standing on ground with nothing under it.",
        note: "A lathe takes billet straight to gears in one machine, which is the whole of \
               what this phase has that 2037 does not. It also wants 133 MW to do it, and \
               there is nothing here to burn: no coal, no ore, twenty-four a second left in \
               the caster, and a culvert where the mill race used to be. The wheels do still \
               turn in it -- 2070 builds a water-driven generator for nothing, which is the \
               joke the valley pays two crates for -- but not enough of them. The rest comes \
               down the corridor. What this region is *for* is the far end of the loop: \
               gears made here are machinery, and machinery is how 1890 gets a generator.",
        kit: &[
            land_at(4, 48, "IronBillet"),
            land_at(14, 48, "Power"),
            land_at(24, 48, "Concentrate"),
            ship(36, 48, "Gear"),
            fix("grid", 46, 48),
        ],
        my: 2,
    },
];

pub fn region(tag: &str) -> Option<(usize, &'static Region)> {
    REGIONS.iter().enumerate().find(|(_, r)| r.tag == tag)
}

/// Where the loads land and where they leave from.
#[derive(Clone, Debug, Default)]
pub struct Ports {
    pub incoming: BTreeMap<String, Id>,
    pub outgoing: BTreeMap<String, Id>,
    /// Everything the region came with. None of it may be deleted: a region's
    /// fixtures are what the region *is*.
    pub fixtures: Vec<Id>,
}

impl Region {
    /// The plot as it stands at tick zero.
    pub fn furnish(&self) -> (World, Ports) {
        let mut w = World::new(self.tag);
        w.plot = PLOT;
        let mut ports = Ports::default();
        // The ground first, resolved out of the shared land at this region's
        // own date, so that anything placed on it finds it already there.
        for (item, f, yields) in land::seams(self.phase) {
            w.seam(item, f.x, f.y, f.w, f.h, yields);
        }
        for f in self.kit {
            let Some(p) = proto(f.proto) else { continue };
            let item = f.item.map(str::to_string);
            let Ok(id) = w.place(p, f.x, f.y, 0, item, None, 0, 0) else { continue };
            w.rate(id, f.rated);
            ports.fixtures.push(id);
            match f.port {
                Port::Plain => {
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

    /// Why nothing may be built on this rectangle, if nothing may.
    ///
    /// The sentence is the point. It names what is standing there, and it names
    /// what the same tiles are in the phase where they are open -- because the
    /// one thing a player has to come away understanding is that these are the
    /// same tiles.
    pub fn built_over(&self, x: i32, y: i32, w: i32, h: i32) -> Option<String> {
        let f = land::taken_by(self.phase, x, y, w, h)?;
        let here = f.face(self.phase);
        let elsewhere: Vec<String> = super::phase::PHASES
            .iter()
            .filter(|p| **p != self.phase && !f.face(**p).taken())
            .map(|p| {
                let face = f.face(*p);
                match face.yields() {
                    Some((item, q)) => format!(
                        "in {} it is open ground with {q} {} a second under it",
                        p.tag(),
                        crate::mp::lower::item_title(item)
                    ),
                    None => format!("in {} it is {}", p.tag(), face.title()),
                }
            })
            .collect();
        let mut s = format!("{} is {} in {}", f.name, here.title(), self.phase.tag());
        if !elsewhere.is_empty() {
            s.push_str(" -- ");
            s.push_str(&elsewhere.join(", and "));
        }
        Some(s)
    }

    /// What this region can dig, in the order the land lays it out.
    pub fn ground(&self) -> Vec<(&'static str, &'static land::Feature, Qty)> {
        land::seams(self.phase)
    }

    /// The most this region could lift out of the ground of one item in a
    /// second, if every deposit had a perfect head on it.
    pub fn yields(&self, item: &str) -> Qty {
        self.ground().iter().filter(|(i, _, _)| *i == item).map(|(_, _, q)| *q).sum()
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tag", self.tag)
            .set("title", self.title)
            .set("phase", self.phase.tag())
            .set("year", self.phase.year() as i64)
            .set("template", self.template)
            .set("plot", PLOT as i64)
            .set("problem", self.problem)
            .set("note", self.note)
            .set("y", self.my as i64)
            .set(
                "ground",
                Json::Arr(
                    self.ground()
                        .iter()
                        .map(|(item, f, q)| {
                            Json::obj()
                                .set("item", *item)
                                .set("itemTitle", crate::mp::lower::item_title(item))
                                .set("where", f.name)
                                .set("perSecond", *q as i64)
                                .set("spent", matches!(f.face(self.phase), land::Face::Spent(_)))
                        })
                        .collect(),
                ),
            )
            .set(
                "builtOver",
                Json::Arr(
                    land::LAND
                        .iter()
                        .filter(|f| f.face(self.phase).taken())
                        .map(|f| {
                            Json::obj()
                                .set("name", f.name)
                                .set("what", f.face(self.phase).title())
                                .set("x", f.x as i64)
                                .set("y", f.y as i64)
                                .set("w", f.w as i64)
                                .set("h", f.h as i64)
                                .set("became", f.became)
                        })
                        .collect(),
                ),
            )
    }
}
