//! Compiled intermediate representation of a factory.
//!
//! The DSL front-end lowers to a `Program`: a set of small, fully-named
//! `Blueprint`s plus a `Deploy` list saying how many copies of each exist.
//! *All* analysis happens on blueprints (tens of nodes). Object counts in the
//! billions live only in counts -- they are never materialised unless you
//! explicitly ask for it.
//!
//! # What changed in v2
//!
//! In v1, `Smelter x4` lowered to four `ActorDef`s. That is fine for four and
//! fatal for ten thousand: the blueprint is the thing every analysis walks, so
//! replication had to stop inflating it. In v2 an `ActorDef` is a **class** --
//! a recipe plus a population `count`. One class describes one machine or a
//! billion. Everything downstream indexes machines as
//! `class.machine_offset + member`, and the analytic tiers never enumerate
//! members at all.

pub type Tick = u64;
pub type Qty = u64;
pub type ItemId = u16;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Stack {
    pub item: ItemId,
    pub qty: Qty,
}

/// Every active machine is the *same* state machine; the kind is only a label
/// derived from whether it has inputs and/or outputs.
///
/// - `Source`    : no inputs, has outputs  -> "produces A every P ticks"
/// - `Process`   : has inputs and outputs  -> "consumes X, takes D ticks, produces Y"
/// - `Sink`      : has inputs, no outputs  -> "consumes A every P ticks"
/// - `Transport` : a process whose outputs equal its inputs, moving them from
///                 one storage to another over `duration` ticks. Structurally
///                 nothing new -- which is the point -- but worth naming,
///                 because a long-duration transport is what splits a factory
///                 into causally independent domains.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    Source,
    Process,
    Sink,
    Transport,
}

impl ActorKind {
    pub fn label(self) -> &'static str {
        match self {
            ActorKind::Source => "source",
            ActorKind::Process => "process",
            ActorKind::Sink => "sink",
            ActorKind::Transport => "link",
        }
    }
}

/// A recipe plus a population. `count` members share the recipe, the cycle
/// time and the wiring, and are therefore mutually interchangeable -- the
/// property tier T5 exists to exploit.
#[derive(Clone, Debug)]
pub struct ActorDef {
    pub name: String,
    pub kind: ActorKind,
    pub inputs: Vec<Stack>,
    pub outputs: Vec<Stack>,
    /// Cycle time. For sources/sinks this is the declared period.
    pub duration: Tick,
    /// How many identical machines this class stands for.
    pub count: u64,
    /// First machine index of this class within one blueprint instance.
    pub machine_offset: u64,
    /// Local storage indices this class may withdraw from / deposit into.
    pub in_stores: Vec<u16>,
    pub out_stores: Vec<u16>,
}

impl ActorDef {
    /// Storage whose arbitration order ranks this class when it wants to
    /// withdraw. Contention is resolved at a storage, so a request needs to
    /// name the one it is queueing at.
    pub fn primary_in(&self) -> Option<u16> {
        self.in_stores.first().copied()
    }
    pub fn primary_out(&self) -> Option<u16> {
        self.out_stores.first().copied()
    }
}

/// How a storage breaks ties when more machines want service than it can serve.
///
/// v1 had no answer to this question, which meant the answer was "lowest array
/// index wins" -- a logistics policy chosen by accident. Whatever a factory
/// game does here is a *game design* decision, so v2 makes it explicit and
/// makes the simulator honour it exactly.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Policy {
    /// Lowest class index wins, always. v1's accidental semantics, kept so the
    /// two can be compared on the same plant.
    Index,
    /// Classes take turns. The storage keeps a pointer into its client list;
    /// each round starts from the pointer and it advances past everyone who
    /// was fully served.
    RoundRobin,
    /// A declared order. Classes not named come last, in index order.
    Priority,
}

impl Policy {
    pub fn label(self) -> &'static str {
        match self {
            Policy::Index => "index",
            Policy::RoundRobin => "round_robin",
            Policy::Priority => "priority",
        }
    }
}

#[derive(Clone, Debug)]
pub struct StorageDef {
    pub name: String,
    /// Total units across *all* item types. This shared-capacity rule is what
    /// makes the reference configuration deadlock.
    pub capacity: Qty,
    /// Item types that can appear here, statically derived from wiring and
    /// from any declared initial contents.
    pub slots: Vec<ItemId>,
    /// Contents at t=0. Without this a production cycle can never start: a
    /// loop that consumes its own output has nothing to consume.
    pub initial: Vec<Stack>,
    /// Base index into the per-instance quantity column.
    pub qty_offset: u32,
    /// Actor *classes* wired to this storage in either direction. Used to wake
    /// waiting machines without any dynamic waiter lists (important at scale),
    /// and as the arbitration order.
    pub clients: Vec<u16>,
    pub policy: Policy,
    /// Client classes in service order. For `Index` this is `clients`; for
    /// `Priority` it is the declared order followed by the rest.
    pub order: Vec<u16>,
    /// `order` restricted to classes that withdraw here, and to those that
    /// deposit here.
    ///
    /// Depositing and withdrawing are two separate contests. Sharing one
    /// rotation between them lets a producer's turn advance the pointer that
    /// decides which consumer eats -- which silently pins the winner and makes
    /// round-robin behave exactly like index. They get a queue each.
    pub takers: Vec<u16>,
    pub givers: Vec<u16>,
}

impl StorageDef {
    #[inline]
    pub fn queue(&self, depositing: bool) -> &[u16] {
        if depositing {
            &self.givers
        } else {
            &self.takers
        }
    }
}

#[derive(Clone, Debug)]
pub struct Blueprint {
    pub name: String,
    pub storages: Vec<StorageDef>,
    pub actors: Vec<ActorDef>,
    /// Width of one instance's quantity column.
    pub qty_stride: u32,
    /// Machines in one instance, summed over class populations.
    pub machines: u64,
    /// lcm of all actor durations: the natural phase modulus for staggering.
    pub base_period: Tick,
}

impl Blueprint {
    /// Number of simulated factory objects in one instance.
    pub fn objects(&self) -> u64 {
        self.storages.len() as u64 + self.machines
    }

    /// Classes plus storages: the size of the thing analysis actually walks.
    pub fn nodes(&self) -> usize {
        self.storages.len() + self.actors.len()
    }

    pub fn slot_of(&self, storage: usize, item: ItemId) -> Option<u32> {
        let s = &self.storages[storage];
        s.slots
            .iter()
            .position(|&i| i == item)
            .map(|p| s.qty_offset + p as u32)
    }

    /// Class owning a machine index, by linear scan over classes (there are
    /// tens of these, never millions).
    pub fn class_of_machine(&self, m: u64) -> usize {
        for (i, a) in self.actors.iter().enumerate() {
            if m >= a.machine_offset && m < a.machine_offset + a.count {
                return i;
            }
        }
        panic!("machine {m} out of range")
    }
}

#[derive(Clone, Copy, Debug)]
pub struct Deploy {
    pub blueprint: u32,
    pub count: u64,
    /// Instance k starts dormant until tick `(k * stagger) % base_period`.
    pub stagger: u64,
}

#[derive(Clone, Debug)]
pub struct Program {
    pub items: Vec<String>,
    pub blueprints: Vec<Blueprint>,
    pub deploys: Vec<Deploy>,
}

impl Program {
    pub fn item_name(&self, id: ItemId) -> &str {
        &self.items[id as usize]
    }

    /// Total factory objects across the whole program.
    pub fn total_objects(&self) -> u128 {
        self.deploys
            .iter()
            .map(|d| self.blueprints[d.blueprint as usize].objects() as u128 * d.count as u128)
            .sum()
    }
}

pub fn gcd(a: u64, b: u64) -> u64 {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

pub fn lcm(a: u64, b: u64) -> u64 {
    if a == 0 || b == 0 {
        return a.max(b);
    }
    a / gcd(a, b) * b
}
