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

//! # Prototype 0 asks: what is it like to build one?
//!
//! The solver has done its job, and the next risk is not a missing tier -- it
//! is optimising a magnificent mathematical creature nobody has tried to
//! *play*. So the fourth experiment is a workbench rather than a solver: place
//! nodes, wire them, compile them into the language the solver already speaks,
//! and then seek to any tick and look.
//!
//! The rule it is built on is that rendering never drives simulation.
//!
//! ```text
//!   Simulation  ->  RoomState(t)  ->  RenderSnapshot  ->  the screen
//! ```
//!
//! `graph` is the document a mouse edits and the source it emits; `snap` is
//! the state at tick *T* in the shape a view needs; `web` serves both to a
//! browser over a socket. None of them may decide anything about physics: a
//! plant built with a mouse is exactly as expressive as a plant written by
//! hand, because it *is* a plant written by hand by the time it is run.

//! # Prototype 1 asks: can it be changed while it is running?
//!
//! Prototype 0's document was a drawing, and editing it started the run again
//! from tick zero. A player does not design a factory and then watch it: they
//! build a bad one, watch it fail, and fix it at tick 12,000 without losing
//! the twelve thousand ticks. So the document becomes a **command log**, and
//! the question the solver is asked keeps its shape:
//!
//! ```text
//!   state(log, T)
//! ```
//!
//! The answer is that an edit is a **rendezvous**. Regions run at different
//! clocks, so "at tick 12,000" only means something at a barrier: bring every
//! region there, harvest the plant's state, compile the new plant, pour the
//! state back in.
//!
//! ```text
//!   cost of an edit  =  O(cells) + O(nodes)
//!   cost of a replay =  O(ticks)
//! ```
//!
//! `live` is the log, the barrier and the `Carry` that crosses it -- which is
//! also, unchanged, the canonical snapshot a joining client would be sent.
//! `why` reads the state and says why a thing is stopped, computing no physics
//! at all. `scenario` is the pressure: a budget, an order and a deadline,
//! posed *about* a plant rather than inside it, in a file the solver never
//! reads.

//! # Experiment 06 asks: is a building worth designing?
//!
//! Everything above treats a machine as a recipe with a multiplier. Place a
//! smelter, place fifty, place fifty thousand -- the interesting decision is
//! *how many*, and `xN` is a poor question to build a game on.
//!
//! So `machine` asks whether the inside of one building is a better question:
//!
//! ```text
//!   a building is a small deterministic factory graph the player assembles,
//!   compiled into a reusable macro-machine once its behaviour is known
//! ```
//!
//! It is deliberately not wired into any of the above. Its own front end, its
//! own binary, its own file format, and an answer that can be thrown away
//! without taking the solver with it.
//!
//! What it borrows is the discipline. Components are state machines,
//! connections carry deterministic quantities, the renderer never simulates,
//! and the clever answer is always compared against the stupid one. What it
//! adds is a refusal: a finished machine does **not** reduce to `input x
//! efficiency = output`. It reduces to a startup transient followed by an exact
//! periodic orbit, and keeping the orbit is what lets two buildings with the
//! same average behave differently when the world outside them changes.

pub mod analytic;
pub mod domains;
pub mod dsl;
pub mod graph;
pub mod json;
pub mod live;
pub mod machine;
pub mod model;
pub mod pop;
pub mod rooms;
pub mod scenario;
pub mod sim;
pub mod snap;
pub mod web;
pub mod why;
