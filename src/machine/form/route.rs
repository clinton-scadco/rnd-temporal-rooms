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

use super::kit::{Mat, Mesh};
use super::layout::{Placed, Plan, RACK_Y, SHAFT_Y};
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
    /// True if the router could not find a way through and the run is a
    /// straight line drawn in defiance of the plant. Rare, and worth knowing.
    pub direct: bool,
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
    /// The height this domain would rather live at.
    home: Mm,
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
            home: 900,
        },
        Domain::Gas => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Steel,
            wide: 115,
            trim: Some((Mesh::Band, Mat::Lag, 1600)),
            elbow: true,
            bend_cost: 30,
            home: RACK_Y,
        },
        Domain::Heat => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Lag,
            wide: 165,
            trim: Some((Mesh::Band, Mat::Steel, 800)),
            elbow: true,
            bend_cost: 34,
            home: RACK_Y,
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
            home: SHAFT_Y,
        },
        Domain::Mech => Treat {
            mesh: Mesh::Cyl,
            mat: Mat::Steel,
            wide: 45,
            trim: None,
            elbow: false,
            bend_cost: 320,
            home: SHAFT_Y,
        },
        Domain::Electrical => Treat {
            mesh: Mesh::Box,
            mat: Mat::Galv,
            wide: 42,
            trim: Some((Mesh::Box, Mat::Dark, 2200)),
            elbow: false,
            bend_cost: 18,
            home: RACK_Y - 800,
        },
        Domain::Material => Treat {
            mesh: Mesh::Box,
            mat: Mat::Galv,
            wide: 230,
            trim: Some((Mesh::Band, Mat::Dark, 2600)),
            elbow: false,
            bend_cost: 48,
            home: super::layout::FEED_Y,
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

struct Grid {
    o: P3,
    n: (i32, i32, i32),
    cell: Mm,
    /// 1 solid, 2 in somebody's clearance, 4 already taken by a route.
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
                    g.fill(u.vol, 1);
                    g.fill(u.clear, 2);
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

    let mut out: Vec<Run> = Vec::with_capacity(links.len());
    let mut done: Vec<(usize, Run)> = Vec::with_capacity(links.len());
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
        let r = one(&mut g, a, sa, b, sb, dom, serve, name);
        done.push((i, r));
    }
    // Back into document order, so that two designs that differ only in the
    // order two wires were drawn still hash the same.
    done.sort_by_key(|(i, _)| *i);
    for (_, r) in done {
        out.push(r);
    }
    out
}

#[allow(clippy::too_many_arguments)]
fn one(
    g: &mut Grid,
    a: &Placed,
    sa: &super::layout::Socket,
    b: &Placed,
    sb: &super::layout::Socket,
    dom: Domain,
    serve: Subst,
    name: String,
) -> Run {
    let t = treat(dom);
    let start = g.cell_of(sa.at.add(sa.out.mul(g.cell)));
    let goal = g.cell_of(sb.at.add(sb.out.mul(g.cell)));

    // A socket is on the surface of the thing it belongs to, so the cell it
    // sits in is inside somebody's solid. Open a pocket one cell across around
    // each end -- and *only* around each end, because opening the whole
    // component lets the pipe leave through the far wall, which looks exactly
    // as wrong as it sounds.
    let held = [pocket(g, sa.at), pocket(g, sb.at)];

    let cells = search(g, start, goal, &t);

    for saved in held {
        for (i, m) in saved {
            g.mark[i] = m;
        }
    }
    let _ = (a, b);

    let mut path: Vec<P3> = Vec::new();
    let direct = cells.is_empty();
    path.push(sa.at);
    for c in &cells {
        path.push(g.world(*c));
    }
    path.push(sb.at);
    let path = simplify(path);

    // Claim it, so the next route has to go round.
    for c in &cells {
        let i = g.idx(*c);
        g.mark[i] |= 4;
    }

    let mut length = 0;
    let mut bends = 0;
    for i in 1..path.len() {
        length += path[i].sub(path[i - 1]).len();
        if i + 1 < path.len() && turns(path[i - 1], path[i], path[i + 1]) {
            bends += 1;
        }
    }
    let props = props_along(&path, dom);
    Run {
        name,
        dom,
        serve,
        bore: sa.bore.max(sb.bore),
        ends: (sa.bore, sb.bore),
        path,
        length,
        bends,
        props,
        direct,
    }
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
                g.mark[i] &= !(1 | 4);
            }
        }
    }
    saved
}

/// A* over `(cell, heading)`, because a bend penalty is a property of an edge
/// rather than of a cell and pretending otherwise produces staircases.
fn search(g: &Grid, start: (i32, i32, i32), goal: (i32, i32, i32), t: &Treat) -> Vec<(i32, i32, i32)> {
    let cells = (g.n.0 * g.n.1 * g.n.2) as usize;
    let n = cells * 7;
    let mut dist: Vec<u32> = vec![u32::MAX; n];
    let mut prev: Vec<u32> = vec![u32::MAX; n];
    let mut heap: BinaryHeap<Reverse<(u32, u32)>> = BinaryHeap::new();

    let key = |c: (i32, i32, i32), d: usize| g.idx(c) * 7 + d;
    let hcost = |c: (i32, i32, i32)| {
        (((c.0 - goal.0).abs() + (c.1 - goal.1).abs() + (c.2 - goal.2).abs()) * 10) as u32
    };

    // Heading 6 means "has not started yet", so the first step is free of a
    // bend penalty whichever way it goes.
    let s = key(start, 6);
    dist[s] = 0;
    heap.push(Reverse((hcost(start), s as u32)));

    let mut seen = 0usize;
    let mut best: Option<u32> = None;
    while let Some(Reverse((_, k))) = heap.pop() {
        let k = k as usize;
        let cell = k / 7;
        let head = k % 7;
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
        if seen > 600_000 {
            break;
        }
        let d0 = dist[k];
        for (di, step) in SIX.iter().enumerate() {
            let nc = (c.0 + step.x, c.1 + step.y, c.2 + step.z);
            if !g.inside(nc) {
                continue;
            }
            let m = g.mark[g.idx(nc)];
            if m & 1 != 0 || m & 4 != 0 {
                continue;
            }
            let mut cost = 10u32;
            if m & 2 != 0 {
                cost += 26;
            }
            if head != 6 && head != di {
                cost += t.bend_cost;
            }
            // Prefer the domain's own height for the long horizontal middle of
            // a run; the ends can be wherever the sockets are.
            let y = g.o.y + nc.1 * g.cell;
            let off = ((y - t.home).abs() / g.cell).min(8) as u32;
            cost += off * 3;
            // Nothing wants to lie on the walkway.
            if nc.1 <= 1 && step.y == 0 {
                cost += 14;
            }
            let nk = key(nc, di);
            let nd = d0 + cost;
            if nd < dist[nk] {
                dist[nk] = nd;
                prev[nk] = k as u32;
                heap.push(Reverse((nd + hcost(nc), nk as u32)));
            }
        }
    }

    let Some(mut k) = best else {
        return Vec::new();
    };
    let mut out = Vec::new();
    loop {
        let cell = (k as usize) / 7;
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
    out
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
pub fn bend_of(r: &Run) -> Mm {
    (outer(r.dom, r.bore) * 3) / 2
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
fn props_along(path: &[P3], dom: Domain) -> Vec<P3> {
    if matches!(dom, Domain::Mech) {
        return Vec::new();
    }
    let gap = match dom {
        Domain::Heat | Domain::Gas => 4000,
        Domain::Rotary => 4000,
        _ => 4500,
    };
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
        if r.path.len() < 2 {
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
