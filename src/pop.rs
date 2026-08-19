//! Tier T5 -- the lumped population engine.
//!
//! v1's compression argument was that a billion *independent* lines are really
//! one line evaluated a few hundred times. Couple them through a shared buffer
//! and that argument dies: nothing is independent any more.
//!
//! This module makes the other argument. Take a class of N identical machines
//! contending for the same storage. They are genuinely coupled -- what one
//! withdraws, another cannot. But at any instant each of them is in one of a
//! very small number of local states, and *the members in a given state are
//! interchangeable*. So instead of
//!
//! ```text
//!   Smelter 1 .. Smelter 1_000_000_000     (a billion machine records)
//! ```
//!
//! keep
//!
//! ```text
//!   Smelter { idle: 1_923, working@+1: 2_011, ..., starved: 842 }
//! ```
//!
//! and evolve *that*. The state is O(cycle time) per class instead of O(N),
//! and N survives only as an integer inside the histogram.
//!
//! # Why this is exact, not an approximation
//!
//! Every machine that is queued at a storage is in the identical local state:
//! idle and asking for the same items, or finished and offering the same
//! items. Permuting the members of such a queue therefore maps the global
//! state to itself. The arbitration policy in `sim.rs` is defined so that only
//! *how many* members a class gets served can affect anything -- which member
//! is a free choice. So the quotient by "relabel machines within a class" is a
//! **strong lumping**: the population dynamics are well defined on their own,
//! and every aggregate the full simulator computes is recoverable from them.
//!
//! `sim.rs` is the ground truth and this is a claim about it, so the two are
//! run against each other rather than sharing code.
//!
//! # The one algorithmic trick
//!
//! Serving a class one member at a time would be O(N) again. Instead, ask
//! directly: *how many members can be served at once?* Feasibility is monotone
//! -- if k members fit then k-1 do, since their allocation is a prefix of the
//! same greedy fill -- so the answer is a binary search over k with an exact
//! scaled feasibility test. Serving ten machines and serving ten billion cost
//! the same handful of tests.

use crate::model::*;
use crate::sim::{Counters, CountersBig};
use std::collections::HashMap;

const Q_DONE: usize = 0;
const Q_STARVED: usize = 1;
/// Stands in for "this request queues at no storage at all" -- a source
/// starting a cycle, or a sink swallowing one.
const NO_STORE: u32 = u32::MAX;

/// The population state of one actor class.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct ClassPop {
    /// Members mid-cycle, as `(finish tick, how many)`, sorted by tick. At most
    /// `duration` entries can be live at once, whatever N is.
    pub working: Vec<(Tick, u64)>,
    /// Members idle, queued to withdraw.
    pub starved: u64,
    /// Members finished, queued to deposit.
    pub done: u64,
}

impl ClassPop {
    pub fn total(&self) -> u64 {
        self.starved + self.done + self.working.iter().map(|w| w.1).sum::<u64>()
    }
    pub fn working_total(&self) -> u64 {
        self.working.iter().map(|w| w.1).sum()
    }
    /// Distinct occupied cells: the actual compressed width of this class.
    pub fn distinct_states(&self) -> usize {
        self.working.len() + (self.starved > 0) as usize + (self.done > 0) as usize
    }
}

pub struct Pop<'a> {
    pub bp: &'a Blueprint,
    pub n_items: usize,
    qty: Vec<Qty>,
    used: Vec<Qty>,
    pub classes: Vec<ClassPop>,
    rr: Vec<u16>,
    pub now: Tick,
    pub c: Counters,
    /// Batch grants performed. The analogue of T1's event count, except one
    /// grant here can stand for a billion machines starting work at once.
    pub grants: u64,
    pub rounds: u64,
    cand: Vec<(u32, u32, u16)>,
}

impl<'a> Pop<'a> {
    pub fn new(bp: &'a Blueprint, n_items: usize) -> Pop<'a> {
        let mut qty = vec![0; bp.qty_stride as usize];
        let mut used = vec![0; bp.storages.len()];
        for (s, sd) in bp.storages.iter().enumerate() {
            for st in &sd.initial {
                let slot = bp.slot_of(s, st.item).expect("initial item has no slot");
                qty[slot as usize] += st.qty;
                used[s] += st.qty;
            }
        }
        let mut p = Pop {
            bp,
            n_items,
            qty,
            used,
            // Every machine starts idle and asking, exactly as `World` does
            // once its dormancy timer fires at t=0.
            classes: bp
                .actors
                .iter()
                .map(|a| ClassPop { working: Vec::new(), starved: a.count, done: 0 })
                .collect(),
            rr: vec![0; bp.storages.len() * 2],
            now: 0,
            c: Counters::zeroed(bp.actors.len(), n_items),
            grants: 0,
            rounds: 0,
            cand: Vec::with_capacity(16),
        };
        // t=0 is itself a tick that has to settle: everyone is idle and asking,
        // and whoever the policy favours starts work before the clock moves.
        p.settle();
        p
    }

    /// Machines in the whole plant, however few records that takes.
    pub fn population(&self) -> u64 {
        self.classes.iter().map(|c| c.total()).sum()
    }

    /// Cells actually occupied across all classes -- the compressed width.
    pub fn distinct_states(&self) -> usize {
        self.classes.iter().map(|c| c.distinct_states()).sum()
    }

    /// Next tick at which any machine finishes. `None` means frozen.
    pub fn next_time(&self) -> Option<Tick> {
        self.classes
            .iter()
            .filter_map(|c| c.working.first().map(|w| w.0))
            .min()
    }

    pub fn frozen(&self) -> bool {
        self.next_time().is_none()
    }

    pub fn run_until(&mut self, t_end: Tick) {
        while let Some(t) = self.next_time() {
            if t > t_end {
                break;
            }
            self.advance_to(t);
        }
        if t_end > self.now {
            self.now = t_end;
        }
    }

    /// Run forward, probing at every quiescent point (the moments just before
    /// the clock advances). Stops early if `probe` returns false.
    pub fn run_probed(&mut self, t_end: Tick, mut probe: impl FnMut(&Pop<'a>) -> bool) -> bool {
        loop {
            let Some(t) = self.next_time() else {
                return probe(self);
            };
            if t > self.now && !probe(self) {
                return false;
            }
            if t > t_end {
                self.now = t_end;
                return true;
            }
            self.advance_to(t);
        }
    }

    fn advance_to(&mut self, t: Tick) {
        self.now = t;
        for c in self.classes.iter_mut() {
            // Everything finishing at exactly t moves to the deposit queue in
            // one step, whether that is one machine or a hundred million.
            let mut moved = 0;
            while let Some(&(dl, n)) = c.working.first() {
                if dl != t {
                    break;
                }
                moved += n;
                c.working.remove(0);
            }
            c.done += moved;
        }
        self.settle();
    }

    /// Arbitration rounds until nothing more can happen at this tick.
    fn settle(&mut self) {
        loop {
            self.rounds += 1;
            let mut progress = false;
            progress |= self.phase(Q_DONE);
            progress |= self.phase(Q_STARVED);
            if !progress {
                return;
            }
        }
    }

    fn rank(&self, store: u16, class: u16, q: usize) -> u32 {
        let sd = &self.bp.storages[store as usize];
        let queue = sd.queue(q == Q_DONE);
        let n = queue.len();
        let pos = queue.iter().position(|&c| c == class).unwrap_or(n) as u32;
        match sd.policy {
            Policy::RoundRobin => {
                let p = self.rr[store as usize * 2 + q] as u32;
                (pos + n as u32 - p) % n.max(1) as u32
            }
            _ => pos,
        }
    }

    fn phase(&mut self, q: usize) -> bool {
        let nc = self.bp.actors.len();

        let mut cand = std::mem::take(&mut self.cand);
        cand.clear();
        for class in 0..nc {
            if self.pending(class, q) == 0 {
                continue;
            }
            let ad = &self.bp.actors[class];
            let primary = if q == Q_DONE { ad.primary_out() } else { ad.primary_in() };
            match primary {
                Some(s) => cand.push((s as u32, self.rank(s, class as u16, q), class as u16)),
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
            progress |= self.serve_group(q, store, k, end, &cand);
            k = end;
        }

        self.cand = cand;
        progress
    }

    #[inline]
    fn pending(&self, class: usize, q: usize) -> u64 {
        if q == Q_DONE {
            self.classes[class].done
        } else {
            self.classes[class].starved
        }
    }

    /// Arbitrate one storage's queue and commit the result.
    ///
    /// This is the closed form of `World::serve_laps` and `World::phase`'s
    /// strict branch: the same allocation, reached without dealing to machines
    /// one at a time. Under `index` each class in turn takes the largest batch
    /// that fits; under `round_robin` the grant is max-min fair, found by
    /// asking how many complete laps the storage can afford rather than
    /// running them.
    fn serve_group(
        &mut self,
        q: usize,
        store: u32,
        lo: usize,
        hi: usize,
        cand: &[(u32, u32, u16)],
    ) -> bool {
        let n = hi - lo;
        let cls: Vec<usize> = (lo..hi).map(|i| cand[i].2 as usize).collect();
        let want: Vec<u64> = cls.iter().map(|&c| self.pending(c, q)).collect();
        let mut grant: Vec<u64> = vec![0; n];

        let round_robin = store != NO_STORE
            && self.bp.storages[store as usize].policy == Policy::RoundRobin;

        if !round_robin {
            // Strict order. Each class takes everything still available to it,
            // on top of what the classes ahead of it have already claimed.
            for i in 0..n {
                grant[i] = self.max_extra(q, &cls, &mut grant, i, want[i]);
            }
        } else {
            let mut dead = vec![false; n];
            loop {
                let active: Vec<usize> =
                    (0..n).filter(|&i| !dead[i] && grant[i] < want[i]).collect();
                if active.is_empty() {
                    break;
                }
                // How many more complete laps can the storage afford? Binary
                // search, because feasibility only ever gets harder as the lap
                // count rises. This is the step that makes a billion machines
                // cost the same as four.
                let cap = active.iter().map(|&i| want[i] - grant[i]).max().unwrap();
                let laps = self.max_laps(q, &cls, &mut grant, &want, &active, cap);
                if laps > 0 {
                    for &i in &active {
                        grant[i] += laps.min(want[i] - grant[i]);
                    }
                    continue;
                }
                // No complete lap left. Deal the remainder one at a time in
                // rotation order; whoever is refused is out for this phase.
                let mut any = false;
                for &i in &active {
                    grant[i] += 1;
                    if self.feasible(q, &cls, &grant) {
                        any = true;
                    } else {
                        grant[i] -= 1;
                        dead[i] = true;
                    }
                }
                if !any {
                    break;
                }
            }
        }

        if grant.iter().all(|&g| g == 0) {
            return false;
        }
        self.grants += 1;
        self.apply(q, &cls, &grant);

        if round_robin {
            if let Some(i) = (0..n).rev().find(|&i| grant[i] > 0) {
                let queue = self.bp.storages[store as usize].queue(q == Q_DONE);
                if !queue.is_empty() {
                    let pos = queue.iter().position(|&x| x == cls[i] as u16).unwrap_or(0);
                    self.rr[store as usize * 2 + q] = ((pos + 1) % queue.len()) as u16;
                }
            }
        }
        true
    }

    /// Largest `k <= cap` that class `slot` may add to an existing plan.
    fn max_extra(
        &self,
        q: usize,
        cls: &[usize],
        grant: &mut Vec<u64>,
        slot: usize,
        cap: u64,
    ) -> u64 {
        let keep = grant[slot];
        let test = |g: &mut Vec<u64>, k: u64, me: &Self| {
            g[slot] = k;
            me.feasible(q, cls, g)
        };
        if cap == 0 || !test(grant, keep + 1, self) {
            grant[slot] = keep;
            return keep;
        }
        let (mut lo, mut hi) = (1u64, cap + 1); // feasible(lo), not feasible(hi)
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if test(grant, keep + mid, self) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        grant[slot] = keep;
        keep + lo
    }

    /// Largest number of additional complete laps the storage can afford.
    fn max_laps(
        &self,
        q: usize,
        cls: &[usize],
        grant: &mut Vec<u64>,
        want: &[u64],
        active: &[usize],
        cap: u64,
    ) -> u64 {
        let base: Vec<u64> = grant.clone();
        let probe = |l: u64, g: &mut Vec<u64>, me: &Self| {
            g.clone_from(&base);
            for &i in active {
                g[i] += l.min(want[i] - base[i]);
            }
            me.feasible(q, cls, g)
        };
        if cap == 0 || !probe(1, grant, self) {
            grant.clone_from(&base);
            return 0;
        }
        let (mut lo, mut hi) = (1u64, cap + 1);
        while hi - lo > 1 {
            let mid = lo + (hi - lo) / 2;
            if probe(mid, grant, self) {
                lo = mid;
            } else {
                hi = mid;
            }
        }
        grant.clone_from(&base);
        lo
    }

    /// Can this whole plan be committed atomically?
    ///
    /// The greedy fill of `sim.rs`, with every quantity multiplied out. It
    /// walks classes in the same order the simulator deals to them, so the two
    /// draw the same amounts from the same storages.
    fn feasible(&self, q: usize, cls: &[usize], grant: &[u64]) -> bool {
        let bp = self.bp;
        if q == Q_STARVED {
            let mut taken: HashMap<u32, u128> = HashMap::new();
            for (idx, &class) in cls.iter().enumerate() {
                let k = grant[idx] as u128;
                if k == 0 {
                    continue;
                }
                for st in &bp.actors[class].inputs {
                    let mut need = st.qty as u128 * k;
                    for &s in &bp.actors[class].in_stores {
                        let Some(slot) = bp.slot_of(s as usize, st.item) else { continue };
                        let avail =
                            self.qty[slot as usize] as u128 - *taken.get(&slot).unwrap_or(&0);
                        if avail == 0 {
                            continue;
                        }
                        let take = need.min(avail);
                        *taken.entry(slot).or_insert(0) += take;
                        need -= take;
                        if need == 0 {
                            break;
                        }
                    }
                    if need > 0 {
                        return false;
                    }
                }
            }
            true
        } else {
            let mut placed: HashMap<u16, u128> = HashMap::new();
            for (idx, &class) in cls.iter().enumerate() {
                let k = grant[idx] as u128;
                if k == 0 {
                    continue;
                }
                for st in &bp.actors[class].outputs {
                    let mut need = st.qty as u128 * k;
                    for &s in &bp.actors[class].out_stores {
                        if bp.slot_of(s as usize, st.item).is_none() {
                            continue;
                        }
                        let room = bp.storages[s as usize].capacity as u128
                            - self.used[s as usize] as u128
                            - *placed.get(&s).unwrap_or(&0);
                        if room == 0 {
                            continue;
                        }
                        let put = need.min(room);
                        *placed.entry(s).or_insert(0) += put;
                        need -= put;
                        if need == 0 {
                            break;
                        }
                    }
                    if need > 0 {
                        return false;
                    }
                }
            }
            true
        }
    }

    /// Commit a plan. Mirrors the greedy fill of `feasible` exactly.
    fn apply(&mut self, q: usize, cls: &[usize], grant: &[u64]) {
        let bp = self.bp;
        for (idx, &class) in cls.iter().enumerate() {
            let k = grant[idx];
            if k == 0 {
                continue;
            }
            let ad = &bp.actors[class];
            if q == Q_STARVED {
                for st in &ad.inputs {
                    let mut need = st.qty * k;
                    for &s in &ad.in_stores {
                        let Some(slot) = bp.slot_of(s as usize, st.item) else { continue };
                        let avail = self.qty[slot as usize];
                        if avail == 0 {
                            continue;
                        }
                        let take = need.min(avail);
                        self.qty[slot as usize] -= take;
                        self.used[s as usize] -= take;
                        need -= take;
                        if need == 0 {
                            break;
                        }
                    }
                    debug_assert_eq!(need, 0, "apply disagreed with feasible");
                    self.c.consumed[st.item as usize] += st.qty * k;
                }
                self.classes[class].starved -= k;
                let dl = self.now + ad.duration;
                let w = &mut self.classes[class].working;
                match w.binary_search_by_key(&dl, |e| e.0) {
                    Ok(i) => w[i].1 += k,
                    Err(i) => w.insert(i, (dl, k)),
                }
            } else {
                for st in &ad.outputs {
                    let mut need = st.qty * k;
                    for &s in &ad.out_stores {
                        let Some(slot) = bp.slot_of(s as usize, st.item) else { continue };
                        let room = bp.storages[s as usize].capacity - self.used[s as usize];
                        if room == 0 {
                            continue;
                        }
                        let put = need.min(room);
                        self.qty[slot as usize] += put;
                        self.used[s as usize] += put;
                        need -= put;
                        if need == 0 {
                            break;
                        }
                    }
                    debug_assert_eq!(need, 0, "apply disagreed with feasible");
                    self.c.produced[st.item as usize] += st.qty * k;
                }
                self.c.cycles[class] += k;
                self.classes[class].done -= k;
                // Straight to the back of the withdraw queue, as in `sim.rs`.
                self.classes[class].starved += k;
            }
        }
    }

    // -------------------------------------------------------- inspection

    pub fn storage_qty(&self, storage: usize, item: ItemId) -> Qty {
        match self.bp.slot_of(storage, item) {
            Some(slot) => self.qty[slot as usize],
            None => 0,
        }
    }

    pub fn storage_used(&self, storage: usize) -> Qty {
        self.used[storage]
    }

    /// Canonical encoding of the population state, with deadlines relative to
    /// now. This is `World::signature` with the machine records already
    /// collapsed -- the same equivalence, reached without ever expanding it.
    pub fn signature(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.qty.len() * 8 + self.classes.len() * 24);
        for q in &self.qty {
            v.extend_from_slice(&q.to_le_bytes());
        }
        for r in &self.rr {
            v.extend_from_slice(&r.to_le_bytes());
        }
        for c in &self.classes {
            v.extend_from_slice(&c.starved.to_le_bytes());
            v.extend_from_slice(&c.done.to_le_bytes());
            for (dl, n) in &c.working {
                v.extend_from_slice(&(dl - self.now).to_le_bytes());
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.push(0xff);
        }
        v
    }

    pub fn clone_state(&self) -> PopState {
        PopState {
            qty: self.qty.clone(),
            used: self.used.clone(),
            classes: self.classes.clone(),
            rr: self.rr.clone(),
            now: self.now,
            c: self.c.clone(),
        }
    }

    pub fn restore(&mut self, s: &PopState) {
        self.qty.clone_from(&s.qty);
        self.used.clone_from(&s.used);
        self.classes.clone_from(&s.classes);
        self.rr.clone_from(&s.rr);
        self.now = s.now;
        self.c.clone_from(&s.c);
    }

    /// Human-readable population of one class, e.g.
    /// `Smelter { working@+7: 2011, starved: 842 }`.
    pub fn describe_class(&self, class: usize) -> String {
        let c = &self.classes[class];
        let mut parts: Vec<String> = Vec::new();
        if c.starved > 0 {
            parts.push(format!("idle/starved: {}", c.starved));
        }
        if c.done > 0 {
            parts.push(format!("blocked: {}", c.done));
        }
        for (dl, n) in &c.working {
            parts.push(format!("working@+{}: {}", dl - self.now, n));
        }
        format!(
            "{} {{ {} }}",
            self.bp.actors[class].name,
            if parts.is_empty() { "-".to_string() } else { parts.join(", ") }
        )
    }
}

#[derive(Clone)]
pub struct PopState {
    qty: Vec<Qty>,
    used: Vec<Qty>,
    classes: Vec<ClassPop>,
    rr: Vec<u16>,
    now: Tick,
    c: Counters,
}

// ============================================================ closed form

/// A periodic orbit found in the *population* state space.
pub struct PopForm {
    pub t0: Tick,
    pub period: Tick,
    pub delta: Counters,
    pub base: Counters,
    pub frozen: bool,
    pub found: bool,
    pub grants: u64,
    pub rounds: u64,
    pub states_visited: usize,
    /// Widest compressed state seen while finding the orbit, against the
    /// machine count it stands for.
    pub max_distinct_states: usize,
    pub population: u64,
    state_t0: Option<PopState>,
}

impl PopForm {
    /// Exact counters for the whole coupled population at tick `t`.
    /// O(period) in work and O(1) in both `t` and the population size.
    pub fn eval(&self, bp: &Blueprint, n_items: usize, t: Tick) -> CountersBig {
        let mut p = Pop::new(bp, n_items);
        if !self.found || t < self.t0 {
            p.run_until(t);
            return CountersBig::from_narrow(&p.c);
        }
        if self.frozen {
            return CountersBig::from_narrow(&self.base);
        }
        let n = ((t - self.t0) / self.period) as u128;
        let r = (t - self.t0) % self.period;
        p.restore(self.state_t0.as_ref().expect("orbit snapshot"));
        p.run_until(self.t0 + r);
        p.c.add_scaled_big(&self.delta, n)
    }

    /// Reconstruct the population state itself at tick `t`.
    pub fn state_at<'b>(&self, bp: &'b Blueprint, n_items: usize, t: Tick) -> (Pop<'b>, u128) {
        let mut p = Pop::new(bp, n_items);
        if !self.found || t < self.t0 {
            p.run_until(t);
            return (p, 0);
        }
        if self.frozen {
            p.restore(self.state_t0.as_ref().expect("frozen snapshot"));
            p.now = t;
            return (p, 0);
        }
        let n = ((t - self.t0) / self.period) as u128;
        let r = (t - self.t0) % self.period;
        p.restore(self.state_t0.as_ref().expect("orbit snapshot"));
        p.run_until(self.t0 + r);
        (p, n)
    }

    pub fn describe(&self) -> String {
        if !self.found {
            "no population orbit found within budget".to_string()
        } else if self.frozen {
            format!("frozen at t={} (no machine will ever finish again)", self.t0)
        } else {
            format!("population orbit of period {} ticks entered at t={}", self.period, self.t0)
        }
    }
}

/// Detect the periodic orbit of the coupled population.
///
/// Identical in spirit to `analytic::orbit`, but the state being hashed is the
/// histogram rather than the machine list. Two consequences, both good:
/// the signature is O(classes x cycle time) instead of O(N), and states that
/// differ only by which machine is where collapse to one -- so a repeat is
/// found sooner, or found at all.
pub fn orbit(bp: &Blueprint, n_items: usize, budget_rounds: u64) -> PopForm {
    let mut p = Pop::new(bp, n_items);
    let mut seen: HashMap<Vec<u8>, (Tick, Counters)> = HashMap::new();
    let mut hit: Option<(Tick, Counters, Tick, Counters)> = None;
    let mut overrun = false;
    let mut max_distinct = 0usize;
    let population = p.population();

    p.run_probed(Tick::MAX, |p| {
        max_distinct = max_distinct.max(p.distinct_states());
        if p.frozen() {
            return false;
        }
        let sig = p.signature();
        if let Some((t_prev, c_prev)) = seen.get(&sig) {
            hit = Some((*t_prev, c_prev.clone(), p.now, p.c.clone()));
            return false;
        }
        if p.rounds > budget_rounds {
            overrun = true;
            return false;
        }
        seen.insert(sig, (p.now, p.c.clone()));
        true
    });

    let states_visited = seen.len();
    let (grants, rounds) = (p.grants, p.rounds);
    let common = |found, frozen, t0, period, base, delta, st| PopForm {
        t0,
        period,
        delta,
        base,
        frozen,
        found,
        grants,
        rounds,
        states_visited,
        max_distinct_states: max_distinct,
        population,
        state_t0: st,
    };

    if p.frozen() {
        return common(
            true,
            true,
            p.now,
            0,
            p.c.clone(),
            Counters::zeroed(bp.actors.len(), n_items),
            Some(p.clone_state()),
        );
    }

    match hit {
        Some((t_prev, c_prev, t_now, c_now)) if !overrun => {
            let mut p0 = Pop::new(bp, n_items);
            p0.run_until(t_prev);
            debug_assert_eq!(p0.c, c_prev);
            let delta = c_now.sub(&c_prev);
            common(true, false, t_prev, t_now - t_prev, c_prev, delta, Some(p0.clone_state()))
        }
        _ => {
            let z = Counters::zeroed(bp.actors.len(), n_items);
            common(false, false, 0, 0, z.clone(), z, None)
        }
    }
}

/// Exact totals for a whole deployment of *coupled* plants at tick `t`.
///
/// The instances of a deployment still do not touch each other -- the coupling
/// v2 introduced lives inside one plant -- so T4's phase-archetype argument
/// applies unchanged on top of T5. The two compressions multiply: archetypes
/// collapse the copies, populations collapse the machines inside a copy.
pub fn deployment_totals(
    bp: &Blueprint,
    n_items: usize,
    pf: &PopForm,
    d: &Deploy,
    t: Tick,
) -> (CountersBig, usize) {
    let arch = crate::analytic::archetypes(bp, d.count, d.stagger);
    let mut total = CountersBig::zeroed(bp.actors.len(), n_items);

    let mut transient: Vec<(Tick, u64)> = Vec::new();
    for a in &arch {
        if t < a.offset {
            continue; // this line has not started yet
        }
        let shifted = t - a.offset;
        if pf.found && shifted >= pf.t0 {
            total.add(&pf.eval(bp, n_items, shifted).scale_u128(a.count as u128));
        } else {
            transient.push((shifted, a.count));
        }
    }

    if !transient.is_empty() {
        transient.sort_unstable_by_key(|&(ts, _)| ts);
        let mut p = Pop::new(bp, n_items);
        for (ts, count) in transient {
            p.run_until(ts);
            total.add(&CountersBig::from_narrow(&p.c).scale_u128(count as u128));
        }
    }
    (total, arch.len())
}
