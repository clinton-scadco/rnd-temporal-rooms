//! The disturbance: strain, ruptures, bleed, manifestations, volleys, and the
//! events between them.
//!
//! Nothing is sent. Every tonne that comes through the gantry leaves strain
//! behind it, and strain is a closed form -- a level and a rate, and the rate
//! only changes at an event -- so the moment a site tears is *solved*, the same
//! way the moment a wall crosses a damage band is:
//!
//! ```text
//!   lanes open, grid holding      strain rises at  tonnes/s x years x share
//!   anchors switched on           strain falls at  ANCHOR_DRAIN per site
//!   manifestations at their stock strain falls at  members x absorb
//!
//!   crosses `open`                the site tears: a rupture
//!   rupture, every second         vents strain as manifestations
//!   crosses 2x, 4x `open`         the bleed widens
//!   falls to `close`              the rupture seals
//! ```
//!
//! # The bleed is the disturbance; the crowd is what comes out of it
//!
//! Inside a rupture's bleed radius the ground is the century the strain came
//! from. That is a rule, not a paint job: an 1890 bleed has no grid in it, so a
//! battery standing in one cannot lay, and a gantry standing in one cannot hold
//! a fracture open -- the interface goes dark, nothing more crosses, and the
//! tear starves. An anchor pins the ground under it to 2037; a pinned rupture
//! still vents, but it does not bleed.
//!
//! A manifestation is made of one century's strain and it wants that century's
//! material back: an echo walks to where the 1890 ore is stacked, a glint to
//! where the 2070 lathes are running. When there is none left -- taken back,
//! laundered into 2037 concentrate before it was ever stacked, or burned with
//! the building it was in -- it has nothing to be, and fades.
//!
//! # No tick loop
//!
//! Same as experiment 16's first version and `sim.rs`: the next thing that
//! happens is found by asking every piece of compressed state when it next
//! changes, and the clock jumps there. The queue is never state.

use super::field::{
    self, BatteryDef, ANCHORS, ANCHOR_DRAIN, ANCHOR_MW, BATTERIES, BLEED_R, BLEED_X, COST, EMIT_EVERY,
    GANTRY, GANTRY_SHARE, GRID_MW, KINDS, LANES, LAUNDER_MW, NODES, SITES, STRUCTURES, VENT_PM,
};
use super::{bearing, dist, turn_between, P, TICK_RATE};
use crate::json::Json;
use crate::model::Tick;

// ==================================================================== state

/// Where a cohort is, as a closed form of time.
#[derive(Clone, Copy, Debug, PartialEq, Eq, PartialOrd, Ord, Hash)]
pub enum Motion {
    /// Walking the road from node `from` to node `to`, having set off at `since`.
    March { from: u8, to: u8, since: Tick },
    /// Standing in a site, taking its anachronism back.
    Assault { site: u8 },
}

/// Every manifestation in one state, as one record.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Cohort {
    pub id: u32,
    pub kind: u8,
    pub count: u64,
    pub hp: u16,
    pub motion: Motion,
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Duty {
    Idle { wake: Tick },
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
    pub hp: i64,
    pub since: Tick,
    pub band: Band,
}

/// A tear in a site.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Rupture {
    pub since: Tick,
    /// Times it has vented.
    pub emits: u32,
    /// The component that tore it, which is the century it bleeds.
    pub source: u8,
    /// Bleed band, 0..=2, as of the last strain event.
    pub band: u8,
}

/// Strain at one site, as a closed form: two levels at `since`, and rates that
/// are derived from everything else and never stored.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Site {
    pub strain: [i64; 2],
    pub since: Tick,
    pub rupture: Option<Rupture>,
}

/// Something the encounter has to hear about.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Effect {
    Band { at: Tick, structure: usize, band: Band },
    Tear { at: Tick, site: usize },
    Seal { at: Tick, site: usize },
    Bleed { at: Tick, site: usize, band: u8 },
}

/// Something only a renderer cares about. Never checkpointed, never hashed.
#[derive(Clone, Debug)]
pub struct Fx {
    pub at: Tick,
    pub what: &'static str,
    pub pos: P,
    pub radius: i64,
    pub n: u64,
    pub kind: u8,
}

#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub events: u64,
    pub impacts: u64,
    pub bands: u64,
    pub strains: u64,
    pub emits: u64,
    pub arrivals: u64,
    pub battery: u64,
    pub volleys: u64,
    pub hits: u64,
    pub splits: u64,
    pub merges: u64,
    pub created: u64,
    pub visits: u64,
    pub tears: u64,
    pub peak_cohorts: usize,
    pub peak_volleys: usize,
    pub peak_alive: u64,
}

#[derive(Clone, Debug)]
pub struct Fight {
    pub now: Tick,
    pub cohorts: Vec<Cohort>,
    pub batteries: Vec<Battery>,
    pub structures: Vec<Structure>,
    pub volleys: Vec<Volley>,
    pub sites: Vec<Site>,
    /// What each lane is asked to carry, in tonnes a second.
    pub lanes: Vec<u64>,
    /// Milli-tonnes that have crossed on each lane, as of `crossed_since`.
    pub crossed: Vec<u64>,
    pub crossed_since: Tick,
    pub anchors: Vec<bool>,
    pub launder: bool,
    pub next_id: u32,
    /// Members ever manifested, killed by a shell, and faded because there was
    /// nothing left for them to be.
    pub spawned: u64,
    pub killed: u64,
    pub faded: u64,

    pub stats: Stats,
    pub fx: Vec<Fx>,
    pub effects: Vec<Effect>,
    pub trace: Option<Vec<String>>,
}

const EV_IMPACT: u8 = 0;
const EV_BAND: u8 = 1;
const EV_STRAIN: u8 = 2;
const EV_EMIT: u8 = 3;
const EV_ARRIVE: u8 = 4;
const EV_BATTERY: u8 = 5;

const LAY_MIN: Tick = 6;
const FX_KEEP: Tick = 4 * TICK_RATE;
const TRACE_CAP: usize = 400;

/// How wide a cohort stands, in milli-tiles, for being hit and for being drawn.
pub fn spread(count: u64) -> i64 {
    (500.0 + 28.0 * (count as f64).sqrt()).min(4_800.0) as i64
}

fn ceil_div(a: i64, b: i64) -> i64 {
    (a + b - 1) / b
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
            sites: SITES.iter().map(|_| Site { strain: [0, 0], since: 0, rupture: None }).collect(),
            lanes: vec![0; LANES.len()],
            crossed: vec![0; LANES.len()],
            crossed_since: 0,
            anchors: vec![false; ANCHORS.len()],
            launder: false,
            next_id: 1,
            spawned: 0,
            killed: 0,
            faded: 0,
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
    //
    // Every command changes a rate, so every command first brings every level
    // up to now under the old rates.

    pub fn stream(&mut self, lane: usize, rate: u64) {
        self.integrate();
        self.lanes[lane] = rate;
        self.rewake();
    }

    pub fn anchor(&mut self, a: usize, on: bool) {
        self.integrate();
        self.anchors[a] = on;
        self.rewake();
    }

    pub fn set_launder(&mut self, on: bool) {
        self.integrate();
        self.launder = on;
        self.rewake();
    }

    pub fn repair(&mut self, s: usize) {
        self.integrate();
        let max = STRUCTURES[s].hp * 1000;
        let was = self.structures[s].band;
        self.structures[s] = Structure { hp: max, since: self.now, band: Band::Intact };
        if was != Band::Intact {
            self.effects.push(Effect::Band { at: self.now, structure: s, band: Band::Intact });
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

    // ------------------------------------------------------------ the grid

    pub fn standing(&self, s: usize) -> bool {
        self.structures[s].hp > 0
    }

    pub fn anchor_live(&self, a: usize) -> bool {
        self.anchors[a] && self.standing(ANCHORS[a].structure)
    }

    pub fn laundering(&self) -> bool {
        self.launder && self.standing(field::CRUSHER)
    }

    /// Megawatts asked for, and the thousandths of what the lanes want that
    /// the lanes actually get. Anchors and laundering are paid first: time held
    /// still comes before time held open.
    pub fn grid(&self) -> (u64, u64) {
        let fixed: u64 = (0..ANCHORS.len()).filter(|&a| self.anchor_live(a)).count() as u64 * ANCHOR_MW
            + if self.laundering() { LAUNDER_MW } else { 0 };
        let lanes: u64 = (0..LANES.len()).filter(|&l| self.lanes[l] > 0).map(|l| LANES[l].mw()).sum();
        let demand = fixed + lanes;
        let factor = if lanes == 0 || demand <= GRID_MW {
            1000
        } else {
            (GRID_MW.saturating_sub(fixed) * 1000 / lanes).min(1000)
        };
        (demand, factor)
    }

    /// Whether a point is standing in an 1890 bleed, where there is no grid.
    pub fn ungridded(&self, p: P) -> bool {
        (0..SITES.len()).any(|s| {
            let r = self.bleed(s);
            r > 0
                && self.sites[s].rupture.is_some_and(|ru| !field::ORIGINS[ru.source as usize].grid())
                && dist(p, field::node(SITES[s].node)) <= r
        })
    }

    /// The interface is dark: its gantry is down, or standing in 1890.
    pub fn dark(&self) -> bool {
        !self.standing(GANTRY) || self.ungridded(STRUCTURES[GANTRY].at)
    }

    /// Milli-tonnes a second actually crossing on a lane.
    pub fn flow(&self, lane: usize) -> u64 {
        if self.dark() {
            return 0;
        }
        self.lanes[lane] * self.grid().1
    }

    pub fn pinned(&self, site: usize) -> bool {
        let at = field::node(SITES[site].node);
        (0..ANCHORS.len())
            .any(|a| self.anchor_live(a) && dist(at, STRUCTURES[ANCHORS[a].structure].at) <= ANCHORS[a].range)
    }

    /// The bleed radius of a site right now: 0 unless torn and unpinned.
    pub fn bleed(&self, site: usize) -> i64 {
        match self.sites[site].rupture {
            Some(r) if !self.pinned(site) => BLEED_R[r.band as usize],
            _ => 0,
        }
    }

    pub fn powered(&self, b: usize) -> bool {
        !self.ungridded(STRUCTURES[BATTERIES[b].pit].at)
    }

    // ------------------------------------------------------------- strain

    /// Thousandths of a lane's strain that lands on a site.
    fn share(&self, site: usize, lane: usize) -> i64 {
        let l = &LANES[lane];
        if site == 0 {
            GANTRY_SHARE
        } else if site == l.dest {
            if l.launder_path.is_some() && self.laundering() {
                0
            } else {
                1000 - GANTRY_SHARE
            }
        } else {
            0
        }
    }

    /// Milli-strain a tick, component `c` of site `s`, before the floor.
    fn raw_rate(&self, s: usize, c: usize) -> i64 {
        let mut r: i64 = 0;
        for l in 0..LANES.len() {
            if LANES[l].origin != c {
                continue;
            }
            let share = self.share(s, l);
            if share > 0 {
                r += self.flow(l) as i64 * LANES[l].years() as i64 * share / (60 * 1000);
            }
        }
        let at = field::node(SITES[s].node);
        for a in 0..ANCHORS.len() {
            if self.anchor_live(a) && dist(at, STRUCTURES[ANCHORS[a].structure].at) <= ANCHORS[a].range {
                r -= ANCHOR_DRAIN;
            }
        }
        for co in &self.cohorts {
            if co.motion == (Motion::Assault { site: s as u8 }) && KINDS[co.kind as usize].origin == c {
                r = r.saturating_sub((co.count as i64).saturating_mul(KINDS[co.kind as usize].absorb));
            }
        }
        r
    }

    /// The rate that actually applies: a component at zero cannot fall.
    pub fn rate(&self, s: usize, c: usize) -> i64 {
        let r = self.raw_rate(s, c);
        if r < 0 && self.sites[s].strain[c] <= 0 {
            0
        } else {
            r
        }
    }

    pub fn total(&self, s: usize) -> i64 {
        self.sites[s].strain[0] + self.sites[s].strain[1]
    }

    /// Strain at tick `t`, which must not be past the site's next event.
    pub fn strain_at(&self, s: usize, t: Tick) -> [i64; 2] {
        let st = &self.sites[s];
        let dt = t.saturating_sub(st.since) as i64;
        [0, 1].map(|c| (st.strain[c] + self.rate(s, c).saturating_mul(dt)).max(0))
    }

    /// The bleed band strain `total` calls for at site `s`.
    fn band_for(s: usize, total: i64) -> u8 {
        let open = SITES[s].open;
        (0..BLEED_X.len()).rev().find(|&b| total >= open * BLEED_X[b]).unwrap_or(0) as u8
    }

    /// Whether a site should be torn, given its total -- with hysteresis.
    fn should_tear(&self, s: usize, total: i64) -> bool {
        if self.sites[s].rupture.is_some() {
            total > field::close_at(s)
        } else {
            total >= SITES[s].open
        }
    }

    fn strain_time(&self, s: usize) -> Option<Tick> {
        let st = &self.sites[s];
        let total = self.total(s);
        let now = self.now.max(st.since);
        // A threshold already passed and not acted on changes things now.
        if self.should_tear(s, total) != st.rupture.is_some() {
            return Some(now);
        }
        if let Some(r) = st.rupture {
            if Self::band_for(s, total) != r.band {
                return Some(now);
            }
        }
        let rates = [self.rate(s, 0), self.rate(s, 1)];
        let rt = rates[0] + rates[1];
        let mut when: Vec<i64> = Vec::new();
        // A component emptying changes the rate of the total.
        for c in 0..2 {
            if rates[c] < 0 && st.strain[c] > 0 {
                when.push(ceil_div(st.strain[c], -rates[c]));
            }
        }
        // Rising to `level`, or falling strictly below it.
        let up = |level: i64| (rt > 0 && total < level).then(|| ceil_div(level - total, rt));
        let down = |level: i64| (rt < 0 && total >= level).then(|| ceil_div(total - level + 1, -rt));
        match st.rupture {
            None => when.extend(up(SITES[s].open)),
            Some(r) => {
                // Seals when it falls to `close`.
                let close = field::close_at(s);
                if rt < 0 && total > close {
                    when.push(ceil_div(total - close, -rt));
                }
                let open = SITES[s].open;
                let b = r.band as usize;
                if b + 1 < BLEED_X.len() {
                    when.extend(up(open * BLEED_X[b + 1]));
                }
                if b > 0 {
                    when.extend(down(open * BLEED_X[b]));
                }
            }
        }
        when.into_iter().min().map(|dt| st.since + dt.max(0) as Tick)
    }

    fn emit_time(&self, s: usize) -> Option<Tick> {
        self.sites[s].rupture.map(|r| r.since + (r.emits as Tick + 1) * EMIT_EVERY)
    }

    // ------------------------------------------------------------ geometry

    fn speed(c: &Cohort) -> i64 {
        KINDS[c.kind as usize].speed
    }

    pub fn pos(c: &Cohort, t: Tick) -> P {
        match c.motion {
            Motion::Assault { site } => field::node(SITES[site as usize].node),
            Motion::March { from, to, since } => {
                let (a, b) = (field::node(from as usize), field::node(to as usize));
                let len = field::edge_len(from as usize, to as usize).max(1);
                let u = ((t.saturating_sub(since)) as i64 * Self::speed(c)).min(len);
                P {
                    x: a.x + ((b.x - a.x) as i128 * u as i128 / len as i128) as i64,
                    y: a.y + ((b.y - a.y) as i128 * u as i128 / len as i128) as i64,
                }
            }
        }
    }

    fn arrival(c: &Cohort) -> Option<Tick> {
        match c.motion {
            Motion::Assault { .. } => None,
            Motion::March { from, to, since } => {
                let len = field::edge_len(from as usize, to as usize);
                let s = Self::speed(c);
                Some(since + ((len + s - 1) / s) as Tick)
            }
        }
    }

    /// The site a manifestation of kind `k` wants: the one holding the most of
    /// its century's material, if any holds any. Ties go to the lower index.
    pub fn target(&self, k: usize) -> Option<usize> {
        let c = KINDS[k].origin;
        (0..SITES.len())
            .filter(|&s| SITES[s].holds == Some(c))
            .filter(|&s| self.standing(SITES[s].structure) && self.sites[s].strain[c] > 0)
            .max_by_key(|&s| (self.sites[s].strain[c], std::cmp::Reverse(s)))
    }

    /// How far a cohort still has to walk, for choosing what to shoot first.
    fn remaining(&self, c: &Cohort, t: Tick) -> i64 {
        let Some(site) = self.target(c.kind as usize) else { return i64::MAX / 4 };
        let goal = SITES[site].node;
        match c.motion {
            Motion::Assault { .. } => 0,
            Motion::March { from, to, since } => {
                let len = field::edge_len(from as usize, to as usize);
                let left = (len - (t.saturating_sub(since)) as i64 * Self::speed(c)).max(0);
                left + field::roads().far[to as usize][goal]
            }
        }
    }

    /// Where a cohort will be at tick `t`, walking on toward its target. What a
    /// gunner leads by.
    fn predict(&self, c: &Cohort, t: Tick) -> P {
        let Motion::March { from, to, since } = c.motion else { return Self::pos(c, t) };
        let goal = self.target(c.kind as usize).map(|s| SITES[s].node);
        let s = Self::speed(c);
        let (mut a, mut b) = (from as usize, to as usize);
        let mut u = (t.saturating_sub(since)) as i64 * s;
        for _ in 0..NODES.len() {
            let len = field::edge_len(a, b);
            if u <= len {
                let probe = Cohort { motion: Motion::March { from: a as u8, to: b as u8, since: 0 }, ..c.clone() };
                return Self::pos(&probe, (u / s) as Tick);
            }
            match goal {
                Some(g) if g != b => {
                    u -= len;
                    a = b;
                    b = field::roads().next[b][g];
                }
                _ => return field::node(b),
            }
        }
        field::node(b)
    }

    fn battery_pos(b: usize) -> P {
        STRUCTURES[BATTERIES[b].pit].at
    }

    fn can_fire(&self, b: usize) -> bool {
        self.standing(BATTERIES[b].pit) && !self.batteries[b].hold && self.powered(b)
    }

    fn in_range(&self, b: usize, c: &Cohort, t: Tick) -> bool {
        dist(Self::battery_pos(b), Self::pos(c, t)) <= BATTERIES[b].range
    }

    /// The first tick at or after `from` at which cohort `c` could be shot at
    /// by battery `b`, on the road it is walking now. The road is a line and
    /// the range is a circle.
    fn entry(&self, b: usize, c: &Cohort, from: Tick) -> Tick {
        match c.motion {
            Motion::Assault { .. } => {
                if self.in_range(b, c, from) {
                    from
                } else {
                    Tick::MAX
                }
            }
            Motion::March { from: na, to: nb, since } => {
                if self.in_range(b, c, from) {
                    return from;
                }
                let (a, e) = (field::node(na as usize), field::node(nb as usize));
                let len = field::edge_len(na as usize, nb as usize).max(1) as f64;
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
        let Some(site) = SITES.iter().position(|d| d.structure == s) else { return 0 };
        self.cohorts
            .iter()
            .filter(|c| c.motion == Motion::Assault { site: site as u8 })
            .map(|c| (c.count as i64).saturating_mul(KINDS[c.kind as usize].bite))
            .fold(0i64, |a, b| a.saturating_add(b))
    }

    fn band_time(&self, s: usize) -> Option<Tick> {
        let st = &self.structures[s];
        if st.band == Band::Destroyed {
            return None;
        }
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
        for s in 0..self.sites.len() {
            if let Some(t) = self.strain_time(s) {
                take((t, EV_STRAIN, s as u32));
            }
            if let Some(t) = self.emit_time(s) {
                take((t, EV_EMIT, s as u32));
            }
        }
        for c in &self.cohorts {
            if let Some(t) = Self::arrival(c) {
                take((t, EV_ARRIVE, c.id));
            }
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

    /// Nothing torn, nothing manifest, nothing in the air.
    pub fn quiet(&self) -> bool {
        self.cohorts.is_empty() && self.volleys.is_empty() && self.sites.iter().all(|s| s.rupture.is_none())
    }

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

    pub fn step(&mut self) -> Option<Tick> {
        let ev = self.next()?;
        self.handle(ev);
        Some(ev.0)
    }

    fn handle(&mut self, (t, class, which): (Tick, u8, u32)) {
        debug_assert!(t >= self.now, "an event in the fight's past");
        self.now = t;
        self.integrate();
        self.stats.events += 1;
        self.stats.visits += (self.cohorts.len()
            + self.volleys.len()
            + self.structures.len()
            + self.batteries.len()
            + self.sites.len()) as u64;
        match class {
            EV_IMPACT => self.impact(which),
            EV_BAND => self.band(which as usize),
            EV_STRAIN => self.strain(which as usize),
            EV_EMIT => self.emit(which as usize),
            EV_ARRIVE => self.arrive(which),
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

    /// Bring every level -- hp, strain, tonnes crossed -- up to now, under the
    /// rates that held since it was last written.
    ///
    /// Hit points last: a building reaching zero at this very tick must not
    /// change the rates the interval *before* it was running under.
    fn integrate(&mut self) {
        let now = self.now;
        let dt = now.saturating_sub(self.crossed_since);
        if dt > 0 {
            for l in 0..LANES.len() {
                self.crossed[l] += self.flow(l) * dt / TICK_RATE;
            }
        }
        self.crossed_since = now;
        let levels: Vec<[i64; 2]> = (0..self.sites.len()).map(|s| self.strain_at(s, now)).collect();
        for (st, level) in self.sites.iter_mut().zip(levels) {
            st.strain = level;
            st.since = now;
        }
        for s in 0..self.structures.len() {
            let rate = self.drain(s);
            let st = &mut self.structures[s];
            if rate > 0 && st.hp > 0 {
                let dt = (now - st.since) as i64;
                st.hp = (st.hp - rate.saturating_mul(dt)).max(0);
            }
            st.since = now;
        }
    }

    // ------------------------------------------------------------ handlers

    fn strain(&mut self, s: usize) {
        self.stats.strains += 1;
        let t = self.now;
        let total = self.total(s);
        let tear = self.should_tear(s, total);
        let at = field::node(SITES[s].node);
        match (self.sites[s].rupture, tear) {
            (None, true) => {
                let st = self.sites[s].strain;
                let source = if st[1] > st[0] { 1 } else { 0 };
                let band = Self::band_for(s, total);
                self.sites[s].rupture = Some(Rupture { since: t, emits: 0, source, band });
                self.stats.tears += 1;
                self.effects.push(Effect::Tear { at: t, site: s });
                self.fx.push(Fx { at: t, what: "tear", pos: at, radius: BLEED_R[band as usize], n: 0, kind: source });
                self.note(t, || {
                    format!(
                        "{} tears: {} ty of {} strain",
                        SITES[s].name,
                        total / field::TY,
                        field::ORIGINS[source as usize].tag()
                    )
                });
            }
            (Some(r), false) => {
                self.sites[s].rupture = None;
                self.effects.push(Effect::Seal { at: t, site: s });
                self.fx.push(Fx { at: t, what: "seal", pos: at, radius: BLEED_R[r.band as usize], n: 0, kind: r.source });
                self.note(t, || format!("{} seals after {} vents", SITES[s].name, r.emits));
            }
            (Some(r), true) => {
                let band = Self::band_for(s, total);
                if band != r.band {
                    self.sites[s].rupture = Some(Rupture { band, ..r });
                    self.effects.push(Effect::Bleed { at: t, site: s, band });
                    self.note(t, || format!("{} bleeds to {} tiles", SITES[s].name, BLEED_R[band as usize] / super::MT));
                }
            }
            (None, false) => {}
        }
        // A component that has just run out: the manifestations standing in it
        // have nothing left to take.
        self.release(s);
    }

    /// Manifestations standing in site `s` whose material is gone: they walk on
    /// to the next site that holds some, or fade.
    fn release(&mut self, s: usize) {
        let t = self.now;
        let here = SITES[s].node;
        let mut gone: Vec<usize> = Vec::new();
        for i in 0..self.cohorts.len() {
            if self.cohorts[i].motion != (Motion::Assault { site: s as u8 }) {
                continue;
            }
            let k = self.cohorts[i].kind as usize;
            let c = KINDS[k].origin;
            if self.sites[s].strain[c] > 0 && self.standing(SITES[s].structure) {
                continue;
            }
            match self.target(k) {
                Some(g) if SITES[g].node != here => {
                    let hop = field::roads().next[here][SITES[g].node];
                    self.cohorts[i].motion = Motion::March { from: here as u8, to: hop as u8, since: t };
                }
                _ => gone.push(i),
            }
        }
        for &i in gone.iter().rev() {
            let c = self.cohorts.remove(i);
            self.faded += c.count;
            let at = field::node(here);
            self.fx.push(Fx { at: t, what: "fade", pos: at, radius: spread(c.count), n: c.count, kind: c.kind });
            self.note(t, || {
                format!("{} x{} at {} fade: nothing left to take back", KINDS[c.kind as usize].name, c.count, SITES[s].name)
            });
        }
    }

    fn emit(&mut self, s: usize) {
        self.stats.emits += 1;
        let t = self.now;
        let Some(mut r) = self.sites[s].rupture else { return };
        r.emits += 1;
        self.sites[s].rupture = Some(r);
        let here = SITES[s].node;
        for k in 0..KINDS.len() {
            let c = KINDS[k].origin;
            let n = (self.sites[s].strain[c] * VENT_PM / 1000 / COST).max(0) as u64;
            if n == 0 {
                continue;
            }
            self.sites[s].strain[c] -= n as i64 * COST;
            let id = self.next_id;
            self.next_id += 1;
            let motion = self.onward(k, here, t);
            self.spawned += n;
            self.stats.created += 1;
            self.fx.push(Fx { at: t, what: "emit", pos: field::node(here), radius: spread(n), n, kind: k as u8 });
            match motion {
                Some(m) => {
                    self.cohorts.push(Cohort { id, kind: k as u8, count: n, hp: KINDS[k].hp, motion: m });
                    self.note(t, || format!("{} vents {} x{n}", SITES[s].name, KINDS[k].name));
                }
                None => {
                    // Nothing anywhere holds what it is made of: it is born with
                    // nothing to be, and the vent is only a flicker.
                    self.faded += n;
                    self.fx.push(Fx { at: t, what: "fade", pos: field::node(here), radius: spread(n), n, kind: k as u8 });
                }
            }
        }
    }

    /// What a manifestation of kind `k` standing at `node` does next.
    fn onward(&self, k: usize, node: usize, t: Tick) -> Option<Motion> {
        let g = self.target(k)?;
        let goal = SITES[g].node;
        if goal == node {
            Some(Motion::Assault { site: g as u8 })
        } else {
            let hop = field::roads().next[node][goal];
            Some(Motion::March { from: node as u8, to: hop as u8, since: t })
        }
    }

    fn arrive(&mut self, id: u32) {
        self.stats.arrivals += 1;
        let t = self.now;
        let Some(i) = self.cohorts.iter().position(|c| c.id == id) else { return };
        let Motion::March { to, .. } = self.cohorts[i].motion else { return };
        let k = self.cohorts[i].kind as usize;
        match self.onward(k, to as usize, t) {
            Some(m) => {
                if let Motion::Assault { site } = m {
                    let n = self.cohorts[i].count;
                    self.note(t, || format!("{} x{n} reach {}", KINDS[k].name, SITES[site as usize].name));
                }
                self.cohorts[i].motion = m;
            }
            None => {
                let c = self.cohorts.remove(i);
                self.faded += c.count;
                self.fx.push(Fx { at: t, what: "fade", pos: field::node(to as usize), radius: spread(c.count), n: c.count, kind: c.kind });
                self.note(t, || format!("{} x{} fade: nothing left of {}", KINDS[k].name, c.count, field::ORIGINS[KINDS[k].origin].tag()));
            }
        }
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
        self.effects.push(Effect::Band { at: t, structure: s, band });
        self.note(t, || format!("{} is {}", STRUCTURES[s].name, band.word()));
        if band != Band::Destroyed {
            return;
        }
        self.fx.push(Fx { at: t, what: "fall", pos: STRUCTURES[s].at, radius: 2_500, n: 0, kind: 0 });
        // A store that burns takes its stock with it: nothing left there is out
        // of its time.
        if let Some(site) = SITES.iter().position(|d| d.structure == s) {
            if let Some(c) = SITES[site].holds {
                self.sites[site].strain[c] = 0;
            }
            self.release(site);
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
                if !self.can_fire(b) {
                    // Lost its grid, or its pit, while laying.
                    self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
                    return;
                }
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
                self.note(t, || format!("{} volley fires ({} shells), lands t={lands}", def.name, def.count));
            }
            Duty::Idle { .. } | Duty::Reload { .. } => {
                if !self.can_fire(b) {
                    self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
                    return;
                }
                match self.choose(b, t) {
                    Some(ci) => {
                        let duty = self.lay(b, ci, t);
                        self.batteries[b].duty = duty;
                    }
                    None => self.batteries[b].duty = Duty::Idle { wake: Tick::MAX },
                }
            }
        }
    }

    /// The manifestation nearest to what it wants, in range.
    fn choose(&self, b: usize, t: Tick) -> Option<usize> {
        self.cohorts
            .iter()
            .enumerate()
            .filter(|(_, c)| self.in_range(b, c, t))
            .min_by_key(|(_, c)| (self.remaining(c, t), c.id))
            .map(|(i, _)| i)
    }

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
                    said.push(format!("#{} splits: hp {} x{}, hp {hp} x{k} (#{nid})", c.id, c.hp, c.count));
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
        self.fx.push(Fx { at: t, what: "impact", pos: v.to, radius: g.blast, n: kills, kind: 0 });
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
            if !self.can_fire(b) {
                self.batteries[b].duty = Duty::Idle { wake: Tick::MAX };
                continue;
            }
            let wake = self.cohorts.iter().map(|c| self.entry(b, c, now)).min().unwrap_or(Tick::MAX);
            let wake = if wake == now && self.choose(b, now).is_none() { now + 1 } else { wake };
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
        (0..self.batteries.len())
            .filter(|&b| self.standing(BATTERIES[b].pit))
            .map(|b| BATTERIES[b].count as u64)
            .sum()
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

    pub fn torn(&self) -> usize {
        self.sites.iter().filter(|s| s.rupture.is_some()).count()
    }

    /// Milli-tonnes crossed on a lane by tick `t`.
    pub fn crossed_at(&self, l: usize, t: Tick) -> u64 {
        self.crossed[l] + self.flow(l) * t.saturating_sub(self.crossed_since) / TICK_RATE
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
            .set("faded", Json::big(self.faded as u128))
            .set("lanes", Json::arr(self.lanes.iter().copied()))
            .set("crossed", Json::Arr(self.crossed.iter().map(|&c| Json::big(c as u128)).collect()))
            .set("crossedSince", self.crossed_since)
            .set("anchors", Json::arr(self.anchors.iter().copied()))
            .set("launder", self.launder)
            .set(
                "sites",
                Json::Arr(
                    self.sites
                        .iter()
                        .map(|s| {
                            let j = Json::obj()
                                .set("strain", Json::arr(s.strain))
                                .set("since", s.since);
                            match s.rupture {
                                Some(r) => j
                                    .set("torn", r.since)
                                    .set("emits", r.emits as u64)
                                    .set("source", r.source as u64)
                                    .set("band", r.band as u64),
                                None => j,
                            }
                        })
                        .collect(),
                ),
            )
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
                                Motion::March { from, to, since } => {
                                    j.set("from", from as u64).set("to", to as u64).set("since", since)
                                }
                                Motion::Assault { site } => j.set("site", site as u64),
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
                        .map(|s| Json::obj().set("hp", s.hp).set("since", s.since).set("band", s.band.word()))
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
        let big = |j: &Json| -> u64 { j.as_u64().or_else(|| j.as_str().and_then(|s| s.parse().ok())).unwrap_or(0) };
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
        f.faded = u(j, "faded")?;
        let lanes = j.at("lanes").as_arr();
        if lanes.len() != f.lanes.len() {
            return Err("fight: a different number of lanes".into());
        }
        for (l, v) in lanes.iter().enumerate() {
            f.lanes[l] = big(v);
        }
        for (l, v) in j.at("crossed").as_arr().iter().enumerate().take(f.crossed.len()) {
            f.crossed[l] = big(v);
        }
        f.crossed_since = u(j, "crossedSince")?;
        for (a, v) in j.at("anchors").as_arr().iter().enumerate().take(f.anchors.len()) {
            f.anchors[a] = v.as_bool().unwrap_or(false);
        }
        f.launder = j.at("launder").as_bool().unwrap_or(false);
        let sites = j.at("sites").as_arr();
        if sites.len() != f.sites.len() {
            return Err("fight: a different number of sites".into());
        }
        for (s, sj) in f.sites.iter_mut().zip(sites) {
            let st = sj.at("strain").as_arr();
            for c in 0..2 {
                s.strain[c] = st.get(c).and_then(Json::as_i128).unwrap_or(0) as i64;
            }
            s.since = u(sj, "since")?;
            s.rupture = if sj.at("torn").is_null() {
                None
            } else {
                Some(Rupture {
                    since: u(sj, "torn")?,
                    emits: u(sj, "emits")? as u32,
                    source: u(sj, "source")? as u8,
                    band: u(sj, "band")? as u8,
                })
            };
        }
        for c in j.at("cohorts").as_arr() {
            let motion = if c.at("site").is_null() {
                Motion::March { from: u(c, "from")? as u8, to: u(c, "to")? as u8, since: u(c, "since")? }
            } else {
                Motion::Assault { site: u(c, "site")? as u8 }
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
        Ok(f)
    }

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
                        .set("kind", f.kind as u64)
                })
                .collect(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_marching_cohort_is_where_its_closed_form_says() {
        let c = Cohort { id: 1, kind: 0, count: 10, hp: 4, motion: Motion::March { from: 0, to: 1, since: 100 } };
        assert_eq!(Fight::pos(&c, 100), field::node(0));
        let end = Fight::arrival(&c).unwrap();
        assert_eq!(Fight::pos(&c, end), field::node(1));
    }

    #[test]
    fn identical_states_merge() {
        let mut f = Fight::new();
        let m = Motion::Assault { site: 1 };
        f.cohorts.push(Cohort { id: 4, kind: 0, count: 10, hp: 2, motion: m });
        f.cohorts.push(Cohort { id: 9, kind: 0, count: 5, hp: 2, motion: m });
        f.cohorts.push(Cohort { id: 7, kind: 0, count: 5, hp: 4, motion: m });
        f.merge();
        assert_eq!(f.cohorts.len(), 2);
        assert_eq!((f.cohorts[0].id, f.cohorts[0].count), (4, 15));
    }

    #[test]
    fn nothing_crossing_means_nothing_happens() {
        let mut f = Fight::new();
        assert_eq!(f.next(), None);
        f.advance(10 * 60 * TICK_RATE);
        assert!(f.quiet() && f.stats.events == 0);
    }

    #[test]
    fn the_first_tear_is_solved_not_found() {
        let mut f = Fight::new();
        f.stream(0, 10);
        let (t, class, which) = f.next().unwrap();
        assert_eq!((class, which), (EV_STRAIN, 0), "the gantry tears first");
        // Rate at the gantry: 10 t/s x 147 years x 300/1000, per tick.
        let rate = 10_000 * 147 * GANTRY_SHARE / 60_000;
        assert_eq!(t, ceil_div(SITES[0].open, rate) as Tick);
        f.advance(t);
        assert!(f.sites[0].rupture.is_some());
        assert!(f.total(0) >= SITES[0].open);
    }
}
