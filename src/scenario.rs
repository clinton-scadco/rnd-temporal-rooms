//! Pressure: a budget, an order, and a deadline.
//!
//! Prototype 0 could simulate a factory and Prototype 1 can change one while
//! it runs, and neither of those gives a player any reason to care whether the
//! plant works. A simulator becomes a problem the moment somebody wants
//! something out of it that it cannot currently deliver.
//!
//! # This is not physics and does not get to pretend it is
//!
//! Everything in this module is a *game rule*. What a smelter costs, whether
//! demolition refunds anything, how long you have: these are design decisions,
//! they will be wrong, and they will be changed by someone with a spreadsheet
//! rather than by someone with a proof. So they live in a file beside the
//! plant instead of inside it, they are parsed by their own parser, and the
//! solver never hears about any of it. `configs/p1-gears.factory` is a plant.
//! `scenarios/first-gears.scenario` is a *problem posed about* that plant, and
//! the plant runs identically with the scenario deleted.
//!
//! ```text
//!   scenario  ->  reads   ->  the plant, the log, the counters
//!   scenario  ->  decides ->  affordable / met / failed
//!   scenario  ->  changes ->  nothing
//! ```
//!
//! # What "delivered" means
//!
//! A sink is the only way anything leaves a factory, so an order counts what
//! sinks have swallowed -- exactly `cycles x batch`, summed over the sink
//! classes that consume the item. That is a different number from the item's
//! total consumption, and deliberately: a gear press consuming plates is not a
//! delivery of plates, and an order that counted it would be satisfiable by
//! building a machine that eats its own supply chain.
//!
//! # What money buys
//!
//! The plant you are given is free; the budget buys *changes*. Spending is
//! computed over the log, one command at a time, as the increase in what the
//! plant is worth -- so demolishing refunds nothing and rebuilding costs
//! again. That is a harsh rule and it is at least a clear one, which is what a
//! rule has to be while the numbers are still guesses.

use crate::graph::{Graph, Kind, Node};
use crate::json::Json;
use crate::live::{At, Fault, Log};
use crate::model::*;
use std::collections::HashMap;

// ==================================================================== costs

/// What things cost. Every field is per *member*, not per placed object: a
/// `Smelter x40` is forty smelters and is priced as forty smelters, because
/// the whole point of the population model is that the object on the canvas is
/// a bookkeeping convenience.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Costs {
    pub source: u64,
    pub process: u64,
    pub sink: u64,
    /// Per vehicle.
    pub link: u64,
    /// Per `storage_per` units of capacity, rounded up: a bay is priced by how
    /// much it holds, which is the only property of a bay a player chooses.
    pub storage: u64,
    pub storage_per: u64,
    pub wire: u64,
}

impl Default for Costs {
    fn default() -> Costs {
        Costs {
            source: 200,
            process: 120,
            sink: 80,
            link: 60,
            storage: 40,
            storage_per: 1_000,
            wire: 5,
        }
    }
}

impl Costs {
    pub fn of_node(&self, n: &Node) -> u64 {
        match n.kind {
            Kind::Storage => {
                let per = self.storage_per.max(1);
                self.storage * n.capacity.div_ceil(per)
            }
            Kind::Source => self.source * n.count,
            Kind::Process => self.process * n.count,
            Kind::Sink => self.sink * n.count,
            Kind::Link => self.link * n.count,
        }
    }

    pub fn of_graph(&self, g: &Graph) -> u64 {
        g.nodes.iter().map(|n| self.of_node(n)).sum::<u64>()
            + self.wire * g.edges.len() as u64
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("source", Json::big(self.source as u128))
            .set("process", Json::big(self.process as u128))
            .set("sink", Json::big(self.sink as u128))
            .set("link", Json::big(self.link as u128))
            .set("storage", Json::big(self.storage as u128))
            .set("storagePer", Json::big(self.storage_per as u128))
            .set("wire", Json::big(self.wire as u128))
    }
}

/// What a log has spent, and on what.
#[derive(Clone, Debug, Default)]
pub struct Spend {
    pub total: u64,
    /// One entry per command that cost anything.
    pub items: Vec<(Tick, String, u64)>,
    /// The first command the budget could not pay for, if there is one.
    pub overspent_at: Option<Tick>,
}

// ================================================================== orders

/// Something the plant is required to do.
#[derive(Clone, Debug, PartialEq)]
pub enum Order {
    /// `deliver 12000 Gear by 60000`
    Deliver { qty: u64, item: String, by: Tick },
    /// `sustain 20 Gear per 100 ticks from 40000 to 60000`
    ///
    /// A rate rather than a total, because a plant that delivers everything in
    /// one enormous burst from a warehouse has not solved the problem the
    /// order was posed about.
    Sustain { qty: u64, per: Tick, item: String, from: Tick, to: Tick },
}

impl Order {
    pub fn item(&self) -> &str {
        match self {
            Order::Deliver { item, .. } | Order::Sustain { item, .. } => item,
        }
    }

    /// Every tick this order has an opinion about.
    pub fn probes(&self) -> Vec<Tick> {
        match *self {
            Order::Deliver { by, .. } => vec![by],
            Order::Sustain { from, to, .. } => vec![from, to],
        }
    }

    pub fn deadline(&self) -> Tick {
        match *self {
            Order::Deliver { by, .. } => by,
            Order::Sustain { to, .. } => to,
        }
    }

    pub fn text(&self) -> String {
        match self {
            Order::Deliver { qty, item, by } => {
                format!("deliver {} {item} by tick {}", commas(*qty), commas(*by))
            }
            Order::Sustain { qty, per, item, from, to } => format!(
                "sustain {} {item} every {} ticks from {} to {}",
                commas(*qty),
                commas(*per),
                commas(*from),
                commas(*to)
            ),
        }
    }
}

fn commas(n: u64) -> String {
    let s = n.to_string();
    let mut out = String::new();
    for (i, c) in s.chars().enumerate() {
        if i > 0 && (s.len() - i) % 3 == 0 {
            out.push(',');
        }
        out.push(c);
    }
    out
}

// ================================================================ scenario

#[derive(Clone, Debug)]
pub struct Scenario {
    pub name: String,
    /// The `.factory` this problem is posed about.
    pub plant: String,
    pub brief: String,
    pub budget: u64,
    pub costs: Costs,
    pub orders: Vec<Order>,
}

impl Scenario {
    /// Every tick any order cares about, ascending and deduplicated -- so one
    /// run of the plant can answer all of them.
    pub fn probes(&self, now: Tick) -> Vec<Tick> {
        let mut v: Vec<Tick> = self.orders.iter().flat_map(|o| o.probes()).collect();
        v.push(now);
        v.sort_unstable();
        v.dedup();
        v
    }

    /// What the log has spent, command by command.
    ///
    /// Walks every command rather than every boundary: three edits at one tick
    /// are one recompile but three purchases, and a budget that only noticed
    /// the net effect would let a player buy and sell in the same instant for
    /// free.
    pub fn spend(&self, log: &Log, upto: Tick) -> Result<Spend, Fault> {
        let mut g = log.base.clone();
        let mut worth = self.costs.of_graph(&g);
        let mut s = Spend::default();
        for c in log.commands.iter().filter(|c| c.at <= upto) {
            c.edit.apply(&mut g).map_err(|e| Fault::at(c.at, &e))?;
            let now = self.costs.of_graph(&g);
            if now > worth {
                let cost = now - worth;
                s.total += cost;
                s.items.push((c.at, format!("{} {}", c.edit.verb(), c.edit.subject()), cost));
                if s.total > self.budget && s.overspent_at.is_none() {
                    s.overspent_at = Some(c.at);
                }
            }
            worth = now;
        }
        Ok(s)
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("name", self.name.clone())
            .set("plant", self.plant.clone())
            .set("brief", self.brief.clone())
            .set("budget", Json::big(self.budget as u128))
            .set("costs", self.costs.to_json())
            .set(
                "orders",
                Json::arr(self.orders.iter().map(|o| o.text()).collect::<Vec<_>>()),
            )
    }
}

/// What sinks have swallowed of one item, at the tick this snapshot is of.
///
/// `cycles x batch`, summed over the sink classes that consume it. Exact, and
/// derived from counters the solver was keeping anyway.
pub fn delivered(a: &At, item: &str) -> u64 {
    let counters = a.room.counters();
    let mut total = 0;
    for (c, act) in a.bp.actors.iter().enumerate() {
        if act.kind != ActorKind::Sink {
            continue;
        }
        for s in &act.inputs {
            if a.prog.item_name(s.item) == item {
                total += counters.cycles[c].saturating_mul(s.qty);
            }
        }
    }
    total
}

/// How the player is doing, at tick `now`.
pub fn evaluate(sc: &Scenario, log: &Log, now: Tick) -> Result<Json, Fault> {
    let probes = sc.probes(now);
    let items: Vec<String> =
        sc.orders.iter().map(|o| o.item().to_string()).collect::<Vec<_>>();
    // One run of the plant, answering every deadline it passes on the way.
    let mut seen: HashMap<(Tick, String), u64> = HashMap::new();
    crate::live::with_states(log, &probes, None, false, |a| {
        for item in &items {
            seen.insert((a.tick, item.clone()), delivered(&a, item));
        }
    })?;
    let got = |t: Tick, item: &str| -> u64 {
        seen.get(&(t.min(now), item.to_string())).copied().unwrap_or(0)
    };

    let spend = sc.spend(log, now)?;
    let mut orders: Vec<Json> = Vec::new();
    let mut won = true;
    let mut lost = false;
    for o in &sc.orders {
        let (need, have, met) = match o {
            Order::Deliver { qty, item, by } => {
                let have = got(*by, item);
                (*qty, have, now >= *by && have >= *qty)
            }
            Order::Sustain { qty, per, item, from, to } => {
                let window = to.saturating_sub(*from);
                let need = qty.saturating_mul(window) / (*per).max(1);
                let have = got(*to, item).saturating_sub(got(*from, item));
                (need, have, now >= *to && have >= need)
            }
        };
        // A deadline that has passed unmet cannot be met later. Saying so is
        // the difference between a target and a scoreboard.
        let failed = now >= o.deadline() && !met;
        won &= met;
        lost |= failed;
        orders.push(
            Json::obj()
                .set("text", o.text())
                .set("item", o.item().to_string())
                .set("need", Json::big(need as u128))
                .set("have", Json::big(have as u128))
                .set("deadline", o.deadline())
                .set("met", met)
                .set("failed", failed)
                .set("progress", if need == 0 { 1.0 } else { have as f64 / need as f64 }),
        );
    }

    Ok(Json::obj()
        .set("scenario", sc.to_json())
        .set("spent", Json::big(spend.total as u128))
        .set("remaining", Json::big(sc.budget.saturating_sub(spend.total) as u128))
        .set("overspent", spend.overspent_at.map(|t| Json::Int(t as i128)))
        .set(
            "purchases",
            Json::Arr(
                spend
                    .items
                    .iter()
                    .map(|(at, what, cost)| {
                        Json::obj()
                            .set("at", *at)
                            .set("what", what.clone())
                            .set("cost", Json::big(*cost as u128))
                    })
                    .collect(),
            ),
        )
        .set("orders", Json::Arr(orders))
        .set("won", won && !sc.orders.is_empty())
        .set("lost", lost))
}

// ================================================================== parsing

/// A scenario file. Line-oriented and small on purpose: this is a rules file
/// that a designer edits between playtests, and every construct in it has to
/// be readable by someone who has never seen the DSL.
///
/// ```text
///   scenario First Gears {
///       plant   p1-gears.factory
///       brief   Ship gears. The rail is not the problem.
///       budget  1000
///
///       cost process 120 each
///       cost link    60  per vehicle
///       cost storage 40  per 1000 capacity
///       cost wire    5   each
///
///       order deliver 12000 Gear by 60000
///       order sustain 20 Gear per 100 ticks from 40000 to 60000
///   }
/// ```
pub fn parse(src: &str) -> Result<Scenario, String> {
    let mut sc = Scenario {
        name: String::new(),
        plant: String::new(),
        brief: String::new(),
        budget: 0,
        costs: Costs::default(),
        orders: Vec::new(),
    };
    let mut open = false;
    for (n, raw) in src.lines().enumerate() {
        let line = raw.split('#').next().unwrap_or("").trim();
        if line.is_empty() {
            continue;
        }
        let at = |msg: String| format!("line {}: {msg}", n + 1);
        let w: Vec<&str> = line.split_whitespace().collect();
        if w[0] == "scenario" {
            let rest = line.trim_start_matches("scenario").trim();
            sc.name = rest.trim_end_matches('{').trim().to_string();
            open = rest.ends_with('{');
            if sc.name.is_empty() {
                return Err(at("a scenario needs a name".into()));
            }
            continue;
        }
        if w[0] == "}" {
            open = false;
            continue;
        }
        if !open {
            return Err(at(format!("`{}` is outside any scenario block", w[0])));
        }
        let num = |s: &str| -> Result<u64, String> {
            s.replace(['_', ','], "")
                .parse::<u64>()
                .map_err(|_| at(format!("`{s}` is not a number")))
        };
        match w[0] {
            "plant" => {
                sc.plant = w.get(1).ok_or_else(|| at("plant needs a file".into()))?.to_string()
            }
            "brief" => sc.brief = line.trim_start_matches("brief").trim().to_string(),
            "budget" => sc.budget = num(w.get(1).ok_or_else(|| at("budget needs a number".into()))?)?,
            "cost" => {
                let what = *w.get(1).ok_or_else(|| at("cost needs a thing".into()))?;
                let amount = num(w.get(2).ok_or_else(|| at("cost needs an amount".into()))?)?;
                match what {
                    "source" => sc.costs.source = amount,
                    "process" => sc.costs.process = amount,
                    "sink" => sc.costs.sink = amount,
                    "link" => sc.costs.link = amount,
                    "wire" => sc.costs.wire = amount,
                    "storage" => {
                        sc.costs.storage = amount;
                        // `per 1000 capacity`, if it says so; otherwise per unit.
                        sc.costs.storage_per = match w.iter().position(|&x| x == "per") {
                            Some(i) => num(w.get(i + 1).ok_or_else(|| at("per what?".into()))?)?,
                            None => 1,
                        };
                    }
                    other => return Err(at(format!("nothing called `{other}` has a price"))),
                }
            }
            "order" => sc.orders.push(order(&w, &at)?),
            other => return Err(at(format!("`{other}` is not a scenario setting"))),
        }
    }
    if sc.plant.is_empty() {
        return Err("a scenario has to say which plant it is about".into());
    }
    if sc.orders.is_empty() {
        return Err(format!("`{}` asks the player for nothing", sc.name));
    }
    Ok(sc)
}

fn order(w: &[&str], at: &impl Fn(String) -> String) -> Result<Order, String> {
    let num = |s: Option<&&str>| -> Result<u64, String> {
        s.ok_or_else(|| at("an order stops in the middle".into()))?
            .replace(['_', ','], "")
            .parse::<u64>()
            .map_err(|_| at(format!("`{}` is not a number", s.unwrap())))
    };
    let name = |s: Option<&&str>| -> Result<String, String> {
        Ok(s.ok_or_else(|| at("an order stops in the middle".into()))?.to_string())
    };
    match w.get(1).copied() {
        // order deliver 12000 Gear by 60000
        Some("deliver") => {
            Ok(Order::Deliver { qty: num(w.get(2))?, item: name(w.get(3))?, by: num(w.get(5))? })
        }
        // order sustain 20 Gear per 100 ticks from 40000 to 60000
        Some("sustain") => Ok(Order::Sustain {
            qty: num(w.get(2))?,
            item: name(w.get(3))?,
            per: num(w.get(5))?,
            from: num(w.get(8))?,
            to: num(w.get(10))?,
        }),
        Some(other) => Err(at(format!("`{other}` is not a kind of order"))),
        None => Err(at("an order with nothing in it".into())),
    }
}
