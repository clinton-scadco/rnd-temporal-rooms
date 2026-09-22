//! # Experiment 15: the temporal fracture world slice
//!
//! Everything above this module is a factory game that happens to be set
//! somewhere. This one asks whether the *setting* is worth having:
//!
//! > Does a world made of fractured industrial phases create real factory
//! > problems -- without any of the late-game time manipulation that made the
//! > premise attractive in the first place?
//!
//! There is no player-created fracture here, no local phasing, no phasing a
//! factory out of the way of the ore body it is standing on. Three regions, two
//! joins, and material that remembers which century it came out of. If the
//! setting only becomes interesting once the player can edit time, the setting
//! is not doing the work, and this is the cheapest possible way to find out.
//!
//! ```text
//!   1890 Mining Valley          ore a shovel deep, a river, a forest,
//!         |                     cast iron, and no grid at all
//!         |  the deep fracture -- natural, 147 years, 98 MW to hold open
//!         v
//!   2037 Industrial District    a national grid, steel, a caster, rolling
//!         |                     stock -- and an ore body four generations
//!         |                     have already been through
//!         |  the near corridor -- engineered, 33 years, already standing
//!         v
//!   2070 Manufacturing Zone     two processes nobody else has, standing on
//!                               ground with nothing left under it
//! ```
//!
//! # The answer, in one paragraph
//!
//! Yes, and the reason it works is that the three phases make each other
//! *necessary* without anything being locked. The district's crushing line wants
//! ninety-three ore a second and its own ore body yields eight, so the ore comes
//! out of 1890 or it does not come. The valley can have motors -- they arrive in
//! crates, priced in gears, which somebody in a later century has to
//! manufacture and ship backwards through a fracture that only stays open while
//! the district's grid is holding it. And the zone has the best process on the
//! map and nothing whatever to put in it.
//!
//! Nothing in that paragraph is a rule about eras. It is four supply
//! relationships and one power bill.
//!
//! # The five things this experiment had to build
//!
//! **Phases that differ mechanically rather than cosmetically.** [`phase`] is
//! a date, a grid flag, a material list and eight table rows, laid over
//! experiment 14's component table -- which already knew that a motor needs a
//! supply, that a crusher may be cast in iron, and that a generator may not be
//! made of anything but steel. Twenty-nine of the thirty-seven components are
//! available in all three centuries, and that ratio is the claim: what separates
//! 1890 from 2037 is how power is made and carried, not a parallel catalogue of
//! Mk2 crushers.
//!
//! **One landscape, three histories.** [`land`] is the module that has to make
//! somebody say *this is the same place*. Fifteen features, authored once, each
//! carrying three faces; the three maps are a fold over that list, so the
//! Kestrel ore body cannot be moved in 1890 without moving the foundry that
//! stands on it in 2037. The brief's own test -- rich deposit in 1890, deposit
//! exhausted with a factory on it in 2037 -- is two rows of that table, and the
//! refusal a player meets when they try to build there says both halves out
//! loud.
//!
//! **An origin that is carried rather than inferred.** Every load in [`gate`]
//! has a phase stamped on it, and a region's exports are drawn FIFO out of what
//! it imported. That one queue makes the intended play the cheap play without a
//! single rule about processing: ore carried through 2037 untouched is still
//! 1890 ore and is a hundred and eighty years out of its time by the time it
//! reaches 2070, while ore *crushed* in 2037 leaves as 2037 concentrate and is
//! thirty-three. Nothing special-cases the crusher. Concentrate is simply a
//! different item, so its provenance queue is empty, and the district's own
//! date is what is left to stamp it with.
//!
//! **A fracture that costs something to use.** An interface is not researched
//! and not bought. It is *held open*, every five simulated seconds, by
//! megawatts somebody is actually delivering to a grid connection -- and when
//! the grid is short it goes dark, nothing new departs, and what is already
//! inside it still lands. The deep fracture wants 98 MW; the district's local
//! coal seam and its river can just about raise that, which is why the slice
//! can be started at all, and everything after that is bootstrapping.
//!
//! **Machinery that can go backwards.** [`run`]'s ledger is two terms: gears
//! delivered into a region out of a later phase, minus the crating cost of every
//! design standing in it. Both are recomputed from state that already exists, so
//! deleting a machine frees its machinery and a replica arrives at the same
//! number without being told. A generator in 1890 costs 1,600 gears and a lathe
//! costs 4,800, because a lathe is two fractures away from anywhere that makes
//! one -- and a *crusher* in 1890 costs nothing at all, provided it is on cast
//! iron. That last one is the difference between a tech tree and this: the
//! valley is not short of crushers, it is short of steel.
//!
//! # What it proves, and the one thing it does not
//!
//! The success criterion in the brief is a sentence about what players do:
//!
//! ```text
//!   mine ore cheaply in 1890  ->  move it through temporal logistics
//!                             ->  process with better 2037 machinery
//! ```
//!
//! [`play`] does exactly that, headlessly, with every machine drawn one
//! component at a time, and `tests/slice.rs` asserts on the same run. The
//! second half of the criterion -- that bringing modern machinery backward feels
//! *possible but expensive* rather than arbitrarily locked -- is the machinery
//! ledger, and the playthrough spends it: the valley ends up making its own
//! electricity out of two imported generators, four local water wheels and a
//! timber pulley, which is `designs/24-hydro.machine`.
//!
//! The best thing in the experiment is a consequence nobody designed. The only
//! component in the catalogue that turns billet into gears in one machine is the
//! lathe, and the lathe is 2070's -- so the machinery 1890 runs on is made in
//! 2070, carried through 2037, and is *still 2070 machinery* when it arrives,
//! because carrying is not making. The deep fracture therefore stops being asked
//! to hold a hundred and forty-seven years and starts being asked to hold a
//! hundred and eighty, and its power bill goes from 98 MW to 112. Nothing
//! anywhere was told to do that. It is one FIFO queue and one subtraction,
//! meeting a catalogue that was written for a different experiment.
//!
//! The thing it does not prove is that a *player* reads the geography and
//! understands it, because that is a play-session question and this is a
//! program. Two things are done about that. [`land::changes`] makes the claim
//! falsifiable: crossing the deep fracture changes 1,221 of the 5,184 tiles on
//! the plot -- 23% -- and it changes something in every layer the brief lists,
//! while the near corridor changes 9%, which is the right way round because
//! thirty-three years is not a hundred and forty-seven. A number, in the same
//! spirit as experiment 09's palette metric, so that "the world changes clearly"
//! is something somebody could be wrong about.
//!
//! And [`net`] puts it where a player can meet it: `slice serve` is three
//! centuries in a browser, walked between with one button, with Prototype 2's
//! region view served unforked underneath and `web/slice/terrain.js` painting
//! the ground of whichever century you are standing in *behind* it. Walking
//! through a fracture repaints the valley and reprices both palettes, and the
//! renderer doing the drawing is never told that either happened.
//!
//! # What is deliberately unchanged
//!
//! A region **is** an [`mp::room::Room`](crate::mp::room). Same clock, same
//! command log, same `(tick, sequence)` order, same host-plus-one-replica-per-
//! player reconstruction, same canonical hash every simulated second. Three
//! regions is three of those, and an arrival out of another century is an
//! [`Act::Deliver`](crate::mp::cmd::Act) -- Prototype 3's one addition, reused
//! without a line changed, because an arrival that only the host knew about
//! would make every replica a different factory whichever century it came from.
//!
//! The one thing below this line that experiment 15 added at all is three goal
//! templates in [`mp::goal`](crate::mp::goal), for the same reason Prototype 3
//! added five: a region's objective has to be rebuildable from a seed and a
//! template id, because that is all a snapshot carries.

pub mod gate;
pub mod land;
pub mod net;
pub mod phase;
pub mod play;
pub mod region;
pub mod run;

pub use phase::Phase;
pub use run::Slice;
