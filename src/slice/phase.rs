//! Three industrial phases of one world, and what each of them can actually
//! make.
//!
//! The whole experiment turns on this file refusing to be a tech tree. A tech
//! tree says *you have not researched motors*; this says *there is no grid in
//! 1890 to put one on, and nobody within a hundred and forty-seven years can
//! roll the steel its frame is made of* -- and then, crucially, lets you carry
//! one back anyway if you can get it there and support it.
//!
//! ```text
//!   1890  Mining Valley          timber, cast iron, a river, and no grid
//!   2037  Industrial District    a grid, steel, and an ore body somebody
//!                                already emptied
//!   2070  Manufacturing Zone     the same grid, and two processes nobody
//!                                else has
//! ```
//!
//! # Where the era numbers come from
//!
//! Almost nowhere in this module, which is the point. Experiment 14 put an
//! [`Era`] on every component and a [`Mat`] on every frame, and those two
//! fields already encode most of what separates a century from the next one: a
//! motor is `Era::Electric` because it needs a grid, and a crusher is offered on
//! steel *or* cast iron because that choice changes what it will shake apart. So
//! a phase is mostly a *date* laid over a table that already existed.
//!
//! What this file adds is four short statements per phase and one table of
//! eight rows:
//!
//! ```text
//!   year      when it is
//!   grid      whether there is a national supply to connect to
//!   mats      what its foundries and rolling mills can turn out
//!   ARRIVES   the eight components that did not exist yet
//! ```
//!
//! Everything else -- which machines are buildable, which designs are legal,
//! what a crate of imported plant costs -- is derived from those, because a
//! second hand-written list is a second thing to get wrong.
//!
//! # The eight rows, and why they are only eight
//!
//! Twenty-nine of the thirty-seven components in the catalogue are available in
//! all three phases, and that ratio is the claim rather than an oversight. A
//! hopper is a hopper. A crusher is a crusher -- experiment 14 was emphatic
//! about there being exactly one of those in this crate, and inventing a
//! Crusher Mk2 for 2037 would have thrown away the argument both experiments
//! are making. What differs between the phases is *how power is made and
//! carried*, which is six rows, and two processes at the far end, which is two
//! more.
//!
//! The one row that is not about a component at all is `mains`. A grid
//! connection is `Era::Any` in the catalogue because a wire is a wire, and it
//! is dated here because what it connects to is a *century* rather than a
//! component. That is the distinction this module exists to make: the valley is
//! not forbidden electricity, it simply has nothing to plug into, and the
//! answer is [`designs/24-hydro.machine`] -- two imported generators, four
//! local wheels, and 126 MW that belong to 1890 because the river does.
//!
//! [`Era`]: crate::machine::era::Era
//! [`Mat`]: crate::machine::era::Mat
//! [`designs/24-hydro.machine`]: crate::slice

use crate::json::Json;
use crate::machine::design::Design;
use crate::machine::era::{Era, Mat, MATS};
use crate::machine::parts::{self, Kind};

// ------------------------------------------------------------------- phases

/// One industrial phase of the world.
///
/// Ordered by date, and the order is load bearing: `<` means *earlier*, which
/// is what every availability question in this module asks.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Phase {
    P1890,
    P2037,
    P2070,
}

pub const PHASES: [Phase; 3] = [Phase::P1890, Phase::P2037, Phase::P2070];

impl Phase {
    pub fn year(self) -> u32 {
        match self {
            Phase::P1890 => 1890,
            Phase::P2037 => 2037,
            Phase::P2070 => 2070,
        }
    }

    /// The year, as the tag everything outside this module names a phase by.
    pub fn tag(self) -> &'static str {
        match self {
            Phase::P1890 => "1890",
            Phase::P2037 => "2037",
            Phase::P2070 => "2070",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Phase::P1890 => "1890",
            Phase::P2037 => "2037",
            Phase::P2070 => "2070",
        }
    }

    /// Which of experiment 14's technology families a plant built here falls
    /// into if nobody fights it.
    ///
    /// *Tends toward*, not *is restricted to*. A 2037 district may build a
    /// water wheel -- the river is still in the same valley -- and a valley
    /// with enough imported machinery may run motors. This is where the
    /// gravity points.
    pub fn era(self) -> Era {
        match self {
            Phase::P1890 => Era::Water,
            Phase::P2037 => Era::Electric,
            Phase::P2070 => Era::Electric,
        }
    }

    pub fn blurb(self) -> &'static str {
        match self {
            Phase::P1890 => {
                "a river, a forest, cast iron, and ore near enough the surface to \
                 shovel -- and no grid, no steel, and no electrical component of \
                 any kind"
            }
            Phase::P2037 => {
                "a national grid, steel, rolling stock, and an ore body that four \
                 generations have already been through"
            }
            Phase::P2070 => {
                "the same grid and two processes nobody else has, standing on \
                 ground with nothing left under it at all"
            }
        }
    }

    /// The sentence a player should be able to hold in their head about what
    /// this phase is *for*.
    pub fn role(self) -> &'static str {
        match self {
            Phase::P1890 => "where the material is",
            Phase::P2037 => "where the material is worth something",
            Phase::P2070 => "where one machine does what three did",
        }
    }

    /// Whether there is a national supply to connect a `mains` to.
    pub fn grid(self) -> bool {
        match self {
            Phase::P1890 => false,
            Phase::P2037 | Phase::P2070 => true,
        }
    }

    /// What this phase's foundries and forges can turn out.
    ///
    /// Timber and cast iron are 1890's whole material vocabulary, and that one
    /// line is what makes the valley's machines a different shape: a crusher
    /// and a mill are offered on steel or cast iron in experiment 14's table,
    /// so in 1890 they go on cast iron and cast iron rates 7 against a crusher
    /// that shakes at exactly 7. `designs/23-watermill.machine` says
    /// `frame iron` three times for that reason and not for flavour.
    pub fn mats(self) -> &'static [Mat] {
        match self {
            Phase::P1890 => &[Mat::Wood, Mat::CastIron],
            Phase::P2037 | Phase::P2070 => &MATS,
        }
    }

    pub fn index(self) -> usize {
        match self {
            Phase::P1890 => 0,
            Phase::P2037 => 1,
            Phase::P2070 => 2,
        }
    }

    pub fn by_tag(tag: &str) -> Option<Phase> {
        PHASES.into_iter().find(|p| p.tag() == tag)
    }

    /// How many fractures lie between two phases.
    pub fn steps_to(self, other: Phase) -> u32 {
        self.index().abs_diff(other.index()) as u32
    }

    /// How far apart in years two phases are, which is the number every
    /// temporal interface in [`super::gate`] is priced on.
    pub fn displacement(self, other: Phase) -> u32 {
        self.year().abs_diff(other.year())
    }

    /// Whether this phase can build one of these at all.
    pub fn builds(self, kind: Kind) -> bool {
        match arrival(kind.tag()) {
            Some(a) => self >= a.at,
            None => true,
        }
    }

    /// Whether this phase can build one of these *on this frame*.
    pub fn builds_on(self, kind: Kind, m: Mat) -> bool {
        self.builds(kind) && frame_ok(kind, m, self)
    }

    /// Every component this phase can make, in catalogue order.
    pub fn holds(self) -> Vec<&'static str> {
        parts::KINDS.iter().filter(|k| self.builds(**k)).map(|k| k.tag()).collect()
    }

    /// Every component it cannot, which is the more interesting list and
    /// always the shorter one.
    pub fn lacks(self) -> Vec<&'static str> {
        parts::KINDS.iter().filter(|k| !self.builds(**k)).map(|k| k.tag()).collect()
    }
}

impl std::fmt::Display for Phase {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.tag())
    }
}

// ------------------------------------------------------------- what arrives

/// Whether this phase can make the frame a component is standing on.
///
/// Only asked where the frame is a *decision*. Experiment 14 offers a material
/// choice on the components where choosing changes something -- a crusher may be
/// steel or cast iron, a wheel timber or cast iron -- and writes `ONLY_STEEL` on
/// the rest. In that table `ONLY_STEEL` means *this component has no frame
/// decision*; it does not mean *this component is a steel artefact*.
///
/// Reading it the second way, which is the first thing this module did, made
/// 1890 unable to build a hopper, a pump, an inlet or an outlet -- so an
/// extraction head cost nine thousand six hundred gears of imported machinery
/// and the Mining Valley could not dig its own ore. That is not a claim about
/// 1890, it is a misreading of a default, and the fix is to ask the question
/// only where the table was answering it.
///
/// What survives is the mechanic that was wanted: the seven components offered
/// on `STEEL_IRON` default to steel, so a 1890 design that uses one has to say
/// `frame iron` -- which costs vibration tolerance, which is exactly the trade
/// experiment 14 built.
pub fn frame_ok(kind: Kind, m: Mat, p: Phase) -> bool {
    parts::phys(kind).mats.len() < 2 || p.mats().contains(&m)
}

/// A component that did not exist yet, and the phase it turns up in.
pub struct Arrival {
    /// The component's tag in [`crate::machine::parts`].
    pub part: &'static str,
    pub at: Phase,
    /// Whether a crate of one, carried backwards, would be any use.
    ///
    /// True for every row but one. A motor in 1890 is a motor: bolt it down,
    /// give it a supply, and it turns. A *grid connection* in 1890 is a wire
    /// with nothing on the far end of it, and there is no quantity of gears
    /// that fixes that -- which is the one place in this experiment where the
    /// answer really is no, and it is a fact about the century rather than
    /// about the component.
    pub crated: bool,
    /// Why it is not earlier, in the sentence a refusal is written from.
    pub why: &'static str,
}

/// The eight components that are not as old as the catalogue.
///
/// Six of them are one fact -- electricity needs a grid and a grid needs a
/// century -- and two are the far end of the process catalogue. One of the six,
/// `mains`, is not a component problem at all: it is a wire with nothing on the
/// other end of it.
pub static ARRIVES: &[Arrival] = &[
    // ---- 2037: the grid, and everything that only makes sense beside one
    Arrival {
        part: "mains",
        at: Phase::P2037,
        crated: false,
        why: "there is no grid in the valley to connect it to",
    },
    Arrival {
        part: "motor",
        at: Phase::P2037,
        crated: true,
        why: "an electric motor wants a supply, and a steel frame to sit in",
    },
    Arrival {
        part: "generator",
        at: Phase::P2037,
        crated: true,
        why: "a generator is steel, precision and a century and a half of winding",
    },
    Arrival {
        part: "turbine",
        at: Phase::P2037,
        crated: true,
        why: "a steam turbine is a steel blading problem the valley cannot solve",
    },
    Arrival {
        part: "heater",
        at: Phase::P2037,
        crated: true,
        why: "heating by electricity presupposes electricity",
    },
    Arrival {
        part: "fan",
        at: Phase::P2037,
        crated: true,
        why: "a forced-air cooler is a motor with blades on it",
    },
    // ---- 2070: the two processes at the far end of the catalogue
    //
    // Deliberately only two, and deliberately these two: they are the last two
    // things Prototype 3's campaign hands over, which is this repository's own
    // ordering of its process catalogue rather than a fresh opinion.
    Arrival {
        part: "lathe",
        at: Phase::P2070,
        crated: true,
        why: "billet straight to gears in one machine is eighty years of control \
              systems, not a bigger press",
    },
    Arrival {
        part: "column",
        at: Phase::P2070,
        crated: true,
        why: "a staged column is the most advanced process in the catalogue, and \
              this is the phase that has it",
    },
];

pub fn arrival(part: &str) -> Option<&'static Arrival> {
    ARRIVES.iter().find(|a| a.part == part)
}

/// The earliest phase that can build one of these on this frame, if any can.
///
/// The *earliest*, which for a component being carried backwards is also the
/// nearest: a motor wanted in 1890 comes out of 2037 rather than 2070, one
/// fracture instead of two, and the price follows.
pub fn made_in(kind: Kind, m: Mat) -> Option<Phase> {
    PHASES.into_iter().find(|p| p.builds_on(kind, m))
}

// -------------------------------------------------------------- the crating

/// Gears per tile of footprint, per fracture crossed.
///
/// The one number in this module that was chosen rather than derived, and it is
/// chosen against the gear lines this repository already has: a machining cell
/// makes twenty-four gears a second, so a four-tile motor is about seventy
/// seconds of somebody else's factory. Expensive enough to be a project,
/// cheap enough to be a *decision*.
pub const CRATE_PER_TILE: u64 = 400;

/// What one of these costs to have delivered into this phase, in gears.
///
/// Zero when the phase can make it. Otherwise the footprint, the crating rate,
/// and the number of fractures it has to come back through -- so a lathe in
/// 1890 is twice the price of a lathe in 2037, because it is twice as far from
/// anywhere that makes one.
pub fn import_cost(kind: Kind, m: Mat, into: Phase) -> u64 {
    if into.builds_on(kind, m) || !crateable(kind) {
        return 0;
    }
    let Some(from) = made_in(kind, m) else { return 0 };
    let p = parts::part(kind);
    (p.w * p.h) as u64 * CRATE_PER_TILE * from.steps_to(into).max(1) as u64
}

/// What a whole design costs to stand up in this phase, in gears.
///
/// A sum over its units, and therefore a pure function of the document: the
/// same design costs the same wherever it is opened, and a region's machinery
/// ledger can be recomputed from its world at any tick instead of being
/// accounted for as it goes. See [`super::run::Slice::machinery`].
pub fn design_cost(d: &Design, into: Phase) -> u64 {
    d.units.iter().map(|u| import_cost(u.kind, u.tune.mat, into)).sum()
}

/// Every unit of a design that had to be crated, with what each one cost.
pub fn imported(d: &Design, into: Phase) -> Vec<(String, Kind, u64)> {
    d.units
        .iter()
        .filter_map(|u| {
            let c = import_cost(u.kind, u.tune.mat, into);
            (c > 0).then(|| (u.name.clone(), u.kind, c))
        })
        .collect()
}

// ------------------------------------------------------------- the refusals

/// Why this phase cannot build this component, when it cannot -- and what to do
/// about it.
///
/// Never a flat no. Every sentence this returns names the phase the component
/// comes out of and the fact that carrying it back is allowed, because the
/// thing this experiment is testing is precisely that the wall is logistical
/// rather than magical.
pub fn refuse(kind: Kind, m: Mat, into: Phase) -> Option<String> {
    if into.builds_on(kind, m) {
        return None;
    }
    let what = kind.title();
    let cost = import_cost(kind, m, into);
    let from = made_in(kind, m);
    let because = if !into.builds(kind) {
        arrival(kind.tag()).map(|a| a.why.to_string()).unwrap_or_default()
    } else {
        format!(
            "{} cannot make {} -- it works in {}, and this one is offered on {}",
            into.tag(),
            m.title().to_lowercase(),
            into.mats().iter().map(|m| m.tag()).collect::<Vec<_>>().join(" and "),
            parts::phys(kind).mats.iter().map(|m| m.tag()).collect::<Vec<_>>().join(" or ")
        )
    };
    if !crateable(kind) {
        return Some(format!(
            "{what} in {}: {because}. A crate of one would be a wire with nothing on the \
             end of it -- make your own electricity instead",
            into.tag()
        ));
    }
    Some(match from {
        Some(f) => format!(
            "{what} in {}: {because}. One comes out of {} for {cost} imported gears, \
             landed across the fracture",
            into.tag(),
            f.tag()
        ),
        None => format!("{what} in {}: {because}, and no phase here makes one", into.tag()),
    })
}

/// Whether a crate of this component, carried backwards, is any use at all.
pub fn crateable(kind: Kind) -> bool {
    arrival(kind.tag()).map(|a| a.crated).unwrap_or(true)
}

/// Whether this design could be stood up in this phase *at any price*.
///
/// The difference between this and [`native`] is the whole shape of the
/// experiment. `native` asks whether it is free; this asks whether it is
/// possible. Exactly one kind of component fails here -- a grid connection in a
/// century with no grid -- and everything else that is not native is merely
/// expensive.
pub fn legal(d: &Design, into: Phase) -> Result<(), String> {
    for u in &d.units {
        if !into.builds(u.kind) && !crateable(u.kind) {
            return Err(format!(
                "{}: {}",
                u.name,
                refuse(u.kind, u.tune.mat, into).unwrap_or_default()
            ));
        }
    }
    Ok(())
}

/// Whether a phase may stand this design up with no imported machinery at all.
///
/// The question a palette asks, and the question the word *native* means
/// everywhere in this experiment.
pub fn native(d: &Design, into: Phase) -> Result<(), String> {
    for u in &d.units {
        if let Some(why) = refuse(u.kind, u.tune.mat, into) {
            return Err(format!("{}: {why}", u.name));
        }
    }
    Ok(())
}

// ------------------------------------------------------------- on the wire

impl Phase {
    pub fn to_json(self) -> Json {
        Json::obj()
            .set("tag", self.tag())
            .set("year", self.year() as i64)
            .set("era", self.era().tag())
            .set("eraTitle", self.era().title())
            .set("role", self.role())
            .set("blurb", self.blurb())
            .set("grid", self.grid())
            .set(
                "materials",
                Json::arr(self.mats().iter().map(|m| m.tag().to_string()).collect::<Vec<_>>()),
            )
            .set("holds", self.holds().len() as i64)
            .set(
                "lacks",
                Json::Arr(
                    self.lacks()
                        .iter()
                        .map(|t| {
                            let a = arrival(t);
                            Json::obj()
                                .set("part", *t)
                                .set("arrives", a.map(|a| Json::Str(a.at.tag().to_string())))
                                .set("why", a.map(|a| Json::Str(a.why.to_string())))
                        })
                        .collect(),
                ),
            )
    }
}

pub fn phases() -> Json {
    Json::Arr(PHASES.iter().map(|p| p.to_json()).collect())
}
