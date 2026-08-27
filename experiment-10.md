## Experiment 10: 3D Machine Authoring

Move the **functional design itself** into 3D.

Not full CAD. Keep it constrained and semantic.

The player should be able to:

- place components in X/Y/Z;
- rotate them;
- stack them;
- snap compatible ports;
- drag a connection between ports;
- move a component and watch its attached routing regenerate;
- see collisions and required clearances;
- optionally ask the system to auto-place/auto-route, but manually override the result.

The fundamental document becomes:

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

Your simulation still doesn't care about coordinates except where geometry determines things such as distance.

That solves the thing you originally wanted: **the player chooses whether the turbine sits beside the exchanger, above it, or on a separate level.**

### Then make the routing actually trustworthy

This is where I'd put most of the next effort.

From the screenshot, pipe generation has reached the point where visual errors will actively undermine the mechanic. It needs rules like:

```text
socket direction
minimum straight section before bend
allowed bend radius
pipe diameter
clearance from equipment
clearance between pipes
preferred elevations
support spacing
junction rules
```

And critically, routing should be allowed to fail:

> “No valid route found.”

That is better than generating nonsense.

A route could roughly be:

```text
port
 ↓ short straight
 ↓ rise to pipe-rack level
 → horizontal run
 ↓ descend
 → destination port
```

Rather than arbitrary shortest-path spaghetti.

Industrial routing has conventions. Exploit them. They make the output more believable *and* constrain the search space.

I'd probably introduce **routing layers/elevations** too:

```text
ground services
low pipe rack
high pipe rack
overhead cable tray
```

That alone would clean up a lot of the scene.

## Make ports much richer

Right now a connection probably knows "steam pipe goes from here to there."

I'd make the port tell the visual system much more:

```text
SteamOutlet {
    position
    direction
    diameter
    domain = fluid
    pressureClass
    preferredRouteHeight
}
```

Then the renderer knows a pipe should leave the vessel normally from its flange, not immediately pull a 90-degree turn through the equipment shell because geometry is apparently optional.

Same for shafts:

```text
RotaryPort {
    axis
    diameter
    maxAngularMisalignment
}
```

Shaft connection rules should be much stricter than piping.

This is part of a bigger point:

> **The procedural generator should understand interfaces, not merely endpoints.**

## I'd also add collision/clearance volumes now

Not for realistic engineering. For game rules.

Components should have:

```text
solid volume
service clearance
hot clearance
connection exclusion zones
```

So you can show:

```text
green = valid
red = collision
yellow = service access blocked
```

Then spatial optimisation becomes gameplay.

Otherwise 3D placement just becomes the same easy `x100000` problem with objects stacked vertically until the entire factory resembles a lasagne.

This gives you meaningful constraints:

- equipment cannot overlap;
- some parts need maintenance access;
- hot objects need separation;
- shafts need alignment;
- big vessels need foundation contact;
- pipes need routes.

That's where the 3D designer starts becoming fun rather than merely prettier.

## Then add automatic structural dressing

Keep improving:

- stairs;
- platforms;
- railings;
- supports;
- foundations;
- pipe racks.

But treat these as **derived from the player's layout**.

For example:

```text
serviceable port > 2m high
→ platform required

platform has no ground access
→ stairs generated

pipe run > 6m
→ support required
```

You've already got the beginnings of this in the render. I'd make it more rigorous before changing the visual assets.

## What about the assets?

I wouldn't ignore them completely.

I'd make **one small polished asset pack**, maybe:

- one good motor/generator;
- one good horizontal vessel;
- one good turbine;
- one good pump;
- one valve;
- one flange;
- improved platform/railing materials.

Put them into the current scene.

That answers an important question:

> Is the procedural system fundamentally ugly, or are the placeholder meshes simply hiding its potential?

But don't spend weeks producing the asset library yet.

I'd call this a **visual spike**, not the next phase.

You want to establish:

```text
better asset + current generator
= obviously better
```

If so, great. Stop. Return to interaction and geometry.

### My priority now would be:

**1. 3D authoring**
The player controls Z and orientation.

**2. Correct connection geometry**
Ports, bend radii, routing rules, collisions.

**3. Spatial gameplay constraints**
Footprint, clearance, access, stacking.

**4. Derived structural geometry**
Platforms, supports, stairs, pipe racks.

**5. Small art-quality spike**
A handful of real meshes/materials.

**6. Only then a proper asset-production pipeline.**

Because right now your graphics question is no longer:

> “Can this system create an industrial scene?”

The screenshot answers that with a pretty comfortable **yes**.

The important question is now:

> **Can a player manipulate this industrial scene directly, and does the procedural system reliably turn their functional 3D decisions into geometry that makes physical visual sense?**

If you prove that, investing in good meshes becomes safe. And once those assets arrive, the visual quality can jump dramatically without another fundamental rewrite.

I'd actually consider the current level of visual roughness ideal for that next experiment. It's ugly enough that nobody will confuse rendering defects with game design defects, which is a rare gift in development.