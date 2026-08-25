//! The library: twenty-nine canonical meshes, twelve materials, and nothing
//! that knows what a turbine is.
//!
//! Experiment 08's second section asks for a *tiny authored asset library* and
//! is explicit about why: procedurally synthesising every detailed mesh is a
//! research project, and hand-modelling one mesh per component is a content
//! pipeline. The middle is a small set of canonical pieces, assembled a great
//! many times.
//!
//! So there is no `turbine.mesh` here. A turbine is a `Cyl` with a `Dome` on
//! each end, a `Rotor` in the middle, four `Anchor` feet and a `Nozzle` per
//! port -- and a mill is those same meshes at different proportions in a
//! different material. Thirty-eight components share twenty-nine meshes, which
//! is the claim in section 2 of the note, made countable.
//!
//! # Canonical space
//!
//! Every mesh is built inside
//!
//! ```text
//!   x, z  in  -0.5 .. 0.5      the footprint, centred on the origin
//!   y     in   0.0 .. 1.0      up, standing on the origin plane
//! ```
//!
//! so that one instance is `(mesh, position, direction, spin, size)` and a
//! four-metre pipe is the same cylinder as a four-centimetre bolt. Placing a
//! piece maps canonical `+Y` onto its direction: a pipe run points along the
//! run, a column points up, a brace points wherever a brace points.
//!
//! # Why the meshes are generated rather than shipped
//!
//! Because they have to exist twice -- once in Rust, to be baked into an `.obj`
//! and counted, and once in the browser, to be drawn -- and a generator is one
//! description of a shape where a pair of files is two. It also keeps the whole
//! experiment inside `std` and inside the repo, which is the rule everything
//! else here follows.

use std::fmt;

// ------------------------------------------------------------------ meshes

#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Mesh {
    /// The workhorse: bodies, panels, plinths, slabs, ducts, walls.
    Box,
    /// Pipes, shafts, shells, vessels, posts, rails.
    Cyl,
    /// A vessel head, and the only reason a tank does not look like a bin.
    Dome,
    /// Hoppers, cyclones, chutes: a frustum, wide at the bottom.
    Cone,
    /// A quarter bend. Bend radius 1.5, tube radius 0.5, in the canonical XY
    /// plane: in at the origin along `+Y`, out at `(1.5, 1.5, 0)` along `+X`.
    Elbow,
    /// A run with a branch: where one wire feeds two.
    Tee,
    Flange,
    /// The stub that takes a pipe off a vessel wall.
    Nozzle,
    Valve,
    /// Insulation banding, and the ribs on a lagged run.
    Band,
    /// An I-profile, for every column, beam and brace.
    Beam,
    /// One panel of walkway grating, with its edge angle.
    Grate,
    /// One stair tread and its riser.
    Step,
    /// A unit length of two-rail handrail: run along `+Y`, uprights along the
    /// local `+Z`.
    Rail,
    /// A pipe support: post, cross-head and cradle.
    Support,
    /// A bolted foot plate. Four of them is why a machine looks fixed down.
    Anchor,
    /// Two flanges and their bolts, between two lengths of shaft.
    Coupling,
    /// A pillow block, which is what a line shaft rests in.
    Bearing,
    Gauge,
    /// A junction box, a control panel, a terminal cabinet.
    Panel,
    /// A finned barrel: motors, and anything that has to lose heat sideways.
    Fins,
    /// A louvred vent, for radiators and for wall infill.
    Louvre,
    Stack,
    Ladder,
    /// A bladed disc. The one piece that says *this thing turns*.
    Rotor,
    /// Experiment 09. A concentric reducer, flanged both ends: the piece that
    /// says a line changed size *on purpose*.
    Reducer,
    /// A pipe clamp: a band with two bolted ears. What actually holds a run
    /// down onto a support.
    Clamp,
    /// A fan ring: shroud, hub and blades. The one cue that says *this box
    /// moves air*.
    Cowl,
    /// A saddle: base plate, web plates and a cradle. What a horizontal vessel
    /// sits in, as opposed to on.
    Saddle,
}

pub const MESHES: [Mesh; 29] = [
    Mesh::Box,
    Mesh::Cyl,
    Mesh::Dome,
    Mesh::Cone,
    Mesh::Elbow,
    Mesh::Tee,
    Mesh::Flange,
    Mesh::Nozzle,
    Mesh::Valve,
    Mesh::Band,
    Mesh::Beam,
    Mesh::Grate,
    Mesh::Step,
    Mesh::Rail,
    Mesh::Support,
    Mesh::Anchor,
    Mesh::Coupling,
    Mesh::Bearing,
    Mesh::Gauge,
    Mesh::Panel,
    Mesh::Fins,
    Mesh::Louvre,
    Mesh::Stack,
    Mesh::Ladder,
    Mesh::Rotor,
    Mesh::Reducer,
    Mesh::Clamp,
    Mesh::Cowl,
    Mesh::Saddle,
];

impl Mesh {
    pub fn tag(self) -> &'static str {
        match self {
            Mesh::Box => "box",
            Mesh::Cyl => "cyl",
            Mesh::Dome => "dome",
            Mesh::Cone => "cone",
            Mesh::Elbow => "elbow",
            Mesh::Tee => "tee",
            Mesh::Flange => "flange",
            Mesh::Nozzle => "nozzle",
            Mesh::Valve => "valve",
            Mesh::Band => "band",
            Mesh::Beam => "beam",
            Mesh::Grate => "grate",
            Mesh::Step => "step",
            Mesh::Rail => "rail",
            Mesh::Support => "support",
            Mesh::Anchor => "anchor",
            Mesh::Coupling => "coupling",
            Mesh::Bearing => "bearing",
            Mesh::Gauge => "gauge",
            Mesh::Panel => "panel",
            Mesh::Fins => "fins",
            Mesh::Louvre => "louvre",
            Mesh::Stack => "stack",
            Mesh::Ladder => "ladder",
            Mesh::Rotor => "rotor",
            Mesh::Reducer => "reducer",
            Mesh::Clamp => "clamp",
            Mesh::Cowl => "cowl",
            Mesh::Saddle => "saddle",
        }
    }

    /// The bend radius of an elbow, as a multiple of the pipe's diameter. The
    /// router needs this number to know where to stop a straight run and the
    /// mesh needs it to know where to put the arc, so it is stated once.
    pub const ELBOW_R: f32 = 1.5;
}

impl fmt::Display for Mesh {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

pub fn by_tag(tag: &str) -> Option<Mesh> {
    MESHES.iter().copied().find(|m| m.tag() == tag)
}

pub fn mat_by_tag(tag: &str) -> Option<Mat> {
    MATS.iter().copied().find(|m| m.tag() == tag)
}

// --------------------------------------------------------------- materials

/// Twelve materials for the whole plant, because the note asked for a shared
/// library rather than a texture per asset -- and because the moment there is
/// one material per component, "procedural assembly" has quietly become
/// "hand-authored models with extra steps".
///
/// Experiment 08 shipped eight, and experiment 09 found that eight was one
/// short of a *language*. Four of them were structural and one was the plant's
/// paint, which left three to say everything about what a thing is for -- so a
/// tank, a pump casing and a wall panel all came out the same colour and the
/// plant flattened. The four added here are the four distinctions a person
/// standing in a works actually makes: what is under pressure, what is water,
/// what is fuel, and what will hurt you.
#[derive(Clone, Copy, PartialEq, Eq, Debug, PartialOrd, Ord, Hash)]
pub enum Mat {
    /// The plant's colour, chosen once per installation from its seed. In the
    /// material language of experiment 09 this is *machinery*: what turns.
    Paint,
    Steel,
    /// Structural steel: dark, and unloved.
    Dark,
    Galv,
    Concrete,
    Copper,
    /// Lagging: pale, matte, and the reason a heat line reads as a heat line.
    Lag,
    Rubber,
    /// Off-white painted vessel steel: tanks, drums, columns, anything with a
    /// pressure in it.
    Cream,
    /// Cold service: a desaturated blue-grey, and the only blue in the plant.
    Water,
    /// Fuel and process: a dark green that reads as *not water* from across a
    /// yard.
    Oil,
    /// Hazard yellow. Handrails, guards, kerbs and the mouth of anything that
    /// will take an arm off. Used sparingly on purpose -- an accent that is
    /// everywhere is a colour.
    Warn,
}

pub const MATS: [Mat; 12] = [
    Mat::Paint,
    Mat::Steel,
    Mat::Dark,
    Mat::Galv,
    Mat::Concrete,
    Mat::Copper,
    Mat::Lag,
    Mat::Rubber,
    Mat::Cream,
    Mat::Water,
    Mat::Oil,
    Mat::Warn,
];

impl Mat {
    pub fn tag(self) -> &'static str {
        match self {
            Mat::Paint => "paint",
            Mat::Steel => "steel",
            Mat::Dark => "dark",
            Mat::Galv => "galv",
            Mat::Concrete => "concrete",
            Mat::Copper => "copper",
            Mat::Lag => "lagging",
            Mat::Rubber => "rubber",
            Mat::Cream => "cream",
            Mat::Water => "water",
            Mat::Oil => "oil",
            Mat::Warn => "warn",
        }
    }

    /// Colour, roughness and metalness. `Paint` is the exception: its colour
    /// comes from the installation's seed, so what is here is the fallback for
    /// a scene nobody seeded.
    pub fn look(self) -> ([u8; 3], u8, bool) {
        match self {
            Mat::Paint => ([88, 122, 148], 60, false),
            Mat::Steel => ([166, 172, 178], 35, true),
            Mat::Dark => ([74, 80, 88], 70, true),
            Mat::Galv => ([148, 156, 160], 45, true),
            Mat::Concrete => ([150, 146, 138], 92, false),
            Mat::Copper => ([176, 116, 74], 40, true),
            Mat::Lag => ([224, 221, 212], 96, false),
            Mat::Rubber => ([44, 44, 48], 88, false),
            Mat::Cream => ([198, 191, 174], 46, false),
            Mat::Water => ([94, 124, 146], 54, false),
            Mat::Oil => ([68, 88, 72], 58, false),
            Mat::Warn => ([206, 166, 46], 62, false),
        }
    }
}

impl fmt::Display for Mat {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// The paint colours an installation may be. Deterministic, and deliberately
/// dull: this is a plant, not a fairground.
pub const PAINTS: [[u8; 3]; 6] = [
    [88, 122, 148],  // works blue
    [122, 132, 96],  // olive
    [156, 132, 84],  // ochre
    [110, 112, 118], // battleship
    [92, 116, 104],  // pale green
    [140, 96, 88],   // oxide red
];

// ------------------------------------------------------------ the triangles

/// A canonical mesh, as triangles. Three parallel arrays and no interleaving,
/// because both consumers -- an `.obj` writer and a WebGL buffer -- want them
/// that way.
#[derive(Clone, Debug, Default)]
pub struct Geom {
    pub pos: Vec<f32>,
    pub nrm: Vec<f32>,
    pub idx: Vec<u32>,
}

impl Geom {
    pub fn tris(&self) -> usize {
        self.idx.len() / 3
    }

    pub fn verts(&self) -> usize {
        self.pos.len() / 3
    }

    fn vert(&mut self, p: [f32; 3], n: [f32; 3]) -> u32 {
        let i = (self.pos.len() / 3) as u32;
        self.pos.extend_from_slice(&p);
        self.nrm.extend_from_slice(&n);
        i
    }

    fn tri(&mut self, a: u32, b: u32, c: u32) {
        self.idx.extend_from_slice(&[a, b, c]);
    }

    /// A flat quad, wound so that `a b c d` goes round the face and the normal
    /// falls out of the winding.
    fn quad(&mut self, a: [f32; 3], b: [f32; 3], c: [f32; 3], d: [f32; 3]) {
        let n = normal(a, b, c);
        let (a, b, c, d) = (self.vert(a, n), self.vert(b, n), self.vert(c, n), self.vert(d, n));
        self.tri(a, b, c);
        self.tri(a, c, d);
    }

    fn cuboid(&mut self, lo: [f32; 3], hi: [f32; 3]) {
        let (x0, y0, z0) = (lo[0], lo[1], lo[2]);
        let (x1, y1, z1) = (hi[0], hi[1], hi[2]);
        self.quad([x0, y0, z1], [x1, y0, z1], [x1, y1, z1], [x0, y1, z1]);
        self.quad([x1, y0, z0], [x0, y0, z0], [x0, y1, z0], [x1, y1, z0]);
        self.quad([x1, y0, z1], [x1, y0, z0], [x1, y1, z0], [x1, y1, z1]);
        self.quad([x0, y0, z0], [x0, y0, z1], [x0, y1, z1], [x0, y1, z0]);
        self.quad([x0, y1, z1], [x1, y1, z1], [x1, y1, z0], [x0, y1, z0]);
        self.quad([x0, y0, z0], [x1, y0, z0], [x1, y0, z1], [x0, y0, z1]);
    }

    /// A frustum about the `Y` axis, with optional caps, `seg` sides.
    fn barrel(&mut self, r0: f32, r1: f32, y0: f32, y1: f32, seg: usize, caps: bool) {
        let slope = ((r0 - r1) / (y1 - y0).abs().max(1e-4)).atan();
        let (cs, sn) = (slope.cos(), slope.sin());
        let base = (self.pos.len() / 3) as u32;
        for i in 0..=seg {
            let a = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let (c, s) = (a.cos(), a.sin());
            let n = [c * cs, sn, s * cs];
            self.vert([c * r0, y0, s * r0], n);
            self.vert([c * r1, y1, s * r1], n);
        }
        for i in 0..seg {
            // Counter-clockwise seen from outside, which is what every
            // renderer downstream assumes when it decides which side of a
            // triangle not to bother drawing.
            let (a, b) = (base + (i as u32) * 2, base + (i as u32) * 2 + 2);
            self.tri(a, a + 1, b);
            self.tri(b, a + 1, b + 1);
        }
        if caps {
            self.disc(r1, y1, seg, true);
            self.disc(r0, y0, seg, false);
        }
    }

    /// A flat disc at height `y`, facing up or down.
    fn disc(&mut self, r: f32, y: f32, seg: usize, up: bool) {
        let n = if up { [0.0, 1.0, 0.0] } else { [0.0, -1.0, 0.0] };
        let c = self.vert([0.0, y, 0.0], n);
        let base = (self.pos.len() / 3) as u32;
        for i in 0..=seg {
            let a = (i as f32 / seg as f32) * std::f32::consts::TAU;
            self.vert([a.cos() * r, y, a.sin() * r], n);
        }
        for i in 0..seg {
            let (a, b) = (base + i as u32, base + i as u32 + 1);
            if up {
                self.tri(c, b, a);
            } else {
                self.tri(c, a, b);
            }
        }
    }

    /// A dome sitting on `y0`, `rings` bands tall.
    fn dome(&mut self, r: f32, h: f32, y0: f32, seg: usize, rings: usize) {
        for k in 0..rings {
            let (t0, t1) = (k as f32 / rings as f32, (k + 1) as f32 / rings as f32);
            let (p0, p1) = (t0 * std::f32::consts::FRAC_PI_2, t1 * std::f32::consts::FRAC_PI_2);
            let base = (self.pos.len() / 3) as u32;
            for i in 0..=seg {
                let a = (i as f32 / seg as f32) * std::f32::consts::TAU;
                for p in [p0, p1] {
                    let (c, s) = (a.cos() * p.cos(), a.sin() * p.cos());
                    self.vert([c * r, y0 + p.sin() * h, s * r], norm([c, p.sin() * r / h, s]));
                }
            }
            for i in 0..seg {
                // The same winding as `barrel`, and for the same reason: the
                // ring is built lower-then-upper per step, so `a, a+1, b` is
                // the order that comes out counter-clockwise from outside. It
                // was `a, b, a+1` here, which is the mirror image -- and a
                // dome whose outside is its inside is culled away by every
                // renderer downstream, which is why tanks were see-through.
                let (a, b) = (base + (i as u32) * 2, base + (i as u32) * 2 + 2);
                self.tri(a, a + 1, b);
                self.tri(b, a + 1, b + 1);
            }
        }
    }

    /// A quarter bend: in at the origin along `+Y`, out along `+X`.
    fn bend(&mut self, bend_r: f32, tube_r: f32, seg: usize, rings: usize) {
        let ring = |th: f32, i: usize| -> ([f32; 3], [f32; 3]) {
            let a = (i as f32 / seg as f32) * std::f32::consts::TAU;
            let rad = [-th.cos(), th.sin(), 0.0];
            let c = [bend_r - bend_r * th.cos(), bend_r * th.sin(), 0.0];
            let n = [rad[0] * a.cos(), rad[1] * a.cos(), a.sin()];
            ([c[0] + n[0] * tube_r, c[1] + n[1] * tube_r, c[2] + n[2] * tube_r], n)
        };
        for k in 0..rings {
            let (t0, t1) = (
                (k as f32 / rings as f32) * std::f32::consts::FRAC_PI_2,
                ((k + 1) as f32 / rings as f32) * std::f32::consts::FRAC_PI_2,
            );
            let base = (self.pos.len() / 3) as u32;
            for i in 0..=seg {
                let (p, n) = ring(t0, i);
                self.vert(p, n);
                let (p, n) = ring(t1, i);
                self.vert(p, n);
            }
            for i in 0..seg {
                let (a, b) = (base + (i as u32) * 2, base + (i as u32) * 2 + 2);
                self.tri(a, b, a + 1);
                self.tri(b, b + 1, a + 1);
            }
        }
    }
}

fn normal(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> [f32; 3] {
    let u = [b[0] - a[0], b[1] - a[1], b[2] - a[2]];
    let v = [c[0] - a[0], c[1] - a[1], c[2] - a[2]];
    norm([
        u[1] * v[2] - u[2] * v[1],
        u[2] * v[0] - u[0] * v[2],
        u[0] * v[1] - u[1] * v[0],
    ])
}

fn norm(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-6);
    [v[0] / l, v[1] / l, v[2] / l]
}

/// The sides a round thing gets. Sixteen is enough to read as round at the
/// distance a plant is looked at, and cheap enough that a design with four
/// hundred pipe segments is still one draw call.
const SEG: usize = 16;

/// The mesh, built. Cheap enough to call per request -- the whole library is
/// about fifteen thousand triangles -- and cached by nothing, because a cache
/// would be the first piece of state in a system whose entire argument is that
/// it has none.
pub fn geom(m: Mesh) -> Geom {
    let mut g = Geom::default();
    match m {
        Mesh::Box => g.cuboid([-0.5, 0.0, -0.5], [0.5, 1.0, 0.5]),
        Mesh::Cyl => g.barrel(0.5, 0.5, 0.0, 1.0, SEG, true),
        Mesh::Dome => g.dome(0.5, 1.0, 0.0, SEG, 5),
        Mesh::Cone => g.barrel(0.5, 0.16, 0.0, 1.0, SEG, true),
        Mesh::Elbow => g.bend(Mesh::ELBOW_R, 0.5, SEG, 6),
        Mesh::Tee => {
            g.barrel(0.5, 0.5, 0.0, 1.0, SEG, true);
            // The branch reaches clear of the run and stops, which is as far
            // out of its own box as anything in this library is allowed. It
            // used to stop at 0.44 -- *inside* a run of radius 0.5 -- so the
            // only part of a tee anybody ever saw was its collar, apparently
            // glued to the side of an ordinary pipe.
            let mut b = Geom::default();
            b.barrel(0.42, 0.42, 0.0, 0.6, SEG, true);
            b.disc(0.52, 0.6, SEG, true);
            across(&mut g, &b, [0.0, 0.5, 0.0]);
        }
        Mesh::Flange => {
            g.barrel(0.5, 0.5, 0.0, 1.0, SEG, true);
            g.barrel(0.34, 0.34, -0.05, 1.05, SEG, true);
        }
        Mesh::Nozzle => {
            g.barrel(0.34, 0.34, 0.0, 0.78, SEG, false);
            g.barrel(0.5, 0.5, 0.78, 1.0, SEG, true);
            g.disc(0.34, 0.0, SEG, false);
        }
        Mesh::Valve => {
            g.barrel(0.44, 0.44, 0.0, 1.0, SEG, true);
            g.barrel(0.56, 0.56, 0.3, 0.7, SEG, true);
            // The stem and the handwheel, across the run: a valve whose wheel
            // you cannot see is a pipe.
            let mut s = Geom::default();
            s.barrel(0.09, 0.09, 0.0, 0.48, 8, false);
            s.barrel(0.34, 0.34, 0.48, 0.56, 10, true);
            across(&mut g, &s, [0.0, 0.5, 0.0]);
        }
        Mesh::Band => g.barrel(0.5, 0.5, 0.0, 1.0, SEG, false),
        Mesh::Beam => {
            g.cuboid([-0.5, 0.0, -0.5], [0.5, 1.0, -0.36]);
            g.cuboid([-0.5, 0.0, 0.36], [0.5, 1.0, 0.5]);
            g.cuboid([-0.16, 0.0, -0.36], [0.16, 1.0, 0.36]);
        }
        Mesh::Grate => {
            g.cuboid([-0.5, 0.88, -0.5], [0.5, 1.0, 0.5]);
            for i in 0..5 {
                let z = -0.4 + i as f32 * 0.2;
                g.cuboid([-0.5, 0.7, z - 0.03], [0.5, 0.88, z + 0.03]);
            }
        }
        Mesh::Step => {
            g.cuboid([-0.5, 0.86, -0.5], [0.5, 1.0, 0.5]);
            g.cuboid([-0.5, 0.0, 0.4], [0.5, 0.86, 0.5]);
        }
        Mesh::Rail => {
            g.cuboid([-0.5, 0.0, 0.0], [0.5, 0.12, 1.0]);
            g.cuboid([-0.5, 0.0, 0.52], [0.5, 1.0, 0.62]);
            g.cuboid([-0.5, 0.0, 0.94], [0.5, 1.0, 1.0]);
        }
        // Post, cross-head and two cradle horns, and every one of them inside
        // the unit box. The horns used to reach to 1.24, which is fine for a
        // piece whose scale is local and ruinous for this one: a support is
        // scaled by its *whole height*, so a quarter of a four-metre post is a
        // metre of steel driven up through the pipe it is meant to be holding.
        Mesh::Support => {
            g.cuboid([-0.12, 0.0, -0.12], [0.12, 0.86, 0.12]);
            g.cuboid([-0.5, 0.86, -0.14], [0.5, 0.94, 0.14]);
            g.cuboid([-0.5, 0.94, -0.14], [-0.34, 1.0, 0.14]);
            g.cuboid([0.34, 0.94, -0.14], [0.5, 1.0, 0.14]);
        }
        Mesh::Anchor => {
            g.cuboid([-0.5, 0.0, -0.5], [0.5, 0.5, 0.5]);
            g.barrel(0.16, 0.16, 0.5, 1.0, 8, true);
        }
        Mesh::Coupling => {
            g.barrel(0.5, 0.5, 0.0, 0.42, SEG, true);
            g.barrel(0.5, 0.5, 0.58, 1.0, SEG, true);
            g.barrel(0.3, 0.3, 0.42, 0.58, SEG, false);
            for i in 0..6 {
                let a = (i as f32 / 6.0) * std::f32::consts::TAU;
                let (c, s) = (a.cos() * 0.38, a.sin() * 0.38);
                g.cuboid([c - 0.05, 0.0, s - 0.05], [c + 0.05, 1.0, s + 0.05]);
            }
        }
        Mesh::Bearing => {
            g.cuboid([-0.5, 0.0, -0.5], [0.5, 0.34, 0.5]);
            g.cuboid([-0.34, 0.34, -0.5], [0.34, 0.8, 0.5]);
            g.barrel(0.3, 0.3, -0.02, 1.02, SEG, false);
        }
        Mesh::Gauge => {
            g.barrel(0.1, 0.1, 0.0, 0.6, 8, false);
            g.barrel(0.5, 0.5, 0.6, 1.0, 12, true);
        }
        Mesh::Panel => {
            g.cuboid([-0.5, 0.0, -0.5], [0.5, 1.0, 0.5]);
            g.cuboid([-0.44, 0.06, 0.5], [0.44, 0.94, 0.56]);
        }
        Mesh::Fins => {
            g.barrel(0.42, 0.42, 0.0, 1.0, SEG, true);
            for i in 0..10 {
                let a = (i as f32 / 10.0) * std::f32::consts::TAU;
                let mut f = Geom::default();
                f.cuboid([-0.04, 0.04, 0.4], [0.04, 0.96, 0.5]);
                turned(&mut g, &f, a.cos(), a.sin());
            }
        }
        Mesh::Louvre => {
            g.cuboid([-0.5, 0.0, -0.06], [0.5, 1.0, 0.06]);
            for i in 0..6 {
                let y = 0.08 + i as f32 * 0.15;
                g.cuboid([-0.44, y, -0.14], [0.44, y + 0.07, 0.02]);
            }
        }
        Mesh::Stack => {
            g.barrel(0.5, 0.3, 0.0, 0.9, SEG, false);
            g.barrel(0.36, 0.36, 0.9, 1.0, SEG, true);
            g.disc(0.5, 0.0, SEG, false);
        }
        Mesh::Ladder => {
            g.cuboid([-0.3, 0.0, -0.04], [-0.22, 1.0, 0.04]);
            g.cuboid([0.22, 0.0, -0.04], [0.3, 1.0, 0.04]);
            for i in 0..4 {
                let y = 0.1 + i as f32 * 0.25;
                g.cuboid([-0.26, y, -0.03], [0.26, y + 0.05, 0.03]);
            }
        }
        Mesh::Rotor => {
            g.barrel(0.16, 0.16, 0.0, 1.0, 10, true);
            g.barrel(0.5, 0.5, 0.36, 0.64, SEG, true);
            for i in 0..12 {
                let a = (i as f32 / 12.0) * std::f32::consts::TAU;
                let mut f = Geom::default();
                f.cuboid([-0.06, 0.3, 0.46], [0.06, 0.7, 0.62]);
                turned(&mut g, &f, a.cos(), a.sin());
            }
        }
        // Wide in at the bottom, narrow out at the top, and a flange on each
        // end so that it is obviously a fitting rather than a pipe that got
        // thinner while nobody was looking.
        Mesh::Reducer => {
            g.barrel(0.5, 0.5, 0.0, 0.14, SEG, false);
            g.barrel(0.5, 0.3, 0.14, 0.8, SEG, false);
            g.barrel(0.3, 0.3, 0.8, 1.0, SEG, false);
            g.barrel(0.6, 0.6, 0.0, 0.08, SEG, true);
            g.barrel(0.4, 0.4, 0.92, 1.0, SEG, true);
            g.disc(0.5, 0.0, SEG, false);
            g.disc(0.3, 1.0, SEG, true);
        }
        // A strap, two ears and two bolts. Half a metre of this is the
        // difference between a pipe resting on a support and a pipe fixed to
        // one.
        Mesh::Clamp => {
            g.barrel(0.56, 0.56, 0.0, 1.0, SEG, false);
            for sgn in [-1.0f32, 1.0] {
                let (a, b) = (sgn * 0.44, sgn * 0.60);
                g.cuboid([a.min(b), 0.0, -0.09], [a.max(b), 1.0, 0.09]);
                let mut bolt = Geom::default();
                bolt.barrel(0.055, 0.055, -0.06, 1.06, 8, true);
                shifted(&mut g, &bolt, [sgn * 0.52, 0.0, 0.0]);
            }
        }
        // A shroud, a hub and six blades: the piece that says a box moves air
        // rather than holding it.
        Mesh::Cowl => {
            g.barrel(0.5, 0.5, 0.0, 1.0, SEG, false);
            g.barrel(0.44, 0.44, 0.0, 1.0, SEG, false);
            g.barrel(0.5, 0.44, 0.0, 0.1, SEG, false);
            g.barrel(0.5, 0.44, 1.0, 0.9, SEG, false);
            g.barrel(0.13, 0.13, 0.16, 0.84, 10, true);
            for i in 0..6 {
                let a = (i as f32 / 6.0) * std::f32::consts::TAU;
                let mut f = Geom::default();
                f.cuboid([-0.05, 0.36, 0.13], [0.05, 0.62, 0.44]);
                turned(&mut g, &f, a.cos(), a.sin());
            }
        }
        // A cradle for a horizontal vessel: base plate, solid web, and a strap
        // that curves up round the shell. The vessel's axis runs along local
        // `+Z`, which is why the callers spin it.
        Mesh::Saddle => {
            g.cuboid([-0.5, 0.0, -0.5], [0.5, 0.12, 0.5]);
            let n = 8;
            for i in 0..n {
                let (x0, x1) = (-0.5 + i as f32 / n as f32, -0.5 + (i + 1) as f32 / n as f32);
                // The mid-ordinate of the segment, on a parabola that sits the
                // shell down into the saddle by a third of its radius.
                let xm = (x0 + x1) / 2.0;
                let h = 0.62 + 0.38 * (xm / 0.5) * (xm / 0.5);
                g.cuboid([x0, 0.12, -0.34], [x1, (h - 0.14).max(0.12), 0.34]);
                g.cuboid([x0, (h - 0.14).max(0.12), -0.44], [x1, h, 0.44]);
            }
        }
    }
    g
}

/// Copy `src` into `dst`, moved. Bolts and hubs, which are the same shape in
/// several places on one mesh.
fn shifted(dst: &mut Geom, src: &Geom, at: [f32; 3]) {
    let base = (dst.pos.len() / 3) as u32;
    for i in 0..src.pos.len() / 3 {
        dst.vert(
            [src.pos[i * 3] + at[0], src.pos[i * 3 + 1] + at[1], src.pos[i * 3 + 2] + at[2]],
            [src.nrm[i * 3], src.nrm[i * 3 + 1], src.nrm[i * 3 + 2]],
        );
    }
    for k in &src.idx {
        dst.idx.push(base + k);
    }
}

/// Copy `src` into `dst`, laid across the run -- a quarter turn about `Z` --
/// and moved to `at`. The two meshes with a branch are the only users.
fn across(dst: &mut Geom, src: &Geom, at: [f32; 3]) {
    let base = (dst.pos.len() / 3) as u32;
    for i in 0..src.pos.len() / 3 {
        let (x, y, z) = (src.pos[i * 3], src.pos[i * 3 + 1], src.pos[i * 3 + 2]);
        let (nx, ny, nz) = (src.nrm[i * 3], src.nrm[i * 3 + 1], src.nrm[i * 3 + 2]);
        dst.vert([at[0] + y, at[1] - x, at[2] + z], [ny, -nx, nz]);
    }
    for k in &src.idx {
        dst.idx.push(base + k);
    }
}

/// Copy `src` into `dst`, turned about the `Y` axis by the given cosine and
/// sine. Blades and fins are one box, twelve times.
fn turned(dst: &mut Geom, src: &Geom, c: f32, s: f32) {
    let base = (dst.pos.len() / 3) as u32;
    for i in 0..src.pos.len() / 3 {
        let (x, y, z) = (src.pos[i * 3], src.pos[i * 3 + 1], src.pos[i * 3 + 2]);
        let (nx, ny, nz) = (src.nrm[i * 3], src.nrm[i * 3 + 1], src.nrm[i * 3 + 2]);
        dst.vert([x * c - z * s, y, x * s + z * c], [nx * c - nz * s, ny, nx * s + nz * c]);
    }
    for k in &src.idx {
        dst.idx.push(base + k);
    }
}
