//! A factory sector: a closed form while nobody is fighting in it, a population
//! while somebody is.
//!
//! A sector is an ordinary plant -- source, bays, machines, a sink -- written in
//! the language every earlier experiment compiles, and solved by the T5
//! population engine. What this module adds is the two ways of *holding* one:
//!
//! ```text
//!   Closed   an orbit: a start state, a transient, a period and a delta.
//!            The state at any tick is one period of evaluation away, and
//!            nothing is stepped between two questions.
//!   Awake    a population state at a tick, stepped forward on the fight's
//!            clock, because the fight can change it at any event.
//! ```
//!
//! and the three transitions between them:
//!
//! ```text
//!   wake    Closed -> Awake    the domain opened around it
//!   edit    Awake  -> Awake    a structure it depends on changed band
//!   close   Awake  -> Closed   the domain settled; find the orbit again
//! ```
//!
//! # The orbit has to be found from wherever the fight left it
//!
//! `pop::orbit` answers for a plant that started empty at tick 0, and a sector
//! the fight has just torn a conveyor out of did not. So `Settled` is the same
//! search -- hash the population state at every quiescent point until one
//! repeats -- started from a *given* state, and its answer is shifted in time
//! rather than replayed: the state at `t0 + n x period + r` is the state at
//! `t0 + r` with every deadline moved `n x period` later and every counter
//! `n x delta` larger.
//!
//! # An edit is a rendezvous, and it is local
//!
//! Prototype 1's edit: harvest the state, keyed by name; change the document;
//! compile it; pour the state back. The difference here is scope. Prototype 1
//! brought *every* region of a plant to the edit's tick, and limitation 11 of
//! the README says so. A sector is its own plant, so a torn-up conveyor in
//! Smelting recompiles Smelting and nothing else is asked to stop.

use super::field::SectorSpec;
use crate::dsl;
use crate::graph::Graph;
use crate::json::Json;
use crate::live::{Carry, Edit, Scrap};
use crate::model::*;
use crate::pop::{Pop, PopState, Port};
use crate::sim::Counters;
use std::collections::HashMap;

/// How long an orbit search may run before a sector is left awake instead.
const BUDGET_ROUNDS: u64 = 2_000_000;

/// A population orbit found from an arbitrary state.
#[derive(Clone)]
pub struct Settled {
    pub t_start: Tick,
    start: PopState,
    pub t0: Tick,
    /// 0 when frozen: nothing in the sector will ever move again.
    pub period: Tick,
    at_t0: PopState,
    delta: Counters,
    pub frozen: bool,
    pub states_visited: usize,
}

impl Settled {
    /// Search for the orbit of the population `start` holds at `t_start`.
    pub fn find(bp: &Blueprint, n_items: usize, start: &PopState) -> Option<Settled> {
        let mut p = Pop::new(bp, n_items);
        p.restore(start);
        let t_start = p.now;
        let rounds0 = p.rounds;
        let mut seen: HashMap<Vec<u8>, Tick> = HashMap::new();
        let mut hit: Option<(Tick, Tick)> = None;
        let mut overrun = false;
        p.run_probed(Tick::MAX, |p| {
            if p.frozen() {
                return false;
            }
            let sig = p.signature();
            if let Some(&tp) = seen.get(&sig) {
                hit = Some((tp, p.now));
                return false;
            }
            if p.rounds - rounds0 > BUDGET_ROUNDS {
                overrun = true;
                return false;
            }
            seen.insert(sig, p.now);
            true
        });
        let states_visited = seen.len();
        if p.frozen() {
            return Some(Settled {
                t_start,
                start: start.clone(),
                t0: p.now,
                period: 0,
                at_t0: p.clone_state(),
                delta: Counters::zeroed(bp.actors.len(), n_items),
                frozen: true,
                states_visited,
            });
        }
        let (tp, tn) = match hit {
            Some(h) if !overrun => h,
            _ => return None,
        };
        let c_now = p.c.clone();
        let mut q = Pop::new(bp, n_items);
        q.restore(start);
        q.run_until(tp);
        let delta = c_now.sub(&q.c);
        Some(Settled {
            t_start,
            start: start.clone(),
            t0: tp,
            period: tn - tp,
            at_t0: q.clone_state(),
            delta,
            frozen: false,
            states_visited,
        })
    }

    /// The population at tick `t`, which must not be before `t_start`. Costs at
    /// most one period of stepping, whatever `t` is.
    pub fn at<'b>(&self, bp: &'b Blueprint, n_items: usize, t: Tick) -> Pop<'b> {
        let mut p = Pop::new(bp, n_items);
        if t < self.t0 {
            p.restore(&self.start);
            p.run_until(t);
            return p;
        }
        p.restore(&self.at_t0);
        if self.frozen {
            p.now = t;
            return p;
        }
        let n = (t - self.t0) / self.period;
        let r = (t - self.t0) % self.period;
        p.run_until(self.t0 + r);
        if n > 0 {
            let shift = n * self.period;
            for c in &mut p.classes {
                for w in &mut c.working {
                    w.0 += shift;
                }
                for w in &mut c.returning {
                    w.0 += shift;
                }
            }
            p.now += shift;
            for (a, d) in p.c.cycles.iter_mut().zip(&self.delta.cycles) {
                *a += d * n;
            }
            for (a, d) in p.c.produced.iter_mut().zip(&self.delta.produced) {
                *a += d * n;
            }
            for (a, d) in p.c.consumed.iter_mut().zip(&self.delta.consumed) {
                *a += d * n;
            }
        }
        p
    }

    /// Units of `item` made per period, and the period: the sector's output as
    /// an exact rate, read off the orbit rather than measured.
    pub fn rate(&self, item: usize) -> (u64, Tick) {
        (self.delta.produced.get(item).copied().unwrap_or(0), self.period)
    }
}

pub enum Mode {
    Closed(Settled),
    Awake { state: PopState, at: Tick },
}

/// One part of the factory.
pub struct Sector {
    pub spec: &'static SectorSpec,
    /// The plant as built. A structure that is repaired goes back to this.
    pub base: Graph,
    /// The plant as it stands now.
    pub graph: Graph,
    prog: Program,
    pub mode: Mode,
    /// Times the domain opened around it.
    pub woke: u32,
    /// Times its topology changed, and it was recompiled.
    pub recompiles: u32,
    /// Times it collapsed back into an orbit.
    pub closes: u32,
    /// Population rounds stepped while awake: the cost of being awake.
    pub stepped: u64,
    /// Closed-form evaluations: the cost of being looked at.
    pub evals: std::cell::Cell<u64>,
    pub edits: Vec<(Tick, String)>,
    pub scrap: Vec<(Tick, Scrap)>,
}

fn compile(g: &Graph) -> Result<Program, String> {
    let src = g.emit();
    let prog = dsl::parse(&src).map_err(|e| format!("{e}"))?;
    if prog.deploys.is_empty() {
        return Err("the sector is never deployed".into());
    }
    Ok(prog)
}

impl Sector {
    pub fn open(spec: &'static SectorSpec) -> Sector {
        let prog0 = dsl::parse(spec.src).unwrap_or_else(|e| panic!("sector {}: {e}", spec.name));
        let graph = Graph::from_program(&prog0);
        let prog = compile(&graph).unwrap_or_else(|e| panic!("sector {}: {e}", spec.name));
        let bp = &prog.blueprints[prog.deploys[0].blueprint as usize];
        let n = prog.items.len();
        let start = Pop::new(bp, n).clone_state();
        let mode = match Settled::find(bp, n, &start) {
            Some(s) => Mode::Closed(s),
            None => Mode::Awake { state: start, at: 0 },
        };
        Sector {
            spec,
            base: graph.clone(),
            graph,
            prog,
            mode,
            woke: 0,
            recompiles: 0,
            closes: 0,
            stepped: 0,
            evals: std::cell::Cell::new(0),
            edits: Vec::new(),
            scrap: Vec::new(),
        }
    }

    pub fn bp(&self) -> &Blueprint {
        &self.prog.blueprints[self.prog.deploys[0].blueprint as usize]
    }

    pub fn n_items(&self) -> usize {
        self.prog.items.len()
    }

    pub fn awake(&self) -> bool {
        matches!(self.mode, Mode::Awake { .. })
    }

    /// Machines in the sector, however few numbers that takes.
    pub fn machines(&self) -> u64 {
        self.bp().actors.iter().map(|a| a.count).sum()
    }

    /// Look at the sector at tick `t`. Does not change what it is.
    pub fn with_pop<R>(&self, t: Tick, f: impl FnOnce(&Pop) -> R) -> R {
        let (bp, n) = (self.bp(), self.n_items());
        match &self.mode {
            Mode::Closed(s) => {
                self.evals.set(self.evals.get() + 1);
                f(&s.at(bp, n, t))
            }
            Mode::Awake { state, at } => {
                let mut p = Pop::new(bp, n);
                p.restore(state);
                if t > *at {
                    p.run_until(t);
                }
                f(&p)
            }
        }
    }

    /// Bring an awake sector's clock to `t`. A closed one has no clock.
    pub fn advance(&mut self, t: Tick) {
        let Mode::Awake { state, at } = &self.mode else { return };
        if t <= *at {
            return;
        }
        let (bp, n) = (self.bp(), self.n_items());
        let mut p = Pop::new(bp, n);
        p.restore(state);
        let r0 = p.rounds;
        p.run_until(t);
        let rounds = p.rounds - r0;
        let st = p.clone_state();
        self.stepped += rounds;
        self.mode = Mode::Awake { state: st, at: t };
    }

    /// Leave the orbit: the domain has opened around this sector.
    pub fn wake(&mut self, t: Tick) {
        if let Mode::Closed(s) = &self.mode {
            let st = s.at(self.bp(), self.n_items(), t).clone_state();
            self.evals.set(self.evals.get() + 1);
            self.woke += 1;
            self.mode = Mode::Awake { state: st, at: t };
        }
    }

    /// Collapse back into an orbit, from wherever the fight left it. Returns
    /// false if no orbit was found within budget and the sector stays awake.
    pub fn close(&mut self, t: Tick) -> bool {
        self.advance(t);
        let Mode::Awake { state, .. } = &self.mode else { return true };
        match Settled::find(self.bp(), self.n_items(), state) {
            Some(s) => {
                self.mode = Mode::Closed(s);
                self.closes += 1;
                true
            }
            None => false,
        }
    }

    /// The sector's state at `t`, addressed by name.
    pub fn carry(&self, t: Tick) -> Carry {
        self.with_pop(t, |p| Carry::from_seed(&p.harvest(), &self.prog, self.bp(), t))
    }

    /// Change the plant at tick `t`: a rendezvous for this sector alone.
    ///
    /// Harvest by name, apply the edits to the document, compile, pour back.
    /// An awake sector stays awake; a closed one -- a repair ordered after the
    /// fight -- is recompiled and immediately settled into its new orbit.
    pub fn edit(&mut self, t: Tick, edits: &[Edit]) -> Result<(), String> {
        if edits.is_empty() {
            return Ok(());
        }
        let was_awake = self.awake();
        self.advance(t);
        let carry = self.carry(t);
        let mut g = self.graph.clone();
        for e in edits {
            e.apply(&mut g)?;
        }
        // Declaration order is class order, and class order is arbitration
        // order. A conveyor put back at the end of the document would be the
        // same plant on paper and a different one at every contended bay, so
        // whatever is in the as-built plant keeps its as-built place.
        let rank = |name: &str| self.base.nodes.iter().position(|n| n.name == name).unwrap_or(usize::MAX);
        g.nodes.sort_by_key(|n| rank(&n.name));
        let erank = |e: &crate::graph::Edge| {
            self.base.edges.iter().position(|b| b.from == e.from && b.to == e.to).unwrap_or(usize::MAX)
        };
        g.edges.sort_by_key(erank);
        let prog = compile(&g)?;
        let bp = &prog.blueprints[prog.deploys[0].blueprint as usize];
        let n = prog.items.len();
        let (seed, lost) = carry.seed(&prog, bp);
        let state = Pop::resume(bp, n, vec![Port::Whole; bp.actors.len()], t, seed).clone_state();
        let settled = if was_awake { None } else { Settled::find(bp, n, &state) };
        for e in edits {
            self.edits.push((t, format!("{} {}", e.verb(), e.subject())));
        }
        for s in lost {
            self.scrap.push((t, s));
        }
        self.graph = g;
        self.prog = prog;
        self.recompiles += 1;
        self.mode = match settled {
            Some(s) => {
                self.closes += 1;
                Mode::Closed(s)
            }
            None => Mode::Awake { state, at: t },
        };
        Ok(())
    }

    /// A sector as a checkpoint left it: its document and its state by name.
    pub fn restore(
        spec: &'static SectorSpec,
        graph: Graph,
        carry: &Carry,
        awake: bool,
        t: Tick,
    ) -> Result<Sector, String> {
        let mut s = Sector::open(spec);
        let prog = compile(&graph)?;
        let bp = &prog.blueprints[prog.deploys[0].blueprint as usize];
        let n = prog.items.len();
        let (seed, _) = carry.seed(&prog, bp);
        let state = Pop::resume(bp, n, vec![Port::Whole; bp.actors.len()], t, seed).clone_state();
        let settled = if awake { None } else { Settled::find(bp, n, &state) };
        s.mode = match settled {
            Some(st) => Mode::Closed(st),
            None => Mode::Awake { state, at: t },
        };
        s.graph = graph;
        s.prog = prog;
        Ok(s)
    }

    /// The nodes that differ from the plant as built.
    pub fn changed(&self) -> Vec<String> {
        let mut out = Vec::new();
        for b in &self.base.nodes {
            match self.graph.node(&b.name) {
                None => out.push(format!("{} gone", b.name)),
                Some(n) if n.count != b.count => {
                    out.push(format!("{} x{} of {}", b.name, n.count, b.count))
                }
                _ => {}
            }
        }
        out
    }

    /// What a view shows about the sector at tick `t`.
    pub fn to_json(&self, t: Tick) -> Json {
        let item = self.prog.items.iter().position(|i| i == self.spec.product);
        let (states, working, starved, made) = self.with_pop(t, |p| {
            let working: u64 = p.classes.iter().map(|c| c.working_total()).sum();
            let starved: u64 = p.classes.iter().map(|c| c.starved + c.done).sum();
            let made = item.map(|i| p.c.produced[i]).unwrap_or(0);
            (p.distinct_states(), working, starved, made)
        });
        let mut j = Json::obj()
            .set("name", self.spec.name)
            .set("mode", if self.awake() { "awake" } else { "closed" })
            .set("machines", Json::big(self.machines() as u128))
            .set("states", states)
            .set("working", Json::big(working as u128))
            .set("idle", Json::big(starved as u128))
            .set("made", Json::big(made as u128))
            .set("product", self.spec.product)
            .set("woke", self.woke as u64)
            .set("recompiles", self.recompiles as u64)
            .set("closes", self.closes as u64)
            .set("stepped", self.stepped)
            .set("evals", self.evals.get())
            .set("changed", Json::arr(self.changed()))
            .set(
                "edits",
                Json::Arr(
                    self.edits
                        .iter()
                        .rev()
                        .take(6)
                        .map(|(t, e)| Json::obj().set("at", *t).set("what", e.clone()))
                        .collect(),
                ),
            );
        if let Mode::Closed(s) = &self.mode {
            let (per, period) = item.map(|i| s.rate(i)).unwrap_or((0, s.period));
            j = j
                .set("period", s.period)
                .set("t0", s.t0)
                .set("frozen", s.frozen)
                .set("perPeriod", Json::big(per as u128))
                .set("orbitStates", s.states_visited)
                .set("ratePerMin", if period == 0 { 0.0 } else { per as f64 * 3600.0 / period as f64 });
        }
        j
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::combat::field::SECTORS;

    #[test]
    fn every_sector_settles_into_an_orbit() {
        for spec in SECTORS {
            let s = Sector::open(spec);
            assert!(!s.awake(), "{} found no orbit", spec.name);
        }
    }

    #[test]
    fn the_closed_form_is_the_stepped_run() {
        for spec in SECTORS {
            let s = Sector::open(spec);
            let t = 123_457;
            let closed = s.carry(t).signature();
            let mut p = Pop::new(s.bp(), s.n_items());
            p.run_until(t);
            let stepped = Carry::from_seed(&p.harvest(), &s.prog, s.bp(), t).signature();
            assert_eq!(closed, stepped, "{}", spec.name);
        }
    }
}
