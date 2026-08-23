//! The vocabulary: thirty-eight components in eight families, seven connection
//! domains, and the numbers that make one design better than another.
//!
//! Experiment 06 had eight components and one brief. They were the right eight
//! for that brief and useless for any other, which is the trap the note behind
//! experiment 07 names in its first paragraph: it is very easy to answer "what
//! else could a machine be?" by typing out a parts catalogue from an
//! engineering supplier, a prospect thrilling to roughly seven people.
//!
//! So the rule here is *families of primitives*, and the test of whether the
//! rule was followed is not an opinion. It is `machine reuse`, which counts how
//! many of the four challenges each component appears in. A component used by
//! one design is a bespoke component wearing a costume.
//!
//! ```text
//!   source     where matter and energy enter: reactor, burner, pump, inlet, mains
//!   sink       where it leaves: outlet as product, skip as waste, radiator as heat
//!   transport  distance, and what it costs: five pipes and a shaft
//!   store      inertia: hopper, tank, drum, flywheel
//!   control    deterministic thresholds: valve, clutch
//!   heat       exchanger, preheater, condenser, furnace, heater
//!   mechanical turbine, generator, motor, gearbox, crank
//!   process    crusher, mill, separator, rolling mill, press, lathe, column
//! ```
//!
//! # A component is a transformation with constraints
//!
//! Almost every one of them is a row in a table rather than a function:
//!
//! ```text
//!   Crusher {
//!       draws  drive  5 rotary   at speed <= 2
//!              in    10 material hardness <= 8, no finer than coarse
//!       makes  out   10 material one size finer
//!       rate   10 batches/tick
//!   }
//! ```
//!
//! which is the shape the note asked for, and it buys the thing that made the
//! note worth acting on: a motor is not part of the crusher's recipe. It
//! supplies the rotary domain. Six crushers can therefore hang off one engine
//! through a shared shaft, and nobody had to write that down as a special case.
//!
//! # The numbers
//!
//! Everything downstream is derived from this file, so re-tuning the experiment
//! is editing one screen of constants and one table.

use super::stuff::{
    Domain, Stuff, Subst, FORM_BILLET, FORM_GEAR, FORM_SCRAP, FORM_STRIP, SIZE_COARSE,
    SIZE_CRUSHED, SIZE_POWDER, SPEED_MAX, TEMP_MAX,
};
use std::fmt;

// ---------------------------------------------------------------- the ports

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    In,
    Out,
}

/// One socket on one component.
///
/// `rate` is how much may cross it in a tick; `cap` is how much may sit behind
/// it waiting. A component never reaches through a port to ask a neighbour for
/// anything: it fills its own output buffers and drains its own input buffers,
/// and the transfer stage moves whatever both ends can afford.
pub struct Port {
    pub name: &'static str,
    pub dom: Domain,
    pub dir: Dir,
    pub rate: u64,
    pub cap: u64,
    /// The machine's boundary. Whatever is left in an external output port at
    /// the end of a tick *leaves* -- it is the machine's product, or its waste.
    ///
    /// Unlike experiment 06, an external port can still be wired: a generator
    /// that powers a motor inside the same machine is a design, not a mistake,
    /// and what the motor does not take is still exported.
    pub external: bool,
}

const fn p(name: &'static str, dom: Domain, dir: Dir, rate: u64, cap: u64) -> Port {
    Port { name, dom, dir, rate, cap, external: false }
}

const fn ext(name: &'static str, dom: Domain, dir: Dir, rate: u64, cap: u64) -> Port {
    Port { name, dom, dir, rate, cap, external: true }
}

impl Domain {
    /// What an empty buffer in this domain is empty *of*. It is a resting
    /// value and nothing more -- an empty buffer accepts any substance -- but
    /// it has to be the same resting value every time or an orbit could not
    /// close on it.
    pub fn rest(self) -> Subst {
        match self {
            Domain::Material => Subst::Ore,
            Domain::Fluid | Domain::Gas => Subst::Water,
            Domain::Heat => Subst::Heat,
            Domain::Rotary => Subst::Torque,
            Domain::Mech => Subst::Stroke,
            Domain::Electrical => Subst::Power,
        }
    }
}

// --------------------------------------------------------- the recipe model

/// A condition on the stuff a component is about to draw.
///
/// These are gates, not preferences: unmet, the component makes nothing and
/// says which one it was. That is deliberate. A crusher that quietly runs at
/// 40% because the drive is too fast teaches the player nothing; one that stops
/// and says "speed 6, and this wants 2 or less -- put a gearbox in" teaches
/// them the whole mechanic in one sentence.
#[derive(Clone, Copy, Debug)]
pub enum Need {
    MinTemp(u8),
    MaxTemp(u8),
    /// No coarser than this.
    MinSize(u8),
    /// No finer than this.
    MaxSize(u8),
    /// This size and no other. A mill grinds what a crusher made and nothing
    /// else, and saying that as `MinSize` and `MaxSize` of the same value made
    /// the inspector print "crushed or finer, crushed or coarser", which is
    /// true, unhelpful, and slightly rude.
    Size(u8),
    MinPurity(u8),
    Form(u8),
    MaxHardness(u8),
    MinSpeed(u8),
    MaxSpeed(u8),
    OneOf(&'static [Subst]),
}

impl Need {
    /// Why this stuff will not do, if it will not.
    pub fn unmet(&self, s: &Stuff) -> Option<String> {
        use super::stuff::{FORM_NAMES, SIZE_NAMES, TEMP_NAMES};
        let temp = |t: u8| TEMP_NAMES[(t as usize).min(9)];
        let size = |t: u8| SIZE_NAMES[(t as usize).min(3)];
        let form = |t: u8| FORM_NAMES[(t as usize).min(4)];
        match *self {
            Need::MinTemp(t) if s.q.temp < t => {
                Some(format!("wants {} or hotter, and this is {}", temp(t), temp(s.q.temp)))
            }
            Need::MaxTemp(t) if s.q.temp > t => {
                Some(format!("wants {} or cooler, and this is {}", temp(t), temp(s.q.temp)))
            }
            Need::MinSize(z) if s.q.size < z => {
                Some(format!("wants {} or finer, and this is {}", size(z), size(s.q.size)))
            }
            Need::MaxSize(z) if s.q.size > z => {
                Some(format!("wants {} or coarser, and this is already {}", size(z), size(s.q.size)))
            }
            Need::Size(z) if s.q.size != z => {
                Some(format!("wants {}, and this is {}", size(z), size(s.q.size)))
            }
            Need::MinPurity(v) if s.q.purity < v => {
                Some(format!("wants {v}% pure or better, and this is {}%", s.q.purity))
            }
            Need::Form(f) if s.q.form != f => {
                Some(format!("wants {}, and this is {}", form(f), form(s.q.form)))
            }
            Need::MaxHardness(h) if s.hardness() > h => Some(format!(
                "{} is hardness {}, and this rates {h}",
                s.name(),
                s.hardness()
            )),
            Need::MinSpeed(v) if s.q.speed < v => Some(format!(
                "wants speed {v} or more, and the drive turns at {} -- gear it up",
                s.q.speed
            )),
            Need::MaxSpeed(v) if s.q.speed > v => Some(format!(
                "wants speed {v} or less, and the drive turns at {} -- gear it down",
                s.q.speed
            )),
            Need::OneOf(list) if !list.contains(&s.subst) => Some(format!(
                "takes {}, and this is {}",
                list.iter().map(|x| x.title()).collect::<Vec<_>>().join(" or "),
                s.name()
            )),
            _ => None,
        }
    }

    /// The condition, said in the affirmative, for a component that is idle and
    /// has nothing to complain about yet.
    pub fn wants(&self) -> String {
        use super::stuff::{FORM_NAMES, SIZE_NAMES, TEMP_NAMES};
        match *self {
            Need::MinTemp(t) => format!("{} or hotter", TEMP_NAMES[(t as usize).min(9)]),
            Need::MaxTemp(t) => format!("{} or cooler", TEMP_NAMES[(t as usize).min(9)]),
            Need::MinSize(z) => format!("{} or finer", SIZE_NAMES[(z as usize).min(3)]),
            Need::MaxSize(z) => format!("{} or coarser", SIZE_NAMES[(z as usize).min(3)]),
            Need::Size(z) => SIZE_NAMES[(z as usize).min(3)].to_string(),
            Need::MinPurity(v) => format!("{v}% pure or better"),
            Need::Form(f) => FORM_NAMES[(f as usize).min(4)].to_string(),
            Need::MaxHardness(h) => format!("hardness {h} or softer"),
            Need::MinSpeed(v) => format!("speed {v}+"),
            Need::MaxSpeed(v) => format!("speed {v} or less"),
            Need::OneOf(list) => {
                list.iter().map(|x| x.title()).collect::<Vec<_>>().join(" or ")
            }
        }
    }
}

/// What one batch takes from one input port.
pub struct Draw {
    pub port: usize,
    pub qty: u64,
    pub need: &'static [Need],
}

const fn draw(port: usize, qty: u64) -> Draw {
    Draw { port, qty, need: &[] }
}

const fn needs(port: usize, qty: u64, need: &'static [Need]) -> Draw {
    Draw { port, qty, need }
}

/// What a component does to the stuff it passes on.
#[derive(Clone, Copy, Debug)]
pub enum Effect {
    /// One step finer: lump, coarse, crushed, powder.
    Finer,
    Size(u8),
    Form(u8),
    PurityUp(u8),
    PurityDown(u8),
    Warmer(u8),
    Temp(u8),
    Speed(u8),
    Become(Subst),
}

impl Effect {
    fn apply(&self, s: &mut Stuff) {
        match *self {
            Effect::Finer => s.q.size = (s.q.size + 1).min(SIZE_POWDER),
            Effect::Size(v) => s.q.size = v,
            Effect::Form(v) => s.q.form = v,
            Effect::PurityUp(v) => s.q.purity = (s.q.purity + v).min(100),
            Effect::PurityDown(v) => s.q.purity = s.q.purity.saturating_sub(v),
            Effect::Warmer(v) => s.q.temp = (s.q.temp + v).min(TEMP_MAX),
            Effect::Temp(v) => s.q.temp = v.min(TEMP_MAX),
            Effect::Speed(v) => s.q.speed = v.min(SPEED_MAX),
            Effect::Become(v) => s.subst = v,
        }
    }

    pub fn said(&self) -> String {
        use super::stuff::{FORM_NAMES, SIZE_NAMES, TEMP_NAMES};
        match *self {
            Effect::Finer => "one size finer".to_string(),
            Effect::Size(v) => format!("comes out {}", SIZE_NAMES[(v as usize).min(3)]),
            Effect::Form(v) => format!("comes out {}", FORM_NAMES[(v as usize).min(4)]),
            Effect::PurityUp(v) => format!("+{v}% pure"),
            Effect::PurityDown(v) => format!("-{v}% pure"),
            Effect::Warmer(v) => format!("+{v} temperature bands"),
            Effect::Temp(v) => format!("comes out {}", TEMP_NAMES[(v as usize).min(9)]),
            Effect::Speed(v) => format!("turns at speed {v}"),
            Effect::Become(v) => format!("becomes {}", v.title()),
        }
    }
}

/// No input seeds this output: it is made rather than changed.
pub const MADE: usize = usize::MAX;

/// What one batch puts into one output port.
pub struct Make {
    pub port: usize,
    pub qty: u64,
    /// Which `draws` entry the stuff comes from, or `MADE`.
    pub from: usize,
    pub eff: &'static [Effect],
}

const fn make(port: usize, qty: u64, from: usize, eff: &'static [Effect]) -> Make {
    Make { port, qty, from, eff }
}

/// A component's whole behaviour, when it is not one of the ones with a mind of
/// its own.
pub struct Recipe {
    pub draws: &'static [Draw],
    pub makes: &'static [Make],
    /// Batches per tick, at most.
    pub rate: u64,
    /// Batches per tick, at *least* -- below this it does nothing at all.
    ///
    /// Only the press has one, and it is the same idea as the turbine's
    /// threshold: a press fed half the strokes it needs does not make half a
    /// gear, it fails to close. A component with a floor is a component that a
    /// buffer can rescue, which is the only reason a flywheel is worth its four
    /// tiles.
    pub floor: u64,
}

impl Recipe {
    /// The stuff this output port would produce from what was drawn.
    ///
    /// A `MADE` output starts from whatever its own domain is empty of -- a
    /// motor's rotary port from `Torque`, a generator's power port from
    /// `Power` -- so a component that invents a stream rather than changing one
    /// does not have to be told what domain it is in twice.
    pub fn out_stuff(&self, m: &Make, drawn: &[Stuff], ports: &[Port]) -> Stuff {
        let mut s = if m.from == MADE {
            Stuff::fresh(ports[m.port].dom.rest())
        } else {
            drawn[m.from]
        };
        for e in m.eff {
            e.apply(&mut s);
        }
        s
    }
}

// ------------------------------------------------------------- the families

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Family {
    Source,
    Sink,
    Transport,
    Store,
    Control,
    Heat,
    Mechanical,
    Process,
}

impl Family {
    pub fn tag(self) -> &'static str {
        match self {
            Family::Source => "source",
            Family::Sink => "sink",
            Family::Transport => "transport",
            Family::Store => "store",
            Family::Control => "control",
            Family::Heat => "heat",
            Family::Mechanical => "mechanical",
            Family::Process => "process",
        }
    }
}

impl fmt::Display for Family {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

// ------------------------------------------------------------ the constants

/// Ticks from cold to full output. A reactor burns fuel the whole way.
pub const WARMUP: u64 = 120;
pub const REACTOR_HEAT: u64 = 1000;
pub const REACTOR_FUEL: u64 = 100;
/// Below this the reaction will not hold.
pub const MIN_THROTTLE: u32 = 20;
/// The grade of heat a reactor makes. A furnace wants 5 or better; an
/// exchanger will boil water with 2.
pub const REACTOR_TEMP: u8 = 6;
pub const BURNER_TEMP: u8 = 5;
pub const HEATER_TEMP: u8 = 4;

/// The fraction a heat pipe loses to the room, and a line shaft to its
/// bearings.
pub const PIPE_LOSS_PCT: u64 = 2;
pub const SHAFT_LOSS_PCT: u64 = 1;

/// Clear tiles between two components that a direct connection can still span.
///
/// Without this a pipe is a component with no reason to exist. With it, the
/// tile grid is load bearing: things that work together sit together, a pipe is
/// how you buy distance, and the price of distance is the loss.
pub const REACH: i32 = 6;

/// Gas per tick below which a turbine will not turn over at all.
pub const TURBINE_MIN: u64 = 40;
pub const SPIN_MAX: u32 = 30;
pub const SPIN_UP: u32 = 2;
pub const SPIN_DOWN: u32 = 1;
pub const TURBINE_EFF: u64 = 75;
/// The speed band a turbine and a motor turn at.
pub const DRIVE_SPEED: u8 = 6;
pub const GENERATOR_EFF: u64 = 90;

/// 5 heat + 2 water makes 2 steam, which is experiment 06's 250/100/100 in the
/// smallest whole numbers a simulator can work in without ever rounding.
pub const BOIL_HEAT: u64 = 5;
pub const BOIL_WATER: u64 = 2;
pub const BOIL_STEAM: u64 = 2;
/// The band steam comes off an exchanger at.
pub const STEAM_TEMP: u8 = 2;

/// A furnace lifts what it heats by this many bands per pass. Two passes take
/// iron past its melting point, which is how a `material` becomes a `fluid`.
pub const FURNACE_LIFT: u8 = 5;

/// A gearbox's ratio is a whole number of bands, and it costs 2% either way.
pub const GEARBOX_LOSS_PCT: u64 = 2;

const ROTARY: Domain = Domain::Rotary;
const HEAT: Domain = Domain::Heat;
const FLUID: Domain = Domain::Fluid;
const GAS: Domain = Domain::Gas;
const MAT: Domain = Domain::Material;
const MECH: Domain = Domain::Mech;
const ELEC: Domain = Domain::Electrical;
const IN: Dir = Dir::In;
const OUT: Dir = Dir::Out;

// ----------------------------------------------------------------- the kinds

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Kind {
    // source
    Reactor,
    Burner,
    Heater,
    Mains,
    Pump,
    Inlet,
    // sink
    Outlet,
    Skip,
    Radiator,
    // transport
    HeatPipe,
    SteamPipe,
    FluidPipe,
    Chute,
    Screw,
    Shaft,
    Cable,
    // store
    Hopper,
    Tank,
    Drum,
    Flywheel,
    // control
    Valve,
    Clutch,
    // heat
    Exchanger,
    Preheater,
    Condenser,
    Furnace,
    // mechanical
    Turbine,
    Generator,
    Motor,
    Gearbox,
    Crank,
    // process
    Crusher,
    Mill,
    Separator,
    RollMill,
    Press,
    Lathe,
    Column,
}

pub const KINDS: [Kind; 38] = [
    Kind::Reactor,
    Kind::Burner,
    Kind::Heater,
    Kind::Mains,
    Kind::Pump,
    Kind::Inlet,
    Kind::Outlet,
    Kind::Skip,
    Kind::Radiator,
    Kind::HeatPipe,
    Kind::SteamPipe,
    Kind::FluidPipe,
    Kind::Chute,
    Kind::Screw,
    Kind::Shaft,
    Kind::Cable,
    Kind::Hopper,
    Kind::Tank,
    Kind::Drum,
    Kind::Flywheel,
    Kind::Valve,
    Kind::Clutch,
    Kind::Exchanger,
    Kind::Preheater,
    Kind::Condenser,
    Kind::Furnace,
    Kind::Turbine,
    Kind::Generator,
    Kind::Motor,
    Kind::Gearbox,
    Kind::Crank,
    Kind::Crusher,
    Kind::Mill,
    Kind::Separator,
    Kind::RollMill,
    Kind::Press,
    Kind::Lathe,
    Kind::Column,
];

pub struct Part {
    pub kind: Kind,
    /// What it is called in a `.machine` file and on the wire.
    pub tag: &'static str,
    pub title: &'static str,
    /// The one sentence that says why you would place one.
    pub blurb: &'static str,
    pub family: Family,
    pub w: u32,
    pub h: u32,
    pub ports: &'static [Port],
    /// `None` for the nine components with a mind of their own: they are in
    /// `sim` by hand because a warm-up, a threshold, a hysteresis loop or a
    /// phase change is not a row in a table.
    pub recipe: Option<&'static Recipe>,
}

// ------------------------------------------------------------------ sources

static REACTOR_PORTS: [Port; 1] = [p("heat", HEAT, OUT, REACTOR_HEAT, REACTOR_HEAT)];

static BURNER_PORTS: [Port; 2] =
    [p("fuel", MAT, IN, 20, 40), p("heat", HEAT, OUT, 400, 400)];
static BURNER_DRAWS: [Draw; 1] = [needs(0, 1, &[Need::OneOf(&[Subst::Coal])])];
static BURNER_MAKES: [Make; 1] =
    [make(1, 20, MADE, &[Effect::Become(Subst::Heat), Effect::Temp(BURNER_TEMP)])];
static BURNER: Recipe = Recipe { draws: &BURNER_DRAWS, makes: &BURNER_MAKES, rate: 20, floor: 0 };

static HEATER_PORTS: [Port; 2] =
    [p("power", ELEC, IN, 60, 60), p("heat", HEAT, OUT, 540, 540)];
static HEATER_DRAWS: [Draw; 1] = [draw(0, 10)];
static HEATER_MAKES: [Make; 1] =
    [make(1, 90, MADE, &[Effect::Become(Subst::Heat), Effect::Temp(HEATER_TEMP)])];
static HEATER: Recipe = Recipe { draws: &HEATER_DRAWS, makes: &HEATER_MAKES, rate: 6, floor: 0 };

static MAINS_PORTS: [Port; 1] = [p("power", ELEC, OUT, 400, 400)];
static PUMP_PORTS: [Port; 1] = [p("water", FLUID, OUT, 200, 400)];
static INLET_PORTS: [Port; 1] = [p("out", MAT, OUT, 100, 200)];

// -------------------------------------------------------------------- sinks

static HOPPER_PORTS: [Port; 2] =
    [p("in", MAT, IN, 200, 2000), p("out", MAT, OUT, 200, 200)];

/// The two sinks have the same three ports and differ only in which column of
/// the scoreboard they add to. That is the entire difference between a product
/// and a mistake, and it is worth being able to see it in one line of the table.
static OUTLET_PORTS: [Port; 3] = [
    ext("solid", MAT, IN, 200, 200),
    ext("liquid", FLUID, IN, 200, 200),
    ext("vapour", GAS, IN, 200, 200),
];
static SKIP_PORTS: [Port; 3] = [
    ext("solid", MAT, IN, 200, 200),
    ext("liquid", FLUID, IN, 200, 200),
    ext("vapour", GAS, IN, 200, 200),
];
static RADIATOR_PORTS: [Port; 1] = [ext("heat", HEAT, IN, 500, 500)];

// ---------------------------------------------------------------- transport

static HEATPIPE_PORTS: [Port; 2] =
    [p("in", HEAT, IN, 400, 400), p("out", HEAT, OUT, 400, 400)];
static STEAMPIPE_PORTS: [Port; 2] =
    [p("in", GAS, IN, 150, 150), p("out", GAS, OUT, 150, 150)];
static FLUIDPIPE_PORTS: [Port; 2] =
    [p("in", FLUID, IN, 150, 150), p("out", FLUID, OUT, 150, 150)];
static CHUTE_PORTS: [Port; 2] =
    [p("in", MAT, IN, 40, 40), p("out", MAT, OUT, 40, 40)];

static SCREW_PORTS: [Port; 3] = [
    p("drive", ROTARY, IN, 20, 20),
    p("in", MAT, IN, 150, 150),
    p("out", MAT, OUT, 150, 150),
];
static SCREW_DRAWS: [Draw; 2] =
    [needs(0, 2, &[Need::MinSpeed(1)]), draw(1, 15)];
static SCREW_MAKES: [Make; 1] = [make(2, 15, 1, &[])];
static SCREW: Recipe = Recipe { draws: &SCREW_DRAWS, makes: &SCREW_MAKES, rate: 10, floor: 0 };

static SHAFT_PORTS: [Port; 2] =
    [p("in", ROTARY, IN, 400, 400), p("out", ROTARY, OUT, 400, 400)];
static CABLE_PORTS: [Port; 2] =
    [p("in", ELEC, IN, 400, 400), p("out", ELEC, OUT, 400, 400)];

// ------------------------------------------------------------------- stores

static TANK_PORTS: [Port; 2] =
    [p("in", GAS, IN, 200, 2000), p("out", GAS, OUT, 200, 200)];
static DRUM_PORTS: [Port; 2] =
    [p("in", FLUID, IN, 200, 2000), p("out", FLUID, OUT, 200, 200)];
static FLYWHEEL_PORTS: [Port; 2] =
    [p("in", ROTARY, IN, 200, 1200), p("out", ROTARY, OUT, 200, 200)];

// ------------------------------------------------------------------ control

static VALVE_PORTS: [Port; 2] =
    [p("in", FLUID, IN, 200, 200), p("out", FLUID, OUT, 200, 200)];
static CLUTCH_PORTS: [Port; 2] =
    [p("in", ROTARY, IN, 200, 200), p("out", ROTARY, OUT, 200, 200)];

// --------------------------------------------------------------------- heat

static EXCHANGER_PORTS: [Port; 3] = [
    p("heat", HEAT, IN, 250, 500),
    p("water", FLUID, IN, 100, 200),
    p("steam", GAS, OUT, 100, 200),
];
static EXCHANGER_DRAWS: [Draw; 2] = [
    needs(0, BOIL_HEAT, &[Need::MinTemp(STEAM_TEMP)]),
    needs(1, BOIL_WATER, &[Need::OneOf(&[Subst::Water])]),
];
static EXCHANGER_MAKES: [Make; 1] = [make(2, BOIL_STEAM, 1, &[Effect::Temp(STEAM_TEMP)])];
static EXCHANGER: Recipe =
    Recipe { draws: &EXCHANGER_DRAWS, makes: &EXCHANGER_MAKES, rate: 50, floor: 0 };

static PREHEATER_PORTS: [Port; 3] = [
    p("heat", HEAT, IN, 120, 240),
    p("in", FLUID, IN, 120, 240),
    p("out", FLUID, OUT, 120, 240),
];
static PREHEATER_DRAWS: [Draw; 2] =
    [needs(0, 2, &[Need::MinTemp(2)]), draw(1, 2)];
static PREHEATER_MAKES: [Make; 1] = [make(2, 2, 1, &[Effect::Warmer(2)])];
static PREHEATER: Recipe =
    Recipe { draws: &PREHEATER_DRAWS, makes: &PREHEATER_MAKES, rate: 60, floor: 0 };

static CONDENSER_PORTS: [Port; 4] = [
    p("vapour", GAS, IN, 120, 240),
    p("coolant", FLUID, IN, 60, 120),
    p("out", FLUID, OUT, 120, 240),
    p("heat", HEAT, OUT, 60, 120),
];
static CONDENSER_DRAWS: [Draw; 2] = [
    draw(0, 2),
    needs(1, 1, &[Need::OneOf(&[Subst::Water]), Need::MaxTemp(1)]),
];
static CONDENSER_MAKES: [Make; 2] = [
    make(2, 2, 0, &[Effect::Temp(1)]),
    make(3, 1, MADE, &[Effect::Become(Subst::Heat), Effect::Temp(2)]),
];
static CONDENSER: Recipe =
    Recipe { draws: &CONDENSER_DRAWS, makes: &CONDENSER_MAKES, rate: 60, floor: 0 };

static FURNACE_PORTS: [Port; 4] = [
    p("heat", HEAT, IN, 500, 1000),
    p("in", MAT, IN, 60, 120),
    p("out", MAT, OUT, 60, 120),
    p("molten", FLUID, OUT, 60, 120),
];

// --------------------------------------------------------------- mechanical

static TURBINE_PORTS: [Port; 2] = [
    // One tick of intake and not a drop more: gas that reaches a turbine and
    // is not used does not queue, it condenses. That one decision is what makes
    // the Steam Buffer a component rather than a decoration.
    p("steam", GAS, IN, 80, 80),
    p("rotary", ROTARY, OUT, 120, 120),
];

static GENERATOR_PORTS: [Port; 2] =
    [p("rotary", ROTARY, IN, 70, 70), ext("power", ELEC, OUT, 200, 200)];

/// The one speed band a generator insists on. It is not in a recipe because a
/// generator is not a recipe: rounding its intake to whole batches would make
/// it throw away up to nine rotary a tick, and experiment 06's designs are
/// measured to two decimal places.
pub const GENERATOR_MIN_SPEED: u8 = 4;

static MOTOR_PORTS: [Port; 2] =
    [p("power", ELEC, IN, 60, 60), p("rotary", ROTARY, OUT, 54, 54)];
static MOTOR_DRAWS: [Draw; 1] = [draw(0, 10)];
static MOTOR_MAKES: [Make; 1] =
    [make(1, 9, MADE, &[Effect::Become(Subst::Torque), Effect::Speed(DRIVE_SPEED)])];
static MOTOR: Recipe = Recipe { draws: &MOTOR_DRAWS, makes: &MOTOR_MAKES, rate: 6, floor: 0 };

static GEARBOX_PORTS: [Port; 2] =
    [p("in", ROTARY, IN, 300, 300), p("out", ROTARY, OUT, 300, 300)];

static CRANK_PORTS: [Port; 2] =
    [p("drive", ROTARY, IN, 100, 100), p("stroke", MECH, OUT, 92, 92)];
static CRANK_DRAWS: [Draw; 1] = [needs(0, 25, &[Need::MinSpeed(2)])];
static CRANK_MAKES: [Make; 1] = [make(1, 23, MADE, &[Effect::Become(Subst::Stroke)])];
static CRANK: Recipe = Recipe { draws: &CRANK_DRAWS, makes: &CRANK_MAKES, rate: 4, floor: 0 };

// ------------------------------------------------------------------ process

static CRUSHER_PORTS: [Port; 3] = [
    p("drive", ROTARY, IN, 50, 50),
    p("in", MAT, IN, 100, 200),
    p("out", MAT, OUT, 100, 200),
];
static CRUSHER_DRAWS: [Draw; 2] = [
    needs(0, 5, &[Need::MaxSpeed(2)]),
    needs(1, 10, &[Need::MaxHardness(8), Need::MaxSize(SIZE_COARSE)]),
];
static CRUSHER_MAKES: [Make; 1] = [make(2, 10, 1, &[Effect::Finer])];
static CRUSHER: Recipe = Recipe { draws: &CRUSHER_DRAWS, makes: &CRUSHER_MAKES, rate: 10, floor: 0 };

static MILL_PORTS: [Port; 3] = [
    p("drive", ROTARY, IN, 80, 80),
    p("in", MAT, IN, 100, 200),
    p("out", MAT, OUT, 100, 200),
];
static MILL_DRAWS: [Draw; 2] =
    [needs(0, 8, &[Need::MinSpeed(4)]), needs(1, 10, &[Need::Size(SIZE_CRUSHED)])];
static MILL_MAKES: [Make; 1] = [make(2, 10, 1, &[Effect::Size(SIZE_POWDER)])];
static MILL: Recipe = Recipe { draws: &MILL_DRAWS, makes: &MILL_MAKES, rate: 10, floor: 0 };

static SEPARATOR_PORTS: [Port; 4] = [
    p("drive", ROTARY, IN, 40, 40),
    p("in", MAT, IN, 100, 200),
    p("rich", MAT, OUT, 40, 80),
    p("tails", MAT, OUT, 60, 120),
];
static SEPARATOR_DRAWS: [Draw; 2] = [
    needs(0, 4, &[Need::MinSpeed(3)]),
    needs(1, 10, &[Need::MinSize(SIZE_POWDER)]),
];
static SEPARATOR_MAKES: [Make; 2] = [
    make(2, 4, 1, &[Effect::PurityUp(42)]),
    make(3, 6, 1, &[Effect::PurityDown(28)]),
];
static SEPARATOR: Recipe =
    Recipe { draws: &SEPARATOR_DRAWS, makes: &SEPARATOR_MAKES, rate: 10, floor: 0 };

static ROLLMILL_PORTS: [Port; 3] = [
    p("drive", ROTARY, IN, 60, 60),
    p("in", MAT, IN, 60, 120),
    p("out", MAT, OUT, 60, 120),
];
static ROLLMILL_DRAWS: [Draw; 2] = [
    needs(0, 10, &[Need::MinSpeed(3)]),
    needs(1, 10, &[Need::Form(FORM_BILLET), Need::MinTemp(4)]),
];
static ROLLMILL_MAKES: [Make; 1] = [make(2, 10, 1, &[Effect::Form(FORM_STRIP)])];
static ROLLMILL: Recipe = Recipe { draws: &ROLLMILL_DRAWS, makes: &ROLLMILL_MAKES, rate: 6, floor: 0 };

static PRESS_PORTS: [Port; 3] = [
    p("drive", MECH, IN, 60, 60),
    p("in", MAT, IN, 60, 120),
    p("out", MAT, OUT, 60, 120),
];
static PRESS_DRAWS: [Draw; 2] =
    [draw(0, 10), needs(1, 10, &[Need::Form(FORM_STRIP)])];
static PRESS_MAKES: [Make; 1] = [make(2, 10, 1, &[Effect::Form(FORM_GEAR)])];
/// Three of its six strokes, and not one fewer. See `Recipe::floor`.
pub const PRESS_FLOOR: u64 = 3;
static PRESS: Recipe =
    Recipe { draws: &PRESS_DRAWS, makes: &PRESS_MAKES, rate: 6, floor: PRESS_FLOOR };

static LATHE_PORTS: [Port; 5] = [
    p("drive", ROTARY, IN, 40, 40),
    p("power", ELEC, IN, 20, 20),
    p("in", MAT, IN, 20, 40),
    p("out", MAT, OUT, 12, 24),
    p("swarf", MAT, OUT, 8, 16),
];
static LATHE_DRAWS: [Draw; 3] = [
    needs(0, 10, &[Need::MinSpeed(5)]),
    draw(1, 5),
    needs(2, 5, &[Need::Form(FORM_BILLET), Need::MaxTemp(2)]),
];
static LATHE_MAKES: [Make; 2] = [
    make(3, 3, 2, &[Effect::Form(FORM_GEAR), Effect::PurityUp(4)]),
    make(4, 2, 2, &[Effect::Form(FORM_SCRAP)]),
];
static LATHE: Recipe = Recipe { draws: &LATHE_DRAWS, makes: &LATHE_MAKES, rate: 4, floor: 0 };

static COLUMN_PORTS: [Port; 5] = [
    p("feed", FLUID, IN, 120, 240),
    p("heat", HEAT, IN, 240, 480),
    p("light", GAS, OUT, 60, 120),
    p("middle", FLUID, OUT, 80, 160),
    p("heavy", FLUID, OUT, 80, 160),
];

// ------------------------------------------------------------------- the table

static PARTS: [Part; 38] = [
    Part { kind: Kind::Reactor, tag: "reactor", title: "Fuel / Heat Source",
        blurb: "burns fuel at its throttle whether or not the heat is wanted",
        family: Family::Source, w: 4, h: 4, ports: &REACTOR_PORTS, recipe: None },
    Part { kind: Kind::Burner, tag: "burner", title: "Burner",
        blurb: "20 coal/tick becomes 400 heat/tick, at a lower grade than a reactor",
        family: Family::Source, w: 3, h: 3, ports: &BURNER_PORTS, recipe: Some(&BURNER) },
    Part { kind: Kind::Heater, tag: "heater", title: "Electric Heater",
        blurb: "60 MW becomes 540 heat/tick, and needs no fuel and no water",
        family: Family::Source, w: 2, h: 2, ports: &HEATER_PORTS, recipe: Some(&HEATER) },
    Part { kind: Kind::Mains, tag: "mains", title: "Grid Connection",
        blurb: "up to 400 MW/tick from outside, and every unit is counted against you",
        family: Family::Source, w: 2, h: 2, ports: &MAINS_PORTS, recipe: None },
    Part { kind: Kind::Pump, tag: "pump", title: "Fluid Inlet",
        blurb: "draws 200/tick of one fluid from outside the machine",
        family: Family::Source, w: 2, h: 2, ports: &PUMP_PORTS, recipe: None },
    Part { kind: Kind::Inlet, tag: "inlet", title: "Material Inlet",
        blurb: "feeds 100/tick of one raw material from outside the machine",
        family: Family::Source, w: 2, h: 2, ports: &INLET_PORTS, recipe: None },

    Part { kind: Kind::Outlet, tag: "outlet", title: "Product Outlet",
        blurb: "where the machine's product leaves: solids, liquids or vapour",
        family: Family::Sink, w: 2, h: 2, ports: &OUTLET_PORTS, recipe: None },
    Part { kind: Kind::Skip, tag: "skip", title: "Waste Skip",
        blurb: "takes solids, liquids or vapour away, and counts every unit as waste",
        family: Family::Sink, w: 2, h: 2, ports: &SKIP_PORTS, recipe: None },
    Part { kind: Kind::Radiator, tag: "radiator", title: "Heat Sink",
        blurb: "dumps 500 heat/tick to the sky so something upstream can keep going",
        family: Family::Sink, w: 2, h: 2, ports: &RADIATOR_PORTS, recipe: None },

    Part { kind: Kind::HeatPipe, tag: "heatpipe", title: "Heat Pipe",
        blurb: "carries 400 heat/tick, and loses 2% of it",
        family: Family::Transport, w: 3, h: 1, ports: &HEATPIPE_PORTS, recipe: None },
    Part { kind: Kind::SteamPipe, tag: "steampipe", title: "Gas Pipe",
        blurb: "carries 150 gas/tick",
        family: Family::Transport, w: 3, h: 1, ports: &STEAMPIPE_PORTS, recipe: None },
    Part { kind: Kind::FluidPipe, tag: "fluidpipe", title: "Fluid Pipe",
        blurb: "carries 150 fluid/tick",
        family: Family::Transport, w: 3, h: 1, ports: &FLUIDPIPE_PORTS, recipe: None },
    Part { kind: Kind::Chute, tag: "chute", title: "Chute",
        blurb: "40 material/tick downhill, for nothing at all",
        family: Family::Transport, w: 3, h: 1, ports: &CHUTE_PORTS, recipe: None },
    Part { kind: Kind::Screw, tag: "screw", title: "Screw Conveyor",
        blurb: "150 material/tick, if you can spare 20 rotary to turn it",
        family: Family::Transport, w: 3, h: 2, ports: &SCREW_PORTS, recipe: Some(&SCREW) },
    Part { kind: Kind::Shaft, tag: "shaft", title: "Line Shaft",
        blurb: "carries 400 rotary/tick at 99%, and reaches four tiles further",
        family: Family::Transport, w: 4, h: 1, ports: &SHAFT_PORTS, recipe: None },

    Part { kind: Kind::Cable, tag: "cable", title: "Power Cable",
        blurb: "carries 400 MW/tick at 99%, so one grid connection can feed a whole plant",
        family: Family::Transport, w: 3, h: 1, ports: &CABLE_PORTS, recipe: None },
    Part { kind: Kind::Hopper, tag: "hopper", title: "Hopper",
        blurb: "holds 2000 of a material; in pulse mode it fills quietly and empties hard",
        family: Family::Store, w: 3, h: 3, ports: &HOPPER_PORTS, recipe: None },
    Part { kind: Kind::Tank, tag: "tank", title: "Gas Buffer",
        blurb: "holds 2000 gas; in pulse mode it fills quietly and empties hard",
        family: Family::Store, w: 3, h: 3, ports: &TANK_PORTS, recipe: None },
    Part { kind: Kind::Drum, tag: "drum", title: "Fluid Drum",
        blurb: "holds 2000 fluid; in pulse mode it fills quietly and empties hard",
        family: Family::Store, w: 3, h: 3, ports: &DRUM_PORTS, recipe: None },
    Part { kind: Kind::Flywheel, tag: "flywheel", title: "Flywheel",
        blurb: "stores 1200 rotary, so a drive that stutters drives something that must not",
        family: Family::Store, w: 2, h: 2, ports: &FLYWHEEL_PORTS, recipe: None },

    Part { kind: Kind::Valve, tag: "valve", title: "Valve",
        blurb: "passes at most what you set it to, and not one unit more",
        family: Family::Control, w: 2, h: 1, ports: &VALVE_PORTS, recipe: None },
    Part { kind: Kind::Clutch, tag: "clutch", title: "Clutch",
        blurb: "engages only once its threshold has gathered, then passes everything",
        family: Family::Control, w: 2, h: 1, ports: &CLUTCH_PORTS, recipe: None },

    Part { kind: Kind::Exchanger, tag: "exchanger", title: "Heat Exchanger",
        blurb: "250 heat and 100 water makes 100 steam; short of either it makes less",
        family: Family::Heat, w: 3, h: 3, ports: &EXCHANGER_PORTS, recipe: Some(&EXCHANGER) },
    Part { kind: Kind::Preheater, tag: "preheater", title: "Preheater",
        blurb: "120 fluid/tick, two temperature bands warmer, for 120 heat",
        family: Family::Heat, w: 3, h: 2, ports: &PREHEATER_PORTS, recipe: Some(&PREHEATER) },
    Part { kind: Kind::Condenser, tag: "condenser", title: "Condenser",
        blurb: "turns 120 vapour/tick back into fluid, using cold water, and sheds the heat",
        family: Family::Heat, w: 3, h: 3, ports: &CONDENSER_PORTS, recipe: Some(&CONDENSER) },
    Part { kind: Kind::Furnace, tag: "furnace", title: "Furnace Chamber",
        blurb: "lifts 60 material/tick five bands; past melting it comes out as a fluid",
        family: Family::Heat, w: 4, h: 3, ports: &FURNACE_PORTS, recipe: None },

    Part { kind: Kind::Turbine, tag: "turbine", title: "Turbine",
        blurb: "80 gas/tick at 75%, but stalls below 40 and spins up slowly",
        family: Family::Mechanical, w: 3, h: 2, ports: &TURBINE_PORTS, recipe: None },
    Part { kind: Kind::Generator, tag: "generator", title: "Generator",
        blurb: "70 rotary/tick at 90%, so 63 MW and no more, and it wants speed 4+",
        family: Family::Mechanical, w: 2, h: 2, ports: &GENERATOR_PORTS, recipe: None },
    Part { kind: Kind::Motor, tag: "motor", title: "Electric Motor",
        blurb: "60 MW/tick becomes 54 rotary at speed 6",
        family: Family::Mechanical, w: 2, h: 2, ports: &MOTOR_PORTS, recipe: Some(&MOTOR) },
    Part { kind: Kind::Gearbox, tag: "gearbox", title: "Gearbox",
        blurb: "trades speed for the ability to turn something heavy, at 2%",
        family: Family::Mechanical, w: 2, h: 2, ports: &GEARBOX_PORTS, recipe: None },
    Part { kind: Kind::Crank, tag: "crank", title: "Crank",
        blurb: "turns 100 rotary/tick into 92 strokes, which is what a press eats",
        family: Family::Mechanical, w: 2, h: 2, ports: &CRANK_PORTS, recipe: Some(&CRANK) },

    Part { kind: Kind::Crusher, tag: "crusher", title: "Crusher",
        blurb: "100/tick one size finer, on 50 rotary -- but only turning slowly",
        family: Family::Process, w: 3, h: 3, ports: &CRUSHER_PORTS, recipe: Some(&CRUSHER) },
    Part { kind: Kind::Mill, tag: "mill", title: "Mill",
        blurb: "crushed to powder, 100/tick, on 80 rotary turning fast",
        family: Family::Process, w: 3, h: 3, ports: &MILL_PORTS, recipe: Some(&MILL) },
    Part { kind: Kind::Separator, tag: "separator", title: "Separator",
        blurb: "splits powder into 40% rich and 60% tailings, and the rich is much richer",
        family: Family::Process, w: 3, h: 3, ports: &SEPARATOR_PORTS, recipe: Some(&SEPARATOR) },
    Part { kind: Kind::RollMill, tag: "rollmill", title: "Rolling Mill",
        blurb: "hot billet into strip, 60/tick -- it will not touch cold metal",
        family: Family::Process, w: 3, h: 2, ports: &ROLLMILL_PORTS, recipe: Some(&ROLLMILL) },
    Part { kind: Kind::Press, tag: "press", title: "Stamping Press",
        blurb: "strip into gears, 60/tick, wasting nothing -- but it needs strokes",
        family: Family::Process, w: 3, h: 3, ports: &PRESS_PORTS, recipe: Some(&PRESS) },
    Part { kind: Kind::Lathe, tag: "lathe", title: "Lathe / CNC",
        blurb: "20 billet/tick into 12 gears and 8 swarf: slower, finer, hungrier",
        family: Family::Process, w: 3, h: 2, ports: &LATHE_PORTS, recipe: Some(&LATHE) },
    Part { kind: Kind::Column, tag: "column", title: "Distillation Column",
        blurb: "splits hot crude into light, middle and heavy; more stages, better split",
        family: Family::Process, w: 3, h: 6, ports: &COLUMN_PORTS, recipe: None },
];

pub fn part(kind: Kind) -> &'static Part {
    let p = &PARTS[kind as usize];
    // The table is indexed by the enum's own discriminant, which is fast, free
    // and silently wrong the first time somebody inserts a row in the wrong
    // place -- a power cable that carries material, and every design that used
    // one refusing to load. It costs nothing to notice.
    debug_assert_eq!(p.kind, kind, "the parts table is out of order at {kind:?}");
    p
}

pub fn by_tag(tag: &str) -> Option<Kind> {
    PARTS.iter().find(|p| p.tag == tag).map(|p| p.kind)
}

impl Part {
    pub fn port_index(&self, name: &str) -> Option<usize> {
        self.ports.iter().position(|p| p.name == name)
    }

    pub fn tiles(&self) -> u32 {
        self.w * self.h
    }

}

impl Kind {
    pub fn tag(self) -> &'static str {
        part(self).tag
    }
    pub fn title(self) -> &'static str {
        part(self).title
    }
    pub fn family(self) -> Family {
        part(self).family
    }
    pub fn recipe(self) -> Option<&'static Recipe> {
        part(self).recipe
    }
}

/// The distillation column's yields, per ten units of feed.
///
/// A column is not a table row because its split is the one thing the player
/// tunes about it, and because the note that asked for this experiment was
/// explicit that a column should expose `separation_quality`, `throughput` and
/// `energy_required` rather than ask anybody to solve vapour-liquid equilibrium
/// equations while eating cereal.
///
/// ```text
///   stages   light  middle  heavy   heat per batch
///        1       2       3      5               8
///        2       3       4      3              14
///        3       4       5      1              22
/// ```
pub const COLUMN_MIN_STAGES: u32 = 1;
pub const COLUMN_MAX_STAGES: u32 = 3;

pub fn column_split(stages: u32) -> (u64, u64, u64, u64) {
    match stages.clamp(COLUMN_MIN_STAGES, COLUMN_MAX_STAGES) {
        1 => (2, 3, 5, 8),
        2 => (3, 4, 3, 14),
        _ => (4, 5, 1, 22),
    }
}

/// The temperature a column needs its feed at before anything separates.
pub const COLUMN_FEED_TEMP: u8 = 4;
/// Units of feed per batch.
pub const COLUMN_BATCH: u64 = 10;
/// Batches per tick.
pub const COLUMN_RATE: u64 = 12;

/// What the light fraction comes off the top as: a vapour, which is why a
/// condenser is not optional.
pub const COLUMN_LIGHT_TEMP: u8 = 3;
