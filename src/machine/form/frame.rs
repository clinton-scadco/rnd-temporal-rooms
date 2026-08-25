//! Structural inference: what has to be there because of what is already
//! there.
//!
//! Section 5 of the note, and the pass that does the most work per line of
//! code, because none of it is a decision the player made:
//!
//! ```text
//!   heavy floor equipment          -> a concrete plinth
//!   equipment on legs or a frame   -> columns, head beams and bracing
//!   a long horizontal run          -> a pipe support every few metres
//!   anything to reach above 4.5 m  -> a platform, a handrail and a stair
//! ```
//!
//! None of it is structural engineering and none of it is trying to be. The
//! goal is *believable visual consequence*: add a turbine and steel appears
//! under it, move a separator and its legs go with it, put a heat main across
//! a plant and a row of supports marches after it. The player never places a
//! column, and the plant is full of them.
//!
//! Experiment 09 added the other half of the sentence -- how a thing is
//! *installed* rather than merely held up:
//!
//! ```text
//!   a plinth gets a pad and four holding-down bolts
//!   a column gets a base plate on top of its pad
//!   a pipe support gets a pad, and a tall one gets a brace
//!   two runs wanting a support in the same bay get one trestle between them
//!   a stair comes down whichever side of its platform has room for it
//! ```
//!
//! The last two are the interesting ones. A rack is a *system* rather than a
//! row of posts, and it falls out of clustering the props that already
//! existed; and the stair rule is a bug that four months of grey-boxing hid,
//! because a flight that lands in the yard is invisible until somebody paints
//! it like a stair.
//!
//! # Why it is a separate pass
//!
//! Because it can only run once the equipment and the pipework exist. That is
//! the entire justification for the pipeline being a pipeline: each pass reads
//! what the ones before it wrote, and nothing ever goes backwards. A plant is
//! not relaxed into shape, it is derived in five passes and then it stops.

use super::kit::{Mat, Mesh};
use super::layout::{Mount, Placed, Plan};
use super::route::Run;
use super::seed::Seed;
use super::{p3, Grade, Mm, Owner, Owns, Piece, Vol, CLOSE, FAR, MEDIUM, P3};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BTreeSet};

/// Above this, a service point needs a platform to stand on.
const REACH: Mm = 4500;
/// Walkways and platforms are this wide.
const WALK: Mm = 1200;
/// A step up, and a step along.
const RISE: Mm = 250;
const GOING: Mm = 300;
const RAIL_H: Mm = 1100;

pub fn infer(
    plan: &Plan,
    routes: &[Run],
    seed: &Seed,
    grade: Grade,
    owners: &mut Vec<Owner>,
    out: &mut Vec<Piece>,
) {
    for u in &plan.units {
        let id = own(owners, &u.name, u.mount.tag(), Owns::Frame);
        let n0 = out.len();
        match u.mount {
            Mount::Plinth => plinth(u, grade, out),
            Mount::Legs | Mount::Frame => frame(u, grade, out),
            Mount::Grade => {
                if u.arch.heavy() {
                    // Even a thing that stands on the floor stands on
                    // something: a slab, poured to its own outline.
                    out.push(Piece::slab(Mat::Concrete, Vol::new(
                        p3(u.vol.lo.x - 200, -150, u.vol.lo.z - 200),
                        p3(u.vol.hi.x + 200, 100, u.vol.hi.z + 200),
                    )).lod(FAR));
                    if grade.detailed() {
                        holding_down(u.vol, 100, out);
                    }
                }
            }
        }
        for p in out[n0..].iter_mut() {
            p.of = id;
        }

        // Anything a person has to get to, above head height, gets a way to
        // get to it. A distillation column is the extreme case and looks it.
        let high = u.sockets.iter().any(|s| s.at.y > REACH) || u.top() > REACH + 3000;
        if high && u.arch != super::layout::Arch::Run {
            let pid = own(owners, &u.name, "platform", Owns::Frame);
            let n1 = out.len();
            let mut r = seed.at(&u.name, "platform");
            let levels = ((u.top() - 2000) / 5000).clamp(1, 3);
            for k in 0..levels {
                let y = u.vol.lo.y + (u.vol.hi.y - u.vol.lo.y) * (k + 1) / (levels + 1);
                platform(u, y.max(3000), out);
            }
            let y = u.vol.lo.y + (u.vol.hi.y - u.vol.lo.y) / (levels + 1);
            stair(u, y.max(3000), plan, grade, &mut r, out);
            for p in out[n1..].iter_mut() {
                p.of = pid;
            }
        }
    }

    // Experiment 09, section 4: a rack is a *system*, not a row of isolated
    // posts. Wherever two different runs want holding up in the same couple of
    // metres, they get one trestle between them instead of two posts beside
    // each other -- which is both what a works looks like and, incidentally,
    // fewer pieces.
    let racked = if grade.articulated() { rack(plan, routes, owners, out) } else { BTreeSet::new() };

    for (ri, r) in routes.iter().enumerate() {
        if r.props.is_empty() {
            continue;
        }
        let id = own(owners, &r.name, "support", Owns::Frame);
        let n0 = out.len();
        // A support is a post and a cradle, sized to the pipe rather than to
        // the plant: forty of these at half a metre across is a fence.
        let od = ((r.bore * 9) / 10).max(320);
        // What the pipe actually measures across, which is not its bore -- a
        // lagged heat main is two thirds as wide again. The cradle stops
        // underneath this rather than at the centreline, which is the
        // difference between holding a pipe up and being driven through it.
        let pipe = super::route::outer(r.dom, r.bore);
        for (pi, &at) in r.props.iter().enumerate() {
            if racked.contains(&(ri, pi)) {
                continue;
            }
            // Do not stand a post inside a machine.
            if plan.units.iter().any(|u| u.vol.grow_flat(200).has(p3(at.x, u.vol.lo.y + 1, at.z))) {
                continue;
            }
            // The post stands from the ground to the underside of the pipe.
            let h = at.y - pipe / 2;
            if h < 900 {
                continue;
            }
            out.push(
                Piece::up(Mesh::Support, Mat::Dark, p3(at.x, 0, at.z), p3(od, h, od))
                    .lod(MEDIUM),
            );
            if grade.detailed() {
                // A post stands on a pad, and a tall one is braced. Both are
                // the difference between a pipe on a stick and a pipe on a
                // support.
                out.push(
                    Piece::up(Mesh::Box, Mat::Concrete, p3(at.x, -150, at.z), p3(od + 400, 300, od + 400))
                        .lod(CLOSE),
                );
                if h > 3500 {
                    let side = if (at.x / 1000) % 2 == 0 { 1 } else { -1 };
                    out.push(
                        Piece::span(
                            Mesh::Beam,
                            Mat::Dark,
                            p3(at.x + side * (od + 500), 100, at.z),
                            p3(at.x, h - 600, at.z),
                            160,
                        )
                        .lod(MEDIUM),
                    );
                }
            }
        }
        for p in out[n0..].iter_mut() {
            p.of = id;
        }
    }
}

/// Where several runs want holding up in the same place, one trestle holds
/// them all up.
///
/// Two metres of grid, a bounding box, a beam across it and a column at each
/// end. The props it took over are returned so the per-run pass knows not to
/// put a post under them as well -- which is the only coordination between the
/// two, and it goes one way, like everything else here.
fn rack(
    plan: &Plan,
    routes: &[Run],
    owners: &mut Vec<Owner>,
    out: &mut Vec<Piece>,
) -> BTreeSet<(usize, usize)> {
    const BAY: Mm = 2000;
    let mut cells: BTreeMap<(Mm, Mm), Vec<(usize, usize)>> = BTreeMap::new();
    for (ri, r) in routes.iter().enumerate() {
        for (pi, at) in r.props.iter().enumerate() {
            if at.y < 2000 {
                continue;
            }
            if plan.units.iter().any(|u| u.vol.grow_flat(200).has(p3(at.x, u.vol.lo.y + 1, at.z))) {
                continue;
            }
            cells.entry((at.x.div_euclid(BAY), at.z.div_euclid(BAY))).or_default().push((ri, pi));
        }
    }

    let mut taken = BTreeSet::new();
    for (_, mine) in cells {
        let runs: BTreeSet<usize> = mine.iter().map(|(r, _)| *r).collect();
        if runs.len() < 2 {
            continue;
        }
        let pts: Vec<P3> = mine.iter().map(|(r, p)| routes[*r].props[*p]).collect();
        let (lo, hi) = pts.iter().fold((pts[0], pts[0]), |(a, b), p| (a.min(*p), b.max(*p)));
        let top = hi.y + 200;
        // The bar spans the way the props are spread, which for a set of
        // parallel runs is across them: exactly where a bar belongs.
        let along_x = (hi.x - lo.x) >= (hi.z - lo.z);
        let (a, b) = if along_x {
            (p3(lo.x - 600, top, (lo.z + hi.z) / 2), p3(hi.x + 600, top, (lo.z + hi.z) / 2))
        } else {
            (p3((lo.x + hi.x) / 2, top, lo.z - 600), p3((lo.x + hi.x) / 2, top, hi.z + 600))
        };
        let name = routes[*runs.iter().next().unwrap()].name.clone();
        let id = own(owners, &name, "rack", Owns::Frame);
        let n0 = out.len();
        for end in [a, b] {
            out.push(Piece::up(Mesh::Beam, Mat::Dark, p3(end.x, 0, end.z), p3(280, top, 280)).lod(MEDIUM));
            out.push(
                Piece::up(Mesh::Box, Mat::Concrete, p3(end.x, -150, end.z), p3(700, 300, 700)).lod(CLOSE),
            );
        }
        out.push(Piece::span(Mesh::Beam, Mat::Dark, a, b, 260).lod(MEDIUM));
        // A second tier, if anything is running well above the bar.
        if hi.y - lo.y > 1200 {
            let low = lo.y + 200;
            let (c, d) = (p3(a.x, low, a.z), p3(b.x, low, b.z));
            out.push(Piece::span(Mesh::Beam, Mat::Dark, c, d, 220).lod(MEDIUM));
        }
        for p in out[n0..].iter_mut() {
            p.of = id;
        }
        taken.extend(mine);
    }
    taken
}

/// Four bolts through the slab, at the corners of whatever is standing on it.
fn holding_down(v: Vol, y: Mm, out: &mut Vec<Piece>) {
    let s = v.size();
    for (dx, dz) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
        out.push(
            Piece::up(
                Mesh::Anchor,
                Mat::Dark,
                p3(v.lo.x + s.x / 2 + dx * (s.x / 2 + 120), y - 100, v.lo.z + s.z / 2 + dz * (s.z / 2 + 120)),
                p3(360, 360, 360),
            )
            .lod(CLOSE),
        );
    }
}

fn own(owners: &mut Vec<Owner>, name: &str, what: &str, class: Owns) -> u16 {
    owners.push(Owner { name: name.to_string(), what: what.to_string(), class });
    (owners.len() - 1) as u16
}

/// Concrete, to the outline of the machine and a bit over.
fn plinth(u: &Placed, grade: Grade, out: &mut Vec<Piece>) {
    let v = Vol::new(
        p3(u.vol.lo.x - 250, -200, u.vol.lo.z - 250),
        p3(u.vol.hi.x + 250, u.lift, u.vol.hi.z + 250),
    );
    out.push(Piece::slab(Mat::Concrete, v).lod(FAR));
    if grade.detailed() {
        // A pad under the plinth, and bolts through the top of it. A block of
        // concrete with a machine balanced on it is not a foundation; this is
        // three more pieces and it stops looking like one.
        out.push(
            Piece::slab(
                Mat::Concrete,
                Vol::new(
                    p3(v.lo.x - 350, -250, v.lo.z - 350),
                    p3(v.hi.x + 350, (u.lift - 150).max(-100), v.hi.z + 350),
                ),
            )
            .lod(MEDIUM),
        );
        holding_down(u.vol, u.lift, out);
    }
}

/// Four columns, a head frame and a pair of braces. This is what holds up
/// everything the plan put in the air.
fn frame(u: &Placed, grade: Grade, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    let (hx, hz) = (s.x / 2 - 200, s.z / 2 - 200);
    let h = u.lift;
    let corners = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
    for (dx, dz) in corners {
        let at = p3(f.x + dx * hx, 0, f.z + dz * hz);
        out.push(Piece::up(Mesh::Beam, Mat::Dark, at, p3(300, h, 300)).lod(FAR));
        out.push(Piece::up(Mesh::Box, Mat::Concrete, p3(at.x, -150, at.z), p3(700, 300, 700)).lod(CLOSE));
        // A column meets its pad through a base plate, and the plate is what
        // makes the joint look like a joint.
        if grade.detailed() {
            out.push(Piece::up(Mesh::Anchor, Mat::Dark, p3(at.x, 100, at.z), p3(420, 420, 420)).lod(CLOSE));
        }
    }
    // Head beams, both ways.
    for dz in [-1, 1] {
        let at = p3(f.x - s.x / 2, h - 150, f.z + dz * hz);
        out.push(Piece::span(Mesh::Beam, Mat::Dark, at, p3(at.x + s.x, at.y, at.z), 300).lod(MEDIUM));
    }
    for dx in [-1, 1] {
        let at = p3(f.x + dx * hx, h - 150, f.z - s.z / 2);
        out.push(Piece::span(Mesh::Beam, Mat::Dark, at, p3(at.x, at.y, at.z + s.z), 300).lod(MEDIUM));
    }
    // Bracing, on the two faces where it will be seen.
    if h > 1500 {
        for dz in [-1, 1] {
            let a = p3(f.x - hx, 100, f.z + dz * hz);
            let b = p3(f.x + hx, h - 300, f.z + dz * hz);
            out.push(Piece::span(Mesh::Beam, Mat::Dark, a, b, 180).lod(MEDIUM));
        }
    }
}

/// A walkway round a machine, at a height, with a handrail on the outside.
///
/// It is built out of one-metre-odd panels rather than one big slab, which
/// costs nothing -- they are all the same instance -- and means a platform
/// round a two-metre vessel and one round a twelve-metre column are the same
/// four lines of code.
fn platform(u: &Placed, y: Mm, out: &mut Vec<Piece>) {
    let v = u.vol.grow_flat(WALK);
    let (x0, z0) = (v.lo.x, v.lo.z);
    let s = v.size();
    let nx = (s.x / WALK).max(3);
    let nz = (s.z / WALK).max(3);
    let (px, pz) = (s.x / nx, s.z / nz);

    // The border of the grid and not its middle: the middle is where the
    // machine is.
    for i in 0..nx {
        for j in 0..nz {
            if i > 0 && i + 1 < nx && j > 0 && j + 1 < nz {
                continue;
            }
            let at = p3(x0 + px * i + px / 2, y - 60, z0 + pz * j + pz / 2);
            out.push(Piece::up(Mesh::Grate, Mat::Galv, at, p3(px, 60, pz)).lod(MEDIUM));
        }
    }
    // Handrail, right round the outside.
    let corners = [
        p3(x0, y, z0),
        p3(x0 + s.x, y, z0),
        p3(x0 + s.x, y, z0 + s.z),
        p3(x0, y, z0 + s.z),
    ];
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let d = b.sub(a);
        let n = (d.len() / WALK).max(1);
        for i in 0..n {
            let at = a.add(p3(d.x * i / n, 0, d.z * i / n));
            out.push(
                Piece::new(Mesh::Rail, Mat::Galv, at, d, p3(80, d.len() / n, RAIL_H))
                    .spin(upright(d))
                    .lod(CLOSE),
            );
        }
    }
}

/// The quarter turn that stands a handrail up.
///
/// A `Rail` is drawn with its run along canonical `+Y` and its height along
/// canonical `+Z`, so the only spin that is not simply wrong is the one that
/// lands `+Z` on the world's up. This used to be chosen by pointing the
/// uprights away from the machine, which sounds like a reasonable thing to
/// want and is not a choice that exists: the rail's `+X` is eighty
/// millimetres wide and symmetrical, so there is no near side or far side to
/// pick. What the old rule actually picked, whenever the platform stood above
/// the middle of the machine it wrapped -- which is most platforms -- was one
/// of the two spins that lay the whole handrail down flat on the decking.
fn upright(along: P3) -> u8 {
    // `right_of(along, s + 1)` is where the rail's `+Z` ends up at spin `s`.
    (super::spin_for(along, super::UP) + 3) & 3
}

/// A flight of stairs from a platform down to the ground.
///
/// Which side it comes down was the one cosmetic choice in this whole file,
/// and experiment 09 took half of it away. A flight from a twelve-metre
/// platform is six metres long, the apron is two and a bit, and a stair picked
/// by a coin lands in the yard about half the time -- which nobody noticed for
/// as long as it was drawn in the same grey as everything else, and which is
/// impossible to miss the moment it is painted like a stair.
///
/// So from grade C the seed proposes and the plot disposes: a flight has to
/// land on the concrete and it has to miss the plant, in that order, and the
/// seed only breaks the tie.
///
/// Ranking those two the other way round is what the rule did first, and it
/// made things worse rather than better. The roomiest side of a platform is
/// the one facing the middle of the works, so a rule that only counted room
/// walked more flights through machinery than the coin toss it replaced --
/// 258 treads inside equipment across `designs/`, against 163 for the coin.
///
/// The fix is that a face has ends as well as a middle. Four faces gave a
/// crowded plant four chances to put a flight somewhere, and on a plant where
/// every face that stays on the concrete also has a machine under it, four is
/// not enough: the stair takes the least bad and walks through a separator.
/// Three landings per face give it twelve, and a plant that tight nearly
/// always has exactly one gap. It finds it 26 times out of 822.
fn stair(u: &Placed, y: Mm, plan: &Plan, grade: Grade, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let plot = plan.plot;
    let v = u.vol.grow_flat(WALK);
    let want = r.pick(4);
    let faces = [
        (p3(v.hi.x, y, v.centre().z), super::EAST),
        (p3(v.centre().x, y, v.hi.z), super::SOUTH),
        (p3(v.lo.x, y, v.centre().z), super::WEST),
        (p3(v.centre().x, y, v.lo.z), super::NORTH),
    ];
    let s = v.size();
    // Twelve landings: face `k % 4`, slid a third of the way towards one
    // corner or the other. The middles are 4..8, so `want` names one of them
    // and grade A and B keep exactly the flight they had.
    let spot = |k: usize| -> (P3, P3) {
        let (at, dir) = faces[k % 4];
        let t = k as Mm / 4 - 1;
        let slide = if dir.x != 0 { p3(0, 0, t * s.z / 3) } else { p3(t * s.x / 3, 0, 0) };
        (at.add(slide), dir)
    };
    let want = want + 4;
    let side = if grade.detailed() {
        let reach = (y / RISE).max(2) * GOING;
        let room = |k: usize| -> Mm {
            let (start, dir) = spot(k);
            let end = start.add(dir.mul(reach));
            // How far the bottom of the flight is inside the plot, which may
            // be negative, and the further inside the better.
            (end.x - plot.lo.x).min(plot.hi.x - end.x).min(end.z - plot.lo.z).min(plot.hi.z - end.z)
        };
        // And what the flight would have to go *through* to get there. The
        // plot test only ever asked whether the stair landed on the concrete;
        // a flight that lands beautifully on the concrete having passed
        // through a separator on the way down is still a flight through a
        // separator. Sampled by tread, because a flight descends, and the
        // thing it misses at the top is not the thing it hits at the bottom.
        let clash = |k: usize| -> Mm {
            let (start, dir) = spot(k);
            let n = (y / RISE).max(2);
            let mut hits = 0;
            for step in 0..=n {
                let at = start.add(dir.mul(GOING * step)).add(p3(0, -RISE * step, 0));
                let tread = Vol::new(
                    p3(at.x - 600, at.y - RISE, at.z - 600),
                    p3(at.x + 600, at.y + RAIL_H, at.z + 600),
                );
                if plan.units.iter().any(|o| o.name != u.name && o.vol.hits(tread)) {
                    hits += 1;
                }
            }
            hits
        };
        // Landing on the plot comes first, because a flight that walks off
        // into the yard to dodge a machine has not solved anything -- it has
        // swapped experiment 09's bug back in to fix this one. Among the sides
        // that do land on the plot, the one that goes through the least plant
        // wins; then the roomiest; then the seed.
        (0..12)
            .max_by_key(|&k| (room(k) >= 0, Reverse(clash(k)), room(k), k == want, Reverse(k)))
            .unwrap_or(want)
    } else {
        want
    };
    let (start, dir) = spot(side);
    let n = (y / RISE).max(2);
    for k in 0..n {
        let at = start.add(dir.mul(GOING * k)).add(p3(0, -RISE * k, 0));
        out.push(
            Piece::new(Mesh::Step, Mat::Galv, p3(at.x, at.y - RISE, at.z), super::UP, p3(1000, RISE, GOING))
                .spin(if dir.x != 0 { 1 } else { 0 })
                .lod(CLOSE),
        );
    }
    if grade.articulated() {
        // A landing, so the flight arrives somewhere rather than at a handrail.
        out.push(
            Piece::up(Mesh::Grate, Mat::Galv, start.add(dir.mul(GOING / 2)), p3(1200, 60, 1200))
                .lod(MEDIUM),
        );
    }

    // Two stringers and a rail, so it reads as a stair from across the plot.
    //
    // `right_of` hands back a direction scaled by a thousand, so the offset
    // has to be divided by a thousand *after* the multiply. Dividing first is
    // integer division of five hundred by a thousand, which is nought -- and
    // both stringers, and both handrails, were being drawn down the centreline
    // of the flight on top of each other. What that looks like from any
    // distance is a bare diagonal wire hanging over the plant.
    let end = start.add(dir.mul(GOING * n)).add(p3(0, -y, 0));
    for off in [-500, 500] {
        let side = super::right_of(dir, 0).mul(off).div(1000);
        let a = start.add(p3(side.x, 0, side.z));
        let b = end.add(p3(side.x, 0, side.z));
        out.push(Piece::span(Mesh::Beam, Mat::Dark, a, b, 160).lod(MEDIUM));
        out.push(
            Piece::span(Mesh::Cyl, Mat::Galv, p3(a.x, a.y + RAIL_H, a.z), p3(b.x, b.y + RAIL_H, b.z), 60)
                .lod(CLOSE),
        );
    }
}
