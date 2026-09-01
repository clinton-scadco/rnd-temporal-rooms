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
    pub colour: &'static str,
    /// Index into [`SITES`].
    pub at: usize,
    pub joined: Tick,
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
        self.advance()?;
        let id = self.next_player;
        for y in &mut self.yards {
            let got = y.room.join(name)?;
            if got != id {
                return Err("the rooms disagree about who just joined".into());
            }
        }
        self.next_player += 1;
        let now = self.now();
        self.cast.push(Cast {
            id,
            name: if name.is_empty() { format!("player {id}") } else { name.to_string() },
            colour: COLOURS[(id as usize - 1) % COLOURS.len()],
            at: 0,
            joined: now,
        });
        self.tell(now, "join", format!("{} joined.", name));
        Ok(id)
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
            Act::PlaceMachine { proto, x, y, face, item: None, design: Some(design) },
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
