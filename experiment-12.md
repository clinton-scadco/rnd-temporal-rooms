The next question should be:

> **Does finishing one factory make me want to start the next one?**

I’d make the next milestone **Prototype 3: progression across Rooms**.

The critical change is that a Room stops being a disposable challenge and becomes one part of a larger game.

### The loop I’d test

```text
Enter Room
   ↓
inspect local problem/resources
   ↓
build machines + factory
   ↓
meet production objective
   ↓
Room becomes productive
   ↓
gain resource / capability / component
   ↓
move to another Room
   ↓
reuse or redesign previous machines
   ↓
new problem requires something different
```

This tests whether all the cool machinery you’ve built produces an actual long-term factory game.

## 1. Add a tiny persistent world

Not a planet. Not procedural galaxies. Human ambition can wait outside.

Maybe **5 Rooms** connected in a fixed graph:

```text
Iron Valley ──────┐
                  ▼
             Industrial Hub ─────► Advanced Works
                  ▲
Coal Basin ───────┘
                  │
                  ▼
             Power Station
```

Each Room has:

- available resources
- physical dimensions/layout
- import/export points
- one meaningful constraint
- objectives
- unlocked technologies

Completing one should change what can be done elsewhere.

That finally gives the Room abstraction gameplay meaning rather than merely computational meaning.

## 2. Make transport between Rooms real

You already have the simulation machinery for this.

Now expose it to players.

For example:

```text
Iron Valley

produces:
Iron Ore

Rail export
   ↓ 45 sec
Industrial Hub
```

The player isn't synchronising those factories. They're establishing actual supply relationships.

This lets you finally exploit your batch logistics at game scale:

```text
Train:
10,000 ore every 90 seconds

Ship:
200,000 every 20 minutes
```

A Room can continue operating while nobody is there.

That is a potentially defining feature.

You leave your iron mine, work somewhere else for twenty minutes, come back, and the exact deterministic factory has simply continued doing what you designed it to do.

## 3. Introduce the design library properly

I think this should now become a major mechanic.

After designing:

```text
Compact Steam Generator Mk3
```

save it.

Later:

```text
My Machines

Compact Steam Generator Mk3
Crusher Mk2
Low-Water Boiler Mk5
Gear Press Mk1
```

Then another Room says:

> Water supply is severely limited.

Suddenly your previous generator isn't useless, but it isn't optimal either.

Copy it:

```text
Compact Steam Generator Mk3
        ↓
Low-Water Generator Mk1
```

Modify heat recovery, exchanger capacity, cooling, whatever your abstraction permits.

**That is the payoff for machine-from-parts.**

You shouldn't constantly force people to design machinery from nothing.

The interesting loop is:

> design → reuse → discover deficiency → improve → reuse again.

## 4. Technology should unlock design possibilities

This is the next thing I'd prototype aggressively.

Avoid:

```text
Research:
Smelting +10%
```

Instead:

```text
Unlock:
High-pressure turbine

Unlock:
Compact electric motor

Unlock:
Counterflow heat exchanger

Unlock:
Centrifugal separator

Unlock:
High-temperature pipe
```

Those unlock **new machine topologies**.

Then progression isn't merely numbers going upwards. Your toolbox becomes richer.

That creates a lovely reason to revisit old designs.

You unlock a new motor and think:

> I could make my old crusher half the size now.

Factory-game brains apparently consider this a recreational activity.

## 5. Give Rooms different constraints

This is probably the single most important gameplay experiment.

Don't just change:

```text
produce 100 gears
```

to:

```text
produce 500 gears
```

Change the **problem**.

For example:

**Room A: abundant space, weak power**

```text
cheap land
limited electricity
```

**Room B: tiny platform**

```text
very constrained footprint
lots of power
```

**Room C: distant resources**

```text
long logistics latency
large train batches
```

**Room D: scarce water**

```text
water recycling matters
```

**Room E: unstable demand**

```text
100 MW normally
400 MW peak every 60 seconds
```

Now the same machine isn't optimal everywhere.

That's essential.

Otherwise players eventually solve “the best generator” and your elaborate machine designer quietly becomes a recipe selector with extra clicks.

## 6. Introduce one real byproduct/recycling chain

This is where I'd finally broaden recipes slightly.

Something like:

```text
Ore
 ↓
Smelting
 ├─ Metal
 └─ Slag
```

Slag could initially be dumped.

Later:

```text
Slag
 ↓ separator
valuable mineral + waste
```

Or power:

```text
Steam
 ↓ turbine
Low-pressure steam
 ↓ condenser
Water
```

This gives machine designs loops and secondary outputs without immediately entering Pyanodons' Department of Administrative Chemistry.

It also starts testing whether player-designed machines become genuinely interesting when a transformation isn't purely linear.

## 7. Add consequences for poor design, but avoid maintenance chores

I would **not** jump into machine breakdown/wear yet.

There's enough consequence already from:

```text
wasted input
blocked output
insufficient buffer
excess heat
poor efficiency
large footprint
slow startup
```

Those are engineering consequences.

“Click wrench every 17 minutes” is administrative punishment masquerading as simulation.

Maintenance can come later if it creates design tradeoffs rather than chores.

---

# The prototype I'd actually build

Call it:

## Prototype 3: The Five Rooms

Five small authored Rooms.

Maybe:

```text
1. Mine
   teaches extraction + transport

2. Power
   requires custom power-machine design

3. Foundry
   imports ore + power
   creates metal

4. Manufacturing
   imports metal
   produces components

5. Final Works
   combines several products
   demands sustained output
```

Allow two players to move freely between them.

All Rooms continue running continuously.

Machines and designs persist.

Inter-Room shipments continue whether players are present or not.

Give maybe **8–12 unlockable components** over the whole prototype.

Completion should take perhaps **60–120 minutes**, not ten.

That's enough to answer the real question:

> **Does the player develop attachment to their factory network and machine designs, and does gaining new capabilities make them voluntarily go back and improve old systems?**

If yes, then you've found the game.

If no, adding recursion, fluids, combat, a trillion machines and seventeen varieties of lovingly modeled flange is unlikely to fix the underlying loop.

I would resist random generation for this prototype too. Make the five Rooms **hand-authored and deliberately nasty**. Once you know what constitutes a good problem, *then* teach procedural generation to produce one.