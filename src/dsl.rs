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
            deploys.push(Deploy { blueprint: id, count, stagger });
        } else {
            bail!(line, "expected `item`, `blueprint` or `deploy`, found {}", p.describe());
        }
    }

    if blueprints.is_empty() {
        bail!(0, "program declares no blueprints");
    }
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
                let (inputs, outputs, duration) =
                    parse_actor_body(p, items, &kindword, &nodename)?;
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
                    inputs,
                    outputs,
                    duration,
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
        for st in &a.inputs {
            if !a.in_stores.iter().any(|&s| storages[s as usize].slots.contains(&st.item)) {
                bail!(
                    0,
                    "`{}` consumes an item that nothing delivers to any storage feeding it",
                    a.name
                );
            }
        }
        for st in &a.outputs {
            if !a.out_stores.iter().any(|&s| storages[s as usize].slots.contains(&st.item)) {
                bail!(0, "`{}` produces an item no storage wired to it accepts", a.name);
            }
        }
        if a.kind == ActorKind::Transport && a.in_stores == a.out_stores {
            bail!(
                0,
                "link `{}` starts and ends at the same storage; it would move nothing",
                a.name
            );
        }
        base_period = lcm(base_period, a.duration);
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

    loop {
        let line = p.line();
        if p.peek() == Some(&Tok::RBrace) {
            p.pos += 1;
            break;
        }
        if p.eat_word("capacity") {
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
                if matches!(w.as_str(), "capacity" | "initial" | "policy" | "priority") {
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
    Ok(StorageBody { capacity, initial, policy, priority })
}

fn parse_actor_body(
    p: &mut Parser,
    items: &mut Items,
    kindword: &str,
    nodename: &str,
) -> Result<(Vec<Stack>, Vec<Stack>, Tick), DslError> {
    p.expect(Tok::LBrace)?;
    let mut inputs = Vec::new();
    let mut outputs = Vec::new();
    let mut duration: Option<Tick> = None;

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
        } else {
            bail!(line, "unexpected {} inside `{nodename}`", p.describe());
        }
    }

    let Some(duration) = duration else {
        bail!(p.line(), "`{nodename}` never says how long a cycle takes");
    };
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
    Ok((inputs, outputs, duration))
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
