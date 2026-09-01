//! Transport between rooms, made real.
//!
//! ```text
//!   Coal Basin
//!     depot ships 240 coal/s
//!        |
//!        |  a train fills, leaves, and is gone for 57 seconds
//!        v
//!   Power Station
//!     30,000 coal lands in the yard, all at once
//! ```
//!
//! Section 2 of the brief asks for this and then says the thing that makes it
//! worth building: *a room can continue operating while nobody is there.* That
//! is not a feature bolted onto the simulation -- it is what the simulation has
//! been for since Prototype 1, and this module is the first thing that spends
//! it. You leave Iron Valley, work in Manufacturing for twenty minutes, come
//! back, and the crushing line has been running the whole time, fed by trains
//! that arrived while nobody was looking.
//!
//! # An arrival is a command
//!
//! The obvious implementation is to reach into the destination's simulation
//! and add thirty thousand coal to a bay. That would work exactly once, on the
//! host, and then every replica would be a different factory.
//!
//! So an arrival is an [`Act::Deliver`](crate::mp::cmd::Act::Deliver): the
//! authority stamps it with a tick and a sequence number like any other
//! intention, it goes down the same command log, and every reconstruction --
//! the host's and one per player -- applies it at the same tick and gets the
//! same bay. The canonical hash covers it, because the carry it lands in is
//! part of the canonical hash. Nothing about the multiplayer proof had to be
//! weakened to move a train between rooms.
//!
//! # Why the settlement is on a lattice
//!
//! What a route dispatches is a *difference*: how much the origin's depot has
//! shipped since the last time anybody looked. If "the last time anybody
//! looked" meant "whenever a browser polled", two clients polling at different
//! rates would batch the same coal into different trains, and the two rooms
//! would diverge from opposite ends.
//!
//! So the ledger only ever settles at multiples of [`SETTLE`], regardless of
//! when it is asked, and a departure is therefore a function of the clock
//! rather than of the network. The same discipline as the accounting lattice
//! in [`crate::mp::goal`], for the same reason, one altitude up.
//!
//! # Conservation
//!
//! What leaves a room is what the origin's depot actually swallowed, and what
//! arrives is what left, minus whatever the destination yard was too small to
//! hold. Nothing is created. A route that is asked for more than its fleet can
//! carry simply does not carry it -- the surplus stays at the origin and
//! counts, as it always did, toward the origin's own objective.

use super::site::site;
use crate::json::Json;
use crate::model::{Qty, Tick};
use crate::mp::goal::commas;
use crate::mp::{as_secs, lower::item_title, secs, SIM_TICK_RATE};

/// How often the shipping office does its arithmetic: every five seconds of
/// simulated time, and never at any other moment.
pub const SETTLE: Tick = secs(5);

/// One edge of the fixed graph. Both directions are not the same lane; a
/// campaign map is a set of supply relationships, and a supply relationship
/// points one way.
pub struct Lane {
    pub from: &'static str,
    pub to: &'static str,
    pub item: &'static str,
    /// One-way running time in seconds, for a fleet at speed 100. Authored on
    /// the map rather than typed by a player: section 11 of Prototype 2's
    /// brief refused to let anybody enter a latency, and a campaign map is
    /// exactly the place that rule would have been quietly dropped.
    pub leagues: u64,
    pub why: &'static str,
}

pub static LANES: &[Lane] = &[
    Lane {
        from: "basin",
        to: "valley",
        item: "Coal",
        leagues: 40,
        why: "The short haul, and the one the crushing line lives on.",
    },
    Lane {
        from: "basin",
        to: "station",
        item: "Coal",
        leagues: 55,
        why: "Three hundred megawatts of fuel, arriving in lumps.",
    },
    Lane {
        from: "basin",
        to: "works",
        item: "Coal",
        leagues: 90,
        why: "A stamping line only wants twelve a second. It wants them without fail.",
    },
    Lane {
        from: "basin",
        to: "final",
        item: "Coal",
        leagues: 100,
        why: "The longest run on the map, for the room with the least patience.",
    },
    Lane {
        from: "station",
        to: "works",
        item: "Power",
        leagues: 35,
        why: "Electricity, on a wire long enough to have a delivery time.",
    },
    Lane {
        from: "works",
        to: "final",
        item: "Gear",
        leagues: 45,
        why: "The order Final Works is judged on, made somewhere else.",
    },
    Lane {
        from: "valley",
        to: "final",
        item: "Concentrate",
        leagues: 70,
        why: "Nothing in Iron Valley makes this yet. That is the point of it.",
    },
];

pub fn lane(from: &str, to: &str, item: &str) -> Option<(usize, &'static Lane)> {
    LANES
        .iter()
        .enumerate()
        .find(|(_, l)| l.from == from && l.to == to && l.item == item)
}

/// What the load is carried in.
#[derive(Debug)]
pub struct Fleet {
    pub tag: &'static str,
    pub title: &'static str,
    /// One vehicle's load.
    pub load: Qty,
    /// How many are on the route at once. When they are all out, the yard
    /// waits -- which is the whole reason a fleet is a choice.
    pub vehicles: u64,
    /// Seconds spent loading and unloading, whatever the distance.
    pub dwell: u64,
    /// Running speed as a percentage of the lane's authored time.
    pub speed: u64,
    pub blurb: &'static str,
}

pub static FLEETS: &[Fleet] = &[
    Fleet {
        tag: "convoy",
        title: "Road Convoy",
        load: 6_000,
        vehicles: 3,
        dwell: 5,
        speed: 100,
        blurb: "Small loads, often. Nothing waits long and nothing arrives in quantity.",
    },
    Fleet {
        tag: "train",
        title: "Train",
        load: 30_000,
        vehicles: 2,
        dwell: 20,
        speed: 150,
        blurb: "Thirty thousand at a time, and twice as fast once it is moving.",
    },
    Fleet {
        tag: "ship",
        title: "Bulk Ship",
        load: 250_000,
        vehicles: 1,
        dwell: 90,
        speed: 70,
        blurb: "A quarter of a million, four minutes apart. Bring a yard.",
    },
];

pub fn fleet(tag: &str) -> Option<&'static Fleet> {
    FLEETS.iter().find(|f| f.tag == tag)
}

impl Fleet {
    /// One round of loading, running and unloading, on this lane -- rounded up
    /// to the settlement lattice so that an arrival always lands on a second
    /// the whole campaign agrees about.
    pub fn trip(&self, l: &Lane) -> Tick {
        let s = self.dwell + l.leagues * 100 / self.speed.max(1);
        let t = secs(s.max(1));
        t.div_ceil(SETTLE) * SETTLE
    }

    /// The most this fleet can move in a second on this lane, if the origin
    /// can keep up with it.
    pub fn capacity(&self, l: &Lane) -> u64 {
        let t = as_secs(self.trip(l)).max(1.0);
        (self.load as f64 * self.vehicles as f64 / t) as u64
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("tag", self.tag)
            .set("title", self.title)
            .set("load", Json::big(self.load as u128))
            .set("vehicles", self.vehicles as i64)
            .set("dwell", self.dwell as i64)
            .set("speed", self.speed as i64)
            .set("blurb", self.blurb)
    }
}

// ==================================================================== routes

/// A standing supply relationship: this room ships that item to that room, in
/// these vehicles, at up to this rate.
#[derive(Clone, Debug)]
pub struct Route {
    pub id: u32,
    pub lane: usize,
    pub fleet: &'static Fleet,
    /// The most the player wants sent, per second.
    ///
    /// A knob rather than a fixed split, because a room that ships coal to
    /// three other rooms has to be able to say which one is starving. Routes
    /// out of the same room and item are filled in the order they were opened,
    /// each up to its cap, until the day's shipping runs out.
    pub cap: u64,
    pub opened: Tick,
    /// The counter at the origin the last settlement read. What is shipped is
    /// a difference, so this is the other half of the difference.
    pub seen: u64,
    /// Waiting to go.
    pub hold: Qty,
    pub since: Tick,
    // ---- what has actually happened, for a panel that would rather show
    // than promise.
    pub moved: u64,
    pub trips: u64,
    pub spilled: u64,
    pub last_left: Option<Tick>,
    pub last_in: Option<Tick>,
}

impl Route {
    pub fn lane(&self) -> &'static Lane {
        &LANES[self.lane]
    }

    pub fn trip(&self) -> Tick {
        self.fleet.trip(self.lane())
    }

    pub fn to_json(&self, flight: &[Load], now: Tick) -> Json {
        let l = self.lane();
        let out: Vec<&Load> = flight.iter().filter(|f| f.route == self.id).collect();
        Json::obj()
            .set("id", self.id as i64)
            .set("from", l.from)
            .set("to", l.to)
            .set("item", l.item)
            .set("itemTitle", item_title(l.item))
            .set("why", l.why)
            .set("fleet", self.fleet.tag)
            .set("fleetTitle", self.fleet.title)
            .set("load", Json::big(self.fleet.load as u128))
            .set("vehicles", self.fleet.vehicles as i64)
            .set("cap", Json::big(self.cap as u128))
            .set("trip", self.trip())
            .set("tripSeconds", as_secs(self.trip()))
            .set("hold", Json::big(self.hold as u128))
            .set("moved", Json::big(self.moved as u128))
            .set("trips", Json::big(self.trips as u128))
            .set("spilled", Json::big(self.spilled as u128))
            .set("inFlight", out.len() as i64)
            .set(
                "due",
                Json::Arr(
                    out.iter()
                        .map(|f| {
                            Json::obj()
                                .set("qty", Json::big(f.qty as u128))
                                .set("at", f.at)
                                .set("in", as_secs(f.at.saturating_sub(now)))
                        })
                        .collect(),
                ),
            )
            .set("lastLeft", self.last_left.map(|t| Json::Int(t as i128)))
            .set("lastIn", self.last_in.map(|t| Json::Int(t as i128)))
            .set("rate", self.moved as f64 / as_secs(now.saturating_sub(self.opened)).max(1.0))
    }
}

/// A vehicle in the air, and when it lands.
///
/// It carries where it is going rather than only which route sent it: a
/// contract cancelled while a train is between rooms does not make the train
/// vanish, and the load has to know where to put itself.
#[derive(Clone, Debug)]
pub struct Load {
    pub route: u32,
    pub from: &'static str,
    pub to: &'static str,
    pub item: &'static str,
    pub at: Tick,
    pub qty: Qty,
}

/// One departure or arrival, for the campaign's news feed.
#[derive(Clone, Debug)]
pub struct Move {
    pub at: Tick,
    pub route: u32,
    pub to: &'static str,
    pub from: &'static str,
    pub item: &'static str,
    pub qty: Qty,
    pub arriving: bool,
}

/// Every route, everything in the air, and the lattice point the arithmetic
/// has reached.
#[derive(Clone, Debug, Default)]
pub struct Ledger {
    pub routes: Vec<Route>,
    pub flight: Vec<Load>,
    pub at: Tick,
    pub next_id: u32,
}

impl Ledger {
    /// Open a standing supply relationship.
    pub fn open(
        &mut self,
        from: &str,
        to: &str,
        item: &str,
        fleet_tag: &str,
        cap: Option<u64>,
        now: Tick,
    ) -> Result<u32, String> {
        let (idx, l) = lane(from, to, item)
            .ok_or_else(|| format!("there is no {} line from {from} to {to}", item_title(item)))?;
        let f = fleet(fleet_tag).ok_or_else(|| format!("there is no `{fleet_tag}` fleet"))?;
        if self.routes.iter().any(|r| r.lane == idx && r.fleet.tag == f.tag) {
            return Err(format!(
                "a {} already runs {}",
                f.title.to_lowercase(),
                lane_words(l)
            ));
        }
        let id = self.next_id + 1;
        self.next_id = id;
        self.routes.push(Route {
            id,
            lane: idx,
            fleet: f,
            cap: cap.unwrap_or_else(|| f.capacity(l)).max(1),
            opened: now,
            // A route opened at minute nine does not get minute one's coal:
            // the counter it differences against starts where the origin is,
            // not at zero.
            seen: u64::MAX,
            hold: 0,
            since: now,
            moved: 0,
            trips: 0,
            spilled: 0,
            last_left: None,
            last_in: None,
        });
        Ok(id)
    }

    pub fn close(&mut self, id: u32) -> Result<&'static Lane, String> {
        let k = self
            .routes
            .iter()
            .position(|r| r.id == id)
            .ok_or("there is no such route")?;
        // What is already in the air still lands. A load does not evaporate
        // because somebody cancelled the contract behind it.
        Ok(self.routes.remove(k).lane())
    }

    pub fn retune(&mut self, id: u32, cap: u64) -> Result<(), String> {
        let r = self
            .routes
            .iter_mut()
            .find(|r| r.id == id)
            .ok_or("there is no such route")?;
        r.cap = cap.max(1);
        Ok(())
    }

    pub fn route(&self, id: u32) -> Option<&Route> {
        self.routes.iter().find(|r| r.id == id)
    }

    /// Everything about to land at or before `t`, taken out of the air.
    pub fn arrivals(&mut self, t: Tick) -> Vec<Load> {
        let mut out = Vec::new();
        let mut kept = Vec::with_capacity(self.flight.len());
        for f in std::mem::take(&mut self.flight) {
            if f.at <= t {
                out.push(f);
            } else {
                kept.push(f);
            }
        }
        self.flight = kept;
        // Sorted, so that two rooms unloading at the same second do it in the
        // same order on every replica.
        out.sort_by_key(|l| (l.at, l.route, l.qty));
        out
    }

    /// Credit a route with what actually made it into the yard.
    pub fn landed(&mut self, id: u32, took: Qty, spilled: Qty, at: Tick) {
        if let Some(r) = self.routes.iter_mut().find(|r| r.id == id) {
            r.moved += took;
            r.spilled += spilled;
            r.last_in = Some(at);
        }
    }

    /// Decide what leaves, at one lattice point.
    ///
    /// `shipped(site, item)` is the origin's cumulative delivery counter,
    /// which is exactly the number the room's own objective is scored on --
    /// shipping the thing you were asked for *is* how it reaches the next
    /// room, and there is deliberately no second counter for exports.
    pub fn dispatch(
        &mut self,
        t: Tick,
        shipped: impl Fn(&str, &str) -> u64,
    ) -> Vec<Move> {
        let mut news = Vec::new();
        // Group by (origin, item): several routes may be waiting for the same
        // yard, and they are filled in the order they were opened.
        let mut order: Vec<usize> = (0..self.routes.len()).collect();
        order.sort_by_key(|&i| self.routes[i].id);
        let mut pool: std::collections::BTreeMap<(&'static str, &'static str), u64> =
            std::collections::BTreeMap::new();
        for &i in &order {
            let l = self.routes[i].lane();
            let now = shipped(l.from, l.item);
            let seen = &mut self.routes[i].seen;
            if *seen == u64::MAX {
                // First settlement after opening: start the difference here.
                *seen = now;
                continue;
            }
            let delta = now.saturating_sub(*seen);
            *seen = now;
            // Every route reading the same counter contributes its own view of
            // the difference exactly once; the pool is what is actually there.
            let key = (l.from, l.item);
            if !pool.contains_key(&key) {
                pool.insert(key, delta);
            }
        }
        for &i in &order {
            let (lane_from, lane_item, trip) = {
                let r = &self.routes[i];
                let l = r.lane();
                (l.from, l.item, r.trip())
            };
            let room = {
                let r = &self.routes[i];
                let want = r.cap.saturating_mul(SETTLE / SIM_TICK_RATE);
                let space = r.fleet.load.saturating_sub(r.hold);
                want.min(space)
            };
            let avail = pool.entry((lane_from, lane_item)).or_insert(0);
            let take = room.min(*avail);
            *avail -= take;
            let r = &mut self.routes[i];
            r.hold += take;
            // A vehicle leaves when it is full, or when it has waited a whole
            // trip's worth and has anything at all on it. A trickle still
            // moves; it simply moves in small, infrequent loads, which is
            // what a trickle deserves.
            let out = self.flight.iter().filter(|f| f.route == r.id).count() as u64;
            let full = r.hold >= r.fleet.load;
            let waited = t.saturating_sub(r.since) >= trip;
            if out < r.fleet.vehicles && r.hold > 0 && (full || waited) {
                let qty = r.hold.min(r.fleet.load);
                r.hold -= qty;
                r.since = t;
                r.trips += 1;
                r.last_left = Some(t);
                let id = r.id;
                let l = r.lane();
                self.flight.push(Load {
                    route: id,
                    from: l.from,
                    to: l.to,
                    item: l.item,
                    at: t + trip,
                    qty,
                });
                news.push(Move {
                    at: t,
                    route: id,
                    from: l.from,
                    to: l.to,
                    item: l.item,
                    qty,
                    arriving: false,
                });
            }
        }
        news
    }

    pub fn to_json(&self, now: Tick) -> Json {
        Json::obj()
            .set("at", self.at)
            .set(
                "routes",
                Json::Arr(self.routes.iter().map(|r| r.to_json(&self.flight, now)).collect()),
            )
            .set("inFlight", self.flight.len() as i64)
            .set(
                "lanes",
                Json::Arr(
                    LANES
                        .iter()
                        .map(|l| {
                            Json::obj()
                                .set("from", l.from)
                                .set("to", l.to)
                                .set("item", l.item)
                                .set("itemTitle", item_title(l.item))
                                .set("leagues", l.leagues as i64)
                                .set("why", l.why)
                                .set(
                                    "open",
                                    self.routes.iter().any(|r| {
                                        let rl = r.lane();
                                        rl.from == l.from && rl.to == l.to && rl.item == l.item
                                    }),
                                )
                        })
                        .collect(),
                ),
            )
            .set("fleets", Json::Arr(FLEETS.iter().map(Fleet::to_json).collect()))
    }
}

fn lane_words(l: &Lane) -> String {
    format!("{} from {} to {}", item_title(l.item), room_name(l.from), room_name(l.to))
}

/// A room's title, or its tag if the map has never heard of it.
fn room_name(tag: &str) -> &str {
    match site(tag) {
        Some((_, s)) => s.title,
        None => "somewhere else",
    }
}

/// A sentence about one departure or arrival, for the news feed.
pub fn moved_words(m: &Move) -> String {
    if m.arriving {
        format!("{} {} reached {}", commas(m.qty), item_title(m.item), room_name(m.to))
    } else {
        format!(
            "{} {} left {} for {}",
            commas(m.qty),
            item_title(m.item),
            room_name(m.from),
            room_name(m.to)
        )
    }
}

/// The lattice point at or before `t`.
pub fn settled(t: Tick) -> Tick {
    t / SETTLE * SETTLE
}
