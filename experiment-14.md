## Experiment 14: Era Machines and Physical Operating Limits

**Goal:** prove that machine design becomes more interesting when components have material, energy and thermal properties, and that old-style machinery is mechanically different rather than cosmetically antique.

Start with one small production problem:

```text
Extract ore
→ crush ore
→ move crushed ore
```

Build it first with an early-industrial component set:

```text
wooden frame
water wheel
line shaft
pulley
belt drive
flywheel
mechanical pump
crusher
hopper
```

Then offer a second technology family:

```text
cast-iron frame
steam engine
steel shaft
gearbox
better bearings
```

And finally a small modern set:

```text
electric motor
steel frame
direct drive
fan / radiator
```

The important part is that these are **different machine topologies**, not:

```text
Wood Crusher Mk1
Steel Crusher Mk2 +20%
```

### Add core machine properties

Every component should expose some subset of:

```text
energy domain
power demand / output
torque
speed / RPM
heat generation
thermal mass
optimal temperature range
maximum temperature
structural material
vibration tolerance
```

Keep the simulation coarse enough to remain understandable.

For temperature, use behavioural bands rather than continuous recalculation:

```text
COLD
NORMAL
WARM
HOT
OVERHEATED
```

A crusher might behave like:

```text
NORMAL        100%
WARM           95%
HOT            75%
OVERHEATED      0%
```

The field can retain exact/fixed-point temperature internally, but machine behaviour should change only at thresholds.

### Cooling should be a design choice

Provide several solutions:

```text
passive air cooling
fan
water loop
heat exchanger
larger frame / spacing
better material
```

Make waste heat reusable.

For example:

```text
Steam Engine
    ↓ waste heat
Dryer
```

or:

```text
Furnace
    ↓
Heat Exchanger
    ↓
Boiler feedwater
```

The success test is:

> Can two valid machines perform the same function but have genuinely different layouts, power systems, thermal behaviour and failure modes?

Also test whether upgrading from a water-wheel/line-shaft factory to electric motors feels like a **change in industrial architecture**, not merely a stat increase.