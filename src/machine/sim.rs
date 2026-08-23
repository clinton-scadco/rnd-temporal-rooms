//! The machine, one tick at a time.
//!
//! Every component is a state machine and every connection carries a
//! deterministic quantity of a deterministic *stuff*, exactly as the rest of
//! this crate does it. Nothing here averages anything: a design that starves
//! for eleven ticks and then boils for three starves for eleven ticks and then
//! boils for three, and the only reason the answer at tick 10^9 is cheap is
//! that `orbit` later notices the machine repeating itself.
//!
//! # A tick
//!
//! ```text
//!   1. transfer   move stuff along wires, obeying both ends
//!   2. export     whatever is left in a boundary port leaves the machine
//!   3. step       every component consumes its inputs and fills its outputs
//! ```
//!
//! In that order, which is the whole latency model: a quantity a component puts
//! into an output buffer during step *t* cannot move until the transfer at
//! *t+1*, so every hop costs a tick and every pipe costs two. Nobody had to
//! write a delay line.
//!
//! Export sits *after* transfer rather than at the end of the tick so that a
//! boundary port can be both. A generator's power leaves the machine, unless
//! something inside the machine took it first -- which is how a plant powers
//! its own conveyors and sells the difference.
//!
//! # Contention
//!
//! When several wires want the same throughput, the split is a policy and the
//! policy is *stated*. It is max-min fair -- everyone gets an equal share,
//! anyone who wants less than their share frees the difference for the rest --
//! and the remainder rotates on a cursor that is part of the machine's state.
//! That last detail is why a fan-out of three on a budget of ten has a period
//! of three rather than a permanent favourite.
//!
//! # What a component may refuse
//!
//! Experiment 06 had one refusal in the whole simulation: a turbine below its
//! threshold. Experiment 07 has a general one, and it is the mechanic rather
//! than an error case. A crusher will not take a drive turning at speed 6; a
//! rolling mill will not touch cold metal; a mill will not take lumps. Each of
//! those stops the component dead and records *which* condition it was, so the
//! inspector can say the sentence that teaches the player the mechanic.

use super::design::{Design, Link, Tune};
use super::parts::{self, Dir, Kind, Need, Recipe};
use super::stuff::{Buf, Domain, Stuff, Subst};
use std::collections::BTreeMap;

pub type Tick = u64;

/// How many offer/accept rounds the transfer stage runs.
///
/// One round can leave throughput stranded: a source splits its budget between
/// two destinations, one of them turns out to be full, and the share it was
/// offered goes nowhere. A second round hands that back out.
const ROUNDS: usize = 3;

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Status {
    /// Nothing to do, and nothing wrong.
    Idle,
    Running,
    /// Wants more of an input than it is being given.
    Starved,
    /// Cannot put its output anywhere.
    Blocked,
    /// A turbine below its threshold: not slow, stopped.
    Stalled,
    /// A reactor on its way up to temperature.
    Warming,
    /// Making more than anyone is taking, and throwing the difference away.
    Venting,
    /// A store holding on to something on purpose.
    Filling,
    /// What arrived is not something this component will accept.
    Refused,
}

impl Status {
    pub fn tag(self) -> &'static str {
        match self {
            Status::Idle => "IDLE",
            Status::Running => "RUNNING",
            Status::Starved => "STARVED",
            Status::Blocked => "BLOCKED",
            Status::Stalled => "STALLED",
            Status::Warming => "WARMING",
            Status::Venting => "VENTING",
            Status::Filling => "FILLING",
            Status::Refused => "REFUSED",
        }
    }
    /// Whether this is the status of a component that is doing its job.
    pub fn well(self) -> bool {
        matches!(self, Status::Running | Status::Warming | Status::Filling)
    }
}

/// Why a component made nothing, as something small enough to live in a state
/// that is copied twenty thousand times.
///
/// The sentence is composed later, by `snap`, out of this and the part table.
/// Putting the sentence itself here would allocate a string per component per
/// tick, which an orbit search would notice.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Stop {
    None,
    /// Not enough in this input port.
    Short(usize),
    /// No room in this output port.
    Full(usize),
    /// `draws[d].need[n]` was not met by what is in the port.
    Unmet(usize, usize),
    /// This port holds something that will not mix with what wants in.
    Wrong(usize),
    /// Enough to run slowly, and this component does not run slowly.
    Below(u64),
}

// ------------------------------------------------------------------- flows

/// What crossed the machine's boundary, by substance and quality.
///
/// A map rather than a handful of named counters, because experiment 07 has no
/// idea in advance what a design produces: 82%-pure iron ore powder and 40%
/// tailings are the same substance and different products, and the scoreboard
/// has to be able to tell them apart without anybody adding a field.
pub type Flow = BTreeMap<Stuff, u64>;
pub type FlowBig = BTreeMap<Stuff, u128>;

fn bump(f: &mut Flow, s: Stuff, n: u64) {
    if n > 0 {
        *f.entry(s).or_insert(0) += n;
    }
}

fn merge(into: &mut FlowBig, from: &Flow, k: u128) {
    for (s, n) in from {
        *into.entry(*s).or_insert(0) += *n as u128 * k;
    }
}

fn merge_big(into: &mut FlowBig, from: &FlowBig, k: u128) {
    for (s, n) in from {
        *into.entry(*s).or_insert(0) += *n * k;
    }
}

fn unmerge(into: &mut FlowBig, from: &FlowBig) {
    for (s, n) in from {
        let e = into.entry(*s).or_insert(0);
        *e -= *n;
    }
    into.retain(|_, v| *v != 0);
}

/// Everything of one substance in a flow, whatever its quality.
pub fn of_subst(f: &FlowBig, s: Subst) -> u128 {
    f.iter().filter(|(k, _)| k.subst == s).map(|(_, v)| *v).sum()
}

/// What one tick did to the machine as a whole.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    /// Electricity leaving the machine. Kept as its own number because the
    /// first brief is written in megawatts and the waveform is drawn in them.
    pub power: u64,
    pub heat_wasted: u64,
    /// Utilisation summed over components, in per mille each.
    pub util_sum: u64,
    /// Matter drawn in from outside.
    pub took: Flow,
    /// Product leaving through a boundary port.
    pub gave: Flow,
    /// Thrown away inside: condensed, skipped, leaked.
    pub lost: Flow,
}

impl Delta {
    pub fn qty_out(&self, s: Subst) -> u64 {
        self.gave.iter().filter(|(k, _)| k.subst == s).map(|(_, v)| *v).sum()
    }
}

/// Everything since tick 0. `u128` because a million ticks of a large plant is
/// past what a `u64` of MW-ticks would survive being multiplied into.
#[derive(Clone, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub power: u128,
    pub heat_wasted: u128,
    pub util_sum: u128,
    pub ticks: u128,
    pub took: FlowBig,
    pub gave: FlowBig,
    pub lost: FlowBig,
}

impl Totals {
    pub fn add(&mut self, d: &Delta) {
        self.power += d.power as u128;
        self.heat_wasted += d.heat_wasted as u128;
        self.util_sum += d.util_sum as u128;
        self.ticks += 1;
        merge(&mut self.took, &d.took, 1);
        merge(&mut self.gave, &d.gave, 1);
        merge(&mut self.lost, &d.lost, 1);
    }

    pub fn plus(mut self, o: &Totals) -> Totals {
        self.power += o.power;
        self.heat_wasted += o.heat_wasted;
        self.util_sum += o.util_sum;
        self.ticks += o.ticks;
        merge_big(&mut self.took, &o.took, 1);
        merge_big(&mut self.gave, &o.gave, 1);
        merge_big(&mut self.lost, &o.lost, 1);
        self
    }

    pub fn scaled(&self, k: u128) -> Totals {
        let mut t = Totals {
            power: self.power * k,
            heat_wasted: self.heat_wasted * k,
            util_sum: self.util_sum * k,
            ticks: self.ticks * k,
            ..Default::default()
        };
        merge_big(&mut t.took, &self.took, k);
        merge_big(&mut t.gave, &self.gave, k);
        merge_big(&mut t.lost, &self.lost, k);
        t
    }

    pub fn minus(&self, o: &Totals) -> Totals {
        let mut t = Totals {
            power: self.power - o.power,
            heat_wasted: self.heat_wasted - o.heat_wasted,
            util_sum: self.util_sum - o.util_sum,
            ticks: self.ticks - o.ticks,
            took: self.took.clone(),
            gave: self.gave.clone(),
            lost: self.lost.clone(),
        };
        unmerge(&mut t.took, &o.took);
        unmerge(&mut t.gave, &o.gave);
        unmerge(&mut t.lost, &o.lost);
        t
    }

    /// The two costs experiment 06 had as fields, kept as questions so that
    /// every design written for that brief still reports the same numbers.
    pub fn fuel(&self) -> u128 {
        of_subst(&self.took, Subst::Coal)
    }
    pub fn water(&self) -> u128 {
        of_subst(&self.took, Subst::Water)
    }
    /// Steam that reached a stalled turbine and condensed, plus anything a skip
    /// swallowed. Reported in units of matter, and separately from heat.
    pub fn vented(&self) -> u128 {
        self.lost.values().sum()
    }
    pub fn grid(&self) -> u128 {
        of_subst(&self.took, Subst::Power)
    }
}

/// One component's live state.
#[derive(Clone, Debug)]
pub struct UnitState {
    /// One buffer per port, in the part's port order.
    pub buf: Vec<Buf>,
    /// Ticks since the machine started, clamped at `WARMUP`. Reactors only.
    pub age: u64,
    /// Turbines only, `0..=SPIN_MAX`.
    pub spin: u32,
    /// Stores only: emptying rather than filling.
    pub draining: bool,
    /// Fair-share rotation, one per port, kept modulo that port's wire count so
    /// that the state space stays finite and an orbit can close.
    pub cursor: Vec<u32>,

    // ------- what happened during the tick just simulated, for the inspector
    pub status: Status,
    pub stop: Stop,
    /// Arrived along wires, per port.
    pub got: Vec<u64>,
    /// Left along wires, per port.
    pub sent: Vec<u64>,
    /// Drawn out of an input buffer by the component itself.
    pub used: Vec<u64>,
    /// Put into an output buffer by the component itself.
    pub made: Vec<u64>,
    /// Left the machine from this port.
    pub shipped: Vec<u64>,
    /// Thrown away by this component this tick, in its own units.
    pub waste: u64,
    /// Per mille of what this component is rated to do.
    pub util: u32,
}

#[derive(Clone)]
pub struct Machine {
    pub names: Vec<String>,
    pub kinds: Vec<Kind>,
    pub tunes: Vec<Tune>,
    pub links: Vec<Link>,
    /// `out_wires[unit][port]` -- link indices leaving that port.
    out_wires: Vec<Vec<Vec<usize>>>,
    in_wires: Vec<Vec<Vec<usize>>>,
    pub st: Vec<UnitState>,
    /// How much crossed each wire during the tick just simulated, and what it
    /// was. Per wire and not per port, because "which of my three pipes is
    /// actually carrying anything" is the question a player asks first.
    pub flow: Vec<u64>,
    pub carried: Vec<Stuff>,
    pub tick: Tick,
    pub last: Delta,
}

impl Machine {
    pub fn new(d: &Design) -> Result<Machine, String> {
        let faults = d.check();
        if let Some(f) = faults.first() {
            return Err(f.what.clone());
        }
        let links = d.links()?;
        let n = d.units.len();
        let mut out_wires = vec![Vec::new(); n];
        let mut in_wires = vec![Vec::new(); n];
        for (i, u) in d.units.iter().enumerate() {
            let np = parts::part(u.kind).ports.len();
            out_wires[i] = vec![Vec::new(); np];
            in_wires[i] = vec![Vec::new(); np];
        }
        let nlinks = links.len();
        for (li, l) in links.iter().enumerate() {
            out_wires[l.from][l.from_port].push(li);
            in_wires[l.to][l.to_port].push(li);
        }
        let st = d
            .units
            .iter()
            .map(|u| {
                let ports = parts::part(u.kind).ports;
                let np = ports.len();
                UnitState {
                    buf: ports.iter().map(|p| Buf::empty(p.dom.rest())).collect(),
                    age: 0,
                    spin: 0,
                    draining: false,
                    cursor: vec![0; np],
                    status: Status::Idle,
                    stop: Stop::None,
                    got: vec![0; np],
                    sent: vec![0; np],
                    used: vec![0; np],
                    made: vec![0; np],
                    shipped: vec![0; np],
                    waste: 0,
                    util: 0,
                }
            })
            .collect();
        Ok(Machine {
            names: d.units.iter().map(|u| u.name.clone()).collect(),
            kinds: d.units.iter().map(|u| u.kind).collect(),
            tunes: d.units.iter().map(|u| u.tune).collect(),
            links,
            out_wires,
            in_wires,
            st,
            flow: vec![0; nlinks],
            carried: vec![Stuff::fresh(Subst::Heat); nlinks],
            tick: 0,
            last: Delta::default(),
        })
    }

    pub fn len(&self) -> usize {
        self.st.len()
    }

    pub fn is_empty(&self) -> bool {
        self.st.is_empty()
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.names.iter().position(|n| n == name)
    }

    /// The wires into and out of a port, for anything that wants to explain
    /// where a component's supply comes from.
    pub fn feeders(&self, unit: usize, port: usize) -> &[usize] {
        &self.in_wires[unit][port]
    }
    pub fn drains(&self, unit: usize, port: usize) -> &[usize] {
        &self.out_wires[unit][port]
    }

    // ------------------------------------------------------------- the tick

    pub fn step(&mut self) -> Delta {
        for s in &mut self.st {
            s.got.iter_mut().for_each(|v| *v = 0);
            s.sent.iter_mut().for_each(|v| *v = 0);
            s.used.iter_mut().for_each(|v| *v = 0);
            s.made.iter_mut().for_each(|v| *v = 0);
            s.shipped.iter_mut().for_each(|v| *v = 0);
            s.waste = 0;
            s.stop = Stop::None;
        }
        self.flow.iter_mut().for_each(|v| *v = 0);
        let mut d = Delta::default();
        self.transfer(&mut d);
        self.export(&mut d);
        self.run_units(&mut d);
        self.rotate();
        self.tick += 1;
        self.last = d.clone();
        d
    }

    /// Move stuff along the wires.
    ///
    /// One wire in ten ends at the machine's edge rather than at a component,
    /// and those are settled here rather than a stage later. A boundary input
    /// -- an outlet, a skip, a radiator -- is not a buffer: whatever crosses it
    /// leaves the machine in the same tick it arrives. That is what lets one
    /// outlet take the light fraction and the middle fraction on two different
    /// ports without either of them contaminating a port the other wanted, and
    /// it keeps a sink's state permanently empty, which an orbit search
    /// appreciates.
    fn transfer(&mut self, d: &mut Delta) {
        let n = self.st.len();
        // What each port has left to spend this tick.
        let mut budget: Vec<Vec<u64>> = (0..n)
            .map(|i| parts::part(self.kinds[i]).ports.iter().map(|p| p.rate).collect())
            .collect();

        for _ in 0..ROUNDS {
            let mut offer = vec![0u64; self.links.len()];
            let mut any = false;

            for u in 0..n {
                let ports = parts::part(self.kinds[u]).ports;
                for p in 0..ports.len() {
                    if ports[p].dir != Dir::Out || self.out_wires[u][p].is_empty() {
                        continue;
                    }
                    let avail = budget[u][p].min(self.st[u].buf[p].qty);
                    if avail == 0 {
                        continue;
                    }
                    let mine = self.st[u].buf[p].stuff;
                    // What each destination could still swallow. Using the
                    // destination's room as the demand is what stops a source
                    // from committing its whole budget to a full neighbour --
                    // and a destination holding something that will not mix
                    // with this is not a destination at all.
                    let wires = self.out_wires[u][p].clone();
                    let demands: Vec<u64> = wires
                        .iter()
                        .map(|&w| {
                            let l = self.links[w];
                            let dp = &parts::part(self.kinds[l.to]).ports[l.to_port];
                            let db = &self.st[l.to].buf[l.to_port];
                            if !db.takes(&mine) {
                                return 0;
                            }
                            budget[l.to][l.to_port].min(dp.cap - db.qty)
                        })
                        .collect();
                    let alloc = share(avail, &demands, self.st[u].cursor[p]);
                    for (k, &w) in wires.iter().enumerate() {
                        offer[w] = alloc[k];
                    }
                }
            }

            for u in 0..n {
                let ports = parts::part(self.kinds[u]).ports;
                for p in 0..ports.len() {
                    if ports[p].dir != Dir::In || self.in_wires[u][p].is_empty() {
                        continue;
                    }
                    let room = budget[u][p].min(ports[p].cap - self.st[u].buf[p].qty);
                    if room == 0 {
                        continue;
                    }
                    let leaves = ports[p].external;
                    let wires = self.in_wires[u][p].clone();
                    let demands: Vec<u64> = wires.iter().map(|&w| offer[w]).collect();
                    let alloc = share(room, &demands, self.st[u].cursor[p]);
                    for (k, &w) in wires.iter().enumerate() {
                        let q = alloc[k];
                        if q == 0 {
                            continue;
                        }
                        let l = self.links[w];
                        let src = self.st[l.from].buf[l.from_port].stuff;
                        // Checked again here rather than only in the offer: an
                        // earlier wire may have filled this port with something
                        // else during this very round.
                        if !leaves && !self.st[u].buf[p].takes(&src) {
                            self.st[u].stop = Stop::Wrong(p);
                            continue;
                        }
                        any = true;
                        let (moved, got) = self.st[l.from].buf[l.from_port].take(q);
                        self.st[l.from].sent[l.from_port] += got;
                        budget[l.from][l.from_port] -= got;
                        if leaves {
                            self.leaves(u, moved, got, d);
                        } else {
                            self.st[u].buf[p].put(moved, got);
                        }
                        self.st[u].got[p] += got;
                        budget[u][p] -= got;
                        self.flow[w] += got;
                        self.carried[w] = moved;
                    }
                }
            }
            if !any {
                break;
            }
        }
    }

    /// Whatever is still sitting in a boundary output port leaves the machine.
    fn export(&mut self, d: &mut Delta) {
        for i in 0..self.st.len() {
            let ports = parts::part(self.kinds[i]).ports;
            for p in 0..ports.len() {
                if !ports[p].external || ports[p].dir != Dir::Out {
                    continue;
                }
                let all = self.st[i].buf[p].qty;
                let (s, n) = self.st[i].buf[p].take(all);
                if n == 0 {
                    continue;
                }
                self.st[i].shipped[p] += n;
                bump(&mut d.gave, s, n);
                if s.subst == Subst::Power {
                    d.power += n;
                }
            }
        }
    }

    fn rotate(&mut self) {
        for u in 0..self.st.len() {
            for p in 0..self.st[u].cursor.len() {
                let k = (self.out_wires[u][p].len().max(self.in_wires[u][p].len())).max(1) as u32;
                self.st[u].cursor[p] = (self.st[u].cursor[p] + 1) % k;
            }
        }
    }

    /// Every component's own state machine.
    fn run_units(&mut self, d: &mut Delta) {
        for i in 0..self.st.len() {
            let kind = self.kinds[i];
            match kind {
                Kind::Reactor => self.reactor(i, d),
                Kind::Mains => self.source(i, Subst::Power, d),
                Kind::Pump => self.source(i, self.tunes[i].subst, d),
                Kind::Inlet => self.source(i, self.tunes[i].subst, d),
                Kind::Hopper | Kind::Tank | Kind::Drum | Kind::Flywheel => self.store(i),
                Kind::Outlet | Kind::Skip | Kind::Radiator => self.dump(i),
                Kind::HeatPipe => self.conduit(i, parts::PIPE_LOSS_PCT, d),
                Kind::SteamPipe | Kind::FluidPipe | Kind::Chute => self.conduit(i, 0, d),
                Kind::Shaft | Kind::Cable => self.conduit(i, parts::SHAFT_LOSS_PCT, d),
                Kind::Valve | Kind::Clutch => self.limiter(i),
                Kind::Gearbox => self.gearbox(i, d),
                Kind::Turbine => self.turbine(i, d),
                Kind::Generator => self.generator(i),
                Kind::Furnace => self.furnace(i),
                Kind::Column => self.column(i),
                _ => {
                    let r = parts::part(kind).recipe.expect("every other kind is a recipe");
                    self.recipe(i, r);
                }
            }
            self.perish(i, d);
            d.util_sum += self.st[i].util as u64;
        }
    }

    /// A stroke is a movement, not a material.
    ///
    /// Everything else in the machine queues: heat waits in a pipe, ore waits
    /// in a hopper, steam waits in a tank. Linear motion does not. A crank
    /// turning drives the ram whether or not the press closed on anything, so
    /// whatever reaches a `mech` input and is not used in the same tick has
    /// happened and is gone -- into heat, like all unwanted work.
    ///
    /// This is the same decision as the turbine's, where gas that arrives and
    /// is not used condenses rather than queueing, and it is what makes a
    /// threshold worth having: a component that cannot run slowly *and* cannot
    /// accumulate is a component that needs a buffer put in front of it on
    /// purpose.
    fn perish(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(self.kinds[i]).ports;
        for p in 0..ports.len() {
            if ports[p].dom != Domain::Mech || ports[p].dir != Dir::In {
                continue;
            }
            let left = self.st[i].buf[p].qty;
            if left == 0 {
                continue;
            }
            self.st[i].buf[p].take(left);
            self.st[i].waste += left;
            d.heat_wasted += left;
        }
    }

    // -------------------------------------------------------- the generic one

    /// A component that is a row in the part table: draw, check, make.
    ///
    /// This is where experiment 07 earns the rewrite. Fourteen of the
    /// thirty-eight components are this function and a table entry, so adding a
    /// press or a separator is a change to `parts.rs` and nothing else -- and
    /// every one of them starves, blocks, refuses and explains itself the same
    /// way, because it is all the same twenty lines.
    ///
    /// The other twenty-four are hand written, but not twenty-four times: one
    /// `conduit` is six kinds of pipe, one `store` is four kinds of buffer, one
    /// `source` is three kinds of inlet and one `dump` is three kinds of
    /// boundary. Only six components -- reactor, gearbox, turbine, generator,
    /// furnace, column -- are genuinely one of a kind, and each of those has a
    /// warm-up, a ratio, a threshold, a rounding, a phase change or a split
    /// that a table row could not have said.
    fn recipe(&mut self, i: usize, r: &'static Recipe) {
        let part = parts::part(self.kinds[i]);
        let mut n = r.rate;
        let mut stop = Stop::None;

        // What each input would supply, and whether it will do.
        let mut drawn: Vec<Stuff> = Vec::with_capacity(r.draws.len());
        for (di, dr) in r.draws.iter().enumerate() {
            let b = self.st[i].buf[dr.port];
            drawn.push(b.stuff);
            if b.qty == 0 {
                n = 0;
                if stop == Stop::None {
                    stop = Stop::Short(dr.port);
                }
                continue;
            }
            for (ni, need) in dr.need.iter().enumerate() {
                if need.unmet(&b.stuff).is_some() {
                    n = 0;
                    stop = Stop::Unmet(di, ni);
                }
            }
            let by_this = b.qty / dr.qty;
            if by_this < n {
                n = by_this;
                if stop == Stop::None || n == 0 {
                    stop = Stop::Short(dr.port);
                }
            }
        }

        // And whether there is anywhere to put the result.
        let outs: Vec<Stuff> =
            r.makes.iter().map(|m| r.out_stuff(m, &drawn, part.ports)).collect();
        for (mi, m) in r.makes.iter().enumerate() {
            let b = self.st[i].buf[m.port];
            if !b.takes(&outs[mi]) {
                n = 0;
                stop = Stop::Wrong(m.port);
                continue;
            }
            let room = part.ports[m.port].cap - b.qty;
            let by_room = room / m.qty;
            if by_room < n {
                n = by_room;
                stop = Stop::Full(m.port);
            }
        }

        // A component with a floor does not run slowly; it fails to run. The
        // check goes here, after everything that could have limited the rate,
        // because it is about what the component was actually able to do and
        // not about what any one input was short of.
        if n < r.floor {
            n = 0;
            stop = Stop::Below(r.floor);
        }

        if n > 0 {
            for dr in r.draws {
                let (_, got) = self.st[i].buf[dr.port].take(dr.qty * n);
                self.st[i].used[dr.port] += got;
            }
            for (mi, m) in r.makes.iter().enumerate() {
                self.st[i].buf[m.port].put(outs[mi], m.qty * n);
                self.st[i].made[m.port] += m.qty * n;
            }
        }

        let s = &mut self.st[i];
        s.util = (n * 1000 / r.rate) as u32;
        s.stop = stop;
        s.status = if n == r.rate {
            Status::Running
        } else {
            match stop {
                Stop::Below(_) => Status::Stalled,
                Stop::Unmet(..) | Stop::Wrong(_) => Status::Refused,
                Stop::Full(_) => Status::Blocked,
                Stop::Short(_) => Status::Starved,
                Stop::None => Status::Idle,
            }
        };
    }

    // ------------------------------------------------------------ the others

    fn reactor(&mut self, i: usize, d: &mut Delta) {
        let thr = self.tunes[i].throttle.clamp(parts::MIN_THROTTLE, 100) as u64;
        let ramp = self.st[i].age.min(parts::WARMUP);
        let made = parts::REACTOR_HEAT * thr / 100 * ramp / parts::WARMUP;
        let cap = parts::part(Kind::Reactor).ports[0].cap;
        let room = cap - self.st[i].buf[0].qty;
        let into = made.min(room);
        let vented = made - into;

        // Fuel burns at the throttle setting from the first tick, warm or not.
        // Nothing about a fire cares whether the boiler is ready.
        bump(&mut d.took, Stuff::fresh(Subst::Coal), parts::REACTOR_FUEL * thr / 100);
        d.heat_wasted += vented;

        let heat = Stuff::with(
            Subst::Heat,
            super::stuff::Qual { temp: parts::REACTOR_TEMP, ..Default::default() },
        );
        let s = &mut self.st[i];
        s.buf[0].put(heat, into);
        s.made[0] = into;
        s.waste = vented;
        s.util = (into * 1000 / parts::REACTOR_HEAT) as u32;
        s.status = if s.age < parts::WARMUP {
            Status::Warming
        } else if vented > 0 {
            Status::Venting
        } else {
            Status::Running
        };
        s.age = (s.age + 1).min(parts::WARMUP);
    }

    /// A pump, an inlet or a grid connection: the same component with a
    /// different substance in it.
    ///
    /// Counted where it is taken out of the world, not where it is used: a
    /// design that fills a buffer it never draws from has still used it.
    fn source(&mut self, i: usize, subst: Subst, d: &mut Delta) {
        let p = &parts::part(self.kinds[i]).ports[0];
        let s = &mut self.st[i];
        let want = Stuff::fresh(subst);
        // A source that has been left holding something else -- a pump retuned
        // from water to crude while it was running -- empties before it draws.
        if !s.buf[0].takes(&want) {
            s.status = Status::Refused;
            s.util = 0;
            return;
        }
        let room = p.cap - s.buf[0].qty;
        let made = p.rate.min(room);
        s.buf[0].put(want, made);
        s.made[0] = made;
        s.util = (made * 1000 / p.rate) as u32;
        s.status = if made == 0 { Status::Blocked } else { Status::Running };
        bump(&mut d.took, want, made);
    }

    /// A hopper, a tank, a drum or a flywheel: in one side, out the other, with
    /// the option of holding on.
    ///
    /// Hysteresis, and the reason a buffer is not just a bigger pipe: one that
    /// fills quietly and empties hard can push a turbine over a threshold that
    /// a steady trickle never would.
    ///
    /// The low-water mark is tested *after* the release, not before. Tested
    /// before, a store that is being refilled at the same time it drains never
    /// sees itself empty -- it latches open on the first pulse and spends the
    /// rest of the run being an expensive pipe.
    fn store(&mut self, i: usize) {
        let ports = parts::part(self.kinds[i]).ports;
        let t = self.tunes[i];
        let s = &mut self.st[i];
        let level = s.buf[0].qty;
        if t.pulse {
            if !s.draining && level >= t.high {
                s.draining = true;
            }
        } else {
            s.draining = true;
        }
        let held = s.buf[0].stuff;
        let room = if s.buf[1].takes(&held) { ports[1].cap - s.buf[1].qty } else { 0 };
        let moved = if s.draining { level.min(ports[1].rate).min(room) } else { 0 };
        let (took, got) = s.buf[0].take(moved);
        if t.pulse && s.draining && s.buf[0].qty <= t.low {
            s.draining = false;
        }
        s.buf[1].put(took, got);
        s.used[0] = got;
        s.made[1] = got;
        s.util = (got * 1000 / ports[1].rate) as u32;
        s.status = if got > 0 {
            Status::Running
        } else if level == 0 {
            Status::Idle
        } else if room == 0 {
            Status::Blocked
        } else {
            Status::Filling
        };
    }

    /// Which column of the scoreboard a boundary input adds to.
    ///
    /// One function, because an outlet, a skip and a radiator are the same
    /// component: three ports, no buffer, and a different heading. Stating the
    /// difference as one argument rather than three implementations is the
    /// honest way to say so.
    fn leaves(&mut self, i: usize, what: Stuff, n: u64, d: &mut Delta) {
        if n == 0 {
            return;
        }
        self.st[i].shipped[0] += n;
        if what.subst == Subst::Heat {
            d.heat_wasted += n;
        } else if self.kinds[i] == Kind::Outlet {
            bump(&mut d.gave, what, n);
            if what.subst == Subst::Power {
                d.power += n;
            }
        } else {
            bump(&mut d.lost, what, n);
        }
    }

    /// An outlet, a skip or a radiator, reporting what went through it. The
    /// counting itself happened during the transfer -- see `leaves` -- because
    /// a boundary is not somewhere anything waits.
    fn dump(&mut self, i: usize) {
        let ports = parts::part(self.kinds[i]).ports;
        let rate: u64 = ports.iter().map(|p| p.rate).sum();
        let s = &mut self.st[i];
        let total: u64 = s.got.iter().sum();
        for p in 0..ports.len() {
            s.used[p] = s.got[p];
        }
        s.waste = total;
        s.util = (total * 1000 / rate.max(1)) as u32;
        s.status = if total > 0 { Status::Running } else { Status::Idle };
    }

    /// A pipe, a chute or a line shaft. Distance, and what it costs.
    fn conduit(&mut self, i: usize, loss_pct: u64, d: &mut Delta) {
        let ports = parts::part(self.kinds[i]).ports;
        let cap_out = ports[1].cap;
        let s = &mut self.st[i];
        let have = s.buf[0].qty;
        let held = s.buf[0].stuff;
        let room = if s.buf[1].takes(&held) { cap_out - s.buf[1].qty } else { 0 };

        // Take as much as will still fit once the leak has taken its cut.
        let mut take = have.min(room * 100 / (100 - loss_pct));
        let mut net = take - take * loss_pct / 100;
        if net > room {
            take = room;
            net = take - take * loss_pct / 100;
        }

        d.heat_wasted += take - net;
        let (what, got) = s.buf[0].take(take);
        s.buf[1].put(what, net);
        s.used[0] = got;
        s.made[1] = net;
        s.waste = got - net;
        s.util = (net * 1000 / ports[1].rate) as u32;
        s.status = if have == 0 {
            Status::Idle
        } else if take < have {
            Status::Blocked
        } else {
            Status::Running
        };
    }

    /// A valve or a clutch: a threshold, stated as a number, doing exactly what
    /// it says.
    ///
    /// The clutch is the one worth having. It will not engage until its
    /// threshold has gathered, which lets one stuttering drive turn something
    /// that must not be turned slowly -- the rotary equivalent of a pulsed
    /// tank, and the reason a shared shaft with six things on it does not have
    /// to be sized for the worst tick.
    fn limiter(&mut self, i: usize) {
        let kind = self.kinds[i];
        let ports = parts::part(kind).ports;
        let t = self.tunes[i];
        let s = &mut self.st[i];
        let have = s.buf[0].qty;
        let held = s.buf[0].stuff;
        let room = if s.buf[1].takes(&held) { ports[1].cap - s.buf[1].qty } else { 0 };
        let limit = t.limit.min(ports[0].rate);

        let engaged = match kind {
            Kind::Clutch => {
                if s.draining {
                    have > 0
                } else {
                    have >= limit
                }
            }
            _ => true,
        };
        s.draining = engaged;
        let allow = match kind {
            Kind::Clutch => {
                if engaged {
                    have
                } else {
                    0
                }
            }
            _ => limit,
        };
        let moved = have.min(allow).min(ports[1].rate).min(room);
        let (what, got) = s.buf[0].take(moved);
        s.buf[1].put(what, got);
        s.used[0] = got;
        s.made[1] = got;
        s.util = (got * 1000 / ports[1].rate) as u32;
        s.status = if got > 0 {
            Status::Running
        } else if have == 0 {
            Status::Idle
        } else if room == 0 {
            Status::Blocked
        } else {
            Status::Filling
        };
    }

    /// Speed for the ability to turn something heavy, or the other way round.
    ///
    /// The quantity on a rotary wire is power and the property is speed, so
    /// what a gearbox actually changes is the *band* -- and since a crusher
    /// will not take more than speed 2 and a mill will not take less than 4,
    /// the ratio is the first thing a player has to get right about a drive
    /// train. It costs 2% either way, which is why gearing down and back up
    /// again is a thing you can do and would rather not.
    fn gearbox(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(Kind::Gearbox).ports;
        let ratio = self.tunes[i].ratio;
        let s = &mut self.st[i];
        let have = s.buf[0].qty;
        let mut what = s.buf[0].stuff;
        what.q.speed = geared(what.q.speed, ratio);
        let room = if s.buf[1].takes(&what) { ports[1].cap - s.buf[1].qty } else { 0 };
        let take = have.min(room * 100 / (100 - parts::GEARBOX_LOSS_PCT)).min(ports[0].rate);
        let net = (take - take * parts::GEARBOX_LOSS_PCT / 100).min(room);
        let (_, got) = s.buf[0].take(take);
        s.buf[1].put(what, net);
        s.used[0] = got;
        s.made[1] = net;
        s.waste = got - net;
        s.util = (net * 1000 / ports[1].rate) as u32;
        s.status = if have == 0 {
            Status::Idle
        } else if net == 0 {
            Status::Blocked
        } else {
            Status::Running
        };
        d.heat_wasted += got - net;
    }

    fn turbine(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(Kind::Turbine).ports;
        let s = &mut self.st[i];
        let have = s.buf[0].qty;
        let out = Stuff::with(
            Subst::Torque,
            super::stuff::Qual { speed: parts::DRIVE_SPEED, purity: 100, ..Default::default() },
        );
        let room = if s.buf[1].takes(&out) { ports[1].cap - s.buf[1].qty } else { 0 };
        let stalled = have < parts::TURBINE_MIN;
        let spin = if stalled {
            s.spin.saturating_sub(parts::SPIN_DOWN)
        } else {
            (s.spin + parts::SPIN_UP).min(parts::SPIN_MAX)
        };
        let rotary_of = |intake: u64| {
            intake * parts::TURBINE_EFF / 100 * spin as u64 / parts::SPIN_MAX as u64
        };

        let mut intake = if stalled { 0 } else { have.min(ports[0].rate) };
        let mut made = rotary_of(intake);
        if made > room {
            // Back-pressure: take only as much gas as the shaft can pass on.
            // Searched rather than solved, because the closed form of an
            // integer-truncated product is not worth the reader's time.
            let (mut lo, mut hi) = (0u64, intake);
            while lo < hi {
                let mid = (lo + hi + 1) / 2;
                if rotary_of(mid) <= room {
                    lo = mid;
                } else {
                    hi = mid - 1;
                }
            }
            intake = lo;
            made = rotary_of(intake);
        }

        // A turbine casing is not a tank. Whatever arrived and was not used
        // condenses, which is exactly why a Gas Buffer upstream is worth its
        // nine tiles.
        let (_, used) = s.buf[0].take(intake);
        let vented = s.buf[0].qty;
        let (lost, _) = s.buf[0].take(vented);
        s.buf[1].put(out, made);
        s.used[0] = used;
        s.made[1] = made;
        s.waste = vented;
        s.spin = spin;
        s.util = (used * 1000 / ports[0].rate) as u32;
        s.status = if stalled {
            Status::Stalled
        } else if room == 0 || used < have.min(ports[0].rate) {
            Status::Blocked
        } else if used < ports[0].rate {
            Status::Starved
        } else {
            Status::Running
        };
        bump(&mut d.lost, lost, vented);
    }

    /// Rotary in, megawatts out, and it will not be turned over slowly.
    ///
    /// Hand written rather than a table row for one reason: a recipe works in
    /// whole batches, and a generator that rounded its intake down to the
    /// nearest ten would quietly discard up to nine rotary a tick. Experiment
    /// 06's six designs are reported to two decimal places, and this is the
    /// component all six of them end at.
    fn generator(&mut self, i: usize) {
        let ports = parts::part(Kind::Generator).ports;
        let s = &mut self.st[i];
        let slow = s.buf[0].qty > 0 && s.buf[0].stuff.q.speed < parts::GENERATOR_MIN_SPEED;
        let intake = if slow { 0 } else { s.buf[0].qty.min(ports[0].rate) };
        let (_, used) = s.buf[0].take(intake);
        let mw = used * parts::GENERATOR_EFF / 100;
        s.buf[1].put(Stuff::fresh(Subst::Power), mw);
        s.used[0] = used;
        s.made[1] = mw;
        s.util = (used * 1000 / ports[0].rate) as u32;
        s.stop = if slow { Stop::Unmet(0, 0) } else { Stop::None };
        s.status = if slow {
            Status::Refused
        } else if used < ports[0].rate {
            Status::Starved
        } else {
            Status::Running
        };
    }

    /// Heat in, hotter material out -- and past its melting point it comes out
    /// of the other port, as a fluid.
    ///
    /// This is the one place where a phase change is visible as what it is: the
    /// wire leaving the `molten` port is a different colour and will not plug
    /// into a rolling mill.
    fn furnace(&mut self, i: usize) {
        let ports = parts::part(Kind::Furnace).ports;
        let per = 5u64; // heat per unit of material
        let s = &mut self.st[i];
        let feed = s.buf[1].stuff;
        let mut hot = feed;
        hot.q.temp = (hot.q.temp + parts::FURNACE_LIFT).min(super::stuff::TEMP_MAX);
        let melts = feed.subst.melt() > 0 && hot.q.temp >= feed.subst.melt();
        let out_port = if melts { 3 } else { 2 };

        let by_heat = s.buf[0].qty / per;
        let by_feed = s.buf[1].qty;
        let hot_enough = s.buf[0].stuff.q.temp >= 5;
        let room = if s.buf[out_port].takes(&hot) {
            ports[out_port].cap - s.buf[out_port].qty
        } else {
            0
        };
        let n = if hot_enough {
            by_heat.min(by_feed).min(room).min(ports[1].rate)
        } else {
            0
        };

        let (_, burned) = s.buf[0].take(n * per);
        let (_, fed) = s.buf[1].take(n);
        s.buf[out_port].put(hot, n);
        s.used[0] = burned;
        s.used[1] = fed;
        s.made[out_port] = n;
        s.util = (n * 1000 / ports[1].rate) as u32;
        s.stop = if !hot_enough && s.buf[0].qty > 0 {
            Stop::Unmet(0, 0)
        } else {
            Stop::None
        };
        s.status = if n == ports[1].rate {
            Status::Running
        } else if !hot_enough && s.buf[0].qty > 0 {
            Status::Refused
        } else if room == 0 {
            Status::Blocked
        } else if n == 0 {
            Status::Starved
        } else {
            Status::Starved
        };
    }

    /// Hot crude in; light, middle and heavy out.
    ///
    /// The split is the only thing the player tunes, and more stages is a
    /// better split for more heat -- `separation_quality`, `throughput` and
    /// `energy_required`, and not one differential equation.
    fn column(&mut self, i: usize) {
        let ports = parts::part(Kind::Column).ports;
        let (l, m, h, heat) = parts::column_split(self.tunes[i].stages);
        let s = &mut self.st[i];
        let feed = s.buf[0].stuff;

        let cold = feed.q.temp < parts::COLUMN_FEED_TEMP;
        let wrong = feed.subst != Subst::Crude;
        let weak = s.buf[1].stuff.q.temp < 2;
        let outs = [
            (2usize, l, Stuff::with(Subst::Light, super::stuff::Qual {
                temp: parts::COLUMN_LIGHT_TEMP, purity: 100, ..Default::default() })),
            (3, m, Stuff::with(Subst::Middle, super::stuff::Qual {
                temp: 2, purity: 100, ..Default::default() })),
            (4, h, Stuff::with(Subst::Heavy, super::stuff::Qual {
                temp: 2, purity: 100, ..Default::default() })),
        ];

        let mut n = parts::COLUMN_RATE;
        let mut stop = Stop::None;
        if s.buf[0].qty == 0 {
            n = 0;
            stop = Stop::Short(0);
        } else if cold || wrong {
            n = 0;
            stop = Stop::Unmet(0, if wrong { 1 } else { 0 });
        }
        if s.buf[1].qty == 0 {
            n = 0;
            if stop == Stop::None {
                stop = Stop::Short(1);
            }
        } else if weak {
            n = 0;
            stop = Stop::Unmet(1, 0);
        }
        n = n.min(s.buf[0].qty / parts::COLUMN_BATCH).min(s.buf[1].qty / heat);
        for (port, qty, what) in outs {
            if !s.buf[port].takes(&what) {
                n = 0;
                stop = Stop::Wrong(port);
                continue;
            }
            let by_room = (ports[port].cap - s.buf[port].qty) / qty;
            if by_room < n {
                n = by_room;
                stop = Stop::Full(port);
            }
        }

        if n > 0 {
            let (_, fed) = s.buf[0].take(n * parts::COLUMN_BATCH);
            let (_, burned) = s.buf[1].take(n * heat);
            s.used[0] = fed;
            s.used[1] = burned;
            for (port, qty, what) in outs {
                s.buf[port].put(what, n * qty);
                s.made[port] = n * qty;
            }
        }
        s.stop = stop;
        s.util = (n * 1000 / parts::COLUMN_RATE) as u32;
        s.status = if n == parts::COLUMN_RATE {
            Status::Running
        } else {
            match stop {
                Stop::Below(_) => Status::Stalled,
                Stop::Unmet(..) | Stop::Wrong(_) => Status::Refused,
                Stop::Full(_) => Status::Blocked,
                Stop::Short(_) => Status::Starved,
                Stop::None => Status::Idle,
            }
        };
    }

    // -------------------------------------------------------- the state key

    /// Everything that decides the future, and nothing that merely records the
    /// past. Two ticks with equal keys have identical futures, which is the
    /// entire basis of `orbit`.
    ///
    /// Counters are deliberately absent -- they grow forever, and a machine
    /// whose orbit closed would never be seen to close if they were in here.
    /// So is anything about a *stuff* that is not there: an empty buffer forgets
    /// what it held, or a machine that once carried hot water could never be
    /// equal to the same machine that once carried cold.
    pub fn key(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.st.len() * 32);
        for s in &self.st {
            for b in &s.buf {
                v.extend_from_slice(&b.qty.to_le_bytes());
                v.extend_from_slice(&b.stuff.bytes());
            }
            v.extend_from_slice(&s.age.to_le_bytes());
            v.push(s.spin as u8);
            v.push(s.draining as u8);
            for c in &s.cursor {
                v.push(*c as u8);
            }
        }
        v
    }
}

/// What a ratio does to a speed band. Positive gears down, negative gears up,
/// and it never leaves the band range.
pub fn geared(speed: u8, ratio: i32) -> u8 {
    let r = ratio.clamp(-8, 8);
    let out = if r >= 2 {
        speed as i32 / r
    } else if r <= -2 {
        speed as i32 * -r
    } else {
        speed as i32
    };
    out.clamp(0, super::stuff::SPEED_MAX as i32) as u8
}

/// Which `Need` a component's `Stop::Unmet` refers to, for a caller that wants
/// to print it. Lives here so the furnace and the column -- which are not table
/// rows -- can borrow the same sentence.
pub fn need_of(kind: Kind, d: usize, n: usize) -> Option<&'static Need> {
    parts::part(kind).recipe.and_then(|r| r.draws.get(d)).and_then(|dr| dr.need.get(n))
}

/// Max-min fair allocation of `budget` across `demands`, starting at `cursor`.
///
/// Everybody gets an equal share; anyone who wants less than their share hands
/// the difference back for the others to divide again. What is left over when
/// the budget no longer divides is handed out one unit at a time, starting at
/// the cursor -- so a fan-out of three on a budget of ten is 4,3,3 and then
/// 3,4,3 and then 3,3,4, rather than a permanent favourite.
fn share(budget: u64, demands: &[u64], cursor: u32) -> Vec<u64> {
    let n = demands.len();
    let mut out = vec![0u64; n];
    if n == 0 || budget == 0 {
        return out;
    }
    let order: Vec<usize> = (0..n).map(|i| (i + cursor as usize) % n).collect();
    let mut left = budget;
    loop {
        let active: Vec<usize> =
            order.iter().copied().filter(|&i| out[i] < demands[i]).collect();
        if active.is_empty() || left == 0 {
            break;
        }
        let each = left / active.len() as u64;
        if each == 0 {
            for &i in &active {
                if left == 0 {
                    break;
                }
                out[i] += 1;
                left -= 1;
            }
            break;
        }
        for &i in &active {
            let take = each.min(demands[i] - out[i]);
            out[i] += take;
            left -= take;
        }
    }
    out
}

