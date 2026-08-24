//! Basic enclosure generation: does this collection of machinery live on a
//! skid, in a shed, or in a building?
//!
//! Section 6, and deliberately the least clever pass in the tree:
//!
//! ```text
//!   1. take the bounds of the equipment
//!   2. expand by clearance
//!   3. generate a floor
//!   4. optionally generate walls and a roof
//!   5. cut openings where the big lines pass through
//! ```
//!
//! Step 5 is the only one with any teeth, and it is the only one that matters,
//! because it is the step where the *machine* decides something about the
//! *building*. A panel is left out wherever a run crosses the wall plane, and a
//! roof panel is left out wherever something is too tall to fit under it. So a
//! distillation column stands through its own roof and a heat main leaves
//! through its own hole, and neither was placed by anybody.
//!
//! The test is not whether this is good architecture. It is whether the
//! machinery can plausibly produce its own surrounding structure, which is a
//! much lower bar and a much more interesting one.

use super::kit::{Mat, Mesh};
use super::layout::{Arch, Plan};
use super::route::Run;
use super::seed::Seed;
use super::{p3, Mm, Owner, Owns, Piece, Style, Vol, CLOSE, FAR, MEDIUM};

/// What the installation turned out to be.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Kind {
    /// A slab and nothing else: an outdoor plant.
    Yard,
    /// A slab with a kerb and a frame: a packaged unit you could crane in.
    Skid,
    /// Walls on the weather sides, and open to the sky.
    Housed,
    /// Walls and a roof, with holes in both where the plant needs them.
    Building,
}

impl Kind {
    pub fn tag(self) -> &'static str {
        match self {
            Kind::Yard => "yard",
            Kind::Skid => "skid",
            Kind::Housed => "housed",
            Kind::Building => "building",
        }
    }
}

/// Room to walk round the outside.
const APRON: Mm = 2400;
/// A wall panel.
const PANEL: Mm = 2000;
const COURSE: Mm = 1500;
/// The tallest a wall gets. Anything taller than this goes through the roof,
/// which is what actually happens and looks far better than a shed the height
/// of a distillation column.
const WALL_MAX: Mm = 9000;

pub fn enclose(
    plan: &Plan,
    routes: &[Run],
    seed: &Seed,
    style: Style,
    owners: &mut Vec<Owner>,
    out: &mut Vec<Piece>,
) -> (Kind, Vol) {
    let mut v = plan.plot;
    for u in &plan.units {
        v = v.join(u.vol);
    }
    let floor = Vol::new(
        p3(v.lo.x - APRON, -300, v.lo.z - APRON),
        p3(v.hi.x + APRON, 0, v.hi.z + APRON),
    );
    let tall = plan.units.iter().map(|u| u.vol.hi.y).max().unwrap_or(3000);
    let area = (floor.size().x as i64 / 1000) * (floor.size().z as i64 / 1000);

    let kind = match style {
        Style::Yard => Kind::Yard,
        Style::Hall => Kind::Building,
        Style::Works => {
            // Small enough and plain enough to have arrived on a lorry; too
            // big to roof; or a building.
            //
            // Height is deliberately *not* what pushes a plant outdoors -- the
            // roof pass already leaves a hole wherever something does not fit
            // under it, which is what real works look like and is the one
            // place in this pass where the machine decides something about the
            // building. What disqualifies a skid is a pressure vessel, a tower
            // or a press: nobody delivers those on the back of anything.
            let vessels = plan
                .units
                .iter()
                .any(|u| matches!(u.arch, Arch::Vessel | Arch::Tower | Arch::Portal));
            if plan.units.len() <= 10 && area < 700 && !vessels {
                Kind::Skid
            } else if area > 800 {
                Kind::Housed
            } else {
                Kind::Building
            }
        }
    };

    let id = owners.len() as u16;
    owners.push(Owner { name: "enclosure".into(), what: kind.tag().to_string(), class: Owns::Shell });
    let n0 = out.len();

    out.push(Piece::slab(Mat::Concrete, floor).lod(FAR));

    let mut r = seed.all("shell");
    let wall = (tall + 1200).min(WALL_MAX).max(4000);
    match kind {
        Kind::Yard => {}
        Kind::Skid => {
            // A kerb and four corner posts: this is a thing that was delivered
            // rather than built.
            let k = 260;
            for (dx, dz) in [(0, -1), (0, 1), (-1, 0), (1, 0)] {
                let s = floor.size();
                let at = p3(
                    floor.centre().x + dx * (s.x / 2 - k / 2),
                    0,
                    floor.centre().z + dz * (s.z / 2 - k / 2),
                );
                let size = if dx != 0 { p3(k, 300, s.z) } else { p3(s.x, 300, k) };
                out.push(Piece::up(Mesh::Box, Mat::Dark, at, size).lod(MEDIUM));
            }
            for (dx, dz) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
                let s = floor.size();
                let at = p3(floor.centre().x + dx * (s.x / 2 - 300), 0, floor.centre().z + dz * (s.z / 2 - 300));
                out.push(Piece::up(Mesh::Beam, Mat::Dark, at, p3(280, tall.min(4000), 280)).lod(MEDIUM));
            }
        }
        Kind::Housed | Kind::Building => {
            let sides: &[usize] = if kind == Kind::Building { &[0, 1, 2, 3] } else { &[0, 3] };
            for &s in sides {
                wall_side(floor, s, wall, routes, plan, &mut r, out);
            }
            if kind == Kind::Building {
                roof(floor, wall, plan, out);
            }
        }
    }

    for p in out[n0..].iter_mut() {
        p.of = id;
    }
    (kind, floor)
}

/// One wall, panel by panel, leaving out whatever the plant needs to get
/// through it.
fn wall_side(
    floor: Vol,
    side: usize,
    height: Mm,
    routes: &[Run],
    plan: &Plan,
    r: &mut super::seed::Rng,
    out: &mut Vec<Piece>,
) {
    let s = floor.size();
    // Along the wall, and the outward normal of it.
    let (along, base, len) = match side {
        0 => (super::EAST, p3(floor.lo.x, 0, floor.lo.z), s.x),
        1 => (super::SOUTH, p3(floor.hi.x, 0, floor.lo.z), s.z),
        2 => (super::WEST, p3(floor.hi.x, 0, floor.hi.z), s.x),
        _ => (super::NORTH, p3(floor.lo.x, 0, floor.hi.z), s.z),
    };
    let n = (len / PANEL).max(1);
    let step = len / n;
    let rows = (height / COURSE).max(2);
    let door = r.pick(n.max(1) as usize) as i32;

    for i in 0..n {
        let mid = base.add(along.mul(step * i + step / 2));
        for j in 0..rows {
            let y = height * j / rows;
            let h = height / rows;
            // A door, and the two courses above the ground it takes up.
            if i == door && j < 2 {
                continue;
            }
            let cell = Vol::new(
                p3(mid.x - step / 2 - 200, y, mid.z - step / 2 - 200),
                p3(mid.x + step / 2 + 200, y + h, mid.z + step / 2 + 200),
            );
            // The opening rule: anything crossing here means no panel here.
            if routes.iter().any(|run| crosses(run, cell)) {
                continue;
            }
            if plan.units.iter().any(|u| u.vol.hits(cell)) {
                continue;
            }
            let mesh = if j + 1 == rows && r.chance(35) { Mesh::Louvre } else { Mesh::Box };
            let size = if along.x != 0 { p3(step, h, 220) } else { p3(220, h, step) };
            let piece = Piece::up(mesh, if mesh == Mesh::Louvre { Mat::Galv } else { Mat::Paint }, p3(mid.x, y, mid.z), size);
            out.push(if mesh == Mesh::Louvre {
                piece.spin(if along.x != 0 { 0 } else { 1 }).lod(CLOSE)
            } else {
                piece.lod(FAR)
            });
        }
    }
}

/// Does any part of this run pass through that box?
fn crosses(run: &Run, cell: Vol) -> bool {
    let pad = run.bore;
    for i in 1..run.path.len() {
        let (a, b) = (run.path[i - 1], run.path[i]);
        let seg = Vol::new(a, b).grow(pad);
        if seg.hits(cell) {
            return true;
        }
    }
    false
}

/// A roof, with a hole wherever something is too tall to be under it.
fn roof(floor: Vol, height: Mm, plan: &Plan, out: &mut Vec<Piece>) {
    let s = floor.size();
    let nx = (s.x / (PANEL * 2)).max(1);
    let nz = (s.z / (PANEL * 2)).max(1);
    let (px, pz) = (s.x / nx, s.z / nz);
    for i in 0..nx {
        for j in 0..nz {
            let at = p3(floor.lo.x + px * i + px / 2, height, floor.lo.z + pz * j + pz / 2);
            let cell = Vol::new(
                p3(at.x - px / 2, height - 200, at.z - pz / 2),
                p3(at.x + px / 2, height + 400, at.z + pz / 2),
            );
            if plan.units.iter().any(|u| u.vol.hi.y > height && u.vol.hits(cell)) {
                continue;
            }
            out.push(Piece::up(Mesh::Box, Mat::Galv, at, p3(px, 260, pz)).lod(FAR));
        }
    }
    // Purlins, so the underside is not a slab of nothing.
    for i in 0..=nx {
        let x = floor.lo.x + px * i;
        out.push(
            Piece::span(Mesh::Beam, Mat::Dark, p3(x, height - 260, floor.lo.z), p3(x, height - 260, floor.hi.z), 260)
                .lod(MEDIUM),
        );
    }
}
