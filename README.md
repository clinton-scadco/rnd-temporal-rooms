# temporal-rooms

An event-driven factory simulator with exact closed-form solvers, built to
answer questions about billions of factory objects without touching most of
them.

**v1** asked: can a billion *independent* deterministic objects be answered
without simulating a billion objects? Yes — a deployment of identical,
non-interacting lines has only as many distinct trajectories as it has starting
phases, so a few hundred closed-form evaluations answer for all of them.

**v2** asked the harder question: can *interaction* be compressed? Also yes.
Ten thousand smelters fighting over one ore bay are not independent, but at any
instant each is in one of a couple of dozen local states, and machines sharing a
state are interchangeable. One billion coupled machines fit in 60 numbers.

Both answers still solved one plant as one object with one clock.

**v3** asks: can *causality* be compressed?

> A factory the size of a continent is not one thing happening. It is many
> regions that cannot possibly affect each other yet, because everything
> between them is a train that has not arrived. Can those regions be run as
> genuinely separate simulations, at genuinely different times, and still
> produce exactly — bit for bit — the answer one global tick loop would have?

Yes. And the thing that makes it work is a detail v2 got wrong.

**Prototype 0** stops asking. Three experiments proved the architecture, and the
next risk was not a missing tier — it was polishing a magnificent mathematical
creature nobody had tried to *play*. So the fourth is a **factory workbench**:
place nodes, wire them, compile them into the language the solver already
speaks, and then drag a timeline across a billion ticks and watch.

## Quick start

```powershell
.\run.ps1          # build + run all fifteen configurations
.\run.ps1 -Test    # 46 cross-validation tests
.\run.ps1 -Serve   # the workbench, at http://127.0.0.1:8787
.\run.ps1 configs/11-railchain.factory                     # just one
```

`run.ps1` exists because rustup installs the MSVC toolchain without setting the
MSVC/Windows-SDK library search paths; it discovers them and sets `LIB`.

## The headline result

`configs/15-continent.factory` is four distant mines railing ore to one smelting
region, which rails plate on to a works. One and a half billion machines.

```
regions          6
  region 0: HeadN MinerNx100        100 machines,   slack 2,100 ticks
  region 1: HeadS MinerSx100        100 machines,   slack 2,100 ticks
  region 2: HeadE MinerEx100        100 machines,   slack 2,100 ticks
  region 3: HeadW MinerWx100        100 machines,   slack 2,100 ticks
  region 4: Yard PlateBay Smelterx1000000000    slack 1,600 ticks
  region 5: Works GearBay GearPressx500000000   slack 1,600 ticks

  channel RailN: region 0 -> region 4
    50 vehicles x 20,000 IronOre,  2,100 out / 2,100 home  =  238.0952 items/tick
    latency derived from geometry: 100 + 4000/2 = 2100

9 probe ticks: decomposed state == byte for byte == monolithic state
to t=20,000: 42 region advances, 412 messages, 36 rendezvous
  a region ran alone for 2857 ticks on average, 4,200 at most
  widest clock skew 4,200 ticks, at
     r0 t=6,300  r1 t=2,100  r2 t=2,100  r3 t=2,100  r4 t=4,200  r5 t=3,200
```

Six clocks, three thousand ticks apart, and the same state as the monolithic
solver — not the same throughput, the same *state*: storage contents, class
populations, in-flight batches, vehicles halfway home, live round-robin
pointers, canonical hash. Forty-two region advances did the work that four
hundred thousand global ticks would have.

The two compressions stack and neither knows about the other:

| | v2 result | v3 result |
|---|---|---|
| machines | 1,500,001,630 | same |
| population cells | 51 | same |
| clocks | 1 | **6** |
| decomposed vs monolithic | — | 446 µs vs 774 µs |

## What v2 got wrong: the trip home

v2 noticed that cutting a transport splits a plant, and computed how long each
piece could run alone. That number was the transport's latency, and it was half
of an answer.

A link moves material from A to B. Cutting it buys **B** a window, because a
batch that has not been loaded yet cannot possibly arrive for `latency` ticks.
It buys **A** nothing at all — because in v2 the vehicle unloaded at B and was
instantly available at A again. That is a zero-latency channel running
*backwards* through the transport, and it means the loading end can never run a
single tick ahead of the unloading end.

So v3 makes the return trip real:

```
link OreTrain x8 {
    moves 6000 IronOre
    distance 2400 speed 2 base 200      # 1400 ticks each way
}
```

A declared distance is symmetric — somewhere far away is far away in both
directions — so it fixes both legs. `takes` and `returns` set them separately
when a plant wants an asymmetric route.

Causal slack is therefore a property of both directions:

```
slack(region) = min( latency        of every channel arriving here,
                     return latency of every channel leaving here )
```

The consequences are visible immediately. On `configs/07-transport.factory`,
which is a v2 configuration untouched:

| region | v2 said | v3 says |
|---|---|---|
| Yard, smelters (receiving) | advance alone 3,000 ticks | slack 3,000 |
| MineHead, miners (sending) | **independent forever** | **slack 0** |

v2 called the mine independent forever because nothing arrives there. Nothing
does — but its trains have to come back, and in that configuration they come
back instantly, so the mine can never lead the yard by one tick. The tests
assert both readings, because v2's was not wrong about arrivals; it was
answering a different question than the one a scheduler asks.

And where zero return trips close a **loop**, the regions on it can never differ
at all. `configs/12-tradeloop.factory` sends plates north and gets gears back:

```
with trips home:     2 regions, slack 600 both ways
without them:        1 region  -- fused, because the loop costs nothing
```

That fusion is not a nicety. A conservative scheduler on a zero-weight
constraint cycle deadlocks. `domains::regions` finds the strongly connected
components of the zero-return graph and glues them back together before the
scheduler ever sees them, so every remaining cycle has strictly positive
weight and progress is guaranteed.

> Distance is what makes a factory distributable, and distance only counts
> when it is paid for in both directions.

## How a region runs alone

A transport class is **lifted out** of both regions and becomes a channel. It
needs no new state to do this, because the four buckets a class already had are
exactly the four places a vehicle can be:

```
starved    waiting to load       <- lives in the sending region
working    in transit            <- a message between them
done       waiting to unload     <- lives in the receiving region
returning  on the trip home      <- a message back
```

So a region has no inbox. A batch landing at tick *t* is delivered straight into
the receiving region's `working` bucket; an empty vehicle getting home at tick
*t* into the sending region's `returning` bucket. Both are ordinary states those
classes could already be in.

Every such message lands strictly in the receiver's future, and that is asserted
on delivery rather than argued:

```rust
assert!(at > self.now, "a message landed in region time that is already settled");
```

That one line is the whole claim that these regions could be running on
different machines. It holds because the scheduler will not let a region settle
past

```
min over inbound  channels of ( clock[sender]   + latency        )
min over outbound channels of ( clock[receiver] + return latency )
```

Each region gets its own real `Blueprint` — its own storage indices, its own
class indices, its own arbitration queues — rather than a mask over a shared
one. A mask is a promise that a region could be handed to another process; a
blueprint is a region that can be.

Regions are also picked up *furthest-behind-first*, which is what turns a
correct scheduler into a useful one: advancing the laggard raises the most
horizons at once.

## Topology decides precedence, not array order

v2 shipped one assumption it could not discharge. When several classes drew one
item from the same *set* of bays, the lumped solver filled them in one pass
where the simulator interleaved, and the totals only agreed because every
configuration happened to list the bays in the same order.

v3 does not answer that question. It deletes it:

```
`Furnace` could draw IronOre from 2 different storages (BayA, BayB).
Give it one input buffer and link the others into it.
```

A machine has exactly one bay per ingredient and one per product. If two bays
should feed one consumer, run a link from one into the other — and then which
material arrives first is decided by transport latency, transport throughput and
the receiving bay's declared policy. That is a property of the factory somebody
built, not of a `Vec`.

Every v2 configuration already satisfied this, so the rule cost nothing and
retired the last unproven assumption in the compression argument.

## Transport is physics now

A link is a deterministic delayed transfer with capacity and throughput
constraints, and throughput is **derived** rather than declared, so there is no
second number to disagree with the first three:

```
throughput = vehicles x batch / (latency + return latency)
```

| shape | declaration | throughput |
|---|---|---|
| train | `x8 { moves 6000 IronOre distance 2400 speed 2 base 200 }` | 17.14 ore/tick |
| belt | `x40 { moves 10 IronOre takes 20 ticks returns 20 ticks }` | 10 items/tick |

Those are the same abstraction. A belt is a shuttle with many small vehicles and
a short trip; a train is a shuttle with few large ones and a long trip. A
physical belt's return path is why a blocked head backs the tail up over
`latency` ticks rather than instantly — which is exactly the causal slack the
scheduler uses. The model and the intuition agree once the return path exists,
and a belt declared with no return path is the unphysical one.

Backpressure crosses a channel as it should: batches that arrive at a full bay
wait there as `done`, holding their vehicles, and the sending region runs out of
things to load. The state dump shows all of it:

```
RailN { homebound@+1050: 5, homebound@+1100: 5, ... homebound@+1500: 5 }
PlateRail { homebound@+1400: 30 }
Smelter { idle/starved: 999352000, blocked: 648000 }
```

## Deployments that share a network

v1 and v2 both leaned on deployed lines never touching each other. That is what
let phase archetypes answer a billion lines. One shared bay deletes the
assumption outright: line 1 and line 250,000,000 are now competing for the same
ore.

`configs/13-orefield.factory` says so directly:

```
shared storage OreNet    { capacity 2000000  policy round_robin }
shared source  Field x50 { produces 4000 IronOre every 20 ticks }
process Smelter x4       { consumes 20 IronOre takes 40 ticks produces 20 IronPlate }

deploy 250_000_000 x SmeltLine
```

The compression moves up a level, and for exactly one reason. With every storage
shared there is **no per-line state left**, so two lines have nothing that could
tell them apart, their machines are interchangeable in the same sense two
smelters inside one line are, and 250,000,000 lines of four smelters simply *is*
one class of a billion smelters:

```
Field    x50              (shared: one set for everybody)
Smelter  x1,000,000,000
Yard     x40              (shared: one set for everybody)

1,000,000,092 objects -> 5 population cells      (200,000,018x)
```

That is a claim, so it is checked rather than asserted: at small line counts the
harness builds the plant *both* ways — as *n* genuinely separate classes over
the shared bays, and as one class *n* times as populous — and requires every
counter to match.

```
 lines   classes   machines  probe ticks  agreement
     1         3         94            8  exact
     2         4         98            8  exact
     3         5        102            8  exact
     5         7        110            8  exact
     8        10        122            8  exact
    13        15        142            8  exact
```

Note what makes this legal. The wide form has *n* classes at one bay and the
tall form has one; under `round_robin` those give the same totals because the
policy is blind to the labels being merged. Under `index` they do not. Higher-
level lumping needs an arbiter that refuses to distinguish the things being
lumped — which is v2's definition of a class, one level up.

### And where it breaks

Give a line a buffer of its own and that buffer is precisely the state that
tells lines apart. `configs/14-privatebay.factory` is the same shared ore field
with a private plate bay per line, and the compiler refuses to collapse it —
writing the sixteen lines out one by one instead, which is an exact answer at a
worse price rather than a refusal. Past 64 lines it says so and stops.

The interesting question is *how much* the lines actually differ, because
round-robin at the shared bay is max-min fair and only the remainder of the last
incomplete lap rotates. Measured:

```
     tick   distinct lines   distinct bay levels
      200                4                     1
    1,000                4                     1
    5,000                4                     1
   20,000                3                     2
   60,000                3                     2
  200,000                2                     1
```

Sixteen lines, and the state space they occupy stays a handful wide. That is the
v4 question in one table: a deployment may yet be a population of *line* states
rather than of machine states.

## Execution modes

A region uses the cheapest exact representation available to it, and says which
one it used:

```
closed form          region hears from nobody; any tick is one evaluation away
      v
population           lumped, stepped by the scheduler
      v
event simulation     the floor: sim.rs, which every answer is checked against
```

Compression is an optimisation of exact semantics, not a requirement imposed on
the player's factory. On `configs/09-population.factory` the Room finds one
region and solves it in closed form; on `configs/11-railchain.factory` every
region has a neighbour to listen to and all three are stepped, and the Room
reports which rung each region used.

The bottom rung is honest but not yet plumbed into a region: `sim.rs` runs whole
plants, not halves of a lifted transport, so a Room cannot currently put *one*
of its regions on it. Nothing has needed that — with the one-bay rule in place
the lumped form is exact on every topology the DSL can express, and the ladder
exists for the day it is not.

## Prototype 0: the workbench

`.\run.ps1 -Serve` opens a canvas at `http://127.0.0.1:8787`. Place a source, a
storage, a processor, a link, a sink; drag a wire between them; edit a recipe;
then drag the timeline and watch the plant you built at any tick you like.

The rule it is built on is one line:

> The renderer never simulates.

```
   Simulation  ->  RoomState(t)  ->  RenderSnapshot  ->  the screen
```

The browser owns pixels, pointer input and a command log. It owns no factory
state, no clock and no physics. It asks

> what does this plant look like at tick 182,400?

and draws the answer. Everything else is a consequence.

### The graph is not the language, and does not get a vote

The canvas edits a document; the document emits DSL source; `dsl::parse` decides
what that source means. So a plant built with a mouse is exactly as expressive
as a plant written by hand — because by the time it runs, it *is* a plant
written by hand. `-Serve` shows the generated source beside the canvas.

That is checked rather than asserted. All fifteen configurations are read into
the document, written back out, and re-parsed, and the plant that comes back
must be the same plant: same storage indices, same class indices, same
arbitration queues, same `Pop::signature()` and same `Room::signature()` at
every probe tick. A layout is a comment (`# @pos Miner 80 80`), so a saved
sketch is still a `.factory` file the harness runs.

### Nothing moves that was not already moving

There is no xy coordinate anywhere in this crate, and the workbench did not add
one. A train's position is derived from two ticks v3 already stored:

```
departure = arrival - latency
progress  = (renderTime - departure) / (arrival - departure)
```

The four buckets of a transport class *are* the four places a vehicle can be, so
the snapshot ships `(departure, arrival, vehicles)` for every leg in the air and
the renderer draws each one wherever that implies. Between two events that
interpolation is exact; past the next event it would be invention, so that is
where the view stops guessing and asks again. `nextEvent` in each snapshot is
where the line falls.

The same trick answers `x 1,000,000`. A class is one number and a distribution
over a handful of states, so far away it draws as one installation with a
utilisation bar, and close up as a few thousand sampled machines in the
proportions the snapshot reports. One placed object, a billion conceptual
machines, no simulation object for either.

### The layout is the decomposition

Given a plant with no positions, the workbench lays out *regions*: each an
internally layered block, blocks of equal causal rank side by side, ranks
stacked down the page. `configs/15-continent.factory` therefore opens drawn as
four mines railing to a smelting region railing to a works, which is what it is.
That was not a goal; it is the most useful thing the layout does.

Underneath sits the scheduler's own timetable — place down the side, time
across, one bar per advance. Forty-two bars, six lanes, and you can watch a
region run four thousand ticks alone while its neighbours wait.

### What building it broke

`experiment-04.md` predicted the builder would expose bad abstractions faster
than another solver feature would. Here is what it actually said, with the
compiler's own words:

| the question | the answer, measured |
|---|---|
| Can I connect machine → machine? | ``REFUSED: `Miner -> Smelter` connects two machines; route them through a storage`` |
| Storage → storage? | ``REFUSED: `A -> B` connects two storages; insert a machine between them`` |
| What does a link connect to? | Bays, only. A link is a machine, so wiring one to a machine is refused by the same rule. |
| Can two links enter the same bay? | Yes. They enter the same *slot*, and the bay's policy arbitrates. |
| Does a storage have ports? | No — it has item slots, derived from whoever fills it, and a slot is not an input or an output. Machines withdraw from and deposit into the same one. |
| Does a processor own an input inventory? | No. It has no inventory at all. |
| Where does its output wait when blocked? | Inside the machine. Four smelters with a full output bay sit at `blocked: 4`, holding their finished batches, having completed 2 cycles between them. |
| What does `x1000` mean spatially? | One object you place. The population is a property, not a count of things on the canvas. |
| Can links cross? | Visually yes; it means nothing. Distance is declared, not measured off the canvas. |

And one construct did not survive being drawn at all.

`storage Bay x3` has been in this language since v1. It cannot be used. A wire
names the *group*, so wiring `Bay` wires all three at once — and then one
machine posts one item to three storages, which v3's one-bay rule refuses. The
error suggested naming an instance, `Miner -> Bay#0 { Ore }`, and `#` opens a
comment, so the suggested fix could not even be typed. Two storages are never
interchangeable, so a population of them was always the wrong idea:

```
`storage Bay x3` cannot be wired: a wire names the group, so all 3 would be
wired at once and no machine may use 3 bays for one item. Declare them
separately.
```

Nothing in `configs/` used it. It took a mouse to notice.

### And what the tests caught

Two mistakes in the snapshot, both the same mistake:

1. `render()` shipped the scheduler's statistics — advances, messages, widest
   skew — alongside the state. Those are properties of a *run*, not of a tick,
   so the same tick reached by scrubbing and by playing produced different
   snapshots. `a_snapshot_is_the_same_snapshot_however_it_was_reached` failed
   the moment it was written. They live in the timetable now.
2. A lifted transport belongs to no region, and `of_class` says so with
   `usize::MAX`. The first snapshot dutifully reported
   `"region": 18446744073709551615`.

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

`domains` and `rooms` are not tiers. `domains` decides which parts of a plant
have to be solved together; `rooms` runs the parts that do not.

## Language

```
item IronOre

blueprint Line {
    source  Miner x50 { produces 1000 IronOre every 60 ticks }

    storage OreBay {
        capacity 200000
        initial 500 Catalyst        # seeds a cycle that would otherwise be dead
        policy round_robin          # index | round_robin | priority
        priority Smelter, GearPress # service order, for policy priority
    }

    process Smelter x10000 { consumes 10 IronOre takes 20 ticks produces 10 IronPlate }

    link    Rail x8 {
        moves 6000 IronOre
        distance 2400 speed 2 base 200   # latency = 200 + 1200, both ways
    }
    link    Belt x40 { moves 10 IronOre takes 20 ticks returns 20 ticks }

    shared storage OreNet   { capacity 2000000 }   # one bay for every deployed line
    shared source  Field x50 { produces 4000 IronOre every 20 ticks }

    wire Miner -> OreBay -> Smelter
    wire Reactor -> CatBay { Catalyst }   # item-qualified
}

deploy 125_000 x Line stagger 7
```

New in v3:

| construct | meaning |
|---|---|
| `returns D ticks` | how long a vehicle takes to get home after unloading |
| `distance D speed S base B` | latency `B + D/S`, symmetric, so both legs |
| `shared storage` / `shared source` | one of these for the whole deployment |

Removed in v3: drawing one item from several bays, or posting one item to
several bays. Route them through a link instead.

Removed in Prototype 0: `x N` on a storage. It has been unusable since v1 and
nobody noticed until it had to be drawn — see above. Declare the bays
separately, which is the same plant, spelled honestly.

## Results

Measured on Windows 11 / x86-64, single-threaded, `opt-level=3 lto=fat`.

| config | objects | classes | pop cells | regions | clock drift | T5 solve | compression |
|---|---|---|---|---|---|---|---|
| 01-spec | 3 | 2 | 2 | 1 | 0 | 65 µs | 1× |
| 02-balanced | 1,000,000 | 3 | 4 | 1 | 0 | 18 µs | 2× |
| 03-megafactory | 1,000,000,005 | 5 | 9 | 1 | 0 | 4.0 ms | 1× |
| 04-science | 220 | 7 | 11 | 1 | 0 | 663 µs | 1× |
| 05-coupled | 9 | 4 | 4 | 1 | 0 | 1.3 ms | 2× |
| 06-cycle | 7 | 3 | 3 | 1 | 0 | 171 µs | 1× |
| 07-transport | 30 | 4 | 5 | 2 | 3,000 | 402 µs | 5× |
| 08-policy | 7 | 5 | 5 | 1 | 0 | 38 µs | 1× |
| 09-population | 10,087 | 6 | 60 | 1 | 0 | 451 µs | 168× |
| 10-billion | 1,008,500,002 | 6 | 60 | 1 | 0 | 871 µs | 16,808,333× |
| 11-railchain | 133 | 6 | 25 | **3** | **2,300** | 83 ms | 5× |
| 12-tradeloop | 54 | 7 | 24 | **2** | **600** | 24 ms | 2× |
| 13-orefield | 1,000,000,092 | 3 | 5 | 1 | 0 | 27 µs | **200,000,018×** |
| 14-privatebay | 98 | 33 | 69 | 1 | 0 | 104 ms | 1× |
| 15-continent | 1,500,001,638 | 12 | 51 | **6** | **4,200** | 2.2 s | **29,411,797×** |

Every configuration is cross-validated three ways: the lumped solver against the
machine-by-machine simulator, the decomposed Room against the monolithic solver
byte for byte, and the Room against the machine-by-machine simulator directly.
Configs 1–10 are v2's, unchanged, and produce v2's numbers exactly — the return
trip defaults to zero, so nothing that did not ask for the new physics got it.

On the two decomposed configurations the Room is also simply faster than the
monolithic solver — 446 µs against 774 µs on config 15 — because a region that
settles alone runs fewer arbitration rounds than one settling inside a plant-wide
fixpoint. That was not the goal and it is not the point; it is a hint about what
the parallel version is worth.

## Limitations

These are real, and worth stating plainly.

1. **Still single-threaded.** Every region is a separate `Blueprint` with a
   separate clock exchanging timestamped messages, which is the shape a thread
   pool or a network wants — and it is still stepped in one loop, on purpose.
   v3's question was whether the decomposition is exact. Optimising before
   knowing the abstraction is how you get an exceptionally fast implementation
   of the wrong thing.
2. **A conservative scheduler, not an optimistic one.** Regions never speculate
   and never roll back, so the parallelism available is exactly the declared
   slack. A plant built entirely from short belts decomposes into regions that
   barely drift.
3. **Lines with private state still do not compress.** Config 14 measures how
   little they actually diverge, which is encouraging, but measuring is not
   proving and nothing yet exploits it.
4. **Orbit transients can be long.** Config 15's orbit is entered at
   t = 29,125,400 and costs 2.2 s to find, all of it transient. The orbit itself
   is 3,200 ticks. Nothing here bounds how long a plant takes to settle.
5. **A region is found, not chosen.** Domain boundaries fall where transport
   latency puts them. A player who builds one compact plant gets one region and
   no decomposition, correctly, and there is no way to ask for a different one.
6. **The event tier is not yet a per-region engine.** `sim.rs` can simulate any
   whole plant machine by machine, and does, on every cross-validation. It
   cannot yet be handed a single region with half a transport hanging off it,
   so the bottom rung of the ladder is a whole-plant fallback rather than a
   per-region one.
7. **Deployment staggering and shared storage are mutually exclusive.** Lines
   that share a bay and start at different phases would need per-phase line
   populations, which is the same v4 dragon as item 3.
8. **The workbench seeks forward cheaply and backward expensively.** A forward
   seek advances the Room it already has; a backward one builds a new one and
   replays. Scrubbing left across a long horizon is therefore the slow
   direction, and the closed form is not yet wired into a seek.
9. **Canvas 2D, not GPU instancing.** Close up, a population draws as a few
   thousand sampled machines rather than the millions the experiment brief
   imagines. The snapshot already carries what an instanced renderer would
   need — class, population, state distribution, seed — so this is a renderer
   limitation, not a model one.
10. **One plant, one page.** No multiplayer, no shared editing, no power, no
    splitters, no fluids. The document is a command log and generic nodes,
    ports, links and storages precisely so those can arrive later.

## Layout

```
src/model.rs      compiled IR: classes with populations, storages with policies
src/dsl.rs        lexer, parser, lowering, validation
src/sim.rs        T1 round-arbitrated event simulator over SoA columns
src/pop.rs        T5 lumped population engine and its closed form
src/analytic.rs   T2 orbit, T3 per-storage rate algebra, T4 archetypes
src/domains.rs    causal decomposition: transit domains, regions, channels
src/rooms.rs      the Room: region blueprints, channels, conservative scheduler
src/graph.rs      Prototype 0: the placed document, and the source it emits
src/snap.rs       Prototype 0: the state at tick T, in the shape a view needs
src/json.rs       Prototype 0: a JSON value, a parser and a writer
src/web.rs        Prototype 0: an HTTP server, in std
src/main.rs       experiment harness, `serve` and `export`
web/              the workbench: canvas, inspector, timeline, timetable
tests/            46 cross-validation tests
configs/          the fifteen configurations
sketches/         where the workbench saves what you build
```

Zero dependencies outside `std`. The workbench added a JSON codec and an HTTP
server rather than a dependency tree larger than the crate they serve.

> **v1:** compress repetition.
> **v2:** compress interaction.
> **v3:** compress causality.
> **Prototype 0:** stop compressing things and go and look at one.
