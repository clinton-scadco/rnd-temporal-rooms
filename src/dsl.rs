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
//! `x N` after a node name replicates it inside the blueprint (`Smelter x4`),
//! and a wire naming the group connects every member of it.

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

/// A named group of nodes inside a blueprint (one entry unless `x N` was used).
enum Group {
    Storages(Vec<u16>),
    Actors(Vec<u16>),
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
    // (from, to, line) resolved after all nodes are known.
    let mut wires: Vec<(String, String, usize)> = Vec::new();

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
                wires.push((prev.clone(), next.clone(), line));
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
                p.expect(Tok::LBrace)?;
                p.expect_word("capacity")?;
                let cap = p.expect_num()?;
                p.expect(Tok::RBrace)?;
                if cap == 0 {
                    bail!(line, "storage `{nodename}` has zero capacity");
                }
                let mut ids = Vec::new();
                for k in 0..mult {
                    ids.push(storages.len() as u16);
                    storages.push(StorageDef {
                        name: inst_name(&nodename, k, mult),
                        capacity: cap,
                        slots: Vec::new(),
                        qty_offset: 0,
                        clients: Vec::new(),
                    });
                }
                groups.insert(nodename, Group::Storages(ids));
            }
            "source" | "process" | "sink" => {
                let (inputs, outputs, duration) =
                    parse_actor_body(p, items, &kindword, &nodename)?;
                let kind = match kindword.as_str() {
                    "source" => ActorKind::Source,
                    "sink" => ActorKind::Sink,
                    _ => ActorKind::Process,
                };
                let mut ids = Vec::new();
                for k in 0..mult {
                    ids.push(actors.len() as u16);
                    actors.push(ActorDef {
                        name: inst_name(&nodename, k, mult),
                        kind,
                        inputs: inputs.clone(),
                        outputs: outputs.clone(),
                        duration,
                        in_stores: Vec::new(),
                        out_stores: Vec::new(),
                    });
                }
                groups.insert(nodename, Group::Actors(ids));
            }
            other => bail!(line, "unknown node kind `{other}`"),
        }
    }

    // ---- resolve wires -------------------------------------------------
    for (from, to, line) in &wires {
        let (Some(a), Some(b)) = (groups.get(from), groups.get(to)) else {
            let missing = if groups.contains_key(from) { to } else { from };
            bail!(*line, "wire references unknown node `{missing}`");
        };
        match (a, b) {
            (Group::Actors(acts), Group::Storages(strs)) => {
                for &a in acts {
                    for &s in strs {
                        push_unique(&mut actors[a as usize].out_stores, s);
                        push_unique(&mut storages[s as usize].clients, a);
                    }
                }
            }
            (Group::Storages(strs), Group::Actors(acts)) => {
                for &a in acts {
                    for &s in strs {
                        push_unique(&mut actors[a as usize].in_stores, s);
                        push_unique(&mut storages[s as usize].clients, a);
                    }
                }
            }
            (Group::Actors(_), Group::Actors(_)) => bail!(
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
    for a in &actors {
        for st in &a.outputs {
            for &s in &a.out_stores {
                push_unique_item(&mut storages[s as usize].slots, st.item);
            }
        }
        for st in &a.inputs {
            for &s in &a.in_stores {
                push_unique_item(&mut storages[s as usize].slots, st.item);
            }
        }
    }
    let mut qty_stride = 0u32;
    for s in &mut storages {
        s.slots.sort_unstable();
        s.qty_offset = qty_stride;
        qty_stride += s.slots.len() as u32;
        s.clients.sort_unstable();
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
        base_period = lcm(base_period, a.duration);
    }
    if actors.is_empty() {
        bail!(0, "blueprint `{name}` has no machines");
    }

    Ok(Blueprint { name, storages, actors, qty_stride, base_period })
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
