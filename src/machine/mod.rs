//! # Experiment 06: the machine designer
//!
//! An isolated prototype, deliberately not wired into anything else. The
//! question it exists to answer is not about scale -- that battle has already
//! consumed enough innocent CPU cycles -- but about play:
//!
//! > Is assembling machines from functional components a fun optimisation
//! > problem that produces understandable but non-obvious designs?
//!
//! The thesis being tested is that a building should be a small deterministic
//! factory graph the player assembles, and then a *reusable macro-machine* once
//! its behaviour is known. So the module tree is that sentence, in order:
//!
//! ```text
//!   parts    eight components, five port types, and the numbers
//!   design   the document: components on a tile grid, wires between ports
//!   sim      the tick: transfer along wires, then every component steps
//!   orbit    run it until it repeats, and keep transient + period
//!   eval     what it is worth, against a brief with four competing halves
//!   snap     state(t) in the shape a renderer needs, and why things are stopped
//!   web      all of the above, over a socket, to a canvas
//! ```
//!
//! ## What is deliberately absent
//!
//! No pressure, no temperature, no torque, no phase change, no electrical
//! network. Every component has a capacity, most have an efficiency, and one --
//! the turbine -- has a threshold. That is the whole physics, and it is already
//! enough for a design to starve, block, stall, pulse and settle.
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

pub mod design;
pub mod eval;
pub mod orbit;
pub mod parts;
pub mod sim;
pub mod snap;
pub mod web;
