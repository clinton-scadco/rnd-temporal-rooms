# v3 — Causality, Transport, and Room Execution

**v1** proved that enormous numbers of independent deterministic factory objects can be compressed into a small number of phase archetypes.

**v2** proved that interaction does not destroy that scaling argument. Large populations of coupled machines can be represented exactly by counts of identical local states, even under contention. One billion interacting machines can collapse to a few dozen population cells.

**v3** asks the next question:

> Can a large factory be decomposed into independently advancing causal regions, connected through explicit transport, while remaining exactly equivalent to one monolithic simulation?

The goal is to turn the domain analysis introduced in v2 into the execution model that a real Room can use.

## 1. Make transport part of the physics

Machines should not be able to consume from arbitrary remote storages as though every inventory were shared memory.

Material moves through explicit **links**.

A link represents a deterministic, capacity-constrained transfer with latency:

```text
Storage A
    │
    │ Link
    │ throughput: 100 items / tick
    │ latency: 30 ticks
    ▼
Storage B
```

The latency may derive from spatial distance:

```text
latency = base_delay + distance / speed
```

Transport type determines how material is scheduled.

A belt may move small quantities frequently:

```text
10 IronOre every tick
latency 20 ticks
```

A train may move a large batch:

```text
12,000 IronOre
depart t=10,000
arrive t=13,000
```

These can share the same underlying abstraction:

> A link is a deterministic delayed transfer with capacity and throughput constraints.

Average flow rates remain derived telemetry, not simulation state.

## 2. Let topology determine precedence

v2 currently has a limitation where batched feasibility assumes classes enumerate shared bays consistently.

v3 should reduce or eliminate this ambiguity by making access to materials explicit through the logistics graph.

Instead of a consumer conceptually reaching into:

```text
[OreBayA, OreBayB, OreBayC]
```

material reaches its local input through links:

```text
OreBayA ──► Link A ──┐
                     ▼
OreBayB ──► Link B ──► Input Buffer ──► Smelters
```

Ordering is therefore produced by:

- physical topology,
- transport latency,
- transport throughput,
- junction/storage arbitration policy.

If one path wins because its material arrives first, that is a property of the factory the player built, not an accidental consequence of array ordering.

Policies such as `round_robin`, `priority`, or `index` still matter where several contenders genuinely meet at one constrained resource.

## 3. Turn transit domains into independent simulations

v2 already identifies **transit domains** by cutting delayed transport links.

v3 should actually execute them independently.

For example:

```text
[ Mine Domain ]
      │
      │ train, 3000 ticks
      ▼
[ Smelting Domain ]
      │
      │ train, 1200 ticks
      ▼
[ Manufacturing Domain ]
```

Each domain has its own deterministic state and current tick.

They do not need to advance together:

```text
Mine            t=120,000
Smelting        t=94,000
Manufacturing   t=180,000
```

A domain only needs to stop advancing when it reaches its next external causal dependency.

If Smelting knows that its next incoming train arrives at tick 110,000, it may safely advance:

```text
94,000 → 110,000
```

without consulting the Mine domain.

At tick 110,000 the arrival becomes an input event, its state changes, and it can continue.

## 4. Cross-validate against the monolithic simulation

Correctness remains the priority.

For every decomposed configuration:

```text
Monolithic T1/T5 simulation
```

must exactly equal:

```text
Domain A
+
Domain B
+
Domain C
+
timestamped transfers
```

at the same observation tick.

Agreement includes:

- storage contents,
- population cells,
- machine/class counters,
- produced and consumed quantities,
- pending transfers,
- arbitration state,
- canonical state hashes.

The decomposed solver must not merely preserve throughput. It must reproduce the exact deterministic result.

## 5. Define the Room above domains

A **Room** remains the persistent gameplay boundary.

A Room may contain one or many causal domains:

```text
ROOM
┌───────────────────────────────────────────┐

 [Mine] ──rail──► [Refinery] ──rail──► [Factory]

└───────────────────────────────────────────┘
```

The player sees one Room.

The simulator may see:

```text
Domain 1
   ↓ timed transfer
Domain 2
   ↓ timed transfer
Domain 3
```

Domain boundaries are therefore computational, not necessarily visible gameplay boundaries.

A compact factory full of shared storage may collapse into one large coupled domain.

A huge rail-based megabase may naturally decompose into many independent domains because distance introduces transport latency.

## 6. Exploit causal slack

Transport latency provides a period during which two regions cannot affect each other.

That interval is **causal slack**.

If:

```text
Train departs A at t=10,000
Train arrives B at t=20,000
```

then after departure, changes inside A cannot affect B before tick 20,000 through that transfer.

That gives the runtime permission to advance the domains independently.

This is the scaling hypothesis v3 needs to test:

> Large spatial factories may become easier to distribute because distance creates deterministic windows of independence.

## 7. Test coupling between deployed populations

v2 still assumes deployed blueprint instances are independent.

v3 should also introduce cases where many previously independent deployments share infrastructure:

```text
              Shared Ore Network
                     │
        ┌────────────┼────────────┐
        ▼            ▼            ▼
     Line 1        Line 2      Line 1,000,000
```

This intentionally destroys the v1 T4 independence assumption.

The experiment is whether v2's population compression can move up another level.

Instead of only:

```text
Smelter x 1,000,000
```

we may eventually be able to represent:

```text
SmeltLine x 1,000,000
```

as populations of equivalent higher-level states.

This is exploratory rather than a required v3 success criterion.

## 8. Preserve multiple exact execution modes

Not every possible player construction needs to admit the strongest compression.

A domain should use the cheapest exact representation available:

```text
closed-form orbit
        ↓
population/lumped simulation
        ↓
event simulation
```

If a topology cannot be safely population-compressed, it may fall back to ordinary deterministic event simulation.

The important property is:

> Compression is an optimisation of exact semantics, not a requirement imposed on the player's factory.

## Primary v3 proof

v3 succeeds if:

> A factory split into independently advancing transit domains, communicating only through timestamped link events, produces exactly the same state as the monolithic deterministic simulation.

The domains must be able to sit at different simulation ticks and advance only as far as their next causal dependency requires.

## Secondary goals

- Make link throughput and latency first-class simulation properties.
- Derive transport latency from spatial distance where appropriate.
- Remove arbitrary remote-storage access from the model.
- Replace the remaining bay-ordering assumption with explicit logistics topology.
- Test cross-deployment coupling.
- Explore whether population lumping can recursively apply to higher-level factory structures.

## Still deliberately out of scope

v3 is not the point to add:

- detailed graphics,
- realistic belt item rendering,
- complex fluids,
- richer recipe chemistry,
- combat,
- parallel execution optimisation,
- timing-wheel optimisation,
- final multiplayer networking.

Those become much safer investments once the causal execution model is proven.

The progression is therefore:

> **v1:** compress repetition.  
> **v2:** compress interaction.  
> **v3:** compress causality.

If v3 works, the simulator will have the core architecture needed for Rooms: enormous deterministic factories that can be represented compactly, split into independently advancing regions, and synchronised through a small stream of explicit causal events rather than continuous object state.