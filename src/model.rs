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
///                 one storage to another over `duration` ticks, after which
///                 the vehicle takes `return_latency` ticks to come home.
///                 Structurally nothing new -- which is the point -- but worth
///                 naming, because a transport is what splits a factory into
///                 causally independent regions.
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

/// Where a transport's latency came from, when it was derived rather than
/// declared. Distance is the thing a player actually builds; latency is what
/// the physics makes of it.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct Geometry {
    pub base: Tick,
    pub distance: u64,
    pub speed: u64,
}

impl Geometry {
    pub fn latency(&self) -> Tick {
        self.base + self.distance / self.speed.max(1)
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
    /// Cycle time. For sources/sinks this is the declared period; for a
    /// transport it is the *outbound* leg only.
    pub duration: Tick,
    /// Transport only: how long a vehicle takes to get back to the loading
    /// end after it has unloaded.
    ///
    /// v2 links teleported home, which is `0`. That is not a rounding detail:
    /// it is the difference between a region that can run ahead of its
    /// neighbour and one that cannot. Material flowing A to B gives B slack of
    /// `duration`; vehicles flowing B back to A give A slack of
    /// `return_latency`. Causal slack is a property of *both* directions, and
    /// a link with no return trip has none in one of them.
    pub return_latency: Tick,
    /// Set when the latency was derived from spatial distance.
    pub geometry: Option<Geometry>,
    /// Declared `shared`: this class exists once for the whole deployment
    /// rather than once per deployed line. A shared class may only touch
    /// shared storage, because there is no private copy for it to live in.
    pub shared: bool,
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

    /// Full round trip. For everything except a transport with a return leg
    /// this is just `duration`.
    pub fn cycle(&self) -> Tick {
        self.duration + self.return_latency
    }

    pub fn is_link(&self) -> bool {
        self.kind == ActorKind::Transport
    }

    /// Sustained throughput of a transport class, as `items per tick`, exact.
    ///
    /// Derived, never declared: it is `vehicles * batch / round trip`, and
    /// there is no second place for it to disagree with itself.
    pub fn throughput(&self) -> (u128, u128) {
        let batch: Qty = self.outputs.iter().map(|s| s.qty).sum();
        (self.count as u128 * batch as u128, self.cycle().max(1) as u128)
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
    /// Declared `shared`: every deployed instance of the blueprint refers to
    /// *this one* storage rather than to a private copy of it. Deployments
    /// stop being independent the moment one of these exists.
    pub shared: bool,
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

    /// Indices of the transport classes: the channels between regions.
    pub fn links(&self) -> Vec<u16> {
        self.actors
            .iter()
            .enumerate()
            .filter(|(_, a)| a.is_link())
            .map(|(i, _)| i as u16)
            .collect()
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
    /// Set when this deploy's lines share infrastructure and the deployment
    /// axis therefore had to be lowered rather than left alone.
    pub origin: Option<Origin>,
}

/// How a deployment of coupled lines was lowered.
#[derive(Clone, Copy, Debug)]
pub struct Origin {
    /// The one-line blueprint it was written as.
    pub blueprint: u32,
    pub lines: u64,
    /// `true`  -- collapsed: every storage was shared, so the lines had no
    ///            private state, were interchangeable, and became populations.
    /// `false` -- spread: some storage was private, so the lines are genuinely
    ///            different objects and had to be written out one by one.
    pub collapsed: bool,
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

// ============================================ v3: deployments that share

impl Blueprint {
    pub fn has_shared(&self) -> bool {
        self.storages.iter().any(|s| s.shared) || self.actors.iter().any(|a| a.shared)
    }

    pub fn all_shared(&self) -> bool {
        self.storages.iter().all(|s| s.shared)
    }

    /// `n` deployed lines written out honestly: every private storage and
    /// every private machine class gets `n` distinct copies, and the shared
    /// ones appear once with all `n` copies wired to them.
    ///
    /// This is what "a million lines on one ore network" *means*. It is also
    /// hopeless at a million, which is the point: it exists as the ground
    /// truth that `collapse` is checked against at four lines and eight.
    pub fn spread(&self, n: u64) -> Blueprint {
        let n = n.max(1) as usize;
        // storage -> its copies (one entry if shared, `n` otherwise)
        let mut smap: Vec<Vec<u16>> = Vec::with_capacity(self.storages.len());
        let mut storages: Vec<StorageDef> = Vec::new();
        for sd in &self.storages {
            let reps = if sd.shared { 1 } else { n };
            let mut ids = Vec::with_capacity(reps);
            for k in 0..reps {
                ids.push(storages.len() as u16);
                let mut copy = sd.clone();
                if !sd.shared {
                    copy.name = format!("{}#{k}", sd.name);
                }
                copy.clients.clear();
                copy.order.clear();
                copy.takers.clear();
                copy.givers.clear();
                storages.push(copy);
            }
            smap.push(ids);
        }

        let mut cmap: Vec<Vec<u16>> = Vec::with_capacity(self.actors.len());
        let mut actors: Vec<ActorDef> = Vec::new();
        for ad in &self.actors {
            let reps = if ad.shared { 1 } else { n };
            let mut ids = Vec::with_capacity(reps);
            for k in 0..reps {
                ids.push(actors.len() as u16);
                let mut copy = ad.clone();
                if !ad.shared {
                    copy.name = format!("{}#{k}", ad.name);
                }
                let pick = |v: &Vec<u16>| -> Vec<u16> {
                    v.iter()
                        .map(|&s| {
                            let reps = smap[s as usize].len();
                            smap[s as usize][if reps == 1 { 0 } else { k }]
                        })
                        .collect()
                };
                copy.in_stores = pick(&ad.in_stores);
                copy.out_stores = pick(&ad.out_stores);
                actors.push(copy);
            }
            cmap.push(ids);
        }

        // Rebuild each storage's arbitration lists by expanding the original
        // ones in place, so the relative order of two *different* classes
        // survives and only the copies of one class are new neighbours.
        let expand = |list: &Vec<u16>, k: usize, shared_store: bool| -> Vec<u16> {
            let mut out = Vec::new();
            for &c in list {
                let copies = &cmap[c as usize];
                if shared_store {
                    out.extend(copies.iter().copied());
                } else if copies.len() == 1 {
                    out.push(copies[0]);
                } else {
                    out.push(copies[k]);
                }
            }
            out
        };
        for (s, sd) in self.storages.iter().enumerate() {
            for (k, &id) in smap[s].iter().enumerate() {
                let sh = sd.shared;
                storages[id as usize].clients = expand(&sd.clients, k, sh);
                storages[id as usize].order = expand(&sd.order, k, sh);
                storages[id as usize].takers = expand(&sd.takers, k, sh);
                storages[id as usize].givers = expand(&sd.givers, k, sh);
            }
        }

        let mut qty_stride = 0u32;
        for sd in &mut storages {
            sd.qty_offset = qty_stride;
            qty_stride += sd.slots.len() as u32;
        }
        let mut machines = 0u64;
        for a in &mut actors {
            a.machine_offset = machines;
            machines += a.count;
        }
        Blueprint {
            name: format!("{}x{n}", self.name),
            storages,
            actors,
            qty_stride,
            machines,
            base_period: self.base_period,
        }
    }

    /// `n` deployed lines claimed as one population.
    ///
    /// Every private class simply becomes `n` times as populous. This is only
    /// a legal move when the lines share *all* of their storage: with nothing
    /// private left, two lines have no state that could tell them apart, so
    /// their machines are interchangeable and belong in one class. Give a line
    /// a buffer of its own and the claim dies -- that buffer is exactly the
    /// state that makes one line different from another.
    pub fn collapse(&self, n: u64) -> Blueprint {
        let mut b = self.clone();
        b.name = format!("{} x{n}", self.name);
        let mut machines = 0u64;
        for a in &mut b.actors {
            if !a.shared {
                a.count = a.count.saturating_mul(n);
            }
            a.machine_offset = machines;
            machines += a.count;
        }
        b.machines = machines;
        b
    }
}
