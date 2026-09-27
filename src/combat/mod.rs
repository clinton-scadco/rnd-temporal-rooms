//! Experiment 16: combat, as an active disturbance of a compressed world.
//!
//! Everything before this runs a factory as a handful of numbers that stand for
//! a great many machines, and a combat system is the obvious thing that could
//! break it. The naive version gives every attacker a position, a path and a
//! health bar, gives every shell an event, writes all of those events down, and
//! wakes the whole world up to find out what they hit. So the question is:
//!
//! > **Can combat coexist with the compressed deterministic simulation without
//! > producing an obscene serialized event stream or requiring the whole world
//! > to run explicitly?**
//!
//! One small defensive encounter, and nothing freeform:
//!
//! ```text
//!   Factory  <-  approach route  <-  enemy wave
//!                     |
//!               walls + turrets
//! ```
//!
//! # Compression, again, one level up
//!
//! v2's argument was that ten thousand smelters queued at one bay are in one
//! state, so they are one number. Ten thousand attackers walking the same leg
//! of the same route, having set off at the same tick, with the same health, are
//! also in one state. So an attacker is never a record here -- a **cohort** is:
//!
//! ```text
//!   cohort #7   brute x2,400   hp 12   marching leg 3 since t=1,380
//! ```
//!
//! and its position at any tick is a closed form of those four numbers. A shell
//! that lands on it does not touch 2,400 things; it moves *k* of them into a
//! different state, which is a split:
//!
//! ```text
//!   t=1,232  volley impacts   cohort #7 splits:  healthy x2,352, damaged x48
//! ```
//!
//! and two cohorts that end up in the same state -- the damaged half of one
//! packet and the damaged half of the next, both piled against the same wall --
//! are merged back into one. The number of cohorts is bounded by *how many
//! different things can be true of an attacker*, not by how many there are, and
//! that is the claim the scaling run measures.
//!
//! # Nothing derived is written down
//!
//! A turret is not ticked. It is an event at the moment it could next do
//! something: when a cohort's path first crosses its range, when it has finished
//! laying, when it has reloaded. A shell is four numbers -- origin, target, the
//! tick it fired, the tick it lands -- and a renderer interpolates it. A wall
//! under assault is an hp, a drain rate and the tick it will cross its next
//! damage band. None of those events is serialized. What is:
//!
//! ```text
//!   the scenario   one seed
//!   the commands   send a wave, repair a structure, hold a battery
//!   checkpoints    occasionally, the compact state -- never the events
//! ```
//!
//! # The disturbance boundary
//!
//! The factory is four sectors, each an ordinary plant in the language the
//! solver has always spoken, each run as a T5 population with a closed-form
//! orbit -- so the factory does not *run* at all while nobody is fighting in
//! it: any tick is one period of evaluation away. A wave opens the combat
//! domain, a rectangle around the route, the walls and the batteries, and the
//! sectors that stand inside it **wake**: they leave their orbit and are stepped
//! as populations on the fight's clock, because a structure that falls is an
//! edit and an edit happens at a tick. The sectors outside it never hear about
//! the fight.
//!
//! When the last cohort is dead or gone and nothing has changed for a few
//! seconds, the domain **closes**: every woken sector finds its orbit again
//! from wherever the fight left it and collapses back into a closed form. A
//! sector whose topology the fight changed -- a conveyor torn up, a smelter hall
//! down to half its furnaces -- was recompiled at the moment it changed, as a
//! Prototype 1 rendezvous, and only that sector was.
//!
//! ```text
//!   field    the encounter: a route, twelve structures, six batteries, four sectors
//!   fight    cohorts, volleys, walls under assault, and the events between them
//!   factory  a sector: an orbit while nobody is fighting in it, a population when woken
//!   run      the encounter: the log, the domain, checkpoints, and the frame a view reads
//!   net      `combat serve`
//! ```

pub mod factory;
pub mod field;
pub mod fight;
pub mod net;
pub mod run;

/// Sixty ticks a second, as everywhere a player can see the clock.
pub const TICK_RATE: u64 = crate::mp::SIM_TICK_RATE;

/// Milli-tiles per tile. Every position in the simulation is an integer number
/// of these, so where a cohort is at tick *t* is an integer division, the same
/// on every machine.
pub const MT: i64 = 1000;

/// A point on the field, in milli-tiles.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Hash, PartialOrd, Ord)]
pub struct P {
    pub x: i64,
    pub y: i64,
}

impl P {
    pub const fn tiles(x: i64, y: i64) -> P {
        P { x: x * MT, y: y * MT }
    }
}

/// Straight-line distance, in milli-tiles, rounded down.
///
/// Floating point is allowed exactly where IEEE 754 promises the same answer on
/// every machine -- `+ - * /` and `sqrt` are correctly rounded -- and nowhere
/// else. No `atan2`, no `sin`: those are library functions, and a library is
/// entitled to disagree with another library in the last bit.
pub fn dist(a: P, b: P) -> i64 {
    let dx = (a.x - b.x) as f64;
    let dy = (a.y - b.y) as f64;
    (dx * dx + dy * dy).sqrt() as i64
}

/// Binary angle units: 65,536 to a turn, 0 pointing east, increasing towards
/// +y. A turret's heading is one of these.
pub const TURN: i32 = 65_536;

/// The bearing from `a` to `b`, in binary angle units, without a library call.
///
/// Octant reduction and the usual rational approximation of `atan` on [0, 1],
/// which is good to about a fifth of a degree -- plenty for a turret, and
/// computed from nothing but multiplication and division, so it is the same
/// number on every replica.
pub fn bearing(a: P, b: P) -> i32 {
    let (dx, dy) = ((b.x - a.x) as f64, (b.y - a.y) as f64);
    if dx == 0.0 && dy == 0.0 {
        return 0;
    }
    let (ax, ay) = (dx.abs(), dy.abs());
    let (lo, hi) = if ax >= ay { (ay, ax) } else { (ax, ay) };
    let z = lo / hi;
    // atan(z) ~ pi/4 z + 0.273 z (1 - z) radians, in turns of 65,536.
    let first = (8192.0 * z + 2847.0 * z * (1.0 - z)) as i32;
    let oct = if ax >= ay { first } else { TURN / 4 - first };
    let a = match (dx >= 0.0, dy >= 0.0) {
        (true, true) => oct,
        (false, true) => TURN / 2 - oct,
        (false, false) => TURN / 2 + oct,
        (true, false) => TURN - oct,
    };
    a.rem_euclid(TURN)
}

/// The signed shortest turn from `from` to `to`.
pub fn turn_between(from: i32, to: i32) -> i32 {
    let d = (to - from).rem_euclid(TURN);
    if d > TURN / 2 {
        d - TURN
    } else {
        d
    }
}

/// SplitMix64: one `u64` of state, and the scenario's only source of chance.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    pub fn below(&mut self, n: u64) -> u64 {
        if n == 0 {
            0
        } else {
            self.next() % n
        }
    }
}

/// FNV-1a, for comparing two reconstructions without shipping either.
pub fn fnv(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for b in bytes {
        h ^= *b as u64;
        h = h.wrapping_mul(0x0100_0000_01b3);
    }
    h
}

/// `m:ss`, from ticks.
pub fn clock(t: u64) -> String {
    let s = t / TICK_RATE;
    format!("{}:{:02}", s / 60, s % 60)
}

/// `12,480`.
pub fn commas(n: u128) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, ch) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(ch);
    }
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bearings_land_in_the_right_octant() {
        let o = P::tiles(0, 0);
        assert_eq!(bearing(o, P::tiles(1, 0)), 0);
        assert_eq!(bearing(o, P::tiles(0, 1)), TURN / 4);
        assert_eq!(bearing(o, P::tiles(-1, 0)), TURN / 2);
        assert_eq!(bearing(o, P::tiles(0, -1)), 3 * TURN / 4);
        let diag = bearing(o, P::tiles(1, 1));
        assert!((diag - TURN / 8).abs() < 40, "{diag}");
        assert_eq!(turn_between(TURN - 100, 100), 200);
        assert_eq!(turn_between(100, TURN - 100), -200);
    }
}
