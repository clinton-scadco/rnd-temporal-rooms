//! temporal-rooms: an event-driven factory simulator with an exact closed-form
//! solver, built to scale to billions of factory objects.
//!
//! Four tiers of answering "what is the state at tick t":
//!
//! | tier | module     | cost in t          | exact |
//! |------|------------|--------------------|-------|
//! | T0   | (none)     | O(t * N)           | yes   |
//! | T1   | `sim`      | O(events)          | yes   |
//! | T2   | `analytic::orbit` | O(1)        | yes   |
//! | T3   | `analytic::rates` | none        | asymptotic |
//! | T4   | `analytic::archetypes` | O(1) per archetype | yes |
//!
//! T0 is never implemented; it is the thing this crate exists to avoid.

pub mod analytic;
pub mod dsl;
pub mod model;
pub mod sim;
