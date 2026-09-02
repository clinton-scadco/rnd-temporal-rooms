//! The campaign: five rooms, one clock, and the arithmetic that connects them.
//!
//! ```text
//!                       one wall clock
//!                             |
//!        +--------+--------+--+-----+--------+
//!        v        v        v        v        v
//!      basin   valley  station   works    final      each an mp::room::Room,
//!        |        |        |        |        |       host + one replica per
//!        +--------+---+----+--------+--------+       player, hashed every
//!                     |                              simulated second
//!                     v
//!                  Ledger        exports become trains; trains become
//!                                Deliver commands in another room's log
//! ```
//!
//! # Why a room is a Room
//!
//! Nothing in this module reimplements Prototype 2. A site *is* an
//! [`mp::room::Room`](crate::mp::room::Room): its own goal, its own command
//! log, its own host reconstruction and one replica per player, compared by
//! canonical hash every simulated second. Five of them is five of that, and
//! the multiplayer proof is unchanged and un-weakened -- which is the point.
//! The campaign adds a clock they share, a ledger between them, a shelf of
//! designs and a set of unlocks, and nothing else.
//!
//! That also settles the question the brief's section 2 asks sideways: *does a
//! room keep running while nobody is there?* It does, because the campaign
//! advances every room on every pump regardless of who is looking at which.
//! There is no "active room". There are five factories and one clock.
//!
//! # What the campaign is authoritative about
//!
//! ```text
//!   the room     tick, sequence, the command log, the canonical hash
//!   the campaign which rooms are open, what may be built, what leaves on a
//!                train, and what is on the shelf
//! ```
//!
//! Every refusal in the second list is *structural* in the same sense as
//! Prototype 2's: it depends on the campaign's state and not on who asked or
//! when the packet arrived, so it is the same refusal on every machine. A
//! refused command never enters a room's log, so no replica ever has to know
//! that a locked component was reached for.

use super::shelf::Shelf;
use super::ship::{self, Ledger, Move, SETTLE};
use super::site::{self, Ports, Site, SITES};
use super::tech::Tech;
use crate::json::Json;
use crate::model::Tick;
use crate::mp::cmd::{Act, Cmd, Effect};
use crate::mp::goal::commas;
use crate::mp::kit::Role;
use crate::mp::room::{Clock, Room, Sim, COLOURS};
use crate::mp::world::{Id, PlayerId};
use crate::mp::{as_secs, hash64, lower::item_title, room_code, secs};
use std::collections::BTreeMap;
use std::time::Instant;

/// One room of the campaign, and the two doors in its wall.
pub struct Yard {
    pub site: &'static Site,
    pub room: Room,
    pub ports: Ports,
}

impl Yard {
    fn open(site: &'static Site, seed: u64) -> Yard {
        // Each room gets its own seed so that nothing about it is a function
        // of its neighbours, and the same seed twice is the same campaign.
        let s = seed ^ hash64(site.tag.as_bytes());
        let mut room = Room::open(s, Some(site.template));
        room.code = site.tag.to_uppercase();
        let (world, ports) = site.furnish();
        room.host = Sim::new(room.goal.clone(), world);
        Yard { site, room, ports }
    }

    /// Whether this room's objective has been met, and when.
    pub fn done_at(&self) -> Option<Tick> {
        self.room.host.acct.done_at
    }

    /// What its depots have shipped, which is the same number its objective is
    /// scored on and the same number its outbound trains are loaded from.
    pub fn shipped(&self, item: &str) -> u64 {
        self.room.host.acct.got(item)
    }
}

/// Somebody playing the campaign, and which room they are standing in.
pub struct Cast {
    pub id: PlayerId,
    pub name: String,
    /// The browser's own token, minted in `localStorage`.
    ///
    /// A campaign seat is worth more than a Prototype 2 seat: it owns five
    /// rooms' worth of building, a position on the map, and everything the
    /// tech tree was unlocked with. Losing it to a refresh was the worst
    /// version of the bug the play session found, and it is found by this and
    /// never by `id` -- which is a small integer any client could type.
    pub key: String,
    pub colour: &'static str,
    /// Index into [`SITES`].
    pub at: usize,
    pub joined: Tick,
    /// Times this seat was picked up again by the same browser. Counted apart
    /// from the rooms' own resynchronisations: coming back is not diverging.
    pub rejoins: u64,
}

/// Something the campaign wants to tell everybody about.
#[derive(Clone, Debug)]
pub struct News {
    pub at: Tick,
    pub kind: &'static str,
    pub what: String,
}

pub struct Camp {
    pub code: String,
    pub seed: u64,
    pub clock: Clock,
    pub started: bool,
    /// Why the campaign stopped beating, if it has: a room whose solver will
    /// not advance. Only [`Camp::heartbeat`] sets it, and only so that the
    /// beat gives up once rather than failing four times a second forever; a
    /// request still gets the same error from its own call to `advance`.
    pub stalled: Option<String>,
    pub yards: Vec<Yard>,
    pub tech: Tech,
    pub shelf: Shelf,
    pub ledger: Ledger,
    pub cast: Vec<Cast>,
    pub next_player: PlayerId,
    /// Room tag -> the tick its objective was met.
    pub done: BTreeMap<&'static str, Tick>,
    pub news: Vec<News>,
    /// Every departure and arrival, newest last, for the shipping panel.
    pub moves: Vec<Move>,
}

const NEWS_KEPT: usize = 80;
const MOVES_KEPT: usize = 120;

impl Camp {
    /// A new campaign: five rooms, furnished, waiting for a clock.
    pub fn open(seed: u64) -> Camp {
        Camp {
            code: room_code(seed),
            seed,
            clock: Clock::Manual(0),
            started: false,
            stalled: None,
            yards: SITES.iter().map(|s| Yard::open(s, seed)).collect(),
            tech: Tech::new(),
            shelf: Shelf::default(),
            ledger: Ledger::default(),
            cast: Vec::new(),
            next_player: 1,
            done: BTreeMap::new(),
            news: Vec::new(),
            moves: Vec::new(),
        }
    }

    // ------------------------------------------------------------- the clock

    /// Start every room at once. There is no matching stop, in any of them.
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

    /// The same, with a clock somebody else is turning: the only way to test a
    /// real-time system without testing the machine it is running on.
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

    /// Bring the whole campaign to the present.
    ///
    /// This is the only thing in the game that knows about more than one room,
    /// and it is deliberately shaped like a metronome rather than like a
    /// request. Every settlement lands on a multiple of [`SETTLE`] whatever
    /// the clock is doing, so a campaign polled sixty times a second and one
    /// polled once a minute load the same trains at the same seconds.
    ///
    /// ```text
    ///   for each five-second lattice point up to now:
    ///     bring every room to it
    ///     unload whatever has landed          (a Deliver in that room's log)
    ///     load whatever the depots have shipped since the last one
    ///   then bring every room to now
    /// ```
    ///
    /// The rooms are advanced *to the lattice point first* because a `Deliver`
    /// is stamped with the room's clock, and an arrival due at second 120 that
    /// is stamped at second 145 is an arrival that did not happen when it
    /// happened.
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

    /// Everything that landed at `t`, as a `Deliver` in the receiving room.
    fn unload(&mut self, t: Tick) {
        for load in self.ledger.arrivals(t) {
            let Some(k) = self.yards.iter().position(|y| y.site.tag == load.to) else { continue };
            let Some(&bay) = self.yards[k].ports.incoming.get(load.item) else {
                self.tell(t, "lost", format!(
                    "{} {} reached {} with nowhere to unload",
                    commas(load.qty),
                    item_title(load.item),
                    self.yards[k].site.title
                ));
                continue;
            };
            let act = Act::Deliver {
                to: bay,
                item: load.item.to_string(),
                qty: load.qty,
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
                        .unwrap_or((load.qty, 0));
                    self.ledger.landed(load.route, took, spilled, t);
                    self.moves.push(Move {
                        at: t,
                        route: load.route,
                        from: load.from,
                        to: load.to,
                        item: load.item,
                        qty: took,
                        arriving: true,
                    });
                    if spilled > 0 {
                        self.tell(t, "spill", format!(
                            "{} at {} was full: {} {} could not be unloaded",
                            self.yards[k]
                                .room
                                .host
                                .world
                                .get(bay)
                                .map(|i| i.name.clone())
                                .unwrap_or_default(),
                            self.yards[k].site.title,
                            commas(spilled),
                            item_title(load.item)
                        ));
                    }
                }
                Err(e) => self.tell(t, "lost", format!(
                    "{} {} could not be unloaded at {}: {e}",
                    commas(load.qty),
                    item_title(load.item),
                    self.yards[k].site.title
                )),
            }
        }
        self.trim();
    }

    /// Everything the depots have shipped since the last settlement, put on
    /// whatever is waiting for it.
    fn load(&mut self, t: Tick) {
        let Camp { ledger, yards, .. } = self;
        let out = ledger.dispatch(t, |site, item| {
            yards.iter().find(|y| y.site.tag == site).map(|y| y.shipped(item)).unwrap_or(0)
        });
        for m in out {
            self.moves.push(m);
        }
        self.trim();
    }

    /// A room that has just been finished hands over its components.
    ///
    /// Dated by the tick the objective was *met* rather than the tick anybody
    /// noticed, because completion is a fact about the room -- decided inside
    /// its own accounting, at the second it became true -- and not about
    /// whichever poll happened next.
    fn award(&mut self) {
        for k in 0..self.yards.len() {
            let Some(at) = self.yards[k].done_at() else { continue };
            let site = self.yards[k].site;
            if self.done.contains_key(site.tag) {
                continue;
            }
            self.done.insert(site.tag, at);
            self.tell(at, "done", format!("{} is producing.", site.title));
            for u in site.unlocks() {
                if self.tech.learn(u.part) {
                    self.tell(at, "unlock", format!("{} unlocked -- {}", u.title, u.opens));
                }
            }
            for s in SITES.iter().filter(|s| s.needs.contains(&site.tag)) {
                if self.is_open(s) {
                    self.tell(at, "open", format!("{} is open. {}", s.title, s.problem));
                }
            }
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

    // ------------------------------------------------------------- players

    /// Somebody arrives. They join every room, not just the one they are
    /// standing in -- a replica that only existed while somebody was looking
    /// at it would have nothing to say about the twenty minutes they were
    /// somewhere else, and having something to say about exactly that is the
    /// whole experiment.
    pub fn join(&mut self, name: &str) -> Result<PlayerId, String> {
        self.join_as(name, "").map(|(id, _)| id)
    }

    /// The same arrival, carrying the browser's own token.
    ///
    /// A token already in the cast is the same person coming back, and coming
    /// back is not the same as arriving. In Prototype 2 that mattered because
    /// a seat owned a factory; here it owns *five*, plus a position on the map
    /// and everything the tech tree has been opened with, and a refresh that
    /// took a fresh seat left all of it standing in a chair nobody was sitting
    /// in any more.
    ///
    /// The rooms are told the token too, so each of them recognises the seat
    /// on its own terms -- their ids were handed out in lockstep and stay in
    /// lockstep. Their replicas are rebuilt from the host's snapshot, because
    /// a browser that was refreshed has no replica at all, which is a worse
    /// thing to be than wrong.
    ///
    /// An empty token is anonymous and matches nobody, so the headless
    /// campaign and the tests go on getting a fresh seat every time they ask.
    ///
    /// Returns the seat, and whether it was already there.
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
                        return Err("the rooms disagree about who just came back".into());
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
                return Err("the rooms disagree about who just joined".into());
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
        self.tell(now, "join", format!("{} joined.", name));
        Ok((id, false))
    }

    /// Whether this browser already has a seat here. Asked before a rejoin, so
    /// that a token left in storage from a campaign that has been thrown away
    /// is a refusal rather than a quiet new arrival.
    pub fn seated(&self, key: &str) -> bool {
        !key.is_empty() && self.cast.iter().any(|c| c.key == key)
    }

    pub fn who(&self, id: PlayerId) -> Option<&Cast> {
        self.cast.iter().find(|c| c.id == id)
    }

    /// Which room a player is standing in.
    pub fn travel(&mut self, id: PlayerId, tag: &str) -> Result<(), String> {
        let (k, s) = site::site(tag).ok_or_else(|| format!("there is no room called {tag}"))?;
        if !self.is_open(s) {
            return Err(format!("{} is not open yet: {}", s.title, self.gate(s)));
        }
        let c = self.cast.iter_mut().find(|c| c.id == id).ok_or("you are not in this campaign")?;
        c.at = k;
        Ok(())
    }

    // --------------------------------------------------------- what is open

    pub fn is_open(&self, s: &Site) -> bool {
        s.needs.iter().all(|n| self.done.contains_key(n))
    }

    /// Why a room is shut, in the sentence a player is shown.
    fn gate(&self, s: &Site) -> String {
        let want: Vec<&str> = s
            .needs
            .iter()
            .filter(|n| !self.done.contains_key(*n))
            .filter_map(|n| site::site(n).map(|(_, s)| s.title))
            .collect();
        match want.len() {
            0 => "it is open".into(),
            1 => format!("{} has to be producing first", want[0]),
            _ => format!("{} have to be producing first", want.join(" and ")),
        }
    }

    // ---------------------------------------------------------- the commands

    /// One intention, in one room.
    ///
    /// The campaign's own refusals happen here, before the room's. There are
    /// three of them, and each is a rule the room below could not enforce
    /// because it does not know there is a campaign:
    ///
    /// ```text
    ///   the room is not open yet
    ///   that component has not been unlocked
    ///   that is a fixture: it is what the room is, not what you built in it
    /// ```
    pub fn submit(&mut self, player: PlayerId, tag: &str, act: Act) -> Result<Cmd, String> {
        self.advance()?;
        let (k, s) = site::site(tag).ok_or_else(|| format!("there is no room called {tag}"))?;
        if !self.is_open(s) {
            return Err(format!("{} is not open yet: {}", s.title, self.gate(s)));
        }
        self.vet(k, &act)?;
        let (cmd, _) = self.yards[k].room.submit_for(player, act)?;
        Ok(cmd)
    }

    /// The campaign's half of the validation.
    fn vet(&self, k: usize, act: &Act) -> Result<(), String> {
        let fixture = |id: Id| self.yards[k].ports.fixtures.contains(&id);
        match act {
            Act::PlaceMachine { proto, design, .. } => match design {
                Some(d) => self.tech.allows(d)?,
                None => self.tech.allows_proto(proto)?,
            },
            Act::PlaceComponent { kind, .. } => {
                if !self.tech.has(kind) {
                    return Err(locked(kind));
                }
            }
            Act::CommitMachineDesign { design, .. } => self.tech.allows(design)?,
            Act::DeleteMachine { id } | Act::DeleteStorage { id } => {
                if fixture(*id) {
                    let what = self.yards[k]
                        .room
                        .host
                        .world
                        .get(*id)
                        .map(|i| i.proto.title)
                        .unwrap_or("that");
                    return Err(format!(
                        "{what} came with {} and cannot be removed",
                        self.yards[k].site.title
                    ));
                }
            }
            // Only the shipping office issues these, and it does not go
            // through this door.
            Act::Deliver { .. } => return Err("an arrival is not something a player does".into()),
            _ => {}
        }
        Ok(())
    }

    // ------------------------------------------------------------- the lanes

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
        for tag in [from, to] {
            let (_, s) = site::site(tag).ok_or_else(|| format!("there is no room called {tag}"))?;
            if !self.is_open(s) {
                return Err(format!("{} is not open yet: {}", s.title, self.gate(s)));
            }
        }
        let now = self.now();
        let id = self.ledger.open(from, to, item, fleet, cap, now)?;
        let r = self.ledger.route(id).expect("just opened");
        let words = format!(
            "{} put a {} on the {} from {} to {}",
            self.who(player).map(|c| c.name.clone()).unwrap_or_else(|| "somebody".into()),
            r.fleet.title.to_lowercase(),
            item_title(item),
            site::site(from).map(|(_, s)| s.title).unwrap_or(from),
            site::site(to).map(|(_, s)| s.title).unwrap_or(to),
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
                "the {} line from {} to {} was closed",
                item_title(l.item),
                site::site(l.from).map(|(_, s)| s.title).unwrap_or(l.from),
                site::site(l.to).map(|(_, s)| s.title).unwrap_or(l.to),
            ),
        );
        Ok(())
    }

    pub fn retune_route(&mut self, id: u32, cap: u64) -> Result<(), String> {
        self.advance()?;
        self.ledger.retune(id, cap)
    }

    // ------------------------------------------------------------- the shelf

    /// Put a placed machine's design on the shelf.
    pub fn keep(
        &mut self,
        player: PlayerId,
        tag: &str,
        id: Id,
        name: &str,
        draft: bool,
    ) -> Result<u32, String> {
        let (k, s) = site::site(tag).ok_or_else(|| format!("there is no room called {tag}"))?;
        let i = self.yards[k].room.host.world.get(id).ok_or("there is no such machine")?;
        let design = if draft {
            i.draft.clone().or_else(|| i.design.clone())
        } else {
            i.design.clone()
        }
        .ok_or("that installation has no design")?;
        let proto = i.proto.tag;
        let now = self.now();
        self.shelf.save(name, proto, design, None, s.tag, now, player)
    }

    /// Copy a shelf entry under a new name.
    pub fn copy(&mut self, player: PlayerId, id: u32, name: &str) -> Result<u32, String> {
        let now = self.now();
        let where_ = self
            .who(player)
            .map(|c| SITES[c.at].tag)
            .unwrap_or(SITES[0].tag);
        self.shelf.derive(id, name, where_, now, player)
    }

    /// Place a machine from the shelf.
    ///
    /// The design comes from the campaign rather than from the browser: a
    /// client that could post any design here would be building something
    /// nobody else can see, which is the rule Prototype 2's `/api/form` was
    /// written around and the rule a library is the obvious way to break.
    pub fn place_saved(
        &mut self,
        player: PlayerId,
        tag: &str,
        saved: u32,
        x: i32,
        y: i32,
        face: u8,
    ) -> Result<Cmd, String> {
        let s = self.shelf.get(saved).ok_or("that design is not on the shelf")?;
        let (proto, design) = (s.proto.clone(), s.design.clone());
        self.submit(
            player,
            tag,
            Act::PlaceMachine {
                proto,
                x,
                y,
                face,
                item: None,
                design: Some(design),
                example: false,
            },
        )
    }

    // -------------------------------------------------------------- the view

    /// Bring one player's replicas up to date and hand back one frame of one
    /// room.
    ///
    /// Delegated wholesale to Prototype 2, including the hash comparison: the
    /// campaign has no opinion about what a room looks like.
    pub fn look(&mut self, player: PlayerId, tag: &str) -> Result<Json, String> {
        self.advance()?;
        let (k, _) = site::site(tag).ok_or_else(|| format!("there is no room called {tag}"))?;
        self.yards[k].room.view(player)
    }

    /// Keep every replica of every room current, whether anybody is looking or
    /// not.
    ///
    /// The expensive, honest version of the experiment: a player who has spent
    /// twenty minutes in Manufacturing has a reconstruction of Iron Valley
    /// that has been fed every command and every arrival, and it is compared
    /// against the host's by hash exactly as often as the one they are staring
    /// at.
    pub fn sync_all(&mut self, player: PlayerId) -> Result<(), String> {
        self.advance()?;
        for y in &mut self.yards {
            y.room.sync(player)?;
        }
        Ok(())
    }

    /// One beat of the campaign's own clock.
    ///
    /// [`Camp::advance`] has always carried the *hosts* -- five rooms, the
    /// ledger, and every train between them, on one clock, whether anybody was
    /// looking or not. What it never carried was the replicas: those moved
    /// only when the browser that owned them polled `/api/state`, and a
    /// browser stops polling constantly. A background tab is throttled to a
    /// `setTimeout` a minute; a laptop that was shut sends nothing at all.
    ///
    /// So the replica stopped where it was, and the poll that eventually came
    /// back had to carry a minute of five rooms in one call, holding the
    /// campaign's single lock, with the other player's poll queued behind it.
    /// The player who froze was never the one who walked away.
    ///
    /// Prototype 2 fixed this with a thread; the campaign has its own server
    /// and did not get it, which is why the play session went on reporting a
    /// freeze that had supposedly been dealt with. This is that thread's other
    /// half. It advances the campaign and then lets every room carry every
    /// replica in it, which is [`Room::heartbeat`] -- the same commands, in
    /// the same order, with the same hash comparison. The only thing that
    /// changes is who asked.
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

    /// What crosses one room's boundary, in both directions.
    ///
    /// Notes 7, 10 and 16 of the play session are one problem: nobody could
    /// tell what a room was being *sent*. Power arrived in Iron Valley from
    /// Coal Basin and the only evidence was a yard somebody had to notice was
    /// pre-placed; coal could not be delivered and the message said so without
    /// saying where the coal had gone.
    ///
    /// So a room states its own imports and exports, with the three places
    /// something can be -- waiting at the source, in the air, or waiting at the
    /// destination -- named separately. Nothing disappears; if it is not
    /// moving, this says which of the three it is sitting in.
    ///
    /// Derived entirely from the ledger and the room's ports, so it costs a
    /// walk over a handful of routes and cannot disagree with the simulation.
    fn io(&self, k: usize) -> Json {
        let y = &self.yards[k];
        let now = self.now();
        let tag = y.site.tag;

        // Where a load ends up, and whether there is room for it. An import
        // with no yard is the case that used to lose material silently.
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

        let line = |r: &super::ship::Route, importing: bool| -> Json {
            let l = r.lane();
            let flight: u64 = self
                .ledger
                .flight
                .iter()
                .filter(|f| f.route == r.id)
                .map(|f| f.qty)
                .sum();
            let next = self
                .ledger
                .flight
                .iter()
                .filter(|f| f.route == r.id)
                .map(|f| f.at)
                .min();
            let (bay, full) = if importing { landing(l.item) } else { (None, None) };
            // The three places a load can be, so that "it is not arriving" is
            // always answerable with "it is here instead".
            let blocked = if importing {
                if y.ports.incoming.get(l.item).is_none() {
                    Some(format!(
                        "{} has nowhere to unload {}",
                        y.site.title,
                        item_title(l.item)
                    ))
                } else if full.is_some_and(|f| f >= 99.0) {
                    Some(format!("the yard it lands in is full"))
                } else {
                    None
                }
            } else if r.hold > 0 && r.last_left.is_none_or(|t| now.saturating_sub(t) > secs(90)) {
                Some(format!("{} waiting, and nothing has left for a while", commas(r.hold)))
            } else {
                None
            };
            Json::obj()
                .set("route", r.id as i64)
                .set("item", l.item)
                .set("itemTitle", item_title(l.item))
                .set("domain", crate::mp::lower::domain_of(l.item).tag())
                .set("from", l.from)
                .set("to", l.to)
                .set("fleet", r.fleet.title)
                .set("cap", Json::big(r.cap as u128))
                .set("rate", r.moved as f64 / as_secs(now.saturating_sub(r.opened)).max(1.0))
                // The three places, named.
                .set("atSource", Json::big(r.hold as u128))
                .set("inTransit", Json::big(flight as u128))
                .set("bay", bay)
                .set("bayFull", full)
                .set("moved", Json::big(r.moved as u128))
                .set("spilled", Json::big(r.spilled as u128))
                .set("nextIn", next.map(|t| Json::Real(as_secs(t.saturating_sub(now)))))
                .set("tripSeconds", as_secs(r.trip()))
                .set("blocked", blocked)
        };

        let imports: Vec<Json> = self
            .ledger
            .routes
            .iter()
            .filter(|r| r.lane().to == tag)
            .map(|r| line(r, true))
            .collect();
        let exports: Vec<Json> = self
            .ledger
            .routes
            .iter()
            .filter(|r| r.lane().from == tag)
            .map(|r| line(r, false))
            .collect();

        // What the room is *able* to take and send, whether or not anybody has
        // opened a route for it. A room that needs coal and has no coal route
        // is the thing note 10 could not see.
        let ports = |m: &BTreeMap<String, Id>| -> Vec<Json> {
            m.iter()
                .map(|(item, id)| {
                    Json::obj()
                        .set("item", item.clone())
                        .set("itemTitle", item_title(item))
                        .set("domain", crate::mp::lower::domain_of(item).tag())
                        .set("at", y.room.host.world.get(*id).map(|i| Json::Str(i.name.clone())))
                })
                .collect()
        };

        Json::obj()
            .set("imports", Json::Arr(imports))
            .set("exports", Json::Arr(exports))
            .set("takes", Json::Arr(ports(&y.ports.incoming)))
            .set("gives", Json::Arr(ports(&y.ports.outgoing)))
    }

    /// The campaign, as a browser sees it.
    pub fn to_json(&mut self, player: PlayerId) -> Result<Json, String> {
        self.advance()?;
        let now = self.now();
        let here = self.who(player).map(|c| c.at).unwrap_or(0);
        let rooms: Vec<Json> = (0..self.yards.len())
            .map(|k| {
                let y = &self.yards[k];
                let open = self.is_open(y.site);
                let p = y.room.host.progress();
                y.site
                    .to_json()
                    .set("io", self.io(k))
                    .set("open", open)
                    .set("gate", (!open).then(|| self.gate(y.site)))
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
                    .set(
                        "imports",
                        Json::arr(y.ports.incoming.keys().cloned().collect::<Vec<_>>()),
                    )
                    .set(
                        "exports",
                        Json::arr(y.ports.outgoing.keys().cloned().collect::<Vec<_>>()),
                    )
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
            .set("at", SITES[here].tag)
            .set("rooms", Json::Arr(rooms))
            .set("finished", self.done.len() as i64)
            .set("tech", self.tech.to_json())
            .set("shelf", self.shelf.to_json())
            .set("shipping", self.ledger.to_json(now))
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
                                .set("at", SITES[c.at].tag)
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
                                .set("what", ship::moved_words(m))
                        })
                        .collect(),
                ),
            ))
    }

    /// The canonical hashes of every room at one lattice point, for a test or
    /// a developer panel that would rather see the proof than be told it.
    pub fn hashes(&self, t: Tick) -> Vec<(&'static str, Vec<(String, Option<u64>)>)> {
        self.yards.iter().map(|y| (y.site.tag, y.room.hashes(t))).collect()
    }

    /// Whether every replica of every room agrees with its host, at the newest
    /// lattice point they all share.
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
        self.yards.iter().find(|y| y.site.tag == tag)
    }

    pub fn yard_mut(&mut self, tag: &str) -> Option<&mut Yard> {
        self.yards.iter_mut().find(|y| y.site.tag == tag)
    }

    /// Whether the whole campaign is behind them.
    pub fn complete(&self) -> bool {
        self.done.len() == SITES.len()
    }
}

fn locked(part: &str) -> String {
    match super::tech::unlock(part) {
        Some(u) => format!("{} has not been unlocked yet -- {}", u.title, u.opens),
        None => format!("there is no `{part}` component"),
    }
}

/// How long the campaign has been going, in the words a result screen uses.
pub fn spell(t: Tick) -> String {
    let s = t / secs(1);
    format!("{}:{:02}", s / 60, s % 60)
}
