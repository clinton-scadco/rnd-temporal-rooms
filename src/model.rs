//! Compiled intermediate representation of a factory.
//!
//! The DSL front-end lowers to a `Program`: a set of small, fully-named
//! `Blueprint`s plus a `Deploy` list saying how many copies of each exist.
//! *All* analysis happens on blueprints (tens of nodes). Object counts in the
//! billions live only in the `count` field of a `Deploy` -- they are never
//! materialised unless you explicitly ask for it.

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
/// - `Source`  : no inputs, has outputs  -> "produces A every P ticks"
/// - `Process` : has inputs and outputs  -> "consumes X, takes D ticks, produces Y"
/// - `Sink`    : has inputs, no outputs  -> "consumes A every P ticks"
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum ActorKind {
    Source,
    Process,
    Sink,
}

impl ActorKind {
    pub fn label(self) -> &'static str {
        match self {
            ActorKind::Source => "source",
            ActorKind::Process => "process",
            ActorKind::Sink => "sink",
        }
    }
}

#[derive(Clone, Debug)]
pub struct ActorDef {
    pub name: String,
    pub kind: ActorKind,
    pub inputs: Vec<Stack>,
    pub outputs: Vec<Stack>,
    /// Cycle time. For sources/sinks this is the declared period.
    pub duration: Tick,
    /// Local storage indices this actor may withdraw from / deposit into.
    pub in_stores: Vec<u16>,
    pub out_stores: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct StorageDef {
    pub name: String,
    /// Total units across *all* item types. This shared-capacity rule is what
    /// makes the reference configuration deadlock.
    pub capacity: Qty,
    /// Item types that can appear here, statically derived from wiring.
    pub slots: Vec<ItemId>,
    /// Base index into the per-instance quantity column.
    pub qty_offset: u32,
    /// Actors wired to this storage in either direction. Used to wake blocked
    /// machines without any dynamic waiter lists (important at scale).
    pub clients: Vec<u16>,
}

#[derive(Clone, Debug)]
pub struct Blueprint {
    pub name: String,
    pub storages: Vec<StorageDef>,
    pub actors: Vec<ActorDef>,
    /// Width of one instance's quantity column.
    pub qty_stride: u32,
    /// lcm of all actor durations: the natural phase modulus for staggering.
    pub base_period: Tick,
}

impl Blueprint {
    /// Number of simulated factory objects in one instance.
    pub fn objects(&self) -> u64 {
        (self.storages.len() + self.actors.len()) as u64
    }

    pub fn slot_of(&self, storage: usize, item: ItemId) -> Option<u32> {
        let s = &self.storages[storage];
        s.slots
            .iter()
            .position(|&i| i == item)
            .map(|p| s.qty_offset + p as u32)
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
