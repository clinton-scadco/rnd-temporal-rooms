//! The encounter: one fight, four factory sectors, one clock, and a log that
//! holds only what a player did.
//!
//! # What is written down
//!
//! ```text
//!   seed          the scenario: which packets, how big, when
//!   commands      (tick, send a wave | repair a structure | hold a battery)
//!   checkpoints   every thirty simulated seconds: the compact state
//! ```
//!
//! and nothing else. Every volley, impact, split, merge, breach and edit is a
//! consequence of those, recomputed by anybody who replays them -- and
//! `replay` and `resume` exist so that claim is a test rather than a hope: the
//! state reached by replaying the log from tick 0, the state reached by resuming
//! from a checkpoint half-way through a fight, and the state the live encounter
//! is in are hashed and compared.
//!
//! # The domain
//!
//! Closed until a wave is sent. Opening it wakes the sectors that stand inside
//! the combat rectangle; the others are never told. It closes `SETTLE` ticks
//! after the fight goes quiet -- nothing alive, nothing in the air, nothing
//! still to be released -- and closing it collapses every woken sector back
//! into an orbit.

use super::factory::{Mode, Sector};
use super::field::{self, Rect, BATTERIES, KINDS, SECTORS, STRUCTURES};
use super::fight::{self, Band, Duty, Fight, Motion};
use super::{clock, commas, fnv, Rng, P, TICK_RATE};
use crate::graph::{Graph, Kind};
use crate::json::{self, Json};
use crate::live::{Carry, Edit};
use crate::model::Tick;

/// Quiet for this long and the domain closes.
pub const SETTLE: Tick = 3 * TICK_RATE;
pub const CHECKPOINT_EVERY: Tick = 30 * TICK_RATE;
/// A wave arrives this long after it is sent.
pub const WAVE_LEAD: Tick = TICK_RATE;
const NOTES: usize = 40;
const KEEP_CHECKPOINTS: usize = 12;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    Wave { nominal: u64 },
    Repair { structure: usize },
    Hold { battery: usize, hold: bool },
}

#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Command {
    pub at: Tick,
    pub cmd: Cmd,
}

impl Command {
    pub fn to_json(&self) -> Json {
        let j = Json::obj().set("at", self.at);
        match &self.cmd {
            Cmd::Wave { nominal } => j.set("op", "wave").set("n", Json::big(*nominal as u128)),
            Cmd::Repair { structure } => j.set("op", "repair").set("s", STRUCTURES[*structure].name),
            Cmd::Hold { battery, hold } => {
                j.set("op", "hold").set("b", BATTERIES[*battery].name).set("on", *hold)
            }
        }
    }

    pub fn from_json(j: &Json) -> Result<Command, String> {
        let at = j.at("at").as_u64().ok_or("a command has no tick")?;
        let cmd = match j.at("op").as_str().unwrap_or("") {
            "wave" => Cmd::Wave {
                nominal: j
                    .at("n")
                    .as_u64()
                    .or_else(|| j.at("n").as_str().and_then(|s| s.parse().ok()))
                    .ok_or("a wave with no size")?,
            },
            "repair" => Cmd::Repair { structure: structure(j.at("s").as_str().unwrap_or(""))? },
            "hold" => Cmd::Hold {
                battery: battery(j.at("b").as_str().unwrap_or(""))?,
                hold: j.at("on").as_bool().unwrap_or(true),
            },
            other => return Err(format!("unknown command `{other}`")),
        };
        Ok(Command { at, cmd })
    }
}

pub fn structure(name: &str) -> Result<usize, String> {
    STRUCTURES
        .iter()
        .position(|s| s.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("there is no structure called `{name}`"))
}

pub fn battery(name: &str) -> Result<usize, String> {
    BATTERIES
        .iter()
        .position(|b| b.name.eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("there is no battery called `{name}`"))
}

/// The compact state at one tick. What a late joiner, a save file or a
/// desync investigation would be handed -- and it contains no events.
#[derive(Clone, Debug)]
pub struct Checkpoint {
    pub tick: Tick,
    pub json: String,
    pub hash: u64,
}

/// One opening and closing of the domain, for the record. Derived.
#[derive(Clone, Debug, Default)]
pub struct Episode {
    pub opened: Tick,
    pub closed: Option<Tick>,
    pub woke: Vec<&'static str>,
    pub recompiled: Vec<&'static str>,
    pub events_before: u64,
    pub events: u64,
    pub peak_cohorts: usize,
}

pub struct Encounter {
    pub seed: u64,
    pub log: Vec<Command>,
    pub fight: Fight,
    pub sectors: Vec<Sector>,
    pub now: Tick,
    pub open: bool,
    pub quiet: Option<Tick>,
    pub waves: u32,
    last_ck: Tick,

    pub episodes: Vec<Episode>,
    pub checkpoints: Vec<Checkpoint>,
    pub checkpoints_taken: u64,
    pub checkpoint_bytes: u64,
    pub rendezvous: u64,
    pub notes: Vec<(Tick, String)>,
}

impl Encounter {
    pub fn new(seed: u64) -> Encounter {
        Encounter {
            seed,
            log: Vec::new(),
            fight: Fight::new(),
            sectors: SECTORS.iter().map(Sector::open).collect(),
            now: 0,
            open: false,
            quiet: None,
            waves: 0,
            last_ck: 0,
            episodes: Vec::new(),
            checkpoints: Vec::new(),
            checkpoints_taken: 0,
            checkpoint_bytes: 0,
            rendezvous: 0,
            notes: Vec::new(),
        }
    }

    fn note(&mut self, t: Tick, s: String) {
        self.notes.push((t, s));
        if self.notes.len() > NOTES {
            self.notes.remove(0);
        }
    }

    /// Sectors whose ground the fight can reach.
    pub fn inside(i: usize) -> bool {
        Rect::of_tiles(SECTORS[i].rect).overlaps(&field::domain())
    }

    // ------------------------------------------------------------ the clock

    /// Bring everything to tick `t`: every fight event up to it, the factory's
    /// reaction to each at its own tick, the domain closing if it settles, and
    /// any checkpoint that falls due on the way.
    pub fn advance_to(&mut self, t: Tick) {
        if t < self.now {
            return;
        }
        loop {
            let fe = self.fight.next_tick();
            let close = if self.open { self.quiet.map(|q| q + SETTLE) } else { None };
            let ck = Some(self.last_ck + CHECKPOINT_EVERY);
            let mut best: Option<(Tick, u8)> = None;
            for (when, k) in [(fe, 0u8), (close, 1), (ck, 2)] {
                if let Some(w) = when {
                    if w <= t && best.map_or(true, |b| (w, k) < b) {
                        best = Some((w, k));
                    }
                }
            }
            let Some((w, k)) = best else { break };
            match k {
                0 => self.fight_event(),
                1 => self.close(w),
                _ => self.checkpoint(w),
            }
        }
        self.fight.advance(t);
        for s in &mut self.sectors {
            s.advance(t);
        }
        self.now = t;
    }

    fn fight_event(&mut self) {
        let Some(t) = self.fight.step() else { return };
        self.react(t);
        if self.fight.quiet() {
            if self.quiet.is_none() && self.open {
                self.quiet = Some(t);
                let (k, l) = (self.fight.killed, self.fight.leaked);
                self.note(t, format!("the fight is over: {} killed, {} got through", commas(k as u128), commas(l as u128)));
            }
        } else {
            self.quiet = None;
        }
        if let Some(ep) = self.episodes.last_mut() {
            if ep.closed.is_none() {
                ep.peak_cohorts = ep.peak_cohorts.max(self.fight.cohorts.len());
            }
        }
    }

    /// The factory hears about structures that crossed a band -- at the tick
    /// they crossed it, which is the only reason a woken sector is stepped on
    /// the fight's clock at all.
    fn react(&mut self, t: Tick) {
        let effects: Vec<fight::Effect> = std::mem::take(&mut self.fight.effects);
        for e in effects {
            let def = &STRUCTURES[e.structure];
            self.note(e.at, format!("{} is {}", def.name, e.band.word()));
            let Some(tie) = def.tie else { continue };
            let edits = {
                let s = &self.sectors[tie.sector];
                wanted(&s.base, &s.graph, tie.node, e.band)
            };
            if edits.is_empty() {
                continue;
            }
            let name = SECTORS[tie.sector].name;
            match self.sectors[tie.sector].edit(t, &edits) {
                Ok(()) => {
                    self.rendezvous += 1;
                    let what: Vec<String> =
                        edits.iter().map(|e| format!("{} {}", e.verb(), e.subject())).collect();
                    self.note(t, format!("{name} recompiled: {}", what.join(", ")));
                    if let Some(ep) = self.episodes.last_mut() {
                        if !ep.recompiled.contains(&name) {
                            ep.recompiled.push(name);
                        }
                    }
                }
                Err(err) => self.note(t, format!("{name} refused an edit: {err}")),
            }
        }
    }

    fn open_domain(&mut self) {
        if self.open {
            return;
        }
        self.open = true;
        let t = self.now;
        let mut ep = Episode { opened: t, events_before: self.fight.stats.events, ..Episode::default() };
        for i in 0..self.sectors.len() {
            if Self::inside(i) {
                self.sectors[i].wake(t);
                ep.woke.push(SECTORS[i].name);
            }
        }
        self.note(t, format!("combat domain opens; {} wake", ep.woke.join(" and ")));
        self.episodes.push(ep);
    }

    fn close(&mut self, t: Tick) {
        self.fight.advance(t);
        let mut stuck = Vec::new();
        for s in &mut self.sectors {
            if s.awake() && !s.close(t) {
                stuck.push(s.spec.name);
            }
        }
        self.open = false;
        self.quiet = None;
        let events = self.fight.stats.events;
        if let Some(ep) = self.episodes.last_mut() {
            ep.closed = Some(t);
            ep.events = events - ep.events_before;
        }
        let msg = if stuck.is_empty() {
            "combat domain settles; every sector collapses back into its orbit".to_string()
        } else {
            format!("combat domain settles; {} found no orbit and stays awake", stuck.join(", "))
        };
        self.note(t, msg);
    }

    fn checkpoint(&mut self, t: Tick) {
        self.fight.advance(t);
        for s in &mut self.sectors {
            s.advance(t);
        }
        self.now = t;
        self.last_ck = t;
        let j = self.state_json();
        let json = j.to_string();
        let hash = fnv(json.as_bytes());
        self.checkpoints_taken += 1;
        self.checkpoint_bytes += json.len() as u64;
        self.checkpoints.push(Checkpoint { tick: t, json, hash });
        if self.checkpoints.len() > KEEP_CHECKPOINTS {
            self.checkpoints.remove(0);
        }
    }

    // ------------------------------------------------------------- commands

    /// Apply a command now, and write it down. The only thing that is.
    pub fn apply(&mut self, cmd: Cmd) -> Result<(), String> {
        let t = self.now;
        match &cmd {
            Cmd::Wave { nominal } => {
                if *nominal == 0 {
                    return Err("a wave of nobody".into());
                }
                self.waves += 1;
                let seed = Rng(self.seed ^ (self.waves as u64).wrapping_mul(0x9e37_79b9_7f4a_7c15)).next();
                self.fight.wave(seed, *nominal, t + WAVE_LEAD);
                self.quiet = None;
                self.note(t, format!("wave {} sent: {} attackers", self.waves, commas(*nominal as u128)));
                self.open_domain();
            }
            Cmd::Repair { structure } => {
                let s = *structure;
                if self.fight.hp_at(s, t) == STRUCTURES[s].hp * 1000 {
                    return Err(format!("{} is not damaged", STRUCTURES[s].name));
                }
                self.fight.repair(s);
                self.note(t, format!("{} repaired", STRUCTURES[s].name));
                self.react(t);
            }
            Cmd::Hold { battery, hold } => {
                self.fight.hold(*battery, *hold);
                let verb = if *hold { "holds fire" } else { "is released" };
                self.note(t, format!("{} {verb}", BATTERIES[*battery].name));
            }
        }
        self.log.push(Command { at: t, cmd });
        Ok(())
    }

    // ------------------------------------------------------ reconstruction

    /// Everything from tick 0, from nothing but the seed and the commands.
    pub fn replay(seed: u64, log: &[Command], until: Tick) -> Encounter {
        let mut e = Encounter::new(seed);
        for c in log {
            e.advance_to(c.at);
            let _ = e.apply(c.cmd.clone());
        }
        e.advance_to(until);
        e
    }

    /// From a checkpoint, and the commands that came after it.
    pub fn resume(seed: u64, cp: &Checkpoint, log: &[Command], until: Tick) -> Result<Encounter, String> {
        let j = json::parse(&cp.json)?;
        let mut e = Encounter::new(seed);
        e.fight = Fight::from_json(j.at("fight"))?;
        e.now = cp.tick;
        e.last_ck = cp.tick;
        e.open = j.at("open").as_bool().unwrap_or(false);
        e.quiet = j.at("quiet").as_u64();
        e.waves = j.at("waves").as_u64().unwrap_or(0) as u32;
        let sj = j.at("sectors").as_arr();
        if sj.len() != SECTORS.len() {
            return Err("a checkpoint with a different factory".into());
        }
        for (i, s) in sj.iter().enumerate() {
            let graph = Graph::from_json(s.at("graph"))?;
            let carry = Carry::from_json(s.at("carry"))?;
            let awake = s.at("awake").as_bool().unwrap_or(false);
            e.sectors[i] = Sector::restore(&SECTORS[i], graph, &carry, awake, cp.tick)?;
        }
        e.log = log.iter().filter(|c| c.at < cp.tick).cloned().collect();
        for c in log.iter().filter(|c| c.at >= cp.tick) {
            e.advance_to(c.at);
            let _ = e.apply(c.cmd.clone());
        }
        e.advance_to(until);
        Ok(e)
    }

    /// The state, and only the state: what a checkpoint holds.
    pub fn state_json(&self) -> Json {
        Json::obj()
            .set("tick", self.now)
            .set("seed", Json::big(self.seed as u128))
            .set("open", self.open)
            .set("quiet", self.quiet)
            .set("waves", self.waves as u64)
            .set("fight", self.fight.to_json())
            .set(
                "sectors",
                Json::Arr(
                    self.sectors
                        .iter()
                        .map(|s| {
                            Json::obj()
                                .set("name", s.spec.name)
                                .set("awake", s.awake())
                                .set("graph", s.graph.to_json())
                                .set("carry", s.carry(self.now).to_json())
                        })
                        .collect(),
                ),
            )
    }

    /// One number for the whole encounter at `now`, independent of how the
    /// run got here: structures' hp as of now rather than as of whenever it
    /// was last written, and sectors by their state rather than their mode.
    pub fn hash(&self) -> u64 {
        let mut f = self.fight.clone();
        for s in 0..f.structures.len() {
            f.structures[s].hp = self.fight.hp_at(s, self.now);
            f.structures[s].since = self.now;
        }
        let mut bytes = f.to_json().to_string().into_bytes();
        bytes.extend_from_slice(format!("{}|{:?}|{}", self.open, self.quiet, self.waves).as_bytes());
        for s in &self.sectors {
            bytes.extend_from_slice(s.graph.emit().as_bytes());
            bytes.extend_from_slice(&s.carry(self.now).signature());
        }
        fnv(&bytes)
    }

    /// The log as it would be saved: the scenario and the commands.
    pub fn log_json(&self) -> Json {
        Json::obj()
            .set("seed", Json::big(self.seed as u128))
            .set("commands", Json::Arr(self.log.iter().map(Command::to_json).collect()))
    }

    // ---------------------------------------------------------------- view

    /// Nominal things inside the combat domain, and the records that stand for
    /// them.
    pub fn overlay(&self) -> Json {
        let f = &self.fight;
        let attackers = f.alive_count();
        let shells = f.shells_in_flight();
        let turrets = f.turrets_alive();
        let standing = f.standing_structures() as u64;
        let machines: u64 = (0..self.sectors.len())
            .filter(|&i| Self::inside(i))
            .map(|i| self.sectors[i].machines())
            .sum();
        let alive_batteries = (0..BATTERIES.len())
            .filter(|&b| f.structures[BATTERIES[b].pit].hp > 0)
            .count();
        let sector_cells: usize = (0..self.sectors.len())
            .filter(|&i| Self::inside(i) && self.sectors[i].awake())
            .map(|i| self.sectors[i].with_pop(self.now, |p| p.distinct_states()))
            .sum();
        Json::obj()
            .set("open", self.open)
            .set(
                "nominal",
                Json::big((attackers + shells + turrets + standing + machines) as u128),
            )
            .set("attackers", Json::big(attackers as u128))
            .set("turrets", turrets)
            .set("shells", shells)
            .set("machines", Json::big(machines as u128))
            .set("cohorts", f.cohorts.len())
            .set("batteries", alive_batteries)
            .set("volleys", f.volleys.len())
            .set("structures", standing)
            .set("sectorCells", sector_cells)
            .set("events", f.stats.events)
            .set("created", f.stats.created)
            .set("splits", f.stats.splits)
            .set("merges", f.stats.merges)
            .set("peakCohorts", f.stats.peak_cohorts)
            .set("spawned", Json::big(f.spawned as u128))
            .set("killed", Json::big(f.killed as u128))
            .set("leaked", Json::big(f.leaked as u128))
            .set("rendezvous", self.rendezvous)
    }

    /// Everything a view draws at `now`, in one answer.
    pub fn frame(&self) -> Json {
        let f = &self.fight;
        let t = self.now;
        let p = |p: P| Json::arr([p.x, p.y]);
        let cohorts = f
            .cohorts
            .iter()
            .map(|c| {
                let k = &KINDS[c.kind as usize];
                let j = Json::obj()
                    .set("id", c.id as u64)
                    .set("kind", k.name)
                    .set("count", Json::big(c.count as u128))
                    .set("hp", c.hp as u64)
                    .set("hpMax", k.hp as u64)
                    .set("speed", k.speed)
                    .set("spread", fight::spread(c.count))
                    .set("pos", p(Fight::pos(c, t)));
                match c.motion {
                    Motion::March { leg, since } => j.set("leg", leg as u64).set("since", since),
                    Motion::Assault { node } => j.set("node", node as u64),
                }
            })
            .collect();
        let batteries = f
            .batteries
            .iter()
            .enumerate()
            .map(|(b, bat)| {
                let alive = f.structures[BATTERIES[b].pit].hp > 0;
                let j = Json::obj()
                    .set("name", BATTERIES[b].name)
                    .set("alive", alive)
                    .set("hold", bat.hold)
                    .set("heading", bat.heading as i64)
                    .set("lastFire", bat.last_fire)
                    .set("volleys", bat.volleys);
                match bat.duty {
                    Duty::Laying { from, to, since, fire, .. } => j
                        .set("duty", "laying")
                        .set("from", from as i64)
                        .set("to", to as i64)
                        .set("since", since)
                        .set("fire", fire),
                    Duty::Reload { until } => j.set("duty", "reload").set("until", until),
                    Duty::Idle { .. } => j.set("duty", "idle"),
                }
            })
            .collect();
        let volleys = f
            .volleys
            .iter()
            .map(|v| {
                Json::obj()
                    .set("id", v.id as u64)
                    .set("gun", field::gun(BATTERIES[v.battery as usize].gun).name)
                    .set("shells", v.shells as u64)
                    .set("from", p(v.from))
                    .set("to", p(v.to))
                    .set("fired", v.fired)
                    .set("lands", v.lands)
            })
            .collect();
        let structures = (0..f.structures.len())
            .map(|s| {
                let hp = f.hp_at(s, t);
                let max = STRUCTURES[s].hp * 1000;
                Json::obj()
                    .set("name", STRUCTURES[s].name)
                    .set("hp", hp)
                    .set("max", max)
                    .set("band", Band::of(hp, max).word())
            })
            .collect();
        let latest = self.checkpoints.last();
        Json::obj()
            .set("ok", true)
            .set("tick", t)
            .set("clock", clock(t))
            .set(
                "domain",
                Json::obj()
                    .set("open", self.open)
                    .set("opened", self.episodes.last().filter(|_| self.open).map(|e| e.opened))
                    .set("quiet", self.quiet)
                    .set("closes", if self.open { self.quiet.map(|q| q + SETTLE) } else { None }),
            )
            .set("cohorts", Json::Arr(cohorts))
            .set("batteries", Json::Arr(batteries))
            .set("volleys", Json::Arr(volleys))
            .set("structures", Json::Arr(structures))
            .set("fx", f.fx_json())
            .set(
                "sectors",
                Json::Arr(
                    self.sectors
                        .iter()
                        .enumerate()
                        .map(|(i, s)| s.to_json(t).set("inside", Self::inside(i)))
                        .collect(),
                ),
            )
            .set("overlay", self.overlay())
            .set(
                "notes",
                Json::Arr(
                    self.notes
                        .iter()
                        .rev()
                        .take(14)
                        .map(|(t, s)| Json::obj().set("at", *t).set("text", s.clone()))
                        .collect(),
                ),
            )
            .set(
                "log",
                Json::obj()
                    .set("commands", self.log.len())
                    .set("bytes", self.log_json().to_string().len())
                    .set("checkpoints", self.checkpoints_taken)
                    .set("checkpointBytes", latest.map(|c| c.json.len() as u64))
                    .set("lastCheckpoint", latest.map(|c| c.tick))
                    .set("derived", self.fight.stats.events)
                    .set("serializedEvents", 0u64),
            )
            .set(
                "episodes",
                Json::Arr(
                    self.episodes
                        .iter()
                        .rev()
                        .take(4)
                        .map(|e| {
                            Json::obj()
                                .set("opened", e.opened)
                                .set("closed", e.closed)
                                .set("woke", Json::arr(e.woke.iter().copied()))
                                .set("recompiled", Json::arr(e.recompiled.iter().copied()))
                                .set("events", e.events)
                                .set("peakCohorts", e.peak_cohorts)
                        })
                        .collect(),
                ),
            )
    }

    /// Whether the sectors stand in the same state as `other`'s, one by one.
    pub fn same_sectors(&self, other: &Encounter, t: Tick) -> Vec<bool> {
        self.sectors
            .iter()
            .zip(&other.sectors)
            .map(|(a, b)| {
                a.graph.emit() == b.graph.emit() && a.carry(t).signature() == b.carry(t).signature()
            })
            .collect()
    }

    pub fn closed_count(&self) -> usize {
        self.sectors.iter().filter(|s| matches!(s.mode, Mode::Closed(_))).count()
    }
}

/// The edits that take a sector's `node` from what it is to what `band` says
/// it should be. Empty when it already is.
///
/// A link runs or it does not: a damaged conveyor still carries ore, a
/// destroyed one carries nothing. A machine hall loses furnaces with each band,
/// because a building on fire does not smelt at full strength and does not
/// smelt at zero either.
pub fn wanted(base: &Graph, now: &Graph, node: &str, band: Band) -> Vec<Edit> {
    let Some(b) = base.node(node) else { return Vec::new() };
    let want = match (b.kind, band) {
        (_, Band::Destroyed) => None,
        (Kind::Link, _) => Some(b.count),
        (_, Band::Intact) => Some(b.count),
        (_, Band::Damaged) => Some((b.count * 3 / 4).max(1)),
        (_, Band::Critical) => Some((b.count / 2).max(1)),
    };
    // The bays this node fills. Taking the node away must not take their
    // contents with it: a hopper under a torn-up conveyor still has the ore that
    // already arrived, and the smelters drain it before they starve. So each
    // one first gets a `holds` slot for what the node delivered -- the clause
    // the language already has for a bay that is filled from somewhere this
    // document cannot see -- and gets its as-built form back when the node does.
    let fed: Vec<&str> = base
        .edges
        .iter()
        .filter(|e| e.from == node)
        .map(|e| e.to.as_str())
        .filter(|s| base.node(s).is_some_and(|n| n.kind == Kind::Storage))
        .collect();
    let delivers: Vec<String> = b.outputs.iter().chain(b.moved()).map(|a| a.item.clone()).collect();
    match (now.node(node), want) {
        (None, None) => Vec::new(),
        (Some(_), None) => {
            let mut out = Vec::new();
            for s in &fed {
                let Some(cur) = now.node(s) else { continue };
                let mut bay = cur.clone();
                for it in &delivers {
                    if !bay.holds.contains(it) {
                        bay.holds.push(it.clone());
                    }
                }
                if bay.holds != cur.holds {
                    out.push(Edit::Retune(bay));
                }
            }
            out.push(Edit::Remove(node.to_string()));
            out
        }
        (None, Some(n)) => {
            let mut node_ = b.clone();
            node_.count = n;
            let mut out = vec![Edit::Place(node_)];
            for e in base.edges.iter().filter(|e| e.from == node || e.to == node) {
                out.push(Edit::Wire { from: e.from.clone(), to: e.to.clone(), item: e.item.clone() });
            }
            for s in &fed {
                if let (Some(cur), Some(built)) = (now.node(s), base.node(s)) {
                    if cur.holds != built.holds {
                        out.push(Edit::Retune(built.clone()));
                    }
                }
            }
            out
        }
        (Some(cur), Some(n)) if cur.count != n => {
            let mut node_ = cur.clone();
            node_.count = n;
            vec![Edit::Retune(node_)]
        }
        _ => Vec::new(),
    }
}
