//! Procedural dressing: thirty-eight components, assembled out of twenty-five
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

use super::kit::{Mat, Mesh};
use super::layout::{Arch, Placed, Plan};
use super::seed::Seed;
use super::{p3, route, Mm, Piece, CLOSE, FAR, MEDIUM, P3};
use crate::machine::parts::{self, Dir};
use crate::machine::stuff::Domain;

/// One component, made of pieces.
pub fn dress(u: &Placed, plan: &Plan, seed: &Seed, id: u16, out: &mut Vec<Piece>) {
    let mut r = seed.at(&u.name, "body");
    let wear = r.range(0, 6) as u8;
    let n0 = out.len();

    match u.arch {
        Arch::Vessel => vessel(u, &mut r, out),
        Arch::Shell => shell(u, &mut r, out),
        Arch::Skid => skid(u, &mut r, out),
        Arch::Portal => portal(u, &mut r, out),
        Arch::Can => can(u, &mut r, out),
        Arch::Bin => bin(u, &mut r, out),
        Arch::Pad => pad(u, &mut r, out),
        Arch::Tower => tower(u, &mut r, out),
        Arch::Wheel => wheel(u, &mut r, out),
        Arch::Inline => inline(u, &mut r, out),
        Arch::Bank => bank(u, &mut r, out),
        Arch::Turbine => turbine(u, &mut r, out),
        Arch::Run => transport(u, plan, &mut r, out),
    }

    // Every port gets a stub, whatever the archetype: it is what makes a pipe
    // look bolted on rather than pushed through the wall.
    for s in &u.sockets {
        if u.arch == Arch::Run {
            continue;
        }
        let b = s.bore;
        out.push(
            Piece::new(Mesh::Nozzle, nozzle_mat(s.dom), s.at, s.out, p3(b * 13 / 10, b * 9 / 10, b * 13 / 10))
                .lod(MEDIUM),
        );
    }

    // A gauge or two, on the machines that would have them.
    if matches!(u.arch, Arch::Vessel | Arch::Shell | Arch::Tower | Arch::Turbine | Arch::Skid) {
        let c = u.vol.centre();
        let f = super::EAST;
        for k in 0..r.range(1, 2) {
            let y = u.vol.lo.y + (u.vol.hi.y - u.vol.lo.y) * (2 + k) / 5;
            out.push(
                Piece::new(Mesh::Gauge, Mat::Steel, p3(u.vol.hi.x, y, c.z + k * 400 - 200), f, p3(260, 320, 260))
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

fn vessel(u: &Placed, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
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
}

fn tower(u: &Placed, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    vessel(u, r, out);
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

fn shell(u: &Placed, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let a = along(u);
    let (len, wide) = extent(u, a);
    let s = u.vol.size();
    let d = wide.min(s.y);
    let c = u.vol.centre();
    let m = skin(u);
    let axis = p3(c.x, u.vol.lo.y + d / 2, c.z);
    let end = a.mul(len / 2 - d / 6);

    out.push(Piece::new(Mesh::Cyl, m, axis.sub(end), a, p3(d, len - d / 3, d)).lod(FAR));
    for sgn in [1, -1] {
        out.push(
            Piece::new(Mesh::Dome, m, axis.add(end.mul(sgn)), a.mul(sgn), p3(d, d / 4, d)).lod(FAR),
        );
    }
    // Saddles.
    for sgn in [1, -1] {
        let at = axis.add(a.mul(sgn * (len / 4)));
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
    if r.chance(60) {
        let side = across(a).mul(wide / 2);
        out.push(
            Piece::up(Mesh::Panel, Mat::Galv, p3(c.x + side.x, u.vol.lo.y, c.z + side.z), p3(700, 1100, 400))
                .lod(CLOSE),
        );
    }
}

// -------------------------------------------------------------------- skids

fn skid(u: &Placed, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
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
    if r.chance(70) {
        let side = across(along(u)).mul(s.z.min(s.x) / 2);
        out.push(
            Piece::up(Mesh::Panel, Mat::Galv, p3(f.x + side.x, f.y + 240, f.z + side.z), p3(800, 1200, 380))
                .lod(CLOSE),
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
    anchors(u, out);
}

// --------------------------------------------------------------------- cans

fn can(u: &Placed, r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let a = along(u);
    let (len, wide) = extent(u, a);
    let s = u.vol.size();
    let d = wide.min(s.y).max(600);
    let c = u.vol.centre();
    let axis = p3(c.x, u.vol.lo.y + d / 2 + 150, c.z);
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
        out.push(
            Piece::up(Mesh::Box, Mat::Dark, p3(at.x, u.vol.lo.y, at.z), p3(if a.x != 0 { 300 } else { d }, 150 + d / 2, if a.x != 0 { d } else { 300 }))
                .lod(MEDIUM),
        );
    }
    if r.chance(80) {
        out.push(
            Piece::up(Mesh::Panel, Mat::Dark, p3(c.x, axis.y + d / 2 - 60, c.z), p3(420, 300, 320))
                .lod(CLOSE),
        );
    }
}

// ---------------------------------------------------------------------- bins

fn bin(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
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

fn wheel(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let a = along(u);
    let (len, wide) = extent(u, a);
    let c = u.vol.centre();
    let axis = p3(c.x, super::layout::SHAFT_Y.min(u.vol.hi.y - 400), c.z);
    let d = wide.min(u.vol.size().y) - 200;

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
}

fn turbine(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let a = along(u);
    let (len, wide) = extent(u, a);
    let c = u.vol.centre();
    let d = wide.min(u.vol.size().y).max(800);
    let axis = p3(c.x, super::layout::SHAFT_Y, c.z);
    let m = skin(u);

    // A casing that tapers: wide where the steam comes in, narrow where the
    // shaft leaves.
    out.push(Piece::new(Mesh::Cyl, m, axis.sub(a.mul(len / 2 - 200)), a, p3(d, (len * 2) / 5, d)).lod(FAR));
    out.push(
        Piece::new(Mesh::Cone, m, axis.add(a.mul(len / 10)), a.neg(), p3(d, (len * 2) / 5, d)).lod(FAR),
    );
    out.push(Piece::new(Mesh::Rotor, Mat::Steel, axis.sub(a.mul(len / 6)), a, p3(d - 100, 400, d - 100)).lod(MEDIUM));
    // The shaft end, and the bearing it runs in.
    out.push(
        Piece::new(Mesh::Cyl, Mat::Steel, axis.add(a.mul(len / 6)), a, p3(260, len / 3, 260)).lod(MEDIUM),
    );
    out.push(
        Piece::new(Mesh::Bearing, Mat::Dark, p3(axis.x, u.vol.lo.y, axis.z).add(a.mul(len / 3)), super::UP, p3(600, axis.y - u.vol.lo.y, 600))
            .spin(if a.x != 0 { 1 } else { 0 })
            .lod(MEDIUM),
    );
    anchors(u, out);
}

// ------------------------------------------------------------------- others

fn inline(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let a = along(u);
    let (len, _) = extent(u, a);
    let c = u.vol.centre();
    let bore = u.sockets.first().map(|s| s.bore).unwrap_or(240);
    let axis = p3(c.x, u.vol.lo.y + 700, c.z);
    out.push(Piece::new(Mesh::Cyl, Mat::Steel, axis.sub(a.mul(len / 2)), a, p3(bore, len, bore)).lod(MEDIUM));
    out.push(
        Piece::new(Mesh::Valve, Mat::Paint, axis.sub(a.mul(bore)), a, p3(bore * 3 / 2, bore * 2, bore * 3 / 2))
            .lod(MEDIUM),
    );
    out.push(Piece::up(Mesh::Beam, Mat::Dark, p3(c.x, 0, c.z), p3(200, axis.y - 200, 200)).lod(MEDIUM));
}

fn bank(u: &Placed, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
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
}

/// A transport component is its own connection: it draws the run between its
/// own two ports, in the treatment its domain gets, and the router then joins
/// the ends of it to whatever it was wired to.
fn transport(u: &Placed, _plan: &Plan, _r: &mut super::seed::Rng, out: &mut Vec<Piece>) {
    let part = parts::part(u.kind);
    let ins: Vec<&super::layout::Socket> =
        u.sockets.iter().filter(|s| part.ports[s.port].dir == Dir::In).collect();
    let outs: Vec<&super::layout::Socket> =
        u.sockets.iter().filter(|s| part.ports[s.port].dir == Dir::Out).collect();
    let (Some(a), Some(b)) = (ins.first(), outs.first()) else {
        return;
    };
    let dom = part.ports[0].dom;
    route::straight(a.at, b.at, a.bore, dom, out);
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
