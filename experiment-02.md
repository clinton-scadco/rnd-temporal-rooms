V2 goal: connected deterministic factories
The question becomes:
How much arbitrary interaction can we introduce while preserving the ability to collapse most of the simulation analytically?

Start with the simplest thing that breaks V1:
        Line A ──┐
                 │
        Line B ──┼──► Shared Storage ──► Consumer
                 │
        Line C ──┘
Now A/B/C aren't independent archetypes. Whether A can deposit depends on what B and C have done.
That's the first dragon.
I would make the V2 progression
1. Shared storage between machines
   Allow arbitrary fan-in/fan-out:
Miner A ──┐
          ▼
Miner B → Ore Bay → Smelter A
          │
          ├────────→ Smelter B
          └────────→ Smelter C
This tests deterministic contention, starvation, backpressure and ordering without yet introducing belts or trains.
2. Arbitrary directed factory graphs
Move beyond the current alternating machine/storage chain:
        ┌→ Process B →┐
Source → Storage      ├→ Storage → Sink
        └→ Process C →┘
Including cycles:
A → B → C
    ↑   │
    └───┘
Cycles matter because realistic production chains eventually contain catalysts, recycled fluids, returned containers, waste processing, etc. Pyanodons will inevitably arrive carrying seventeen fluids and demand revenge.
3. Batch transport with latency
Then introduce what we discussed earlier:
Storage A

  ↓ withdraw 10,000

[IN TRANSIT]

  arrival @ tick 50,000

  ↓

Storage B
Transport should be a deterministic scheduled transfer, not 166.67 items/sec.
Now two otherwise-independent sections can communicate through timestamped batches.
This becomes extremely important later for Room boundaries.
But I'd change one conceptual thing
V1 currently seems to think primarily in terms of objects.
For V2 I'd start explicitly thinking in terms of dependency domains.
Consider this factory:
A → B → C          D → E → F
        │
        └────────────→ G
D/E/F don't necessarily need to participate in simulation of A/B/C until something couples them.
More dramatically:
[10 million object factory]

Sector A ─────┐
Sector B      │
Sector C      ├── Shared Rail Network
Sector D      │
Sector E ─────┘
The important scaling unit might not ultimately be:
one blueprint

but:
one connected region whose state can causally affect itself.

That could give you dynamic analytical boundaries.
Imagine automatically discovering simulation islands
Suppose the player builds:
Mine
 ↓
Smelting
 ↓
Train Station

      ~~~~ 3000 ticks travel ~~~~

Factory
 ↓
Assembly
While the train is travelling, those systems are causally disconnected.
Conceptually:
┌──────── ROOM DOMAIN A ────────┐

Mine → Smelting → Station

└───────────────────────────────┘

        Transfer
        arrives t=83,000

┌──────── ROOM DOMAIN B ────────┐

Station → Factory → Assembly

└───────────────────────────────┘
Domain B doesn't care what A is doing between now and tick 83,000.
It only cares:
at 83,000:
    receive 12,000 IronPlate
That is exactly the abstraction we wanted for Rooms, except now we're discovering that the same concept may be useful inside a Room.
V1 also revealed something important about fairness
This result:
Smelter #0  100%
Smelter #1  100%
Smelter #2   66.7%
Smelter #3   66.7%
isn't merely an interesting discrepancy between T2 and T3.
It's a game-design decision hiding inside an implementation detail.
Currently:
lower machine index wins contention.

That's deterministic, but players will absolutely notice.
Imagine four factories drawing from a shared train station and Factory #0 always receives resources first because it happened to be built first.
That's deterministic but potentially terrible gameplay.
So V2 should explicitly define contention semantics.
For example, storage could maintain deterministic round-robin arbitration:
tick 100:
requests = [A, B, C]

winner = A

next priority = B
Next batch:
winner = B
next priority = C
Or players could configure priority:
Ore Bay

Priority:
1. Steel
2. Gears
3. Everything else
Factorio already makes priority/splitting a gameplay mechanic. Your deterministic architecture shouldn't accidentally decide that lowest array index is the logistics policy.
I wouldn't optimise T1 yet
Your current:
5–7M events/sec single-threaded

is plenty for the next experiment.
A timing wheel and parallelisation will make the benchmark prettier, but they don't answer a new architectural question.
I'd leave:
binary heap
single thread
22.8 bytes/object
alone for now.
Because V2 might fundamentally change what an "event" and an "object" need to look like.
Optimising the current event queue before discovering the coupling model risks producing an exceptionally fast implementation of the wrong abstraction. A proud software engineering tradition, but avoidable.
The experiment I'd build next
Call it something like 04-coupled.
item IronOre
item IronPlate

blueprint CoupledSmelting {

    source MinerA {
        produces 100 IronOre every 60 ticks
    }

    source MinerB {
        produces 100 IronOre every 73 ticks
    }

    storage OreBay {
        capacity 10000
    }

    process Smelter x4 {
        consumes 10 IronOre
        takes 20 ticks
        produces 10 IronPlate
    }

    storage PlateBay {
        capacity 10000
    }

    sink Shipping {
        consumes 300 IronPlate every 120 ticks
    }

    wire MinerA -> OreBay
    wire MinerB -> OreBay

    wire OreBay -> Smelter
    wire Smelter -> PlateBay
    wire PlateBay -> Shipping
}
The mismatched 60, 73, 20, 120 periods are deliberate. Don't give the orbit solver a polite little LCM.
Then hammer it with:
- finite buffers,
- fan-in,
- fan-out,
- deterministic contention,
- starvation,
- backpressure,
- unequal production periods,
- eventually cycles.
See what happens to T2.
Does an orbit still emerge quickly?
Mathematically, with bounded integer state, yes, eventually.
Practically, perhaps the period becomes monstrous. That's precisely what we need to learn.
And then the really important V2 experiment
Take something like:
10,000 identical smelting blocks
and connect them through one shared resource.
You've deliberately destroyed T4's independence.
Then ask:
Can we recover compression because many coupled entities remain equivalent?

For example, rather than:
Smelter 1
Smelter 2
...
Smelter 10,000
perhaps the state can become:
SmelterArchetype {
    Idle:       1,923
    Working@+1: 2,011
    Working@+2: 1,997
    ...
    Starved:      842
}
Now we're no longer compressing independent factory instances.
We're compressing equivalent states inside a coupled simulation.
If that works, that is potentially the real breakthrough.
V1 proves:
A billion independent deterministic objects don't need to cost a billion objects.

V2 should try to prove:
A billion interacting deterministic objects don't necessarily need to cost a billion objects either.

And after that, I think we'd finally have enough evidence to design the actual Room architecture, because we'd know whether a Room can safely be megabase-sized or whether Rooms themselves need to act as strategically placed causal/compression boundaries.