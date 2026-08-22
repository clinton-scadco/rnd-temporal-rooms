//! The vocabulary: eight components, five port types, and the handful of
//! numbers that make one design better than another.
//!
//! The point of experiment 06 is that a building is *assembled* rather than
//! *selected*, so the vocabulary has to be small enough to hold in your head
//! and rich enough that two people solve the same brief differently. That
//! means every component gets exactly one interesting constraint and no more:
//!
//! ```text
//!   Fuel / Heat Source   burns whether or not anyone wants the heat
//!   Heat Pipe            finite throughput, and it leaks
//!   Water Source         finite throughput
//!   Heat Exchanger       needs heat AND water, in a fixed ratio
//!   Steam Pipe           finite throughput
//!   Steam Buffer         can hold, and can release in pulses
//!   Turbine              will not spin below a threshold, and spins up slowly
//!   Generator            finite rotary intake, fixed conversion loss
//! ```
//!
//! None of that is thermodynamics. There is no pressure, no temperature, no
//! torque and no phase change -- a steam unit is a steam unit. What the numbers
//! are chosen for is the *shape of the optimisation*: a capacity to run out of,
//! an efficiency to lose to, and one threshold -- the turbine's -- that makes
//! buffering a real decision rather than a free upgrade.

use std::fmt;

// -------------------------------------------------------------- port types

/// What a connection carries. Two ports may be wired only if these match.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum PortKind {
    Heat,
    Fluid,
    Steam,
    Rotary,
    Electrical,
}

impl PortKind {
    pub fn tag(self) -> &'static str {
        match self {
            PortKind::Heat => "heat",
            PortKind::Fluid => "fluid",
            PortKind::Steam => "steam",
            PortKind::Rotary => "rotary",
            PortKind::Electrical => "electrical",
        }
    }
}

impl fmt::Display for PortKind {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Dir {
    In,
    Out,
}

/// One socket on one component.
///
/// `rate` is how much may cross it in a tick; `cap` is how much may sit behind
/// it waiting. Those two numbers are the entire flow model. A component never
/// reaches through a port to ask a neighbour for anything: it fills its own
/// output buffers and drains its own input buffers, and the transfer stage
/// moves whatever both ends can afford.
pub struct Port {
    pub name: &'static str,
    pub kind: PortKind,
    pub dir: Dir,
    pub rate: u64,
    pub cap: u64,
    /// An external port is the machine's boundary: an unconnected outbound one
    /// exports rather than spills, and it is what a compiled macro-machine
    /// advertises to whatever places it.
    pub external: bool,
}

const fn port(name: &'static str, kind: PortKind, dir: Dir, rate: u64, cap: u64) -> Port {
    Port { name, kind, dir, rate, cap, external: false }
}

// -------------------------------------------------------------- components

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Kind {
    Reactor,
    HeatPipe,
    Pump,
    Exchanger,
    SteamPipe,
    Tank,
    Turbine,
    Generator,
}

pub const KINDS: [Kind; 8] = [
    Kind::Reactor,
    Kind::HeatPipe,
    Kind::Pump,
    Kind::Exchanger,
    Kind::SteamPipe,
    Kind::Tank,
    Kind::Turbine,
    Kind::Generator,
];

pub struct Part {
    pub kind: Kind,
    /// What it is called in a `.machine` file and on the wire.
    pub tag: &'static str,
    pub title: &'static str,
    /// The one sentence that says why you would place one.
    pub blurb: &'static str,
    /// Footprint, in tiles.
    pub w: u32,
    pub h: u32,
    pub ports: &'static [Port],
}

// The numbers. Everything downstream is derived from this table, so re-tuning
// the experiment is editing one screen of constants.

/// Ticks from cold to full output. A reactor burns fuel the whole way.
pub const WARMUP: u64 = 120;
/// Heat produced per tick at 100% throttle.
pub const REACTOR_HEAT: u64 = 1000;
/// Fuel burned per tick at 100% throttle.
pub const REACTOR_FUEL: u64 = 100;
/// Below this the reaction will not hold.
pub const MIN_THROTTLE: u32 = 20;
/// The fraction of everything a heat pipe carries that it loses to the room.
pub const PIPE_LOSS_PCT: u64 = 2;

/// Clear tiles between two components that a direct connection can still span.
///
/// Without this a pipe is a component with no reason to exist -- a reactor's
/// heat port reaches every exchanger on the plot for free, and the Heat Pipe is
/// a 2% tax nobody would ever volunteer for. With it, the tile grid is load
/// bearing: things that work together have to sit together, a pipe is how you
/// buy distance, and the price of distance is the loss. That is the same
/// sentence as "minimise footprint", which is why it belongs in the brief.
pub const REACH: i32 = 6;

/// Steam per tick below which a turbine will not turn over at all.
pub const TURBINE_MIN: u64 = 40;
/// Spin is an integer 0..=SPIN_MAX and output is proportional to it.
pub const SPIN_MAX: u32 = 30;
pub const SPIN_UP: u32 = 2;
pub const SPIN_DOWN: u32 = 1;
/// Rotary out of a fully spun turbine, as a percentage of steam in.
pub const TURBINE_EFF: u64 = 75;
/// MW out of a generator, as a percentage of rotary in.
pub const GENERATOR_EFF: u64 = 90;

/// 5 heat + 2 water makes 2 steam, which is the brief's 250/100/100 in the
/// smallest whole numbers a simulator can work in without ever rounding.
pub const BOIL_HEAT: u64 = 5;
pub const BOIL_WATER: u64 = 2;
pub const BOIL_STEAM: u64 = 2;

static REACTOR: [Port; 1] =
    [port("heat", PortKind::Heat, Dir::Out, REACTOR_HEAT, REACTOR_HEAT)];

static HEATPIPE: [Port; 2] = [
    port("in", PortKind::Heat, Dir::In, 400, 400),
    port("out", PortKind::Heat, Dir::Out, 400, 400),
];

static PUMP: [Port; 1] = [port("water", PortKind::Fluid, Dir::Out, 200, 400)];

static EXCHANGER: [Port; 3] = [
    port("heat", PortKind::Heat, Dir::In, 250, 500),
    port("water", PortKind::Fluid, Dir::In, 100, 200),
    port("steam", PortKind::Steam, Dir::Out, 100, 200),
];

static STEAMPIPE: [Port; 2] = [
    port("in", PortKind::Steam, Dir::In, 150, 150),
    port("out", PortKind::Steam, Dir::Out, 150, 150),
];

static TANK: [Port; 2] = [
    port("in", PortKind::Steam, Dir::In, 200, 2000),
    port("out", PortKind::Steam, Dir::Out, 200, 200),
];

static TURBINE: [Port; 2] = [
    // One tick of intake and not a drop more: steam that reaches a turbine and
    // is not used does not queue, it condenses. That one decision is what makes
    // the Steam Buffer a component rather than a decoration -- see `sim`.
    port("steam", PortKind::Steam, Dir::In, 80, 80),
    port("rotary", PortKind::Rotary, Dir::Out, 120, 120),
];

static GENERATOR: [Port; 2] = [
    port("rotary", PortKind::Rotary, Dir::In, 70, 70),
    Port {
        name: "power",
        kind: PortKind::Electrical,
        dir: Dir::Out,
        rate: 200,
        cap: 0,
        external: true,
    },
];

static PARTS: [Part; 8] = [
    Part {
        kind: Kind::Reactor,
        tag: "reactor",
        title: "Fuel / Heat Source",
        blurb: "burns fuel at its throttle setting whether or not the heat is wanted",
        w: 4,
        h: 4,
        ports: &REACTOR,
    },
    Part {
        kind: Kind::HeatPipe,
        tag: "heatpipe",
        title: "Heat Pipe",
        blurb: "carries 400 heat/tick, and loses 2% of it",
        w: 3,
        h: 1,
        ports: &HEATPIPE,
    },
    Part {
        kind: Kind::Pump,
        tag: "pump",
        title: "Water Source",
        blurb: "draws 200 water/tick from outside the machine",
        w: 2,
        h: 2,
        ports: &PUMP,
    },
    Part {
        kind: Kind::Exchanger,
        tag: "exchanger",
        title: "Heat Exchanger",
        blurb: "250 heat and 100 water makes 100 steam; short of either it makes less",
        w: 3,
        h: 3,
        ports: &EXCHANGER,
    },
    Part {
        kind: Kind::SteamPipe,
        tag: "steampipe",
        title: "Steam Pipe",
        blurb: "carries 150 steam/tick",
        w: 3,
        h: 1,
        ports: &STEAMPIPE,
    },
    Part {
        kind: Kind::Tank,
        tag: "tank",
        title: "Steam Buffer",
        blurb: "holds 2000 steam; in pulse mode it fills quietly and empties hard",
        w: 3,
        h: 3,
        ports: &TANK,
    },
    Part {
        kind: Kind::Turbine,
        tag: "turbine",
        title: "Turbine",
        blurb: "80 steam/tick at 75%, but stalls below 40 and spins up slowly",
        w: 3,
        h: 2,
        ports: &TURBINE,
    },
    Part {
        kind: Kind::Generator,
        tag: "generator",
        title: "Generator",
        blurb: "70 rotary/tick at 90%, so 63 MW and no more",
        w: 2,
        h: 2,
        ports: &GENERATOR,
    },
];

pub fn part(kind: Kind) -> &'static Part {
    &PARTS[kind as usize]
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

    /// The rate a component is judged against when reporting utilisation: the
    /// throughput of the thing it exists to do.
    pub fn rated(&self) -> u64 {
        match self.kind {
            Kind::Reactor => REACTOR_HEAT,
            Kind::Exchanger => 100,
            Kind::Turbine => 80,
            Kind::Generator => 70,
            _ => self.ports[0].rate,
        }
    }
}

impl Kind {
    pub fn tag(self) -> &'static str {
        part(self).tag
    }
}
