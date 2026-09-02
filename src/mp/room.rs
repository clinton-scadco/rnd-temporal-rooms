//! The Room: one authoritative clock, one command log, and one reconstruction
//! of the world per player.
//!
//! ```text
//!   host                          player A                player B
//!   ----                          --------                --------
//!   clock  --------------------------->  ticks  ------------->
//!   validate, stamp (tick, seq)
//!   apply   ------ broadcast ---------->  apply  ------------> apply
//!   hash @ every second                   hash                 hash
//!            \____________________ compared _________________/
//! ```
//!
//! # Why the replicas live here
//!
//! A client of this game is a browser, and a browser cannot run the solver --
//! it is thirty thousand lines of Rust and the whole point of the project is
//! that it is exact. A "client" in this module is therefore a `Sim` in the
//! host process that is fed **nothing but the broadcast command stream**: its
//! own world document, its own compiled plant, its own `Carry`, its own
//! accounts. It shares no memory with the host's simulation and it is never
//! copied from it after the join.
//!
//! That is the strongest form of the proof available without shipping a second
//! implementation: three independent reconstructions, from one ordered stream
//! of intentions, compared by hash every simulated second. If the browser one
//! day runs the solver in WebAssembly, this module does not change -- the
//! replica moves across the wire and the comparison stays exactly where it is.
//!
//! # The three things a replica is given, and no more
//!
//! ```text
//!   join:   snapshot @ tick X  (world document, Carry, accounts, goal)
//!   then:   every command with seq > X's seq
//!   never:  the host's simulation state
//! ```
//!
//! The snapshot goes through JSON on the way, even for a replica sitting in
//! the same process, because a snapshot that is really a `clone()` proves
//! nothing about a snapshot that is really a socket.

use super::cmd::{self, Act, Cmd, Effect};
use super::goal::{self, Acct, Goal, Progress};
use super::kit::Role;
use super::world::{Build, Id, Install, PlayerId, World};
use super::{as_secs, hash64, room_code, Rng, CHECK, GHOST_LIFE, HEARTBEAT, PLOT, SIM_TICK_RATE};
use crate::graph::Graph;
use crate::json::{self, Json};
use crate::live::{self, At, Carry, Fault, Log};
use crate::machine::design::Design;
use crate::model::{ActorKind, Tick};
use crate::snap;
use std::collections::BTreeMap;
use std::time::Instant;

// ==================================================================== ghosts

/// Something that was here a moment ago.
///
/// Deliberately not a rollback. Restoring issues a *new* placement command at
/// the tick the player presses it, so the twelve seconds the thing was missing
/// really did happen, and the factory really did run without it.
#[derive(Clone, Debug)]
pub struct Ghost {
    pub at: Tick,
    pub by: PlayerId,
    /// The identity it had. The captured wiring is written in terms of it.
    pub was: Id,
    pub name: String,
    pub title: String,
    pub proto: &'static str,
    pub role: Role,
    pub x: i32,
    pub y: i32,
    /// The footprint it had, kept rather than recomputed: a ghost is drawn on
    /// every frame and a machine's size comes from running its design until it
    /// repeats itself.
    pub w: i32,
    pub h: i32,
    pub face: u8,
    pub item: Option<String>,
    pub design: Option<Design>,
    /// What it was joined to when it died.
    ///
    /// Note 12 from the play session: restoring a building that had been
    /// deleted brought back the building and none of its wiring, which is
    /// technically consistent and experientially obnoxious. A ghost is a
    /// tombstone now -- it carries the connections, and Restore puts back as
    /// many of them as the room still has room for.
    pub conns: Vec<(Id, Id, String)>,
    pub hauls: Vec<(String, Id, Id, String)>,
}

impl Ghost {
    fn of(
        i: &Install,
        by: PlayerId,
        at: Tick,
        conns: Vec<(Id, Id, String)>,
        hauls: Vec<(String, Id, Id, String)>,
    ) -> Ghost {
        let (w, h) = i.size();
        Ghost {
            at,
            by,
            was: i.id,
            name: i.name.clone(),
            title: i.proto.title.to_string(),
            proto: i.proto.tag,
            role: i.proto.role,
            x: i.x,
            y: i.y,
            w,
            h,
            face: i.face,
            item: i.item.clone(),
            design: i.design.clone(),
            conns,
            hauls,
        }
    }

    /// The command that would put it back, wiring and all.
    ///
    /// One command rather than a placement followed by a handful of
    /// connections, because a restore that was several commands would be
    /// several commands that could interleave with somebody else's -- and
    /// because "the building came back and two of its three wires did not" has
    /// to be one answer that every replica computes the same way.
    pub fn restore(&self) -> Act {
        Act::Restore {
            was: self.was,
            proto: self.proto.to_string(),
            x: self.x,
            y: self.y,
            face: self.face,
            item: self.item.clone(),
            design: self.design.clone(),
            conns: self.conns.clone(),
            hauls: self.hauls.clone(),
        }
    }

    fn to_json(&self, now: Tick) -> Json {
        let (w, h) = (self.w, self.h);
        Json::obj()
            .set("name", self.name.clone())
            .set("title", self.title.clone())
            .set("proto", self.proto)
            .set("x", self.x as i64)
            .set("y", self.y as i64)
            .set("w", w as i64)
            .set("h", h as i64)
            .set("face", self.face as i64)
            .set("item", self.item.clone())
            .set("by", self.by as i64)
            .set("at", self.at)
            // How much comes back with it, so the button can say so before it
            // is pressed rather than after.
            .set("conns", (self.conns.len() + self.hauls.len()) as i64)
            // The command that puts it back, stated by the room. The browser
            // used to assemble this itself and left the design out of it.
            .set("restore", self.restore().to_json())
            .set("fades", as_secs((self.at + GHOST_LIFE).saturating_sub(now)))
    }
}

// ======================================================================= sim

/// One reconstruction of the room: a document, a plant compiled from it, and
/// the state of that plant at a tick.
pub struct Sim {
    pub world: World,
    pub goal: Goal,
    pub acct: Acct,
    /// The plant as compiled at [`Sim::since`].
    pub base: Graph,
    pub build: Build,
    /// The tick the carry belongs to, and the beginning of the current epoch.
    pub since: Tick,
    pub carry: Carry,
    pub now: Tick,
    pub ghosts: Vec<Ghost>,
    /// The last sequence number folded in.
    pub seq: u64,
    /// Canonical hashes, one per simulated second, kept long enough for a
    /// straggler to be compared against.
    pub checks: Vec<(Tick, u64)>,
    /// Set when this replica could not do what the host said it did, which is
    /// the one condition that calls for a snapshot rather than a command.
    pub fault: Option<String>,
    /// Heat wasted and power raised per cycle, by machine name, so a
    /// checkpoint costs a multiply rather than a recompile.
    waste: BTreeMap<String, (u128, u128)>,
}

const CHECKS_KEPT: usize = 400;

impl Sim {
    pub fn new(goal: Goal, world: World) -> Sim {
        let mut s = Sim {
            world,
            goal,
            acct: Acct::default(),
            base: Graph::default(),
            build: Build::default(),
            since: 0,
            carry: Carry::default(),
            now: 0,
            ghosts: Vec::new(),
            seq: 0,
            checks: Vec::new(),
            fault: None,
            waste: BTreeMap::new(),
        };
        s.rebuild();
        s
    }

    /// Recompile the document. Cheap -- a few dozen nodes -- and the only
    /// thing that ever changes what the solver is running.
    fn rebuild(&mut self) {
        self.build = self.world.compile();
        self.base = self.build.graph.clone();
        self.waste = self
            .world
            .installs
            .iter()
            .filter_map(|i| i.lowered.as_ref().map(|m| (i.name.clone(), (m.wasted, m.power))))
            .collect();
    }

    /// Bring this reconstruction to `to`, counting everything the goal is
    /// entitled to count on the way.
    ///
    /// The counting happens whether anybody is looking or not, because a rate
    /// measured only when somebody asks is not a rate. `at_end` is called once,
    /// at `to`, with the plant in hand, for a caller that wants to draw it.
    fn run<R>(&mut self, to: Tick, mut at_end: impl FnMut(&At) -> R) -> Result<Option<R>, Fault> {
        if to < self.now {
            return Err(Fault::new("a room does not run backwards"));
        }
        let mut checks: Vec<Tick> = Vec::new();
        let mut cp = self.acct.at + CHECK;
        while cp <= to {
            checks.push(cp);
            cp += CHECK;
        }
        // Everything a lattice point is measured against, taken *before* the
        // run. The document does not change while time passes, and the books
        // have to be closed at each second with what was true at that second.
        //
        // An earlier version of this closed them all at the end of the call,
        // which made a replica that advanced forty seconds at a time disagree
        // with one that advanced twenty: the same room, hashed with a
        // different amount of the future already in it. That is the exact
        // class of bug this experiment exists to catch, and it was caught by
        // `room test` rather than by reading the code.
        let doc = self.world.signature();
        let foot = self.world.footprint();
        let stats = self.stats();
        let goal = self.goal.clone();
        let mut notes: Vec<(Tick, u64)> = Vec::new();
        // A probe is a lattice point if it is on the lattice and has not been
        // counted before. Asking that arithmetically rather than by searching
        // `checks` matters for a replica that has been away: an idle hour is
        // three thousand six hundred of them.
        let counted = self.acct.at;
        let on_lattice = move |t: Tick| t % CHECK == 0 && t > counted;

        if !self.build.runnable {
            // Nothing to simulate: an empty plot, or a factory whose last
            // machine has just been deleted. The clock does not stop for that,
            // and neither do the books -- they count zeroes, which is exactly
            // what a room with nothing in it delivered.
            for c in checks {
                self.acct.count(c, &BTreeMap::new(), &BTreeMap::new(), foot, 0, 0);
                settle(&mut self.acct, &goal, c, &stats);
                notes.push((c, hash_of(c, &doc, &self.carry, &self.acct)));
            }
            for (t, h) in notes {
                self.push_check(t, h);
            }
            self.since = to;
            self.now = to;
            return Ok(None);
        }

        let log = Log { base: self.base.clone(), commands: Vec::new() };
        let mut probes = checks.clone();
        if probes.last() != Some(&to) {
            probes.push(to);
        }
        // A carry is only a resumption point once there has been something to
        // resume from; before that the epoch starts cold at tick zero.
        let start = (self.since > 0).then_some((self.since, &self.carry));
        let waste = &self.waste;
        let mut acct = std::mem::take(&mut self.acct);
        let mut last: Option<(Tick, Carry)> = None;
        let mut out: Option<R> = None;
        let res = live::with_states(&log, &probes, start, false, |a| {
            let carry = Carry::take(a.room, a.prog, a.bp, a.tick);
            if on_lattice(a.tick) {
                let (shipped, drawn) = books(&a);
                let (w, p) = burnt(&a, waste);
                acct.count(a.tick, &shipped, &drawn, foot, w, p);
                settle(&mut acct, &goal, a.tick, &stats);
                notes.push((a.tick, hash_of(a.tick, &doc, &carry, &acct)));
            }
            if a.tick == to {
                out = Some(at_end(&a));
            }
            last = Some((a.tick, carry));
        });
        // The books come back even when the run failed. They cannot be put
        // back the way they were -- they were moved out -- and a plant that
        // will not run is a resynchronisation rather than a recovery. What
        // matters is that no *hash* was recorded for a tick whose carry was
        // never harvested, and none was: `notes` is dropped with the error.
        self.acct = acct;
        res?;
        for (t, h) in notes {
            self.push_check(t, h);
        }
        if let Some((t, c)) = last {
            self.carry = c;
            self.since = t;
        }
        self.now = to;
        Ok(out)
    }

    /// Bring this reconstruction to `to`.
    pub fn advance(&mut self, to: Tick) -> Result<(), Fault> {
        self.run(to, |_| ()).map(|_| ())
    }

    /// The same, and a look at the plant when it gets there.
    pub fn look<R>(&mut self, to: Tick, f: impl FnMut(&At) -> R) -> Result<Option<R>, Fault> {
        self.run(to, f)
    }

    /// What the world looks like from outside, for the result screen.
    fn stats(&self) -> goal::Done {
        goal::Done {
            at: 0,
            installs: self.world.installs.len(),
            machines: self.world.installs.iter().filter(|i| i.proto.role == Role::Machine).count(),
            designs: self.world.installs.iter().filter(|i| i.design.is_some()).count(),
            footprint: self.world.footprint(),
            shipped: BTreeMap::new(),
            drawn: BTreeMap::new(),
        }
    }

    /// The canonical hash of this room at one lattice point.
    pub fn hash_at(&self, t: Tick) -> u64 {
        hash_of(t, &self.world.signature(), &self.carry, &self.acct)
    }

    fn push_check(&mut self, t: Tick, h: u64) {
        if self.checks.last().is_some_and(|&(last, _)| last == t) {
            self.checks.pop();
        }
        self.checks.push((t, h));
        if self.checks.len() > CHECKS_KEPT {
            let cut = self.checks.len() - CHECKS_KEPT;
            self.checks.drain(..cut);
        }
    }

    pub fn check(&self, t: Tick) -> Option<u64> {
        self.checks.iter().rev().find(|(at, _)| *at == t).map(|(_, h)| *h)
    }

    /// The last lattice point this reconstruction has reached.
    pub fn probe(&self) -> Tick {
        self.checks.last().map(|(t, _)| *t).unwrap_or(0)
    }

    /// Fold one canonical command in.
    pub fn apply(&mut self, c: &Cmd) -> Result<Vec<Effect>, Fault> {
        self.advance(c.tick)?;
        match cmd::apply(&mut self.world, c) {
            Ok(mut effects) => {
                // Only a command that was actually applied moves the sequence.
                //
                // This used to be one line higher, before the `match`, and
                // that was a silent desynchronisation with a fuse in it. The
                // host is a `Sim` like any other, and `Room::submit_for`
                // refuses a command by rolling the *room's* counter back --
                // but the host's had already moved, so the two disagreed by
                // one from the first refusal onwards. The next command reused
                // the number, and every replica built from a snapshot after
                // that skipped it: `c.seq > sim.seq` was false for a command
                // the replica had never seen.
                //
                // What it looked like was a player who joined, built something,
                // and could not see it -- with no fault, no mismatch and no
                // resynchronisation, because as far as the room was concerned
                // that replica was up to date. It cost an afternoon, and it was
                // found by a test that refuses two connections on purpose.
                self.seq = self.seq.max(c.seq);
                if let Act::Deliver { to, item, qty, from } = &c.act {
                    effects.push(self.land(*to, item, *qty, from));
                }
                for e in &effects {
                    if let Effect::Removed { install, by, at, conns, hauls } = e {
                        self.ghosts.push(Ghost::of(
                            install,
                            *by,
                            *at,
                            conns.clone(),
                            hauls.clone(),
                        ));
                    }
                }
                self.ghosts.retain(|g| g.at + GHOST_LIFE > c.tick);
                if c.act.structural() {
                    self.rebuild();
                }
                Ok(effects)
            }
            Err(e) => {
                // The host accepted this and this replica could not. That is
                // precisely the condition a snapshot exists for; carrying on
                // would be inventing a second factory.
                self.fault = Some(format!("`{}` was refused here: {e}", c.act.verb()));
                Err(Fault::at(c.tick, &e))
            }
        }
    }

    /// Put a load that arrived from another room into a bay.
    ///
    /// This is the one place in the game where a quantity appears in a plant
    /// without a node having made it, and it works because of the rendezvous
    /// Prototype 1 built: [`Sim::apply`] has just advanced this reconstruction
    /// to the arrival's tick, so `carry` *is* the plant's state at that tick,
    /// and the next `run` pours it back in. No recompile, no restart, and no
    /// second authority -- the arrival came down the command log like
    /// everything else, so every replica does this at the same tick with the
    /// same number.
    ///
    /// What will not fit is spilled. A yard is a decision about how long a
    /// room can be left alone, and undersizing one should cost something.
    fn land(&mut self, to: Id, item: &str, qty: crate::model::Qty, from: &str) -> Effect {
        let (name, cap) = match self.world.get(to) {
            Some(i) => (i.name.clone(), i.capacity()),
            None => (format!("#{to}"), 0),
        };
        let slot = self.carry.qty.entry((name.clone(), item.to_string())).or_insert(0);
        let room = cap.saturating_sub(*slot);
        let taken = qty.min(room);
        *slot += taken;
        Effect::Arrived {
            to,
            name,
            item: item.to_string(),
            qty: taken,
            spilled: qty - taken,
            from: from.to_string(),
        }
    }

    pub fn progress(&self) -> Progress {
        goal::evaluate(&self.goal, &self.acct)
    }

    // ------------------------------------------------------------ snapshot

    /// Everything a joining replica is given.
    pub fn snapshot(&self) -> Json {
        Json::obj()
            .set("tick", self.now)
            .set("since", self.since)
            .set("seq", Json::big(self.seq as u128))
            .set("goalSeed", Json::big(self.goal.seed as u128))
            .set("template", self.goal.template)
            .set("world", self.world.to_json(&self.build, true))
            .set("carry", self.carry.to_json())
            .set("accounts", self.acct.to_json())
    }

    /// A reconstruction of the room from one, with nothing else to go on.
    pub fn of_snapshot(j: &Json) -> Result<Sim, String> {
        let goal = Goal::of_seed(
            j.at("goalSeed").as_u64().unwrap_or(0),
            j.at("template").as_str(),
        );
        let world = World::from_json(j.at("world"))?;
        let mut s = Sim::new(goal, world);
        s.carry = Carry::from_json(j.at("carry"))?;
        s.acct = Acct::from_json(j.at("accounts"));
        s.since = j.at("since").as_u64().unwrap_or(0);
        s.now = j.at("tick").as_u64().unwrap_or(0);
        s.seq = j.at("seq").as_u64().unwrap_or(0);
        Ok(s)
    }
}

/// The canonical hash of one room at one lattice point.
///
/// Three things, and nothing else: the document, the simulation state, and the
/// books. Not the clock anybody is running, not how far ahead a replica has
/// been advanced, not what any of them is drawing.
pub fn hash_of(t: Tick, doc: &[u8], carry: &Carry, acct: &Acct) -> u64 {
    let mut v = Vec::with_capacity(doc.len() + 96);
    v.extend_from_slice(&t.to_le_bytes());
    v.extend_from_slice(doc);
    v.extend_from_slice(&carry.signature());
    v.extend_from_slice(&acct.signature());
    hash64(&v)
}

/// Whether the goal has just been met, recorded once, at a lattice point.
///
/// Completion is a fact about the room rather than about whoever noticed it,
/// so it is decided inside the accounting -- at the second it became true --
/// and not by whichever client happened to poll next.
fn settle(acct: &mut Acct, goal: &Goal, at: Tick, stats: &goal::Done) {
    if acct.done_at.is_some() {
        return;
    }
    if goal::evaluate(goal, acct).met {
        acct.done_at = Some(at);
        acct.done = Some(goal::Done {
            at,
            shipped: acct.shipped.clone(),
            drawn: acct.drawn.clone(),
            ..stats.clone()
        });
    }
}

/// What every delivery point has swallowed, and what the plant has drawn.
///
/// Deliveries are `cycles x batch` over the sink classes, which is exact and
/// costs nothing: the solver was keeping the counters anyway.
fn books(a: &At) -> (BTreeMap<String, u64>, BTreeMap<String, u64>) {
    let c = a.room.counters();
    let mut shipped: BTreeMap<String, u64> = BTreeMap::new();
    for (i, act) in a.bp.actors.iter().enumerate() {
        if act.kind != ActorKind::Sink {
            continue;
        }
        for s in &act.inputs {
            let name = a.prog.item_name(s.item).to_string();
            *shipped.entry(name).or_insert(0) += c.cycles[i].saturating_mul(s.qty);
        }
    }
    let mut drawn: BTreeMap<String, u64> = BTreeMap::new();
    for (i, name) in a.prog.items.iter().enumerate() {
        if c.consumed[i] > 0 {
            drawn.insert(name.clone(), c.consumed[i]);
        }
    }
    (shipped, drawn)
}

/// Heat wasted and power raised, cycle-weighted over the machines installed.
fn burnt(a: &At, waste: &BTreeMap<String, (u128, u128)>) -> (u128, u128) {
    let c = a.room.counters();
    let mut w = 0u128;
    let mut p = 0u128;
    for (i, act) in a.bp.actors.iter().enumerate() {
        if let Some((wasted, power)) = waste.get(&act.name) {
            w += wasted * c.cycles[i] as u128;
            p += power * c.cycles[i] as u128;
        }
    }
    (w, p)
}

// ==================================================================== player

pub struct Player {
    pub id: PlayerId,
    pub name: String,
    /// The browser's own token, minted in `localStorage` and never shown to
    /// anybody else. A seat is found by this and not by `id`, because `id` is
    /// a small integer that any client could type.
    ///
    /// It is deliberately not part of the world, the command stream, or any
    /// hash: two rooms reconstructed from the same log are identical whether
    /// the people playing them refreshed their browsers or not.
    pub key: String,
    pub colour: &'static str,
    /// This player's own reconstruction. Fed by the command stream, compared
    /// by hash, and never copied from the host's.
    pub sim: Sim,
    pub joined: Tick,
    pub seen: u64,
    /// Ephemeral presence. Lossy on purpose: a cursor that arrives late is a
    /// cursor in the wrong place, and a cursor in the wrong place is a
    /// cosmetic problem rather than a divergence.
    pub cursor: Option<(f64, f64)>,
    pub selection: Option<Id>,
    pub editing: Option<Id>,
    pub view: String,
    /// The tick this player's browser last asked for a frame.
    ///
    /// The room beats on its own now, so a replica is at the current tick
    /// whether anybody is watching it or not -- which means "behind" has
    /// stopped being the answer to "is this person still here?". This is that
    /// answer: not where their copy of the room is, but when their screen last
    /// collected one. It is measured in ticks rather than wall time on
    /// purpose, so a test with a hand-driven clock measures the same thing a
    /// play session does.
    pub last_seen: Tick,
    pub mismatches: u64,
    /// Corrections after a disagreement. Counted apart from [`Self::rejoins`]
    /// on purpose: this number is the experiment's, and a browser that was
    /// refreshed is not evidence of divergence.
    pub resyncs: u64,
    /// Times this seat was picked up again by the same browser.
    pub rejoins: u64,
    pub agreed: u64,
}

pub const COLOURS: [&str; 6] =
    ["#7cc4ff", "#ffb457", "#8ef0a0", "#ff8fa3", "#c9a0ff", "#ffe066"];

impl Player {
    pub fn to_json(&self, now: Tick) -> Json {
        Json::obj()
            .set("id", self.id as i64)
            .set("name", self.name.clone())
            .set("colour", self.colour)
            .set("joinedAt", self.joined)
            .set("joinedSeconds", as_secs(self.joined))
            .set(
                "cursor",
                match self.cursor {
                    Some((x, y)) => Json::arr(vec![x, y]),
                    None => Json::Null,
                },
            )
            .set("selection", self.selection.map(|i| Json::Int(i as i128)))
            .set("editing", self.editing.map(|i| Json::Int(i as i128)))
            .set("view", self.view.clone())
            .set("tick", self.sim.now)
            .set("behind", now.saturating_sub(self.sim.now))
            // How long since this browser collected a frame. Somebody who
            // alt-tabbed is not somebody who diverged, and the room should say
            // which one it is looking at.
            .set("away", now.saturating_sub(self.last_seen))
            .set("awaySeconds", as_secs(now.saturating_sub(self.last_seen)))
            .set("agreed", Json::big(self.agreed as u128))
            .set("mismatches", Json::big(self.mismatches as u128))
            .set("resyncs", Json::big(self.resyncs as u128))
            .set("rejoins", Json::big(self.rejoins as u128))
            .set("fault", self.sim.fault.clone())
            // The last command this reconstruction folded in. Worth sending:
            // a replica whose sequence has run ahead of the log is the one
            // shape of desynchronisation that reports itself as healthy.
            .set("seq", Json::big(self.sim.seq as u128))
    }
}

// ====================================================================== room

/// What is driving the clock. A wall clock for a game; a number for a test,
/// which is the only way a test of a real-time system is worth running.
pub enum Clock {
    Wall(Instant),
    Manual(Tick),
}

impl Clock {
    pub fn now(&self) -> Tick {
        match self {
            Clock::Wall(t) => t.elapsed().as_millis() as u64 * SIM_TICK_RATE / 1000,
            Clock::Manual(t) => *t,
        }
    }
}

/// Something that happened, for the log a player reads.
#[derive(Clone, Debug)]
pub struct Event {
    pub at: Tick,
    pub by: PlayerId,
    pub verb: &'static str,
    pub what: String,
}

pub struct Room {
    pub code: String,
    pub seed: u64,
    pub goal: Goal,
    pub clock: Clock,
    /// The canonical history. Every replica is a function of this and the
    /// starting world, and of nothing else.
    pub log: Vec<Cmd>,
    pub seq: u64,
    pub host: Sim,
    pub players: Vec<Player>,
    pub events: Vec<Event>,
    pub next_player: PlayerId,
    /// Set once the host has begun advancing time. Before that the goal is on
    /// screen and the factory is not running, which is the only pause the game
    /// has.
    pub started: bool,
    /// Why the room stopped beating, if it has.
    ///
    /// Only ever set by [`Room::heartbeat`], and only when the host's own
    /// simulation refuses to advance -- which is a room that cannot be played
    /// rather than one that is behind. It exists so that the beat gives up
    /// once instead of failing four times a second forever; a poll still gets
    /// the same error it always did, from its own call.
    pub stalled: Option<String>,
}

const EVENTS_KEPT: usize = 60;

impl Room {
    /// A new room, its code, its goal, and the plot it starts on.
    pub fn open(seed: u64, template: Option<&str>) -> Room {
        let goal = Goal::of_seed(seed, template);
        let world = starting_world(&goal, seed);
        Room {
            code: room_code(seed),
            seed,
            goal: goal.clone(),
            clock: Clock::Manual(0),
            log: Vec::new(),
            seq: 0,
            host: Sim::new(goal, world),
            players: Vec::new(),
            events: Vec::new(),
            next_player: 1,
            started: false,
            stalled: None,
        }
    }

    /// Start the clock. There is no matching stop.
    pub fn start(&mut self) {
        if !self.started {
            self.started = true;
            self.clock = Clock::Wall(Instant::now());
        }
    }

    /// Start it with a clock somebody else is turning.
    ///
    /// A real-time system tested against a real clock is a test that passes on
    /// a fast machine, and the thing being proved here has nothing to do with
    /// wall time: it is that three reconstructions of the same command stream
    /// agree. So the harness drives the tick and the room cannot tell.
    pub fn start_manual(&mut self) {
        self.started = true;
        self.clock = Clock::Manual(0);
    }

    /// Move a hand-driven clock forward.
    pub fn set_now(&mut self, t: Tick) {
        if let Clock::Manual(c) = &mut self.clock {
            *c = (*c).max(t);
        }
    }

    pub fn now(&self) -> Tick {
        if self.started {
            self.clock.now()
        } else {
            0
        }
    }

    pub fn player(&self, id: PlayerId) -> Option<&Player> {
        self.players.iter().find(|p| p.id == id)
    }

    /// Whether this browser already has a seat here. Asked before a rejoin, so
    /// that coming back to a room that has moved on is a refusal rather than a
    /// quiet new arrival.
    pub fn seated(&self, key: &str) -> bool {
        !key.is_empty() && self.players.iter().any(|p| p.key == key)
    }

    /// Somebody arrives, at whatever tick the room happens to be at.
    ///
    /// The host is not stopped, not paused, and not asked. It hands over a
    /// snapshot of where it is, and the joiner catches up on its own.
    pub fn join(&mut self, name: &str) -> Result<PlayerId, String> {
        self.join_as(name, "").map(|(id, _)| id)
    }

    /// The same arrival, carrying the browser's own token.
    ///
    /// A token already in the room is the same person coming back, and coming
    /// back is not the same as arriving: they keep their seat, their id, and
    /// therefore everything their id owns and every command they have already
    /// sent. Only the replica is new, rebuilt from the host's snapshot the way
    /// a correction is -- because a browser that was refreshed has no replica
    /// at all, which is a worse thing to be than wrong.
    ///
    /// An empty token is anonymous and matches nobody, so the CLI harness and
    /// the tests go on getting a fresh seat every time they ask.
    ///
    /// Returns the seat, and whether it was already there.
    pub fn join_as(&mut self, name: &str, key: &str) -> Result<(PlayerId, bool), String> {
        if !key.is_empty() {
            if let Some(k) = self.players.iter().position(|p| p.key == key) {
                if !name.is_empty() {
                    self.players[k].name = name.to_string();
                }
                // Presence is ephemeral and the old one is a lie the moment
                // the browser went away: a cursor from before the refresh
                // would sit on the plot until the first poll moved it.
                self.players[k].cursor = None;
                self.players[k].selection = None;
                self.players[k].editing = None;
                self.players[k].sim = self.fresh_sim()?;
                self.players[k].rejoins += 1;
                let now = self.now();
                self.players[k].last_seen = now;
                return Ok((self.players[k].id, true));
            }
        }
        let sim = self.fresh_sim()?;
        let now = self.now();
        let id = self.next_player;
        self.next_player += 1;
        self.players.push(Player {
            id,
            name: if name.is_empty() { format!("player {id}") } else { name.to_string() },
            key: key.to_string(),
            colour: COLOURS[(id as usize - 1) % COLOURS.len()],
            sim,
            joined: now,
            seen: self.seq,
            cursor: None,
            selection: None,
            editing: None,
            view: "world".into(),
            last_seen: now,
            mismatches: 0,
            resyncs: 0,
            rejoins: 0,
            agreed: 0,
        });
        Ok((id, false))
    }

    /// An authoritative replica: the host's snapshot, and every command it did
    /// not already contain.
    ///
    /// Shared by the correction path and the arrival path because they want
    /// exactly the same object -- the only difference between a client that
    /// diverged and a client that has never existed is which of them we are
    /// embarrassed about.
    fn fresh_sim(&mut self) -> Result<Sim, String> {
        let now = self.now();
        self.host.advance(now).map_err(|f| f.msg.clone())?;
        // Through JSON, deliberately: a snapshot that is really a clone proves
        // nothing about a snapshot that is really a socket.
        let snap = self.host.snapshot().to_string();
        let parsed = json::parse(&snap).map_err(|e| format!("the snapshot did not survive: {e}"))?;
        let mut sim = Sim::of_snapshot(&parsed)?;
        let pending: Vec<Cmd> = self.log.iter().filter(|c| c.seq > sim.seq).cloned().collect();
        for c in &pending {
            let _ = sim.apply(c);
        }
        let _ = sim.advance(now);
        Ok(sim)
    }

    /// A player's intention, validated, stamped and applied.
    ///
    /// The tick is the host's, never the client's: a command that spent two
    /// hundred milliseconds in the air happens two hundred milliseconds later,
    /// which is the only interpretation that two clients can agree on without
    /// asking each other.
    pub fn submit(&mut self, player: PlayerId, act: Act) -> Result<Cmd, String> {
        self.submit_for(player, act).map(|(c, _)| c)
    }

    /// The same, and what it did.
    ///
    /// Prototype 3 wants the effects: a train that arrived at a yard too small
    /// for it spilled, and the shipping office is entitled to know how much
    /// before the next one leaves.
    pub fn submit_for(&mut self, player: PlayerId, act: Act) -> Result<(Cmd, Vec<Effect>), String> {
        if !self.started {
            return Err("the room has not started yet".into());
        }
        if player != 0 && self.player(player).is_none() {
            return Err("you are not in this room".into());
        }
        let tick = self
            .now()
            .max(self.host.now)
            .max(self.log.last().map(|c| c.tick).unwrap_or(0));
        self.seq += 1;
        let c = Cmd { room: self.code.clone(), tick, seq: self.seq, player, act };
        let effects = match self.host.apply(&c) {
            Ok(e) => e,
            Err(f) => {
                // Refused: it never happened, and it never enters the log.
                self.seq -= 1;
                return Err(f.msg);
            }
        };
        for e in &effects {
            match e {
                Effect::Recommitted { name, to, .. } => {
                    self.note(tick, player, "redesign", format!("{name} is now {to}"));
                }
                // Half a restore is news. The old behaviour was to put the
                // building back with none of its wiring and say nothing at
                // all, which left the player to work out from a machine that
                // would not start that anything had been lost.
                Effect::Restored { name, wanted, made, failed, .. } if *wanted > 0 => {
                    self.note(
                        tick,
                        player,
                        "restore",
                        if failed.is_empty() {
                            format!("{name} is back, with all {made} of its connections")
                        } else {
                            format!(
                                "{name} is back, with {made} of {wanted} connections -- {}",
                                failed.join("; ")
                            )
                        },
                    );
                }
                Effect::Arrived { name, spilled, .. } if *spilled > 0 => {
                    self.note(
                        tick,
                        player,
                        "spill",
                        format!("{name} was full: {} could not be unloaded", goal::commas(*spilled)),
                    );
                }
                _ => {}
            }
        }
        self.note(tick, player, c.act.verb(), describe(&c.act, &self.host.world));
        self.log.push(c.clone());
        Ok((c, effects))
    }

    fn note(&mut self, at: Tick, by: PlayerId, verb: &'static str, what: String) {
        self.events.push(Event { at, by, verb, what });
        if self.events.len() > EVENTS_KEPT {
            let cut = self.events.len() - EVENTS_KEPT;
            self.events.drain(..cut);
        }
    }

    /// Bring one player's replica up to date: every command it has not seen,
    /// then the clock, then the comparison.
    pub fn sync(&mut self, id: PlayerId) -> Result<(), String> {
        let now = self.now();
        // The authority runs first, and would run even if nobody ever asked:
        // the room's clock is not driven by anybody's browser.
        self.host.advance(now).map_err(|f| f.msg)?;
        let Some(k) = self.players.iter().position(|p| p.id == id) else {
            return Err("you are not in this room".into());
        };
        let pending: Vec<Cmd> = self
            .log
            .iter()
            .filter(|c| c.seq > self.players[k].sim.seq)
            .cloned()
            .collect();
        for c in pending {
            if self.players[k].sim.apply(&c).is_err() {
                self.resync(k)?;
                return Ok(());
            }
        }
        if self.players[k].sim.advance(now).is_err() {
            self.resync(k)?;
            return Ok(());
        }
        // The comparison. Both sides have counted every lattice point they
        // have passed, so the newest one they share is the newest one either
        // of them can be asked about.
        let probe = self.players[k].sim.probe().min(self.host.probe());
        if probe > 0 {
            match (self.host.check(probe), self.players[k].sim.check(probe)) {
                (Some(h), Some(c)) if h == c => self.players[k].agreed += 1,
                (Some(_), Some(_)) => {
                    self.players[k].mismatches += 1;
                    self.resync(k)?;
                }
                _ => {}
            }
        }
        Ok(())
    }

    /// One beat of the room's own clock.
    ///
    /// The clock was always the room's. The *replicas* were the browsers': a
    /// replica moved only when the browser that owned it polled, which is a
    /// fine arrangement right up until a browser stops polling. A tab in the
    /// background gets a `setTimeout` once a minute rather than five times a
    /// second; a laptop that was shut gets none at all; and a click on a link
    /// in the left-hand menu got none ever again. So the replica stopped where
    /// it was, and then the poll came back and *one call* had to carry it
    /// three thousand six hundred ticks -- holding, for as long as that took,
    /// the one lock the other player's poll was waiting on.
    ///
    /// That is the freeze the play session kept hitting, and the important
    /// half of it is the wrong half: it was not the browser that went away
    /// that froze, it was the one that stayed.
    ///
    /// So the room beats on its own. Every replica is carried on the same
    /// cadence whether or not anybody is looking at it, the work per beat is
    /// bounded by the beat rather than by how long somebody was gone, and a
    /// poll becomes what it always claimed to be -- a read of a frame that is
    /// already there.
    ///
    /// Nothing about the proof changes. A beat is `sync`, which is the same
    /// commands in the same order followed by the same comparison; the only
    /// difference is who asked for it. Returns the tick the room reached.
    pub fn heartbeat(&mut self) -> Tick {
        let now = self.now();
        if !self.started {
            return now;
        }
        // A host that cannot run is a dead room, not a slow one, and beating at
        // it four times a second would be a hot loop that says nothing new.
        // The reason is recorded once, here, and the room is then left alone
        // for the next poll to report it.
        //
        // Deliberately not `host.fault`, which was the first thing this
        // reached for and was wrong: on a replica that field means "the host
        // did something I could not", but the host is also a `Sim`, and a
        // command refused during its own apply sets it too. One player opening
        // a design somebody else already had open is enough -- an ordinary
        // refusal, in an ordinary game -- and the room would have stopped
        // beating for the rest of the session. Whether the solver will run is
        // a different question from whether a command was allowed, and it
        // needs a different flag.
        if self.stalled.is_some() {
            return now;
        }
        if let Err(f) = self.host.advance(now) {
            self.stalled = Some(f.msg);
            return now;
        }
        // The authority ran first, above, so every replica below is compared
        // against a host that has already been where it is going.
        let ids: Vec<PlayerId> = self.players.iter().map(|p| p.id).collect();
        for id in ids {
            // A replica that could not be carried is corrected inside `sync`.
            // A room does not stop beating because one browser's copy of it
            // went wrong -- that is precisely the case the correction exists
            // for.
            let _ = self.sync(id);
        }
        now
    }

    /// The correction: an authoritative snapshot, and the commands since.
    ///
    /// Whole-room for now, and the architecture is the thing that matters --
    /// the snapshot is already per-room rather than per-application, so
    /// resending one deterministic region rather than all of them is a change
    /// to what is put in the envelope, not to who sends it.
    fn resync(&mut self, k: usize) -> Result<(), String> {
        self.players[k].sim = self.fresh_sim()?;
        self.players[k].resyncs += 1;
        Ok(())
    }

    /// Everything one player's browser needs to draw one frame.
    pub fn view(&mut self, id: PlayerId) -> Result<Json, String> {
        let now = self.now();
        if id == 0 {
            self.host.advance(now).map_err(|f| f.msg)?;
        } else {
            // Still a sync, and it is nearly always a no-op now: the beat got
            // here first. What it costs is the handful of ticks since the last
            // beat, which is the whole point -- the catch-up that used to be
            // measured in minutes is measured in one beat, and it happens on
            // the room's thread rather than in the middle of somebody's poll.
            self.sync(id)?;
            if let Some(p) = self.players.iter_mut().find(|p| p.id == id) {
                p.last_seen = now;
            }
        }
        let host_probe = self.host.probe();
        let host_hash = self.host.check(host_probe);
        let players: Vec<Json> = self.players.iter().map(|p| p.to_json(now)).collect();
        let events: Vec<Json> = self
            .events
            .iter()
            .rev()
            .take(24)
            .map(|e| {
                Json::obj()
                    .set("at", e.at)
                    .set("seconds", as_secs(e.at))
                    .set("by", e.by as i64)
                    .set("verb", e.verb)
                    .set("what", e.what.clone())
            })
            .collect();

        let sim: &mut Sim = if id == 0 {
            &mut self.host
        } else {
            let k = self.players.iter().position(|p| p.id == id).unwrap();
            &mut self.players[k].sim
        };
        let plant = sim
            .look(now, |a: &At| snap::render(a.prog, a.bp, a.plan, a.room, a.tick))
            .map_err(|f| f.msg)?
            .unwrap_or(Json::Null);
        let progress = sim.progress();
        let ghosts: Vec<Json> = sim
            .ghosts
            .iter()
            .filter(|g| g.at + GHOST_LIFE > now)
            .map(|g| g.to_json(now))
            .collect();
        let world = sim.world.to_json(&sim.build, false);
        let sim_now = sim.now;
        let probe = sim.probe();
        let hash = sim.check(probe);
        let acct = sim.acct.to_json();
        let goal = self.goal.to_json(&progress);

        Ok(Json::obj()
            .set("ok", true)
            .set("code", self.code.clone())
            .set("you", id as i64)
            .set("started", self.started)
            .set("tick", now)
            .set("seconds", as_secs(now))
            .set("tickRate", SIM_TICK_RATE)
            .set("goal", goal)
            .set("world", world)
            .set("plant", plant)
            .set("accounts", acct)
            .set("ghosts", Json::Arr(ghosts))
            .set("players", Json::Arr(players))
            .set("events", Json::Arr(events))
            .set("commands", Json::big(self.log.len() as u128))
            .set(
                "sync",
                Json::obj()
                    .set("probe", probe)
                    .set("probeSeconds", as_secs(probe))
                    // How far this reconstruction trails the room's clock at
                    // the moment the frame was cut. With the beat running it
                    // is a beat's worth of ticks or none; a number that grows
                    // is the room failing to keep up with itself, which is a
                    // different problem from a browser failing to keep up with
                    // the room, and the screen should be able to tell them
                    // apart.
                    .set("behind", now.saturating_sub(sim_now))
                    .set("beat", HEARTBEAT)
                    .set("hash", hash.map(|h| Json::Str(format!("{h:016x}"))))
                    .set("hostProbe", host_probe)
                    .set(
                        "hostHash",
                        host_hash.map(|h| Json::Str(format!("{h:016x}"))),
                    )
                    .set(
                        "agrees",
                        match (hash, host_hash) {
                            (Some(a), Some(b)) if probe == host_probe => Json::Bool(a == b),
                            _ => Json::Null,
                        },
                    ),
            ))
    }

    /// The hash every replica has for one lattice point, for a test or a
    /// developer panel that wants to see the proof rather than be told it.
    pub fn hashes(&self, t: Tick) -> Vec<(String, Option<u64>)> {
        let mut v = vec![("host".to_string(), self.host.check(t))];
        for p in &self.players {
            v.push((p.name.clone(), p.sim.check(t)));
        }
        v
    }
}

/// What a command did, in the words an event log uses.
fn describe(a: &Act, w: &World) -> String {
    let name = |id: Id| {
        w.get(id)
            .map(|i| i.name.clone())
            .or_else(|| w.haul(id).map(|h| h.name.clone()))
            .unwrap_or_else(|| format!("#{id}"))
    };
    match a {
        Act::PlaceMachine { proto, x, y, .. } | Act::PlaceStorage { proto, x, y, .. } => {
            let title = super::kit::proto(proto).map(|p| p.title).unwrap_or(proto);
            format!("{title} at {x},{y}")
        }
        Act::DeleteMachine { id } | Act::DeleteStorage { id } => format!("deleted {}", name(*id)),
        Act::Restore { proto, x, y, conns, hauls, .. } => {
            let title = super::kit::proto(proto).map(|p| p.title).unwrap_or(proto);
            let n = conns.len() + hauls.len();
            format!("restored {title} at {x},{y}{}", match n {
                0 => String::new(),
                1 => " and its connection".to_string(),
                n => format!(" and its {n} connections"),
            })
        }
        Act::CreateConnection { from, to, item } => {
            format!("{} -> {} ({item})", name(*from), name(*to))
        }
        Act::DeleteConnection { from, to, .. } => {
            format!("unwired {} -> {}", name(*from), name(*to))
        }
        Act::CreateWorldLink { proto, from, to, item } => {
            format!("{proto} {} -> {} ({item})", name(*from), name(*to))
        }
        Act::DeleteWorldLink { id } => format!("removed transport {}", name(*id)),
        Act::OpenDesign { id } => format!("opened {}", name(*id)),
        Act::CloseDesign { id, .. } => format!("closed {}", name(*id)),
        Act::PlaceComponent { id, kind, .. } => format!("{kind} in {}", name(*id)),
        Act::DeleteComponent { id, unit } => format!("{unit} out of {}", name(*id)),
        Act::TuneComponent { id, unit, field, value } => {
            format!("{unit}.{field} = {value} in {}", name(*id))
        }
        Act::ConnectComponent { id, from, to, .. } => {
            format!("{from} -> {to} in {}", name(*id))
        }
        Act::DisconnectComponent { id, from, to, .. } => {
            format!("unwired {from} -> {to} in {}", name(*id))
        }
        Act::CommitMachineDesign { id, .. } => format!("committed {}", name(*id)),
        Act::Deliver { to, item, qty, from } => format!(
            "{} {} from {from} into {}",
            goal::commas(*qty),
            super::lower::item_title(item),
            name(*to)
        ),
    }
}

/// The plot a room begins on: the raw materials its goal is about, a bay for
/// each, and somewhere to ship the answer.
///
/// Laid out from the seed, so both players see the same yard, and spread out
/// enough that the first thing anybody does is decide where the factory
/// actually goes.
/// What a patch of ground of this kind is worth, in the standalone rooms.
///
/// Prototype 3's five rooms each write their own numbers, because scarcity is
/// what makes them different from each other. A room rolled from a seed has no
/// such opinion, so the ground is worth what the catalogue's head can lift and
/// the constraint lives in the goal instead.
fn per_second(p: &'static super::kit::Proto) -> crate::model::Qty {
    match p.tag {
        "waterpump" => 400,
        "crudewell" => 200,
        _ => 100,
    }
}

pub fn starting_world(goal: &Goal, seed: u64) -> World {
    let mut w = World::new("Room");
    let mut r = Rng(seed ^ 0x51ed_2701);
    let mut y = 6;
    let mut ship = 8;
    for (tag, item) in goal.starting_kit() {
        let Some(p) = super::kit::proto(tag) else { continue };
        match p.role {
            // Ground down the western edge, each with a bay beside it, and the
            // delivery points down the eastern one. Everything in between is
            // the game -- including, since experiment 13, the heads that work
            // the ground: a room hands out an opportunity and never an output.
            Role::Source => {
                let jitter = r.between(0, 1) as i32;
                if let Some(item) = p.extracts() {
                    w.seam(item, 3, y + jitter, 8, 6, per_second(p));
                }
            }
            Role::Storage => {
                let _ = w.place(p, 14, y, 0, None, None, 0, 0);
                y += 12;
            }
            _ => {
                let _ = w.place(p, PLOT - 10, ship, 0, item, None, 0, 0);
                ship += 12;
            }
        }
    }
    w
}
