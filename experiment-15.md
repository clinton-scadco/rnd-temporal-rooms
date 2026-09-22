## Experiment 15: Temporal Fracture World Slice

**Goal:** prove the setting creates meaningful factory problems without requiring the full late-game time manipulation system.

Build a tiny world of perhaps **three connected regions** representing different historical phases.

Something like:

```text
1890 Mining Valley
        │
        │ natural fracture
        ▼
2037 Industrial District
        │
        │ route
        ▼
2070 Manufacturing Zone
```

Each should visibly and mechanically belong to its era.

### 1890 Mining Valley

Provide:

```text
iron deposit
river
timber
coal
limited modern infrastructure
```

Natural machine solution tends toward:

```text
water wheel
wooden framing
line shafts
steam
```

But do **not** forbid modern equipment.

A player may import a modern motor if they can actually get it there and support it.

### 2037 Industrial District

Provide:

```text
electrical grid
modern logistics
better machine components
exhausted local ore deposit
```

This should naturally become the processing centre.

### 2070 Manufacturing Zone

Provide a limited set of more advanced components or processes.

The exact date is less important than showing:

```text
same world
different industrial capability
```

### Add temporal material origin

Items carry:

```text
originPhase
```

At this stage, keep temporal compatibility simple.

For example:

```text
same-phase logistics
    = normal

cross-fracture logistics
    = requires temporal interface
```

Do not yet build arbitrary player-created fractures or local phasing.

### Add temporal geography visibly

Crossing a fracture should make the world change clearly:

```text
vegetation
roads
buildings
resource state
industrial infrastructure
```

Same approximate geography, different history.

One useful test:

```text
1890:
rich iron deposit

2037:
deposit exhausted
factory standing above it
```

You don't yet need to let the player phase that factory away.

Just prove the player understands:

> this is the same place at a different time.

### Success criteria

The test succeeds if players naturally do something like:

```text
mine ore cheaply in 1890
        ↓
move it through temporal logistics
        ↓
process with better 2037 machinery
```

and if bringing modern machinery backward feels possible but expensive rather than arbitrarily locked.

This validates the premise:

> **technology is constrained by industrial capability and logistics, not by a magical era lock.**