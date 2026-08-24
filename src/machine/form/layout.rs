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
//! # The third dimension is inferred, not authored
//!
//! Section 1 of the note asks for free 3D placement with semantic snapping.
//! What is here instead is *free 2D placement with inferred elevation*, and the
//! reason is the core rule two files up: the generated form may not change the
//! machine. Height in this experiment is a consequence of what a component is
//! -- a cyclone discharges downwards so it stands on legs, a shaft has to line
//! up with the shaft it drives so every rotary socket in the plant is at the
//! same height -- and every one of those is a rule the renderer can apply
//! without the simulator ever hearing about it.
//!
//! That turns out to be the more interesting half of the idea anyway. Elevation
//! that the player has to place by hand is CAD. Elevation that *falls out of
//! the machine* is the thing being tested.
//!
//! # Sockets snap
//!
//! A port's face is chosen by looking at what it is wired to. Two components
//! wired together put their sockets on the faces nearest each other, so a plant
//! whose plan reads left to right builds pipework that reads left to right, and
//! moving a component to the far side of its neighbour turns both sockets round
//! without anybody editing anything. That is the "semantic snapping" of the
//! note, arrived at from the other end: nothing snaps *to* anything, the
//! geometry simply notices where its partner is.
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
    /// Quarter turns clockwise from east: which way the machine faces, inferred
    /// from what it is wired to.
    pub yaw: u8,
    /// The tile footprint, as the design gave it.
    pub tile: (i32, i32, i32, i32),
    /// The body itself, off the ground by `lift`.
    pub vol: Vol,
    /// How high the underside sits.
    pub lift: Mm,
    /// The volume nothing else may route through.
    pub clear: Vol,
    pub sockets: Vec<Socket>,
}

/// A typed port, in space.
#[derive(Clone, Copy, Debug)]
pub struct Socket {
    pub port: usize,
    pub dom: Domain,
    pub dir: Dir,
    /// On the surface of the component.
    pub at: P3,
    /// Which way it faces: the direction a pipe leaves in.
    pub out: P3,
    /// The bore of whatever connects to it, from the port's rate. A 400/tick
    /// heat main is visibly a main; a 20/tick drive is visibly a drive.
    pub bore: Mm,
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
    use Arch::*;
    use Kind::*;
    use Mount::*;
    match k {
        Reactor => (Vessel, Plinth, 9000),
        Burner => (Skid, Plinth, 4200),
        Heater => (Can, Grade, 2000),
        Mains => (Pad, Grade, 3400),
        Pump => (Can, Plinth, 1400),
        Inlet => (Bin, Legs, 3600),
        Outlet => (Pad, Grade, 1800),
        Skip => (Pad, Grade, 2200),
        Radiator => (Bank, Frame, 2800),
        HeatPipe | SteamPipe | FluidPipe | Chute | Screw | Shaft | Cable => (Run, Grade, 1000),
        Hopper => (Bin, Legs, 4600),
        Tank => (Vessel, Plinth, 7000),
        Drum => (Shell, Plinth, 3000),
        Flywheel => (Wheel, Plinth, 2600),
        Valve | Clutch => (Inline, Grade, 1200),
        Exchanger => (Shell, Plinth, 3600),
        Preheater => (Shell, Plinth, 2400),
        Condenser => (Shell, Frame, 3200),
        Furnace => (Skid, Plinth, 5200),
        Kind::Turbine => (Arch::Turbine, Plinth, 3000),
        Generator => (Can, Plinth, 2600),
        Motor => (Can, Plinth, 1800),
        Gearbox => (Skid, Plinth, 1800),
        Crank => (Wheel, Plinth, 2000),
        Crusher => (Skid, Plinth, 5000),
        Mill => (Shell, Plinth, 3400),
        Separator => (Bin, Legs, 5200),
        RollMill => (Portal, Plinth, 3400),
        Press => (Portal, Plinth, 6000),
        Lathe => (Skid, Grade, 2200),
        Column => (Tower, Grade, 15000),
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
fn yaw_of(d: &Design, i: usize) -> u8 {
    let me = centre_tile(&d.units[i]);
    let (mut ax, mut az, mut n) = (0i64, 0i64, 0i64);
    for w in &d.wires {
        let (from, to) = (d.index_of(&w.from), d.index_of(&w.to));
        let (from, to) = match (from, to) {
            (Some(a), Some(b)) => (a, b),
            _ => continue,
        };
        if from == i {
            let p = centre_tile(&d.units[to]);
            ax += (p.0 - me.0) as i64;
            az += (p.1 - me.1) as i64;
            n += 1;
        } else if to == i {
            let p = centre_tile(&d.units[from]);
            ax += (me.0 - p.0) as i64;
            az += (me.1 - p.1) as i64;
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
fn face(yaw: u8) -> P3 {
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
            // a line shaft are at exactly the height of the ports it joins.
            let y = run_height(u.kind);
            Vol::new(p3(mid.x - dx, y - 250, mid.z - dz), p3(mid.x + dx, y + 250, mid.z + dz))
        } else {
            Vol::new(p3(x0 + INSET, lift, z0 + INSET), p3(x1 - INSET, lift + h, z1 - INSET))
        };
        units.push(Placed {
            name: u.name.clone(),
            kind: u.kind,
            arch,
            mount,
            yaw: yaw_of(d, i),
            tile: (u.x, u.y, u.w(), u.h()),
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
            p3((u.tile.0 + u.tile.2) * TILE, u.vol.hi.y, (u.tile.1 + u.tile.3) * TILE),
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
fn sockets(d: &Design, units: &[Placed], i: usize) -> Vec<Socket> {
    let me = &units[i];
    let part = parts::part(me.kind);
    let mut out: Vec<Socket> = Vec::new();

    // Which face each port wants, before crowding is taken into account.
    let mut want: Vec<(usize, P3, Mm)> = Vec::new();
    for (pi, port) in part.ports.iter().enumerate() {
        let peer = peer_of(d, units, &me.name, port.name, port.dir);
        let f = choose_face(me, port.dom, port.dir, peer);
        want.push((pi, f, height_for(me, port.dom, port.dir)));
    }

    for (pi, f, y) in want.iter().copied() {
        let port = &part.ports[pi];
        // Ports sharing a face get spread along it, in port order, so two
        // steam lines off one shell do not leave from the same square inch.
        let mates: Vec<usize> = want
            .iter()
            .filter(|(_, g, gy)| *g == f && (*gy - y).abs() < 400)
            .map(|(k, _, _)| *k)
            .collect();
        let slot = mates.iter().position(|&k| k == pi).unwrap_or(0) as i32;
        let n = mates.len() as i32;
        let at = on_face(me, f, y, slot, n);
        out.push(Socket {
            port: pi,
            dom: port.dom,
            dir: port.dir,
            at,
            out: f,
            bore: bore(port.rate),
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

/// Which of the four sides a port leaves by.
///
/// If it is wired, the side nearest its partner -- which is the whole of the
/// "semantic snapping" idea, and costs four comparisons. If it is not, the
/// front for an output and the back for an input, so an unwired component still
/// faces the way its machine flows.
fn choose_face(me: &Placed, dom: Domain, dir: Dir, peer: Option<&Placed>) -> P3 {
    let _ = dom;
    let Some(peer) = peer else {
        return if dir == Dir::Out { face(me.yaw) } else { face(me.yaw + 2) };
    };
    let c = me.vol.centre();
    let p = peer.vol.centre();
    let (dx, dz) = (p.x - c.x, p.z - c.z);
    if dx.abs() >= dz.abs() {
        if dx >= 0 {
            super::EAST
        } else {
            super::WEST
        }
    } else if dz >= 0 {
        super::SOUTH
    } else {
        super::NORTH
    }
}

/// How high up the face a port sits, by domain. The five lines in this
/// function are the plant's grammar.
fn height_for(me: &Placed, dom: Domain, dir: Dir) -> Mm {
    // A transport component *is* its domain's height: both its ports sit on
    // the axis of the run it draws.
    if me.arch == Arch::Run {
        return me.vol.centre().y;
    }
    let (lo, hi) = (me.vol.lo.y, me.vol.hi.y);
    let span = (hi - lo).max(600);
    let clamp = |y: Mm| y.clamp(lo + 250, hi - 250);
    match dom {
        // Every shaft in the plant at one height, unless the machine is not
        // tall enough to have one, in which case halfway up it.
        Domain::Rotary | Domain::Mech => {
            if hi > SHAFT_Y + 400 && lo < SHAFT_Y - 200 {
                SHAFT_Y
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

/// The point on the face, spread across it if it has company, and snapped to
/// the lanes the router lays pipe along.
fn on_face(me: &Placed, f: P3, y: Mm, slot: i32, n: i32) -> P3 {
    let c = me.vol.centre();
    let s = me.vol.size();
    let off = if n <= 1 { 0 } else { (slot * 2 - (n - 1)) * (s.x.min(s.z) / (2 * n + 2)) };
    let (y, o) = (lane(y), lane(c.x + off));
    let oz = lane(c.z + off);
    match (f.x, f.z) {
        (x, _) if x > 0 => p3(me.vol.hi.x, y, oz),
        (x, _) if x < 0 => p3(me.vol.lo.x, y, oz),
        (_, z) if z > 0 => p3(o, y, me.vol.hi.z),
        _ => p3(o, y, me.vol.lo.z),
    }
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
        s.push_str(&format!("|{}:{}:{},{}", u.name, u.kind.tag(), u.x, u.y));
    }
    for w in &d.wires {
        s.push_str(&format!("|{}.{}>{}.{}", w.from, w.from_port, w.to, w.to_port));
    }
    hash(s.as_bytes())
}
