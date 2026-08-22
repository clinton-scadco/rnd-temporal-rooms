//! Prototype 1: a plant that changes while it is running.
//!
//! Prototype 0's document was a *drawing*. You edited it, the whole thing was
//! compiled, and the run started again from tick zero. That is fine for a
//! workbench and useless for a game, because a player does not design a
//! factory and then watch it: they build a bad one, watch it fail, and fix it
//! at tick 12,000 without losing the twelve thousand ticks.
//!
//! So the document stops being a drawing and becomes a **history**:
//!
//! ```text
//!   base plant at t=0
//!   tick 12,000: place Processor Smelter2
//!   tick 12,020: wire OreYard -> Smelter2
//!   tick 12,500: retune GearPress   (recipe changed)
//!   tick 13,000: retune Rail        (8 vehicles -> 12)
//! ```
//!
//! and the thing the solver is asked for is still a pure function of two
//! arguments:
//!
//! ```text
//!   state(log, T)
//! ```
//!
//! That matters more than it looks. v1 to v3 all leaned on "the state at tick
//! T is a function of the plant and T", and every convenience in the stack --
//! the stateless server, the scrubbable timeline, the reload that cannot
//! desynchronise -- is downstream of it. A log is still one argument. Nothing
//! above this line had to change its mind about anything.
//!
//! # What an edit costs
//!
//! A region in v3 runs alone, at its own clock, as far ahead as its causal
//! slack allows. Two regions of one plant are routinely thousands of ticks
//! apart. So "apply this edit at tick 12,000" has to answer: *whose* tick
//! 12,000?
//!
//! There is only one honest answer, and `Room::run_until` already stated it --
//! in between global barriers there is no such thing as "the state of the
//! plant". An edit is therefore a **rendezvous**: every region is brought to
//! the edit's tick, the plant's state is harvested, the new plant is compiled,
//! and the state is poured back in. The price of a player edit is one barrier
//! and one recompile of a graph with a few dozen nodes. It is not a replay,
//! and it is not proportional to anything the player has built:
//!
//! ```text
//!   cost of an edit  =  O(cells)  +  O(nodes)
//!   cost of a replay =  O(ticks)
//! ```
//!
//! where `cells` is the compressed width of the population state -- tens, for
//! a plant with a billion machines in it.
//!
//! # What crosses the boundary
//!
//! [`Carry`]: contents, populations, arbitration pointers, counters. Keyed by
//! **name**, because a name is the only identity a document has; storage
//! indices, class indices and region membership are all things the next
//! compile is entitled to choose differently, and an edit that adds a link
//! moves the region boundaries underneath everything.
//!
//! What does *not* cross is every opinion the scheduler formed on the way
//! here: advances, messages, skew, closed forms. Those are properties of a
//! run, not of a tick, and Prototype 0 already learned that lesson once.
//!
//! A `Carry` is also, not by coincidence, exactly the canonical snapshot the
//! networking proof needs: a log plus snapshots at its boundaries is what lets
//! a late joiner start at tick 80,000 without replaying 80,000 ticks.

use crate::graph::{Edge, Graph, Kind, Node};
use crate::json::Json;
use crate::model::*;
use crate::pop::{self, ClassPop};
use crate::rooms::{self, Plan, Room};
use crate::dsl;
use std::collections::HashMap;

// ==================================================================== edits

/// One thing a player can do to a factory.
///
/// Deliberately small, and deliberately at the granularity of the *document*
/// rather than of the compiled plant. "Upgrade the rail from 8 vehicles to 12"
/// is a `Retune` carrying the whole node, not a `SetVehicleCount`: the
/// document is the authority on what a node is, the language is the authority
/// on what it means, and an edit that could say only some of a node would be a
/// third opinion.
#[derive(Clone, Debug, PartialEq)]
pub enum Edit {
    /// Introduce an item name. Items are a property of the plant, not of a
    /// node, so they get their own edit.
    Item(String),
    Place(Node),
    /// Replace a node's properties, keeping its name and its wiring.
    Retune(Node),
    Remove(String),
    Wire { from: String, to: String, item: Option<String> },
    Unwire { from: String, to: String },
    /// Rename the plant, and how many copies of it are deployed. Neither is a
    /// property of any node, and a log that could not say them could not
    /// reconstruct the document -- which is the one thing a log has to do.
    Name(String),
    Deploy(u64),
}

impl Edit {
    /// The node this edit is about, for a UI that wants to point at it.
    pub fn subject(&self) -> &str {
        match self {
            Edit::Item(s) => s,
            Edit::Place(n) | Edit::Retune(n) => &n.name,
            Edit::Remove(s) => s,
            Edit::Wire { from, .. } | Edit::Unwire { from, .. } => from,
            Edit::Name(s) => s,
            Edit::Deploy(_) => "the plant",
        }
    }

    pub fn verb(&self) -> &'static str {
        match self {
            Edit::Item(_) => "item",
            Edit::Place(_) => "place",
            Edit::Retune(_) => "retune",
            Edit::Remove(_) => "remove",
            Edit::Wire { .. } => "wire",
            Edit::Unwire { .. } => "unwire",
            Edit::Name(_) => "name",
            Edit::Deploy(_) => "deploy",
        }
    }

    /// Apply this edit to a document, or say why it cannot be.
    ///
    /// Every refusal here is a *structural* one -- a name that is not there, a
    /// wire between two bays -- and is therefore the same refusal on every
    /// machine that replays this log. That is the whole reason legality lives
    /// in the log rather than in the browser that produced it: two clients
    /// that disagree about which commands were accepted have already
    /// desynchronised, whatever the simulator does afterwards.
    pub fn apply(&self, g: &mut Graph) -> Result<(), String> {
        match self {
            Edit::Item(name) => {
                if name.is_empty() {
                    return Err("an item needs a name".into());
                }
                if !g.items.contains(name) {
                    g.items.push(name.clone());
                }
                Ok(())
            }
            Edit::Place(n) => {
                if g.nodes.iter().any(|x| x.name == n.name) {
                    return Err(format!("`{}` is already here", n.name));
                }
                g.nodes.push(n.clone());
                Ok(())
            }
            Edit::Retune(n) => {
                let Some(old) = g.nodes.iter_mut().find(|x| x.name == n.name) else {
                    return Err(format!("there is no `{}` to retune", n.name));
                };
                if old.kind != n.kind {
                    // Retuning is the thing you do to a machine you own. A
                    // smelter that becomes a bay is a demolition and a build,
                    // and the difference is not cosmetic: one keeps the
                    // machines that are mid-cycle and the other does not.
                    return Err(format!(
                        "`{}` is a {}; a {} would be a different building",
                        n.name,
                        old.kind.word(),
                        n.kind.word()
                    ));
                }
                let (x, y) = (old.x, old.y);
                *old = n.clone();
                old.x = x;
                old.y = y;
                Ok(())
            }
            Edit::Remove(name) => {
                if !g.nodes.iter().any(|x| x.name == *name) {
                    return Err(format!("there is no `{name}` to remove"));
                }
                g.nodes.retain(|x| x.name != *name);
                g.edges.retain(|e| e.from != *name && e.to != *name);
                Ok(())
            }
            Edit::Wire { from, to, item } => {
                let a = g.node(from).ok_or(format!("there is no `{from}`"))?.kind;
                let b = g.node(to).ok_or(format!("there is no `{to}`"))?.kind;
                if (a == Kind::Storage) == (b == Kind::Storage) {
                    return Err(if a == Kind::Storage {
                        format!("`{from} -> {to}` connects two storages; insert a machine between them")
                    } else {
                        format!("`{from} -> {to}` connects two machines; route them through a storage")
                    });
                }
                if g.edges.iter().any(|e| e.from == *from && e.to == *to) {
                    return Err(format!("`{from} -> {to}` is already wired"));
                }
                g.edges.push(Edge { from: from.clone(), to: to.clone(), item: item.clone() });
                Ok(())
            }
            Edit::Unwire { from, to } => {
                let before = g.edges.len();
                g.edges.retain(|e| !(e.from == *from && e.to == *to));
                if g.edges.len() == before {
                    return Err(format!("`{from} -> {to}` is not wired"));
                }
                Ok(())
            }
            Edit::Name(name) => {
                if name.is_empty() {
                    return Err("a plant needs a name".into());
                }
                g.name = name.clone();
                Ok(())
            }
            Edit::Deploy(n) => {
                if *n == 0 {
                    return Err("a plant deployed zero times is not deployed".into());
                }
                g.deploy = *n;
                Ok(())
            }
        }
    }

    pub fn to_json(&self) -> Json {
        let j = Json::obj().set("op", self.verb());
        match self {
            Edit::Item(name) => j.set("name", name.clone()),
            Edit::Place(n) | Edit::Retune(n) => j.set("node", n.to_json()),
            Edit::Remove(name) => j.set("name", name.clone()),
            Edit::Wire { from, to, item } => {
                j.set("from", from.clone()).set("to", to.clone()).set("item", item.clone())
            }
            Edit::Unwire { from, to } => j.set("from", from.clone()).set("to", to.clone()),
            Edit::Name(name) => j.set("name", name.clone()),
            Edit::Deploy(n) => j.set("count", Json::big(*n as u128)),
        }
    }

    /// This edit, with everything that is only about where a box is drawn
    /// taken out. Two logs that differ by a drag describe the same factory,
    /// and a cache that thought otherwise would recompile on every pixel.
    pub fn key(&self) -> String {
        let mut j = self.to_json();
        if let (Json::Obj(fields), Edit::Place(_) | Edit::Retune(_)) = (&mut j, self) {
            if let Some((_, Json::Obj(node))) = fields.iter_mut().find(|(k, _)| k == "node") {
                node.retain(|(k, _)| k != "x" && k != "y");
            }
        }
        j.to_string()
    }

    pub fn from_json(j: &Json) -> Result<Edit, String> {
        let op = j.at("op").as_str().ok_or("a command has no op")?;
        let name = || j.at("name").as_str().unwrap_or_default().to_string();
        let from = || j.at("from").as_str().unwrap_or_default().to_string();
        let to = || j.at("to").as_str().unwrap_or_default().to_string();
        Ok(match op {
            "item" => Edit::Item(name()),
            "place" => Edit::Place(Node::from_json(j.at("node"))?),
            "retune" => Edit::Retune(Node::from_json(j.at("node"))?),
            "remove" => Edit::Remove(name()),
            "wire" => Edit::Wire {
                from: from(),
                to: to(),
                item: j.at("item").as_str().map(str::to_string),
            },
            "unwire" => Edit::Unwire { from: from(), to: to() },
            "name" => Edit::Name(name()),
            "deploy" => Edit::Deploy(j.at("count").as_u64().unwrap_or(1).max(1)),
            other => return Err(format!("unknown command `{other}`")),
        })
    }
}

/// An edit and the tick it happens at.
#[derive(Clone, Debug, PartialEq)]
pub struct Command {
    pub at: Tick,
    pub edit: Edit,
}

impl Command {
    pub fn to_json(&self) -> Json {
        self.edit.to_json().set("at", self.at)
    }
    pub fn from_json(j: &Json) -> Result<Command, String> {
        Ok(Command { at: j.at("at").as_u64().unwrap_or(0), edit: Edit::from_json(j)? })
    }
}

/// A plant, and everything that has happened to it.
#[derive(Clone, Debug, PartialEq)]
pub struct Log {
    pub base: Graph,
    pub commands: Vec<Command>,
}

impl Log {
    pub fn new(base: Graph) -> Log {
        Log { base, commands: Vec::new() }
    }

    /// The document as it stands at tick `t`.
    pub fn graph_at(&self, t: Tick) -> Result<Graph, Fault> {
        let mut g = self.base.clone();
        let mut last = 0;
        for c in self.commands.iter().filter(|c| c.at <= t) {
            if c.at < last {
                return Err(Fault::at(c.at, "the command log is out of order").about(&g));
            }
            last = c.at;
            // A refused edit leaves the document untouched -- which is a
            // tested property, not a hope -- so what comes back is the plant as
            // it stood before the command nobody could apply.
            if let Err(e) = c.edit.apply(&mut g) {
                return Err(Fault::at(c.at, &e).about(&g));
            }
        }
        Ok(g)
    }

    /// The ticks at which the plant becomes a different plant, up to `t`.
    ///
    /// Always starts at 0, and never repeats: several edits at one tick are
    /// one recompile, so a player who places a machine, wires it up and sets
    /// its recipe in the same instant pays for one barrier rather than three.
    pub fn boundaries(&self, t: Tick) -> Vec<Tick> {
        let mut v = vec![0];
        for c in &self.commands {
            if c.at > t {
                break;
            }
            if c.at > 0 && *v.last().unwrap() != c.at {
                v.push(c.at);
            }
        }
        v
    }

    /// A stable identity for the prefix of this log up to and including tick
    /// `t`. Two logs with the same key describe the same plant history and
    /// therefore have the same state at every tick in range.
    pub fn key(&self, t: Tick) -> String {
        let mut s = strip_layout(&self.base.emit());
        for c in self.commands.iter().filter(|c| c.at <= t) {
            s.push_str(&format!("\n@{} {}", c.at, c.edit.key()));
        }
        s
    }

    pub fn to_json(&self) -> Json {
        Json::obj().set("base", self.base.to_json()).set(
            "commands",
            Json::Arr(self.commands.iter().map(Command::to_json).collect()),
        )
    }

    pub fn from_json(j: &Json) -> Result<Log, String> {
        let base = Graph::from_json(j.at("base"))?;
        let mut commands = Vec::new();
        for c in j.at("commands").as_arr() {
            commands.push(Command::from_json(c)?);
        }
        Ok(Log { base, commands })
    }
}

/// A position is a comment, so two documents that differ only in where their
/// boxes are drawn are the same plant -- and dragging a node across a canvas
/// must not throw away a compiled plan or a running Room.
fn strip_layout(src: &str) -> String {
    src.lines().filter(|l| !l.trim_start().starts_with("# @pos ")).collect::<Vec<_>>().join("\n")
}

/// Why a log could not be answered.
///
/// # Two failures that are not the same failure
///
/// A command can be **refused**: wiring two bays together, naming a node that
/// already exists, retuning something that is not there. That command can
/// never work, on any machine, at any tick, and it does not belong on the log.
///
/// A plant can simply **not compile yet**. A processor you have just placed and
/// not wired up produces items with nowhere to go, and the language says so --
/// but the *document* is fine, and so is the command that made it. That is what
/// a factory looks like halfway through being built.
///
/// Prototype 0 never had to tell these apart, because the browser applied its
/// own edits and drew a half-built plant next to a red error. Prototype 1 made
/// the server authoritative, and the first thing that broke was placing a
/// machine: the plant did not compile, so no document came back, so nothing
/// appeared. Hence `refused`, and hence a fault that carries the document it is
/// complaining about.
#[derive(Clone, Debug)]
pub struct Fault {
    pub msg: String,
    /// The command that caused it, when it was a command and not the base
    /// plant.
    pub at: Option<Tick>,
    pub line: Option<usize>,
    pub node: Option<String>,
    pub source: Option<String>,
    /// True when a command was rejected outright, and should come back off the
    /// log. False when the plant is merely unfinished.
    pub refused: bool,
    /// The document this is about, so a view can still draw the factory it
    /// cannot yet run.
    pub graph: Option<Graph>,
}

impl Fault {
    pub fn new(msg: &str) -> Fault {
        Fault {
            msg: msg.into(),
            at: None,
            line: None,
            node: None,
            source: None,
            refused: false,
            graph: None,
        }
    }
    /// A command that was rejected.
    pub fn at(t: Tick, msg: &str) -> Fault {
        Fault { at: Some(t), refused: true, ..Fault::new(msg) }
    }
    /// The document as it stood, for a view that still has to draw something.
    pub fn about(mut self, g: &Graph) -> Fault {
        self.graph = Some(g.clone());
        self
    }
    pub fn of_dsl(e: &dsl::DslError, src: &str) -> Fault {
        Fault::dsl(None, e, src)
    }
    fn dsl(at: Option<Tick>, e: &dsl::DslError, src: &str) -> Fault {
        Fault {
            msg: e.msg.clone(),
            at,
            line: (e.line > 0).then_some(e.line),
            node: blamed(&e.msg).or_else(|| node_on_line(src, e.line)),
            source: Some(src.to_string()),
            refused: false,
            graph: None,
        }
    }
    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("ok", false)
            .set("error", self.msg.clone())
            .set("at", self.at.map(|t| Json::Int(t as i128)))
            .set("line", self.line.map(|l| Json::Int(l as i128)))
            .set("node", self.node.clone())
            .set("source", self.source.clone())
            .set("refused", self.refused)
            .set("graph", match &self.graph {
                Some(g) => g.to_json(),
                None => Json::Null,
            })
    }
}

/// The node a message is about, when the message names one.
///
/// Errors about the plant as a whole -- `Smelter produces items but has
/// nowhere to put them` -- carry no line at all, and the first version of this
/// dutifully blamed line zero, which is `item CrudeOre`. A backticked name in
/// the message is a better witness than a line number that was never set.
fn blamed(msg: &str) -> Option<String> {
    let name = msg.split('`').nth(1)?;
    (!name.is_empty()
        && name.chars().all(|c| c.is_alphanumeric() || c == '_')
        && name.chars().next().is_some_and(|c| c.is_alphabetic()))
    .then(|| name.to_string())
}

/// The generated source is one declaration per line, so a line number names a
/// node -- which is what puts a red ring on a canvas rather than a line number
/// in a panel nobody is looking at.
fn node_on_line(src: &str, line: usize) -> Option<String> {
    if line == 0 {
        return None;
    }
    let text = src.lines().nth(line.saturating_sub(1))?;
    text.split_whitespace()
        .find(|w| {
            !matches!(*w, "shared" | "source" | "storage" | "process" | "sink" | "link" | "wire")
        })
        .map(|w| w.trim_matches(|c: char| !c.is_alphanumeric() && c != '_').to_string())
        .filter(|w| !w.is_empty())
}

// ==================================================================== carry

/// A machine or a batch that an edit destroyed.
///
/// Reported rather than swallowed. Demolishing a bay that still has four
/// hundred ore in it is a decision with a cost, and a game that quietly
/// deleted the ore would be lying to the player about what they just did.
#[derive(Clone, Debug, PartialEq)]
pub struct Scrap {
    pub what: String,
    pub detail: String,
}

/// The state of a plant at one instant, addressed by name.
///
/// This is the only thing that survives an edit, and it is also -- with no
/// changes at all -- the canonical snapshot a joining client would be sent.
#[derive(Clone, Debug, Default)]
pub struct Carry {
    pub now: Tick,
    /// (bay, item) -> quantity.
    pub qty: HashMap<(String, String), Qty>,
    /// class -> the four buckets, with both ends of a lifted transport already
    /// merged back together.
    pub classes: HashMap<String, ClassPop>,
    /// bay -> the class each of its two round-robin pointers is resting on.
    ///
    /// A pointer is an index into a client list, and an edit is exactly the
    /// thing that can change that list. Carrying the index would silently
    /// hand the next turn to a different machine, which is a policy change
    /// nobody asked for; carrying the *client* survives the list moving.
    pub rr: HashMap<String, [Option<String>; 2]>,
    pub produced: HashMap<String, u64>,
    pub consumed: HashMap<String, u64>,
    pub cycles: HashMap<String, u64>,
}

impl Carry {
    /// Everything the successor plant needs, taken at a barrier.
    pub fn take(room: &Room, prog: &Program, bp: &Blueprint, now: Tick) -> Carry {
        let seed = room.harvest(bp);
        let mut c = Carry { now, ..Carry::default() };
        for (s, sd) in bp.storages.iter().enumerate() {
            for (k, &item) in sd.slots.iter().enumerate() {
                let q = seed.qty[sd.qty_offset as usize + k];
                if q > 0 {
                    c.qty.insert((sd.name.clone(), prog.item_name(item).to_string()), q);
                }
            }
            let named = |queue: &[u16], p: u16| -> Option<String> {
                queue.get(p as usize).map(|&cl| bp.actors[cl as usize].name.clone())
            };
            c.rr.insert(
                sd.name.clone(),
                [named(&sd.givers, seed.rr[s * 2]), named(&sd.takers, seed.rr[s * 2 + 1])],
            );
        }
        for (i, a) in bp.actors.iter().enumerate() {
            c.classes.insert(a.name.clone(), seed.classes[i].clone());
            c.cycles.insert(a.name.clone(), seed.c.cycles[i]);
        }
        for (i, name) in prog.items.iter().enumerate() {
            if seed.c.produced[i] > 0 {
                c.produced.insert(name.clone(), seed.c.produced[i]);
            }
            if seed.c.consumed[i] > 0 {
                c.consumed.insert(name.clone(), seed.c.consumed[i]);
            }
        }
        c
    }

    /// Pour this state into a plant that may be a different plant.
    ///
    /// Everything the new plant has and the old one did not starts empty and
    /// idle: a bay you just built is empty, a machine you just bought is
    /// asking for work. Everything the old plant had and the new one does not
    /// is scrap.
    pub fn seed(&self, prog: &Program, bp: &Blueprint) -> (pop::Seed, Vec<Scrap>) {
        let mut scrap: Vec<Scrap> = Vec::new();
        let mut qty = vec![0 as Qty; bp.qty_stride as usize];
        let mut placed: Vec<(String, String)> = Vec::new();
        for (s, sd) in bp.storages.iter().enumerate() {
            for (k, &item) in sd.slots.iter().enumerate() {
                let key = (sd.name.clone(), prog.item_name(item).to_string());
                if let Some(&q) = self.qty.get(&key) {
                    qty[sd.qty_offset as usize + k] = q;
                    placed.push(key);
                }
            }
            let _ = s;
        }
        let mut lost: Vec<&(String, String)> =
            self.qty.keys().filter(|k| !placed.contains(k)).collect();
        lost.sort();
        for k in lost {
            scrap.push(Scrap {
                what: k.0.clone(),
                detail: format!("{} {} scrapped with the bay", self.qty[k], k.1),
            });
        }

        let mut classes: Vec<ClassPop> = Vec::with_capacity(bp.actors.len());
        for a in &bp.actors {
            match self.classes.get(&a.name) {
                None => classes.push(fresh(a.count)),
                Some(old) => {
                    let (p, dropped) = reconcile(old, a.count);
                    if dropped > 0 {
                        scrap.push(Scrap {
                            what: a.name.clone(),
                            detail: format!(
                                "{dropped} of {} taken out of service mid-cycle",
                                old.total()
                            ),
                        });
                    }
                    classes.push(p);
                }
            }
        }
        let mut gone: Vec<&String> =
            self.classes.keys().filter(|n| !bp.actors.iter().any(|a| a.name == **n)).collect();
        gone.sort();
        for n in gone {
            let p = &self.classes[n];
            if p.done > 0 || p.working_total() > 0 {
                scrap.push(Scrap {
                    what: n.clone(),
                    detail: format!(
                        "{} demolished holding {} finished and {} unfinished batches",
                        n,
                        p.done,
                        p.working_total()
                    ),
                });
            }
        }

        let mut rr = vec![0u16; bp.storages.len() * 2];
        for (s, sd) in bp.storages.iter().enumerate() {
            let Some(names) = self.rr.get(&sd.name) else { continue };
            for q in 0..2 {
                let queue = if q == 0 { &sd.givers } else { &sd.takers };
                if let Some(name) = &names[q] {
                    if let Some(p) = queue
                        .iter()
                        .position(|&c| bp.actors[c as usize].name == *name)
                    {
                        rr[s * 2 + q] = p as u16;
                    }
                }
            }
        }

        let mut c = crate::sim::Counters::zeroed(bp.actors.len(), prog.items.len());
        for (i, a) in bp.actors.iter().enumerate() {
            c.cycles[i] = self.cycles.get(&a.name).copied().unwrap_or(0);
        }
        for (i, name) in prog.items.iter().enumerate() {
            c.produced[i] = self.produced.get(name).copied().unwrap_or(0);
            c.consumed[i] = self.consumed.get(name).copied().unwrap_or(0);
        }
        (pop::Seed { qty, classes, rr, c }, scrap)
    }

    /// A canonical encoding, for comparing one client's idea of tick T with
    /// another's. Sorted by name, so it does not depend on how either of them
    /// happened to index the plant.
    pub fn signature(&self) -> Vec<u8> {
        let mut v = Vec::new();
        v.extend_from_slice(&self.now.to_le_bytes());
        let mut qty: Vec<_> = self.qty.iter().collect();
        qty.sort();
        for ((bay, item), q) in qty {
            v.extend_from_slice(bay.as_bytes());
            v.push(b'/');
            v.extend_from_slice(item.as_bytes());
            v.extend_from_slice(&q.to_le_bytes());
        }
        let mut classes: Vec<_> = self.classes.iter().collect();
        classes.sort_by(|a, b| a.0.cmp(b.0));
        for (name, p) in classes {
            v.extend_from_slice(name.as_bytes());
            v.extend_from_slice(&p.starved.to_le_bytes());
            v.extend_from_slice(&p.done.to_le_bytes());
            for (dl, n) in &p.working {
                v.extend_from_slice(&dl.saturating_sub(self.now).to_le_bytes());
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.push(0xfe);
            for (dl, n) in &p.returning {
                v.extend_from_slice(&dl.saturating_sub(self.now).to_le_bytes());
                v.extend_from_slice(&n.to_le_bytes());
            }
            v.push(0xff);
        }
        let mut rr: Vec<_> = self.rr.iter().collect();
        rr.sort_by(|a, b| a.0.cmp(b.0));
        for (bay, who) in rr {
            v.extend_from_slice(bay.as_bytes());
            for w in who {
                v.extend_from_slice(w.as_deref().unwrap_or("-").as_bytes());
                v.push(b',');
            }
        }
        v
    }

    pub fn to_json(&self) -> Json {
        let bucket = |v: &[(Tick, u64)]| {
            Json::arr(
                v.iter()
                    .map(|&(at, n)| Json::obj().set("at", at).set("n", Json::big(n as u128)))
                    .collect::<Vec<_>>(),
            )
        };
        let mut qty: Vec<_> = self.qty.iter().collect();
        qty.sort();
        let mut classes: Vec<_> = self.classes.iter().collect();
        classes.sort_by(|a, b| a.0.cmp(b.0));
        let mut rr: Vec<_> = self.rr.iter().collect();
        rr.sort_by(|a, b| a.0.cmp(b.0));
        let totals = |m: &HashMap<String, u64>| {
            let mut v: Vec<_> = m.iter().collect();
            v.sort_by(|a, b| a.0.cmp(b.0));
            Json::Arr(
                v.into_iter()
                    .map(|(k, n)| Json::obj().set("name", k.clone()).set("n", Json::big(*n as u128)))
                    .collect(),
            )
        };
        Json::obj()
            .set("now", self.now)
            .set(
                "qty",
                Json::Arr(
                    qty.into_iter()
                        .map(|((bay, item), q)| {
                            Json::obj()
                                .set("bay", bay.clone())
                                .set("item", item.clone())
                                .set("qty", Json::big(*q as u128))
                        })
                        .collect(),
                ),
            )
            .set(
                "classes",
                Json::Arr(
                    classes
                        .into_iter()
                        .map(|(name, p)| {
                            Json::obj()
                                .set("name", name.clone())
                                .set("starved", Json::big(p.starved as u128))
                                .set("done", Json::big(p.done as u128))
                                .set("working", bucket(&p.working))
                                .set("returning", bucket(&p.returning))
                        })
                        .collect(),
                ),
            )
            .set(
                "rr",
                Json::Arr(
                    rr.into_iter()
                        .map(|(bay, who)| {
                            Json::obj()
                                .set("bay", bay.clone())
                                .set("givers", who[0].clone())
                                .set("takers", who[1].clone())
                        })
                        .collect(),
                ),
            )
            .set("produced", totals(&self.produced))
            .set("consumed", totals(&self.consumed))
            .set("cycles", totals(&self.cycles))
    }

    pub fn from_json(j: &Json) -> Result<Carry, String> {
        let bucket = |v: &Json| -> Vec<(Tick, u64)> {
            v.as_arr()
                .iter()
                .filter_map(|e| Some((e.at("at").as_u64()?, e.at("n").as_u64()?)))
                .collect()
        };
        let mut c = Carry { now: j.at("now").as_u64().unwrap_or(0), ..Carry::default() };
        for e in j.at("qty").as_arr() {
            let (Some(bay), Some(item), Some(q)) =
                (e.at("bay").as_str(), e.at("item").as_str(), e.at("qty").as_u64())
            else {
                return Err("a carried quantity is malformed".into());
            };
            c.qty.insert((bay.to_string(), item.to_string()), q);
        }
        for e in j.at("classes").as_arr() {
            let name = e.at("name").as_str().ok_or("a carried class has no name")?;
            c.classes.insert(
                name.to_string(),
                ClassPop {
                    working: bucket(e.at("working")),
                    starved: e.at("starved").as_u64().unwrap_or(0),
                    done: e.at("done").as_u64().unwrap_or(0),
                    returning: bucket(e.at("returning")),
                },
            );
        }
        for e in j.at("rr").as_arr() {
            let bay = e.at("bay").as_str().ok_or("a carried pointer has no bay")?;
            c.rr.insert(
                bay.to_string(),
                [
                    e.at("givers").as_str().map(str::to_string),
                    e.at("takers").as_str().map(str::to_string),
                ],
            );
        }
        for (field, into) in
            [("produced", 0), ("consumed", 1), ("cycles", 2)]
        {
            for e in j.at(field).as_arr() {
                let (Some(name), Some(n)) = (e.at("name").as_str(), e.at("n").as_u64()) else {
                    return Err(format!("a carried {field} total is malformed"));
                };
                let m = match into {
                    0 => &mut c.produced,
                    1 => &mut c.consumed,
                    _ => &mut c.cycles,
                };
                m.insert(name.to_string(), n);
            }
        }
        Ok(c)
    }
}

fn fresh(count: u64) -> ClassPop {
    ClassPop { working: Vec::new(), starved: count, done: 0, returning: Vec::new() }
}

/// Make a population of `have` members into one of `want`.
///
/// Growing is easy: a machine you have just bought is idle and asking, exactly
/// as every machine is at t=0.
///
/// Shrinking has to choose which members to take away, and the choice is a
/// game rule rather than a physical fact, so it is made once, here, and made
/// in the order that destroys the least: idle members first, then vehicles
/// running home empty, then members holding a finished batch, then members
/// mid-cycle. Only the last two lose anything, and a player who scales a line
/// down while it is loaded is told what it cost.
fn reconcile(old: &ClassPop, want: u64) -> (ClassPop, u64) {
    let have = old.total();
    let mut p = old.clone();
    if want == have {
        return (p, 0);
    }
    if want > have {
        p.starved += want - have;
        return (p, 0);
    }
    let mut drop = have - want;
    let mut lost = 0;
    let take = |n: &mut u64, drop: &mut u64| {
        let k = (*n).min(*drop);
        *n -= k;
        *drop -= k;
        k
    };
    take(&mut p.starved, &mut drop);
    // Latest first: the vehicle furthest from being useful again.
    while drop > 0 {
        let Some(last) = p.returning.last_mut() else { break };
        let k = last.1.min(drop);
        last.1 -= k;
        drop -= k;
        if last.1 == 0 {
            p.returning.pop();
        }
    }
    lost += take(&mut p.done, &mut drop);
    while drop > 0 {
        let Some(last) = p.working.last_mut() else { break };
        let k = last.1.min(drop);
        last.1 -= k;
        drop -= k;
        lost += k;
        if last.1 == 0 {
            p.working.pop();
        }
    }
    (p, lost)
}

// =================================================================== running

/// Everything a caller may look at, at the tick it asked about.
pub struct At<'a> {
    pub prog: &'a Program,
    pub bp: &'a Blueprint,
    pub plan: &'a Plan,
    pub room: &'a Room<'a>,
    pub graph: &'a Graph,
    pub source: &'a str,
    pub tick: Tick,
    /// What the edits along the way destroyed.
    pub scrapped: &'a [Scrap],
}

/// Compile one epoch of a log: the plant as of tick `from`.
fn compile(log: &Log, from: Tick) -> Result<(Graph, Program, String), Fault> {
    let g = log.graph_at(from)?;
    let src = g.emit();
    let prog = dsl::parse(&src)
        .map_err(|e| Fault::dsl(if from == 0 { None } else { Some(from) }, &e, &src).about(&g))?;
    if prog.deploys.is_empty() {
        return Err(Fault::new("the plant is never deployed").about(&g));
    }
    Ok((g, prog, src))
}

/// Answer a question about a log at tick `t`, starting from a state already
/// known at some boundary.
///
/// `start` names a boundary whose state is already known; passing `None`
/// starts at tick 0 with a fresh plant. This is the whole of the incremental
/// story: with a carry in hand, answering tick 80,000 costs one epoch of
/// simulation rather than 80,000 ticks of it.
///
/// `ticks` must be ascending. Asking about several at once matters more than
/// it looks: a scenario wants to know what was delivered at four different
/// deadlines, and asking four times would run the plant four times.
pub fn with_states(
    log: &Log,
    ticks: &[Tick],
    start: Option<(Tick, &Carry)>,
    record: bool,
    mut f: impl FnMut(At),
) -> Result<Timeline, Fault> {
    let t = ticks.last().copied().unwrap_or(0);
    let mut bounds = log.boundaries(t);
    let mut carry: Option<Carry> = None;
    // A resumption point need not be a boundary. The plant is the same plant
    // everywhere inside an epoch, so a state cached at tick 40,137 is a
    // perfectly good place to start the epoch that contains it -- which is what
    // makes scrubbing a timeline cost the ticks between two answers rather
    // than the ticks since the beginning.
    if let Some((from, c)) = start {
        let i = bounds
            .iter()
            .rposition(|&b| b <= from)
            .ok_or_else(|| Fault::new("resumed from before the plant existed"))?;
        bounds.drain(..i);
        bounds[0] = from;
        carry = Some(c.clone());
    }
    let first = bounds[0];

    let mut tl = Timeline::default();
    let mut scrapped: Vec<Scrap> = Vec::new();
    for (i, &from) in bounds.iter().enumerate() {
        let last_epoch = i + 1 == bounds.len();
        let next = bounds.get(i + 1).copied().unwrap_or(Tick::MAX);
        let (g, prog, src) = compile(log, from)?;
        let bp = &prog.blueprints[prog.deploys[0].blueprint as usize];
        let plan = rooms::plan(bp);
        let mut room = match &carry {
            None => Room::new(&plan, prog.items.len()),
            Some(c) => {
                let (seed, mut lost) = c.seed(&prog, bp);
                scrapped.append(&mut lost);
                Room::resume(&plan, bp, prog.items.len(), from, seed)
            }
        };
        if record {
            room.trace = Some(Vec::new());
            tl.epochs.push(from);
        }
        // An edit takes effect at its own tick, so a probe that lands exactly
        // on a boundary belongs to the plant the edit made, not the one it
        // replaced.
        for &probe in ticks.iter().filter(|&&p| p >= from && p >= first && p < next) {
            room.run_until(probe);
            f(At {
                prog: &prog,
                bp,
                plan: &plan,
                room: &room,
                graph: &g,
                source: &src,
                tick: probe,
                scrapped: &scrapped,
            });
        }
        if last_epoch {
            tl.absorb(&room);
            return Ok(tl);
        }
        room.run_until(next);
        tl.absorb(&room);
        carry = Some(Carry::take(&room, &prog, bp, next));
    }
    unreachable!("boundaries() always yields at least tick 0")
}

/// The scheduler's own log of a whole run, across however many plants the run
/// went through.
///
/// One `Room` records the advances of one epoch, and a run with three edits in
/// it is three Rooms. The advances carry absolute ticks, so they simply
/// concatenate -- and the picture that comes out is the one worth having: the
/// timetable of a *factory*, with the moments it was rebuilt marked on it.
#[derive(Default)]
pub struct Timeline {
    pub advances: Vec<rooms::Advance>,
    pub steps: u64,
    pub messages: u64,
    pub rendezvous: u64,
    pub max_skew: Tick,
    pub max_advance: Tick,
    pub total_advance: u128,
    /// The tick each epoch began at.
    pub epochs: Vec<Tick>,
}

impl Timeline {
    fn absorb(&mut self, room: &Room) {
        if let Some(t) = &room.trace {
            self.advances.extend_from_slice(t);
        }
        self.steps += room.steps;
        self.messages += room.messages;
        self.rendezvous += room.rendezvous;
        self.max_skew = self.max_skew.max(room.max_skew);
        self.max_advance = self.max_advance.max(room.max_advance);
        self.total_advance += room.total_advance;
    }

    pub fn mean_advance(&self) -> f64 {
        if self.steps == 0 {
            0.0
        } else {
            self.total_advance as f64 / self.steps as f64
        }
    }

    pub fn to_json(&self) -> Json {
        // A picture of a run needs enough bars to read, not every bar there
        // was.
        const DRAWN: usize = 4_000;
        Json::obj()
            .set(
                "advances",
                Json::Arr(
                    self.advances
                        .iter()
                        .take(DRAWN)
                        .map(|a| {
                            Json::obj()
                                .set("region", a.region)
                                .set("from", a.from)
                                .set("to", a.to)
                                .set("blocked", a.blocked)
                        })
                        .collect(),
                ),
            )
            .set("recorded", self.advances.len())
            .set("truncated", self.advances.len() > DRAWN)
            .set("steps", Json::big(self.steps as u128))
            .set("messages", Json::big(self.messages as u128))
            .set("rendezvous", Json::big(self.rendezvous as u128))
            .set("maxSkew", self.max_skew)
            .set("maxAdvance", self.max_advance)
            .set("meanAdvance", self.mean_advance())
            .set("epochs", Json::arr(self.epochs.iter().map(|&t| t as i64).collect::<Vec<_>>()))
            .set("skewClocks", Json::arr(Vec::<i64>::new()))
    }
}

/// The scheduler's log of running this log to `t`.
pub fn timetable(log: &Log, t: Tick) -> Result<Json, Fault> {
    Ok(with_states(log, &[t], None, true, |_| {})?.to_json())
}

pub fn with_state_from<R>(
    log: &Log,
    t: Tick,
    start: Option<(Tick, &Carry)>,
    f: impl FnOnce(At) -> R,
) -> Result<R, Fault> {
    let mut out = None;
    let mut f = Some(f);
    with_states(log, &[t], start, false, |a| {
        if let Some(f) = f.take() {
            out = Some(f(a));
        }
    })?;
    out.ok_or_else(|| Fault::new("the plant was never asked about"))
}

pub fn with_state<R>(log: &Log, t: Tick, f: impl FnOnce(At) -> R) -> Result<R, Fault> {
    with_state_from(log, t, None, f)
}

/// The plant's state at `t`, in the form the next epoch would need it.
pub fn carry_at(log: &Log, t: Tick) -> Result<Carry, Fault> {
    with_state(log, t, |a| Carry::take(a.room, a.prog, a.bp, t))
}
