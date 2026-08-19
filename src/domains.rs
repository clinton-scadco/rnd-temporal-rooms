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
    ///
    /// This is v2's answer and it is only half of one: it counts the material
    /// coming *in* and ignores the vehicles that have to come *back*. See
    /// `Region::slack`.
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
    // A transport's output item *is* its input item, so it satisfies the
    // definition trivially and says nothing by doing so. Feedback is a claim
    // about recipes, and a transport does not have one.
    let feedback_classes: Vec<u16> = bp
        .actors
        .iter()
        .enumerate()
        .filter(|(_, a)| a.kind != ActorKind::Transport)
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

// ===================================================== v3: executable regions

/// A transport lifted out of the plant and turned into a channel between two
/// regions.
///
/// Both directions matter and they are not the same length. Material takes
/// `latency` ticks to reach the far end; the empty vehicle takes
/// `return_latency` ticks to come back. The first is how much slack the
/// *receiving* region has, the second is how much the *sending* region has.
#[derive(Clone, Copy, Debug)]
pub struct Channel {
    pub class: u16,
    pub src_region: usize,
    pub dst_region: usize,
    pub from_store: u16,
    pub to_store: u16,
    pub latency: Tick,
    pub return_latency: Tick,
}

/// One independently advancing region: the unit a Room is actually built from.
#[derive(Clone, Debug)]
pub struct Region {
    pub storages: Vec<u16>,
    /// Classes wholly inside, links included when both their ends are here.
    pub classes: Vec<u16>,
    pub machines: u64,
    pub capacity: Qty,
    /// Channels arriving here, and channels leaving here.
    pub inbound: Vec<usize>,
    pub outbound: Vec<usize>,
}

impl Region {
    /// How far this region may advance beyond its neighbours, guaranteed,
    /// with no knowledge of what they are doing.
    ///
    /// Material arriving gives `latency` ticks of ignorance; vehicles that
    /// must come home before this region can load again give `return_latency`.
    /// A region has to be right about both, so its slack is the smaller of
    /// them. `None` means nothing constrains it at all.
    pub fn slack(&self, chans: &[Channel]) -> Option<Tick> {
        self.inbound
            .iter()
            .map(|&c| chans[c].latency)
            .chain(self.outbound.iter().map(|&c| chans[c].return_latency))
            .min()
    }
}

/// The executable decomposition: regions plus the channels between them.
pub struct RegionGraph {
    pub regions: Vec<Region>,
    pub channels: Vec<Channel>,
    /// storage -> region.
    pub of_storage: Vec<usize>,
    /// class -> region, or `usize::MAX` for a link that straddles two.
    pub of_class: Vec<usize>,
    /// Transit domains that had to be glued back together because a link
    /// between them teleports its vehicle home.
    pub fused: usize,
}

impl RegionGraph {
    pub fn is_split(&self) -> bool {
        self.regions.len() > 1
    }

    /// The smallest slack anywhere: how far apart two region clocks are
    /// guaranteed to be allowed to drift.
    pub fn min_slack(&self) -> Option<Tick> {
        self.regions.iter().filter_map(|r| r.slack(&self.channels)).min()
    }
}

/// Cut every transport, then glue back together anything a zero-length return
/// trip has pinned into lockstep.
///
/// The gluing is the part v2 did not know it needed. Cutting a link buys the
/// *receiving* side a window, because material takes time to arrive. It buys
/// the *sending* side nothing at all unless the vehicle also takes time to get
/// back -- and if it does not, the sending region can never run a single tick
/// ahead of the receiving one. Where that pins a whole cycle of regions
/// together they are not separate units of simulation, and pretending
/// otherwise would deadlock the scheduler rather than merely slow it down.
pub fn regions(bp: &Blueprint) -> RegionGraph {
    let ns = bp.storages.len();
    let nc = bp.actors.len();
    let mut dsu = Dsu::new(ns + nc);
    for (c, ad) in bp.actors.iter().enumerate() {
        if ad.kind == ActorKind::Transport {
            continue;
        }
        for &s in ad.in_stores.iter().chain(ad.out_stores.iter()) {
            dsu.union(ns + c, s as usize);
        }
    }

    // Provisional region ids, numbered by first appearance so the result is a
    // function of the blueprint and not of the union order.
    let mut roots: Vec<usize> = Vec::new();
    let mut prov = vec![usize::MAX; ns + nc];
    for i in 0..ns + nc {
        if i >= ns && bp.actors[i - ns].kind == ActorKind::Transport {
            continue;
        }
        let r = dsu.find(i);
        prov[i] = match roots.iter().position(|&x| x == r) {
            Some(p) => p,
            None => {
                roots.push(r);
                roots.len() - 1
            }
        };
    }
    let n_prov = roots.len();

    let ends = |ad: &ActorDef| -> (usize, usize) {
        let from = ad.primary_in().map(|s| prov[s as usize]).unwrap_or(usize::MAX);
        let to = ad.primary_out().map(|s| prov[s as usize]).unwrap_or(usize::MAX);
        (from, to)
    };

    // Fuse the strongly connected components of the zero-return-trip graph.
    // An edge dst -> src means "src may never lead dst", so a cycle of them
    // forces every clock in it to be equal.
    let mut adj: Vec<Vec<usize>> = vec![Vec::new(); n_prov];
    let mut radj: Vec<Vec<usize>> = vec![Vec::new(); n_prov];
    for ad in bp.actors.iter().filter(|a| a.is_link()) {
        let (from, to) = ends(ad);
        if from == to || from == usize::MAX || to == usize::MAX || ad.return_latency > 0 {
            continue;
        }
        adj[to].push(from);
        radj[from].push(to);
    }
    let scc = kosaraju(&adj, &radj);
    let n_final = scc.iter().copied().max().map(|m| m + 1).unwrap_or(0);
    let fused = n_prov - n_final;

    let of_storage: Vec<usize> = (0..ns).map(|s| scc[prov[s]]).collect();
    let of_class: Vec<usize> = (0..nc)
        .map(|c| {
            if bp.actors[c].is_link() {
                let (from, to) = ends(&bp.actors[c]);
                let (a, b) = (scc[from], scc[to]);
                if a == b {
                    a
                } else {
                    usize::MAX
                }
            } else {
                scc[prov[ns + c]]
            }
        })
        .collect();

    let mut regions: Vec<Region> = (0..n_final)
        .map(|_| Region {
            storages: Vec::new(),
            classes: Vec::new(),
            machines: 0,
            capacity: 0,
            inbound: Vec::new(),
            outbound: Vec::new(),
        })
        .collect();
    for s in 0..ns {
        regions[of_storage[s]].storages.push(s as u16);
        regions[of_storage[s]].capacity += bp.storages[s].capacity;
    }
    for c in 0..nc {
        if of_class[c] == usize::MAX {
            continue;
        }
        regions[of_class[c]].classes.push(c as u16);
        regions[of_class[c]].machines += bp.actors[c].count;
    }

    let mut channels: Vec<Channel> = Vec::new();
    for (c, ad) in bp.actors.iter().enumerate() {
        if !ad.is_link() || of_class[c] != usize::MAX {
            continue;
        }
        let (from, to) = ends(ad);
        let ch = Channel {
            class: c as u16,
            src_region: scc[from],
            dst_region: scc[to],
            from_store: ad.primary_in().expect("a link withdraws somewhere"),
            to_store: ad.primary_out().expect("a link deposits somewhere"),
            latency: ad.duration,
            return_latency: ad.return_latency,
        };
        regions[ch.src_region].outbound.push(channels.len());
        regions[ch.dst_region].inbound.push(channels.len());
        channels.push(ch);
    }

    RegionGraph { regions, channels, of_storage, of_class, fused }
}

/// Strongly connected components, numbered so that the mapping is a function
/// of the graph and not of the traversal.
fn kosaraju(adj: &[Vec<usize>], radj: &[Vec<usize>]) -> Vec<usize> {
    let n = adj.len();
    let mut order: Vec<usize> = Vec::with_capacity(n);
    let mut seen = vec![false; n];
    for s in 0..n {
        if seen[s] {
            continue;
        }
        // Iterative, because a long transport chain is a long recursion.
        let mut stack = vec![(s, 0usize)];
        seen[s] = true;
        while let Some((v, i)) = stack.pop() {
            if i < adj[v].len() {
                stack.push((v, i + 1));
                let w = adj[v][i];
                if !seen[w] {
                    seen[w] = true;
                    stack.push((w, 0));
                }
            } else {
                order.push(v);
            }
        }
    }
    let mut comp = vec![usize::MAX; n];
    let mut next = 0;
    for &s in order.iter().rev() {
        if comp[s] != usize::MAX {
            continue;
        }
        let mut stack = vec![s];
        comp[s] = next;
        while let Some(v) = stack.pop() {
            for &w in &radj[v] {
                if comp[w] == usize::MAX {
                    comp[w] = next;
                    stack.push(w);
                }
            }
        }
        next += 1;
    }
    // Renumber by lowest member, so ids do not depend on the sweep order.
    let mut first = vec![usize::MAX; next];
    for v in 0..n {
        if first[comp[v]] == usize::MAX {
            first[comp[v]] = v;
        }
    }
    let mut rank: Vec<usize> = (0..next).collect();
    rank.sort_by_key(|&c| first[c]);
    let mut inv = vec![0usize; next];
    for (i, &c) in rank.iter().enumerate() {
        inv[c] = i;
    }
    comp.iter().map(|&c| inv[c]).collect()
}
