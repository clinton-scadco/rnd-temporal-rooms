//! temporal-rooms: an event-driven factory simulator with an exact closed-form
//! solver, built to scale to billions of factory objects.
//!
//! # v1 asked: can independence be compressed?
//!
//! Yes. A deployment of identical, non-interacting lines has only as many
//! distinct trajectories as it has starting phases, so a billion lines cost a
//! few hundred closed-form evaluations. Tiers T1-T4 are that result.
//!
//! # v2 asks: can *interaction* be compressed?
//!
//! v1's answer relied on lines never touching each other, which is not what a
//! factory is. Wire several machines to one buffer and whether A can act
//! depends on what B already did; the archetypes stop being independent and
//! the argument collapses.
//!
//! The v2 answer is that the coupling destroys independence but not
//! **equivalence**. Ten thousand smelters fighting over one ore bay are not
//! independent, yet at any instant each is in one of a couple of dozen local
//! states, and machines sharing a state are interchangeable. So the thing to
//! compress is not "identical factories" but "identical *states* inside one
//! coupled factory", which survives contention intact. That is tier T5.
//!
//! Getting there needed one honest admission first: v1 resolved contention by
//! whichever machine happened to have the lower array index, which is a
//! logistics policy nobody chose. T5 is only well defined once arbitration is
//! something the plant declares, so v2 makes it a `Policy` on each storage.
//!
//! | tier | module     | cost in *t*        | cost in objects | exact |
//! |------|------------|--------------------|-----------------|-------|
//! | T0   | (none)     | O(t * N)           | O(N)            | yes   |
//! | T1   | `sim`      | O(events)          | O(N)            | yes   |
//! | T2   | `analytic::orbit` | O(1)        | O(N)            | yes   |
//! | T3   | `analytic::rates` | none        | O(1)            | asymptotic |
//! | T4   | `analytic::archetypes` | O(1) per archetype | O(1) | yes, if uncoupled |
//! | T5   | `pop`      | O(1)               | **O(1)**        | **yes, even coupled** |
//!
//! T0 is never implemented; it is the thing this crate exists to avoid.
//!
//! # v3 asks: can *causality* be compressed?
//!
//! v1 and v2 both still solved one plant as one object with one clock. A
//! factory the size of a continent is not one thing happening: it is many
//! regions that cannot possibly affect each other yet, because everything
//! between them is a train that has not arrived.
//!
//! v3 cuts the plant at its transports and runs the pieces as separate
//! simulations at separate times, exchanging nothing but timestamped batches
//! and timestamped empty vehicles -- and the result is bit-for-bit the state
//! one global tick loop would have reached.
//!
//! Making that work needed one correction to v2. A v2 transport delivered its
//! load and then *teleported its vehicle home*, which is a zero-latency channel
//! running backwards through the transport, so the loading end could never run
//! a tick ahead of the unloading end. Causal slack is a property of both
//! directions:
//!
//! ```text
//!   slack(region) = min( latency        of every channel arriving here,
//!                        return latency of every channel leaving here )
//! ```
//!
//! `domains` and `rooms` are not tiers. `domains` decides which parts of a
//! plant have to be solved together in the first place; `rooms` runs the parts
//! that do not.

pub mod analytic;
pub mod domains;
pub mod dsl;
pub mod model;
pub mod pop;
pub mod rooms;
pub mod sim;
