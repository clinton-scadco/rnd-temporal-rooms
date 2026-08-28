//! # Prototype 2: the multiplayer vertical slice
//!
//! Every experiment above this one proved a compression trick and then put it
//! back in the box. This one stops proving things separately and asks the only
//! question that cannot be answered in a laboratory:
//!
//! > Can two players continuously build and redesign a deterministic factory
//! > together, in real time, while the simulation keeps running and both
//! > clients remain exactly synchronized?
//!
//! Nothing here is a new solver. The pieces were all built already, and the
//! work of this module is to make them one game:
//!
//! ```text
//!   graph + dsl + rooms      the world, compiled and run
//!   live                     an edit is a rendezvous, and a Carry crosses it
//!   machine::design + orbit  the inside of one building, as an exact orbit
//!   scenario                 pressure, posed about a plant rather than in it
//! ```
//!
//! ## The shape of the thing
//!
//! ```text
//!   player intention  ->  host validates, stamps (tick, sequence)  ->  broadcast
//!                                                                        |
//!            +-----------------------------------------------------------+
//!            v
//!   GAME DOCUMENT      physical installations, positions, owned designs
//!         | compile
//!         v
//!   SIMULATION IR      population classes, storages, channels, regions
//!         | live::with_states
//!         v
//!   state(tick)        the only thing anybody is allowed to draw
//! ```
//!
//! Every replica -- the host's, and one per joined player -- reconstructs that
//! chain from nothing but the command stream. They are separate objects with
//! separate carries in [`room`], and they are compared by hash on every poll.
//! That is not a diagnostic bolted on afterwards; it is the experiment.
//!
//! ## What is deliberately different from Prototype 1
//!
//! Prototype 1's log was a list of `live::Edit`s, which is a list of *document
//! diffs*. A multiplayer command cannot be a document diff, because two
//! players produce them concurrently against documents that are already
//! different. So a [`cmd::Cmd`] is an **intention** -- "put a yard here",
//! "commit this design" -- and the diff is *derived*, deterministically, by
//! compiling the world before and after and asking [`live::Edit`] what the
//! difference was. The solver below never learns that anything changed.
//!
//! ```text
//!   Cmd (intention)  ->  World (document)  ->  Graph  ->  diff  ->  live::Log
//! ```
//!
//! ## What time is
//!
//! One second is sixty ticks, everywhere, and gameplay numbers are written in
//! seconds and compiled here. A source that produces `30 Ore/second` is `30
//! Ore every 60 ticks` and never `0.5 Ore/tick`: fractional rates are spelled
//! as exact integer schedules, because a rate that accumulates in a float is a
//! desynchronisation with a delay fuse in it.

pub mod cmd;
pub mod goal;
pub mod net;
pub mod room;
pub mod kit;
pub mod lower;
pub mod world;

use crate::model::Tick;

/// Sixty ticks is one second. The only conversion in the game.
pub const SIM_TICK_RATE: u64 = 60;

/// Seconds, as ticks. Gameplay is authored in the left-hand unit and simulated
/// in the right-hand one.
pub const fn secs(s: u64) -> Tick {
    s * SIM_TICK_RATE
}

/// Ticks, as seconds, for a panel. One decimal, because the UI speaks seconds
/// and the debug views speak ticks.
pub fn as_secs(t: Tick) -> f64 {
    t as f64 / SIM_TICK_RATE as f64
}

/// How often the room takes a canonical accounting sample.
///
/// Goal progress has to be a function of the log and nothing else, so it is
/// only ever measured on this lattice. A replica that happened to be asked
/// about tick 12,345 and one that was asked about 12,400 must agree about what
/// has been delivered, and they do, because neither of them counts anything
/// except at a multiple of `CHECK`.
pub const CHECK: Tick = SIM_TICK_RATE;

/// How long a deleted thing leaves a ghost behind, with a Restore on it.
pub const GHOST_LIFE: Tick = secs(8);

/// One machine-designer tick is one game second.
///
/// Experiment 06 had its own clock and its own idea of a big number: a turbine
/// hall makes "104 MW" per tick and a stamping line "20 gears" per tick. Read
/// at sixty ticks a second those become 6,240 MW and 1,200 gears a second,
/// which is the exact mistake section 2 of the brief warns about -- resolution
/// mistaken for pace. So the designer's tick is re-read as a second, one
/// orbit becomes `period` seconds of world time, and a machine that made 20
/// gears a tick makes 20 gears a second.
pub const DESIGN_TICK: Tick = SIM_TICK_RATE;

/// The side of the world, in tiles. Big enough for a factory nobody will
/// finish, small enough that a placement can be refused for being outside it.
pub const PLOT: i32 = 128;

/// One world tile, in design tiles. A machine's footprint in the world is the
/// footprint of the design inside it, which is what makes the space goals a
/// question about engineering rather than about packing.
pub const TILE_IN_DESIGN_TILES: i32 = 4;

/// FNV-1a, 64 bit. Canonical hashes only have to agree with each other, and
/// this one is four lines and has no dependencies.
pub fn hash64(bytes: &[u8]) -> u64 {
    let mut h: u64 = 0xcbf2_9ce4_8422_2325;
    for &b in bytes {
        h ^= b as u64;
        h = h.wrapping_mul(0x100_0000_01b3);
    }
    h
}

/// The deterministic stream a room seed becomes.
///
/// splitmix64: no state beyond a `u64`, identical on every machine, and it
/// never sees a float. Goal generation is the only thing in the game that is
/// random, and it has to be the same random on both clients.
#[derive(Clone, Copy, Debug)]
pub struct Rng(pub u64);

impl Rng {
    pub fn next(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9e37_79b9_7f4a_7c15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xbf58_476d_1ce4_e5b9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94d0_49bb_1331_11eb);
        z ^ (z >> 31)
    }

    /// A number in `lo..=hi`, inclusive, without a float anywhere near it.
    pub fn between(&mut self, lo: u64, hi: u64) -> u64 {
        if hi <= lo {
            return lo;
        }
        lo + self.next() % (hi - lo + 1)
    }

    /// One of `n`, or 0 if there are none.
    pub fn pick(&mut self, n: usize) -> usize {
        if n == 0 {
            0
        } else {
            (self.next() % n as u64) as usize
        }
    }
}

/// A short room code: six characters, no vowels and no ambiguous glyphs, so it
/// can be read out loud and typed by somebody who is not looking.
pub fn room_code(seed: u64) -> String {
    const ALPHABET: &[u8] = b"BCDFGHJKLMNPQRSTVWXZ23456789";
    let mut r = Rng(seed);
    (0..6).map(|_| ALPHABET[r.pick(ALPHABET.len())] as char).collect()
}
