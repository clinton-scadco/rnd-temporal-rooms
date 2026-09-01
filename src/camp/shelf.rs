//! My Machines: the design library, and the lineage it remembers.
//!
//! Section 3 of the brief asks for this to become a major mechanic rather than
//! a save button, and the reason is one sentence long:
//!
//! > You shouldn't constantly force people to design machinery from nothing.
//!
//! ```text
//!   design  ->  reuse  ->  discover deficiency  ->  improve  ->  reuse again
//! ```
//!
//! The shelf is what turns that loop from a slogan into a thing the game
//! actually does. A design saved in Coal Basin is placeable in Iron Valley; a
//! design that turned out to be too wide for Coal Basin is *derived* from
//! rather than replaced, so both exist and the lineage says which came from
//! which.
//!
//! # Derive, never mutate
//!
//! A saved design is immutable. `derive` makes a copy with a new name and a
//! pointer back to its parent, and the parent goes on being whatever it was
//! when it worked. That is the same rule Prototype 2 made about placed
//! machines -- every machine owns its design, and editing one copy does not
//! touch the other -- carried up one level, and it is the rule that lets a
//! player keep `Compact Steam Plant Mk3` while they find out whether
//! `Low-Coal Mk1` was a good idea.
//!
//! ```text
//!   Compact Steam Plant Mk3
//!           |
//!           +-- Narrow Mk1        (Coal Basin wanted six fewer tiles)
//!           +-- Pulse Mk1         (Final Works wanted a surge)
//! ```
//!
//! # What a shelf entry is not
//!
//! It is not a blueprint that propagates. Placing from the shelf copies the
//! design into the machine, and from that moment the two are strangers -- so
//! editing a shelf entry never reaches into a running factory in another room,
//! and a machine that has been improved in place does not silently rewrite the
//! library. Automatic propagation is a genuinely good feature and a genuinely
//! separate experiment.

use crate::json::Json;
use crate::machine::design::Design;
use crate::model::Tick;
use crate::mp::lower::{self, Macro};
use crate::mp::world::PlayerId;
use crate::mp::{as_secs, goal::commas};

/// One design, as it was when somebody thought it was worth keeping.
#[derive(Clone, Debug)]
pub struct Saved {
    pub id: u32,
    pub name: String,
    /// The catalogue prototype this design belongs in. A design is the inside
    /// of a building, and a building still needs a shell to be placed as.
    pub proto: String,
    pub design: Design,
    /// What it compiles to, kept so a library of thirty machines does not
    /// re-run thirty orbits every time somebody opens a panel.
    pub macr: Option<Macro>,
    /// The entry this one was copied from, if it was copied from one.
    pub from: Option<u32>,
    /// The room it was saved in, which is usually the room that made it
    /// necessary.
    pub site: String,
    pub at: Tick,
    pub by: PlayerId,
}

impl Saved {
    /// What it does, in one line, for a list.
    pub fn note(&self) -> String {
        let Some(m) = &self.macr else { return "will not compile".into() };
        let list = |v: &[(String, crate::model::Qty)]| {
            v.iter()
                .map(|(i, q)| format!("{} {}", commas(*q), lower::item_title(i)))
                .collect::<Vec<_>>()
                .join(" + ")
        };
        format!(
            "{} -> {} every {}s, in {}x{}",
            if m.takes.is_empty() { "nothing".into() } else { list(&m.takes) },
            if m.gives.is_empty() { "nothing".into() } else { list(&m.gives) },
            as_secs(m.cycle),
            m.w,
            m.h
        )
    }

    pub fn to_json(&self, shelf: &Shelf) -> Json {
        Json::obj()
            .set("id", self.id as i64)
            .set("name", self.name.clone())
            .set("proto", self.proto.clone())
            .set("site", self.site.clone())
            .set("at", self.at)
            .set("seconds", as_secs(self.at))
            .set("by", self.by as i64)
            .set("from", self.from.map(|f| Json::Int(f as i128)))
            .set("fromName", self.from.and_then(|f| shelf.get(f)).map(|s| s.name.clone()))
            .set("note", self.note())
            .set("components", self.design.units.len() as i64)
            .set(
                "macro",
                match &self.macr {
                    Some(m) => m.to_json(),
                    None => Json::Null,
                },
            )
            .set(
                "children",
                Json::arr(
                    shelf
                        .items
                        .iter()
                        .filter(|s| s.from == Some(self.id))
                        .map(|s| s.name.clone())
                        .collect::<Vec<_>>(),
                ),
            )
    }
}

#[derive(Clone, Debug, Default)]
pub struct Shelf {
    pub items: Vec<Saved>,
    next: u32,
}

impl Shelf {
    pub fn get(&self, id: u32) -> Option<&Saved> {
        self.items.iter().find(|s| s.id == id)
    }

    pub fn named(&self, name: &str) -> Option<&Saved> {
        self.items.iter().find(|s| s.name == name)
    }

    /// Keep a design. `from` is the entry it came from, when the player used
    /// Copy rather than Save.
    #[allow(clippy::too_many_arguments)]
    pub fn save(
        &mut self,
        name: &str,
        proto: &str,
        design: Design,
        from: Option<u32>,
        site: &str,
        at: Tick,
        by: PlayerId,
    ) -> Result<u32, String> {
        let name = name.trim();
        if name.is_empty() {
            return Err("a design on the shelf needs a name".into());
        }
        if self.named(name).is_some() {
            return Err(format!("`{name}` is already on the shelf -- copy it instead"));
        }
        if from.is_some_and(|f| self.get(f).is_none()) {
            return Err("that design is not on the shelf".into());
        }
        self.next += 1;
        let id = self.next;
        self.items.push(Saved {
            id,
            name: name.to_string(),
            proto: proto.to_string(),
            macr: lower::lower(&design).ok(),
            design,
            from,
            site: site.to_string(),
            at,
            by,
        });
        Ok(id)
    }

    /// Copy an entry under a new name, remembering where it came from.
    ///
    /// This is the whole mechanic: the answer to "water is short here" is not
    /// a new machine and not a modified one, it is *this machine, changed*,
    /// with both of them still on the shelf afterwards.
    pub fn derive(&mut self, id: u32, name: &str, site: &str, at: Tick, by: PlayerId) -> Result<u32, String> {
        let parent = self.get(id).ok_or("that design is not on the shelf")?;
        let proto = parent.proto.clone();
        let mut d = parent.design.clone();
        d.name = name.trim().to_string();
        self.save(name, &proto, d, Some(id), site, at, by)
    }

    pub fn rename(&mut self, id: u32, name: &str) -> Result<(), String> {
        let name = name.trim().to_string();
        if name.is_empty() {
            return Err("a design on the shelf needs a name".into());
        }
        if self.items.iter().any(|s| s.name == name && s.id != id) {
            return Err(format!("`{name}` is already on the shelf"));
        }
        let s = self.items.iter_mut().find(|s| s.id == id).ok_or("that design is not on the shelf")?;
        s.name = name.clone();
        s.design.name = name;
        Ok(())
    }

    /// Take one off. What was built from it goes on running: a placed machine
    /// owns its design outright, and the shelf was only ever a place to keep a
    /// copy.
    pub fn forget(&mut self, id: u32) -> Result<String, String> {
        let k = self.items.iter().position(|s| s.id == id).ok_or("that design is not on the shelf")?;
        // The lineage survives its parent. A design that came from one that
        // has been thrown away still came from it.
        Ok(self.items.remove(k).name)
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set("count", self.items.len() as i64)
            .set(
                "designs",
                Json::Arr(self.items.iter().map(|s| s.to_json(self)).collect()),
            )
    }
}
