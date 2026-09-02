//! The game document: physical installations, where they are, and what is
//! wired to what.
//!
//! ```text
//!   GAME DOCUMENT      a mine at 12,40 facing east; a yard; a rail between them
//!         | compile
//!         v
//!   SIMULATION IR      population classes, storages, channels, regions
//! ```
//!
//! The arrow only points down. Nothing below this line knows that a mine has a
//! position, and nothing above it knows that a class of forty machines is one
//! object -- which is what lets the world be a place a player builds in while
//! the solver goes on believing it is running a spreadsheet with a billion
//! rows in it. Population compression stays an implementation detail, and the
//! player never sees an `x100000` field, because there isn't one.
//!
//! # Place and delete, and nothing else
//!
//! A committed installation does not move. Wanting it elsewhere is a delete
//! and a place, at two different ticks, by whoever is holding the mouse. That
//! is a gameplay rule with a networking reason behind it: a drag is a stream
//! of positions with no canonical order, and a place is one command with one
//! tick and one sequence number. Everything before the commit -- the ghost,
//! the rotation, the red outline over a collision -- is a picture in one
//! player's browser that no other machine ever hears about.
//!
//! # Commissioning
//!
//! A machine that has just been placed is not yet wired to anything, and the
//! language below is entitled to refuse a plant whose smelter has nowhere to
//! put its plates. In a game that never pauses, "the factory does not compile"
//! cannot be an outcome, so the compiler answers a narrower question than the
//! language does:
//!
//! ```text
//!   is this installation commissioned?
//!     every input item arrives at exactly one bay wired into it, and
//!     every output item leaves to exactly one bay wired out of it
//! ```
//!
//! Anything that fails is left out of the IR and told why, in a sentence its
//! inspector shows. It is still in the document, still drawn, still deletable;
//! it simply is not running yet, which is what a half-built factory looks
//! like. The check is a fixpoint, because dropping a machine can starve the
//! bay that fed the next one.

use super::kit::{proto, Proto, Role, Spec};
use super::lower::{self, Macro};
use super::{PLOT, SIM_TICK_RATE};
use crate::graph::{Amount, Edge, Graph, Kind, Node};
use crate::json::Json;
use crate::machine::design::Design;
use crate::machine::stuff::Domain;
use crate::model::{Geometry, Policy, Qty, Tick};
use std::collections::{BTreeMap, BTreeSet};

pub type Id = u64;
pub type PlayerId = u32;

/// One physical thing, placed.
#[derive(Clone, Debug)]
pub struct Install {
    pub id: Id,
    pub proto: &'static Proto,
    /// The name the simulator knows it by, and the key its state is carried
    /// under across every rebuild of the plant.
    pub name: String,
    pub x: i32,
    pub y: i32,
    pub face: u8,
    /// A delivery depot ships one item, chosen when it is placed.
    ///
    /// A *bay* may carry one too, and it means something different: this yard
    /// is supplied from outside the room. Nothing inside delivers to it -- a
    /// train does, from somewhere the compiler below has never heard of -- so
    /// the commissioning check has to be told, or every machine drawing from
    /// an import yard would be refused for being fed by nobody. Prototype 3's
    /// rooms are the only things that set it, when they furnish themselves.
    pub item: Option<String>,
    /// What *this* one is rated at, when the catalogue's number is not the
    /// right one: a source's items a second, or a bay's capacity.
    ///
    /// Nothing a player places ever carries one. It exists so that a room can
    /// be *furnished* -- a coal seam that yields forty-five a second rather
    /// than a hundred, a water table that yields sixty -- because a room whose
    /// problem is scarcity cannot state it any other way. Prototype 3's five
    /// rooms are the whole reason this field is here.
    pub rated: Option<Qty>,
    /// A machine owns its design outright. Duplicating a machine copies the
    /// design; editing one copy does not touch the other.
    pub design: Option<Design>,
    /// The design, as a recipe. Recomputed only when the design changes, which
    /// is why a machine can be inspected sixty times a second.
    pub lowered: Option<Macro>,
    /// The design being edited, which the live machine above knows nothing
    /// about until somebody commits it.
    pub draft: Option<Design>,
    pub editor: Option<PlayerId>,
    pub placed: Tick,
    pub by: PlayerId,
}

impl Install {
    /// Footprint as placed. A machine's comes from the design inside it, which
    /// is what makes the space goals a question about engineering.
    pub fn size(&self) -> (i32, i32) {
        // Whatever is designed takes the size of what was designed into it --
        // and since experiment 13 that includes an extraction head, whose
        // footprint is the first thing anybody standing in Coal Basin has to
        // argue with. Keying this off `Role::Machine` left the heads wearing
        // the catalogue's box, so two of them on one seam overlapped.
        let (w, h) = match (&self.lowered, self.proto.role.designed()) {
            (Some(m), true) => (m.w, m.h),
            _ => (self.proto.w, self.proto.h),
        };
        if self.face & 1 == 1 {
            (h, w)
        } else {
            (w, h)
        }
    }

    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        let (w, h) = self.size();
        (self.x, self.y, self.x + w, self.y + h)
    }

    /// Centre, in tenths of a tile, so a distance never needs a float.
    pub fn centre(&self) -> (i32, i32) {
        let (w, h) = self.size();
        (self.x * 10 + w * 5, self.y * 10 + h * 5)
    }

    pub fn overlaps(&self, other: &Install) -> bool {
        let (ax0, ay0, ax1, ay1) = self.bounds();
        let (bx0, by0, bx1, by1) = other.bounds();
        ax0 < bx1 && bx0 < ax1 && ay0 < by1 && by0 < ay1
    }

    /// What one cycle of this installation consumes and produces. A source
    /// takes nothing, a sink gives nothing, and a machine answers with its
    /// orbit.
    pub fn recipe(&self) -> (Vec<Amount>, Vec<Amount>, Tick) {
        let amount = |i: &str, q: Qty| Amount { item: i.to_string(), qty: q };
        match self.proto.spec {
            // An extraction head runs its design like any other machine, and
            // then the ground has the last word.
            Spec::Extract { item, .. } => match &self.lowered {
                Some(m) => {
                    let cycle = m.cycle.max(1);
                    let give = m.gives_of(item);
                    // What the ground allows over one turn of this design.
                    //
                    // The first version of this stretched the *cycle* until
                    // the average came out right, which is arithmetically
                    // identical and plays quite differently: a head capped to
                    // a third of its design delivered three times as much,
                    // three times less often, and every machine downstream sat
                    // idle between the lumps. The campaign lost eleven percent
                    // of its gears to it.
                    //
                    // So the cycle is the design's and the *amounts* are
                    // scaled. Exact, because a head's cycle is a whole number
                    // of seconds and the cap is written in seconds.
                    let allowed = match self.rated {
                        Some(cap) => {
                            (cap as u128 * cycle as u128 / SIM_TICK_RATE as u128) as Qty
                        }
                        None => give,
                    };
                    let keep = allowed.min(give);
                    let scale = |q: Qty| match give {
                        0 => 0,
                        g => (q as u128 * keep as u128 / g as u128) as Qty,
                    };
                    (
                        // Whatever the head draws of the substance it is
                        // standing on comes out of the *ground*, and must not
                        // also be asked of a bay. Inside the machine an inlet
                        // is a boundary flow like any other -- it has to be, or
                        // the design would not balance -- and out here the
                        // boundary it crosses is the deposit.
                        //
                        // Anything else it draws is a real input: a head that
                        // washes its ore wants water delivered like everything
                        // else in the room.
                        m.takes
                            .iter()
                            .filter(|(i, _)| i != item)
                            .map(|(i, q)| amount(i, scale(*q)))
                            .collect(),
                        m.gives
                            .iter()
                            .map(|(i, q)| amount(i, if i == item { keep } else { scale(*q) }))
                            .collect(),
                        cycle,
                    )
                }
                None => (Vec::new(), Vec::new(), SIM_TICK_RATE),
            },
            Spec::Sink { .. } => {
                let item = self.item.clone().unwrap_or_default();
                (vec![amount(&item, 1)], Vec::new(), 1)
            }
            Spec::Machine { .. } => match &self.lowered {
                Some(m) => (
                    m.takes.iter().map(|(i, q)| amount(i, *q)).collect(),
                    m.gives.iter().map(|(i, q)| amount(i, *q)).collect(),
                    m.cycle,
                ),
                None => (Vec::new(), Vec::new(), SIM_TICK_RATE),
            },
            _ => (Vec::new(), Vec::new(), SIM_TICK_RATE),
        }
    }

    /// Which items this installation wants in, and which it puts out.
    pub fn wants(&self) -> Vec<String> {
        self.recipe().0.into_iter().map(|a| a.item).collect()
    }
    pub fn makes(&self) -> Vec<String> {
        self.recipe().1.into_iter().map(|a| a.item).collect()
    }

    pub fn is_storage(&self) -> bool {
        self.proto.role == Role::Storage
    }

    /// The connection points this installation actually has.
    ///
    /// Derived, never authored. A machine's ports are the boundary flows of
    /// the design inside it -- the hopper that has to be fed, the chute that
    /// has to go somewhere, the terminal the motor draws through -- read
    /// straight off the orbit `lower` compiled. That is the whole of the first
    /// change experiment 13 asks for: the outside of a machine is a fact about
    /// its inside, rather than a socket the catalogue declared.
    ///
    /// A *bay* answers with nothing, and that is not an oversight. A bay holds
    /// what is put into it and offers what it holds, so its ports are a
    /// question about the room rather than about the bay -- which is why
    /// [`World::ports_of`] exists and this does not try.
    pub fn ports(&self) -> Vec<Port> {
        if self.is_storage() {
            return Vec::new();
        }
        let (takes, gives, cycle) = self.recipe();
        let secs = super::as_secs(cycle).max(1e-9);
        let port = |a: &Amount, out: bool| Port {
            item: a.item.clone(),
            domain: lower::domain_of(&a.item),
            out,
            qty: a.qty,
            per_second: a.qty as f64 / secs,
        };
        takes
            .iter()
            .map(|a| port(a, false))
            .chain(gives.iter().map(|a| port(a, true)))
            .collect()
    }

    /// How much this bay holds. The catalogue's number unless the room said
    /// otherwise when it was furnished.
    pub fn capacity(&self) -> Qty {
        match self.proto.spec {
            Spec::Storage { capacity } => self.rated.unwrap_or(capacity),
            _ => 0,
        }
    }
}

/// A transport between two bays. Its latency is the distance between them.
#[derive(Clone, Debug)]
pub struct Haul {
    pub id: Id,
    pub proto: &'static Proto,
    pub name: String,
    pub from: Id,
    pub to: Id,
    pub item: String,
    pub placed: Tick,
    pub by: PlayerId,
}

/// One connection point on the outside of an installation.
///
/// The unit the connect UI offers and the unit [`World::connect`] validates
/// against. Two of them mate when they name the same item, which -- because an
/// item belongs to exactly one domain -- also means they are in the same
/// domain. Electricity therefore cannot arrive down a pipe, and does not have
/// to be routed through a bay to arrive at all.
#[derive(Clone, Debug, PartialEq)]
pub struct Port {
    pub item: String,
    pub domain: Domain,
    /// Out of the installation rather than into it.
    pub out: bool,
    /// Per cycle, and the same number per second, as the recipe states them.
    pub qty: Qty,
    pub per_second: f64,
}

impl Port {
    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("item", self.item.clone())
            .set("title", lower::item_title(&self.item))
            .set("domain", self.domain.tag())
            .set("unit", self.domain.unit())
            .set("out", self.out)
            .set("qty", Json::big(self.qty as u128))
            .set("perSecond", self.per_second)
    }
}

/// A connection between two installations, in the direction stuff moves.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Conn {
    pub from: Id,
    pub to: Id,
    /// The item this wire is about. Always named, because a machine with two
    /// products and two bays has to say which goes where -- and because a
    /// player who wired the wrong bay should be told at the moment they did
    /// it rather than by a factory that quietly does not start.
    pub item: String,
}

/// Something in the ground.
///
/// The world's answer to note 1 of the play session: an ore mine that produced
/// ore because the catalogue said so was the last magical object left in the
/// game, and a room that handed out free output was a room that had already
/// answered its own question.
///
/// A deposit is *terrain*. Nobody places one, nobody deletes one, and no
/// command names one -- it arrives with the room, the way a plot size does. It
/// offers an opportunity: this much of this, per second, to whatever machine
/// is standing on it. What you actually get is the smaller of that and what
/// the head you designed can lift, and which of the two is binding is the
/// decision the room is asking you about.
///
/// # One number, on purpose
///
/// Experiment 13 sketches quality, capacity and extraction properties. There
/// is one number here, and it is a rate. Depletion would make a room's answer
/// depend on how long you took, which is a different game; a quality that fed
/// the stuff model would be a good idea and is a whole pass of its own. A
/// field nothing reads is worse than a field that is not there.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Deposit {
    pub id: Id,
    /// What comes out of it.
    pub item: String,
    pub x: i32,
    pub y: i32,
    pub w: i32,
    pub h: i32,
    /// The most that can be drawn out of it in a second, however good the
    /// machine standing on it happens to be.
    pub yields: Qty,
}

impl Deposit {
    /// What this is called, in a sentence a player reads. Ground has names.
    pub fn title(&self) -> &'static str {
        match self.item.as_str() {
            "IronOre" => "ore body",
            "Coal" => "coal seam",
            "Water" => "water table",
            "Crude" => "crude field",
            "IronBillet" => "billet stock",
            _ => "deposit",
        }
    }

    pub fn bounds(&self) -> (i32, i32, i32, i32) {
        (self.x, self.y, self.x + self.w, self.y + self.h)
    }

    /// Whether a footprint touches this ground at all.
    ///
    /// Overlap rather than containment: experiment 13 says "on or beside it",
    /// and a head that has to be centred on a seam to the tile would be a
    /// placement puzzle rather than an engineering one.
    pub fn touches(&self, x: i32, y: i32, w: i32, h: i32) -> bool {
        let (ax0, ay0, ax1, ay1) = self.bounds();
        x < ax1 && ax0 < x + w && y < ay1 && ay0 < y + h
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("id", Json::big(self.id as u128))
            .set("item", self.item.clone())
            .set("itemTitle", lower::item_title(&self.item))
            .set("title", self.title())
            .set("domain", lower::domain_of(&self.item).tag())
            .set("x", self.x as i64)
            .set("y", self.y as i64)
            .set("w", self.w as i64)
            .set("h", self.h as i64)
            .set("yields", Json::big(self.yields as u128))
    }
}

/// The buffer a direct connection implies.
///
/// When two machines are wired to each other there is still something between
/// them -- a chute, a hopper, a length of pipe -- and the solver has always had
/// a word for that: a storage node. What changed is who has to put it there.
/// The player used to; now the compiler does, sized from what actually passes
/// through it and named after the connection that made it.
///
/// It is not in the document, and it is not addressable: no command names one,
/// nothing can be built on one, and deleting the connection deletes it. It is
/// what the room *means*, derived the same way a machine's footprint is
/// derived from the design inside it.
#[derive(Clone, Debug)]
pub struct Bridge {
    /// A synthetic identity, taken from the top of the range so that it can
    /// never collide with one the document handed out -- ids are assigned by
    /// replaying the log from 1, and a room would need eighteen quintillion
    /// buildings in it to meet these coming the other way.
    pub id: Id,
    pub name: String,
    pub item: String,
    pub from: Id,
    pub to: Id,
    pub capacity: Qty,
}

/// How many cycles of the larger side a derived buffer holds.
///
/// Small on purpose. A derived buffer exists so that a direct connection has
/// somewhere to put a cycle's worth of output while the other end is busy; it
/// is not a stockpile, and a player who wants one of those places a bay. Four
/// is enough that neither end stalls on granularity and few enough that
/// nobody can use a connection as free storage.
const BRIDGE_CYCLES: Qty = 4;

#[derive(Clone, Debug, Default)]
pub struct World {
    pub name: String,
    pub installs: Vec<Install>,
    /// What is in the ground here. Terrain, not buildings: nothing places one
    /// and nothing deletes one, and the whole document is a function of the
    /// room plus the command log exactly as before.
    pub deposits: Vec<Deposit>,
    pub hauls: Vec<Haul>,
    pub conns: Vec<Conn>,
    /// The next identity to hand out. Part of the document, because ids are
    /// assigned by replaying the log and must be the same on every replica.
    pub next_id: Id,
    /// The side of this plot, in tiles. Zero means the default.
    ///
    /// Prototype 2 had one plot size because it had one room. A campaign of
    /// five has five, and "very constrained footprint" is a sentence a room
    /// can only say by being small.
    pub plot: i32,
}

impl World {
    pub fn new(name: &str) -> World {
        World { name: name.to_string(), next_id: 1, plot: PLOT, ..World::default() }
    }

    /// The side of this plot, defaulted for a document that predates the field.
    pub fn plot(&self) -> i32 {
        if self.plot > 0 {
            self.plot
        } else {
            PLOT
        }
    }

    pub fn get(&self, id: Id) -> Option<&Install> {
        self.installs.iter().find(|i| i.id == id)
    }
    pub fn get_mut(&mut self, id: Id) -> Option<&mut Install> {
        self.installs.iter_mut().find(|i| i.id == id)
    }
    /// Put something in the ground.
    ///
    /// Only a room furnishing itself calls this, and only before anybody has
    /// joined: what is under the plot is part of what the room *is*, not part
    /// of what was done to it, so it arrives with the starting document rather
    /// than as a command. Deposits take identities from the same counter as
    /// buildings, so nothing anywhere has to know which kind of thing an id
    /// belongs to.
    pub fn seam(&mut self, item: &str, x: i32, y: i32, w: i32, h: i32, yields: Qty) -> Id {
        let id = self.next_id;
        self.next_id += 1;
        self.deposits.push(Deposit {
            id,
            item: item.to_string(),
            x,
            y,
            w,
            h,
            yields,
        });
        id
    }

    /// The n-th patch of ground of one kind, in document order.
    ///
    /// For a harness that wants to build a factory without a pointer. A player
    /// finds ground by looking at it.
    pub fn nth_ground(&self, item: &str, n: usize) -> Option<&Deposit> {
        self.deposits.iter().filter(|d| d.item == item).nth(n)
    }

    /// The ground under a footprint, if any of it is worth standing on.
    pub fn ground(&self, item: &str, x: i32, y: i32, w: i32, h: i32) -> Option<&Deposit> {
        self.deposits.iter().find(|d| d.item == item && d.touches(x, y, w, h))
    }

    /// The deposit one installation is standing on.
    pub fn under(&self, id: Id) -> Option<&Deposit> {
        let i = self.get(id)?;
        let item = i.proto.extracts()?;
        let (w, h) = i.size();
        self.ground(item, i.x, i.y, w, h)
    }

    /// The heads standing on one patch of ground, in placement order.
    pub fn worked(&self, d: &Deposit) -> Vec<&Install> {
        let mut on: Vec<&Install> = self
            .installs
            .iter()
            .filter(|i| {
                i.proto.extracts() == Some(d.item.as_str()) && {
                    let (w, h) = i.size();
                    d.touches(i.x, i.y, w, h)
                }
            })
            .collect();
        on.sort_by_key(|i| i.id);
        on
    }

    /// What one head's design could lift in a second, before the ground has
    /// its say. Integer throughout, because a rate that accumulates in a float
    /// is a desynchronisation with a delay fuse in it.
    fn appetite(i: &Install) -> Qty {
        let Some(item) = i.proto.extracts() else { return 0 };
        let Some(m) = &i.lowered else { return 0 };
        let give = m.gives_of(item);
        if m.cycle == 0 {
            return 0;
        }
        (give as u128 * SIM_TICK_RATE as u128 / m.cycle as u128) as Qty
    }

    /// Share every patch of ground out among the heads standing on it.
    ///
    /// First come, first served, in the order the heads were placed: each
    /// takes what its design can lift until the seam is spent, and one that
    /// arrives after the ground is spoken for turns at nothing until something
    /// in front of it is taken down.
    ///
    /// Equal shares were the other option and are worse to play against: a new
    /// head would quietly slow every head already standing there, and a player
    /// would watch a factory they had not touched get worse. This way the only
    /// thing a new head can do is take what is left.
    ///
    /// Deterministic, because ids are handed out by replaying the log and
    /// every replica replays the same one. Called from the two places that can
    /// change the answer -- a placement and a removal -- and nowhere else.
    fn reprice(&mut self) {
        let ground: Vec<(Id, Qty)> =
            self.deposits.iter().map(|d| (d.id, d.yields)).collect();
        for (did, yields) in ground {
            let Some(d) = self.deposits.iter().find(|d| d.id == did) else { continue };
            let ids: Vec<Id> = self.worked(d).iter().map(|i| i.id).collect();
            let mut left = yields;
            for id in ids {
                let Some(k) = self.installs.iter().position(|i| i.id == id) else { continue };
                let want = Self::appetite(&self.installs[k]);
                let share = want.min(left);
                left -= share;
                self.installs[k].rated = Some(share);
            }
        }
    }

    pub fn haul(&self, id: Id) -> Option<&Haul> {
        self.hauls.iter().find(|h| h.id == id)
    }
    pub fn named(&self, name: &str) -> Option<&Install> {
        self.installs.iter().find(|i| i.name == name)
    }

    /// The bounding box of everything placed, in tiles, and its area. The
    /// space goals are asked about this.
    pub fn extent(&self) -> (i32, i32, i32, i32) {
        let mut b = (i32::MAX, i32::MAX, i32::MIN, i32::MIN);
        for i in &self.installs {
            let (x0, y0, x1, y1) = i.bounds();
            b = (b.0.min(x0), b.1.min(y0), b.2.max(x1), b.3.max(y1));
        }
        if b.0 > b.2 {
            (0, 0, 0, 0)
        } else {
            b
        }
    }

    pub fn footprint(&self) -> i64 {
        let (x0, y0, x1, y1) = self.extent();
        ((x1 - x0) as i64) * ((y1 - y0) as i64)
    }

    // ------------------------------------------------------------ placing

    /// Whether something of this size may stand here.
    ///
    /// The whole of the placement rule, and it is deliberately blunt: inside
    /// the plot, and touching nothing. There is no service clearance in the
    /// world layer because clearance is the *machine* designer's problem, and
    /// having two different opinions about how close is too close is how a
    /// player learns not to trust either of them.
    pub fn free(&self, x: i32, y: i32, w: i32, h: i32, ignore: Option<Id>) -> Result<(), String> {
        let plot = self.plot();
        if x < 0 || y < 0 || x + w > plot || y + h > plot {
            return Err(format!("{w}x{h} at {x},{y} is off the plot"));
        }
        for i in &self.installs {
            if Some(i.id) == ignore {
                continue;
            }
            let (bx0, by0, bx1, by1) = i.bounds();
            if x < bx1 && bx0 < x + w && y < by1 && by0 < y + h {
                return Err(format!("that overlaps {}", i.name));
            }
        }
        Ok(())
    }

    /// Put something in the world, and name it.
    pub fn place(
        &mut self,
        proto: &'static Proto,
        x: i32,
        y: i32,
        face: u8,
        item: Option<String>,
        design: Option<Design>,
        at: Tick,
        by: PlayerId,
    ) -> Result<Id, String> {
        if face > 3 {
            return Err(format!("there are four ways to face, not {face}"));
        }
        if proto.role == Role::Transport {
            return Err("a transport is created between two bays, not placed".into());
        }
        // `designed()` rather than `Role::Machine`, because an extraction head
        // is a designed thing too: it is a machine that happens to stand on a
        // deposit, and how much of the seam it lifts is a fact about the
        // design inside it.
        //
        // No design is a *chassis*: a footprint with nothing inside it,
        // standing there not running until somebody designs it. It used to be
        // a refusal, and one level up it used to be quietly filled in with the
        // catalogue's answer -- which is what note 7 of the play session meant
        // by prebuilt machines taking the fun out of the game entirely.
        let lowered = match (proto.role.designed(), &design) {
            (true, Some(d)) => Some(lower::lower(d)?),
            _ => None,
        };
        if let Spec::Sink { item: fixed, .. } = proto.spec {
            match (&fixed, &item) {
                (Some(_), _) => {}
                (None, Some(i)) if lower::ITEMS.contains(&i.as_str()) => {}
                (None, _) => return Err("a depot ships one item, and none was named".into()),
            }
        }
        let id = self.next_id;
        let mut inst = Install {
            id,
            proto,
            name: format!("{}{}", proto.short, id),
            x,
            y,
            face,
            item: match proto.spec {
                Spec::Sink { item: Some(fixed), .. } => Some(fixed.to_string()),
                Spec::Sink { item: None, .. } | Spec::Storage { .. } => item,
                _ => None,
            },
            rated: None,
            design,
            lowered,
            draft: None,
            editor: None,
            placed: at,
            by,
        };
        let (w, h) = inst.size();
        self.free(x, y, w, h, None)?;
        // An extraction head has to stand on something worth extracting. This
        // is the whole of note 1: the world offers an opportunity and the
        // machine is how well you take it, so a head in the middle of an empty
        // field is a refusal with a reason on it rather than a building that
        // quietly produces nothing.
        if let Some(item) = proto.extracts() {
            let Some(d) = self.ground(item, x, y, w, h) else {
                return Err(format!(
                    "a {} has to stand on {}, and there is none here",
                    proto.title,
                    match item {
                        "IronOre" => "an ore body",
                        "Coal" => "a coal seam",
                        "Water" => "a water table",
                        "Crude" => "a crude field",
                        "IronBillet" => "billet stock",
                        other => other,
                    }
                ));
            };
            // What is left of it, so that a head placed on ground somebody has
            // already drained is refused where the player can see it rather
            // than standing there turning at nothing.
            let taken: Qty = self.worked(d).iter().map(|i| Self::appetite(i)).sum();
            if taken >= d.yields {
                return Err(format!(
                    "this {} is already spoken for: {} of {} a second",
                    d.title(),
                    taken.min(d.yields),
                    d.yields
                ));
            }
            // A placeholder. `reprice` below hands out the real share once the
            // building is actually standing there.
            inst.rated = Some(0);
        }
        inst.name = format!("{}{}", proto.short, id);
        let extracts = inst.proto.extracts().is_some();
        self.installs.push(inst);
        self.next_id += 1;
        if extracts {
            self.reprice();
        }
        Ok(id)
    }

    /// Rate one installation differently from its catalogue entry.
    ///
    /// Only a room furnishing itself calls this, and only before anybody has
    /// joined: it is part of what the room *is*, not part of what was done to
    /// it, so it arrives with the starting document rather than as a command.
    pub fn rate(&mut self, id: Id, rated: Option<Qty>) {
        if let Some(i) = self.get_mut(id) {
            i.rated = rated;
        }
    }

    /// Take something out, along with every wire and every transport that
    /// touched it. Returns what was removed, so a ghost can be left behind.
    pub fn remove(&mut self, id: Id) -> Result<Install, String> {
        let Some(k) = self.installs.iter().position(|i| i.id == id) else {
            return Err("there is nothing there to delete".into());
        };
        let gone = self.installs.remove(k);
        self.conns.retain(|c| c.from != id && c.to != id);
        self.hauls.retain(|h| h.from != id && h.to != id);
        // Taking a head down gives its share of the seam back to whatever is
        // still standing on it.
        if gone.proto.extracts().is_some() {
            self.reprice();
        }
        Ok(gone)
    }

    // ----------------------------------------------------------- wiring

    /// Wire a bay to a machine, or a machine to a bay.
    /// The ports a bay has, which is a question about the room rather than
    /// about the bay.
    ///
    /// A bay holds what is put into it and offers what it holds, so it has no
    /// ports of its own: it has the ones the room has given it. An import yard
    /// declares its item because a train fills it from a simulation this
    /// document has never heard of; everything else a bay can offer is
    /// something already wired into it.
    ///
    /// This is what makes "never ask a question whose answer is already
    /// determined" possible: a bay with one item in it has one port, and
    /// connecting to it needs no menu.
    pub fn ports_of(&self, id: Id) -> Vec<Port> {
        let Some(i) = self.get(id) else { return Vec::new() };
        if !i.is_storage() {
            return i.ports();
        }
        let mut items: Vec<String> = Vec::new();
        let mut note = |item: &str| {
            if !items.iter().any(|i| i == item) {
                items.push(item.to_string());
            }
        };
        if let Some(item) = &i.item {
            note(item);
        }
        for c in self.conns.iter().filter(|c| c.to == id || c.from == id) {
            note(&c.item);
        }
        for h in self.hauls.iter().filter(|h| h.to == id || h.from == id) {
            note(&h.item);
        }
        // Both ways round, and with no rate: what a bay can pass through is
        // decided by what fills it and what draws on it, and neither of those
        // is the bay's own number.
        items
            .iter()
            .flat_map(|item| {
                [false, true].map(|out| Port {
                    item: item.clone(),
                    domain: lower::domain_of(item),
                    out,
                    qty: 0,
                    per_second: 0.0,
                })
            })
            .collect()
    }

    /// Join two installations, in the direction stuff moves.
    ///
    /// # What used to be here, and why it is not
    ///
    /// This function used to open with a rule:
    ///
    /// ```text
    ///   two machines cannot be wired together -- route them through a bay
    /// ```
    ///
    /// It was logically immaculate. Every flow in the room passed through a
    /// declared buffer, every buffer was a node the solver already understood,
    /// and the whole factory was one alternating chain of machine and bay. It
    /// was also, in the words of the play session that found it, *work*: the
    /// player did not want a warehouse between a crusher and a smelter, and
    /// they very much did not want one between a generator and the thing it
    /// was powering. Electricity does not queue in a shed.
    ///
    /// So the rule is gone, and what replaces it is the ports the two ends
    /// actually have. A connection is legal when the producer really produces
    /// that item and the consumer really consumes it -- facts derived from the
    /// designs inside them, not from the catalogue -- and since an item
    /// belongs to exactly one domain, matching the item matches the domain.
    ///
    /// The buffer has not disappeared; it has stopped being the player's
    /// problem. [`World::compile`] puts a small one in wherever two machines
    /// are joined directly, sized from what passes through it. A bay is now
    /// something you place because you *want* storage -- a stockpile, a surge
    /// tank, something for a train to load from -- rather than punctuation the
    /// parser demanded.
    pub fn connect(&mut self, from: Id, to: Id, item: &str) -> Result<(), String> {
        if from == to {
            return Err("a connection has to go somewhere".into());
        }
        let a = self.get(from).ok_or("one end of that connection is not there")?;
        let b = self.get(to).ok_or("one end of that connection is not there")?;
        // The one pairing that is still refused, and for a reason that has
        // nothing to do with buffering: two bays are joined by a *transport*,
        // which has a length, a fleet and a latency. A wire between them would
        // be a belt with none of those.
        if a.is_storage() && b.is_storage() {
            return Err("two bays are joined by a transport, not a wire".into());
        }
        if !lower::ITEMS.contains(&item) {
            return Err(format!("`{item}` is not an item"));
        }
        if self.conns.iter().any(|c| c.from == from && c.to == to && c.item == item) {
            return Err("that connection is already there".into());
        }
        // The ports. A bay is exempt at both ends because it has no opinion
        // about items until the room gives it one.
        if !a.is_storage() && !a.makes().iter().any(|i| i == item) {
            return Err(format!(
                "{} has no {} output",
                a.name,
                lower::item_title(item)
            ));
        }
        if !b.is_storage() && !b.wants().iter().any(|i| i == item) {
            return Err(format!(
                "{} has no {} input",
                b.name,
                lower::item_title(item)
            ));
        }
        // Arbitration, unchanged in meaning and now stated about ports rather
        // than about bays. A machine that could draw one item from two places
        // needs a rule nobody has declared, and a machine whose output went two
        // ways would need a split nobody has sized -- so both are refused here,
        // where the refusal can name the connection somebody just drew, rather
        // than by a factory that quietly does not start.
        if !b.is_storage() && self.conns.iter().any(|c| c.to == to && c.item == item) {
            return Err(format!(
                "{} already has somewhere to draw {} from",
                b.name,
                lower::item_title(item)
            ));
        }
        if !a.is_storage() && self.conns.iter().any(|c| c.from == from && c.item == item) {
            return Err(format!(
                "{}'s {} already goes somewhere",
                a.name,
                lower::item_title(item)
            ));
        }
        self.conns.push(Conn { from, to, item: item.to_string() });
        Ok(())
    }

    pub fn disconnect(&mut self, from: Id, to: Id, item: &str) -> Result<(), String> {
        let before = self.conns.len();
        self.conns.retain(|c| !(c.from == from && c.to == to && c.item == item));
        if self.conns.len() == before {
            Err("that wire is not there".into())
        } else {
            Ok(())
        }
    }

    /// Run a transport between two bays. Its latency is their distance apart.
    pub fn link(
        &mut self,
        proto: &'static Proto,
        from: Id,
        to: Id,
        item: &str,
        at: Tick,
        by: PlayerId,
    ) -> Result<Id, String> {
        if proto.role != Role::Transport {
            return Err(format!("a {} is not a transport", proto.title));
        }
        let a = self.get(from).ok_or("one end of that transport is not there")?;
        let b = self.get(to).ok_or("one end of that transport is not there")?;
        if !a.is_storage() || !b.is_storage() {
            return Err("a transport runs between two bays".into());
        }
        if from == to {
            return Err("a transport has to go somewhere".into());
        }
        if !lower::ITEMS.contains(&item) {
            return Err(format!("`{item}` is not an item"));
        }
        if self.hauls.iter().any(|h| h.from == from && h.to == to && h.item == item) {
            return Err("that transport is already running".into());
        }
        if self.hauls.iter().any(|h| h.to == to && h.item == item)
            || self.conns.iter().any(|c| c.to == to && c.item == item)
        {
            // Same rule as a wire: one deliverer of one item per bay.
            return Err(format!(
                "something already delivers {} to {}",
                lower::item_title(item),
                b.name
            ));
        }
        let id = self.next_id;
        self.hauls.push(Haul {
            id,
            proto,
            name: format!("{}{}", proto.short, id),
            from,
            to,
            item: item.to_string(),
            placed: at,
            by,
        });
        self.next_id += 1;
        Ok(id)
    }

    pub fn unlink(&mut self, id: Id) -> Result<Haul, String> {
        let Some(k) = self.hauls.iter().position(|h| h.id == id) else {
            return Err("there is no transport there to delete".into());
        };
        Ok(self.hauls.remove(k))
    }

    /// Every buffer this document implies, in document order.
    ///
    /// One per connection that joins two machines directly. Deterministic in
    /// both its order and its names, because every replica compiles the same
    /// document and the simulator carries state by name: a buffer that was
    /// called something else on one client would be a different factory.
    pub fn bridges(&self) -> Vec<Bridge> {
        let mut out = Vec::new();
        for (k, c) in self.conns.iter().enumerate() {
            let (Some(a), Some(z)) = (self.get(c.from), self.get(c.to)) else { continue };
            if a.is_storage() || z.is_storage() {
                continue;
            }
            // Enough for a few cycles of whichever end moves more at a time,
            // so that granularity alone never stalls the pair.
            let per = |i: &Install, out: bool| -> Qty {
                let (takes, gives, _) = i.recipe();
                let side = if out { gives } else { takes };
                side.iter().find(|a| a.item == c.item).map(|a| a.qty).unwrap_or(0)
            };
            let capacity = per(a, true).max(per(z, false)).saturating_mul(BRIDGE_CYCLES).max(1);
            out.push(Bridge {
                id: Id::MAX - k as Id,
                name: format!("{}_{}_{}", a.name, z.name, c.item),
                item: c.item.clone(),
                from: c.from,
                to: c.to,
                capacity,
            });
        }
        out
    }

    /// Distance between the two ends of a transport, in the units the language
    /// derives latency from. Tenths of a tile, taxicab, because a belt goes
    /// round corners.
    pub fn span(&self, h: &Haul) -> u64 {
        let (Some(a), Some(b)) = (self.get(h.from), self.get(h.to)) else { return 0 };
        let (ax, ay) = a.centre();
        let (bx, by) = b.centre();
        (((ax - bx).abs() + (ay - by).abs()) as u64) * 100
    }

    // --------------------------------------------------------- compiling

    /// The document, as a plant the solver can run.
    pub fn compile(&self) -> Build {
        let mut b = Build::default();
        let mut g = Graph { name: self.name.clone(), ..Graph::default() };
        g.items = lower::ITEMS.iter().map(|s| s.to_string()).collect();

        // Every buffer the document implies but does not contain: one per
        // connection that joins two machines directly. From here down they are
        // storages like any other, which is the point -- the commissioning
        // check, the wiring and the solver never learn that the player did not
        // place them.
        let bridges = self.bridges();
        let bridge_of: BTreeMap<(Id, Id, String), &Bridge> =
            bridges.iter().map(|b| ((b.from, b.to, b.item.clone()), b)).collect();
        let bridge = |c: &Conn| bridge_of.get(&(c.from, c.to, c.item.clone())).copied();

        // Every actor's wiring, before anything is dropped.
        let mut feeds: BTreeMap<Id, Vec<(Id, String)>> = BTreeMap::new();
        let mut deposits: BTreeMap<Id, Vec<(Id, String)>> = BTreeMap::new();
        for c in &self.conns {
            let Some(a) = self.get(c.from) else { continue };
            match bridge(c) {
                // Machine to machine: the producer deposits into the derived
                // buffer and the consumer draws from it, so the fixpoint below
                // sees exactly what it would have seen had somebody placed a
                // bay there by hand.
                Some(b) => {
                    deposits.entry(c.from).or_default().push((b.id, c.item.clone()));
                    feeds.entry(c.to).or_default().push((b.id, c.item.clone()));
                }
                None if a.is_storage() => {
                    feeds.entry(c.to).or_default().push((c.from, c.item.clone()))
                }
                None => deposits.entry(c.from).or_default().push((c.to, c.item.clone())),
            }
        }
        for h in &self.hauls {
            feeds.entry(h.id).or_default().push((h.from, h.item.clone()));
            deposits.entry(h.id).or_default().push((h.to, h.item.clone()));
        }

        // What every actor is asking for and offering.
        let mut wants: BTreeMap<Id, Vec<String>> = BTreeMap::new();
        let mut makes: BTreeMap<Id, Vec<String>> = BTreeMap::new();
        let mut actors: Vec<Id> = Vec::new();
        for i in &self.installs {
            if i.is_storage() {
                continue;
            }
            actors.push(i.id);
            wants.insert(i.id, i.wants());
            makes.insert(i.id, i.makes());
        }
        for h in &self.hauls {
            actors.push(h.id);
            wants.insert(h.id, vec![h.item.clone()]);
            makes.insert(h.id, vec![h.item.clone()]);
        }
        actors.sort_unstable();

        // Bays that are filled from outside the room. Nothing in this document
        // delivers to them and nothing ever will: a train does, and the train
        // is in another simulation. They are seeded into the commissioning
        // check as though they had a supplier, because they have one.
        let outside: BTreeMap<Id, String> = self
            .installs
            .iter()
            .filter(|i| i.is_storage())
            .filter_map(|i| i.item.clone().map(|item| (i.id, item)))
            .collect();

        // The fixpoint. Dropping a machine can empty the bay that fed the next
        // one, so this runs until nobody else has to go.
        let mut live: BTreeSet<Id> = actors.iter().copied().collect();
        loop {
            let mut slots: BTreeMap<Id, BTreeSet<String>> = BTreeMap::new();
            for (bay, item) in &outside {
                slots.entry(*bay).or_default().insert(item.clone());
            }
            for &a in &live {
                for (bay, item) in deposits.get(&a).into_iter().flatten() {
                    slots.entry(*bay).or_default().insert(item.clone());
                }
            }
            let mut cut: Option<(Id, String)> = None;
            for &a in &live {
                let no_feed = feeds.get(&a).map(Vec::as_slice).unwrap_or(&[]);
                let no_dep = deposits.get(&a).map(Vec::as_slice).unwrap_or(&[]);
                for item in &wants[&a] {
                    let n = no_feed
                        .iter()
                        .filter(|(bay, _)| {
                            slots.get(bay).is_some_and(|s| s.contains(item))
                        })
                        .count();
                    if n != 1 {
                        cut = Some((
                            a,
                            if n == 0 {
                                format!("nothing delivers {} to a bay wired into it", lower::item_title(item))
                            } else {
                                format!("{n} bays could supply its {}", lower::item_title(item))
                            },
                        ));
                        break;
                    }
                }
                if cut.is_some() {
                    break;
                }
                for item in &makes[&a] {
                    let n = no_dep.iter().filter(|(_, i)| i == item).count();
                    if n != 1 {
                        cut = Some((
                            a,
                            if n == 0 {
                                format!("its {} has nowhere to go", lower::item_title(item))
                            } else {
                                format!("its {} is wired to {n} bays", lower::item_title(item))
                            },
                        ));
                        break;
                    }
                }
                if cut.is_some() {
                    break;
                }
                if wants[&a].is_empty() && makes[&a].is_empty() {
                    cut = Some((
                        a,
                        match self.get(a) {
                            // A head standing on ground that is already fully
                            // worked. Saying so is the difference between a
                            // puzzle and a bug.
                            Some(i) if i.proto.extracts().is_some() => {
                                "the ground under it is already spoken for".to_string()
                            }
                            Some(i) if i.proto.role.designed() && i.design.is_none() => {
                                "nothing has been designed inside it yet".to_string()
                            }
                            _ => "it neither consumes nor produces anything".to_string(),
                        },
                    ));
                    break;
                }
            }
            match cut {
                Some((a, why)) => {
                    live.remove(&a);
                    b.idle.push((a, why));
                }
                None => break,
            }
        }

        // ---- nodes, in a stable order ----------------------------------
        for i in &self.installs {
            if !i.is_storage() {
                continue;
            }
            if i.proto.role != Role::Storage {
                continue;
            }
            let mut n = Node::new(&i.name, Kind::Storage);
            n.capacity = i.capacity();
            n.policy = Policy::RoundRobin;
            // A bay holds exactly what is put into it, and what is put into an
            // import yard comes from another room -- so the slot has to be
            // declared rather than derived. An empty declaration is the honest
            // one: the yard starts empty, because the first train has not
            // arrived yet.
            if let Some(item) = &i.item {
                n.holds.push(item.clone());
            }
            g.nodes.push(n);
        }
        // The derived ones, emitted here because that is what they are: a
        // storage between two machines, which is the shape the player used to
        // have to build. Only where both ends are running -- a buffer between
        // two machines that are not turning is a node with nothing to say.
        for b in &bridges {
            if !live.contains(&b.from) || !live.contains(&b.to) {
                continue;
            }
            let mut n = Node::new(&b.name, Kind::Storage);
            n.capacity = b.capacity;
            n.policy = Policy::RoundRobin;
            n.holds.push(b.item.clone());
            g.nodes.push(n);
        }
        for i in &self.installs {
            if i.is_storage() || !live.contains(&i.id) {
                continue;
            }
            let (inputs, outputs, duration) = i.recipe();
            // A head that needs nothing is a source; one whose design draws
            // power -- an electric mining head, which is exactly the thing
            // experiment 13 wants somebody to build -- is a process like any
            // other. The role says where it sits in the room; the recipe says
            // what kind of node it is.
            let kind = match i.proto.role {
                Role::Source if inputs.is_empty() => Kind::Source,
                Role::Sink => Kind::Sink,
                _ => Kind::Process,
            };
            let mut n = Node::new(&i.name, kind);
            n.inputs = inputs;
            n.outputs = outputs;
            n.duration = duration;
            n.count = match i.proto.spec {
                Spec::Sink { count, .. } => count,
                _ => 1,
            };
            g.nodes.push(n);
            b.running.push(i.id);
        }
        for h in &self.hauls {
            if !live.contains(&h.id) {
                continue;
            }
            let Spec::Transport { load, vehicles, speed, base } = h.proto.spec else { continue };
            let mut n = Node::new(&h.name, Kind::Link);
            n.inputs = vec![Amount { item: h.item.clone(), qty: load }];
            n.outputs = n.inputs.clone();
            n.count = vehicles;
            let geo = Geometry { base, distance: self.span(h), speed };
            n.duration = geo.latency();
            n.returns = geo.latency();
            n.geometry = Some(geo);
            g.nodes.push(n);
            b.running.push(h.id);
        }

        // ---- wires ------------------------------------------------------
        //
        // A deposit names its item; a withdrawal does not. That asymmetry is
        // the language's: a bay holds what is put into it, so an input-side
        // qualifier that named an item nobody delivers would be a compile
        // error rather than a filter.
        for c in &self.conns {
            let (Some(a), Some(z)) = (self.get(c.from), self.get(c.to)) else { continue };
            if let Some(b) = bridge(c) {
                // Two edges and a buffer, where the document has one line.
                if !live.contains(&b.from) || !live.contains(&b.to) {
                    continue;
                }
                g.edges.push(Edge {
                    from: a.name.clone(),
                    to: b.name.clone(),
                    item: Some(c.item.clone()),
                });
                g.edges.push(Edge { from: b.name.clone(), to: z.name.clone(), item: None });
                continue;
            }
            let actor = if a.is_storage() { z.id } else { a.id };
            if !live.contains(&actor) {
                continue;
            }
            g.edges.push(Edge {
                from: a.name.clone(),
                to: z.name.clone(),
                item: (!a.is_storage()).then(|| c.item.clone()),
            });
        }
        for h in &self.hauls {
            if !live.contains(&h.id) {
                continue;
            }
            let (Some(a), Some(z)) = (self.get(h.from), self.get(h.to)) else { continue };
            g.edges.push(Edge { from: a.name.clone(), to: h.name.clone(), item: None });
            g.edges.push(Edge {
                from: h.name.clone(),
                to: z.name.clone(),
                item: Some(h.item.clone()),
            });
        }

        // Only the items anybody mentions, so the plant reads like a plant.
        let mut used: BTreeSet<String> = BTreeSet::new();
        for n in &g.nodes {
            for a in n.inputs.iter().chain(n.outputs.iter()) {
                used.insert(a.item.clone());
            }
            // An import yard's item is mentioned by nothing else in the plant
            // until somebody wires a machine to it, and a `holds` clause naming
            // an item the file never declared is a parse error.
            used.extend(n.holds.iter().cloned());
        }
        g.items.retain(|i| used.contains(i));
        b.runnable = g.nodes.iter().any(|n| n.kind.is_machine());
        b.graph = g;
        b
    }

    /// A canonical encoding of the document, for comparing one replica's idea
    /// of the world with another's.
    ///
    /// Everything a command can change is in here, including the drafts that
    /// no simulation will ever see: two clients that disagree about what is
    /// being designed have desynchronised, even if their factories agree.
    pub fn signature(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(self.name.as_bytes());
        v.extend_from_slice(&self.next_id.to_le_bytes());
        v.extend_from_slice(&self.plot().to_le_bytes());
        // Terrain first. Nothing a command can do changes it, and that is
        // exactly why it belongs in here: two replicas that disagreed about
        // what was in the ground would agree about every command and build
        // different factories.
        let mut ground: Vec<&Deposit> = self.deposits.iter().collect();
        ground.sort_by_key(|d| d.id);
        for d in ground {
            v.extend_from_slice(&d.id.to_le_bytes());
            v.extend_from_slice(d.item.as_bytes());
            for n in [d.x, d.y, d.w, d.h] {
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.extend_from_slice(&d.yields.to_le_bytes());
            v.push(0xf4);
        }
        let mut installs: Vec<&Install> = self.installs.iter().collect();
        installs.sort_by_key(|i| i.id);
        for i in installs {
            v.extend_from_slice(&i.id.to_le_bytes());
            v.extend_from_slice(i.proto.tag.as_bytes());
            v.extend_from_slice(i.name.as_bytes());
            for n in [i.x, i.y, i.face as i32] {
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.extend_from_slice(i.item.as_deref().unwrap_or("-").as_bytes());
            v.extend_from_slice(&i.rated.unwrap_or(0).to_le_bytes());
            if let Some(d) = &i.design {
                v.extend_from_slice(d.emit().as_bytes());
            }
            v.push(0xf0);
            if let Some(d) = &i.draft {
                v.extend_from_slice(d.emit().as_bytes());
            }
            v.extend_from_slice(&i.editor.unwrap_or(u32::MAX).to_le_bytes());
            v.extend_from_slice(&i.placed.to_le_bytes());
            v.push(0xf1);
        }
        let mut hauls: Vec<&Haul> = self.hauls.iter().collect();
        hauls.sort_by_key(|h| h.id);
        for h in hauls {
            v.extend_from_slice(&h.id.to_le_bytes());
            v.extend_from_slice(h.proto.tag.as_bytes());
            v.extend_from_slice(&h.from.to_le_bytes());
            v.extend_from_slice(&h.to.to_le_bytes());
            v.extend_from_slice(h.item.as_bytes());
            v.push(0xf2);
        }
        let mut conns: Vec<&Conn> = self.conns.iter().collect();
        conns.sort_by(|a, b| (a.from, a.to, &a.item).cmp(&(b.from, b.to, &b.item)));
        for c in conns {
            v.extend_from_slice(&c.from.to_le_bytes());
            v.extend_from_slice(&c.to.to_le_bytes());
            v.extend_from_slice(c.item.as_bytes());
            v.push(0xf3);
        }
        v
    }

    // ------------------------------------------------------------- wire

    /// The document, for a client or for a snapshot.
    ///
    /// `designs` is the difference between the two. A snapshot has to carry
    /// every machine's design, because that is what the receiving replica
    /// rebuilds its recipe and its footprint from; a *frame* does not, because
    /// the browser asks for one design at a time when it opens a machine, and
    /// sending five of them sixty times a minute is a few hundred kilobytes a
    /// second of something nobody is reading.
    pub fn to_json(&self, build: &Build, designs: bool) -> Json {
        let idle: BTreeMap<Id, &String> = build.idle.iter().map(|(id, why)| (*id, why)).collect();
        Json::obj()
            .set("name", self.name.clone())
            .set("nextId", Json::big(self.next_id as u128))
            .set("plot", self.plot() as i64)
            .set(
                "installs",
                Json::Arr(
                    self.installs
                        .iter()
                        .map(|i| {
                            let (w, h) = i.size();
                            Json::obj()
                                .set("id", Json::big(i.id as u128))
                                .set("proto", i.proto.tag)
                                .set("title", i.proto.title)
                                .set("role", i.proto.role.word())
                                .set("name", i.name.clone())
                                .set("x", i.x as i64)
                                .set("y", i.y as i64)
                                .set("w", w as i64)
                                .set("h", h as i64)
                                .set("face", i.face as i64)
                                .set("item", i.item.clone())
                                .set("rated", i.rated.map(|q| Json::big(q as u128)))
                                .set(
                                    "capacity",
                                    (i.proto.role == Role::Storage)
                                        .then(|| Json::big(i.capacity() as u128)),
                                )
                                .set("placedAt", i.placed)
                                .set("placedBy", i.by as i64)
                                .set("editor", i.editor.map(|p| Json::Int(p as i128)))
                                .set("hasDraft", i.draft.is_some())
                                .set(
                                    "draft",
                                    match (&i.draft, designs) {
                                        (Some(d), true) => d.to_json(),
                                        _ => Json::Null,
                                    },
                                )
                                .set(
                                    "design",
                                    match (&i.design, designs) {
                                        (Some(d), true) => d.to_json(),
                                        _ => Json::Null,
                                    },
                                )
                                .set("wants", Json::arr(i.wants()))
                                .set("makes", Json::arr(i.makes()))
                                // The connection points this thing actually
                                // has, which is what the connect UI offers and
                                // what a wire is drawn between. A bay's are a
                                // question about the room, so they come from
                                // the world rather than from the building.
                                .set(
                                    "ports",
                                    Json::Arr(
                                        self.ports_of(i.id).iter().map(|p| p.to_json()).collect(),
                                    ),
                                )
                                .set("running", build.running.contains(&i.id))
                                .set("idle", idle.get(&i.id).map(|s| s.to_string()))
                                .set(
                                    "macro",
                                    match &i.lowered {
                                        Some(m) => m.to_json(),
                                        None => Json::Null,
                                    },
                                )
                        })
                        .collect(),
                ),
            )
            .set(
                "hauls",
                Json::Arr(
                    self.hauls
                        .iter()
                        .map(|h| {
                            let geo = match h.proto.spec {
                                Spec::Transport { speed, base, load, vehicles } => {
                                    let g = Geometry { base, distance: self.span(h), speed };
                                    Json::obj()
                                        .set("latency", g.latency())
                                        .set("seconds", super::as_secs(g.latency()))
                                        .set("load", Json::big(load as u128))
                                        .set("vehicles", Json::big(vehicles as u128))
                                        .set("distance", Json::big(g.distance as u128))
                                }
                                _ => Json::Null,
                            };
                            Json::obj()
                                .set("id", Json::big(h.id as u128))
                                .set("proto", h.proto.tag)
                                .set("title", h.proto.title)
                                .set("name", h.name.clone())
                                .set("from", Json::big(h.from as u128))
                                .set("to", Json::big(h.to as u128))
                                .set("item", h.item.clone())
                                .set("running", build.running.contains(&h.id))
                                .set("idle", idle.get(&h.id).map(|s| s.to_string()))
                                .set("geometry", geo)
                        })
                        .collect(),
                ),
            )
            .set(
                "conns",
                Json::Arr({
                    // The derived buffer, named on the connection that made
                    // it, so a panel can ask the plant what is sitting in one
                    // without knowing that the compiler invented it.
                    let bridges = self.bridges();
                    self.conns
                        .iter()
                        .map(|c| {
                            let b = bridges.iter().find(|b| {
                                b.from == c.from && b.to == c.to && b.item == c.item
                            });
                            Json::obj()
                                .set("from", Json::big(c.from as u128))
                                .set("to", Json::big(c.to as u128))
                                .set("item", c.item.clone())
                                .set("title", lower::item_title(&c.item))
                                .set("domain", lower::domain_of(&c.item).tag())
                                .set("unit", lower::domain_of(&c.item).unit())
                                // A connection with a buffer is one the player
                                // did not have to put a bay in the middle of.
                                .set("buffer", b.map(|b| Json::Str(b.name.clone())))
                                .set("capacity", b.map(|b| Json::Int(b.capacity as i128)))
                        })
                        .collect()
                }),
            )
            .set(
                "deposits",
                Json::Arr(
                    self.deposits
                        .iter()
                        .map(|d| {
                            // Who is working it and how much is left, so the
                            // panel can say "nothing is" or "all of it is"
                            // rather than leaving the player to work it out.
                            let on = self.worked(d);
                            let taken: Qty =
                                on.iter().map(|i| i.rated.unwrap_or(0)).sum();
                            d.to_json()
                                .set(
                                    "worked",
                                    Json::Arr(
                                        on.iter()
                                            .map(|i| Json::Str(i.name.clone()))
                                            .collect(),
                                    ),
                                )
                                .set("taken", Json::big(taken as u128))
                                .set("spare", Json::big(d.yields.saturating_sub(taken) as u128))
                        })
                        .collect(),
                ),
            )
            .set("footprint", self.footprint())
            .set(
                "extent",
                Json::arr({
                    let (x0, y0, x1, y1) = self.extent();
                    vec![x0 as i64, y0 as i64, x1 as i64, y1 as i64]
                }),
            )
    }

    /// A world, rebuilt from the wire.
    ///
    /// Everything derived is derived again rather than trusted: a machine's
    /// footprint and its recipe come from re-running the design that arrived,
    /// not from the numbers that arrived beside it. A replica that believed a
    /// sender's arithmetic would be checking that the sender can serialise,
    /// which is not the property under test.
    pub fn from_json(j: &Json) -> Result<World, String> {
        let mut w = World {
            name: j.at("name").as_str().unwrap_or("Room").to_string(),
            next_id: j.at("nextId").as_u64().unwrap_or(1).max(1),
            plot: j.at("plot").as_i128().unwrap_or(PLOT as i128) as i32,
            ..World::default()
        };
        for e in j.at("deposits").as_arr() {
            let (Some(id), Some(item)) = (e.at("id").as_u64(), e.at("item").as_str()) else {
                continue;
            };
            let n = |k: &str| e.at(k).as_i128().unwrap_or(0) as i32;
            w.deposits.push(Deposit {
                id,
                item: item.to_string(),
                x: n("x"),
                y: n("y"),
                w: n("w"),
                h: n("h"),
                yields: e.at("yields").as_u64().unwrap_or(0),
            });
        }
        for e in j.at("installs").as_arr() {
            let tag = e.at("proto").as_str().ok_or("an installation has no prototype")?;
            let p = proto(tag).ok_or(format!("there is no `{tag}` in the catalogue"))?;
            let design = match e.at("design") {
                // A machine without its design is not a machine this replica
                // can rebuild, and quietly substituting the catalogue's would
                // be the worst kind of desynchronisation: one that looks
                // right. Frames leave the designs out on purpose; snapshots
                // never do, and this is what tells them apart.
                Json::Null if p.role.designed() => {
                    return Err(format!("`{tag}` arrived without its design"))
                }
                Json::Null => None,
                d => Some(Design::from_json(d)?),
            };
            // `designed()` rather than `Role::Machine`: an extraction head
            // owns a design too, and a replica that rebuilt one without
            // lowering it would have a head with no recipe -- which the plant
            // then drops, along with everything downstream of it. That is a
            // snapshot that looks like a world and is a different factory.
            let lowered = match (&design, p.role.designed()) {
                (Some(d), true) => Some(lower::lower(d)?),
                _ => None,
            };
            let id = e.at("id").as_u64().ok_or("an installation has no id")?;
            w.installs.push(Install {
                id,
                proto: p,
                name: e
                    .at("name")
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}{}", p.short, id)),
                x: e.at("x").as_i128().unwrap_or(0) as i32,
                y: e.at("y").as_i128().unwrap_or(0) as i32,
                face: e.at("face").as_u64().unwrap_or(0) as u8,
                item: e.at("item").as_str().map(str::to_string),
                rated: e.at("rated").as_u64(),
                design,
                lowered,
                draft: match e.at("draft") {
                    Json::Null => None,
                    d => Some(Design::from_json(d)?),
                },
                editor: e.at("editor").as_u64().map(|p| p as PlayerId),
                placed: e.at("placedAt").as_u64().unwrap_or(0),
                by: e.at("placedBy").as_u64().unwrap_or(0) as PlayerId,
            });
        }
        for e in j.at("hauls").as_arr() {
            let tag = e.at("proto").as_str().ok_or("a transport has no prototype")?;
            let p = proto(tag).ok_or(format!("there is no `{tag}` in the catalogue"))?;
            let id = e.at("id").as_u64().ok_or("a transport has no id")?;
            w.hauls.push(Haul {
                id,
                proto: p,
                name: e
                    .at("name")
                    .as_str()
                    .map(str::to_string)
                    .unwrap_or_else(|| format!("{}{}", p.short, id)),
                from: e.at("from").as_u64().unwrap_or(0),
                to: e.at("to").as_u64().unwrap_or(0),
                item: e.at("item").as_str().unwrap_or_default().to_string(),
                placed: e.at("placedAt").as_u64().unwrap_or(0),
                by: e.at("placedBy").as_u64().unwrap_or(0) as PlayerId,
            });
        }
        for e in j.at("conns").as_arr() {
            w.conns.push(Conn {
                from: e.at("from").as_u64().unwrap_or(0),
                to: e.at("to").as_u64().unwrap_or(0),
                item: e.at("item").as_str().unwrap_or_default().to_string(),
            });
        }
        Ok(w)
    }
}

/// The document, lowered -- and what was left out of it.
#[derive(Clone, Debug, Default)]
pub struct Build {
    pub graph: Graph,
    /// Ids that made it into the IR.
    pub running: Vec<Id>,
    /// Ids that did not, and the sentence their inspector shows.
    pub idle: Vec<(Id, String)>,
    /// False when there is nothing to simulate at all -- an empty plot, or a
    /// factory whose last machine has just been deleted. The clock goes on;
    /// there is simply nothing for it to do.
    pub runnable: bool,
}

impl Build {
    pub fn why_idle(&self, id: Id) -> Option<&str> {
        self.idle.iter().find(|(i, _)| *i == id).map(|(_, w)| w.as_str())
    }
}

/// A machine prototype's design, freshly parsed. Every placed machine gets its
/// own copy, and they diverge from that moment on.
pub fn stock_design(tag: &str) -> Result<Design, String> {
    let p = proto(tag).ok_or(format!("there is no `{tag}` in the catalogue"))?;
    let src = p.design_source().ok_or(format!("a {} has no design", p.title))?;
    Design::parse(src)
}
