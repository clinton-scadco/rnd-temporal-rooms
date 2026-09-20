//! Small, tileable material textures. Generated once per library, shared by
//! the WebGL texture array and the software renderer. R modulates colour;
//! G modulates roughness. Neither channel changes the service colour palette.
use super::kit::Mat;

pub const SIZE: usize = 64;

pub struct Surface {
    /// Metres per repeat, independent of the dimensions of a mesh instance.
    pub metres: f32,
    pub levels: Vec<Vec<u8>>,
}

fn hash(x: i32, y: i32, seed: u32) -> f32 {
    let mut h = (x as u32).wrapping_mul(0x9e3779b9) ^ (y as u32).wrapping_mul(0x85ebca6b) ^ seed;
    h ^= h >> 16;
    h = h.wrapping_mul(0x7feb352d);
    h ^= h >> 15;
    (h & 65535) as f32 / 65535.0
}

/// Periodic value noise: neighbouring repeats meet without a seam.
fn noise(x: f32, y: f32, cells: i32, seed: u32) -> f32 {
    let (x, y) = (x * cells as f32, y * cells as f32);
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let smooth = |v: f32| v * v * (3.0 - 2.0 * v);
    let (u, v) = (smooth(x.fract()), smooth(y.fract()));
    let at = |dx: i32, dy: i32| hash((ix + dx).rem_euclid(cells), (iy + dy).rem_euclid(cells), seed);
    let a = at(0, 0) * (1.0 - u) + at(1, 0) * u;
    let b = at(0, 1) * (1.0 - u) + at(1, 1) * u;
    a * (1.0 - v) + b * v
}

/// Galvanised zinc crystals, using periodic Voronoi cells rather than speckles.
fn zinc(x: f32, y: f32) -> f32 {
    let (x, y) = (x * 9.0, y * 9.0);
    let (ix, iy) = (x.floor() as i32, y.floor() as i32);
    let (mut best, mut value) = (10.0, 0.5);
    for dy in -1..=1 {
        for dx in -1..=1 {
            let (cx, cy) = ((ix + dx).rem_euclid(9), (iy + dy).rem_euclid(9));
            let px = (ix + dx) as f32 + hash(cx, cy, 71);
            let py = (iy + dy) as f32 + hash(cx, cy, 93);
            let d = (x - px).powi(2) + (y - py).powi(2);
            if d < best {
                best = d;
                value = hash(cx, cy, 107);
            }
        }
    }
    value
}

impl Surface {
    pub fn new(mat: Mat) -> Self {
        let metres = match mat {
            Mat::Concrete => 1.2,
            Mat::Lag => 0.7,
            Mat::Galv => 0.45,
            Mat::Steel | Mat::Copper => 0.35,
            Mat::Rubber => 0.25,
            _ => 0.6,
        };
        let mut data = Vec::with_capacity(SIZE * SIZE * 2);
        for y in 0..SIZE {
            for x in 0..SIZE {
                let (u, v) = (x as f32 / SIZE as f32, y as f32 / SIZE as f32);
                let fine = hash(x as i32, y as i32, 17) - 0.5;
                let broad = noise(u, v, 5, 31) - 0.5;
                let grain = noise(u, v, 24, 59) - 0.5;
                let (tone, rough) = match mat {
                    Mat::Concrete => (0.91 + broad * 0.26 + grain * 0.13 + fine * 0.11, 0.07 + grain * 0.1),
                    Mat::Galv => {
                        let crystal = zinc(u, v) - 0.5;
                        (0.97 + crystal * 0.28 + fine * 0.03, crystal * 0.2)
                    }
                    Mat::Steel | Mat::Copper => {
                        let brush = hash(0, y as i32, 113) - 0.5;
                        (0.98 + brush * 0.14 + broad * 0.06 + fine * 0.025, brush * 0.18)
                    }
                    Mat::Lag => {
                        let weave = ((x % 4 < 2) as u8 as f32 + (y % 4 < 2) as u8 as f32 - 1.0) * 0.035;
                        (0.97 + broad * 0.1 + weave + fine * 0.025, 0.025 + grain * 0.04)
                    }
                    Mat::Rubber => (0.96 + grain * 0.1 + fine * 0.05, grain * 0.08),
                    Mat::Dark => (0.94 + broad * 0.14 + grain * 0.06 + fine * 0.04, 0.035 + grain * 0.12),
                    // Enamel: restrained orange peel, not uniformly rusty paint.
                    _ => (0.98 + broad * 0.07 + grain * 0.045 + fine * 0.025, grain * 0.12),
                };
                data.push(((tone - 0.7) / 0.6 * 255.0).clamp(0.0, 255.0).round() as u8);
                data.push(((rough / 0.4 + 0.5) * 255.0).clamp(0.0, 255.0).round() as u8);
            }
        }
        let mut levels = vec![data];
        let mut size = SIZE;
        while size > 1 {
            let prev = levels.last().unwrap();
            let next_size = size / 2;
            let mut next = vec![0; next_size * next_size * 2];
            for y in 0..next_size {
                for x in 0..next_size {
                    for c in 0..2 {
                        let mut sum = 0u32;
                        for dy in 0..2 {
                            for dx in 0..2 {
                                sum += prev[((y * 2 + dy) * size + x * 2 + dx) * 2 + c] as u32;
                            }
                        }
                        next[(y * next_size + x) * 2 + c] = ((sum + 2) / 4) as u8;
                    }
                }
            }
            levels.push(next);
            size = next_size;
        }
        Self { metres, levels }
    }

    fn texel(&self, uv: [f32; 2], level: usize) -> [f32; 2] {
        let size = (SIZE >> level) as i32;
        let (x, y) = (uv[0] * size as f32 - 0.5, uv[1] * size as f32 - 0.5);
        let (ix, iy) = (x.floor() as i32, y.floor() as i32);
        let (u, v) = (x - x.floor(), y - y.floor());
        let mut out = [0.0; 2];
        for (dy, wy) in [(0, 1.0 - v), (1, v)] {
            for (dx, wx) in [(0, 1.0 - u), (1, u)] {
                let i = (((iy + dy).rem_euclid(size) * size + (ix + dx).rem_euclid(size)) * 2) as usize;
                for c in 0..2 { out[c] += self.levels[level][i + c] as f32 / 255.0 * wx * wy; }
            }
        }
        out
    }

    /// Triplanar projection in piece-local metres: it follows turns and moves,
    /// without stretching when a canonical cylinder becomes a long pipe.
    /// The footprint chooses mip levels so fine grain cannot shimmer at range.
    pub fn sample(&self, local: [f32; 3], normal: [f32; 3], footprint: f32) -> [f32; 2] {
        let p = local.map(|v| v / self.metres);
        let weights = normal.map(|v| v.powi(4));
        let sum: f32 = weights.iter().sum();
        let lod = (footprint * SIZE as f32 / self.metres).max(1.0).log2().min(6.0);
        let level = lod.floor() as usize;
        let t = lod - lod.floor();
        let mut out = [0.0; 2];
        for (axis, uv) in [[p[1], p[2]], [p[2], p[0]], [p[0], p[1]]].iter().enumerate() {
            let a = self.texel(*uv, level);
            let b = self.texel(*uv, (level + 1).min(6));
            for c in 0..2 { out[c] += (a[c] * (1.0 - t) + b[c] * t) * weights[axis] / sum.max(1e-6); }
        }
        [0.7 + out[0] * 0.6, (out[1] - 0.5) * 0.4]
    }
}
