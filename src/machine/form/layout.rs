//! Semantic 3D layout: the first pass, and the one that decides whether any of
//! the rest can look like anything.
//!
//! The designer's document is two-dimensional on purpose -- footprint is one of
//! the things a brief asks the player to minimise, so where a component sits is
//! part of the design rather than part of the drawing. This pass takes that
//! plan and gives every component the four things the later passes need:
//!
//! ```text
//!   a volume        how big it is, and how far off the ground
//!   an orientation  which way it faces
//!   sockets         where each typed port actually is, in space
//!   a clearance     how much room around it is not available to pipework
//! ```
//!
//! # The third dimension was inferred; now it is authored
//!
//! Experiment 08 took a two-dimensional document and *inferred* elevation from
//! what each component was -- a cyclone discharges downwards so it stands on
//! legs, a shaft has to line up with the shaft it drives so every rotary
//! socket in the plant is at the same height. That was the right first answer,
//! and it is still the default, because a component the player has not lifted
//! stands exactly where experiment 08 put it.
//!
//! Experiment 10 adds the other half: `up`. The document now says how many
//! tiles off the slab a component sits, and this pass believes it. So the
//! sentence that used to read
//!
//! ```text
//!   height = f(what it is)
//! ```
//!
//! now reads
//!
//! ```text
//!   height = up * TILE + f(what it is)
//! ```
//!
//! which is the smallest change that makes the note's actual request true:
//! *the player chooses whether the turbine sits beside the exchanger, above
//! it, or on a separate level.*
//!
//! # A port is an interface, not a coordinate
//!
//! This is the larger of experiment 10's two changes to this file, and the one
//! the pipework notices.
//!
//! Experiment 08 chose a port's face by looking at what it was wired to and
//! picking the nearest side. That produces pipework that reads correctly at a
//! glance and nonsense on inspection: a steam outlet would appear on whichever
//! wall happened to face the turbine, including the floor, and a shaft would
//! leave a motor out of its side.
//!
//! Now every archetype declares, per domain and direction, *which of its six
//! faces that port is allowed to be on*:
//!
//! ```text
//!   a can's shaft leaves the end of the barrel, and only the end
//!   a vessel's gas leaves the top, and only the top
//!   a shell's process ports are on the tube ends
//!   a bin takes material in at the top and drops it out of the bottom
//!   a turbine exhausts downwards and drives forwards
//! ```
//!
//! The old nearest-side rule still runs -- but only *within* the allowed set.
//! So two components wired together still put their sockets on the faces
//! nearest each other whenever the archetype leaves a choice, and where it
//! does not, the machine has to be turned round instead. Which is exactly what
//! `face` is for, and why the two halves of this experiment are one change:
//! rotation is only interesting if something is bolted to the thing rotating.
//!
//! # Heights, and why they are a table
//!
//! ```text
//!   rotary   1400   every shaft in the plant is at the same height, so a line
//!                   shaft is a straight line and a coupling is believable
//!   fluid     700   pumps push along the floor
//!   gas      high   steam leaves the top of a shell
//!   heat     high   and so does heat, on a rack
//!   material top in, bottom out -- an ore line visibly falls downhill
//! ```
//!
//! Those five lines are why a stranger can read the flow of a plant they have
//! never seen: not because the pipes are labelled, but because everything in a
//! domain agrees about where it lives.

use super::seed::hash;
use super::{p3, Mm, P3, TILE, Vol};
use crate::machine::design::{Design, Unit};
use crate::machine::parts::{self, Dir, Kind};
use crate::machine::stuff::Domain;

/// What sort of object a component is, once you stop caring what it does. This
/// is the only place the thirty-eight kinds are collapsed, and it is why the
/// asset library is twenty-five meshes rather than thirty-eight models.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Arch {
    /// A vertical pressure vessel: reactor, tank, drum on end.
    Vessel,
    /// A horizontal shell on saddles: exchanger, condenser, mill.
    Shell,
    /// A machine on a bedplate, in a box: gearbox, press, lathe, crusher.
    Skid,
    /// An open portal frame with the works in the middle: press, rolling mill.
    Portal,
    /// A barrel with an end bell: motor, generator, pump.
    Can,
    /// Something that discharges downwards: hopper, cyclone, chute.
    Bin,
    /// A pad, a kerb, a boundary: inlet, outlet, skip, mains.
    Pad,
    /// Tall and thin, with platforms: the distillation column.
    Tower,
    /// A disc on pedestals: flywheel, crank.
    Wheel,
    /// A fitting in a line rather than a machine: valve, clutch.
    Inline,
    /// A component that *is* its connection: the six transports.
    Run,
    /// Fins to the weather: radiator.
    Bank,
    /// A casing with a rotor and an exhaust: the turbine.
    Turbine,
}

/// How a component meets the ground.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Mount {
    /// Straight onto the slab.
    Grade,
    /// On a concrete plinth: heavy, or wet, or both.
    Plinth,
    /// On legs, because something has to fit underneath it.
    Legs,
    /// On a steel frame, at working height.
    Frame,
}

/// One component, placed.
#[derive(Clone, Debug)]
pub struct Placed {
    pub name: String,
    pub kind: Kind,
    pub arch: Arch,
    pub mount: Mount,
    /// Quarter turns clockwise from east: which way the machine faces. Authored
    /// when the player said, and otherwise inferred from what it is wired to.
    pub yaw: u8,
    /// Whether that was the player's decision. A machine the player turned is a
    /// machine nothing else may turn back.
    pub turned: bool,
    /// The tile footprint, as the design gave it.
    pub tile: (i32, i32, i32, i32),
    /// Experiment 10: which tile of the `up` axis it stands on, and how high
    /// the deck under it therefore is. Zero is the slab.
    pub level: i32,
    pub base: Mm,
    /// The body itself, off its deck by `lift`.
    pub vol: Vol,
    /// How high the underside sits above its deck.
    pub lift: Mm,
    /// The volume nothing else may route through.
    pub clear: Vol,
    pub sockets: Vec<Socket>,
}

/// A typed port, in space.
///
/// Experiment 10 turned this from a point into an interface. The note put it
/// better than the struct does:
///
/// > The procedural generator should understand interfaces, not merely
/// > endpoints.
///
/// Everything below `out` is what the router is now obliged to respect: how
/// far the line has to run straight before it may turn, what class of fitting
/// bolts to it, and which of the plant's elevations it belongs on.
#[derive(Clone, Copy, Debug)]
pub struct Socket {
    pub port: usize,
    pub dom: Domain,
    pub dir: Dir,
    /// On the surface of the component.
    pub at: P3,
    /// Which way it faces: the direction a pipe leaves in, normal to the flange.
    pub out: P3,
    /// The bore of whatever connects to it, from the port's rate. A 400/tick
    /// heat main is visibly a main; a 20/tick drive is visibly a drive.
    pub bore: Mm,
    /// How much straight pipe has to leave the flange before anything may bend.
    /// A line that turns the instant it clears the shell looks like a mistake
    /// because it is one.
    pub stub: Mm,
    /// What bolts to it, which decides how big the flange is and how tight a
    /// bend the line is allowed.
    pub class: Press,
    /// Where a line off this port would rather travel once it is clear of the
    /// machine: the plant's own storeys, not an arbitrary height.
    pub layer: Layer,
    /// For a rotary or mechanical port, the axis the coupling has to lie on.
    /// Shafts are held to a much stricter rule than pipes, and this is it.
    pub axis: Option<P3>,
}

impl Placed {
    pub fn socket(&self, port: usize) -> Option<&Socket> {
        self.sockets.iter().find(|s| s.port == port)
    }
    pub fn top(&self) -> Mm {
        self.vol.hi.y
    }
}

pub struct Plan {
    pub units: Vec<Placed>,
    /// Every component's footprint, joined: the plot, in millimetres.
    pub plot: Vol,
}

impl Plan {
    pub fn find(&self, name: &str) -> Option<&Placed> {
        self.units.iter().find(|u| u.name == name)
    }
    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.units.iter().position(|u| u.name == name)
    }
}

// ---------------------------------------------------------------- elevations

/// The plant's storeys.
///
/// Experiment 10, and one of the cheapest good ideas in the note: industrial
/// routing has conventions, and conventions both make the output believable
/// *and* constrain the search space. A line does not travel at whatever height
/// A* found convenient -- it climbs to its layer, runs along it, and comes
/// down at the far end. Five of them, because material has to fall and the
/// note's four had nowhere for it to fall to.
///
/// ```text
///   Ground   700   pumped services, along the floor
///   Drive   1250   shafts and rods: every one in the plant at one height
///   Feed    2750   chutes and conveyors, above head height and falling
///   Rack    4250   the process rack: steam, heat, anything hot
///   Tray    5600   cable tray, over the top of everything else
/// ```
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Layer {
    Ground,
    Drive,
    Feed,
    Rack,
    Tray,
}

pub const LAYERS: [Layer; 5] = [Layer::Ground, Layer::Drive, Layer::Feed, Layer::Rack, Layer::Tray];

impl Layer {
    pub fn y(self) -> Mm {
        match self {
            Layer::Ground => FLUID_Y,
            Layer::Drive => SHAFT_Y,
            Layer::Feed => FEED_Y,
            Layer::Rack => RACK_Y,
            Layer::Tray => TRAY_Y,
        }
    }
    pub fn tag(self) -> &'static str {
        match self {
            Layer::Ground => "ground",
            Layer::Drive => "drive",
            Layer::Feed => "feed",
            Layer::Rack => "rack",
            Layer::Tray => "tray",
        }
    }
    /// Which storey a domain belongs on. The single table that replaced seven
    /// separately-chosen numbers, and the reason a plant now has *lines* of
    /// pipework in it rather than a cloud.
    pub fn of(dom: Domain) -> Layer {
        match dom {
            Domain::Fluid => Layer::Ground,
            Domain::Rotary | Domain::Mech => Layer::Drive,
            Domain::Material => Layer::Feed,
            Domain::Gas | Domain::Heat => Layer::Rack,
            Domain::Electrical => Layer::Tray,
        }
    }
}

/// What bolts to a port.
///
/// The note asked a port to carry a `pressureClass`, and the honest version of
/// that in a game with no pressures in it is *how serious the connection is*,
/// which the document already says: a 400/tick heat main and a 20/tick drive
/// are not the same fitting. So it is derived rather than authored -- one less
/// number for a player to be asked about, and one that can never disagree with
/// the simulation it came from.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Press {
    /// Slip-on, small, bends where it likes.
    Light,
    /// Bolted, and wants a straight run either side of a fitting.
    Standard,
    /// A main. Long radius bends, long straights, and a lot of steel.
    Heavy,
}

impl Press {
    pub fn tag(self) -> &'static str {
        match self {
            Press::Light => "light",
            Press::Standard => "standard",
            Press::Heavy => "heavy",
        }
    }
    pub fn of(dom: Domain, rate: u64) -> Press {
        match dom {
            // A shaft is not a pressure class at all, but it is the strictest
            // interface in the plant, so it takes the strictest row.
            Domain::Rotary | Domain::Mech => Press::Heavy,
            Domain::Electrical => Press::Light,
            _ if rate >= 300 => Press::Heavy,
            _ if rate >= 80 => Press::Standard,
            _ => Press::Light,
        }
    }
    /// The straight run this class wants off a flange before anything is
    /// allowed to bend, for a pipe of this bore.
    ///
    /// In half-bore-widths, and then clamped hard, because the honest
    /// engineering answer -- four or five diameters -- is longer than the gaps
    /// between machines in a plant whose plan is drawn on two-metre tiles. Half
    /// a metre to a metre is enough to read as *bolted to a flange* and short
    /// enough to fit between two machines standing next to each other, which is
    /// the whole population of cases this rule exists to make look right.
    pub fn stub(self, bore: Mm) -> Mm {
        let n = match self {
            Press::Light => 2,
            Press::Standard => 3,
            Press::Heavy => 4,
        };
        // Rounded up to the router's own half-metre. A stub the grid cannot
        // represent is a stub the router quietly shortens, and a rule that is
        // quietly shortened is not a rule -- so the number the port asks for
        // is a number the router can give it exactly.
        let want = ((bore * n) / 2).clamp(400, 1000);
        ((want + LANE - 1) / LANE) * LANE
    }
}

// ------------------------------------------------------------- the numbers

/// Where a shaft is, and every shaft is at the same height. This one constant
/// is most of why a drive train reads as a drive train.
pub const SHAFT_Y: Mm = 1250;
/// Where a pipe rack is. Heat and steam live up here.
pub const RACK_Y: Mm = 4250;
/// Working height for a fluid line.
pub const FLUID_Y: Mm = 750;
/// Where a conveyor or chute hands material over.
pub const FEED_Y: Mm = 2750;
/// Over the top of all of it: cable tray.
pub const TRAY_Y: Mm = 5600;

/// The router works on a half-metre grid whose cells are centred on the odd
/// quarter-metres, and a socket that is not on one of those lines makes every
/// run start with a diagonal jog. So sockets are snapped to the same lines --
/// by at most a quarter of a metre, which the stub on the outside of the
/// machine covers, and which buys pipework that is orthogonal from the first
/// millimetre to the last.
pub const LANE: Mm = 500;

pub fn lane(v: Mm) -> Mm {
    let h = LANE / 2;
    ((v - h).div_euclid(LANE)) * LANE + h
}

/// The nearest lane rather than the one below.
///
/// Used for the point a socket actually sits at, because a socket is on a
/// *surface* and rounding it always downwards walks it up to half a metre into
/// the machine. Rounding to the nearest moves it by at most a quarter, in
/// whichever direction is shorter, and nobody has ever noticed a flange a
/// finger's width proud of a shell. What everybody notices is the alternative:
/// a socket off the router's grid puts a diagonal in the first section of
/// every line that leaves it.
pub fn lane_near(v: Mm) -> Mm {
    lane(v + LANE / 2)
}

fn on_grid(p: P3) -> P3 {
    p3(lane_near(p.x), lane_near(p.y), lane_near(p.z))
}

/// The gap left between a component's body and the edge of its tiles, so two
/// components that touch on the plan do not merge into one object.
const INSET: Mm = 250;
/// How much room a machine needs around it.
const CLEAR: Mm = 700;

/// A plinth is this tall, and a component on legs stands this high.
const PLINTH: Mm = 400;
const LEGS: Mm = 2400;
const FRAME: Mm = 3200;

// ------------------------------------------------------------- the archetype

/// What a component is, how it stands, and how tall it is. Thirty-eight rows,
/// and the whole of experiment 08's opinion about what machinery looks like.
///
/// Height is not derived from footprint on purpose. A 2x2 tank and a 2x2
/// gearbox are the same square metre of plan and profoundly different objects,
/// and a plant where everything is as tall as it is wide looks like a city.
pub fn shape(k: Kind) -> (Arch, Mount, Mm) {
    (arch(k), mount(k), parts::height(k) as Mm)
}

/// What sort of object each of the thirty-eight is.
pub fn arch(k: Kind) -> Arch {
    use Arch::*;
    use Kind::*;
    match k {
        Reactor | Tank => Vessel,
        Burner | Furnace | Gearbox | Crusher | Lathe => Skid,
        Heater | Pump | Generator | Motor => Can,
        Mains | Outlet | Skip => Pad,
        Inlet | Hopper | Separator => Bin,
        Radiator => Bank,
        HeatPipe | SteamPipe | FluidPipe | Chute | Screw | Shaft | Cable => Run,
        Drum | Exchanger | Preheater | Condenser | Mill => Shell,
        Flywheel | Crank => Wheel,
        Valve | Clutch => Inline,
        Kind::Turbine => Arch::Turbine,
        RollMill | Press => Portal,
        Column => Tower,
    }
}

/// How each of them meets the ground. The numbers are in the catalogue, next
/// to the heights, because since experiment 10 the document needs both to say
/// whether two components clash.
pub fn mount(k: Kind) -> Mount {
    match parts::lift(k) {
        0 => Mount::Grade,
        v if v == Mount::Plinth.lift() as u32 => Mount::Plinth,
        v if v == Mount::Legs.lift() as u32 => Mount::Legs,
        _ => Mount::Frame,
    }
}

impl Arch {
    pub fn tag(self) -> &'static str {
        match self {
            Arch::Vessel => "vessel",
            Arch::Shell => "shell",
            Arch::Skid => "skid",
            Arch::Portal => "portal",
            Arch::Can => "can",
            Arch::Bin => "bin",
            Arch::Pad => "pad",
            Arch::Tower => "tower",
            Arch::Wheel => "wheel",
            Arch::Inline => "inline",
            Arch::Run => "run",
            Arch::Bank => "bank",
            Arch::Turbine => "turbine",
        }
    }

    /// Whether this thing is heavy enough to be worth a foundation of its own.
    /// The structural pass turns this into concrete.
    pub fn heavy(self) -> bool {
        matches!(self, Arch::Vessel | Arch::Shell | Arch::Skid | Arch::Portal | Arch::Turbine | Arch::Tower)
    }
}

impl Mount {
    pub fn tag(self) -> &'static str {
        match self {
            Mount::Grade => "grade",
            Mount::Plinth => "plinth",
            Mount::Legs => "legs",
            Mount::Frame => "frame",
        }
    }
    pub fn lift(self) -> Mm {
        match self {
            Mount::Grade => 0,
            Mount::Plinth => PLINTH,
            Mount::Legs => LEGS,
            Mount::Frame => FRAME,
        }
    }
}

// ---------------------------------------------------------------- nozzles

/// One of a machine's six faces, in its own frame: `Front` is the way it
/// faces, whatever the world thinks about it.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Side {
    Front,
    Back,
    Left,
    Right,
    Top,
    Bottom,
}

pub const SIDES: [Side; 6] = [Side::Front, Side::Back, Side::Left, Side::Right, Side::Top, Side::Bottom];

impl Side {
    pub fn tag(self) -> &'static str {
        match self {
            Side::Front => "front",
            Side::Back => "back",
            Side::Left => "left",
            Side::Right => "right",
            Side::Top => "top",
            Side::Bottom => "bottom",
        }
    }
    /// Where this face points once the machine has been turned.
    pub fn world(self, yaw: u8) -> P3 {
        match self {
            Side::Top => super::UP,
            Side::Bottom => super::DOWN,
            Side::Front => face(yaw),
            Side::Back => face(yaw + 2),
            Side::Right => face(yaw + 1),
            Side::Left => face(yaw + 3),
        }
    }
    pub fn horizontal(self) -> bool {
        !matches!(self, Side::Top | Side::Bottom)
    }
}

/// Which faces of an archetype a port of this domain and direction is allowed
/// to be on.
///
/// This is experiment 10's answer to the note, and the whole of it fits on a
/// page. Read it as a specification of the *machine* rather than of the
/// picture: a can is a barrel with a shaft out of the end, so the shaft leaves
/// the end; a bin discharges downwards, so material leaves the bottom. The
/// router is then obliged to leave along the face, normally, for a distance
/// the class decides -- which is the difference between a pipe bolted to a
/// flange and a pipe that has been pushed through the shell.
///
/// The list is in preference order. Where it has more than one entry, the old
/// experiment-08 rule chooses between them by looking at the partner; where it
/// has one, the only way to change the answer is to turn the machine round.
pub fn nozzle(arch: Arch, dom: Domain, dir: Dir) -> &'static [Side] {
    use Arch::*;
    use Domain::*;
    use Side::*;
    let out = dir == Dir::Out;
    match (arch, dom) {
        // A shaft leaves the end of the barrel. There is no other answer, and
        // pretending otherwise is what made experiment 08's drive trains look
        // like they had been assembled in the dark.
        (Can | Wheel | Skid | Portal | Arch::Turbine, Rotary | Mech) => {
            if out {
                &[Front, Back]
            } else {
                &[Back, Front]
            }
        }
        (_, Rotary | Mech) => &[Front, Back],

        // A vessel vents upwards, takes its heat in the side and drains low.
        (Vessel, Gas | Heat) if out => &[Top],
        (Vessel, Gas | Heat) => &[Left, Right, Back, Front, Top],
        (Vessel, Fluid) => &[Left, Right, Back, Front],
        (Vessel, Material) if out => &[Bottom, Left, Right, Back, Front],
        (Vessel, Material) => &[Top],

        // A shell is a tube bundle: the process is in one end and out of the
        // other, and the service ports are on top of it.
        // The tube ends first, and the top of the shell when both ends are
        // pressed against the next machine. A nozzle on the crown of a shell
        // is not a compromise, it is what one looks like.
        (Shell, Gas | Heat) => {
            if out {
                &[Front, Back, Top]
            } else {
                &[Back, Front, Top]
            }
        }
        (Shell, Fluid) if out => &[Bottom, Left, Right],
        (Shell, Fluid) => &[Top, Left, Right],
        (Shell, Material) if out => &[Bottom, Left, Right, Back, Front],
        (Shell, Material) => &[Top],

        // A turbine takes steam in at the top and exhausts it downwards,
        // which is most of why one is shaped the way it is.
        (Arch::Turbine, Gas | Heat) if out => &[Bottom, Back],
        (Arch::Turbine, Gas | Heat) => &[Top, Back, Left, Right],
        (Arch::Turbine, Fluid) => &[Bottom, Back, Left, Right],

        // A bin is a funnel. In at the top, out of the bottom, and the whole
        // point of it is that gravity does the work.
        (Bin, Material) if out => &[Bottom],
        (Bin, Material) => &[Top],
        (Bin, _) => &[Left, Right, Back, Front],

        // A column is a tall vessel: vapour off the top, bottoms off the
        // bottom, feed in the side.
        (Tower, Gas) if out => &[Top],
        (Tower, Fluid) if out => &[Bottom, Left, Right],
        (Tower, Material) if out => &[Bottom, Left, Right],
        (Tower, _) => &[Left, Right, Back, Front],

        // A radiator takes heat in at the back and loses it upwards.
        (Bank, Heat) if out => &[Top, Front],
        (Bank, Heat) => &[Back, Left, Right, Top],

        // A fitting in a line, and a component that *is* a line, both take the
        // line in one end and pass it out of the other.
        (Inline | Run, _) => {
            if out {
                &[Front]
            } else {
                &[Back]
            }
        }

        // Electricity is not fussy, but it is consistent: up and over.
        (_, Electrical) => &[Top, Left, Right, Back, Front],
        // Material falls, wherever it is.
        (_, Material) if out => &[Bottom, Left, Right, Front, Back],
        (_, Material) => &[Top, Left, Right, Back, Front],
        // Steam and heat rise.
        (_, Gas | Heat) if out => &[Top, Front, Left, Right, Back],
        (_, Gas | Heat) => &[Top, Back, Left, Right, Front],
        // And water is pumped along the floor.
        (_, Fluid) => &[Left, Right, Front, Back],
    }
}

// -------------------------------------------------------------------- yaw

/// Which way a component faces: along the flow through it, snapped to a
/// quarter turn.
///
/// The vector is from the middle of everything that feeds it to the middle of
/// everything it feeds, which is the closest thing a functional design has to
/// an opinion about direction. A component with neither is left facing east,
/// and a tie goes to the earlier direction -- both of those are arbitrary, and
/// both have to be *fixed*, because a scene that rebuilds differently is not a
/// scene, it is a bug with a camera.
///
/// Experiment 10 weights each wire by the *rate* its port carries, and that
/// one change is worth the whole paragraph. A heat exchanger's axis is set by
/// the five hundred units of heat going through it, not by the hundred of
/// cooling water trickling in from one side -- and now that the tube ends are
/// pinned to that axis, getting it wrong no longer merely looks odd, it puts
/// the steam outlet on a face with nothing in front of it.
fn yaw_of(d: &Design, i: usize) -> u8 {
    // Experiment 10: if the player turned it, it is turned. Inference is what
    // happens to a component nobody has an opinion about, which is still most
    // of them.
    if let Some(f) = d.units[i].face {
        return f & 3;
    }
    let me = centre_tile(&d.units[i]);
    let (mut ax, mut az, mut n) = (0i64, 0i64, 0i64);
    for w in &d.wires {
        let (from, to) = (d.index_of(&w.from), d.index_of(&w.to));
        let (from, to) = match (from, to) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        // How much this wire is worth having an opinion about.
        let w8 = parts::part(d.units[from].kind)
            .port_index(&w.from_port)
            .map(|k| parts::part(d.units[from].kind).ports[k].rate.max(1) as i64)
            .unwrap_or(1);
        if from == i {
            let p = centre_tile(&d.units[to]);
            ax += (p.0 - me.0) as i64 * w8;
            az += (p.1 - me.1) as i64 * w8;
            n += 1;
        } else if to == i {
            let p = centre_tile(&d.units[from]);
            ax += (me.0 - p.0) as i64 * w8;
            az += (me.1 - p.1) as i64 * w8;
            n += 1;
        }
    }
    if n == 0 {
        return 0;
    }
    if ax.abs() >= az.abs() {
        if ax >= 0 {
            0
        } else {
            2
        }
    } else if az >= 0 {
        1
    } else {
        3
    }
}

fn centre_tile(u: &Unit) -> (i32, i32) {
    (u.x * 2 + u.w(), u.y * 2 + u.h())
}

/// East, south, west, north.
pub fn face(yaw: u8) -> P3 {
    match yaw & 3 {
        0 => super::EAST,
        1 => super::SOUTH,
        2 => super::WEST,
        _ => super::NORTH,
    }
}

// ----------------------------------------------------------------- the pass

/// The plan, in three dimensions.
pub fn plan(d: &Design) -> Plan {
    let mut units: Vec<Placed> = Vec::with_capacity(d.units.len());
    for (i, u) in d.units.iter().enumerate() {
        let (arch, mount, h) = shape(u.kind);
        let lift = mount.lift();
        // Experiment 10: the deck the player put it on. Zero is the slab, and
        // a design that never says `up` is a design experiment 08 would build
        // exactly as it always did.
        let base = u.z * TILE;
        let (x0, z0) = (u.x * TILE, u.y * TILE);
        let (x1, z1) = (x0 + u.w() * TILE, z0 + u.h() * TILE);
        // A transport component is its own connection, so it gets a slender
        // volume down the middle of its tiles rather than a body filling them.
        let vol = if arch == Arch::Run {
            let mid = p3((x0 + x1) / 2, 0, (z0 + z1) / 2);
            let along = (x1 - x0) >= (z1 - z0);
            let half = if along { (x1 - x0) / 2 - INSET } else { (z1 - z0) / 2 - INSET };
            let (dx, dz) = if along { (half, 400) } else { (400, half) };
            // Centred on the height its domain lives at, so that the ports of
            // a line shaft are at exactly the height of the ports it joins --
            // unless the player has lifted it, in which case they have said
            // where the run goes and this pass has nothing to add.
            let y = if u.z > 0 { base + TILE / 2 } else { run_height(u.kind) };
            Vol::new(p3(mid.x - dx, y - 250, mid.z - dz), p3(mid.x + dx, y + 250, mid.z + dz))
        } else {
            Vol::new(
                p3(x0 + INSET, base + lift, z0 + INSET),
                p3(x1 - INSET, base + lift + h, z1 - INSET),
            )
        };
        units.push(Placed {
            name: u.name.clone(),
            kind: u.kind,
            arch,
            mount,
            yaw: yaw_of(d, i),
            turned: u.face.is_some(),
            tile: (u.x, u.y, u.w(), u.h()),
            level: u.z,
            base,
            vol,
            lift,
            clear: vol.grow_flat(CLEAR),
            sockets: Vec::new(),
        });
    }

    // Sockets come second because a socket's face depends on where its partner
    // ended up, and its partner is only placed once everything is.
    for i in 0..units.len() {
        units[i].sockets = sockets(d, &units, i);
    }

    let plot = units.iter().fold(None::<Vol>, |acc, u| {
        let v = Vol::new(
            p3(u.tile.0 * TILE, 0, u.tile.1 * TILE),
            p3((u.tile.0 + u.tile.2) * TILE, u.vol.hi.y.max(u.base), (u.tile.1 + u.tile.3) * TILE),
        );
        Some(match acc {
            None => v,
            Some(a) => a.join(v),
        })
    });
    Plan { units, plot: plot.unwrap_or(Vol::new(p3(0, 0, 0), p3(TILE, TILE, TILE))) }
}

/// The height a transport component's own run sits at: the same height as
/// everything else in its domain, which is what makes a line shaft straight.
fn run_height(k: Kind) -> Mm {
    match k {
        Kind::Shaft => SHAFT_Y,
        Kind::Cable => RACK_Y - 800,
        Kind::HeatPipe | Kind::SteamPipe => RACK_Y,
        Kind::FluidPipe => FLUID_Y,
        Kind::Chute | Kind::Screw => FEED_Y,
        _ => SHAFT_Y,
    }
}

/// Where each port of one component ends up.
///
/// Two passes, because a face is chosen per port and the *position on* it
/// depends on how many other ports chose the same face. Nothing here consults
/// the seed: a socket is a fact about the machine, and two builds of one
/// design have to put the flange in the same place.
fn sockets(d: &Design, units: &[Placed], i: usize) -> Vec<Socket> {
    let me = &units[i];
    let part = parts::part(me.kind);
    let mut out: Vec<Socket> = Vec::new();

    // Which face each port wants, before crowding is taken into account.
    let mut want: Vec<(usize, Side, Mm)> = Vec::new();
    let mut peers: Vec<Option<&Placed>> = Vec::new();
    for (pi, port) in part.ports.iter().enumerate() {
        let peer = peer_of(d, units, &me.name, port.name, port.dir);
        let stub = Press::of(port.dom, port.rate).stub(bore(port.rate));
        let side = choose_side(me, units, i, port.dom, port.dir, peer, stub);
        want.push((pi, side, height_for(me, port.dom, port.dir, side)));
        peers.push(peer);
    }

    for (pi, side, y) in want.iter().copied() {
        let port = &part.ports[pi];
        // Ports sharing a face get spread along it, in port order, so two
        // steam lines off one shell do not leave from the same square inch.
        let mates: Vec<usize> = want
            .iter()
            .filter(|(_, g, gy)| *g == side && (*gy - y).abs() < 400)
            .map(|(k, _, _)| *k)
            .collect();
        let slot = mates.iter().position(|&k| k == pi).unwrap_or(0) as i32;
        let n = mates.len() as i32;
        let f = side.world(me.yaw);
        let mut at = on_face(me, side, f, y, slot, n);
        // A drive is the one connection that has to be a straight line, so its
        // two ends are put on one: both sockets snap to the midpoint of the
        // two machines' centres, which is a decision each end can make on its
        // own and still agree about. Whatever is left after each of them
        // clamps it to its own face is real misalignment, and `space` says so.
        if matches!(port.dom, Domain::Rotary | Domain::Mech) {
            if let Some(peer) = peers[pi] {
                at = onto_axis(me, side, at, peer);
            }
        }
        let bore = bore(port.rate);
        let class = Press::of(port.dom, port.rate);
        out.push(Socket {
            port: pi,
            dom: port.dom,
            dir: port.dir,
            at,
            out: f,
            bore,
            // The straight run off the flange, in bore-widths, and never less
            // than one cell of the router's grid -- a stub the router cannot
            // represent is a rule that quietly does not apply.
            stub: class.stub(bore),
            class,
            layer: Layer::of(port.dom),
            axis: match port.dom {
                Domain::Rotary | Domain::Mech => Some(face(me.yaw)),
                _ => None,
            },
        });
    }
    out
}

/// The component on the other end of the first wire attached to this port, if
/// there is one.
fn peer_of<'a>(
    d: &Design,
    units: &'a [Placed],
    name: &str,
    port: &str,
    dir: Dir,
) -> Option<&'a Placed> {
    for w in &d.wires {
        let hit = if dir == Dir::Out {
            w.from == name && w.from_port == port
        } else {
            w.to == name && w.to_port == port
        };
        if hit {
            let other = if dir == Dir::Out { &w.to } else { &w.from };
            return units.iter().find(|u| &u.name == other);
        }
    }
    None
}

/// Which of the six faces a port leaves by.
///
/// Experiment 08 asked only "which side is the partner on". Experiment 10 asks
/// the archetype first -- `nozzle` says which faces this kind of machine is
/// even willing to put this kind of port on -- and only then, among whatever
/// is left, asks about the partner. Most of the time that is still the same
/// answer. When it is not, the difference is a shaft that leaves the end of a
/// motor instead of the middle of its side, and the player's remedy is to turn
/// the motor round rather than to hope.
#[allow(clippy::too_many_arguments)]
fn choose_side(
    me: &Placed,
    units: &[Placed],
    mine: usize,
    dom: Domain,
    dir: Dir,
    peer: Option<&Placed>,
    stub: Mm,
) -> Side {
    let allowed = nozzle(me.arch, dom, dir);
    let first = *allowed.first().unwrap_or(&Side::Front);

    // Among the faces the machine allows, the one whose outward normal points
    // most nearly at the partner. Ties go to the earlier entry, which is the
    // archetype's own preference -- so an unhelpful partner never overrules
    // "steam leaves the top".
    let c = me.vol.centre();
    let to = peer.map(|p| p.vol.centre().sub(c)).unwrap_or(face(me.yaw).mul(1000));
    let mut best: Option<(i64, Side)> = None;
    let mut fallback: Option<(i64, Side)> = None;
    for (i, &side) in allowed.iter().enumerate() {
        let n = side.world(me.yaw);
        // Dot product, less a small bias so that earlier entries win ties.
        let dot = (n.x as i64) * (to.x as i64)
            + (n.y as i64) * (to.y as i64)
            + (n.z as i64) * (to.z as i64)
            - i as i64;
        if fallback.is_none_or(|b| dot > b.0) {
            fallback = Some((dot, side));
        }
        // A nozzle on a face that is pressed against the next machine is a
        // nozzle nothing can be bolted to. Experiment 08 never noticed,
        // because it let the pipe turn the instant it left the shell; now that
        // the straight off a flange is a rule, a blocked face is a *lost
        // connection*, so the face is not offered in the first place.
        // A coupling is the one connection that *wants* its partner up
        // against the flange: a turbine and the generator bolted to the end
        // of it are half a metre apart on purpose. So the partner is not an
        // obstacle to a shaft, only to a pipe -- which needs somewhere to go.
        let couples = matches!(dom, Domain::Rotary | Domain::Mech);
        if blocked(me, units, mine, side, stub, if couples { peer } else { None }) {
            continue;
        }
        if best.is_none_or(|b| dot > b.0) {
            best = Some((dot, side));
        }
    }
    // Every face is blocked: take the best of a bad lot rather than inventing
    // a seventh side of a box.
    best.or(fallback).map(|b| b.1).unwrap_or(first)
}

/// The least air a face needs in front of it to be worth putting a nozzle on:
/// half a tile.
///
/// The stub alone is not enough of a test. A four-hundred-millimetre stub fits
/// in a five-hundred-millimetre slot between two machines and then has nowhere
/// at all to go, which is a lost connection dressed up as a legal one. Half a
/// tile is the smallest gap the router's grid can actually turn a pipe in.
const ROOM: Mm = 1000;

/// Whether the straight run off one face would come out inside something else.
fn blocked(
    me: &Placed,
    units: &[Placed],
    mine: usize,
    side: Side,
    stub: Mm,
    ignore: Option<&Placed>,
) -> bool {
    let n = side.world(me.yaw);
    let v = me.vol;
    let stub = stub.max(ROOM);
    // The slab of air the stub needs: off the whole face, `stub` deep, which
    // is deliberately coarser than the pipe. A nozzle wants elbow room, not a
    // tube of exactly its own diameter.
    let a = match (n.x, n.y, n.z) {
        (x, _, _) if x > 0 => Vol::new(p3(v.hi.x, v.lo.y, v.lo.z), p3(v.hi.x + stub, v.hi.y, v.hi.z)),
        (x, _, _) if x < 0 => Vol::new(p3(v.lo.x - stub, v.lo.y, v.lo.z), p3(v.lo.x, v.hi.y, v.hi.z)),
        (_, y, _) if y > 0 => Vol::new(p3(v.lo.x, v.hi.y, v.lo.z), p3(v.hi.x, v.hi.y + stub, v.hi.z)),
        (_, y, _) if y < 0 => Vol::new(p3(v.lo.x, v.lo.y - stub, v.lo.z), p3(v.hi.x, v.lo.y, v.hi.z)),
        (_, _, z) if z > 0 => Vol::new(p3(v.lo.x, v.lo.y, v.hi.z), p3(v.hi.x, v.hi.y, v.hi.z + stub)),
        _ => Vol::new(p3(v.lo.x, v.lo.y, v.lo.z - stub), p3(v.hi.x, v.hi.y, v.lo.z)),
    };
    // Downwards is special: the slab is also in the way, and a nozzle in the
    // floor is no more use than a nozzle in a wall.
    if n.y < 0 && a.lo.y < 0 {
        return true;
    }
    units
        .iter()
        .enumerate()
        .any(|(j, o)| j != mine && ignore.is_none_or(|p| p.name != o.name) && o.vol.hits(a))
}

/// How high up the face a port sits, by domain. The five lines in this
/// function are the plant's grammar.
fn height_for(me: &Placed, dom: Domain, dir: Dir, side: Side) -> Mm {
    // A transport component *is* its domain's height: both its ports sit on
    // the axis of the run it draws.
    if me.arch == Arch::Run {
        return me.vol.centre().y;
    }
    let (lo, hi) = (me.vol.lo.y, me.vol.hi.y);
    if !side.horizontal() {
        return if side == Side::Top { hi } else { lo };
    }
    let span = (hi - lo).max(600);
    let clamp = |y: Mm| y.clamp(lo + 250, hi - 250);
    match dom {
        // Every shaft in the plant at one height *above its own deck*, unless
        // the machine is not tall enough to have one, in which case halfway up
        // it. The deck term is experiment 10: a drive train on a mezzanine is
        // still a straight line, it is just a straight line six metres up.
        Domain::Rotary | Domain::Mech => {
            let want = me.base + SHAFT_Y;
            if hi > want + 400 && lo < want - 200 {
                want
            } else {
                clamp(lo + span / 2)
            }
        }
        Domain::Fluid => clamp(lo + span / 4),
        Domain::Gas => clamp(lo + (span * 4) / 5),
        Domain::Heat => clamp(lo + (span * 3) / 4),
        Domain::Electrical => clamp(lo + span / 3),
        // Material falls: in at the top, out at the bottom. An ore line built
        // out of this rule cascades downhill without anyone saying so.
        Domain::Material => {
            if dir == Dir::In {
                clamp(hi - span / 6)
            } else {
                clamp(lo + span / 6)
            }
        }
    }
}

/// Slide a socket along its own face until it is on the axis it shares with
/// its partner -- or as near to it as the face reaches.
///
/// The midpoint rather than the partner's own centre, because both ends run
/// this and both have to arrive at the same line. Snapping to the partner
/// would have each machine adopt the other's axis and leave the pair exactly
/// as far apart as it found them, with the offsets swapped, which is a very
/// tidy way of achieving nothing.
fn onto_axis(me: &Placed, side: Side, at: P3, peer: &Placed) -> P3 {
    if !side.horizontal() {
        return at;
    }
    let (a, b) = (me.vol.centre(), peer.vol.centre());
    let mid = p3((a.x + b.x) / 2, (a.y + b.y) / 2, (a.z + b.z) / 2);
    let (lo, hi) = (me.vol.lo, me.vol.hi);
    let edge = 200;
    // Only sideways. The height of a drive is already agreed -- every shaft in
    // the plant is at `SHAFT_Y` above its own deck, which is the one constant
    // that makes a drive train read as a drive train -- and averaging it with
    // the partner's body centre would undo that for the sake of a coupling
    // that was already in line.
    let y = at.y;
    match side.world(me.yaw).axis() {
        // Along east-west: the face is a wall in z, and the socket slides in z.
        Some(0) => {
            p3(at.x, y, lane_near(mid.z.clamp(lo.z + edge, (hi.z - edge).max(lo.z + edge))))
        }
        _ => p3(lane_near(mid.x.clamp(lo.x + edge, (hi.x - edge).max(lo.x + edge))), y, at.z),
    }
}

/// The point on the face, spread across it if it has company, and snapped to
/// the lanes the router lays pipe along.
fn on_face(me: &Placed, side: Side, f: P3, y: Mm, slot: i32, n: i32) -> P3 {
    let c = me.vol.centre();
    let s = me.vol.size();
    let off = if n <= 1 { 0 } else { (slot * 2 - (n - 1)) * (s.x.min(s.z) / (2 * n + 2)) };
    if !side.horizontal() {
        // Top and bottom nozzles spread across the *plan* instead of up a
        // wall, and along whichever way the machine is longer.
        let along_x = s.x >= s.z;
        let (ox, oz) = if along_x { (off, 0) } else { (0, off) };
        return on_grid(p3(c.x + ox, y, c.z + oz));
    }
    let (o, oz) = (c.x + off, c.z + off);
    on_grid(match (f.x, f.z) {
        (x, _) if x > 0 => p3(me.vol.hi.x, y, oz),
        (x, _) if x < 0 => p3(me.vol.lo.x, y, oz),
        (_, z) if z > 0 => p3(o, y, me.vol.hi.z),
        _ => p3(o, y, me.vol.lo.z),
    })
}

/// A pipe's bore, from the rate its port carries. The one place in the visual
/// pipeline where a *simulation number* is allowed to decide a dimension --
/// and it is allowed because it runs the safe way round: the machine tells the
/// picture how big to be, never the other way.
pub fn bore(rate: u64) -> Mm {
    (160 + (rate as Mm) / 2).clamp(160, 520)
}

// ------------------------------------------------------------- the digest

/// Everything about a design that could possibly change its shape, hashed.
///
/// This is the `component layout` term of the note's `VisualSeed`, and it is
/// used for exactly the choices that belong to the whole installation -- its
/// paint, its enclosure. Per-component dressing deliberately does not see it;
/// `seed` explains why at length.
pub fn digest(d: &Design) -> u64 {
    let mut s = String::with_capacity(64 + d.units.len() * 24);
    s.push_str(&d.name);
    s.push('|');
    s.push_str(d.brief.tag());
    for u in &d.units {
        s.push_str(&format!(
            "|{}:{}:{},{},{}:{}",
            u.name,
            u.kind.tag(),
            u.x,
            u.y,
            u.z,
            u.face.map(|f| f as i32).unwrap_or(-1)
        ));
    }
    for w in &d.wires {
        s.push_str(&format!("|{}.{}>{}.{}", w.from, w.from_port, w.to, w.to_port));
    }
    hash(s.as_bytes())
}
