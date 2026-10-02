//! Small, tileable material textures. Generated once per library, shared by
//! the WebGL texture array and the software renderer. R modulates colour;
//! G modulates roughness. Neither channel changes the service colour palette.
use super::kit::Mat;

pub const SIZE: usize = 64;

fn smooth(a: f32, b: f32, v: f32) -> f32 {
    let t = ((v - a) / (b - a)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

/// Integer lattice noise; the WebGL shader uses the same hash and scale.
/// Piece-local metres keep weather attached when machinery is moved/turned.
fn weather_noise(p: [f32; 3]) -> f32 {
    let cell = p.map(|v| v.floor() as i32);
    let f = p.map(|v| { let t = v - v.floor(); t * t * (3.0 - 2.0 * t) });
    let mut value = 0.0;
    for z in 0..2 {
        for y in 0..2 {
            for x in 0..2 {
                let mut h = ((cell[0] + x) as u32).wrapping_mul(0x9e3779b9)
                    ^ ((cell[1] + y) as u32).wrapping_mul(0x85ebca6b)
                    ^ ((cell[2] + z) as u32).wrapping_mul(0xc2b2ae35);
                h ^= h >> 16;
                h = h.wrapping_mul(0x7feb352d);
                h ^= h >> 15;
                let weight = |k: usize, s: i32| if s == 0 { 1.0 - f[k] } else { f[k] };
                value += (h & 65535) as f32 / 65535.0 * weight(0, x) * weight(1, y) * weight(2, z);
            }
        }
    }
    value
}

/// Sparse chips expose steel at joints; broad enamel stays quiet. Subpixel
/// damage fades out rather than turning into larger blotches. Wear never touches
/// geometry or the service palette. Returns colour and added roughness.
pub fn finish(base: [f32; 3], mat: Mat, p: [f32; 3], n: [f32; 3], length: f32, tint: u8, footprint: f32) -> ([f32; 3], f32) {
    if matches!(mat, Mat::Concrete | Mat::Rubber | Mat::Lag) { return (base, 0.0); }
    let seed = tint as f32 * 7.13;
    let broad = weather_noise(p.map(|v| v * 2.2 + seed));
    let fine = weather_noise(p.map(|v| v * 95.0 + seed));
    let detail = 1.0 - smooth(0.004, 0.025, footprint);
    let joint_width = (length * 0.08).min(0.045).max(0.001);
    let joint = (1.0 - smooth(0.0, joint_width, p[1].min(length - p[1]))) * (1.0 - n[1].abs());
    let wear = 0.018 + tint.min(6) as f32 * 0.006;
    let damage = smooth(0.62 - wear, 0.82, broad);
    let chip = smooth(0.72 - joint * 0.14, 0.90 - joint * 0.10, fine)
        * damage * (0.04 + joint * 0.96) * detail;
    let oxide = damage * (joint * 0.075 + chip * 0.25);
    let streak = weather_noise([p[0] * 7.0 + seed, p[1] * 0.18, p[2] * 7.0]);
    let grime = smooth(0.64, 0.88, streak) * (0.018 + wear * 0.3 + joint * 0.025) * (1.0 - n[1].abs());
    let painted = matches!(mat, Mat::Paint | Mat::Cream | Mat::Water | Mat::Oil | Mat::Warn);
    let exposed = if painted { chip } else { 0.0 };
    let out = std::array::from_fn(|k| {
        let steel = [0.09, 0.105, 0.12][k];
        let rust = [0.19, 0.065, 0.023][k];
        let c = base[k] * (1.0 - exposed) + steel * exposed;
        (c * (1.0 - oxide) + rust * oxide) * (1.0 - grime)
    });
    (out, oxide + exposed * 0.3)
}

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
                    Mat::Wood => {
                        let rings = (x as f32 * 0.55 + broad * 3.0).sin();
                        (0.96 + rings * 0.09 + fine * 0.025, 0.08 + grain * 0.08)
                    },
                    Mat::CastIron => (0.94 + grain * 0.10 + fine * 0.05, 0.08 + fine * 0.1),
                    Mat::Concrete => (0.95 + broad * 0.10 + grain * 0.045 + fine * 0.025, 0.07 + grain * 0.1),
                    Mat::Galv => {
                        let crystal = zinc(u, v) - 0.5;
                        (0.98 + crystal * 0.10 + fine * 0.015, crystal * 0.12)
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
