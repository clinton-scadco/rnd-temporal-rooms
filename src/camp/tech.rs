//! What the player is allowed to build, and how that grows.
//!
//! Section 4 of the brief is a refusal dressed up as a feature request:
//!
//! ```text
//!   avoid:   Research: Smelting +10%
//!   instead: Unlock: counterflow heat exchanger
//! ```
//!
//! A percentage is a number that moves. A component is a *topology* that did
//! not exist before -- and the difference is that the second one sends you
//! back to a machine you finished an hour ago, because now there is a
//! different shape it could be. That is the whole of the progression system,
//! and it is why there is no research tree in this file, no science packs, and
//! nothing anywhere that multiplies a rate.
//!
//! # Twelve components, and what each of them opens
//!
//! ```text
//!   motor gearbox shaft      a drive train: something to turn a crusher with
//!   separator                a split, and therefore a byproduct
//!   preheater condenser      heat and vapour that come back rather than leave
//!   furnace rollmill press   hot metal, and a shape to put it in
//!   crank                    rotary becomes strokes
//!   lathe                    one machine where three were, and swarf
//!   column                   the crude chain, which nobody has touched yet
//! ```
//!
//! Twenty-six of the thirty-eight components are there from the first minute,
//! because a first room with six parts in it is a tutorial rather than a
//! factory. The twelve that are not are the twelve that change what a machine
//! can *be*.
//!
//! # The catalogue follows the parts
//!
//! There is no second list of unlocked *machines*. A prototype in
//! [`crate::mp::kit`] is placeable exactly when every component in its stock
//! design has been unlocked, which is computed rather than authored -- so the
//! Steam Crusher appears the moment the separator does, and nobody has to
//! remember to write that down twice.

use crate::machine::design::Design;
use crate::machine::parts;
use crate::json::Json;
use std::collections::BTreeSet;

/// One component, and the sentence that says why it is worth having.
pub struct Unlock {
    /// The component's tag in [`crate::machine::parts`]. An unlock *is* a
    /// component; there is nothing else it could be.
    pub part: &'static str,
    pub title: &'static str,
    /// What becomes possible, in the words a player would use.
    pub opens: &'static str,
}

/// The twelve.
pub static UNLOCKS: &[Unlock] = &[
    Unlock {
        part: "motor",
        title: "Compact Electric Motor",
        opens: "rotary out of the grid, so a process no longer has to sit next to a boiler",
    },
    Unlock {
        part: "gearbox",
        title: "Gearbox",
        opens: "speed traded for the ability to turn something heavy: a crusher wants slow",
    },
    Unlock {
        part: "shaft",
        title: "Line Shaft",
        opens: "one drive reaching four components instead of one",
    },
    Unlock {
        part: "separator",
        title: "Centrifugal Separator",
        opens: "a split into rich and tailings -- the first output nobody asked for",
    },
    Unlock {
        part: "preheater",
        title: "Counterflow Preheater",
        opens: "heat that has already done a job doing a second one",
    },
    Unlock {
        part: "condenser",
        title: "Condenser",
        opens: "vapour that comes back as fluid instead of leaving as loss",
    },
    Unlock {
        part: "furnace",
        title: "Furnace Chamber",
        opens: "metal hot enough to be shaped, and past melting, poured",
    },
    Unlock {
        part: "rollmill",
        title: "Rolling Mill",
        opens: "hot billet into strip, which is the only thing a press will take",
    },
    Unlock {
        part: "press",
        title: "Stamping Press",
        opens: "strip into gears, wasting nothing at all",
    },
    Unlock {
        part: "crank",
        title: "Crank",
        opens: "rotary into strokes, which is what a press eats",
    },
    Unlock {
        part: "lathe",
        title: "Lathe / CNC",
        opens: "billet straight to gears in one machine, and swarf to do something about",
    },
    Unlock {
        part: "column",
        title: "Distillation Column",
        opens: "crude split three ways, and a chain this campaign has not started",
    },
];

pub fn unlock(part: &str) -> Option<&'static Unlock> {
    UNLOCKS.iter().find(|u| u.part == part)
}

/// Everything a campaign begins with: every component that is not one of the
/// twelve.
///
/// Derived rather than listed, so the two can never disagree about a part.
pub fn starting() -> Vec<&'static str> {
    parts::KINDS
        .iter()
        .map(|k| k.tag())
        .filter(|t| unlock(t).is_none())
        .collect()
}

/// What this campaign may build with.
#[derive(Clone, Debug)]
pub struct Tech {
    got: BTreeSet<&'static str>,
}

impl Default for Tech {
    fn default() -> Tech {
        Tech::new()
    }
}

impl Tech {
    pub fn new() -> Tech {
        Tech { got: starting().into_iter().collect() }
    }

    pub fn has(&self, part: &str) -> bool {
        self.got.contains(part)
    }

    /// Learn one component. Answers whether it was new, so that a room
    /// completed twice does not announce the same discovery twice.
    pub fn learn(&mut self, part: &'static str) -> bool {
        self.got.insert(part)
    }

    /// The unlocks that have arrived, in the table's order.
    pub fn earned(&self) -> Vec<&'static Unlock> {
        UNLOCKS.iter().filter(|u| self.has(u.part)).collect()
    }

    /// Whether a design may be built, and which component is the problem.
    ///
    /// Checked on the *draft* as well as the commit, so that a player is told
    /// at the moment they reach for a locked component rather than at the
    /// moment they try to run the machine they built out of it.
    pub fn allows(&self, d: &Design) -> Result<(), String> {
        for u in &d.units {
            let tag = u.kind.tag();
            if !self.has(tag) {
                return Err(format!(
                    "{} has not been unlocked yet -- {}",
                    u.kind.title(),
                    unlock(tag).map(|u| u.opens).unwrap_or("it is not available here"),
                ));
            }
        }
        Ok(())
    }

    /// Whether a catalogue prototype may be placed.
    ///
    /// A machine is unlocked when its parts are. Nothing else decides it.
    pub fn allows_proto(&self, tag: &str) -> Result<(), String> {
        match crate::mp::world::stock_design(tag) {
            Ok(d) => self.allows(&d),
            // Not a machine: a bay, a mine, a rail. Those are never locked.
            Err(_) => Ok(()),
        }
    }

    /// Which component of a stock design is still missing, for a palette that
    /// would rather grey a button out than refuse a click.
    pub fn missing_for(&self, tag: &str) -> Vec<&'static str> {
        let Ok(d) = crate::mp::world::stock_design(tag) else { return Vec::new() };
        let mut out: Vec<&'static str> = Vec::new();
        for u in &d.units {
            let t = u.kind.tag();
            if !self.has(t) && !out.contains(&t) {
                out.push(t);
            }
        }
        out
    }

    pub fn to_json(&self) -> Json {
        Json::obj()
            .set(
                "unlocks",
                Json::Arr(
                    UNLOCKS
                        .iter()
                        .map(|u| {
                            Json::obj()
                                .set("part", u.part)
                                .set("title", u.title)
                                .set("opens", u.opens)
                                .set("got", self.has(u.part))
                        })
                        .collect(),
                ),
            )
            .set("earned", self.earned().len() as i64)
            .set("total", UNLOCKS.len() as i64)
            .set(
                "parts",
                Json::arr(self.got.iter().map(|t| t.to_string()).collect::<Vec<_>>()),
            )
    }
}
