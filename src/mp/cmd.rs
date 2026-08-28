//! What a player is allowed to do, and what it means when they do it.
//!
//! ```text
//!   Command {
//!       roomId
//!       tick
//!       sequence
//!       playerId
//!       type
//!       payload
//!   }
//! ```
//!
//! Ordering is `(tick, sequence)` and nothing else. Not arrival order, not
//! frame timing, not who has the better connection: the host stamps both
//! numbers, and every replica sorts by them, so "who clicked first" is a
//! question with an answer rather than a race.
//!
//! # An intention, not a diff
//!
//! Prototype 1's log held `live::Edit`s, which are *document diffs* -- replace
//! this node, remove that wire. That works for one player and cannot work for
//! two, because two clients produce diffs against documents that have already
//! diverged. So a command here says what the player *meant*: put a yard at
//! 40,12; wire this bay to that press; commit this design. The document
//! applies it, the plant is recompiled from the document, and the diff -- if
//! anybody wants to look at one -- is derived rather than transmitted.
//!
//! # Refusals are part of the protocol
//!
//! Every rejection in this file is a *structural* one: the id is not there,
//! the item is not made, two bays cannot be wired together, somebody else is
//! editing that machine. Structural means it is the same rejection on every
//! machine, at every tick, which is what lets a refusal be broadcast as
//! confidently as an acceptance. A command that is refused never enters the
//! log, so replaying the log can never reproduce it.
//!
//! # What is deliberately not a command
//!
//! Cursors. Selections. Hover. Ghost previews, rotation before placement,
//! the red outline over a collision. None of it is here, none of it is
//! ordered, and none of it is replicated through this path -- see
//! [`super::room::Presence`], which is allowed to lose packets because
//! nothing downstream of it is allowed to remember them.

use super::kit::{proto, Role};
use super::world::{Id, Install, PlayerId, World};
use crate::json::Json;
use crate::machine::design::{Design, Tune, Unit, Wire};
use crate::machine::parts::{self, Kind};
use crate::machine::stuff::Subst;
use crate::model::Tick;

/// One thing a player meant.
#[derive(Clone, Debug)]
pub enum Act {
    PlaceMachine {
        proto: String,
        x: i32,
        y: i32,
        face: u8,
        /// A depot's item; a machine's is decided by its design.
        item: Option<String>,
        /// Present when this placement is a restore or a duplicate: the design
        /// the new machine owns from its first tick. Absent means the
        /// catalogue's.
        design: Option<Design>,
    },
    DeleteMachine {
        id: Id,
    },
    PlaceStorage {
        proto: String,
        x: i32,
        y: i32,
        face: u8,
    },
    DeleteStorage {
        id: Id,
    },
    CreateConnection {
        from: Id,
        to: Id,
        item: String,
    },
    DeleteConnection {
        from: Id,
        to: Id,
        item: String,
    },
    CreateWorldLink {
        proto: String,
        from: Id,
        to: Id,
        item: String,
    },
    DeleteWorldLink {
        id: Id,
    },
    /// Take out a draft of a machine's design, and the lock that goes with it.
    OpenDesign {
        id: Id,
    },
    /// Put the draft away. `keep` false throws it out.
    CloseDesign {
        id: Id,
        keep: bool,
    },
    PlaceComponent {
        id: Id,
        kind: String,
        x: i32,
        y: i32,
        z: i32,
        face: Option<u8>,
    },
    DeleteComponent {
        id: Id,
        unit: String,
    },
    TuneComponent {
        id: Id,
        unit: String,
        field: String,
        value: String,
    },
    ConnectComponent {
        id: Id,
        from: String,
        from_port: String,
        to: String,
        to_port: String,
    },
    DisconnectComponent {
        id: Id,
        from: String,
        from_port: String,
        to: String,
        to_port: String,
    },
    /// Replace the live design with the draft, atomically, at one tick.
    CommitMachineDesign {
        id: Id,
        design: Design,
    },
}

impl Act {
    pub fn verb(&self) -> &'static str {
        match self {
            Act::PlaceMachine { .. } => "PlaceMachine",
            Act::DeleteMachine { .. } => "DeleteMachine",
            Act::PlaceStorage { .. } => "PlaceStorage",
            Act::DeleteStorage { .. } => "DeleteStorage",
            Act::CreateConnection { .. } => "CreateConnection",
            Act::DeleteConnection { .. } => "DeleteConnection",
            Act::CreateWorldLink { .. } => "CreateWorldLink",
            Act::DeleteWorldLink { .. } => "DeleteWorldLink",
            Act::OpenDesign { .. } => "OpenDesign",
            Act::CloseDesign { .. } => "CloseDesign",
            Act::PlaceComponent { .. } => "PlaceComponent",
            Act::DeleteComponent { .. } => "DeleteComponent",
            Act::TuneComponent { .. } => "TuneComponent",
            Act::ConnectComponent { .. } => "ConnectComponent",
            Act::DisconnectComponent { .. } => "DisconnectComponent",
            Act::CommitMachineDesign { .. } => "CommitMachineDesign",
        }
    }

    /// Which installation this is about, for a UI that wants to point at one.
    pub fn about(&self) -> Option<Id> {
        match *self {
            Act::DeleteMachine { id }
            | Act::DeleteStorage { id }
            | Act::DeleteWorldLink { id }
            | Act::OpenDesign { id }
            | Act::CloseDesign { id, .. }
            | Act::PlaceComponent { id, .. }
            | Act::DeleteComponent { id, .. }
            | Act::TuneComponent { id, .. }
            | Act::ConnectComponent { id, .. }
            | Act::DisconnectComponent { id, .. }
            | Act::CommitMachineDesign { id, .. } => Some(id),
            Act::CreateConnection { from, .. } | Act::DeleteConnection { from, .. } => Some(from),
            Act::CreateWorldLink { from, .. } => Some(from),
            _ => None,
        }
    }

    /// Whether this command changes what the simulator is running.
    ///
    /// Everything that only touches a *draft* does not: the live machine keeps
    /// its old design, keeps its population, and keeps its place in the queue
    /// at every bay it is wired to. That is section 13 of the brief, and it is
    /// enforced here rather than hoped for -- a draft edit never reaches the
    /// rendezvous, so it cannot cost one.
    pub fn structural(&self) -> bool {
        !matches!(
            self,
            Act::OpenDesign { .. }
                | Act::CloseDesign { .. }
                | Act::PlaceComponent { .. }
                | Act::DeleteComponent { .. }
                | Act::TuneComponent { .. }
                | Act::ConnectComponent { .. }
                | Act::DisconnectComponent { .. }
        )
    }
}

/// A command, as it is ordered and replicated.
#[derive(Clone, Debug)]
pub struct Cmd {
    pub room: String,
    pub tick: Tick,
    pub seq: u64,
    pub player: PlayerId,
    pub act: Act,
}

impl Cmd {
    /// The total order. Tick, then sequence -- and sequence alone is already
    /// total, which is the point: the host assigns it, so two commands that
    /// arrive in the same millisecond from two continents still have an order
    /// that every replica computes the same way.
    pub fn key(&self) -> (Tick, u64) {
        (self.tick, self.seq)
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("room", self.room.clone())
            .set("tick", self.tick)
            .set("seq", Json::big(self.seq as u128))
            .set("player", self.player as i64)
            .set("type", self.act.verb())
            .set("payload", payload(&self.act))
    }

    pub fn from_json(j: &Json) -> Result<Cmd, String> {
        Ok(Cmd {
            room: j.at("room").as_str().unwrap_or_default().to_string(),
            tick: j.at("tick").as_u64().unwrap_or(0),
            seq: j.at("seq").as_u64().unwrap_or(0),
            player: j.at("player").as_u64().unwrap_or(0) as PlayerId,
            act: act_from_json(
                j.at("type").as_str().ok_or("a command has no type")?,
                j.at("payload"),
            )?,
        })
    }
}

fn payload(a: &Act) -> Json {
    let o = Json::obj();
    match a {
        Act::PlaceMachine { proto, x, y, face, item, design } => o
            .set("proto", proto.clone())
            .set("x", *x as i64)
            .set("y", *y as i64)
            .set("face", *face as i64)
            .set("item", item.clone())
            .set(
                "design",
                match design {
                    Some(d) => d.to_json(),
                    None => Json::Null,
                },
            ),
        Act::PlaceStorage { proto, x, y, face } => o
            .set("proto", proto.clone())
            .set("x", *x as i64)
            .set("y", *y as i64)
            .set("face", *face as i64),
        Act::DeleteMachine { id } | Act::DeleteStorage { id } | Act::DeleteWorldLink { id } => {
            o.set("id", Json::big(*id as u128))
        }
        Act::CreateConnection { from, to, item } | Act::DeleteConnection { from, to, item } => o
            .set("from", Json::big(*from as u128))
            .set("to", Json::big(*to as u128))
            .set("item", item.clone()),
        Act::CreateWorldLink { proto, from, to, item } => o
            .set("proto", proto.clone())
            .set("from", Json::big(*from as u128))
            .set("to", Json::big(*to as u128))
            .set("item", item.clone()),
        Act::OpenDesign { id } => o.set("id", Json::big(*id as u128)),
        Act::CloseDesign { id, keep } => o.set("id", Json::big(*id as u128)).set("keep", *keep),
        Act::PlaceComponent { id, kind, x, y, z, face } => o
            .set("id", Json::big(*id as u128))
            .set("kind", kind.clone())
            .set("x", *x as i64)
            .set("y", *y as i64)
            .set("z", *z as i64)
            .set("face", face.map(|f| Json::Int(f as i128))),
        Act::DeleteComponent { id, unit } => {
            o.set("id", Json::big(*id as u128)).set("unit", unit.clone())
        }
        Act::TuneComponent { id, unit, field, value } => o
            .set("id", Json::big(*id as u128))
            .set("unit", unit.clone())
            .set("field", field.clone())
            .set("value", value.clone()),
        Act::ConnectComponent { id, from, from_port, to, to_port }
        | Act::DisconnectComponent { id, from, from_port, to, to_port } => o
            .set("id", Json::big(*id as u128))
            .set("from", from.clone())
            .set("fromPort", from_port.clone())
            .set("to", to.clone())
            .set("toPort", to_port.clone()),
        Act::CommitMachineDesign { id, design } => {
            o.set("id", Json::big(*id as u128)).set("design", design.to_json())
        }
    }
}

fn act_from_json(kind: &str, p: &Json) -> Result<Act, String> {
    let id = || p.at("id").as_u64().unwrap_or(0);
    let from = || p.at("from").as_u64().unwrap_or(0);
    let to = || p.at("to").as_u64().unwrap_or(0);
    let s = |k: &str| p.at(k).as_str().unwrap_or_default().to_string();
    let n = |k: &str| p.at(k).as_i128().unwrap_or(0) as i32;
    let face = || p.at("face").as_u64().unwrap_or(0) as u8;
    let item = || p.at("item").as_str().map(str::to_string);
    Ok(match kind {
        "PlaceMachine" => Act::PlaceMachine {
            proto: s("proto"),
            x: n("x"),
            y: n("y"),
            face: face(),
            item: item(),
            design: match p.at("design") {
                Json::Null => None,
                d => Some(Design::from_json(d)?),
            },
        },
        "DeleteMachine" => Act::DeleteMachine { id: id() },
        "PlaceStorage" => {
            Act::PlaceStorage { proto: s("proto"), x: n("x"), y: n("y"), face: face() }
        }
        "DeleteStorage" => Act::DeleteStorage { id: id() },
        "CreateConnection" => {
            Act::CreateConnection { from: from(), to: to(), item: s("item") }
        }
        "DeleteConnection" => {
            Act::DeleteConnection { from: from(), to: to(), item: s("item") }
        }
        "CreateWorldLink" => Act::CreateWorldLink {
            proto: s("proto"),
            from: from(),
            to: to(),
            item: s("item"),
        },
        "DeleteWorldLink" => Act::DeleteWorldLink { id: id() },
        "OpenDesign" => Act::OpenDesign { id: id() },
        "CloseDesign" => Act::CloseDesign { id: id(), keep: p.at("keep").as_bool().unwrap_or(true) },
        "PlaceComponent" => Act::PlaceComponent {
            id: id(),
            kind: s("kind"),
            x: n("x"),
            y: n("y"),
            z: n("z"),
            face: p.at("face").as_u64().map(|f| f as u8),
        },
        "DeleteComponent" => Act::DeleteComponent { id: id(), unit: s("unit") },
        "TuneComponent" => Act::TuneComponent {
            id: id(),
            unit: s("unit"),
            field: s("field"),
            value: s("value"),
        },
        "ConnectComponent" => Act::ConnectComponent {
            id: id(),
            from: s("from"),
            from_port: s("fromPort"),
            to: s("to"),
            to_port: s("toPort"),
        },
        "DisconnectComponent" => Act::DisconnectComponent {
            id: id(),
            from: s("from"),
            from_port: s("fromPort"),
            to: s("to"),
            to_port: s("toPort"),
        },
        "CommitMachineDesign" => Act::CommitMachineDesign {
            id: id(),
            design: Design::from_json(p.at("design"))?,
        },
        other => return Err(format!("unknown command `{other}`")),
    })
}

/// What applying a command did, beyond changing the document.
#[derive(Clone, Debug)]
pub enum Effect {
    /// Something was taken out of the world, and should leave a ghost.
    Removed { install: Box<Install>, by: PlayerId, at: Tick },
    /// A transport was taken out. Cheap enough to rebuild that it gets a
    /// mention rather than a ghost.
    Unlinked { name: String },
    /// A machine's design was replaced, and this is what it cost.
    Recommitted { id: Id, name: String, from: String, to: String },
}

/// Apply one command to the document, or say why it cannot be applied.
///
/// This is the only function in the game that changes a world, and it is a
/// pure function of `(world, cmd)`. The host runs it to decide whether to
/// broadcast; every replica runs it again on the broadcast command and gets
/// the same answer, or the experiment has failed.
pub fn apply(w: &mut World, c: &Cmd) -> Result<Vec<Effect>, String> {
    let mut out = Vec::new();
    match &c.act {
        Act::PlaceMachine { proto: tag, x, y, face, item, design } => {
            let p = proto(tag).ok_or(format!("there is no `{tag}` in the catalogue"))?;
            if p.role == Role::Storage {
                return Err("a bay is placed with PlaceStorage".into());
            }
            let d = match (p.role, design) {
                (Role::Machine, Some(d)) => Some(d.clone()),
                (Role::Machine, None) => Some(super::world::stock_design(tag)?),
                _ => None,
            };
            w.place(p, *x, *y, *face, item.clone(), d, c.tick, c.player)?;
        }
        Act::PlaceStorage { proto: tag, x, y, face } => {
            let p = proto(tag).ok_or(format!("there is no `{tag}` in the catalogue"))?;
            if p.role != Role::Storage {
                return Err(format!("a {} is not a bay", p.title));
            }
            w.place(p, *x, *y, *face, None, None, c.tick, c.player)?;
        }
        Act::DeleteMachine { id } | Act::DeleteStorage { id } => {
            let want_storage = matches!(c.act, Act::DeleteStorage { .. });
            let inst = w.get(*id).ok_or("there is nothing there to delete")?;
            if inst.is_storage() != want_storage {
                return Err(format!("{} is not that kind of thing", inst.name));
            }
            if let Some(p) = inst.editor {
                if p != c.player {
                    return Err(format!("player {p} is editing {}", inst.name));
                }
            }
            let gone = w.remove(*id)?;
            out.push(Effect::Removed { install: Box::new(gone), by: c.player, at: c.tick });
        }
        Act::CreateConnection { from, to, item } => w.connect(*from, *to, item)?,
        Act::DeleteConnection { from, to, item } => w.disconnect(*from, *to, item)?,
        Act::CreateWorldLink { proto: tag, from, to, item } => {
            let p = proto(tag).ok_or(format!("there is no `{tag}` in the catalogue"))?;
            w.link(p, *from, *to, item, c.tick, c.player)?;
        }
        Act::DeleteWorldLink { id } => {
            let h = w.unlink(*id)?;
            out.push(Effect::Unlinked { name: h.name });
        }
        // ---- the draft --------------------------------------------------
        Act::OpenDesign { id } => {
            let player = c.player;
            let i = designable(w, *id)?;
            match i.editor {
                Some(p) if p != player => {
                    return Err(format!("player {p} is already editing {}", i.name))
                }
                _ => {}
            }
            if i.draft.is_none() {
                i.draft = i.design.clone();
            }
            i.editor = Some(player);
        }
        Act::CloseDesign { id, keep } => {
            let player = c.player;
            let i = designable(w, *id)?;
            if i.editor.is_some_and(|p| p != player) {
                return Err(format!("{} is not yours to close", i.name));
            }
            i.editor = None;
            if !keep {
                i.draft = None;
            }
        }
        Act::PlaceComponent { id, kind, x, y, z, face } => {
            let player = c.player;
            let k = parts::by_tag(kind).ok_or(format!("there is no `{kind}` component"))?;
            let d = draft_of(w, *id, player)?;
            let name = unique_name(d, k);
            d.units.push(Unit {
                name,
                kind: k,
                x: *x,
                y: *y,
                z: *z,
                face: *face,
                tune: Tune::default_for(k),
            });
            if let Some(f) = d.check().into_iter().find(|f| f.unit.as_deref() == d.units.last().map(|u| u.name.as_str())) {
                d.units.pop();
                return Err(f.what);
            }
        }
        Act::DeleteComponent { id, unit } => {
            let player = c.player;
            let d = draft_of(w, *id, player)?;
            let Some(k) = d.units.iter().position(|u| u.name == *unit) else {
                return Err(format!("there is no `{unit}` in this design"));
            };
            d.units.remove(k);
            d.wires.retain(|x| x.from != *unit && x.to != *unit);
        }
        Act::TuneComponent { id, unit, field, value } => {
            let player = c.player;
            let d = draft_of(w, *id, player)?;
            let Some(u) = d.units.iter_mut().find(|u| u.name == *unit) else {
                return Err(format!("there is no `{unit}` in this design"));
            };
            tune(u, field, value)?;
            if let Some(f) = d.check().into_iter().find(|f| f.unit.as_deref() == Some(unit.as_str())) {
                return Err(f.what);
            }
        }
        Act::ConnectComponent { id, from, from_port, to, to_port } => {
            let player = c.player;
            let d = draft_of(w, *id, player)?;
            d.can_wire(from, from_port, to, to_port)?;
            d.wires.push(Wire {
                from: from.clone(),
                from_port: from_port.clone(),
                to: to.clone(),
                to_port: to_port.clone(),
            });
        }
        Act::DisconnectComponent { id, from, from_port, to, to_port } => {
            let player = c.player;
            let d = draft_of(w, *id, player)?;
            let before = d.wires.len();
            d.wires.retain(|x| {
                !(x.from == *from
                    && x.from_port == *from_port
                    && x.to == *to
                    && x.to_port == *to_port)
            });
            if d.wires.len() == before {
                return Err("that connection is not there".into());
            }
        }
        // ---- the commit --------------------------------------------------
        Act::CommitMachineDesign { id, design } => {
            let i = w.get(*id).ok_or("there is no such machine")?;
            if !i.proto.role.designed() {
                return Err(format!("{} is not a machine you can design", i.name));
            }
            if i.editor.is_some_and(|p| p != c.player) {
                return Err(format!("{} is being edited by somebody else", i.name));
            }
            let old = i.lowered.clone();
            let m = super::lower::lower(design)?;
            // A redesign that grew has to fit where the machine already
            // stands. Refusing is the only honest answer: the alternative is
            // a commit that silently moves a building, and place-and-delete
            // exists so that nothing ever silently moves.
            let (w0, h0) = if i.face & 1 == 1 { (m.h, m.w) } else { (m.w, m.h) };
            let (x, y, name) = (i.x, i.y, i.name.clone());
            w.free(x, y, w0, h0, Some(*id))?;
            let i = w.get_mut(*id).expect("checked above");
            i.design = Some(design.clone());
            i.lowered = Some(m.clone());
            i.draft = None;
            i.editor = None;
            out.push(Effect::Recommitted {
                id: *id,
                name,
                from: old.map(|m| summary(&m)).unwrap_or_default(),
                to: summary(&m),
            });
        }
    }
    Ok(out)
}

fn summary(m: &super::lower::Macro) -> String {
    let list = |v: &[(String, crate::model::Qty)]| {
        v.iter()
            .map(|(i, q)| format!("{q} {i}"))
            .collect::<Vec<_>>()
            .join(", ")
    };
    format!("{} -> {} every {}s", list(&m.takes), list(&m.gives), super::as_secs(m.cycle))
}

fn designable(w: &mut World, id: Id) -> Result<&mut Install, String> {
    let i = w.get_mut(id).ok_or("there is no such machine")?;
    if !i.proto.role.designed() {
        return Err(format!("{} has no design to open", i.name));
    }
    Ok(i)
}

/// The draft this player is allowed to edit.
///
/// The lock is the whole of the concurrency control, and it is deliberately
/// the cheapest thing that works: one editor at a time, per machine, named in
/// the document so that everybody can see whose it is. Collaborative editing
/// of one 3D design is a research project; this is a lock.
fn draft_of(w: &mut World, id: Id, player: PlayerId) -> Result<&mut Design, String> {
    let i = designable(w, id)?;
    match i.editor {
        None => return Err(format!("{} is not open for editing", i.name)),
        Some(p) if p != player => return Err(format!("player {p} is editing {}", i.name)),
        _ => {}
    }
    i.draft.as_mut().ok_or_else(|| "that machine has no draft".to_string())
}

/// A name nobody in this design is using, chosen the same way on every
/// replica: the kind's letters, and the lowest number that is free.
fn unique_name(d: &Design, k: Kind) -> String {
    let p = parts::part(k);
    let stem: String = p
        .tag
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .take(2)
        .collect::<String>()
        .to_uppercase();
    for n in 1..1000 {
        let name = format!("{stem}{n}");
        if !d.units.iter().any(|u| u.name == name) {
            return name;
        }
    }
    format!("{stem}{}", d.units.len() + 1)
}

/// One setting on one component.
fn tune(u: &mut Unit, field: &str, value: &str) -> Result<(), String> {
    let num = || value.parse::<i64>().map_err(|_| format!("`{value}` is not a number"));
    match field {
        "throttle" => u.tune.throttle = num()?.clamp(0, 100) as u32,
        "pulse" => u.tune.pulse = value == "true" || value == "1",
        "high" => u.tune.high = num()?.max(0) as u64,
        "low" => u.tune.low = num()?.max(0) as u64,
        "ratio" => u.tune.ratio = num()? as i32,
        "limit" => u.tune.limit = num()?.max(0) as u64,
        "stages" => u.tune.stages = num()?.max(0) as u32,
        "subst" => {
            u.tune.subst = Subst::by_tag(value).ok_or(format!("there is no `{value}` to draw"))?
        }
        other => return Err(format!("`{other}` is not a setting")),
    }
    Ok(())
}
