# Prototype 2: Multiplayer Vertical Slice

Build the first small but complete multiplayer vertical slice of the game.

This prototype should combine the systems already proven separately:

- deterministic Room simulation
- machine-from-parts design
- 3D machine authoring
- spatial world placement
- continuous simulation
- goals
- multiplayer command replication

The purpose is no longer to prove another compression trick.

The question is:

> Can two players continuously build and redesign a deterministic factory together, in real time, while the simulation keeps running and both clients remain exactly synchronized?

## 1. Core gameplay loop

The prototype flow should be:

```text
Host creates Room
        ↓
goal selected from good preset templates
        ↓
simulation starts immediately
        ↓
second player joins using Room code
        ↓
both players build simultaneously
        ↓
design machines in 3D
        +
place machines in world
        ↓
connect logistics
        ↓
observe production
        ↓
redesign / expand while clock keeps running
        ↓
complete goal
```

The game never pauses during normal play.

Machine design, world construction, inspection and multiplayer interaction all happen while simulation time continues advancing.

## 2. Simulation tick rate

Standardize gameplay simulation at:

```text
60 ticks = 1 second
```

Define:

```text
SIM_TICK_RATE = 60
```

Keep simulation logic integer/tick based.

Gameplay definitions should preferably use human-facing durations:

```text
process time: 2 seconds
source period: 1 second
transport time: 15 seconds
```

Compile these into:

```text
120 ticks
60 ticks
900 ticks
```

Do not expose arbitrary old experimental tick values directly to players.

Resources should remain integer quantities wherever practical.

For example:

```text
10 IronOre every 1 second
```

becomes:

```text
10 IronOre every 60 ticks
```

Do not simply multiply every old prototype value by 60 without considering gameplay pacing.

## 3. Continuous clock

The Room begins advancing as soon as the host starts the game.

There is no normal pause control.

The UI may still support developer/debug pause and timeline inspection outside normal gameplay, but the multiplayer game loop assumes:

```text
Room clock continuously advances
```

If players are redesigning something, the rest of the factory continues running.

Buffers may drain.

Resources may continue arriving.

Poor designs may continue wasting capacity.

The player is solving problems inside a live system.

## 4. Construction semantics: place or delete

Remove dragging/moving of committed world objects.

Construction uses discrete actions:

```text
Place
Delete
Connect
Disconnect
```

A committed machine or component does not move.

To relocate something:

```text
delete old object
place new object
```

This rule applies both to:

- machines in the world
- components inside the 3D machine designer

Before placement, allow normal placement assistance:

```text
ghost preview
rotation
snapping
collision indication
connection preview
valid/invalid placement feedback
```

Once placed, position and orientation become historical state.

This keeps player commands discrete and multiplayer synchronization simple.

## 5. Recently deleted ghosts

When a player deletes an object, leave a temporary translucent ghost at its previous location.

Suggested initial behavior:

```text
ghost lifetime: 5–10 seconds
```

The ghost should show:

- what was deleted
- where it existed
- which player deleted it if useful
- a Restore action

Restore should issue a new placement command. It is not a rollback of simulation time.

Example:

```text
tick 12,100: DeleteComponent(Generator42)
tick 12,180: Restore → PlaceComponent(copy of Generator42)
```

Do not build the full historical ghost timeline yet.

Preserve enough construction history that a future version could display previous states, but keep this prototype to recent deletion ghosts.

## 6. Multiplayer authority model

Use host-authoritative Room simulation.

Player A selects:

```text
Host Room
```

The game creates a short Room code:

```text
ABCD12
```

Player B selects:

```text
Join Room
ABCD12
```

No accounts, matchmaking, dedicated-server infrastructure or persistent cloud world are required for this prototype.

The host is authoritative for:

```text
Room clock
command ordering
simulation state
goal state
canonical hashes
```

Clients submit intentions.

For example:

```text
PlaceMachine
DeleteMachine
PlaceComponent
DeleteComponent
CreateConnection
DeleteConnection
CommitMachineDesign
```

The host:

1. validates the command;
2. assigns the canonical simulation tick;
3. assigns deterministic ordering when several commands occur at the same tick;
4. applies it;
5. broadcasts the canonical command.

All clients replay the same command stream.

## 7. Command structure

Use explicit deterministic commands.

Conceptually:

```text
Command {
    roomId
    tick
    sequence
    playerId
    type
    payload
}
```

Ordering should be fully deterministic:

```text
tick
then sequence
```

Do not allow browser arrival order or frame timing to decide simulation semantics.

Typical commands:

```text
PlaceMachine
DeleteMachine

PlaceComponent
DeleteComponent

CreateConnection
DeleteConnection

CommitMachineDesign

PlaceStorage
DeleteStorage

CreateWorldLink
DeleteWorldLink
```

Do not synchronize mouse movement or drag state.

Ghost previews remain purely client-side until committed.

## 8. Join-in-progress

Player B must be able to join after the Room has already been running.

The host sends:

```text
canonical snapshot @ tick X
+
command stream after X
+
goal state
+
required design/world document state
```

The joining client reconstructs the Room and advances to the current canonical point.

Then compare canonical hashes.

Joining must not restart or pause the host simulation.

This is one of the primary technical proofs of the prototype.

## 9. Periodic synchronization checks

Continue using canonical deterministic hashes.

At regular intervals:

```text
host hash
client A hash
client B hash
```

should match for the same canonical Room state.

If a client differs:

```text
identify mismatched Room/domain
resend authoritative snapshot
replay subsequent commands
```

Do not resynchronize the entire application if only one deterministic simulation region differs.

The first implementation may use whole-Room correction if necessary, but retain the architecture for smaller-scope resync later.

## 10. The world layer

The original factory simulation becomes the world-building layer.

Players can place physical installations such as:

```text
source
storage
custom machine
sink / delivery point
transport connection
```

Do not expose arbitrary population controls such as:

```text
x100000
```

to normal players.

The player places physical installations.

The compiler may still lower equivalent objects into population classes internally.

The distinction remains:

```text
GAME DOCUMENT
physical objects + positions
        ↓
compiler
        ↓
SIMULATION IR
population classes + regions + channels
```

Population compression remains an implementation detail.

## 11. World placement

World construction should also use place/delete semantics.

Provide:

```text
placement ghost
rotation
snap rules where appropriate
collision feedback
connection preview
```

Physical footprint matters.

A machine should have:

```text
position
orientation
bounds
connection points
```

Links derive distance from the spatial layout where applicable.

Do not allow the user to type arbitrary link distance for normal gameplay.

## 12. 3D machine designer

Players can enter the 3D designer for a custom machine.

The designer supports:

```text
place component
delete component
rotate before placement
snap compatible ports
create connection
delete connection
inspect component
```

Do not support dragging existing committed components.

If a player wants a component elsewhere:

```text
delete
place again
```

This should match the world construction model.

### Component placement

Use semantic snapping.

Components expose:

```text
typed ports
position
orientation
solid volume
clearance volume
mount rules
```

Examples of connection types:

```text
material
fluid
gas
heat
rotary
electrical
```

Connections must obey compatibility rules.

## 13. Machine design editing is draft-based

Editing an already-running machine must not mutate its live internal simulation piece by piece.

When the player opens a placed machine:

```text
live machine continues running
```

The player edits a separate draft design.

Example:

```text
LIVE
SteamPlant v1
continues operating

DRAFT
remove turbine
add larger turbine
reroute steam
```

When the player presses:

```text
Commit Design
```

send one authoritative command:

```text
CommitMachineDesign(machineId, newDesign)
```

The host applies the replacement at a canonical tick.

This avoids needing semantics for partially dismantled live machines.

## 14. Machine replacement semantics

For the prototype, commit should replace the machine design atomically.

At tick T:

```text
old design ends
new design begins
```

Define a simple deterministic state-transition rule.

Prefer something conservative initially, for example:

- external buffers remain where possible;
- compatible stored resources transfer;
- incompatible internal state is discarded or returned according to one explicit rule;
- the new machine begins from a deterministic initial internal state.

Do not attempt physically realistic reconstruction downtime yet unless it becomes necessary for gameplay.

The replacement itself must be reproducible from the command log.

## 15. Machine instances and versions

Editing one machine does not mutate every machine copied from the same original design.

For the prototype:

> Every placed machine owns its current machine design.

Duplicating a machine copies its current design.

After that:

```text
Machine A
Machine B
```

are independent.

Editing Machine A does not alter Machine B.

You may expose a display name/version such as:

```text
Compact Steam Plant v3
```

but do not build automatic blueprint propagation yet.

A proper reusable design library can come later.

## 16. Simultaneous editing

Both players must be able to operate at different abstraction levels at the same time.

Example:

```text
Player A
editing internals of GeneratorPlant

Player B
placing storage and transport in world
```

Neither activity pauses the other.

Both ultimately emit commands into the same authoritative Room history.

For the first implementation, avoid two players editing the same machine draft simultaneously.

Use a lightweight machine-edit lock:

```text
Player A is editing SteamPlant
```

Player B may inspect it but not open an editable draft until Player A commits or exits.

This is much cheaper than inventing collaborative 3D CRDT engineering because apparently this project does have limits.

## 17. Goals

Do not generate completely arbitrary goals.

Create approximately:

```text
15–30 goal templates
```

that are manually authored to represent potentially interesting factory problems.

Initial families:

### Delivery

```text
Deliver X Gears
Deliver X Steel
Deliver X ProductA + Y ProductB
```

### Sustained throughput

```text
Maintain X Gears/sec for Y seconds
```

### Power

```text
Sustain X MW for Y seconds
```

### Efficiency

```text
Produce X output using no more than Y water
```

or:

```text
Waste less than X% heat
```

### Space

```text
Reach X throughput inside footprint Y
```

### Mixed production

```text
Maintain X ProductA and Y ProductB simultaneously
```

Goal templates may parameterize:

```text
quantity
required rate
time window
resource availability
starting budget
starting technology
```

## 18. Goal randomization

When the host creates a Room:

```text
RoomSeed
GoalSeed
```

are selected.

The goal generator chooses from the good preset templates and picks values within manually defined valid ranges.

Example:

```text
template:
Sustain Gear production

rate range:
8–15/sec

duration:
30–60 sec
```

Do not allow unconstrained random values.

The generated goal must be deterministic from the seed.

All clients receive the same goal.

Provide a developer option to force a specific seed/template for debugging.

## 19. Goal begins with the game

The goal is visible before players start meaningful construction.

Suggested flow:

```text
Host Room
      ↓
goal generated
      ↓
show objective
      ↓
Start Room
      ↓
clock runs continuously
```

Once started, do not pause for planning.

Players are expected to design while time advances.

## 20. Resource pacing for 60 Hz

Rebalance prototype resources for 60 simulation ticks per second.

The game should not visually or economically feel like quantities are changing sixty times faster merely because simulation resolution increased.

Prefer definitions such as:

```text
Mine:
  30 Ore / second

Smelter:
  consumes 10 Ore
  process time 2 seconds

Train:
  capacity 1000
  round trip 20 seconds
```

Compile these into integer ticks.

Where fractional rates are needed, express them through deterministic batches rather than floating-point accumulation.

Example:

Instead of:

```text
0.5 Ore / tick
```

use:

```text
30 Ore every 60 ticks
```

or another exact integer schedule.

## 21. Player feedback

The live nature of the simulation means failure must be understandable quickly.

Clicking a world machine should show:

```text
status
current inputs
current outputs
utilisation
starved / blocked reason
next meaningful event
```

Example:

```text
Smelting Plant

STARVED
needs 50 IronOre
available 0

next ore delivery:
3.2 seconds

upstream rail:
100% utilised
```

Inside a custom machine, expose component-level diagnostics.

Example:

```text
Turbine

STARVED
steam demand: 80/s
available: 56/s
utilisation: 70%
```

Use seconds in the normal UI; keep exact ticks available in developer/debug views.

## 22. Multiplayer presence

Keep multiplayer presence lightweight.

Show:

```text
other player cursor
player name / color
current selection
machine currently being edited
```

Cursor/presence state does not need to be deterministic or part of the Room simulation.

Do not confuse:

```text
ephemeral presence
```

with:

```text
authoritative construction command
```

Cursor updates can be lossy and high frequency.

Commands cannot.

## 23. Timeline/history

Keep the deterministic history architecture.

For normal gameplay, expose only:

```text
recent deleted ghosts
recent construction events
```

Do not yet expose full timeline scrubbing during a live multiplayer Room.

The workbench/debug mode may retain full timeline inspection.

The simulation should continue retaining enough history/snapshots for:

```text
replay
late join
desync diagnosis
future historical ghost view
```

## 24. Win state

When the goal conditions are satisfied:

```text
Goal Complete
```

record:

```text
completion tick
elapsed real simulation time
resource usage
final throughput
machine count
custom machine count
```

Do not immediately stop the Room.

Allow players to continue observing/building after completion.

The result screen can summarize the run, but the simulation may keep advancing.

## 25. Primary multiplayer test

Run this exact scenario:

```text
Player A hosts
goal generated
clock begins

Player A starts machine design

Player B joins late using code

Player B constructs world logistics

Player A commits machine design

Player B places that machine

both continue changing factory

one player deletes/rebuilds something

goal completes
```

At multiple probe ticks:

```text
Host state
==
Player A state
==
Player B state
```

including:

```text
world objects
machine designs
simulation populations
storage levels
pending transports
goal progress
region state
canonical hash
```

## 26. Failure tests

Deliberately test:

```text
two commands same tick
rapid place/delete
disconnect + reconnect
join during active machine editing
delete machine another player is inspecting
commit design while world state changes
invalid connection request
late arriving client command
client hash mismatch
```

The game should resolve all of them deterministically or explicitly refuse the command.

No undefined “who clicked first according to whichever packet arrived first” behavior.

## 27. What not to build yet

Do not add:

```text
accounts
public matchmaking
dedicated servers
cloud persistence
voice chat
large progression tree
combat
full campaign
recursive custom machines
blueprint auto-propagation
collaborative editing of one machine draft
full historical ghost timeline
final visual asset library
advanced fluids
maintenance/wear
complex electricity simulation
```

These may be valuable later.

They do not answer the question this prototype exists to answer.

## Success criteria

Prototype 2 succeeds if:

1. One player can host and receive a Room code.
2. A second player can join an already-running Room.
3. The simulation runs continuously at 60 ticks/sec.
4. Both players can place/delete world installations.
5. Both players can create/remove connections.
6. Players can design machines in 3D using place/delete semantics.
7. Machine editing happens in a draft while the live machine continues operating.
8. A committed machine redesign becomes one deterministic command.
9. Players can work at machine and world scales simultaneously.
10. A randomized-from-curated-template goal gives the Room a concrete objective.
11. Late join/reconnect reproduces the authoritative state.
12. Canonical state hashes remain identical across host and clients.
13. Completing the goal feels like the result of designing and operating a factory, rather than merely increasing a population field.

The central proof is:

> **A continuously running deterministic factory can be collaboratively constructed at multiple scales by multiple players, with only discrete player commands synchronized between them.**

If that works, the underlying technical thesis has survived contact with an actual game rather than another increasingly elaborate laboratory apparatus.