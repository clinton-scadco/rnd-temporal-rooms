//! Experiment 14: what a component is made of, how hot it gets, and how hard
//! it shakes.
//!
//! Experiments 06 to 13 built machines out of components that were pure
//! transformations. A crusher took ore and rotary and made smaller ore, and it
//! would do that forever, at the same rate, in a cupboard, made of anything.
//! That was the right simplification for the question those experiments were
//! asking -- *is assembling machines from functional components fun* -- and it
//! is the wrong one for the question this experiment asks:
//!
//! > Does machine design get more interesting when components have material,
//! > energy and thermal properties -- and is old machinery **mechanically**
//! > different, rather than cosmetically antique?
//!
//! The failure this is written against is the one every factory game reaches:
//!
//! ```text
//!   Wood Crusher Mk1
//!   Steel Crusher Mk2   +20%
//! ```
//!
//! which is not a technology tree, it is a multiplier with a costume. So there
//! is exactly *one* crusher in this crate, and there always was. What changes
//! between eras is not the crusher. It is everything around it:
//!
//! ```text
//!   water      water wheel -> line shaft -> belt -> pulley -> crusher
//!   steam      boiler -> steam engine -> gearbox -> crusher
//!   electric   mains -> motor -> coupling -> crusher
//! ```
//!
//! Three power systems, three drive trains, three shapes on the ground, and --
//! this is the part that had to be simulated rather than asserted -- three
//! different ways to fail:
//!
//! ```text
//!   water      runs out of torque, and shakes its own frame apart
//!   steam      cooks itself unless the waste heat has somewhere to go
//!   electric   stops the moment the grid does, and is billed for every unit
//! ```
//!
//! # Three properties, and why these three
//!
//! The note behind this experiment lists ten: energy domain, power, torque,
//! speed, heat generation, thermal mass, optimal range, maximum temperature,
//! structural material and vibration tolerance. Four of them were already here
//! -- a port *is* an energy domain, a rate *is* a power demand, `Qual::speed`
//! is the speed band, and a `Need::MinSpeed` is what torque feels like from the
//! other end. This module adds the other six, and they collapse into three
//! mechanics:
//!
//! ```text
//!   material     what it is made of: how well it conducts, tolerates and lasts
//!   temperature  heat in, heat out, and a body that is somewhere in between
//!   vibration    what it shakes, and what has to put up with being shaken
//! ```
//!
//! # Temperature is exact and behaviour is not
//!
//! The body temperature of a component is an integer -- degrees above ambient,
//! held exactly, moved by whole units of heat every tick. Its *behaviour* moves
//! only at thresholds:
//!
//! ```text
//!   COLD          60%     below its operating range
//!   NORMAL       100%
//!   WARM          95%
//!   HOT           75%
//!   OVERHEATED     0%     and it will not restart until it is NORMAL again
//! ```
//!
//! That split is the whole design. A continuous derating curve would give the
//! player a number that always moves a little and never means anything; a band
//! gives them a thing that is true or false, a warning before it is false, and
//! a sentence to read when it is. It also keeps the state space finite, which
//! is not a detail: `orbit` compiles a design by watching for its state to
//! repeat, and it can only do that because a body temperature is one of a few
//! hundred integers rather than one of infinitely many floats.
//!
//! # Cooling is a design decision, not a statistic
//!
//! A component sheds heat to the air at a rate set by what it is made of and
//! how much room it has been given. That is the free option and it is usually
//! not enough. The rest are things the player builds:
//!
//! ```text
//!   spacing       clear tiles around the footprint, up to a doubling
//!   material      steel conducts nearly three times as well as timber
//!   radiator      the waste port, wired to the sky
//!   fan           the same, four times faster, and it wants power to do it
//!   water jacket  the same again, and the heat comes out the other side
//!   exchanger     ... as steam, if the body was hot enough to make any
//! ```
//!
//! The last two are the point. Heat leaving a `waste` port carries a *grade* --
//! the body temperature it came off at, mapped onto the same scale everything
//! else in this crate measures temperature on -- so a steam engine running at
//! 140 degrees has waste heat a jacket will take, and a motor settling at 30 has
//! nothing worth plumbing. Reusing waste heat is therefore not a flat bonus. It
//! is available to one era and not to another, because of how hot that era's
//! machinery runs.
//!
//! It is also deliberately capped one band below boiling. An engine whose own
//! waste heat could raise the steam that drives it is not a clever design, it is
//! a bug with a diagram, and `WASTE_GRADE_MAX` is where that is refused.

use super::stuff::TEMP_MAX;
use crate::json::Json;
use std::fmt;

// -------------------------------------------------------------------- eras

/// Which technology family a component belongs to.
///
/// `Any` is not a fourth era, it is the absence of one: a hopper is a hopper,
/// and pretending that a Mk2 hopper exists is the mistake this experiment is
/// written against. Roughly two thirds of the catalogue is `Any`, and that
/// ratio is the claim -- the eras differ in how power is made and carried, not
/// in what a bin is.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Era {
    Any,
    Water,
    Steam,
    Electric,
}

pub const ERAS: [Era; 3] = [Era::Water, Era::Steam, Era::Electric];

impl Era {
    pub fn tag(self) -> &'static str {
        match self {
            Era::Any => "any",
            Era::Water => "water",
            Era::Steam => "steam",
            Era::Electric => "electric",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Era::Any => "Any era",
            Era::Water => "Water and timber",
            Era::Steam => "Steam and cast iron",
            Era::Electric => "Electric and steel",
        }
    }

    /// What building a plant out of this family actually feels like.
    pub fn blurb(self) -> &'static str {
        match self {
            Era::Any => "a bin, a chute and a crusher are the same in every century",
            Era::Water => {
                "one prime mover, one line shaft, and belts to everything -- slow, \
                 torquey, cold, and only as strong as the timber it is bolted to"
            }
            Era::Steam => {
                "power where you want it and heat you did not ask for: an engine \
                 that must be cooled, and waste heat hot enough to be worth catching"
            }
            Era::Electric => {
                "a motor per machine, coupled straight on: small, fast, quiet, and \
                 metered by the unit"
            }
        }
    }

    /// The failure this family is prone to, which is the thing that makes it a
    /// different machine rather than a different sprite.
    pub fn fails(self) -> &'static str {
        match self {
            Era::Any => "",
            Era::Water => "it runs out of torque, and it shakes its own frame apart",
            Era::Steam => "it cooks itself, unless the waste heat has somewhere to go",
            Era::Electric => "it stops when the grid does, and every unit is billed",
        }
    }

    pub fn by_tag(tag: &str) -> Option<Era> {
        [Era::Any, Era::Water, Era::Steam, Era::Electric]
            .into_iter()
            .find(|e| e.tag() == tag)
    }
}

impl fmt::Display for Era {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

// --------------------------------------------------------------- materials

/// What a component's frame is made of.
///
/// Three numbers each, and every one of them is a reason to choose a different
/// one: timber is cheap and insulating and weak, cast iron is heavy and
/// tolerant and middling, steel is the best at everything and is the thing the
/// third era has and the first does not.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Mat {
    Wood,
    CastIron,
    Steel,
}

pub const MATS: [Mat; 3] = [Mat::Wood, Mat::CastIron, Mat::Steel];

impl Mat {
    pub fn tag(self) -> &'static str {
        match self {
            Mat::Wood => "wood",
            Mat::CastIron => "iron",
            Mat::Steel => "steel",
        }
    }

    pub fn title(self) -> &'static str {
        match self {
            Mat::Wood => "Timber frame",
            Mat::CastIron => "Cast-iron frame",
            Mat::Steel => "Steel frame",
        }
    }

    /// How well it carries heat out of the body and into the air, as a
    /// percentage of the body's temperature shed per tick.
    ///
    /// Timber is an insulator, which is exactly the problem with it: a wooden
    /// machine does not run cool because it is old and gentle, it runs hot
    /// because the heat cannot get out.
    pub fn cond(self) -> u64 {
        match self {
            Mat::Wood => 55,
            Mat::CastIron => 100,
            Mat::Steel => 150,
        }
    }

    /// The hottest the frame itself will stand, whatever the machine inside it
    /// would tolerate. Timber chars, cast iron cracks, steel is the reason a
    /// steam plant can be pushed.
    pub fn ceiling(self) -> u32 {
        match self {
            Mat::Wood => 110,
            Mat::CastIron => 220,
            Mat::Steel => 320,
        }
    }

    /// How much vibration it will carry without coming apart.
    ///
    /// This is the number that makes the first era a different machine. A
    /// crusher shakes at 7. Timber rates 4. So a water-wheel plant cannot bolt
    /// its crusher to its line shaft, and the belt -- which is slack, and
    /// therefore passes torque without passing shake -- stops being a lossy
    /// pipe and becomes the reason the layout is shaped the way it is.
    pub fn tol(self) -> u8 {
        match self {
            Mat::Wood => 4,
            Mat::CastIron => 7,
            Mat::Steel => 9,
        }
    }

    pub fn by_tag(tag: &str) -> Option<Mat> {
        MATS.into_iter().find(|m| m.tag() == tag)
    }

    pub fn to_json(self) -> Json {
        Json::obj()
            .set("tag", self.tag())
            .set("title", self.title())
            .set("conducts", self.cond() as i64)
            .set("ceiling", self.ceiling() as i64)
            .set("tolerates", self.tol() as i64)
    }
}

impl fmt::Display for Mat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

// ------------------------------------------------------------------- bands

/// The five things a body temperature can mean.
#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug, Hash)]
pub enum Band {
    Cold,
    Normal,
    Warm,
    Hot,
    Overheated,
}

/// What each band does to output, in per mille.
///
/// These five numbers are the whole behavioural model, and they are here rather
/// than spread through `sim` so that re-tuning the experiment is editing one
/// screen.
pub const DUTY: [u64; 5] = [600, 1000, 950, 750, 0];

impl Band {
    pub fn tag(self) -> &'static str {
        match self {
            Band::Cold => "COLD",
            Band::Normal => "NORMAL",
            Band::Warm => "WARM",
            Band::Hot => "HOT",
            Band::Overheated => "OVERHEATED",
        }
    }

    /// Per mille of rated output in this band.
    pub fn duty(self) -> u64 {
        DUTY[self as usize]
    }

    /// Whether a component in this band is doing its job.
    pub fn well(self) -> bool {
        matches!(self, Band::Normal | Band::Warm)
    }

    /// One sentence a player can act on.
    pub fn note(self) -> &'static str {
        match self {
            Band::Cold => "below its operating range -- it needs to be run in, or kept warm",
            Band::Normal => "in its operating range",
            Band::Warm => "above its range, and losing a little to it",
            Band::Hot => "far above its range -- a quarter of its output is gone",
            Band::Overheated => {
                "tripped on temperature, and it will not restart until it is back \
                 in range -- give it cooling, spacing, or a better frame"
            }
        }
    }
}

impl fmt::Display for Band {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

// ------------------------------------------------------- the physical table

/// The physical facts about one kind of component.
///
/// A row of zeroes is a legitimate row: a chute has no heat, no vibration and
/// no opinion about what it is made of, and saying so explicitly is cheaper
/// than an `Option` on every field and more honest than leaving it out.
pub struct Phys {
    /// Which row of the table this is, so that inserting one in the wrong place
    /// is caught rather than silently giving a water wheel a steam engine's
    /// temperature limit.
    pub kind: super::parts::Kind,
    pub era: Era,
    /// Rated demand (negative) or output (positive) in the units of whichever
    /// energy domain the component works in, per tick. Declared rather than
    /// derived, because a port's rate is what may *cross* it and this is what
    /// the machine is for.
    pub power: i64,
    /// How heavy a load it will turn, or needs to be turned by: `0..=9`, the
    /// same scale `Qual::speed` uses, because the two are the two halves of one
    /// fact about a drive.
    pub torque: u8,
    /// The speed band it runs at, or wants. `0` where speed means nothing.
    pub speed: u8,
    /// Heat units put into its own body per tick at full output.
    pub heat: u64,
    /// Heat units per degree of body temperature. A heavy machine takes longer
    /// to warm up and longer to cool down, and that is all thermal mass is.
    pub mass: u64,
    /// The bottom of the optimal range. `0` for anything that does not have to
    /// be warm to work.
    pub lo: u32,
    /// The top of the optimal range.
    pub hi: u32,
    /// Where the machine itself trips, before its frame is considered.
    pub max: u32,
    /// What it comes on, and what else it may be built out of. The first entry
    /// of `mats` is always `mat`, so a palette can offer the list and a file
    /// can leave the default unwritten.
    pub mat: Mat,
    pub mats: &'static [Mat],
    /// What it shakes at, at full output.
    pub vib: u8,
}

impl Phys {
    /// Whether this component has a body worth tracking the temperature of. A
    /// chute does not; a steam engine emphatically does.
    pub fn thermal(&self) -> bool {
        self.heat > 0 && self.mass > 0
    }

    /// The temperature this component trips at, given the frame it is on: the
    /// lower of what the machine will stand and what the frame will.
    pub fn ceiling(&self, m: Mat) -> u32 {
        self.max.min(m.ceiling())
    }

    /// Which band a body temperature falls in.
    ///
    /// Derived from three numbers rather than five thresholds, so that adding a
    /// component means stating its operating range and its limit and nothing
    /// else. WARM is the first third of the space between the top of the range
    /// and the trip; HOT is the rest of it.
    pub fn band(&self, temp: u32, m: Mat) -> Band {
        let max = self.ceiling(m);
        if temp >= max {
            return Band::Overheated;
        }
        if temp < self.lo {
            return Band::Cold;
        }
        if temp <= self.hi {
            return Band::Normal;
        }
        if temp <= self.hi + (max - self.hi) / 3 {
            Band::Warm
        } else {
            Band::Hot
        }
    }

    /// Where this component would settle with a waste port wired to something
    /// that will take everything it offers, on this frame, with this much room
    /// around it. `None` when it has no waste port to wire.
    pub fn cooled_at(&self, m: Mat, clear: u32) -> u32 {
        let c = cool(m, clear) + WASTE_COND;
        ((self.heat * 100 / c) as u32).min(1 << 20)
    }

    /// Where this component would settle with no cooling but the air, on this
    /// frame, with this much room around it.
    ///
    /// Useful before anything is simulated: it is the number that tells a
    /// player whether a design needs a radiator or merely wants one, and the
    /// inspector prints it next to the temperature so that "it is climbing" and
    /// "it is going to stop climbing at 94" are different sentences.
    pub fn settles_at(&self, m: Mat, clear: u32) -> u32 {
        let c = cool(m, clear);
        if c == 0 {
            return u32::MAX;
        }
        // The fixed point of `t -> t - shed(t) + heat`, which for integer
        // flooring is the largest t with shed(t) < heat, plus one.
        ((self.heat * 100 / c) as u32).min(1 << 20)
    }
}

/// The percentage of its own temperature a body sheds to the air each tick.
///
/// Two terms, and they are the two cooling decisions that cost nothing to
/// build: what it is made of, and how much air is around it. Four clear tiles
/// doubles it, and there is no credit past four -- a machine alone in a field
/// is not twice as cool as a machine with a corridor round it.
pub fn cool(m: Mat, clear: u32) -> u64 {
    m.cond() * (4 + clear.min(CLEAR_MAX) as u64) / 4
}

/// The most spacing that buys anything.
pub const CLEAR_MAX: u32 = 4;

/// Heat shed to the air in one tick by a body at `temp`.
pub fn shed(temp: u32, m: Mat, clear: u32) -> u64 {
    temp as u64 * cool(m, clear) / 100
}

/// How fast heat leaves a body through a cooling path somebody plumbed, as a
/// percentage of its temperature per tick.
///
/// Twice what still air manages off steel, and -- the part that matters -- it
/// is *proportional to temperature*, exactly as the passive term is. The first
/// version of this was a flat rate, and a flat rate turns out to be a small
/// disaster: a radiator could pull heat out of a stone-cold engine as fast as
/// out of a glowing one, so the body never warmed up, so its waste heat was
/// never worth catching, so the jacket downstream of it sat refusing ambient
/// heat forever. Cooling has to care what it is cooling.
pub const WASTE_COND: u64 = 200;

/// The most heat that will leave the body this tick through its waste port.
pub fn wasteable(temp: u32) -> u64 {
    temp as u64 * WASTE_COND / 100
}

/// Degrees of body temperature per band of the temperature scale everything
/// else in this crate is measured on.
///
/// This is the exchange rate between a machine's own warmth and the `Qual::temp`
/// of the heat that comes off it, and it is what decides whether waste heat is
/// worth catching: a motor settling at 30 degrees has nothing anybody wants, and
/// a steam engine at 140 has a jacket's worth.
pub const DEG_PER_BAND: u32 = 50;

/// The best grade any body heat comes off at, however hot the body.
///
/// One band, and the number is load bearing rather than cautious. An exchanger
/// boils water at band 2. If waste heat could reach band 2 then a steam engine
/// would boil the water that drives it, a design that is free power and an
/// afternoon of somebody's life spent finding out why. Waste heat here is
/// low-grade by definition: enough to warm feedwater, never enough to raise
/// steam.
pub const WASTE_GRADE_MAX: u8 = 1;

/// What the heat coming off a body at `temp` is worth, on the `0..=9` scale.
pub fn grade(temp: u32) -> u8 {
    (temp / DEG_PER_BAND).min(WASTE_GRADE_MAX as u32).min(TEMP_MAX as u32) as u8
}

/// The temperature a body must reach before its waste heat is worth catching at
/// all: one band, which is what a jacket will accept and what a cool machine
/// never reaches.
pub const GRADE_MIN: u32 = DEG_PER_BAND;

// --------------------------------------------------------------- vibration

/// Which domain carries vibration from one component to the next.
///
/// Only rotary, and only through rigid couplings. That is not a simplification,
/// it is the mechanic: a belt is the one drive component that passes torque
/// without passing shake, and it exists so that the first era has an answer to
/// a problem the third era does not have.
pub const SHAKE_DOMAIN: super::stuff::Domain = super::stuff::Domain::Rotary;

/// What a design does about vibration, said the way a player can act on.
pub fn shake_note(load: u8, mat: Mat, worst: &str) -> String {
    if load <= mat.tol() {
        return format!(
            "{} shakes at {load}, and {} rates {}",
            worst,
            mat.title().to_lowercase(),
            mat.tol()
        );
    }
    let cure = match mat {
        Mat::Wood => "put it on cast iron, or break the drive with a belt",
        Mat::CastIron => "put it on steel, or break the drive with a belt",
        Mat::Steel => "break the drive with a belt -- there is nothing stiffer to bolt it to",
    };
    format!(
        "{} shakes at {load} and {} rates {} -- {cure}",
        worst,
        mat.title().to_lowercase(),
        mat.tol()
    )
}

// ------------------------------------------------------------- on the wire

pub fn eras() -> Json {
    Json::Arr(
        ERAS.iter()
            .map(|e| {
                Json::obj()
                    .set("tag", e.tag())
                    .set("title", e.title())
                    .set("blurb", e.blurb())
                    .set("fails", e.fails())
            })
            .collect(),
    )
}

pub fn mats() -> Json {
    Json::Arr(MATS.iter().map(|m| m.to_json()).collect())
}

pub fn bands() -> Json {
    Json::Arr(
        [Band::Cold, Band::Normal, Band::Warm, Band::Hot, Band::Overheated]
            .iter()
            .map(|b| {
                Json::obj()
                    .set("tag", b.tag())
                    .set("duty", b.duty() as i64)
                    .set("well", b.well())
                    .set("note", b.note())
            })
            .collect(),
    )
}

impl Phys {
    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("era", self.era.tag())
            .set("eraTitle", self.era.title())
            .set("power", self.power)
            .set("torque", self.torque as i64)
            .set("speed", self.speed as i64)
            .set("heat", self.heat as i64)
            .set("mass", self.mass as i64)
            .set("thermal", self.thermal())
            .set("lo", self.lo as i64)
            .set("hi", self.hi as i64)
            .set("max", self.max as i64)
            .set("material", self.mat.tag())
            .set(
                "materials",
                Json::arr(self.mats.iter().map(|m| m.tag()).collect::<Vec<_>>()),
            )
            .set("vibration", self.vib as i64)
            .set(
                "settlesAt",
                if self.thermal() {
                    Json::Int(self.settles_at(self.mat, 0) as i128)
                } else {
                    Json::Null
                },
            )
            .set("kind", self.kind.tag())
    }
}
