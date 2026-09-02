//! What a player may put in the world, and what it does when they do.
//!
//! Every number in this file is a **gameplay** number, written in seconds and
//! compiled into ticks by [`super::secs`]. That is the whole discipline of
//! section 20 of the brief: a mine produces `60 IronOre / second`, never `1
//! IronOre / tick`, and certainly never `0.5`. Fractions are spelled as exact
//! integer schedules, so nothing anywhere accumulates a remainder that two
//! clients could round differently.
//!
//! ```text
//!   authored          compiled
//!   60 ore / second   produces 60 IronOre every 60 ticks
//!   2 second cycle    takes 120 ticks
//!   10 second load    base 600
//! ```
//!
//! # Six machines, and no recipes
//!
//! The machines in this catalogue have no recipe. They have a *design* -- the
//! same `.machine` documents experiments 06 to 10 were argued about in -- and
//! their recipe is whatever [`super::lower`] finds when it runs the design
//! until it repeats itself. Nobody typed "a stamping line makes 49.5 gears a
//! second"; the stamping line makes 1,980 gears every forty seconds because
//! that is what its press, its furnace and its motors do, and changing any of
//! them changes the number.
//!
//! That is the join between the two halves of the project, and it is the
//! reason a placed machine can be opened, redesigned, and committed back into
//! a factory that never stopped running.

use super::lower::Macro;
use super::{secs, PLOT};
use crate::json::Json;
use crate::model::{Qty, Tick};

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Role {
    Source,
    Storage,
    Machine,
    Sink,
    Transport,
}

impl Role {
    pub fn word(self) -> &'static str {
        match self {
            Role::Source => "source",
            Role::Storage => "storage",
            Role::Machine => "machine",
            Role::Sink => "sink",
            Role::Transport => "transport",
        }
    }

    /// Whether an installation of this role owns an editable design.
    ///
    /// Extraction heads do, since experiment 13: they are machines that happen
    /// to stand on a deposit, and how much of the seam you get is a question
    /// about the head you built.
    pub fn designed(self) -> bool {
        matches!(self, Role::Machine | Role::Source)
    }
}

/// What the prototype is, beyond a box with a name on it.
#[derive(Clone, Copy, Debug)]
pub enum Spec {
    /// Draws one substance out of the ground it is standing on.
    ///
    /// It used to be `Source { item, per_second }` -- a building that produced
    /// because the catalogue said it did, standing anywhere, at a rate nobody
    /// could argue with. That was the last magical object in the world, and
    /// note 1 of the play session went straight at it.
    ///
    /// So the *world* now offers the opportunity -- a [`Deposit`] in the
    /// ground, with a number on it -- and this is the machine that takes it.
    /// It runs a design like any other machine, so how much of the seam you
    /// actually get is a question about the head you built; and the seam caps
    /// it, so a better head on a thin seam buys you nothing. Which of those
    /// two is binding is the decision the room is really asking about.
    ///
    /// [`Deposit`]: super::world::Deposit
    Extract { item: &'static str, design: &'static str },
    /// Holds this much.
    Storage { capacity: Qty },
    /// Runs whatever design it was placed with.
    Machine { design: &'static str },
    /// Swallows `count` of the item it is set to, per tick. A delivery point
    /// is deliberately granular rather than batched: an order for eleven gears
    /// a second must be able to count eleven gears a second, and a sink that
    /// swallowed six hundred at a time would round the whole game.
    Sink { count: u64, item: Option<&'static str> },
    /// Carries batches between two bays. Latency comes from the distance
    /// between the bays and never from a number anybody typed.
    Transport { load: Qty, vehicles: u64, speed: u64, base: Tick },
}

#[derive(Debug)]
pub struct Proto {
    pub tag: &'static str,
    pub title: &'static str,
    /// The word an installation of this prototype is named with, before its
    /// id. Names are the identity the simulator carries state by, so they are
    /// generated from this and never from anything a player types.
    pub short: &'static str,
    pub role: Role,
    /// Footprint in world tiles. A machine's is derived from its design and
    /// this is only the fallback for one that will not compile.
    pub w: i32,
    pub h: i32,
    pub spec: Spec,
    pub blurb: &'static str,
}

/// How far apart the world's numbers are pitched.
///
/// Every rate here is chosen against a machine that already exists, because
/// the machines were not chosen at all -- their recipes are whatever their
/// designs do:
///
/// ```text
///   a steam crusher eats 93 ore/s      a mine gives 100
///   a compact plant drinks 160 water/s a water intake gives 400
///   a compact plant burns 40 coal/s    a coal pit gives 100
///   a machining cell wants 129 MW      a compact plant makes 108
/// ```
///
/// That last line is the one that makes the game: nothing in the catalogue
/// powers itself, so a gear line is always a gear line *and* a power station,
/// and a power station is always a coal problem.
pub static PROTOS: &[Proto] = &[
    // ---- extraction --------------------------------------------------
    Proto {
        tag: "oremine",
        short: "Mine",
        title: "Ore Head",
        role: Role::Source,
        w: 4,
        h: 4,
        spec: Spec::Extract {
            item: "IronOre",
            design: include_str!("../../designs/heads/oremine.machine"),
        },
        blurb: "Stands on an ore body. One inlet is 100 a second; the body decides whether it gets it.",
    },
    Proto {
        tag: "waterpump",
        short: "Intake",
        title: "Water Intake",
        role: Role::Source,
        w: 3,
        h: 3,
        spec: Spec::Extract {
            item: "Water",
            design: include_str!("../../designs/heads/waterpump.machine"),
        },
        blurb: "Stands on a water table. Two inlets is 400 a second, and a compact plant drinks 160.",
    },
    Proto {
        tag: "coalpit",
        short: "Coal",
        title: "Coal Head",
        role: Role::Source,
        w: 4,
        h: 3,
        spec: Spec::Extract {
            item: "Coal",
            design: include_str!("../../designs/heads/coalpit.machine"),
        },
        blurb: "Stands on a coal seam. One inlet is 100 a second, or two and a half compact plants.",
    },
    Proto {
        tag: "billetcaster",
        short: "Caster",
        title: "Billet Caster",
        role: Role::Source,
        w: 4,
        h: 3,
        spec: Spec::Extract {
            item: "IronBillet",
            design: include_str!("../../designs/heads/billetcaster.machine"),
        },
        blurb: "Stands on billet stock. 100 a second, ready to be pressed into something.",
    },
    Proto {
        tag: "crudewell",
        short: "Well",
        title: "Crude Well",
        role: Role::Source,
        w: 3,
        h: 3,
        spec: Spec::Extract {
            item: "Crude",
            design: include_str!("../../designs/heads/crudewell.machine"),
        },
        blurb: "Stands on a crude field. One inlet is 200 a second, for anyone with a refinery.",
    },
    // ---- storage -----------------------------------------------------
    Proto {
        tag: "bay",
        short: "Bay",
        title: "Bay",
        role: Role::Storage,
        w: 4,
        h: 4,
        spec: Spec::Storage { capacity: 20_000 },
        blurb: "20,000 of one item. Enough for one cycle of most machines and not two.",
    },
    Proto {
        tag: "yard",
        short: "Yard",
        title: "Yard",
        role: Role::Storage,
        w: 8,
        h: 6,
        spec: Spec::Storage { capacity: 120_000 },
        blurb: "120,000, and the only bay big enough to keep a crushing line fed while a train is away.",
    },
    // ---- machines ----------------------------------------------------
    Proto {
        tag: "steamplant",
        short: "Plant",
        title: "Compact Steam Plant",
        role: Role::Machine,
        w: 3,
        h: 2,
        spec: Spec::Machine { design: include_str!("../../designs/03-compact.machine") },
        blurb: "Coal and water in, 108 MW out. Its reactor runs at 40% on purpose.",
    },
    Proto {
        tag: "turbinehall",
        short: "Hall",
        title: "Turbine Hall",
        role: Role::Machine,
        w: 4,
        h: 3,
        spec: Spec::Machine { design: include_str!("../../designs/15-turbinehall.machine") },
        blurb: "132 MW, more water, more room. The one worth opening up and arguing with.",
    },
    Proto {
        tag: "pulseplant",
        short: "Pulse",
        title: "Pulse Plant",
        role: Role::Machine,
        w: 4,
        h: 2,
        spec: Spec::Machine { design: include_str!("../../designs/05-pulsed.machine") },
        blurb: "Fills a buffer quietly and empties it hard. Its average is poor and its peak is not.",
    },
    Proto {
        tag: "powderline",
        short: "Line",
        title: "Powder Line",
        role: Role::Machine,
        w: 5,
        h: 3,
        spec: Spec::Machine { design: include_str!("../../designs/18-powderline.machine") },
        blurb: "Two crushings and a milling, on 135 MW. Its mill is short of drive and it shows.",
    },
    Proto {
        tag: "crusher",
        short: "Crusher",
        title: "Steam Crusher",
        role: Role::Machine,
        w: 8,
        h: 4,
        spec: Spec::Machine { design: include_str!("../../designs/11-steamcrusher.machine") },
        blurb: "Makes its own power out of coal, and 37 concentrate a second out of 93 ore.",
    },
    Proto {
        tag: "stamping",
        short: "Press",
        title: "Stamping Line",
        role: Role::Machine,
        w: 6,
        h: 3,
        spec: Spec::Machine { design: include_str!("../../designs/08-stamping.machine") },
        blurb: "49 gears a second from billet -- and 60 MW off the grid to do it.",
    },
    Proto {
        tag: "machining",
        short: "Cell",
        title: "Machining Cell",
        role: Role::Machine,
        w: 3,
        h: 3,
        spec: Spec::Machine { design: include_str!("../../designs/09-machining.machine") },
        blurb: "Small, tidy, 24 gears a second, on 64 MW. Three fit where one stamping line does not.",
    },
    Proto {
        tag: "refinery",
        short: "Refinery",
        title: "Refinery Unit",
        role: Role::Machine,
        w: 7,
        h: 3,
        spec: Spec::Machine { design: include_str!("../../designs/10-refinery.machine") },
        blurb: "Crude in; light and middle fractions out, at 36 and 48 a second.",
    },
    // ---- delivery ----------------------------------------------------
    Proto {
        tag: "depot",
        short: "Depot",
        title: "Delivery Depot",
        role: Role::Sink,
        w: 4,
        h: 4,
        spec: Spec::Sink { count: 20, item: None },
        blurb: "Ships up to 1,200 a second of whatever it is wired to. Orders count what leaves here -- and so does everything a train takes to another room.",
    },
    Proto {
        tag: "grid",
        short: "Grid",
        title: "Grid Connection",
        role: Role::Sink,
        w: 3,
        h: 3,
        spec: Spec::Sink { count: 50, item: Some("Power") },
        blurb: "Sells electricity, up to 3,000 MW of it. Power goals are counted here.",
    },
    // ---- transport ---------------------------------------------------
    Proto {
        tag: "belt",
        short: "Belt",
        title: "Belt",
        role: Role::Transport,
        w: 1,
        h: 1,
        spec: Spec::Transport { load: 500, vehicles: 6, speed: 40, base: secs(1) },
        blurb: "Cheap, short, and continuous-ish: six loads of 500 in flight at a time.",
    },
    Proto {
        tag: "rail",
        short: "Rail",
        title: "Rail",
        role: Role::Transport,
        w: 1,
        h: 1,
        spec: Spec::Transport { load: 5_000, vehicles: 2, speed: 400, base: secs(10) },
        blurb: "Ten seconds to load and ten times the speed. Wins over about fifty tiles.",
    },
];

pub fn proto(tag: &str) -> Option<&'static Proto> {
    PROTOS.iter().find(|p| p.tag == tag)
}

impl Proto {
    /// The footprint as placed: turned a quarter turn if the player turned it.
    pub fn footprint(&self, face: u8) -> (i32, i32) {
        if face & 1 == 1 {
            (self.h, self.w)
        } else {
            (self.w, self.h)
        }
    }

    /// What one of these produces in a second, for a palette that would rather
    /// say so than make the player place one to find out.
    pub fn rate_note(&self) -> String {
        match self.spec {
            Spec::Extract { item, .. } => {
                format!("draws {} out of the ground", super::lower::item_title(item))
            }
            Spec::Storage { capacity } => format!("holds {capacity}"),
            Spec::Sink { count, item } => match item {
                Some(i) => format!("takes {} {}/s", count * super::SIM_TICK_RATE, super::lower::item_title(i)),
                None => format!("takes {}/s of one item", count * super::SIM_TICK_RATE),
            },
            Spec::Transport { load, vehicles, .. } => format!("{vehicles} x {load} per trip"),
            // A chassis, until somebody designs it. What it does is not the
            // catalogue's to say.
            Spec::Machine { .. } => "empty until you design it".to_string(),
        }
    }

    /// The design a freshly placed machine owns a copy of.
    pub fn design_source(&self) -> Option<&'static str> {
        match self.spec {
            Spec::Machine { design } | Spec::Extract { design, .. } => Some(design),
            _ => None,
        }
    }

    /// The substance this prototype draws out of the ground, if it draws one.
    pub fn extracts(&self) -> Option<&'static str> {
        match self.spec {
            Spec::Extract { item, .. } => Some(item),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Json {
        let j = Json::obj()
            .set("tag", self.tag)
            .set("title", self.title)
            .set("short", self.short)
            .set("role", self.role.word())
            .set("w", self.w as i64)
            .set("h", self.h as i64)
            .set("blurb", self.blurb)
            .set("note", self.rate_note());
        match self.spec {
            Spec::Extract { item, .. } => j
                .set("item", item)
                .set("example", true)
                // The palette has to say what it needs to stand on, or the
                // first thing a player learns about extraction is a refusal.
                .set("extracts", item)
                .set("needsDeposit", true),
            Spec::Storage { capacity } => j.set("capacity", Json::big(capacity as u128)),
            Spec::Sink { count, item } => j
                .set("count", Json::big(count as u128))
                .set("item", item.map(|s| s.to_string()))
                .set("choosesItem", item.is_none()),
            // Whether the book has a worked answer for this one. It is not
            // what a placement gives you -- that is a chassis -- but it can be
            // asked for by name, and the palette says so. Set on the existing
            // machine arm below, which also carries the example's numbers.
            Spec::Transport { load, vehicles, speed, base } => j
                .set("load", Json::big(load as u128))
                .set("vehicles", Json::big(vehicles as u128))
                .set("speed", Json::big(speed as u128))
                .set("base", base),
            Spec::Machine { design } => {
                let j = j.set("example", true);
                let d = crate::machine::design::Design::parse(design).ok();
                let m = d.as_ref().and_then(|d| super::lower::lower(d).ok());
                let (w, h) = m
                    .as_ref()
                    .map(|m: &Macro| (m.w, m.h))
                    .unwrap_or((self.w, self.h));
                j.set("w", w as i64).set("h", h as i64).set(
                    "macro",
                    match m {
                        Some(m) => m.to_json(),
                        None => Json::Null,
                    },
                )
            }
        }
    }
}

/// The whole catalogue, once, for a palette.
pub fn catalogue() -> Json {
    Json::obj()
        .set("ok", true)
        .set("plot", PLOT as i64)
        .set("tickRate", super::SIM_TICK_RATE)
        .set("items", Json::arr(super::lower::ITEMS.iter().map(|s| s.to_string()).collect::<Vec<_>>()))
        .set("protos", Json::Arr(PROTOS.iter().map(Proto::to_json).collect()))
}
