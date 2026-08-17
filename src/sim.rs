//! Discrete-event simulator (tier T1).
//!
//! No tick loop exists anywhere in this file. Time advances by popping the next
//! scheduled event, so cost is O(events), not O(ticks x objects).
//!
//! State lives in struct-of-arrays columns indexed by `instance * stride + local`,
//! so one `World` is simultaneously the single-instance simulator used by the
//! analytic solver and the flat arena used for million-object runs.
//!
//! Every machine -- source, process, sink -- runs the identical cycle:
//!
//! ```text
//!   Idle --[withdraw all inputs atomically]--> Working(duration)
//!                     |                              |
//!                  (fails)                        (elapses)
//!                     v                              v
//!                  Starved <----wake----     [deposit all outputs]
//!                                                    |
//!                                                 (fails)
//!                                                    v
//!                                                 Blocked <----wake----
//! ```
//!
//! Blocked machines are re-woken by storage mutations. Waiter lists are static
//! (`StorageDef::clients`), so a blocked machine costs zero heap allocation --
//! the property that makes billions of objects viable.

use crate::model::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const S_WORKING: u8 = 1;
pub const S_STARVED: u8 = 2;
pub const S_BLOCKED: u8 = 3;
pub const S_DORMANT: u8 = 4;
const RETRY: u8 = 0x80;

const PRIO_FINISH: u64 = 0;
const PRIO_RETRY: u64 = 1;

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Ev {
    pub at: Tick,
    /// `prio << 56 | global_actor_index` -- keeps events 16 bytes and total
    /// ordering fully deterministic.
    key: u64,
}

impl Ev {
    #[inline]
    fn new(at: Tick, prio: u64, who: u64) -> Ev {
        Ev { at, key: (prio << 56) | who }
    }
    #[inline]
    fn who(self) -> u64 {
        self.key & 0x00ff_ffff_ffff_ffff
    }
}

/// Extensive quantities that grow linearly across a periodic orbit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counters {
    /// Completed cycles per blueprint-local actor, summed over instances.
    pub cycles: Vec<u64>,
    pub produced: Vec<u64>,
    pub consumed: Vec<u64>,
}

impl Counters {
    pub fn zeroed(n_actors: usize, n_items: usize) -> Counters {
        Counters {
            cycles: vec![0; n_actors],
            produced: vec![0; n_items],
            consumed: vec![0; n_items],
        }
    }

    pub fn sub(&self, other: &Counters) -> Counters {
        Counters {
            cycles: zip_sub(&self.cycles, &other.cycles),
            produced: zip_sub(&self.produced, &other.produced),
            consumed: zip_sub(&self.consumed, &other.consumed),
        }
    }

    /// `self + n * delta` in 128-bit space: horizons like t = 10^18 overflow u64.
    pub fn add_scaled_big(&self, delta: &Counters, n: u128) -> CountersBig {
        let f = |a: &Vec<u64>, b: &Vec<u64>| -> Vec<u128> {
            a.iter().zip(b).map(|(x, y)| *x as u128 + *y as u128 * n).collect()
        };
        CountersBig {
            cycles: f(&self.cycles, &delta.cycles),
            produced: f(&self.produced, &delta.produced),
            consumed: f(&self.consumed, &delta.consumed),
        }
    }

    pub fn is_zero(&self) -> bool {
        self.cycles.iter().all(|&x| x == 0)
            && self.produced.iter().all(|&x| x == 0)
            && self.consumed.iter().all(|&x| x == 0)
    }

    /// Aggregate a per-instance result over `mult` identical instances.
    pub fn scale(&self, mult: u128) -> CountersBig {
        CountersBig {
            cycles: self.cycles.iter().map(|&x| x as u128 * mult).collect(),
            produced: self.produced.iter().map(|&x| x as u128 * mult).collect(),
            consumed: self.consumed.iter().map(|&x| x as u128 * mult).collect(),
        }
    }
}

fn zip_sub(a: &[u64], b: &[u64]) -> Vec<u64> {
    a.iter().zip(b).map(|(x, y)| x - y).collect()
}

/// Wide accumulator for deployment-level totals (a billion instances overflows u64).
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct CountersBig {
    pub cycles: Vec<u128>,
    pub produced: Vec<u128>,
    pub consumed: Vec<u128>,
}

impl CountersBig {
    pub fn zeroed(n_actors: usize, n_items: usize) -> CountersBig {
        CountersBig {
            cycles: vec![0; n_actors],
            produced: vec![0; n_items],
            consumed: vec![0; n_items],
        }
    }
    pub fn add(&mut self, o: &CountersBig) {
        for (a, b) in self.cycles.iter_mut().zip(&o.cycles) {
            *a += b;
        }
        for (a, b) in self.produced.iter_mut().zip(&o.produced) {
            *a += b;
        }
        for (a, b) in self.consumed.iter_mut().zip(&o.consumed) {
            *a += b;
        }
    }
    pub fn from_narrow(c: &Counters) -> CountersBig {
        c.scale(1)
    }
    /// Aggregate this single-instance result over `mult` identical instances.
    pub fn scale_u128(&self, mult: u128) -> CountersBig {
        CountersBig {
            cycles: self.cycles.iter().map(|x| x * mult).collect(),
            produced: self.produced.iter().map(|x| x * mult).collect(),
            consumed: self.consumed.iter().map(|x| x * mult).collect(),
        }
    }
}

#[derive(Clone)]
pub struct Snapshot {
    qty: Vec<Qty>,
    used: Vec<Qty>,
    state: Vec<u8>,
    deadline: Vec<Tick>,
    heap: BinaryHeap<Reverse<Ev>>,
    now: Tick,
    c: Counters,
    events: u64,
}

pub struct World<'a> {
    pub bp: &'a Blueprint,
    pub n_inst: u64,
    pub stagger: u64,
    pub n_items: usize,
    /// Per-instance item quantities, `inst * qty_stride + slot`.
    qty: Vec<Qty>,
    /// Per-storage occupancy, `inst * n_storages + s`.
    used: Vec<Qty>,
    /// Machine state, `inst * n_actors + a`.
    state: Vec<u8>,
    /// Absolute time of the pending event for WORKING / DORMANT machines.
    deadline: Vec<Tick>,
    heap: BinaryHeap<Reverse<Ev>>,
    pub now: Tick,
    pub c: Counters,
    pub events: u64,
    // Reused scratch, so the hot path never allocates.
    plan: Vec<(u32, u16, Qty)>,
    touched: Vec<u16>,
}

impl<'a> World<'a> {
    /// Bytes of state for `n_inst` instances, before the event heap.
    pub fn state_bytes(bp: &Blueprint, n_inst: u64) -> u128 {
        let per = bp.qty_stride as u128 * 8
            + bp.storages.len() as u128 * 8
            + bp.actors.len() as u128 * 9;
        per * n_inst as u128
    }

    pub fn new(bp: &'a Blueprint, n_items: usize, n_inst: u64, stagger: u64) -> World<'a> {
        let ns = bp.storages.len() as u64;
        let na = bp.actors.len() as u64;
        let mut w = World {
            bp,
            n_inst,
            stagger,
            n_items,
            qty: vec![0; (n_inst * bp.qty_stride as u64) as usize],
            used: vec![0; (n_inst * ns) as usize],
            state: vec![S_DORMANT | RETRY; (n_inst * na) as usize],
            deadline: vec![0; (n_inst * na) as usize],
            heap: BinaryHeap::with_capacity((n_inst * na) as usize),
            now: 0,
            c: Counters::zeroed(na as usize, n_items),
            events: 0,
            plan: Vec::with_capacity(8),
            touched: Vec::with_capacity(8),
        };
        // Instance k lies dormant until its phase offset, which is how a
        // deployment of identical lines acquires distinct archetypes.
        let modulus = bp.base_period.max(1);
        for i in 0..n_inst {
            let offset = if stagger == 0 {
                0
            } else {
                (i as u128 * stagger as u128 % modulus as u128) as u64
            };
            for a in 0..na {
                let gi = i * na + a;
                w.deadline[gi as usize] = offset;
                w.heap.push(Reverse(Ev::new(offset, PRIO_RETRY, gi)));
            }
        }
        w
    }

    pub fn heap_bytes(&self) -> u128 {
        self.heap.capacity() as u128 * std::mem::size_of::<Reverse<Ev>>() as u128
    }

    pub fn total_bytes(&self) -> u128 {
        (self.qty.capacity() * 8
            + self.used.capacity() * 8
            + self.state.capacity()
            + self.deadline.capacity() * 8) as u128
            + self.heap_bytes()
    }

    /// True once every event scheduled at the current tick has been applied.
    pub fn quiescent(&self) -> bool {
        match self.heap.peek() {
            None => true,
            Some(Reverse(e)) => e.at > self.now,
        }
    }

    /// No events remain: the plant is frozen for all future time.
    pub fn frozen(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn run_until(&mut self, t_end: Tick) {
        while let Some(&Reverse(ev)) = self.heap.peek() {
            if ev.at > t_end {
                break;
            }
            self.heap.pop();
            self.now = ev.at;
            self.events += 1;
            self.step(ev);
        }
        if t_end > self.now {
            self.now = t_end;
        }
    }

    /// Run until quiescent-and-probeable, calling `probe` at every point where
    /// the clock is about to advance. Stops early if `probe` returns false.
    pub fn run_probed(&mut self, t_end: Tick, mut probe: impl FnMut(&World<'a>) -> bool) -> bool {
        loop {
            let next = self.heap.peek().map(|r| r.0);
            let Some(ev) = next else {
                // Frozen: no event will ever fire again.
                return probe(self);
            };
            if ev.at > self.now && !probe(self) {
                return false;
            }
            if ev.at > t_end {
                self.now = t_end;
                return true;
            }
            self.heap.pop();
            self.now = ev.at;
            self.events += 1;
            self.step(ev);
        }
    }

    fn step(&mut self, ev: Ev) {
        let na = self.bp.actors.len() as u64;
        let who = ev.who();
        let inst = who / na;
        let a = (who % na) as usize;
        let gi = who as usize;
        let st = self.state[gi] & !RETRY;
        self.state[gi] = st;
        self.touched.clear();
        match st {
            S_WORKING => {
                if self.deadline[gi] == ev.at {
                    self.finish(inst, a);
                }
            }
            S_DORMANT | S_STARVED => self.begin(inst, a),
            S_BLOCKED => self.finish(inst, a),
            _ => {}
        }
        self.wake_touched(inst);
    }

    /// Attempt to withdraw the full input set atomically and start a cycle.
    fn begin(&mut self, inst: u64, a: usize) {
        let bp = self.bp;
        let ad = &bp.actors[a];
        let na = bp.actors.len() as u64;
        let gi = (inst * na + a as u64) as usize;

        let mut plan = std::mem::take(&mut self.plan);
        plan.clear();
        let ok = self.reserve_inputs(inst, ad, &mut plan);
        if !ok {
            self.state[gi] = S_STARVED;
            self.plan = plan;
            return;
        }
        let ns = bp.storages.len() as u64;
        for &(qi, s, q) in plan.iter() {
            self.qty[qi as usize] -= q;
            self.used[(inst * ns + s as u64) as usize] -= q;
            if !self.touched.contains(&s) {
                self.touched.push(s);
            }
        }
        for st in &ad.inputs {
            self.c.consumed[st.item as usize] += st.qty;
        }
        self.plan = plan;

        self.state[gi] = S_WORKING;
        let dl = self.now + ad.duration;
        self.deadline[gi] = dl;
        self.heap.push(Reverse(Ev::new(dl, PRIO_FINISH, gi as u64)));
    }

    /// Attempt to deposit the full output set atomically, then start the next cycle.
    fn finish(&mut self, inst: u64, a: usize) {
        let bp = self.bp;
        let ad = &bp.actors[a];
        let na = bp.actors.len() as u64;
        let gi = (inst * na + a as u64) as usize;

        let mut plan = std::mem::take(&mut self.plan);
        plan.clear();
        let ok = self.reserve_space(inst, ad, &mut plan);
        if !ok {
            self.state[gi] = S_BLOCKED;
            self.plan = plan;
            return;
        }
        let ns = bp.storages.len() as u64;
        for &(qi, s, q) in plan.iter() {
            self.qty[qi as usize] += q;
            self.used[(inst * ns + s as u64) as usize] += q;
            if !self.touched.contains(&s) {
                self.touched.push(s);
            }
        }
        for st in &ad.outputs {
            self.c.produced[st.item as usize] += st.qty;
        }
        self.plan = plan;
        self.c.cycles[a] += 1;

        // Immediately attempt the next cycle at the same tick. Doing this before
        // waking neighbours keeps a self-feeding machine correct.
        self.begin(inst, a);
    }

    fn reserve_inputs(
        &self,
        inst: u64,
        ad: &ActorDef,
        plan: &mut Vec<(u32, u16, Qty)>,
    ) -> bool {
        let bp = self.bp;
        for st in &ad.inputs {
            let mut need = st.qty;
            for &s in &ad.in_stores {
                let Some(slot) = bp.slot_of(s as usize, st.item) else {
                    continue;
                };
                let qi = (inst * bp.qty_stride as u64) as u32 + slot;
                let planned: Qty = plan
                    .iter()
                    .filter(|(q, _, _)| *q == qi)
                    .map(|(_, _, v)| *v)
                    .sum();
                let avail = self.qty[qi as usize] - planned;
                if avail == 0 {
                    continue;
                }
                let take = need.min(avail);
                plan.push((qi, s, take));
                need -= take;
                if need == 0 {
                    break;
                }
            }
            if need > 0 {
                return false;
            }
        }
        true
    }

    fn reserve_space(&self, inst: u64, ad: &ActorDef, plan: &mut Vec<(u32, u16, Qty)>) -> bool {
        let bp = self.bp;
        let ns = bp.storages.len() as u64;
        for st in &ad.outputs {
            let mut need = st.qty;
            for &s in &ad.out_stores {
                let Some(slot) = bp.slot_of(s as usize, st.item) else {
                    continue;
                };
                let qi = (inst * bp.qty_stride as u64) as u32 + slot;
                let planned: Qty = plan
                    .iter()
                    .filter(|(_, ps, _)| *ps == s)
                    .map(|(_, _, v)| *v)
                    .sum();
                let sd = &bp.storages[s as usize];
                let room = sd.capacity - self.used[(inst * ns + s as u64) as usize] - planned;
                if room == 0 {
                    continue;
                }
                let put = need.min(room);
                plan.push((qi, s, put));
                need -= put;
                if need == 0 {
                    break;
                }
            }
            if need > 0 {
                return false;
            }
        }
        true
    }

    /// Re-arm machines that were waiting on a storage we just mutated.
    fn wake_touched(&mut self, inst: u64) {
        if self.touched.is_empty() {
            return;
        }
        let bp = self.bp;
        let na = bp.actors.len() as u64;
        let touched = std::mem::take(&mut self.touched);
        for &s in touched.iter() {
            for &client in &bp.storages[s as usize].clients {
                let gi = (inst * na + client as u64) as usize;
                let st = self.state[gi];
                if st & RETRY != 0 {
                    continue;
                }
                if st == S_STARVED || st == S_BLOCKED {
                    self.state[gi] = st | RETRY;
                    self.heap.push(Reverse(Ev::new(self.now, PRIO_RETRY, gi as u64)));
                }
            }
        }
        self.touched = touched;
        self.touched.clear();
    }

    // -------------------------------------------------------- inspection

    pub fn storage_qty(&self, inst: u64, storage: usize, item: ItemId) -> Qty {
        match self.bp.slot_of(storage, item) {
            Some(slot) => self.qty[(inst * self.bp.qty_stride as u64 + slot as u64) as usize],
            None => 0,
        }
    }

    pub fn storage_used(&self, inst: u64, storage: usize) -> Qty {
        self.used[(inst * self.bp.storages.len() as u64 + storage as u64) as usize]
    }

    pub fn actor_state(&self, inst: u64, actor: usize) -> u8 {
        self.state[(inst * self.bp.actors.len() as u64 + actor as u64) as usize] & !RETRY
    }

    pub fn state_name(s: u8) -> &'static str {
        match s {
            S_WORKING => "working",
            S_STARVED => "starved",
            S_BLOCKED => "blocked",
            S_DORMANT => "dormant",
            _ => "idle",
        }
    }

    /// Canonical encoding of the *complete* dynamical state, with times made
    /// relative to now. Two equal signatures at t1 < t2 prove the trajectory
    /// repeats with period t2 - t1. Only meaningful when quiescent.
    pub fn signature(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.qty.len() * 8 + self.state.len() * 9);
        for q in &self.qty {
            v.extend_from_slice(&q.to_le_bytes());
        }
        for (i, s) in self.state.iter().enumerate() {
            let st = *s & !RETRY;
            v.push(st);
            let rel = if st == S_WORKING || st == S_DORMANT {
                self.deadline[i].saturating_sub(self.now)
            } else {
                0
            };
            v.extend_from_slice(&rel.to_le_bytes());
        }
        v
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            qty: self.qty.clone(),
            used: self.used.clone(),
            state: self.state.clone(),
            deadline: self.deadline.clone(),
            heap: self.heap.clone(),
            now: self.now,
            c: self.c.clone(),
            events: self.events,
        }
    }

    pub fn restore(&mut self, s: &Snapshot) {
        self.qty.clone_from(&s.qty);
        self.used.clone_from(&s.used);
        self.state.clone_from(&s.state);
        self.deadline.clone_from(&s.deadline);
        self.heap.clone_from(&s.heap);
        self.now = s.now;
        self.c.clone_from(&s.c);
        self.events = s.events;
    }
}
