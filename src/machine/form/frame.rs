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
use super::{p3, Mm, Owner, Owns, Piece, Vol, CLOSE, FAR, MEDIUM, P3};

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
    owners: &mut Vec<Owner>,
    out: &mut Vec<Piece>,
) {
    for u in &plan.units {
        let id = own(owners, &u.name, u.mount.tag(), Owns::Frame);
        let n0 = out.len();
        match u.mount {
            Mount::Plinth => plinth(u, out),
            Mount::Legs | Mount::Frame => frame(u, out),
            Mount::Grade => {
                if u.arch.heavy() {
                    // Even a thing that stands on the floor stands on
                    // something: a slab, poured to its own outline.
                    out.push(Piece::slab(Mat::Concrete, Vol::new(
                        p3(u.vol.lo.x - 200, -150, u.vol.lo.z - 200),
                        p3(u.vol.hi.x + 200, 100, u.vol.hi.z + 200),
                    )).lod(FAR));
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
            stair(u, y.max(3000), &mut r, out);
            for p in out[n1..].iter_mut() {
                p.of = pid;
            }
        }
    }

    for r in routes {
        if r.props.is_empty() {
            continue;
        }
        let id = own(owners, &r.name, "support", Owns::Frame);
        let n0 = out.len();
        // A support is a post and a cradle, sized to the pipe rather than to
        // the plant: forty of these at half a metre across is a fence.
        let od = ((r.bore * 9) / 10).max(320);
        for &at in &r.props {
            // Do not stand a post inside a machine.
            if plan.units.iter().any(|u| u.vol.grow_flat(200).has(p3(at.x, u.vol.lo.y + 1, at.z))) {
                continue;
            }
            let h = at.y;
            if h < 900 {
                continue;
            }
            out.push(
                Piece::up(Mesh::Support, Mat::Dark, p3(at.x, 0, at.z), p3(od, h, od))
                    .lod(MEDIUM),
            );
        }
        for p in out[n0..].iter_mut() {
            p.of = id;
        }
    }
}

fn own(owners: &mut Vec<Owner>, name: &str, what: &str, class: Owns) -> u16 {
    owners.push(Owner { name: name.to_string(), what: what.to_string(), class });
    (owners.len() - 1) as u16
}

/// Concrete, to the outline of the machine and a bit over.
fn plinth(u: &Placed, out: &mut Vec<Piece>) {
    let v = Vol::new(
        p3(u.vol.lo.x - 250, -200, u.vol.lo.z - 250),
        p3(u.vol.hi.x + 250, u.lift, u.vol.hi.z + 250),
    );
    out.push(Piece::slab(Mat::Concrete, v).lod(FAR));
}

/// Four columns, a head frame and a pair of braces. This is what holds up
/// everything the plan put in the air.
fn frame(u: &Placed, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    let (hx, hz) = (s.x / 2 - 200, s.z / 2 - 200);
    let h = u.lift;
    let corners = [(-1, -1), (1, -1), (1, 1), (-1, 1)];
    for (dx, dz) in corners {
        let at = p3(f.x + dx * hx, 0, f.z + dz * hz);
        out.push(Piece::up(Mesh::Beam, Mat::Dark, at, p3(300, h, 300)).lod(FAR));
        out.push(Piece::up(Mesh::Box, Mat::Concrete, p3(at.x, -150, at.z), p3(700, 300, 700)).lod(CLOSE));
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
    let mid = u.vol.centre();
    for k in 0..4 {
        let (a, b) = (corners[k], corners[(k + 1) % 4]);
        let d = b.sub(a);
        let n = (d.len() / WALK).max(1);
        for i in 0..n {
            let at = a.add(p3(d.x * i / n, 0, d.z * i / n));
            out.push(
                Piece::new(Mesh::Rail, Mat::Galv, at, d, p3(80, d.len() / n, RAIL_H))
                    .spin(spin_out(d, mid.sub(at)))
                    .lod(CLOSE),
            );
        }
    }
}

/// Which quarter turn puts the rail's uprights on the far side from `inward`.
fn spin_out(along: P3, inward: P3) -> u8 {
    let mut best = 0u8;
    let mut score = i64::MIN;
    for s in 0..4u8 {
        let f = super::right_of(along, (s + 1) & 3);
        let dot = -(f.x as i64 * inward.x as i64 + f.y as i64 * inward.y as i64 + f.z as i64 * inward.z as i64);
        if dot > score {
            score = dot;
            best = s;
        }
    }
    best
}

/// A flight of stairs from a platform down to the ground, on whichever side
/// the seed picks -- which is the one cosmetic choice in this whole file.
fn stair(u: &Placed, y: Mm, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let v = u.vol.grow_flat(WALK);
    let side = r.pick(4);
    let (start, dir) = match side {
        0 => (p3(v.hi.x, y, v.centre().z), super::EAST),
        1 => (p3(v.centre().x, y, v.hi.z), super::SOUTH),
        2 => (p3(v.lo.x, y, v.centre().z), super::WEST),
        _ => (p3(v.centre().x, y, v.lo.z), super::NORTH),
    };
    let n = (y / RISE).max(2);
    for k in 0..n {
        let at = start.add(dir.mul(GOING * k)).add(p3(0, -RISE * k, 0));
        out.push(
            Piece::new(Mesh::Step, Mat::Galv, p3(at.x, at.y - RISE, at.z), super::UP, p3(1000, RISE, GOING))
                .spin(if dir.x != 0 { 1 } else { 0 })
                .lod(CLOSE),
        );
    }
    // Two stringers and a rail, so it reads as a stair from across the plot.
    let end = start.add(dir.mul(GOING * n)).add(p3(0, -y, 0));
    for off in [-500, 500] {
        let side = super::right_of(dir, 0).mul(off / 1000);
        let a = start.add(p3(side.x, 0, side.z));
        let b = end.add(p3(side.x, 0, side.z));
        out.push(Piece::span(Mesh::Beam, Mat::Dark, a, b, 160).lod(MEDIUM));
        out.push(
            Piece::span(Mesh::Cyl, Mat::Galv, p3(a.x, a.y + RAIL_H, a.z), p3(b.x, b.y + RAIL_H, b.z), 60)
                .lod(CLOSE),
        );
    }
}
