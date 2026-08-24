//! Where every cosmetic choice comes from, and why it is not a random number.
//!
//! Section 7 of the note is the load-bearing one for anything that ever gets
//! networked or saved:
//!
//! ```text
//!   network:  design + positions + seed
//!   not:      38 MB of generated mesh
//! ```
//!
//! which only works if two machines that were sent the same three things build
//! the same plant. So nothing in `form` calls a clock, an address or an
//! iteration order it does not control. Every choice comes from here.
//!
//! # Two seeds, not one
//!
//! The note suggests
//!
//! ```text
//!   VisualSeed = hash(designId, component layout, styleId, worldSeed)
//! ```
//!
//! and that is what [`Seed::of`] computes -- but it is deliberately *not* what
//! a gauge on a turbine is drawn from. Fold the layout into every cosmetic
//! stream and moving one generator reshuffles the dressing on all forty
//! components, which is a catastrophic result for the property the primary
//! experiment is actually testing:
//!
//! > The important property is **reactivity**, not photorealism.
//!
//! Reactivity means the geometry near the change moves and the rest does not.
//! So the layout digest decides the things that genuinely belong to the whole
//! installation -- its paint, its enclosure style -- while each component draws
//! from a stream named after *itself*:
//!
//! ```text
//!   whole installation   hash(designId, layout, styleId, worldSeed)
//!   one component        hash(designId, styleId, worldSeed, name, purpose)
//! ```
//!
//! Move a generator and its shaft reroutes, its plinth follows it, and the
//! reactor thirty metres away is untouched down to the last handwheel. That is
//! a test in `tests/form.rs` rather than a hope.

/// FNV-1a, 64-bit. Chosen because it is four lines, has no state to configure,
/// and gives the same answer on every machine that will ever run this -- which
/// is the entire specification.
pub fn hash(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x1000_0000_01b3);
    }
    h
}

pub fn mix(a: u64, b: u64) -> u64 {
    let mut h = a ^ b.wrapping_mul(0x9e37_79b9_7f4a_7c15);
    h ^= h >> 29;
    h = h.wrapping_mul(0xbf58_476d_1ce4_e5b9);
    h ^= h >> 32;
    h
}

/// The installation's visual seed, and the streams that come off it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Seed {
    /// `hash(designId, layout, styleId, worldSeed)` -- the whole installation.
    pub whole: u64,
    /// `hash(designId, styleId, worldSeed)` -- everything a component's own
    /// stream is named against. Survives a neighbour moving.
    pub local: u64,
}

impl Seed {
    pub fn of(design_id: &str, layout: u64, style_id: &str, world: u64) -> Seed {
        let local = mix(mix(hash(design_id.as_bytes()), hash(style_id.as_bytes())), world);
        Seed { whole: mix(local, layout), local }
    }

    /// A stream for one named thing and one purpose. Two purposes on the same
    /// component never share a sequence, so adding a coat of paint cannot move
    /// a handwheel.
    pub fn at(&self, who: &str, purpose: &str) -> Rng {
        let h = mix(mix(self.local, hash(who.as_bytes())), hash(purpose.as_bytes()));
        Rng::new(h)
    }

    /// A stream for the installation itself.
    pub fn all(&self, purpose: &str) -> Rng {
        Rng::new(mix(self.whole, hash(purpose.as_bytes())))
    }
}

/// SplitMix64. Small, well-distributed, and -- the only property that matters
/// here -- the same sequence everywhere.
#[derive(Clone, Debug)]
pub struct Rng(u64);

impl Rng {
    pub fn new(s: u64) -> Rng {
        Rng(s | 1)
    }

    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// One of `n`.
    pub fn pick(&mut self, n: usize) -> usize {
        if n == 0 {
            return 0;
        }
        (self.next() % n as u64) as usize
    }

    /// Inclusive, in whatever unit the caller is counting in -- usually
    /// millimetres, because the whole pipeline is integers.
    pub fn range(&mut self, lo: i32, hi: i32) -> i32 {
        if hi <= lo {
            return lo;
        }
        lo + (self.next() % ((hi - lo + 1) as u64)) as i32
    }

    pub fn chance(&mut self, percent: u32) -> bool {
        (self.next() % 100) < percent as u64
    }
}
