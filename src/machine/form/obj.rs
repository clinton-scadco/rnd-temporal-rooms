//! The scene, baked: one `.obj` and one `.mtl`.
//!
//! Nothing in the experiment needs this. It exists because a claim about
//! geometry that can only be checked by looking at a WebGL canvas is a claim
//! nobody can check in a test, in a terminal, or in six months -- and because
//! `machine form designs/03-compact.machine --obj plant.obj` opening in any
//! modelling package on earth is a much stronger statement than a screenshot.
//!
//! It is also where the instancing claim gets tested from the other side. The
//! `.obj` is what the scene would cost if every piece really were an
//! independent object: a few hundred kilobytes of triangles standing in for a
//! few thousand instances of twenty-five meshes.

use super::kit::{self, Mat};
use super::{Piece, Scene};
use std::collections::BTreeSet;
use std::fmt::Write;

/// The `.obj`, and the `.mtl` that goes with it.
pub fn write(s: &Scene) -> (String, String) {
    let mut obj = String::with_capacity(1 << 16);
    let _ = writeln!(obj, "# {} -- experiment 08, procedural machine form", s.name);
    let _ = writeln!(obj, "# {} pieces, {} triangles, seed {:016x}", s.pieces.len(), s.tris(), s.seed.whole);
    let _ = writeln!(obj, "mtllib machine.mtl");

    let mut base = 1u32;
    let mut group = usize::MAX;
    let mut mat: Option<Mat> = None;
    // Grouped by owner, so a modelling package shows the plant as a tree of
    // components rather than one lump.
    let mut order: Vec<usize> = (0..s.pieces.len()).collect();
    order.sort_by_key(|&i| (s.pieces[i].of, s.pieces[i].mat as u8, s.pieces[i].mesh as u8));

    for i in order {
        let p = &s.pieces[i];
        if p.of as usize != group {
            group = p.of as usize;
            let o = s.owner(p.of);
            let _ = writeln!(obj, "g {}_{}", o.name.replace(' ', "_").replace("->", "to"), o.what);
        }
        if mat != Some(p.mat) {
            mat = Some(p.mat);
            let _ = writeln!(obj, "usemtl {}", p.mat.tag());
        }
        base += bake(&mut obj, p, base);
    }

    let mut mtl = String::new();
    let _ = writeln!(mtl, "# experiment 08, eight materials for a whole plant");
    for m in kit::MATS {
        let (c, rough, metal) = m.look();
        let c = if m == Mat::Paint { s.paint } else { c };
        let f = |v: u8| (v as f64) / 255.0;
        let _ = writeln!(mtl, "newmtl {}", m.tag());
        let _ = writeln!(mtl, "Kd {:.3} {:.3} {:.3}", f(c[0]), f(c[1]), f(c[2]));
        let _ = writeln!(mtl, "Ks {:.3} {:.3} {:.3}", if metal { 0.6 } else { 0.1 }, if metal { 0.6 } else { 0.1 }, if metal { 0.6 } else { 0.1 });
        let _ = writeln!(mtl, "Ns {:.1}", (100 - rough.min(100)) as f64 * 4.0);
        let _ = writeln!(mtl, "illum 2");
    }
    let _ = mtl;

    let used: BTreeSet<&str> = s.pieces.iter().map(|p| p.mesh.tag()).collect();
    let _ = writeln!(obj, "# meshes used: {}", used.into_iter().collect::<Vec<_>>().join(" "));
    (obj, mtl)
}

/// One piece, as triangles in world space. Returns how many vertices it wrote.
fn bake(obj: &mut String, p: &Piece, base: u32) -> u32 {
    let g = kit::geom(p.mesh);
    let (r, f, u) = axes(p);
    let (sx, sy, sz) = (p.size.x as f64 / 1000.0, p.size.y as f64 / 1000.0, p.size.z as f64 / 1000.0);
    let at = [p.at.x as f64 / 1000.0, p.at.y as f64 / 1000.0, p.at.z as f64 / 1000.0];

    for i in 0..g.verts() {
        let v = [g.pos[i * 3] as f64 * sx, g.pos[i * 3 + 1] as f64 * sy, g.pos[i * 3 + 2] as f64 * sz];
        let _ = writeln!(
            obj,
            "v {:.4} {:.4} {:.4}",
            at[0] + r[0] * v[0] + u[0] * v[1] + f[0] * v[2],
            at[1] + r[1] * v[0] + u[1] * v[1] + f[1] * v[2],
            at[2] + r[2] * v[0] + u[2] * v[1] + f[2] * v[2]
        );
    }
    for i in 0..g.verts() {
        // Non-uniform scale, so the normal is scaled by the reciprocal. The
        // `.obj` is for looking at, not for a renderer's inner loop.
        let n = [
            g.nrm[i * 3] as f64 / sx.max(1e-4),
            g.nrm[i * 3 + 1] as f64 / sy.max(1e-4),
            g.nrm[i * 3 + 2] as f64 / sz.max(1e-4),
        ];
        let w = [
            r[0] * n[0] + u[0] * n[1] + f[0] * n[2],
            r[1] * n[0] + u[1] * n[1] + f[1] * n[2],
            r[2] * n[0] + u[2] * n[1] + f[2] * n[2],
        ];
        let l = (w[0] * w[0] + w[1] * w[1] + w[2] * w[2]).sqrt().max(1e-6);
        let _ = writeln!(obj, "vn {:.4} {:.4} {:.4}", w[0] / l, w[1] / l, w[2] / l);
    }
    for t in g.idx.chunks(3) {
        let (a, b, c) = (base + t[0], base + t[1], base + t[2]);
        let _ = writeln!(obj, "f {a}//{a} {b}//{b} {c}//{c}");
    }
    g.verts() as u32
}

/// The piece's world axes: canonical `+X`, `+Z` and `+Y`, in that order of
/// return, as unit vectors. This is `frame_of` in floating point, and the two
/// have to agree -- which they do, because the integer one is this one
/// evaluated on axis-aligned inputs.
fn axes(p: &Piece) -> ([f64; 3], [f64; 3], [f64; 3]) {
    let d = [p.dir.x as f64, p.dir.y as f64, p.dir.z as f64];
    let l = (d[0] * d[0] + d[1] * d[1] + d[2] * d[2]).sqrt().max(1e-9);
    let up = [d[0] / l, d[1] / l, d[2] / l];
    let refv = if up[1].abs() > 0.99 { [0.0, 0.0, 1.0] } else { [0.0, 1.0, 0.0] };
    let right = unit(cross(up, refv));
    let fwd = unit(cross(right, up));
    match p.spin & 3 {
        0 => (right, fwd, up),
        1 => (fwd, neg(right), up),
        2 => (neg(right), neg(fwd), up),
        _ => (neg(fwd), right, up),
    }
}

fn cross(a: [f64; 3], b: [f64; 3]) -> [f64; 3] {
    [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]]
}

fn unit(v: [f64; 3]) -> [f64; 3] {
    let l = (v[0] * v[0] + v[1] * v[1] + v[2] * v[2]).sqrt().max(1e-9);
    [v[0] / l, v[1] / l, v[2] / l]
}

fn neg(v: [f64; 3]) -> [f64; 3] {
    [-v[0], -v[1], -v[2]]
}
