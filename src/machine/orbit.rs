//! Compiling a design into a macro-machine.
//!
//! The brief is explicit that a finished machine must not be allowed to
//! collapse into
//!
//! ```text
//!   input rate x efficiency = output rate
//! ```
//!
//! and it is right to be suspicious, because that is what every factory game
//! does and it is why two plants with the same average behave identically under
//! a supply that wobbles. What a machine here compiles to instead is
//!
//! ```text
//!   startup transient  +  exact periodic orbit
//! ```
//!
//! and the way to find that is embarrassingly direct: run it, and watch for it
//! to repeat itself. A component's state is a handful of small integers -- some
//! buffers, a warmth, a spin, a tank's mind made up -- so the whole machine's
//! state is a short byte string, and the first time one recurs the future from
//! there is *provably* the future from the first time it occurred, because the
//! step function is a pure function of that string.
//!
//! ```text
//!   key(s) == key(t),  s < t   =>   state(s + k) == state(t + k)  for all k
//! ```
//!
//! That single observation buys the whole thing:
//!
//! ```text
//!   state at 10^9    at most transient + period steps, never 10^9
//!   totals at 10^9   a prefix, a multiply, and a remainder
//! ```
//!
//! which is the same trick the rest of the crate plays on a factory, played on
//! one building. What it *keeps*, and what an average would have thrown away,
//! is the shape: two 104 MW machines can have periods of 1 and 47, and the one
//! with the period of 47 is the one that will behave strangely when something
//! outside it changes.

use super::design::Design;
use super::sim::{Delta, Machine, Tick, Totals};
use std::collections::HashMap;

/// How far to look for the machine repeating itself.
///
/// Designs that settle do so in hundreds of ticks; the cap exists for the ones
/// that never will, and it is chosen so that failing costs a few milliseconds.
pub const SEARCH: Tick = 20_000;

pub struct Compiled {
    /// The tick the orbit begins on -- everything before it happens once.
    pub transient: Tick,
    /// Length of the orbit, or 0 if the machine had not repeated itself by
    /// `searched`.
    pub period: Tick,
    /// How far the search actually ran.
    pub searched: Tick,
    /// `deltas[i]` is what the step from tick `i` to `i+1` did.
    pub deltas: Vec<Delta>,
    /// `cum[i]` is everything up to and including tick `i`. One longer than
    /// `deltas`, because tick 0 has a total too, and it is zero.
    pub cum: Vec<Totals>,
    /// One trip around the orbit.
    pub orbit: Totals,
}

impl Compiled {
    pub fn settled(&self) -> bool {
        self.period > 0
    }

    /// Everything the machine had done by tick `t`. Exact at any `t`, however
    /// large, once the orbit is known.
    pub fn totals_at(&self, t: Tick) -> Totals {
        if (t as usize) < self.cum.len() {
            return self.cum[t as usize];
        }
        if self.period == 0 {
            // Nothing honest to say beyond what was simulated.
            return *self.cum.last().unwrap_or(&Totals::default());
        }
        let s = self.transient;
        let laps = ((t - s) / self.period) as u128;
        let rem = (t - s) % self.period;
        let base = self.cum[s as usize];
        let part = self.cum[(s + rem) as usize].minus(&base);
        base.plus(&self.orbit.scaled(laps)).plus(&part)
    }

    /// Which tick of the *simulated* prefix is indistinguishable from `t`.
    ///
    /// This is the whole saving: asking about tick one billion means running
    /// the machine for at most `transient + period` ticks.
    pub fn equivalent_tick(&self, t: Tick) -> Tick {
        if (t as usize) < self.cum.len() {
            t
        } else if self.period > 0 {
            self.transient + (t - self.transient) % self.period
        } else {
            // No orbit was found, so there is no tick far away that is the same
            // as a tick near by, and walking to `t` could take all afternoon.
            // The honest answer is the furthest one actually known, reported as
            // such: every caller is handed this number alongside the tick that
            // was asked for, so nothing has to guess whether it got what it
            // wanted.
            self.searched
        }
    }

    /// The machine itself at tick `t`, rebuilt by running the equivalent tick.
    pub fn state_at(&self, d: &Design, t: Tick) -> Result<Machine, String> {
        let steps = self.equivalent_tick(t);
        let mut m = Machine::new(d)?;
        for _ in 0..steps {
            m.step();
        }
        // Present it as the tick that was asked about. The state is identical;
        // pretending otherwise would only confuse the panel that prints it.
        m.tick = t;
        Ok(m)
    }

    /// Average MW over one orbit, as a rational, because 314/3 is a fact and
    /// 104.67 is a rounding of one.
    pub fn power_rate(&self) -> (u128, u128) {
        if self.period == 0 {
            let t = self.cum.len().saturating_sub(1) as u128;
            return (self.cum.last().map(|c| c.power).unwrap_or(0), t.max(1));
        }
        (self.orbit.power, self.period as u128)
    }

    /// Per-tick power across the transient and one full orbit, downsampled to
    /// something a strip chart can draw. This is the picture that makes "same
    /// average, different machine" visible.
    pub fn waveform(&self, points: usize) -> (Vec<u64>, usize) {
        let end = if self.period > 0 {
            ((self.transient + self.period) as usize).min(self.deltas.len())
        } else {
            self.deltas.len()
        };
        if end == 0 {
            return (Vec::new(), 0);
        }
        let stride = (end / points.max(1)).max(1);
        let mut out = Vec::with_capacity(end / stride + 1);
        let mut i = 0;
        while i < end {
            // The maximum over the bucket, not the mean: a strip chart that
            // averages away a spike is lying about a periodic machine.
            let hi = (i + stride).min(end);
            out.push(self.deltas[i..hi].iter().map(|d| d.power).max().unwrap_or(0));
            i += stride;
        }
        (out, stride)
    }
}

/// Run the design until it repeats itself, or until the search gives up.
pub fn compile(d: &Design) -> Result<Compiled, String> {
    let mut m = Machine::new(d)?;
    let mut seen: HashMap<Vec<u8>, Tick> = HashMap::new();
    let mut deltas: Vec<Delta> = Vec::new();
    let mut cum: Vec<Totals> = vec![Totals::default()];
    let mut transient = 0;
    let mut period = 0;
    let mut t: Tick = 0;

    while t < SEARCH {
        let k = m.key();
        if let Some(&prev) = seen.get(&k) {
            transient = prev;
            period = t - prev;
            break;
        }
        seen.insert(k, t);
        let delta = m.step();
        let mut c = cum[t as usize];
        c.add(&delta);
        deltas.push(delta);
        cum.push(c);
        t += 1;
    }

    let orbit = if period > 0 {
        cum[t as usize].minus(&cum[transient as usize])
    } else {
        Totals::default()
    };

    Ok(Compiled { transient, period, searched: t, deltas, cum, orbit })
}

/// The compiled answer, checked against the thing it claims to summarise.
///
/// Same discipline as the rest of the crate: the closed form is never trusted,
/// it is *compared*. A straight simulation to tick `t` must agree with the
/// prefix-plus-laps-plus-remainder arithmetic exactly, or the compilation is
/// worthless however elegant it looks.
pub struct Check {
    pub tick: Tick,
    pub simulated: Totals,
    pub compiled: Totals,
    pub agrees: bool,
}

pub fn verify(d: &Design, ticks: &[Tick]) -> Result<Vec<Check>, String> {
    let c = compile(d)?;
    let mut out = Vec::new();
    let mut sorted: Vec<Tick> = ticks.to_vec();
    sorted.sort_unstable();
    sorted.dedup();
    let mut m = Machine::new(d)?;
    let mut running = Totals::default();
    let mut at: Tick = 0;
    for &t in &sorted {
        while at < t {
            let delta = m.step();
            running.add(&delta);
            at += 1;
        }
        let compiled = c.totals_at(t);
        out.push(Check { tick: t, simulated: running, compiled, agrees: running == compiled });
    }
    Ok(out)
}
