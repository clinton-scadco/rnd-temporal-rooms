//! Spatial rules: what the plant's geometry says about whether the plant is
//! any good.
//!
//! Experiment 10's third section, and the one that makes the other two matter.
//! The note's warning was exact:
//!
//! > Otherwise 3D placement just becomes the same easy `x100000` problem with
//! > objects stacked vertically until the entire factory resembles a lasagne.
//!
//! Free placement in three dimensions is only interesting if space is scarce,
//! and space is only scarce if something *needs* it. So every component claims
//! four volumes rather than one:
//!
//! ```text
//!   solid       the machine itself: nothing else may be here at all
//!   service     the room a person needs in front of it to work on it
//!   hot         the separation a hot machine needs from a cold one
//!   exclusion   the straight run off each flange, which is pipework's
//! ```
//!
//! and this pass reports, per component, which of them are being trodden on:
//!
//! ```text
//!   green    every rule kept
//!   yellow   something is in the way of servicing or cooling it
//!   red      it is inside another machine, unsupported, or misaligned
//! ```
//!
//! # It reports, it does not refuse
//!
//! Nothing here stops a plant being built. The document's own `check` refuses
//! the things that are not a *design* at all -- two components on the same
//! tiles at the same height, a wire between ports that cannot carry each other
//! -- and everything in this file is downstream of that: it is the difference
//! between an illegal machine and a badly laid-out one, and only the second
//! kind is interesting to play with.
//!
//! That distinction is also why this file is in `form` rather than in
//! `design`. A service clearance is a fact about geometry, geometry is
//! derived, and the core rule of this module tree is that derived geometry
//! never edits the machine. The verdict is a *reading* of the plant, in the
//! same sense that the scoreboard is a reading of the simulation.
//!
//! # The six rules
//!
//! ```text
//!   equipment cannot overlap        two solids intersect
//!   some parts need maintenance     something is in the service volume
//!   hot objects need separation     a cold machine is inside a hot one's halo
//!   shafts need alignment           a drive's two ends are not on one axis
//!   big vessels need foundations    a tower on a mezzanine is not a design
//!   pipes need routes               the router said no
//! ```
//!
//! Five of the six come straight from the note. The sixth is what experiment
//! 10 did to the router, and it belongs in the same list because it is the
//! same kind of fact: a thing the player can see, understand and fix by moving
//! something.

use super::layout::{Arch, Placed, Plan};
use super::route::{Run, Tier};
use super::{p3, Mm, Vol, P3};
use crate::machine::parts::Kind;
use crate::machine::stuff::Domain;

// ------------------------------------------------------------- the numbers

/// How much room a person needs in front of a machine to work on it.
pub const SERVICE: Mm = 1400;
/// How high that room has to be, which is how tall a person is plus a
/// spanner's worth of optimism.
pub const HEADROOM: Mm = 2200;
/// How far a hot machine has to be from a cold one.
pub const HOT: Mm = 1800;
/// How far out of line the two ends of a shaft may be before the coupling is a
/// lie. A real one would be a millimetre; a game's is a quarter of a tile,
/// because the point is to make alignment a thing the player arranges rather
/// than a thing they measure.
pub const MISALIGN: Mm = 500;

/// Which components are hot enough to need room around them.
pub fn hot(k: Kind) -> bool {
    matches!(k, Kind::Reactor | Kind::Burner | Kind::Furnace | Kind::Heater | Kind::Crusher)
}

/// And which ones mind. Anything with a motor in it, anything with a cable on
/// it, and anything holding something that would rather not boil.
pub fn minds_heat(k: Kind) -> bool {
    matches!(
        k,
        Kind::Generator
            | Kind::Motor
            | Kind::Mains
            | Kind::Cable
            | Kind::Tank
            | Kind::Drum
            | Kind::Pump
            | Kind::Gearbox
            | Kind::Lathe
    )
}

/// Whether anybody ever has to get to it. A pad, a kerb and a length of pipe
/// do not need standing room; everything with moving parts does.
pub fn serviceable(a: Arch) -> bool {
    !matches!(a, Arch::Pad | Arch::Run)
}

/// Whether it is heavy enough that the ground has to be underneath it. A
/// fifteen-metre column on a mezzanine is not a design, it is a dare.
pub fn needs_ground(a: Arch) -> bool {
    matches!(a, Arch::Tower | Arch::Vessel)
}

// -------------------------------------------------------------- the verdict

/// Green, yellow, red. The note asked for exactly these three and they are
/// worth keeping to three: a scale with five points on it is a scale nobody
/// reads at a glance, and glanceable is the entire purpose.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Default)]
pub enum Verdict {
    #[default]
    Clear,
    /// Something is in the way of servicing or cooling it. It will work; a
    /// person will hate it.
    Watch,
    /// It is inside something, unsupported, or driving a shaft that does not
    /// line up.
    Bad,
}

impl Verdict {
    pub fn tag(self) -> &'static str {
        match self {
            Verdict::Clear => "clear",
            Verdict::Watch => "watch",
            Verdict::Bad => "bad",
        }
    }
    /// The colour the note asked for, so that the browser and the terminal
    /// agree without either of them choosing.
    pub fn colour(self) -> &'static str {
        match self {
            Verdict::Clear => "green",
            Verdict::Watch => "yellow",
            Verdict::Bad => "red",
        }
    }
    fn worst(self, o: Verdict) -> Verdict {
        if o > self {
            o
        } else {
            self
        }
    }
}

/// One component, with everything a client needs to draw it, select it and
/// colour it, and nothing it would have to rebuild the plant to work out.
#[derive(Clone, Debug)]
pub struct Placement {
    pub name: String,
    pub kind: String,
    pub arch: &'static str,
    /// Where the player put it: tiles east, tiles south, tiles up.
    pub tile: (i32, i32, i32),
    pub yaw: u8,
    pub turned: bool,
    pub solid: Vol,
    pub service: Vol,
    pub verdict: Verdict,
}

/// Something the player would want to know, attached to whatever they should
/// look at.
#[derive(Clone, Debug)]
pub struct Issue {
    /// The component or connection at fault.
    pub of: String,
    /// The rule, in three words, for a list.
    pub rule: &'static str,
    /// The whole sentence, for a tooltip.
    pub what: String,
    /// Red rather than yellow.
    pub bad: bool,
}

fn issue(of: &str, rule: &'static str, what: String, bad: bool) -> Issue {
    Issue { of: of.to_string(), rule, what, bad }
}

// ---------------------------------------------------------------- the pass

/// The room a person needs to work on one component, on one of its sides.
///
/// A slab off one face, from the deck it stands on to head height. Which face
/// is the interesting part, and it is not a fixed one: see `access`.
pub fn room(u: &Placed, side: super::layout::Side) -> Vol {
    let f = side.world(u.yaw);
    let v = u.vol;
    match (f.x, f.z) {
        (x, _) if x > 0 => Vol::new(
            p3(v.hi.x, u.base, v.lo.z),
            p3(v.hi.x + SERVICE, u.base + HEADROOM, v.hi.z),
        ),
        (x, _) if x < 0 => Vol::new(
            p3(v.lo.x - SERVICE, u.base, v.lo.z),
            p3(v.lo.x, u.base + HEADROOM, v.hi.z),
        ),
        (_, z) if z > 0 => Vol::new(
            p3(v.lo.x, u.base, v.hi.z),
            p3(v.hi.x, u.base + HEADROOM, v.hi.z + SERVICE),
        ),
        _ => Vol::new(
            p3(v.lo.x, u.base, v.lo.z - SERVICE),
            p3(v.hi.x, u.base + HEADROOM, v.lo.z),
        ),
    }
}

/// Which side of a component a person can actually get at it from, and what is
/// in the way if none of them work.
///
/// The rule is *existence*, not preference. A machine in a plant does not have
/// to be approachable from a particular side -- it has to be approachable. So
/// the four sides are tried in a fixed order beginning with the one it faces,
/// and the first that is clear is the answer. Only a machine boxed in on all
/// four sides is a machine nobody can service, and that is a fault worth
/// showing the player because there is exactly one way to fix it: move
/// something.
///
/// This started out as "the front must be clear", which flagged half of every
/// design in the repository. A rule that fires on everything is not a rule, it
/// is a background colour.
pub fn access(u: &Placed, units: &[Placed], routes: &[Run]) -> (Vol, Option<String>) {
    use super::layout::Side;
    let mine = &u.name;
    let mut first = None;
    let mut blocker = None;
    for side in [Side::Front, Side::Left, Side::Right, Side::Back] {
        let v = room(u, side);
        if first.is_none() {
            first = Some(v);
        }
        let by = units
            .iter()
            .find(|o| &o.name != mine && o.vol.hits(v))
            .map(|o| o.name.clone())
            .or_else(|| {
                routes
                    .iter()
                    .find(|r| r.laid() && crosses(&r.path, v))
                    .map(|r| r.name.clone())
            });
        match by {
            None => return (v, None),
            Some(by) => blocker = blocker.or(Some(by)),
        }
    }
    (first.unwrap_or(Vol::new(u.vol.lo, u.vol.lo)), blocker)
}

/// The straight run off one flange that belongs to the pipework and to nothing
/// else. The router already refuses to bend inside it; this is the same rule
/// said to the *equipment*, so that a machine parked against a nozzle is a
/// fault rather than a surprise.
pub fn exclusions(u: &Placed) -> Vec<(usize, Vol)> {
    u.sockets
        .iter()
        .enumerate()
        // A shaft is exempt, and it is the exemption that proves the rule: a
        // coupling *wants* its partner up against the flange. The turbine and
        // the generator bolted to the end of it are half a metre apart on
        // purpose, and `shaft alignment` is the rule that governs them.
        .filter(|(_, s)| !matches!(s.dom, Domain::Rotary | Domain::Mech))
        .map(|(i, s)| {
            let a = s.at;
            let b = s.at.add(s.out.mul(s.stub));
            (i, Vol::new(a, b).grow(s.bore / 2))
        })
        .collect()
}

/// Every spatial rule the plant breaks, and a verdict per component.
pub fn check(plan: &Plan, routes: &[Run]) -> (Vec<Placement>, Vec<Issue>) {
    let mut issues: Vec<Issue> = Vec::new();
    let mut verdicts: Vec<Verdict> = vec![Verdict::Clear; plan.units.len()];

    // ------------------------------------------------ equipment cannot overlap
    //
    // The document refuses this in tiles. This catches what tiles cannot: a
    // component that was turned, or lifted onto a deck whose thickness the
    // tile grid rounded away.
    for i in 0..plan.units.len() {
        for j in i + 1..plan.units.len() {
            let (a, b) = (&plan.units[i], &plan.units[j]);
            if a.vol.hits(b.vol) {
                issues.push(issue(
                    &a.name,
                    "collision",
                    format!("{} is inside {}", a.name, b.name),
                    true,
                ));
                verdicts[i] = verdicts[i].worst(Verdict::Bad);
                verdicts[j] = verdicts[j].worst(Verdict::Bad);
            }
        }
    }

    // ------------------------------------------- some parts need maintenance
    let mut rooms: Vec<Vol> = Vec::with_capacity(plan.units.len());
    for i in 0..plan.units.len() {
        let u = &plan.units[i];
        if !serviceable(u.arch) {
            rooms.push(Vol::new(u.vol.lo, u.vol.lo));
            continue;
        }
        let (room, blocked) = access(u, &plan.units, routes);
        rooms.push(room);
        if let Some(by) = blocked {
            issues.push(issue(
                &u.name,
                "service access",
                format!(
                    "{} is boxed in on all four sides -- {by} is against the last of them",
                    u.name
                ),
                false,
            ));
            verdicts[i] = verdicts[i].worst(Verdict::Watch);
        }
    }

    // ------------------------------------------------ nozzles need somewhere
    //
    // A backstop rather than a rule the player will often trip. `layout` will
    // not put a nozzle on a face with nothing in front of it in the first
    // place, so this only fires when a machine is moved *into* a straight that
    // was already clear -- which is precisely the case a warning is for,
    // because the pipe that was there is now routed round the outside.
    for i in 0..plan.units.len() {
        let u = &plan.units[i];
        for (k, zone) in exclusions(u) {
            let Some(by) = plan
                .units
                .iter()
                .enumerate()
                .find(|(j, o)| *j != i && o.vol.hits(zone))
                .map(|(_, o)| o.name.clone())
            else {
                continue;
            };
            let port = crate::machine::parts::part(u.kind).ports[u.sockets[k].port].name;
            issues.push(issue(
                &u.name,
                "nozzle blocked",
                format!("{by} is against {}.{port}, and a line has to leave it straight", u.name),
                false,
            ));
            verdicts[i] = verdicts[i].worst(Verdict::Watch);
        }
    }

    // ------------------------------------------- hot objects need separation
    for i in 0..plan.units.len() {
        let u = &plan.units[i];
        if !hot(u.kind) {
            continue;
        }
        let halo = u.vol.grow(HOT);
        for (j, o) in plan.units.iter().enumerate() {
            if j == i || !minds_heat(o.kind) || !o.vol.hits(halo) {
                continue;
            }
            issues.push(issue(
                &o.name,
                "hot clearance",
                format!("{} is within {} m of {}, which is hot", o.name, HOT / 1000, u.name),
                false,
            ));
            verdicts[j] = verdicts[j].worst(Verdict::Watch);
        }
    }

    // ------------------------------------------- big vessels need foundations
    for i in 0..plan.units.len() {
        let u = &plan.units[i];
        if u.level > 0 && needs_ground(u.arch) {
            issues.push(issue(
                &u.name,
                "foundation",
                format!("{} is {} m up and needs to stand on the ground", u.name, u.base / 1000),
                true,
            ));
            verdicts[i] = verdicts[i].worst(Verdict::Bad);
        }
    }

    // ------------------------------------------------- shafts need alignment
    //
    // The strictest interface in the plant, and the one the note singled out.
    // A pipe bends; a shaft does not, so the two ends of a drive have to be on
    // one axis and the player is the only one who can arrange that.
    for r in routes {
        if !matches!(r.dom, Domain::Rotary | Domain::Mech) || !r.laid() {
            continue;
        }
        let (a, b) = (r.path[0], r.path[r.path.len() - 1]);
        let off = misalignment(a, b);
        if off > MISALIGN {
            issues.push(issue(
                &r.name,
                "shaft alignment",
                format!(
                    "{} is {} mm out of line -- a shaft needs its two ends on one axis",
                    r.name, off
                ),
                true,
            ));
            for n in ends_of(&r.name) {
                if let Some(k) = plan.index_of(&n) {
                    verdicts[k] = verdicts[k].worst(Verdict::Bad);
                }
            }
        }
    }

    // ------------------------------------------------------ pipes need routes
    for r in routes {
        match r.tier {
            Tier::Lost => {
                issues.push(issue(
                    &r.name,
                    "no route",
                    format!("no valid route found for {}", r.name),
                    true,
                ));
                for n in ends_of(&r.name) {
                    if let Some(k) = plan.index_of(&n) {
                        verdicts[k] = verdicts[k].worst(Verdict::Bad);
                    }
                }
            }
            Tier::Tight => {
                issues.push(issue(
                    &r.name,
                    "tight route",
                    format!("{} could only be run by relaxing the routing rules", r.name),
                    false,
                ));
                for n in ends_of(&r.name) {
                    if let Some(k) = plan.index_of(&n) {
                        verdicts[k] = verdicts[k].worst(Verdict::Watch);
                    }
                }
            }
            Tier::Clean => {}
        }
    }

    // Issues in a fixed order, because a list that reshuffles itself between
    // two identical builds is not a report, it is weather.
    issues.sort_by(|a, b| (!a.bad, a.rule, &a.of).cmp(&(!b.bad, b.rule, &b.of)));

    let places = plan
        .units
        .iter()
        .enumerate()
        .map(|(i, u)| Placement {
            name: u.name.clone(),
            kind: u.kind.tag().to_string(),
            arch: u.arch.tag(),
            tile: (u.tile.0, u.tile.1, u.level),
            yaw: u.yaw,
            turned: u.turned,
            solid: u.vol,
            service: rooms[i],
            verdict: verdicts[i],
        })
        .collect();
    (places, issues)
}

/// How far the far end of a drive is off the axis the near end leaves on.
///
/// Measured across the axis rather than along it: a shaft may be any length,
/// and may only be one straight line.
fn misalignment(a: P3, b: P3) -> Mm {
    let d = b.sub(a);
    // Whichever axis carries most of the distance is the shaft's axis; what is
    // left over on the other two is the misalignment.
    let (x, y, z) = (d.x.abs(), d.y.abs(), d.z.abs());
    if x >= y && x >= z {
        y + z
    } else if y >= z {
        x + z
    } else {
        x + y
    }
}

/// Whether a routed path passes through a volume.
fn crosses(path: &[P3], v: Vol) -> bool {
    for i in 1..path.len() {
        let (a, b) = (path[i - 1], path[i]);
        let n = (a.taxi(b) / 250).clamp(1, 400);
        for k in 0..=n {
            let p = p3(
                a.x + (b.x - a.x) * k / n,
                a.y + (b.y - a.y) * k / n,
                a.z + (b.z - a.z) * k / n,
            );
            if v.has(p) {
                return true;
            }
        }
    }
    false
}

/// The two component names out of a run's own name, which is always
/// `A.port -> B.port`.
fn ends_of(name: &str) -> Vec<String> {
    name.split("->")
        .map(|half| half.trim().split('.').next().unwrap_or("").to_string())
        .filter(|s| !s.is_empty())
        .collect()
}
