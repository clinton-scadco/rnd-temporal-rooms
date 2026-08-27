//! Connection routing: A* on a coarse grid, and what each domain looks like
//! once it has a path.
//!
//! Section 4 of the note, almost literally. Given two sockets and a plant full
//! of obstacles, find a path minimising
//!
//! ```text
//!   distance + bend penalty + collision penalty + clearance penalty
//! ```
//!
//! and then turn the path into straight sections, elbows and flanges. A coarse
//! grid with A* is explicitly declared sufficient, and it is, provided the
//! search state includes *which way the pipe is travelling* -- otherwise a bend
//! penalty cannot be charged and every run comes out as a staircase.
//!
//! # The seven domains do not look alike
//!
//! ```text
//!   fluid       painted pipe, flanged, the occasional valve
//!   gas         steel pipe, lightly banded, up on the rack
//!   heat        fat lagged pipe, banded every three-quarters of a metre
//!   rotary      thin bright shaft, couplings, and it hates bending
//!   mech        thin bright rod, and it hates bending even more
//!   electrical  galvanised conduit, clipped, no elbows worth the name
//!   material    square chute, wide, and it wants to go downhill
//! ```
//!
//! That table is the answer to the question the primary experiment asks. With
//! the labels hidden, a viewer can tell a steam main from a drive shaft from a
//! cable tray, because those three things are not the same shape, the same
//! size, the same colour or at the same height -- and none of that was drawn by
//! anybody. It came out of the port's domain and the port's rate.
//!
//! # Experiment 09: the same routing, spoken properly
//!
//! `dress` lays the pipe and `vocabulary` says how it is made: a pair of
//! flanges at each equipment interface rather than one, an isolation valve
//! where a line leaves a machine, a reducer where the two ends are different
//! sizes, a clamp at every support, a lagging collar either side of every hot
//! elbow, and a tee where two lines leave one socket. None of it moves a pipe;
//! all of it is placed on the path the router already found.
//!
//! # Order matters, so it is fixed
//!
//! Routes are laid in order of bore, widest first, and ties are broken by the
//! order the wires appear in the document. A route may not cross one already
//! laid. Both of those are arbitrary; both of them have to be *stable*, or the
//! same design would build differently on two machines, which is the one thing
//! section 7 does not allow.
//!
//! # Experiment 10: routing that can be trusted, and can refuse
//!
//! The note that asked for experiment 10 was blunt about this file:
//!
//! > From the screenshot, pipe generation has reached the point where visual
//! > errors will actively undermine the mechanic.
//!
//! It is right, and the reason is that experiment 08's router was a *shortest
//! path* solver with a bend penalty bolted on. Shortest paths on a grid are
//! staircases, and a staircase drawn in 800mm lagged pipe is a picture of
//! something that could not be built. So the search no longer walks cells. It
//! walks **straight sections**:
//!
//! ```text
//!   a node is (corner, heading)
//!   an edge is a straight run of at least `straight` millimetres
//! ```
//!
//! which makes six of the note's nine rules true by construction rather than
//! by penalty -- there is no path in the search space that bends twice in a
//! metre, so no amount of bad luck can produce one.
//!
//! ```text
//!   socket direction              the first and last sections are the flange
//!                                 normal, and nothing else is offered
//!   minimum straight before bend  the gate cells: the first bend is `stub`
//!                                 from the flange, and so is the last
//!   allowed bend radius           `straight` is at least twice the radius,
//!                                 so every corner can afford its own elbow
//!   pipe diameter                 the bore, from the port's rate
//!   clearance from equipment      a cost inside it, forbidden through it
//!   clearance between pipes       a laid route claims its cells and charges
//!                                 for the ones beside them
//!   preferred elevations          `Layer`: five storeys, one per domain
//!   support spacing               `span`, per domain, and the structural pass
//!                                 reads exactly the same list
//!   junction rules                two lines off one socket get a tee
//! ```
//!
//! # And it is allowed to fail
//!
//! Experiment 08 could not fail. If A* found nothing it drew a straight line
//! from one socket to the other, through whatever was in the way, and called
//! it `direct`. The note is right about that too:
//!
//! > “No valid route found.” That is better than generating nonsense.
//!
//! So a run is laid at the first of three tiers that works:
//!
//! ```text
//!   clean   every rule above, in full
//!   tight   half the straights, and it may share a corridor with another line
//!   lost    no valid route found -- and nothing is drawn
//! ```
//!
//! A lost run still exists: it keeps its name, it is counted, and the designer
//! says so. What it does not do is invent geometry. That is the whole point of
//! the tier: a plant with a hole in it is a plant the player can fix, and a
//! plant with a pipe through a turbine is a plant that has lied to them.

use super::kit::{Mat, Mesh};
use super::layout::{Layer, Plan, Socket};
use super::seed::Seed;
use super::{p3, paint, spin_for, Grade, Mm, Owner, Owns, Piece, Vol, CLOSE, FAR, MEDIUM, P3, SIX};
use crate::machine::design::Design;
use crate::machine::parts;
use crate::machine::stuff::{Domain, Subst};
use std::cmp::Reverse;
use std::collections::BinaryHeap;

/// One connection, routed.
#[derive(Clone, Debug)]
pub struct Run {
    /// `R1.heat -> HX1.heat`, which is also what the inspector calls it.
    pub name: String,
    pub dom: Domain,
    /// Experiment 09: what is actually in it, traced upstream through the
    /// document to whichever source is feeding it. Decides the colour, and
    /// nothing else.
    pub serve: Subst,
    pub bore: Mm,
    /// The bore at each end. They differ when a big machine feeds a small one,
    /// and a line that changes size wants a reducer rather than a step.
    pub ends: (Mm, Mm),
    /// Corner to corner, socket to socket.
    pub path: Vec<P3>,
    pub length: Mm,
    pub bends: usize,
    /// Where the structural pass will have to put something.
    pub props: Vec<P3>,
    /// Experiment 10: which set of rules this run had to be laid under, and
    /// whether it could be laid at all.
    pub tier: Tier,
}

/// How hard the router had to try.
///
/// Not a quality score -- a statement about what the player is looking at. A
/// `Tight` run is a real run that had to squeeze; a `Lost` one is a hole in
/// the plant, drawn as nothing, and reported as such.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord)]
pub enum Tier {
    Clean,
    Tight,
    Lost,
}

impl Tier {
    pub fn tag(self) -> &'static str {
        match self {
            Tier::Clean => "clean",
            Tier::Tight => "tight",
            Tier::Lost => "lost",
        }
    }
    pub fn said(self) -> &'static str {
        match self {
            Tier::Clean => "routed",
            Tier::Tight => "routed, but only by relaxing the rules",
            Tier::Lost => "no valid route found",
        }
    }
    pub fn lost(self) -> bool {
        self == Tier::Lost
    }
}

impl Run {
    /// Whether there is any pipe to draw. A lost run has a name, a domain and
    /// a bore, and no geometry whatsoever.
    pub fn laid(&self) -> bool {
        self.path.len() >= 2
    }
}

// ------------------------------------------------------------------- rules

/// What a domain's pipework is allowed to do.
///
/// The note asked for these by name. They are here rather than spread through
/// the search because a rule that is a magic number inside an inner loop is a
/// rule nobody can change, and half the value of writing them down is being
/// able to argue about them.
#[derive(Clone, Copy, Debug)]
pub struct Rules {
    /// The shortest straight section the router may produce between two bends.
    pub straight: Mm,
    /// The radius every corner is drawn at, and therefore the length each
    /// corner takes out of the straights either side of it.
    pub bend: Mm,
    /// Which storey the long middle of a run belongs on.
    pub layer: Layer,
    /// How far apart this domain's supports go.
    pub span: Mm,
    /// What a corner costs, in tenths of a cell.
    pub turn: u32,
}

impl Rules {
    /// The shortest section a run may contain, under one tier's rules.
    ///
    /// Two floors, and only one of them is negotiable. The domain's own
    /// minimum is a *style*: shafts run four metres straight because that is
    /// what a line shaft looks like, and a tight route is allowed to argue
    /// with it. The other is geometry -- a section with a bend on each end has
    /// to give up the radius twice and still be a section -- and nothing is
    /// allowed to argue with that, because the result is not a shorter run, it
    /// is two elbows drawn through each other.
    pub fn least(&self, tier: Tier) -> Mm {
        match tier {
            Tier::Clean => self.straight,
            // Twice the radius, plus the diameter it was derived from.
            _ => ((self.bend * 8) / 3).max(self.straight / 2),
        }
    }
}

/// The rules for one run, which depend on the domain and on how big the line
/// is: a 900mm lagged main is not allowed the bends a garden hose is.
pub fn rules(dom: Domain, bore: Mm) -> Rules {
    let t = treat(dom);
    let od = outer(dom, bore);
    // Three diameters of long-radius bend, which is what makes a corner look
    // like a fitting rather than a crease.
    let bend = (od * 3) / 2;
    Rules {
        // Twice the bend radius, plus a diameter to see it by: a straight that
        // cannot pay for the elbow on each of its ends is the defect that ate
        // forty per cent of experiment 08's corners.
        straight: (bend * 2 + od).max(t.min_straight),
        bend,
        layer: t.layer,
        span: t.span,
        turn: t.bend_cost,
    }
}

// -------------------------------------------------------------- treatments

struct Treat {
    mesh: Mesh,
    mat: Mat,
    /// Outside diameter, as a percentage of the port's bore.
    wide: i32,
    /// What goes round it, and how often.
    trim: Option<(Mesh, Mat, Mm)>,
    /// Whether the domain bends with an elbow, a mitre or not at all.
    elbow: bool,
    /// Charged per corner, in tenths of a cell.
    bend_cost: u32,
    /// The floor under `Rules::straight`: the shortest straight section this
    /// domain will tolerate however small the pipe is.
    min_straight: Mm,
    /// Which of the plant's storeys it belongs on.
    layer: Layer,
    /// How far apart it needs holding up.
    span: Mm,
}

/// The outside diameter of a run: what the pipe actually measures across, as
/// opposed to the bore the document asked for.
///
/// Public because the structural pass has to cradle these things, and a
/// support sized from the bore of a lagged main is a support that goes
/// straight through it.
pub fn outer(dom: Domain, bore: Mm) -> Mm {
    (bore * treat(dom).wide / 100).max(90)
}

fn treat(d: Domain) -> Treat {
    match d {
        Domain::Fluid => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Paint,
            wide: 100,
            trim: Some((Mesh::Flange, Mat::Steel, 4000)),
            elbow: true,
            bend_cost: 30,
            min_straight: 1200,
            layer: Layer::Ground,
            span: 4500,
        },
        Domain::Gas => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Steel,
            wide: 115,
            trim: Some((Mesh::Band, Mat::Lag, 1600)),
            elbow: true,
            bend_cost: 30,
            min_straight: 1500,
            layer: Layer::Rack,
            span: 4000,
        },
        Domain::Heat => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Lag,
            wide: 165,
            trim: Some((Mesh::Band, Mat::Steel, 800)),
            elbow: true,
            bend_cost: 34,
            min_straight: 2000,
            layer: Layer::Rack,
            span: 4000,
        },
        Domain::Rotary => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Steel,
            wide: 55,
            trim: Some((Mesh::Coupling, Mat::Steel, 3000)),
            elbow: false,
            // A shaft that bends is a gearbox nobody placed, so make the
            // router work very hard to avoid one.
            bend_cost: 260,
            // A shaft that bends is a gearbox nobody placed, so it is not
            // merely expensive to bend one -- there is no short straight it
            // could bend into.
            min_straight: 4000,
            layer: Layer::Drive,
            span: 4000,
        },
        Domain::Mech => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Steel,
            wide: 45,
            trim: None,
            elbow: false,
            bend_cost: 320,
            min_straight: 5000,
            layer: Layer::Drive,
            span: 0,
        },
        Domain::Electrical => Treat {
            mesh: Mesh::Box,
            mat: Mat::Galv,
            wide: 42,
            trim: Some((Mesh::Box, Mat::Dark, 2200)),
            elbow: false,
            bend_cost: 18,
            min_straight: 1000,
            layer: Layer::Tray,
            span: 4500,
        },
        Domain::Material => Treat {
            mesh: Mesh::Box,
            mat: Mat::Galv,
            wide: 230,
            trim: Some((Mesh::Band, Mat::Dark, 2600)),
            elbow: false,
            bend_cost: 48,
            min_straight: 2500,
            layer: Layer::Feed,
            span: 4500,
        },
    }
}

// -------------------------------------------------------------- the grid

/// Half a metre. Fine enough that a pipe threads between two machines, coarse
/// enough that a forty-metre plant is a hundred thousand cells.
const CELL: Mm = 500;
/// How far above the tallest thing the router may go.
const SKY: Mm = 3000;
/// The margin round the plot a pipe may use to get round the outside.
const MARGIN: Mm = 3000;

/// What is in a cell.
const SOLID: u8 = 1;
/// Somebody's service clearance: passable, expensive, and a rule the tight
/// tier is allowed to break more cheaply than the clean one.
const CLEAR: u8 = 2;
/// A laid route is in it. Nothing else may go through.
const TAKEN: u8 = 4;
/// Beside a laid route. Passable, but a line that runs down the side of
/// another line for twenty metres is what a rack is for, not what a pipe does.
const BESIDE: u8 = 8;

struct Grid {
    o: P3,
    n: (i32, i32, i32),
    cell: Mm,
    mark: Vec<u8>,
}

impl Grid {
    fn build(plan: &Plan) -> Grid {
        let mut v = plan.plot;
        for u in &plan.units {
            v = v.join(u.vol);
        }
        let lo = p3(v.lo.x - MARGIN, 0, v.lo.z - MARGIN);
        let hi = p3(v.hi.x + MARGIN, v.hi.y + SKY, v.hi.z + MARGIN);
        let mut cell = CELL;
        // Keep the search bounded on a plant the size of a small town.
        loop {
            let n = (
                ((hi.x - lo.x) / cell + 1).max(2),
                ((hi.y - lo.y) / cell + 1).max(2),
                ((hi.z - lo.z) / cell + 1).max(2),
            );
            if (n.0 as i64) * (n.1 as i64) * (n.2 as i64) <= 400_000 || cell >= 2000 {
                let mut g = Grid { o: lo, n, cell, mark: vec![0; (n.0 * n.1 * n.2) as usize] };
                for u in &plan.units {
                    g.fill(u.vol, SOLID);
                    g.fill(u.clear, CLEAR);
                }
                return g;
            }
            cell *= 2;
        }
    }

    fn idx(&self, c: (i32, i32, i32)) -> usize {
        ((c.0 * self.n.1 + c.1) * self.n.2 + c.2) as usize
    }

    fn inside(&self, c: (i32, i32, i32)) -> bool {
        c.0 >= 0 && c.1 >= 0 && c.2 >= 0 && c.0 < self.n.0 && c.1 < self.n.1 && c.2 < self.n.2
    }

    fn cell_of(&self, p: P3) -> (i32, i32, i32) {
        (
            ((p.x - self.o.x) / self.cell).clamp(0, self.n.0 - 1),
            ((p.y - self.o.y) / self.cell).clamp(0, self.n.1 - 1),
            ((p.z - self.o.z) / self.cell).clamp(0, self.n.2 - 1),
        )
    }

    fn world(&self, c: (i32, i32, i32)) -> P3 {
        p3(
            self.o.x + c.0 * self.cell + self.cell / 2,
            self.o.y + c.1 * self.cell + self.cell / 2,
            self.o.z + c.2 * self.cell + self.cell / 2,
        )
    }

    fn fill(&mut self, v: Vol, bit: u8) {
        let a = self.cell_of(v.lo);
        let b = self.cell_of(v.hi);
        for x in a.0..=b.0 {
            for y in a.1..=b.1 {
                for z in a.2..=b.2 {
                    let i = self.idx((x, y, z));
                    self.mark[i] |= bit;
                }
            }
        }
    }

    /// Millimetres to whole cells, never fewer than one, and rounded *up*: a
    /// rule the grid cannot represent is a rule that quietly does not apply,
    /// which is worse than not having written it down.
    ///
    /// Up rather than down for exactly that reason. Rounding a 1200mm minimum
    /// down to two cells makes it a 1000mm minimum, and then the code says one
    /// thing and the plant does another -- which is how a rule turns into a
    /// comment.
    fn cells(&self, mm: Mm) -> i32 {
        ((mm + self.cell - 1) / self.cell).max(1)
    }

    /// Claim a laid route, and charge for the corridor beside it.
    fn claim(&mut self, cells: &[(i32, i32, i32)]) {
        for &c in cells {
            if !self.inside(c) {
                continue;
            }
            let i = self.idx(c);
            self.mark[i] |= TAKEN;
        }
        for &c in cells {
            for d in SIX {
                let n = (c.0 + d.x, c.1 + d.y, c.2 + d.z);
                if !self.inside(n) {
                    continue;
                }
                let i = self.idx(n);
                if self.mark[i] & TAKEN == 0 {
                    self.mark[i] |= BESIDE;
                }
            }
        }
    }
}

// -------------------------------------------------------------- the search

/// The whole document, routed.
pub fn run(d: &Design, plan: &Plan, seed: &Seed) -> Vec<Run> {
    let _ = seed;
    let mut g = Grid::build(plan);
    let links = match d.links() {
        Ok(l) => l,
        Err(_) => return Vec::new(),
    };

    // Widest first, then in document order: big lines get the good routes, and
    // adding a small one later cannot shove a main out of the way.
    let mut order: Vec<usize> = (0..links.len()).collect();
    order.sort_by_key(|&i| {
        let l = links[i];
        let rate = parts::part(d.units[l.from].kind).ports[l.from_port].rate;
        (Reverse(rate), i)
    });

    let mut done: Vec<(usize, Run)> = Vec::with_capacity(links.len());
    // What each laid run took, and the two flanges it took it between. A line
    // that leaves the same nozzle as one already laid is a *branch*, and a
    // branch shares its parent's corridor -- which is exactly the situation
    // `junctions` already assumes when it puts a tee in. Without this, the
    // second line off a socket has to fight the first one for the same
    // half-metre of air and loses, which is a strange way to draw a tee.
    let mut laid: Vec<(P3, P3, Vec<(i32, i32, i32)>)> = Vec::new();

    for &i in &order {
        let l = links[i];
        let (a, b) = (&plan.units[l.from], &plan.units[l.to]);
        let (Some(sa), Some(sb)) = (a.socket(l.from_port), b.socket(l.to_port)) else {
            continue;
        };
        let dom = parts::part(a.kind).ports[l.from_port].dom;
        let name = format!(
            "{}.{} -> {}.{}",
            d.wires[i].from, d.wires[i].from_port, d.wires[i].to, d.wires[i].to_port
        );
        let serve = paint::service(d, l.from, l.from_port);

        let kin: Vec<usize> = laid
            .iter()
            .enumerate()
            .filter(|(_, (p, q, _))| {
                *p == sa.at || *q == sa.at || *p == sb.at || *q == sb.at
            })
            .map(|(k, _)| k)
            .collect();
        let held: Vec<Vec<(usize, u8)>> =
            kin.iter().map(|&k| release(&mut g, &laid[k].2)).collect();

        let (r, taken) = one(&mut g, sa, sb, dom, serve, name);

        for saved in held {
            for (i, m) in saved {
                g.mark[i] |= m;
            }
        }
        laid.push((sa.at, sb.at, taken));
        done.push((i, r));
    }
    // Back into document order, so that two designs that differ only in the
    // order two wires were drawn still hash the same.
    done.sort_by_key(|(i, _)| *i);
    done.into_iter().map(|(_, r)| r).collect()
}

/// Give a laid run's cells back for the duration of one search, remembering
/// what they were so they can be handed straight back.
fn release(g: &mut Grid, cells: &[(i32, i32, i32)]) -> Vec<(usize, u8)> {
    let mut saved = Vec::with_capacity(cells.len());
    for &c in cells {
        if !g.inside(c) {
            continue;
        }
        let i = g.idx(c);
        saved.push((i, g.mark[i] & (TAKEN | BESIDE)));
        g.mark[i] &= !(TAKEN | BESIDE);
    }
    saved
}

/// One connection, at the first tier that can carry it.
fn one(
    g: &mut Grid,
    sa: &Socket,
    sb: &Socket,
    dom: Domain,
    serve: Subst,
    name: String,
) -> (Run, Vec<(i32, i32, i32)>) {
    let bore = sa.bore.max(sb.bore);
    let rule = rules(dom, bore);

    let mut laid: Option<(Tier, Vec<(i32, i32, i32)>, Vec<P3>)> = None;
    for tier in [Tier::Clean, Tier::Tight] {
        // The socket cells are inside their own machine's solid, so a pocket
        // is opened around each -- and *only* around each, because opening the
        // whole component lets the pipe leave through the far wall, which
        // looks exactly as wrong as it sounds.
        let held = [pocket(g, sa.at), pocket(g, sb.at)];
        let got = attempt(g, sa, sb, &rule, tier);
        for saved in held {
            for (i, m) in saved {
                g.mark[i] = m;
            }
        }
        if let Some((cells, path)) = got {
            laid = Some((tier, cells, path));
            break;
        }
    }

    let (tier, path, took) = match laid {
        Some((tier, cells, path)) => {
            g.claim(&cells);
            (tier, path, cells)
        }
        // No valid route found. Nothing is drawn, and the plant says so.
        None => (Tier::Lost, Vec::new(), Vec::new()),
    };

    let mut length = 0;
    let mut bends = 0;
    for i in 1..path.len() {
        length += path[i].sub(path[i - 1]).len();
        if i + 1 < path.len() && turns(path[i - 1], path[i], path[i + 1]) {
            bends += 1;
        }
    }
    let props = if path.len() >= 2 { props_along(&path, rule.span) } else { Vec::new() };
    (
        Run { name, dom, serve, bore, ends: (sa.bore, sb.bore), path, length, bends, props, tier },
        took,
    )
}

/// One try, under one tier's rules. `None` means there is no such route, which
/// is a legitimate answer and the point of the whole exercise.
fn attempt(
    g: &Grid,
    sa: &Socket,
    sb: &Socket,
    rule: &Rules,
    tier: Tier,
) -> Option<(Vec<(i32, i32, i32)>, Vec<P3>)> {
    // What the tight tier relaxes: the straight off the flange, the *stylistic*
    // half of the straight between bends, and permission to share a corridor.
    // What it never relaxes is the geometric half -- see `Rules::least`. That
    // one is the whole reason the search walks sections instead of cells, and
    // giving it up hands back the staircases experiment 10 exists to remove. A
    // run that cannot be laid without them is better lost.
    let ease = if tier == Tier::Tight { 2 } else { 1 };
    let stub_a = g.cells(sa.stub / ease);
    let stub_b = g.cells(sb.stub / ease);
    let straight = g.cells(rule.least(tier));

    // Two flanges pointing at each other on one axis are *coupled*, and a
    // coupling is not a routing problem. This is the commonest connection in
    // the plant -- a turbine driving the generator bolted to the end of it --
    // and under the gate rule below it would be refused, because both stubs
    // want more room than there is between the two machines. There is no bend
    // to keep clear of: there is no bend.
    if let Some(path) = coupled(g, sa, sb, tier) {
        let mut taken = Vec::new();
        walk(g, path[0], path[1], &mut taken);
        return Some((taken, path));
    }

    // The gates: where the first bend is allowed to be, and where the last one
    // is. Between the flange and its gate the line is straight by definition,
    // which is the note's "minimum straight section before bend" made true by
    // construction rather than by hoping.
    let a0 = g.cell_of(sa.at);
    let b0 = g.cell_of(sb.at);
    let a1 = step(a0, sa.out, stub_a);
    let b1 = step(b0, sb.out, stub_b);
    if !g.inside(a1) || !g.inside(b1) {
        return None;
    }
    // Both stubs have to actually be clear, or the flange is inside something.
    if !clear_run(g, a0, sa.out, stub_a, tier) || !clear_run(g, b0, sb.out, stub_b, tier) {
        return None;
    }
    // The far gate is where the *last bend* is allowed to be, so the line may
    // arrive at it from any direction at all -- what the rule guarantees is
    // the straight between it and the flange, and that is guaranteed by there
    // being no corner in it.
    //
    // The one arrival that is not allowed is straight back out of the machine,
    // because turning round at the gate is not a bend, it is a mistake. This
    // was the first version's error and it cost two runs in every design: a
    // line was made to approach a flange along the flange's own axis for four
    // metres, which is a fine rule for a pipe rack and an impossible one for a
    // machine with two metres of yard beside it.
    let cells = search(g, a1, sa.out, stub_a, b1, sb.out, stub_b, straight, rule, tier)?;

    // Socket, gate, corners, gate, socket -- then let `simplify` collapse the
    // ones that turned out to be collinear.
    let mut path = Vec::with_capacity(cells.len() + 4);
    path.push(sa.at);
    path.push(sa.at.add(sa.out.mul(stub_a * g.cell)));
    for c in cells.iter().skip(1).take(cells.len().saturating_sub(2)) {
        path.push(g.world(*c));
    }
    path.push(sb.at.add(sb.out.mul(stub_b * g.cell)));
    path.push(sb.at);
    let path = square(simplify(path));

    // The cells this run has now taken: every cell of every section, so the
    // next route has to go round rather than through.
    let mut taken = Vec::new();
    for i in 1..path.len() {
        walk(g, path[i - 1], path[i], &mut taken);
    }
    Some((taken, path))
}

/// The straight line between two flanges that are already facing each other.
///
/// `None` unless the two sockets are on one axis, pointing at each other, with
/// clear air in between -- in which case the answer is two points and no
/// search at all. It is also the only route a shaft is ever really allowed:
/// `space` will call anything else misaligned.
fn coupled(g: &Grid, sa: &Socket, sb: &Socket, tier: Tier) -> Option<Vec<P3>> {
    if sa.out != sb.out.neg() {
        return None;
    }
    let d = sb.at.sub(sa.at);
    // On the axis, pointing the right way along it, and actually apart.
    if d.axis() != sa.out.axis() {
        return None;
    }
    let along = d.x * sa.out.x + d.y * sa.out.y + d.z * sa.out.z;
    if along <= 0 {
        return None;
    }
    let mut cells = Vec::new();
    walk(g, sa.at, sb.at, &mut cells);
    // Skip the two end cells: those are inside the machines the flanges are
    // bolted to, and a flange is allowed to be inside its own machine.
    for c in cells.iter().skip(1).take(cells.len().saturating_sub(2)) {
        if !g.inside(*c) {
            return None;
        }
        let m = g.mark[g.idx(*c)];
        if m & SOLID != 0 {
            return None;
        }
        if m & TAKEN != 0 && tier == Tier::Clean {
            return None;
        }
    }
    Some(vec![sa.at, sb.at])
}

fn step(c: (i32, i32, i32), d: P3, k: i32) -> (i32, i32, i32) {
    (c.0 + d.x * k, c.1 + d.y * k, c.2 + d.z * k)
}

/// Whether the `k` cells out of a socket are actually available.
fn clear_run(g: &Grid, from: (i32, i32, i32), d: P3, k: i32, tier: Tier) -> bool {
    for i in 1..=k {
        let c = step(from, d, i);
        if !g.inside(c) {
            return false;
        }
        let m = g.mark[g.idx(c)];
        if m & SOLID != 0 {
            return false;
        }
        if m & TAKEN != 0 && tier == Tier::Clean {
            return false;
        }
    }
    true
}

/// Every cell a straight section passes through.
fn walk(g: &Grid, a: P3, b: P3, out: &mut Vec<(i32, i32, i32)>) {
    let d = b.sub(a);
    let n = (d.len() / g.cell).max(1);
    for i in 0..=n {
        out.push(g.cell_of(p3(a.x + d.x * i / n, a.y + d.y * i / n, a.z + d.z * i / n)));
    }
}

/// A* over `(corner, heading)`, where an edge is a *straight section* rather
/// than a step.
///
/// This is the whole of experiment 10's routing change. Because the shortest
/// edge in the graph is `straight` cells long, there is no path in the search
/// space that bends twice inside a metre -- so the minimum-straight rule is
/// not a penalty that a determined enough cost function can talk its way out
/// of, it is a property of what "path" means here.
///
/// It also collapses the search. Experiment 08 expanded a hundred thousand
/// cells to cross a plant; this expands a few hundred corners, and spends the
/// difference on being able to afford the rules.
#[allow(clippy::too_many_arguments)]
fn search(
    g: &Grid,
    start: (i32, i32, i32),
    head: P3,
    // How many cells the line has already travelled in `head` to get here: the
    // stub off the flange, which is part of the same straight section as
    // anything that carries on in the same direction.
    run_in: i32,
    goal: (i32, i32, i32),
    // `away` is the one direction the goal may not be arrived from: back out
    // of the machine it belongs to. `run_out` is the stub on the far side of
    // it, which is part of the same section as an arrival along that axis.
    away: P3,
    run_out: i32,
    straight: i32,
    rule: &Rules,
    tier: Tier,
) -> Option<Vec<(i32, i32, i32)>> {
    let cells = (g.n.0 * g.n.1 * g.n.2) as usize;
    let n = cells * 6;
    let mut dist: Vec<u32> = vec![u32::MAX; n];
    let mut prev: Vec<u32> = vec![u32::MAX; n];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();

    let dir_of = |d: P3| SIX.iter().position(|&s| s == d).unwrap_or(0);
    let key = |c: (i32, i32, i32), d: usize| g.idx(c) * 6 + d;
    let hcost = |c: (i32, i32, i32)| {
        (((c.0 - goal.0).abs() + (c.1 - goal.1).abs() + (c.2 - goal.2).abs()) * 10) as u32
    };

    let s = key(start, dir_of(head));
    dist[s] = 0;
    heap.push(Reverse((hcost(start), s as u32)));

    let mut seen = 0usize;
    let mut best: Option<u32> = None;
    while let Some(Reverse((_, k))) = heap.pop() {
        let k = k as usize;
        let cell = k / 6;
        let from = SIX[k % 6];
        let c = (
            (cell as i32) / (g.n.1 * g.n.2),
            ((cell as i32) / g.n.2) % g.n.1,
            (cell as i32) % g.n.2,
        );
        if c == goal {
            best = Some(k as u32);
            break;
        }
        seen += 1;
        if seen > 200_000 {
            break;
        }
        let d0 = dist[k];
        let at_start = k == s;
        for &d in SIX.iter() {
            // A section never doubles back. Nor does it continue in the
            // direction it arrived in -- that is not a new section, it is the
            // same one, and it is already represented by one longer edge.
            //
            // The exception is the very first node, whose heading is not a
            // section this search laid but the stub off the flange. Carrying
            // straight on out of a nozzle is a run, not a bend, and refusing
            // to consider it was worth four lost drive shafts.
            if d == from.neg() || (d == from && !at_start) {
                continue;
            }
            let turn = if d == from { 0 } else { rule.turn };
            let mut run = 0u32;
            let mut at;
            for i in 1.. {
                let next = step(c, d, i);
                if !g.inside(next) {
                    break;
                }
                let Some(cost) = step_cost(g, next, rule, tier) else { break };
                run += cost;
                at = next;
                // The goal is a corner like any other, except that it may only
                // be arrived at along the flange normal.
                // Continuing straight out of the flange is *one section with
                // the stub*, so what it owes is the minimum less what the stub
                // has already paid. Anything else owes the whole minimum.
                //
                // Leaving this out was worth one bad corner in every design:
                // a line would leave a flange, run half a metre, and turn --
                // a straight section a fifth of the length of the elbow that
                // was then drawn on the end of it.
                let mut need = straight;
                if at_start && d == from {
                    need = (straight - run_in).max(1);
                }
                let long_enough = i >= need;
                let is_goal = at == goal;
                if is_goal && d == away {
                    break;
                }
                // Arriving at the far gate is a corner like any other, and
                // the section into it owes the same minimum -- less the stub
                // beyond it, when the line carries straight on through into
                // the flange. Letting the goal be reached by any edge at all
                // was worth one short section on the far end of every run,
                // which is the end a viewer is looking at.
                if is_goal {
                    let owed = if d == away.neg() { (straight - run_out).max(1) } else { straight };
                    if i < owed {
                        break;
                    }
                } else if !long_enough {
                    continue;
                }
                let nk = key(at, dir_of(d));
                let nd = d0 + run + turn;
                if nd < dist[nk] {
                    dist[nk] = nd;
                    prev[nk] = k as u32;
                    heap.push(Reverse((nd + hcost(at), nk as u32)));
                }
                if is_goal {
                    break;
                }
            }
        }
    }

    let mut k = best?;
    let mut out = Vec::new();
    loop {
        let cell = (k as usize) / 6;
        out.push((
            (cell as i32) / (g.n.1 * g.n.2),
            ((cell as i32) / g.n.2) % g.n.1,
            (cell as i32) % g.n.2,
        ));
        let p = prev[k as usize];
        if p == u32::MAX {
            break;
        }
        k = p;
    }
    out.reverse();
    Some(out)
}

/// What one cell of travel costs, or `None` if the line may not go there.
///
/// The four terms are the note's four, and the layer term is the one that does
/// the visible work: a run that is not yet where it is going would rather be
/// on its own storey than anywhere else, so lines climb to the rack, travel,
/// and come down -- which is what a plant looks like.
fn step_cost(g: &Grid, c: (i32, i32, i32), rule: &Rules, tier: Tier) -> Option<u32> {
    let m = g.mark[g.idx(c)];
    if m & SOLID != 0 {
        return None;
    }
    if m & TAKEN != 0 && tier == Tier::Clean {
        return None;
    }
    let mut cost = 10u32;
    if m & TAKEN != 0 {
        cost += 60;
    }
    if m & BESIDE != 0 {
        cost += 8;
    }
    if m & CLEAR != 0 {
        cost += if tier == Tier::Clean { 26 } else { 10 };
    }
    let y = g.o.y + c.1 * g.cell;
    cost += (((y - rule.layer.y()).abs() / g.cell).min(10) as u32) * 4;
    // Nothing wants to lie across the walkway.
    if c.1 <= 1 {
        cost += 14;
    }
    Some(cost)
}

/// Open the cells around one socket, and remember what they were.
fn pocket(g: &mut Grid, at: P3) -> Vec<(usize, u8)> {
    let c = g.cell_of(at);
    let mut saved = Vec::with_capacity(27);
    for dx in -1..=1 {
        for dy in -1..=1 {
            for dz in -1..=1 {
                let n = (c.0 + dx, c.1 + dy, c.2 + dz);
                if !g.inside(n) {
                    continue;
                }
                let i = g.idx(n);
                saved.push((i, g.mark[i]));
                g.mark[i] &= !(SOLID | TAKEN);
            }
        }
    }
    saved
}

/// Square the path off.
///
/// The search works in cell centres and the sockets do not sit on them, so the
/// section leaving a flange and the section arriving at one can each be a few
/// centimetres off axis. Rather than leave a diagonal in an orthogonal plant,
/// the gate point is moved onto the flange's own two axes -- it is a corner
/// either way, and this way it is a corner at a right angle.
fn square(mut path: Vec<P3>) -> Vec<P3> {
    let n = path.len();
    if n < 3 {
        return path;
    }
    for (i, j) in [(0usize, 1usize), (n - 1, n - 2)] {
        let (a, b) = (path[i], path[j]);
        let d = b.sub(a);
        path[j] = match d.axis() {
            Some(0) => p3(b.x, a.y, a.z),
            Some(1) => p3(a.x, b.y, a.z),
            Some(2) => p3(a.x, a.y, b.z),
            // Not on an axis at all: keep whichever component is largest and
            // let the elbow at the next corner absorb the rest.
            _ if d.x.abs() >= d.y.abs() && d.x.abs() >= d.z.abs() => p3(b.x, a.y, a.z),
            _ if d.y.abs() >= d.z.abs() => p3(a.x, b.y, a.z),
            _ => p3(a.x, a.y, b.z),
        };
    }
    path.dedup();
    path
}

/// Collinear points are not corners.
fn simplify(mut p: Vec<P3>) -> Vec<P3> {
    p.dedup();
    let mut out: Vec<P3> = Vec::with_capacity(p.len());
    for (i, q) in p.iter().copied().enumerate() {
        if i == 0 || i + 1 == p.len() {
            out.push(q);
            continue;
        }
        if turns(p[i - 1], q, p[i + 1]) {
            out.push(q);
        }
    }
    out.dedup();
    out
}

/// Which of a run's corners are drawn as real elbows, exactly as `dress`
/// decides it.
///
/// Public so the invariant can be asserted from outside: no straight is ever
/// asked to give up more length than it has.
pub fn elbows_of(r: &Run) -> Vec<bool> {
    elbows(&r.path, bend_of(r), treat(r.dom).elbow)
}

/// The bend radius a run's elbows are drawn at, which is the length each of
/// them takes out of the straight either side of it.
///
/// One number, read by the router when it decides how long a straight has to
/// be and by `dress` when it puts an elbow in one. Experiment 09 found out the
/// hard way what happens when two places decide the same thing separately.
pub fn bend_of(r: &Run) -> Mm {
    rules(r.dom, r.bore).bend
}

/// Which corners of a path get a real elbow, decided once.
///
/// This used to be decided twice -- once by the loop that shortens the
/// straights to leave room for a bend, and once by the loop that puts the bend
/// in -- with two different tests, and two different tests are two different
/// answers. A run would have most of a metre of pipe taken out of it for an
/// elbow that the second loop then declined to fit, and the corner came out as
/// a hole with a stub either side of it. It was a little over forty per cent
/// of every corner in the repository.
///
/// So the decision is made here, in one place, and every loop reads it.
///
/// An elbow eats `bend` from the straight either side of it, and a straight
/// with an elbow on both ends has to be able to pay twice. The budget is spent
/// greedily in path order, which is arbitrary and, much more importantly,
/// fixed: the same path always spends it the same way.
fn elbows(path: &[P3], bend: Mm, allowed: bool) -> Vec<bool> {
    let mut out = vec![false; path.len()];
    if !allowed || path.len() < 3 {
        return out;
    }
    let mut left: Vec<Mm> = (0..path.len())
        .map(|i| if i == 0 { 0 } else { path[i].sub(path[i - 1]).len() })
        .collect();
    for i in 1..path.len() - 1 {
        let (a, b, c) = (path[i - 1], path[i], path[i + 1]);
        let (u, v) = (b.sub(a), c.sub(b));
        if !turns(a, b, c) || !u.is_axis() || !v.is_axis() {
            continue;
        }
        // Strictly greater, so that a straight always survives its own bends
        // rather than being spent down to nothing.
        if left[i] > bend && left[i + 1] > bend {
            out[i] = true;
            left[i] -= bend;
            left[i + 1] -= bend;
        }
    }
    out
}

fn turns(a: P3, b: P3, c: P3) -> bool {
    let (u, v) = (b.sub(a), c.sub(b));
    // Parallel if the cross product vanishes; scaled down to keep it in range.
    let cx = (u.y / 10) * (v.z / 10) - (u.z / 10) * (v.y / 10);
    let cy = (u.z / 10) * (v.x / 10) - (u.x / 10) * (v.z / 10);
    let cz = (u.x / 10) * (v.y / 10) - (u.y / 10) * (v.x / 10);
    cx != 0 || cy != 0 || cz != 0
}

/// Where this run will need holding up: every few metres of travel, wherever
/// it happens to be horizontal when the tape runs out. The structural pass
/// decides what that turns into.
///
/// The distance is measured along the whole run rather than along each
/// straight, because a thirteen-metre span made of eight short sections is
/// still a thirteen-metre span, and the first version of this function
/// cheerfully left it hanging in the air.
fn props_along(path: &[P3], gap: Mm) -> Vec<P3> {
    if gap <= 0 {
        return Vec::new();
    }
    let mut out = Vec::new();
    let mut since = gap / 2;
    for i in 1..path.len() {
        let (a, b) = (path[i - 1], path[i]);
        let len = b.sub(a).len();
        if len == 0 {
            continue;
        }
        if a.y != b.y {
            // Going up or down is not a span, but the tape keeps running:
            // a riser does not reset the need for the next support.
            since += len / 2;
            continue;
        }
        let mut t = 0;
        while since + (len - t) >= gap {
            t += gap - since;
            since = 0;
            out.push(p3(a.x + (b.x - a.x) * t / len, a.y, a.z + (b.z - a.z) * t / len));
        }
        since += len - t;
    }
    out
}

// ------------------------------------------------------------- the pipework

/// A routed connection, as pieces.
pub fn dress(r: &Run, seed: &Seed, grade: Grade, id: u16, out: &mut Vec<Piece>) {
    // A run the router could not lay is drawn as nothing at all. That is the
    // entire behavioural difference experiment 10 asked for here, and it is
    // worth more than the rest of this function put together: the plant now
    // shows a hole where the player has asked for something impossible,
    // instead of a pipe through a machine.
    if !r.laid() {
        return;
    }
    let t = treat(r.dom);
    let od = outer(r.dom, r.bore);
    let bend = bend_of(r);
    let mut rng = seed.at(&r.name, "run");
    let n0 = out.len();
    // One decision, read by the straights, by the corners and by the lagging.
    let bent = elbows(&r.path, bend, t.elbow);

    for i in 1..r.path.len() {
        let (mut a, mut b) = (r.path[i - 1], r.path[i]);
        let seg = b.sub(a);
        let d = seg.len();
        if d == 0 {
            continue;
        }
        // Make room for the elbows this segment runs into -- for exactly the
        // elbows that are going to arrive, and no others.
        if bent[i - 1] {
            a = a.add(unit_mm(seg, bend));
        }
        if bent[i] {
            b = b.sub(unit_mm(seg, bend));
        }
        let len = b.sub(a).len();
        if len <= 0 {
            continue;
        }
        let mut piece = Piece::span(t.mesh, t.mat, a, b, od);
        // A square section wants to sit square with the world.
        if t.mesh == Mesh::Box {
            piece = piece.spin(0);
        }
        // Only the mains survive to the far view: at that distance a plant is
        // its equipment and its big lines, and everything else is a smudge.
        out.push(piece.lod(if od >= 420 { FAR } else { MEDIUM }));

        // What goes round it: flanges, bands, couplings, clips.
        if let Some((tm, tmat, gap)) = t.trim {
            let n = len / gap;
            for k in 1..=n {
                let at = a.add(unit_mm(b.sub(a), (k * len) / (n + 1)));
                let w = match tm {
                    Mesh::Flange => od * 14 / 10,
                    Mesh::Coupling => od * 16 / 10,
                    Mesh::Box => od * 13 / 10,
                    _ => od * 12 / 10,
                };
                let thick = match tm {
                    Mesh::Coupling => od * 2,
                    Mesh::Flange => od / 3,
                    _ => od / 5,
                };
                out.push(
                    Piece::new(tm, tmat, at, b.sub(a), p3(w, thick, w))
                        .lod(if tm == Mesh::Coupling { MEDIUM } else { CLOSE }),
                );
            }
        }
    }

    // The corners.
    for i in 1..r.path.len().saturating_sub(1) {
        let (a, b, c) = (r.path[i - 1], r.path[i], r.path[i + 1]);
        let (u, v) = (b.sub(a), c.sub(b));
        if !turns(a, b, c) || !u.is_axis() || !v.is_axis() {
            continue;
        }
        if bent[i] {
            let at = b.sub(unit_mm(u, bend));
            out.push(
                Piece::new(Mesh::Elbow, t.mat, at, u, p3(od, od, od))
                    .spin(spin_for(u, v))
                    .lod(MEDIUM),
            );
        } else {
            // A mitre: two stubs and no pretending. Shafts and conduit do
            // this, and so does any corner too tight for a bend radius -- on
            // the big lines that is most of them, because a heat main is 858mm
            // across and the router's steps are 500, so the pipe is wider than
            // the jogs in its own path and no elbow could physically fit.
            //
            // The stubs run back *into* the straights rather than out past the
            // corner. Now that a straight is only ever trimmed for an elbow
            // that actually arrives, the two straights meet at the corner by
            // themselves, and anything carried beyond it is not a mitre, it is
            // a lump on the outside of the bend.
            out.push(Piece::new(t.mesh, t.mat, b, u.neg(), p3(od, od / 2, od)).lod(MEDIUM));
            out.push(Piece::new(t.mesh, t.mat, b, v, p3(od, od / 2, od)).lod(MEDIUM));
        }
    }

    // Both ends, bolted.
    for (p, q) in [(r.path[0], r.path[1]), (r.path[r.path.len() - 1], r.path[r.path.len() - 2])] {
        out.push(
            Piece::new(Mesh::Flange, Mat::Steel, p, q.sub(p), p3(od * 15 / 10, od / 3, od * 15 / 10))
                .lod(MEDIUM),
        );
    }

    // One valve on a fluid line that is long enough to want one, and a bearing
    // wherever a shaft crosses a support. Both are dressing; neither is load
    // bearing in any sense the simulator would recognise.
    if r.dom == Domain::Fluid && r.length > 6000 && rng.chance(70) {
        // On the line, and along it. This used to be built twice the width of
        // its own pipe and pointed due east whatever the pipe was doing, so on
        // any run that was not going east it was a barrel of nothing sticking
        // out sideways through the middle of a straight.
        if let Some(&at) = r.props.first() {
            if let Some(d) = heading_at(&r.path, at) {
                out.push(
                    Piece::new(
                        Mesh::Valve,
                        Mat::Paint,
                        at.sub(unit_mm(d, od * 3 / 5)),
                        d,
                        p3(od * 13 / 10, od * 12 / 10, od * 13 / 10),
                    )
                    .lod(CLOSE),
                );
            }
        }
    }

    if grade.detailed() {
        vocabulary(r, seed, od, out);
    }

    for p in out[n0..].iter_mut() {
        p.of = id;
    }
}

/// Experiment 09, section 2: the same routing, with the vocabulary of how a
/// line is actually *made*.
///
/// Nothing here moves a pipe. Every piece is placed on the path the router
/// already found, at a point that path already passes through:
///
/// ```text
///   a bolted joint at every equipment interface -- a pair of flanges, not one
///   an isolation valve where a line leaves a machine
///   a reducer where the two ends are not the same size
///   a clamp wherever a run crosses one of its own supports
///   a lagging collar either side of every elbow on a hot line
///   a pressure gauge on a third of the process lines
/// ```
///
/// The note's claim was that industrial scenes get believable very quickly
/// when the connections look *engineered* rather than merely connected. That
/// is this function, and it is six rules long.
fn vocabulary(r: &Run, seed: &Seed, od: Mm, out: &mut Vec<Piece>) {
    if r.path.len() < 2 {
        return;
    }
    let mut rng = seed.at(&r.name, "vocabulary");
    let bolted = !matches!(r.dom, Domain::Rotary | Domain::Mech | Domain::Electrical);

    // Both ends, properly. Experiment 08 put one flange on each end of a run;
    // a joint is two flanges and a gap, and the difference is most of why a
    // pipe looks bolted to a machine rather than pushed into it.
    let last = r.path.len() - 1;
    for (end, p, q) in [(0usize, r.path[0], r.path[1]), (1, r.path[last], r.path[last - 1])] {
        let d = q.sub(p);
        let run = d.len();
        if run == 0 {
            continue;
        }
        if bolted {
            out.push(
                Piece::new(
                    Mesh::Flange,
                    Mat::Steel,
                    p.add(unit_mm(d, od / 2)),
                    d,
                    p3(od * 15 / 10, od / 3, od * 15 / 10),
                )
                .lod(CLOSE),
            );
        }
        // A line that changes size does it once, near the end that wanted the
        // smaller bore, rather than by quietly being two sizes at once.
        let (mine, theirs) = if end == 0 { (r.ends.0, r.ends.1) } else { (r.ends.1, r.ends.0) };
        if bolted && mine + 60 < theirs && run > od * 5 {
            out.push(
                Piece::new(
                    Mesh::Reducer,
                    Mat::Steel,
                    p.add(unit_mm(d, od * 3 / 2)),
                    d.neg(),
                    p3(od, od * 6 / 5, od),
                )
                .lod(MEDIUM),
            );
        }
        // An isolation valve on anything anybody would ever want to shut off,
        // on the machine's side of the run.
        let wants_valve = matches!(r.dom, Domain::Fluid | Domain::Gas | Domain::Heat);
        if wants_valve && end == 0 && run > od * 8 && r.length > 3000 {
            out.push(
                Piece::new(
                    Mesh::Valve,
                    Mat::Steel,
                    p.add(unit_mm(d, od * 3)),
                    d,
                    p3(od * 13 / 10, od * 12 / 10, od * 13 / 10),
                )
                .lod(CLOSE),
            );
        }
    }

    // A clamp where the run meets each of its own supports. The support itself
    // belongs to the structural pass; what holds the pipe *down onto* it
    // belongs here, and the two agree because both are derived from `props`.
    for &at in &r.props {
        if at.y < 900 {
            continue;
        }
        let Some(d) = heading_at(&r.path, at) else { continue };
        out.push(
            Piece::new(Mesh::Clamp, Mat::Dark, at.sub(unit_mm(d, od / 6)), d, p3(od * 12 / 10, od / 3, od * 12 / 10))
                .lod(CLOSE),
        );
    }

    // Lagging is not continuous: it stops at every fitting and is made off
    // against a collar. Only the hot domains have any to make off.
    if matches!(r.dom, Domain::Heat | Domain::Gas) {
        let bend = bend_of(r);
        let mat = if r.dom == Domain::Heat { Mat::Steel } else { Mat::Lag };
        // The same decision again, because a collar made off against an elbow
        // that is not there is a ring of lagging floating in a straight.
        let bent = elbows_of(r);
        for i in 1..r.path.len().saturating_sub(1) {
            let (a, b, c) = (r.path[i - 1], r.path[i], r.path[i + 1]);
            let (u, v) = (b.sub(a), c.sub(b));
            if !bent[i] {
                continue;
            }
            out.push(
                Piece::new(Mesh::Band, mat, b.sub(unit_mm(u, bend + od / 4)), u, p3(od * 12 / 10, od / 4, od * 12 / 10))
                    .lod(CLOSE),
            );
            out.push(
                Piece::new(Mesh::Band, mat, b.add(unit_mm(v, bend)), v, p3(od * 12 / 10, od / 4, od * 12 / 10))
                    .lod(CLOSE),
            );
        }
    }

    // A gauge where a process line leaves its machine, on a third of them. The
    // seed decides which, from a stream of its own, so that adding the whole
    // vocabulary cannot disturb a single thing experiment 08 already chose.
    if matches!(r.dom, Domain::Fluid | Domain::Gas | Domain::Heat) && rng.chance(35) {
        let (p, q) = (r.path[0], r.path[1]);
        let d = q.sub(p);
        if d.len() > od * 6 {
            out.push(
                Piece::new(Mesh::Gauge, Mat::Steel, p.add(unit_mm(d, od * 5)), super::right_of(d, 0), p3(240, 300, 240))
                    .lod(CLOSE),
            );
        }
    }
}

/// Which way the run is travelling where it passes through `at`.
///
/// A prop is always on a segment of the path, because that is where
/// `props_along` put it -- but it is looked up rather than assumed, because
/// hanging a clamp in mid-air would be a very quiet way to be wrong.
fn heading_at(path: &[P3], at: P3) -> Option<P3> {
    for i in 1..path.len() {
        let (a, b) = (path[i - 1], path[i]);
        let d = b.sub(a);
        if d.len() == 0 {
            continue;
        }
        let (lo, hi) = (a.min(b), a.max(b));
        let on = at.x >= lo.x - 1
            && at.x <= hi.x + 1
            && at.y >= lo.y - 1
            && at.y <= hi.y + 1
            && at.z >= lo.z - 1
            && at.z <= hi.z + 1;
        if on {
            return Some(d);
        }
    }
    None
}

/// Where two or more runs of one domain leave the same socket, one of them is
/// a branch -- so the split gets a tee, rather than two pipes emerging from
/// the same square inch of steel and hoping nobody looks.
///
/// This is the one piece of dressing that cannot be decided from inside a
/// single run, which is why it runs across the whole set once they are laid.
pub fn junctions(runs: &[Run], owners: &[Owner], grade: Grade, out: &mut Vec<Piece>) {
    if !grade.detailed() {
        return;
    }
    for r in runs.iter() {
        if !r.laid() {
            continue;
        }
        let at = r.path[0];
        // The first run out of a socket carries the tee and the rest are
        // branches off it. "First" is document order, which is fixed.
        let mates: Vec<&Run> =
            runs.iter().filter(|o| o.dom == r.dom && o.path.first() == Some(&at)).collect();
        if mates.len() < 2 || !std::ptr::eq(mates[0], r) {
            continue;
        }
        let Some(id) = owners.iter().position(|o| o.class == Owns::Run && o.name == r.name) else {
            continue;
        };
        let t = treat(r.dom);
        let od = outer(r.dom, r.bore);
        let d = r.path[1].sub(at);
        if d.len() < od * 3 {
            continue;
        }
        let branch = mates[1].path.get(1).map(|p| p.sub(at)).unwrap_or(d);
        out.push(
            Piece::new(Mesh::Tee, t.mat, at.add(unit_mm(d, od)), d, p3(od * 11 / 10, od * 3 / 2, od * 11 / 10))
                .spin(spin_for(d, branch))
                .lod(MEDIUM)
                .of(id as u16),
        );
    }
}

/// The straight bit of pipe a transport component *is*.
pub fn straight(a: P3, b: P3, bore: Mm, dom: Domain, out: &mut Vec<Piece>) {
    let t = treat(dom);
    let od = outer(dom, bore);
    out.push(Piece::span(t.mesh, t.mat, a, b, od).lod(FAR));
    let len = b.sub(a).len();
    if let Some((tm, tmat, gap)) = t.trim {
        let n = len / gap;
        for k in 1..=n {
            let at = a.add(unit_mm(b.sub(a), (k * len) / (n + 1)));
            out.push(Piece::new(tm, tmat, at, b.sub(a), p3(od * 13 / 10, od / 4, od * 13 / 10)).lod(CLOSE));
        }
    }
    for (p, q) in [(a, b), (b, a)] {
        out.push(
            Piece::new(Mesh::Flange, Mat::Steel, p, q.sub(p), p3(od * 15 / 10, od / 3, od * 15 / 10))
                .lod(MEDIUM),
        );
    }
}

/// `d`, rescaled to length `k`. Integer, and therefore off by up to a
/// millimetre, which nothing in a plant has ever minded.
fn unit_mm(d: P3, k: Mm) -> P3 {
    let l = d.len().max(1);
    p3(d.x * k / l, d.y * k / l, d.z * k / l)
}
