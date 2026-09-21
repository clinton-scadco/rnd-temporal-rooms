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
use super::era::{self, Band, Mat};
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
    /// Experiment 14: tripped on its own body temperature, and it will not
    /// restart until it is back inside its operating range. Distinct from
    /// `Stalled` on purpose -- a stalled turbine starts the moment the gas
    /// arrives, and an overheated engine does not start when the steam does.
    Overheated,
    /// Experiment 14: bolted to a frame that will not carry what it is being
    /// shaken by. Nothing about this one is transient: it is wrong in the
    /// document, and it is wrong every tick until the document changes.
    Shaking,
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
            Status::Overheated => "OVERHEATED",
            Status::Shaking => "SHAKING",
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
    /// Experiment 14: heat radiated off warm castings into the air.
    ///
    /// Deliberately *not* added to `heat_wasted`. That column has meant one
    /// thing since experiment 06 -- heat the design threw away on purpose,
    /// through a radiator or out of a leaking pipe -- and folding a motor's
    /// body warmth into it would have quietly re-scored eighteen designs that
    /// had not changed. This is reported beside it, not inside it.
    pub body_heat: u64,
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
    pub body_heat: u128,
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
        self.body_heat += d.body_heat as u128;
        self.util_sum += d.util_sum as u128;
        self.ticks += 1;
        merge(&mut self.took, &d.took, 1);
        merge(&mut self.gave, &d.gave, 1);
        merge(&mut self.lost, &d.lost, 1);
    }

    pub fn plus(mut self, o: &Totals) -> Totals {
        self.power += o.power;
        self.heat_wasted += o.heat_wasted;
        self.body_heat += o.body_heat;
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
            body_heat: self.body_heat * k,
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
            body_heat: self.body_heat - o.body_heat,
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
    /// Experiment 14: heat units held in the component's own body.
    ///
    /// Divided by the part's thermal mass this is its temperature in degrees
    /// above ambient, held exactly. It is an integer for the same reason every
    /// other quantity in this crate is one: `orbit` compiles a design by
    /// noticing its state repeat, and a state with a float in it repeats
    /// approximately, which is to say never.
    pub warmth: u64,
    /// Experiment 14: tripped on temperature, and latched there until the body
    /// is back inside its operating range. The hysteresis is the point -- a
    /// component that cut out at the threshold and cut back in one degree below
    /// it would chatter, and chatter is not a failure mode a player can read.
    pub tripped: bool,
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
    /// Experiment 14: heat that left the body for the air this tick. Reported
    /// rather than scored -- see `Delta::body_heat`.
    pub body: u64,
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
    /// Experiment 14: what each component's frame is made of, how much clear
    /// air it was given, and how hard the drive it is bolted to shakes.
    ///
    /// All three are facts about the *document* rather than about the tick, so
    /// they are worked out once when the machine is built and never again. The
    /// third is the interesting one: vibration travels through rigid rotary
    /// couplings and stops at a belt, so `shake[i]` is the worst thing anywhere
    /// in the rigid cluster component `i` belongs to -- which is why a crusher
    /// three shafts away is still the crusher that pulls a timber mill apart.
    pub mats: Vec<Mat>,
    pub clear: Vec<u32>,
    pub shake: Vec<u8>,
    /// Which rigid cluster each component belongs to, as the index of one
    /// member of it.
    pub group: Vec<usize>,
    /// How much crossed each wire during the tick just simulated, and what it
    /// was. Per wire and not per port, because "which of my three pipes is
    /// actually carrying anything" is the question a player asks first.
    ///
    /// `flow` is what arrived and `lost` is what the run kept, which since the
    /// transport family went is the only place a player can see the price of
    /// distance. It used to be a component's own `waste` column, and it is a
    /// better number here: a wire that loses four a tick is a wire you can
    /// shorten.
    pub flow: Vec<u64>,
    pub lost: Vec<u64>,
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
                    warmth: 0,
                    tripped: false,
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
                    body: 0,
                    util: 0,
                }
            })
            .collect();
        let kinds: Vec<Kind> = d.units.iter().map(|u| u.kind).collect();
        let mats: Vec<Mat> = d.units.iter().map(|u| u.tune.mat).collect();
        let clear: Vec<u32> = (0..n).map(|i| d.clearance(i)).collect();
        let (shake, group) = shake_loads(&kinds, &links);
        Ok(Machine {
            names: d.units.iter().map(|u| u.name.clone()).collect(),
            kinds,
            tunes: d.units.iter().map(|u| u.tune).collect(),
            links,
            out_wires,
            in_wires,
            st,
            mats,
            clear,
            shake,
            group,
            flow: vec![0; nlinks],
            lost: vec![0; nlinks],
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
        self.lost.iter_mut().for_each(|v| *v = 0);
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
                            let room = budget[l.to][l.to_port].min(dp.cap - db.qty);
                            // What the destination can take is not what the
                            // source has to send: the run takes its cut in
                            // between, so ask for the gross. This is where the
                            // whole of the old conduit lives now -- the length
                            // of the connection, priced.
                            let gross = gross_for(room, l.loss_pct);
                            // And a belt carries what a belt carries, however
                            // much either end could manage.
                            l.carries(gross)
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
                    // The offers are gross -- what the far end would send --
                    // and `room` is net, because a port has room for what
                    // arrives rather than for what set out. So the share is
                    // settled in net units and turned back into gross one wire
                    // at a time. Doing it the other way round would let a long
                    // heat main and a short one contend on different scales,
                    // and the long one would win.
                    let demands: Vec<u64> = wires
                        .iter()
                        .map(|&w| net_of(offer[w], self.links[w].loss_pct))
                        .collect();
                    let alloc = share(room, &demands, self.st[u].cursor[p]);
                    for (k, &w) in wires.iter().enumerate() {
                        if alloc[k] == 0 {
                            continue;
                        }
                        let l = self.links[w];
                        // `want` is what may *arrive*; `q` is what has to set
                        // out for that much to arrive.
                        let want = alloc[k];
                        let q = gross_for(want, l.loss_pct).min(offer[w]);
                        if q == 0 {
                            continue;
                        }
                        let src = self.st[l.from].buf[l.from_port].stuff;
                        // Checked again here rather than only in the offer: an
                        // earlier wire may have filled this port with something
                        // else during this very round.
                        if !leaves && !self.st[u].buf[p].takes(&src) {
                            self.st[u].stop = Stop::Wrong(p);
                            continue;
                        }
                        any = true;
                        // `q` is gross -- what leaves the far end. What arrives
                        // is what is left after the run has taken its cut, and
                        // the difference is gone: down a line of bearings, out
                        // of a lagged pipe into the plant, off a slipping belt.
                        let (moved, got) = self.st[l.from].buf[l.from_port].take(q);
                        // Clamped to what was allocated, because both
                        // conversions round in the plant's favour and two
                        // roundings in the same direction would let a wire
                        // deliver one unit more than the port had room for.
                        // The port's budget is unsigned, and a unit over is
                        // not a unit over -- it is eighteen quintillion.
                        let net = net_of(got, l.loss_pct).min(want);
                        let shed = got - net;
                        self.st[l.from].sent[l.from_port] += got;
                        budget[l.from][l.from_port] -= got;
                        if shed > 0 {
                            // Heat lost out of a run is heat wasted; anything
                            // else is a stream that went missing, and the
                            // scoreboard has a column for each. The old
                            // conduit put all of it in `heat_wasted`, which
                            // was wrong about a chute and nobody noticed
                            // because a chute lost nothing.
                            if moved.subst == Subst::Heat {
                                d.heat_wasted += shed;
                            } else {
                                bump(&mut d.lost, moved, shed);
                            }
                        }
                        if leaves {
                            self.leaves(u, moved, net, d);
                        } else {
                            self.st[u].buf[p].put(moved, net);
                        }
                        self.st[u].got[p] += net;
                        budget[u][p] -= net;
                        self.flow[w] += net;
                        self.lost[w] += shed;
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
            // Experiment 14's two gates, and they come first because both of
            // them mean the component does not run at all this tick. A
            // derating would have been softer and would have taught nobody
            // anything: the whole argument of Need since experiment 07 is that
            // a machine which stops and says why is a better teacher than one
            // which quietly runs at forty percent.
            match self.halt(i) {
                Some(why) => self.halted(i, why),
                None => match kind {
                    Kind::Reactor => self.reactor(i, d),
                    Kind::Mains => self.source(i, Subst::Power, d),
                    Kind::Pump => self.source(i, self.tunes[i].subst, d),
                    Kind::Inlet => self.source(i, self.tunes[i].subst, d),
                    Kind::Hopper | Kind::Tank | Kind::Drum | Kind::Flywheel => self.store(i),
                    Kind::Outlet | Kind::Skip | Kind::Radiator => self.dump(i),
                    Kind::Valve | Kind::Clutch => self.limiter(i),
                    Kind::Gearbox => self.gearbox(i, d),
                    Kind::Pulley => self.pulley(i, d),
                    Kind::Turbine => self.turbine(i, d),
                    Kind::Generator => self.generator(i),
                    Kind::Furnace => self.furnace(i),
                    Kind::Column => self.column(i),
                    Kind::Fan => self.fan(i, d),
                    _ => {
                        let r = parts::part(kind).recipe.expect("every other kind is a recipe");
                        self.recipe(i, r);
                    }
                },
            }
            self.perish(i, d);
            self.thermal(i);
            d.body_heat += self.st[i].body;
            d.util_sum += self.st[i].util as u64;
        }
    }

    // --------------------------------------------- experiment 14: the body

    /// The component's body temperature, in degrees above ambient.
    pub fn temp(&self, i: usize) -> u32 {
        let ph = parts::phys(self.kinds[i]);
        if ph.mass == 0 {
            0
        } else {
            (self.st[i].warmth / ph.mass) as u32
        }
    }

    /// Which band it is running in, on the frame it was built on.
    ///
    /// A label rather than a mechanic. What decides output is `duty_at`, which
    /// is a curve; this is what the panel calls the place on it.
    pub fn band(&self, i: usize) -> Band {
        parts::phys(self.kinds[i]).band(self.temp(i), self.mats[i])
    }

    // ------------------------------------------------- who is waiting on whom

    /// The neighbour most likely to be the reason this component is short.
    ///
    /// The port the component itself named, if it named one, and otherwise
    /// whichever input is wired at all. Then the wire into that port that
    /// carried the *least* — because with three feeders on one port, the one
    /// that delivered nothing is the one worth looking at.
    pub fn supplier(&self, i: usize) -> Option<usize> {
        let ports = parts::part(self.kinds[i]).ports;
        let look: Vec<usize> = match self.st[i].stop {
            Stop::Short(p) => vec![p],
            _ => (0..ports.len()).filter(|&p| ports[p].dir == Dir::In).collect(),
        };
        let mut best: Option<(u64, usize)> = None;
        for p in look {
            for &w in self.feeders(i, p) {
                let got = self.flow[w];
                if best.is_none_or(|(b, _)| got < b) {
                    best = Some((got, self.links[w].from));
                }
            }
        }
        best.map(|(_, u)| u)
    }

    /// And the neighbour most likely to be the reason it has nowhere to put
    /// what it made.
    pub fn consumer(&self, i: usize) -> Option<usize> {
        let ports = parts::part(self.kinds[i]).ports;
        let look: Vec<usize> = match self.st[i].stop {
            Stop::Full(p) => vec![p],
            _ => (0..ports.len()).filter(|&p| ports[p].dir == Dir::Out).collect(),
        };
        let mut best: Option<(u64, usize)> = None;
        for p in look {
            for &w in self.drains(i, p) {
                let took = self.flow[w];
                if best.is_none_or(|(b, _)| took < b) {
                    best = Some((took, self.links[w].to));
                }
            }
        }
        best.map(|(_, u)| u)
    }

    /// Follow the wires until the trail stops, and return what is at the end.
    ///
    /// This exists because of the single most common complaint about a plant of
    /// twenty components: fifteen of them say STARVED, none of them says why,
    /// and the one that is actually broken is somewhere in the middle saying
    /// something else entirely.
    ///
    /// STARVED means somebody upstream is not supplying. BLOCKED means somebody
    /// downstream is not taking. Both are *symptoms*, and both point in a known
    /// direction, so a cascade is a path and the fault is at the end of it. The
    /// walk stops at the first component that is neither — which is either the
    /// real fault (refused, stalled, overheated, shaking, idle) or a component
    /// running flat out, which is its own answer: there is simply less of the
    /// stuff than this wanted.
    ///
    /// `None` when the component is the end of its own trail, which is the case
    /// worth saying nothing about.
    pub fn root_cause(&self, i: usize) -> Option<usize> {
        let mut seen = vec![false; self.len()];
        seen[i] = true;
        let mut at = i;
        // Bounded by the component count: a cycle cannot outrun the `seen` set,
        // and neither can a chain.
        for _ in 0..self.len() {
            let next = match self.st[at].status {
                Status::Starved => self.supplier(at),
                Status::Blocked => self.consumer(at),
                _ => break,
            };
            match next {
                Some(n) if !seen[n] => {
                    seen[n] = true;
                    at = n;
                }
                _ => break,
            }
        }
        (at != i).then_some(at)
    }

    /// The input ports this component genuinely cannot run without.
    ///
    /// A recipe says so itself. For the hand-written ones it is every input
    /// that is not a boundary — a skip has three and is waiting for none of
    /// them, and saying it was waiting for all three would be worse than saying
    /// nothing.
    pub fn needed_inputs(&self, i: usize) -> Vec<usize> {
        let part = parts::part(self.kinds[i]);
        if let Some(r) = part.recipe {
            let mut v: Vec<usize> = r.draws.iter().map(|d| d.port).collect();
            v.dedup();
            return v;
        }
        match self.kinds[i] {
            Kind::Outlet | Kind::Skip | Kind::Radiator => Vec::new(),
            _ => part
                .ports
                .iter()
                .enumerate()
                .filter(|(_, q)| q.dir == Dir::In)
                .map(|(p, _)| p)
                .collect(),
        }
    }

    /// Which component in `i`'s rigid cluster is the one doing the shaking, so
    /// that a panel can name it rather than describe it.
    pub fn shaker(&self, i: usize) -> Option<usize> {
        if self.shake[i] == 0 {
            return None;
        }
        (0..self.len())
            .find(|&j| self.group[j] == self.group[i] && parts::phys(self.kinds[j]).vib == self.shake[i])
    }

    /// Per mille of rated output this component may do this tick.
    ///
    /// One thousand for anything without a body, which is most of the
    /// catalogue and every design written before experiment 14 -- so the whole
    /// mechanic is arithmetically invisible until somebody builds something
    /// that gets hot.
    pub fn duty(&self, i: usize) -> u64 {
        if !parts::phys(self.kinds[i]).thermal() {
            return 1000;
        }
        if self.st[i].tripped {
            return 0;
        }
        parts::phys(self.kinds[i]).duty_at(self.temp(i), self.mats[i])
    }

    /// A rated figure, derated by where on the duty curve the body is.
    fn derate(&self, i: usize, rated: u64) -> u64 {
        let q = self.duty(i);
        if q >= 1000 {
            return rated;
        }
        rated * q / 1000
    }

    /// Why this component is not going to run at all this tick, if it is not.
    ///
    /// Two reasons, and they are the two experiment 14 added. Both are checked
    /// before anything else because both mean the same thing: the machine is
    /// there, it is wired correctly, its inputs have arrived, and it is not
    /// going to move.
    fn halt(&self, i: usize) -> Option<Status> {
        if self.shake[i] > self.mats[i].tol() {
            return Some(Status::Shaking);
        }
        if parts::phys(self.kinds[i]).thermal() && self.st[i].tripped {
            return Some(Status::Overheated);
        }
        None
    }

    fn halted(&mut self, i: usize, why: Status) {
        let s = &mut self.st[i];
        s.util = 0;
        s.status = why;
        s.stop = Stop::None;
    }

    /// Heat in, heat out, and a body somewhere in between.
    ///
    /// Run after the component, so the heat is made by the work that was
    /// actually done and the band it decides governs the *next* tick. That
    /// ordering is not an implementation detail. It is what makes a temperature
    /// something a player watches climb towards a threshold rather than
    /// something that has already happened by the time it is drawn.
    fn thermal(&mut self, i: usize) {
        let kind = self.kinds[i];
        let ph = parts::phys(kind);
        self.st[i].body = 0;
        if !ph.thermal() {
            return;
        }
        self.st[i].warmth += ph.heat * self.st[i].util as u64 / 1000;

        // Out through the waste port, if there is one, the player wired it to
        // something, and the body is warm enough for what comes off it to be
        // worth anything. Unwired, it is not a hole in the casing: the heat
        // stays in the body and the air is the only way out.
        //
        // The temperature floor is not fussiness. Without it a cold machine
        // pushes grade-0 heat into the pipe on the first tick, and a jacket
        // downstream -- which will not touch heat that cold -- ends up holding
        // a full buffer of it forever, refusing, while the genuinely hot heat
        // that arrives later has nowhere to blend into. One tick of startup
        // poisons the cooling loop for the rest of the run.
        if let Some(p) = kind.waste_port() {
            if !self.out_wires[i][p].is_empty() && self.temp(i) >= era::GRADE_MIN {
                let port = &parts::part(kind).ports[p];
                let room = port.cap - self.st[i].buf[p].qty;
                let take = self
                    .st[i]
                    .warmth
                    .min(port.rate)
                    .min(room)
                    .min(era::wasteable(self.temp(i)));
                let grade = era::grade(self.temp(i));
                let heat = Stuff::with(
                    Subst::Heat,
                    super::stuff::Qual { temp: grade, purity: 100, ..Default::default() },
                );
                if take > 0 && self.st[i].buf[p].takes(&heat) {
                    self.st[i].warmth -= take;
                    self.st[i].buf[p].put(heat, take);
                    self.st[i].made[p] += take;
                }
            }
        }

        // And to the air, which is free, weak, and the only cooling a design
        // gets without deciding to have any.
        let out = era::shed(self.temp(i), self.mats[i], self.clear[i]).min(self.st[i].warmth);
        self.st[i].warmth -= out;
        self.st[i].body = out;

        // The latch. Trips at the ceiling, clears only once the body is back
        // inside the operating range.
        let t = self.temp(i);
        let mat = self.mats[i];
        if self.st[i].tripped {
            if t <= ph.hi {
                self.st[i].tripped = false;
            }
        } else if ph.band(t, mat) == Band::Overheated {
            self.st[i].tripped = true;
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
    /// components are this function and a table entry, so adding a
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
        // Experiment 14: a warm component is a slower component, and a cold one
        // is a component that has not been run in. The band is the only thing
        // that changes here, and `derate` is 1:1 for everything without a body,
        // which is most of the catalogue and all of the older designs.
        let rated = self.derate(i, r.rate);
        let mut n = rated;
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

        let capped = rated < r.rate && n == rated && n > 0;
        let s = &mut self.st[i];
        s.util = (n * 1000 / r.rate) as u32;
        s.stop = stop;
        s.status = if n == r.rate || capped {
            // Doing everything its temperature allows is not the same as being
            // short of anything, and calling it STARVED would send the player
            // looking for a supply problem that is not there.
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

    /// A valve or a clutch: a threshold, stated as a number, doing exactly what
    /// it says.
    ///
    /// The clutch is the one worth having. It will not engage until its
    /// threshold has gathered, which lets one stuttering drive turn something
    /// that must not be turned slowly -- the rotary equivalent of a pulsed
    /// tank, and the reason one drive with six things hanging off it does not
    /// have to be sized for the worst tick.
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
        let rate = self.derate(i, ports[0].rate);
        let s = &mut self.st[i];
        let have = s.buf[0].qty;
        let mut what = s.buf[0].stuff;
        what.q.speed = geared(what.q.speed, ratio);
        let room = if s.buf[1].takes(&what) { ports[1].cap - s.buf[1].qty } else { 0 };
        let take = have.min(room * 100 / (100 - parts::GEARBOX_LOSS_PCT)).min(rate);
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
        let rate = self.derate(i, ports[0].rate);
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

        let mut intake = if stalled { 0 } else { have.min(rate) };
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
        } else if room == 0 || used < have.min(rate) {
            Status::Blocked
        } else if used < rate {
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
        let rate = self.derate(i, ports[0].rate);
        let s = &mut self.st[i];
        let slow = s.buf[0].qty > 0 && s.buf[0].stuff.q.speed < parts::GENERATOR_MIN_SPEED;
        let intake = if slow { 0 } else { s.buf[0].qty.min(rate) };
        let (_, used) = s.buf[0].take(intake);
        let mw = used * parts::GENERATOR_EFF / 100;
        s.buf[1].put(Stuff::fresh(Subst::Power), mw);
        s.used[0] = used;
        s.made[1] = mw;
        s.util = (used * 1000 / ports[0].rate) as u32;
        s.stop = if slow { Stop::Unmet(0, 0) } else { Stop::None };
        s.status = if slow {
            Status::Refused
        } else if used < rate {
            Status::Starved
        } else {
            Status::Running
        };
    }

    /// A pair of pulleys and the belt over them: a ratio, in timber, that lets
    /// go if it is asked for too much.
    ///
    /// The gearbox's poor relation on purpose. It costs three times as much to
    /// pass power through, and above `PULLEY_SLIP` it simply does not pass it:
    /// the belt slips, the surplus is gone, and nothing downstream gets any of
    /// it. That is the first era's ceiling, and it is not a number on a
    /// scoreboard -- it is the reason a water-driven plant is a row of small
    /// machines on one long shaft rather than one large machine.
    fn pulley(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(Kind::Pulley).ports;
        let ratio = self.tunes[i].ratio;
        let s = &mut self.st[i];
        let have = s.buf[0].qty;
        let mut what = s.buf[0].stuff;
        what.q.speed = geared(what.q.speed, ratio);
        let room = if s.buf[1].takes(&what) { ports[1].cap - s.buf[1].qty } else { 0 };
        let grip = parts::PULLEY_SLIP.min(ports[0].rate);
        let take = have.min(room * 100 / (100 - parts::PULLEY_LOSS_PCT)).min(grip);
        let net = (take - take * parts::PULLEY_LOSS_PCT / 100).min(room);
        let (_, got) = s.buf[0].take(take);
        // Whatever was offered above the grip does not queue on the pulley. A
        // belt that is slipping is losing the surplus to friction, every tick,
        // for as long as the drive keeps offering it.
        let slipped = if have > grip { s.buf[0].take(have - grip).1 } else { 0 };
        s.buf[1].put(what, net);
        s.used[0] = got;
        s.made[1] = net;
        s.waste = got - net + slipped;
        s.util = (net * 1000 / ports[1].rate) as u32;
        s.status = if have == 0 {
            Status::Idle
        } else if slipped > 0 {
            Status::Venting
        } else if net == 0 {
            Status::Blocked
        } else {
            Status::Running
        };
        d.heat_wasted += got - net + slipped;
    }

    /// Forced air: heat out, for power in, and none at all without it.
    ///
    /// The one cooling component that can fail. A radiator is a lump of metal
    /// and works whether or not anything else does; a fan is a motor, and a
    /// design that cools its engine with one has coupled its engine to its
    /// grid connection whether it meant to or not. That coupling is the whole
    /// reason it is a separate component and not a bigger radiator.
    fn fan(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(Kind::Fan).ports;
        let s = &mut self.st[i];
        let mw = s.buf[0].qty.min(ports[0].rate);
        let can = mw * parts::FAN_PER_MW;
        let moved = s.buf[1].qty.min(can).min(ports[1].rate);
        // Power is drawn for the air actually moved, rounded up, so a fan with
        // nothing to cool is not billed for standing still.
        let spent = (moved + parts::FAN_PER_MW - 1) / parts::FAN_PER_MW;
        let (_, used) = s.buf[0].take(spent);
        let (what, got) = s.buf[1].take(moved);
        s.used[0] = used;
        s.used[1] = got;
        s.waste = got;
        s.util = (got * 1000 / ports[1].rate) as u32;
        s.status = if got > 0 {
            Status::Running
        } else if s.buf[1].qty > 0 && mw == 0 {
            Status::Starved
        } else {
            Status::Idle
        };
        if got > 0 {
            if what.subst == Subst::Heat {
                d.heat_wasted += got;
            } else {
                bump(&mut d.lost, what, got);
            }
        }
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
            // Experiment 14. A body temperature is part of the future -- it is
            // what decides next tick's duty -- so it belongs here, and it is an
            // integer precisely so that it can. `warmth` is bounded by the
            // point at which shedding matches generation, so the state space
            // stays finite and an orbit can still close on it.
            v.extend_from_slice(&s.warmth.to_le_bytes());
            v.push(s.tripped as u8);
            for c in &s.cursor {
                v.push(*c as u8);
            }
        }
        v
    }
}

/// How hard the drive each component is bolted to shakes.
///
/// Vibration travels through rigid rotary connections -- a shaft and a pair of
/// couplings, which is what a rotary wire is unless it is told otherwise -- and
/// stops dead at a belt, because a belt is slack. So the question "will this
/// hold together" is not about one component, it is about the *rigid cluster*
/// it belongs to: the worst offender anywhere in it is what every frame in it
/// has to carry.
///
/// That one rule is the difference between the three eras being three drive
/// trains and the three eras being three sprites. A crusher shakes at 7. Timber
/// rates 4. Steel rates 9. So the third era bolts a motor straight on and never
/// thinks about it, the second era gets away with cast iron, and the first era
/// has to put a belt between the crusher and everything it owns -- which is not
/// a stat, it is a shape on the ground.
///
/// The belt used to be a five-tile component and is now a word on the wire,
/// and the rule got *shorter* as a result: it is no longer "skip this link if
/// either end happens to be a belt", which was a statement about two
/// components pretending to be a statement about a connection. It is "skip
/// this link if it is slack", which is what was always meant.
/// Returns the load on each component and which rigid cluster it is in, so a
/// panel can both judge a frame and name the thing that is shaking it.
pub fn shake_loads(kinds: &[Kind], links: &[Link]) -> (Vec<u8>, Vec<usize>) {
    let n = kinds.len();
    let mut parent: Vec<usize> = (0..n).collect();
    fn find(parent: &mut Vec<usize>, mut i: usize) -> usize {
        while parent[i] != i {
            parent[i] = parent[parent[i]];
            i = parent[i];
        }
        i
    }
    for l in links {
        if parts::part(kinds[l.from]).ports[l.from_port].dom != era::SHAKE_DOMAIN {
            continue;
        }
        if l.belt {
            continue;
        }
        let (a, b) = (find(&mut parent, l.from), find(&mut parent, l.to));
        if a != b {
            parent[a] = b;
        }
    }
    let mut worst = vec![0u8; n];
    for i in 0..n {
        let r = find(&mut parent, i);
        worst[r] = worst[r].max(parts::phys(kinds[i]).vib);
    }
    let group: Vec<usize> = (0..n).map(|i| find(&mut parent, i)).collect();
    let load = group.iter().map(|&r| worst[r]).collect();
    (load, group)
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

/// What arrives when `gross` sets out down a run that loses `pct`.
///
/// Integer, and rounded in the plant's favour: a run that loses one percent of
/// forty carries forty, because `40 * 1 / 100` is zero. That is deliberate.
/// The alternative is to round the loss up, and then every short connection in
/// a small design bleeds a unit a tick for no reason a player could see.
pub fn net_of(gross: u64, pct: u64) -> u64 {
    gross - gross * pct / 100
}

/// What has to set out for `net` to arrive. The inverse of `net_of`, rounded
/// up, so asking for what a port has room for never under-fills it.
pub fn gross_for(net: u64, pct: u64) -> u64 {
    if pct == 0 || net == 0 {
        return net;
    }
    let keep = 100 - pct.min(99);
    (net * 100).div_ceil(keep)
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

