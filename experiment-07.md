Definitely. Power is a good first slice because the chain is obvious, but the component vocabulary can expand into a much more general **industrial machine construction kit**.

I’d avoid adding dozens of one-off components. Better to define a few families of primitives that can combine into many machines. Otherwise you’ll accidentally recreate a parts catalog from an engineering supplier, which is a thrilling prospect for roughly seven people.

A useful expanded set would be:

- **Energy / heat**: burner, electric heater, reactor core, heat exchanger, heat sink/radiator, condenser, boiler, thermal storage, insulation.
- **Mechanical**: electric motor, combustion engine, turbine, gearbox, shaft, clutch, flywheel, crank, compressor, pump, blower/fan.
- **Fluid handling**: fluid inlet, pipe, valve, pump, tank, mixer, splitter/manifold, filter, separator, condenser, evaporator.
- **Material handling**: hopper, chute, feeder, conveyor, screw conveyor, loader, buffer bin, sorter, ejector.
- **Processing**: crusher, grinder/mill, press, roller, extruder, cutter, drill, furnace chamber, kiln, mixer, agitator, reactor vessel.
- **Separation / chemistry**: distillation column, centrifuge, cyclone separator, membrane/filter, settling tank, electrolyser, scrubber, crystallizer.
- **Control / regulation**: thermostat, regulator, governor, pressure valve, flow limiter, sensor, controller. These need not become programmable logic yet. They can simply provide deterministic thresholds and control rules.
- **Storage / inertia**: tank, accumulator, battery, flywheel, thermal mass, hopper. These are important because they make two designs with the same average throughput behave differently.

Then define a small set of **connection domains** rather than component-specific connections:

```text
material
fluid
gas
heat
rotary
linear/mechanical
electrical
```

Possibly later:

```text
pressure
vacuum
control
```

I would not make `steam` its own fundamental port type. Steam is a fluid/gas with properties. Likewise petrol, oxygen, coolant, molten metal, etc. should ideally be instances of material/fluid, not new physics categories every Tuesday.

### Example machines this immediately enables

A **crusher** could be:

```text
Electric Motor
    ↓ rotary
Gearbox
    ↓
Crusher Chamber
    ↓
Output Hopper
```

The optimization questions become motor sizing, gearbox ratio, chamber capacity and input/output buffering.

A **combustion engine**:

```text
Fuel ──────────────┐
                   ▼
Air → Compressor → Combustion Chamber
                   ↓ heat/pressure
                 Piston/Turbine
                   ↓ rotary
                 Flywheel
                   ↓
                 Shaft
```

Then bolt a generator onto the shaft for electricity, or use the rotary output directly to drive something else.

A **chemical reactor**:

```text
Feed A ─┐
Feed B ─┼→ Mixer → Reactor Vessel → Cooler → Separator
Catalyst┘                            │
                                    └→ waste
```

Player choices:

- reactor size,
- residence time,
- heating/cooling capacity,
- feed ratios,
- separation capacity,
- recycle loops.

That gets you remarkably close to Pyanodons territory without needing a hardcoded "advanced acid plant" recipe.

### Distillation is particularly promising

A refinery-like machine could be assembled from:

```text
Feed Pump
    ↓
Preheater
    ↓
Boiler
    ↓
Distillation Column
    ├→ light fraction
    ├→ middle fraction
    └→ heavy fraction
```

Then add:

```text
Condenser
Reboiler
Recycle
Additional column stages
```

The player is effectively designing a refinery unit rather than placing `Oil Refinery Mk3`.

But keep the model abstract. A column might expose:

```text
separation_quality
throughput
energy_required
```

rather than asking the player to solve vapor-liquid equilibrium equations while eating cereal.

### Manufacturing machines

This is where the idea gets really interesting because you can design what would traditionally be an assembler.

Suppose the goal is making gears.

One machine might be:

```text
Metal Inlet
   ↓
Heater
   ↓
Rolling Mill
   ↓
Stamping Press
   ↓
Output Hopper
```

Another:

```text
Metal Billet
   ↓
Lathe/CNC
   ↓
Gear
```

Different trade-offs:

```text
stamping:
high throughput
low precision
low material waste
expensive tooling

machining:
lower throughput
high precision
high power
produces scrap
```

Now the "recipe" is still beautifully simple:

```text
Iron → Gear
```

but the **machine performing the recipe** is where complexity lives.

I think that's the sweet spot you were originally reaching for.

### Components should modify transformations

A useful generic definition might be:

```text
Component {
    ports
    capacity
    transformation
    constraints
    efficiency
    state
}
```

For example:

```text
Crusher {
    input: Material
    drive: Rotary

    transformation:
        chunks → crushed

    constraints:
        hardness <= 8

    capacity:
        100 material/tick

    rotaryDemand:
        50 power
}
```

Then a motor isn't "part of the crusher recipe."

It simply supplies the required rotary domain.

That lets the player do stupidly wonderful things like run six crushers from one huge engine via a shared shaft:

```text
             ┌→ Crusher
Engine → Shaft → Crusher
             ├→ Crusher
             └→ Crusher
```

Which is exactly the sort of emergent industrial nonsense I'd want this mechanic to permit.

### I'd add properties to materials now too, but sparingly

You don't need the full property-based recipe system we initially discussed, but this machine designer benefits from a few properties:

```text
temperature
pressure
phase
particle_size
purity
moisture
```

Potentially:

```text
hardness
viscosity
```

These can act as intermediate states.

For example:

```text
IronOre
particle_size: coarse
```

Crusher:

```text
coarse → crushed
```

Mill:

```text
crushed → powder
```

Separator:

```text
purity 0.4 → purity 0.8
```

Furnace:

```text
temperature ambient → molten
```

Then the outer game's item can still simply be called **Iron Ore**. Internally, the machine design manipulates its properties.

That gives you complexity without requiring `CrushedIronOre`, `FineIronOre`, `HotFineIronOre`, `SlightlyMoistFineIronOre` as twenty-seven separate inventory icons. Pyanodons, please remain seated.

### I'd structure the next experiment around four machines

Rather than adding all components and hoping something interesting occurs, give the designer four challenges:

| Challenge | New systems tested |
|---|---|
| Generate electricity | heat, fluid, rotary, electrical |
| Crush ore | motor, gearbox, rotary, material transformation |
| Distill mixed fluid | heat, phase, fluid separation |
| Manufacture gears | material handling, press/machining, buffering |

Build a component set capable of solving all four.

If the same motor, pump, heat exchanger, buffer, shaft, etc. naturally appear across several designs, you've found good primitives.

If every challenge requires ten bespoke components used nowhere else, the abstraction is wrong.

The larger direction I'd aim for is:

> **Recipes define what transformation is required. Machines are player-designed networks that provide the physical processes needed to perform that transformation.**

So `Iron Plate → Gear` remains simple and readable, while the player can spend an unreasonable amount of time designing a beautiful 40-ton monstrosity that accomplishes it 7% more efficiently. That sounds substantially more promising as a source of depth than simply adding more intermediate recipes.