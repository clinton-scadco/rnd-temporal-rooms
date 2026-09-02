The playtest notes are actually pretty coherent. They look like 16 separate complaints, but most of them collapse into **three underlying problems**:

1. the factory still feels too much like an abstract graph,
2. Room boundaries and logistics aren't legible enough,
3. multiplayer recovery is still prototype-grade.

I would **not add more progression content yet**. I'd do a focused **P3.1 physicalisation pass** first, because several current abstractions are fighting the game you're discovering.

The biggest design change I'd make is this:

> **Machines expose real external ports derived from their internals. Bays become optional storage infrastructure, not mandatory adapters between everything.**

That addresses a ridiculous number of your notes at once.

## 1. Stop forcing everything through bays

Right now the player effectively builds:

```text
Source
  ↓
Bay
  ↓
Machine
  ↓
Bay
  ↓
Machine
```

Logically immaculate. Delightful if your target audience is warehouse middleware.

Instead, a custom machine should derive its external connections from the components inside it.

Suppose inside a crusher you have:

```text
Ore Hopper → Crusher → Output Chute
                ↑
              Motor
```

The compiled machine might expose:

```text
Crusher Mk3

INPUT
  IronOre      material port

INPUT
  Electricity  electrical terminal

OUTPUT
  CrushedOre   material port
```

Then in the Room:

```text
Mine ──belt──► Crusher ──belt──► Smelter
                  │
                power
                  │
               Substation
```

No mandatory bays.

Bays then become things you build **because you want storage**:

```text
Mine ─► Ore Warehouse ─► Crusher
```

or:

```text
Crusher ─► Buffer ─► Train Terminal
```

That makes storage a strategic tool rather than punctuation required by the parser.

### Different connection domains should behave differently

I'd stop treating all connections as vaguely equivalent wires.

At minimum:

```text
MATERIAL
belt / conveyor

FLUID
pipe

ELECTRICAL
cable / bus

BULK TRANSPORT
rail / truck route
```

Especially power. Electricity should absolutely not travel through a bay.

A generator exposes:

```text
ElectricalOut
```

and consumers expose:

```text
ElectricalIn
```

They connect into an electrical network.

Power distribution can initially be extremely simple. Don't accidentally spend September implementing load-flow equations because a wire looked at you funny.

---

# 2. Mines and water should be resource sites, not magical sources

I agree strongly with note 1.

Don't place a magical:

```text
IRON SOURCE
```

Instead the Room contains a **resource site**:

```text
Iron Deposit
quality / capacity / extraction properties
```

The player builds a machine on or beside it:

```text
Iron Deposit
     ↓
Extractor
     ↓
IronOre
```

And crucially, **the extractor itself can use the machine designer**.

Maybe its required functional capability is:

```text
Resource Extractor requires:
  extraction head
  mechanical drive
  material output
```

Then players might construct:

```text
Electric Motor
     ↓
Gearbox
     ↓
Mining Head
     ↓
Hopper
```

Water behaves similarly:

```text
Water Source
     ↓
Intake / Pumping Station
```

requiring perhaps:

```text
intake
pump
power
fluid outlet
```

Now extraction becomes another machine-design problem rather than a special exception.

That's much more consistent with your whole game.

## I would generalise this into `Site`

A Room can contain immutable environmental features:

```text
Site {
    type
    properties
    bounds
}
```

Examples later:

```text
Ore Deposit
Water Source
Geothermal Vent
Oil Field
Gas Deposit
Sunlight zone?
```

Machines interact with Sites through compatible components.

So the world provides **opportunities**, not free outputs.

---

# 3. Kill prebuilt machines

Your playtest answered that experiment.

> **Prebuilt machines remove the interesting part of the game.**

So don't make them the main progression mechanism.

What I'd retain is a **player design library**.

Once someone creates:

```text
SteamPlant Mk4
```

they can reuse it.

That's fundamentally different from the game saying:

> Here's the Steam Plant. Place three.

Progression becomes:

```text
unlock components
      ↓
design machine
      ↓
save design
      ↓
reuse design
      ↓
later improve it
```

You can have tutorial examples or ghost/example designs if desperately necessary, but I wouldn't put finished machines into the normal tech unlock tree.

Unlock **parts and capabilities**, not answers.

---

# 4. Room input/output needs its own first-class UI

Notes 7, 9 and 15 are really one problem.

Players don't understand what crosses Room boundaries.

That cannot remain implicit in a preplaced yard.

I'd give every Room a permanent **I/O panel**.

Something like:

```text
IRON VALLEY

IMPORTS
Power
  required       24 MW
  available      31 MW
  from           Coal Basin
  status         ✓ supplied

EXPORTS
Iron Ore
  current        82/s
  destination    Industrial Hub
  queued         4,200
  blocked        0
```

And in the actual world, Room boundary connections should have a distinctive visual representation.

Perhaps the edge of the map contains **gateway connections**:

```text
          TO INDUSTRIAL HUB
                 ↓
═══════ RAIL EXPORT ═══════
```

not necessarily a fake warehouse pretending to be the edge of reality.

Then when coal can't be delivered:

```text
Coal Basin → Iron Valley

BLOCKED
12,000 coal waiting
destination import full
next train cannot unload
```

You should be able to click that warning and have the relevant route highlighted.

Nothing should silently disappear.

Ever.

If the destination cannot accept it, it remains:

```text
at source
in transit
or waiting at destination
```

and the UI tells you which.

---

# 5. Connections need to become physical objects now

Your note 12 is probably the other major design decision.

I think you've reached the point where abstract SVG lines should die in the normal Room view.

Connections should:

- have a route;
- consume space;
- avoid collisions;
- use orthogonal/appropriate routing;
- visually communicate their transport domain;
- display the item/fluid they carry;
- originate at actual machine ports.

For example:

```text
Machine
   └─ material output socket
          │
          └──────┐
                 │ belt
                 └────► machine input
```

Auto-routing is fine. In fact I'd keep it automatic initially.

Player action:

```text
select output
select destination
```

Game:

```text
find valid orthogonal route
show ghost
commit
```

If there is no valid path:

```text
NO ROUTE
```

rather than drawing through the neighbouring turbine because topology needed emotional support.

### Connection appearance should encode payload

If it's iron:

```text
IronOre
────────►
```

show icon/label on hover and perhaps sparse moving visual samples.

A one-item bay or machine port should infer the item automatically.

So note 10 becomes:

> Never ask a question whose answer is already uniquely determined.

If a source only contains IronOre, connecting it shouldn't ask "which item?"

---

# 6. Improve the inspector substantially

This is cheap compared with the value.

Hover should populate the inspector immediately.

Click can pin it.

I would show this for every building:

```text
GEAR PRESS

STATUS
Working
72% utilisation

INPUTS
Iron Plate
required       2 / cycle
available      48
incoming       12/s
current need   satisfied

POWER
required       4 MW
available      3.6 MW
status         constrained

OUTPUTS
Gear
production     5.4/s
buffered       16
downstream     connected
```

For connections:

```text
IRON BELT

carries        IronOre
capacity       30/s
current        24/s
utilisation    80%
latency        1.8 s
```

For inter-Room connections:

```text
POWER IMPORT

Coal Basin → Iron Valley
available      31 MW
requested      24 MW
```

This is fundamental game usability, not polish.

---

# 7. Restore should restore the local structure

Deleting and restoring only the building is technically consistent and experientially obnoxious.

When deleting, capture a **tombstone**:

```text
DeletedBuilding {
    building definition
    position
    design
    connections[]
}
```

Restore attempts:

```text
restore building
restore each previous connection
```

If something has changed and a connection can no longer be restored:

```text
Building restored
2/3 connections restored

Steam line could not be restored:
route occupied
```

Do not silently do half the job.

---

# 8. Make connection tools contextual

I agree with the floating palette.

A permanent side-menu tool list is becoming inappropriate now that the game has semantic objects.

Click/hover/select a machine:

```text
CONNECT

IronOre OUT  ●
Steam OUT    ●
Power IN     ○
Water IN     ○
```

Click `IronOre OUT`, compatible targets highlight.

Likewise selecting an empty input:

```text
Water IN
```

highlights nearby compatible outputs/storage/network.

The global floating toolbar can just expose:

```text
Build
Connect
Delete
Inspect
Machine Designs
```

rather than containing seventeen increasingly obscure wire types below the fold, where UI controls traditionally go to die.

---

# 9. Fix multiplayer recovery as a subsystem, not individual bugs

Notes 4, 13 and 14 are all telling you the networking model needs a proper **session/rejoin protocol**.

I would do this now before adding more game state.

### Persistent player identity

Generate once:

```text
playerId = UUID
```

store in localStorage.

On refresh:

```text
RoomCode
PlayerId
lastKnownCommandSequence
lastKnownTick
```

go back to host.

Host recognizes:

```text
same player
```

rather than adding a new participant.

### Heartbeat

Yes, add one.

But don't use it merely as:

```text
I'm alive
```

Have the host regularly send:

```text
Heartbeat {
    authoritativeTick
    latestCommandSequence
    canonicalHashCheckpoint
}
```

Client can determine:

```text
I am 300 ticks behind
I am missing command 812
my state hash disagrees
```

Then:

```text
catch up
request missing commands
or request authoritative snapshot
```

### Room switching

Switching Rooms should not tear down the actual multiplayer session.

Think:

```text
SESSION
  Room 1
  Room 2
  Room 3

Client views Room 2
```

Changing Room is essentially changing the **view/subscription**, not navigating away from the game.

That should eliminate a whole family of bizarre browser lifecycle failures.

---

# 10. Goal state needs to be live, not latched

Note 8 sounds like the goal implementation may still have traces of:

```text
condition became true
→ DONE
```

when the UI expects continuous evaluation.

I'd distinguish:

```text
ACHIEVEMENT GOAL
"produce 10,000 gears"
once achieved, stays achieved
```

from:

```text
STATE REQUIREMENT
"Coal Basin must export 100 MW"
currently true/false
```

and:

```text
SUSTAINED REQUIREMENT
"export 100 MW for 60 seconds"
progress accumulates while true
resets/decays when false
```

Room completion itself can remain permanent if that's the game rule.

But the inspector should continue showing:

```text
Room completed ✓

Current requirement:
Power export 64/100 MW ⚠
```

Otherwise players can't tell that their supposedly successful factory has since collapsed into industrial tragedy.

---

# What I'd do next

I wouldn't call this another Five Rooms content pass.

I'd make **Prototype 3.1: Physical Factory Pass**.

Its job is to fix the abstraction layer exposed by play.

The headline changes:

```text
Sites
  ↓
player-designed extraction machines

Machines
  ↓
derived physical ports

Ports
  ↓
direct physical connections

Storage
  ↓
optional infrastructure, not mandatory glue

Room boundaries
  ↓
explicit import/export network

Connections
  ↓
physical routed objects

Multiplayer
  ↓
persistent session + heartbeat/rejoin
```

Then replay **the same Five Rooms**.

Don't add new Rooms.

Don't add twenty more components.

Don't improve progression yet.

See whether the same scenario suddenly feels less like operating a graph editor and more like constructing an industrial system.

Because the biggest conclusion from this playtest is not “we need prettier visuals.”

It's:

> **The simulation abstraction is still leaking through the game UI.**

You built bays, sources, sinks, wires and Rooms because they were excellent mathematical objects.

Now the game is telling you which of those should remain internal concepts and which deserve physical manifestations.

That's exactly the kind of information a vertical slice is supposed to painfully extract from you.