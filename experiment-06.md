## Experiment 06: Machine Designer

Build a standalone prototype for designing a **machine/building from functional internal parts**.

The core idea is:

> A building is a small deterministic factory graph assembled by the player, then compiled into a reusable macro-machine once its behavior is known.

Do **not** integrate this into the main game yet. Treat it as an isolated gameplay experiment.

### Goal

Test whether designing the internals of a machine is interesting enough to become a major game mechanic.

The player should solve a constrained engineering problem by connecting components rather than selecting a predefined recipe or increasing `xN`.

Use one challenge initially:

> Produce at least 100 MW from one fuel source while minimizing footprint, water use, and wasted heat.

### Initial component set

Keep the vocabulary deliberately small:

```text
Fuel / Heat Source
Heat Pipe
Water Source
Heat Exchanger
Steam Pipe
Steam Buffer / Tank
Turbine
Generator
```

Each component has a small number of deterministic properties.

Example:

```text
Reactor
  produces heat: 1000/tick

Heat Exchanger
  heat capacity: 250/tick
  water capacity: 100/tick
  produces steam: 100/tick

Turbine
  steam capacity: 80/tick
  rotary output efficiency: 75%

Generator
  rotary capacity: 70/tick
  electrical efficiency: 90%
```

Numbers are experimental, not intended as realism.

### Typed ports

Components connect through explicit typed ports.

Initial port types:

```text
heat
fluid
steam
rotary
electrical
```

For example:

```text
                ┌─────────────────┐
heat ──────────►│ Heat Exchanger  │────► steam
water ─────────►│                 │
                └─────────────────┘
```

Connections are legal only when the port types are compatible.

Do not introduce detailed real-world electrical, thermodynamic, pressure, torque, or fluid simulation yet. Properties should be just rich enough to create capacity and efficiency constraints.

### Simulation model

Reuse the deterministic principles from `temporal-rooms`.

Components are state machines and connections carry deterministic quantities through time.

Do not reduce everything immediately to average rates.

The design may contain:

- startup transients,
- buffers,
- blocked components,
- starvation,
- periodic behavior,
- batch-like behavior.

If the machine eventually settles into an orbit, retain the exact transient + periodic behavior.

### Player interaction

Provide a small 2D designer where the player can:

1. Place components.
2. Move them.
3. Connect compatible ports.
4. Remove components/connections.
5. Run/pause the design.
6. Scrub to arbitrary ticks.
7. Inspect component state.
8. See flow/utilisation visually.
9. See overall machine inputs, outputs, waste and efficiency.

The renderer must continue following:

```text
Simulation
    ↓
State(t)
    ↓
RenderSnapshot
    ↓
Renderer
```

The renderer never simulates.

### Feedback

Make bottlenecks obvious.

Clicking a component should explain things such as:

```text
Turbine
status: STARVED
needs: 80 steam/tick
available: 63 steam/tick
utilisation: 78.8%
```

or:

```text
Heat Exchanger
status: BLOCKED
steam output buffer full
unused heat: 47/tick
```

The player should be able to understand *why* a design performs poorly.

### Evaluation

Continuously report the whole design's performance:

```text
Electrical output
Fuel consumption
Water consumption
Heat wasted
Footprint
Component count
Utilisation
```

These create competing optimisation goals.

Do not make one metric the only score. A compact 100 MW plant and an efficient 100 MW plant should potentially be different designs.

### Macro-machine compilation

Once a design is valid, allow it to be treated as a reusable custom building.

Conceptually:

```text
CustomMachine {
    externalInputs
    externalOutputs
    internalState
    transient
    periodicOrbit
}
```

The external factory should not need to continuously materialise every internal component when nothing externally relevant is happening.

A saved design might become:

```text
Compact Reactor v3

Inputs:
  FuelCell
  Water

Outputs:
  Electricity
  WasteWater

Footprint:
  18 × 12
```

The player can later place that custom building as one installation while retaining the ability to inspect its internals.

### Important constraint

Do **not** assume every finished machine reduces to:

```text
input rate × efficiency = output rate
```

The compiled result may instead be:

```text
startup transient
+
exact periodic orbit
```

Two machines with identical average output may therefore behave differently under changing supply or demand.

### What this experiment is trying to prove

Not simulation scalability. That battle has already consumed enough innocent CPU cycles.

The question is:

> **Is assembling machines from functional components a fun optimisation problem that produces understandable but non-obvious designs?**

Success means players can build multiple genuinely different solutions to the same target, understand why one performs better, and feel motivated to improve or reuse their designs.

If that works, the mechanic can later expand beyond power generation into things like combustion engines, distillation, refining, chemical plants, heat recovery, compressors, pumps and more complicated production machinery.