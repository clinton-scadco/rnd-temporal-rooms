//! What the room is for: a curated template, a seed, and the arithmetic that
//! decides whether the factory has done it yet.
//!
//! Goals are **not** generated. Twenty-one of them are written out by hand
//! below, each one a factory problem somebody thought was interesting, and the
//! seed only chooses among them and picks numbers inside ranges the template
//! declares. An unconstrained random goal is a random *brief*, and a random
//! brief is how a game ends up asking for eleven thousand gears from a plot
//! with no iron on it.
//!
//! ```text
//!   RoomSeed  ->  template  ->  numbers inside the template's ranges
//! ```
//!
//! The whole of it is a pure function of the seed, so the host and every
//! client compute the same objective without any of them sending it.
//!
//! # Progress is measured on a lattice
//!
//! A sustained rate is a question about a window, and a window needs two
//! measurements. If each replica measured whenever it happened to be asked,
//! two clients polling at different rates would disagree about a rate, then
//! about a completion, then about everything -- so nothing here is ever
//! counted except at a multiple of [`super::CHECK`]. A replica asked about
//! tick 12,345 answers with the state at 12,345 and the *accounts* at 12,300,
//! and says so.
//!
//! # Efficiency, and one honest substitution
//!
//! The brief suggests "waste less than X% heat". A percentage needs a
//! denominator, and the machine model has no number for "heat made" -- only
//! heat that became power and heat that was thrown away. So the efficiency
//! family asks two questions it can actually answer: a cap on an *input*
//! (`no more than 40,000 water`), and, for power plants, the share of the
//! heat they raised that never reached a turbine:
//!
//! ```text
//!   wasted / (wasted + power)   summed over the machines, weighted by cycles
//! ```
//!
//! which is a percentage with a meaning, and moves when the player turns a
//! reactor down.

use super::lower::item_title;
use super::{as_secs, secs, Rng, CHECK};
use crate::json::Json;
use crate::model::Tick;
use std::collections::BTreeMap;

// ==================================================================== shapes

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Family {
    Delivery,
    Throughput,
    Power,
    Efficiency,
    Space,
    Mixed,
}

impl Family {
    pub fn word(self) -> &'static str {
        match self {
            Family::Delivery => "delivery",
            Family::Throughput => "throughput",
            Family::Power => "power",
            Family::Efficiency => "efficiency",
            Family::Space => "space",
            Family::Mixed => "mixed",
        }
    }
}

/// What a goal actually asks for.
#[derive(Clone, Debug, PartialEq)]
pub enum Shape {
    /// Ship a pile of something.
    Deliver { item: String, qty: u64 },
    /// Ship two piles.
    DeliverPair { a: (String, u64), b: (String, u64) },
    /// Hold a rate for a while. A warehouse cannot answer this one.
    Sustain { item: String, per_sec: u64, secs: u64 },
    SustainPair { a: (String, u64), b: (String, u64), secs: u64 },
    /// Ship a pile without spending more than this much of something else.
    Frugal { item: String, qty: u64, cap_item: String, cap_qty: u64 },
    /// Hold a rate inside a footprint.
    Compact { item: String, per_sec: u64, secs: u64, tiles: i64 },
    /// Hold a load on the grid without throwing away more than this share of
    /// the heat that was raised for it.
    CleanPower { mw: u64, secs: u64, max_waste_pct: u64 },
    /// A load that will not sit still.
    ///
    /// Three requirements, and the third is the one with teeth:
    ///
    /// ```text
    ///   every second            >= base
    ///   some second in every N  >= peak
    ///   at most `hold` seconds in every N are above `spill`
    /// ```
    ///
    /// Section 5 of Prototype 3's brief asks for a room whose demand is
    /// unstable, and this is the only goal in the game that a *flat* factory
    /// cannot answer. Four steam plants held wide open deliver a beautiful
    /// straight line and fail on the third requirement; the answer is a plant
    /// that idles and then surges, which is a fact about a machine's orbit
    /// rather than about its average -- and keeping the orbit instead of the
    /// average is the one thing experiment 06 refused to give up.
    ///
    /// It asks about a peak *somewhere* in each window rather than at a named
    /// second, because the phase of a machine's orbit depends on the tick it
    /// was placed at, and a goal nobody can aim at is a lottery.
    Peak { base: u64, peak: u64, spill: u64, hold: u64, every: u64, secs: u64 },
    /// Two problems at once, both of which must be true.
    Both(Box<Shape>, Box<Shape>),
}

impl Shape {
    /// The seconds of history the shape needs to answer at all.
    pub fn window(&self) -> Tick {
        match self {
            Shape::Sustain { secs: s, .. }
            | Shape::SustainPair { secs: s, .. }
            | Shape::Compact { secs: s, .. }
            | Shape::CleanPower { secs: s, .. }
            | Shape::Peak { secs: s, .. } => secs(*s),
            Shape::Both(a, b) => a.window().max(b.window()),
            _ => 0,
        }
    }

    /// Every item this goal counts deliveries of.
    pub fn items(&self) -> Vec<String> {
        match self {
            Shape::Deliver { item, .. }
            | Shape::Sustain { item, .. }
            | Shape::Compact { item, .. } => vec![item.clone()],
            Shape::Frugal { item, .. } => vec![item.clone()],
            Shape::DeliverPair { a, b } => vec![a.0.clone(), b.0.clone()],
            Shape::SustainPair { a, b, .. } => vec![a.0.clone(), b.0.clone()],
            Shape::CleanPower { .. } | Shape::Peak { .. } => vec!["Power".to_string()],
            Shape::Both(a, b) => {
                let mut v = a.items();
                for i in b.items() {
                    if !v.contains(&i) {
                        v.push(i);
                    }
                }
                v
            }
        }
    }
}

// ================================================================ templates

pub struct Template {
    pub id: &'static str,
    pub family: Family,
    pub title: &'static str,
    /// Why this one is worth posing, in the sentence a designer would say.
    pub note: &'static str,
    pub make: fn(&mut Rng) -> Shape,
}

fn s(x: &str) -> String {
    x.to_string()
}

/// Twenty-one problems, written on purpose.
pub static TEMPLATES: &[Template] = &[
    // ---- delivery ----------------------------------------------------
    Template {
        id: "first-gears",
        family: Family::Delivery,
        title: "First Gears",
        note: "The smallest complete chain: billet in, gears out, something shipping them.",
        make: |r| Shape::Deliver { item: s("Gear"), qty: r.between(6_000, 12_000) },
    },
    Template {
        id: "gear-order",
        family: Family::Delivery,
        title: "Standing Gear Order",
        note: "Big enough that one press will not do it before anybody gets bored.",
        make: |r| Shape::Deliver { item: s("Gear"), qty: r.between(18_000, 30_000) },
    },
    Template {
        id: "concentrate-order",
        family: Family::Delivery,
        title: "Concentrate Contract",
        note: "Crushing is the coal-hungry half of the catalogue, and this is where you find out.",
        make: |r| Shape::Deliver { item: s("Concentrate"), qty: r.between(6_000, 12_000) },
    },
    Template {
        id: "light-order",
        family: Family::Delivery,
        title: "Light Fraction Order",
        note: "One refinery makes both fractions whether you wanted both or not.",
        make: |r| Shape::Deliver { item: s("LightFraction"), qty: r.between(4_000, 9_000) },
    },
    Template {
        id: "two-products",
        family: Family::Delivery,
        title: "Two Products",
        note: "Two chains that share nothing but the plot they are built on.",
        make: |r| Shape::DeliverPair {
            a: (s("Gear"), r.between(5_000, 9_000)),
            b: (s("Concentrate"), r.between(4_000, 7_000)),
        },
    },
    Template {
        id: "refinery-split",
        family: Family::Delivery,
        title: "Both Fractions",
        note: "The refinery's ratio is fixed at 3:4, so one of these two is always the binding one.",
        make: |r| Shape::DeliverPair {
            a: (s("LightFraction"), r.between(3_000, 6_000)),
            b: (s("MiddleFraction"), r.between(4_000, 8_000)),
        },
    },
    Template {
        id: "billet-stock",
        family: Family::Delivery,
        title: "Billet Stockpile",
        note: "No machine required at all -- which is worth knowing before you build one.",
        make: |r| Shape::Deliver { item: s("IronBillet"), qty: r.between(8_000, 16_000) },
    },
    // ---- sustained throughput ----------------------------------------
    Template {
        id: "steady-gears",
        family: Family::Throughput,
        title: "Steady Gears",
        note: "A rate, not a pile: a buffer that fills for four minutes and empties in ten seconds fails this.",
        make: |r| Shape::Sustain {
            item: s("Gear"),
            per_sec: r.between(18, 40),
            secs: r.between(30, 60),
        },
    },
    Template {
        id: "steady-concentrate",
        family: Family::Throughput,
        title: "Steady Concentrate",
        note: "Crushers are lumpy. Holding a rate through the lumps needs a bay in the right place.",
        make: |r| Shape::Sustain {
            item: s("Concentrate"),
            per_sec: r.between(12, 28),
            secs: r.between(30, 60),
        },
    },
    Template {
        id: "steady-light",
        family: Family::Throughput,
        title: "Steady Light Fraction",
        note: "The refinery is fast and thirsty for crude; the well is the thing to count.",
        make: |r| Shape::Sustain {
            item: s("LightFraction"),
            per_sec: r.between(20, 34),
            secs: r.between(30, 45),
        },
    },
    Template {
        id: "steady-pair",
        family: Family::Throughput,
        title: "Two Rates At Once",
        note: "Two lines, one clock. Neither may be starved to feed the other.",
        make: |r| Shape::SustainPair {
            a: (s("Gear"), r.between(12, 24)),
            b: (s("Concentrate"), r.between(10, 20)),
            secs: r.between(30, 45),
        },
    },
    // ---- power --------------------------------------------------------
    Template {
        id: "keep-the-lights-on",
        family: Family::Power,
        title: "Keep The Lights On",
        note: "One compact plant is 108 MW. The interesting numbers are just above that.",
        make: |r| Shape::Sustain {
            item: s("Power"),
            per_sec: r.between(130, 260),
            secs: r.between(30, 60),
        },
    },
    Template {
        id: "big-grid",
        family: Family::Power,
        title: "Grid Contract",
        note: "Enough that the coal, not the turbines, decides whether you make it.",
        make: |r| Shape::Sustain {
            item: s("Power"),
            per_sec: r.between(320, 560),
            secs: r.between(45, 90),
        },
    },
    Template {
        id: "power-bank",
        family: Family::Power,
        title: "Megawatt-Seconds",
        note: "A total rather than a rate, for a room that would rather build than hold on.",
        make: |r| Shape::Deliver { item: s("Power"), qty: r.between(12_000, 40_000) },
    },
    Template {
        id: "clean-power",
        family: Family::Efficiency,
        title: "Clean Power",
        note: "A reactor at full throttle heats the sky. Open the plant up and turn it down.",
        make: |r| Shape::CleanPower {
            mw: r.between(110, 230),
            secs: r.between(30, 45),
            max_waste_pct: r.between(20, 45),
        },
    },
    Template {
        id: "power-and-product",
        family: Family::Mixed,
        title: "Power And Product",
        note: "The stamping line runs off the grid, so the two halves of this one are the same half.",
        make: |r| Shape::SustainPair {
            a: (s("Power"), r.between(90, 180)),
            b: (s("Gear"), r.between(10, 20)),
            secs: r.between(30, 45),
        },
    },
    // ---- efficiency ---------------------------------------------------
    Template {
        id: "frugal-gears",
        family: Family::Efficiency,
        title: "Gears On A Water Budget",
        note: "Everything that makes power drinks. Count the intakes before you count the presses.",
        make: |r| Shape::Frugal {
            item: s("Gear"),
            qty: r.between(3_000, 6_000),
            cap_item: s("Water"),
            cap_qty: r.between(60_000, 120_000),
        },
    },
    Template {
        id: "frugal-concentrate",
        family: Family::Efficiency,
        title: "Concentrate On A Coal Budget",
        note: "A steam crusher burns coal to crush ore. There is a cheaper way to power one.",
        make: |r| Shape::Frugal {
            item: s("Concentrate"),
            qty: r.between(4_000, 8_000),
            cap_item: s("Coal"),
            cap_qty: r.between(20_000, 45_000),
        },
    },
    Template {
        id: "frugal-light",
        family: Family::Efficiency,
        title: "Fractions Without The Fire",
        note: "The refinery's burner is the whole of its coal bill, and the whole of its heat.",
        make: |r| Shape::Frugal {
            item: s("LightFraction"),
            qty: r.between(3_000, 6_000),
            cap_item: s("Coal"),
            cap_qty: r.between(6_000, 14_000),
        },
    },
    // ---- space ---------------------------------------------------------
    Template {
        id: "compact-gears",
        family: Family::Space,
        title: "Gears In A Small Yard",
        note: "A machine's footprint is its design's footprint, so this is a question about the inside of one.",
        make: |r| Shape::Compact {
            item: s("Gear"),
            per_sec: r.between(12, 24),
            secs: r.between(25, 40),
            tiles: r.between(1_100, 2_000) as i64,
        },
    },
    Template {
        id: "compact-power",
        family: Family::Space,
        title: "Power In A Small Yard",
        note: "Turbine halls are wide. Compact plants are not. That is the entire puzzle.",
        make: |r| Shape::Compact {
            item: s("Power"),
            per_sec: r.between(120, 240),
            secs: r.between(25, 40),
            tiles: r.between(1_100, 2_200) as i64,
        },
    },
    // ---- prototype 3: the five rooms -----------------------------------
    //
    // These five take no numbers from the seed at all. A campaign is a
    // *sequence* of problems that lean on each other -- the second room's
    // coal comes from the first, and the fourth room's presses were unlocked
    // by the third -- and a sequence whose middle term is rolled cannot be
    // balanced by anybody. So the rooms are authored, exactly as section 5 of
    // the brief asks for: five different problems rather than one problem with
    // five sizes.
    //
    // They are templates rather than a table of their own because a goal has
    // to survive a snapshot, and the only thing a snapshot carries is a seed
    // and a template id. See `camp::site` for the rooms these belong to.
    Template {
        id: "site-basin",
        family: Family::Space,
        title: "Coal Basin",
        note: "All the coal and water anybody could want, on a platform the size of a car park.",
        make: |_| Shape::Compact {
            item: s("Power"),
            per_sec: 400,
            secs: 45,
            tiles: 480,
        },
    },
    Template {
        id: "site-valley",
        family: Family::Delivery,
        title: "Iron Valley",
        note: "Ore to the horizon and a coal seam that will keep one plant alight. The rest arrives by rail.",
        make: |_| Shape::Deliver { item: s("OrePowder"), qty: 24_000 },
    },
    Template {
        id: "site-station",
        family: Family::Power,
        title: "Power Station",
        note: "Water to spare and no fuel at all. Everything it burns spent a minute on a train.",
        make: |_| Shape::Sustain { item: s("Power"), per_sec: 320, secs: 45 },
    },
    Template {
        id: "site-works",
        family: Family::Throughput,
        title: "Manufacturing",
        note: "No coal, no water, no grid. Everything this room runs on spent two minutes on a train.",
        make: |_| Shape::Sustain { item: s("Gear"), per_sec: 45, secs: 45 },
    },
    Template {
        id: "site-final",
        family: Family::Mixed,
        title: "Final Works",
        note: "A load that will not sit still, and a gear line that has to keep going while it moves.",
        make: |_| {
            Shape::Both(
                Box::new(Shape::Peak {
                    base: 110,
                    peak: 380,
                    spill: 240,
                    hold: 2,
                    every: 10,
                    secs: 75,
                }),
                Box::new(Shape::SustainPair {
                    a: (s("Gear"), 30),
                    b: (s("Concentrate"), 20),
                    secs: 45,
                }),
            )
        },
    },
];

pub fn template(id: &str) -> Option<&'static Template> {
    TEMPLATES.iter().find(|t| t.id == id)
}

impl Template {
    /// Whether a seed may roll this one.
    ///
    /// The five campaign rooms may not be rolled. They are authored to lean
    /// on each other, and one of them turning up on its own in a single-room
    /// game would be a brief with half its premises missing -- "no coal, no
    /// water, no grid" is a fine problem when there is a railway, and an
    /// unwinnable one when there is not.
    pub fn rollable(&self) -> bool {
        !self.id.starts_with("site-")
    }
}

/// The templates a seed may choose among.
pub fn rollable() -> Vec<&'static Template> {
    TEMPLATES.iter().filter(|t| t.rollable()).collect()
}

// ==================================================================== goal

#[derive(Clone, Debug)]
pub struct Goal {
    pub template: &'static str,
    pub family: Family,
    pub title: String,
    pub note: String,
    pub shape: Shape,
    pub seed: u64,
}

impl Goal {
    /// The objective this seed asks for. Deterministic, and the only random
    /// thing in the game.
    pub fn of_seed(seed: u64, forced: Option<&str>) -> Goal {
        let mut r = Rng(seed);
        let t = match forced.and_then(template) {
            Some(t) => t,
            None => {
                let open = rollable();
                open[r.pick(open.len())]
            }
        };
        let shape = (t.make)(&mut r);
        Goal {
            template: t.id,
            family: t.family,
            title: t.title.to_string(),
            note: t.note.to_string(),
            shape,
            seed,
        }
    }

    /// The objective in one sentence, in seconds and whole items.
    pub fn brief(&self) -> String {
        sentence(&self.shape)
    }


    /// What the room is furnished with at tick zero.
    ///
    /// Not a hint and not a tutorial: the raw materials the goal is about, a
    /// bay each, and somewhere to ship the answer. Everything between them is
    /// the game.
    /// What the room is furnished with.
    ///
    /// Two lists, and the split is experiment 13's first change: the *ground*
    /// is what the room has, and the buildings are what it hands you. Raw
    /// material used to be a building -- a mine that produced ore because the
    /// catalogue said so -- and is now a patch of ground with a number on it
    /// and nothing standing on it at all.
    pub fn starting_kit(&self) -> Furnishing {
        let mut ground: Vec<&'static str> = Vec::new();
        let mut want = |item: &'static str| {
            if !ground.contains(&item) {
                ground.push(item);
            }
        };
        let items = self.shape.items();
        for item in &items {
            match item.as_str() {
                "Gear" => {
                    want("IronBillet");
                    want("Coal");
                }
                "Concentrate" | "OrePowder" => {
                    want("IronOre");
                    want("Coal");
                    want("Water");
                }
                "LightFraction" | "MiddleFraction" | "HeavyFraction" => {
                    want("Crude");
                    want("Coal");
                    want("Water");
                }
                "Power" => {
                    want("Coal");
                    want("Water");
                }
                "IronBillet" => want("IronBillet"),
                "IronOre" => want("IronOre"),
                _ => want("IronOre"),
            }
        }
        if let Shape::Frugal { cap_item, .. } = &self.shape {
            match cap_item.as_str() {
                "Water" => want("Water"),
                "Coal" => want("Coal"),
                _ => {}
            }
        }
        // A stamping line runs off the grid, so anything that ends in gears
        // needs somewhere to make electricity as well.
        if items.iter().any(|i| i == "Gear") {
            want("Water");
        }
        let mut builds: Vec<(&'static str, Option<String>)> = Vec::new();
        // A bay beside each seam, because somewhere to put what comes out of
        // the ground is not the interesting part of the problem.
        for _ in &ground {
            builds.push(("bay", None));
        }
        for item in items {
            if item == "Power" {
                builds.push(("grid", None));
            } else {
                builds.push(("depot", Some(item)));
            }
        }
        Furnishing { ground, builds }
    }

    pub fn to_json(&self, p: &Progress) -> Json {
        Json::obj()
            .set("template", self.template)
            .set("family", self.family.word())
            .set("title", self.title.clone())
            .set("note", self.note.clone())
            .set("brief", self.brief())
            .set("seed", Json::big(self.seed as u128))
            .set("window", self.shape.window())
            .set("progress", p.to_json())
    }
}

/// A room as it stands before anybody has built anything: what is under it,
/// and what is on it.
#[derive(Clone, Debug, Default)]
pub struct Furnishing {
    /// Items there is ground for, in the order the room lays them out.
    pub ground: Vec<&'static str>,
    /// Buildings the room comes with: a bay per seam, and the delivery points.
    pub builds: Vec<(&'static str, Option<String>)>,
}

/// One objective, in the sentence a player reads.
///
/// A free function rather than a method because [`Shape::Both`] has to ask it
/// about its halves, and a goal made of two problems should read as two
/// sentences rather than as a new kind of grammar.
pub fn sentence(shape: &Shape) -> String {
    let n = commas;
    match shape {
        Shape::Deliver { item, qty } => format!("Deliver {} {}.", n(*qty), item_title(item)),
        Shape::DeliverPair { a, b } => format!(
            "Deliver {} {} and {} {}.",
            n(a.1),
            item_title(&a.0),
            n(b.1),
            item_title(&b.0)
        ),
        Shape::Sustain { item, per_sec, secs } => format!(
            "Hold {} {} a second for {secs} seconds.",
            n(*per_sec),
            item_title(item)
        ),
        Shape::SustainPair { a, b, secs } => format!(
            "Hold {} {} and {} {} a second, both at once, for {secs} seconds.",
            n(a.1),
            item_title(&a.0),
            n(b.1),
            item_title(&b.0)
        ),
        Shape::Frugal { item, qty, cap_item, cap_qty } => format!(
            "Deliver {} {} having drawn no more than {} {}.",
            n(*qty),
            item_title(item),
            n(*cap_qty),
            item_title(cap_item)
        ),
        Shape::Compact { item, per_sec, secs, tiles } => format!(
            "Hold {} {} a second for {secs} seconds, with the whole factory inside {} tiles.",
            n(*per_sec),
            item_title(item),
            n(*tiles as u64)
        ),
        Shape::CleanPower { mw, secs, max_waste_pct } => format!(
            "Hold {} MW for {secs} seconds while wasting less than {max_waste_pct}% of the heat you raise.",
            n(*mw)
        ),
        Shape::Peak { base, peak, spill, hold, every, secs } => format!(
            "For {secs} seconds: never below {} MW, over {} MW at least once in every {every} seconds, and above {} MW for no more than {hold} seconds in any {every}.",
            n(*base),
            n(*peak),
            n(*spill)
        ),
        Shape::Both(a, b) => format!("{} {}", sentence(a), sentence(b)),
    }
}

pub fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ================================================================ accounting

/// One canonical measurement, taken at a multiple of [`super::CHECK`].
#[derive(Clone, Debug, Default)]
pub struct Sample {
    pub at: Tick,
    /// Cumulative deliveries, by item, since the room began.
    pub shipped: BTreeMap<String, u64>,
    pub footprint: i64,
    /// Wasted heat and raised power, cycle-weighted over the machines that
    /// were installed at the time.
    pub wasted: u128,
    pub power: u128,
}

impl Sample {
    pub fn got(&self, item: &str) -> u64 {
        self.shipped.get(item).copied().unwrap_or(0)
    }
    pub fn waste_pct(&self) -> u64 {
        let total = self.wasted + self.power;
        if total == 0 {
            0
        } else {
            (self.wasted * 100 / total) as u64
        }
    }
}

/// The room's books.
///
/// Cumulative totals are accumulated from *deltas* rather than read straight
/// off the counters, because a delivery depot that is deleted and rebuilt
/// starts counting from zero again -- and an order that un-delivered eight
/// thousand gears because somebody moved a shed would be a strange game.
#[derive(Clone, Debug, Default)]
pub struct Acct {
    pub shipped: BTreeMap<String, u64>,
    pub drawn: BTreeMap<String, u64>,
    /// The last raw reading, to difference against.
    ///
    /// Carried in a snapshot along with everything else. It has to be: a
    /// joining replica whose first reading was differenced against zero would
    /// count every delivery in the room's history a second time, on top of the
    /// totals it was just handed.
    pub raw_ship: BTreeMap<String, u64>,
    pub raw_draw: BTreeMap<String, u64>,
    /// The recent past, at one sample a second, long enough for the longest
    /// window any goal asks about.
    pub samples: Vec<Sample>,
    /// The last lattice point counted.
    pub at: Tick,
    pub done_at: Option<Tick>,
    pub done: Option<Done>,
}

/// The longest window any template asks for, plus a second of slack.
pub const HISTORY: usize = 95;

impl Acct {
    /// Fold one canonical measurement into the books.
    pub fn count(
        &mut self,
        at: Tick,
        shipped: &BTreeMap<String, u64>,
        drawn: &BTreeMap<String, u64>,
        footprint: i64,
        wasted: u128,
        power: u128,
    ) {
        if at < self.at {
            return;
        }
        bump(&mut self.shipped, &mut self.raw_ship, shipped);
        bump(&mut self.drawn, &mut self.raw_draw, drawn);
        self.at = at;
        let s = Sample {
            at,
            shipped: self.shipped.clone(),
            footprint,
            wasted,
            power,
        };
        match self.samples.last_mut() {
            Some(last) if last.at == at => *last = s,
            _ => self.samples.push(s),
        }
        if self.samples.len() > HISTORY {
            let cut = self.samples.len() - HISTORY;
            self.samples.drain(..cut);
        }
    }

    pub fn got(&self, item: &str) -> u64 {
        self.shipped.get(item).copied().unwrap_or(0)
    }
    pub fn used(&self, item: &str) -> u64 {
        self.drawn.get(item).copied().unwrap_or(0)
    }

    /// The sample `window` ticks before the newest one, if the room has been
    /// running that long.
    fn window(&self, window: Tick) -> Option<(&Sample, &Sample)> {
        let now = self.samples.last()?;
        if now.at < window {
            return None;
        }
        let want = now.at - window;
        let then = self.samples.iter().find(|s| s.at == want)?;
        Some((then, now))
    }

    pub fn signature(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&self.at.to_le_bytes());
        for (k, n) in &self.shipped {
            v.extend_from_slice(k.as_bytes());
            v.extend_from_slice(&n.to_le_bytes());
        }
        v.push(0xa0);
        for (k, n) in &self.drawn {
            v.extend_from_slice(k.as_bytes());
            v.extend_from_slice(&n.to_le_bytes());
        }
        v.push(0xa1);
        v.extend_from_slice(&self.done_at.unwrap_or(Tick::MAX).to_le_bytes());
        v
    }

    pub fn to_json(&self) -> Json {
        let totals = |m: &BTreeMap<String, u64>| {
            Json::Arr(
                m.iter()
                    .map(|(k, n)| {
                        Json::obj().set("item", k.clone()).set("qty", Json::big(*n as u128))
                    })
                    .collect(),
            )
        };
        Json::obj()
            .set("at", self.at)
            .set("shipped", totals(&self.shipped))
            .set("drawn", totals(&self.drawn))
            .set("rawShipped", totals(&self.raw_ship))
            .set("rawDrawn", totals(&self.raw_draw))
            .set(
                "samples",
                Json::Arr(
                    self.samples
                        .iter()
                        .map(|s| {
                            Json::obj()
                                .set("at", s.at)
                                .set("shipped", totals(&s.shipped))
                                .set("footprint", s.footprint)
                                .set("wasted", Json::big(s.wasted))
                                .set("power", Json::big(s.power))
                        })
                        .collect(),
                ),
            )
            .set("doneAt", self.done_at.map(|t| Json::Int(t as i128)))
            .set(
                "done",
                match &self.done {
                    Some(d) => d.to_json(),
                    None => Json::Null,
                },
            )
    }

    pub fn from_json(j: &Json) -> Acct {
        let totals = |v: &Json| -> BTreeMap<String, u64> {
            v.as_arr()
                .iter()
                .filter_map(|e| Some((e.at("item").as_str()?.to_string(), e.at("qty").as_u64()?)))
                .collect()
        };
        Acct {
            shipped: totals(j.at("shipped")),
            drawn: totals(j.at("drawn")),
            raw_ship: totals(j.at("rawShipped")),
            raw_draw: totals(j.at("rawDrawn")),
            samples: j
                .at("samples")
                .as_arr()
                .iter()
                .map(|e| Sample {
                    at: e.at("at").as_u64().unwrap_or(0),
                    shipped: totals(e.at("shipped")),
                    footprint: e.at("footprint").as_i128().unwrap_or(0) as i64,
                    wasted: e.at("wasted").as_u64().unwrap_or(0) as u128,
                    power: e.at("power").as_u64().unwrap_or(0) as u128,
                })
                .collect(),
            at: j.at("at").as_u64().unwrap_or(0),
            done_at: j.at("doneAt").as_u64(),
            done: Done::from_json(j.at("done")),
        }
    }
}

/// Cumulative totals are accumulated from differences, so that a delivery
/// depot which is deleted and rebuilt -- and whose counter therefore starts
/// again at zero -- does not un-deliver eight thousand gears.
fn bump(
    total: &mut BTreeMap<String, u64>,
    raw: &mut BTreeMap<String, u64>,
    now: &BTreeMap<String, u64>,
) {
    for (k, v) in now {
        let was = raw.get(k).copied().unwrap_or(0);
        if *v > was {
            *total.entry(k.clone()).or_insert(0) += v - was;
        }
        raw.insert(k.clone(), *v);
    }
    // Something that vanished is not a debit; it is a counter that went away
    // with the depot it belonged to.
    raw.retain(|k, _| now.contains_key(k));
}

// ================================================================== progress

/// What kind of question a requirement is asking.
///
/// The play session completed a room, disconnected one of the power stations
/// that had completed it, watched the number fall, and could not tell from the
/// screen whether the room was still passed. Both answers were true and the
/// panel only had room for one of them: the *room* stays finished, because
/// that is the campaign's rule, and the *requirement* is false right now.
///
/// So a requirement says which sort it is, and the panel can stop pretending
/// they are the same sort.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    /// A pile that only grows: ship ten thousand gears. Once true, true
    /// forever, because nothing in the game un-ships anything.
    Achievement,
    /// A fact about the factory as it is now -- a rate over a trailing window,
    /// a footprint, a share of heat wasted. It became true and it can stop
    /// being true, and a player whose factory has quietly collapsed is
    /// entitled to find that out from the objective panel rather than from the
    /// next room.
    State,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Achievement => "achievement",
            Kind::State => "state",
        }
    }
}

/// One requirement, and how it is going.
#[derive(Clone, Debug)]
pub struct Line {
    pub what: String,
    pub have: f64,
    pub need: f64,
    pub unit: &'static str,
    pub met: bool,
    pub kind: Kind,
}

#[derive(Clone, Debug)]
pub struct Progress {
    pub lines: Vec<Line>,
    pub met: bool,
    /// The tick every requirement was first satisfied at, if it has been.
    pub done_at: Option<Tick>,
    pub done: Option<Done>,
    /// True while the room has not been running long enough to answer a
    /// windowed question at all.
    pub warming: bool,
}

impl Progress {
    /// Whether every *live* requirement is satisfied at this instant.
    ///
    /// [`Progress::met`] answers "is the whole objective true", which for a
    /// finished room is a question about history. This answers "is the factory
    /// doing it *now*", which is the question a player standing in a room that
    /// used to work is actually asking. They differ exactly when something has
    /// been unplugged since.
    pub fn holding(&self) -> bool {
        self.lines.iter().filter(|l| l.kind == Kind::State).all(|l| l.met)
    }

    /// The live requirements that are not being met.
    pub fn slipped(&self) -> Vec<&Line> {
        self.lines.iter().filter(|l| l.kind == Kind::State && !l.met).collect()
    }
}

/// What the run looked like at the moment it was won.
#[derive(Clone, Debug, Default)]
pub struct Done {
    pub at: Tick,
    pub installs: usize,
    pub machines: usize,
    pub designs: usize,
    pub footprint: i64,
    pub shipped: BTreeMap<String, u64>,
    pub drawn: BTreeMap<String, u64>,
}

impl Done {
    pub fn to_json(&self) -> Json {
        let totals = |m: &BTreeMap<String, u64>| {
            Json::Arr(
                m.iter()
                    .map(|(k, n)| {
                        Json::obj().set("item", k.clone()).set("qty", Json::big(*n as u128))
                    })
                    .collect(),
            )
        };
        Json::obj()
            .set("at", self.at)
            .set("seconds", as_secs(self.at))
            .set("installs", self.installs as i64)
            .set("machines", self.machines as i64)
            .set("designs", self.designs as i64)
            .set("footprint", self.footprint)
            .set("shipped", totals(&self.shipped))
            .set("drawn", totals(&self.drawn))
    }

    pub fn from_json(j: &Json) -> Option<Done> {
        let at = j.at("at").as_u64()?;
        let totals = |v: &Json| -> BTreeMap<String, u64> {
            v.as_arr()
                .iter()
                .filter_map(|e| Some((e.at("item").as_str()?.to_string(), e.at("qty").as_u64()?)))
                .collect()
        };
        Some(Done {
            at,
            installs: j.at("installs").as_u64().unwrap_or(0) as usize,
            machines: j.at("machines").as_u64().unwrap_or(0) as usize,
            designs: j.at("designs").as_u64().unwrap_or(0) as usize,
            footprint: j.at("footprint").as_i128().unwrap_or(0) as i64,
            shipped: totals(j.at("shipped")),
            drawn: totals(j.at("drawn")),
        })
    }
}

impl Progress {
    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("met", self.met)
            .set("warming", self.warming)
            // Whether the factory is doing it *now*, as opposed to whether it
            // ever did. A finished room whose power station has been unplugged
            // is `met` and not `holding`, and the panel says both.
            .set("holding", self.holding())
            .set(
                "slipped",
                Json::Arr(
                    self.slipped().iter().map(|l| Json::Str(l.what.clone())).collect(),
                ),
            )
            .set("doneAt", self.done_at.map(|t| Json::Int(t as i128)))
            .set("doneSeconds", self.done_at.map(|t| Json::Real(as_secs(t))))
            .set(
                "done",
                match &self.done {
                    Some(d) => d.to_json(),
                    None => Json::Null,
                },
            )
            .set(
                "lines",
                Json::Arr(
                    self.lines
                        .iter()
                        .map(|l| {
                            Json::obj()
                                .set("what", l.what.clone())
                                .set("have", l.have)
                                .set("need", l.need)
                                .set("unit", l.unit)
                                .set("met", l.met)
                                .set("kind", l.kind.word())
                        })
                        .collect(),
                ),
            )
    }
}

/// How the room is doing, at the last lattice point it counted.
pub fn evaluate(goal: &Goal, acct: &Acct) -> Progress {
    let mut lines: Vec<Line> = Vec::new();
    let mut warming = false;
    judge(&goal.shape, acct, &mut lines, &mut warming);
    let met = !lines.is_empty() && lines.iter().all(|l| l.met);
    Progress { lines, met, done_at: acct.done_at, done: acct.done.clone(), warming }
}

/// One shape's requirements, appended. Recursive, so that [`Shape::Both`] is
/// two problems rather than a third kind of problem.
fn judge(shape: &Shape, acct: &Acct, lines: &mut Vec<Line>, warming: &mut bool) {
    let rate = |acct: &Acct, item: &str, secs_of: u64| -> Option<f64> {
        let (then, now) = acct.window(secs(secs_of))?;
        Some((now.got(item) - then.got(item)) as f64 / secs_of.max(1) as f64)
    };
    let pile = |lines: &mut Vec<Line>, item: &str, qty: u64| {
        let have = acct.got(item) as f64;
        lines.push(Line {
            what: format!("{} delivered", item_title(item)),
            have,
            need: qty as f64,
            unit: "",
            met: have >= qty as f64,
            kind: Kind::Achievement,
        });
    };
    match shape {
        Shape::Deliver { item, qty } => pile(lines, item, *qty),
        Shape::DeliverPair { a, b } => {
            pile(lines, &a.0, a.1);
            pile(lines, &b.0, b.1);
        }
        Shape::Frugal { item, qty, cap_item, cap_qty } => {
            pile(lines, item, *qty);
            let used = acct.used(cap_item) as f64;
            lines.push(Line {
                what: format!("{} drawn", item_title(cap_item)),
                have: used,
                need: *cap_qty as f64,
                unit: "at most",
                // A budget is spent, never refunded, so it is history like the
                // pile above it rather than a fact about this second.
                met: used <= *cap_qty as f64,
                kind: Kind::Achievement,
            });
        }
        Shape::Sustain { item, per_sec, secs: w } => {
            let got = rate(acct, item, *w);
            *warming |= got.is_none();
            lines.push(Line {
                what: format!("{} a second, held for {w}s", item_title(item)),
                have: got.unwrap_or(0.0),
                need: *per_sec as f64,
                unit: if item == "Power" { "MW" } else { "/s" },
                met: got.unwrap_or(0.0) >= *per_sec as f64,
                kind: Kind::State,
            });
        }
        Shape::SustainPair { a, b, secs: w } => {
            for (item, need) in [a, b] {
                let got = rate(acct, item, *w);
                *warming |= got.is_none();
                lines.push(Line {
                    what: format!("{} a second, held for {w}s", item_title(item)),
                    have: got.unwrap_or(0.0),
                    need: *need as f64,
                    unit: if item == "Power" { "MW" } else { "/s" },
                    met: got.unwrap_or(0.0) >= *need as f64,
                    kind: Kind::State,
                });
            }
        }
        Shape::Compact { item, per_sec, secs: w, tiles } => {
            let got = rate(acct, item, *w);
            *warming |= got.is_none();
            lines.push(Line {
                what: format!("{} a second, held for {w}s", item_title(item)),
                have: got.unwrap_or(0.0),
                need: *per_sec as f64,
                unit: if item == "Power" { "MW" } else { "/s" },
                met: got.unwrap_or(0.0) >= *per_sec as f64,
                kind: Kind::State,
            });
            let foot = acct.samples.last().map(|s| s.footprint).unwrap_or(0) as f64;
            lines.push(Line {
                what: "the factory's footprint".into(),
                have: foot,
                need: *tiles as f64,
                unit: "tiles at most",
                met: foot <= *tiles as f64,
                kind: Kind::State,
            });
        }
        Shape::CleanPower { mw, secs: w, max_waste_pct } => {
            let got = rate(acct, "Power", *w);
            *warming |= got.is_none();
            lines.push(Line {
                what: format!("megawatts, held for {w}s"),
                have: got.unwrap_or(0.0),
                need: *mw as f64,
                unit: "MW",
                met: got.unwrap_or(0.0) >= *mw as f64,
                kind: Kind::State,
            });
            let pct = acct.samples.last().map(|s| s.waste_pct()).unwrap_or(0) as f64;
            lines.push(Line {
                what: "of the heat raised, wasted".into(),
                have: pct,
                need: *max_waste_pct as f64,
                unit: "% at most",
                met: pct <= *max_waste_pct as f64,
                kind: Kind::State,
            });
        }
        Shape::Peak { base, peak, spill, hold, every, secs: w } => {
            // Every second in the window, as a delivery rather than a total:
            // a peak is a fact about one second, and a cumulative counter has
            // no opinion about seconds at all.
            match deliveries(acct, "Power", *w) {
                None => {
                    *warming = true;
                    lines.push(Line {
                        what: format!("the load, second by second, over {w}s"),
                        have: 0.0,
                        need: *base as f64,
                        unit: "MW",
                        met: false,
                        kind: Kind::State,
                    });
                }
                Some(mw) => {
                    let span = (*every as usize).max(1).min(mw.len());
                    let floor = mw.iter().copied().min().unwrap_or(0);
                    // The worst window, twice over: the quietest `every`
                    // seconds anywhere in the run, and the busiest. A surge
                    // that happened once is not a surge, and a plant that
                    // spilled once is not a spiller.
                    let surge = mw
                        .windows(span)
                        .map(|w| w.iter().copied().max().unwrap_or(0))
                        .min()
                        .unwrap_or(0);
                    let over = mw
                        .windows(span)
                        .map(|w| w.iter().filter(|&&v| v > *spill).count() as u64)
                        .max()
                        .unwrap_or(0);
                    lines.push(Line {
                        what: "the floor, at its worst second".into(),
                        have: floor as f64,
                        need: *base as f64,
                        unit: "MW",
                        met: floor >= *base,
                        kind: Kind::State,
                    });
                    lines.push(Line {
                        what: format!("a surge, in every {every}s"),
                        have: surge as f64,
                        need: *peak as f64,
                        unit: "MW",
                        met: surge >= *peak,
                        kind: Kind::State,
                    });
                    lines.push(Line {
                        what: format!("seconds over {} MW, in any {every}s", commas(*spill)),
                        have: over as f64,
                        need: *hold as f64,
                        unit: "at most",
                        met: over <= *hold,
                        kind: Kind::State,
                    });
                }
            }
        }
        Shape::Both(a, b) => {
            judge(a, acct, lines, warming);
            judge(b, acct, lines, warming);
        }
    }
}

/// What was delivered in each of the last `w` seconds, or `None` if the room
/// has not been running that long.
///
/// The books hold cumulative totals at one sample a second, so a second's
/// delivery is a difference between neighbours -- and the neighbours are on
/// the lattice, which is what makes two replicas agree about a spike.
fn deliveries(acct: &Acct, item: &str, w: u64) -> Option<Vec<u64>> {
    let now = acct.samples.last()?;
    if now.at < secs(w) {
        return None;
    }
    let from = now.at - secs(w);
    let mut out = Vec::with_capacity(w as usize);
    let mut prev: Option<&Sample> = None;
    for s in acct.samples.iter().filter(|s| s.at >= from) {
        if let Some(p) = prev {
            out.push(s.got(item).saturating_sub(p.got(item)));
        }
        prev = Some(s);
    }
    (out.len() as u64 >= w).then_some(out)
}

/// The next lattice point at or after `t`.
pub fn next_check(t: Tick) -> Tick {
    t.div_ceil(CHECK) * CHECK
}
