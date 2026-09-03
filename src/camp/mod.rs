//! # Prototype 3: The Five Rooms
//!
//! Prototype 2 answered a technical question and left a design one standing.
//! Two people can build one deterministic factory together while it runs --
//! fine. But a Room was still a disposable challenge: you met the objective,
//! the screen said so, and there was nothing the finished factory was *for*.
//!
//! So this one asks the only question left:
//!
//! > **Does finishing one factory make me want to start the next one?**
//!
//! ```text
//!   enter Room -> inspect -> build -> meet objective -> Room becomes productive
//!        ^                                                      |
//!        |                                                      v
//!   reuse or redesign  <-  new problem needs something else  <- gain capability
//! ```
//!
//! # What had to become real for that loop to close
//!
//! Four things, and each of them is a module here.
//!
//! **A world with more than one room in it.** [`site`] holds five, hand
//! authored, in a fixed graph. Not procedural: until you know what a good
//! problem looks like, generating them is generating noise.
//!
//! **Transport between rooms.** [`ship`] turns what a room's depots ship into
//! trainloads that arrive somewhere else a minute later. The rooms all advance
//! on one clock whether anybody is looking at them or not, which is the first
//! time this project has cashed the promise it has been making since Prototype
//! 1: you leave, you work somewhere else for twenty minutes, you come back,
//! and the exact deterministic factory you designed has simply carried on.
//!
//! **A design library.** [`shelf`] keeps machines, and -- much more
//! importantly -- keeps *lineage*, so the answer to a new constraint is a
//! derived copy rather than a rewrite.
//!
//! **Progression that is not a number.** [`tech`] unlocks twelve *components*.
//! Not `smelting +10%`: a separator, a press, a condenser -- things that change
//! what a machine can be, and therefore send you back to one you finished an
//! hour ago.
//!
//! [`run`] is the campaign that ties them together.
//!
//! # What is deliberately unchanged
//!
//! A room here **is** an [`mp::room::Room`](crate::mp::room). Same clock, same
//! command log, same `(tick, sequence)` order, same host-plus-one-replica-per-
//! player reconstruction, same canonical hash every simulated second. Five
//! rooms is five of those, and the multiplayer proof is not weakened by any of
//! it -- the campaign only adds a clock they share and a ledger between them.
//!
//! The one addition below this line is [`Act::Deliver`](crate::mp::cmd::Act),
//! and it is a *command* rather than a poke at a simulation for exactly the
//! reason everything else here is a command: an arrival that only the host
//! knew about would make every replica a different factory. So a train landing
//! in Manufacturing is stamped, ordered and logged like a player putting down
//! a bay, and lands in the carry at a rendezvous -- which is the mechanism
//! Prototype 1 built for editing a running plant, used unchanged for
//! *supplying* one.
//!
//! ```text
//!   depot ships 240 coal/s at Coal Basin
//!        | ledger, every five simulated seconds, on a lattice
//!   train leaves with 30,000
//!        | 57 seconds
//!   Deliver{ to: Yard14, item: Coal, qty: 30,000 }  ->  Power Station's log
//!        |
//!   carry.qty[(Yard14, Coal)] += 30,000     at a rendezvous, on every replica
//! ```
//!
//! # The five problems
//!
//! ```text
//!   Coal Basin      a platform too small for the plant it needs
//!   Iron Valley     all the land in the world, and no fuel
//!   Power Station   every lump of coal is a minute away, in trainloads
//!   Manufacturing   no coal, no water, no grid: two live supply chains
//!   Final Works     a load that will not sit still
//! ```
//!
//! Five different questions rather than one question at five sizes, which is
//! the instruction in the brief that mattered most. What makes them different
//! is not the size of the number on the order -- it is that the machine that
//! answered the last room is the wrong machine for this one.

pub mod play;
pub mod run;
pub mod shelf;
pub mod ship;
pub mod site;
pub mod tech;

pub mod net;

pub use run::Camp;
