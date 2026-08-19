//! Discrete-event simulator (tier T1) -- the ground truth.
//!
//! No tick loop exists anywhere in this file. Time advances by popping the next
//! scheduled event, so cost is O(events), not O(ticks x objects).
//!
//! # v2: arbitration is a declared policy, not an accident
//!
//! v1 processed one machine per event, so whichever event happened to pop
//! first won any contention -- which meant *lowest array index always wins*.
//! That is deterministic and it is also a logistics policy nobody chose.
//!
//! v2 replaces this with **rounds**. At tick `t`:
//!
//! ```text
//!   round:
//!     phase A -- every machine whose work has finished tries to deposit,
//!                in the order its storage's policy dictates
//!     phase B -- every idle machine tries to withdraw, likewise
//!   repeat while anything succeeded (a deposit can unblock a withdrawal
//!   at the same tick), then advance the clock
//! ```
//!
//! Contention is resolved *at a storage*, between its client **classes**, by
//! `Policy`. Within a class, service is FIFO -- which for a set of machines
//! that are all idle-and-waiting is exactly round-robin rotation.
//!
//! That split is the load-bearing idea of the whole experiment:
//!
//! > A class is precisely the set of machines the arbiter refuses to
//! > distinguish. Every member of a queue is in the identical local state, so
//! > *which* member gets served is an automorphism of the state -- it changes
//! > who did the work and cannot change how much work was done.
//!
//! Aggregate answers therefore do not depend on intra-class order at all,
//! which is why `pop.rs` can throw that information away and still be exact.
//!
//! # Layout
//!
//! State lives in struct-of-arrays columns indexed by `instance * stride +
//! local`, so one `World` is simultaneously the single-instance simulator used
//! by the analytic solver and the flat arena used for million-object runs.
//! Waiting machines live in intrusive linked lists (`next_link`), so a stalled
//! machine costs four bytes and zero allocations.

use crate::model::*;
use std::cmp::Reverse;
use std::collections::BinaryHeap;

pub const S_IDLE: u8 = 0;
/// Cycle in progress; `deadline` holds the finish time.
pub const S_WORKING: u8 = 1;
/// Waiting in its class's withdraw queue.
pub const S_STARVED: u8 = 2;
/// Work complete, waiting in its class's deposit queue.
pub const S_DONE: u8 = 3;
/// Not started yet; `deadline` holds the wake time.
pub const S_DORMANT: u8 = 4;

pub const NIL: u32 = u32::MAX;
/// Stands in for "this request queues at no storage at all" -- a source
/// starting a cycle, or a sink swallowing one.
const NO_STORE: u32 = u32::MAX;

pub fn state_name(s: u8) -> &'static str {
    match s {
        S_WORKING => "working",
        S_STARVED => "starved",
        S_DONE => "blocked",
        S_DORMANT => "dormant",
        _ => "idle",
    }
}

#[derive(Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Debug)]
pub struct Ev {
    pub at: Tick,
    /// Global machine index. Using it as the tiebreak makes the pop order
    /// ascending by instance, so events at one tick arrive already grouped.
    who: u64,
}

/// Extensive quantities that grow linearly across a periodic orbit.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Counters {
    /// Completed cycles per blueprint-local actor *class*, summed over the
    /// class population and over instances.
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
    status: Vec<u8>,
    deadline: Vec<Tick>,
    link: Vec<u32>,
    qhead: Vec<u32>,
    qtail: Vec<u32>,
    qlen: Vec<u64>,
    rr: Vec<u16>,
    member_cycles: Vec<u32>,
    heap: BinaryHeap<Reverse<Ev>>,
    now: Tick,
    c: Counters,
    events: u64,
    rounds: u64,
}

/// Queue selector: three intrusive lists per (instance, class).
const Q_DONE: usize = 0;
const Q_STARVED: usize = 1;
/// Machines mid-cycle, in the order they started.
///
/// Keeping them in a queue rather than just a status field is what makes
/// intra-class rotation survive a cycle. Timer events fire in machine-index
/// order, so re-queueing finishers straight from the event stream resets the
/// rotation to index order every time a whole class finishes together -- and
/// then the low indices win every contention, which is the exact behaviour v2
/// exists to remove. Members of a class share a cycle time, so they finish in
/// the order they started, and popping this queue from the front hands them
/// back in that order instead.
const Q_WORK: usize = 2;
const NQ: usize = 3;

pub struct World<'a> {
    pub bp: &'a Blueprint,
    pub n_inst: u64,
    pub stagger: u64,
    pub n_items: usize,
    /// Per-instance item quantities, `inst * qty_stride + slot`.
    qty: Vec<Qty>,
    /// Per-storage occupancy, `inst * n_storages + s`.
    used: Vec<Qty>,
    /// Machine status, `inst * machines + m`.
    status: Vec<u8>,
    /// Absolute time of the pending timer for WORKING / DORMANT machines.
    deadline: Vec<Tick>,
    /// Intrusive FIFO link, one entry per machine.
    link: Vec<u32>,
    /// Queue heads/tails/lengths, `(inst * n_classes + class) * NQ + q`.
    qhead: Vec<u32>,
    qtail: Vec<u32>,
    qlen: Vec<u64>,
    /// Round-robin service pointers, `(inst * n_storages + s) * NQ + q`.
    /// Deposits and withdrawals rotate independently.
    rr: Vec<u16>,
    /// Optional per-machine completed-cycle counts, for fairness histograms.
    member_cycles: Vec<u32>,
    /// machine index within an instance -> class index. Precomputed because
    /// the alternative is a scan over classes on every single transition.
    machine_class: Vec<u16>,
    heap: BinaryHeap<Reverse<Ev>>,
    pub now: Tick,
    pub c: Counters,
    pub events: u64,
    /// Arbitration rounds executed. Interesting because it is the price of
    /// having a contention policy at all.
    pub rounds: u64,
    // Reused scratch, so the hot path never allocates.
    fired: Vec<u64>,
    finishing: Vec<u64>,
    plan: Vec<(u32, u16, Qty)>,
    cand: Vec<(u32, u32, u16)>,
}

impl<'a> World<'a> {
    /// Bytes of state for `n_inst` instances, before the event heap.
    pub fn state_bytes(bp: &Blueprint, n_inst: u64) -> u128 {
        let per = bp.qty_stride as u128 * 8
            + bp.storages.len() as u128 * 10
            + bp.machines as u128 * 13
            + bp.actors.len() as u128 * (NQ as u128 * 16);
        per * n_inst as u128
    }

    pub fn new(bp: &'a Blueprint, n_items: usize, n_inst: u64, stagger: u64) -> World<'a> {
        Self::build(bp, n_items, n_inst, stagger, false)
    }

    /// As `new`, but also tracks completed cycles for every individual machine
    /// so per-member fairness can be measured. Costs 4 bytes per machine.
    pub fn new_tracked(bp: &'a Blueprint, n_items: usize, n_inst: u64, stagger: u64) -> World<'a> {
        Self::build(bp, n_items, n_inst, stagger, true)
    }

    fn build(
        bp: &'a Blueprint,
        n_items: usize,
        n_inst: u64,
        stagger: u64,
        track: bool,
    ) -> World<'a> {
        let ns = bp.storages.len() as u64;
        let nc = bp.actors.len() as u64;
        let nm = bp.machines;
        let total_m = n_inst * nm;
        assert!(
            total_m <= u32::MAX as u64,
            "materialising {total_m} machines exceeds the 32-bit machine index"
        );
        let mut w = World {
            bp,
            n_inst,
            stagger,
            n_items,
            qty: vec![0; (n_inst * bp.qty_stride as u64) as usize],
            used: vec![0; (n_inst * ns) as usize],
            status: vec![S_DORMANT; total_m as usize],
            deadline: vec![0; total_m as usize],
            link: vec![NIL; total_m as usize],
            qhead: vec![NIL; (n_inst * nc) as usize * NQ],
            qtail: vec![NIL; (n_inst * nc) as usize * NQ],
            qlen: vec![0; (n_inst * nc) as usize * NQ],
            rr: vec![0; (n_inst * ns) as usize * NQ],
            member_cycles: if track { vec![0; total_m as usize] } else { Vec::new() },
            machine_class: {
                let mut t = vec![0u16; nm as usize];
                for (ci, ad) in bp.actors.iter().enumerate() {
                    for m in ad.machine_offset..ad.machine_offset + ad.count {
                        t[m as usize] = ci as u16;
                    }
                }
                t
            },
            heap: BinaryHeap::with_capacity(total_m as usize),
            now: 0,
            c: Counters::zeroed(nc as usize, n_items),
            events: 0,
            rounds: 0,
            fired: Vec::with_capacity(64),
            finishing: Vec::new(),
            plan: Vec::with_capacity(8),
            cand: Vec::with_capacity(16),
        };

        // Seed declared initial contents. Without these a cycle can never turn
        // over: a loop that consumes its own output starts with nothing.
        for i in 0..n_inst {
            for (s, sd) in bp.storages.iter().enumerate() {
                for st in &sd.initial {
                    let slot = bp.slot_of(s, st.item).expect("initial item has no slot");
                    w.qty[(i * bp.qty_stride as u64 + slot as u64) as usize] += st.qty;
                    w.used[(i * ns + s as u64) as usize] += st.qty;
                }
            }
        }

        // Instance k lies dormant until its phase offset, which is how a
        // deployment of identical lines acquires distinct archetypes.
        let modulus = bp.base_period.max(1);
        for i in 0..n_inst {
            let offset = if stagger == 0 {
                0
            } else {
                (i as u128 * stagger as u128 % modulus as u128) as u64
            };
            for m in 0..nm {
                let gi = i * nm + m;
                w.deadline[gi as usize] = offset;
                w.heap.push(Reverse(Ev { at: offset, who: gi }));
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
            + self.status.capacity()
            + self.deadline.capacity() * 8
            + self.link.capacity() * 4
            + self.qhead.capacity() * 4
            + self.qtail.capacity() * 4
            + self.qlen.capacity() * 8
            + self.rr.capacity() * 2
            + self.member_cycles.capacity() * 4) as u128
            + self.heap_bytes()
    }

    /// No timers remain: the plant is frozen for all future time.
    ///
    /// Machines sitting in a starve or deposit queue can only be revived by a
    /// storage mutation, and every mutation is caused by a machine finishing a
    /// cycle. With no timer pending, nothing will ever finish.
    pub fn frozen(&self) -> bool {
        self.heap.is_empty()
    }

    pub fn run_until(&mut self, t_end: Tick) {
        while let Some(&Reverse(ev)) = self.heap.peek() {
            if ev.at > t_end {
                break;
            }
            self.advance_to(ev.at);
        }
        if t_end > self.now {
            self.now = t_end;
        }
    }

    /// Run forward, calling `probe` at every point where the clock is about to
    /// advance -- exactly the moments at which the state is quiescent and so
    /// comparable across time. Stops early if `probe` returns false.
    pub fn run_probed(&mut self, t_end: Tick, mut probe: impl FnMut(&World<'a>) -> bool) -> bool {
        loop {
            let Some(&Reverse(ev)) = self.heap.peek() else {
                // Frozen: no timer will ever fire again.
                return probe(self);
            };
            if ev.at > self.now && !probe(self) {
                return false;
            }
            if ev.at > t_end {
                self.now = t_end;
                return true;
            }
            self.advance_to(ev.at);
        }
    }

    /// Fire every timer at `t`, then run arbitration rounds to quiescence.
    fn advance_to(&mut self, t: Tick) {
        self.now = t;
        let nm = self.bp.machines;

        // Drain all timers at this tick. Heap order is (time, global machine
        // index), so `fired` comes out ascending and therefore grouped by
        // instance -- arbitration is per instance, so that grouping is free.
        let mut fired = std::mem::take(&mut self.fired);
        fired.clear();
        while let Some(&Reverse(ev)) = self.heap.peek() {
            if ev.at != t {
                break;
            }
            self.heap.pop();
            self.events += 1;
            fired.push(ev.who);
        }

        let nc = self.bp.actors.len();
        let mut finishing = std::mem::take(&mut self.finishing);
        let mut i = 0;
        while i < fired.len() {
            let inst = fired[i] / nm;
            let start = i;
            while i < fired.len() && fired[i] / nm == inst {
                i += 1;
            }
            finishing.clear();
            finishing.resize(nc, 0);
            for k in start..i {
                let gi = fired[k];
                let m = gi % nm;
                match self.status[gi as usize] {
                    S_DORMANT => {
                        if self.deadline[gi as usize] == t {
                            self.enqueue(inst, m, Q_STARVED);
                        }
                    }
                    // Count them rather than moving them: the machine to hand
                    // back is whoever is at the head of the work queue, not
                    // whoever the heap happened to name.
                    S_WORKING => {
                        if self.deadline[gi as usize] == t {
                            finishing[self.machine_class[m as usize] as usize] += 1;
                        }
                    }
                    _ => {}
                }
            }
            for class in 0..nc {
                let mut n = finishing[class];
                if n == 0 {
                    continue;
                }
                let slot = self.qslot(inst, class as u64, Q_WORK);
                while n > 0 {
                    let gi = self.pop_front(slot);
                    debug_assert!(gi != NIL, "work queue shorter than the timers it owns");
                    debug_assert_eq!(self.deadline[gi as usize], t, "work queue out of order");
                    self.enqueue(inst, gi as u64 % nm, Q_DONE);
                    n -= 1;
                }
            }
            self.arbitrate(inst);
        }
        self.fired = fired;
        self.finishing = finishing;
    }

    // ------------------------------------------------------------ queues

    #[inline]
    fn qslot(&self, inst: u64, class: u64, q: usize) -> usize {
        ((inst * self.bp.actors.len() as u64 + class) as usize) * NQ + q
    }

    /// Append a machine to the back of one of its class's queues. FIFO here is
    /// what makes intra-class service a fair rotation.
    fn enqueue(&mut self, inst: u64, m: u64, q: usize) {
        let nm = self.bp.machines;
        let gi = (inst * nm + m) as u32;
        let class = self.machine_class[m as usize] as u64;
        self.status[gi as usize] = match q {
            Q_DONE => S_DONE,
            Q_WORK => S_WORKING,
            _ => S_STARVED,
        };
        self.link[gi as usize] = NIL;
        let slot = self.qslot(inst, class, q);
        if self.qtail[slot] == NIL {
            self.qhead[slot] = gi;
        } else {
            self.link[self.qtail[slot] as usize] = gi;
        }
        self.qtail[slot] = gi;
        self.qlen[slot] += 1;
    }

    /// Return a machine that failed its attempt to the *front* of its queue:
    /// it never got served, so it keeps its place in the rotation.
    fn requeue_front(&mut self, slot: usize, gi: u32) {
        self.link[gi as usize] = self.qhead[slot];
        self.qhead[slot] = gi;
        if self.qtail[slot] == NIL {
            self.qtail[slot] = gi;
        }
        self.qlen[slot] += 1;
    }

    fn pop_front(&mut self, slot: usize) -> u32 {
        let gi = self.qhead[slot];
        if gi == NIL {
            return NIL;
        }
        self.qhead[slot] = self.link[gi as usize];
        if self.qhead[slot] == NIL {
            self.qtail[slot] = NIL;
        }
        self.link[gi as usize] = NIL;
        self.qlen[slot] -= 1;
        gi
    }

    // ------------------------------------------------------- arbitration

    /// Rank of a class among a storage's clients, lower served first.
    ///
    /// `Index` and `Priority` read straight off the storage's declared order.
    /// `RoundRobin` rotates that order by the storage's live pointer, so the
    /// class that went first last time goes last this time.
    fn rank(&self, inst: u64, store: u16, class: u16, q: usize) -> u32 {
        let sd = &self.bp.storages[store as usize];
        let queue = sd.queue(q == Q_DONE);
        let n = queue.len();
        let pos = queue.iter().position(|&c| c == class).unwrap_or(n) as u32;
        match sd.policy {
            Policy::RoundRobin => {
                let p = self.rr[self.rslot(inst, store, q)] as u32;
                (pos + n as u32 - p) % n.max(1) as u32
            }
            _ => pos,
        }
    }

    #[inline]
    fn rslot(&self, inst: u64, store: u16, q: usize) -> usize {
        ((inst * self.bp.storages.len() as u64 + store as u64) as usize) * NQ + q
    }

    /// Run arbitration rounds for one instance until nothing more can happen
    /// at the current tick.
    fn arbitrate(&mut self, inst: u64) {
        loop {
            self.rounds += 1;
            let mut progress = false;
            progress |= self.phase(inst, Q_DONE);
            progress |= self.phase(inst, Q_STARVED);
            if !progress {
                return;
            }
        }
    }

    /// One phase: every class with a pending request of this kind attempts it,
    /// under the policy of the storage it is queueing at. Returns whether
    /// anything succeeded.
    fn phase(&mut self, inst: u64, q: usize) -> bool {
        let nc = self.bp.actors.len();

        // Build the candidate list: (primary storage, rank, class). Sorting by
        // storage first gives each storage its own contiguous run, so each one
        // is arbitrated independently and the whole thing is a total order.
        let mut cand = std::mem::take(&mut self.cand);
        cand.clear();
        for class in 0..nc {
            if self.qlen[self.qslot(inst, class as u64, q)] == 0 {
                continue;
            }
            let ad = &self.bp.actors[class];
            let primary = if q == Q_DONE { ad.primary_out() } else { ad.primary_in() };
            match primary {
                Some(s) => cand.push((s as u32, self.rank(inst, s, class as u16, q), class as u16)),
                // A source withdraws from nowhere and a sink deposits nowhere,
                // so there is no storage for them to queue at and nothing they
                // can contend for. They sort last and always succeed.
                None => cand.push((NO_STORE, 0, class as u16)),
            }
        }
        if cand.is_empty() {
            self.cand = cand;
            return false;
        }
        cand.sort_unstable();

        let mut progress = false;
        let mut k = 0;
        while k < cand.len() {
            let store = cand[k].0;
            let mut end = k;
            while end < cand.len() && cand[end].0 == store {
                end += 1;
            }
            let rr_here = store != NO_STORE
                && self.bp.storages[store as usize].policy == Policy::RoundRobin;
            if rr_here {
                progress |= self.serve_laps(inst, q, store as u16, &cand[k..end]);
            } else {
                // Strict order: each class in turn takes everything it can.
                for j in k..end {
                    while self.serve_one(inst, q, cand[j].2) {
                        progress = true;
                    }
                }
            }
            k = end;
        }

        self.cand = cand;
        progress
    }

    /// Serve one member of a class, or report that the storage refused.
    fn serve_one(&mut self, inst: u64, q: usize, class: u16) -> bool {
        let slot = self.qslot(inst, class as u64, q);
        let gi = self.pop_front(slot);
        if gi == NIL {
            return false;
        }
        let ok = if q == Q_DONE {
            self.try_deposit(inst, gi)
        } else {
            self.try_withdraw(inst, gi)
        };
        if !ok {
            self.requeue_front(slot, gi);
        }
        ok
    }

    /// Round-robin: deal one member per class per lap until the storage runs
    /// dry.
    ///
    /// The obvious implementation -- let each class take everything it can
    /// before moving on -- is not round-robin at all. A class of six thousand
    /// idle smelters is never satisfied, so a pointer that waits for
    /// satisfaction never moves and the class behind it starves exactly as it
    /// would have under `index`. Dealing one at a time is what actually shares.
    ///
    /// The result is max-min fair: every class gets the same number of grants
    /// until it stops asking or the storage stops giving, and only the
    /// remainder of the final incomplete lap depends on the rotation pointer.
    fn serve_laps(&mut self, inst: u64, q: usize, store: u16, group: &[(u32, u32, u16)]) -> bool {
        let mut progress = false;
        let n = group.len().min(64);
        let mut alive = [true; 64];
        let mut got = [false; 64];
        loop {
            let mut any = false;
            for i in 0..n {
                if !alive[i] {
                    continue;
                }
                if self.serve_one(inst, q, group[i].2) {
                    any = true;
                    progress = true;
                    got[i] = true;
                } else {
                    // Resources only shrink within a phase, so a class that has
                    // been refused once will be refused for the rest of it.
                    alive[i] = false;
                }
            }
            if !any {
                break;
            }
        }
        // The pointer only ever decides who picks up the remainder of the last
        // incomplete lap, so advancing past the last class that got anything is
        // enough to make that remainder rotate.
        if let Some(i) = (0..n).rev().find(|&i| got[i]) {
            let queue = self.bp.storages[store as usize].queue(q == Q_DONE);
            if !queue.is_empty() {
                let pos = queue.iter().position(|&x| x == group[i].2).unwrap_or(0);
                let rslot = self.rslot(inst, store, q);
                self.rr[rslot] = ((pos + 1) % queue.len()) as u16;
            }
        }
        progress
    }

    // ------------------------------------------------------- transitions

    /// Deposit a finished machine's outputs atomically; on success it counts a
    /// cycle and immediately attempts the next one.
    fn try_deposit(&mut self, inst: u64, gi: u32) -> bool {
        let nm = self.bp.machines;
        let m = gi as u64 % nm;
        let class = self.machine_class[m as usize] as usize;
        let ad = &self.bp.actors[class];

        let mut plan = std::mem::take(&mut self.plan);
        plan.clear();
        if !self.reserve_space(inst, ad, &mut plan) {
            self.plan = plan;
            return false;
        }
        let ns = self.bp.storages.len() as u64;
        for &(qi, s, qv) in plan.iter() {
            self.qty[qi as usize] += qv;
            self.used[(inst * ns + s as u64) as usize] += qv;
        }
        for st in &ad.outputs {
            self.c.produced[st.item as usize] += st.qty;
        }
        self.plan = plan;
        self.c.cycles[class] += 1;
        if !self.member_cycles.is_empty() {
            self.member_cycles[gi as usize] += 1;
        }

        // The machine now joins the *back* of the withdraw queue rather than
        // starting its next cycle on the spot.
        //
        // v1 let a machine that had just deposited re-withdraw immediately,
        // ahead of everyone waiting. That is an incumbency advantage nobody
        // declared, and it is the same class of bug as "lowest index wins":
        // an arbitration rule smuggled in as an implementation detail. Phases
        // are arbitrated separately here, so the deposit is settled first and
        // the withdrawal takes its turn under the storage's actual policy.
        // The round loop reruns both phases within the tick, so nothing is
        // delayed by a tick -- only reordered, fairly.
        self.enqueue(inst, m, Q_STARVED);
        true
    }

    /// Withdraw the full input set atomically and start a cycle.
    fn try_withdraw(&mut self, inst: u64, gi: u32) -> bool {
        let nm = self.bp.machines;
        let m = gi as u64 % nm;
        let class = self.machine_class[m as usize] as usize;
        let ad = &self.bp.actors[class];

        let mut plan = std::mem::take(&mut self.plan);
        plan.clear();
        if !self.reserve_inputs(inst, ad, &mut plan) {
            self.plan = plan;
            return false;
        }
        let ns = self.bp.storages.len() as u64;
        for &(qi, s, qv) in plan.iter() {
            self.qty[qi as usize] -= qv;
            self.used[(inst * ns + s as u64) as usize] -= qv;
        }
        for st in &ad.inputs {
            self.c.consumed[st.item as usize] += st.qty;
        }
        self.plan = plan;

        let dl = self.now + ad.duration;
        self.deadline[gi as usize] = dl;
        self.heap.push(Reverse(Ev { at: dl, who: gi as u64 }));
        self.enqueue(inst, m, Q_WORK);
        true
    }

    fn reserve_inputs(&self, inst: u64, ad: &ActorDef, plan: &mut Vec<(u32, u16, Qty)>) -> bool {
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

    pub fn machine_status(&self, inst: u64, machine: u64) -> u8 {
        self.status[(inst * self.bp.machines + machine) as usize]
    }

    pub fn member_cycles(&self, inst: u64, machine: u64) -> u32 {
        self.member_cycles[(inst * self.bp.machines + machine) as usize]
    }

    /// Histogram of machine statuses within one class, for one instance.
    /// `[idle, working, starved, done, dormant]`.
    pub fn class_histogram(&self, inst: u64, class: usize) -> [u64; 5] {
        let ad = &self.bp.actors[class];
        let mut h = [0u64; 5];
        for m in ad.machine_offset..ad.machine_offset + ad.count {
            h[self.machine_status(inst, m) as usize] += 1;
        }
        h
    }

    /// Canonical encoding of the *complete* dynamical state, with times made
    /// relative to now.
    ///
    /// Note what is **not** in here: which individual machine sits where in a
    /// queue. Every member of a queue is in the identical local state, so
    /// permuting them is a symmetry of the system; the signature quotients it
    /// out deliberately. Two equal signatures at t1 < t2 therefore prove the
    /// trajectory repeats *up to relabelling machines within a class*, which is
    /// exactly as much as any aggregate answer needs -- and is a far coarser,
    /// far more often satisfied condition than literal state equality.
    pub fn signature(&self) -> Vec<u8> {
        let nm = self.bp.machines;
        let mut v = Vec::with_capacity(self.qty.len() * 8 + 64);
        for q in &self.qty {
            v.extend_from_slice(&q.to_le_bytes());
        }
        for r in &self.rr {
            v.extend_from_slice(&r.to_le_bytes());
        }
        // Per class, a sorted multiset of (status, relative deadline).
        let mut bucket: Vec<(u8, Tick)> = Vec::new();
        for inst in 0..self.n_inst {
            for ad in &self.bp.actors {
                bucket.clear();
                for m in ad.machine_offset..ad.machine_offset + ad.count {
                    let gi = (inst * nm + m) as usize;
                    let st = self.status[gi];
                    let rel = if st == S_WORKING || st == S_DORMANT {
                        self.deadline[gi].saturating_sub(self.now)
                    } else {
                        0
                    };
                    bucket.push((st, rel));
                }
                bucket.sort_unstable();
                for (st, rel) in bucket.iter() {
                    v.push(*st);
                    v.extend_from_slice(&rel.to_le_bytes());
                }
                v.push(0xff);
            }
        }
        v
    }

    pub fn snapshot(&self) -> Snapshot {
        Snapshot {
            qty: self.qty.clone(),
            used: self.used.clone(),
            status: self.status.clone(),
            deadline: self.deadline.clone(),
            link: self.link.clone(),
            qhead: self.qhead.clone(),
            qtail: self.qtail.clone(),
            qlen: self.qlen.clone(),
            rr: self.rr.clone(),
            member_cycles: self.member_cycles.clone(),
            heap: self.heap.clone(),
            now: self.now,
            c: self.c.clone(),
            events: self.events,
            rounds: self.rounds,
        }
    }

    pub fn restore(&mut self, s: &Snapshot) {
        self.qty.clone_from(&s.qty);
        self.used.clone_from(&s.used);
        self.status.clone_from(&s.status);
        self.deadline.clone_from(&s.deadline);
        self.link.clone_from(&s.link);
        self.qhead.clone_from(&s.qhead);
        self.qtail.clone_from(&s.qtail);
        self.qlen.clone_from(&s.qlen);
        self.rr.clone_from(&s.rr);
        self.member_cycles.clone_from(&s.member_cycles);
        self.heap.clone_from(&s.heap);
        self.now = s.now;
        self.c.clone_from(&s.c);
        self.events = s.events;
        self.rounds = s.rounds;
    }
}
