//! The factory DSL: lexer, recursive-descent parser, and lowering to `Program`.
//!
//! ```text
//! item IronOre
//! item IronPlate
//!
//! blueprint SmeltLine {
//!     source  Miner   { produces 100 IronOre every 60 ticks }
//!     storage Bay     { capacity 1000 }
//!     process Smelter { consumes 10 IronOre takes 20 ticks produces 10 IronPlate }
//!
//!     wire Miner -> Bay -> Smelter -> Bay
//! }
//!
//! deploy 1000000 x SmeltLine stagger 7
//! ```
//!
//! `x N` after a machine name sets its **population**: one class of N
//! interchangeable machines, not N separate nodes. `x N` on a storage still
//! makes N distinct storages, because two storages are never interchangeable.
//!
//! v2 additions:
//!
//! ```text
//! storage Bay {
//!     capacity 10000
//!     initial 500 Catalyst        # seeds a cycle that would otherwise be dead
//!     policy round_robin          # index | round_robin | priority
//!     priority Smelter, GearPress # service order, for policy priority
//! }
//!
//! link Rail x2 { moves 12000 IronOre takes 3000 ticks }
//! wire StationA -> Rail -> StationB
//! ```
//!
//! A `link` is not a new primitive -- it lowers to a process whose outputs
//! equal its inputs and whose two ends are different storages. That is the
//! point: batch transport with latency is already expressible, and naming it
//! only lets the domain analysis recognise it.
//!
//! v3 additions:
//!
//! ```text
//! link Rail x4 {
//!     moves 4000 IronOre
//!     distance 1200 speed 2 base 50   # latency = 50 + 1200/2 = 650, both ways
//! }
//!
//! link Belt x40 { moves 10 IronOre takes 20 ticks returns 20 ticks }
//!
//! storage OreNet { shared  capacity 400000 }   # one bay for every deployed line
//! ```
//!
//! `returns` is the load-bearing addition. A v2 link teleported its vehicle
//! home the instant it unloaded, and a vehicle that comes back for free is a
//! zero-latency channel running *backwards* through the transport -- so the
//! loading end can never run ahead of the unloading end. Declaring the return
//! trip is what buys the sending region its causal slack. `distance` implies a
//! symmetric round trip, because a place that is far away is far away in both
//! directions.
//!
//! v3 also *removes* something: a machine may no longer draw one item from
//! several storages, or post one item to several storages. Material reaches a
//! machine through the logistics graph, not by reaching into every bay that
//! happens to contain what it wants.

use crate::model::*;
use std::collections::HashMap;
use std::fmt;

#[derive(Debug)]
pub struct DslError {
    pub line: usize,
    pub msg: String,
}

impl fmt::Display for DslError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "line {}: {}", self.line, self.msg)
    }
}

/// Deployments of lines that share some but not all of their storage have to
/// be written out line by line. Past this many, say so instead.
const SPREAD_CAP: u64 = 64;

macro_rules! bail {
    ($line:expr, $($t:tt)*) => {
        return Err(DslError { line: $line, msg: format!($($t)*) })
    };
}

// ---------------------------------------------------------------- lexer

#[derive(Clone, Debug, PartialEq)]
enum Tok {
    Ident(String),
    Num(u64),
    LBrace,
    RBrace,
    Arrow,
}

fn lex(src: &str) -> Result<Vec<(Tok, usize)>, DslError> {
    let mut out = Vec::new();
    for (lineno, raw) in src.lines().enumerate() {
        let line = lineno + 1;
        let text = match raw.find('#') {
            Some(i) => &raw[..i],
            None => raw,
        };
        let b = text.as_bytes();
        let mut i = 0;
        while i < b.len() {
            let c = b[i] as char;
            if c.is_whitespace() || c == ',' || c == ';' {
                i += 1;
            } else if c == '{' {
                out.push((Tok::LBrace, line));
                i += 1;
            } else if c == '}' {
                out.push((Tok::RBrace, line));
                i += 1;
            } else if c == '-' && i + 1 < b.len() && b[i + 1] == b'>' {
                out.push((Tok::Arrow, line));
                i += 2;
            } else if c.is_ascii_digit() {
                let start = i;
                while i < b.len() && ((b[i] as char).is_ascii_digit() || b[i] == b'_') {
                    i += 1;
                }
                let s: String = text[start..i].chars().filter(|c| *c != '_').collect();
                match s.parse::<u64>() {
                    Ok(n) => out.push((Tok::Num(n), line)),
                    Err(_) => bail!(line, "number `{s}` does not fit in 64 bits"),
                }
            } else if c.is_ascii_alphabetic() || c == '_' {
                let start = i;
                while i < b.len() && ((b[i] as char).is_ascii_alphanumeric() || b[i] == b'_') {
                    i += 1;
                }
                out.push((Tok::Ident(text[start..i].to_string()), line));
            } else {
                bail!(line, "unexpected character `{c}`");
            }
        }
    }
    Ok(out)
}

// ---------------------------------------------------------------- parser

struct Parser {
    toks: Vec<(Tok, usize)>,
    pos: usize,
}

impl Parser {
    fn line(&self) -> usize {
        self.toks
            .get(self.pos)
            .or_else(|| self.toks.last())
            .map(|t| t.1)
            .unwrap_or(0)
    }

    fn peek(&self) -> Option<&Tok> {
        self.toks.get(self.pos).map(|t| &t.0)
    }

    fn next(&mut self) -> Option<Tok> {
        let t = self.toks.get(self.pos).map(|t| t.0.clone());
        self.pos += 1;
        t
    }

    fn eat_word(&mut self, w: &str) -> bool {
        if let Some(Tok::Ident(s)) = self.peek() {
            if s == w {
                self.pos += 1;
                return true;
            }
        }
        false
    }

    fn expect_word(&mut self, w: &str) -> Result<(), DslError> {
        if self.eat_word(w) {
            Ok(())
        } else {
            bail!(self.line(), "expected `{w}`, found {}", self.describe())
        }
    }

    fn expect_ident(&mut self) -> Result<String, DslError> {
        let line = self.line();
        match self.next() {
            Some(Tok::Ident(s)) => Ok(s),
            _ => bail!(line, "expected a name"),
        }
    }

    fn expect_num(&mut self) -> Result<u64, DslError> {
        let line = self.line();
        match self.next() {
            Some(Tok::Num(n)) => Ok(n),
            _ => bail!(line, "expected a number"),
        }
    }

    fn expect(&mut self, t: Tok) -> Result<(), DslError> {
        let line = self.line();
        if self.next().as_ref() == Some(&t) {
            Ok(())
        } else {
            bail!(line, "expected {t:?}")
        }
    }

    fn describe(&self) -> String {
        match self.peek() {
            Some(Tok::Ident(s)) => format!("`{s}`"),
            Some(Tok::Num(n)) => format!("`{n}`"),
            Some(Tok::LBrace) => "`{`".into(),
            Some(Tok::RBrace) => "`}`".into(),
            Some(Tok::Arrow) => "`->`".into(),
            None => "end of input".into(),
        }
    }

    /// Optional replication suffix after a node name. Both `x 4` and the
    /// glued form `x4` are accepted; the latter lexes as one identifier.
    fn multiplicity(&mut self) -> Result<u64, DslError> {
        let glued = match self.peek() {
            Some(Tok::Ident(s))
                if s.len() > 1
                    && s.starts_with('x')
                    && s[1..].bytes().all(|b| b.is_ascii_digit()) =>
            {
                Some(s[1..].to_string())
            }
            _ => None,
        };
        if let Some(digits) = glued {
            self.pos += 1;
            let n: u64 = digits
                .parse()
                .map_err(|_| DslError { line: self.line(), msg: format!("bad count `x{digits}`") })?;
            if n == 0 {
                bail!(self.line(), "replication count must be at least 1");
            }
            return Ok(n);
        }
        if self.eat_word("x") {
            let n = self.expect_num()?;
            if n == 0 {
                bail!(self.line(), "replication count must be at least 1");
            }
            Ok(n)
        } else {
            Ok(1)
        }
    }
}

// ---------------------------------------------------------------- interning

struct Items {
    names: Vec<String>,
    index: HashMap<String, ItemId>,
    declared: bool,
}

impl Items {
    fn intern(&mut self, name: &str, line: usize) -> Result<ItemId, DslError> {
        if let Some(&id) = self.index.get(name) {
            return Ok(id);
        }
        if self.declared {
            bail!(line, "unknown item `{name}` (items are declared explicitly in this file)");
        }
        if self.names.len() >= u16::MAX as usize {
            bail!(line, "too many distinct item types");
        }
        let id = self.names.len() as ItemId;
        self.names.push(name.to_string());
        self.index.insert(name.to_string(), id);
        Ok(id)
    }
}

// ---------------------------------------------------------------- lowering

/// A named node inside a blueprint. A machine name denotes exactly one class
/// however large its population is; a storage name may denote several distinct
/// storages if it was replicated.
enum Group {
    Storages(Vec<u16>),
    /// Index of the single actor class.
    Class(u16),
}

pub fn parse(src: &str) -> Result<Program, DslError> {
    let mut p = Parser { toks: lex(src)?, pos: 0 };

    // Pass 1: item declarations, so `declared` mode is known before blueprints.
    let mut items = Items { names: Vec::new(), index: HashMap::new(), declared: false };
    {
        let mut q = Parser { toks: p.toks.clone(), pos: 0 };
        while let Some(t) = q.peek() {
            if matches!(t, Tok::Ident(s) if s == "item") {
                let line = q.line();
                q.pos += 1;
                let name = q.expect_ident()?;
                if items.index.contains_key(&name) {
                    bail!(line, "item `{name}` declared twice");
                }
                items.intern(&name, line)?;
            } else {
                q.pos += 1;
            }
        }
        items.declared = !items.names.is_empty();
    }

    let mut blueprints: Vec<Blueprint> = Vec::new();
    let mut bp_index: HashMap<String, u32> = HashMap::new();
    let mut deploys: Vec<Deploy> = Vec::new();

    while p.peek().is_some() {
        let line = p.line();
        if p.eat_word("item") {
            p.expect_ident()?;
        } else if p.eat_word("blueprint") {
            let name = p.expect_ident()?;
            if bp_index.contains_key(&name) {
                bail!(line, "blueprint `{name}` declared twice");
            }
            let bp = parse_blueprint(&mut p, &mut items, name.clone())?;
            bp_index.insert(name, blueprints.len() as u32);
            blueprints.push(bp);
        } else if p.eat_word("deploy") {
            let count = p.expect_num()?;
            p.expect_word("x")?;
            let bpname = p.expect_ident()?;
            let Some(&id) = bp_index.get(&bpname) else {
                bail!(line, "deploy of unknown blueprint `{bpname}`");
            };
            let stagger = if p.eat_word("stagger") { p.expect_num()? } else { 0 };
            deploys.push(Deploy { blueprint: id, count, stagger, origin: None });
        } else {
            bail!(line, "expected `item`, `blueprint` or `deploy`, found {}", p.describe());
        }
    }

    if blueprints.is_empty() {
        bail!(0, "program declares no blueprints");
    }

    // ---- deployments that share infrastructure -------------------------
    //
    // Until now a deployment was a stack of lines that never touched each
    // other, which is what let T4 answer a billion of them with a handful of
    // phase archetypes. A shared storage destroys that outright: line 1 and
    // line 1,000,000 are now competing for the same ore.
    //
    // The compression has to move up a level to survive, and it can -- but
    // only when *nothing* is private. With every storage shared there is no
    // per-line state left, so two lines have nothing that could distinguish
    // them, their machines are interchangeable, and `N` lines of `k` machines
    // are one class of `N * k`. Give a line a buffer of its own and that
    // buffer is precisely the state that tells lines apart, and the argument
    // stops working. `Blueprint::spread` is what it would mean instead, and it
    // does not scale, which is the honest answer rather than a missing one.
    let mut fused: Vec<Blueprint> = Vec::new();
    for d in deploys.iter_mut() {
        let bp = &blueprints[d.blueprint as usize];
        if !bp.has_shared() || d.count <= 1 {
            continue;
        }
        if d.stagger != 0 {
            bail!(
                0,
                "`{}` has shared storage, so its lines are not independent and \
                 cannot be staggered against each other",
                bp.name
            );
        }
        let lines = d.count;
        let collapsed = bp.all_shared();
        if !collapsed && lines > SPREAD_CAP {
            let private: Vec<&str> = bp
                .storages
                .iter()
                .filter(|s| !s.shared)
                .map(|s| s.name.as_str())
                .collect();
            bail!(
                0,
                "`{}` shares some storage but keeps {} private ({}), and {} lines \
                 is past the {} that can be written out one by one. Lines with \
                 private state are not interchangeable, so this deployment cannot \
                 be collapsed into populations either. Share the rest, or deploy fewer.",
                bp.name,
                private.len(),
                private.join(", "),
                lines,
                SPREAD_CAP
            );
        }
        // Collapse where the lines are interchangeable, and otherwise fall
        // back to writing them out: an exact answer at a worse price is still
        // an exact answer, and refusing to answer is not.
        fused.push(if collapsed { bp.collapse(lines) } else { bp.spread(lines) });
        d.origin = Some(Origin { blueprint: d.blueprint, lines, collapsed });
        d.blueprint = (blueprints.len() + fused.len() - 1) as u32;
        d.count = 1;
    }
    blueprints.extend(fused);

    Ok(Program { items: items.names, blueprints, deploys })
}

fn parse_blueprint(p: &mut Parser, items: &mut Items, name: String) -> Result<Blueprint, DslError> {
    p.expect(Tok::LBrace)?;

    let mut storages: Vec<StorageDef> = Vec::new();
    let mut actors: Vec<ActorDef> = Vec::new();
    let mut groups: HashMap<String, Group> = HashMap::new();
    // (from, to, item filter, line) resolved after all nodes are known.
    // An empty filter means "everything this machine handles".
    let mut wires: Vec<(String, String, Vec<String>, usize)> = Vec::new();
    // Declared priority lists, resolved to class indices once wiring is known.
    let mut prio: Vec<(u16, Vec<String>, usize)> = Vec::new();

    loop {
        let line = p.line();
        match p.peek() {
            Some(Tok::RBrace) => {
                p.pos += 1;
                break;
            }
            None => bail!(line, "unterminated blueprint `{name}`"),
            _ => {}
        }

        if p.eat_word("wire") {
            let mut prev = p.expect_ident()?;
            if p.peek() != Some(&Tok::Arrow) {
                bail!(line, "a wire needs at least one `->`");
            }
            while p.peek() == Some(&Tok::Arrow) {
                p.pos += 1;
                let next = p.expect_ident()?;
                // `-> CatBay { Catalyst }` restricts what may travel this way.
                // Without it a machine with two outputs and two output bays has
                // no way to say which goes where, and the greedy fill will
                // happily post product into the catalyst tank.
                let mut filter = Vec::new();
                if p.peek() == Some(&Tok::LBrace) {
                    p.pos += 1;
                    while let Some(Tok::Ident(w)) = p.peek() {
                        filter.push(w.clone());
                        p.pos += 1;
                    }
                    p.expect(Tok::RBrace)?;
                    if filter.is_empty() {
                        bail!(line, "`{prev} -> {next} {{}}` names no items");
                    }
                }
                wires.push((prev.clone(), next.clone(), filter, line));
                prev = next;
            }
            continue;
        }

        // `shared` in front of anything means "one of these for the whole
        // deployment", not one per deployed line.
        let shared_decl = p.eat_word("shared");
        let kindword = match p.peek() {
            Some(Tok::Ident(s)) => s.clone(),
            _ => bail!(line, "expected a node declaration, found {}", p.describe()),
        };
        p.pos += 1;
        let nodename = p.expect_ident()?;
        let mult = p.multiplicity()?;
        if groups.contains_key(&nodename) {
            bail!(line, "`{nodename}` declared twice in blueprint `{name}`");
        }

        match kindword.as_str() {
            "storage" => {
                let body = parse_storage_body(p, items, &nodename)?;
                let mut ids = Vec::new();
                for k in 0..mult {
                    let id = storages.len() as u16;
                    ids.push(id);
                    if !body.priority.is_empty() {
                        prio.push((id, body.priority.clone(), line));
                    }
                    storages.push(StorageDef {
                        name: inst_name(&nodename, k, mult),
                        shared: body.shared || shared_decl,
                        capacity: body.capacity,
                        slots: Vec::new(),
                        initial: body.initial.clone(),
                        qty_offset: 0,
                        clients: Vec::new(),
                        policy: body.policy,
                        order: Vec::new(),
                        takers: Vec::new(),
                        givers: Vec::new(),
                    });
                }
                groups.insert(nodename, Group::Storages(ids));
            }
            "source" | "process" | "sink" | "link" => {
                let body = parse_actor_body(p, items, &kindword, &nodename)?;
                let kind = match kindword.as_str() {
                    "source" => ActorKind::Source,
                    "sink" => ActorKind::Sink,
                    "link" => ActorKind::Transport,
                    _ => ActorKind::Process,
                };
                // One class, whatever the population. This is the single most
                // important difference from v1: `x 10000` must not make the
                // blueprint ten thousand nodes long, or every analysis that
                // walks the blueprint scales with the object count again.
                let id = actors.len() as u16;
                actors.push(ActorDef {
                    name: nodename.clone(),
                    kind,
                    inputs: body.inputs,
                    outputs: body.outputs,
                    duration: body.duration,
                    return_latency: body.return_latency,
                    geometry: body.geometry,
                    shared: shared_decl,
                    count: mult,
                    machine_offset: 0,
                    in_stores: Vec::new(),
                    out_stores: Vec::new(),
                });
                groups.insert(nodename, Group::Class(id));
            }
            other => bail!(line, "unknown node kind `{other}`"),
        }
    }

    // ---- resolve wires -------------------------------------------------
    // (class, storage, 1 = deposits / 0 = withdrawals) -> permitted items.
    let mut filt: HashMap<(u16, u16, u8), Vec<ItemId>> = HashMap::new();
    for (from, to, names, line) in &wires {
        let mut allow = Vec::new();
        for n in names {
            allow.push(items.intern(n, *line)?);
        }
        let (Some(a), Some(b)) = (groups.get(from), groups.get(to)) else {
            let missing = if groups.contains_key(from) { to } else { from };
            bail!(*line, "wire references unknown node `{missing}`");
        };
        match (a, b) {
            (Group::Class(c), Group::Storages(strs)) => {
                for &s in strs {
                    push_unique(&mut actors[*c as usize].out_stores, s);
                    push_unique(&mut storages[s as usize].clients, *c);
                    if !allow.is_empty() {
                        filt.entry((*c, s, 1)).or_default().extend(allow.iter().copied());
                    }
                }
            }
            (Group::Storages(strs), Group::Class(c)) => {
                for &s in strs {
                    push_unique(&mut actors[*c as usize].in_stores, s);
                    push_unique(&mut storages[s as usize].clients, *c);
                    if !allow.is_empty() {
                        filt.entry((*c, s, 0)).or_default().extend(allow.iter().copied());
                    }
                }
            }
            (Group::Class(_), Group::Class(_)) => bail!(
                *line,
                "`{from} -> {to}` connects two machines; route them through a storage"
            ),
            (Group::Storages(_), Group::Storages(_)) => bail!(
                *line,
                "`{from} -> {to}` connects two storages; insert a machine between them"
            ),
        }
    }

    // ---- derive storage item slots -------------------------------------
    // A storage holds exactly what is *put into* it: the outputs of the
    // machines that deposit there, plus anything seeded at t=0.
    //
    // Deriving slots from consumers as well -- v1's rule -- quietly gives a bay
    // a slot for everything its customers happen to want. Wire an assembler
    // needing gears and copper to a gear bay and a copper bay, and both bays
    // acquire both slots; the gear bay is then, on paper, a place copper might
    // be, and every analysis that reasons per storage believes it. Nothing can
    // be withdrawn from a bay that nothing delivers to, so the producing side
    // is the only side that should define what a bay holds.
    for (ci, a) in actors.iter().enumerate() {
        for &s in &a.out_stores {
            let allow = filt.get(&(ci as u16, s, 1));
            for st in &a.outputs {
                if allow.map_or(true, |v| v.contains(&st.item)) {
                    push_unique_item(&mut storages[s as usize].slots, st.item);
                }
            }
        }
    }
    // Declared initial contents need a slot even if nothing is wired for them.
    for s in &mut storages {
        let seeds: Vec<ItemId> = s.initial.iter().map(|st| st.item).collect();
        for it in seeds {
            push_unique_item(&mut s.slots, it);
        }
    }
    let mut qty_stride = 0u32;
    for s in &mut storages {
        s.slots.sort_unstable();
        s.qty_offset = qty_stride;
        qty_stride += s.slots.len() as u32;
        s.clients.sort_unstable();
    }

    // An input-side qualifier is documentation, since a bay's contents are
    // fixed by who fills it -- but naming an item that never arrives there is
    // always a mistake, so say so.
    for ((c, s, dir), items_named) in filt.iter() {
        if *dir != 0 {
            continue;
        }
        for it in items_named {
            if !storages[*s as usize].slots.contains(it) {
                bail!(
                    0,
                    "`{} -> {}` names an item that nothing ever delivers to `{}`",
                    storages[*s as usize].name,
                    actors[*c as usize].name,
                    storages[*s as usize].name
                );
            }
        }
    }

    // ---- resolve arbitration order -------------------------------------
    // `clients` is in class-index order, which is exactly `Policy::Index`.
    // `RoundRobin` rotates that list at run time; `Priority` reorders it here.
    let mut declared: HashMap<u16, Vec<u16>> = HashMap::new();
    for (sid, names, line) in &prio {
        let mut order = Vec::new();
        for n in names {
            match groups.get(n) {
                Some(Group::Class(c)) => {
                    if !storages[*sid as usize].clients.contains(c) {
                        bail!(
                            *line,
                            "`{n}` is named in a priority list but is not wired to that storage"
                        );
                    }
                    push_unique(&mut order, *c);
                }
                Some(Group::Storages(_)) => {
                    bail!(*line, "`{n}` is a storage; priority lists name machines")
                }
                None => bail!(*line, "priority list names unknown node `{n}`"),
            }
        }
        declared.insert(*sid, order);
    }
    for (i, s) in storages.iter_mut().enumerate() {
        let mut order = declared.remove(&(i as u16)).unwrap_or_default();
        if s.policy == Policy::Priority && order.is_empty() {
            bail!(0, "storage `{}` uses policy priority but declares no priority list", s.name);
        }
        for &c in &s.clients {
            push_unique(&mut order, c);
        }
        s.takers = order
            .iter()
            .copied()
            .filter(|&c| actors[c as usize].in_stores.contains(&(i as u16)))
            .collect();
        s.givers = order
            .iter()
            .copied()
            .filter(|&c| actors[c as usize].out_stores.contains(&(i as u16)))
            .collect();
        s.order = order;
    }

    // ---- machine numbering ---------------------------------------------
    let mut machines = 0u64;
    for a in &mut actors {
        a.machine_offset = machines;
        machines += a.count;
    }

    // ---- validation ----------------------------------------------------
    let mut base_period = 1u64;
    for a in &actors {
        if !a.inputs.is_empty() && a.in_stores.is_empty() {
            bail!(0, "`{}` consumes items but no storage feeds it", a.name);
        }
        if !a.outputs.is_empty() && a.out_stores.is_empty() {
            bail!(0, "`{}` produces items but has nowhere to put them", a.name);
        }
        // A machine can now be fully wired and still have an ingredient with
        // nowhere to come from -- if every bay feeding it is filled by someone
        // who does not make that ingredient. Silent permanent starvation is a
        // miserable thing to debug, so it is a compile error.
        // Exactly one bay per ingredient, and exactly one bay per product.
        //
        // v2 let a machine reach into every storage wired to it that happened
        // to hold what it wanted, filling greedily in `in_stores` order. That
        // made array order a logistics policy all over again -- the same
        // mistake v2 removed from contention -- and it was the one assumption
        // the lumped solver could not discharge, because it fills a whole
        // class in one pass where the simulator interleaves members.
        //
        // v3 deletes the question instead of answering it. Material reaches a
        // machine through the logistics graph: if two bays should feed one
        // consumer, run a link from one into the other and let transport
        // latency and the receiving bay's policy decide, which is a property
        // of the factory somebody built rather than of a `Vec`.
        for st in &a.inputs {
            let from: Vec<&str> = a
                .in_stores
                .iter()
                .filter(|&&s| storages[s as usize].slots.contains(&st.item))
                .map(|&s| storages[s as usize].name.as_str())
                .collect();
            match from.len() {
                0 => bail!(
                    0,
                    "`{}` consumes an item that nothing delivers to any storage feeding it",
                    a.name
                ),
                1 => {}
                _ => bail!(
                    0,
                    "`{}` could draw {} from {} different storages ({}). \
                     Give it one input buffer and link the others into it.",
                    a.name,
                    items.names[st.item as usize],
                    from.len(),
                    from.join(", ")
                ),
            }
        }
        for st in &a.outputs {
            let to: Vec<&str> = a
                .out_stores
                .iter()
                .filter(|&&s| storages[s as usize].slots.contains(&st.item))
                .map(|&s| storages[s as usize].name.as_str())
                .collect();
            match to.len() {
                0 => bail!(0, "`{}` produces an item no storage wired to it accepts", a.name),
                1 => {}
                _ => bail!(
                    0,
                    "`{}` could post {} to {} different storages ({}). \
                     Name the one that should have it with `{} -> {} {{ {} }}`.",
                    a.name,
                    items.names[st.item as usize],
                    to.len(),
                    to.join(", "),
                    a.name,
                    to[0],
                    items.names[st.item as usize]
                ),
            }
        }
        if a.kind == ActorKind::Transport && a.in_stores == a.out_stores {
            bail!(
                0,
                "link `{}` starts and ends at the same storage; it would move nothing",
                a.name
            );
        }
        if a.kind != ActorKind::Transport && a.return_latency != 0 {
            bail!(0, "`{}` is not a link and cannot have a return trip", a.name);
        }
        // A shared machine has no private line to live in, so everything it
        // touches has to be shared too.
        if a.shared {
            for &s in a.in_stores.iter().chain(a.out_stores.iter()) {
                if !storages[s as usize].shared {
                    bail!(
                        0,
                        "shared `{}` is wired to private storage `{}`; \
                         one of the two has to change",
                        a.name,
                        storages[s as usize].name
                    );
                }
            }
        }
        base_period = lcm(base_period, a.cycle());
    }
    if actors.is_empty() {
        bail!(0, "blueprint `{name}` has no machines");
    }

    Ok(Blueprint { name, storages, actors, qty_stride, machines, base_period })
}

struct StorageBody {
    capacity: Qty,
    initial: Vec<Stack>,
    policy: Policy,
    priority: Vec<String>,
    shared: bool,
}

fn parse_storage_body(
    p: &mut Parser,
    items: &mut Items,
    nodename: &str,
) -> Result<StorageBody, DslError> {
    p.expect(Tok::LBrace)?;
    let mut capacity: Option<Qty> = None;
    let mut initial: Vec<Stack> = Vec::new();
    let mut policy = Policy::Index;
    let mut priority: Vec<String> = Vec::new();
    let mut shared = false;

    loop {
        let line = p.line();
        if p.peek() == Some(&Tok::RBrace) {
            p.pos += 1;
            break;
        }
        if p.eat_word("shared") {
            shared = true;
        } else if p.eat_word("capacity") {
            capacity = Some(p.expect_num()?);
        } else if p.eat_word("initial") {
            let qty = p.expect_num()?;
            let item = p.expect_ident()?;
            if qty == 0 {
                bail!(line, "`{nodename}` declares zero initial {item}");
            }
            initial.push(Stack { item: items.intern(&item, line)?, qty });
        } else if p.eat_word("policy") {
            let w = p.expect_ident()?;
            policy = match w.as_str() {
                "index" => Policy::Index,
                "round_robin" => Policy::RoundRobin,
                "priority" => Policy::Priority,
                other => bail!(line, "unknown policy `{other}` (index, round_robin, priority)"),
            };
        } else if p.eat_word("priority") {
            // Commas are whitespace to the lexer, so a priority list is just a
            // run of names, ended by the next keyword or the closing brace.
            while let Some(Tok::Ident(w)) = p.peek() {
                if matches!(
                    w.as_str(),
                    "capacity" | "initial" | "policy" | "priority" | "shared"
                ) {
                    break;
                }
                priority.push(w.clone());
                p.pos += 1;
            }
            if priority.is_empty() {
                bail!(line, "`{nodename}` has an empty priority list");
            }
            if policy == Policy::Index {
                policy = Policy::Priority;
            }
        } else {
            bail!(line, "unexpected {} inside storage `{nodename}`", p.describe());
        }
    }

    let Some(capacity) = capacity else {
        bail!(p.line(), "storage `{nodename}` never declares a capacity");
    };
    if capacity == 0 {
        bail!(p.line(), "storage `{nodename}` has zero capacity");
    }
    let seeded: Qty = initial.iter().map(|s| s.qty).sum();
    if seeded > capacity {
        bail!(
            p.line(),
            "storage `{nodename}` starts with {seeded} units but holds only {capacity}"
        );
    }
    Ok(StorageBody { capacity, initial, policy, priority, shared })
}

/// Everything a machine declaration can carry, gathered before it is resolved
/// so the keywords may appear in any order.
struct ActorBody {
    inputs: Vec<Stack>,
    outputs: Vec<Stack>,
    duration: Tick,
    return_latency: Tick,
    geometry: Option<Geometry>,
}

fn parse_actor_body(
    p: &mut Parser,
    items: &mut Items,
    kindword: &str,
    nodename: &str,
) -> Result<ActorBody, DslError> {
    p.expect(Tok::LBrace)?;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut duration: Option<Tick> = None;
    let mut returns: Option<Tick> = None;
    let mut distance: Option<u64> = None;
    let mut speed: Option<u64> = None;
    let mut base: Tick = 0;

    loop {
        let line = p.line();
        if p.peek() == Some(&Tok::RBrace) {
            p.pos += 1;
            break;
        }
        if p.eat_word("consumes") {
            let qty = p.expect_num()?;
            let item = p.expect_ident()?;
            if qty == 0 {
                bail!(line, "`{nodename}` consumes zero {item}");
            }
            inputs.push(Stack { item: items.intern(&item, line)?, qty });
        } else if p.eat_word("produces") {
            let qty = p.expect_num()?;
            let item = p.expect_ident()?;
            if qty == 0 {
                bail!(line, "`{nodename}` produces zero {item}");
            }
            outputs.push(Stack { item: items.intern(&item, line)?, qty });
        } else if p.eat_word("moves") {
            // Transport sugar. Consuming and producing the same stack, with
            // the two ends wired to different storages, is all a batch
            // transfer with latency ever was.
            let qty = p.expect_num()?;
            let item = p.expect_ident()?;
            if qty == 0 {
                bail!(line, "link `{nodename}` moves zero {item}");
            }
            let id = items.intern(&item, line)?;
            inputs.push(Stack { item: id, qty });
            outputs.push(Stack { item: id, qty });
        } else if p.eat_word("every") || p.eat_word("takes") {
            let d = p.expect_num()?;
            p.expect_word("ticks")?;
            if d == 0 {
                bail!(line, "`{nodename}` has a zero-tick cycle");
            }
            duration = Some(d);
        } else if p.eat_word("returns") {
            // The trip home. Without it a vehicle teleports back to the
            // loading end, which is v2's behaviour and is why v2's sending
            // regions had no slack at all.
            let d = p.expect_num()?;
            p.expect_word("ticks")?;
            returns = Some(d);
        } else if p.eat_word("distance") {
            distance = Some(p.expect_num()?);
        } else if p.eat_word("speed") {
            let s = p.expect_num()?;
            if s == 0 {
                bail!(line, "`{nodename}` declares zero speed");
            }
            speed = Some(s);
        } else if p.eat_word("base") {
            base = p.expect_num()?;
            p.eat_word("ticks");
        } else {
            bail!(line, "unexpected {} inside `{nodename}`", p.describe());
        }
    }

    // Geometry, if declared, fixes both legs: somewhere far away is far away
    // in both directions. Explicit `takes` / `returns` still win, so a one-way
    // conveyor with a fast return path stays expressible.
    let geometry = match (distance, speed) {
        (Some(d), Some(s)) => Some(Geometry { base, distance: d, speed: s }),
        (Some(_), None) => bail!(p.line(), "`{nodename}` declares a distance but no speed"),
        (None, Some(_)) => bail!(p.line(), "`{nodename}` declares a speed but no distance"),
        (None, None) => {
            if base != 0 {
                bail!(p.line(), "`{nodename}` declares a base delay but no distance");
            }
            None
        }
    };
    if geometry.is_some() && kindword != "link" {
        bail!(p.line(), "`{nodename}` is not a link, so distance means nothing to it");
    }
    let geo_latency = geometry.map(|g| g.latency());
    if let Some(l) = geo_latency {
        if l == 0 {
            bail!(p.line(), "`{nodename}` works out to a zero-tick trip; give it a base delay");
        }
    }
    let duration = match duration.or(geo_latency) {
        Some(d) => d,
        None => bail!(p.line(), "`{nodename}` never says how long a cycle takes"),
    };
    let return_latency = returns.or(geo_latency).unwrap_or(0);
    if return_latency != 0 && kindword != "link" {
        bail!(p.line(), "`{nodename}` is not a link, so it has no trip home");
    }
    match kindword {
        "source" if !inputs.is_empty() => {
            bail!(p.line(), "source `{nodename}` cannot consume; make it a process")
        }
        "source" if outputs.is_empty() => bail!(p.line(), "source `{nodename}` produces nothing"),
        "sink" if !outputs.is_empty() => {
            bail!(p.line(), "sink `{nodename}` cannot produce; make it a process")
        }
        "sink" if inputs.is_empty() => bail!(p.line(), "sink `{nodename}` consumes nothing"),
        "process" if inputs.is_empty() || outputs.is_empty() => {
            bail!(p.line(), "process `{nodename}` needs both inputs and outputs")
        }
        "link" if inputs.is_empty() => bail!(p.line(), "link `{nodename}` moves nothing"),
        _ => {}
    }
    Ok(ActorBody { inputs, outputs, duration, return_latency, geometry })
}

fn inst_name(base: &str, k: u64, mult: u64) -> String {
    if mult == 1 {
        base.to_string()
    } else {
        format!("{base}#{k}")
    }
}

fn push_unique(v: &mut Vec<u16>, x: u16) {
    if !v.contains(&x) {
        v.push(x);
    }
}

fn push_unique_item(v: &mut Vec<ItemId>, x: ItemId) {
    if !v.contains(&x) {
        v.push(x);
    }
}
