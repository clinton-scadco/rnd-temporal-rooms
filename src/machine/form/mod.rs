//! # Experiment 08: procedural machine form
//!
//! A machine design is a document: components on a tile grid, typed ports,
//! wires, tunings. Experiment 08 asks whether that document can be turned into
//! a *plausible industrial object* without anybody modelling one, and the whole
//! module tree is one sentence with arrows in it:
//!
//! ```text
//!   machine design
//!     -> semantic 3D layout     volumes, mounts, orientation, sockets
//!     -> connection routing     A* on a coarse grid, per domain
//!     -> structural inference   what has to hold all that up
//!     -> procedural dressing    what a works actually looks like
//!     -> renderable machine     instance buffers, batched, by LOD
//! ```
//!
//! ## The core rule
//!
//! > The generated mesh never defines the machine.
//!
//! Everything in `form` reads `Design` and writes `Scene`. Nothing here is read
//! by `sim`, `orbit`, `eval` or `snap`, and nothing here can be. That is not a
//! convention -- it is the module graph, and `tests/form.rs` checks the claim
//! from the other end by rebuilding every design under three different styles
//! and four different world seeds and asserting the verdict never moves.
//!
//! ```text
//!   RenderGeometry = Generate(MachineDesign, VisualSeed)
//! ```
//!
//! ## Millimetres, and why there is not a float in sight
//!
//! Every position, size and direction in a `Scene` is an `i32` in millimetres.
//! Floats appear exactly twice: inside `kit`, which builds canonical unit
//! meshes, and at the boundary where a `Scene` is written for a renderer.
//!
//! The reason is section 7. A scene that is going to be described over a
//! network as `design + seed` has to rebuild *identically* on the other end,
//! and "identically" is a much easier promise to keep in integers than in
//! accumulated floating-point transforms. It also makes the hash of a scene a
//! real hash: two builds agree bit for bit or they do not agree at all.
//!
//! One tile of the designer's grid is two metres.
//!
//! ## What a piece is
//!
//! ```text
//!   Piece {
//!       mesh   one of twenty-five
//!       mat    one of eight
//!       at     where the mesh's origin lands
//!       dir    where the mesh's +Y points
//!       spin   quarter turns about dir
//!       size   millimetres, per canonical axis
//!       lod    the furthest level it survives to
//!       of     which component, route or structure it belongs to
//!   }
//! ```
//!
//! Forty-eight bytes, and a plant is a few thousand of them. That is the
//! answer to section 10: a scene is not a tree of objects, it is a sorted list
//! that groups into a handful of instanced draw calls.

pub mod body;
pub mod frame;
pub mod kit;
pub mod layout;
pub mod obj;
pub mod route;
pub mod seed;
pub mod shell;
pub mod shot;

use super::design::Design;
use crate::json::Json;
use kit::{Mat, Mesh};
use seed::Seed;
use std::collections::BTreeMap;
use std::fmt;

// -------------------------------------------------------------- millimetres

pub type Mm = i32;

/// One tile of the designer's grid, in millimetres. The designer's footprint
/// score is in tiles, so this is the one number that connects "minimise the
/// plot" to "how big is the building".
pub const TILE: Mm = 2000;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, PartialOrd, Ord, Hash)]
pub struct P3 {
    pub x: Mm,
    pub y: Mm,
    pub z: Mm,
}

pub const fn p3(x: Mm, y: Mm, z: Mm) -> P3 {
    P3 { x, y, z }
}

pub const UP: P3 = p3(0, 1, 0);
pub const DOWN: P3 = p3(0, -1, 0);
pub const EAST: P3 = p3(1, 0, 0);
pub const WEST: P3 = p3(-1, 0, 0);
pub const SOUTH: P3 = p3(0, 0, 1);
pub const NORTH: P3 = p3(0, 0, -1);

/// The six directions anything orthogonal can point, in a fixed order, because
/// "iterate the neighbours" has to mean the same thing every time or the router
/// is not deterministic.
pub const SIX: [P3; 6] = [EAST, WEST, SOUTH, NORTH, UP, DOWN];

impl P3 {
    pub fn add(self, o: P3) -> P3 {
        p3(self.x + o.x, self.y + o.y, self.z + o.z)
    }
    pub fn sub(self, o: P3) -> P3 {
        p3(self.x - o.x, self.y - o.y, self.z - o.z)
    }
    pub fn mul(self, k: Mm) -> P3 {
        p3(self.x * k, self.y * k, self.z * k)
    }
    pub fn div(self, k: Mm) -> P3 {
        let k = if k == 0 { 1 } else { k };
        p3(self.x / k, self.y / k, self.z / k)
    }
    pub fn min(self, o: P3) -> P3 {
        p3(self.x.min(o.x), self.y.min(o.y), self.z.min(o.z))
    }
    pub fn max(self, o: P3) -> P3 {
        p3(self.x.max(o.x), self.y.max(o.y), self.z.max(o.z))
    }
    /// Manhattan, which is the only distance an orthogonal router cares about.
    pub fn taxi(self, o: P3) -> Mm {
        (self.x - o.x).abs() + (self.y - o.y).abs() + (self.z - o.z).abs()
    }
    /// Euclidean length, rounded to the nearest millimetre. `f64::sqrt` is
    /// correctly rounded by IEEE 754, so this is the same number everywhere.
    pub fn len(self) -> Mm {
        let (x, y, z) = (self.x as f64, self.y as f64, self.z as f64);
        (x * x + y * y + z * z).sqrt().round() as Mm
    }
    /// Which axis this points along, if it points along one.
    pub fn axis(self) -> Option<u8> {
        match (self.x != 0, self.y != 0, self.z != 0) {
            (true, false, false) => Some(0),
            (false, true, false) => Some(1),
            (false, false, true) => Some(2),
            _ => None,
        }
    }
    pub fn is_axis(self) -> bool {
        self.axis().is_some()
    }
    pub fn neg(self) -> P3 {
        p3(-self.x, -self.y, -self.z)
    }
}

impl fmt::Display for P3 {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "({}, {}, {})", self.x, self.y, self.z)
    }
}

/// An axis-aligned volume. Components have one, so do their clearances, so does
/// the whole installation.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub struct Vol {
    pub lo: P3,
    pub hi: P3,
}

impl Vol {
    pub fn new(a: P3, b: P3) -> Vol {
        Vol { lo: a.min(b), hi: a.max(b) }
    }
    pub fn centre(self) -> P3 {
        p3(
            (self.lo.x + self.hi.x) / 2,
            (self.lo.y + self.hi.y) / 2,
            (self.lo.z + self.hi.z) / 2,
        )
    }
    /// The middle of the footprint, on the ground.
    pub fn foot(self) -> P3 {
        p3((self.lo.x + self.hi.x) / 2, self.lo.y, (self.lo.z + self.hi.z) / 2)
    }
    pub fn size(self) -> P3 {
        self.hi.sub(self.lo)
    }
    pub fn grow(self, m: Mm) -> Vol {
        Vol { lo: p3(self.lo.x - m, self.lo.y - m, self.lo.z - m), hi: p3(self.hi.x + m, self.hi.y + m, self.hi.z + m) }
    }
    /// Grow sideways only. Clearance around a machine is about getting a person
    /// or a spanner past it, not about the sky.
    pub fn grow_flat(self, m: Mm) -> Vol {
        Vol { lo: p3(self.lo.x - m, self.lo.y, self.lo.z - m), hi: p3(self.hi.x + m, self.hi.y, self.hi.z + m) }
    }
    pub fn has(self, p: P3) -> bool {
        p.x >= self.lo.x && p.x <= self.hi.x && p.y >= self.lo.y && p.y <= self.hi.y && p.z >= self.lo.z && p.z <= self.hi.z
    }
    pub fn hits(self, o: Vol) -> bool {
        self.lo.x < o.hi.x && o.lo.x < self.hi.x && self.lo.y < o.hi.y && o.lo.y < self.hi.y && self.lo.z < o.hi.z && o.lo.z < self.hi.z
    }
    pub fn join(self, o: Vol) -> Vol {
        Vol { lo: self.lo.min(o.lo), hi: self.hi.max(o.hi) }
    }
    pub fn around(p: P3) -> Vol {
        Vol { lo: p, hi: p }
    }
}

// ------------------------------------------------------------------ a piece

/// How far away a piece is still worth drawing. Section 9: the *simulation*
/// representation is identical at every level, and only the pile of triangles
/// changes.
pub const CLOSE: u8 = 0;
pub const MEDIUM: u8 = 1;
pub const FAR: u8 = 2;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Piece {
    pub mesh: Mesh,
    pub mat: Mat,
    pub at: P3,
    /// Where canonical `+Y` points. Need not be a unit vector, and need not be
    /// axis-aligned -- a brace points where a brace points.
    pub dir: P3,
    pub spin: u8,
    pub size: P3,
    pub lod: u8,
    /// A little variation in colour and wear, from the seed.
    pub tint: u8,
    /// Index into `Scene::owners`.
    pub of: u16,
}

impl Piece {
    pub fn new(mesh: Mesh, mat: Mat, at: P3, dir: P3, size: P3) -> Piece {
        Piece { mesh, mat, at, dir, spin: 0, size, lod: MEDIUM, tint: 0, of: 0 }
    }
    /// Standing on its footprint, pointing up: most of a plant.
    pub fn up(mesh: Mesh, mat: Mat, at: P3, size: P3) -> Piece {
        Piece::new(mesh, mat, at, UP, size)
    }
    /// From one point to another, `thick` across: pipes, shafts, beams, braces.
    pub fn span(mesh: Mesh, mat: Mat, a: P3, b: P3, thick: Mm) -> Piece {
        let d = b.sub(a);
        Piece::new(mesh, mat, a, d, p3(thick, d.len(), thick))
    }
    /// A box between two corners. The canonical box stands on its footprint, so
    /// this is the form most structural pieces are written in.
    pub fn slab(mat: Mat, v: Vol) -> Piece {
        let s = v.size();
        Piece::up(Mesh::Box, mat, v.foot(), p3(s.x, s.y, s.z))
    }
    pub fn lod(mut self, l: u8) -> Piece {
        self.lod = l;
        self
    }
    pub fn spin(mut self, s: u8) -> Piece {
        self.spin = s & 3;
        self
    }
    pub fn tint(mut self, t: u8) -> Piece {
        self.tint = t;
        self
    }
    pub fn of(mut self, i: u16) -> Piece {
        self.of = i;
        self
    }

    /// The volume this piece occupies, roughly: the canonical mesh's unit box,
    /// scaled, pointed and placed. Used for bounds and for the enclosure pass,
    /// not for collision -- a bent elbow's true extent is its own business.
    pub fn vol(&self) -> Vol {
        let (r, f) = frame_of(self.dir);
        let u = unit(self.dir);
        let mut v = Vol::around(self.at);
        for &(a, b, c) in &[
            (-self.size.x / 2, 0, -self.size.z / 2),
            (self.size.x / 2, 0, self.size.z / 2),
            (-self.size.x / 2, self.size.y, -self.size.z / 2),
            (self.size.x / 2, self.size.y, self.size.z / 2),
        ] {
            let (rr, ff) = spun(r, f, self.spin);
            let p = self.at.add(mul_f(rr, a)).add(mul_f(u, b)).add(mul_f(ff, c));
            v = v.join(Vol::around(p));
        }
        v
    }
}

/// The direction as a thousandth-scaled unit vector, so that a piece's extent
/// can be worked out in integers.
fn unit(d: P3) -> P3 {
    let l = d.len().max(1);
    p3(d.x * 1000 / l, d.y * 1000 / l, d.z * 1000 / l)
}

fn mul_f(v: P3, k: Mm) -> P3 {
    p3(v.x * k / 1000, v.y * k / 1000, v.z * k / 1000)
}

/// The frame a piece is placed in: canonical `+X` and `+Z`, given where `+Y`
/// points. Exactly this construction appears in the renderer, in JavaScript,
/// and in the `.obj` writer -- all three have to agree, because the router
/// computes an elbow's exit point from it and the other two draw the elbow
/// with it.
///
/// Two properties are load bearing, and both were wrong the first time:
///
/// ```text
///   dir = up          the frame is the world's own axes, unturned
///   dir horizontal    canonical +Z is up, so a handrail stands up
/// ```
///
/// and `[right, up, fwd]` is a proper rotation rather than a reflection, which
/// is the difference between a plant and a plant with every cylinder cap
/// inside out.
pub fn frame_of(d: P3) -> (P3, P3) {
    let up = unit(d);
    // Any reference that is not parallel to the direction will do; which one is
    // picked has to be a fixed rule rather than a good idea.
    let r = if up.y.abs() > 990 { SOUTH.mul(1000) } else { UP.mul(1000) };
    let right = unit(cross(up, r));
    let fwd = unit(cross(right, up));
    (right, fwd)
}

fn cross(a: P3, b: P3) -> P3 {
    p3(
        (a.y * b.z - a.z * b.y) / 1000,
        (a.z * b.x - a.x * b.z) / 1000,
        (a.x * b.y - a.y * b.x) / 1000,
    )
}

/// The frame, turned by `spin` quarter turns about the direction.
fn spun(r: P3, f: P3, spin: u8) -> (P3, P3) {
    match spin & 3 {
        0 => (r, f),
        1 => (f, r.neg()),
        2 => (r.neg(), f.neg()),
        _ => (f.neg(), r),
    }
}

/// Where canonical `+X` ends up, in the world. The router needs this to point
/// an elbow's exit at the direction the route turns to.
pub fn right_of(dir: P3, spin: u8) -> P3 {
    let (r, f) = frame_of(dir);
    spun(r, f, spin).0
}

/// The spin that turns a piece's canonical `+X` onto `want`, if one does.
pub fn spin_for(dir: P3, want: P3) -> u8 {
    let w = unit(want);
    for s in 0..4u8 {
        let r = right_of(dir, s);
        if r.taxi(w) < 100 {
            return s;
        }
    }
    0
}

// ------------------------------------------------------------------- styles

/// The authored half of the look. The derived half -- whether this thing ends
/// up a skid, a shed or a hall -- is in `shell`, because that is a consequence
/// of the machine rather than a choice about it.
#[derive(Clone, Copy, PartialEq, Eq, Debug, Default)]
pub enum Style {
    /// Enclose it if it deserves enclosing.
    #[default]
    Works,
    /// Never enclose it: an outdoor plant on a slab.
    Yard,
    /// Always enclose it: everything happens inside a building.
    Hall,
}

pub const STYLES: [Style; 3] = [Style::Works, Style::Yard, Style::Hall];

impl Style {
    pub fn tag(self) -> &'static str {
        match self {
            Style::Works => "works",
            Style::Yard => "yard",
            Style::Hall => "hall",
        }
    }
    pub fn by_tag(t: &str) -> Option<Style> {
        STYLES.iter().copied().find(|s| s.tag() == t)
    }
}

impl fmt::Display for Style {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        f.write_str(self.tag())
    }
}

/// What the visual compiler was asked for. Three arguments and no state, which
/// is the same shape as everything else in this crate.
#[derive(Clone, Copy, Debug)]
pub struct Ask {
    pub style: Style,
    pub world: u64,
}

impl Default for Ask {
    fn default() -> Self {
        Ask { style: Style::Works, world: 0 }
    }
}

// -------------------------------------------------------------- who owns it

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Owns {
    /// A component of the design.
    Unit,
    /// A connection between two ports.
    Run,
    /// Something inferred to hold the above up.
    Frame,
    /// The enclosure.
    Shell,
}

impl Owns {
    pub fn tag(self) -> &'static str {
        match self {
            Owns::Unit => "unit",
            Owns::Run => "run",
            Owns::Frame => "frame",
            Owns::Shell => "shell",
        }
    }
}

#[derive(Clone, Debug)]
pub struct Owner {
    pub name: String,
    /// The component kind, the connection domain, or what the structure is.
    pub what: String,
    pub class: Owns,
}

// -------------------------------------------------------------- the machine

#[derive(Clone, Debug)]
pub struct Scene {
    pub name: String,
    pub style: Style,
    /// What the enclosure pass decided this installation is.
    pub shell: shell::Kind,
    pub seed: Seed,
    pub paint: [u8; 3],
    pub owners: Vec<Owner>,
    pub pieces: Vec<Piece>,
    pub bounds: Vol,
    /// Section 9's very far level: one box, and nothing else at all.
    pub proxy: Vol,
    pub routes: Vec<route::Run>,
}

impl Scene {
    /// The claim in section 7, as a number. Two builds of the same design with
    /// the same seed agree here or the experiment has failed.
    pub fn hash(&self) -> u64 {
        let mut h: u64 = 0xcbf2_9ce4_8422_2325;
        let mut eat = |v: i64| {
            for b in v.to_le_bytes() {
                h ^= b as u64;
                h = h.wrapping_mul(0x1000_0000_01b3);
            }
        };
        for p in &self.pieces {
            eat(p.mesh as i64);
            eat(p.mat as i64);
            eat(p.at.x as i64);
            eat(p.at.y as i64);
            eat(p.at.z as i64);
            eat(p.dir.x as i64);
            eat(p.dir.y as i64);
            eat(p.dir.z as i64);
            eat(p.size.x as i64);
            eat(p.size.y as i64);
            eat(p.size.z as i64);
            eat(p.spin as i64);
            eat(p.lod as i64);
            eat(p.tint as i64);
            eat(p.of as i64);
        }
        h
    }

    pub fn tris(&self) -> usize {
        let mut per: BTreeMap<Mesh, usize> = BTreeMap::new();
        self.pieces
            .iter()
            .map(|p| *per.entry(p.mesh).or_insert_with(|| kit::geom(p.mesh).tris()))
            .sum()
    }

    pub fn owner(&self, i: u16) -> &Owner {
        &self.owners[i as usize]
    }

    /// The pieces that belong to one named component, connection or structure.
    /// This is how the reactivity test asks "what moved?".
    pub fn pieces_of(&self, name: &str) -> Vec<&Piece> {
        let ids: Vec<u16> = self
            .owners
            .iter()
            .enumerate()
            .filter(|(_, o)| o.name == name)
            .map(|(i, _)| i as u16)
            .collect();
        self.pieces.iter().filter(|p| ids.contains(&p.of)).collect()
    }

    /// One draw call's worth: a mesh, a material, and every instance of it,
    /// sorted so that the first `keep[level]` of them are the ones that survive
    /// to that level.
    pub fn batches(&self) -> Vec<Batch> {
        let mut by: BTreeMap<(Mesh, Mat), Vec<Piece>> = BTreeMap::new();
        for p in &self.pieces {
            by.entry((p.mesh, p.mat)).or_default().push(*p);
        }
        by.into_iter()
            .map(|((mesh, mat), mut inst)| {
                // Descending by level of survival, so a distant view draws a
                // prefix of the same buffer rather than a different buffer.
                inst.sort_by(|a, b| b.lod.cmp(&a.lod).then(cmp_piece(a, b)));
                let keep = [
                    inst.len(),
                    inst.iter().filter(|p| p.lod >= MEDIUM).count(),
                    inst.iter().filter(|p| p.lod >= FAR).count(),
                ];
                Batch { mesh, mat, inst, keep }
            })
            .collect()
    }

    pub fn stats(&self) -> Stats {
        let batches = self.batches();
        Stats {
            units: self.owners.iter().filter(|o| o.class == Owns::Unit).count(),
            runs: self.routes.len(),
            pieces: self.pieces.len(),
            close: self.pieces.len(),
            medium: self.pieces.iter().filter(|p| p.lod >= MEDIUM).count(),
            far: self.pieces.iter().filter(|p| p.lod >= FAR).count(),
            batches: batches.len(),
            tris: self.tris(),
            meshes: batches.iter().map(|b| b.mesh).collect::<std::collections::BTreeSet<_>>().len(),
            bends: self.routes.iter().map(|r| r.bends).sum(),
            run_mm: self.routes.iter().map(|r| r.length as i64).sum(),
            supports: self.pieces.iter().filter(|p| p.mesh == Mesh::Support).count(),
            size: self.bounds.size(),
        }
    }
}

fn cmp_piece(a: &Piece, b: &Piece) -> std::cmp::Ordering {
    (a.of, a.at, a.size, a.dir, a.spin).cmp(&(b.of, b.at, b.size, b.dir, b.spin))
}

pub struct Batch {
    pub mesh: Mesh,
    pub mat: Mat,
    pub inst: Vec<Piece>,
    /// How many instances survive to close, medium and far.
    pub keep: [usize; 3],
}

#[derive(Clone, Copy, Debug, Default)]
pub struct Stats {
    pub units: usize,
    pub runs: usize,
    pub pieces: usize,
    pub close: usize,
    pub medium: usize,
    pub far: usize,
    pub batches: usize,
    pub tris: usize,
    pub meshes: usize,
    pub bends: usize,
    pub run_mm: i64,
    pub supports: usize,
    pub size: P3,
}

// ------------------------------------------------------------- the pipeline

/// A design, made into a plant.
///
/// Five passes, in the order of the diagram at the top of this file, and each
/// one only ever reads what the passes before it wrote. There is no second
/// visit, no relaxation step and no settling: the plant is not simulated into
/// existence, it is derived.
pub fn build(d: &Design, ask: Ask) -> Result<Scene, String> {
    if let Some(f) = d.check().first() {
        return Err(f.what.clone());
    }
    let seed = Seed::of(&d.name, layout::digest(d), ask.style.tag(), ask.world);
    let paint = kit::PAINTS[seed.all("paint").pick(kit::PAINTS.len())];

    let plan = layout::plan(d);
    let mut owners: Vec<Owner> = plan
        .units
        .iter()
        .map(|u| Owner { name: u.name.clone(), what: u.kind.tag().to_string(), class: Owns::Unit })
        .collect();

    let mut pieces = Vec::new();
    for (i, u) in plan.units.iter().enumerate() {
        body::dress(u, &plan, &seed, i as u16, &mut pieces);
    }

    let routes = route::run(d, &plan, &seed);
    for r in &routes {
        let id = owners.len() as u16;
        owners.push(Owner { name: r.name.clone(), what: r.dom.tag().to_string(), class: Owns::Run });
        route::dress(r, &seed, id, &mut pieces);
    }

    frame::infer(&plan, &routes, &seed, &mut owners, &mut pieces);
    let (kind, bounds) = shell::enclose(&plan, &routes, &seed, ask.style, &mut owners, &mut pieces);

    let mut all = bounds;
    for p in &pieces {
        all = all.join(p.vol());
    }
    // The proxy is the installation's silhouette, not its bounding box: a
    // stack that pokes eleven metres into the sky should not make the far-away
    // proxy eleven metres tall.
    let mut solid = Vol { lo: p3(0, 0, 0), hi: p3(0, 0, 0) };
    let mut first = true;
    for p in pieces.iter().filter(|p| p.lod >= FAR) {
        let v = p.vol();
        if first {
            solid = v;
            first = false;
        } else {
            solid = solid.join(v);
        }
    }

    Ok(Scene {
        name: d.name.clone(),
        style: ask.style,
        shell: kind,
        seed,
        paint,
        owners,
        pieces,
        bounds: all,
        proxy: if first { all } else { solid },
        routes,
    })
}

/// The library itself, as a scene: one of everything, on a grid, four metres
/// apart, in the order `MESHES` declares them.
///
/// This is a debugging tool that earned its keep in the first hour of having
/// it. A plant is thousands of pieces and a mesh that is subtly wrong -- a
/// dome with a seam, an elbow that turns the wrong way -- is invisible in the
/// pile and obvious here.
pub fn sheet() -> Scene {
    let across = 5;
    let mut pieces = Vec::new();
    let mut owners = Vec::new();
    for (i, &m) in kit::MESHES.iter().enumerate() {
        let (x, z) = ((i % across) as Mm * 4000, (i / across) as Mm * 4000);
        owners.push(Owner { name: m.tag().into(), what: "mesh".into(), class: Owns::Unit });
        pieces.push(
            Piece::up(m, kit::Mat::Steel, p3(x, 300, z), p3(2000, 2000, 2000))
                .of(i as u16)
                .lod(FAR),
        );
        pieces.push(
            Piece::up(kit::Mesh::Box, kit::Mat::Concrete, p3(x, 0, z), p3(3000, 300, 3000))
                .of(i as u16)
                .lod(FAR),
        );
    }
    let bounds = pieces.iter().fold(Vol::around(p3(0, 0, 0)), |a, p| a.join(p.vol()));
    Scene {
        name: "the library".into(),
        style: Style::Yard,
        shell: shell::Kind::Yard,
        seed: Seed::of("kit", 0, "yard", 0),
        paint: kit::PAINTS[0],
        owners,
        pieces,
        bounds,
        proxy: bounds,
        routes: Vec::new(),
    }
}

// -------------------------------------------------------------- to a client

/// Metres, for a renderer. The only place a float ever gets near a position.
fn m(v: Mm) -> f64 {
    (v as f64) / 1000.0
}

fn xyz(p: P3) -> Json {
    Json::arr(vec![m(p.x), m(p.y), m(p.z)])
}

impl Scene {
    /// The scene, in the shape the browser wants it: one flat instance array
    /// per (mesh, material) pair, and three counts saying how much of each one
    /// to draw at each level of detail.
    pub fn to_json(&self) -> Json {
        let batches: Vec<Json> = self
            .batches()
            .iter()
            .map(|b| {
                let mut inst: Vec<f64> = Vec::with_capacity(b.inst.len() * 12);
                for p in &b.inst {
                    inst.extend_from_slice(&[m(p.at.x), m(p.at.y), m(p.at.z)]);
                    inst.extend_from_slice(&[p.dir.x as f64, p.dir.y as f64, p.dir.z as f64]);
                    inst.extend_from_slice(&[m(p.size.x), m(p.size.y), m(p.size.z)]);
                    inst.push(p.spin as f64);
                    inst.push(p.tint as f64);
                    inst.push(p.of as f64);
                }
                Json::obj()
                    .set("mesh", b.mesh.tag())
                    .set("mat", b.mat.tag())
                    .set("n", b.inst.len() as i64)
                    .set("keep", Json::arr(b.keep.iter().map(|&k| k as i64).collect::<Vec<_>>()))
                    .set("inst", Json::arr(inst))
            })
            .collect();
        let s = self.stats();
        Json::obj()
            .set("name", self.name.clone())
            .set("style", self.style.tag())
            .set("shell", self.shell.tag())
            .set("seed", format!("{:016x}", self.seed.whole))
            .set("paint", Json::arr(self.paint.iter().map(|&c| c as i64).collect::<Vec<_>>()))
            .set("hash", format!("{:016x}", self.hash()))
            .set(
                "owners",
                Json::Arr(
                    self.owners
                        .iter()
                        .map(|o| {
                            Json::obj()
                                .set("name", o.name.clone())
                                .set("what", o.what.clone())
                                .set("class", o.class.tag())
                        })
                        .collect(),
                ),
            )
            .set("batches", Json::Arr(batches))
            .set(
                "bounds",
                Json::obj().set("lo", xyz(self.bounds.lo)).set("hi", xyz(self.bounds.hi)),
            )
            .set(
                "proxy",
                Json::obj().set("lo", xyz(self.proxy.lo)).set("hi", xyz(self.proxy.hi)),
            )
            .set(
                "stats",
                Json::obj()
                    .set("units", s.units as i64)
                    .set("runs", s.runs as i64)
                    .set("pieces", s.pieces as i64)
                    .set("close", s.close as i64)
                    .set("medium", s.medium as i64)
                    .set("far", s.far as i64)
                    .set("batches", s.batches as i64)
                    .set("meshes", s.meshes as i64)
                    .set("tris", s.tris as i64)
                    .set("bends", s.bends as i64)
                    .set("runMetres", (s.run_mm / 1000) as i64)
                    .set("supports", s.supports as i64),
            )
    }
}

/// The library itself, sent once: twenty-five meshes and eight materials. A
/// client asks for this on connect and never again -- everything after it is
/// instances.
pub fn kit_json() -> Json {
    let meshes: Vec<Json> = kit::MESHES
        .iter()
        .map(|&m| {
            let g = kit::geom(m);
            Json::obj()
                .set("tag", m.tag())
                .set("pos", Json::arr(g.pos.iter().map(|&v| v as f64).collect::<Vec<_>>()))
                .set("nrm", Json::arr(g.nrm.iter().map(|&v| v as f64).collect::<Vec<_>>()))
                .set("idx", Json::arr(g.idx.iter().map(|&v| v as i64).collect::<Vec<_>>()))
                .set("tris", g.tris() as i64)
        })
        .collect();
    let mats: Vec<Json> = kit::MATS
        .iter()
        .map(|&x| {
            let (c, rough, metal) = x.look();
            Json::obj()
                .set("tag", x.tag())
                .set("colour", Json::arr(c.iter().map(|&v| v as i64).collect::<Vec<_>>()))
                .set("rough", rough as i64)
                .set("metal", metal)
        })
        .collect();
    Json::obj()
        .set("ok", true)
        .set("meshes", Json::Arr(meshes))
        .set("mats", Json::Arr(mats))
        .set("styles", Json::arr(STYLES.iter().map(|s| s.tag()).collect::<Vec<_>>()))
}
