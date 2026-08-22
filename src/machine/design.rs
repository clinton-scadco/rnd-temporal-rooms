//! The document a mouse edits: components on a tile grid, and wires between
//! their ports.
//!
//! It is deliberately *not* a command log. Prototype 1 needed one because a
//! player edits a factory that is already running and must not lose the twelve
//! thousand ticks behind them; a machine designer is the other case entirely --
//! you are designing the thing before it is installed, so an edit legitimately
//! starts the machine again from cold. That keeps the whole experiment a pure
//! function of two arguments:
//!
//! ```text
//!   state(design, t)
//! ```
//!
//! which is what makes the compiled macro-machine at the end of this module
//! tree meaningful at all.
//!
//! # The file
//!
//! ```text
//!   machine "Compact Reactor v3"
//!
//!   reactor   R1  at 0,0  throttle 42
//!   heatpipe  HP1 at 5,1
//!   exchanger HX1 at 9,0
//!   tank      T1  at 9,6  pulse 1200 0
//!
//!   wire R1.heat -> HP1.in
//!   wire HP1.out -> HX1.heat
//! ```
//!
//! Positions are tiles, not pixels, because footprint is one of the things the
//! brief asks the player to minimise -- so where a component sits is part of
//! the design rather than part of the drawing.

use super::parts::{self, Dir, Kind, PortKind};
use crate::json::Json;

/// The settings a component exposes to the player. One struct for all eight
/// kinds: a `Tune` field that a kind does not use is simply never read, which
/// is cheaper than eight variants of a thing that holds at most three numbers.
#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub struct Tune {
    /// Reactor, percent. Clamped to `MIN_THROTTLE..=100`.
    pub throttle: u32,
    /// Tank: hold until `high`, then empty down to `low`.
    pub pulse: bool,
    pub high: u64,
    pub low: u64,
}

impl Default for Tune {
    fn default() -> Self {
        Tune { throttle: 100, pulse: false, high: 1200, low: 0 }
    }
}

impl Tune {
    fn is_default_for(&self, kind: Kind) -> bool {
        match kind {
            Kind::Reactor => self.throttle == 100,
            Kind::Tank => !self.pulse,
            _ => true,
        }
    }
}

#[derive(Clone, Debug)]
pub struct Unit {
    pub name: String,
    pub kind: Kind,
    /// Top-left tile.
    pub x: i32,
    pub y: i32,
    pub tune: Tune,
}

impl Unit {
    pub fn w(&self) -> i32 {
        parts::part(self.kind).w as i32
    }
    pub fn h(&self) -> i32 {
        parts::part(self.kind).h as i32
    }
    /// Clear tiles between two footprints: zero if they touch.
    pub fn gap_to(&self, other: &Unit) -> i32 {
        let dx = (other.x - (self.x + self.w())).max(self.x - (other.x + other.w())).max(0);
        let dy = (other.y - (self.y + self.h())).max(self.y - (other.y + other.h())).max(0);
        dx + dy
    }

    fn overlaps(&self, other: &Unit) -> bool {
        self.x < other.x + other.w()
            && other.x < self.x + self.w()
            && self.y < other.y + other.h()
            && other.y < self.y + self.h()
    }
}

/// A connection, named the way the player drew it: component and port.
#[derive(Clone, Debug, PartialEq, Eq)]
pub struct Wire {
    pub from: String,
    pub from_port: String,
    pub to: String,
    pub to_port: String,
}

#[derive(Clone, Debug, Default)]
pub struct Design {
    pub name: String,
    pub units: Vec<Unit>,
    pub wires: Vec<Wire>,
}

/// A resolved wire: indices, so the simulator never looks up a string.
#[derive(Clone, Copy, Debug)]
pub struct Link {
    pub from: usize,
    pub from_port: usize,
    pub to: usize,
    pub to_port: usize,
}

/// Something wrong with the document, said in the shortest true way, and
/// attached to whichever component the player should look at.
#[derive(Clone, Debug)]
pub struct Fault {
    pub what: String,
    pub unit: Option<String>,
}

fn fault(what: impl Into<String>, unit: Option<&str>) -> Fault {
    Fault { what: what.into(), unit: unit.map(|s| s.to_string()) }
}

impl Design {
    pub fn empty() -> Design {
        Design { name: "Machine".into(), units: Vec::new(), wires: Vec::new() }
    }

    pub fn index_of(&self, name: &str) -> Option<usize> {
        self.units.iter().position(|u| u.name == name)
    }

    pub fn unit(&self, name: &str) -> Option<&Unit> {
        self.units.iter().find(|u| u.name == name)
    }

    pub fn count_of(&self, kind: Kind) -> usize {
        self.units.iter().filter(|u| u.kind == kind).count()
    }

    // --------------------------------------------------------------- checks

    /// Everything wrong with the document, in the order a player would want to
    /// fix it. An empty answer means the design can be simulated -- which is
    /// not the same as the design being any good.
    pub fn check(&self) -> Vec<Fault> {
        let mut out = Vec::new();
        for (i, u) in self.units.iter().enumerate() {
            if u.name.is_empty() {
                out.push(fault("a component with no name", None));
            }
            if self.units[..i].iter().any(|o| o.name == u.name) {
                out.push(fault(format!("two components called {}", u.name), Some(&u.name)));
            }
            if u.x < 0 || u.y < 0 {
                out.push(fault(
                    format!("{} is off the plot", u.name),
                    Some(&u.name),
                ));
            }
            for o in &self.units[..i] {
                if u.overlaps(o) {
                    out.push(fault(
                        format!("{} overlaps {}", u.name, o.name),
                        Some(&u.name),
                    ));
                }
            }
            if u.kind == Kind::Reactor {
                let t = u.tune.throttle;
                if t < parts::MIN_THROTTLE || t > 100 {
                    out.push(fault(
                        format!(
                            "{} throttle {t}% is outside {}..100",
                            u.name,
                            parts::MIN_THROTTLE
                        ),
                        Some(&u.name),
                    ));
                }
            }
            if u.kind == Kind::Tank && u.tune.pulse {
                let cap = parts::part(Kind::Tank).ports[0].cap;
                if u.tune.high > cap || u.tune.low >= u.tune.high {
                    out.push(fault(
                        format!("{} pulse {}..{} is not a range inside 0..{cap}", u.name, u.tune.low, u.tune.high),
                        Some(&u.name),
                    ));
                }
            }
        }
        for (i, w) in self.wires.iter().enumerate() {
            if let Err(e) = self.resolve_one(w) {
                out.push(fault(e, Some(&w.from)));
                continue;
            }
            if self.wires[..i].contains(w) {
                out.push(fault(
                    format!("{}.{} is already wired to {}.{}", w.from, w.from_port, w.to, w.to_port),
                    Some(&w.from),
                ));
            }
        }
        out
    }

    /// Whether a wire *could* be drawn, which the canvas wants to know while
    /// the pointer is still moving. Same rule the compiler uses, so nothing can
    /// be drawn that will then be refused.
    pub fn can_wire(&self, from: &str, from_port: &str, to: &str, to_port: &str) -> Result<(), String> {
        let w = Wire {
            from: from.into(),
            from_port: from_port.into(),
            to: to.into(),
            to_port: to_port.into(),
        };
        self.resolve_one(&w)?;
        if self.wires.contains(&w) {
            return Err("already wired".into());
        }
        Ok(())
    }

    fn resolve_one(&self, w: &Wire) -> Result<Link, String> {
        let from = self
            .index_of(&w.from)
            .ok_or_else(|| format!("no component called {}", w.from))?;
        let to = self
            .index_of(&w.to)
            .ok_or_else(|| format!("no component called {}", w.to))?;
        if from == to {
            return Err(format!("{} cannot be wired to itself", w.from));
        }
        let fp = parts::part(self.units[from].kind);
        let tp = parts::part(self.units[to].kind);
        let fi = fp
            .port_index(&w.from_port)
            .ok_or_else(|| format!("{} has no port called {}", w.from, w.from_port))?;
        let ti = tp
            .port_index(&w.to_port)
            .ok_or_else(|| format!("{} has no port called {}", w.to, w.to_port))?;
        let a = &fp.ports[fi];
        let b = &tp.ports[ti];
        if a.dir != Dir::Out || b.dir != Dir::In {
            return Err(format!(
                "{}.{} to {}.{} runs the wrong way -- an output goes to an input",
                w.from, w.from_port, w.to, w.to_port
            ));
        }
        // The boundary before the type, because a generator's power port fails
        // both and only one of the two answers is useful.
        if a.external {
            return Err(format!(
                "{}.{} is the machine's boundary, not a socket -- electricity leaves here",
                w.from, w.from_port
            ));
        }
        if b.external {
            return Err(format!(
                "{}.{} is the machine's boundary, not a socket",
                w.to, w.to_port
            ));
        }
        if a.kind != b.kind {
            return Err(format!(
                "{}.{} carries {} and {}.{} takes {}",
                w.from, w.from_port, a.kind, w.to, w.to_port, b.kind
            ));
        }
        let gap = self.units[from].gap_to(&self.units[to]);
        if gap > parts::REACH {
            return Err(format!(
                "{} and {} are {gap} tiles apart and a connection reaches {} --                  move them together, or put a pipe between them",
                w.from, w.to, parts::REACH
            ));
        }
        Ok(Link { from, from_port: fi, to, to_port: ti })
    }

    /// The wires, as indices. Only callable on a document that passed `check`.
    pub fn links(&self) -> Result<Vec<Link>, String> {
        self.wires.iter().map(|w| self.resolve_one(w)).collect()
    }

    // ------------------------------------------------------------ geometry

    /// The plot the machine occupies: bounding box, and how much of it is
    /// actually machine. Both matter -- the brief says minimise footprint, and
    /// a sprawl of well-utilised components is still a sprawl.
    pub fn footprint(&self) -> (u32, u32, u32) {
        if self.units.is_empty() {
            return (0, 0, 0);
        }
        let x0 = self.units.iter().map(|u| u.x).min().unwrap();
        let y0 = self.units.iter().map(|u| u.y).min().unwrap();
        let x1 = self.units.iter().map(|u| u.x + u.w()).max().unwrap();
        let y1 = self.units.iter().map(|u| u.y + u.h()).max().unwrap();
        let tiles: u32 = self.units.iter().map(|u| parts::part(u.kind).tiles()).sum();
        ((x1 - x0) as u32, (y1 - y0) as u32, tiles)
    }

    // ------------------------------------------------------------- the file

    pub fn parse(src: &str) -> Result<Design, String> {
        let mut d = Design::empty();
        for (n, raw) in src.lines().enumerate() {
            let line = raw.split('#').next().unwrap_or("").trim();
            if line.is_empty() {
                continue;
            }
            let at = |e: String| format!("line {}: {e}", n + 1);
            let mut w = line.split_whitespace();
            let head = w.next().unwrap_or("");
            if head == "machine" {
                let rest = line["machine".len()..].trim();
                d.name = rest.trim_matches('"').to_string();
                continue;
            }
            if head == "wire" {
                let rest: Vec<&str> = w.collect();
                let joined = rest.join(" ");
                let (a, b) = joined
                    .split_once("->")
                    .ok_or_else(|| at("a wire is `wire A.port -> B.port`".into()))?;
                let (from, from_port) = split_port(a.trim()).map_err(|e| at(e))?;
                let (to, to_port) = split_port(b.trim()).map_err(|e| at(e))?;
                d.wires.push(Wire { from, from_port, to, to_port });
                continue;
            }
            let kind = parts::by_tag(head)
                .ok_or_else(|| at(format!("`{head}` is not a component")))?;
            let name = w
                .next()
                .ok_or_else(|| at("a component needs a name".into()))?
                .to_string();
            let mut u = Unit { name, kind, x: 0, y: 0, tune: Tune::default() };
            while let Some(word) = w.next() {
                match word {
                    "at" => {
                        let pos = w.next().ok_or_else(|| at("`at` needs x,y".into()))?;
                        let (xs, ys) = pos
                            .split_once(',')
                            .ok_or_else(|| at("`at` needs x,y".into()))?;
                        u.x = xs.trim().parse().map_err(|_| at(format!("`{xs}` is not a tile")))?;
                        u.y = ys.trim().parse().map_err(|_| at(format!("`{ys}` is not a tile")))?;
                    }
                    "throttle" => {
                        let v = w.next().ok_or_else(|| at("`throttle` needs a percent".into()))?;
                        u.tune.throttle = v
                            .trim_end_matches('%')
                            .parse()
                            .map_err(|_| at(format!("`{v}` is not a percent")))?;
                    }
                    "pulse" => {
                        u.tune.pulse = true;
                        let hi = w.next().ok_or_else(|| at("`pulse` needs high low".into()))?;
                        let lo = w.next().ok_or_else(|| at("`pulse` needs high low".into()))?;
                        u.tune.high = hi.parse().map_err(|_| at(format!("`{hi}` is not a level")))?;
                        u.tune.low = lo.parse().map_err(|_| at(format!("`{lo}` is not a level")))?;
                    }
                    other => return Err(at(format!("`{other}` means nothing here"))),
                }
            }
            d.units.push(u);
        }
        Ok(d)
    }

    pub fn emit(&self) -> String {
        let mut s = format!("machine \"{}\"\n\n", self.name);
        let wide = self.units.iter().map(|u| u.name.len()).max().unwrap_or(4).max(4);
        for u in &self.units {
            s.push_str(&format!(
                "{:<9} {:<w$} at {},{}",
                u.kind.tag(),
                u.name,
                u.x,
                u.y,
                w = wide
            ));
            if !u.tune.is_default_for(u.kind) {
                match u.kind {
                    Kind::Reactor => s.push_str(&format!("  throttle {}", u.tune.throttle)),
                    Kind::Tank => s.push_str(&format!("  pulse {} {}", u.tune.high, u.tune.low)),
                    _ => {}
                }
            }
            s.push('\n');
        }
        if !self.wires.is_empty() {
            s.push('\n');
        }
        for w in &self.wires {
            s.push_str(&format!(
                "wire {}.{} -> {}.{}\n",
                w.from, w.from_port, w.to, w.to_port
            ));
        }
        s
    }

    // ------------------------------------------------------------ the wire

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("name", self.name.clone())
            .set(
                "units",
                Json::Arr(
                    self.units
                        .iter()
                        .map(|u| {
                            Json::obj()
                                .set("name", u.name.clone())
                                .set("kind", u.kind.tag())
                                .set("x", u.x as i64)
                                .set("y", u.y as i64)
                                .set("throttle", u.tune.throttle as i64)
                                .set("pulse", u.tune.pulse)
                                .set("high", u.tune.high as i64)
                                .set("low", u.tune.low as i64)
                        })
                        .collect(),
                ),
            )
            .set(
                "wires",
                Json::Arr(
                    self.wires
                        .iter()
                        .map(|w| {
                            Json::obj()
                                .set("from", w.from.clone())
                                .set("fromPort", w.from_port.clone())
                                .set("to", w.to.clone())
                                .set("toPort", w.to_port.clone())
                        })
                        .collect(),
                ),
            )
    }

    pub fn from_json(j: &Json) -> Result<Design, String> {
        let mut d = Design::empty();
        if let Some(n) = j.at("name").as_str() {
            d.name = n.to_string();
        }
        for u in j.at("units").as_arr() {
            let tag = u.at("kind").as_str().unwrap_or("");
            let kind = parts::by_tag(tag).ok_or_else(|| format!("`{tag}` is not a component"))?;
            let mut tune = Tune::default();
            if let Some(v) = u.at("throttle").as_u64() {
                tune.throttle = v as u32;
            }
            tune.pulse = u.at("pulse").as_bool().unwrap_or(false);
            if let Some(v) = u.at("high").as_u64() {
                tune.high = v;
            }
            if let Some(v) = u.at("low").as_u64() {
                tune.low = v;
            }
            d.units.push(Unit {
                name: u.at("name").as_str().unwrap_or("").to_string(),
                kind,
                x: u.at("x").as_i128().unwrap_or(0) as i32,
                y: u.at("y").as_i128().unwrap_or(0) as i32,
                tune,
            });
        }
        for w in j.at("wires").as_arr() {
            d.wires.push(Wire {
                from: w.at("from").as_str().unwrap_or("").to_string(),
                from_port: w.at("fromPort").as_str().unwrap_or("").to_string(),
                to: w.at("to").as_str().unwrap_or("").to_string(),
                to_port: w.at("toPort").as_str().unwrap_or("").to_string(),
            });
        }
        Ok(d)
    }

    /// The catalogue, as the palette needs it. It lives here rather than in the
    /// browser so that adding a component is a change to one table in Rust.
    pub fn catalogue() -> Json {
        Json::Arr(
            parts::KINDS
                .iter()
                .map(|&k| {
                    let p = parts::part(k);
                    Json::obj()
                        .set("kind", p.tag)
                        .set("title", p.title)
                        .set("blurb", p.blurb)
                        .set("w", p.w as i64)
                        .set("h", p.h as i64)
                        .set(
                            "ports",
                            Json::Arr(
                                p.ports
                                    .iter()
                                    .map(|q| {
                                        Json::obj()
                                            .set("name", q.name)
                                            .set("type", q.kind.tag())
                                            .set("dir", if q.dir == Dir::In { "in" } else { "out" })
                                            .set("rate", q.rate as i64)
                                            .set("cap", q.cap as i64)
                                            .set("external", q.external)
                                    })
                                    .collect(),
                            ),
                        )
                })
                .collect(),
        )
    }
}

fn split_port(s: &str) -> Result<(String, String), String> {
    s.split_once('.')
        .map(|(a, b)| (a.trim().to_string(), b.trim().to_string()))
        .ok_or_else(|| format!("`{s}` is not `component.port`"))
}

/// Which port types exist, for a client that wants to colour them.
pub fn port_kinds() -> Json {
    Json::arr(
        [
            PortKind::Heat,
            PortKind::Fluid,
            PortKind::Steam,
            PortKind::Rotary,
            PortKind::Electrical,
        ]
        .iter()
        .map(|k| k.tag())
        .collect::<Vec<_>>(),
    )
}
