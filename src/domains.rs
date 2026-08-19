//! Causal domain decomposition -- finding the Room boundaries instead of
//! declaring them.
//!
//! Two nodes belong to the same domain if a change to one can affect the other
//! *at the same instant*. Wiring two machines to one storage does exactly that:
//! whether A can withdraw depends on what B already took. So contention fuses
//! its participants into a single indivisible unit of simulation.
//!
//! Transport is different. A batch that departs at tick `t` and lands at
//! `t + D` carries no information backwards and none forwards until it
//! arrives. For those `D` ticks the two ends are causally independent -- they
//! could be solved on different machines, at different times, in either order,
//! and the answer would be identical.
//!
//! So there are two decompositions worth having:
//!
//! * **hard domains** -- connected components of the whole wiring graph. Two
//!   hard domains never interact at all, ever.
//! * **transit domains** -- connected components once transports are cut. These
//!   interact, but only through scheduled batches, and each one can be advanced
//!   independently for as long as its shortest inbound transport takes.
//!
//! The second number is the interesting one. It is the answer to "how big can
//! a Room be, and how long can it run alone before it has to talk to anyone".

use crate::model::*;

/// A node in the wiring graph: storages and actor classes together.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Node {
    Storage(u16),
    Class(u16),
}

pub struct Domain {
    pub nodes: Vec<Node>,
    /// Machines inside, summed over class populations.
    pub machines: u64,
    /// Storage units of buffer inside.
    pub capacity: Qty,
    /// Transport classes delivering into this domain, and their cycle times.
    pub inbound: Vec<(u16, Tick)>,
    pub outbound: Vec<(u16, Tick)>,
}

impl Domain {
    /// How long this domain can be advanced without hearing from anyone else.
    ///
    /// Nothing can arrive sooner than the fastest inbound transport, so up to
    /// that horizon the domain's future is a function of its own state alone.
    /// A domain with no inbound transport is independent forever.
    pub fn independent_for(&self) -> Option<Tick> {
        self.inbound.iter().map(|&(_, d)| d).min()
    }
}

pub struct Report {
    pub hard: Vec<Domain>,
    pub transit: Vec<Domain>,
    /// Storages where two or more *machines* compete to withdraw, with the
    /// classes involved. Note the unit: four machines of one class contend
    /// just as really as four machines of four classes -- the difference is
    /// only that the first four are interchangeable and the second four are
    /// not, which is a statement about the arbiter, not about the queue.
    pub withdraw_contention: Vec<(u16, Vec<u16>, u64)>,
    /// Storages where two or more machines compete to deposit.
    pub deposit_contention: Vec<(u16, Vec<u16>, u64)>,
    /// Item cycles: classes whose output can reach their own input.
    pub feedback_classes: Vec<u16>,
}

struct Dsu {
    parent: Vec<usize>,
}

impl Dsu {
    fn new(n: usize) -> Dsu {
        Dsu { parent: (0..n).collect() }
    }
    fn find(&mut self, a: usize) -> usize {
        if self.parent[a] == a {
            return a;
        }
        let r = self.find(self.parent[a]);
        self.parent[a] = r;
        r
    }
    fn union(&mut self, a: usize, b: usize) {
        let (ra, rb) = (self.find(a), self.find(b));
        if ra != rb {
            self.parent[ra] = rb;
        }
    }
}

/// Decompose a blueprint. `cut_transports` controls whether transport classes
/// are treated as edges (hard domains) or removed (transit domains).
fn decompose(bp: &Blueprint, cut_transports: bool) -> Vec<Domain> {
    let ns = bp.storages.len();
    let nc = bp.actors.len();
    let mut dsu = Dsu::new(ns + nc);

    for (c, ad) in bp.actors.iter().enumerate() {
        if cut_transports && ad.kind == ActorKind::Transport {
            continue;
        }
        for &s in ad.in_stores.iter().chain(ad.out_stores.iter()) {
            dsu.union(ns + c, s as usize);
        }
    }

    let mut roots: Vec<usize> = Vec::new();
    let mut of: Vec<usize> = vec![usize::MAX; ns + nc];
    for i in 0..ns + nc {
        // A cut transport is not part of any domain; it is the channel between
        // them, which is precisely why it gets to be a boundary.
        if cut_transports && i >= ns && bp.actors[i - ns].kind == ActorKind::Transport {
            continue;
        }
        let r = dsu.find(i);
        of[i] = match roots.iter().position(|&x| x == r) {
            Some(p) => p,
            None => {
                roots.push(r);
                roots.len() - 1
            }
        };
    }

    let mut out: Vec<Domain> = roots
        .iter()
        .map(|_| Domain {
            nodes: Vec::new(),
            machines: 0,
            capacity: 0,
            inbound: Vec::new(),
            outbound: Vec::new(),
        })
        .collect();

    for s in 0..ns {
        if of[s] == usize::MAX {
            continue;
        }
        out[of[s]].nodes.push(Node::Storage(s as u16));
        out[of[s]].capacity += bp.storages[s].capacity;
    }
    for c in 0..nc {
        if of[ns + c] == usize::MAX {
            continue;
        }
        out[of[ns + c]].nodes.push(Node::Class(c as u16));
        out[of[ns + c]].machines += bp.actors[c].count;
    }

    if cut_transports {
        for (c, ad) in bp.actors.iter().enumerate() {
            if ad.kind != ActorKind::Transport {
                continue;
            }
            for &s in &ad.out_stores {
                if of[s as usize] != usize::MAX {
                    out[of[s as usize]].inbound.push((c as u16, ad.duration));
                }
            }
            for &s in &ad.in_stores {
                if of[s as usize] != usize::MAX {
                    out[of[s as usize]].outbound.push((c as u16, ad.duration));
                }
            }
        }
    }
    out
}

pub fn analyse(bp: &Blueprint) -> Report {
    let mut withdraw_contention = Vec::new();
    let mut deposit_contention = Vec::new();
    for (s, _sd) in bp.storages.iter().enumerate() {
        let takers: Vec<u16> = bp
            .actors
            .iter()
            .enumerate()
            .filter(|(_, a)| a.in_stores.contains(&(s as u16)))
            .map(|(i, _)| i as u16)
            .collect();
        let givers: Vec<u16> = bp
            .actors
            .iter()
            .enumerate()
            .filter(|(_, a)| a.out_stores.contains(&(s as u16)))
            .map(|(i, _)| i as u16)
            .collect();
        let tn: u64 = takers.iter().map(|&c| bp.actors[c as usize].count).sum();
        let gn: u64 = givers.iter().map(|&c| bp.actors[c as usize].count).sum();
        if tn > 1 {
            withdraw_contention.push((s as u16, takers, tn));
        }
        if gn > 1 {
            deposit_contention.push((s as u16, givers, gn));
        }
    }

    // A class is in feedback if some item it produces can, through any chain of
    // machines, come back as one of its inputs.
    let n_items = bp
        .actors
        .iter()
        .flat_map(|a| a.inputs.iter().chain(a.outputs.iter()))
        .map(|s| s.item as usize + 1)
        .max()
        .unwrap_or(0);
    let mut reach = vec![vec![false; n_items]; n_items];
    for a in &bp.actors {
        for i in &a.inputs {
            for o in &a.outputs {
                reach[i.item as usize][o.item as usize] = true;
            }
        }
    }
    for k in 0..n_items {
        for i in 0..n_items {
            if reach[i][k] {
                for j in 0..n_items {
                    if reach[k][j] {
                        reach[i][j] = true;
                    }
                }
            }
        }
    }
    let feedback_classes: Vec<u16> = bp
        .actors
        .iter()
        .enumerate()
        .filter(|(_, a)| {
            a.outputs
                .iter()
                .any(|o| a.inputs.iter().any(|i| reach[o.item as usize][i.item as usize]))
        })
        .map(|(i, _)| i as u16)
        .collect();

    Report {
        hard: decompose(bp, false),
        transit: decompose(bp, true),
        withdraw_contention,
        deposit_contention,
        feedback_classes,
    }
}

pub fn node_name(bp: &Blueprint, n: Node) -> String {
    match n {
        Node::Storage(s) => bp.storages[s as usize].name.clone(),
        Node::Class(c) => {
            let a = &bp.actors[c as usize];
            if a.count == 1 {
                a.name.clone()
            } else {
                format!("{}x{}", a.name, a.count)
            }
        }
    }
}
