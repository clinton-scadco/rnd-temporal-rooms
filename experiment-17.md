## Experiment 17: World Fields and Industrial Ecology

**Goal:** prove that spatial world properties can become a general simulation layer influencing factories without simulating every microscopic environmental object.

Build a Room with a coarse environmental grid.

Start with:

```text
Temperature
Moisture
Contamination
Biomass
```

plus one static resource layer:

```text
IronRichness
```

Something like:

```text
WorldCell {
    temperature
    moisture
    contamination
    biomass
    ironRichness
}
```

The environmental grid should be much coarser than building placement.

For example:

```text
construction resolution: ~1 m
field resolution:         ~16–32 m
```

### Static field test

Use iron richness.

Instead of a discrete ore node:

```text
IronRichness[x,y]
```

A mining machine samples the richness beneath its footprint.

Conceptually:

```text
output =
base extraction
× average richness under machine
```

Extraction gradually reduces richness.

Now resource placement becomes genuinely spatial.

### Dynamic field test

Add a few machines with simple environmental effects:

```text
Furnace
+ heat
+ contamination

Cooling Tower
- heat
+ moisture

Scrubber
- contamination
+ power demand

Forest / biomass patch
affected by heat/moisture/contamination
```

Biomachines are **optional** for proving the system.

But include one if you want to test whether undesirable environmental state can become useful:

```text
Fungal Processor

likes:
warm
wet
contaminated
```

Then the optimal factory might deliberately create a nasty little swamp behind the refinery.

A touching achievement for industrial civilization.

### Field evolution

Do not update every field at 60 Hz.

Use appropriate cadences.

For example:

```text
temperature:       4–10 updates/sec
moisture:          1 update/sec
contamination:     1 update / 5 sec
biomass:           1 update / 10 sec
```

Those numbers are experimental, not sacred.

Use deterministic fixed-point maths.

Heat can use simple neighbour diffusion rather than general matrix multiplication:

```text
new =
current
+ neighbour diffusion
+ machine sources
- ambient decay
```

### Environmental classes

Compile continuous values into bands:

```text
Temperature:
COLD / NORMAL / HOT / EXTREME

Moisture:
DRY / NORMAL / WET

Contamination:
CLEAN / DIRTY / TOXIC
```

Then construct an environmental signature:

```text
HOT + WET + TOXIC
```

Machines only change simulation behaviour when their signature changes.

That preserves compression.

Instead of:

```text
1,000,000 machines all recomputed continuously
```

you might have:

```text
720,000 NORMAL
210,000 HOT
70,000 HOT+TOXIC
```

### Visual requirement

Add debug overlays toggled individually:

```text
Heat
Moisture
Contamination
Biomass
Iron richness
```

Render them as heatmaps over the Room.

Also show environmental values in the building inspector:

```text
LOCAL ENVIRONMENT

Temperature      67°C / HOT
Moisture         81% / WET
Contamination    72 / TOXIC

Machine effect:
Throughput       82%
Cooling demand   14 MW
```

That will immediately tell you whether these fields are legible enough to become gameplay.
