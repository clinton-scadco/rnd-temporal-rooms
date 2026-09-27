//! The encounter: one district, two lanes through one gantry, four factory
//! sectors, one clock, and a log that holds only what a player did.
//!
//! # What is written down
//!
//! ```text
//!   seed          the scenario (kept for the file format; nothing is random
//!                 any more -- a disturbance is caused, not rolled)
//!   commands      (tick, stream a lane | anchor | launder | repair | hold)
//!   checkpoints   every thirty simulated seconds: the compact state
//! ```
//!
//! and nothing else. Every tonne that crossed, every tear, vent, split, merge,
//! fade and edit is a consequence of those, recomputed by anybody who replays
//! them -- and `replay` and `resume` exist so that claim is a test.
//!
//! # The domain
//!
//! Closed until something tears. A tear wakes only the sectors that rupture
//! could ever reach -- the circle of its widest bleed, not a rectangle somebody
//! drew -- and the others are never told. It closes `SETTLE` ticks after the
//! district goes quiet: nothing torn, nothing manifest, nothing in the air.

use super::factory::{Mode, Sector};
use super::field::{self, ANCHORS, BATTERIES, KINDS, LANES, SECTORS, SITES, STRUCTURES};
use super::fight::{self, Band, Duty, Effect, Fight, Motion};
use super::{clock, commas, fnv, P, TICK_RATE};
use crate::graph::{Graph, Kind};
use crate::json::{self, Json};
use crate::live::{Carry, Edit};
use crate::model::Tick;

pub const SETTLE: Tick = 3 * TICK_RATE;
pub const CHECKPOINT_EVERY: Tick = 30 * TICK_RATE;
const NOTES: usize = 40;
const KEEP_CHECKPOINTS: usize = 12;
/// Tonnes a second a lane may be asked for. Past this it is a different
/// experiment.
pub const MAX_RATE: u64 = 1_000_000;

#[derive(Clone, Debug, PartialEq, Eq)]
pub enum Cmd {
    Stream { lane: usize, rate: u64 },
    Anchor { anchor: usize, on: bool },
    Launder { on: bool },
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
            Cmd::Stream { lane, rate } => {
                j.set("op", "stream").set("lane", LANES[*lane].tag).set("rate", Json::big(*rate as u128))
            }
            Cmd::Anchor { anchor, on } => j.set("op", "anchor").set("a", ANCHORS[*anchor].name).set("on", *on),
            Cmd::Launder { on } => j.set("op", "launder").set("on", *on),
            Cmd::Repair { structure } => j.set("op", "repair").set("s", STRUCTURES[*structure].name),
            Cmd::Hold { battery, hold } => {
                j.set("op", "hold").set("b", BATTERIES[*battery].name).set("on", *hold)
            }
        }
    }

    pub fn from_json(j: &Json) -> Result<Command, String> {
        let at = j.at("at").as_u64().ok_or("a command has no tick")?;
        let on = j.at("on").as_bool().unwrap_or(true);
        let cmd = match j.at("op").as_str().unwrap_or("") {
            "stream" => Cmd::Stream {
                lane: lane(j.at("lane").as_str().unwrap_or(""))?,
                rate: j
                    .at("rate")
                    .as_u64()
                    .or_else(|| j.at("rate").as_str().and_then(|s| s.parse().ok()))
                    .ok_or("a stream with no rate")?,
            },
            "anchor" => Cmd::Anchor { anchor: anchor(j.at("a").as_str().unwrap_or(""))?, on },
            "launder" => Cmd::Launder { on },
            "repair" => Cmd::Repair { structure: structure(j.at("s").as_str().unwrap_or(""))? },
            "hold" => Cmd::Hold { battery: battery(j.at("b").as_str().unwrap_or(""))?, hold: on },
            other => return Err(format!("unknown command `{other}`")),
        };
        Ok(Command { at, cmd })
    }
}

fn find<T>(list: &[T], name: &str, of: impl Fn(&T) -> &str, what: &str) -> Result<usize, String> {
    list.iter()
        .position(|x| of(x).eq_ignore_ascii_case(name))
        .ok_or_else(|| format!("there is no {what} called `{name}`"))
}

pub fn structure(name: &str) -> Result<usize, String> {
    find(STRUCTURES, name, |s| s.name, "structure")
}
pub fn battery(name: &str) -> Result<usize, String> {
    find(BATTERIES, name, |b| b.name, "battery")
}
pub fn anchor(name: &str) -> Result<usize, String> {
    find(ANCHORS, name, |a| a.name, "anchor")
}
pub fn lane(tag: &str) -> Result<usize, String> {
    find(LANES, tag, |l| l.tag, "lane")
}

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
    pub tore: Vec<&'static str>,
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
    last_ck: Tick,
    /// Whether the interface was dark when last looked at. Derived, for notes.
    was_dark: bool,

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
            last_ck: 0,
            was_dark: false,
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

    /// Whether any rupture torn right now could reach sector `i`.
    pub fn inside(&self, i: usize) -> bool {
        (0..SITES.len()).any(|s| self.fight.sites[s].rupture.is_some() && field::reaches(s, i))
    }

    // ------------------------------------------------------------ the clock

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
                let (k, f) = (self.fight.killed, self.fight.faded);
                self.note(
                    t,
                    format!("the district is quiet: {} destroyed, {} faded with nothing to take back", commas(k as u128), commas(f as u128)),
                );
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

    /// The factory hears about what the disturbance did, at the tick it did it.
    fn react(&mut self, t: Tick) {
        let effects: Vec<Effect> = std::mem::take(&mut self.fight.effects);
        for e in effects {
            match e {
                Effect::Band { at, structure, band } => {
                    self.note(at, format!("{} is {}", STRUCTURES[structure].name, band.word()));
                }
                Effect::Tear { at, site } => self.tear(at, site),
                Effect::Seal { at, site } => {
                    self.note(at, format!("{} seals", SITES[site].name));
                }
                Effect::Bleed { at, site, band } => {
                    let r = field::BLEED_R[band as usize] / super::MT;
                    let pinned = if self.fight.pinned(site) { " -- pinned, so it does not" } else { "" };
                    self.note(at, format!("{} bleeds {r} tiles into {}{pinned}", SITES[site].name, self.source_of(site)));
                }
            }
        }
        let dark = self.fight.dark();
        if dark != self.was_dark {
            self.was_dark = dark;
            let msg = if dark {
                if self.fight.standing(field::GANTRY) {
                    "the interface goes dark: the gantry is standing in 1890, and 1890 has no grid"
                } else {
                    "the interface goes dark: the gantry is down"
                }
            } else {
                "the interface lights again"
            };
            self.note(t, msg.into());
        }
        self.reconcile(t);
    }

    fn source_of(&self, site: usize) -> &'static str {
        self.fight.sites[site]
            .rupture
            .map(|r| field::ORIGINS[r.source as usize].tag())
            .unwrap_or("2037")
    }

    fn tear(&mut self, t: Tick, site: usize) {
        let src = self.source_of(site);
        if !self.open {
            self.open = true;
            self.quiet = None;
            self.episodes.push(Episode { opened: t, events_before: self.fight.stats.events, ..Episode::default() });
        }
        let mut woke = Vec::new();
        for i in 0..self.sectors.len() {
            if field::reaches(site, i) && !self.sectors[i].awake() {
                self.sectors[i].wake(t);
                woke.push(SECTORS[i].name);
            }
        }
        if let Some(ep) = self.episodes.last_mut() {
            ep.tore.push(SITES[site].name);
            for w in &woke {
                if !ep.woke.contains(w) {
                    ep.woke.push(w);
                }
            }
        }
        let wake = if woke.is_empty() { String::new() } else { format!("; {} wake", woke.join(" and ")) };
        self.note(t, format!("{} tears into {src}{wake}", SITES[site].name));
    }

    /// Make every tied sector node what its structure -- and, for the gantry,
    /// the grid -- says it should be. Idempotent: empty edits cost nothing.
    fn reconcile(&mut self, t: Tick) {
        let dark = self.fight.dark();
        for (s, def) in STRUCTURES.iter().enumerate() {
            let Some(tie) = def.tie else { continue };
            let band = if s == field::GANTRY && dark { Band::Destroyed } else { self.fight.structures[s].band };
            let edits = {
                let sec = &self.sectors[tie.sector];
                wanted(&sec.base, &sec.graph, tie.node, band)
            };
            if edits.is_empty() {
                continue;
            }
            let name = SECTORS[tie.sector].name;
            match self.sectors[tie.sector].edit(t, &edits) {
                Ok(()) => {
                    self.rendezvous += 1;
                    let what: Vec<String> = edits.iter().map(|e| format!("{} {}", e.verb(), e.subject())).collect();
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
            "the disturbance settles; every woken sector collapses back into its orbit".to_string()
        } else {
            format!("the disturbance settles; {} found no orbit and stays awake", stuck.join(", "))
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
            Cmd::Stream { lane, rate } => {
                if *rate > MAX_RATE {
                    return Err("more than a million tonnes a second is a different experiment".into());
                }
                if self.fight.lanes[*lane] == *rate {
                    return Err(format!("the {} is already carrying that", LANES[*lane].name));
                }
                self.fight.stream(*lane, *rate);
                let l = &LANES[*lane];
                let msg = if *rate == 0 {
                    format!("{} closed", l.name)
                } else {
                    format!("{}: {} t/s of {} ({} years out of time)", l.name, commas(*rate as u128), l.item, l.years())
                };
                self.note(t, msg);
            }
            Cmd::Anchor { anchor, on } => {
                if self.fight.anchors[*anchor] == *on {
                    return Err(format!("{} is already {}", ANCHORS[*anchor].name, if *on { "on" } else { "off" }));
                }
                self.fight.anchor(*anchor, *on);
                self.note(t, format!("{} {}", ANCHORS[*anchor].name, if *on { "powered" } else { "off" }));
            }
            Cmd::Launder { on } => {
                if self.fight.launder == *on {
                    return Err(format!("laundering is already {}", if *on { "on" } else { "off" }));
                }
                self.fight.set_launder(*on);
                self.note(
                    t,
                    if *on {
                        "laundering: 1890 ore goes straight to the crusher and leaves as 2037 concentrate".into()
                    } else {
                        "laundering off: 1890 ore is stacked in the yard again".into()
                    },
                );
            }
            Cmd::Repair { structure } => {
                let s = *structure;
                if self.fight.hp_at(s, t) == STRUCTURES[s].hp * 1000 {
                    return Err(format!("{} is not damaged", STRUCTURES[s].name));
                }
                self.fight.repair(s);
                self.note(t, format!("{} repaired", STRUCTURES[s].name));
            }
            Cmd::Hold { battery, hold } => {
                self.fight.hold(*battery, *hold);
                let verb = if *hold { "holds fire" } else { "is released" };
                self.note(t, format!("{} {verb}", BATTERIES[*battery].name));
            }
        }
        self.react(t);
        self.log.push(Command { at: t, cmd });
        Ok(())
    }

    // ------------------------------------------------------ reconstruction

    pub fn replay(seed: u64, log: &[Command], until: Tick) -> Encounter {
        let mut e = Encounter::new(seed);
        for c in log {
            e.advance_to(c.at);
            let _ = e.apply(c.cmd.clone());
        }
        e.advance_to(until);
        e
    }

    pub fn resume(seed: u64, cp: &Checkpoint, log: &[Command], until: Tick) -> Result<Encounter, String> {
        let j = json::parse(&cp.json)?;
        let mut e = Encounter::new(seed);
        e.fight = Fight::from_json(j.at("fight"))?;
        e.now = cp.tick;
        e.last_ck = cp.tick;
        e.open = j.at("open").as_bool().unwrap_or(false);
        e.quiet = j.at("quiet").as_u64();
        e.was_dark = e.fight.dark();
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

    pub fn state_json(&self) -> Json {
        Json::obj()
            .set("tick", self.now)
            .set("seed", Json::big(self.seed as u128))
            .set("open", self.open)
            .set("quiet", self.quiet)
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
    /// run got here: every level as of now rather than as of whenever it was
    /// last written, and sectors by their state rather than their mode.
    pub fn hash(&self) -> u64 {
        let mut f = self.fight.clone();
        for s in 0..f.structures.len() {
            f.structures[s].hp = self.fight.hp_at(s, self.now);
            f.structures[s].since = self.now;
        }
        for s in 0..f.sites.len() {
            f.sites[s].strain = self.fight.strain_at(s, self.now);
            f.sites[s].since = self.now;
        }
        for l in 0..f.crossed.len() {
            f.crossed[l] = self.fight.crossed_at(l, self.now);
        }
        f.crossed_since = self.now;
        let mut bytes = f.to_json().to_string().into_bytes();
        bytes.extend_from_slice(format!("{}|{:?}", self.open, self.quiet).as_bytes());
        for s in &self.sectors {
            bytes.extend_from_slice(s.graph.emit().as_bytes());
            bytes.extend_from_slice(&s.carry(self.now).signature());
        }
        fnv(&bytes)
    }

    pub fn log_json(&self) -> Json {
        Json::obj()
            .set("seed", Json::big(self.seed as u128))
            .set("commands", Json::Arr(self.log.iter().map(Command::to_json).collect()))
    }

    // ---------------------------------------------------------------- view

    /// Nominal things the disturbance involves, and the records that stand for
    /// them.
    pub fn overlay(&self) -> Json {
        let f = &self.fight;
        let manifest = f.alive_count();
        let shells = f.shells_in_flight();
        let turrets = f.turrets_alive();
        let standing = f.standing_structures() as u64;
        let tonnes: u64 = (0..LANES.len()).map(|l| f.crossed_at(l, self.now) / 1000).sum();
        let machines: u64 = self.sectors.iter().filter(|s| s.awake()).map(|s| s.machines()).sum();
        let alive_batteries = (0..BATTERIES.len()).filter(|&b| f.structures[BATTERIES[b].pit].hp > 0).count();
        let sector_cells: usize =
            self.sectors.iter().filter(|s| s.awake()).map(|s| s.with_pop(self.now, |p| p.distinct_states())).sum();
        Json::obj()
            .set("open", self.open)
            .set("nominal", Json::big((manifest + shells + turrets + standing + machines) as u128))
            .set("manifest", Json::big(manifest as u128))
            .set("turrets", turrets)
            .set("shells", shells)
            .set("machines", Json::big(machines as u128))
            .set("tonnes", Json::big(tonnes as u128))
            .set("cohorts", f.cohorts.len())
            .set("batteries", alive_batteries)
            .set("volleys", f.volleys.len())
            .set("structures", standing)
            .set("strainRecords", f.sites.len() * 2)
            .set("ruptures", f.torn())
            .set("sectorCells", sector_cells)
            .set("events", f.stats.events)
            .set("created", f.stats.created)
            .set("splits", f.stats.splits)
            .set("merges", f.stats.merges)
            .set("tears", f.stats.tears)
            .set("peakCohorts", f.stats.peak_cohorts)
            .set("spawned", Json::big(f.spawned as u128))
            .set("killed", Json::big(f.killed as u128))
            .set("faded", Json::big(f.faded as u128))
            .set("rendezvous", self.rendezvous)
    }

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
                    .set("origin", field::ORIGINS[k.origin].tag())
                    .set("count", Json::big(c.count as u128))
                    .set("hp", c.hp as u64)
                    .set("hpMax", k.hp as u64)
                    .set("speed", k.speed)
                    .set("spread", fight::spread(c.count))
                    .set("pos", p(Fight::pos(c, t)))
                    .set("goal", f.target(c.kind as usize).map(|s| SITES[s].node as u64));
                match c.motion {
                    Motion::March { from, to, since } => {
                        j.set("from", from as u64).set("to", to as u64).set("since", since)
                    }
                    Motion::Assault { site } => j.set("site", site as u64).set("node", SITES[site as usize].node as u64),
                }
            })
            .collect();
        let sites = (0..SITES.len())
            .map(|s| {
                let st = &f.sites[s];
                let j = Json::obj()
                    .set("name", SITES[s].name)
                    .set("strain", Json::arr(st.strain))
                    .set("rate", Json::arr([f.rate(s, 0), f.rate(s, 1)]))
                    .set("since", st.since)
                    .set("pinned", f.pinned(s))
                    .set("bleed", f.bleed(s));
                match st.rupture {
                    Some(r) => j
                        .set("torn", r.since)
                        .set("emits", r.emits as u64)
                        .set("source", field::ORIGINS[r.source as usize].tag())
                        .set("band", r.band as u64),
                    None => j,
                }
            })
            .collect();
        let (demand, factor) = f.grid();
        let lanes = (0..LANES.len())
            .map(|l| {
                Json::obj()
                    .set("tag", LANES[l].tag)
                    .set("rate", Json::big(f.lanes[l] as u128))
                    .set("flow", Json::big(f.flow(l) as u128))
                    .set("crossed", Json::big(f.crossed_at(l, t) as u128))
            })
            .collect();
        let anchors = (0..ANCHORS.len())
            .map(|a| {
                Json::obj()
                    .set("name", ANCHORS[a].name)
                    .set("on", f.anchors[a])
                    .set("live", f.anchor_live(a))
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
                    .set("powered", f.powered(b))
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
            .set("grid", Json::obj().set("demand", demand).set("supply", field::GRID_MW).set("factor", factor))
            .set("dark", f.dark())
            .set("launder", f.launder)
            .set("laundering", f.laundering())
            .set("lanes", Json::Arr(lanes))
            .set("anchors", Json::Arr(anchors))
            .set("sites", Json::Arr(sites))
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
                        .map(|(i, s)| s.to_json(t).set("inside", self.inside(i)))
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
                        .take(16)
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
                                .set("tore", Json::arr(e.tore.iter().copied()))
                                .set("woke", Json::arr(e.woke.iter().copied()))
                                .set("recompiled", Json::arr(e.recompiled.iter().copied()))
                                .set("events", e.events)
                                .set("peakCohorts", e.peak_cohorts)
                        })
                        .collect(),
                ),
            )
    }

    pub fn same_sectors(&self, other: &Encounter, t: Tick) -> Vec<bool> {
        self.sectors
            .iter()
            .zip(&other.sectors)
            .map(|(a, b)| a.graph.emit() == b.graph.emit() && a.carry(t).signature() == b.carry(t).signature())
            .collect()
    }

    pub fn closed_count(&self) -> usize {
        self.sectors.iter().filter(|s| matches!(s.mode, Mode::Closed(_))).count()
    }
}

/// The edits that take a sector's `node` from what it is to what `band` says
/// it should be. Empty when it already is.
///
/// A link runs or it does not: a damaged haul road still carries ore, a
/// destroyed one -- or one whose gantry has gone dark -- carries nothing. A
/// machine hall loses machines with each band.
pub fn wanted(base: &Graph, now: &Graph, node: &str, band: Band) -> Vec<Edit> {
    let Some(b) = base.node(node) else { return Vec::new() };
    let want = match (b.kind, band) {
        (_, Band::Destroyed) => None,
        (Kind::Link, _) => Some(b.count),
        (_, Band::Intact) => Some(b.count),
        (_, Band::Damaged) => Some((b.count * 3 / 4).max(1)),
        (_, Band::Critical) => Some((b.count / 2).max(1)),
    };
    // The bays this node fills keep what it already delivered: each gets a
    // `holds` slot while the node is gone, and its as-built form back after.
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
