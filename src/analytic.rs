//! Closed-form analysis: the part that answers "what is the state at tick t"
//! without visiting tick t.
//!
//! Two independent methods, deliberately kept separate so they can check
//! each other:
//!
//! * `orbit` (tier T2) -- **exact**. A plant with finite storages has a finite
//!   dynamical state space, so its trajectory is eventually periodic. Simulate
//!   the transient once, detect the repeat, and then
//!   `state(t) = base + floor((t - t0)/P) * delta + replay(remainder)`.
//!   Cost is independent of t: answering for t = 10^18 costs the same as t = 10^3.
//!
//! * `rates` (tier T3) -- **asymptotic**. A fluid relaxation: treat cycles as
//!   continuous rates and push starvation up / backpressure down to a fixpoint.
//!   Never simulates anything, costs O(nodes), and tells you the bottleneck and
//!   whether the plant is terminal. It cannot see latency or integrality, which
//!   is exactly why we validate it against the orbit.

use crate::model::*;
use crate::sim::{Counters, CountersBig, Snapshot, World};
use std::collections::HashMap;
use std::collections::HashSet;

// ===================================================================== T2

pub struct ClosedForm {
    pub blueprint: String,
    /// First tick of the repeating orbit.
    pub t0: Tick,
    /// Orbit length in ticks. Zero means the plant froze (no events remain).
    pub period: Tick,
    /// Counter increase over one orbit.
    pub delta: Counters,
    /// Counters at `t0`.
    pub base: Counters,
    pub frozen: bool,
    pub found: bool,
    /// Events consumed to reach and confirm the orbit.
    pub transient_events: u64,
    /// Distinct quiescent states visited during detection.
    pub states_visited: usize,
    state_t0: Option<Snapshot>,
}

impl ClosedForm {
    /// Reconstruct the exact *dynamical* state at tick `t` -- storage contents
    /// and machine states, which are bounded and therefore exactly periodic --
    /// together with the number of whole orbits already elapsed.
    ///
    /// Cost is O(period) regardless of how large `t` is.
    pub fn world_at<'b>(&self, bp: &'b Blueprint, n_items: usize, t: Tick) -> (World<'b>, u128) {
        let mut w = World::new(bp, n_items, 1, 0);
        if !self.found || t < self.t0 {
            w.run_until(t);
            return (w, 0);
        }
        if self.frozen {
            w.restore(self.state_t0.as_ref().expect("frozen snapshot"));
            w.now = t;
            return (w, 0);
        }
        let n = ((t - self.t0) / self.period) as u128;
        let r = (t - self.t0) % self.period;
        w.restore(self.state_t0.as_ref().expect("orbit snapshot"));
        w.run_until(self.t0 + r);
        (w, n)
    }

    /// Exact counters for one instance at absolute tick `t`. O(period), and
    /// crucially O(1) in `t`.
    pub fn eval(&self, bp: &Blueprint, n_items: usize, t: Tick) -> CountersBig {
        if self.found && self.frozen && t >= self.t0 {
            return CountersBig::from_narrow(&self.base);
        }
        let (w, n) = self.world_at(bp, n_items, t);
        w.c.add_scaled_big(&self.delta, n)
    }

    /// Instance shifted by `offset` ticks: its whole trajectory is the base
    /// trajectory translated in time.
    pub fn eval_shifted(&self, bp: &Blueprint, n_items: usize, t: Tick, offset: Tick) -> CountersBig {
        if t < offset {
            return CountersBig::zeroed(bp.actors.len(), n_items);
        }
        self.eval(bp, n_items, t - offset)
    }

    /// Fraction of wall time one machine of a class spends occupied, exact.
    /// For a transport that includes the trip home: a vehicle halfway back to
    /// the mine is not available to load.
    pub fn steady_duty(&self, bp: &Blueprint, actor: usize) -> Rat {
        let ad = &bp.actors[actor];
        self.steady_cycles_per_tick(actor)
            .mul(Rat::new(ad.cycle() as u128, 1))
            .div(Rat::new(ad.count as u128, 1))
    }

    pub fn steady_cycles_per_tick(&self, actor: usize) -> Rat {
        if self.period == 0 {
            Rat::zero()
        } else {
            Rat::new(self.delta.cycles[actor] as u128, self.period as u128)
        }
    }

    pub fn steady_output_per_tick(&self, item: usize) -> Rat {
        if self.period == 0 {
            Rat::zero()
        } else {
            Rat::new(self.delta.produced[item] as u128, self.period as u128)
        }
    }

    pub fn describe(&self) -> String {
        if !self.found {
            "no orbit found within budget".to_string()
        } else if self.frozen {
            format!("frozen at t={} (no events remain; state constant forever)", self.t0)
        } else {
            format!("orbit of period {} ticks entered at t={}", self.period, self.t0)
        }
    }
}

/// Detect the periodic orbit of a single instance.
///
/// Every time the clock is about to advance we canonicalise the full state
/// (storage contents plus machine states with *relative* deadlines). A repeat
/// is a mathematical proof of periodicity, not a heuristic.
pub fn orbit(bp: &Blueprint, n_items: usize, budget_events: u64) -> ClosedForm {
    let mut w = World::new(bp, n_items, 1, 0);
    let mut seen: HashMap<Vec<u8>, (Tick, Counters)> = HashMap::new();
    let mut hit: Option<(Tick, Counters, Tick, Counters)> = None;
    let mut overrun = false;

    w.run_probed(Tick::MAX, |w| {
        if w.frozen() {
            return false;
        }
        let sig = w.signature();
        if let Some((t_prev, c_prev)) = seen.get(&sig) {
            hit = Some((*t_prev, c_prev.clone(), w.now, w.c.clone()));
            return false;
        }
        if w.events > budget_events {
            overrun = true;
            return false;
        }
        seen.insert(sig, (w.now, w.c.clone()));
        true
    });

    let states_visited = seen.len();
    let transient_events = w.events;

    if w.frozen() {
        let n_actors = bp.actors.len();
        return ClosedForm {
            blueprint: bp.name.clone(),
            t0: w.now,
            period: 0,
            delta: Counters::zeroed(n_actors, n_items),
            base: w.c.clone(),
            frozen: true,
            found: true,
            transient_events,
            states_visited,
            state_t0: Some(w.snapshot()),
        };
    }

    match hit {
        Some((t_prev, c_prev, t_now, c_now)) if !overrun => {
            // Rebuild the exact state at t_prev so remainders can be replayed.
            let mut w0 = World::new(bp, n_items, 1, 0);
            w0.run_until(t_prev);
            debug_assert_eq!(w0.c, c_prev);
            ClosedForm {
                blueprint: bp.name.clone(),
                t0: t_prev,
                period: t_now - t_prev,
                delta: c_now.sub(&c_prev),
                base: c_prev,
                frozen: false,
                found: true,
                transient_events,
                states_visited,
                state_t0: Some(w0.snapshot()),
            }
        }
        _ => ClosedForm {
            blueprint: bp.name.clone(),
            t0: 0,
            period: 0,
            delta: Counters::zeroed(bp.actors.len(), n_items),
            base: Counters::zeroed(bp.actors.len(), n_items),
            frozen: false,
            found: false,
            transient_events,
            states_visited,
            state_t0: None,
        },
    }
}

// ================================================================ rationals

/// Exact non-negative rational. Enough for rate algebra without float drift.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Rat {
    pub n: u128,
    pub d: u128,
}

impl Rat {
    pub fn new(n: u128, d: u128) -> Rat {
        assert!(d != 0, "rational with zero denominator");
        if n == 0 {
            return Rat { n: 0, d: 1 };
        }
        let g = gcd128(n, d);
        Rat { n: n / g, d: d / g }
    }
    pub fn zero() -> Rat {
        Rat { n: 0, d: 1 }
    }
    pub fn is_zero(self) -> bool {
        self.n == 0
    }
    pub fn add(self, o: Rat) -> Rat {
        Rat::new(self.n * o.d + o.n * self.d, self.d * o.d)
    }
    pub fn mul(self, o: Rat) -> Rat {
        Rat::new(self.n * o.n, self.d * o.d)
    }
    pub fn div(self, o: Rat) -> Rat {
        assert!(!o.is_zero(), "division by zero rate");
        Rat::new(self.n * o.d, self.d * o.n)
    }
    pub fn min(self, o: Rat) -> Rat {
        if self.le(o) {
            self
        } else {
            o
        }
    }
    pub fn le(self, o: Rat) -> bool {
        self.n * o.d <= o.n * self.d
    }
    pub fn lt(self, o: Rat) -> bool {
        self.n * o.d < o.n * self.d
    }
    pub fn to_f64(self) -> f64 {
        self.n as f64 / self.d as f64
    }
    pub fn show(self) -> String {
        if self.d == 1 {
            format!("{}", self.n)
        } else {
            format!("{:.4} ({}/{})", self.to_f64(), self.n, self.d)
        }
    }
}

fn gcd128(a: u128, b: u128) -> u128 {
    if b == 0 {
        a
    } else {
        gcd128(b, a % b)
    }
}

// ===================================================================== T3

pub struct RateReport {
    /// Steady-state cycles per tick for each actor.
    pub cycles: Vec<Rat>,
    /// Fraction of wall time each machine spends working.
    pub duty: Vec<Rat>,
    pub produced_per_tick: Vec<Rat>,
    pub consumed_per_tick: Vec<Rat>,
    pub iterations: u32,
    pub converged: bool,
    /// Items that are produced but never consumed: with finite storage these
    /// eventually saturate and stall everything upstream.
    pub accumulators: Vec<ItemId>,
    /// Items no machine can ever make, because making them needs them.
    /// A production *cycle* with nothing seeded into it is the classic case:
    /// perfectly well-formed on paper, permanently dead in practice.
    pub unattainable: Vec<ItemId>,
    pub terminal: bool,
    /// Actors still running at their unconstrained maximum -- the bottlenecks.
    pub bottlenecks: Vec<usize>,
}

/// Fluid fixpoint. No time stepping at all: O(nodes x iterations).
///
/// Flows are balanced **per storage**, not per item. v1 could get away with
/// per-item balance because every item had one producer stage and one consumer
/// stage. A transport breaks that immediately: it consumes IronOre and produces
/// IronOre, so an item-global balance cannot tell ore sitting at the mine from
/// ore sitting in the yard, concludes that ore both feeds and starves itself,
/// and converges to a fixpoint of nonsense. Balancing at each storage separately
/// asks the only question that was ever meaningful: is this *bay* filling faster
/// than it drains.
pub fn rates(bp: &Blueprint, n_items: usize) -> RateReport {
    let na = bp.actors.len();
    let ns = bp.storages.len();
    // A class of N machines has N times the capacity of one. This is the only
    // place the population size enters the rate algebra at all.
    let cap: Vec<Rat> =
        bp.actors.iter().map(|a| Rat::new(a.count as u128, a.cycle() as u128)).collect();

    // Which bay each machine actually draws each ingredient from, and drops
    // each product into: the first wired storage that can hold it, matching the
    // greedy fill in `sim.rs`.
    let pick = |stores: &[u16], item: ItemId| -> Option<usize> {
        stores
            .iter()
            .map(|&s| s as usize)
            .find(|&s| bp.slot_of(s, item).is_some())
    };

    // Attainability. An item exists if something seeded it, or if some machine
    // can make it from items that themselves exist. Least fixpoint, so a loop
    // that feeds only itself never qualifies -- which is exactly the verdict a
    // catalyst cycle with an empty buffer deserves.
    let mut have: HashSet<ItemId> = HashSet::new();
    for sd in &bp.storages {
        for st in &sd.initial {
            have.insert(st.item);
        }
    }
    loop {
        let before = have.len();
        for a in &bp.actors {
            if a.inputs.iter().all(|s| have.contains(&s.item)) {
                for s in &a.outputs {
                    have.insert(s.item);
                }
            }
        }
        if have.len() == before {
            break;
        }
    }
    let unattainable: Vec<ItemId> = (0..n_items as ItemId).filter(|i| !have.contains(i)).collect();

    let mut cyc: Vec<Rat> = (0..na)
        .map(|a| {
            if bp.actors[a].inputs.iter().any(|s| !have.contains(&s.item)) {
                Rat::zero()
            } else {
                cap[a]
            }
        })
        .collect();

    let idx = |s: usize, i: usize| s * n_items + i;
    let mut iterations = 0;
    let mut converged = false;
    for _ in 0..256 {
        iterations += 1;
        let (prod, cons) = flows(bp, &cyc, n_items);
        let mut next = cyc.clone();
        for a in 0..na {
            let ad = &bp.actors[a];
            let mut lim = cap[a];
            // Starvation: everyone drawing from a bay that is filling too
            // slowly scales back in proportion.
            for st in &ad.inputs {
                let Some(s) = pick(&ad.in_stores, st.item) else { continue };
                let (p, c) = (prod[idx(s, st.item as usize)], cons[idx(s, st.item as usize)]);
                if c.is_zero() {
                    continue;
                }
                if p.lt(c) {
                    lim = lim.min(cyc[a].mul(p).div(c));
                }
            }
            // Backpressure: a bay that cannot drain throttles whoever fills it.
            for st in &ad.outputs {
                let Some(s) = pick(&ad.out_stores, st.item) else { continue };
                let (p, c) = (prod[idx(s, st.item as usize)], cons[idx(s, st.item as usize)]);
                if p.is_zero() {
                    continue;
                }
                if c.lt(p) {
                    lim = if c.is_zero() { Rat::zero() } else { lim.min(cyc[a].mul(c).div(p)) };
                }
            }
            next[a] = lim.min(cyc[a]);
        }
        if next == cyc {
            converged = true;
            break;
        }
        cyc = next;
    }

    let (prod, cons) = flows(bp, &cyc, n_items);
    // Duty is per machine, so divide the class rate by the class population.
    let duty: Vec<Rat> = (0..na)
        .map(|a| {
            cyc[a]
                .mul(Rat::new(bp.actors[a].cycle() as u128, 1))
                .div(Rat::new(bp.actors[a].count as u128, 1))
        })
        .collect();

    // A bay that is filled with something nobody takes out of *it* saturates,
    // however busily that item is handled elsewhere in the plant.
    let mut accumulators = Vec::new();
    for s in 0..ns {
        for i in 0..n_items {
            if !prod[idx(s, i)].is_zero() && cons[idx(s, i)].is_zero() {
                if !accumulators.contains(&(i as ItemId)) {
                    accumulators.push(i as ItemId);
                }
            }
        }
    }

    let mut produced_per_tick = vec![Rat::zero(); n_items];
    let mut consumed_per_tick = vec![Rat::zero(); n_items];
    for i in 0..n_items {
        for s in 0..ns {
            produced_per_tick[i] = produced_per_tick[i].add(prod[idx(s, i)]);
            consumed_per_tick[i] = consumed_per_tick[i].add(cons[idx(s, i)]);
        }
    }

    let terminal = cyc.iter().all(|r| r.is_zero());
    let bottlenecks: Vec<usize> = (0..na).filter(|&a| !terminal && cyc[a] == cap[a]).collect();

    RateReport {
        cycles: cyc,
        duty,
        produced_per_tick,
        consumed_per_tick,
        iterations,
        converged,
        accumulators,
        unattainable,
        terminal,
        bottlenecks,
    }
}

/// Deposit and withdrawal rates for every (storage, item) pair.
fn flows(bp: &Blueprint, cyc: &[Rat], n_items: usize) -> (Vec<Rat>, Vec<Rat>) {
    let n = bp.storages.len() * n_items;
    let mut prod = vec![Rat::zero(); n];
    let mut cons = vec![Rat::zero(); n];
    for (a, ad) in bp.actors.iter().enumerate() {
        for st in &ad.outputs {
            if let Some(s) = ad
                .out_stores
                .iter()
                .map(|&s| s as usize)
                .find(|&s| bp.slot_of(s, st.item).is_some())
            {
                let k = s * n_items + st.item as usize;
                prod[k] = prod[k].add(cyc[a].mul(Rat::new(st.qty as u128, 1)));
            }
        }
        for st in &ad.inputs {
            if let Some(s) = ad
                .in_stores
                .iter()
                .map(|&s| s as usize)
                .find(|&s| bp.slot_of(s, st.item).is_some())
            {
                let k = s * n_items + st.item as usize;
                cons[k] = cons[k].add(cyc[a].mul(Rat::new(st.qty as u128, 1)));
            }
        }
    }
    (prod, cons)
}
// ===================================================================== T4

/// One phase archetype of a deployment: `count` instances that all start at
/// `offset`, and therefore all follow the same trajectory shifted by `offset`.
#[derive(Clone, Copy, Debug)]
pub struct Archetype {
    pub offset: Tick,
    pub count: u64,
}

/// Collapse `count` staggered instances into distinct phase archetypes.
///
/// Instance k starts at `(k * stagger) mod P`. That sequence is periodic in k
/// with length `L = P / gcd(stagger, P)`, so a deployment of any size -- ten or
/// ten billion -- has at most `L <= P` archetypes. This is the entire scaling
/// argument: analysis cost depends on the blueprint, never on the object count.
pub fn archetypes(bp: &Blueprint, count: u64, stagger: u64) -> Vec<Archetype> {
    if count == 0 {
        return Vec::new();
    }
    let p = bp.base_period.max(1);
    if stagger == 0 {
        return vec![Archetype { offset: 0, count }];
    }
    let l = p / gcd(stagger % p, p).max(1);
    let l = l.max(1).min(count);
    let mut out = Vec::with_capacity(l as usize);
    for j in 0..l {
        let offset = (j as u128 * stagger as u128 % p as u128) as u64;
        let mult = count / l + if j < count % l { 1 } else { 0 };
        if mult > 0 {
            out.push(Archetype { offset, count: mult });
        }
    }
    out
}

/// Exact totals for an entire deployment at tick `t`, from one orbit solve.
///
/// Archetypes whose shifted time still lies in the transient cannot use the
/// orbit, so they are answered by a *single* forward pass that stops at each
/// required time in ascending order -- not one replay per archetype.
pub fn deployment_totals(
    bp: &Blueprint,
    n_items: usize,
    cf: &ClosedForm,
    d: &Deploy,
    t: Tick,
) -> (CountersBig, usize) {
    let arch = archetypes(bp, d.count, d.stagger);
    let mut total = CountersBig::zeroed(bp.actors.len(), n_items);

    let mut transient: Vec<(Tick, u64)> = Vec::new();
    for a in &arch {
        if t < a.offset {
            continue; // this line has not started yet
        }
        let shifted = t - a.offset;
        if cf.found && shifted >= cf.t0 {
            total.add(&cf.eval(bp, n_items, shifted).scale_u128(a.count as u128));
        } else {
            transient.push((shifted, a.count));
        }
    }

    if !transient.is_empty() {
        transient.sort_unstable_by_key(|&(ts, _)| ts);
        let mut w = World::new(bp, n_items, 1, 0);
        for (ts, count) in transient {
            w.run_until(ts);
            total.add(&CountersBig::from_narrow(&w.c).scale_u128(count as u128));
        }
    }
    (total, arch.len())
}
