//! Logistics across a hundred and eighty years: what a fracture is, what holds
//! one open, and why the ore that came out of 1890 is still 1890 ore when it
//! gets to 2070.
//!
//! ```text
//!   1890 Mining Valley
//!         |  the deep fracture -- natural, 147 years, 98 MW to hold open
//!         v
//!   2037 Industrial District
//!         |  the near corridor -- engineered, 33 years, 53 MW, already standing
//!         v
//!   2070 Manufacturing Zone
//! ```
//!
//! # An origin is carried, not inferred
//!
//! Every load in this module has an [`origin`](Stamped) on it, and the rule the
//! brief asks for falls out of one line:
//!
//! ```text
//!   same phase        -> ordinary logistics
//!   different phase   -> a temporal interface, and it costs power to hold
//! ```
//!
//! The interesting half is what "different phase" means for material that has
//! already moved once. A region's exports are drawn **FIFO out of what it
//! imported**, and only the remainder is stamped with the region's own phase.
//! So:
//!
//! ```text
//!   1890 ore -> 2037 -> shipped on raw    still 1890 ore: 180 years to 2070
//!   1890 ore -> 2037 -> crushed -> 2070   2037 concentrate: 33 years
//! ```
//!
//! Nothing in this module special-cases processing. The second line is true
//! because concentrate is a different item from ore, so its provenance queue is
//! empty and the district's own phase is what is left to stamp it with. The
//! mechanic that makes the intended play the cheap play is one queue and a
//! subtraction, and it is exact: material cannot be laundered by being carried
//! through a later century, because carrying is not making.
//!
//! **Where the exactness stops, said plainly.** The solver holds one number per
//! bay per item, so two lorryloads of ore with different origins sitting in the
//! same bay are one number and are fungible. Provenance is therefore tracked on
//! the *flow* -- what crossed, in which direction, from where -- and not on each
//! lump. That is the honest granularity available underneath, and it is enough
//! for every question this experiment asks, because every such question is
//! about a crossing.
//!
//! # A fracture is held open by somebody's grid
//!
//! There is no build cost and no research. An interface is *opened* by a
//! command and then it either holds or it does not, every five simulated
//! seconds, depending on whether the region that owns it is delivering the
//! megawatts:
//!
//! ```text
//!   draw = HOLD_MW + PER_DECADE_MW * widest displacement / 10
//! ```
//!
//! and when the grid is short, the interface goes **dark**: nothing new
//! departs, everything already inside it still lands, and the panel says which
//! region was short and by how much. That is the whole economy of the
//! experiment. Technology in the Mining Valley is not gated by a tree. It is
//! gated by whether somebody a hundred and forty-seven years away is keeping
//! the lights on.

use super::phase::Phase;
use super::region::region;
use crate::json::Json;
use crate::model::{Qty, Tick};
use crate::mp::goal::commas;
use crate::mp::{as_secs, lower::item_title, secs, SIM_TICK_RATE};
use std::collections::{BTreeMap, VecDeque};

/// How often the shipping office does its arithmetic, and the only moments an
/// interface is asked whether it is holding.
///
/// On a lattice for the reason Prototype 3's ledger is: what leaves is a
/// *difference*, and a difference taken whenever a browser happened to poll
/// would batch the same ore into different loads on two clients.
pub const SETTLE: Tick = secs(5);

// ----------------------------------------------------------------- fractures

/// One join between two phases of the same world.
pub struct Fracture {
    pub tag: &'static str,
    /// The earlier region, and the later one. Not an ordering of importance: an
    /// ordering of *dates*, which is what every number here is computed from.
    pub early: &'static str,
    pub late: &'static str,
    /// Whether it was there before anybody arrived.
    pub natural: bool,
    /// Whether the world comes with an interface already standing on it.
    pub standing: bool,
    /// Which region's grid holds it open. Authored rather than derived, because
    /// the answer is *whoever built it* and that is a fact about the world
    /// rather than about the arithmetic.
    pub held_by: &'static str,
    /// One-way running time in seconds for a fleet at speed 100.
    pub leagues: u64,
    pub why: &'static str,
}

pub static FRACTURES: &[Fracture] = &[
    Fracture {
        tag: "deep",
        early: "valley",
        late: "district",
        natural: true,
        standing: false,
        held_by: "district",
        leagues: 40,
        why: "The natural one, and the only reason this world is one world. A \
              hundred and forty-seven years, nobody's doing, and no gantry on it \
              until the district puts one there.",
    },
    Fracture {
        tag: "near",
        early: "district",
        late: "zone",
        natural: false,
        standing: true,
        held_by: "district",
        leagues: 25,
        why: "Thirty-three years and an engineered corridor: somebody in 2037 \
              stabilised this one, which is why it feels like a route rather than \
              a crossing. The district still pays for it every second.",
    },
];

pub fn fracture(tag: &str) -> Option<(usize, &'static Fracture)> {
    FRACTURES.iter().enumerate().find(|(_, f)| f.tag == tag)
}

impl Fracture {
    /// The two phases this joins.
    pub fn phases(&self) -> (Phase, Phase) {
        (
            region(self.early).map(|(_, r)| r.phase).unwrap_or(Phase::P1890),
            region(self.late).map(|(_, r)| r.phase).unwrap_or(Phase::P2037),
        )
    }

    /// The years between its two ends: the floor under every draw on it.
    pub fn gap(&self) -> u32 {
        let (a, b) = self.phases();
        a.displacement(b)
    }

    /// Whether this fracture joins these two regions, in either direction.
    pub fn joins(&self, a: &str, b: &str) -> bool {
        (self.early == a && self.late == b) || (self.early == b && self.late == a)
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tag", self.tag)
            .set("early", self.early)
            .set("late", self.late)
            .set("natural", self.natural)
            .set("standing", self.standing)
            .set("heldBy", self.held_by)
            .set("years", self.gap() as i64)
            .set("leagues", self.leagues as i64)
            .set("holdMW", draw(self.gap()) as i64)
            .set("gauge", gauge(self.gap()) as i64)
            .set("why", self.why)
    }
}

/// What a load has to get through to go from one phase to another.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Crossing {
    /// Same phase. Ordinary logistics, and nothing in this module applies.
    Same,
    /// A fracture, by index into [`FRACTURES`].
    Through(usize),
    /// Two phases with no fracture between them: not a refusal a player can
    /// meet on this map, and the shape the rule has to have anyway.
    None,
}

/// What lies between two regions.
///
/// The general rule rather than this map's answer to it. With one region per
/// phase every lane here crosses something, so `Same` is the floor of the rule
/// rather than a case the map exercises -- which is why `tests/slice.rs` asks
/// it about all nine ordered pairs of phases directly.
pub fn crossing(from: &str, to: &str) -> Crossing {
    let (Some((_, a)), Some((_, b))) = (region(from), region(to)) else { return Crossing::None };
    if a.phase == b.phase {
        return Crossing::Same;
    }
    match FRACTURES.iter().position(|f| f.joins(from, to)) {
        Some(i) => Crossing::Through(i),
        None => Crossing::None,
    }
}

// ------------------------------------------------------------- what it costs

/// Megawatts to hold an interface open at all, before it carries anything.
pub const HOLD_MW: u64 = 40;

/// Megawatts per decade of temporal displacement it is asked to hold.
pub const PER_DECADE_MW: u64 = 4;

/// What an interface draws while it is holding `years` of displacement.
///
/// The two constants are pitched against the plants this repository already
/// has: the deep fracture at 147 years wants 98 MW, which is one compact steam
/// plant and a water-driven generator -- so the district can open it out of
/// what it has locally, and *then* import the coal it actually wants. A slice
/// whose first fracture needed the coal that was on the other side of it would
/// be a slice nobody could start.
pub fn draw(years: u32) -> u64 {
    HOLD_MW + PER_DECADE_MW * years as u64 / 10
}

/// How much an interface will pass in a second, at this displacement.
///
/// Narrower the further it reaches, which is the one thing a fracture should
/// obviously be: a hundred and forty-seven years is a thousand units a second,
/// and a road is not.
pub fn gauge(years: u32) -> u64 {
    BASE_GAUGE * 50 / (50 + years as u64)
}

pub const BASE_GAUGE: u64 = 4_000;

// -------------------------------------------------------------------- fleets

/// What a load is carried in, and the phase that can field one.
///
/// This is section 2 of the brief's "modern logistics" spent rather than
/// asserted. A lane out of the Mining Valley may be worked by carts and by
/// nothing else, so 1890's exports are limited by *haulage* rather than by what
/// is in the ground -- which is the correct shape for the era and is also the
/// thing that makes the district's ore hunger a logistics problem instead of an
/// arithmetic one.
#[derive(Debug)]
pub struct Fleet {
    pub tag: &'static str,
    pub title: &'static str,
    /// The earliest phase that can field one.
    pub from: Phase,
    pub load: Qty,
    pub vehicles: u64,
    pub dwell: u64,
    pub speed: u64,
    pub blurb: &'static str,
}

pub static FLEETS: &[Fleet] = &[
    Fleet {
        tag: "wagon",
        title: "Wagon Road",
        from: Phase::P1890,
        load: 3_000,
        vehicles: 4,
        dwell: 10,
        speed: 60,
        blurb: "Carts, and four of them. About a hundred and fifty a second if \
                nothing goes wrong, which is the whole of 1890's haulage.",
    },
    Fleet {
        tag: "tramway",
        title: "Mineral Tramway",
        from: Phase::P1890,
        load: 8_000,
        vehicles: 3,
        dwell: 15,
        speed: 90,
        blurb: "Narrow gauge, horse-worked at the top and gravity-worked at the                 bottom. Four hundred a second, and it is the most 1890 can do.",
    },
    Fleet {
        tag: "train",
        title: "Train",
        from: Phase::P2037,
        load: 30_000,
        vehicles: 2,
        dwell: 20,
        speed: 150,
        blurb: "Thirty thousand at a time and twice as fast once it is moving. \
                The switchyard is why this exists and 1890 has no switchyard.",
    },
    Fleet {
        tag: "hauler",
        title: "Automated Hauler",
        from: Phase::P2070,
        load: 80_000,
        vehicles: 2,
        dwell: 10,
        speed: 260,
        blurb: "Eighty thousand, loaded in ten seconds, by nobody at all.",
    },
];

/// `a` or `an`, so that a refusal reads like a sentence somebody wrote.
fn article(word: &str) -> &'static str {
    match word.chars().next().map(|c| c.to_ascii_lowercase()) {
        Some('a' | 'e' | 'i' | 'o' | 'u') => "an",
        _ => "a",
    }
}

pub fn fleet(tag: &str) -> Option<&'static Fleet> {
    FLEETS.iter().find(|f| f.tag == tag)
}

impl Fleet {
    /// One round of loading, running and unloading, rounded up to the
    /// settlement lattice so an arrival always lands on a second the whole
    /// slice agrees about.
    pub fn trip(&self, l: &Lane) -> Tick {
        let s = self.dwell + l.leagues() * 100 / self.speed.max(1);
        secs(s.max(1)).div_ceil(SETTLE) * SETTLE
    }

    /// The most this fleet moves in a second on this lane, if the origin keeps
    /// up with it.
    pub fn capacity(&self, l: &Lane) -> u64 {
        let t = as_secs(self.trip(l)).max(1.0);
        (self.load as f64 * self.vehicles as f64 / t) as u64
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tag", self.tag)
            .set("title", self.title)
            .set("from", self.from.tag())
            .set("load", Json::big(self.load as u128))
            .set("vehicles", self.vehicles as i64)
            .set("dwell", self.dwell as i64)
            .set("speed", self.speed as i64)
            .set("blurb", self.blurb)
    }
}

// --------------------------------------------------------------------- lanes

/// One supply relationship the map allows, in one direction.
pub struct Lane {
    pub from: &'static str,
    pub to: &'static str,
    pub item: &'static str,
    pub why: &'static str,
}

pub static LANES: &[Lane] = &[
    // ---- forward through the deep fracture: what 1890 is for
    Lane {
        from: "valley",
        to: "district",
        item: "IronOre",
        why: "The experiment's own success criterion: ore that is a shovel deep \
              in 1890, crushed by machinery that will not exist for a century.",
    },
    Lane {
        from: "valley",
        to: "district",
        item: "Coal",
        why: "The district's seam is forty-five a second. Its plants are not.",
    },
    Lane {
        from: "valley",
        to: "district",
        item: "OrePowder",
        why: "The valley can mill its own, on water. Whether that is worth the \
              carts it takes is the question the region is really asking.",
    },
    // ---- backward through the deep fracture: what makes 1890 possible
    Lane {
        from: "district",
        to: "valley",
        item: "Gear",
        why: "Machinery, going the wrong way down the years. A generator in a \
              crate is what electricity in 1890 is made of.",
    },
    // ---- the near corridor
    Lane {
        from: "district",
        to: "zone",
        item: "IronBillet",
        why: "The caster is in 2037 and the lathes are in 2070.",
    },
    Lane {
        from: "district",
        to: "zone",
        item: "Power",
        why: "There is nothing to burn in 2070 and nothing left to dig.",
    },
    Lane {
        from: "district",
        to: "zone",
        item: "Concentrate",
        why: "What the district made out of somebody else's century.",
    },
    Lane {
        from: "zone",
        to: "district",
        item: "Gear",
        why: "One lathe does what a furnace, a mill and a press did. Whether \
              those gears are worth two crossings to get to 1890 is a decision.",
    },
];

impl Lane {
    pub fn leagues(&self) -> u64 {
        match crossing(self.from, self.to) {
            Crossing::Through(i) => FRACTURES[i].leagues,
            // A same-phase lane is a road, and a road on this plot is short.
            Crossing::Same => 10,
            Crossing::None => 60,
        }
    }

    pub fn fracture(&self) -> Option<(usize, &'static Fracture)> {
        match crossing(self.from, self.to) {
            Crossing::Through(i) => Some((i, &FRACTURES[i])),
            _ => None,
        }
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("from", self.from)
            .set("to", self.to)
            .set("item", self.item)
            .set("itemTitle", item_title(self.item))
            .set("leagues", self.leagues() as i64)
            .set("fracture", self.fracture().map(|(_, f)| Json::Str(f.tag.to_string())))
            .set("why", self.why)
    }
}

pub fn lane(from: &str, to: &str, item: &str) -> Option<(usize, &'static Lane)> {
    LANES.iter().enumerate().find(|(_, l)| l.from == from && l.to == to && l.item == item)
}

// ---------------------------------------------------------------- provenance

/// A quantity of something, and the phase it came out of.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stamped {
    pub origin: Phase,
    pub qty: Qty,
}

/// How far out of its own time a parcel is, arriving here.
pub fn displacement(s: Stamped, into: Phase) -> u32 {
    s.origin.displacement(into)
}

/// Everything a region has been sent and has not yet sent on, per item, oldest
/// first.
///
/// A queue rather than a total, because the answer to "what is this made of"
/// has to survive being partially shipped onward.
#[derive(Clone, Debug, Default)]
pub struct Bin {
    pub held: VecDeque<Stamped>,
    /// Cumulative, by origin, for the panel that wants to say what a region has
    /// *ever* been sent rather than what is left.
    pub landed: BTreeMap<Phase, u64>,
}

impl Bin {
    pub fn put(&mut self, s: Stamped) {
        if s.qty == 0 {
            return;
        }
        *self.landed.entry(s.origin).or_default() += s.qty;
        match self.held.back_mut() {
            Some(last) if last.origin == s.origin => last.qty += s.qty,
            _ => self.held.push_back(s),
        }
    }

    /// Take `qty` out, oldest first, and say what it was.
    pub fn take(&mut self, qty: Qty) -> Vec<Stamped> {
        take_from(&mut self.held, qty)
    }

    pub fn total(&self) -> Qty {
        self.held.iter().map(|s| s.qty).sum()
    }
}

/// Draw `qty` off the front of a queue of parcels, and say what came off.
///
/// The one operation both queues need, so that "oldest first" means the same
/// thing on the way in and on the way out.
fn take_from(q: &mut VecDeque<Stamped>, mut qty: Qty) -> Vec<Stamped> {
    let mut out = Vec::new();
    while qty > 0 {
        let Some(front) = q.front_mut() else { break };
        let n = front.qty.min(qty);
        out.push(Stamped { origin: front.origin, qty: n });
        front.qty -= n;
        qty -= n;
        if front.qty == 0 {
            q.pop_front();
        }
    }
    out
}

/// Merge runs of the same origin, so a list of parcels stays short.
fn fold(v: &mut Vec<Stamped>) {
    let mut out: Vec<Stamped> = Vec::with_capacity(v.len());
    for s in v.drain(..) {
        match out.last_mut() {
            Some(last) if last.origin == s.origin => last.qty += s.qty,
            _ => out.push(s),
        }
    }
    *v = out;
}

// ---------------------------------------------------------------- interfaces

/// A temporal interface, standing on one fracture.
#[derive(Clone, Debug)]
pub struct Interface {
    pub fracture: usize,
    pub opened: Tick,
    /// Whether it was holding at the last settlement, and what it needed.
    pub lit: bool,
    pub want_mw: u64,
    pub had_mw: u64,
    /// The widest displacement it was asked to hold, in years.
    pub worst: u32,
    /// How long it has spent dark, and how much it has passed.
    pub dark: Tick,
    pub carried: u64,
    /// What it refused to take because the gauge was full.
    pub over: u64,
}

impl Interface {
    pub fn new(fracture: usize, at: Tick) -> Interface {
        Interface {
            fracture,
            opened: at,
            lit: false,
            want_mw: draw(FRACTURES[fracture].gap()),
            had_mw: 0,
            worst: FRACTURES[fracture].gap(),
            dark: 0,
            carried: 0,
            over: 0,
        }
    }

    pub fn fracture(&self) -> &'static Fracture {
        &FRACTURES[self.fracture]
    }

    /// Why it is not carrying anything, in the sentence a panel shows.
    pub fn why_dark(&self) -> Option<String> {
        (!self.lit).then(|| {
            let f = self.fracture();
            let held = region(f.held_by).map(|(_, r)| r.title).unwrap_or(f.held_by);
            format!(
                "the {} fracture is dark: it wants {} MW to hold {} years open and {held} is \
                 delivering {}",
                f.tag, self.want_mw, self.worst, self.had_mw
            )
        })
    }

    pub fn to_json(&self) -> Json {
        let f = self.fracture();
        Json::obj()
            .set("fracture", f.tag)
            .set("years", f.gap() as i64)
            .set("heldBy", f.held_by)
            .set("opened", self.opened)
            .set("lit", self.lit)
            .set("wantMW", self.want_mw as i64)
            .set("haveMW", self.had_mw as i64)
            .set("holding", self.worst as i64)
            .set("gauge", gauge(self.worst) as i64)
            .set("darkSeconds", as_secs(self.dark))
            .set("carried", Json::big(self.carried as u128))
            .set("overGauge", Json::big(self.over as u128))
            .set("why", self.why_dark())
    }
}

// -------------------------------------------------------------------- routes

/// A standing supply relationship: this region sends that item to that one, in
/// these vehicles, at up to this rate.
#[derive(Clone, Debug)]
pub struct Route {
    pub id: u32,
    pub lane: usize,
    pub fleet: &'static Fleet,
    pub cap: u64,
    pub opened: Tick,
    /// The origin's cumulative delivery counter as the last settlement read it.
    pub seen: u64,
    /// Waiting to go, and what it is made of.
    pub hold: Vec<Stamped>,
    pub since: Tick,
    pub moved: u64,
    pub trips: u64,
    pub spilled: u64,
    /// Turned away at the interface, because the gauge was full or the fracture
    /// was dark.
    pub held_back: u64,
    pub last_left: Option<Tick>,
    pub last_in: Option<Tick>,
}

impl Route {
    pub fn lane(&self) -> &'static Lane {
        &LANES[self.lane]
    }

    pub fn trip(&self) -> Tick {
        self.fleet.trip(self.lane())
    }

    pub fn waiting(&self) -> Qty {
        self.hold.iter().map(|s| s.qty).sum()
    }

    /// The widest displacement anything waiting on this route represents,
    /// arriving where it is going.
    pub fn worst(&self, into: Phase) -> u32 {
        self.hold.iter().map(|s| displacement(*s, into)).max().unwrap_or(0)
    }

    pub fn to_json(&self, flight: &[Load], now: Tick) -> Json {
        let l = self.lane();
        let out: Vec<&Load> = flight.iter().filter(|f| f.route == self.id).collect();
        Json::obj()
            .set("id", self.id as i64)
            .set("from", l.from)
            .set("to", l.to)
            .set("item", l.item)
            .set("itemTitle", item_title(l.item))
            .set("why", l.why)
            .set("fracture", l.fracture().map(|(_, f)| Json::Str(f.tag.to_string())))
            .set("fleet", self.fleet.tag)
            .set("fleetTitle", self.fleet.title)
            .set("cap", Json::big(self.cap as u128))
            .set("tripSeconds", as_secs(self.trip()))
            .set("waiting", Json::big(self.waiting() as u128))
            .set(
                "madeOf",
                Json::Arr(
                    self.hold
                        .iter()
                        .map(|s| {
                            Json::obj()
                                .set("origin", s.origin.tag())
                                .set("qty", Json::big(s.qty as u128))
                        })
                        .collect(),
                ),
            )
            .set("moved", Json::big(self.moved as u128))
            .set("trips", Json::big(self.trips as u128))
            .set("spilled", Json::big(self.spilled as u128))
            .set("heldBack", Json::big(self.held_back as u128))
            .set("inFlight", out.len() as i64)
            .set(
                "due",
                Json::Arr(
                    out.iter()
                        .map(|f| {
                            Json::obj()
                                .set("qty", Json::big(f.qty() as u128))
                                .set("at", f.at)
                                .set("in", as_secs(f.at.saturating_sub(now)))
                        })
                        .collect(),
                ),
            )
            .set("lastLeft", self.last_left.map(|t| Json::Int(t as i128)))
            .set("lastIn", self.last_in.map(|t| Json::Int(t as i128)))
            .set("rate", self.moved as f64 / as_secs(now.saturating_sub(self.opened)).max(1.0))
    }
}

/// A vehicle between two centuries, and when it lands.
#[derive(Clone, Debug)]
pub struct Load {
    pub route: u32,
    pub from: &'static str,
    pub to: &'static str,
    pub item: &'static str,
    pub at: Tick,
    /// What is on it, and where each part of it came from.
    pub parcels: Vec<Stamped>,
}

impl Load {
    pub fn qty(&self) -> Qty {
        self.parcels.iter().map(|s| s.qty).sum()
    }
}

/// One departure or arrival, for the slice's news feed.
#[derive(Clone, Debug)]
pub struct Move {
    pub at: Tick,
    pub route: u32,
    pub from: &'static str,
    pub to: &'static str,
    pub item: &'static str,
    pub qty: Qty,
    /// The widest displacement on it, in years: zero for ordinary logistics.
    pub years: u32,
    pub arriving: bool,
}

/// Every route, every interface, everything in the air, and the lattice point
/// the arithmetic has reached.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    pub routes: Vec<Route>,
    pub flight: Vec<Load>,
    pub gates: Vec<Interface>,
    /// (region, item) -> what it was *sent* and has not yet accounted for.
    pub bins: BTreeMap<(String, String), Bin>,
    /// (region, item) -> what it has *shipped* at its own depot and not yet put
    /// on a vehicle, oldest first, with each parcel's origin on it.
    ///
    /// The two queues are separate, and the reason is a bug worth remembering.
    /// With one queue a route could lift a parcel out of it before the region's
    /// depot had counted the material -- a route fills up to its own cap, which
    /// has nothing to do with what was produced -- and then the delta that
    /// arrived a moment later found the queue empty and stamped the region's own
    /// phase on material that had come out of another century. Gears made in
    /// 2070 reached 1890 with half of them claiming to be from 2037.
    ///
    /// So: `bins` is what came *in*, and is what provenance is drawn from;
    /// `outs` is what went *out* of a depot, and is what vehicles are loaded
    /// from. Nothing is ever drawn from the queue it was just put into.
    pub outs: BTreeMap<(String, String), VecDeque<Stamped>>,
    /// (region, item) -> how much a route lifted out of that region's production
    /// at the last settlement, per second.
    ///
    /// One window behind, which is deliberate rather than sloppy: it is read by
    /// [`Ledger::power`], which runs *before* dispatch, and the alternative is
    /// an interface whose draw depends on what the same call is about to try to
    /// push through it. A lag of five simulated seconds in an accounting figure
    /// is honest; a circular dependency is not.
    pub drew: BTreeMap<(String, String), u64>,
    pub at: Tick,
    pub next_id: u32,
}

impl Ledger {
    /// The world as it stands before anybody has opened anything: the corridors
    /// somebody else already built.
    pub fn new(at: Tick) -> Ledger {
        let mut l = Ledger::default();
        for (i, f) in FRACTURES.iter().enumerate() {
            if f.standing {
                l.gates.push(Interface::new(i, at));
            }
        }
        l
    }

    pub fn gate(&self, fracture: usize) -> Option<&Interface> {
        self.gates.iter().find(|g| g.fracture == fracture)
    }

    fn gate_mut(&mut self, fracture: usize) -> Option<&mut Interface> {
        self.gates.iter_mut().find(|g| g.fracture == fracture)
    }

    /// Put an interface on a fracture.
    pub fn open_gate(&mut self, tag: &str, at: Tick) -> Result<usize, String> {
        let (i, f) = fracture(tag).ok_or_else(|| format!("there is no `{tag}` fracture"))?;
        if self.gate(i).is_some() {
            return Err(format!("there is already an interface on the {} fracture", f.tag));
        }
        self.gates.push(Interface::new(i, at));
        Ok(i)
    }

    pub fn close_gate(&mut self, tag: &str) -> Result<(), String> {
        let (i, f) = fracture(tag).ok_or_else(|| format!("there is no `{tag}` fracture"))?;
        let k = self
            .gates
            .iter()
            .position(|g| g.fracture == i)
            .ok_or_else(|| format!("there is no interface on the {} fracture", f.tag))?;
        self.gates.remove(k);
        Ok(())
    }

    /// Open a standing supply relationship.
    pub fn open(
        &mut self,
        from: &str,
        to: &str,
        item: &str,
        fleet_tag: &str,
        cap: Option<u64>,
        now: Tick,
    ) -> Result<u32, String> {
        let (idx, l) = lane(from, to, item)
            .ok_or_else(|| format!("nothing carries {} from {from} to {to}", item_title(item)))?;
        let f = fleet(fleet_tag).ok_or_else(|| format!("there is no `{fleet_tag}` fleet"))?;
        let (_, origin) = region(from).ok_or_else(|| format!("there is no region called {from}"))?;
        if origin.phase < f.from {
            return Err(format!(
                "{} cannot field {} {} -- there is none in {} and there will not be until {}",
                origin.title,
                article(f.title),
                f.title.to_lowercase(),
                origin.phase.tag(),
                f.from.tag()
            ));
        }
        if self.routes.iter().any(|r| r.lane == idx && r.fleet.tag == f.tag) {
            return Err(format!(
                "a {} already carries {} from {from} to {to}",
                f.title.to_lowercase(),
                item_title(item)
            ));
        }
        let id = self.next_id + 1;
        self.next_id = id;
        self.routes.push(Route {
            id,
            lane: idx,
            fleet: f,
            cap: cap.unwrap_or_else(|| f.capacity(l)).max(1),
            opened: now,
            // A route opened at minute nine does not get minute one's ore.
            seen: u64::MAX,
            hold: Vec::new(),
            since: now,
            moved: 0,
            trips: 0,
            spilled: 0,
            held_back: 0,
            last_left: None,
            last_in: None,
        });
        Ok(id)
    }

    pub fn close(&mut self, id: u32) -> Result<&'static Lane, String> {
        let k = self.routes.iter().position(|r| r.id == id).ok_or("there is no such route")?;
        let r = self.routes.remove(k);
        // Whatever it was holding goes back where it was standing, with its
        // origins intact. Cancelling a contract does not evaporate the ore.
        let l = r.lane();
        let out = self.outs.entry((l.from.to_string(), l.item.to_string())).or_default();
        for s in r.hold.into_iter().rev() {
            out.push_front(s);
        }
        Ok(l)
    }

    pub fn retune(&mut self, id: u32, cap: u64) -> Result<(), String> {
        let r = self.routes.iter_mut().find(|r| r.id == id).ok_or("there is no such route")?;
        r.cap = cap.max(1);
        Ok(())
    }

    pub fn route(&self, id: u32) -> Option<&Route> {
        self.routes.iter().find(|r| r.id == id)
    }

    /// Everything about to land at or before `t`, taken out of the air and put
    /// in a fixed order, so two replicas unload in the same order.
    pub fn arrivals(&mut self, t: Tick) -> Vec<Load> {
        let mut out = Vec::new();
        let mut kept = Vec::with_capacity(self.flight.len());
        for f in std::mem::take(&mut self.flight) {
            if f.at <= t {
                out.push(f);
            } else {
                kept.push(f);
            }
        }
        self.flight = kept;
        out.sort_by_key(|l| (l.at, l.route, l.qty()));
        out
    }

    /// Credit a route with what actually made it into the yard, and remember
    /// where it came from.
    pub fn landed(&mut self, id: u32, took: &[Stamped], spilled: Qty, at: Tick) {
        let Some(r) = self.routes.iter_mut().find(|r| r.id == id) else { return };
        r.moved += took.iter().map(|s| s.qty).sum::<u64>();
        r.spilled += spilled;
        r.last_in = Some(at);
        let l = r.lane();
        let key = (l.to.to_string(), l.item.to_string());
        let bin = self.bins.entry(key).or_default();
        for s in took {
            bin.put(*s);
        }
    }

    /// What a region is holding of one item, and what it is made of.
    pub fn bin(&self, region: &str, item: &str) -> Option<&Bin> {
        self.bins.get(&(region.to_string(), item.to_string()))
    }

    /// How much of what is standing in a region came from a later phase than
    /// the region itself: the machinery a valley has been sent.
    pub fn from_later(&self, tag: &str, item: &str, here: Phase) -> u64 {
        self.bin(tag, item)
            .map(|b| b.landed.iter().filter(|(p, _)| **p > here).map(|(_, n)| *n).sum())
            .unwrap_or(0)
    }

    // ------------------------------------------------------------- the beat

    /// Ask every interface whether it is holding, at one lattice point.
    ///
    /// `grid(region)` is the megawatts that region delivered to its own grid
    /// connection over the last window, *less* whatever it is shipping to
    /// another century -- because power sent down a corridor to 2070 is not
    /// power holding a fracture open, and a model that let one megawatt do both
    /// jobs would be quietly giving the answer away.
    ///
    /// The draw is computed *before* anything is dispatched, from what is
    /// already queued and already in the air, so that it is a function of the
    /// clock rather than of the order two calls happened to be made in.
    pub fn power(&mut self, t: Tick, grid: impl Fn(&str) -> u64) {
        for k in 0..self.gates.len() {
            let i = self.gates[k].fracture;
            let f = &FRACTURES[i];
            // The widest displacement this fracture is being asked to hold:
            // its own gap is the floor, and anything queued or flying over it
            // that is further out of its time than that raises it.
            let mut worst = f.gap();
            for r in &self.routes {
                let l = r.lane();
                if l.fracture().map(|(j, _)| j) != Some(i) {
                    continue;
                }
                let Some((_, to)) = region(l.to) else { continue };
                worst = worst.max(r.worst(to.phase));
                for load in self.flight.iter().filter(|x| x.route == r.id) {
                    for s in &load.parcels {
                        worst = worst.max(displacement(*s, to.phase));
                    }
                }
            }
            let want = draw(worst);
            let had = grid(f.held_by);
            let g = &mut self.gates[k];
            g.worst = worst;
            g.want_mw = want;
            g.had_mw = had;
            g.lit = had >= want;
            if !g.lit {
                g.dark += SETTLE;
            }
            let _ = t;
        }
    }

    /// Decide what leaves, at one lattice point.
    ///
    /// `shipped(region, item)` is the origin's cumulative delivery counter --
    /// the same number its own objective is scored on, because shipping the
    /// thing you were asked for *is* how it reaches the next region.
    pub fn dispatch(&mut self, t: Tick, shipped: impl Fn(&str, &str) -> u64) -> Vec<Move> {
        let mut news = Vec::new();
        self.drew.clear();
        let mut order: Vec<usize> = (0..self.routes.len()).collect();
        order.sort_by_key(|&i| self.routes[i].id);

        // ---- 1. what each origin has produced since the last settlement, once
        // per (origin, item) however many routes are watching it.
        let mut pool: BTreeMap<(&'static str, &'static str), u64> = BTreeMap::new();
        for &i in &order {
            let l = self.routes[i].lane();
            let now = shipped(l.from, l.item);
            let seen = &mut self.routes[i].seen;
            if *seen == u64::MAX {
                *seen = now;
                continue;
            }
            let delta = now.saturating_sub(*seen);
            *seen = now;
            pool.entry((l.from, l.item)).or_insert(delta);
        }

        // ---- 2. everything a region shipped since the last settlement is
        // stamped here, and here only: drawn FIFO out of what that region was
        // *sent*, with the remainder stamped with the region's own phase. It
        // happens for all of a region's production whether or not a route is
        // waiting for any of it, which is what keeps the answer a function of
        // the clock rather than of who asked.
        //
        // This is the whole of the provenance mechanic, and the reason nothing
        // in it mentions processing: ore carried through 2037 untouched comes
        // off the front of the ore queue and keeps its 1890 stamp, while
        // concentrate *made* in 2037 finds an empty concentrate queue -- nobody
        // ever sent the district any concentrate -- and is stamped 2037.
        // Carrying is not making, and one subtraction is what says so.
        for ((tag, item), delta) in &pool {
            if *delta == 0 {
                continue;
            }
            let Some((_, r)) = region(tag) else { continue };
            let key = (tag.to_string(), item.to_string());
            let bin = self.bins.entry(key.clone()).or_default();
            let mut parcels = bin.take(*delta);
            let carried: u64 = parcels.iter().map(|s| s.qty).sum();
            if carried < *delta {
                parcels.push(Stamped { origin: r.phase, qty: delta - carried });
            }
            fold(&mut parcels);
            let out = self.outs.entry(key).or_default();
            for s in parcels {
                match out.back_mut() {
                    Some(last) if last.origin == s.origin => last.qty += s.qty,
                    _ => out.push_back(s),
                }
            }
        }

        // ---- 3. fill the routes out of their origins' bins, oldest first.
        for &i in &order {
            let (from, item, trip, id) = {
                let r = &self.routes[i];
                let l = r.lane();
                (l.from, l.item, r.trip(), r.id)
            };
            let room = {
                let r = &self.routes[i];
                let want = r.cap.saturating_mul(SETTLE / SIM_TICK_RATE);
                want.min(r.fleet.load.saturating_sub(r.waiting()))
            };
            if room > 0 {
                let out = self.outs.entry((from.to_string(), item.to_string())).or_default();
                let got = take_from(out, room);
                // What a route actually lifted out of a region's production,
                // which is the number `power` needs and is emphatically not the
                // size of the pool it lifted it from. Getting those two
                // confused made every megawatt the district delivered look like
                // a megawatt it had exported, and both fractures went dark the
                // moment anybody opened a power run.
                let took: u64 = got.iter().map(|s| s.qty).sum();
                if took > 0 {
                    *self
                        .drew
                        .entry((from.to_string(), item.to_string()))
                        .or_default() += took / (SETTLE / SIM_TICK_RATE).max(1);
                }
                let r = &mut self.routes[i];
                r.hold.extend(got);
                fold(&mut r.hold);
            }

            // ---- 4. and let a vehicle go, if the fracture will have it.
            let r = &self.routes[i];
            let l = r.lane();
            let Some((_, to)) = region(l.to) else { continue };
            let out = self.flight.iter().filter(|f| f.route == id).count() as u64;
            let waiting = r.waiting();
            let full = waiting >= r.fleet.load;
            let waited = t.saturating_sub(r.since) >= trip;
            if out >= r.fleet.vehicles || waiting == 0 || !(full || waited) {
                continue;
            }

            // The interface's word. A dark fracture passes nothing, a narrow
            // one passes its gauge, and either way what is turned away stays
            // exactly where it is and counts as turned away.
            let allowed = match l.fracture() {
                None => waiting,
                Some((j, f)) => match self.gate(j) {
                    None => {
                        let r = &mut self.routes[i];
                        r.held_back += waiting;
                        r.since = t;
                        continue;
                    }
                    Some(g) if !g.lit => {
                        let r = &mut self.routes[i];
                        r.held_back += waiting;
                        r.since = t;
                        continue;
                    }
                    Some(g) => {
                        let _ = f;
                        gauge(g.worst).saturating_mul(SETTLE / SIM_TICK_RATE).min(waiting)
                    }
                },
            };
            if allowed == 0 {
                let r = &mut self.routes[i];
                r.held_back += waiting;
                continue;
            }

            let take = allowed.min(self.routes[i].fleet.load);
            let mut parcels = Vec::new();
            {
                let r = &mut self.routes[i];
                let mut left = take;
                let mut kept: Vec<Stamped> = Vec::new();
                for s in std::mem::take(&mut r.hold) {
                    if left == 0 {
                        kept.push(s);
                        continue;
                    }
                    let n = s.qty.min(left);
                    parcels.push(Stamped { origin: s.origin, qty: n });
                    left -= n;
                    if s.qty > n {
                        kept.push(Stamped { origin: s.origin, qty: s.qty - n });
                    }
                }
                r.hold = kept;
                r.held_back += waiting - take;
                r.since = t;
                r.trips += 1;
                r.last_left = Some(t);
            }
            fold(&mut parcels);
            let years = parcels.iter().map(|s| displacement(*s, to.phase)).max().unwrap_or(0);
            let qty = parcels.iter().map(|s| s.qty).sum();
            if let Some((j, _)) = l.fracture() {
                if let Some(g) = self.gate_mut(j) {
                    g.carried += qty;
                    g.over += waiting - take;
                }
            }
            self.flight.push(Load {
                route: id,
                from: l.from,
                to: l.to,
                item: l.item,
                at: t + trip,
                parcels,
            });
            news.push(Move {
                at: t,
                route: id,
                from: l.from,
                to: l.to,
                item: l.item,
                qty,
                years,
                arriving: false,
            });
        }
        news
    }

    pub fn to_json(&self, now: Tick) -> Json {
        Json::obj()
            .set("at", self.at)
            .set(
                "routes",
                Json::Arr(self.routes.iter().map(|r| r.to_json(&self.flight, now)).collect()),
            )
            .set("inFlight", self.flight.len() as i64)
            .set("fractures", Json::Arr(FRACTURES.iter().map(|f| f.to_json()).collect()))
            .set("interfaces", Json::Arr(self.gates.iter().map(|g| g.to_json()).collect()))
            .set(
                "lanes",
                Json::Arr(
                    LANES
                        .iter()
                        .map(|l| {
                            l.to_json().set(
                                "open",
                                self.routes.iter().any(|r| {
                                    let rl = r.lane();
                                    rl.from == l.from && rl.to == l.to && rl.item == l.item
                                }),
                            )
                        })
                        .collect(),
                ),
            )
            .set("fleets", Json::Arr(FLEETS.iter().map(Fleet::to_json).collect()))
            .set(
                "provenance",
                Json::Arr(
                    self.bins
                        .iter()
                        .filter(|(_, b)| !b.landed.is_empty())
                        .map(|((tag, item), b)| {
                            Json::obj()
                                .set("region", tag.clone())
                                .set("item", item.clone())
                                .set("itemTitle", item_title(item))
                                .set("holding", Json::big(b.total() as u128))
                                .set(
                                    "waitingToLeave",
                                    Json::big(
                                        self.outs
                                            .get(&(tag.clone(), item.clone()))
                                            .map(|q| q.iter().map(|s| s.qty).sum::<u64>())
                                            .unwrap_or(0) as u128,
                                    ),
                                )
                                .set(
                                    "sentFrom",
                                    Json::Arr(
                                        b.landed
                                            .iter()
                                            .map(|(p, n)| {
                                                Json::obj()
                                                    .set("phase", p.tag())
                                                    .set("qty", Json::big(*n as u128))
                                            })
                                            .collect(),
                                    ),
                                )
                        })
                        .collect(),
                ),
            )
    }
}

/// A sentence about one departure or arrival, for the news feed.
///
/// It names the displacement when there is one, because "thirty thousand ore
/// reached the district" and "thirty thousand ore reached the district, out of
/// 1890" are different sentences and only the second one is this experiment.
pub fn moved_words(m: &Move) -> String {
    let (a, b) = (name(m.from), name(m.to));
    let years = if m.years == 0 {
        String::new()
    } else {
        format!(", {} years out of its time", m.years)
    };
    if m.arriving {
        format!("{} {} reached {b}{years}", commas(m.qty), item_title(m.item))
    } else {
        format!("{} {} left {a} for {b}{years}", commas(m.qty), item_title(m.item))
    }
}

fn name(tag: &str) -> &str {
    match region(tag) {
        Some((_, r)) => r.title,
        None => "somewhere else",
    }
}

/// The lattice point at or before `t`.
pub fn settled(t: Tick) -> Tick {
    t / SETTLE * SETTLE
}
