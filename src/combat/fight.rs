//! The fight: cohorts, volleys, structures under assault, and the events
//! between them.
//!
//! There is no tick loop here, the same as in `sim.rs`. The next thing that
//! happens is found by asking every piece of state when it next changes, and
//! the clock jumps there:
//!
//! ```text
//!   a volley           lands at its impact tick
//!   a structure        crosses its next damage band at hp / drain
//!   a marching cohort  reaches the end of its leg
//!   a packet           is released
//!   a battery          finishes laying, finishes reloading, or sees
//!                      something come into range
//! ```
//!
//! Every one of those times is computed from state, so the *queue* is never
//! state. A checkpoint does not contain an event, and a fight restored from one
//! finds exactly the same next event the original was about to process,
//! because it is asking the same numbers the same question.
//!
//! Finding the minimum is a scan, not a heap: the scan is over cohorts,
//! batteries, volleys in flight and structures, which is to say over the
//! *compressed* state, and the whole point is that that is small. A wave of a
//! million attackers costs the same scan as a wave of a hundred.

use super::field::{self, BatteryDef, SKind, BATTERIES, KINDS, ROUTE, STRUCTURES};
use super::{bearing, dist, turn_between, Rng, P, TICK_RATE};
use crate::json::Json;
use crate::model::Tick;

// ==================================================================== state

/// Where a cohort is, as a closed form of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Motion {
    /// Walking from `ROUTE[leg]` to `ROUTE[leg + 1]`, having set off at `since`.
    March { leg: u16, since: Tick },
    /// Stopped at a node, chewing on whatever stands on it.
    Assault { node: u16 },
}

/// Every attacker in one state, as one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cohort {
    pub id: u32,
    pub kind: u8,
    pub count: u64,
    pub hp: u16,
    pub motion: Motion,
}

/// A battery's one piece of changing state.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duty {
    /// Nothing to shoot at. `wake` is the first tick anything could be:
    /// computed, not polled, and `Tick::MAX` when nothing is coming.
    Idle { wake: Tick },
    /// Turning from `from` to `to` between `since` and `fire`, and then firing
    /// at `at`, where the shell will land at `lands`.
    Laying { from: i32, to: i32, since: Tick, fire: Tick, at: P, lands: Tick },
    Reload { until: Tick },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Battery {
    pub heading: i32,
    pub duty: Duty,
    pub last_fire: Option<Tick>,
    pub volleys: u64,
    pub hold: bool,
}

/// A volley in flight: four numbers and a count. The renderer interpolates it;
/// nothing else ever looks at it until it lands.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Volley {
    pub id: u32,
    pub battery: u16,
    pub shells: u32,
    pub from: P,
    pub to: P,
    pub fired: Tick,
    pub lands: Tick,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord)]
pub enum Band {
    Intact,
    Damaged,
    Critical,
    Destroyed,
}

impl Band {
    pub fn word(self) -> &'static str {
        match self {
            Band::Intact => "intact",
            Band::Damaged => "damaged",
            Band::Critical => "critical",
            Band::Destroyed => "destroyed",
        }
    }
    fn parse(s: &str) -> Band {
        match s {
            "damaged" => Band::Damaged,
            "critical" => Band::Critical,
            "destroyed" => Band::Destroyed,
            _ => Band::Intact,
        }
    }
    /// The band a structure is in, from its milli-hp. Behaviour changes only at
    /// these thresholds, the way experiment 14's temperatures do.
    pub fn of(hp: i64, max: i64) -> Band {
        if hp <= 0 {
            Band::Destroyed
        } else if hp * 100 <= max * 40 {
            Band::Critical
        } else if hp * 100 <= max * 75 {
            Band::Damaged
        } else {
            Band::Intact
        }
    }
    /// The milli-hp at or below which the next band starts.
    fn floor(self, max: i64) -> i64 {
        match self {
            Band::Intact => max * 75 / 100,
            Band::Damaged => max * 40 / 100,
            _ => 0,
        }
    }
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Structure {
    /// Milli-hp, as of `since`. The drain since then is derived from who is
    /// standing at its node, so it is never stored.
    pub hp: i64,
    pub since: Tick,
    pub band: Band,
}

/// Part of a wave that has not been released yet.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Packet {
    pub at: Tick,
    pub kind: u8,
    pub count: u64,
}

/// Something the factory has to hear about: a structure crossed a band.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Effect {
    pub at: Tick,
    pub structure: usize,
    pub band: Band,
}

/// Something only a renderer cares about. Never checkpointed, never hashed:
/// a flash that a restored fight forgot to draw is not a different fight.
#[derive(Clone, Debug)]
pub struct Fx {
    pub at: Tick,
    pub what: &'static str,
    pub pos: P,
    pub radius: i64,
    pub n: u64,
}

/// Counters about the run rather than the fight. Not state either.
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub events: u64,
    pub impacts: u64,
    pub bands: u64,
    pub arrivals: u64,
    pub spawns: u64,
    pub battery: u64,
    pub volleys: u64,
    pub hits: u64,
    pub splits: u64,
    pub merges: u64,
    /// Cohort records ever created: spawns plus splits. The thing a naive
    /// engine would have created one of per attacker.
    pub created: u64,
    /// Records looked at to find each next event, summed: cohorts, volleys,
    /// structures and batteries. The actual work, in the units it is done in.
    pub visits: u64,
    pub peak_cohorts: usize,
    pub peak_volleys: usize,
    pub peak_alive: u64,
}

#[derive(Clone, Debug)]
pub struct Fight {
    pub now: Tick,
    /// Sorted by id.
    pub cohorts: Vec<Cohort>,
    pub batteries: Vec<Battery>,
    pub structures: Vec<Structure>,
    pub volleys: Vec<Volley>,
    /// Sorted by release tick.
    pub pending: Vec<Packet>,
    pub next_id: u32,
    pub spawned: u64,
    pub killed: u64,
    pub leaked: u64,

    pub stats: Stats,
    pub fx: Vec<Fx>,
    pub effects: Vec<Effect>,
    /// The first few hundred events, in words, when somebody asks.
    pub trace: Option<Vec<String>>,
}

/// Event classes, in the order two at one tick are handled.
const EV_IMPACT: u8 = 0;
const EV_BAND: u8 = 1;
const EV_ARRIVE: u8 = 2;
const EV_SPAWN: u8 = 3;
const EV_BATTERY: u8 = 4;

/// The shortest a battery takes to lay, however little it has to turn.
const LAY_MIN: Tick = 6;
/// How long a flash stays in the render list.
const FX_KEEP: Tick = 3 * TICK_RATE;
const TRACE_CAP: usize = 400;

/// How wide a cohort stands, in milli-tiles, for being hit and for being drawn.
pub fn spread(count: u64) -> i64 {
    (500.0 + 28.0 * (count as f64).sqrt()).min(4_800.0) as i64
}

impl Default for Fight {
    fn default() -> Fight {
        Fight::new()
    }
}

impl Fight {
    pub fn new() -> Fight {
        Fight {
            now: 0,
            cohorts: Vec::new(),
            batteries: BATTERIES
                .iter()
                .map(|b| Battery {
                    heading: b.rest,
                    duty: Duty::Idle { wake: Tick::MAX },
                    last_fire: None,
                    volleys: 0,
                    hold: false,
                })
                .collect(),
            structures: STRUCTURES
                .iter()
                .map(|s| Structure { hp: s.hp * 1000, since: 0, band: Band::Intact })
                .collect(),
            volleys: Vec::new(),
            pending: Vec::new(),
            next_id: 1,
            spawned: 0,
            killed: 0,
            leaked: 0,
            stats: Stats::default(),
            fx: Vec::new(),
            effects: Vec::new(),
            trace: None,
        }
    }

    fn note(&mut self, t: Tick, s: impl FnOnce() -> String) {
        if let Some(tr) = &mut self.trace {
            if tr.len() < TRACE_CAP {
                tr.push(format!("t={:<6} {}", t, s()));
            }
        }
    }

    // ------------------------------------------------------------ commands

    /// A wave: `nominal` attackers, released as `PACKETS` packets from `start`.
    ///
    /// The seed decides the mix and the spacing, and nothing else is random
    /// anywhere in the fight. The nominal size decides nothing but the counts:
    /// a wave of a hundred and a wave of a million are the same twelve packets
    /// at the same twelve ticks.
    pub fn wave(&mut self, seed: u64, nominal: u64, start: Tick) {
        let mut rng = Rng(seed);
        let weights: Vec<u64> = (0..field::PACKETS).map(|_| 60 + rng.below(80)).collect();
        let total: u64 = weights.iter().sum();
        let mut left = nominal;
        let mut out = Vec::new();
        for (i, w) in weights.iter().enumerate() {
            let n = if i + 1 == field::PACKETS {
                left
            } else {
                ((nominal as u128 * *w as u128) / total as u128) as u64
            };
            let n = n.min(left);
            left -= n;
            // Every third packet is brutes, and the seed moves which third.
            let kind = ((i as u64 + seed % 3) % 3 == 0) as u8;
            let at = start + i as Tick * field::PACKET_GAP + rng.below(30);
            if n > 0 {
                out.push(Packet { at, kind, count: n });
            }
        }
        self.pending.extend(out);
        self.pending.sort_by_key(|p| (p.at, p.kind, p.count));
    }

    /// Put a structure back to full strength. Its battery, if it has one,
    /// starts looking for work at once.
    pub fn repair(&mut self, s: usize) {
        self.integrate();
        let max = STRUCTURES[s].hp * 1000;
        let was = self.structures[s].band;
        self.structures[s] = Structure { hp: max, since: self.now, band: Band::Intact };
        if was != Band::Intact {
            self.effects.push(Effect { at: self.now, structure: s, band: Band::Intact });
        }
        for (b, def) in BATTERIES.iter().enumerate() {
            if def.pit == s && !self.batteries[b].hold {
                self.batteries[b].duty = Duty::Idle { wake: self.now };
            }
        }
        self.rewake();
    }

    pub fn hold(&mut self, b: usize, hold: bool) {
        self.batteries[b].hold = hold;
        if hold {
            self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
        } else if matches!(self.batteries[b].duty, Duty::Idle { .. }) {
            self.batteries[b].duty = Duty::Idle { wake: self.now };
        }
        self.rewake();
    }

    // ------------------------------------------------------------ geometry

    fn speed(c: &Cohort) -> i64 {
        KINDS[c.kind as usize].speed
    }

    /// Where a cohort is at tick `t`, which must not be past its next arrival.
    pub fn pos(c: &Cohort, t: Tick) -> P {
        match c.motion {
            Motion::Assault { node } => ROUTE[node as usize],
            Motion::March { leg, since } => {
                let (a, b) = (ROUTE[leg as usize], ROUTE[leg as usize + 1]);
                let len = field::leg_len(leg as usize);
                let u = ((t.saturating_sub(since)) as i64 * Self::speed(c)).min(len);
                P {
                    x: a.x + ((b.x - a.x) as i128 * u as i128 / len as i128) as i64,
                    y: a.y + ((b.y - a.y) as i128 * u as i128 / len as i128) as i64,
                }
            }
        }
    }

    /// How far along the route a cohort is: the "most advanced" rule every
    /// battery picks targets by.
    fn progress(c: &Cohort, t: Tick) -> i64 {
        match c.motion {
            Motion::Assault { node } => field::along(node as usize),
            Motion::March { leg, since } => {
                let len = field::leg_len(leg as usize);
                field::along(leg as usize)
                    + ((t.saturating_sub(since)) as i64 * Self::speed(c)).min(len)
            }
        }
    }

    fn arrival(c: &Cohort) -> Option<Tick> {
        match c.motion {
            Motion::Assault { .. } => None,
            Motion::March { leg, since } => {
                let len = field::leg_len(leg as usize);
                let s = Self::speed(c);
                Some(since + ((len + s - 1) / s) as Tick)
            }
        }
    }

    fn standing(&self, node: usize) -> Option<usize> {
        STRUCTURES
            .iter()
            .position(|s| s.node == Some(node))
            .filter(|&s| self.structures[s].hp > 0)
    }

    /// Where a cohort will be at tick `t`, walking on past nodes nothing stands
    /// on and stopping at the first one something does. What a gunner leads by.
    fn predict(&self, c: &Cohort, t: Tick) -> P {
        let Motion::March { leg, since } = c.motion else { return Self::pos(c, t) };
        let s = Self::speed(c);
        let mut leg = leg as usize;
        let mut u = (t.saturating_sub(since)) as i64 * s;
        loop {
            let len = field::leg_len(leg);
            if u <= len {
                let probe = Cohort { motion: Motion::March { leg: leg as u16, since: 0 }, ..c.clone() };
                return Self::pos(&probe, (u / s) as Tick);
            }
            let node = leg + 1;
            if node + 1 >= ROUTE.len() || self.standing(node).is_some() {
                return ROUTE[node];
            }
            u -= len;
            leg = node;
        }
    }

    fn battery_pos(b: usize) -> P {
        STRUCTURES[BATTERIES[b].pit].at
    }

    fn alive(&self, b: usize) -> bool {
        self.structures[BATTERIES[b].pit].hp > 0
    }

    fn in_range(&self, b: usize, c: &Cohort, t: Tick) -> bool {
        dist(Self::battery_pos(b), Self::pos(c, t)) <= BATTERIES[b].range
    }

    /// The first tick at or after `from` at which cohort `c` could be shot at
    /// by battery `b`, on the leg it is walking now. Solved, not searched: the
    /// leg is a line and the range is a circle.
    fn entry(&self, b: usize, c: &Cohort, from: Tick) -> Tick {
        match c.motion {
            Motion::Assault { .. } => {
                if self.in_range(b, c, from) {
                    from
                } else {
                    Tick::MAX
                }
            }
            Motion::March { leg, since } => {
                if self.in_range(b, c, from) {
                    return from;
                }
                let (a, e) = (ROUTE[leg as usize], ROUTE[leg as usize + 1]);
                let len = field::leg_len(leg as usize) as f64;
                let cpos = Self::battery_pos(b);
                let (dx, dy) = ((e.x - a.x) as f64 / len, (e.y - a.y) as f64 / len);
                let (ox, oy) = ((a.x - cpos.x) as f64, (a.y - cpos.y) as f64);
                let r = BATTERIES[b].range as f64;
                let bb = dx * ox + dy * oy;
                let cc = ox * ox + oy * oy - r * r;
                let disc = bb * bb - cc;
                if disc < 0.0 {
                    return Tick::MAX;
                }
                let root = disc.sqrt();
                let (u1, u2) = (-bb - root, -bb + root);
                let s = Self::speed(c) as f64;
                let u_now = ((from - since) as f64 * s).min(len);
                if u2 < u_now || u1 > len {
                    return Tick::MAX;
                }
                let u_in = u1.max(u_now);
                let end = Self::arrival(c).unwrap_or(Tick::MAX);
                let mut t = (since + (u_in / s).ceil() as Tick).max(from);
                // The circle is solved in reals and walked in integers; a tick
                // or two of rounding is settled by asking the integer question.
                for _ in 0..4 {
                    if t >= end || self.in_range(b, c, t) {
                        break;
                    }
                    t += 1;
                }
                t.min(end)
            }
        }
    }

    // ---------------------------------------------------------- scheduling

    fn drain(&self, s: usize) -> i64 {
        let Some(node) = STRUCTURES[s].node else { return 0 };
        self.cohorts
            .iter()
            .filter(|c| c.motion == Motion::Assault { node: node as u16 })
            .map(|c| c.count as i64 * KINDS[c.kind as usize].bite)
            .fold(0i64, |a, b| a.saturating_add(b))
    }

    fn band_time(&self, s: usize) -> Option<Tick> {
        let st = &self.structures[s];
        if st.band == Band::Destroyed {
            return None;
        }
        // Already past a threshold nobody has acted on -- another event at the
        // same tick brought the hp up to date first -- so the band changes now.
        // Skipping it here once left a wall at zero hp still standing, with a
        // million attackers politely queued in front of it for twenty minutes.
        if Band::of(st.hp, STRUCTURES[s].hp * 1000) != st.band {
            return Some(st.since.max(self.now));
        }
        let rate = self.drain(s);
        if rate == 0 {
            return None;
        }
        let floor = st.band.floor(STRUCTURES[s].hp * 1000);
        let need = st.hp - floor;
        Some(st.since + ((need + rate - 1) / rate).max(0) as Tick)
    }

    fn duty_time(&self, b: usize) -> Option<Tick> {
        match self.batteries[b].duty {
            Duty::Idle { wake } => (wake != Tick::MAX).then_some(wake),
            Duty::Laying { fire, .. } => Some(fire),
            Duty::Reload { until } => Some(until),
        }
    }

    /// The next event: `(tick, class, which)`. Every candidate is computed from
    /// state; nothing is remembered between calls.
    pub fn next(&self) -> Option<(Tick, u8, u32)> {
        let mut best: Option<(Tick, u8, u32)> = None;
        let mut take = |e: (Tick, u8, u32)| {
            if best.map_or(true, |b| e < b) {
                best = Some(e);
            }
        };
        for v in &self.volleys {
            take((v.lands, EV_IMPACT, v.id));
        }
        for s in 0..self.structures.len() {
            if let Some(t) = self.band_time(s) {
                take((t, EV_BAND, s as u32));
            }
        }
        for c in &self.cohorts {
            if let Some(t) = Self::arrival(c) {
                take((t, EV_ARRIVE, c.id));
            }
        }
        if let Some(p) = self.pending.first() {
            take((p.at, EV_SPAWN, 0));
        }
        for b in 0..self.batteries.len() {
            if let Some(t) = self.duty_time(b) {
                take((t, EV_BATTERY, b as u32));
            }
        }
        best
    }

    pub fn next_tick(&self) -> Option<Tick> {
        self.next().map(|e| e.0)
    }

    /// Nothing is alive, in the air, or still to come.
    pub fn quiet(&self) -> bool {
        self.cohorts.is_empty() && self.volleys.is_empty() && self.pending.is_empty()
    }

    /// Run every event up to and including tick `t`.
    pub fn advance(&mut self, t: Tick) {
        while let Some(ev) = self.next() {
            if ev.0 > t {
                break;
            }
            self.handle(ev);
        }
        if t > self.now {
            self.now = t;
        }
        self.prune_fx();
    }

    /// Handle exactly one event. Public so the encounter can interleave the
    /// factory's reaction at the event's own tick.
    pub fn step(&mut self) -> Option<Tick> {
        let ev = self.next()?;
        self.handle(ev);
        Some(ev.0)
    }

    fn handle(&mut self, (t, class, which): (Tick, u8, u32)) {
        debug_assert!(t >= self.now, "an event in the fight's past");
        self.now = t;
        // Every structure's hp is brought up to now before anything changes who
        // is standing at it, because that is what its drain was a function of.
        self.integrate();
        self.stats.events += 1;
        self.stats.visits +=
            (self.cohorts.len() + self.volleys.len() + self.structures.len() + self.batteries.len()) as u64;
        match class {
            EV_IMPACT => self.impact(which),
            EV_BAND => self.band(which as usize),
            EV_ARRIVE => self.arrive(which),
            EV_SPAWN => self.spawn(),
            _ => self.battery(which as usize),
        }
        self.merge();
        self.rewake();
        let alive: u64 = self.cohorts.iter().map(|c| c.count).sum();
        let st = &mut self.stats;
        st.peak_cohorts = st.peak_cohorts.max(self.cohorts.len());
        st.peak_volleys = st.peak_volleys.max(self.volleys.len());
        st.peak_alive = st.peak_alive.max(alive);
    }

    fn integrate(&mut self) {
        for s in 0..self.structures.len() {
            let rate = self.drain(s);
            let st = &mut self.structures[s];
            if rate > 0 && st.hp > 0 {
                let dt = (self.now - st.since) as i64;
                st.hp = (st.hp - rate.saturating_mul(dt)).max(0);
            }
            st.since = self.now;
        }
    }

    // ------------------------------------------------------------ handlers

    fn spawn(&mut self) {
        let p = self.pending.remove(0);
        let id = self.next_id;
        self.next_id += 1;
        self.cohorts.push(Cohort {
            id,
            kind: p.kind,
            count: p.count,
            hp: KINDS[p.kind as usize].hp,
            motion: Motion::March { leg: 0, since: p.at },
        });
        self.spawned += p.count;
        self.stats.spawns += 1;
        self.stats.created += 1;
        self.note(p.at, || {
            format!("cohort #{id} released: {} x{}", KINDS[p.kind as usize].name, p.count)
        });
    }

    fn arrive(&mut self, id: u32) {
        self.stats.arrivals += 1;
        let t = self.now;
        let Some(i) = self.cohorts.iter().position(|c| c.id == id) else { return };
        let Motion::March { leg, .. } = self.cohorts[i].motion else { return };
        let node = leg as usize + 1;
        if node + 1 == ROUTE.len() {
            let c = self.cohorts.remove(i);
            self.leaked += c.count;
            self.fx.push(Fx { at: t, what: "leak", pos: ROUTE[node], radius: 0, n: c.count });
            self.note(t, || format!("cohort #{id} x{} leaves the domain into the works", c.count));
            return;
        }
        let next = match self.standing(node) {
            Some(s) => {
                let n = self.cohorts[i].count;
                self.note(t, || format!("cohort #{id} x{n} reaches {}", STRUCTURES[s].name));
                Motion::Assault { node: node as u16 }
            }
            None => Motion::March { leg: node as u16, since: t },
        };
        self.cohorts[i].motion = next;
    }

    fn band(&mut self, s: usize) {
        self.stats.bands += 1;
        let t = self.now;
        let max = STRUCTURES[s].hp * 1000;
        let band = Band::of(self.structures[s].hp, max);
        if band == self.structures[s].band {
            return;
        }
        self.structures[s].band = band;
        self.effects.push(Effect { at: t, structure: s, band });
        self.note(t, || format!("{} is {}", STRUCTURES[s].name, band.word()));
        if band != Band::Destroyed {
            return;
        }
        self.fx.push(Fx { at: t, what: "fall", pos: STRUCTURES[s].at, radius: 2_500, n: 0 });
        if let Some(node) = STRUCTURES[s].node {
            for c in &mut self.cohorts {
                if c.motion == (Motion::Assault { node: node as u16 }) {
                    c.motion = Motion::March { leg: node as u16, since: t };
                }
            }
        }
        for (b, def) in BATTERIES.iter().enumerate() {
            if def.pit == s {
                self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
            }
        }
    }

    fn battery(&mut self, b: usize) {
        self.stats.battery += 1;
        let t = self.now;
        match self.batteries[b].duty {
            Duty::Laying { to, at, lands, .. } => {
                let id = self.next_id;
                self.next_id += 1;
                let def = &BATTERIES[b];
                self.volleys.push(Volley {
                    id,
                    battery: b as u16,
                    shells: def.count,
                    from: Self::battery_pos(b),
                    to: at,
                    fired: t,
                    lands,
                });
                let bat = &mut self.batteries[b];
                bat.heading = to;
                bat.last_fire = Some(t);
                bat.volleys += 1;
                bat.duty = Duty::Reload { until: t + field::gun(def.gun).reload };
                self.stats.volleys += 1;
                self.note(t, || {
                    format!("{} volley fires ({} shells), lands t={lands}", def.name, def.count)
                });
            }
            Duty::Idle { .. } | Duty::Reload { .. } => {
                let was_idle = matches!(self.batteries[b].duty, Duty::Idle { .. });
                if !self.alive(b) || self.batteries[b].hold {
                    self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
                    return;
                }
                match self.target(b, t) {
                    Some(ci) => {
                        let duty = self.lay(b, ci, t);
                        if was_idle {
                            let (id, n) = (self.cohorts[ci].id, self.cohorts[ci].count);
                            self.note(t, || {
                                format!("cohort #{id} x{n} enters range of {}", BATTERIES[b].name)
                            });
                        }
                        self.batteries[b].duty = duty;
                    }
                    None => self.batteries[b].duty = Duty::Idle { wake: Tick::MAX },
                }
            }
        }
    }

    fn target(&self, b: usize, t: Tick) -> Option<usize> {
        self.cohorts
            .iter()
            .enumerate()
            .filter(|(_, c)| self.in_range(b, c, t))
            .max_by_key(|(_, c)| (Self::progress(c, t), std::cmp::Reverse(c.id)))
            .map(|(i, _)| i)
    }

    /// Turn, then fire, leading the target by the flight time. Three passes of
    /// "where will it be when the shell gets there", which converges because a
    /// shell is much faster than anything it is fired at.
    fn lay(&self, b: usize, ci: usize, t: Tick) -> Duty {
        let def: &BatteryDef = &BATTERIES[b];
        let g = field::gun(def.gun);
        let from = self.batteries[b].heading;
        let origin = Self::battery_pos(b);
        let c = &self.cohorts[ci];
        let mut guess = t + LAY_MIN;
        let mut out = (from, t + LAY_MIN, Self::pos(c, t), t + LAY_MIN + g.min_flight);
        for _ in 0..3 {
            let at = self.predict(c, guess);
            let to = bearing(origin, at);
            let turn = turn_between(from, to).abs() as Tick;
            let fire = t + LAY_MIN.max((turn + g.turn as Tick - 1) / g.turn as Tick);
            let d = dist(origin, at);
            let flight = g.min_flight.max(((d + g.shell_speed - 1) / g.shell_speed) as Tick);
            out = (to, fire, at, fire + flight);
            guess = fire + flight;
        }
        let (to, fire, at, lands) = out;
        Duty::Laying { from, to, since: t, fire, at, lands }
    }

    fn impact(&mut self, id: u32) {
        self.stats.impacts += 1;
        let t = self.now;
        let Some(vi) = self.volleys.iter().position(|v| v.id == id) else { return };
        let v = self.volleys.remove(vi);
        let def = &BATTERIES[v.battery as usize];
        let g = field::gun(def.gun);
        let mut left = g.hits * v.shells as u64;
        let mut near: Vec<(i64, usize)> = self
            .cohorts
            .iter()
            .enumerate()
            .map(|(i, c)| (dist(Self::pos(c, t), v.to), i))
            .filter(|&(d, i)| d <= g.blast + spread(self.cohorts[i].count))
            .collect();
        near.sort_by_key(|&(d, i)| (d, self.cohorts[i].id));
        let mut born: Vec<Cohort> = Vec::new();
        let (mut hits, mut kills) = (0u64, 0u64);
        let mut said: Vec<String> = Vec::new();
        for (_, i) in near {
            if left == 0 {
                break;
            }
            let c = &mut self.cohorts[i];
            let k = c.count.min(left);
            left -= k;
            hits += k;
            let hp = c.hp.saturating_sub(g.damage);
            c.count -= k;
            if hp == 0 {
                kills += k;
                if self.trace.is_some() {
                    said.push(format!("#{} loses x{k}, x{} left", c.id, c.count));
                }
            } else {
                let nid = self.next_id;
                self.next_id += 1;
                if self.trace.is_some() {
                    said.push(format!(
                        "#{} splits: hp {} x{}, hp {hp} x{k} (#{nid})",
                        c.id, c.hp, c.count
                    ));
                }
                born.push(Cohort { id: nid, kind: c.kind, count: k, hp, motion: c.motion });
                self.stats.splits += (c.count > 0) as u64;
                self.stats.created += 1;
            }
        }
        self.killed += kills;
        self.stats.hits += hits;
        self.cohorts.retain(|c| c.count > 0);
        self.cohorts.extend(born);
        self.fx.push(Fx { at: t, what: "impact", pos: v.to, radius: g.blast, n: kills });
        self.note(t, || {
            let who = if said.is_empty() { "nothing there".to_string() } else { said.join("; ") };
            format!("{} volley impacts: {who}", def.name)
        });
    }

    /// Cohorts in the same state are the same cohort. Keep the oldest id.
    fn merge(&mut self) {
        if self.cohorts.len() < 2 {
            return;
        }
        self.cohorts.sort_by_key(|c| (c.kind, c.hp, c.motion, c.id));
        let mut out: Vec<Cohort> = Vec::with_capacity(self.cohorts.len());
        for c in self.cohorts.drain(..) {
            match out.last_mut() {
                Some(l) if l.kind == c.kind && l.hp == c.hp && l.motion == c.motion => {
                    l.count += c.count;
                    self.stats.merges += 1;
                }
                _ => out.push(c),
            }
        }
        out.sort_by_key(|c| c.id);
        self.cohorts = out;
    }

    /// Recompute when every idle battery could next have something to shoot.
    fn rewake(&mut self) {
        let now = self.now;
        for b in 0..self.batteries.len() {
            let Duty::Idle { .. } = self.batteries[b].duty else { continue };
            if self.batteries[b].hold || !self.alive(b) {
                self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
                continue;
            }
            let wake = self.cohorts.iter().map(|c| self.entry(b, c, now)).min().unwrap_or(Tick::MAX);
            // An event handler for this battery at this very tick has already
            // looked and found nothing; it is not asked twice.
            let wake = if wake == now && self.target(b, now).is_none() { now + 1 } else { wake };
            self.batteries[b].duty = Duty::Idle { wake };
        }
    }

    fn prune_fx(&mut self) {
        let keep = self.now.saturating_sub(FX_KEEP);
        self.fx.retain(|f| f.at >= keep);
    }

    // ---------------------------------------------------------- inspection

    pub fn hp_at(&self, s: usize, t: Tick) -> i64 {
        let st = &self.structures[s];
        let dt = t.saturating_sub(st.since) as i64;
        (st.hp - self.drain(s).saturating_mul(dt)).max(0)
    }

    pub fn alive_count(&self) -> u64 {
        self.cohorts.iter().map(|c| c.count).sum()
    }

    pub fn shells_in_flight(&self) -> u64 {
        self.volleys.iter().map(|v| v.shells as u64).sum()
    }

    pub fn turrets_alive(&self) -> u64 {
        (0..self.batteries.len()).filter(|&b| self.alive(b)).map(|b| BATTERIES[b].count as u64).sum()
    }

    pub fn standing_structures(&self) -> usize {
        self.structures.iter().filter(|s| s.hp > 0).count()
    }

    pub fn lost_structures(&self) -> Vec<&'static str> {
        (0..self.structures.len())
            .filter(|&s| self.structures[s].hp <= 0)
            .map(|s| STRUCTURES[s].name)
            .collect()
    }

    // --------------------------------------------------------------- JSON

    /// The fight's whole state, and nothing else: no events, no flashes, no
    /// statistics. This is what a checkpoint holds and what a hash is of.
    pub fn to_json(&self) -> Json {
        let p = |p: P| Json::arr([p.x, p.y]);
        Json::obj()
            .set("now", self.now)
            .set("next", self.next_id as u64)
            .set("spawned", Json::big(self.spawned as u128))
            .set("killed", Json::big(self.killed as u128))
            .set("leaked", Json::big(self.leaked as u128))
            .set(
                "cohorts",
                Json::Arr(
                    self.cohorts
                        .iter()
                        .map(|c| {
                            let j = Json::obj()
                                .set("id", c.id as u64)
                                .set("kind", c.kind as u64)
                                .set("count", Json::big(c.count as u128))
                                .set("hp", c.hp as u64);
                            match c.motion {
                                Motion::March { leg, since } => {
                                    j.set("leg", leg as u64).set("since", since)
                                }
                                Motion::Assault { node } => j.set("node", node as u64),
                            }
                        })
                        .collect(),
                ),
            )
            .set(
                "batteries",
                Json::Arr(
                    self.batteries
                        .iter()
                        .map(|b| {
                            let j = Json::obj()
                                .set("heading", b.heading as i64)
                                .set("lastFire", b.last_fire)
                                .set("volleys", b.volleys)
                                .set("hold", b.hold);
                            match b.duty {
                                Duty::Idle { wake } => j.set("duty", "idle").set(
                                    "wake",
                                    if wake == Tick::MAX { Json::Null } else { Json::from(wake) },
                                ),
                                Duty::Laying { from, to, since, fire, at, lands } => j
                                    .set("duty", "laying")
                                    .set("from", from as i64)
                                    .set("to", to as i64)
                                    .set("since", since)
                                    .set("fire", fire)
                                    .set("at", p(at))
                                    .set("lands", lands),
                                Duty::Reload { until } => j.set("duty", "reload").set("until", until),
                            }
                        })
                        .collect(),
                ),
            )
            .set(
                "structures",
                Json::Arr(
                    self.structures
                        .iter()
                        .map(|s| {
                            Json::obj()
                                .set("hp", s.hp)
                                .set("since", s.since)
                                .set("band", s.band.word())
                        })
                        .collect(),
                ),
            )
            .set(
                "volleys",
                Json::Arr(
                    self.volleys
                        .iter()
                        .map(|v| {
                            Json::obj()
                                .set("id", v.id as u64)
                                .set("battery", v.battery as u64)
                                .set("shells", v.shells as u64)
                                .set("from", p(v.from))
                                .set("to", p(v.to))
                                .set("fired", v.fired)
                                .set("lands", v.lands)
                        })
                        .collect(),
                ),
            )
            .set(
                "pending",
                Json::Arr(
                    self.pending
                        .iter()
                        .map(|k| {
                            Json::obj()
                                .set("at", k.at)
                                .set("kind", k.kind as u64)
                                .set("count", Json::big(k.count as u128))
                        })
                        .collect(),
                ),
            )
    }

    pub fn from_json(j: &Json) -> Result<Fight, String> {
        let u = |j: &Json, k: &str| -> Result<u64, String> {
            let v = j.at(k);
            v.as_u64()
                .or_else(|| v.as_str().and_then(|s| s.parse().ok()))
                .ok_or_else(|| format!("fight: `{k}` is missing"))
        };
        let i = |j: &Json, k: &str| -> Result<i64, String> {
            j.at(k).as_i128().map(|v| v as i64).ok_or_else(|| format!("fight: `{k}` is missing"))
        };
        let pt = |j: &Json| -> Result<P, String> {
            let a = j.as_arr();
            match (a.first().and_then(Json::as_i128), a.get(1).and_then(Json::as_i128)) {
                (Some(x), Some(y)) => Ok(P { x: x as i64, y: y as i64 }),
                _ => Err("fight: a point is not two integers".into()),
            }
        };
        let mut f = Fight::new();
        f.now = u(j, "now")?;
        f.next_id = u(j, "next")? as u32;
        f.spawned = u(j, "spawned")?;
        f.killed = u(j, "killed")?;
        f.leaked = u(j, "leaked")?;
        for c in j.at("cohorts").as_arr() {
            let motion = if c.at("node").is_null() {
                Motion::March { leg: u(c, "leg")? as u16, since: u(c, "since")? }
            } else {
                Motion::Assault { node: u(c, "node")? as u16 }
            };
            f.cohorts.push(Cohort {
                id: u(c, "id")? as u32,
                kind: u(c, "kind")? as u8,
                count: u(c, "count")?,
                hp: u(c, "hp")? as u16,
                motion,
            });
        }
        let bats = j.at("batteries").as_arr();
        if bats.len() != f.batteries.len() {
            return Err("fight: a different number of batteries".into());
        }
        for (b, bj) in f.batteries.iter_mut().zip(bats) {
            b.heading = i(bj, "heading")? as i32;
            b.last_fire = bj.at("lastFire").as_u64();
            b.volleys = u(bj, "volleys")?;
            b.hold = bj.at("hold").as_bool().unwrap_or(false);
            b.duty = match bj.at("duty").as_str().unwrap_or("idle") {
                "laying" => Duty::Laying {
                    from: i(bj, "from")? as i32,
                    to: i(bj, "to")? as i32,
                    since: u(bj, "since")?,
                    fire: u(bj, "fire")?,
                    at: pt(bj.at("at"))?,
                    lands: u(bj, "lands")?,
                },
                "reload" => Duty::Reload { until: u(bj, "until")? },
                _ => Duty::Idle { wake: bj.at("wake").as_u64().unwrap_or(Tick::MAX) },
            };
        }
        let sts = j.at("structures").as_arr();
        if sts.len() != f.structures.len() {
            return Err("fight: a different number of structures".into());
        }
        for (s, sj) in f.structures.iter_mut().zip(sts) {
            s.hp = i(sj, "hp")?;
            s.since = u(sj, "since")?;
            s.band = Band::parse(sj.at("band").as_str().unwrap_or(""));
        }
        for v in j.at("volleys").as_arr() {
            f.volleys.push(Volley {
                id: u(v, "id")? as u32,
                battery: u(v, "battery")? as u16,
                shells: u(v, "shells")? as u32,
                from: pt(v.at("from"))?,
                to: pt(v.at("to"))?,
                fired: u(v, "fired")?,
                lands: u(v, "lands")?,
            });
        }
        for k in j.at("pending").as_arr() {
            f.pending.push(Packet {
                at: u(k, "at")?,
                kind: u(k, "kind")? as u8,
                count: u(k, "count")?,
            });
        }
        Ok(f)
    }

    /// What a view needs beyond the state: flashes, and each battery's name.
    pub fn fx_json(&self) -> Json {
        Json::Arr(
            self.fx
                .iter()
                .map(|f| {
                    Json::obj()
                        .set("at", f.at)
                        .set("what", f.what)
                        .set("pos", Json::arr([f.pos.x, f.pos.y]))
                        .set("r", f.radius)
                        .set("n", Json::big(f.n as u128))
                })
                .collect(),
        )
    }
}

/// Where a structure is drawn, which is also where it is hit.
pub fn is_on_route(s: usize) -> bool {
    STRUCTURES[s].node.is_some() && STRUCTURES[s].kind != SKind::Shed
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marching_cohort_is_where_its_closed_form_says() {
        let c = Cohort { id: 1, kind: 0, count: 10, hp: 4, motion: Motion::March { leg: 0, since: 100 } };
        assert_eq!(Fight::pos(&c, 100), ROUTE[0]);
        let end = Fight::arrival(&c).unwrap();
        assert_eq!(Fight::pos(&c, end), ROUTE[1]);
    }

    #[test]
    fn identical_states_merge() {
        let mut f = Fight::new();
        let m = Motion::Assault { node: 2 };
        f.cohorts.push(Cohort { id: 4, kind: 0, count: 10, hp: 2, motion: m });
        f.cohorts.push(Cohort { id: 9, kind: 0, count: 5, hp: 2, motion: m });
        f.cohorts.push(Cohort { id: 7, kind: 0, count: 5, hp: 4, motion: m });
        f.merge();
        assert_eq!(f.cohorts.len(), 2);
        assert_eq!((f.cohorts[0].id, f.cohorts[0].count), (4, 15));
    }
}
