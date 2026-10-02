//! Procedural dressing: forty-four components, assembled out of thirty
//! meshes.
//!
//! This is the pass that would traditionally be a folder of models. What is
//! here instead is thirteen archetypes, each of which knows how to put a
//! handful of canonical pieces together at whatever proportions it was handed:
//!
//! ```text
//!   Vessel   barrel, dome, skirt, bands, nozzles, ladder
//!   Shell    barrel on saddles, dished ends, nozzles, a gauge
//!   Can      finned or smooth barrel, end bell, feet, terminal box
//!   Skid     bedplate, casing, anchors, control panel
//!   Portal   four columns, a head beam, and the works hanging in the middle
//!   Turbine  casing, rotor, exhaust, bearing pedestal, coupling
//!   ...
//! ```
//!
//! A reactor and a gas buffer are the same six calls with different numbers. So
//! are a mill and a heat exchanger. The variety a viewer sees comes from
//! proportion, material and dressing -- and, mostly, from what the plant has
//! been made to *do*, which is the claim the whole experiment exists to test.
//!
//! # What the seed is allowed to touch
//!
//! Wear, a gauge or two, which side the control panel is on, how many bands go
//! round a vessel. Never a dimension that another pass depends on -- a nozzle
//! is where the socket is, and the socket is where the layout said, because the
//! router has already been told to aim at it.
//!
//! # What experiment 09 added, and where
//!
//! Grey and paint retain the experiment 08 assembly for comparison. Detailed
//! grades also seat the bodies on their supports and fit casings around the
//! authored shaft axes. The logical volumes and interfaces stay with layout;
//! dressing connects the visible skin back to them. The grades add:
//!
//! ```text
//!   C  detail        bedplates, bolts, saddles, manways, discharge flanges
//!   D  articulated   fan cowls, shaft stubs, tube sheets, exhaust hoods,
//!                    guards, davits, governor pedestals, level instruments
//! ```
//!
//! The note that asked for experiment 09 proposed buying those from an asset
//! generator. They are eight to twenty lines each instead, because the thing
//! that was missing from a pump was never a better pump mesh -- it was a
//! volute, a bedplate and a shaft sticking out of the right end.

use super::kit::{Mat, Mesh};
use super::layout::{Arch, Placed, Plan};
use super::seed::Seed;
use super::{p3, Grade, Mm, Piece, CLOSE, FAR, MEDIUM, P3};
use crate::machine::stuff::Domain;

/// Eight directions round a circle, in thousandths. Integer trigonometry for
/// the handful of places -- bolt circles, nozzle clusters -- that want to put
/// something *round* something else.
const RING: [(Mm, Mm); 8] = [
    (1000, 0),
    (707, 707),
    (0, 1000),
    (-707, 707),
    (-1000, 0),
    (-707, -707),
    (0, -1000),
    (707, -707),
];

/// One component, made of pieces.
///
/// Experiment 09 added the `grade` argument and nothing else to this
/// function's contract: the same canonical meshes dress the same logical
/// component. Detailed grades refine support clearances and casing joints;
/// neither the interfaces nor the routed connections move with those details.
pub fn dress(u: &Placed, _plan: &Plan, seed: &Seed, grade: Grade, id: u16, out: &mut Vec<Piece>) {
    let mut r = seed.at(&u.name, "body");
    let wear = r.range(0, 6) as u8;
    let n0 = out.len();
    let g = grade;

    match u.arch {
        Arch::Vessel => vessel(u, &mut r, g, out),
        Arch::Shell => shell(u, &mut r, g, out),
        Arch::Skid => skid(u, &mut r, g, out),
        Arch::Portal => portal(u, &mut r, out),
        Arch::Can => can(u, &mut r, g, out),
        Arch::Bin => bin(u, &mut r, g, out),
        Arch::Pad => pad(u, &mut r, out),
        Arch::Tower => tower(u, &mut r, g, out),
        Arch::Wheel => wheel(u, &mut r, g, out),
        Arch::Inline => inline(u, &mut r, out),
        Arch::Bank => bank(u, &mut r, g, out),
        Arch::Turbine => turbine(u, &mut r, g, out),
    }

    // Every port gets a stub, whatever the archetype: it is what makes a pipe
    // look bolted on rather than pushed through the wall.
    let bodies: Vec<_> = out[n0..].iter().filter(|p| {
        matches!(p.mesh, Mesh::Box | Mesh::Cyl | Mesh::Fins | Mesh::Cone | Mesh::Dome | Mesh::Volute) && p.mat != Mat::Dark
    }).cloned().collect();
    for s in u.sockets.iter().filter(|s| s.connected) {
        let b = super::route::outer(s.dom, s.join_bore);
        if g.detailed() {
            // The pump's fluid belongs to its process casing, and an external
            // drive belongs to its rear housing. Do not attach a water neck
            // to whichever motor surface happens to be nearest the socket.
            let solids = if g.articulated() && matches!(u.kind, crate::machine::parts::Kind::Pump | crate::machine::parts::Kind::MechPump) {
                if s.dom == Domain::Fluid { &bodies[..1] }
                else if matches!(s.dom, Domain::Rotary | Domain::Mech) { &bodies[1..2] }
                else { &bodies[..] }
            } else { &bodies[..] };
            if let Some(start) = neck_start(s, solids, u.vol.size().len()) {
                let run = s.at.sub(start);
                if run.len() > 30 {
                    out.push(Piece::new(Mesh::Cyl, nozzle_mat(s.dom), start, run, p3(b, run.len(), b)).lod(MEDIUM));
                }
            }
        }
        out.push(
            Piece::new(Mesh::Nozzle, nozzle_mat(s.dom), s.at, s.out, p3(b * 13 / 10, b * 9 / 10, b * 13 / 10))
                .lod(MEDIUM),
        );
    }

    // A gauge or two, on the machines that would have them.
    if matches!(u.arch, Arch::Vessel | Arch::Shell | Arch::Tower | Arch::Turbine | Arch::Skid) {
        let c = u.vol.centre();
        let body = out[n0..].iter().find(|p| matches!(p.mesh, Mesh::Cyl | Mesh::Box) && p.mat != Mat::Dark).cloned();
        for k in 0..r.range(1, 2) {
            let y = u.vol.lo.y + (u.vol.hi.y - u.vol.lo.y) * (2 + k) / 5;
            let mut at = p3(u.vol.hi.x, y, c.z + k * 400 - 200);
            let mut f = super::EAST;
            if g.detailed() {
                if let Some(p) = &body {
                    let b = p.vol();
                    // Mount to the actual shell, not the clearance box. On a
                    // horizontal barrel the instruments sit on its side.
                    if p.mesh == Mesh::Cyl && p.dir.y == 0 {
                        f = across(p.dir);
                        at = b.centre().add(f.mul(p.size.x / 2 - 20)).add(p.dir.mul(k * 300 - 150));
                    } else {
                        at = p3(b.hi.x - 20, y.clamp(b.lo.y + 100, b.hi.y - 100), b.centre().z);
                    }
                }
            }
            out.push(
                Piece::new(Mesh::Gauge, Mat::Steel, at, f, p3(260, 320, 260))
                    .lod(CLOSE),
            );
        }
    }

    for p in out[n0..].iter_mut() {
        p.of = id;
        if p.tint == 0 {
            p.tint = wear;
        }
    }
}

/// Test the authored solids in integer local coordinates. The pipe router
/// owns the logical socket; dressing only bridges the gap to the visible skin.
fn inside_body(p: &Piece, at: P3) -> bool {
    let delta = at.sub(p.at);
    let dot = |a: P3, b: P3| a.x as i64 * b.x as i64 + a.y as i64 * b.y as i64 + a.z as i64 * b.z as i64;
    let x = dot(delta, super::right_of(p.dir, p.spin)) / p.size.x.max(1) as i64;
    let z = dot(delta, super::right_of(p.dir, (p.spin + 1) & 3)) / p.size.z.max(1) as i64;
    let y = dot(delta, p.dir) / p.dir.len().max(1) as i64;
    if y < 0 || y > p.size.y as i64 { return false; }
    let radial = x * x + z * z;
    match p.mesh {
        Mesh::Box => x.abs() <= 500 && z.abs() <= 500,
        Mesh::Cyl => radial <= 500 * 500,
        // A conservative core lies inside the scroll at every axial depth,
        // including its rounded shoulders. Necks overlap the casting skin.
        Mesh::Volute => radial <= 250 * 250,
        Mesh::Fins => radial <= 420 * 420,
        Mesh::Cone => radial <= (500 - 340 * y / p.size.y.max(1) as i64).pow(2),
        Mesh::Dome => radial + (500 * y / p.size.y.max(1) as i64).pow(2) <= 500 * 500,
        _ => false,
    }
}

fn neck_start(s: &super::layout::Socket, bodies: &[Piece], reach: Mm) -> Option<P3> {
    // Prefer a straight, face-normal neck. If a port is above a reduced
    // casing, an inclined neck still physically meets its unchanged interface.
    let march = |target: P3| {
        let d = target.sub(s.at);
        let len = d.len().max(1);
        (0..=len).step_by(10).find_map(|t| {
            let point = s.at.add(p3(
                (d.x as i64 * t as i64 / len as i64) as Mm,
                (d.y as i64 * t as i64 / len as i64) as Mm,
                (d.z as i64 * t as i64 / len as i64) as Mm,
            ));
            bodies.iter().any(|p| inside_body(p, point)).then_some(point)
        })
    };
    march(s.at.sub(s.out.mul(reach))).or_else(|| {
        bodies.iter().filter_map(|p| march(p.vol().centre())).min_by_key(|p| p.sub(s.at).len())
    })
}

/// What a stub is made of: the domain it carries, in one glance. Lagged for
/// heat, copper for electrical, bare steel for everything else.
fn nozzle_mat(d: Domain) -> Mat {
    match d {
        Domain::Heat => Mat::Lag,
        Domain::Electrical => Mat::Copper,
        Domain::Rotary | Domain::Mech => Mat::Steel,
        _ => Mat::Steel,
    }
}

/// The material a machine's body is, by what it does. Heat equipment is
/// lagged, process equipment is painted, structure is galvanised -- and after
/// four seconds of looking at a plant, a stranger can tell them apart.
fn skin(u: &Placed) -> Mat {
    use crate::machine::parts::Family;
    match u.kind.family() {
        Family::Heat => Mat::Lag,
        Family::Source if u.kind == crate::machine::parts::Kind::Reactor => Mat::Lag,
        Family::Store => Mat::Steel,
        Family::Sink => Mat::Galv,
        _ => Mat::Paint,
    }
}

/// The long axis of a footprint. A shell lies down the length of its plot,
/// which is how a player's plan becomes a machine's orientation for free.
fn along(u: &Placed) -> P3 {
    let s = u.vol.size();
    if s.x >= s.z {
        super::EAST
    } else {
        super::SOUTH
    }
}

fn across(a: P3) -> P3 {
    if a.x != 0 {
        super::SOUTH
    } else {
        super::EAST
    }
}

/// How long the body is along `a`, and how wide across it.
fn extent(u: &Placed, a: P3) -> (Mm, Mm) {
    let s = u.vol.size();
    if a.x != 0 {
        (s.x, s.z)
    } else {
        (s.z, s.x)
    }
}

// ------------------------------------------------------------------ vessels

fn vessel(u: &Placed, r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let d = s.x.min(s.z);
    let f = u.vol.foot();
    let head = d / 3;
    let barrel = (s.y - head - 300).max(600);
    let m = skin(u);

    // A skirt, so it does not look like a bottle standing on a table.
    out.push(Piece::up(Mesh::Cyl, Mat::Dark, f, p3(d * 9 / 10, 300, d * 9 / 10)).lod(FAR));
    out.push(Piece::up(Mesh::Cyl, m, p3(f.x, f.y + 300, f.z), p3(d, barrel, d)).lod(FAR));
    out.push(Piece::up(Mesh::Dome, m, p3(f.x, f.y + 300 + barrel, f.z), p3(d, head, d)).lod(FAR));

    // Bands: the seed picks how many, and they are the cheapest possible way
    // to make two vessels of the same size not be the same vessel.
    let bands = r.range(2, 4);
    for i in 0..bands {
        let y = f.y + 300 + barrel * (i + 1) / (bands + 1);
        out.push(Piece::up(Mesh::Band, Mat::Steel, p3(f.x, y, f.z), p3(d + 60, 120, d + 60)).lod(CLOSE));
    }
    out.push(
        Piece::up(Mesh::Ladder, Mat::Galv, p3(f.x + d / 2, f.y + 300, f.z), p3(700, barrel, 700))
            .lod(CLOSE),
    );

    // Installation: a vessel this size is held down, and getting into it is
    // somebody's job on a Tuesday.
    if g.detailed() {
        for k in (0..8).step_by(2) {
            let (cx, cz) = RING[k];
            let at = p3(f.x + cx * d / 2200, f.y, f.z + cz * d / 2200);
            out.push(Piece::up(Mesh::Anchor, Mat::Dark, at, p3(300, 300, 300)).lod(CLOSE));
        }
        let man = (d / 5).clamp(500, 900);
        let y = f.y + 300 + barrel / 3;
        out.push(
            Piece::new(Mesh::Flange, Mat::Steel, p3(f.x + d / 2 - 60, y, f.z), super::EAST, p3(man, man / 3, man))
                .lod(CLOSE),
        );
        // A sealed access cover, recessed inside the flange's bolt circle.
        // The reusable flange is hollow; without the lid a manway looks open.
        out.push(Piece::new(Mesh::Cyl, Mat::Steel,
            p3(f.x + d / 2 - 60 + man / 3 - 25, y, f.z), super::EAST,
            p3(man * 7 / 10, 45, man * 7 / 10)).lod(CLOSE));
        // A narrow fabricated seam where the dished head meets the barrel.
        out.push(Piece::up(Mesh::Band, Mat::Steel,
            p3(f.x, f.y + 300 + barrel - 25, f.z), p3(d + 24, 50, d + 24)).lod(MEDIUM));
    }

    // Articulation: what is bolted to the top of it, and what lets the skirt
    // breathe. Neither changes the shape of the vessel by a millimetre.
    if g.articulated() {
        // External level instrument: two shell taps, a slim vertical body,
        // and retaining collars. It sits beside the access platform.
        let z = f.z + d / 2 + 110;
        let low = f.y + 300 + barrel / 4;
        let high = f.y + 300 + barrel * 3 / 4;
        out.push(Piece::up(Mesh::Cyl, Mat::Steel, p3(f.x, low, z), p3(100, high - low, 100)).lod(CLOSE));
        for y in [low, high] {
            out.push(Piece::new(Mesh::Cyl, Mat::Steel, p3(f.x, y, f.z + d / 2 - 30),
                super::SOUTH, p3(85, 140, 85)).lod(CLOSE));
            out.push(Piece::up(Mesh::Band, Mat::Steel, p3(f.x, y - 15, z), p3(140, 30, 140)).lod(CLOSE));
        }
        for k in [1usize, 5] {
            let (cx, cz) = RING[k];
            let at = p3(f.x + cx * d / 5000, f.y + 300 + barrel + head / 2, f.z + cz * d / 5000);
            out.push(Piece::up(Mesh::Cyl, Mat::Steel, at, p3(d / 8, head, d / 8)).lod(MEDIUM));
        }
        for k in [0usize, 4] {
            let (cx, cz) = RING[k];
            let at = p3(f.x + cx * d / 2100, f.y + 40, f.z + cz * d / 2100);
            out.push(
                Piece::up(Mesh::Louvre, Mat::Galv, at, p3(d / 3, 220, 160))
                    .spin(if cx == 0 { 1 } else { 0 })
                    .lod(CLOSE),
            );
        }
        // A davit: the little crane that lifts the head off. Two beams, and
        // the vessel suddenly looks maintained rather than delivered.
        let top = f.y + 300 + barrel + head;
        out.push(Piece::up(Mesh::Beam, Mat::Dark, p3(f.x + d / 2, f.y + 300 + barrel - 150, f.z), p3(160, head + 1350, 160)).lod(MEDIUM));
        out.push(
            Piece::span(Mesh::Beam, Mat::Dark, p3(f.x + d / 2, top + 1200, f.z), p3(f.x + d / 6, top + 1200, f.z), 140)
                .lod(MEDIUM),
        );
    }
}

fn tower(u: &Placed, r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    vessel(u, r, g, out);
    // A column is a vessel with trays, and the trays are what say *column*.
    let s = u.vol.size();
    let d = s.x.min(s.z);
    let f = u.vol.foot();
    for i in 1..6 {
        let y = f.y + s.y * i / 6;
        out.push(Piece::up(Mesh::Band, Mat::Steel, p3(f.x, y, f.z), p3(d + 110, 90, d + 110)).lod(MEDIUM));
    }
}

// ------------------------------------------------------------------- shells

fn shell(u: &Placed, r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let a = along(u);
    let (len, wide) = extent(u, a);
    let s = u.vol.size();
    let full_d = wide.min(s.y);
    // Reserve real room for the saddles below the shell. The axis stays put,
    // so the interfaces and the routed plant remain in the same places.
    let d = if g.detailed() { full_d * 4 / 5 } else { full_d };
    let c = u.vol.centre();
    let m = skin(u);
    let axis = p3(c.x, u.vol.lo.y + full_d / 2, c.z);
    let end = a.mul(len / 2 - d / 6);

    out.push(Piece::new(Mesh::Cyl, m, axis.sub(end), a, p3(d, len - d / 3, d)).lod(FAR));
    for sgn in [1, -1] {
        out.push(
            Piece::new(Mesh::Dome, m, axis.add(end.mul(sgn)), a.mul(sgn), p3(d, d / 4, d)).lod(FAR),
        );
    }
    // Saddles. A horizontal vessel does not stand on a block, it sits in a
    // cradle -- which is a mesh at grade C and a block before it, and is the
    // single clearest example of what "archetype articulation" buys.
    for sgn in [1, -1] {
        let at = axis.add(a.mul(sgn * (len / 4)));
        if g.detailed() {
            let height = (full_d - d) * 50 / 62;
            let rise = height * 38 / 100;
            let width = 2 * ((d as i64 * rise as i64 - (rise as i64).pow(2)).max(1).isqrt() as Mm);
            out.push(
                Piece::up(Mesh::Saddle, Mat::Dark, p3(at.x, u.vol.lo.y, at.z), p3(width, height, 460))
                    .spin(if a.x != 0 { 1 } else { 0 })
                    .lod(MEDIUM),
            );
        } else {
            out.push(
                Piece::up(
                    Mesh::Box,
                    Mat::Dark,
                    p3(at.x, u.vol.lo.y, at.z),
                    p3(if a.x != 0 { 400 } else { d + 200 }, d / 2, if a.x != 0 { d + 200 } else { 400 }),
                )
                .lod(MEDIUM),
            );
        }
    }

    // The tube sheet. An exchanger is a shell with a bolted head at each end,
    // and the two big flanges are the entire reason it does not read as a
    // tank lying down.
    if g.articulated() {
        for sgn in [1, -1] {
            out.push(
                Piece::new(
                    Mesh::Flange,
                    Mat::Steel,
                    axis.add(end.mul(sgn)).sub(a.mul(sgn * 40)),
                    a.mul(sgn),
                    p3(d * 12 / 10, d / 8, d * 12 / 10),
                )
                .lod(MEDIUM),
            );
        }
        // A manway on the crown, and a lifting lug either side of it.
        let man = (d / 4).clamp(500, 1000);
        out.push(
            Piece::up(Mesh::Flange, Mat::Steel, p3(axis.x, axis.y + d / 2 - 60, axis.z), p3(man, man / 3, man))
                .lod(CLOSE),
        );
        out.push(
            Piece::up(Mesh::Dome, Mat::Steel, p3(c.x, axis.y + d / 2 + man / 3 - 60, c.z), p3(man * 8 / 10, man / 3, man * 8 / 10))
                .lod(CLOSE),
        );
    }
    if r.chance(60) {
        let side = across(a).mul(wide / 2);
        out.push(
            Piece::up(Mesh::Panel, Mat::Galv, p3(c.x + side.x, u.vol.lo.y, c.z + side.z), p3(700, 1100, 400))
                .lod(CLOSE),
        );
    }
}

// -------------------------------------------------------------------- skids

fn skid(u: &Placed, r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    let m = skin(u);
    out.push(Piece::up(Mesh::Box, Mat::Dark, f, p3(s.x, 240, s.z)).lod(MEDIUM));
    out.push(
        Piece::up(Mesh::Box, m, p3(f.x, f.y + 240, f.z), p3(s.x - 300, s.y - 240, s.z - 300)).lod(FAR),
    );
    anchors(u, out);

    // A crusher or a furnace gets a feed throat and a stack, which is the
    // difference between a machine and a crate.
    if u.kind == crate::machine::parts::Kind::Crusher {
        out.push(
            Piece::up(Mesh::Cone, Mat::Galv, p3(f.x, u.vol.hi.y - 200, f.z), p3(s.x - 600, 1400, s.z - 600))
                .lod(MEDIUM),
        );
    }
    if matches!(u.kind, crate::machine::parts::Kind::Furnace | crate::machine::parts::Kind::Burner) {
        let off = across(along(u)).mul(s.z.min(s.x) / 3);
        out.push(
            Piece::up(
                Mesh::Stack,
                Mat::Steel,
                p3(f.x + off.x, u.vol.hi.y - 200, f.z + off.z),
                p3(700, 3200, 700),
            )
            .lod(FAR),
        );
    }
    if r.chance(70) || g.articulated() {
        let side = across(along(u)).mul(s.z.min(s.x) / 2);
        out.push(
            Piece::up(Mesh::Panel, Mat::Galv, p3(f.x + side.x, f.y + 240, f.z + side.z), p3(800, 1200, 380))
                .lod(CLOSE),
        );
    }
    if g.articulated() {
        // The guard over the drive end. Yellow, in the material language, and
        // the one piece of a crusher that tells you which end to stand at.
        let a = along(u);
        let at = f.add(a.mul(s.x.max(s.z) / 2 - 500));
        out.push(
            Piece::up(Mesh::Box, Mat::Warn, p3(at.x, u.vol.hi.y - 100, at.z), p3(900, 500, 900))
                .lod(MEDIUM),
        );
        // A stool for whatever drives it, on the same bedplate.
        out.push(
            Piece::up(Mesh::Box, Mat::Dark, p3(f.x - a.x * (s.x / 3), f.y + 240, f.z - a.z * (s.z / 3)), p3(s.x / 4, 400, s.z / 4))
                .lod(MEDIUM),
        );
    }
}

fn portal(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    let m = skin(u);
    out.push(Piece::up(Mesh::Box, Mat::Dark, f, p3(s.x, 300, s.z)).lod(MEDIUM));
    // Four legs and a head: a press is a hole with a machine round it.
    for (dx, dz) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
        let at = p3(f.x + dx * (s.x / 2 - 300), f.y + 300, f.z + dz * (s.z / 2 - 300));
        out.push(Piece::up(Mesh::Beam, Mat::Dark, at, p3(420, s.y - 300, 420)).lod(MEDIUM));
    }
    out.push(
        Piece::up(Mesh::Box, m, p3(f.x, u.vol.hi.y - 900, f.z), p3(s.x - 200, 900, s.z - 200)).lod(FAR),
    );
    // The ram, halfway down the gap.
    out.push(
        Piece::up(Mesh::Box, Mat::Steel, p3(f.x, f.y + s.y / 2, f.z), p3(s.x / 3, s.y / 3, s.z / 3))
            .lod(FAR),
    );
    out.push(Piece::up(Mesh::Box, Mat::Steel, p3(f.x, f.y + 500, f.z), p3(s.x * 3 / 4, 400, s.z * 3 / 4)).lod(MEDIUM));
    out.push(Piece::up(Mesh::Box, Mat::Dark, p3(f.x, f.y + 900, f.z), p3(s.x / 3, 160, s.z / 3)).lod(MEDIUM));
    for side in [-1, 1] {
        out.push(Piece::up(Mesh::Cyl, Mat::Steel, p3(f.x + side * s.x / 4, f.y + 1100, f.z), p3(140, (s.y - 1900).max(400), 140)).lod(MEDIUM));
    }
    out.push(Piece::up(Mesh::Panel, Mat::Galv, p3(f.x + s.x / 2 - 200, f.y + 500, f.z + s.z / 3), p3(450, 900, 280)).lod(CLOSE));
    anchors(u, out);
}

// --------------------------------------------------------------------- cans

fn can(u: &Placed, r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    if g.articulated() && matches!(u.kind, crate::machine::parts::Kind::Pump | crate::machine::parts::Kind::MechPump) {
        pump(u, out);
        return;
    }
    let drive_socket = u.sockets.iter().find(|s| matches!(s.dom, Domain::Rotary | Domain::Mech));
    let a = if g.detailed() { drive_socket.map(|s| s.out).unwrap_or_else(|| super::layout::face(u.yaw)) } else { along(u) };
    let (len, wide) = extent(u, a);
    let s = u.vol.size();
    let mut d = wide.min(s.y).max(600);
    let c = u.vol.centre();
    let mut axis = p3(c.x, u.vol.lo.y + d / 2 + 150, c.z);
    if g.detailed() {
        if let Some(socket) = u.sockets.iter().find(|s| matches!(s.dom, Domain::Rotary | Domain::Mech)) {
            axis.y = socket.at.y;
            if a.x != 0 { axis.z = socket.at.z; } else { axis.x = socket.at.x; }
        }
        d = d.min((axis.y - u.vol.lo.y - 300).max(100) * 2);
    }
    let barrel = if u.kind == crate::machine::parts::Kind::Motor { Mesh::Fins } else { Mesh::Cyl };
    let m = skin(u);

    out.push(
        Piece::new(barrel, m, axis.sub(a.mul(len / 2 - 150)), a, p3(d, len - 300, d)).lod(FAR),
    );
    for sgn in [1, -1] {
        out.push(
            Piece::new(
                Mesh::Dome,
                Mat::Steel,
                axis.add(a.mul(sgn * (len / 2 - 150))),
                a.mul(sgn),
                p3(d - 60, 220, d - 60),
            )
            .lod(MEDIUM),
        );
    }
    // Feet, and the terminal box that says it is electrical.
    for sgn in [1, -1] {
        let at = axis.add(a.mul(sgn * (len / 4)));
        if g.detailed() {
            for side in [-1, 1] {
                let foot = at.add(across(a).mul(side * d * 27 / 100));
                let floor = u.vol.lo.y + 160;
                let height = (axis.y - d * 39 / 100 - floor).max(40);
                out.push(Piece::up(Mesh::Box, Mat::Dark, p3(foot.x, floor, foot.z),
                    p3(if a.x != 0 { 300 } else { d / 5 }, height, if a.x != 0 { d / 5 } else { 300 })).lod(MEDIUM));
            }
        } else {
            out.push(
                Piece::up(Mesh::Box, Mat::Dark, p3(at.x, u.vol.lo.y, at.z), p3(if a.x != 0 { 300 } else { d }, 150 + d / 2, if a.x != 0 { d } else { 300 }))
                    .lod(MEDIUM),
            );
        }
    }
    if u.kind == crate::machine::parts::Kind::Motor && (r.chance(80) || g.articulated()) {
        out.push(
            Piece::up(Mesh::Panel, Mat::Dark, p3(axis.x, axis.y + d / 2 - 60, axis.z), p3(420, 300, 320))
                .lod(CLOSE),
        );
    }

    // A bedplate, and the bolts through it. A machine that is *installed*
    // rather than *placed* is most of what the note was asking for.
    if g.detailed() {
        out.push(
            Piece::up(Mesh::Box, Mat::Dark, p3(c.x, u.vol.lo.y, c.z), p3(s.x, 160, s.z)).lod(MEDIUM),
        );
        anchors(u, out);
        for sgn in [-1, 1] {
            out.push(Piece::new(Mesh::Band, Mat::Rubber, axis.add(a.mul(sgn * (len / 2 - 165))),
                a.mul(sgn), p3(d + 12, 25, d + 12)).lod(CLOSE));
        }
    }

    // Which end the drive leaves by, so that the shaft comes out of the right
    // one -- and the fan goes on the other, which is where a fan goes.
    if g.articulated() {
        let drive = u
            .sockets
            .iter()
            .find(|so| matches!(so.dom, Domain::Rotary | Domain::Mech))
            .map(|so| if so.out.x * a.x + so.out.z * a.z >= 0 { 1 } else { -1 })
            .unwrap_or(1);
        // Split castings have machined rims and fastening circles. These
        // live on the dished end bells, within the existing casing envelope.
        let side = across(a);
        for end in [-1, 1] {
            let rim = axis.add(a.mul(end * (len / 2 - 170)));
            out.push(Piece::new(Mesh::Band, Mat::Steel, rim, a.mul(end),
                p3(d + 30, 65, d + 30)).lod(CLOSE));
            for (cx, cy) in RING {
                let bolt = axis.add(a.mul(end * (len / 2 - 18)))
                    .add(side.mul(cx * d / 2600)).add(p3(0, cy * d / 2600, 0));
                out.push(Piece::new(Mesh::Anchor, Mat::Steel, bolt, a.mul(end),
                    p3(110, 45, 110)).lod(CLOSE));
            }
        }
        let nose = axis.add(a.mul(drive * (len / 2 - 150)));
        if drive_socket.is_none() {
            out.push(Piece::new(Mesh::Cyl, Mat::Steel, nose, a.mul(drive), p3(d / 5, len / 5, d / 5)).lod(MEDIUM));
        }
        let tail = axis.sub(a.mul(drive * (len / 2 - 260)));
        out.push(
            Piece::new(Mesh::Cowl, Mat::Galv, tail, a.mul(-drive), p3(d * 9 / 10, len / 6, d * 9 / 10))
                .lod(MEDIUM),
        );
        // A lifting eye, because nothing this heavy is carried.
        out.push(
            Piece::up(Mesh::Band, Mat::Steel, p3(axis.x, axis.y + d / 2 - 40, axis.z), p3(220, 260, 220))
                .lod(CLOSE),
        );
    }
}

// A pump needs two readable masses: the process casing and its drive. This
// scroll casting supplies its silhouette; the router owns every real interface.
fn pump(u: &Placed, out: &mut Vec<Piece>) {
    let drive = u.sockets.iter().find(|s| matches!(s.dom, Domain::Rotary | Domain::Mech));
    let a = drive.map(|s| s.out.neg()).unwrap_or_else(|| super::layout::face(u.yaw));
    let (len, wide) = extent(u, a);
    let c = u.vol.centre();
    let y = drive.map(|s| s.at.y).unwrap_or(u.vol.lo.y + u.vol.size().y * 4 / 7);
    let axis = drive.map(|s| if a.x != 0 { p3(c.x, y, s.at.z) } else { p3(s.at.x, y, c.z) }).unwrap_or(p3(c.x, y, c.z));
    let d = wide.min(u.vol.size().y).min((y - u.vol.lo.y - 250).max(300) * 2);
    let side = across(a);
    let m = skin(u);
    let case = axis.add(a.mul(len / 4));
    let back = axis.sub(a.mul(len / 4));
    let case_len = len / 6;
    let motor_len = len / 3;
    let motor_d = d * 3 / 5;
    let powered = drive.is_none();

    out.push(Piece::new(Mesh::Volute, m, case.sub(a.mul(case_len / 2)), a, p3(d, case_len, d)).lod(FAR));
    out.push(Piece::new(if powered { Mesh::Fins } else { Mesh::Cyl }, m,
        back.sub(a.mul(motor_len / 2)), a, p3(motor_d, motor_len, motor_d)).lod(FAR));
    for end in [-1, 1] {
        let rim = case.add(a.mul(end * case_len / 2));
        out.push(Piece::new(Mesh::Band, Mat::Steel, rim.sub(a.mul(end * 25)), a.mul(end),
            p3(d * 56 / 100, 55, d * 56 / 100)).lod(MEDIUM));
        out.push(Piece::new(Mesh::Dome, m, rim, a.mul(end), p3(d / 2, 65, d / 2)).lod(MEDIUM));
        if end == 1 {
            for (cx, cy) in RING {
                let bolt = rim.add(a.mul(45)).add(side.mul(cx * d / 4500)).add(p3(0, cy * d / 4500, 0));
                out.push(Piece::new(Mesh::Anchor, Mat::Steel, bolt, a, p3(85, 40, 85)).lod(CLOSE));
            }
            out.push(Piece::new(Mesh::Cyl, Mat::Steel, rim.add(a.mul(80)), a,
                p3(d / 4, 55, d / 4)).lod(CLOSE));
        }
    }

    let shaft_start = back.add(a.mul(motor_len / 2));
    let shaft_len = len / 2 - motor_len / 2 - case_len / 2;
    out.push(Piece::new(Mesh::Cyl, Mat::Steel, shaft_start, a, p3(160, shaft_len, 160)).lod(CLOSE));
    out.push(Piece::new(Mesh::Coupling, Mat::Steel, shaft_start.add(a.mul(shaft_len / 2 - 90)), a,
        p3(300, 180, 300)).lod(MEDIUM));
    // An open guard leaves the coupling visible through the space between
    // its straps. The yellow belongs to the guard, not the entire casing.
    let guard_w = 500;
    let guard_h = 340;
    for sign in [-1, 1] {
        let at = shaft_start.add(side.mul(sign * guard_w / 2)).add(p3(0, guard_h / 2, 0));
        out.push(Piece::new(Mesh::Box, Mat::Warn, at, a, p3(35, shaft_len, guard_h)).lod(MEDIUM));
    }
    for i in 0..3 {
        let at = shaft_start.add(a.mul((shaft_len - 45) * i / 2)).add(p3(0, guard_h, 0));
        out.push(Piece::new(Mesh::Box, Mat::Warn, at, a, p3(guard_w + 35, 45, 35)).lod(CLOSE));
    }

    // Separate seats support each casting on the shared bedplate.
    let floor = u.vol.lo.y + 160;
    let size = u.vol.size();
    out.push(Piece::up(Mesh::Box, Mat::Dark, p3(c.x, u.vol.lo.y, c.z), p3(size.x, 160, size.z)).lod(MEDIUM));
    for (centre, dia) in [(case, d), (back, motor_d)] {
        for sign in [-1, 1] {
            let at = centre.add(side.mul(sign * dia / 4));
            let height = (y - dia * 38 / 100 - floor).max(40);
            out.push(Piece::up(Mesh::Box, Mat::Dark, p3(at.x, floor, at.z),
                p3(if a.x != 0 { 340 } else { 260 }, height, if a.x != 0 { 260 } else { 340 })).lod(MEDIUM));
        }
    }
    anchors(u, out);
    if powered {
        out.push(Piece::new(Mesh::Cowl, Mat::Galv, back.sub(a.mul(motor_len / 2 - 60)), a.neg(),
            p3(motor_d, 180, motor_d)).lod(MEDIUM));
        out.push(Piece::up(Mesh::Panel, Mat::Galv, back.add(p3(0, motor_d * 2 / 5 - 40, 0)),
            p3(320, 240, 260)).lod(CLOSE));
    } else {
        out.push(Piece::new(Mesh::Dome, m, back.sub(a.mul(motor_len / 2)), a.neg(),
            p3(motor_d, 90, motor_d)).lod(MEDIUM));
    }
    let instrument = case.add(side.mul(d * 30 / 100)).add(p3(0, d / 5, 0));
    out.push(Piece::new(Mesh::Gauge, Mat::Steel, instrument, side, p3(180, 150, 180)).lod(CLOSE));
    out.push(Piece::new(Mesh::Band, Mat::Steel, case.add(p3(0, d * 42 / 100 - 40, 0)), side,
        p3(150, 100, 150)).lod(CLOSE));
}

// ---------------------------------------------------------------------- bins

fn bin(u: &Placed, _r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    let d = s.x.min(s.z);
    let cone = (s.y * 2) / 5;

    // The legs are not drawn here. A bin on legs is a bin something fits
    // under, and *what holds it up* belongs to the structural pass -- which is
    // what makes it move when the bin moves and vanish when the bin does.
    out.push(Piece::up(Mesh::Cone, Mat::Galv, f, p3(d, cone, d)).lod(FAR));
    out.push(Piece::up(Mesh::Cyl, Mat::Galv, p3(f.x, f.y + cone, f.z), p3(d, s.y - cone, d)).lod(FAR));
    out.push(
        Piece::up(Mesh::Cyl, Mat::Steel, p3(f.x, f.y - 300, f.z), p3(d / 4, 400, d / 4)).lod(MEDIUM),
    );
    if g.detailed() {
        // The discharge is flanged, and there is a slide gate under it.
        out.push(
            Piece::up(Mesh::Flange, Mat::Steel, p3(f.x, f.y - 340, f.z), p3(d / 3, 160, d / 3)).lod(CLOSE),
        );
    }
    if g.articulated() {
        out.push(
            Piece::up(Mesh::Valve, Mat::Warn, p3(f.x, f.y - 200, f.z), p3(d / 3, 500, d / 3)).lod(MEDIUM),
        );
        // A level instrument, up where the material is.
        out.push(
            Piece::new(Mesh::Gauge, Mat::Steel, p3(f.x + d / 2, f.y + cone + (s.y - cone) / 2, f.z), super::EAST, p3(260, 320, 260))
                .lod(CLOSE),
        );
    }
}

fn pad(u: &Placed, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    use crate::machine::parts::Kind;
    let s = u.vol.size();
    let f = u.vol.foot();
    match u.kind {
        Kind::Mains => {
            out.push(Piece::up(Mesh::Box, Mat::Concrete, f, p3(s.x, 300, s.z)).lod(MEDIUM));
            out.push(
                Piece::up(Mesh::Panel, Mat::Galv, p3(f.x, f.y + 300, f.z), p3(s.x - 400, s.y - 1400, s.z / 2))
                    .lod(FAR),
            );
            // Bushings. Copper, because the domain deserves a colour.
            for k in -1..2 {
                out.push(
                    Piece::up(
                        Mesh::Cyl,
                        Mat::Copper,
                        p3(f.x + k * 500, u.vol.hi.y - 1100, f.z),
                        p3(180, 1100, 180),
                    )
                    .lod(MEDIUM),
                );
            }
        }
        Kind::Skip => {
            out.push(Piece::up(Mesh::Box, Mat::Dark, f, p3(s.x, 200, s.z)).lod(MEDIUM));
            // An open box: four walls and no lid.
            let t = 120;
            for (dx, dz, w, l) in [(0, -1, s.x, t), (0, 1, s.x, t), (-1, 0, t, s.z), (1, 0, t, s.z)] {
                out.push(
                    Piece::up(
                        Mesh::Box,
                        Mat::Galv,
                        p3(f.x + dx * (s.x / 2), f.y + 200, f.z + dz * (s.z / 2)),
                        p3(w, s.y - 200, l),
                    )
                    .lod(FAR),
                );
            }
        }
        _ => {
            out.push(Piece::up(Mesh::Box, Mat::Concrete, f, p3(s.x, 400, s.z)).lod(FAR));
            out.push(
                Piece::up(Mesh::Box, Mat::Galv, p3(f.x, f.y + 400, f.z), p3(s.x - 500, s.y - 400, s.z - 500))
                    .lod(FAR),
            );
            if r.chance(50) {
                out.push(
                    Piece::up(Mesh::Panel, Mat::Paint, p3(f.x, f.y + 400, f.z + s.z / 3), p3(600, 900, 300))
                        .lod(CLOSE),
                );
            }
        }
    }
}

// ------------------------------------------------------------------ turning

fn wheel(u: &Placed, _r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let drive = u.sockets.iter().find(|s| matches!(s.dom, Domain::Rotary | Domain::Mech));
    let a = if g.detailed() { drive.map(|s| s.out).unwrap_or_else(|| along(u)) } else { along(u) };
    let (len, wide) = extent(u, a);
    let c = u.vol.centre();
    let y = drive.map(|s| s.at.y).unwrap_or(u.base + super::layout::SHAFT_Y).min(u.vol.hi.y - 400);
    let axis = drive.map(|s| if a.x != 0 { p3(c.x, y, s.at.z) } else { p3(s.at.x, y, c.z) }).unwrap_or(p3(c.x, y, c.z));
    let d = (wide.min(u.vol.size().y) - 200).min((y - u.vol.lo.y - 150).max(300) * 2);

    out.push(Piece::new(Mesh::Rotor, Mat::Steel, axis.sub(a.mul(200)), a, p3(d, 400, d)).lod(FAR));
    for sgn in [1, -1] {
        let at = axis.add(a.mul(sgn * (len / 2 - 250)));
        out.push(
            Piece::new(Mesh::Bearing, Mat::Dark, p3(at.x, u.vol.lo.y, at.z), super::UP, p3(500, axis.y - u.vol.lo.y, 500))
                .spin(if a.x != 0 { 1 } else { 0 })
                .lod(MEDIUM),
        );
    }
    anchors(u, out);
    // A guard round anything with that much stored energy in it. Two rings,
    // yellow, and a flywheel stops looking like a coin on its edge.
    if g.articulated() {
        for k in [-1, 1] {
            out.push(
                Piece::new(Mesh::Band, Mat::Warn, axis.sub(a.mul(200 - k * 300)), a, p3(d + 300, 160, d + 300))
                    .lod(MEDIUM),
            );
        }
    }
    if g.articulated() && u.kind == crate::machine::parts::Kind::Crank {
        let side = across(a);
        let pin = axis.add(side.mul(d / 3)).add(a.mul(280));
        out.push(Piece::new(Mesh::Cyl, Mat::Steel, pin.sub(a.mul(120)), a, p3(220, 480, 220)).lod(MEDIUM));
        out.push(Piece::span(Mesh::Beam, Mat::Steel, axis.add(a.mul(240)), pin, 240).lod(MEDIUM));
        if let Some(socket) = u.sockets.iter().find(|s| s.dom == Domain::Mech && s.connected) {
            out.push(Piece::span(Mesh::Cyl, Mat::Steel, pin, socket.at, 180).lod(MEDIUM));
        }
        out.push(Piece::up(Mesh::Box, Mat::Dark, u.vol.foot(), p3(u.vol.size().x, 140, u.vol.size().z)).lod(MEDIUM));
    }
}

fn turbine(u: &Placed, _r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let drive = u.sockets.iter().find(|s| matches!(s.dom, Domain::Rotary | Domain::Mech));
    let a = if g.detailed() { drive.map(|s| s.out).filter(|d| d.y == 0).unwrap_or_else(|| along(u)) } else { along(u) };
    let (len, wide) = extent(u, a);
    let c = u.vol.centre();
    let mut d = wide.min(u.vol.size().y).max(800);
    let y = if g.detailed() { drive.map(|s| s.at.y).unwrap_or(u.base + super::layout::SHAFT_Y) } else { super::layout::SHAFT_Y };
    let axis = drive.map(|s| if a.x != 0 { p3(c.x, y, s.at.z) } else { p3(s.at.x, y, c.z) }).unwrap_or(p3(c.x, y, c.z));
    if g.detailed() { d = d.min((y - u.vol.lo.y - 180).max(100) * 2); }
    let m = skin(u);

    // A casing that tapers: wide where the steam comes in, narrow where the
    // shaft leaves.
    out.push(Piece::new(Mesh::Cyl, m, axis.sub(a.mul(len / 2 - 200)), a, p3(d, (len * 2) / 5, d)).lod(FAR));
    let joint = axis.add(a.mul(-len / 2 + 200 + len * 2 / 5));
    if g.detailed() {
        out.push(Piece::new(Mesh::Cone, m, joint, a, p3(d, (len * 3 / 10 - 200).max(200), d)).lod(FAR));
        out.push(Piece::new(Mesh::Band, Mat::Rubber, joint.sub(a.mul(12)), a, p3(d + 16, 24, d + 16)).lod(CLOSE));
    } else {
        out.push(Piece::new(Mesh::Cone, m, axis.add(a.mul(len / 10)), a.neg(), p3(d, (len * 2) / 5, d)).lod(FAR));
    }
    out.push(Piece::new(Mesh::Rotor, Mat::Steel, axis.sub(a.mul(len / 6)), a, p3(d - 100, 400, d - 100)).lod(MEDIUM));
    // The shaft end, and the bearing it runs in.
    if drive.is_some_and(|s| s.connected) {
        out.push(Piece::new(Mesh::Cyl, Mat::Steel, axis.add(a.mul(len / 6)), a, p3(260, len / 3, 260)).lod(MEDIUM));
    }
    out.push(
        Piece::new(Mesh::Bearing, Mat::Dark, p3(axis.x, u.vol.lo.y, axis.z).add(a.mul(len / 3)), super::UP, p3(600, axis.y - u.vol.lo.y, 600))
            .spin(if a.x != 0 { 1 } else { 0 })
            .lod(MEDIUM),
    );
    anchors(u, out);

    // A turbine casing is lagged where the steam is and bare where the shaft
    // is, and it is bolted together down the middle. Three bands and a flange
    // is the whole of it.
    if g.detailed() {
        for k in 1..4 {
            out.push(
                Piece::new(Mesh::Band, Mat::Lag, axis.sub(a.mul(len / 2 - len * k / 8)), a, p3(d + 80, 140, d + 80))
                    .lod(CLOSE),
            );
        }
    }
    if g.articulated() {
        out.push(
            Piece::new(Mesh::Flange, Mat::Steel, joint.sub(a.mul(d / 20)), a, p3(d * 11 / 10, d / 10, d * 11 / 10))
                .lod(MEDIUM),
        );
        // The exhaust hood: a turbine throws its steam downwards at whatever
        // is condensing it, and the flare under the casing is the cue.
        let hood = axis.sub(a.mul(len / 5));
        out.push(
            Piece::up(Mesh::Cone, m, p3(hood.x, u.vol.lo.y, hood.z), p3(d * 9 / 10, axis.y - u.vol.lo.y, d * 9 / 10))
                .lod(MEDIUM),
        );
        // The governor, on its own pedestal, at the shaft end.
        let gov = axis.add(a.mul(len / 2 - 300)).add(across(a).mul(d / 2 + 200));
        out.push(
            Piece::up(Mesh::Box, Mat::Dark, p3(gov.x, u.vol.lo.y, gov.z), p3(600, axis.y - u.vol.lo.y, 600))
                .lod(MEDIUM),
        );
        out.push(
            Piece::up(Mesh::Panel, Mat::Galv, p3(gov.x, axis.y, gov.z), p3(520, 700, 380)).lod(CLOSE),
        );
    }
}

// ------------------------------------------------------------------- others

fn inline(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let a = super::layout::face(u.yaw);
    let (len, _) = extent(u, a);
    let c = u.vol.centre();
    let bore = u.sockets.first().map(|s| s.bore).unwrap_or(240);
    let socket = u.sockets.iter().find(|s| s.connected).or(u.sockets.first());
    let axis = socket.map(|s| if a.x != 0 { p3(c.x, s.at.y, s.at.z) } else { p3(s.at.x, s.at.y, c.z) }).unwrap_or(p3(c.x, u.vol.lo.y + 700, c.z));
    out.push(Piece::new(Mesh::Cyl, Mat::Steel, axis.sub(a.mul(len / 2)), a, p3(bore, len, bore)).lod(MEDIUM));
    out.push(
        Piece::new(Mesh::Valve, Mat::Paint, axis.sub(a.mul(bore)), a, p3(bore * 3 / 2, bore * 2, bore * 3 / 2))
            .lod(MEDIUM),
    );
    out.push(Piece::up(Mesh::Beam, Mat::Dark, p3(axis.x, u.base, axis.z), p3(200, (axis.y - u.base - bore / 2).max(50), 200)).lod(MEDIUM));
}

fn bank(u: &Placed, _r: &mut super::seed::Rng, g: Grade, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    let a = along(u);
    let (len, wide) = extent(u, a);
    // Its frame, like every other frame in the plant, is inferred rather than
    // drawn: see `frame`.
    out.push(Piece::up(Mesh::Box, Mat::Galv, f, p3(s.x, 300, s.z)).lod(FAR));
    // Louvres across the long side, and fans in the top.
    let n = (len / 900).max(2);
    for i in 0..n {
        let at = f.add(a.mul(len * (2 * i + 1) / (2 * n) - len / 2));
        out.push(
            Piece::new(Mesh::Louvre, Mat::Galv, p3(at.x, f.y + 300, at.z), super::UP, p3(len / n - 80, s.y - 400, wide))
                .spin(if a.x != 0 { 1 } else { 0 })
                .lod(MEDIUM),
        );
    }
    if g.detailed() {
        // Headers down both long sides: the thing has to be plumbed into
        // something, and a cooler with no pipework on it is a cupboard.
        for sgn in [1, -1] {
            let off = across(a).mul(sgn * (wide / 2 - 100));
            let mid = p3(f.x + off.x, f.y + 300, f.z + off.z);
            out.push(
                Piece::span(
                    Mesh::Cyl,
                    Mat::Steel,
                    mid.sub(a.mul(len / 2 - 200)),
                    mid.add(a.mul(len / 2 - 200)),
                    260,
                )
                .lod(MEDIUM),
            );
        }
        anchors(u, out);
    }
    // The fans. A cooling unit is a box that moves air, and until there is a
    // fan in the top of it, it is just a box.
    if g.articulated() {
        for i in 0..n {
            let at = f.add(a.mul(len * (2 * i + 1) / (2 * n) - len / 2));
            let dia = (len / n - 200).min(wide - 200).max(400);
            out.push(
                Piece::up(Mesh::Cowl, Mat::Galv, p3(at.x, u.vol.hi.y - 300, at.z), p3(dia, 500, dia))
                    .lod(MEDIUM),
            );
        }
    }
}
fn anchors(u: &Placed, out: &mut Vec<Piece>) {
    let s = u.vol.size();
    let f = u.vol.foot();
    for (dx, dz) in [(-1, -1), (1, -1), (1, 1), (-1, 1)] {
        out.push(
            Piece::up(
                Mesh::Anchor,
                Mat::Dark,
                p3(f.x + dx * (s.x / 2 - 150), f.y, f.z + dz * (s.z / 2 - 150)),
                p3(360, 360, 360),
            )
            .lod(CLOSE),
        );
    }
}
