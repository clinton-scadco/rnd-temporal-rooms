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

**Prototype 1** asks the first question that is about a game rather than about a
solver:

> Can the simulation stay deterministic and cheap while the player is
> continuously changing the graph?

Yes, and the price is one sentence long: **an edit is a rendezvous.** Bring
every region to the edit's tick, harvest the plant's state, compile the new
plant, pour the state back in. It costs `O(cells) + O(nodes)` — tens of
numbers for a plant with a billion machines in it — and it is not a replay.

**Experiment 06** stops asking about factories altogether and asks about one
*building*. Everything above treats a machine as a recipe with a multiplier, and
`xN` is a thin question to build a game on — so the machine designer is a
standalone prototype in which the player assembles the inside of a building out
of typed components, and the finished thing compiles to a startup transient plus
an exact periodic orbit rather than to an average.

**Experiment 07** asks whether that generalises past a power plant. It expands
the eight components into a construction kit of thirty-eight across eight
families, makes a wire carry a *substance with properties* rather than a number,
and then tries to answer four different briefs with the one vocabulary. The
recipe stays `Iron → Gear`; the machine that performs it is where the
complexity lives.

**Experiment 08** asks what all of that *looks like*. A design is components on
a grid with typed ports; a plant is vessels, pipework, steel and a building. So
the visual compiler takes the one and derives the other, in five passes, from
nothing but the document and a seed:

> **Can the player's engineering design itself become the art direction?**

Yes — and the load-bearing half of the answer is the direction the arrow points.
`RenderGeometry = Generate(MachineDesign, VisualSeed)`, never the reverse: the
generated mesh never defines the machine.

**Experiment 09** takes the grey box that came out of it and asks how far a
*look* gets without touching the generator. One axis, four builds of the same
plant, compared side by side: the baseline, a pure repaint, the vocabulary of
how things are joined and installed, and articulated archetypes. The repaint —
which is a pass that may write one field of a piece and cannot move anything —
turns out to do over half of the work.

**Experiment 10** hands the third dimension to the player: a component gets an
elevation and a quarter turn, a port becomes an interface rather than a
coordinate, and the router is allowed to say *no valid route found* rather than
draw nonsense.

**Prototype 2** stops proving things separately. Four laboratories and no game
is a poor place to stop, so the eleventh experiment is the first small complete
one: two players, one room code, one clock that never pauses, machines designed
in three dimensions and placed in a world that is already running, and three
independent reconstructions of the same command stream, compared by hash every
simulated second.

> **Can two players continuously build and redesign a deterministic factory
> together, in real time, while the simulation keeps running and both clients
> remain exactly synchronized?**

Yes — and the solver did not change by one line. What changed is what a command
*is*: an intention rather than a document diff, validated and stamped by one
authority, with the diff derived deterministically on the other side.

**Prototype 3** asks the only question Prototype 2 left standing. Two people can
build one deterministic factory together while it runs — fine. But a Room was
still a disposable challenge: meet the objective, read the screen, and there is
nothing the finished factory is *for*.

> **Does finishing one factory make me want to start the next one?**

So the twelfth experiment is five hand-authored Rooms in a fixed graph, on one
clock, with trains between them, twelve unlockable *components* rather than a
research tree of percentages, and a design library that remembers where every
machine came from. A Room becomes a supplier rather than a scoreboard, and it
keeps supplying while you are somewhere else — which is the first time this
project has spent the promise it has been making since Prototype 1.

## Quick start

```powershell
.\run.ps1          # build + run all fifteen configurations
.\run.ps1 -Test    # 211 cross-validation tests
.\run.ps1 -Serve   # the workbench, at http://127.0.0.1:8787
.\run.ps1 configs/11-railchain.factory                     # just one

# Prototype 1: play a scenario without a browser
.\run.ps1 -Play scenarios/first-gears.scenario
.\run.ps1 -Play scenarios/first-gears.scenario --buy "GearPress=3@15000"

# Experiment 06: the machine designer, which shares nothing but the repo
.\run.ps1 -Machine                             # every design, judged
.\run.ps1 -Machine serve                       # the designer, at :8788
.\run.ps1 -Machine run designs/03-compact.machine
.\run.ps1 -Machine why designs/04-stalled.machine
.\run.ps1 -Machine compile designs/05-pulsed.machine
.\run.ps1 -Machine check                       # its front end, without a browser

# Experiment 08: the same document, built as a plant
.\run.ps1 -Machine forms                       # all sixteen, counted and hashed
.\run.ps1 -Machine form designs/15-turbinehall.machine --png hall.png
.\run.ps1 -Machine form designs/10-refinery.machine --obj refinery.obj
.\run.ps1 -Machine kit --png sheet.png         # the asset library, all of it

# Experiment 09: the same plant, built four ways, compared
.\run.ps1 -Machine read designs/10-refinery.machine --png sheet.png
.\run.ps1 -Machine reads                       # every design, at every grade
.\run.ps1 -Machine form designs/03-compact.machine --grade a --png then.png

# Experiment 10: where everything is, which face every port ended up on,
# how every connection was routed, and what is in the way
.\run.ps1 -Machine space designs/17-stacked.machine
.\run.ps1 -Machine spaces                      # every design, routed and judged

# Prototype 2: two players, one factory, one clock that does not stop
.\run.ps1 -Room                                # the game, at :8790
.\run.ps1 -Room test                           # the primary multiplayer test
.\run.ps1 -Room fail                           # the failure tests
.\run.ps1 -Room goals                          # twenty-one hand-written problems
.\run.ps1 -Room parts                          # the catalogue, and what it compiles to
.\run.ps1 -Room check                          # its front end, without a browser
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

## Prototype 1: a factory you can change while it runs

Prototype 0's document was a *drawing*. You edited it, the whole thing was
compiled, and the run started again from tick zero. That is fine for a workbench
and useless for a game, because a player does not design a factory and then
watch it: they build a bad one, watch it fail, and fix it at tick 12,000 without
losing the twelve thousand ticks.

So the document stops being a drawing and becomes a **history**:

```text
  base plant at t=0
  tick 12,000: place  Smelter2
  tick 12,000: wire   OreYard -> Smelter2
  tick 12,500: retune GearPress    (recipe changed)
  tick 13,000: retune Rail         (8 vehicles -> 12)
```

and the thing the solver is asked for is still a pure function of two arguments:

```text
  state(log, T)
```

That matters more than it looks. v1 to v3 all leaned on *the state at tick T is a
function of the plant and T*, and every convenience in the stack is downstream of
it: the stateless server, the scrubbable timeline, the reload that cannot
desynchronise. A log is still one argument. Nothing above that line had to change
its mind about anything.

### An edit is a rendezvous

A region in v3 runs alone, at its own clock, as far ahead as its causal slack
allows. Two regions of one plant are routinely thousands of ticks apart. So
"apply this edit at tick 12,000" has to answer: **whose** tick 12,000?

There is one honest answer, and `Room::run_until` had already stated it — in
between global barriers there is no such thing as "the state of the plant". An
edit therefore forces one:

```text
  1. bring every region to the edit's tick        (a barrier)
  2. harvest the plant's state                    (O(cells))
  3. compile the new plant                        (O(nodes))
  4. pour the state back in, and settle           (O(cells))
```

```text
  cost of an edit  =  O(cells) + O(nodes)
  cost of a replay =  O(ticks)
```

`cells` is the compressed width of the population state: tens, for a plant with
a billion machines in it. So a player may edit as often as they like and the
cost never becomes proportional to what they have built.

### What crosses the boundary, and what does not

A `Carry` is contents, populations, arbitration pointers and counters, keyed by
**name** — because a name is the only identity a document has. Storage indices,
class indices and region membership are all things the next compile is entitled
to choose differently, and an edit that adds a link moves the region boundaries
underneath everything.

Two details are load-bearing and neither is obvious:

- **The round-robin pointer is a client, not an index.** A storage's fairness
  pointer is a position in its client list, and an edit is exactly the thing
  that changes that list. Carrying the index silently hands the next turn to a
  different machine — a policy change nobody asked for. The pointer is carried
  as the *class it is resting on* and re-resolved.
- **A closed form is a claim about a plant that started empty at t=0**, and a
  resumed region did not. `Room::new` may label a sealed region `Closed`; a
  resumed one is honestly a population run.

What does *not* cross is every opinion the scheduler formed on the way here:
advances, messages, skew. Those are properties of a run, not of a tick — the
lesson Prototype 0 already learned once when the same snapshot came out
different depending on whether it was scrubbed to or played to.

### The test that carries the whole thing

The load-bearing test is the one where the edit **does nothing**.

Retune a node to exactly what it already is at tick *k*: the plant is the same
plant, the emitted source is byte-identical, and the only difference between the
two runs is that one went through the barrier, the harvest, the recompile and
the reseed. If the states still agree at every probe afterwards, the machinery
carries everything and invents nothing.

```text
an_edit_that_changes_nothing_changes_nothing    12 plants x 4 cut points x 5 probes
edits_do_not_accumulate_error                   9 no-op edits in a row
a_carry_is_worth_the_ticks_it_replaces          snapshot at a boundary + the rest of the log
a_plant_that_does_not_compile_still_has_a_document
                                                the half-built case, which broke once
```

That last one is the networking proof rehearsed early. `POST /api/verify?t=N`
answers tick *N* twice — once from the beginning, once from the canonical
snapshot at *N/2* — and compares signatures. It is also printed by every
headless `play`, so a desync would have to survive being noticed by accident.

### The carry is the snapshot

The cache that makes scrubbing fast and the object a joining client would be
sent turned out to be the same type. Prototype 0 cached a compiled `Plan` and a
live `Room`, which meant leaking both into `'static` to escape a
self-referential borrow. A `Carry` is plain owned data with a JSON encoding and
a canonical signature, so that machinery is simply gone — and P2's *server:
command log + canonical snapshots* is now a description of code that exists.

### Prototype 1 also has pressure

A simulator becomes a problem the moment somebody wants something out of it that
it cannot currently deliver. `scenarios/first-gears.scenario` is a budget, an
order and a deadline, posed *about* `configs/p1-gears.factory` rather than
inside it — its own file, its own parser, and the solver never hears about any
of it. The plant runs identically with the scenario deleted.

Three things came for free, or nearly:

- **Finite resources needed no new construct.** An ore deposit is a storage with
  contents and no producer wired to it. When it empties the mine starves for the
  ordinary reason, and every part of the machinery that explains starvation
  explains this too.
- **Delivery is what leaves through a sink** — `cycles x batch`, summed over the
  sink classes that consume the item. Not the item's total consumption: a gear
  press eating plates is not a delivery of plates, and an order that counted it
  would be satisfiable by building a machine that eats its own supply chain.
- **Costs are per member.** `Smelter x40` is forty smelters and is priced as
  forty smelters, because the object on the canvas is a bookkeeping convenience.

### Why is this not running?

The usual answer is a status word floating over a machine. What a player needs
is the sentence after it, and every number in that sentence was already in the
simulation state:

```text
Smelter                          Smelter
STARVED                          BLOCKED
needs 25 IronOre                 holding 25 IronPlate
OreYard holds 0                  PlateBay: 1,985 of 2,000 · 99.2% full
from Rail: 100% busy, 0 idle     to GearPress: 100% busy, 0.250/tick
next delivery t=14,200 (+840)
```

Bay contents are in `Pop::qty`; populations are the four buckets; the arrival
tick of the next train is the deadline v3 has stored for every batch in the air
since transports became channels. `why.rs` reads. It computes no physics, which
is exactly why it was worth writing now.

It also answers the question one level up. A class is a **constraint** when it
never waits — no member idle, no member blocked — and something drawing on it
*is* waiting. That definition is mechanical rather than clever, and it finds the
honest bottleneck instead of the loud one.

### What the scenario caught

`p1-gears.factory` is underbuilt in two places at once: a rail moving 0.5
ore/tick, and a gear press consuming 0.25 plate/tick behind it. The rail is the
bottleneck a player notices, because it is the one with a queue in front of it.
The press is the one that binds.

```powershell
.\run.ps1 -Play scenarios/first-gears.scenario
#   holding the plant back: GearPress at 0.125/tick, starving Delivery
#   [MISS] deliver 12,000 Gear by tick 60,000 — 7,200 of 12,000

.\run.ps1 -Play scenarios/first-gears.scenario --buy "Rail=8@15000"
#   spent 420
#   holding the plant back: GearPress at 0.125/tick, starving Delivery
#   [MISS] deliver 12,000 Gear by tick 60,000 — 7,200 of 12,000

.\run.ps1 -Play scenarios/first-gears.scenario --buy "GearPress=3@15000"
#   spent 240
#   holding the plant back: Rail at 0.500/tick, starving Smelter
#   [MET ] deliver 12,000 Gear by tick 60,000 — 14,500 of 12,000
```

Seven extra rail vehicles deliver **exactly** as many gears as none did — to the
unit — and the constraint report never stops pointing at the press. Two extra
presses cost less, meet the order, and move the bottleneck down the chain, at
which point the smelters start reporting `TRANSPORT LIMITED` and the rail
*becomes* worth buying.

`buying_the_wrong_upgrade_buys_nothing` is a test now. It was written asserting
the opposite and failed, which is a fair description of how the mistake feels to
make.

### What changing it while it runs broke

| the question | the answer, measured |
|---|---|
| Can a mid-run edit break the plant? | Yes, and it names its own tick: ``t=30,000 · Smelter · `Smelter` produces items but has nowhere to put them``. Demolishing a load-bearing bay is an ordinary way to hit it. |
| Then how do you demolish anything? | All at once. Several commands at one tick are **one** recompile, so the half-demolished states in between are never compiled and never have to be legal. |
| When does an edit take effect? | At its own tick. `sig(plain, 20_000) != sig(edited, 20_000)` is a test; an off-by-one here would be a command log that does not mean what it says. |
| What happens to a machine you scale down mid-cycle? | It is taken out of service and the batch is lost — idle members first, then vehicles running home empty, then finished batches, then work in progress. Only the last two lose anything, and the player is told what it cost. |
| What happens to a bay you demolish with ore in it? | The ore is scrapped, and `scrapped` says so. A game that quietly deleted it would be lying about what the player just did. |
| Can you rename a node? | No. A name is the identity everything crosses an edit by, so renaming is a demolition and a rebuild — and the inspector stopped offering a name field rather than pretending otherwise. |
| Does the browser apply edits? | No. It *proposes* them: append the command, ask again, take the graph that comes back. There is one implementation of what an edit means, it is in Rust, and it is the one a replaying client would use. |
| Then how do you place a machine? | This broke, and it is the best bug of the experiment. A machine you have just placed is not wired, so the *plant* does not compile — and the first version returned no document, so nothing appeared on the canvas and there was nothing to wire. A refused command and an unfinished plant are not the same failure: one can never work, the other is what a factory looks like halfway through being built. A `Fault` now carries `refused` and the document it is complaining about. |
| Is dragging a node an edit? | No, and the cache has to know that. `Log::key` strips positions from the base source *and* from every command, or a plant would recompile once per pixel. |

## Experiment 06: the machine designer

Everything above treats a building as a recipe with a multiplier. The only
decision a player makes about a smelter is *how many smelters*, and `xN` is a
thin question to build a game on. So the sixth experiment is a standalone
prototype — its own binary, its own front end, its own file format, nothing
wired into the solver — asking a different one:

> A building is a small deterministic factory graph the player assembles, then
> compiled into a reusable macro-machine once its behaviour is known.
>
> **Is assembling machines from functional components a fun optimisation
> problem that produces understandable but non-obvious designs?**

One brief, with four halves that fight:

> Produce at least **100 MW** from **one** fuel source, while minimising
> **footprint**, **water use** and **wasted heat**.

### Eight components, one constraint each

| component | footprint | the one interesting thing about it |
|---|---|---|
| Fuel / Heat Source | 4×4 | burns at its throttle setting whether or not the heat is wanted |
| Heat Pipe | 3×1 | 400 heat/tick, and it loses 2% of everything it carries |
| Water Source | 2×2 | 200 water/tick, and no more |
| Heat Exchanger | 3×3 | needs heat **and** water in a fixed 5:2 ratio; short of either it makes less |
| Steam Pipe | 3×1 | 150 steam/tick |
| Steam Buffer | 3×3 | holds 2000; in *pulse* mode it fills quietly and empties hard |
| Turbine | 3×2 | 80 steam/tick at 75% — but stalls below 40, and spins up slowly |
| Generator | 2×2 | 70 rotary/tick at 90%, so 63 MW and not one more |

Ports are typed — `heat`, `fluid`, `steam`, `rotary`, `electrical` — and a
connection is legal only between an output and an input of the same type. There
is no pressure, no temperature, no torque and no phase change. Every component
has a capacity, most have an efficiency, and exactly one has a *threshold*.

### A tick

```text
1. transfer   move quantities along wires, obeying both ends
2. step       every component consumes its inputs and fills its outputs
```

In that order, which is the whole latency model: a quantity put into an output
buffer during step *t* cannot move until the transfer at *t+1*, so every hop
costs a tick and every pipe costs two. Nobody wrote a delay line.

Contention is a *stated* policy, because v2 already learned that lesson the
expensive way: max-min fair, with the remainder rotating on a cursor that is
part of the machine's state. That last detail is why a fan-out of three on a
budget of ten has a period of three rather than a permanent favourite.

### Why a pipe exists

The first version had no reason for one. A reactor's heat port reached every
exchanger on the plot for free, so a Heat Pipe was a 2% tax nobody would ever
volunteer for. The fix is one constant:

```text
a direct connection spans 6 clear tiles. Further than that needs a pipe.
```

which makes the tile grid load-bearing. Things that work together have to sit
together; a pipe is how you buy distance; the price of distance is the loss.
That is the same sentence as *minimise footprint*, which is why it belongs in
the brief rather than in a tooltip.

### The compiled macro-machine

The brief is explicit that a finished machine must not collapse into `input ×
efficiency = output`, and it is right to be suspicious — that is what every
factory game does, and it is why two plants with the same average behave
identically under a supply that wobbles. What a machine compiles to here is

```text
startup transient  +  exact periodic orbit
```

found by the least clever method available: run it, and watch for it to repeat
itself. A component's state is a handful of small integers — some buffers, a
warmth, a spin, a tank's mind made up — so the whole machine's state is a short
byte string, and the step function is a pure function of that string. Therefore

```text
key(s) == key(t),  s < t   =>   state(s + k) == state(t + k)  for all k
```

and that one observation buys everything:

```powershell
.\run.ps1 -Machine compile designs/05-pulsed.machine
```

```text
{"name":"Pulsed",
 "externalInputs":[{"what":"Fuel","rate":20},{"what":"Water","rate":80}],
 "externalOutputs":[{"what":"Electricity","rate":51.714285714285715}],
 "footprint":"16 x 8","internalComponents":10,"internalStateBytes":271,
 "transient":211,"periodicOrbit":21,
 "note":"211 ticks of startup, then the same 21 ticks forever"}

tick 1,000,000,000 is indistinguishable from tick 223 — 223 steps, not 1,000,000,000
by then: 51,714,277,321 MW-ticks, 20,000,000,000 fuel, 79,999,995,584 water
```

The average is `1086/21` MW, kept as a rational, because 51.71 is a rounding of
a fact and comparing two designs by their roundings is how you end up unable to
explain why the worse one won. And the orbit is the part an average throws
away: two of the designs below produce **exactly** the same 216 MW and are not
the same machine.

Nothing here is trusted. `machine verify` reaches every probe twice — once by a
straight tick-by-tick run, once by prefix + laps + remainder — and the tests do
it either side of the transient, on exact multiples of the period, and on ticks
that are not.

### Six designs, one brief

```powershell
.\run.ps1 -Machine
```

```text
design                           MW   water  wasted     plot   parts  util  start    period
----------------------------------------------------------------------------------------------
01-first-try                  54.00    80.0   800.0     15x6       5   65%    120         1
                           ! 54.00 MW is short of 100 MW by 46.00
02-more-of-everything        216.00   320.0   200.0    15x19      17   83%    127         4
03-compact                   108.00   160.0     0.0     12x6       8   81%    122         2
04-stalled                     0.00    80.0     0.0     16x8      10   18%    122         3
                           ! 0.00 MW is short of 100 MW by 100.00
05-pulsed                     51.71    80.0     0.0     16x8      10   36%    211        21
                           ! 51.71 MW is short of 100 MW by 48.29
06-radial                    216.00   320.0     0.0    14x13      15   87%    122         4
```

There is deliberately **no score**. A single number would be maximised within
two attempts and the interesting part — that a *compact* 100 MW plant and a
*clean* 216 MW plant are different machines — would be gone by the third.

Three of the six meet the brief, and none of the three dominates:

- **03-compact** noticed what the reactor is *for*. Two exchangers can take 500
  heat a tick, so a reactor at full throttle is heating the sky with the other
  half. Turned down to 40% it makes 108 MW on 40 fuel/tick, wastes nothing, and
  fits in 12×6.
- **02-more-of-everything** is the design everybody builds second: four of each,
  full throttle, heat pipes to reach the far half of the plot. 216 MW — and 285
  tiles, 200 wasted heat a tick, and seventeen components.
- **06-radial** is the same 216 MW with the reactor in the *middle*, so all four
  exchangers are in reach and both heat pipes are gone, and with the throttle
  set to what four exchangers can actually swallow. Two thirds of the land,
  fifteen components, nothing wasted at all.

### The one nobody guesses first

`04-stalled` and `05-pulsed` differ by a single word in the file.

```diff
-tank      TK1 at 7,0
+tank      TK1 at 7,0  pulse 400 0
```

Three turbines on 80 steam a tick is 26 each, and a turbine below 40 does not
turn slowly — it does not turn at all. So the machine produces nothing, and
quietly condenses every drop of steam it makes, forever. Gather the same trickle
into 400 and throw it at them and it is 67 each for two ticks in seven, which is
over the line; a turbine spins up faster than it spins down, so it does not
quite fall back between pulses.

**0.00 MW → 51.71 MW. Nothing was added and nothing was made bigger.** The
period goes from 3 to 21, which is the orbit reporting that a genuinely
different machine is now running.

That is the shape of result the experiment was looking for: a decision that is
non-obvious beforehand, explainable in one sentence afterwards, and visible in
numbers the tool was already showing.

### Saying why

A design tool where the player can see that the machine is bad but not *why* is
a puzzle with the solution torn out, so every component composes its own
explanation out of state that was already there — the same sentences in the
inspector, in `machine why`, and in the "holding it back" list.

```powershell
.\run.ps1 -Machine why designs/04-stalled.machine
```

```text
HX1        Heat Exchanger       STARVED
    needs: 250 heat + 100 water/tick
    arriving: 200 heat, 80 water
    short of heat: 200 of 250
    making 80 steam/tick
    utilisation: 80.0%

T1         Turbine              STALLED
    needs: 80 steam/tick
    available: 27 steam/tick
    spin: 0/30
    below the 40 steam/tick it needs to turn over at all
    a Steam Buffer in pulse mode can push a trickle over the line
    steam condensed and lost: 27/tick
    rotary out: 0/tick
```

### The designer

```powershell
.\run.ps1 -Machine serve        # http://127.0.0.1:8788
```

Place, move, wire, unwire, run, pause, and scrub — to tick 10⁹, which is free,
and the timeline says so out loud: *`t=1,000,000,000 answered by simulating
t=223`*. The rule the workbench is built on holds here too, and is the one thing
that was never allowed to bend:

```text
Simulation  ->  State(t)  ->  RenderSnapshot  ->  Renderer
```

The canvas does not know that a heat pipe leaks 2%. It knows this wire carried
392 units last tick and that its rate is 400, which is enough to draw it thick.

The browser owns a copy of exactly two rules — whether a wire is legal, and what
the file looks like — because refusing to draw an illegal connection has to
happen while the pointer is still moving. Both copies are fetched from, or
checked against, the Rust that has the final word, by a test that needs no
browser:

```powershell
.\run.ps1 -Machine check
```

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Do genuinely different designs meet the same brief? | Yes. 108 MW on 72 tiles and 216 MW on 182, neither dominating, plus a worse 216 MW on 285. |
| Is any of it non-obvious? | Throttling *down* to make the same power, and rescuing three dead turbines with one word, were both found by building the thing and reading its own complaints. |
| Does the orbit survive compilation? | Yes, and it matters: periods of 1, 2, 3, 4, 4 and 21 across six designs, two of which have identical averages. |
| Is the answer at tick 10⁹ cheap? | 223 steps rather than a billion, cross-checked against a straight run at every probe. |
| Does a component always say why it is stopped? | Yes, and it is a test — a component that explains nothing fails it. |
| Is water an interesting axis? | **No.** Water per megawatt is fixed by the chain unless steam is vented, so it only separates designs that are already wasteful. It is reported honestly and it is the weakest of the four halves. |
| Is the Steam Buffer ever *optimal*? | Not found yet. It rescues a machine that is below the threshold; it never beats simply building fewer turbines. A real limitation of the current numbers, not a feature. |
| Does it scale? | Not asked. That battle has already consumed enough innocent CPU cycles. |

## Experiment 07: the construction kit

Experiment 06 proved that assembling a *power plant* out of parts is an
interesting optimisation problem. It proved nothing about assembling anything
else, because it had eight components and they were the eight a power plant
needs. The follow-up asks the obvious next question and names the obvious trap
in the same breath:

> Expand the vocabulary into a general industrial construction kit — but define
> a few **families of primitives** that combine into many machines, or you will
> accidentally recreate a parts catalogue from an engineering supplier, which is
> a thrilling prospect for roughly seven people.

So the target is not "more components". It is:

> **Recipes define what transformation is required. Machines are player-designed
> networks that provide the physical processes needed to perform it.**

`Iron Plate → Gear` stays one line. The forty-ton monstrosity that performs it
7% more efficiently is where the game is.

### What a wire carries

Experiment 06 had five port types and a connection carried a *number*. That can
never express the sentence above, because a machine that only moves amounts can
never change what a thing **is**. So a quantity is now a `Stuff`: a substance,
plus five small properties.

```text
domains     material  fluid  gas  heat  rotary  mech  electrical
properties  temperature  particle size  purity  form  speed
substances  water coal ore iron slag crude light middle heavy
            + heat, torque, stroke, electricity
```

And a component's job is to *modify* it:

```text
crusher     size    lump      -> coarse -> crushed
mill        size    crushed   -> powder
separator   purity  40%       -> 82% rich  +  12% tailings
furnace     temp    ambient   -> red
rolling     form    billet    -> strip
press       form    strip     -> gear
```

The outer game still has one item called Iron Ore. Nothing here needs
`CrushedIronOre`, `FineIronOre` or `SlightlyMoistFineIronOre` to exist as
separate icons. Pyanodons, please remain seated.

**Phase is not a property — it is a domain.** Water boiled by an exchanger comes
out in `gas`; iron melted past band 7 leaves a furnace through a different port,
in `fluid`. A phase change is therefore something you can *see*: the wire changes
colour, and it will not plug in where the old one did.

```text
furnace   heat in  +  material in
              -> out     material, five bands hotter
              -> molten   fluid, if that took it past melting
```

That also settles the thing the note was firm about: `steam` is not a port type.
Steam is water, in gas, at a temperature.

Every property is a small integer — a band, a percent, one of four words —
because `orbit` compiles a design by watching its state repeat, and a state
containing a float repeats approximately, which is to say never. A whole stuff
is six bytes, and two stuffs are equal or they are not.

### A component is a row in a table

Thirty-eight components in eight families, and fourteen of them are pure data:

```text
Crusher {
    draws  drive  5 rotary   at speed <= 2
           in    10 material hardness <= 8, no finer than coarse
    makes  out   10 material one size finer
    rate   10 batches/tick
}
```

```powershell
.\run.ps1 -Machine parts process
```

The other twenty-four are hand-written, but not twenty-four times over: one
`conduit` is six kinds of pipe, one `store` is four kinds of buffer, one
`source` is three kinds of inlet, one `dump` is three kinds of boundary. Six
components are genuinely one of a kind — reactor, gearbox, turbine, generator,
furnace, column — and each has a warm-up, a ratio, a spin-up curve, a rounding,
a phase change or a separation split that a table row could not have expressed.
The generator is on that list for an unglamorous reason: rounding its intake to
whole batches would discard up to nine rotary a tick, and experiment 06's six
designs are reported to two decimal places.

The payoff is the one the note was really after:

> a motor is not part of the crusher's recipe. It supplies the rotary domain.

Six crushers can hang off one engine through a shared shaft, and nobody had to
write that down as a special case:

```text
             ┌→ Crusher
Engine → Shaft → Crusher
             ├→ Crusher
             └→ Crusher
```

### Four briefs, one component set

One brief proves a component set can answer one question. Four of them, answered
by the same thirty-eight components, is the only evidence that the vocabulary is
a vocabulary rather than an elaborate way of writing `Boiler Mk2`.

```powershell
.\run.ps1 -Machine
```

```text
GENERATE ELECTRICITY  --  heat, fluid, gas, rotary, electrical
design                          made     grid   water  wasted     plot  parts  util  start   period
01-first-try                   54.00      0.0    80.0   800.0     15x6      5   65%    120        1
02-more-of-everything         216.00      0.0   320.0   200.0    15x19     17   83%    127        4
03-compact                    108.00      0.0   160.0     0.0     12x6      8   81%    123        2
04-stalled                      0.00      0.0    80.0     0.0     16x8     10   18%    122        3
05-pulsed                      51.71      0.0    80.0     0.0     16x8     10   36%    212       21
06-radial                     216.00      0.0   320.0     0.0    14x13     15   87%    123        4

CRUSH ORE  --  motor, gearbox, rotary, material transformation
07-crushline                   35.67    220.2     0.0     2.0    21x14     15   64%    152      108
11-steamcrusher                37.40      0.0   311.2   224.1    32x16     25   70%    120       60

DISTIL MIXED FLUID  --  heat, phase change, fluid separation
10-refinery                    36.00      0.0    18.0    30.0    26x10     10   46%    192        6

MANUFACTURE GEARS  --  material handling, forming, buffering
08-stamping                    49.50    121.0     0.0     5.4    22x12     13   56%     57       40
09-machining                   24.00    128.9     0.0     0.0     11x9      9   49%      4       18
12-onemotor                    18.00     60.0     0.0    15.1    22x12     11   27%     58       50
```

Experiment 06's six are unchanged to the decimal place, which was a constraint
rather than a coincidence: they are the regression test for everything
underneath. The three transients that moved by one tick moved because a boundary
port now exports *after* the transfer rather than during the step, so that a
generator can power a motor inside its own machine and export the difference.

### The same brief, twice, differently

`07-crushline` and `11-steamcrusher` both crush ore past the target. Neither
dominates, and the difference is not a number — it is which domain the rotary
comes from.

| | 07-crushline | 11-steamcrusher |
|---|---|---|
| concentrate | 35.67/tick | 37.40/tick |
| grid | **220 MW** | **none** |
| fuel | none | 100/tick |
| water | none | 311/tick |
| plot | 21×14, 15 parts | 32×16, 25 parts |

The steam crusher has no generator and no motor anywhere in it. Four turbines
drive two line shafts directly, because rotary is a *domain* and a turbine
already makes it — putting it through a generator and back through a motor
would be two conversions and 19% for nothing. What it costs is precisely
everything the power brief was trying to minimise.

### Refusal is the mechanic, not the error case

Experiment 06 had one refusal in the whole simulation: a turbine below its
threshold. Experiment 07 has a general one, and it is *the* interesting
addition, because it creates a design that is wired correctly, is short of
nothing, and produces nothing.

```powershell
.\run.ps1 -Machine why designs/08-stamping.machine
```

```text
R1         Rolling Mill         REFUSED
    needs 60 drive/tick — speed 3+
    needs 60 in/tick — billet, scorching or hotter
    drive: Rotary (speed 6) (60 held)
    in: Iron Ore (lump, 40% pure, red) (120 held)
    REFUSED — wants billet, and this is raw
    something upstream has to shape it first
```

That is a real transcript from building the gear line: the iron inlet had been
left on its default substance, so a rolling mill was being handed hot *ore*. The
tool found it, named the property, and said what fixes it — which is the whole
argument for making a component's constraints data rather than code, because the
sentence is generated from the same table the simulation obeys.

The load-bearing one is the drive train. A crusher will not take a shaft turning
at speed 6; a mill will not take one turning at speed 1. So a motor cannot drive
both, and the diff between a machine that makes nothing and a machine that works
is one component:

```diff
+ gearbox   GB1 at 6,0   ratio 4
- wire MO1.rotary -> C1.drive
+ wire MO1.rotary -> GB1.in
+ wire GB1.out    -> C1.drive
```

### Half the drive is not half the gears

`12-onemotor` is `08-stamping` with one motor instead of two. The shaft splits
54 rotary fairly: 27 to the rolling mill, 27 to the crank. The mill is content
to run slowly and does. The crank is content to run slowly and does, at 23
strokes a tick. The press is not — **a press fed half the strokes it needs does
not make half a gear, it fails to close** — and a stroke that arrives unused does
not queue for later. It has happened.

```text
08-stamping    two motors    49.50 gears/tick    MET
12-onemotor    one motor     18.00 gears/tick    short by 2.00, and 15 heat/tick of
                                                 strokes falling on nothing
```

`mech` is the only domain in the kit that cannot be stored, which is the same
decision the turbine's condensation was in experiment 06 and produces the same
shape of result: a threshold plus something that perishes is where non-obvious
designs live.

### The compiled machine keeps the properties

The whole point of compiling an orbit was that a finished machine advertises
exact external rates. Now those rates have properties attached, and the outer
factory sees this and never opens it again:

```powershell
.\run.ps1 -Machine compile designs/07-crushline.machine
```

```text
in    Iron Ore (lump, 40% pure)      89.17/tick
in    Electricity                   220.19/tick
out   Iron Ore (powder, 82% pure)    35.67/tick
waste Iron Ore (powder, 12% pure)    53.50/tick
waste Heat (ambient)                  2.00/tick

plot  21 x 14 · 15 parts inside · 675 bytes to resume it
loop  152 ticks of startup, then the same 108 ticks forever

tick 1,000,000,000 is indistinguishable from tick 244 — 244 steps, not a billion
```

One item goes in, the same item comes out, and everything that happened to it is
in the parentheses. That is the sentence the whole `stuff` module exists to make
printable.

### Counting the primitives instead of arguing about them

The note proposed its own acceptance test, so it is a command rather than a
paragraph:

> If the same motor, pump, heat exchanger, buffer and shaft naturally appear
> across several designs, you have found good primitives. If every challenge
> requires ten bespoke components used nowhere else, the abstraction is wrong.

```powershell
.\run.ps1 -Machine reuse
```

```text
reactor      source            12       12  power distil crush
pump         source            12       17  power distil crush
exchanger    heat              11       24  power crush
turbine      mechanical        11       30  power crush
generator    mechanical        10       25  power
outlet       sink               6        6  crush gears distil
shaft        transport          5        7  crush gears power
inlet        source             5        7  crush gears
motor        mechanical         4        9  crush gears
...
  28 of 38 components are used at all, and 11 of those appear in more than one
  brief.

  not used by any shipped design: heater, steampipe, fluidpipe, chute, screw,
  hopper, drum, flywheel, valve, clutch
```

The infrastructure passed: reactors, pumps, exchangers, turbines, shafts, motors
and outlets all turn up across two or three briefs, and a reactor that was built
for a power plant now heats a distillation column. The process components did
not span briefs and were never going to — a crusher belongs to the crush brief
the way a verb belongs to a sentence.

**Ten components earned nothing, and that is the most useful thing the
experiment produced.** It is not that they are badly designed. It is that
*every port already has a capacity*, so every component in the kit is already a
buffer, and a dedicated store has nothing left to do. Experiment 06's Steam
Buffer only mattered because a turbine **discards** what it cannot use; nothing
else in the kit does, so nothing else can be rescued by putting a tank in front
of it. The control family has the same problem from the other end: a valve
limits a flow nobody was over-supplying.

The fix is not more components. It is smaller port capacities — roughly one
tick's worth — so that a store is a decision instead of a decoration. That is a
one-line change to the table and a different experiment.

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Can one component set answer four different briefs? | Yes. 38 components, 8 families, and a met design for each of power, crush, distil and gears. |
| Does a recipe stay simple while the machine gets complicated? | Yes. `Iron → Gear` is performed by a 13-part stamping line and a 9-part machining cell with no component in common except the inlet and the outlet. |
| Do properties beat intermediate items? | Yes, and the macro-machine is the proof: `Iron Ore (lump, 40%)` in, `Iron Ore (powder, 82%)` out, one item, five properties. |
| Is a constraint better than a rate penalty? | Yes, and it was not close. "REFUSED — wants billet, and this is raw" teaches the mechanic in one line; a component quietly running at 40% teaches nothing. |
| Does the orbit survive the richer state? | Yes. Periods of 6, 18, 40, 50, 60 and 108 across the new designs, tick 10⁹ still answered in a few hundred steps, still cross-checked against a straight run. |
| Are the primitives primitives? | Mostly. Seven infrastructure components span two or three briefs each; ten components are used by nothing. |
| Do stores and controls earn their place? | **No.** Every port is already a buffer, so a buffer buys nothing. Diagnosed above; the fix is smaller capacities, not more parts. |
| Is `mech` a domain or an affectation? | Honestly, an affectation — one producer, one consumer. It pays for itself only because it is the domain that cannot be stored, which is what makes the press interesting. |
| Is the chemistry family in? | No. Mixers, reactor vessels, electrolysers and scrubbers were cut. The four briefs did not need them, and adding components no brief needs is exactly the parts-catalogue failure the note warned about. |
| Does it scale? | Still not asked. Still not the point. |

## Experiment 08: procedural machine form

Experiments 06 and 07 produced a document: components on a tile grid, typed
ports, wires, tunings, and a verdict. What they did not produce was a *thing*.
The follow-up asks whether the document is enough to build one:

```text
Machine Design
  ↓
semantic 3D layout
  ↓
connection routing
  ↓
structural inference
  ↓
procedural dressing
  ↓
renderable machine
```

and the question underneath it is the one worth the effort:

> **Can the player's engineering design itself become the art direction?**

If yes, machine variety stops being a content-production problem and becomes an
emergent consequence of how players build things.

### The core rule is a direction, and it is the whole architecture

> The generated mesh never defines the machine.

```text
RenderGeometry = Generate(MachineDesign, VisualSeed)
```

`form` reads `design` and writes `Scene`. Nothing in `sim`, `orbit`, `eval` or
`snap` mentions `form`, so no amount of changing what a plant looks like can
change what it does. That is not a convention anybody has to remember — it is
the module graph — and `tests/form.rs` checks it from the far end anyway:

```rust
for style in [Works, Yard, Hall] {
    for world in [0, 1, 7, 9_999] {
        build(&d, Ask { style, world });
        assert_eq!(before.headline(), eval::report(&d, &compile(&d)).headline());
    }
}
```

Every design in the repository, judged, built twelve different ways, judged
again. If the day ever comes that somebody reads a component's *height* to
decide a rate, the whole architecture has quietly inverted, and this is the test
that notices.

### Five passes, and nothing goes backwards

```text
layout   volumes, mounts, orientation, sockets, clearances
route    A* per connection, on a half-metre grid, then elbows and flanges
frame    plinths, legs, columns, bracing, pipe supports, platforms, stairs
body     thirteen archetypes, assembled out of twenty-five meshes
shell    slab, walls, roof -- with holes where the plant needs them
```

Each pass only ever reads what the passes before it wrote. A plant is not
relaxed into shape or settled into existence; it is derived, in five passes, and
then it stops.

Every position, size and direction in a scene is an `i32` in millimetres, and
floats appear exactly twice: inside the mesh library, and at the boundary where
a scene is written for a renderer. A scene that is going to be described over a
network as `design + seed` has to rebuild identically on the other end, and
that is a far easier promise to keep in integers than in accumulated
floating-point transforms.

### The third dimension is inferred, not authored

The note asks for free 3D placement with semantic snapping. What is here
instead is **free 2D placement with inferred elevation**, and the reason is the
core rule: height that the player places by hand is CAD, and height that *falls
out of the machine* is the thing actually being tested.

```text
rotary   1250   every shaft in the plant at one height, so a line shaft is a
                straight line and a coupling is believable
fluid     750   pumps push along the floor
gas      high   steam leaves the top of a shell, onto the rack
heat     high   and so does heat
material top in, bottom out -- an ore line visibly falls downhill
```

Those five lines are most of why a stranger can read the flow of a plant they
have never seen. Not because anything is labelled — because everything in a
domain agrees about where it lives. A cyclone discharges downwards, so it stands
on legs; something has to fit underneath it, so there is a frame; the frame is
three metres tall, so there is a platform and a stair. Nobody placed any of
that, and none of it is visible in the simulator.

### Sockets snap by looking

Nothing snaps *to* anything. A port simply chooses which of its component's four
faces to leave by, based on where the thing it is wired to ended up:

```text
HX1.steam ──► T1        the outlet is on HX1's east face
T1 moved west           the outlet is now on HX1's west face
```

Two components wired together put their sockets on the faces nearest each other,
so a plant whose plan reads left to right builds pipework that reads left to
right — and moving a component to the far side of its neighbour turns both
sockets round with nobody editing anything.

A socket's *bore* comes from its port's rate, which is the one place in the
visual pipeline where a simulation number decides a dimension. It runs the safe
way round: the machine tells the picture how big to be, never the reverse. A
400/tick heat main is visibly a main; a 20/tick drive is visibly a drive.

### Routing: A* with a heading

The note's cost function, almost literally:

```text
distance + bend penalty + collision penalty + clearance penalty
```

with one thing it does not mention and which matters more than the rest: the
search state is `(cell, heading)`, not `cell`. A bend penalty is a property of
an *edge*, and a router that cannot charge one produces staircases instead of
pipe runs. A shaft is charged 260 for a corner against 10 for a metre of
travel, which is why every drive train in the sixteen designs comes out dead
straight, and why a heat main is allowed to turn.

The seven domains then get seven treatments — and this table is the answer to
the experiment's actual question:

```text
fluid       painted pipe, flanged, the occasional valve
gas         steel pipe, lightly banded, up on the rack
heat        fat lagged pipe, banded every three-quarters of a metre
rotary      thin bright shaft, couplings, straight
mech        thin bright rod, and it will not bend at all
electrical  galvanised conduit, clipped
material    square chute, wide
```

With the labels hidden, a viewer can tell a steam main from a drive shaft from a
cable tray, because those three things are not the same shape, the same size,
the same colour or at the same height. None of that was drawn. It came out of
the port's domain and the port's rate.

### Structure is a consequence, not a decoration

```text
heavy floor equipment          -> a concrete plinth
equipment on legs or a frame   -> columns, head beams and bracing
a long horizontal run          -> a pipe support every few metres
anything to reach above 4.5 m  -> a platform, a handrail and a stair
```

The steam crusher, 25 components, produces **64 pipe supports** across 304
metres of run. The player placed none of them and cannot see them in the
document. Move the machine and they move; delete it and they go.

The first version of the support rule measured each straight section on its own
and left a thirteen-metre span in the refinery hanging in the air, because that
span was made of eight short sections with bends between them. Measuring along
the whole run fixed it, and the test that caught it is still there.

### The machinery produces its own building

The enclosure pass is deliberately the least clever thing in the tree — bounds,
clearance, floor, walls, roof — and it has exactly one idea in it: a wall panel
is left out wherever a run crosses the wall plane, and a roof panel is left out
wherever something is too tall to fit under it. So a nine-metre reactor stands
through its own roof and a heat main leaves through its own hole, and neither
opening was placed by anybody.

What the plant turns out to be is derived too:

```text
skid      small, and with no vessel, tower or press in it
building   under 800 m2 of plot
housed     over it -- walls on the weather sides, open to the sky
```

Of the sixteen designs, one is a skid, six are buildings and nine are housed,
and nobody chose any of it.

### A small kit, arranged

Twenty-five canonical meshes, eight materials, 2,428 triangles in the entire
library:

```text
box cyl dome cone elbow tee flange nozzle valve band
beam grate step rail support anchor coupling bearing gauge panel
fins louvre stack ladder rotor
```

There is no `turbine.mesh`. A turbine is a `Cyl` with a `Cone` for its exhaust,
a `Rotor` in the middle, a `Bearing` for its shaft end, four `Anchor` feet and a
`Nozzle` per port — and a mill is those same meshes at different proportions in
a different material. Thirty-eight components collapse into thirteen archetypes,
and the archetype table is the whole of this experiment's opinion about what
machinery looks like.

The material library is the same argument from the other side: eight materials
for a whole plant, because the moment there is one material per component,
"procedural assembly" has become "hand-authored models with extra steps". Heat
equipment is lagged, process equipment is painted, structure is galvanised, and
after four seconds of looking at a plant a stranger can tell them apart.

> Experiment 09 later found that eight was one short of a *language* and took it
> to twelve, and added four meshes to the twenty-five. Every number in this
> section is still what `-Machine forms --grade a` prints today, hash for hash;
> see [Experiment 09](#experiment-09-the-readability-pass).

### Sixteen plants, and what they cost to draw

```powershell
.\run.ps1 -Machine forms
```

```text
design                     parts  runs  pieces  calls    tris      plot      hash  shell     same
01-first-try                   5     4     504     32   16768    36x17m  d0f7a180  building  yes
02-more-of-everything         17    18     777     33   41744    35x42m  3945fc00  housed    yes
03-compact                     8     8     541     31   21104    30x17m  331db5cf  building  yes
06-radial                     15    16     637     34   34416    33x30m  cbe503b8  housed    yes
07-crushline                  15    19     697     28   32476    47x32m  8d2a79c0  housed    yes
09-machining                   9    13     365     21   15788    26x22m  eaa0757e  skid      yes
10-refinery                   10    12     979     31   47536    57x24m  4d9c72e5  housed    yes
11-steamcrusher               25    30    1290     38   67100    69x36m  664d20c7  housed    yes
13-longreach                   8     7     507     31   19504    75x16m  7ccc2e2d  housed    yes
15-turbinehall                12    12     564     33   29860    39x26m  d09dac8f  housed    yes

  10,984 pieces across 16 designs, drawn in 510 calls: 25 meshes and 8
  materials, arranged.
```

The interesting column is not `tris`. It is `calls`: the number of draw calls a
whole plant costs. Twenty-five components produce 1,290 pieces and **38 draw
calls**, because a scene is not a tree of objects — it is a sorted list that
groups into one instance buffer per mesh-and-material pair. The count grows with
the *variety* of a plant and not with its size, which is what section 10 asked
for.

The `same` column is section 7, checked on every design every time anybody runs
the command: built twice, hashed twice, identical.

### Two seeds, not one

The note proposes

```text
VisualSeed = hash(designId, component layout, styleId, worldSeed)
```

and that is computed — but it is deliberately *not* what a gauge on a turbine is
drawn from. Fold the layout into every cosmetic stream and moving one generator
reshuffles the dressing on all forty components, which is a catastrophic result
for the property the primary experiment is actually testing:

> The important property is **reactivity**, not photorealism.

So the layout digest decides what genuinely belongs to the whole installation —
its paint, its enclosure — and each component draws from a stream named after
*itself*:

```text
whole installation   hash(designId, layout, styleId, worldSeed)
one component        hash(designId, styleId, worldSeed, name, purpose)
```

Move a generator and its shaft reroutes, its plinth follows it, and the reactor
thirty metres away is untouched down to the last handwheel — which is a test
rather than a hope:

```rust
assert_ne!(before.pieces_of("G2"), after.pieces_of("G2"));   // it moved
assert_eq!(before.pieces_of("R1"), after.pieces_of("R1"));   // and nothing else did
```

### Level of detail is a prefix

Every batch's instances arrive sorted so that the ones surviving furthest come
first, with three counts beside them. Drawing the medium view is drawing a
prefix of exactly the same buffer: nothing is re-uploaded, nothing is re-sorted.

```text
close      1290 pieces      equipment, pipes, valves, flanges, rails, gauges
medium      984             equipment, primary pipes, structure
far         427             simplified equipment forms and the mains
very far      1             one box
```

The simulation representation is of course identical at all four, because the
simulator has never heard of any of them.

### It renders without a browser

The plant is drawn in the designer by about four hundred lines of WebGL 2 — one
instance buffer per batch, a frame built in the vertex shader from the same
integer direction the router used. But a claim about what a generated plant
*looks like* is worthless if the only way to check it is to open a canvas and
squint, so there is also a software rasteriser and a PNG writer in `std`:

```powershell
.\run.ps1 -Machine form designs/15-turbinehall.machine --png hall.png
.\run.ps1 -Machine kit --png sheet.png
```

`kit --png` earned its keep in the first hour. A plant is thousands of pieces,
and a mesh that is subtly wrong is invisible in the pile and obvious on a sheet
of twenty-five. It is how the two real bugs in this experiment were found: a
placement frame that was a *reflection* rather than a rotation, which mirrored
every upright piece in the plant, and a triangle winding that turned every
cylinder inside out. Both were invisible in a table of numbers and unmissable in
a 60 kB picture.

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Do functional layouts produce plausible industrial forms? | Yes. Sixteen designs, no bespoke geometry, and a refinery does not look like a crushing plant. |
| Do connections communicate what is connected? | Yes, and this is the strongest result. Domain decides treatment, rate decides bore, and the domain's height decides where it lives — so a drive train, a steam main and a cable tray are three visibly different things. |
| Does changing a component regenerate coherently? | Yes, and locally: geometry near the change moves and the rest is identical piece for piece. |
| Is generation deterministic? | Yes. One 64-bit hash per scene, checked on all sixteen designs on every run of `forms`, and again through a file round trip. |
| Does it work from a small authored set? | Yes. 25 meshes, 8 materials, 2,428 triangles of library for 10,984 pieces of plant. |
| Does complexity come from arrangement? | Yes: 1,290 pieces from 22 distinct meshes in the largest design. |
| Is the renderer downstream of the simulation? | Yes, by module graph, and tested by rebuilding every design twelve ways and re-judging it. |
| Did the machine designer become 3D? | **No**, and deliberately. Placement stayed 2D and elevation became *derived*, because the core rule forbids the picture from feeding back into the machine. Free 3D placement would be a different experiment with a different risk. |
| Is the routing good? | It is adequate and it is honest. A* with a heading gives clean orthogonal runs and refuses to pass through equipment, but two lines that could share a rack do not know about each other, and a plant with adjacent machines produces more bends than a person would draw. |
| Is the enclosure any good? | No, and it was not meant to be. It is bounds, clearance, floor, walls and roof with holes cut by the plant. The test was whether machinery can produce its own surrounding structure, which is a much lower bar and a much more interesting one. |
| Does it scale? | Not asked, again. A hundred plants at once is the next question, and instance buffers are the reason to think it is answerable. |

The thing being tested was whether engineering can be art direction. What the
sixteen designs say is that it can, and that the mechanism is smaller than
expected: **five height conventions, seven pipe treatments, a bore taken from a
rate, and a rule that anything above head height needs a stair.** Almost none of
the plausibility comes from the meshes.

## Experiment 09: the readability pass

Experiment 08 answered its question and produced a grey box. The follow-up note
was blunt about it:

> base meshes placed correctly, simple pipes and supports, very limited material
> variety, almost no secondary detail, weak distinction between "primary
> equipment", "secondary support" and "cosmetic dressing".

and equally blunt about the fix, which was explicitly *not* more geometry:

> **No geometry changes. Just improve the material/paint assignment rules.**

So experiment 09 adds no new pass, no new simulation feature and no new
component. It adds one axis — how much of the visual language to apply — and
then builds the same plant along it four times and compares the pictures.

```text
A  grey     experiment 08 exactly
B  paint    the same geometry, piece for piece, in the material language
C  detail   + how things are joined and installed
D  full     + archetype articulation
```

### The comparison is only worth something if it is the same machine

The whole apparatus rests on one property: nothing but the *look* may differ
between the four. That is enforced where it cannot be forgotten — in a
signature:

```rust
pub fn apply(
    plan: &Plan, routes: &[Run], owners: &[Owner], grade: Grade,
    pieces: &mut [Piece],
)
```

`paint::apply` runs last, may read everything, and may write exactly one field
of a piece. It could not move a machine if it wanted to. `tests/read.rs` checks
it from the outside anyway, on every design in the repository:

```text
the material pass moved no geometry     mesh, position, size, direction, spin,
                                        level and owner, piece for piece
...and it did repaint it                163 of 741 pieces on the refinery
every grade is the same machine         same components, same routes, same
                                        bores, same bends
grade A is experiment 08 exactly        eight scene hashes out of the README
                                        table above, still exact
```

That last one is the one worth having. Four meshes, four materials, a repaint, a
connection vocabulary and a set of articulated archetypes later, `--grade a`
still produces `331db5cf` for `03-compact` — so the baseline in the comparison
is the real baseline and not a reconstruction of one.

### 1. The material language

Eight materials was four structural ones, a seeded paint and three others, which
left almost nothing to say what a thing is *for* — so a tank, a pump casing and a
wall panel all came out the same colour and the plant flattened. Twelve says:

```text
pressure vessels, tanks, columns     off-white painted steel
heat equipment, heat mains           lagging
rotating and process machinery       the works colour, from the seed
structural steel                     dark, unloved
walkways, ladders, cladding          galvanised
foundations                          concrete
stair treads, guards, kerbs          hazard yellow
cold service                         blue-grey
fuel and process service             dark green
steam                                bright steel, lagged at the joints
drives                               bright steel
electrical                           galvanised conduit, copper at the ends
```

The top eight rows are decided by what a component is *for* rather than by what
it looks like, which is the same trick experiment 08 played with bores and
heights: the palette is another consequence of the machine.

The one that goes furthest is service. A fluid line is not just a fluid line, so
`paint::service` walks upstream through the document — hop by hop, staying in one
domain — until it finds the source and asks what that source was tuned to draw:

```text
pump P1 draws crude  ->  PH1  ->  PH2  ->  CO1     every one of those lines is oil
pump P2 draws water  ->  CD1                        that one is water
```

Nobody wrote "the refinery is green". A distillation train comes out green and a
boiler house comes out blue because of one `draws` keyword in a text file, read
by a pass that has never simulated a tick.

There is a specific thing to report about hazard yellow, because it is the one
rule that had to be walked back. Handrails were yellow first, and on a compact
plant it looked superb; on the refinery, which has five platforms and a column,
the entire installation turned into yellow scaffolding. An accent that appears on
every edge is not an accent, it is a colour scheme. The yellow now goes on stair
treads instead — one flight per machine rather than one rail per edge — and on
guards, kerbs and chute bands.

### 2. The connection vocabulary

Experiment 08's pipework was *connected*. Experiment 09's is *made*:

```text
a bolted joint at every equipment interface -- a pair of flanges, not one
an isolation valve where a line leaves a machine
a reducer where the two ends of a run are not the same size
a clamp wherever a run crosses one of its own supports
a lagging collar either side of every elbow on a hot line
a tee where two lines leave one socket
a pressure gauge on a third of the process lines
```

Every one of those is placed on the path the router already found, at a point
that path already passes through. The clamp is the nicest of them, because it is
where two passes agree without talking: `route` puts a clamp at each of a run's
`props` and `frame` puts a support under the same list, so the pipe is fixed to
the steel by construction rather than by luck.

### 3. Installed, rather than placed

The note's third section was about machines that look dropped rather than
mounted, and it is right that this is a surprisingly big deal. A plinth gets a
pad and four holding-down bolts, a column gets a base plate on top of its pad, a
horizontal vessel gets a proper cradle instead of a block, a pipe support gets a
pad and, if it is tall, a brace.

Two of these were more interesting than expected.

**A rack is a system, not a row of posts.** Wherever two different runs want
holding up within the same two-metre bay, they get one trestle between them —
two columns, a cross beam, and a second tier if anything is running well above
the first. It falls out of clustering the `props` that already existed, and it
turns a fence of individual posts into something that looks designed.

**The paint found a bug that four months of grey-boxing hid.** A flight of stairs
from a twelve-metre platform is six metres long, the apron round a plant is two
and a bit, and experiment 08 picked which side the flight came down with a coin.
About half of them landed in the yard. Nobody noticed for as long as a stair was
drawn in the same grey as everything else, and it was impossible to miss the
moment one was painted like a stair. From grade C the seed proposes and the plot
disposes: the side with the most room wins, and the seed only breaks the tie.

### 4. Archetypes, articulated

The note suggested buying better archetype meshes from an asset generator. What
went in instead is eight to twenty lines per archetype, because the thing missing
from a pump was never a better pump mesh:

```text
Can       a bedplate and bolts, a shaft stub out of the driven end and a
          cooling cowl out of the other -- which end is which comes from
          where the rotary socket is
Shell     a tube-sheet flange at each dished end, a manway on the crown, and
          a saddle rather than a block under each end
Turbine   lagging bands on the steam half, a casing joint flange, an exhaust
          hood flaring down at whatever is condensing it, a governor pedestal
Bank      headers down both long sides and a fan cowl per bay: a cooling unit
          is a box that moves air, and until there is a fan in the top of it,
          it is a box
Vessel    holding-down bolts, a manway, skirt vents, a nozzle cluster on the
          head, and a davit to lift the head off with
Skid      an access panel, a guard over the drive end, a stool for whatever
          drives it
```

Four new meshes carry most of it: `reducer`, `clamp`, `cowl` and `saddle`.
Twenty-nine meshes and twelve materials, 3,248 triangles of library for the whole
kit.

### What the four grades cost, and what they buy

```powershell
.\run.ps1 -Machine read designs/10-refinery.machine --png sheet.png
```

```text
   grade     pieces  calls  mats  tones  chroma  legible  what changed
A  grey         741     31     6     35      7%      6/8  baseline: experiment 08 as it shipped
B  paint        741     33    10     32      6%      6/8  material pass: the same geometry, repainted
C  detail       905     38    10     31      5%      6/8  + connection and installation vocabulary
D  full         941     39    10     32      5%      6/8  + archetype articulation

A to B: 741 pieces, 163 of them repainted, none moved.
A to D: 200 pieces added, and the routes are identical -- 12 runs, 149 m, 50 bends.
```

`--png` writes the four panels on one sheet, captioned, from one camera framed on
one volume — because a camera that fits itself to each build in turn zooms out
every time a piece is added, which reads as the plant getting smaller.

`tones` and `chroma` are measured off the rendered pixels rather than off the
intent, over the machine only: the sky and the concrete apron are identical in
all four grades and between them are most of the frame. `legible` is the count of
equipment kinds drawn from a mesh-and-material signature no other kind shares —
the same test experiment 08 applied to the seven domains, turned on the equipment.

### The honest result

**The pixel metrics barely moved, and the plant looks enormously better.**

Chroma goes 7% to 5%. Tones go 35 to 32. Across all sixteen designs the average
gain in tones between A and D is *two*. If the question had been asked as "does
the readability pass make the plant more colourful", the answer measured off the
pixels would be no.

That is worth reporting rather than tuning away, because it says something about
what was actually wrong with the grey box. A works **is** grey — the honest
palette for a plant is concrete, galvanised steel, lagging and dark structural
steel, and turning up the saturation would have produced a toy. What the pass
bought was not colour, it was **hierarchy**: the same greys, distributed by what
a thing is for instead of by which archetype function happened to write it. The
numbers that do move are the ones about distinction rather than intensity —
materials in use go from six or seven to nine or ten on every design, and
`legible` goes from 11/13 to 13/13 on the largest one.

The other honest result is where the gain sits. The note's bet was that **D**
would look dramatically better than **A**, and it does — but B, the pass that
adds no geometry at all, does over half of the work, and C does most of the rest.
D is the smallest step of the three and the most code. If this had to be done
again with a fixed budget, it would be spent in exactly the order the note
proposed.

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Does better material assignment beat more geometry? | Yes, decisively, and it is not close. B is a pure repaint — 163 pieces of 741 changed material and nothing else — and it is the single largest visible improvement in the experiment. |
| At which point does it stop looking like a grey-box prototype? | At **C**. B makes it legible; C is where it stops looking like a diagram, because that is where the connections start looking engineered rather than merely connected. |
| Can the palette be derived rather than authored? | Yes. Every material comes from what a component is for, and the one colour that needed to know something else — water or oil — comes from walking the document upstream to a `draws` keyword. |
| Did it need AI-generated assets? | No, and it is worth being clear about why. What a pump was missing was not fidelity, it was a bedplate, a shaft out of the right end and a cowl out of the other. That is twenty lines, it is deterministic, and it stays in the same twenty-nine-mesh kit. |
| Did the readability pass stay downstream? | Yes. Four grades × sixteen designs, judged before and after, and the verdicts never move. |
| Is the pixel metric any good? | Only as a floor. It catches a plant that has gone monochrome and it agrees that the coverage of the four panels is the same, but it cannot see hierarchy, which is the thing the experiment was actually about. `legible` is the better number and it is still crude. |
| Is the material language finished? | No. There is no dirt, no edge wear, no heat staining, no insulation texture and no warning markings, because there are no textures at all — twelve flat materials with a roughness each. The note listed AI-assisted texturing as the second-best use of AI here, and that is the next thing this pipeline could actually use. |
| Did anything get worse? | The draw call count went up by two to eight per plant, because more distinct mesh-and-material pairs is exactly what a material language *is*. Thirty-nine calls for a refinery is still thirty-nine calls. |

The thing being tested was whether a grey-box procedural output can be turned
into a deliberate visual style without touching the generator. It can, and the
mechanism is smaller than expected again: **twelve materials assigned by
function, six rules about how things are joined, four rules about how things are
mounted, and one flight of stairs that goes where there is room for it.**

## Experiment 10: 3D authoring

Experiment 08 asked whether a functional design could be turned into a
recognisable industrial scene, and the answer was yes. Experiment 09 asked how
far a look gets without touching the generator, and the answer was *most of the
way*. So the interesting question stopped being about pictures:

> **Can a player manipulate this industrial scene directly, and does the
> procedural system reliably turn their functional 3D decisions into geometry
> that makes physical visual sense?**

The note that asked for it put the deliverable in eight lines:

```text
Component {
    type
    position: xyz
    rotation
}

Connection {
    fromPort
    toPort
}
```

which is a very small change to the document and a very large one to everything
downstream of it.

### 1. The document grew an axis and a rotation

```text
reactor   R1  at 0,0    throttle 40
exchanger HX1 at 4,0    face east
exchanger HX2 at 4,0,3  face east     # six metres above HX1, not beside it
```

`at x,y` still means what it always did; `at x,y,z` adds the third tile. The
grid is cubic — one tile is two metres east, south *or up* — so the whole of the
document's geometry stays integer and stays comparable.

Three rules fall straight out of it:

```text
two components may share tiles         if they do not share a height
reach is measured in three dimensions  stacking is a way of being close
a component above the slab needs a deck  which the structural pass builds
```

`face` is the other half, and it is the half that makes the first half worth
having. An authored rotation turns the footprint, turns the machine, and turns
every nozzle on it. An *inferred* one — which is what every component starts
with, and what experiment 08 always used — turns the machine and leaves the
footprint alone, because a footprint that reshaped itself when somebody drew a
wire on the far side would be a document editing itself.

Neither reaches the simulator. `tests/space.rs` turns every component in a plant
a quarter turn, lifts a generator two storeys, and asserts the scoreboard does
not move by so much as a tick.

### 2. A port became an interface

This is the change the pipework noticed, and the note said it best:

> The procedural generator should understand interfaces, not merely endpoints.

Experiment 08 chose a port's face by looking at what it was wired to and picking
the nearest side. That reads correctly at a glance and is nonsense on
inspection: a steam outlet would appear on whichever wall happened to face the
turbine, including the floor, and a shaft would leave a motor out of its side.

Now every archetype declares, per domain and direction, which of its six faces
that port is allowed to be on:

```text
a can's shaft leaves the end of the barrel, and only the end
a vessel vents upwards, and only upwards
a shell's process ports are on the tube ends, or on the crown of the shell
a bin takes material in at the top and drops it out of the bottom
a turbine takes steam in at the top and exhausts it downwards
```

The old nearest-side rule still runs, but only *within* the allowed set — and
never onto a face with less than half a tile of air in front of it, because a
nozzle nothing can be bolted to is a lost connection dressed up as a legal one.
Where the archetype leaves no choice, the remedy is to turn the machine, which
is what `face` is for.

Each socket then carries what the router is obliged to respect:

```text
out       the flange normal: the direction a line leaves in
bore      from the port's rate, as it always was
class     light / standard / heavy, derived from domain and rate
stub      the straight run off the flange before anything may bend
layer     which of the plant's five storeys a line off it belongs on
axis      for a shaft, the line the coupling has to lie on
```

`machine space` prints the table:

```text
  port            domain     face    class       bore  stub  at
  ------------------------------------------------------------------------
  R1.heat         heat       up      heavy        520  1000  (4250, 9250, 4250)
  HX1.heat        heat       up      standard     285   500  (11250, 4250, 3250)
  HX1.steam       gas        east    standard     210   500  (13750, 3250, 3250)
  T1.rotary       rotary     east    heavy        220   500  (21750, 1250, 2250)
  G1.rotary       rotary     west    heavy        195   500  (22250, 1250, 2250)
```

### 3. The router walks straight sections, and is allowed to refuse

Experiment 08's router was a shortest-path solver with a bend penalty bolted on.
Shortest paths on a grid are staircases, and a staircase drawn in 858mm lagged
pipe is a picture of something that could not be built. So the search space
changed shape:

```text
a node is (corner, heading)
an edge is a straight run of at least `straight` millimetres
```

Six of the note's nine rules are now true *by construction* rather than by
penalty — there is no path in the search space that bends twice in a metre, so
no amount of bad luck can produce one:

| rule | how |
|---|---|
| socket direction | the first and last sections are the flange normal, and nothing else is offered |
| minimum straight before bend | the gates: the first bend is `stub` from the flange, and so is the last |
| allowed bend radius | `straight` is at least twice the radius plus a diameter, so every corner can afford its own elbows |
| pipe diameter | the bore, from the port's rate |
| clearance from equipment | a cost inside it, forbidden through it |
| clearance between pipes | a laid route claims its cells and charges for the ones beside them |
| preferred elevations | five named storeys, one per domain |
| support spacing | per domain, and the structural pass reads the same list |
| junction rules | two lines off one socket share a corridor and get a tee |

The elevations are the cheapest good idea in the note. Industrial routing has
conventions; exploiting them makes the output believable *and* constrains the
search:

```text
Ground   700 mm   pumped services, along the floor
Drive   1250 mm   shafts and rods: every one in the plant at one height
Feed    2750 mm   chutes and conveyors, above head height and falling
Rack    4250 mm   the process rack: steam, heat, anything hot
Tray    5600 mm   cable tray, over the top of everything else
```

And then the part that matters most. Experiment 08 could not fail: when A* found
nothing it drew a straight line through the plant and moved on, which made
"every wire has a route" true and meaningless. A run is now laid at the first of
three tiers that works:

```text
clean   every rule above, in full
tight   half the straight off the flange, the stylistic half of the straight
        between bends, and permission to share a corridor
lost    no valid route found -- and nothing at all is drawn
```

The geometric half of the minimum straight — twice the bend radius plus a
diameter — is never relaxed, because giving it up does not produce a shorter run,
it produces two elbows drawn through each other.

That immediately earned its keep. `07-crushline` has two motors bolted to the
same end of the same shaft. Only one of them can be: a coupling is a straight
line between two flanges, one motor is already on that line, and the other is
three and a half metres to the south. Experiment 08 drew it anyway — a shaft with
two right angles in it, arriving at a flange sideways — and nobody noticed for
two experiments. Experiment 10 declines:

```text
  connection                domain     tier    metres  bends  elevation
  ------------------------------------------------------------------------
  MO1.rotary -> SH1.in      rotary     clean        2      0  drive
  MO2.rotary -> SH1.in      rotary     lost         0      0  drive

  rule              level   what
  ------------------------------------------------------------------------
  no route          red     no valid route found for MO2.rotary -> SH1.in
```

Across the seventeen designs, 204 connections of 209 are laid under the full
set, four are squeezed and one is refused. All five exceptions are drive shafts,
which is the domain with by far the strictest rules — four metres of straight
between bends, because a shaft that bends is a gearbox nobody placed.

### 4. Space is scarce, and the plant says who is in whose way

The note's warning was exact:

> Otherwise 3D placement just becomes the same easy `x100000` problem with
> objects stacked vertically until the entire factory resembles a lasagne.

Free placement is only interesting if space is scarce, and space is only scarce
if something *needs* it. So every component claims four volumes rather than one:

```text
solid       the machine itself: nothing else may be here at all
service     the room a person needs to work on it -- on any one of its sides
hot         the separation a hot machine needs from a cold one
exclusion   the straight run off each flange, which belongs to the pipework
```

and `space::check` reads the finished plant and reports six rules:

| rule | red or yellow |
|---|---|
| equipment cannot overlap | red — two solids intersect |
| big vessels need foundations | red — a tower on a mezzanine is not a design |
| shafts need alignment | red — a drive's two ends are not on one axis |
| pipes need routes | red for refused, yellow for squeezed |
| some parts need maintenance access | yellow — boxed in on all four sides |
| hot objects need separation | yellow — a cold machine inside a hot one's halo |

The first thing it found was that this repository has never had an aligned drive
train. Twenty-two of its drives arrive at their far flange between two and five
metres off the axis they left on — one design in three. Every one of them was
drawn by experiment 08 as a shaft with two right angles in it, and every one of
them is now red.

It reports; it does not refuse. The *document* refuses the things that are not a
design at all — two components on the same tiles at the same height, a wire
between ports that cannot carry each other — and everything here is downstream
of that, because the difference between an illegal machine and a badly laid-out
one is the whole of what makes the second one interesting to play with.

The service rule is worth a note, because the first version of it was wrong in
an instructive way. It required the *front* to be clear, and it fired on half of
every design in the repository — a rule that fires on everything is not a rule,
it is a background colour. What a machine actually needs is to be approachable
from *somewhere*, so the four sides are tried in order and only a machine boxed
in on all of them is a fault. That took the count from 92 warnings to six.

### 5. Building upwards has consequences nobody drew

```text
a component above the slab       -> a deck, its columns, its edge and its rail
a deck with no way onto it       -> a flight of stairs from the storey below
a floor with a machine through it -> a hole in the floor
a serviceable port above head height -> a platform, measured from its own deck
a run longer than its span       -> a support, and a clamp where it lands
```

`designs/17-stacked.machine` is the whole experiment in one file: `03-compact`,
with the second exchanger train put on a mezzanine over the first.

```text
                    03-compact   17-stacked
electricity            108.00       108.00   /tick
fuel                    40.00        40.00   /tick
water                  160.00       160.00   /tick
footprint                  72           60   tiles
plot covered              81%          97%
connections           8 clean      8 clean
```

Same power, same fuel, on five sixths of the ground — and the cost of the trade
is visible from the outside, because the deck needs columns down to the slab, an
edge, a handrail and a flight of stairs, all of which appear without being asked
for and all of which take up room the spatial rules then measure.

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Can the player author in three dimensions? | Yes, and the document barely changed: one integer and one optional quarter-turn per component. Everything hard was downstream. |
| Did the third axis need to be authored at all? | Yes, and this was the surprise. Experiment 08's *inferred* elevation is better than it sounds and is still the default — but inference cannot express "put the second train over the first", and that is the decision the note wanted the player to own. |
| Is the routing trustworthy now? | Much more so. The staircases are gone by construction rather than by tuning, and the remaining defects are ones the plant reports about itself. |
| Was letting it fail the right call? | Unambiguously. It found a defect in a shipped design that two experiments of looking at screenshots had missed, and a hole in a plant is a thing a player can fix. |
| Did spatial constraints make placement interesting? | Enough to tell. Stacking buys footprint and costs steel, service access is a real constraint on packing, and shaft alignment is the first rule in this repository that a player has to arrange geometry to satisfy. Whether it is *fun* is a question for a play session, not a test suite. |
| Did the derived structure keep up? | Mostly. Decks, columns, rails, stairs and holes in floors all fall out. What does not yet is a deck's own effect on routing: the structural pass runs after the router, so a pipe can pass through a floor that did not exist when it was laid. |
| What about the asset spike? | Not done, deliberately. The note called it a visual spike rather than the next phase, and priorities 1 to 4 filled the experiment. The kit is still twenty-nine meshes. |
| Did anything get worse? | Runs are longer, and visibly so. Enforcing a minimum straight on the section that arrives at a flange means a line that used to jog twice now goes round the outside, and a heat main that used to cut the corner now climbs to the rack first. That is the price of every corner being an elbow that fits, and it is worth paying. |

The thing being tested was whether a player's three-dimensional decisions can be
turned into geometry that makes physical sense. They can, and the mechanism is
smaller than expected for the third time: **six faces per archetype, a straight
section as the unit of search, three tiers, and permission to say no.**

## Prototype 2: the multiplayer vertical slice

Everything above this line proved something and then put it back in the box. A
solver that compresses a billion machines, a designer that turns a building
into an exact periodic orbit, a renderer that turns that design into a plant,
and an editor that lets a player move it around in three dimensions — four
laboratories, no game. So the eleventh experiment stops proving things
separately:

> **Can two players continuously build and redesign a deterministic factory
> together, in real time, while the simulation keeps running and both clients
> remain exactly synchronized?**

Yes. And the price is one sentence long, the same shape as Prototype 1's: **a
command is an intention, not a diff.**

```text
   intention  →  host validates, stamps (tick, sequence)  →  broadcast
                                                                 │
        ┌────────────────────────────────────────────────────────┘
        ▼
  GAME DOCUMENT      physical installations, positions, owned designs
        │ compile
        ▼
  SIMULATION IR      population classes, storages, channels, regions
        │ live::with_states
        ▼
  state(tick)        the only thing anybody is allowed to draw
```

Nothing below the second arrow learned that the game is multiplayer. `dsl`,
`pop`, `rooms`, `domains` and `live` are untouched; `mp` is 4,800 lines of
document, compiler, protocol and arbitration sitting on top of them.

```powershell
.\run.ps1 -Room              # the game, at http://127.0.0.1:8790
.\run.ps1 -Room test         # section 25's scenario, played headlessly
.\run.ps1 -Room fail         # section 26: eleven ways to be ambiguous
.\run.ps1 -Room check        # the front end, without a browser
```

### 1. A command is an intention

Prototype 1's log held `live::Edit`s, which are *document diffs*: replace this
node, remove that wire. That works for one player and cannot work for two,
because two clients produce diffs against documents that have already diverged.
So a command here says what somebody *meant* —

```text
PlaceMachine   { proto: "stamping", x: 40, y: 6, face: 0 }
CreateConnection { from: 12, to: 8, item: "IronBillet" }
CommitMachineDesign { id: 8, design: … }
```

— and the diff is *derived*, deterministically, by recompiling the document.
The host validates, stamps `(tick, sequence)`, applies, and broadcasts; every
replica applies the same command in the same order and gets the same answer, or
the experiment has failed. Ordering is `tick` then `sequence` and nothing else:
not arrival order, not frame timing, not who has the better connection.

A refused command **never enters the log**, so replaying the log cannot
reproduce it — and every refusal in `mp::cmd` is structural (the id is not
there, the item is not made, somebody else holds the lock), which is what makes
it safe to broadcast a rejection as confidently as an acceptance.

### 2. The clock does not stop

```text
SIM_TICK_RATE = 60          one second, everywhere
```

Gameplay is authored in seconds and compiled into ticks. A mine produces `100
IronOre / second`, never `1.67 / tick`; a belt's latency is `base + distance /
speed` in whole ticks. The one pause the game has is before the host presses
start, and there is no matching stop — the goal is on screen, then the clock
runs, and every player builds inside a system that is already going.

That has a consequence the brief was explicit about and the implementation had
to answer: **"the factory does not compile" cannot be an outcome.** The
language below is entitled to refuse a plant whose press has nowhere to put its
gears, and a player who has just placed a press is in exactly that state for as
long as it takes them to place a bay. So the compiler asks a narrower question
than the language does:

```text
is this installation commissioned?
  every input item arrives at exactly one bay wired into it, and
  every output item leaves to exactly one bay wired out of it
```

Anything that fails is left out of the IR and told why, in a sentence its
inspector shows — *"nothing delivers coal to a bay wired into it"*. It is still
in the document, still drawn, still deletable. The check is a fixpoint, because
dropping one machine can starve the bay that fed the next one.

### 3. A machine's recipe is its orbit

This is the join between the two halves of the project, and it is the shortest
file in the experiment.

Experiment 06 refused to let a finished machine collapse into `input ×
efficiency = output`, and compiled it to a startup transient plus an exact
periodic orbit instead. That refusal is what makes the lowering trivial: a
machine that repeats itself every `period` ticks, having taken this and given
that, **is** a recipe.

```text
orbit:  took { Coal 495, IronBillet 1980, Power 4840 }
        gave { Iron(gear) 1980 }                        over 40 ticks
world:  process { consumes 99 Coal, 396 IronBillet, 968 Power
                  takes 8 s  produces 396 Gear }
```

Nobody typed "a stamping line makes 49.5 gears a second". It makes 396 gears
every eight seconds because that is what its press, its furnace and its two
motors do, and adding a third motor changes the number — which is the whole
reason a placed machine is worth opening.

Two conversions happen on the way across, and both are gameplay decisions:

**A designer tick is a game second.** Experiment 06's clock was its own, and
its numbers were chosen to read well against it: 108 MW a tick, 20 gears a
tick. Read at sixty ticks a second those become 6,480 MW and 1,200 gears a
second, which is section 20's warning about mistaking resolution for pace
arriving exactly on schedule.

**The orbit is reduced to its primitive cycle.** An orbit is a fact about the
machine's *internal* state coming round again, and it can be sixty ticks long
while every flow across the boundary repeats every ten:

```text
period 60, flows { 6000, 5610, 18672, 2244 }   gcd 6
cycle  10, flows { 1000,  935,  3112,  374 }
```

Identical rate, exact arithmetic, and the finest granularity the orbit
justifies. Without it a steam crusher swallows eighteen thousand water in one
gulp and needs a bay the size of a lake to run at all.

The catalogue is six machines and no recipes:

```text
steamplant       1.0s  40 Coal + 160 Water                     ->  108 Power
turbinehall      1.0s  50 Coal + 200 Water                     ->  132 Power
crusher         10.0s  1000 Coal + 935 IronOre + 3112 Water    ->  374 Concentrate
stamping         8.0s  99 Coal + 396 IronBillet + 968 Power    ->  396 Gear
machining        9.0s  360 IronBillet + 1160 Power             ->  216 Gear
refinery         1.0s  42 Coal + 120 Crude + 18 Water          ->  36 LightFraction
                                                                 + 48 MiddleFraction
```

Only the crusher powers itself — it burns coal to raise its own steam, which is
the design it was argued into over three experiments. Everything that makes
gears runs off the grid, so a gear line is always a gear line *and* a power
station, and a power station is always a coal problem. That is not a balance
decision; it is what the designs happen to do, discovered by lowering them.

The lowering also produced the experiment's most embarrassing bug. `Totals::
power` is a *mirror* of the Power entries in `gave`, kept separately because
the first brief is written in megawatts — and the first version of the lowering
added it on top, doubling every generator in the game. The test that caught it
does not check that 216 MW is a plausible number; it checks that the lowered
rate is the orbit's rate, in integers, for every machine in the catalogue.

### 4. The machine keeps running while you redesign it

Opening a placed machine does not stop it. The player edits a **draft**, which
lives in the document beside the live design and is replicated like everything
else — so the other player can watch the redesign happen without being able to
touch it:

```text
LIVE    Machining Cell, 216 Gear every 9 s, population intact, still queueing
DRAFT   + one motor, wired to the cable and the line shaft
```

Component-level commands (`PlaceComponent`, `ConnectComponent`,
`TuneComponent`, …) apply to the draft and are marked non-structural, so they
never reach a rendezvous and cannot cost one. Then:

```text
CommitMachineDesign(machineId, design)
```

is one command at one canonical tick. The old design ends, the new one begins,
external bays keep their contents, and the machine's population starts from a
deterministic cold state — the conservative rule section 14 asked for, chosen
because a transient poured into a population is a state nobody can name.

A commit that would make the machine *bigger* than the space it stands in is
refused, because place-and-delete means nothing ever silently moves.

Two players cannot edit one draft. The lock is one field in the document,
visible to everybody, and it is deliberately the cheapest thing that works —
collaborative editing of one 3D design is a research project, and this is a
lock.

### 4a. Why is it stopped? Asked at two altitudes

The live nature of the simulation means failure has to be understandable in
seconds, and there are two different questions hiding in "why is nothing coming
out of that".

Outside, the world inspector reads the solver's own diagnosis — `why::diagnose`,
written for Prototype 1 and unchanged:

```text
Press8   STARVED
needs 396 IronBillet, 12 in Bay4
next delivery in 3.2s (Belt14)
utilisation 41%   busy 0 / idle 1 / blocked 0
```

Inside, the machine answers about itself, at the phase its own orbit is in when
read against the room's clock — a designer tick is a game second, so a settled
machine at second 4,000 is indistinguishable from itself at `transient + 4000 %
period`:

```text
holding it back
  MI1 (mill)      STARVED   67 rotary of the 80 it wants
  C2  (crusher)   BLOCKED   its out port is full
```

The two altitudes deliberately do not talk to each other. The inner view is the
machine on its design's own supply; the outer one is the authority on whether
the machine turned at all. Coupling them — running the component simulation
against the world's actual deliveries rather than against the orbit — is the
obvious next thing and was not needed to answer this experiment's question.

### 5. Place, delete, and the ghost in between

Committed objects do not move. Wanting one elsewhere is a delete and a place,
at two different ticks, and everything before the commit — the ghost under the
pointer, the rotation, the red outline over a collision — is a picture in one
browser that no other machine ever hears about. The reason is on the wire
rather than on the screen: a drag is a stream of positions with no canonical
order; a place is one command with one tick and one sequence number.

A deleted thing leaves a translucent ghost for eight seconds with a **Restore**
on it, and restore issues a *new placement command at the tick it is pressed*.
The seconds the thing was missing really happened, and the factory really did
run without it:

```text
tick 9,600   DeleteStorage(Yard13)      the press loses its power buffer
tick 9,960   Restore → PlaceStorage     a new yard, empty, at the same tiles
```

### 6. Joining a room that is already running

```text
join:   snapshot @ tick X   (world document, Carry, accounts, goal)
then:   every command with seq > X's
never:  the host's simulation state
```

The `Carry` is Prototype 1's, unchanged — the note that introduced it said it
was "also, not by coincidence, exactly the canonical snapshot the networking
proof needs", and that turned out to be true a year of experiments later. The
snapshot goes through JSON on the way even for a replica sitting in the same
process, because a snapshot that is really a `clone()` proves nothing about a
snapshot that is really a socket.

Joining does not pause, stop or rewind the host. `joining_does_not_disturb_the_
host` checks that, by comparing the host's clock, probe and books either side
of an arrival.

### 7. Three reconstructions, compared every second

A client of this game is a browser, and a browser cannot run the solver. So a
"client" in `mp::room` is a `Sim` in the host process fed **nothing but the
broadcast command stream**: its own world document, its own compiled plant, its
own carry, its own books. It shares no memory with the host's simulation and is
never copied from it after the join.

Every simulated second, each of them hashes:

```text
hash = FNV-1a( tick ‖ world.signature() ‖ carry.signature() ‖ accounts.signature() )
```

and the host compares. A mismatch resends a snapshot and replays the tail.

```text
probe @ 60s     host 907d6f32250c7005   Ada 907d6f32250c7005   Bee 907d6f32250c7005
probe @ 150s    host 0a224030bfbbcf71   Ada 0a224030bfbbcf71   Bee 0a224030bfbbcf71
probe @ 1070s   host d6603f2757ba9d72   Ada d6603f2757ba9d72   Bee d6603f2757ba9d72
```

**The hash caught a real desynchronisation, and it is worth writing down.** The
room closes its books at every lattice point, and the first version closed them
all at the *end* of whatever interval a client happened to ask about. Ada
polling every twenty seconds and Bee every forty then computed different hashes
for the same second — the same room, hashed with a different amount of the
future already in it. Nothing about that is visible in a test where everybody
polls together, and nothing about it is visible in the code. It showed up as
three mismatches in `room test`, which is exactly what an acceptance command is
for.

### 8. Goals: twenty-one problems, not a generator

Twenty-one templates are written by hand, each one a factory problem somebody
thought was interesting; the seed chooses among them and picks numbers inside
ranges the template declares, and nothing else is random.

```text
delivery      first-gears          Deliver 9,886 gears.
throughput    steady-gears         Hold 21 gears a second for 53 seconds.
power         big-grid             Hold 496 power a second for 70 seconds.
efficiency    frugal-concentrate   Deliver 6,519 concentrate having drawn no more
                                   than 42,772 coal.
space         compact-gears        Hold 18 gears a second for 32 seconds, with the
                                   whole factory inside 1,372 tiles.
mixed         power-and-product    Hold 148 power and 18 gears a second, both at
                                   once, for 44 seconds.
```

The seed also chooses the room code and the starting plot, so a room worth
playing again is one number long.

Progress is measured **only on a lattice** of one sample a second. A rate is a
question about a window, and a window needs two measurements; if each replica
measured whenever it happened to be asked, two clients polling at different
rates would disagree about a rate, then about a completion, then about
everything. So a replica asked about tick 12,345 answers with the *state* at
12,345 and the *accounts* at 12,300, and says so.

One honest substitution: the brief suggests "waste less than X% heat", and a
percentage needs a denominator the machine model does not define. The
efficiency family asks two questions it can actually answer instead — a cap on
an input, and, for power plants, `wasted / (wasted + power)` summed over the
machines and weighted by cycles, which is a percentage with a meaning that
moves when a reactor is turned down.

When the goal is met the room records the tick, the totals, the machine count
and the footprint — and **keeps running**. There is no stop.

### 9. The failure tests

Section 26 lists nine ways for two players to be ambiguous. `room fail` runs
eleven:

```text
ok   two commands at the same tick            both at tick 300, ordered 1 then 2
ok   two players placing on the same tiles    that overlaps Bay8
ok   place and delete in the same instant     it existed for zero ticks and left a ghost
ok   a bay wired to itself                    two bays cannot be wired together
ok   two players in one draft                 player 1 is already editing Press11
ok   deleting a machine somebody is editing   player 1 is editing Press11
ok   joining while a machine is being edited  the joiner is handed the draft and the lock
ok   committing while the world changes       one command at tick 720
ok   a command that arrives late              stamped at tick 1200 -- the host's clock
ok   leaving and coming back                  rebuilt from a snapshot, agrees at 34s
ok   a client whose hash does not match       1 mismatch, 1 snapshot resent, agreeing again
```

The last one has to reach in and corrupt a replica, because there is no other
way to see the correction path without waiting for a bug.

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Can two players build one factory while it runs? | Yes. `room test` plays section 25's scenario end to end — host, goal, late join at tick 720, world logistics at one scale while a machine is redesigned at another, a commit, a delete, a restore — and the three reconstructions agree at every probe. |
| Did the solver have to change? | No. Not one line of `dsl`, `pop`, `rooms`, `domains`, `sim` or `live`. The multiplayer layer is a document, a compiler and a protocol on top of an interface that already had the right shape. |
| Was `Carry` really the snapshot? | Yes, unchanged, including through JSON. The one thing that had to be added was the *books* — a joining replica that started differencing its delivery counters from zero would count the room's whole history twice. |
| Is an intention really better than a diff? | Yes, and it is not close. A diff needs a common ancestor; an intention needs a validator. It also made the entire refusal vocabulary fall out for free, because "why can't I do that" and "why won't this compile" turn out to be the same question asked at two altitudes. |
| Does the machine designer survive contact with a live factory? | Yes, and the draft is why. The live machine keeps its population and its place in every queue while somebody rebuilds it in another window, and the redesign lands as one command at one tick. |
| Does a machine's orbit make a good recipe? | Better than expected. It gives every machine a *cycle* as well as a rate, so a design with a long orbit runs in enormous batches and needs a bay that can hold one — a real consequence, discovered rather than designed, and the first time experiment 06's "keep the orbit, don't average it" refusal has cost the player anything. |
| Is it a game yet? | It is a vertical slice. Two people can host, join, build, redesign, argue about a footprint and finish a goal; the parts that are missing are content and polish rather than architecture. Whether it is *fun* is a question for a play session, not a test suite. |
| What did the hashes actually buy? | One real desynchronisation, in the accounting rather than the simulation, invisible to every test where both clients poll at the same rate. That is the entire argument for canonical hashes stated as an incident. |
| Is a machine's design really its own? | Yes, and it is the one place the brief's rule needed a UI rather than an argument: **duplicate** is a placement command carrying a copy of the design the machine has at that instant. From the moment the copy lands the two are strangers, and editing one does nothing at all to the other. |
| Are the diagnostics enough to play with? | At the world altitude, yes — Prototype 1's `why` was already written for exactly this. Inside a machine they are the machine's own, read at the phase its orbit is in, which answers "could this design use more?" but not "is the world feeding it?". Those are two questions and this prototype answers them in two windows. |
| What is still whole-room? | The correction. A hash mismatch resends the whole snapshot rather than the one deterministic region that differs. The architecture is already per-room rather than per-application, so narrowing it is a change to what goes in the envelope rather than to who sends it. |
| What is deliberately not here? | Accounts, matchmaking, dedicated servers, cloud persistence, blueprint propagation, collaborative editing of one draft, the full historical ghost timeline, and any attempt at prediction on the client. Section 27 asked for none of them and none of them answer the question. |

The central proof was:

> **A continuously running deterministic factory can be collaboratively
> constructed at multiple scales by multiple players, with only discrete player
> commands synchronized between them.**

It survived. The clock never stopped, the browser never guessed, and the three
copies of the room agreed to the bit.

## Prototype 3: the five rooms

Prototype 2 answered a technical question and left a design one standing. Two
people can build one deterministic factory together while it runs — proved,
hashed, and not in doubt. But a Room was still a disposable challenge. You met
the objective, the screen said so, and the factory you had just spent an hour
on had nothing further to do.

> **Does finishing one factory make me want to start the next one?**

Answering that needed four things to stop being hypothetical.

```text
   Coal Basin ─── coal ───┬──────────────► Power Station
    (basin)               │                  (station)
        │                 │                      │
        ├──── coal ──► Iron Valley               │ power
        │              (valley)                  ▼
        │                 │              Manufacturing
        │                 │ concentrate    (works)
        │                 │                      │ gears
        └──── coal ──►  Final Works ◄────────────┘
                         (final)
```

```powershell
.\run.ps1 -Camp              # the campaign, at http://127.0.0.1:8795
.\run.ps1 -Camp play         # all five rooms, played headlessly
.\run.ps1 -Camp map          # five rooms, seven lanes, three fleets
.\run.ps1 -Camp tech         # the twelve components, and what each one opens
.\run.ps1 -Camp refuse       # everything the campaign will not allow
```

### 1. A room is still a Room

Nothing in `camp` reimplements Prototype 2. A site **is** an `mp::room::Room`:
its own goal, its own command log, its own `(tick, sequence)` ordering, its own
host reconstruction and one replica per player, compared by canonical hash
every simulated second. Five of them is five of that.

The campaign adds a clock they share, a ledger between them, a shelf of designs
and a set of unlocks — and nothing else. The whole of `web/room/` is served
here unchanged and unforked, the same way Prototype 2 served experiment 10's
renderer; the campaign front end is a *shell* around it with a map, a library,
a component list and a shipping board.

That also settles the question the brief asks sideways. *Does a room keep
running while nobody is there?* It does, because the campaign advances all five
on every pump regardless of who is looking at which. There is no active room.
There are five factories and one clock.

### 2. An arrival is a command

The obvious way to move thirty thousand coal between rooms is to reach into the
destination's simulation and add it to a bay. That works exactly once, on the
host, and then every replica is a different factory.

So an arrival is an `Act::Deliver` — stamped, ordered and logged like a player
putting down a bay:

```text
depot ships 240 coal/s at Coal Basin
     │ ledger, every five simulated seconds, on a lattice
train leaves with 30,000
     │ 57 seconds
Deliver { to: Yard14, item: Coal, qty: 30,000 }  →  Power Station's log
     │
carry.qty[(Yard14, Coal)] += 30,000    at a rendezvous, on every replica
```

The last line is the interesting one. `Carry` is Prototype 1's mechanism for
*editing* a running plant: bring the region to the tick, harvest, pour the
state back in. Used unchanged, it is also the mechanism for *supplying* one.
The canonical hash covers it because the carry is part of the canonical hash,
and nothing about the multiplayer proof had to be weakened to move a train
between rooms.

The settlement is on a lattice for the same reason the accounting is. What a
route dispatches is a *difference* — how much the origin has shipped since the
last look — and if "the last look" meant "whenever a browser polled", two
clients polling at different rates would batch the same coal into different
trains and the two rooms would diverge from opposite ends. So the ledger only
ever settles at multiples of five simulated seconds, whatever the clock is
doing, and a departure is a function of the clock rather than of the network.

Conservation is not assumed. What leaves is what the origin's depot actually
swallowed; what arrives is what left, minus whatever the destination yard was
too small to hold — and the spill is reported, because a yard is a decision
about how long a room can be left alone and undersizing one should cost
something.

### 3. Progression is a component, never a percentage

The brief was blunt about this and it was right:

```text
avoid:   Research: Smelting +10%
instead: Unlock: counterflow heat exchanger
```

A percentage is a number that moves. A component is a *topology that did not
exist before*, and the difference is that the second one sends you back to a
machine you finished an hour ago. Twelve of the thirty-eight components are
held back:

```text
motor gearbox shaft      a drive train: something to turn a crusher with
separator                a split, and therefore a byproduct
preheater condenser      heat and vapour that come back rather than leave
furnace rollmill press   hot metal, and a shape to put it in
crank                    rotary becomes strokes
lathe                    one machine where three were, and swarf
column                   the crude chain, which nobody has touched yet
```

There is no second list of unlocked *machines*. A prototype in the catalogue is
placeable exactly when every component in its stock design has been unlocked,
which is computed rather than authored — so the Steam Crusher appears the
moment the separator does, and nobody has to remember to write that down twice.
A locked prototype is shown rather than hidden, greyed, with the missing
component named: a progression nobody can look forward to is a progression
nobody notices.

The ladder is checked by a test rather than by hope. Walking the rooms in
dependency order, every room's intended answer must be buildable from the
components handed over *before* it — which is the property that makes a
progression a progression rather than a lock with its own key inside.

### 4. Five problems, not one problem five times

This was the single most important instruction in the brief: do not turn
"produce 100 gears" into "produce 500 gears" and call it a second room.

| room | the problem | the objective |
|---|---|---|
| **Coal Basin** | a platform too small for the plant it needs | 400 MW held for 45 s, inside 480 tiles |
| **Iron Valley** | all the land in the world, and a seam worth 35 coal/s | deliver 24,000 ore powder |
| **Power Station** | every lump of fuel is a minute away, in trainloads | 320 MW held for 45 s |
| **Manufacturing** | no coal, no water, no grid: two live supply chains | 45 gears/s held for 45 s |
| **Final Works** | a load that will not sit still | see below |

Final Works is the one that could not have been asked before experiment 06. Its
demand is:

```text
never below 110 MW
over 380 MW at least once in every 10 seconds
above 240 MW for no more than 2 seconds in any 10
```

Four steam plants held wide open satisfy the first two requirements and fail the
third every second of the run. The answer is a plant that idles and then
surges — which is a fact about a machine's *orbit* rather than about its
average, and keeping the orbit instead of the average is the one thing
experiment 06 refused to give up. The stock Pulse Plant makes 362 MW every
seven seconds and nothing in between; two flat plants under it give the floor.
Six experiments later, that refusal finally bought something a player has to
care about.

### What was tried and thrown away

The brief asks for a room whose problem is scarce water, answered by recycling
steam through a condenser. It is not here, and the reason is worth writing down
rather than quietly dropping.

In this machine model the water cost of a megawatt is fixed. Every stock plant
lands within two percent of `1.48 water/MW`, because the chain from exchanger to
turbine to generator has no slack in it — and a turbine *consumes* its steam
rather than exhausting it, so there is nothing downstream for a condenser to
catch. A room whose problem was water would have been a room with exactly one
answer, arrived at by arithmetic rather than by design.

So the water room became a fuel-logistics room, and the condenser earns its
place as an unlock that pays off inside the refinery chain, where the light
fraction really does come off as vapour. Measuring the candidate constraints
before authoring the rooms — footprint, waste, latency, yield — cost an
afternoon and saved shipping a room that cannot be solved.

### What had to change below the campaign

Four things, all small, all in Prototype 2 or below, and each one a gap the
single-room version could not have found:

**A world has a plot size.** Prototype 2 had one room and one `PLOT`. "Very
constrained footprint" is a sentence a room can only say by being smaller than
the others.

**An installation can be rated.** A coal seam that yields 35 a second rather
than 100 is how a room states scarcity. Nothing a player places ever carries
one; it exists so that a room can be *furnished*.

**A bay can be filled from outside.** This one was a genuine hole. A bay holds
exactly what is put into it, and the producing side is the only side allowed to
say what that is — a rule `dsl` enforces for a good reason. An import yard is
filled by a channel from another region, so nothing in the document can declare
its slot, and every machine drawing from one was refused for being fed by
nobody. The fix is one keyword:

```text
storage Yard14 { capacity 240000 holds Coal policy round_robin }
```

`initial 0 Coal` could not have said it, and is still refused, for the good
reason that it usually means somebody mistyped a quantity.

**A submitted command can report its effects.** A train that arrived at a yard
too small for it spilled, and the shipping office is entitled to know how much
before the next one leaves.

### The playthrough

`camp play` is this experiment's acceptance command. It plays all five rooms end
to end with the clock in a vice — building, opening supply lines, keeping a
design on the shelf, going back to Iron Valley once the separator arrives — and
then asks the three questions the prototype exists to answer.

```text
Coal Basin met at 0:53      unlocked: motor, gearbox, shaft
  a stamping line is refused: the press has not been unlocked
  Iron Valley is shut: Coal Basin has to be producing first
Iron Valley met at 7:31     unlocked: separator, preheater, condenser
  a train starts running coal, Basin to Valley: 50 seconds, 30,000 a load
Power Station met at 10:21  unlocked: furnace, rollmill, press, crank
Manufacturing met at 14:24  unlocked: lathe
  a stamping line -- the press arrived with the Power Station
Iron Valley, again: a steam crusher, which was not placeable an hour ago
Final Works met at 18:51    unlocked: column

what moved between the rooms
  basin   -> valley  Coal        train     200,000 in 21 trips, 0 spilled
  basin   -> station Coal        train      84,000 in 11 trips, 0 spilled
  basin   -> works   Coal        convoy     11,550 in  5 trips, 0 spilled
  station -> works   Power       convoy     90,000 in 16 trips, 0 spilled
  basin   -> final   Coal        train      11,400 in  2 trips, 0 spilled
  works   -> final   Gear        convoy      7,500 in  5 trips, 0 spilled
  valley  -> final   Concentrate convoy      3,000 in  3 trips, 0 spilled

the proof
  rooms finished        5 of 5
  hash comparisons      219
  basin     agree  host 03ea7da8…  Ada 03ea7da8…  Bruno 03ea7da8…
  valley    agree  host 3d2fd37f…  Ada 3d2fd37f…  Bruno 3d2fd37f…
  station   agree  host 185d8192…  Ada 185d8192…  Bruno 185d8192…
  works     agree  host 73885438…  Ada 73885438…  Bruno 73885438…
  final     agree  host ecffed22…  Ada ecffed22…  Bruno ecffed22…
```

Nineteen minutes of simulated time for a run that already knows the answers; a
player spends the rest of the hour designing. Two players hold ten replicas
between them — five rooms each — and every one of them is fed the command
stream of a room they may not have looked at for ten minutes.

`camp refuse` does for the campaign what `room fail` does for the room:

```text
walking into a room that is not open      Final Works is not open yet: Iron
                                          Valley and Manufacturing have to be
                                          producing first
placing a machine whose parts are locked  Furnace Chamber has not been unlocked
                                          yet -- metal hot enough to be shaped,
                                          and past melting, poured
bulldozing the seam it came with          Coal Pit came with Coal Basin and
                                          cannot be removed
issuing an arrival by hand                an arrival is not something a player
                                          does
opening a lane the map does not have      there is no coal line from basin to
                                          basin
saving two designs under one name         `Mk1` is already on the shelf -- copy
                                          it instead
```

### What it answered, and what it did not

| the question | the answer |
|---|---|
| Does a room keep running while nobody is there? | Yes, and it is tested rather than asserted: the same script run polling every second and polling every sixty produces the same canonical hash and the same deliveries. That is the whole promise of `state(log, T)`, finally spent on something a player would notice. |
| Did the solver have to change? | No. Not one line of `pop`, `rooms`, `domains`, `sim` or `analytic`. `dsl` gained one keyword, and it gained it because an import yard is a real thing the language could not say. |
| Was `Carry` really the mechanism for supply as well as for edits? | Yes, unchanged. An arrival is a rendezvous with a quantity added at it, which is what an edit already was. |
| Is an unlockable component better than an unlockable number? | Much better, and the reason is legible in the playthrough: "a steam crusher, which was not placeable an hour ago" is a line about going *back*, and no percentage produces one. Deriving machine unlocks from component unlocks rather than authoring both was the decision that made it cheap. |
| Are five problems really different? | Four of them are, and the fifth is honest about being a logistics problem rather than a design one. Measuring first is what stopped it being five sizes of one problem: one of the constraints the brief suggested turned out not to exist in this machine model, and finding that out before authoring was worth more than the room it cost. |
| Does the design library earn "major mechanic"? | Not yet. Save, copy, lineage and place-from-shelf all work and are tested, but nothing in the five rooms *forces* a derived design — the stock catalogue answers every objective. The mechanic is built; the content that would make it necessary is not. |
| What is the campaign authoritative about? | Which rooms are open, what may be built, what leaves on a train, and what is on the shelf. Every one of those refusals is structural in exactly Prototype 2's sense — it depends on campaign state and not on who asked or when the packet arrived — so it is the same refusal on every machine, and a refused command never enters a room's log. |
| Is the whole thing 60–120 minutes? | The scripted run is nineteen minutes of simulated time and knows every answer in advance. A player who has to *design* the powder line rather than place it, and who wants Iron Valley shipping concentrate rather than powder, is in the right band. Whether they *want* to is a play session, not a test suite. |
| What is deliberately not here? | Procedural rooms, blueprint propagation, maintenance and wear, recursive machines, and any attempt at making the shelf propagate into placed machines. The brief asked for none of them, and the first one in particular is the thing you do *after* you know what a good problem looks like. |

The central question was:

> **Does the player develop attachment to their factory network and machine
> designs, and does gaining new capabilities make them voluntarily go back and
> improve old systems?**

The second half now has a mechanism and a demonstration: the separator arrives
in Iron Valley's completion screen, Final Works asks for concentrate, and the
only place to make it is a room that has been quietly shipping powder for ten
minutes. The first half is a question about an evening with two people and a
browser, and no test suite is going to answer it.

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
   seek resumes from the carry it already has; a backward one starts again from
   the last boundary at or before the target. Scrubbing left across a long
   horizon is therefore the slow direction, and the closed form is not yet
   wired into a seek. One carry is cached, not a ladder of them, so there is an
   obvious next move here and it has not been made.
9. **Canvas 2D, not GPU instancing.** Close up, a population draws as a few
   thousand sampled machines rather than the millions the experiment brief
   imagines. The snapshot already carries what an instanced renderer would
   need — class, population, state distribution, seed — so this is a renderer
   limitation, not a model one.
10. **One plant, one page.** No multiplayer, no shared editing, no power, no
    splitters, no fluids. The document is a command log and generic nodes,
    ports, links and storages precisely so those can arrive later.
11. **The edit barrier is global.** An edit synchronises *every* region, not
    just the ones it touches. Placing a smelter on one continent stops a mine
    on another. The cost is small — the barrier is the only thing paid for, and
    a region that is already ahead simply waits — but the *scope* is wrong, and
    a plant with many independent regions and a busy player will feel it before
    the cost does.
12. **An edit is only ever at the present or the beginning.** The log is
    required to be in order, so there is no editing the past and no branching
    timeline. Scrubbing back and building would need the log to fork, which is
    a different feature wearing this one's clothes.
13. **A refused command is refused by the server, one round trip later.** The
    browser proposes and un-proposes; for one person on loopback that is
    invisible, and for a player on a real connection it would not be.
14. **The costs in `scenarios/` are guesses.** They are a rules file, they are
    meant to be edited between playtests, and nothing checks that they make a
    good game — only that they mean the same thing twice.
15. **Experiment 07's stores buy nothing.** Every port has its own capacity, so
    every component is already a buffer and a dedicated one has no work to do.
    Ten of thirty-eight components are used by no shipped design for this
    reason. The fix is smaller port capacities, not more components.
16. **A property is a band, not a quantity.** Temperature is one of ten, size is
    one of four, speed is one of ten. That is what keeps an orbit findable and
    it is also why nothing here can express "hot enough, but only just". A
    finer scale would need a different way of closing the orbit.
17. **Blending averages, and averaging is lossy.** Two lots of the same
    substance meeting in a port come out weighted and rounded. It is
    deterministic and it is not reversible, so a design that mixes 82% and 12%
    ore has genuinely thrown the separation away — correct, but it means purity
    can be destroyed by a careless wire and the tool does not warn first.
18. **The machine designer is still not the game.** Experiments 06, 07 and 08
    share a binary, a server and a directory with the workbench and touch none
    of its code. Nothing decides yet whether a compiled macro-machine is placed
    into a `Blueprint` as one node or as its own sub-plant, and that is the
    actual integration question.
19. **Routes do not know about each other.** Experiment 08's router refuses to
    cross a run already laid, which is enough to keep pipework legible and not
    nearly enough to make it look designed. Two lines going the same way should
    share a rack and gain a bracket; instead they take neighbouring lanes and
    each grows its own posts. It is the most obvious next thing and it is not a
    change to the architecture, only to the cost function.
20. **Elevation is derived, so there is no upper storey.** A machine's height
    comes from what it *is*, which means a player cannot put a pump under a
    platform on purpose. Free 3D placement would fix that and would immediately
    raise the question the core rule exists to prevent: whether the picture is
    allowed to change the machine.
21. **One plant at a time, and no world.** Every scene is built from scratch,
    at full detail, for one installation. Instance buffers are the reason to
    believe a hundred at once is answerable; nothing has tried.
22. **The enclosure is four rules and a wall.** Openings come from the machine,
    which is the good part; everything else is a rectangle. No bays, no
    mezzanines, no relationship between the building and what it contains
    beyond "is it taller than nine metres".
23. **Twelve flat materials, and not one texture.** Experiment 09 gave the plant
    a material *language* and no material *surfaces*: no dirt, no edge wear, no
    heat staining, no insulation texture, no warning markings, no lettering.
    Every material is a colour, a roughness and a metalness. That is the single
    biggest remaining gap between this and something shippable, and it is the
    one place in the whole pipeline where a generative tool would obviously
    earn its keep.
24. **The readability metric cannot see hierarchy.** `tones` and `chroma` are
    measured off the rendered pixels and they barely move between the grades,
    while the plant improves enormously — because what improved was which grey
    went where, and neither number can see that. `legible` counts how many
    equipment kinds have a mesh-and-material signature nobody else shares,
    which is closer and still crude. There is no measurement here of the thing
    the experiment was actually about.
25. **A grade is a global switch, not a distance.** The four looks are a
    build-time argument, so a plant is articulated everywhere or nowhere. The
    obvious use of the axis — full articulation on the installation you are
    standing in, grade A on the twelve behind it — needs the grade to be per
    piece and per distance, which is the same machinery as the LOD prefix and
    has not been wired to it.
26. **The campaign is one process and one save.** `camp serve` holds a single
    campaign in memory. There is no persistence, so closing the window closes
    the world — which is fine for a prototype whose question is about an hour
    and wrong for anything else.
27. **The design library is available, not necessary.** The five rooms can all
    be answered out of the stock catalogue. Save, copy, lineage and
    place-from-shelf work and are tested, but no room yet poses a constraint
    that *requires* a derived design, so the mechanic the brief called major is
    on probation until a room forces it.
28. **A route is filled in the order it was opened.** Three rooms wanting coal
    from one yard are served first-come, each up to the cap the player set. It
    is deterministic and legible, and it is not the priority system a real
    shortage wants — a room that opened its lane late starves quietly rather
    than negotiating.
29. **Rooms are authored, and there are five.** Deliberately: the brief was
    explicit that procedural generation should wait until somebody knows what a
    good problem looks like. What that means today is that the campaign's whole
    content is one table, and it ends.

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
src/live.rs       Prototype 1: the command log, the barrier, and the carry
src/why.rs        Prototype 1: why a thing is not running, and what binds
src/scenario.rs   Prototype 1: budgets, orders, deadlines -- and no physics
src/main.rs       experiment harness, `serve`, `export` and `play`
web/              the workbench: canvas, inspector, timeline, timetable, brief
tests/            211 cross-validation tests
configs/          the fifteen configurations, plus the first scenario plant
scenarios/        problems posed about a plant, in their own little language
sketches/         where the workbench saves what you build

src/machine/stuff.rs   Ex 07: seven domains, thirteen substances, five properties
src/machine/parts.rs   Ex 07: thirty-eight components in eight families, and the numbers
src/machine/design.rs  Ex 06: components on a tile grid, wires between their ports
src/machine/sim.rs     Ex 06: transfer along wires, then every component steps
src/machine/orbit.rs   Ex 06: run it until it repeats; keep transient + period
src/machine/eval.rs    Ex 07: four briefs, competing costs, and no score
src/machine/snap.rs    Ex 06: state(t) for a renderer, and why things are stopped
src/machine/web.rs     Ex 06: its own small server, so it can be thrown away
src/bin/machine.rs     Ex 08/09: run, why, compile, verify, parts, reuse, form, kit, read
web/machine/           Ex 06: the designer, Ex 08: the plant in WebGL, Ex 09: its four looks
designs/               Ex 08: sixteen answers to four briefs
tests/machine_web.mjs  Ex 06: the front end, checked without a browser

src/machine/form/mod.rs     Ex 08: millimetres, pieces, batches, and the pipeline
src/machine/form/kit.rs     Ex 08/09: twenty-nine canonical meshes, twelve materials
src/machine/form/seed.rs    Ex 08: where every cosmetic choice comes from
src/machine/form/layout.rs  Ex 08: volumes, mounts, orientation, sockets, clearance
src/machine/form/route.rs   Ex 08: A* with a heading; Ex 09: the connection vocabulary
src/machine/form/frame.rs   Ex 08: plinths, columns, supports, platforms, stairs
src/machine/form/paint.rs   Ex 09: the material language, as one pass over one field
src/machine/form/body.rs    Ex 08: thirteen archetypes; Ex 09: articulated
src/machine/form/shell.rs   Ex 08: slab, walls, roof, and the holes the plant cuts
src/machine/form/obj.rs     Ex 08: the scene, baked, for anything that opens .obj
src/machine/form/shot.rs    Ex 08: a rasteriser and a PNG writer, so it can be seen
                            Ex 09: contact sheets, captions, and the palette metric
tests/form.rs               Ex 08: the five claims, checked rather than asserted
tests/read.rs               Ex 09: the five claims about what did *not* change
tests/space.rs              Ex 10: placement, interfaces, routing and clashes

src/mp/mod.rs        P2: sixty ticks a second, and the two seeds a room is
src/mp/kit.rs        P2: what may be placed, in seconds rather than in ticks
src/mp/lower.rs      P2: a machine design becomes a world recipe -- its own orbit
src/mp/world.rs      P2: the game document, and the compiler down to the IR
src/mp/cmd.rs        P2: sixteen intentions, and every refusal they can meet
src/mp/goal.rs       P2: twenty-one hand-written problems, and the books
src/mp/room.rs       P2: the clock, the log, and one reconstruction per player
src/mp/net.rs        P2: the only stateful server in the repository
src/bin/room.rs      P2: serve, test, fail, goals, parts
web/room/            P2: the lobby, the plot, the inspector, the machine window
tests/mp.rs          P2: twenty-five properties, with the clock held still
tests/room_web.mjs   P2: two players and a whole session, without a browser

src/camp/mod.rs      P3: five rooms, one clock, and what had to become real
src/camp/site.rs     P3: the five rooms, hand-authored and deliberately nasty
src/camp/tech.rs     P3: twelve components, never a percentage
src/camp/shelf.rs    P3: My Machines, and the lineage a copy remembers
src/camp/ship.rs     P3: lanes, fleets, and a ledger that settles on a lattice
src/camp/run.rs      P3: the campaign -- five Rooms, a pump, and the refusals
src/camp/net.rs      P3: the same client, one campaign, five room codes
src/bin/camp.rs      P3: serve, play, map, tech, refuse
web/camp/            P3: the map, the shelf, the components, the shipping board
tests/camp.rs        P3: eighteen properties, including the one about leaving
tests/camp_web.mjs   P3: the campaign half of the client, without a browser
```

Zero dependencies outside `std`. The workbench added a JSON codec and an HTTP
server rather than a dependency tree larger than the crate they serve.

> **v1:** compress repetition.
> **v2:** compress interaction.
> **v3:** compress causality.
> **Prototype 0:** stop compressing things and go and look at one.
> **Prototype 1:** let someone change it while it is running, and give them a
> reason to want to.
> **Experiment 06:** stop scaling buildings and start designing one.
> **Experiment 07:** and then design something that is not a power plant.
> **Experiment 08:** and then go and look at it.
> **Experiment 09:** and then find out how much of *looking* is paint.
> **Experiment 10:** and then let somebody move it.
> **Prototype 2:** and then let two people build one together, without stopping
> the clock.
> **Prototype 3:** and then give them somewhere to go next, and make the thing
> they leave behind keep working.

The thing this file asked for last — *a reason to keep a room open* — is what
Prototype 3 is. A finished room becomes a supplier, keeps supplying while
nobody is there, and hands over a component that makes an hour-old machine
worth reopening. The `Carry` that carries it is still Prototype 1's, unchanged:
the snapshot the networking proof needed, and then the object an arrival lands
in, turned out to be the object an edit already produced.

What is left is content rather than architecture — more machines, more rooms, a
constraint the stock catalogue cannot answer so that the design library becomes
necessary rather than merely available — and the one piece of engineering
deliberately left whole-room: a hash mismatch resends the entire snapshot,
where the region structure underneath it could resend one deterministic region
and replay the rest.
