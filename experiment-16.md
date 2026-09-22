## Experiment 16: Combat / Active Disturbance Visual Prototype

**Goal:** prove combat can coexist with the compressed deterministic simulation without producing an obscene serialized event stream or requiring the whole world to run explicitly.

Keep this extremely narrow.

Build one small defensive encounter:

```text
Factory
   ↓
approach route
   ↓
enemy wave
   ↓
walls + turrets
```

No freeform RTS armies yet. Civilization may survive without 8,000 independently pathfinding goblins for one more sprint.

### Combat simulation

Use:

```text
deterministic paths / nav nodes
enemy cohorts where possible
event-driven weapons
analytical projectile trajectories
local explicit combat domain
```

Example event sequence:

```text
t=1200 cohort enters range
t=1218 turret volley fires
t=1232 volley impacts
t=1232 cohort splits:
       healthy x80
       damaged x20
t=1260 next movement completes
```

Do not serialize those derived events.

Serialize only:

```text
player commands
wave seed / scenario input
occasional snapshots/checkpoints
```

### Visual requirement

This test absolutely needs something visible.

Render:

```text
enemy cohorts moving along routes
turret rotation
muzzle flashes
projectile tracers / shells
impacts
damage state changes
destroyed structures
```

But those are rendering consequences of compact simulation state.

A projectile can be represented as:

```text
origin
target
fireTick
impactTick
```

and interpolated visually.

### Show compression explicitly

Add a debug overlay:

```text
COMBAT DOMAIN

Nominal entities: 12,480

Compressed state:
Enemy cohorts      14
Turret populations  6
Projectiles         38
Unique structures  22

Events processed/s: ...
```

That matters because this experiment isn't only about whether explosions look amusing.

It needs to prove:

> thousands of nominal combatants do not necessarily mean thousands of independently simulated actors.

### Disturbance boundary

Only the affected region wakes into explicit/event simulation.

Outside it:

```text
factory regions
→ continue through normal compressed solver
```

When combat finishes:

```text
topology stabilises
damage states settle
        ↓
recompile affected region
        ↓
collapse back into normal populations/orbits
```

Also test one destroyed belt/power connection and confirm only the necessary local topology is invalidated.

### Success criteria

Run increasingly large waves:

```text
100
1,000
10,000
100,000 nominal attackers
```

and measure runtime against **distinct cohorts/states**, not nominal unit count.

The visual scene does not need to draw 100,000 individual models. Use representative rendering/instancing.
