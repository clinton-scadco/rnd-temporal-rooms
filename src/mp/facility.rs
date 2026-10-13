//! Capabilities of a works plot, derived from its component graph.
use crate::machine::{design::Design, parts::{self, Kind}};
use crate::model::Qty;
use crate::json::Json;
use std::collections::{BTreeMap, BTreeSet};

#[derive(Clone, Debug, Default, PartialEq)]
pub struct Layout {
    pub capacity: Qty,
    pub ground: Vec<String>,
    pub inputs: Vec<(String, Qty)>,
    pub outputs: Vec<(String, Qty)>,
    pub shipping: Vec<(String, Qty)>,
    pub storage: bool,
    pub dispatch: bool,
}

impl Layout {
    pub fn to_json(&self) -> Json {
        let flows = |v: &[(String, Qty)]| Json::Arr(v.iter().map(|(i,q)|
            Json::obj().set("item", i.clone()).set("perSecond", *q)).collect());
        Json::obj().set("capacity", self.capacity).set("storage", self.storage)
            .set("dispatch", self.dispatch).set("ground", Json::arr(self.ground.clone()))
            .set("inputs", flows(&self.inputs)).set("outputs", flows(&self.outputs))
            .set("shipping", flows(&self.shipping))
    }
}

fn item(d: &Design, u: usize) -> String {
    super::lower::item_of(&d.units[u].tune.drawn()).to_string()
}

fn reached(d: &Design, start: usize) -> BTreeSet<usize> {
    let mut todo = vec![start];
    let mut seen = BTreeSet::new();
    while let Some(at) = todo.pop() {
        if !seen.insert(at) { continue; }
        for w in &d.wires {
            if w.from == d.units[at].name {
                if let Some(next) = d.index_of(&w.to) { todo.push(next); }
            }
        }
    }
    seen
}

/// Storage ports exist only where an intake reaches a container, and an
/// outlet is actually connected downstream of that container. A filter is
/// part of this graph, not an arbitrary item selection on the outer building.
pub fn layout(d: &Design) -> Layout {
    let mut out = Layout::default();
    for (n,u) in d.units.iter().enumerate() {
        if matches!(u.kind, Kind::MiningDrill | Kind::WaterIntake) { out.ground.push(item(d,n)); }
    }
    out.ground.sort(); out.ground.dedup();
    let containers: Vec<usize> = d.units.iter().enumerate()
        .filter(|(_,u)| matches!(u.kind, Kind::BulkBin | Kind::StorageTank)).map(|(n,_)|n).collect();
    let logistics = d.units.iter().all(|u| matches!(u.kind,
        Kind::Inlet | Kind::Pump | Kind::BulkBin | Kind::StorageTank |
        Kind::Outlet | Kind::Sorter | Kind::DispatchDock));
    out.storage = logistics && !containers.is_empty();
    out.dispatch = logistics && containers.is_empty() && d.units.iter().any(|u|u.kind==Kind::DispatchDock);
    if !out.storage { return out; }

    let mut inputs: BTreeMap<String, Qty> = BTreeMap::new();
    let mut exits: BTreeMap<(usize, String), Qty> = BTreeMap::new();
    let mut active = BTreeSet::new();
    let mut commodities: BTreeMap<usize, BTreeSet<String>> = BTreeMap::new();
    for (source,u) in d.units.iter().enumerate().filter(|(_,u)| matches!(u.kind, Kind::Inlet | Kind::Pump)) {
        let it = item(d,source);
        let reachable = reached(d,source);
        let mut connected = false;
        for &bin in &containers {
            if !reachable.contains(&bin) { continue; }
            connected = true;
            active.insert(bin);
            commodities.entry(bin).or_default().insert(it.clone());
            for to in reached(d,bin) {
                let v = &d.units[to];
                if !matches!(v.kind, Kind::Outlet | Kind::Sorter | Kind::DispatchDock) { continue; }
                if v.kind == Kind::Sorter && v.tune.drawn().item_tag() != it { continue; }
                let rate = parts::part(v.kind).ports.iter()
                    .filter(|p|p.dom == u.tune.subst.home()).map(|p|p.rate).sum();
                if rate > 0 { exits.insert((to,it.clone()),rate); }
            }
        }
        if connected { *inputs.entry(it).or_default() += parts::part(u.kind).ports[0].rate; }
    }
    for &bin in &active { out.capacity += parts::part(d.units[bin].kind).ports[0].cap; }
    let mut outputs: BTreeMap<String, Qty> = BTreeMap::new();
    let mut shipping: BTreeMap<String, Qty> = BTreeMap::new();
    for ((exit,it),rate) in &exits {
        let shared = exits.keys().filter(|(n,_)|n==exit).count().max(1) as Qty;
        let target = if d.units[*exit].kind==Kind::DispatchDock { &mut shipping } else { &mut outputs };
        *target.entry(it.clone()).or_default() += rate / shared;
    }
    // Shared container ports have a finite aggregate budget, including mixed
    // goods. Reserve equal shares across their authored item routes.
    for (it,rate) in &mut inputs {
        let cap: Qty = active.iter().filter(|b|commodities[b].contains(it)).map(|b|
            parts::part(d.units[*b].kind).ports[0].rate / commodities[b].len().max(1) as Qty).sum();
        *rate = (*rate).min(cap);
    }
    for it in inputs.keys() {
        let cap: Qty = active.iter().filter(|b|commodities[b].contains(it)).map(|b|
            parts::part(d.units[*b].kind).ports[1].rate / commodities[b].len().max(1) as Qty).sum();
        let total = outputs.get(it).copied().unwrap_or(0)+shipping.get(it).copied().unwrap_or(0);
        if total>cap {
            if let Some(q)=outputs.get_mut(it) { *q=*q*cap/total; }
            if let Some(q)=shipping.get_mut(it) { *q=*q*cap/total; }
        }
    }
    out.inputs=inputs.into_iter().collect();
    out.outputs=outputs.into_iter().collect();
    out.shipping=shipping.into_iter().collect();
    out
}

pub fn port(layout: &Layout, item: &str, out: bool) -> Option<Qty> {
    let side = if out { &layout.outputs } else { &layout.inputs };
    side.iter().find(|(i,_)|i==item).map(|(_,q)|*q)
}

