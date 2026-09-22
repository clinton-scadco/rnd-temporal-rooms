//! The slice: three regions, one clock, two fractures, and the machinery
//! ledger that decides what 1890 is allowed to be.
//!
//! ```text
//!                        one wall clock
//!                              |
//!              +---------------+---------------+
//!              v               v               v
//!           valley         district          zone      each an mp::room::Room
//!            1890            2037            2070      host + one replica per
//!              |               |               |       player, hashed every
//!              +------+--------+-------+-------+        simulated second
//!                     |                |
//!                     v                v
//!                  Ledger          Interfaces     exports become stamped loads;
//!                                                 loads become Deliver commands
//!                                                 in another century's log
//! ```
//!
//! # What is authoritative about what
//!
//! ```text
//!   the region   tick, sequence, the command log, the canonical hash
//!   the slice    what may be built in which century, what a crate of imported
//!                machinery costs, which fractures are lit, and what crossed
//! ```
//!
//! Every refusal in the second list is *structural* in Prototype 2's sense: it
//! depends on the slice's state and not on who asked or when the packet
//! arrived, so it is the same refusal on every machine, and a refused command
//! never enters a region's log.
//!
//! # Three regions, all open, on purpose
//!
//! There is no ladder here. Prototype 3 gated its rooms behind each other
//! because it was asking whether finishing one made you want the next; this is
//! asking whether three centuries of one place make *one* factory problem, and
//! the answer is only visible if all three are live at once. The district cannot
//! get ore until the valley is mining, the valley cannot get a motor until the
//! district is making gears, and neither of those is a lock -- it is a supply
//! chain with a hundred and forty-seven years in the middle of it.
//!
//! # The machinery ledger, and why it is recomputed rather than accounted
//!
//! A region's imported machinery is
//!
//! ```text
//!   gears delivered into it from a later phase
//!     -  the crating cost of every design standing in it
//! ```
//!
//! and both halves are read off state that already exists -- the ledger's
//! provenance bins and the region's own world. Nothing is decremented anywhere,
//! so nothing can drift: deleting a machine frees its machinery because the sum
//! is one term shorter, and a replica that recomputes it gets the same number
//! without having been told. That is the same discipline as
//! [`crate::mp::goal`]'s accounting -- derive, do not track -- pointed at an
//! economy instead of at a score.

use super::gate::{self, Ledger, Move, Stamped, SETTLE};
use super::land;
use super::phase::{self, Phase};
use super::region::{self, Ports, Region, REGIONS};
use crate::json::Json;
use crate::model::Tick;
use crate::mp::cmd::{Act, Cmd, Effect};
use crate::mp::goal::commas;
use crate::mp::kit::Role;
use crate::mp::room::{Clock, Room, Sim, COLOURS};
use crate::mp::world::{Id, PlayerId};
use crate::mp::{as_secs, hash64, lower::item_title, room_code, secs, SIM_TICK_RATE};
use std::collections::BTreeMap;
use std::time::Instant;

/// One region of the slice, and the two doors in its wall.
pub struct Yard {
    pub region: &'static Region,
    pub room: Room,
    pub ports: Ports,
}

impl Yard {
    fn open(region: &'static Region, seed: u64) -> Yard {
        let s = seed ^ hash64(region.tag.as_bytes());
        let mut room = Room::open(s, Some(region.template));
        room.code = region.tag.to_uppercase();
        let (world, ports) = region.furnish();
        room.host = Sim::new(room.goal.clone(), world);
        Yard { region, room, ports }
    }

    pub fn done_at(&self) -> Option<Tick> {
        self.room.host.acct.done_at
    }

    /// What its depots have shipped, which is the number its objective is
    /// scored on and the number its outbound loads are drawn from.
    pub fn shipped(&self, item: &str) -> u64 {
        self.room.host.acct.got(item)
    }

    /// What it delivered to its grid connection, per second, over the last
    /// settlement window.
    ///
    /// The one number a fracture cares about. Read off the room's own canonical
    /// samples rather than off anything this module keeps, so the megawatts
    /// holding a fracture open are the same megawatts the region's objective is
    /// scored on -- which is the point. There is no separate accounting for
    /// "power spent on time travel", because there should not be: it is
    /// electricity, and the grid does not know what it is for.
    pub fn grid_mw(&self) -> u64 {
        let s = &self.room.host.acct.samples;
        let Some(now) = s.last() else { return 0 };
        let want = now.at.saturating_sub(SETTLE);
        let Some(then) = s.iter().rev().find(|x| x.at <= want) else { return 0 };
        let dt = now.at.saturating_sub(then.at);
        if dt == 0 {
            return 0;
        }
        (now.got("Power").saturating_sub(then.got("Power"))) * SIM_TICK_RATE / dt
    }

    /// The crating cost of everything *standing* in this region, in gears.
    ///
    /// Live designs only. Drafts are free, which is a decision rather than an
    /// omission: drawing a design with an imported motor in it is a thing
    /// anybody may do in any century, and what costs gears is standing it up.
    ///
    /// The first version counted drafts as well, so that a commit could never
    /// be refused for machinery that had been there while the thing was being
    /// drawn -- and it double-counted, because the draft's own cost was already
    /// in this sum by the time the commit was priced against it. Reserving is
    /// the nicer behaviour and this is the correct one; the cost of choosing
    /// correctness is that a player can draw something they cannot yet afford
    /// and be told so at the commit, by name, with the shortfall in gears.
    pub fn crated(&self) -> u64 {
        self.room
            .host
            .world
            .installs
            .iter()
            .filter_map(|i| i.design.as_ref())
            .map(|d| phase::design_cost(d, self.region.phase))
            .sum()
    }
}

/// Somebody looking at the slice, and which century they are standing in.
pub struct Cast {
    pub id: PlayerId,
    pub name: String,
    pub key: String,
    pub colour: &'static str,
    /// Index into [`REGIONS`].
    pub at: usize,
    pub joined: Tick,
    pub rejoins: u64,
}

#[derive(Clone, Debug)]
pub struct News {
    pub at: Tick,
    pub kind: &'static str,
    pub what: String,
}

pub struct Slice {
    pub code: String,
    pub seed: u64,
    pub clock: Clock,
    pub started: bool,
    pub stalled: Option<String>,
    pub yards: Vec<Yard>,
    pub ledger: Ledger,
    pub cast: Vec<Cast>,
    pub next_player: PlayerId,
    pub done: BTreeMap<&'static str, Tick>,
    pub news: Vec<News>,
    pub moves: Vec<Move>,
}

const NEWS_KEPT: usize = 80;
const MOVES_KEPT: usize = 120;

impl Slice {
    /// A new slice: three regions, furnished out of one landscape, waiting for
    /// a clock.
    pub fn open(seed: u64) -> Slice {
        Slice {
            code: room_code(seed),
            seed,
            clock: Clock::Manual(0),
            started: false,
            stalled: None,
            yards: REGIONS.iter().map(|r| Yard::open(r, seed)).collect(),
            ledger: Ledger::new(0),
            cast: Vec::new(),
            next_player: 1,
            done: BTreeMap::new(),
            news: Vec::new(),
            moves: Vec::new(),
        }
    }

    // ------------------------------------------------------------- the clock

    pub fn start(&mut self) {
        if !self.started {
            self.started = true;
            self.clock = Clock::Wall(Instant::now());
            for y in &mut self.yards {
                y.room.started = true;
                y.room.clock = Clock::Manual(0);
            }
        }
    }

    /// The same, with a clock somebody else is turning.
    pub fn start_manual(&mut self) {
        self.started = true;
        self.clock = Clock::Manual(0);
        for y in &mut self.yards {
            y.room.started = true;
            y.room.clock = Clock::Manual(0);
        }
    }

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

    // -------------------------------------------------------------- the pump

    /// Bring the whole slice to the present.
    ///
    /// ```text
    ///   for each five-second lattice point up to now:
    ///     bring every region to it
    ///     unload whatever has landed        (a Deliver in that region's log)
    ///     ask every interface whether it is holding
    ///     load whatever the depots have shipped since the last point
    ///   then bring every region to now
    /// ```
    ///
    /// The order of the middle three is the only part with a rule in it. The
    /// regions advance *first*, because a `Deliver` is stamped with the
    /// receiving region's clock and an arrival due at second 120 that is
    /// stamped at 145 is an arrival that did not happen when it happened. The
    /// interfaces are asked *before* anything is dispatched, because whether a
    /// fracture is lit must not depend on what this same call is about to try to
    /// push through it.
    pub fn advance(&mut self) -> Result<(), String> {
        if !self.started {
            return Ok(());
        }
        let now = self.now();
        while self.ledger.at + SETTLE <= now {
            let t = self.ledger.at + SETTLE;
            for y in &mut self.yards {
                y.room.set_now(t);
                y.room.host.advance(t).map_err(|f| f.msg)?;
            }
            self.unload(t);
            self.light(t);
            self.load(t);
            self.ledger.at = t;
            self.award();
        }
        for y in &mut self.yards {
            y.room.set_now(now);
            y.room.host.advance(now).map_err(|f| f.msg)?;
        }
        self.award();
        Ok(())
    }

    /// Everything that landed at `t`, as a `Deliver` in the receiving region.
    fn unload(&mut self, t: Tick) {
        for load in self.ledger.arrivals(t) {
            let Some(k) = self.yards.iter().position(|y| y.region.tag == load.to) else { continue };
            let qty = load.qty();
            let Some(&bay) = self.yards[k].ports.incoming.get(load.item) else {
                self.tell(
                    t,
                    "lost",
                    format!(
                        "{} {} reached {} with nowhere to unload",
                        commas(qty),
                        item_title(load.item),
                        self.yards[k].region.title
                    ),
                );
                continue;
            };
            let act = Act::Deliver {
                to: bay,
                item: load.item.to_string(),
                qty,
                from: load.from.to_string(),
            };
            match self.yards[k].room.submit_for(0, act) {
                Ok((_, effects)) => {
                    let (took, spilled) = effects
                        .iter()
                        .find_map(|e| match e {
                            Effect::Arrived { qty, spilled, .. } => Some((*qty, *spilled)),
                            _ => None,
                        })
                        .unwrap_or((qty, 0));
                    // Conservation, and provenance with it: what got in is the
                    // front of what was on the vehicle, and what spilled is the
                    // back of it. A yard that was too small does not decide
                    // which century the ore it did take came out of.
                    let mut landed: Vec<Stamped> = Vec::new();
                    let mut left = took;
                    for s in &load.parcels {
                        if left == 0 {
                            break;
                        }
                        let n = s.qty.min(left);
                        landed.push(Stamped { origin: s.origin, qty: n });
                        left -= n;
                    }
                    let years = landed
                        .iter()
                        .map(|s| s.origin.displacement(self.yards[k].region.phase))
                        .max()
                        .unwrap_or(0);
                    self.ledger.landed(load.route, &landed, spilled, t);
                    self.moves.push(Move {
                        at: t,
                        route: load.route,
                        from: load.from,
                        to: load.to,
                        item: load.item,
                        qty: took,
                        years,
                        arriving: true,
                    });
                    if spilled > 0 {
                        self.tell(
                            t,
                            "spill",
                            format!(
                                "the {} yard at {} was full: {} {} could not be unloaded",
                                item_title(load.item),
                                self.yards[k].region.title,
                                commas(spilled),
                                item_title(load.item)
                            ),
                        );
                    }
                }
                Err(e) => self.tell(
                    t,
                    "lost",
                    format!(
                        "{} {} could not be unloaded at {}: {e}",
                        commas(qty),
                        item_title(load.item),
                        self.yards[k].region.title
                    ),
                ),
            }
        }
        self.trim();
    }

    /// Ask every interface whether its region's grid is holding it open, and
    /// say so the first time the answer changes.
    fn light(&mut self, t: Tick) {
        let before: Vec<bool> = self.ledger.gates.iter().map(|g| g.lit).collect();
        // What each region has to spare: what it put on the grid, less what it
        // is already sending to another century.
        let mw: BTreeMap<&'static str, u64> = self
            .yards
            .iter()
            .map(|y| {
                let sold = self
                    .ledger
                    .drew
                    .get(&(y.region.tag.to_string(), "Power".to_string()))
                    .copied()
                    .unwrap_or(0);
                (y.region.tag, y.grid_mw().saturating_sub(sold))
            })
            .collect();
        self.ledger.power(t, |tag| mw.get(tag).copied().unwrap_or(0));
        for (k, was) in before.into_iter().enumerate() {
            let g = &self.ledger.gates[k];
            if g.lit == was {
                continue;
            }
            let (lit, f, want, had, worst) = (g.lit, g.fracture(), g.want_mw, g.had_mw, g.worst);
            let held = region::region(f.held_by).map(|(_, r)| r.title).unwrap_or(f.held_by);
            if lit {
                self.tell(
                    t,
                    "lit",
                    format!(
                        "the {} fracture is holding {worst} years open on {want} MW from {held}",
                        f.tag
                    ),
                );
            } else {
                self.tell(
                    t,
                    "dark",
                    format!(
                        "the {} fracture went dark: it wants {want} MW and {held} is \
                         delivering {had}",
                        f.tag
                    ),
                );
            }
        }
    }

    /// Everything the depots have shipped since the last settlement, put on
    /// whatever is waiting for it.
    fn load(&mut self, t: Tick) {
        let Slice { ledger, yards, .. } = self;
        let out = ledger.dispatch(t, |tag, item| {
            yards.iter().find(|y| y.region.tag == tag).map(|y| y.shipped(item)).unwrap_or(0)
        });
        for m in out {
            self.moves.push(m);
        }
        self.trim();
    }

    fn award(&mut self) {
        for k in 0..self.yards.len() {
            let Some(at) = self.yards[k].done_at() else { continue };
            let r = self.yards[k].region;
            if self.done.contains_key(r.tag) {
                continue;
            }
            self.done.insert(r.tag, at);
            self.tell(at, "done", format!("{} met its objective.", r.title));
        }
    }

    fn tell(&mut self, at: Tick, kind: &'static str, what: String) {
        self.news.push(News { at, kind, what });
        if self.news.len() > NEWS_KEPT {
            let cut = self.news.len() - NEWS_KEPT;
            self.news.drain(..cut);
        }
    }

    fn trim(&mut self) {
        if self.moves.len() > MOVES_KEPT {
            let cut = self.moves.len() - MOVES_KEPT;
            self.moves.drain(..cut);
        }
    }

    // ------------------------------------------------------------- the ledger

    /// Gears this region has been sent out of a later century, and what is left
    /// of them once everything standing in it is paid for.
    pub fn machinery(&self, tag: &str) -> (u64, u64, i64) {
        let Some(y) = self.yards.iter().find(|y| y.region.tag == tag) else { return (0, 0, 0) };
        let landed = self.ledger.from_later(tag, "Gear", y.region.phase);
        let spent = y.crated();
        (landed, spent, landed as i64 - spent as i64)
    }

    /// What is free to spend on a crate, in gears.
    pub fn free_machinery(&self, tag: &str) -> i64 {
        self.machinery(tag).2
    }

    // ------------------------------------------------------------- players

    pub fn join(&mut self, name: &str) -> Result<PlayerId, String> {
        self.join_as(name, "").map(|(id, _)| id)
    }

    /// The same arrival, carrying the browser's own token: a token already in
    /// the cast is the same person coming back.
    pub fn join_as(&mut self, name: &str, key: &str) -> Result<(PlayerId, bool), String> {
        self.advance()?;
        if !key.is_empty() {
            if let Some(k) = self.cast.iter().position(|c| c.key == key) {
                let id = self.cast[k].id;
                if !name.is_empty() {
                    self.cast[k].name = name.to_string();
                }
                let known = self.cast[k].name.clone();
                for y in &mut self.yards {
                    let (got, _) = y.room.join_as(&known, key)?;
                    if got != id {
                        return Err("the regions disagree about who just came back".into());
                    }
                }
                self.cast[k].rejoins += 1;
                let now = self.now();
                self.tell(now, "join", format!("{known} is back."));
                return Ok((id, true));
            }
        }
        let id = self.next_player;
        for y in &mut self.yards {
            let (got, _) = y.room.join_as(name, key)?;
            if got != id {
                return Err("the regions disagree about who just joined".into());
            }
        }
        self.next_player += 1;
        let now = self.now();
        self.cast.push(Cast {
            id,
            name: if name.is_empty() { format!("player {id}") } else { name.to_string() },
            key: key.to_string(),
            colour: COLOURS[(id as usize - 1) % COLOURS.len()],
            at: 0,
            joined: now,
            rejoins: 0,
        });
        self.tell(now, "join", format!("{name} joined."));
        Ok((id, false))
    }

    pub fn seated(&self, key: &str) -> bool {
        !key.is_empty() && self.cast.iter().any(|c| c.key == key)
    }

    pub fn who(&self, id: PlayerId) -> Option<&Cast> {
        self.cast.iter().find(|c| c.id == id)
    }

    /// Which century a player is standing in. Every region is open, so this
    /// only ever fails on a name nobody has heard of.
    pub fn travel(&mut self, id: PlayerId, tag: &str) -> Result<(), String> {
        let (k, _) = region::region(tag).ok_or_else(|| format!("there is no region called {tag}"))?;
        let c = self.cast.iter_mut().find(|c| c.id == id).ok_or("you are not in this slice")?;
        c.at = k;
        Ok(())
    }

    // ---------------------------------------------------------- the commands

    /// One intention, in one century.
    pub fn submit(&mut self, player: PlayerId, tag: &str, act: Act) -> Result<Cmd, String> {
        self.advance()?;
        let (k, _) = region::region(tag).ok_or_else(|| format!("there is no region called {tag}"))?;
        self.vet(k, &act)?;
        let (cmd, _) = self.yards[k].room.submit_for(player, act)?;
        Ok(cmd)
    }

    /// The slice's half of the validation: the four things a region cannot know
    /// about itself.
    ///
    /// ```text
    ///   somebody else built there, a hundred and fifty years ago
    ///   that component does not exist in this century, and no crate helps
    ///   that design needs more imported machinery than has arrived
    ///   that is a fixture: it is what the region is, not what you built in it
    ///   an arrival is not something a player does
    /// ```
    fn vet(&self, k: usize, act: &Act) -> Result<(), String> {
        let r = self.yards[k].region;
        let fixture = |id: Id| self.yards[k].ports.fixtures.contains(&id);
        // Where a placement would stand, and how big it would be. A machine's
        // footprint is its design's, so an empty chassis is asked about its
        // prototype and a designed one about what it will become.
        let footprint = |proto: &str, face: u8| -> (i32, i32) {
            crate::mp::kit::proto(proto).map(|p| p.footprint(face)).unwrap_or((1, 1))
        };
        match act {
            Act::PlaceMachine { proto, x, y, face, design, .. } => {
                let (w, h) = footprint(proto, *face);
                if let Some(why) = r.built_over(*x, *y, w, h) {
                    return Err(format!("there is no room at {x},{y}: {why}"));
                }
                if let Some(d) = design {
                    phase::legal(d, r.phase)?;
                    self.afford(k, 0, phase::design_cost(d, r.phase))?;
                }
            }
            Act::PlaceStorage { proto, x, y, face } => {
                let (w, h) = footprint(proto, *face);
                if let Some(why) = r.built_over(*x, *y, w, h) {
                    return Err(format!("there is no room at {x},{y}: {why}"));
                }
            }
            Act::Restore { proto, x, y, face, design, .. } => {
                let (w, h) = footprint(proto, *face);
                if let Some(why) = r.built_over(*x, *y, w, h) {
                    return Err(format!("there is no room at {x},{y}: {why}"));
                }
                if let Some(d) = design {
                    phase::legal(d, r.phase)?;
                    self.afford(k, 0, phase::design_cost(d, r.phase))?;
                }
            }
            // Drawing is free. A design with an imported motor in it may be
            // drawn by anybody in any century; what costs gears is standing it
            // up, which is the commit below. The draft does reserve what it
            // would need -- see `Yard::crated` -- so a commit is never refused
            // for machinery that was there while it was being drawn.
            Act::PlaceComponent { kind, .. } => {
                if let Some(k) = crate::machine::parts::by_tag(kind) {
                    if !r.phase.builds(k) && !phase::crateable(k) {
                        return Err(phase::refuse(
                            k,
                            crate::machine::design::Tune::default_for(k).mat,
                            r.phase,
                        )
                        .unwrap_or_else(|| "that component does not exist yet".into()));
                    }
                }
            }
            Act::CommitMachineDesign { id, design } => {
                phase::legal(design, r.phase)?;
                let was = self.yards[k]
                    .room
                    .host
                    .world
                    .get(*id)
                    .and_then(|i| i.design.as_ref())
                    .map_or(0, |d| phase::design_cost(d, r.phase));
                self.afford(k, was, phase::design_cost(design, r.phase))?;
            }
            Act::DeleteMachine { id } | Act::DeleteStorage { id } => {
                if fixture(*id) {
                    let what = self.yards[k]
                        .room
                        .host
                        .world
                        .get(*id)
                        .map(|i| i.proto.title)
                        .unwrap_or("that");
                    return Err(format!("{what} came with {} and cannot be removed", r.title));
                }
            }
            Act::Deliver { .. } => {
                return Err("an arrival is not something a player does".into())
            }
            _ => {}
        }
        Ok(())
    }

    /// Whether this region has the imported machinery for a design that costs
    /// `now` where the thing it replaces cost `was`.
    ///
    /// The sentence it refuses with names the shortfall in gears and says who
    /// has to make them, because a refusal that only said "no" would be the tech
    /// tree this experiment exists to avoid.
    fn afford(&self, k: usize, was: u64, now: u64) -> Result<(), String> {
        if now <= was {
            return Ok(());
        }
        let r = self.yards[k].region;
        let free = self.free_machinery(r.tag) + was as i64;
        if free >= now as i64 {
            return Ok(());
        }
        let short = now as i64 - free;
        Err(format!(
            "that design needs {} gears of imported machinery and {} is {} short -- \
             somebody later has to make them, and a lit fracture has to bring them back",
            commas(now),
            r.title,
            commas(short as u64)
        ))
    }

    // ---------------------------------------------------------- the fractures

    /// Put an interface on a fracture.
    pub fn open_gate(&mut self, player: PlayerId, tag: &str) -> Result<usize, String> {
        self.advance()?;
        let now = self.now();
        let i = self.ledger.open_gate(tag, now)?;
        let f = &gate::FRACTURES[i];
        let who = self.who(player).map(|c| c.name.clone()).unwrap_or_else(|| "somebody".into());
        let held = region::region(f.held_by).map(|(_, r)| r.title).unwrap_or(f.held_by);
        self.tell(
            now,
            "gate",
            format!(
                "{who} put an interface on the {} fracture -- {} years, {} MW off {held}'s grid",
                f.tag,
                f.gap(),
                gate::draw(f.gap())
            ),
        );
        Ok(i)
    }

    pub fn close_gate(&mut self, tag: &str) -> Result<(), String> {
        self.advance()?;
        let now = self.now();
        self.ledger.close_gate(tag)?;
        self.tell(now, "gate", format!("the interface on the {tag} fracture was taken down"));
        Ok(())
    }

    pub fn open_route(
        &mut self,
        player: PlayerId,
        from: &str,
        to: &str,
        item: &str,
        fleet: &str,
        cap: Option<u64>,
    ) -> Result<u32, String> {
        self.advance()?;
        let now = self.now();
        let id = self.ledger.open(from, to, item, fleet, cap, now)?;
        let r = self.ledger.route(id).expect("just opened");
        let l = r.lane();
        let words = format!(
            "{} put {} on the {} run from {} to {}{}",
            self.who(player).map(|c| c.name.clone()).unwrap_or_else(|| "somebody".into()),
            r.fleet.title.to_lowercase(),
            item_title(item),
            name(l.from),
            name(l.to),
            match l.fracture() {
                Some((_, f)) => format!(" -- {} years, through the {} fracture", f.gap(), f.tag),
                None => String::new(),
            }
        );
        self.tell(now, "route", words);
        Ok(id)
    }

    pub fn close_route(&mut self, id: u32) -> Result<(), String> {
        self.advance()?;
        let now = self.now();
        let l = self.ledger.close(id)?;
        self.tell(
            now,
            "route",
            format!(
                "the {} run from {} to {} was closed",
                item_title(l.item),
                name(l.from),
                name(l.to)
            ),
        );
        Ok(())
    }

    pub fn retune_route(&mut self, id: u32, cap: u64) -> Result<(), String> {
        self.advance()?;
        self.ledger.retune(id, cap)
    }

    // -------------------------------------------------------------- the view

    pub fn look(&mut self, player: PlayerId, tag: &str) -> Result<Json, String> {
        self.advance()?;
        let (k, _) = region::region(tag).ok_or_else(|| format!("there is no region called {tag}"))?;
        self.yards[k].room.view(player)
    }

    /// Keep every replica of every region current, whether anybody is looking
    /// or not.
    pub fn sync_all(&mut self, player: PlayerId) -> Result<(), String> {
        self.advance()?;
        for y in &mut self.yards {
            y.room.sync(player)?;
        }
        Ok(())
    }

    pub fn heartbeat(&mut self) {
        if !self.started || self.stalled.is_some() {
            return;
        }
        if let Err(e) = self.advance() {
            self.stalled = Some(e);
            return;
        }
        for y in &mut self.yards {
            y.room.heartbeat();
        }
    }

    /// What crosses one region's boundary, in both directions, and what century
    /// each of it is from.
    fn io(&self, k: usize) -> Json {
        let y = &self.yards[k];
        let now = self.now();
        let tag = y.region.tag;

        let landing = |item: &str| -> (Option<String>, Option<f64>) {
            match y.ports.incoming.get(item) {
                None => (None, None),
                Some(&bay) => {
                    let name = y.room.host.world.get(bay).map(|i| i.name.clone());
                    let cap = y.room.host.world.get(bay).map(|i| i.capacity()).unwrap_or(0);
                    let held = name
                        .as_ref()
                        .and_then(|n| y.room.host.carry.qty.get(&(n.clone(), item.to_string())))
                        .copied()
                        .unwrap_or(0);
                    let full = if cap > 0 { Some(held as f64 * 100.0 / cap as f64) } else { None };
                    (name, full)
                }
            }
        };

        let line = |r: &gate::Route, importing: bool| -> Json {
            let l = r.lane();
            let flight: u64 =
                self.ledger.flight.iter().filter(|f| f.route == r.id).map(|f| f.qty()).sum();
            let next =
                self.ledger.flight.iter().filter(|f| f.route == r.id).map(|f| f.at).min();
            let (bay, full) = if importing { landing(l.item) } else { (None, None) };
            // The four places a load can be now that one of them is a fracture.
            let blocked = if importing {
                if y.ports.incoming.get(l.item).is_none() {
                    Some(format!("{} has nowhere to unload {}", y.region.title, item_title(l.item)))
                } else if full.is_some_and(|f| f >= 99.0) {
                    Some("the yard it lands in is full".to_string())
                } else {
                    None
                }
            } else if let Some(why) =
                l.fracture().and_then(|(i, _)| self.ledger.gate(i).and_then(|g| g.why_dark()))
            {
                Some(why)
            } else if l.fracture().is_some_and(|(i, _)| self.ledger.gate(i).is_none()) {
                Some(format!(
                    "there is no interface on the {} fracture",
                    l.fracture().map(|(_, f)| f.tag).unwrap_or("")
                ))
            } else if r.waiting() > 0 && r.last_left.is_none_or(|t| now.saturating_sub(t) > secs(90))
            {
                Some(format!("{} waiting, and nothing has left for a while", commas(r.waiting())))
            } else {
                None
            };
            Json::obj()
                .set("route", r.id as i64)
                .set("item", l.item)
                .set("itemTitle", item_title(l.item))
                .set("domain", crate::mp::lower::domain_of(l.item).tag())
                .set("rate", r.moved as f64 / as_secs(now.saturating_sub(r.opened)).max(1.0))
                .set("from", l.from)
                .set("to", l.to)
                .set("fracture", l.fracture().map(|(_, f)| Json::Str(f.tag.to_string())))
                .set("years", l.fracture().map(|(_, f)| Json::Int(f.gap() as i128)))
                .set("fleet", r.fleet.title)
                .set("cap", Json::big(r.cap as u128))
                .set("atSource", Json::big(r.waiting() as u128))
                .set("inTransit", Json::big(flight as u128))
                .set("bay", bay)
                .set("bayFull", full)
                .set("moved", Json::big(r.moved as u128))
                .set("spilled", Json::big(r.spilled as u128))
                .set("heldBack", Json::big(r.held_back as u128))
                .set("nextIn", next.map(|t| Json::Real(as_secs(t.saturating_sub(now)))))
                .set("blocked", blocked)
        };

        let imports: Vec<Json> =
            self.ledger.routes.iter().filter(|r| r.lane().to == tag).map(|r| line(r, true)).collect();
        let exports: Vec<Json> = self
            .ledger
            .routes
            .iter()
            .filter(|r| r.lane().from == tag)
            .map(|r| line(r, false))
            .collect();

        let ports = |m: &BTreeMap<String, Id>| -> Vec<Json> {
            m.iter()
                .map(|(item, id)| {
                    Json::obj()
                        .set("item", item.clone())
                        .set("itemTitle", item_title(item))
                        .set("domain", crate::mp::lower::domain_of(item).tag())
                        .set("at", y.room.host.world.get(*id).map(|i| Json::Str(i.name.clone())))
                        .set(
                            "sentFrom",
                            Json::Arr(
                                self.ledger
                                    .bin(tag, item)
                                    .map(|b| {
                                        b.landed
                                            .iter()
                                            .map(|(p, n)| {
                                                Json::obj()
                                                    .set("phase", p.tag())
                                                    .set("qty", Json::big(*n as u128))
                                            })
                                            .collect()
                                    })
                                    .unwrap_or_default(),
                            ),
                        )
                })
                .collect()
        };

        Json::obj()
            .set("imports", Json::Arr(imports))
            .set("exports", Json::Arr(exports))
            .set("takes", Json::Arr(ports(&y.ports.incoming)))
            .set("gives", Json::Arr(ports(&y.ports.outgoing)))
    }

    /// The slice, as a browser or a terminal sees it.
    pub fn to_json(&mut self, player: PlayerId) -> Result<Json, String> {
        self.advance()?;
        let now = self.now();
        let here = self.who(player).map(|c| c.at).unwrap_or(0);
        let regions: Vec<Json> = (0..self.yards.len())
            .map(|k| {
                let y = &self.yards[k];
                let p = y.room.host.progress();
                let (landed, spent, free) = self.machinery(y.region.tag);
                y.region
                    .to_json()
                    .set("io", self.io(k))
                    .set("done", y.done_at().is_some())
                    .set("doneAt", y.done_at().map(|t| Json::Int(t as i128)))
                    .set("goal", y.room.goal.to_json(&p))
                    .set("installs", y.room.host.world.installs.len() as i64)
                    .set(
                        "machines",
                        y.room
                            .host
                            .world
                            .installs
                            .iter()
                            .filter(|i| i.proto.role == Role::Machine)
                            .count() as i64,
                    )
                    .set("footprint", y.room.host.world.footprint())
                    .set("gridMW", y.grid_mw() as i64)
                    .set(
                        "machinery",
                        Json::obj()
                            .set("landed", Json::big(landed as u128))
                            .set("standing", Json::big(spent as u128))
                            .set("free", free),
                    )
                    .set(
                        "here",
                        Json::arr(
                            self.cast
                                .iter()
                                .filter(|c| c.at == k)
                                .map(|c| c.name.clone())
                                .collect::<Vec<_>>(),
                        ),
                    )
            })
            .collect();
        let crossings: Vec<Json> = gate::FRACTURES
            .iter()
            .map(|f| {
                let (a, b) = f.phases();
                f.to_json().set("changes", land::changes(a, b).to_json())
            })
            .collect();
        Ok(Json::obj()
            .set("ok", true)
            .set("code", self.code.clone())
            .set("seed", Json::big(self.seed as u128))
            .set("you", player as i64)
            .set("started", self.started)
            .set("tick", now)
            .set("seconds", as_secs(now))
            .set("at", REGIONS[here].tag)
            .set("phases", phase::phases())
            .set("regions", Json::Arr(regions))
            .set("crossings", Json::Arr(crossings))
            .set("land", land::to_json())
            .set("shipping", self.ledger.to_json(now))
            .set("finished", self.done.len() as i64)
            .set(
                "cast",
                Json::Arr(
                    self.cast
                        .iter()
                        .map(|c| {
                            Json::obj()
                                .set("id", c.id as i64)
                                .set("name", c.name.clone())
                                .set("colour", c.colour)
                                .set("at", REGIONS[c.at].tag)
                                .set("joinedAt", c.joined)
                        })
                        .collect(),
                ),
            )
            .set(
                "news",
                Json::Arr(
                    self.news
                        .iter()
                        .rev()
                        .take(24)
                        .map(|n| {
                            Json::obj()
                                .set("at", n.at)
                                .set("seconds", as_secs(n.at))
                                .set("kind", n.kind)
                                .set("what", n.what.clone())
                        })
                        .collect(),
                ),
            )
            .set(
                "moves",
                Json::Arr(
                    self.moves
                        .iter()
                        .rev()
                        .take(20)
                        .map(|m| {
                            Json::obj()
                                .set("at", m.at)
                                .set("seconds", as_secs(m.at))
                                .set("arriving", m.arriving)
                                .set("years", m.years as i64)
                                .set("what", gate::moved_words(m))
                        })
                        .collect(),
                ),
            ))
    }

    pub fn hashes(&self, t: Tick) -> Vec<(&'static str, Vec<(String, Option<u64>)>)> {
        self.yards.iter().map(|y| (y.region.tag, y.room.hashes(t))).collect()
    }

    /// Whether every replica of every region agrees with its host.
    pub fn agrees(&self) -> bool {
        self.yards.iter().all(|y| {
            let probe = y
                .room
                .players
                .iter()
                .map(|p| p.sim.probe())
                .chain(std::iter::once(y.room.host.probe()))
                .min()
                .unwrap_or(0);
            probe == 0
                || y.room.players.iter().all(|p| {
                    match (y.room.host.check(probe), p.sim.check(probe)) {
                        (Some(a), Some(b)) => a == b,
                        _ => true,
                    }
                })
        })
    }

    pub fn yard(&self, tag: &str) -> Option<&Yard> {
        self.yards.iter().find(|y| y.region.tag == tag)
    }

    pub fn yard_mut(&mut self, tag: &str) -> Option<&mut Yard> {
        self.yards.iter_mut().find(|y| y.region.tag == tag)
    }

    pub fn phase_of(&self, tag: &str) -> Option<Phase> {
        region::region(tag).map(|(_, r)| r.phase)
    }

    pub fn complete(&self) -> bool {
        self.done.len() == REGIONS.len()
    }
}

fn name(tag: &str) -> &str {
    match region::region(tag) {
        Some((_, r)) => r.title,
        None => "somewhere else",
    }
}

/// How long the slice has been going, in the words a result screen uses.
pub fn spell(t: Tick) -> String {
    let s = t / secs(1);
    format!("{}:{:02}", s / 60, s % 60)
}
