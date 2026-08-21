//! The workbench document: a factory as placed nodes and wires, plus the two
//! conversions that keep the language authoritative.
//!
//! ```text
//!   Graph  --emit-->  DSL source  --dsl::parse-->  Program/Blueprint
//!     ^                                                  |
//!     +---------------- from_program ---------------------+
//! ```
//!
//! The canvas never compiles anything itself. It edits this document, the
//! document emits source, and the existing front-end decides what that source
//! means -- so a plant built with a mouse is exactly as expressive as a plant
//! written by hand, and no more. Anything the builder can draw, the language
//! can already say; anything the language rejects, the builder finds out about
//! from the same error the file would have produced.
//!
//! # What a port turned out to be
//!
//! The obvious builder model is nodes with ports and edges between them. This
//! model has no room for one. A machine's connection points are its
//! ingredients and its products; a storage's are its item slots, which are
//! *derived* from whoever deposits there rather than declared. And a storage
//! slot is not an input or an output -- machines withdraw from and deposit
//! into the same slot, with the storage's policy deciding who wins. So a port
//! here is an item, in both directions, and `Edge` carries an optional item
//! qualifier for the one case where a machine with two products and two bays
//! has to say which goes where. That is the whole of it.
//!
//! # Positions
//!
//! Layout is written back as `# @pos Name x y` comments. The lexer already
//! drops comments, so a saved sketch stays a valid `.factory` file that the
//! harness can run, and it still opens on the canvas where it was left.

use crate::json::Json;
use crate::model::*;
use std::collections::HashMap;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Kind {
    Source,
    Storage,
    Process,
    Sink,
    Link,
}

impl Kind {
    pub fn word(self) -> &'static str {
        match self {
            Kind::Source => "source",
            Kind::Storage => "storage",
            Kind::Process => "process",
            Kind::Sink => "sink",
            Kind::Link => "link",
        }
    }

    pub fn parse(w: &str) -> Option<Kind> {
        Some(match w {
            "source" => Kind::Source,
            "storage" => Kind::Storage,
            "process" => Kind::Process,
            "sink" => Kind::Sink,
            "link" => Kind::Link,
            _ => return None,
        })
    }

    pub fn is_machine(self) -> bool {
        self != Kind::Storage
    }
}

/// An item and a quantity, by name -- the document deals in names, because a
/// document that referred to interned ids could not be edited without the
/// program that interned them.
#[derive(Clone, Debug, PartialEq)]
pub struct Amount {
    pub item: String,
    pub qty: Qty,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Node {
    pub name: String,
    pub kind: Kind,
    /// Population for a machine class. Always 1 for a storage: two storages
    /// are never interchangeable, so `x N` on one would mean N distinct bays.
    pub count: u64,
    pub shared: bool,
    pub x: f64,
    pub y: f64,

    // ---- machines
    pub inputs: Vec<Amount>,
    pub outputs: Vec<Amount>,
    pub duration: Tick,
    pub returns: Tick,
    pub geometry: Option<Geometry>,

    // ---- storages
    pub capacity: Qty,
    pub policy: Policy,
    pub priority: Vec<String>,
    pub initial: Vec<Amount>,
}

impl Node {
    pub fn new(name: &str, kind: Kind) -> Node {
        Node {
            name: name.to_string(),
            kind,
            count: 1,
            shared: false,
            x: 0.0,
            y: 0.0,
            inputs: Vec::new(),
            outputs: Vec::new(),
            duration: 60,
            returns: 0,
            geometry: None,
            capacity: if kind == Kind::Storage { 10_000 } else { 0 },
            policy: Policy::RoundRobin,
            priority: Vec::new(),
            initial: Vec::new(),
        }
    }

    /// A link declares one `moves` clause rather than a matching consume and
    /// produce, so the document keeps the pair in step.
    pub fn moved(&self) -> Option<&Amount> {
        if self.kind == Kind::Link {
            self.inputs.first()
        } else {
            None
        }
    }
}

/// A wire. `item` qualifies it where a machine has more than one product and
/// more than one bay to put products in.
#[derive(Clone, Debug, PartialEq)]
pub struct Edge {
    pub from: String,
    pub to: String,
    pub item: Option<String>,
}

#[derive(Clone, Debug, PartialEq)]
pub struct Graph {
    pub name: String,
    pub items: Vec<String>,
    pub nodes: Vec<Node>,
    pub edges: Vec<Edge>,
    pub deploy: u64,
    pub stagger: u64,
}

impl Default for Graph {
    fn default() -> Graph {
        Graph {
            name: "Sketch".into(),
            items: Vec::new(),
            nodes: Vec::new(),
            edges: Vec::new(),
            deploy: 1,
            stagger: 0,
        }
    }
}

impl Graph {
    pub fn node(&self, name: &str) -> Option<&Node> {
        self.nodes.iter().find(|n| n.name == name)
    }

    // =========================================================== emission

    /// The document as DSL source.
    pub fn emit(&self) -> String {
        let mut s = String::new();
        for it in &self.items {
            s.push_str("item ");
            s.push_str(it);
            s.push('\n');
        }
        if !self.items.is_empty() {
            s.push('\n');
        }
        s.push_str("blueprint ");
        s.push_str(&self.name);
        s.push_str(" {\n");

        for n in &self.nodes {
            s.push_str(&format!("    # @pos {} {} {}\n", n.name, n.x.round(), n.y.round()));
            s.push_str("    ");
            if n.shared {
                s.push_str("shared ");
            }
            s.push_str(n.kind.word());
            s.push(' ');
            s.push_str(&n.name);
            if n.kind.is_machine() && n.count != 1 {
                s.push_str(&format!(" x{}", n.count));
            }
            s.push_str(" { ");
            match n.kind {
                Kind::Storage => emit_storage(n, &mut s),
                _ => emit_machine(n, &mut s),
            }
            s.push_str("}\n");
        }

        if !self.edges.is_empty() {
            s.push('\n');
        }
        for e in &self.edges {
            s.push_str(&format!("    wire {} -> {}", e.from, e.to));
            if let Some(it) = &e.item {
                s.push_str(&format!(" {{ {it} }}"));
            }
            s.push('\n');
        }
        s.push_str("}\n\n");
        s.push_str(&format!("deploy {} x {}", self.deploy, self.name));
        if self.stagger != 0 {
            s.push_str(&format!(" stagger {}", self.stagger));
        }
        s.push('\n');
        s
    }

    // =========================================================== importing

    /// Read a compiled program back into a document, so any of the existing
    /// configurations opens on the canvas.
    ///
    /// A deployment whose lines share infrastructure is shown as the *one-line*
    /// blueprint the player wrote rather than as the lowered form the solver
    /// runs, because that is the thing they placed.
    pub fn from_program(prog: &Program) -> Graph {
        let d = prog.deploys.first().copied().unwrap_or(Deploy {
            blueprint: 0,
            count: 1,
            stagger: 0,
            origin: None,
        });
        let (bp_id, lines) = match d.origin {
            Some(o) => (o.blueprint as usize, o.lines),
            None => (d.blueprint as usize, d.count),
        };
        let bp = &prog.blueprints[bp_id];
        let mut g = Graph::from_blueprint(bp, &prog.items);
        g.deploy = lines;
        g.stagger = d.stagger;
        g
    }

    pub fn from_blueprint(bp: &Blueprint, items: &[String]) -> Graph {
        let name = |i: ItemId| items[i as usize].clone();
        let amounts =
            |v: &[Stack]| v.iter().map(|s| Amount { item: name(s.item), qty: s.qty }).collect();

        let mut nodes: Vec<Node> = Vec::new();
        let mut edges: Vec<Edge> = Vec::new();

        // Storages and machines are emitted in an order that interleaves them
        // readably while advancing both cursors monotonically -- so the
        // re-parsed program has byte-identical storage and class indexing, and
        // the workbench provably cannot permute the plant it was handed.
        let (mut ai, mut si) = (0usize, 0usize);
        let mut filled = vec![false; bp.storages.len()];
        while ai < bp.actors.len() || si < bp.storages.len() {
            let take_storage = si < bp.storages.len()
                && (ai >= bp.actors.len()
                    || filled[si]
                    || bp.actors[ai].in_stores.contains(&(si as u16)));
            if take_storage {
                let sd = &bp.storages[si];
                let mut n = Node::new(&sanitise(&sd.name), Kind::Storage);
                n.shared = sd.shared;
                n.capacity = sd.capacity;
                n.policy = sd.policy;
                n.initial = amounts(&sd.initial);
                if sd.policy == Policy::Priority {
                    n.priority =
                        sd.order.iter().map(|&c| sanitise(&bp.actors[c as usize].name)).collect();
                }
                nodes.push(n);
                si += 1;
            } else {
                let a = &bp.actors[ai];
                let mut n = Node::new(&sanitise(&a.name), kind_of(a));
                n.count = a.count;
                n.shared = a.shared;
                n.duration = a.duration;
                n.returns = a.return_latency;
                n.geometry = a.geometry;
                n.inputs = amounts(&a.inputs);
                n.outputs = amounts(&a.outputs);
                for &s in &a.out_stores {
                    filled[s as usize] = true;
                }
                nodes.push(n);
                ai += 1;
            }
        }

        // Wires, in class order. A deposit edge carries an item qualifier when
        // the bay takes only some of what the machine makes -- which is
        // exactly the information `slots` records.
        for a in &bp.actors {
            for &s in &a.in_stores {
                edges.push(Edge {
                    from: sanitise(&bp.storages[s as usize].name),
                    to: sanitise(&a.name),
                    item: None,
                });
            }
            for &s in &a.out_stores {
                let sd = &bp.storages[s as usize];
                let taken: Vec<ItemId> =
                    a.outputs.iter().map(|o| o.item).filter(|i| sd.slots.contains(i)).collect();
                let qualified = taken.len() < a.outputs.len();
                if qualified && taken.is_empty() {
                    // Wired, but nothing this machine makes ends up here.
                    edges.push(Edge {
                        from: sanitise(&a.name),
                        to: sanitise(&sd.name),
                        item: None,
                    });
                    continue;
                }
                for it in taken {
                    edges.push(Edge {
                        from: sanitise(&a.name),
                        to: sanitise(&sd.name),
                        item: if qualified { Some(name(it)) } else { None },
                    });
                    if !qualified {
                        break;
                    }
                }
            }
        }

        let mut g = Graph {
            name: sanitise(&bp.name),
            items: items.to_vec(),
            nodes,
            edges,
            deploy: 1,
            stagger: 0,
        };
        g.autolayout_regions(bp);
        g
    }

    /// Positions for a plant that arrived without any.
    ///
    /// The obvious layout is longest-path layering along the wires, and for a
    /// plant with one region that is what this does. For a plant that
    /// decomposes it lays out the *regions* instead: each one an internally
    /// layered block, blocks of equal causal rank side by side, ranks stacked
    /// down the page. Four mines railing to a smelting region then come out
    /// drawn as four mines railing to a smelting region, which is what the
    /// plant is.
    ///
    /// That a factory's default arrangement turns out to be its causal
    /// decomposition was not a goal. It is the most useful thing the layout
    /// does.
    pub fn autolayout(&mut self) {
        self.place(&HashMap::new(), &HashMap::new());
    }

    fn autolayout_regions(&mut self, bp: &Blueprint) {
        let g = crate::domains::regions(bp);
        let mut of: HashMap<String, usize> = HashMap::new();
        for (s, sd) in bp.storages.iter().enumerate() {
            if g.of_storage[s] != usize::MAX {
                of.insert(sanitise(&sd.name), g.of_storage[s]);
            }
        }
        for (c, a) in bp.actors.iter().enumerate() {
            if g.of_class[c] != usize::MAX {
                of.insert(sanitise(&a.name), g.of_class[c]);
            }
        }
        // Causal rank: how many channels deep a region sits. Regions nobody
        // feeds are rank 0, which puts every mine on the top row.
        let mut rank = vec![0usize; g.regions.len()];
        for _ in 0..g.regions.len() {
            let mut moved = false;
            for ch in &g.channels {
                if rank[ch.dst_region] < rank[ch.src_region] + 1 {
                    rank[ch.dst_region] = rank[ch.src_region] + 1;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }
        let ranks: HashMap<usize, usize> = rank.iter().copied().enumerate().collect();
        self.place(&of, &ranks);
    }

    fn place(&mut self, of: &HashMap<String, usize>, rank: &HashMap<usize, usize>) {
        const COL: f64 = 230.0;
        const ROW: f64 = 150.0;
        const GAP_X: f64 = 150.0;
        const GAP_Y: f64 = 130.0;
        const WRAP: usize = 6;

        // Which block each node belongs to. A lifted transport belongs to
        // neither of the regions it joins, so it is positioned afterwards,
        // between the two bays it actually connects.
        let block_of: Vec<Option<usize>> =
            self.nodes.iter().map(|n| of.get(&n.name).copied()).collect();
        let mut blocks: Vec<usize> = block_of.iter().flatten().copied().collect();
        blocks.sort_unstable();
        blocks.dedup();
        if blocks.is_empty() {
            // No decomposition was offered: the whole plant is one block.
            blocks.push(usize::MAX);
        }

        let idx: HashMap<String, usize> =
            self.nodes.iter().enumerate().map(|(i, n)| (n.name.clone(), i)).collect();

        // Depth inside a block, over the wires that stay inside it. Longest
        // path, relaxed until it settles: a feedback loop is a factory, not a
        // mistake, so the pass count is bounded by the node count and stops.
        let mut depth = vec![0usize; self.nodes.len()];
        for _ in 0..self.nodes.len() {
            let mut moved = false;
            for e in &self.edges {
                let (Some(&a), Some(&b)) = (idx.get(&e.from), idx.get(&e.to)) else { continue };
                if block_of[a] != block_of[b] {
                    continue;
                }
                if depth[b] < depth[a] + 1 {
                    depth[b] = depth[a] + 1;
                    moved = true;
                }
            }
            if !moved {
                break;
            }
        }

        // Lay each block out at its own origin, then move the whole block.
        let mut extent: HashMap<usize, (f64, f64)> = HashMap::new();
        for &b in &blocks {
            let mut used: HashMap<usize, usize> = HashMap::new();
            let (mut w, mut h) = (0.0f64, 0.0f64);
            for i in 0..self.nodes.len() {
                if block_of[i].unwrap_or(usize::MAX) != b {
                    continue;
                }
                // A long chain wraps: fifteen stages in one row is technically
                // correct and unreadable. Every row still reads left to right.
                let d = depth[i];
                let row = used.entry(d).or_insert(0);
                let x = (d % WRAP) as f64 * COL;
                let y = (d / WRAP) as f64 * ROW * 2.0 + *row as f64 * ROW;
                *row += 1;
                self.nodes[i].x = x;
                self.nodes[i].y = y;
                w = w.max(x + COL);
                h = h.max(y + ROW);
            }
            extent.insert(b, (w, h));
        }

        let mut rows: Vec<Vec<usize>> = Vec::new();
        for &b in &blocks {
            let r = rank.get(&b).copied().unwrap_or(0);
            while rows.len() <= r {
                rows.push(Vec::new());
            }
            rows[r].push(b);
        }
        let mut origin: HashMap<usize, (f64, f64)> = HashMap::new();
        let mut y = 80.0;
        for row in &rows {
            let mut x = 80.0;
            let mut tall = 0.0f64;
            for &b in row {
                let (w, h) = extent[&b];
                origin.insert(b, (x, y));
                x += w + GAP_X;
                tall = tall.max(h);
            }
            if !row.is_empty() {
                y += tall + GAP_Y;
            }
        }
        for i in 0..self.nodes.len() {
            let b = block_of[i].unwrap_or(usize::MAX);
            if let Some(&(ox, oy)) = origin.get(&b) {
                self.nodes[i].x += ox;
                self.nodes[i].y += oy;
            }
        }

        // A lifted transport goes between the two bays it joins, which is both
        // where it belongs and where its track wants to be drawn.
        for i in 0..self.nodes.len() {
            if block_of[i].is_some() {
                continue;
            }
            let name = self.nodes[i].name.clone();
            let from =
                self.edges.iter().find(|e| e.to == name).and_then(|e| idx.get(&e.from).copied());
            let to =
                self.edges.iter().find(|e| e.from == name).and_then(|e| idx.get(&e.to).copied());
            let (Some(a), Some(b)) = (from, to) else { continue };
            let mut x = (self.nodes[a].x + self.nodes[b].x) / 2.0 + 30.0;
            let mut y = (self.nodes[a].y + self.nodes[b].y) / 2.0;
            // Four mines railing to one yard would otherwise stack four rails
            // in the same place.
            for _ in 0..24 {
                let clash = (0..self.nodes.len()).any(|j| {
                    j != i
                        && (self.nodes[j].x - x).abs() < 110.0
                        && (self.nodes[j].y - y).abs() < 52.0
                });
                if !clash {
                    break;
                }
                y += 58.0;
                x += 14.0;
            }
            self.nodes[i].x = x;
            self.nodes[i].y = y;
        }
    }

    /// Overlay `# @pos` comments from a source file onto a laid-out document.
    pub fn apply_positions(&mut self, src: &str) {
        for line in src.lines() {
            let Some(rest) = line.trim_start().strip_prefix("# @pos ") else { continue };
            let mut f = rest.split_whitespace();
            let (Some(name), Some(x), Some(y)) = (f.next(), f.next(), f.next()) else { continue };
            let (Ok(x), Ok(y)) = (x.parse::<f64>(), y.parse::<f64>()) else { continue };
            if let Some(n) = self.nodes.iter_mut().find(|n| n.name == name) {
                n.x = x;
                n.y = y;
            }
        }
    }

    // ================================================================ json

    pub fn to_json(&self) -> Json {
        let amounts = |v: &[Amount]| {
            Json::arr(
                v.iter()
                    .map(|a| Json::obj().set("item", a.item.clone()).set("qty", Json::big(a.qty as u128)))
                    .collect::<Vec<_>>(),
            )
        };
        let nodes: Vec<Json> = self
            .nodes
            .iter()
            .map(|n| {
                let mut j = Json::obj()
                    .set("name", n.name.clone())
                    .set("kind", n.kind.word())
                    .set("count", Json::big(n.count as u128))
                    .set("shared", n.shared)
                    .set("x", n.x)
                    .set("y", n.y);
                if n.kind.is_machine() {
                    j = j
                        .set("inputs", amounts(&n.inputs))
                        .set("outputs", amounts(&n.outputs))
                        .set("duration", n.duration)
                        .set("returns", n.returns)
                        .set(
                            "geometry",
                            match n.geometry {
                                Some(g) => Json::obj()
                                    .set("base", g.base)
                                    .set("distance", g.distance)
                                    .set("speed", g.speed),
                                None => Json::Null,
                            },
                        );
                } else {
                    j = j
                        .set("capacity", Json::big(n.capacity as u128))
                        .set("policy", n.policy.label())
                        .set("priority", Json::arr(n.priority.clone()))
                        .set("initial", amounts(&n.initial));
                }
                j
            })
            .collect();
        let edges: Vec<Json> = self
            .edges
            .iter()
            .map(|e| {
                Json::obj()
                    .set("from", e.from.clone())
                    .set("to", e.to.clone())
                    .set("item", e.item.clone())
            })
            .collect();
        Json::obj()
            .set("name", self.name.clone())
            .set("items", Json::arr(self.items.clone()))
            .set("nodes", Json::Arr(nodes))
            .set("edges", Json::Arr(edges))
            .set("deploy", Json::big(self.deploy as u128))
            .set("stagger", self.stagger)
    }

    pub fn from_json(j: &Json) -> Result<Graph, String> {
        let amounts = |v: &Json| -> Vec<Amount> {
            v.as_arr()
                .iter()
                .filter_map(|a| {
                    Some(Amount {
                        item: a.at("item").as_str()?.to_string(),
                        qty: a.at("qty").as_u64()?,
                    })
                })
                .collect()
        };
        let mut nodes = Vec::new();
        for n in j.at("nodes").as_arr() {
            let name = n.at("name").as_str().ok_or("a node has no name")?.to_string();
            let kw = n.at("kind").as_str().unwrap_or("process");
            let kind = Kind::parse(kw).ok_or(format!("`{name}` has unknown kind `{kw}`"))?;
            let mut node = Node::new(&name, kind);
            node.count = n.at("count").as_u64().unwrap_or(1).max(1);
            node.shared = n.at("shared").as_bool().unwrap_or(false);
            node.x = n.at("x").as_f64().unwrap_or(0.0);
            node.y = n.at("y").as_f64().unwrap_or(0.0);
            if kind.is_machine() {
                node.inputs = amounts(n.at("inputs"));
                node.outputs = amounts(n.at("outputs"));
                node.duration = n.at("duration").as_u64().unwrap_or(60);
                node.returns = n.at("returns").as_u64().unwrap_or(0);
                let g = n.at("geometry");
                node.geometry = if g.is_null() {
                    None
                } else {
                    Some(Geometry {
                        base: g.at("base").as_u64().unwrap_or(0),
                        distance: g.at("distance").as_u64().unwrap_or(0),
                        speed: g.at("speed").as_u64().unwrap_or(1).max(1),
                    })
                };
                node.count = node.count.max(1);
            } else {
                node.count = 1;
                node.capacity = n.at("capacity").as_u64().unwrap_or(0);
                node.policy = match n.at("policy").as_str().unwrap_or("index") {
                    "round_robin" => Policy::RoundRobin,
                    "priority" => Policy::Priority,
                    _ => Policy::Index,
                };
                node.priority = n
                    .at("priority")
                    .as_arr()
                    .iter()
                    .filter_map(|p| p.as_str().map(str::to_string))
                    .collect();
                node.initial = amounts(n.at("initial"));
            }
            nodes.push(node);
        }
        let mut edges = Vec::new();
        for e in j.at("edges").as_arr() {
            edges.push(Edge {
                from: e.at("from").as_str().ok_or("an edge has no source")?.to_string(),
                to: e.at("to").as_str().ok_or("an edge has no target")?.to_string(),
                item: e.at("item").as_str().map(str::to_string),
            });
        }
        Ok(Graph {
            name: j.at("name").as_str().unwrap_or("Sketch").to_string(),
            items: j
                .at("items")
                .as_arr()
                .iter()
                .filter_map(|i| i.as_str().map(str::to_string))
                .collect(),
            nodes,
            edges,
            deploy: j.at("deploy").as_u64().unwrap_or(1).max(1),
            stagger: j.at("stagger").as_u64().unwrap_or(0),
        })
    }
}

fn kind_of(a: &ActorDef) -> Kind {
    match a.kind {
        ActorKind::Source => Kind::Source,
        ActorKind::Sink => Kind::Sink,
        ActorKind::Transport => Kind::Link,
        ActorKind::Process => Kind::Process,
    }
}

/// A deployment of lines with private state is written out one line at a time,
/// and `Blueprint::spread` names the copies `Bay#3`. `#` opens a comment, so
/// those names cannot be written back out.
///
/// `from_program` shows a shared deployment as the one-line blueprint that was
/// actually authored, so this normally never fires -- but `from_blueprint` is
/// public and can be handed a spread plant, and a document that silently
/// emitted an unreadable file would be worse than one that renames.
fn sanitise(name: &str) -> String {
    name.replace('#', "_")
}

fn emit_storage(n: &Node, s: &mut String) {
    s.push_str(&format!("capacity {} ", n.capacity));
    for a in &n.initial {
        s.push_str(&format!("initial {} {} ", a.qty, a.item));
    }
    s.push_str(&format!("policy {} ", n.policy.label()));
    if n.policy == Policy::Priority && !n.priority.is_empty() {
        s.push_str(&format!("priority {} ", n.priority.join(", ")));
    }
}

fn emit_machine(n: &Node, s: &mut String) {
    // A link declares one `moves` clause where a process declares a matching
    // consume and produce. Geometry is the honest way to declare its timing --
    // one distance fixes both legs -- so it is used whenever the two legs
    // agree with it, and explicit `takes`/`returns` cover everything else.
    if let Some(m) = n.moved() {
        s.push_str(&format!("moves {} {} ", m.qty, m.item));
        let derived = match n.geometry {
            Some(g) => {
                s.push_str(&format!("distance {} speed {} base {} ", g.distance, g.speed, g.base));
                g.latency()
            }
            // No geometry: both legs have to be stated outright.
            None => Tick::MAX,
        };
        if n.duration != derived {
            s.push_str(&format!("takes {} ticks ", n.duration));
        }
        if n.returns != derived {
            s.push_str(&format!("returns {} ticks ", n.returns));
        }
        return;
    }

    for a in &n.inputs {
        s.push_str(&format!("consumes {} {} ", a.qty, a.item));
    }
    // A source or a sink states a period; a process states how long one cycle
    // takes. Same number, two readings.
    match n.kind {
        Kind::Source | Kind::Sink => s.push_str(&format!("every {} ticks ", n.duration)),
        _ => s.push_str(&format!("takes {} ticks ", n.duration)),
    }
    for a in &n.outputs {
        s.push_str(&format!("produces {} {} ", a.qty, a.item));
    }
}
