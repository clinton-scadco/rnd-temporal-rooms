# temporal-rooms

An event-driven factory simulator with an **exact closed-form solver**, built to
answer questions about billions of factory objects without touching most of them.

The premise: given a plant like

```
Source -> Storage -> Processor -> Storage
```

we want two things that are usually treated as alternatives — a faithful
event-based simulation, and a way to compute the state at tick *t* without
visiting tick *t*. This crate provides both, and proves they agree.

## Quick start

```powershell
.\run.ps1          # build + run all three configurations
.\run.ps1 -Test    # run the cross-validation suite
```

`run.ps1` exists because rustup installs the MSVC toolchain without setting the
MSVC/Windows-SDK library search paths; it discovers them and sets `LIB`.

## The DSL

```
item IronOre
item IronPlate

blueprint SmeltLine {
    source  Miner      { produces 100 IronOre every 60 ticks }
    storage OreBay     { capacity 1000 }
    process Smelter x4 { consumes 10 IronOre takes 20 ticks produces 10 IronPlate }
    storage PlateBay   { capacity 1000 }
    sink    Shipping   { consumes 100 IronPlate every 60 ticks }

    wire Miner -> OreBay -> Smelter -> PlateBay -> Shipping
}

deploy 125_000 x SmeltLine stagger 7
```

| construct | meaning |
|---|---|
| `item N` | declares an item type. If any are declared, all must be. |
| `source N { produces Q I every D ticks }` | emits `Q` of `I` every `D` ticks |
| `process N { consumes Q I  takes D ticks  produces Q J }` | multiple `consumes`/`produces` lines allowed |
| `sink N { consumes Q I every D ticks }` | drains items out of the plant |
| `storage N { capacity C }` | `C` is a **total unit count shared across item types** |
| `N x4` | replicate a node four times inside the blueprint (`x 4` also works) |
| `wire A -> B -> C` | chain sugar; must alternate machine/storage |
| `deploy K x B stagger S` | `K` copies of blueprint `B`, copy *k* starting at tick `(kS mod P)` |

Two design decisions carry most of the weight:

- **A blueprint is small; a deployment is huge.** All analysis runs on the
  blueprint (tens of nodes). Object counts in the billions live only in a
  `count` field and are never materialised unless you ask.
- **Source, process and sink are one state machine.** A source is a machine with
  no inputs, a sink one with no outputs. There is a single code path for all of
  them (`sim.rs`), so there is a single thing to get right.

## The four tiers

| tier | what it is | cost in *t* | exact |
|---|---|---|---|
| T0 | a tick loop | O(*t* · N) | yes |
| T1 | discrete-event simulation | O(events) | **yes** |
| T2 | periodic-orbit closed form | **O(1)** | **yes** |
| T3 | rate algebra (fluid fixpoint) | none at all | asymptotic |
| T4 | archetype compression | O(1) per archetype | **yes** |

T0 is never implemented. It is the thing this crate exists to avoid.

### T1 — discrete-event simulation (`sim.rs`)

Time advances by popping the next scheduled event. Every machine runs:

```
Idle --[withdraw all inputs atomically]--> Working(D) --[deposit all outputs]--> Idle
         |                                                     |
      (fails)                                               (fails)
         v                                                     v
      Starved <---- woken by a storage mutation ----------> Blocked
```

Withdrawals and deposits are all-or-nothing. Blocked machines are re-woken by
the **static** client list of the storage they are waiting on, so a blocked
machine costs zero allocation — the property that makes huge arenas viable.
State lives in struct-of-arrays columns indexed `instance * stride + local`.

### T2 — the periodic orbit (`analytic::orbit`)

A plant with finite storages has a **finite dynamical state space**: bounded
integer buffer levels, and machine phases bounded by cycle times. A deterministic
map on a finite set is eventually periodic. So:

1. Simulate the transient once. Whenever the clock is about to advance, encode
   the complete state canonically — buffer contents plus machine states with
   deadlines made *relative* to now.
2. A repeated encoding is a **proof** of periodicity, not a heuristic.
3. Then for any *t* past the orbit entry `t₀`:

```
state(t) = base + ⌊(t − t₀)/P⌋ · Δ + replay((t − t₀) mod P)
```

Answering for *t* = 10¹⁸ costs exactly what *t* = 10³ costs. Buffer contents are
bounded, so they are *exactly* periodic; only counters grow, and they grow
linearly by Δ per orbit.

If the event queue empties, the plant is **frozen** — deadlocked — and the state
is constant for all future time. That is a closed form too, and a useful verdict.

### T3 — rate algebra (`analytic::rates`)

A fluid relaxation, in exact rationals, with no simulation at all. Start each
machine at its unconstrained rate `1/D`, then iterate to a fixpoint pushing
**starvation** downstream (consumers of a scarce item scale proportionally) and
**backpressure** upstream (a full buffer throttles producers to the drain rate).
O(machines × iterations); converges in 2–5 iterations on these plants.

It answers "is this plant sustainable, what is its throughput, and where is the
bottleneck" instantly — and it is the one tier that is *not* exact, which is why
the test suite checks it against T2.

### T4 — archetype compression (`analytic::archetypes`)

Instance *k* starts at `(k · stagger) mod P`. That sequence is periodic in *k*
with length `L = P / gcd(stagger, P)`, so a deployment of **any** size has at
most `L ≤ P` distinct phase archetypes. An instance offset by `o` follows the
base trajectory translated in time, so:

```
totals(t) = Σ_archetypes  multiplicity_j · closed_form(t − offset_j)
```

One orbit solve plus `L` O(1) evaluations answers for a billion lines. Analysis
cost depends on the blueprint, never on the object count. This is the entire
scaling argument.

## Results

Measured on Windows 11 / x86-64, single-threaded, `opt-level=3 lto=fat`.

| config | objects | orbit | T1 event sim | T2+T4 exact | agreement |
|---|---|---|---|---|---|
| 01-spec | 3 | frozen at t=2060 | 186 events, 3.9 µs | 0.3 µs | exact |
| 02-balanced | 1,000,000 | period 60 from t=120 | 64.9M events, 12.6 s | 61 µs | exact |
| 03-megafactory | 1,000,000,005 | period 270 from t=10975 | not materialised | 1.6 ms | exact |

"Exact" means every counter — cycles per machine, units produced and consumed
per item — matches the event simulator bit for bit.

### Configuration 1: the reference plant deadlocks

The specification as literally written **stops permanently at tick 2060**.

`IronPlate` has no consumer, and the processor returns plate to the *same*
storage the ore lives in, where capacity is shared. Total occupancy therefore
only ever grows. At t=600 the storage is at 990/1000 and the source stalls at
t=660. The processor keeps converting the remaining ore into plate until, at
t=2060, the storage holds 1000 plate, the processor is starved, the source is
blocked, and the event queue is empty.

Both analytic tiers see it, from opposite directions: T3 reports the structural
cause without simulating anything (*"IronPlate is produced but never consumed"*,
asymptotic throughput zero), and T2 reports the exact tick.

Note the honest consequence: for a plant that freezes, T1 also terminates early,
so the closed form buys **no** time-horizon speedup here. It buys a verdict.

### Configuration 2: T3 is right in aggregate and wrong per machine

Four smelters share one ore buffer. The fluid model predicts they each run at
83.3% duty. The exact orbit says otherwise:

| machine | T3 (fluid) | T2 (exact) |
|---|---|---|
| Miner | 100% | 100% ✓ |
| Smelter#0, #1 | 83.3% | **100%** |
| Smelter#2, #3 | 83.3% | **66.7%** |
| Shipping | 100% | 100% ✓ |

Aggregate throughput agrees exactly (5/3 plate/tick). But withdrawal order is
deterministic by machine index, so the first two smelters run flat out and the
last two absorb the remainder. **The fluid model's proportional-sharing
assumption is not a property of the system; it is an artefact of the model.**
Anything that reasons about individual machine utilisation — heat, wear,
per-machine power draw, maintenance scheduling — needs T2, not T3.

Configuration 3 shows the same split: the three gear presses and two assemblers
agree with the fluid model, the four smelters do not.

### Configuration 3: past a billion objects

66,666,667 lines × 15 objects = **1,000,000,005 objects**, `stagger 7`,
lcm(60,20,30,45,27) = 540 → exactly **540 phase archetypes**.

Totals at t = 10¹⁸ for all billion objects: **1.59 ms**. The same question by
simulation would need ~3.5 × 10²⁵ events.

The chain is deliberately unbalanced — miner 1.667 ore/tick, smelters 2.0,
presses 2.0, assemblers 0.667 gear/tick, depot 0.185 engine/tick. Both solvers
identify the depot as the single bottleneck and propagate backpressure up the
whole chain, leaving the miner at 66.7% duty.

## Scaling: what was measured, and what was not

Measured directly:

- **1,000,000 objects fully materialised and simulated event by event** —
  64.9M events in 12.6 s, matching the closed form exactly.
- **22.6–22.8 bytes per object** of live arena (about half state, half event
  heap), at 5–7M events/s single-threaded.
- **1,000,000,005 objects solved analytically** in 1.6 ms.

Extrapolated, not measured:

- Materialising 10⁹ objects would need ~10 GiB of column state plus a comparable
  event heap. That fits on a large machine; the event *rate* is what makes long
  horizons impractical, which is the reason the analytic path exists.

## Limitations

These are real, and worth stating plainly.

1. **Lines are independent.** v1 has no wiring *between* blueprint instances.
   A shared bus or a common ore field would couple instances, break archetype
   independence, and require either genuine global simulation or a coupled
   analysis. This is the single biggest gap between this model and a real
   factory game.
2. **Orbit period is bounded but not guaranteed small.** Finiteness guarantees
   an orbit exists; nothing guarantees it is short. The solver takes an event
   budget and reports `found: false` rather than hanging. Observed periods here
   were 60 and 270 ticks.
3. **T3 is aggregate-only**, as demonstrated above.
4. **The event simulator is single-threaded.** Instances never interact, so it
   is embarrassingly parallel — an obvious next step, along with a bucketed
   timing wheel to replace the binary heap (whose `log n` on a 750k-entry heap
   is the current hot spot).
5. **Storage capacity is a shared unit count**, matching the spec's "1000
   units". Per-item-slot capacity would be a different, easy variant.

## Layout

```
src/model.rs      compiled IR: blueprints, deployments
src/dsl.rs        lexer, parser, lowering, validation
src/sim.rs        T1 discrete-event simulator over SoA columns
src/analytic.rs   T2 orbit, T3 rate algebra, T4 archetypes
src/main.rs       experiment harness
tests/            12 cross-validation tests
configs/          the three configurations
```

Zero dependencies outside `std`.
