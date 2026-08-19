# temporal-rooms

An event-driven factory simulator with exact closed-form solvers, built to
answer questions about billions of factory objects without touching most of
them.

**v1** asked: can a billion *independent* deterministic objects be answered
without simulating a billion objects? Yes — a deployment of identical,
non-interacting lines has only as many distinct trajectories as it has starting
phases, so a few hundred closed-form evaluations answer for all of them.

That result rested on lines never touching each other, which is not what a
factory is. Wire several machines to one buffer and whether A can act depends on
what B already did; the archetypes stop being independent and the argument
collapses.

**v2** asks the harder question: can *interaction* be compressed?

> Ten thousand smelters fighting over one ore bay are not independent. But at
> any instant each of them is in one of a couple of dozen local states, and
> machines sharing a state are interchangeable. So the thing to compress is not
> "identical factories" but **identical states inside one coupled factory** —
> and that survives contention intact.

It works, it is exact, and it costs nothing in the population size. One billion
coupled machines are held in 60 numbers and solved in 0.7 ms.

## Quick start

```powershell
.\run.ps1          # build + run all ten configurations
.\run.ps1 -Test    # run the cross-validation suite
.\run.ps1 configs/09-population.factory   # just one
```

`run.ps1` exists because rustup installs the MSVC toolchain without setting the
MSVC/Windows-SDK library search paths; it discovers them and sets `LIB`.

## The headline result

`configs/09-population.factory` and `configs/10-billion.factory` are the same
plant at two scales. Three miner classes on mismatched periods feed one ore bay;
two smelter classes with different recipes fight over it; the shortfall has to be
divided every tick. Nothing in either plant is independent of anything else.

| | config 9 | config 10 |
|---|---|---|
| machines | 10,085 | **1,008,500,000** |
| occupied population cells | 60 | **60** |
| orbit | period 120, entered t=416 | period 120, entered t=416 |
| T5 solve | 447 µs | **705 µs** |
| T1 (machine by machine) | 104,239 events, 6.6 ms | refuses to run |
| agreement | exact, every counter | — |

A hundred thousand times the machines, the same sixty numbers, the same orbit.
The population size survives only as the integer inside each cell.

Here is what the state actually looks like mid-orbit — the compression is not an
abstraction over the state, it *is* the state:

```
t=4000  OreBay[0/50000: empty]   PlateBay[1700/50000: 1700 IronPlate]
  MinerA   { working@+20: 20 }
  SmelterA { idle/starved: 4000, working@+10: 80, working@+16: 80, working@+20: 80,
             working@+40: 240, working@+60: 80, ... working@+200: 80 }
  SmelterB { idle/starved: 1040, working@+20: 240, ... working@+300: 80 }
  Shipping { working@+6: 2, working@+10: 23 }
```

## Why this is exact, not an approximation

Every machine queued at a storage is in the identical local state: idle and
asking for the same items, or finished and offering the same items. Permuting
the members of such a queue maps the global state to itself. Arbitration is
defined so that only *how many* members a class is served can affect anything —
which member is a free choice.

So "relabel machines within a class" is a **strong lumping**: the population
dynamics are well defined on their own, and every aggregate the full simulator
computes is recoverable from them. `sim.rs` is the ground truth and `pop.rs` is
a claim about it, so the two share no code and are run against each other.

The one algorithmic trick: serving a class one member at a time would be O(N)
again. Instead ask directly *how many members can be served at once*. Feasibility
is monotone — if k members fit then k−1 do, their allocation being a prefix of
the same greedy fill — so the answer is a binary search with an exact scaled
feasibility test. Serving four machines and serving four billion cost the same
handful of tests.

## Contention had to be fixed first

v1 processed one machine per event, so whichever event popped first won any
contention. That meant **lowest array index always wins** — deterministic, and a
logistics policy nobody chose. T5 is only well defined once arbitration is
something the plant declares, so v2 makes it a `Policy` on each storage and
replaces event-order-decides with explicit **rounds**:

```
round:
  phase A -- every machine whose work finished tries to deposit, in policy order
  phase B -- every idle machine tries to withdraw, likewise
repeat while anything succeeded, then advance the clock
```

`configs/08-policy.factory` is three identical shops on one bay with supply for
about half of them — experiment-02.md's complaint made concrete. Same plant, same
numbers, one keyword changed:

| class | `index` | `round_robin` |
|---|---|---|
| ShopA | **100.0%** | 55.6% |
| ShopB | 66.7% | 55.6% |
| ShopC | **0.0%** | 55.6% |
| Depot (throughput) | 27.8% | 27.8% |

Under `index`, ShopC never runs. Not once, in either direction, forever, purely
because it was written last. Both answers are exact and deterministic. They are
different games, and now the plant says which one it wants.

Two decisions carry the weight:

- **A class is exactly the set of machines the arbiter refuses to distinguish.**
  Within a class service is FIFO rotation, so `Smelter x4` shares by
  construction. Want v1's lopsided split? Declare four separate classes.
- **Round-robin deals one member per class per lap**, not "everything you can
  take". The obvious implementation is not round-robin at all: a class of six
  thousand idle smelters is never satisfied, so a pointer that waits for
  satisfaction never moves and the class behind it starves exactly as under
  `index`. Dealing one at a time is what actually shares, and the result is
  max-min fair.

Measured across every configuration, every class now splits its work to within
the one cycle integrality forces:

```
SmelterA  x6,000   cycles per machine: min 6    max 7    gap 1   (perfectly shared)
SmelterB  x4,000   cycles per machine: min 9    max 10   gap 1   (perfectly shared)
Miner     x20      cycles per machine: min 200  max 200  gap 0   (perfectly shared)
```

Getting there needed one non-obvious fix. Timer events fire in machine-index
order, so re-queueing finishers straight from the event stream resets the
rotation to index order every time a whole class finishes together — and then
low indices win every contention again. Machines mid-cycle are kept in their own
queue and handed back in the order they started (`Q_WORK` in `sim.rs`). The
lumped solver knows nothing about any of this and still matched, before and
after, which is the lumping argument demonstrated rather than asserted.

## The tiers

| tier | module | cost in *t* | cost in objects | exact |
|---|---|---|---|---|
| T0 | (none) | O(*t*·N) | O(N) | yes |
| T1 | `sim` | O(events) | O(N) | yes |
| T2 | `analytic::orbit` | O(1) | O(N) | yes |
| T3 | `analytic::rates` | none | O(1) | asymptotic |
| T4 | `analytic::archetypes` | O(1)/archetype | O(1) | yes, if uncoupled |
| T5 | `pop` | **O(1)** | **O(1)** | **yes, even coupled** |

T0 is never implemented; it is the thing this crate exists to avoid.

T2 and T5 find the same orbit wherever both can run — because v2 wrote T2's state
signature to quotient out machine identity too. The difference between them is
not the answer but the price: **T2 still walks a materialised machine list, so it
stops at exactly the scale T5 exists for.** On config 10 it cannot start.

`domains` is not a tier. It decides which parts of a plant have to be solved
together in the first place.

## Language

```
item IronOre
item Catalyst

blueprint Line {
    source  Miner x50 { produces 1000 IronOre every 60 ticks }

    storage OreBay {
        capacity 200000
        initial 500 Catalyst        # seeds a cycle that would otherwise be dead
        policy round_robin          # index | round_robin | priority
        priority Smelter, GearPress # service order, for policy priority
    }

    process Smelter x10000 { consumes 10 IronOre takes 20 ticks produces 10 IronPlate }
    link    Rail x2       { moves 12000 IronOre takes 3000 ticks }

    wire Miner -> OreBay -> Smelter
    wire Reactor -> CatBay { Catalyst }   # item-qualified
}

deploy 125_000 x Line stagger 7
```

New in v2:

| construct | meaning |
|---|---|
| `x N` on a machine | **population**, not N nodes. One class, count N. |
| `initial Q I` | storage contents at t=0. Without it, a cycle can never turn. |
| `policy P` | `index`, `round_robin` or `priority` |
| `priority A, B` | declared service order |
| `link N { moves Q I takes D ticks }` | batch transport with latency |
| `wire A -> B { I, J }` | only these items travel this way |

`x N` changing meaning is the load-bearing edit. In v1, `Smelter x4` lowered to
four `ActorDef`s — fine for four, fatal for ten thousand, because the blueprint
is the thing every analysis walks. A blueprint must stay small however many
machines it stands for.

A **`link` is not a new primitive.** It lowers to a process whose outputs equal
its inputs and whose two ends are different storages. Batch transport with
latency was already expressible; naming it only lets the domain analysis
recognise what it is.

## Cycles

`configs/06-cycle.factory` is a reactor that consumes catalyst and gives it back.
On paper the loop balances; in practice it turns only if something seeded it.

Delete `initial 40 Catalyst` and T3 declares the plant dead before anything is
simulated: catalyst is **unattainable**, because making it requires already
having it. That is a least-fixpoint reachability check over items, and it costs
nothing. The simulator agrees by simply never running the reactor.

Catalysts, returned containers, recycled coolant and waste reprocessing are all
this shape, so the check earns its place.

## Domains: finding Room boundaries instead of declaring them

Two nodes are in the same domain if a change to one can affect the other *at the
same instant*. Wiring two machines to one storage does exactly that, so
contention fuses its participants into a single indivisible unit of simulation.

Transport is different. A batch departing at *t* and landing at *t+D* carries no
information for those D ticks. So there are two decompositions:

- **hard domains** — components of the whole wiring graph. These never interact.
- **transit domains** — components once transports are cut. These interact only
  through scheduled batches.

On `configs/07-transport.factory`:

```
hard domains     1
transit domains  2
  domain 0: MineHead Minerx4
    4 machines, 20,000 buffer; nothing ever arrives, independent forever
  domain 1: Yard PlateBay Smelterx20 Shipping
    21 machines, 25,000 buffer; can be advanced alone for 3,000 ticks
```

That second number is the Room answer: how long a region can run without hearing
from anyone. It is derived from the graph, not declared by a player.

## T3 was wrong in a more interesting way than v1 knew

v1 found that the fluid model gets aggregate throughput right and individual
machine utilisation wrong. v2 found *why*, and found a second bug.

**The bug.** v1 balanced flows per item. A transport consumes IronOre and
produces IronOre, so an item-global balance cannot tell ore at the mine from ore
in the yard, concludes ore both feeds and starves itself, and converges to a
fixpoint of nonsense (`0.0500 (10388959865371/207769559501340)` — a real number
this repo used to print). Balancing at each **storage** separately asks the only
question that was ever meaningful: is this bay filling faster than it drains.
Config 7 then agrees with the exact solver to the last digit.

The same fix exposed an older modelling wart: a storage was being given slots for
items its *consumers* wanted, not only items its *producers* deposit. Wire an
assembler needing gears and copper to a gear bay and a copper bay, and both bays
acquired both slots. A storage now holds exactly what is put into it.

**The deeper point.** Where T3 still diverges, it is not imprecise. On config 9:

| class | T3 | exact |
|---|---|---|
| SmelterA | 15 cycles/tick | **10** |
| SmelterB | 6.67 cycles/tick | **10** |

A fluid model has to assume *some* rule for dividing a scarce input, and it
assumes each machine takes a share proportional to its appetite. That is a
contention policy — an unstated one, and not the one the plant declared.
Aggregate throughput still comes out right; who did the work does not.

## Results

Measured on Windows 11 / x86-64, single-threaded, `opt-level=3 lto=fat`.

| config | objects | classes | pop cells | T5 solve | compression |
|---|---|---|---|---|---|
| 01-spec | 3 | 2 | 2 | 108 µs | 1× |
| 02-balanced | 1,000,000 | 3 | 4 | 20 µs | 2× |
| 03-megafactory | 1,000,000,005 | 5 | 9 | 2.9 ms | 1× |
| 04-science | 220 | 7 | 11 | 747 µs | 1× |
| 05-coupled | 9 | 4 | 4 | 1.1 ms | 2× |
| 06-cycle | 7 | 3 | 3 | 189 µs | 1× |
| 07-transport | 30 | 4 | 5 | 391 µs | 5× |
| 08-policy | 7 | 5 | 5 | 49 µs | 1× |
| 09-population | 10,087 | 6 | 60 | 447 µs | **168×** |
| 10-billion | 1,008,500,002 | 6 | 60 | 705 µs | **16,808,333×** |

Every one of these is cross-validated against the event simulator wherever the
event simulator can run. "Exact" means every counter — cycles per class, units
produced and consumed per item — matches bit for bit.

Still measured directly:

- **1,000,000 objects fully materialised**, 48.3M events in 9.8 s, matching the
  closed form exactly. (v1 needed 64.9M events for the same plant; rounds
  removed the retry-event traffic.)
- **29–61 bytes per object** of live arena, at 5–22M events/s single-threaded.
- **1,000,000,005 objects** (config 3) solved by T4+T5 in 365 µs.

### Did the orbit become monstrous?

experiment-02.md's open question. `configs/05-coupled.factory` uses periods 60,
73, 20 and 120 deliberately chosen not to line up — lcm 8,760 — with two sources
fighting over one bay, a full 10,000-unit buffer to traverse, and live
backpressure and starvation.

**Orbit of period 300, entered at t=9,640.** The transient is long, because the
ore bay has to fill first; the orbit is thirty times shorter than the lcm. Across
all ten configurations the longest orbit is 3,000 ticks, and that one is a train
timetable rather than an emergent period.

## Limitations

These are real, and worth stating plainly.

1. **Coupling is still within one blueprint instance.** Deployed lines remain
   independent of each other, so T4 stacks on top of T5 unchanged. A shared ore
   field feeding a million separate lines is still not expressible, and that is
   the obvious next dragon.
2. **Domains are found, not yet exploited.** The decomposition and the
   independence window are computed and reported; nothing yet solves domains
   separately or advances them at different rates. That is the machinery a Room
   would actually be built from.
3. **T2 no longer scales in object count**, by design. It walks machines. T5
   covers the same ground, and on plants past a few million machines T2 simply
   cannot start.
4. **Batched feasibility assumes consistent bay ordering.** When several classes
   draw the same item from the same *set* of storages, the lumped solver fills
   them in one pass while the simulator interleaves. The totals agree as long as
   those classes list the storages in the same order — true for everything here,
   and the cross-validation would catch a violation, but it is an assumption and
   not a proof.
5. **Orbit length is bounded but not guaranteed small.** Finiteness guarantees an
   orbit exists; nothing guarantees it is short. The solver takes a budget and
   reports `found: false` rather than hanging.
6. **Still single-threaded.** Instances never interact, so the deployment axis is
   embarrassingly parallel. Untouched deliberately: v2's question was the
   coupling model, and optimising before knowing the abstraction is how you get
   an exceptionally fast implementation of the wrong thing.

## Layout

```
src/model.rs      compiled IR: classes with populations, storages with policies
src/dsl.rs        lexer, parser, lowering, validation
src/sim.rs        T1 round-arbitrated event simulator over SoA columns
src/pop.rs        T5 lumped population engine and its closed form
src/analytic.rs   T2 orbit, T3 per-storage rate algebra, T4 archetypes
src/domains.rs    causal decomposition, contention and feedback detection
src/main.rs       experiment harness
tests/            19 cross-validation tests
configs/          the ten configurations
```

Zero dependencies outside `std`.
