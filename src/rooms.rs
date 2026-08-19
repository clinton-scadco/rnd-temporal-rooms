//! v3 -- the Room: a plant executed as several independently advancing
//! regions that talk only in timestamped events.
//!
//! v1 compressed repetition, v2 compressed interaction. Both still solved one
//! plant as one object with one clock. A Room does not have one clock.
//!
//! # What a region is
//!
//! Cut every transport out of the wiring graph and the plant falls into pieces
//! that share no storage. Two machines in different pieces cannot affect each
//! other at the same instant, because everything between them is a batch that
//! takes real time to travel. `domains::regions` finds those pieces, and this
//! module runs each one as a **separate simulation with its own clock**:
//!
//! ```text
//!   Mine            t = 120,000
//!   Smelting        t =  94,000
//!   Manufacturing   t = 180,000
//! ```
//!
//! Nothing here is an approximation of a global tick loop. The clocks really
//! are different, and the answer is bit-for-bit the answer the monolithic
//! solver gives.
//!
//! # What a channel is
//!
//! A transport class is *lifted out* of both regions and becomes a channel.
//! Its population splits between the two ends without gaining a single new
//! state, because the four buckets a class already had are exactly the four
//! places a vehicle can be:
//!
//! ```text
//!   starved    waiting to load       <- lives in the sending region
//!   working    in transit            <- a message between them
//!   done       waiting to unload     <- lives in the receiving region
//!   returning  on the trip home      <- a message back
//! ```
//!
//! So a region needs no inbox: a batch that will land at tick `t` is delivered
//! straight into the receiving region's `working` bucket, and an empty vehicle
//! that gets home at tick `t` into the sending region's `returning` bucket.
//! Every such message lands strictly in the receiver's future, which is
//! asserted on delivery -- that assertion is the whole claim that these
//! regions could be running on different machines.
//!
//! # Why the scheduler is allowed to let them drift
//!
//! Region `r` may settle any tick up to
//!
//! ```text
//!   min over inbound  channels of ( clock[sender]   + latency        )
//!   min over outbound channels of ( clock[receiver] + return latency )
//! ```
//!
//! The first line is material: nothing can arrive sooner than a batch that has
//! not been loaded yet. The second is vehicles: this region cannot load again
//! until one comes back, and the earliest that can happen is the far end's
//! present plus the trip home.
//!
//! Both lines are needed and v2 only knew about the first. A link whose
//! vehicle teleports home has zero return latency, so the *sending* region can
//! never advance a single tick past the receiving one -- and a cycle of such
//! links pins a whole set of regions into lockstep. `domains::regions` fuses
//! those back together before the scheduler ever sees them, which is why this
//! loop cannot deadlock: every remaining cycle of constraints has strictly
//! positive weight.

use crate::domains::{self, RegionGraph};
use crate::model::*;
use crate::pop::{self, ClassPop, Emit, Pop, PopForm, Port};
use crate::sim::Counters;

/// How a region is being solved. The plant picks the cheapest exact
/// representation available to it, not the one the player deserves.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Mode {
    /// The region hears from nobody, so its whole future is a periodic orbit
    /// and any tick is one evaluation away.
    Closed,
    /// Lumped population, stepped by the scheduler.
    Population,
}

impl Mode {
    pub fn label(self) -> &'static str {
        match self {
            Mode::Closed => "closed form",
            Mode::Population => "population",
        }
    }
}

/// The static half of a decomposition: one self-contained `Blueprint` per
/// region, plus the index maps back to the plant it came from.
///
/// Building real blueprints rather than masking a shared one is deliberate.
/// A region that owns its own storage indices, its own class indices and its
/// own arbitration queues is a thing that could be handed to another process;
/// a mask is a promise that it could be.
pub struct Plan {
    pub graph: RegionGraph,
    pub bps: Vec<Blueprint>,
    pub ports: Vec<Vec<Port>>,
    /// region -> local class -> global class.
    pub class_up: Vec<Vec<u16>>,
    /// region -> global class -> local class, `u16::MAX` when absent.
    pub class_down: Vec<Vec<u16>>,
    /// region -> local storage -> global storage.
    pub store_up: Vec<Vec<u16>>,
    pub store_down: Vec<Vec<u16>>,
}

impl Plan {
    pub fn regions(&self) -> usize {
        self.bps.len()
    }

    /// Where a channel's two ends live, as (region, local class).
    fn ends(&self, ch: usize) -> ((usize, u16), (usize, u16)) {
        let c = self.graph.channels[ch];
        let s = c.src_region;
        let d = c.dst_region;
        ((s, self.class_down[s][c.class as usize]), (d, self.class_down[d][c.class as usize]))
    }
}

/// Split a plant into region blueprints.
pub fn plan(bp: &Blueprint) -> Plan {
    let graph = domains::regions(bp);
    let nr = graph.regions.len();
    let nc = bp.actors.len();
    let ns = bp.storages.len();

    let mut bps = Vec::with_capacity(nr);
    let mut ports = Vec::with_capacity(nr);
    let mut class_up = Vec::with_capacity(nr);
    let mut class_down = Vec::with_capacity(nr);
    let mut store_up = Vec::with_capacity(nr);
    let mut store_down = Vec::with_capacity(nr);

    for r in 0..nr {
        let reg = &graph.regions[r];

        let s_up: Vec<u16> = reg.storages.clone();
        let mut s_down = vec![u16::MAX; ns];
        for (l, &g) in s_up.iter().enumerate() {
            s_down[g as usize] = l as u16;
        }

        // Every class that has any business here: the ones that live here, and
        // the ends of channels that touch here. Global order is preserved, so
        // arbitration order survives the split untouched.
        let mut c_up: Vec<u16> = Vec::new();
        let mut roles: Vec<Port> = Vec::new();
        for c in 0..nc {
            let g = c as u16;
            if graph.of_class[c] == r {
                c_up.push(g);
                roles.push(Port::Whole);
                continue;
            }
            if let Some(ch) = graph.channels.iter().find(|ch| ch.class == g) {
                if ch.src_region == r {
                    c_up.push(g);
                    roles.push(Port::Out);
                } else if ch.dst_region == r {
                    c_up.push(g);
                    roles.push(Port::In);
                }
            }
        }
        let mut c_down = vec![u16::MAX; nc];
        for (l, &g) in c_up.iter().enumerate() {
            c_down[g as usize] = l as u16;
        }

        let mut storages: Vec<StorageDef> = s_up
            .iter()
            .map(|&g| {
                let sd = &bp.storages[g as usize];
                let map = |v: &Vec<u16>| -> Vec<u16> {
                    v.iter()
                        .map(|&c| c_down[c as usize])
                        .filter(|&c| c != u16::MAX)
                        .collect()
                };
                StorageDef {
                    name: sd.name.clone(),
                    shared: sd.shared,
                    capacity: sd.capacity,
                    slots: sd.slots.clone(),
                    initial: sd.initial.clone(),
                    qty_offset: 0,
                    clients: map(&sd.clients),
                    policy: sd.policy,
                    order: map(&sd.order),
                    takers: map(&sd.takers),
                    givers: map(&sd.givers),
                }
            })
            .collect();
        let mut qty_stride = 0u32;
        for sd in &mut storages {
            sd.qty_offset = qty_stride;
            qty_stride += sd.slots.len() as u32;
        }

        let mut machines = 0u64;
        let mut base_period = 1u64;
        let actors: Vec<ActorDef> = c_up
            .iter()
            .zip(roles.iter())
            .map(|(&g, &role)| {
                let ad = &bp.actors[g as usize];
                let local = |v: &Vec<u16>| -> Vec<u16> {
                    v.iter()
                        .map(|&s| s_down[s as usize])
                        .filter(|&s| s != u16::MAX)
                        .collect()
                };
                // A lifted transport stands at one end only. The other end of
                // its wiring belongs to a region this one cannot see.
                let (in_stores, out_stores) = match role {
                    Port::Whole => (local(&ad.in_stores), local(&ad.out_stores)),
                    Port::Out => (local(&ad.in_stores), Vec::new()),
                    Port::In => (Vec::new(), local(&ad.out_stores)),
                };
                let out = ActorDef {
                    name: ad.name.clone(),
                    kind: ad.kind,
                    inputs: ad.inputs.clone(),
                    outputs: ad.outputs.clone(),
                    duration: ad.duration,
                    return_latency: ad.return_latency,
                    geometry: ad.geometry,
                    shared: ad.shared,
                    count: ad.count,
                    machine_offset: machines,
                    in_stores,
                    out_stores,
                };
                machines += ad.count;
                base_period = lcm(base_period, ad.cycle());
                out
            })
            .collect();

        bps.push(Blueprint {
            name: format!("{}#region{}", bp.name, r),
            storages,
            actors,
            qty_stride,
            machines,
            base_period,
        });
        ports.push(roles);
        class_up.push(c_up);
        class_down.push(c_down);
        store_up.push(s_up);
        store_down.push(s_down);
    }

    Plan { graph, bps, ports, class_up, class_down, store_up, store_down }
}

/// A plant running as several regions at several different times.
pub struct Room<'a> {
    pub plan: &'a Plan,
    pub n_items: usize,
    pub pops: Vec<Pop<'a>>,
    pub modes: Vec<Mode>,
    /// Closed forms for regions that hear from nobody.
    pub forms: Vec<Option<PopForm>>,
    /// Region advances performed.
    pub steps: u64,
    /// Messages handed from one region to another.
    pub messages: u64,
    /// Advances that were cut short by a neighbour rather than by running out
    /// of local work: the times a region actually had to wait for news.
    pub rendezvous: u64,
    pub max_advance: Tick,
    pub total_advance: u128,
    /// Widest gap between two region clocks seen during the run.
    pub max_skew: Tick,
    /// Every region's clock at the moment that gap was widest.
    pub skew_clocks: Vec<Tick>,
}

impl<'a> Room<'a> {
    pub fn new(plan: &'a Plan, n_items: usize) -> Room<'a> {
        let mut pops: Vec<Pop<'a>> = (0..plan.regions())
            .map(|r| Pop::new_ported(&plan.bps[r], n_items, plan.ports[r].clone()))
            .collect();

        let modes: Vec<Mode> = (0..plan.regions())
            .map(|r| {
                let reg = &plan.graph.regions[r];
                if reg.inbound.is_empty() && reg.outbound.is_empty() {
                    Mode::Closed
                } else {
                    Mode::Population
                }
            })
            .collect();
        let forms: Vec<Option<PopForm>> = (0..plan.regions())
            .map(|r| {
                if modes[r] == Mode::Closed {
                    let f = pop::orbit(&plan.bps[r], n_items, 20_000_000);
                    if f.found {
                        Some(f)
                    } else {
                        None
                    }
                } else {
                    None
                }
            })
            .collect();
        let modes: Vec<Mode> = modes
            .into_iter()
            .enumerate()
            .map(|(r, m)| if m == Mode::Closed && forms[r].is_none() { Mode::Population } else { m })
            .collect();

        let mut room = Room {
            plan,
            n_items,
            pops: Vec::new(),
            modes,
            forms,
            steps: 0,
            messages: 0,
            rendezvous: 0,
            max_advance: 0,
            total_advance: 0,
            max_skew: 0,
            skew_clocks: Vec::new(),
        };
        // t=0 settles inside every region independently, and a link that can
        // load immediately does so. Those departures have to be handed over
        // before anything advances.
        std::mem::swap(&mut room.pops, &mut pops);
        for r in 0..room.plan.regions() {
            room.drain(r);
        }
        room
    }

    pub fn clock(&self, r: usize) -> Tick {
        self.pops[r].now
    }

    pub fn min_clock(&self) -> Tick {
        self.pops.iter().map(|p| p.now).min().unwrap_or(0)
    }

    /// The last tick region `r` may settle without hearing from anyone.
    ///
    /// Everything this region cannot see is either a batch that has not been
    /// loaded yet or a vehicle that has not started home yet, and both take
    /// declared time. `Tick::MAX` means nothing constrains it at all.
    pub fn safe_time(&self, r: usize) -> Tick {
        let g = &self.plan.graph;
        let reg = &g.regions[r];
        let mut safe = Tick::MAX;
        for &c in &reg.inbound {
            let ch = g.channels[c];
            safe = safe.min(self.pops[ch.src_region].now.saturating_add(ch.latency));
        }
        for &c in &reg.outbound {
            let ch = g.channels[c];
            safe = safe.min(self.pops[ch.dst_region].now.saturating_add(ch.return_latency));
        }
        safe
    }

    /// Hand everything region `r` has produced to the regions it is for.
    fn drain(&mut self, r: usize) {
        if self.pops[r].outbox.is_empty() {
            return;
        }
        let msgs: Vec<Emit> = std::mem::take(&mut self.pops[r].outbox);
        for m in msgs {
            let global = self.plan.class_up[r][m.class as usize];
            let ch = self
                .plan
                .graph
                .channels
                .iter()
                .position(|c| c.class == global)
                .expect("a port class with no channel");
            let ((src, src_local), (dst, dst_local)) = self.plan.ends(ch);
            // Loading end sends cargo forward; unloading end sends the empty
            // vehicle back. Nothing else crosses.
            let (peer, local) =
                if r == src { (dst, dst_local) } else { (src, src_local) };
            self.pops[peer].deliver(local, m.at, m.count);
            self.messages += 1;
        }
    }

    /// Advance one region as far as it is allowed to go. Returns false when no
    /// region can move, which means every clock has reached `horizon`.
    fn step(&mut self, horizon: Tick) -> bool {
        let n = self.plan.regions();
        let mut best: Option<(Tick, usize, Tick)> = None;
        for r in 0..n {
            let now = self.pops[r].now;
            if now >= horizon {
                continue;
            }
            let safe = self.safe_time(r);
            let target = safe.min(horizon);
            if target <= now {
                continue;
            }
            // Prefer the region that is furthest behind: it is the one holding
            // everybody else up, and advancing it raises the most horizons.
            let key = now;
            if best.map_or(true, |(k, _, _)| key < k) {
                best = Some((key, r, target));
            }
        }
        let Some((_, r, target)) = best else {
            return false;
        };

        let before = self.pops[r].now;
        // A region with nothing left to do still moves its clock: its silence
        // up to `target` is information its neighbours are entitled to.
        self.pops[r].run_until(target);
        let advanced = self.pops[r].now - before;
        self.steps += 1;
        self.total_advance += advanced as u128;
        self.max_advance = self.max_advance.max(advanced);
        if target < horizon {
            self.rendezvous += 1;
        }
        self.drain(r);

        let (lo, hi) = self.clock_range();
        if hi - lo > self.max_skew {
            self.max_skew = hi - lo;
            self.skew_clocks = self.pops.iter().map(|p| p.now).collect();
        }
        true
    }

    fn clock_range(&self) -> (Tick, Tick) {
        let mut lo = Tick::MAX;
        let mut hi = 0;
        for p in &self.pops {
            lo = lo.min(p.now);
            hi = hi.max(p.now);
        }
        (lo, hi)
    }

    /// Bring every region to tick `t`. Only at such a moment is the decomposed
    /// state comparable with a monolithic one -- in between, there is no such
    /// thing as "the state of the plant".
    pub fn run_until(&mut self, t: Tick) {
        while self.step(t) {}
    }

    // -------------------------------------------------------- observation

    /// The plant's counters, reassembled. A transport's consumption is counted
    /// where it loaded and its production where it unloaded, so the two halves
    /// land in different regions and simply add up.
    pub fn counters(&self) -> Counters {
        let nc = self.plan.graph.of_class.len();
        let mut c = Counters::zeroed(nc, self.n_items);
        for (r, p) in self.pops.iter().enumerate() {
            for (l, &g) in self.plan.class_up[r].iter().enumerate() {
                c.cycles[g as usize] += p.c.cycles[l];
            }
            for i in 0..self.n_items {
                c.produced[i] += p.c.produced[i];
                c.consumed[i] += p.c.consumed[i];
            }
        }
        c
    }

    /// The population of every class, with the two halves of each lifted
    /// transport put back together.
    pub fn classes(&self) -> Vec<ClassPop> {
        let nc = self.plan.graph.of_class.len();
        let mut out: Vec<ClassPop> = (0..nc)
            .map(|_| ClassPop {
                working: Vec::new(),
                starved: 0,
                done: 0,
                returning: Vec::new(),
            })
            .collect();
        for (r, p) in self.pops.iter().enumerate() {
            for (l, &g) in self.plan.class_up[r].iter().enumerate() {
                let src = &p.classes[l];
                let dst = &mut out[g as usize];
                dst.starved += src.starved;
                dst.done += src.done;
                merge_into(&mut dst.working, &src.working);
                merge_into(&mut dst.returning, &src.returning);
            }
        }
        out
    }

    pub fn storage_qty(&self, storage: usize, item: ItemId) -> Qty {
        let r = self.plan.graph.of_storage[storage];
        let local = self.plan.store_down[r][storage] as usize;
        self.pops[r].storage_qty(local, item)
    }

    pub fn storage_used(&self, storage: usize) -> Qty {
        let r = self.plan.graph.of_storage[storage];
        let local = self.plan.store_down[r][storage] as usize;
        self.pops[r].storage_used(local)
    }

    /// The decomposed state written out in exactly the byte layout
    /// `Pop::signature` uses, so the two can be compared directly rather than
    /// counter by counter.
    ///
    /// Only meaningful once every clock has been brought to the same tick.
    pub fn signature(&self, bp: &Blueprint) -> Vec<u8> {
        let now = self.pops[0].now;
        debug_assert!(self.pops.iter().all(|p| p.now == now), "regions are not aligned");
        let mut v = Vec::new();
        for (s, sd) in bp.storages.iter().enumerate() {
            for &it in &sd.slots {
                v.extend_from_slice(&self.storage_qty(s, it).to_le_bytes());
            }
        }
        for s in 0..bp.storages.len() {
            let r = self.plan.graph.of_storage[s];
            let local = self.plan.store_down[r][s] as usize;
            for q in 0..2 {
                v.extend_from_slice(&self.pops[r].rr_at(local, q).to_le_bytes());
            }
        }
        for c in self.classes() {
            v.extend_from_slice(&c.starved.to_le_bytes());
            v.extend_from_slice(&c.done.to_le_bytes());
            for (dl, n) in &c.working {
                v.extend_from_slice(&(dl - now).to_le_bytes());
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.push(0xfe);
            for (dl, n) in &c.returning {
                v.extend_from_slice(&(dl - now).to_le_bytes());
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.push(0xff);
        }
        v
    }

    /// Mean ticks advanced per scheduler step: how much work a region gets to
    /// do between two pieces of news.
    pub fn mean_advance(&self) -> f64 {
        if self.steps == 0 {
            0.0
        } else {
            self.total_advance as f64 / self.steps as f64
        }
    }
}

/// Merge one sorted `(tick, count)` run into another.
fn merge_into(dst: &mut Vec<(Tick, u64)>, src: &[(Tick, u64)]) {
    for &(t, n) in src {
        match dst.binary_search_by_key(&t, |e| e.0) {
            Ok(i) => dst[i].1 += n,
            Err(i) => dst.insert(i, (t, n)),
        }
    }
}
