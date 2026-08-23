//! # Experiments 06 and 07: the machine designer, and the construction kit
//!
//! An isolated prototype, deliberately not wired into anything else. The
//! question it exists to answer is not about scale -- that battle has already
//! consumed enough innocent CPU cycles -- but about play:
//!
//! > Is assembling machines from functional components a fun optimisation
//! > problem that produces understandable but non-obvious designs?
//!
//! Experiment 06 answered that for one machine: a power plant, out of eight
//! components, against one brief. Experiment 07 asks whether the answer
//! generalises, which is a different question and a more dangerous one, because
//! the easy way to answer it is to type out a parts catalogue from an
//! engineering supplier. What it aims at instead is:
//!
//! > **Recipes define what transformation is required. Machines are
//! > player-designed networks that provide the physical processes needed to
//! > perform it.**
//!
//! `Iron -> Gear` stays one line; the machine that performs it is where the
//! complexity lives. So the module tree is that sentence, in order:
//!
//! ```text
//!   stuff    what a wire carries: a domain, a substance, five properties
//!   parts    thirty-eight components in eight families, and the numbers
//!   design   the document: components on a tile grid, wires between ports
//!   sim      the tick: transfer along wires, export, then every component steps
//!   orbit    run it until it repeats, and keep transient + period
//!   eval     what it is worth, against whichever of four briefs it claims
//!   snap     state(t) in the shape a renderer needs, and why things are stopped
//!   web      all of the above, over a socket, to a canvas
//! ```
//!
//! ## What is deliberately absent
//!
//! No pressure, no torque, no vapour-liquid equilibrium, no electrical network.
//! Temperature is a band, purity is a percent, size is one of four words, and a
//! phase change is a change of *domain* rather than a number inside a box. Every
//! component has a capacity, most have an efficiency, some have a constraint,
//! and two -- the turbine and the press -- have a threshold. That is the whole
//! physics, and it is already enough for a design to starve, block, stall,
//! refuse, pulse and settle.
//!
//! The chemistry family is absent on purpose: mixers, reactor vessels,
//! electrolysers and scrubbers were cut because none of the four briefs needs
//! them, and a component no brief needs is exactly the failure being avoided.
//!
//! ## What is deliberately present
//!
//! The refusal to average. A finished machine here does *not* compile to
//!
//! ```text
//!   input rate x efficiency = output rate
//! ```
//!
//! It compiles to a startup transient followed by an exact periodic orbit, and
//! two machines with identical average output can have periods of 1 and 47.
//! Keeping that distinction is what will let a compiled building behave
//! differently from another compiled building when the supply outside it
//! changes -- which is the whole reason to compile one rather than average it.
//!
//! And the refusal to be vague about *what* is flowing. A compiled macro-machine
//! advertises `Iron Ore (lump, 40% pure)` in and `Iron Ore (powder, 82% pure)`
//! out: one item as far as the outer game is concerned, and everything that
//! happened to it in the parentheses.

pub mod design;
pub mod eval;
pub mod orbit;
pub mod parts;
pub mod sim;
pub mod snap;
pub mod stuff;
pub mod web;
