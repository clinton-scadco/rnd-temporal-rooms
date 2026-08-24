//! The plant, rendered, without a browser.
//!
//! A software rasteriser and a PNG writer, in about three hundred lines of
//! `std`, for the same reason `obj` exists: a claim about what a generated
//! plant *looks like* is worthless if the only way to check it is to open a
//! WebGL canvas and squint. `machine form FILE --png shot.png` puts the
//! answer in a file that a test, a terminal or a commit can carry.
//!
//! It is deliberately the same shading model as the browser's -- sun, sky, a
//! little haze -- so that the two pictures agree. It is not the same code, and
//! it never can be, which makes it a second opinion rather than a mirror.
//!
//! The PNG is written with stored (uncompressed) deflate blocks. A real
//! encoder would be another five hundred lines of Huffman tables to save a few
//! hundred kilobytes on a debugging screenshot, and this crate has a rule
//! about dependencies that it would rather keep.

use super::kit::{self, Mat};
use super::Scene;

pub struct Shot {
    pub w: usize,
    pub h: usize,
    px: Vec<[f32; 3]>,
    z: Vec<f32>,
}

/// Where the camera is, in the same terms the browser's orbit control uses.
#[derive(Clone, Copy, Debug)]
pub struct Eye {
    pub yaw: f32,
    pub pitch: f32,
    /// A multiple of the scene's own size, so a framing works for a skid and
    /// for a refinery.
    pub zoom: f32,
}

impl Default for Eye {
    fn default() -> Self {
        Eye { yaw: 0.72, pitch: 0.34, zoom: 1.0 }
    }
}

/// The scene, at whatever level of detail, from wherever.
pub fn render(s: &Scene, w: usize, h: usize, eye: Eye, lod: u8) -> Shot {
    let mut shot = Shot { w, h, px: vec![[0.80, 0.84, 0.88]; w * h], z: vec![f32::MAX; w * h] };
    let b = s.bounds;
    let at = [
        mid(b.lo.x, b.hi.x),
        mid(b.lo.y, b.hi.y) * 0.7,
        mid(b.lo.z, b.hi.z),
    ];
    let span = [
        (b.hi.x - b.lo.x) as f32 / 1000.0,
        (b.hi.y - b.lo.y) as f32 / 1000.0,
        (b.hi.z - b.lo.z) as f32 / 1000.0,
    ];
    let dist = (span[0].max(span[2]).max(span[1]) * 1.35 + 12.0) * eye.zoom;
    let cam = [
        at[0] + dist * eye.pitch.cos() * eye.yaw.sin(),
        at[1] + dist * eye.pitch.sin(),
        at[2] + dist * eye.pitch.cos() * eye.yaw.cos(),
    ];

    // A right-handed look-at, and a perspective divide. Nothing here is
    // clever; it only has to be right.
    let fwd = unit(sub(at, cam));
    let right = unit(cross(fwd, [0.0, 1.0, 0.0]));
    let up = cross(right, fwd);
    let f = 1.0 / (0.45f32).tan();
    let aspect = w as f32 / h as f32;

    let sun = unit([0.45, 0.82, 0.35]);
    for p in s.pieces.iter().filter(|p| p.lod >= lod) {
        let g = kit::geom(p.mesh);
        let (r, fw, u) = axes(p);
        let sz = [p.size.x as f32 / 1000.0, p.size.y as f32 / 1000.0, p.size.z as f32 / 1000.0];
        let org = [p.at.x as f32 / 1000.0, p.at.y as f32 / 1000.0, p.at.z as f32 / 1000.0];
        let (base, rough, metal) = look(s, p.mat, p.tint);

        let n = g.verts();
        let mut world: Vec<[f32; 3]> = Vec::with_capacity(n);
        let mut view: Vec<[f32; 3]> = Vec::with_capacity(n);
        // World-space normals, once per vertex: the shading is per pixel, the
        // same as the browser's, so that two pictures of one plant agree.
        let mut nrm: Vec<[f32; 3]> = Vec::with_capacity(n);
        for i in 0..n {
            let l = [g.pos[i * 3] * sz[0], g.pos[i * 3 + 1] * sz[1], g.pos[i * 3 + 2] * sz[2]];
            let p = [
                org[0] + r[0] * l[0] + u[0] * l[1] + fw[0] * l[2],
                org[1] + r[1] * l[0] + u[1] * l[1] + fw[1] * l[2],
                org[2] + r[2] * l[0] + u[2] * l[1] + fw[2] * l[2],
            ];
            let d = sub(p, cam);
            world.push(p);
            view.push([dot(d, right), dot(d, up), dot(d, fwd)]);
            let ln = [
                g.nrm[i * 3] / sz[0].max(1e-4),
                g.nrm[i * 3 + 1] / sz[1].max(1e-4),
                g.nrm[i * 3 + 2] / sz[2].max(1e-4),
            ];
            nrm.push(unit([
                r[0] * ln[0] + u[0] * ln[1] + fw[0] * ln[2],
                r[1] * ln[0] + u[1] * ln[1] + fw[1] * ln[2],
                r[2] * ln[0] + u[2] * ln[1] + fw[2] * ln[2],
            ]));
        }

        let look = Look { base, rough, metal, sun, cam };
        for t in g.idx.chunks(3) {
            let (a, b2, c) = (t[0] as usize, t[1] as usize, t[2] as usize);
            let (va, vb, vc) = (view[a], view[b2], view[c]);
            if va[2] < 0.2 || vb[2] < 0.2 || vc[2] < 0.2 {
                continue;
            }
            let p0 = project(va, f, aspect, w, h);
            let p1 = project(vb, f, aspect, w, h);
            let p2 = project(vc, f, aspect, w, h);
            // Backfaces by winding, in screen space, which is the one test
            // that does not care what the normals have been up to.
            if (p1[0] - p0[0]) * (p2[1] - p0[1]) - (p1[1] - p0[1]) * (p2[0] - p0[0]) >= 0.0 {
                continue;
            }
            shot.tri(
                [p0, p1, p2],
                [nrm[a], nrm[b2], nrm[c]],
                [world[a], world[b2], world[c]],
                &look,
            );
        }
    }
    shot.haze();
    shot
}

fn mid(a: i32, b: i32) -> f32 {
    ((a + b) as f32 / 2.0) / 1000.0
}

fn lerp(a: f32, b: f32, t: f32) -> f32 {
    a + (b - a) * t
}

/// Material colour, with the installation's paint and the piece's wear.
fn look(s: &Scene, m: Mat, tint: u8) -> ([f32; 3], f32, bool) {
    let (c, rough, metal) = m.look();
    let c = if m == Mat::Paint { s.paint } else { c };
    let k = 1.0 - (tint as f32) * 0.035;
    let g = |v: u8| ((v as f32 / 255.0).powf(2.2)) * k;
    ([g(c[0]), g(c[1]), g(c[2])], rough as f32 / 100.0, metal)
}

fn project(v: [f32; 3], f: f32, aspect: f32, w: usize, h: usize) -> [f32; 3] {
    let x = (v[0] * f / aspect) / v[2];
    let y = (v[1] * f) / v[2];
    [(x * 0.5 + 0.5) * w as f32, (0.5 - y * 0.5) * h as f32, v[2]]
}

/// Everything a pixel needs that is the same for the whole piece.
struct Look {
    base: [f32; 3],
    rough: f32,
    metal: bool,
    sun: [f32; 3],
    cam: [f32; 3],
}

impl Shot {
    /// One triangle: a depth test, an interpolated normal and a shaded pixel.
    /// Screen-space interpolation rather than perspective-correct, because the
    /// error is invisible at the size a machine is looked at and the loop is
    /// half as long.
    fn tri(&mut self, p: [[f32; 3]; 3], n: [[f32; 3]; 3], w: [[f32; 3]; 3], look: &Look) {
        let (a, b, c) = (p[0], p[1], p[2]);
        let minx = a[0].min(b[0]).min(c[0]).floor().max(0.0) as usize;
        let maxx = (a[0].max(b[0]).max(c[0]).ceil() as isize).clamp(0, self.w as isize) as usize;
        let miny = a[1].min(b[1]).min(c[1]).floor().max(0.0) as usize;
        let maxy = (a[1].max(b[1]).max(c[1]).ceil() as isize).clamp(0, self.h as isize) as usize;
        if minx >= maxx || miny >= maxy {
            return;
        }
        let area = edge(a, b, c);
        if area.abs() < 1e-6 {
            return;
        }
        for y in miny..maxy {
            for x in minx..maxx {
                let q = [x as f32 + 0.5, y as f32 + 0.5, 0.0];
                let (w0, w1, w2) = (edge(b, c, q) / area, edge(c, a, q) / area, edge(a, b, q) / area);
                if w0 < 0.0 || w1 < 0.0 || w2 < 0.0 {
                    continue;
                }
                let z = w0 * a[2] + w1 * b[2] + w2 * c[2];
                let i = y * self.w + x;
                if z >= self.z[i] {
                    continue;
                }
                let mix = |v: [[f32; 3]; 3], k: usize| w0 * v[0][k] + w1 * v[1][k] + w2 * v[2][k];
                let nn = unit([mix(n, 0), mix(n, 1), mix(n, 2)]);
                let pos = [mix(w, 0), mix(w, 1), mix(w, 2)];
                self.z[i] = z;
                self.px[i] = shade(nn, pos, look);
            }
        }
    }

    /// Distance haze, applied at the end because it only needs the depth.
    fn haze(&mut self) {
        for i in 0..self.px.len() {
            if self.z[i] == f32::MAX {
                continue;
            }
            let t = ((self.z[i] - 40.0) / 220.0).clamp(0.0, 0.55);
            for k in 0..3 {
                self.px[i][k] = lerp(self.px[i][k], [0.72, 0.76, 0.80][k], t);
            }
        }
    }

    /// sRGB bytes, three per pixel.
    pub fn rgb(&self) -> Vec<u8> {
        let mut out = Vec::with_capacity(self.px.len() * 3);
        for p in &self.px {
            for k in 0..3 {
                out.push((p[k].max(0.0).powf(1.0 / 2.2).min(1.0) * 255.0) as u8);
            }
        }
        out
    }

    pub fn png(&self) -> Vec<u8> {
        png(self.w, self.h, &self.rgb())
    }
}

/// Sun, sky and a little shine -- the same three lines as the browser's
/// fragment shader, which is what makes this a second opinion rather than a
/// different plant.
fn shade(n: [f32; 3], pos: [f32; 3], look: &Look) -> [f32; 3] {
    let lam = dot(n, look.sun).max(0.0);
    let sky = 0.5 + 0.5 * n[1];
    let amb = [lerp(0.20, 0.46, sky), lerp(0.21, 0.51, sky), lerp(0.24, 0.58, sky)];
    let view = unit(sub(look.cam, pos));
    let half = unit([view[0] + look.sun[0], view[1] + look.sun[1], view[2] + look.sun[2]]);
    let power = lerp(90.0, 8.0, look.rough);
    let spec = dot(n, half).max(0.0).powf(power) * (0.04 + if look.metal { 0.5 } else { 0.0 }) * lam;
    [
        look.base[0] * (amb[0] + lam * 0.85) + spec,
        look.base[1] * (amb[1] + lam * 0.85) + spec,
        look.base[2] * (amb[2] + lam * 0.85) + spec,
    ]
}

fn edge(a: [f32; 3], b: [f32; 3], c: [f32; 3]) -> f32 {
    (b[0] - a[0]) * (c[1] - a[1]) - (b[1] - a[1]) * (c[0] - a[0])
}

fn axes(p: &super::Piece) -> ([f32; 3], [f32; 3], [f32; 3]) {
    let d = [p.dir.x as f32, p.dir.y as f32, p.dir.z as f32];
    let up = unit(d);
    let refv = if up[1].abs() > 0.99 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
    let right = unit(cross(up, refv));
    let fwd = cross(right, up);
    match p.spin & 3 {
        0 => (right, fwd, up),
        1 => (fwd, neg(right), up),
        2 => (neg(right), neg(fwd), up),
        _ => (neg(fwd), right, up),
    }
}

fn sub(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[0] - b[0], a[1] - b[1], a[2] - b[2]]
}
fn dot(a: [f32; 3], b: [f32; 3]) -> f32 {
    a[0] * b[0] + a[1] * b[1] + a[2] * b[2]
}
fn cross(a: [f32; 3], b: [f32; 3]) -> [f32; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}
fn unit(v: [f32; 3]) -> [f32; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}
fn neg(v: [f32; 3]) -> [f32; 3] {
    [-v[0], -v[1], -v[2]]
}

// ------------------------------------------------------------------- a PNG

/// An 8-bit RGB PNG with a stored-block zlib stream.
pub fn png(w: usize, h: usize, rgb: &[u8]) -> Vec<u8> {
    let mut raw = Vec::with_capacity(h * (1 + w * 3));
    for y in 0..h {
        raw.push(0); // filter: none
        raw.extend_from_slice(&rgb[y * w * 3..(y + 1) * w * 3]);
    }

    let mut z = vec![0x78, 0x01];
    let mut i = 0;
    while i < raw.len() {
        let n = (raw.len() - i).min(65535);
        let last = if i + n >= raw.len() { 1 } else { 0 };
        z.push(last);
        z.extend_from_slice(&(n as u16).to_le_bytes());
        z.extend_from_slice(&(!(n as u16)).to_le_bytes());
        z.extend_from_slice(&raw[i..i + n]);
        i += n;
    }
    z.extend_from_slice(&adler32(&raw).to_be_bytes());

    let mut out = vec![0x89, b'P', b'N', b'G', 0x0d, 0x0a, 0x1a, 0x0a];
    let mut ihdr = Vec::new();
    ihdr.extend_from_slice(&(w as u32).to_be_bytes());
    ihdr.extend_from_slice(&(h as u32).to_be_bytes());
    ihdr.extend_from_slice(&[8, 2, 0, 0, 0]);
    chunk(&mut out, b"IHDR", &ihdr);
    chunk(&mut out, b"IDAT", &z);
    chunk(&mut out, b"IEND", &[]);
    out
}

fn chunk(out: &mut Vec<u8>, tag: &[u8; 4], body: &[u8]) {
    out.extend_from_slice(&(body.len() as u32).to_be_bytes());
    out.extend_from_slice(tag);
    out.extend_from_slice(body);
    let mut crc = Vec::with_capacity(4 + body.len());
    crc.extend_from_slice(tag);
    crc.extend_from_slice(body);
    out.extend_from_slice(&crc32(&crc).to_be_bytes());
}

fn crc32(data: &[u8]) -> u32 {
    let mut c: u32 = 0xffff_ffff;
    for &b in data {
        c ^= b as u32;
        for _ in 0..8 {
            c = if c & 1 != 0 { 0xedb8_8320 ^ (c >> 1) } else { c >> 1 };
        }
    }
    !c
}

fn adler32(data: &[u8]) -> u32 {
    let (mut a, mut b) = (1u32, 0u32);
    for &x in data {
        a = (a + x as u32) % 65521;
        b = (b + a) % 65521;
    }
    (b << 16) | a
}
