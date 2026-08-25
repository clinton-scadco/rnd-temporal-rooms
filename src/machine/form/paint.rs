//! Experiment 09: the material language, as a pass.
//!
//! The note that asked for experiment 09 was blunt about where the biggest
//! gain was, and it was not geometry:
//!
//! > **No geometry changes. Just improve the material/paint assignment rules.**
//!
//! So this file is a pass that runs last, after every other pass has placed
//! everything it is going to place, and it is allowed to write exactly one
//! field of a `Piece`. It cannot move anything, because it is not given
//! anything it could move anything with. `tests/read.rs` checks the claim from
//! the outside -- grades A and B are the same plant piece for piece, and the
//! only difference between them is which of twelve materials each piece is --
//! but the reason the claim holds is that the signature of [`apply`] does not
//! permit anything else.
//!
//! # What the language says
//!
//! ```text
//!   pressure vessels, tanks, columns     off-white painted steel
//!   heat equipment, heat mains           lagging
//!   rotating and process machinery       the works colour, from the seed
//!   structural steel                     dark, unloved
//!   walkways, ladders, cladding          galvanised
//!   foundations                          concrete
//!   handrails, guards, kerbs             hazard yellow
//!   cold service                         blue-grey
//!   fuel and process service             dark green
//!   steam                                bright steel, lagged at the joints
//!   drives                               bright steel
//!   electrical                           galvanised conduit, copper at the ends
//! ```
//!
//! Twelve rows, and the top eight are decided by *what a component is for*
//! rather than by what it looks like. That is the whole trick: the palette is
//! not a style sheet applied to geometry, it is another consequence of the
//! machine, in exactly the way experiment 08's bores and heights were.
//!
//! # Service, and why a refinery is a different colour from a power plant
//!
//! A fluid line is not just a fluid line. Water is blue-grey and crude is dark
//! green, and knowing which is which means asking the *design* what is in the
//! pipe -- so [`service`] walks upstream from a port, hop by hop, staying
//! inside one domain, until it finds a source and the substance that source
//! was tuned to draw.
//!
//! It reads the document and nothing else. No tick is simulated, no state is
//! consulted, and a line whose origin cannot be traced falls back to its
//! domain's resting substance. The result is that a distillation train comes
//! out green and a boiler house comes out blue, from the same rule, with
//! nobody having said so.

use super::kit::{Mat, Mesh};
use super::layout::{Arch, Placed, Plan};
use super::route::Run;
use super::seed::hash;
use super::{Grade, Owner, Owns, Piece, P3};
use crate::machine::design::Design;
use crate::machine::parts::{self, Family, Kind};
use crate::machine::stuff::{Domain, Subst};

/// The one entry point, and the one field it may write.
///
/// Everything else in the signature is `&`, which is not an accident: it is
/// how the "no geometry changes" claim is enforced rather than remembered.
pub fn apply(
    plan: &Plan,
    routes: &[Run],
    owners: &[Owner],
    grade: Grade,
    pieces: &mut [Piece],
) {
    if !grade.painted() {
        return;
    }
    for p in pieces.iter_mut() {
        let o = &owners[p.of as usize];
        let mat = match o.class {
            Owns::Unit => match plan.find(&o.name) {
                Some(u) if u.arch == Arch::Run => transport(u, routes, p),
                Some(u) => unit(u, p),
                None => p.mat,
            },
            Owns::Run => match routes.iter().find(|r| r.name == o.name) {
                Some(r) => line(r.dom, r.serve, p),
                None => p.mat,
            },
            Owns::Frame => frame(&o.what, p),
            Owns::Shell => shell(&o.what, p),
        };
        p.mat = mat;
        // Structure and concrete weather; paint does not, much. The variation
        // comes from where the piece *is* rather than from a stream, so that
        // moving one machine cannot re-weather the wall behind it.
        if matches!(o.class, Owns::Frame | Owns::Shell) && p.tint == 0 {
            p.tint = weather(p.at, mat);
        }
    }
}

/// A little variation, from a position. Two panels of the same wall are not
/// quite the same panel, and nothing had to remember which was which.
fn weather(at: P3, m: Mat) -> u8 {
    let spread = match m {
        Mat::Concrete => 5,
        Mat::Dark | Mat::Galv => 4,
        Mat::Warn => 2,
        _ => 3,
    };
    let h = hash(&[
        (at.x >> 8) as u8,
        (at.x >> 4) as u8,
        (at.y >> 7) as u8,
        (at.z >> 8) as u8,
        (at.z >> 4) as u8,
    ]);
    (h % spread) as u8
}

// ------------------------------------------------------------------ bodies

/// What a component's body is painted, by what the component is *for*.
///
/// This is the row of the table that does the most work, because it is the one
/// a person reads first: a plant where the vessels are one colour, the
/// machinery is another and the hot equipment is a third has a visual
/// hierarchy before a single detail has been added to it.
pub fn skin(u: &Placed) -> Mat {
    match u.kind {
        // Anything with a fire or a reaction in it is lagged, whatever family
        // the catalogue files it under.
        Kind::Reactor | Kind::Burner | Kind::Furnace => Mat::Lag,
        // A radiator's job is to lose heat, so it is emphatically not lagged.
        Kind::Radiator => Mat::Galv,
        Kind::Mains => Mat::Galv,
        // Shape wins over family for the two archetypes that are *vessels*
        // whatever else they do. A distillation column is filed under process
        // because of what it performs; it is still a tall steel pressure
        // vessel, and painting it like a gearbox was the first thing that
        // looked wrong.
        _ if matches!(u.arch, Arch::Vessel | Arch::Tower) => Mat::Cream,
        _ => match u.kind.family() {
            Family::Heat => Mat::Lag,
            Family::Store => Mat::Cream,
            Family::Sink => Mat::Galv,
            Family::Mechanical | Family::Process | Family::Control => Mat::Paint,
            _ => match u.arch {
                Arch::Bin | Arch::Pad => Mat::Galv,
                _ => Mat::Paint,
            },
        },
    }
}

/// One piece of one component.
///
/// The old material is the fallback rather than the input: a piece the language
/// has no opinion about keeps what experiment 08 gave it, which is what stops
/// this pass from being a rewrite of the plant in twelve colours.
fn unit(u: &Placed, p: &Piece) -> Mat {
    match p.mesh {
        // Structure, and the things bolted to it.
        Mesh::Beam | Mesh::Anchor | Mesh::Bearing | Mesh::Saddle | Mesh::Coupling => Mat::Dark,
        // Sheet metal and access.
        Mesh::Grate | Mesh::Ladder | Mesh::Louvre | Mesh::Panel | Mesh::Cowl => Mat::Galv,
        // Handrail steel is galvanised like the grating it stands on. The
        // yellow is spent on the *stairs* instead -- see `frame`.
        Mesh::Rail => Mat::Galv,
        Mesh::Step => Mat::Warn,
        // Bright work: what turns, what is read, what is bolted.
        Mesh::Rotor | Mesh::Gauge | Mesh::Flange | Mesh::Band | Mesh::Reducer => Mat::Steel,
        Mesh::Clamp => Mat::Dark,
        Mesh::Stack => Mat::Dark,
        // A stub says what leaves through it, which experiment 08 already got
        // right -- so it is left alone.
        Mesh::Nozzle => p.mat,
        Mesh::Valve => Mat::Paint,
        // A guard over something that turns.
        Mesh::Tee => Mat::Steel,
        _ => match p.mat {
            Mat::Concrete => Mat::Concrete,
            Mat::Copper => Mat::Copper,
            Mat::Rubber => Mat::Rubber,
            // A bedplate, a skirt, a foot: dark, whatever the body is.
            Mat::Dark => Mat::Dark,
            _ => skin(u),
        },
    }
}

/// A transport component draws its own run, so it is painted as pipework --
/// in the service of whatever is actually flowing through it, which is found
/// by asking the routes that arrive at it.
fn transport(u: &Placed, routes: &[Run], p: &Piece) -> Mat {
    let dom = parts::part(u.kind).ports.first().map(|q| q.dom).unwrap_or(Domain::Fluid);
    let serve = routes
        .iter()
        .filter(|r| r.dom == dom && (r.name.starts_with(&format!("{}.", u.name)) || r.name.contains(&format!("-> {}.", u.name))))
        .map(|r| r.serve)
        .next()
        .unwrap_or_else(|| dom.rest());
    line(dom, serve, p)
}

// ---------------------------------------------------------------- pipework

/// What a run is made of: its domain decides the treatment, its service
/// decides the colour.
///
/// The seven domains looked different in experiment 08 by *shape* -- bore,
/// height, whether it bends. Here they also look different by material, and
/// the fluid domain splits again by what is in it, which is the one place the
/// palette knows something the geometry does not.
fn line(dom: Domain, serve: Subst, p: &Piece) -> Mat {
    let body = match dom {
        Domain::Heat => Mat::Lag,
        Domain::Gas => Mat::Steel,
        Domain::Fluid => match serve {
            Subst::Crude | Subst::Light | Subst::Middle | Subst::Heavy => Mat::Oil,
            _ => Mat::Water,
        },
        Domain::Rotary | Domain::Mech => Mat::Steel,
        Domain::Electrical => Mat::Galv,
        Domain::Material => Mat::Galv,
    };
    match p.mesh {
        // The joints are bright steel in every service, which is what makes a
        // flange read as a flange rather than as a fat bit of pipe.
        Mesh::Flange | Mesh::Reducer | Mesh::Tee => {
            if dom == Domain::Electrical {
                Mat::Copper
            } else {
                Mat::Steel
            }
        }
        Mesh::Coupling => Mat::Steel,
        Mesh::Clamp | Mesh::Support => Mat::Dark,
        Mesh::Valve => match dom {
            Domain::Heat | Domain::Gas => Mat::Steel,
            _ => Mat::Warn,
        },
        Mesh::Band => match dom {
            // A steam main is bare steel with lagged collars; a heat main is
            // lagged with steel banding. Two lines of the same size, opposite
            // ways round, and a stranger can tell them apart at thirty metres.
            Domain::Gas => Mat::Lag,
            Domain::Heat => Mat::Steel,
            // A transfer chute is marked where it is opened.
            Domain::Material => Mat::Warn,
            _ => Mat::Dark,
        },
        Mesh::Elbow | Mesh::Cyl | Mesh::Box => {
            // The conduit's clips arrive as dark boxes and stay dark boxes.
            if dom == Domain::Electrical && p.mat == Mat::Dark {
                Mat::Dark
            } else {
                body
            }
        }
        _ => body,
    }
}

// --------------------------------------------------------------- the steel

fn frame(what: &str, p: &Piece) -> Mat {
    match p.mesh {
        Mesh::Grate | Mesh::Rail => Mat::Galv,
        // The accent is spent on the way *up*. Every platform in a plant has a
        // handrail round it, so a yellow handrail is not an accent, it is a
        // colour scheme -- but a yellow stair is the one thing in the frame
        // that says where a person goes, and there is one of those per
        // machine rather than one per edge.
        Mesh::Step => Mat::Warn,
        Mesh::Beam | Mesh::Support | Mesh::Clamp | Mesh::Anchor => Mat::Dark,
        // The stair's own handrail is a plain tube, so it is told apart from a
        // pipe by what it belongs to rather than by what it is.
        Mesh::Cyl if what == "platform" => Mat::Warn,
        _ => match p.mat {
            Mat::Concrete => Mat::Concrete,
            other => other,
        },
    }
}

fn shell(what: &str, p: &Piece) -> Mat {
    match p.mesh {
        Mesh::Louvre => Mat::Galv,
        Mesh::Beam => Mat::Dark,
        // A skid's kerb is what people trip over, so it is painted like it.
        Mesh::Box if what == "skid" && p.mat == Mat::Dark => Mat::Warn,
        _ => match p.mat {
            Mat::Concrete => Mat::Concrete,
            Mat::Galv => Mat::Galv,
            // The walls are the plant's own colour, which is the one place the
            // seed still chooses anything about the palette.
            other => other,
        },
    }
}

// ----------------------------------------------------------------- service

/// What is actually in the pipe, found by walking upstream through the
/// document.
///
/// Hop by hop, staying in one domain, until either a source says what it draws
/// or the trail runs out. Twenty-four hops is a bound rather than a budget --
/// a cycle would otherwise walk forever, and a plant with a recycle loop in it
/// is a perfectly ordinary plant.
///
/// This is a *design* query. It does not simulate, it does not look at a tick,
/// and its answer changes only when somebody edits the machine -- which is the
/// same contract every other input to `form` has.
pub fn service(d: &Design, unit: usize, port: usize) -> Subst {
    let Some(u) = d.units.get(unit) else {
        return Subst::Water;
    };
    let ports = parts::part(u.kind).ports;
    let Some(dom) = ports.get(port).map(|q| q.dom) else {
        return Subst::Water;
    };
    let mut at = unit;
    let mut seen: Vec<usize> = Vec::with_capacity(8);
    for _ in 0..24 {
        if seen.contains(&at) {
            break;
        }
        seen.push(at);
        let here = &d.units[at];
        // A source is the end of the walk, and the only thing that actually
        // knows an answer. Both of the kinds that draw from the world are
        // asked, because a pump drawing crude is exactly the case this whole
        // function exists for.
        if matches!(here.kind, Kind::Inlet | Kind::Pump) {
            return here.tune.subst;
        }
        let Some(next) = feeder(d, at, dom) else { break };
        at = next;
    }
    dom.rest()
}

/// Whatever feeds this component in this domain, in document order.
fn feeder(d: &Design, to: usize, dom: Domain) -> Option<usize> {
    for w in &d.wires {
        let (Some(a), Some(b)) = (d.index_of(&w.from), d.index_of(&w.to)) else {
            continue;
        };
        if b != to {
            continue;
        }
        let ports = parts::part(d.units[a].kind).ports;
        if ports.iter().any(|q| q.name == w.from_port && q.dom == dom) {
            return Some(a);
        }
    }
    None
}
