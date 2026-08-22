//! The machine, one tick at a time.
//!
//! Every component is a state machine and every connection carries a
//! deterministic quantity, exactly as the rest of this crate does it. Nothing
//! here averages anything: a design that starves for eleven ticks and then
//! boils for three starves for eleven ticks and then boils for three, and the
//! only reason the answer at tick 10^9 is cheap is that `orbit` later notices
//! the machine repeating itself.
//!
//! # A tick
//!
//! ```text
//!   1. transfer   move quantities along wires, obeying both ends
//!   2. step       every component consumes its inputs and fills its outputs
//! ```
//!
//! In that order, which is the whole latency model: a quantity a component puts
//! into an output buffer during step *t* cannot move until the transfer at
//! *t+1*, so every hop costs a tick and every pipe costs two. Nobody had to
//! write a delay line.
//!
//! # Contention
//!
//! v2 of the solver learned this the hard way, so this experiment does not
//! repeat the mistake: when several wires want the same throughput, the split
//! is a policy and the policy is *stated*. It is max-min fair -- everyone gets
//! an equal share, anyone who wants less than their share frees the difference
//! for the rest -- and the remainder rotates on a cursor that is part of the
//! machine's state. That last detail is why a fan-out of three on a budget of
//! ten has a period of three rather than a permanent favourite.
//!
//! # Waste
//!
//! Only heat-typed losses count as *heat wasted*: what a reactor vents because
//! nobody took it, and what a heat pipe leaks. The turbine's 25% and the
//! generator's 10% are conversion efficiencies rather than mistakes, and
//! reporting them as waste would tell the player to stop building turbines.
//! Steam that reaches a stalled turbine and condenses *is* a mistake, so it is
//! reported, separately, in steam.

use super::design::{Design, Link, Tune};
use super::parts::{self, Dir, Kind};

pub type Tick = u64;

/// How many offer/accept rounds the transfer stage runs.
///
/// One round can leave throughput stranded: a source splits its budget between
/// two destinations, one of them turns out to be full, and the share it was
/// offered goes nowhere. A second round hands that back out. Three is well past
/// the point where a graph this size still has anything left to reallocate.
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
    /// A pulse tank holding on to steam on purpose.
    Filling,
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
        }
    }
    /// Whether this is the status of a component that is doing its job.
    pub fn well(self) -> bool {
        matches!(self, Status::Running | Status::Warming | Status::Filling)
    }
}

/// What one tick did to the machine as a whole.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Delta {
    pub power: u64,
    pub fuel: u64,
    pub water: u64,
    pub heat_wasted: u64,
    pub steam_vented: u64,
    /// Utilisation summed over components, in per mille each.
    pub util_sum: u64,
}

/// Everything since tick 0. `u128` because a million ticks of a large plant
/// is past what a `u64` of MW-ticks would survive being multiplied into.
#[derive(Clone, Copy, Debug, Default, PartialEq, Eq)]
pub struct Totals {
    pub power: u128,
    pub fuel: u128,
    pub water: u128,
    pub heat_wasted: u128,
    pub steam_vented: u128,
    pub util_sum: u128,
    pub ticks: u128,
}

impl Totals {
    pub fn add(&mut self, d: &Delta) {
        self.power += d.power as u128;
        self.fuel += d.fuel as u128;
        self.water += d.water as u128;
        self.heat_wasted += d.heat_wasted as u128;
        self.steam_vented += d.steam_vented as u128;
        self.util_sum += d.util_sum as u128;
        self.ticks += 1;
    }

    pub fn plus(mut self, o: &Totals) -> Totals {
        self.power += o.power;
        self.fuel += o.fuel;
        self.water += o.water;
        self.heat_wasted += o.heat_wasted;
        self.steam_vented += o.steam_vented;
        self.util_sum += o.util_sum;
        self.ticks += o.ticks;
        self
    }

    pub fn scaled(&self, k: u128) -> Totals {
        Totals {
            power: self.power * k,
            fuel: self.fuel * k,
            water: self.water * k,
            heat_wasted: self.heat_wasted * k,
            steam_vented: self.steam_vented * k,
            util_sum: self.util_sum * k,
            ticks: self.ticks * k,
        }
    }

    pub fn minus(&self, o: &Totals) -> Totals {
        Totals {
            power: self.power - o.power,
            fuel: self.fuel - o.fuel,
            water: self.water - o.water,
            heat_wasted: self.heat_wasted - o.heat_wasted,
            steam_vented: self.steam_vented - o.steam_vented,
            util_sum: self.util_sum - o.util_sum,
            ticks: self.ticks - o.ticks,
        }
    }
}

/// One component's live state.
#[derive(Clone, Debug)]
pub struct UnitState {
    /// One buffer per port, in the part's port order.
    pub buf: Vec<u64>,
    /// Ticks since the machine started, clamped at `WARMUP`. Reactors only.
    pub age: u64,
    /// Turbines only, `0..=SPIN_MAX`.
    pub spin: u32,
    /// Pulse tanks only: emptying rather than filling.
    pub draining: bool,
    /// Fair-share rotation, one per port, kept modulo that port's wire count so
    /// that the state space stays finite and an orbit can close.
    pub cursor: Vec<u32>,

    // ------- what happened during the tick just simulated, for the inspector
    pub status: Status,
    /// Arrived along wires, per port.
    pub got: Vec<u64>,
    /// Left along wires, per port.
    pub sent: Vec<u64>,
    /// Drawn out of an input buffer by the component itself.
    pub used: Vec<u64>,
    /// Put into an output buffer by the component itself.
    pub made: Vec<u64>,
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
    /// How much crossed each wire during the tick just simulated. Per wire and
    /// not per port, because "which of my three pipes is actually carrying
    /// anything" is the question a player asks first.
    pub flow: Vec<u64>,
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
                let np = parts::part(u.kind).ports.len();
                UnitState {
                    buf: vec![0; np],
                    age: 0,
                    spin: 0,
                    draining: false,
                    cursor: vec![0; np],
                    status: Status::Idle,
                    got: vec![0; np],
                    sent: vec![0; np],
                    used: vec![0; np],
                    made: vec![0; np],
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
            s.waste = 0;
        }
        self.flow.iter_mut().for_each(|v| *v = 0);
        self.transfer();
        let d = self.run_units();
        self.rotate();
        self.tick += 1;
        self.last = d;
        d
    }

    /// Move quantities along the wires.
    fn transfer(&mut self) {
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
                    let avail = budget[u][p].min(self.st[u].buf[p]);
                    if avail == 0 {
                        continue;
                    }
                    // What each destination could still swallow. Using the
                    // destination's room as the demand is what stops a source
                    // from committing its whole budget to a full neighbour.
                    let wires = self.out_wires[u][p].clone();
                    let demands: Vec<u64> = wires
                        .iter()
                        .map(|&w| {
                            let l = self.links[w];
                            let dp = &parts::part(self.kinds[l.to]).ports[l.to_port];
                            budget[l.to][l.to_port].min(dp.cap - self.st[l.to].buf[l.to_port])
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
                    let room = budget[u][p].min(ports[p].cap - self.st[u].buf[p]);
                    if room == 0 {
                        continue;
                    }
                    let wires = self.in_wires[u][p].clone();
                    let demands: Vec<u64> = wires.iter().map(|&w| offer[w]).collect();
                    let alloc = share(room, &demands, self.st[u].cursor[p]);
                    for (k, &w) in wires.iter().enumerate() {
                        let q = alloc[k];
                        if q == 0 {
                            continue;
                        }
                        any = true;
                        let l = self.links[w];
                        self.st[l.from].buf[l.from_port] -= q;
                        self.st[l.from].sent[l.from_port] += q;
                        budget[l.from][l.from_port] -= q;
                        self.st[u].buf[p] += q;
                        self.st[u].got[p] += q;
                        budget[u][p] -= q;
                        self.flow[w] += q;
                    }
                }
            }
            if !any {
                break;
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
    fn run_units(&mut self) -> Delta {
        let mut d = Delta::default();
        for i in 0..self.st.len() {
            match self.kinds[i] {
                Kind::Reactor => self.reactor(i, &mut d),
                Kind::HeatPipe => self.pipe(i, parts::PIPE_LOSS_PCT, &mut d),
                Kind::SteamPipe => self.pipe(i, 0, &mut d),
                Kind::Pump => self.pump(i, &mut d),
                Kind::Exchanger => self.exchanger(i),
                Kind::Tank => self.tank(i),
                Kind::Turbine => self.turbine(i, &mut d),
                Kind::Generator => self.generator(i, &mut d),
            }
            d.util_sum += self.st[i].util as u64;
        }
        d
    }

    // ------------------------------------------------------------ the parts

    fn reactor(&mut self, i: usize, d: &mut Delta) {
        let thr = self.tunes[i].throttle.clamp(parts::MIN_THROTTLE, 100) as u64;
        let ramp = self.st[i].age.min(parts::WARMUP);
        let made = parts::REACTOR_HEAT * thr / 100 * ramp / parts::WARMUP;
        let cap = parts::part(Kind::Reactor).ports[0].cap;
        let room = cap - self.st[i].buf[0];
        let into = made.min(room);
        let vented = made - into;

        // Fuel burns at the throttle setting from the first tick, warm or not.
        // Nothing about a fire cares whether the boiler is ready.
        d.fuel += parts::REACTOR_FUEL * thr / 100;
        d.heat_wasted += vented;

        let s = &mut self.st[i];
        s.buf[0] += into;
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

    fn pipe(&mut self, i: usize, loss_pct: u64, d: &mut Delta) {
        let ports = parts::part(self.kinds[i]).ports;
        let cap_out = ports[1].cap;
        let have = self.st[i].buf[0];
        let room = cap_out - self.st[i].buf[1];

        // Take as much as will still fit once the leak has taken its cut.
        let mut take = have.min(room * 100 / (100 - loss_pct));
        let mut net = take - take * loss_pct / 100;
        if net > room {
            take = room;
            net = take - take * loss_pct / 100;
        }

        d.heat_wasted += take - net;
        let s = &mut self.st[i];
        s.buf[0] -= take;
        s.buf[1] += net;
        s.used[0] = take;
        s.made[1] = net;
        s.waste = take - net;
        s.util = (net * 1000 / ports[1].rate) as u32;
        s.status = if have == 0 {
            Status::Idle
        } else if take < have {
            Status::Blocked
        } else {
            Status::Running
        };
    }

    fn pump(&mut self, i: usize, d: &mut Delta) {
        let p = &parts::part(Kind::Pump).ports[0];
        let room = p.cap - self.st[i].buf[0];
        let made = p.rate.min(room);
        // Water is counted where it is taken out of the world, not where it
        // boils: a design that fills a buffer it never uses has still used it.
        d.water += made;
        let s = &mut self.st[i];
        s.buf[0] += made;
        s.made[0] = made;
        s.util = (made * 1000 / p.rate) as u32;
        s.status = if made == 0 { Status::Blocked } else { Status::Running };
    }

    fn exchanger(&mut self, i: usize) {
        let ports = parts::part(Kind::Exchanger).ports;
        let per_tick = ports[2].rate / parts::BOIL_STEAM; // batches at full rate
        let s = &mut self.st[i];
        let room = ports[2].cap - s.buf[2];
        let by_heat = s.buf[0] / parts::BOIL_HEAT;
        let by_water = s.buf[1] / parts::BOIL_WATER;
        let by_room = room / parts::BOIL_STEAM;
        let n = by_heat.min(by_water).min(by_room).min(per_tick);

        s.buf[0] -= n * parts::BOIL_HEAT;
        s.buf[1] -= n * parts::BOIL_WATER;
        s.buf[2] += n * parts::BOIL_STEAM;
        s.used[0] = n * parts::BOIL_HEAT;
        s.used[1] = n * parts::BOIL_WATER;
        s.made[2] = n * parts::BOIL_STEAM;
        s.util = (n * parts::BOIL_STEAM * 1000 / ports[2].rate) as u32;
        s.status = if n == per_tick {
            Status::Running
        } else if by_room == n && by_room < by_heat.min(by_water) {
            Status::Blocked
        } else {
            Status::Starved
        };
    }

    fn tank(&mut self, i: usize) {
        let ports = parts::part(Kind::Tank).ports;
        let t = self.tunes[i];
        let s = &mut self.st[i];
        let level = s.buf[0];
        // Hysteresis, and the reason a Steam Buffer is not just a bigger pipe:
        // a tank that fills quietly and empties hard can push a turbine over a
        // threshold that a steady trickle never would.
        //
        // The low-water mark is tested *after* the release, not before. Tested
        // before, a tank that is being refilled at the same time it drains
        // never sees itself empty -- it latches open on the first pulse and
        // spends the rest of the run being an expensive pipe.
        if t.pulse {
            if !s.draining && level >= t.high {
                s.draining = true;
            }
        } else {
            s.draining = true;
        }
        let room = ports[1].cap - s.buf[1];
        let moved = if s.draining { level.min(ports[1].rate).min(room) } else { 0 };
        s.buf[0] -= moved;
        if t.pulse && s.draining && s.buf[0] <= t.low {
            s.draining = false;
        }
        s.buf[1] += moved;
        s.used[0] = moved;
        s.made[1] = moved;
        s.util = (moved * 1000 / ports[1].rate) as u32;
        s.status = if moved > 0 {
            Status::Running
        } else if level == 0 {
            Status::Idle
        } else if s.draining {
            Status::Blocked
        } else {
            Status::Filling
        };
    }

    fn turbine(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(Kind::Turbine).ports;
        let s = &mut self.st[i];
        let have = s.buf[0];
        let room = ports[1].cap - s.buf[1];
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
            // Back-pressure: take only as much steam as the shaft can pass on.
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
        // condenses, which is exactly why a Steam Buffer upstream is worth its
        // nine tiles.
        s.buf[0] -= intake;
        let vented = s.buf[0];
        s.buf[0] = 0;
        s.buf[1] += made;
        s.used[0] = intake;
        s.made[1] = made;
        s.waste = vented;
        s.spin = spin;
        s.util = (intake * 1000 / ports[0].rate) as u32;
        s.status = if stalled {
            Status::Stalled
        } else if room == 0 || intake < have.min(ports[0].rate) {
            Status::Blocked
        } else if intake < ports[0].rate {
            Status::Starved
        } else {
            Status::Running
        };
        d.steam_vented += vented;
    }

    fn generator(&mut self, i: usize, d: &mut Delta) {
        let ports = parts::part(Kind::Generator).ports;
        let s = &mut self.st[i];
        let intake = s.buf[0].min(ports[0].rate);
        let mw = intake * parts::GENERATOR_EFF / 100;
        s.buf[0] -= intake;
        s.used[0] = intake;
        s.made[1] = mw;
        s.sent[1] = mw; // electricity leaves the machine the moment it exists
        s.util = (intake * 1000 / ports[0].rate) as u32;
        s.status = if intake == 0 {
            Status::Starved
        } else if intake < ports[0].rate {
            Status::Starved
        } else {
            Status::Running
        };
        d.power += mw;
    }

    // -------------------------------------------------------- the state key

    /// Everything that decides the future, and nothing that merely records the
    /// past. Two ticks with equal keys have identical futures, which is the
    /// entire basis of `orbit`.
    ///
    /// Counters are deliberately absent -- they grow forever, and a machine
    /// whose orbit closed would never be seen to close if they were in here.
    pub fn key(&self) -> Vec<u8> {
        let mut v = Vec::with_capacity(self.st.len() * 24);
        for s in &self.st {
            for b in &s.buf {
                v.extend_from_slice(&b.to_le_bytes());
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
