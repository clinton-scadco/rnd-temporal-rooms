## Experiment 08: Procedural Machine Form

Build a standalone 3D prototype that takes a functional machine design and turns it into a plausible industrial object using procedural assembly.

The experiment is not about final art quality. It is about proving this pipeline:

```text
Machine Design
  ↓
semantic 3D layout
  ↓
connection routing
  ↓
structural inference
  ↓
procedural dressing
  ↓
renderable machine
```

The authoritative object remains the machine design. Generated geometry is disposable derived state.

### Core rule

> The generated mesh never defines the machine.

A machine is still:

```text
components
positions
orientations
typed ports
connections
properties
```

Rendering is:

```text
RenderGeometry = Generate(MachineDesign, VisualSeed)
```

Changing the visual generator must not change simulation behaviour.

## 1. Convert the machine designer to 3D

Use the existing power-machine component set first:

```text
Reactor / Heat Source
Heat Pipe
Heat Exchanger
Water Source
Steam Pipe
Tank
Turbine
Generator
```

Each component gets:

```text
bounding volume
orientation
typed sockets
mounting points
clearance volume
visual archetype
```

Example:

```text
Turbine
  body: cylindrical
  steamIn: fluid socket
  exhaustOut: fluid socket
  rotaryOut: axial socket
  mount: floor
```

The player places coarse functional volumes, not finished models.

### Placement

Use free 3D positioning with **semantic snapping**.

Compatible sockets should snap when brought near each other:

```text
Turbine.rotaryOut
        ↕
Generator.rotaryIn
```

Rotation and alignment should be inferred where possible.

Do not attempt full unrestricted CAD behaviour yet. Humanity already has CAD software and appears sufficiently punished.

## 2. Build a tiny authored asset library

Do not procedurally synthesize every detailed mesh.

Use procedural assembly from reusable authored assets.

Initial equipment assets:

```text
reactor vessel
heat exchanger
turbine body
generator body
tank
pump
```

Initial structural assets:

```text
beam
column
brace
platform
stairs
railing
concrete plinth
```

Initial connection/detail assets:

```text
flange
valve
pipe support
shaft coupling
bearing block
```

Around 20–30 authored meshes is enough for the experiment.

Use simple placeholders initially if necessary. The procedural system matters more than lovingly sculpted pressure gauges nobody can yet connect.

## 3. Procedural primitive geometry

Generate simple geometric structures directly rather than authoring every possible length.

Procedural candidates:

```text
pipes
shafts
beams
cables
ducts
platform floors
walls
foundations
simple tanks
```

For example:

```text
pipe(start, end, radius)
shaft(start, end, radius)
beam(start, end, profile)
```

Generate or stretch these from canonical geometry.

## 4. Connection routing

Implement automatic routing between compatible sockets.

For fluid/heat connections, prefer industrial-looking orthogonal routing.

Given:

```text
start socket
end socket
obstacles
```

find a path minimizing something like:

```text
distance
+ bend penalty
+ collision penalty
+ clearance penalty
```

A coarse 3D grid with A* is sufficient for the experiment.

Convert the resulting route into:

```text
straight sections
elbows
flanges
```

Different connection domains should produce different visual treatments:

```text
fluid      → pipe
heat       → insulated pipe
rotary     → shaft
electrical → conduit/cable
```

## 5. Structural inference

After equipment and connections exist, infer the structure required to support them.

Generate:

```text
foundations
equipment plinths
legs
columns
frames
bracing
platforms
stairs
railings
pipe supports
```

Use simple deterministic rules.

Examples:

```text
heavy floor equipment
→ concrete base

elevated equipment
→ support frame

service point above threshold height
→ access platform

long pipe span
→ pipe support every N metres
```

The goal is believable visual consequence, not structural engineering.

## 6. Basic enclosure generation

Add a simple optional housing pass.

A connected group of machinery may become:

```text
open skid
partially enclosed machine
industrial building
```

Begin very simply:

1. calculate the equipment bounds;
2. expand by clearance;
3. generate floor;
4. optionally generate walls and roof;
5. create openings where major pipes/conveyors pass through.

Do not attempt Tiny Glade-level architectural sophistication yet.

The test is simply whether the machinery can plausibly produce its own surrounding structure.

## 7. Deterministic visual generation

Every cosmetic choice must derive from a stable seed.

For example:

```text
VisualSeed =
    hash(
        designId,
        component layout,
        styleId,
        worldSeed
    )
```

That seed may choose:

```text
equipment variant
panel arrangement
minor pipe dressing
prop placement
wear variation
```

Given the same design and seed, generation must produce the same result.

This is important for multiplayer and saves:

```text
network:
  design + positions + seed

not:
  38 MB generated mesh
```

## 8. Materials

Use a small shared material library rather than unique textures per asset.

Initial set:

```text
painted steel
bare steel
dark structural steel
galvanized metal
concrete
copper/brass
insulation
rubber
```

Allow parameter variation such as:

```text
paint colour
roughness
wear
dirt
heat discoloration
```

Prefer trim sheets and tiled materials where possible.

## 9. LOD from the semantic model

Generate different visual complexity by viewing distance.

Example:

```text
close
  equipment
  pipes
  valves
  flanges
  supports
  railings
  small details

medium
  major equipment
  primary pipes
  structure

far
  simplified equipment forms

very far
  one installation proxy
```

The simulation representation must remain unchanged at every LOD.

## 10. GPU-friendly rendering

Treat repeated procedural pieces as instances rather than thousands of independent render objects.

Batch by:

```text
mesh + material
```

Good instancing candidates include:

```text
pipe elbows
flanges
beams
columns
railings
pipe supports
valves
```

The visual compiler should ideally output instance buffers rather than a giant scene of individually managed objects.

## Primary experiment

Build at least ten different power installations from the same component vocabulary.

Examples:

```text
compact reactor
wide multi-turbine plant
high-buffer plant
long-distance heat arrangement
multi-exchanger plant
oversized turbine system
```

Generate each procedurally.

Then test:

> With component labels hidden, can a viewer roughly understand the machinery's functional structure from its appearance?

Adding a turbine should visibly create consequences such as:

```text
new turbine body
new steam branch
shaft changes
additional support
platform expansion
```

Moving the generator should cause:

```text
shaft rerouting
support regeneration
clearance changes
```

The important property is **reactivity**, not photorealism.

## Secondary experiment: prove generality

Once power works, use the same rendering vocabulary for one mechanically different machine, preferably:

```text
Motor
  ↓ shaft
Gearbox
  ↓
Crusher
  ↓
Output Hopper
```

Add only the minimum new assets necessary.

If the same structural, routing and dressing systems generate something recognisably different from the power station, the experiment has succeeded at a much deeper level.

## Out of scope

Do not yet build:

```text
full game world
terrain
characters
vehicles
weather
realistic structural calculations
advanced fluid visuals
fully procedural detailed equipment meshes
WFC-driven machine generation
destruction
final lighting/art direction
```

WFC or similar approaches can later be tested for decorative infill such as wall panels, vents and surface detailing. It should not determine the machine's functional form.

## Success criteria

Experiment 07 succeeds if:

1. Functional machine layouts reliably generate plausible 3D industrial forms.
2. Connections visually communicate what components are connected.
3. Moving or changing a component regenerates nearby geometry coherently.
4. Generated geometry is deterministic.
5. The system works from a small reusable authored asset set.
6. Large amounts of visible complexity come from procedural arrangement rather than bespoke complete-machine models.
7. The renderer remains completely downstream of simulation semantics.

The thing being tested is ultimately:

> **Can the player's engineering design itself become the art direction?**

If the answer is yes, then machine variety stops being primarily a content-production problem and becomes an emergent consequence of how players build things.