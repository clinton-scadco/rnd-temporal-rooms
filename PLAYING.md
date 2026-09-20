# Playing the campaign

A walkthrough of Prototype 3 — the five rooms — written so that each part can
be tested in sequence. It is a *working* route rather than a good one: every
step is something the mouse can do, and nothing in it needs a design that is
not derivable from what the room gives you.

If you only read one section, read [Designing the miner](#2-designing-the-miner).
It is the first thing the game asks of you and the first thing it explains
badly.

```powershell
.\run.ps1 -Camp                 # the game, at http://127.0.0.1:8795
.\run.ps1 -Camp play            # the same campaign, played headlessly, in ~4s
.\run.ps1 -Camp map             # the rooms, the lanes, the fleets
.\run.ps1 -Camp tech            # the twelve components and where they come from
```

`-Camp play` is the reference route. Everything below is that script, performed
by hand. When something you build behaves differently from what is written
here, diffing against `src/camp/play.rs` is the fastest way to find out why.

---

## 0. The two rules that explain most confusion

**A placement is an empty box.** Every machine in the build palette — the
Extraction Head, the Compact Steam Plant, all of them — arrives with *nothing
inside it*. The palette's blurb ("108 MW out") describes what the book's design
does, not what you just placed. Until you open it and draw something, the
inspector says `nothing has been designed inside it yet` and it does nothing at
all.

**A room's objective is not a room's export.** Coal Basin is finished by selling
400 MW to the grid. The coal that Iron Valley, the Power Station, Manufacturing
and Final Works all run on is a *second*, entirely separate line you must build
in Coal Basin — heads on the big seam, into a bay, into the Delivery Depot. Skip
it and every later room starves with no error message anywhere.

---

## 1. First five minutes

1. `.\run.ps1 -Camp`, open <http://127.0.0.1:8795>, type a name, **Enter**.
2. The briefing card appears. **Start the campaign.** The clock now runs and
   never pauses again, in any of the five rooms.
3. You land on the **map**. Coal Basin is the only room that is not greyed out.
   Click it, then **go there** — or use the room buttons in the header.
4. You are in Coal Basin: a 40×40 plot with a **Grid Connection** at 30,2, a
   **Delivery Depot** at 30,8, and three hatched patches of ground on the left.

The hatched patches are the whole point. Hover them:

| ground | at | yields |
|---|---|---|
| coal seam | 2,2 (6×4) | 400 /s |
| coal seam | 2,7 (6×4) | 900 /s |
| water table | 2,12 (6×4) | 1,600 /s |

Nothing produces anything yet. The room ships you opportunities, not machines.

**Objective:** hold **400 MW/s for 45 seconds**, with the whole factory's
bounding box inside **480 tiles**.

---

## 2. Designing the miner

This is the part that stops people, so it is written out click by click.

### Place the chassis

1. In the left **build** panel, click **Extraction Head**. Its size reads
   `on ground` rather than a tile count, because its footprint will come from
   whatever you draw inside it.
2. Move over the coal seam at 2,2. The ground brightens and gets a solid border
   when what you are holding could work it.
3. Click at **2,2**. You now have `Head6`, 2×2, and the inspector says
   *nothing has been designed inside it yet*.

### Open the bench

4. With the head selected, press **design it** at the bottom of the inspector
   (the same button reads *open the machine* once there is something in it).
   The `machine` view opens.
5. Start drawing. The first edit takes the draft on its own — you do not have
   to ask for one. The machine that is running and the document you are editing
   are still different objects, and nothing outside changes until you press
   **commit design**; the bar says `· draft` while yours is open, and
   **discard** throws it away.

### Draw four inlets and two outlets

The right panel's **components** dropdown defaults to `everything`. You want
two entries from the `source` and `sink` headings:

| part | what it moves |
|---|---|
| **Material Inlet** | 100 /s of one solid, out of whatever is underneath |
| **Fluid Inlet** (pump) | 200 /s of one fluid |
| **Product Outlet** | 200 /s out, per phase (`solid` / `liquid` / `vapour`) |

Two material inlets saturate one outlet. That ratio is the entire arithmetic of
a mining head.

6. Click **Material Inlet**, then move the pointer over the 3D view. A box
   follows it, at the storey the bar says you are on: that is where the
   component will go. **Green** means the rules will take it, **red** means it
   overlaps something. Click four times, spread apart — a component is 2×2
   design tiles and they must not overlap.
   You should end up with `IN1 IN2 IN3 IN4`.
7. Click **Product Outlet**, place two: `OU1 OU2`.

*Keys in the bench:* **R** turns what you are holding — the tick on the ghost
says which way it will face — **E** / **PageUp** goes up a storey, **Q** /
**PageDown** comes down, **Esc** puts down whatever you are holding. Stay at
level 0 for this. The bar tells you what the pointer is for while you hold
something.

### Tell them what to draw

8. Click `IN1` in the 3D view. The **this design** panel now shows it, with a
   **tune** section at the bottom.
9. The **draws** dropdown reads **Iron Ore** — that is a Material Inlet's
   default, and it is wrong here. Set it to **Coal**.
10. Repeat for `IN2`, `IN3`, `IN4`.

> A Material Inlet is a *material* port and a Fluid Inlet is a *fluid* port.
> Setting an inlet to Water, or a pump to Coal, fails the check with
> `IN1 is a material inlet and Water is a fluid`. Water and Crude use the pump;
> Coal, Iron Ore and Iron use the inlet.

### Wire them

11. Press **wire** in the bar (or **wire** in the selected component's panel,
    which starts it from that one). Click `IN1`, then click `OU1`. Between the
    two clicks a line follows the pointer and every component that could take
    what `IN1` makes is boxed in blue; clicking one of those makes the
    connection. Where there is more than one legal pair of ports a menu asks
    which — here there is one, `out → OU1.solid`, so it is made on the click.
12. Same for `IN2 → OU1`, `IN3 → OU2`, `IN4 → OU2`. The wire tool stays on
    until you press **Esc** or press **wire** again.

    Connections you have made are drawn as blue lines between the components,
    and each component's panel lists its own under **connections** with a
    `×` to take one back.

### Commit

13. Press **commit design**. The machine changes at one canonical tick, on every
    client at once.
14. Press **keep on the shelf** and name it `Coal Head`. **Do this.** It is the
    difference between drawing six components once and drawing them in every
    room for the rest of the campaign.
15. **back to the room.**

The inspector now reads `2×2 tiles`, cycle `1.0s`, gives `400 Coal`. It also
still says *its coal has nowhere to go* — correct, and the next section fixes
it.

> **A wart to expect:** the inspector's "the machine inside" block lists
> `takes 400 Coal` as well as `gives 400 Coal`. Inside the machine an inlet is a
> boundary flow like any other; out here that boundary is the *deposit*. A head
> never needs coal delivered to it. Only the `gives` line is about the room.

### Now the water intake

Same procedure on the water table at **2,12**, with two differences: use **Fluid
Inlet** (pump) rather than Material Inlet, and wire `water → OUn.liquid`. Four
pumps and four outlets — one pump saturates one outlet — is 800 water/s. A
pump's default `draws` is already Water, so there is no tuning step.

Keep it on the shelf as `Water Intake`.

---

## 3. Coal Basin

With `Coal Head` and `Water Intake` on the shelf, everything below is placement.
Shelf designs live in the left panel under **from the shelf**; clicking one puts
it under the pointer already drawn.

### The power line — this is what finishes the room

| what | where | notes |
|---|---|---|
| Coal Head | 2,2 | the 400/s seam |
| Water Intake | 2,12 | the water table |
| Bay | 8,2 | coal |
| Bay | 8,8 | water |
| Bay | 24,2 | power |
| Compact Steam Plant ×4 | 14,2 · 14,5 · 14,8 · 14,11 | 3×2 each |

The Compact Steam Plant also arrives empty. Draw it once from the book —
`designs/03-compact.machine` — and shelf it:

```text
reactor   R1  at 0,0  throttle 40     # the tune that matters: 40, not 100
exchanger HX1 at 4,0   turbine T1 at 7,0   generator G1 at 10,0
exchanger HX2 at 4,3   turbine T2 at 7,3   generator G2 at 10,3
pump      W1  at 0,4

R1.heat  -> HX1.heat      R1.heat  -> HX2.heat
W1.water -> HX1.water     W1.water -> HX2.water
HX1.steam -> T1.steam     HX2.steam -> T2.steam
T1.rotary -> G1.rotary    T2.rotary -> G2.rotary
```

Six components, one tune, eight wires, and 108 MW on 40 coal + 160 water a
second. Shelf it as `Compact Steam Plant Mk1` and place the other three from
the shelf.

Wiring — the **connect** tool, or the port buttons on a selected building:

```text
Coal Head    → Bay(8,2)      Coal
Water Intake → Bay(8,8)      Water
Bay(8,2)     → each plant    Coal
Bay(8,8)     → each plant    Water
each plant   → Bay(24,2)     Power
Bay(24,2)    → Grid          Power
```

Four plants is 432 MW against a 400 MW bar, and the bounding box comes to
**384 tiles** against a 480 budget. The objective panel warms up over about
45 seconds and then latches.

### The coal export — do this too

Three more heads on the 900/s seam, side by side, into a bay, into the Depot:

```text
Coal Head at 2,7  ·  4,7  ·  6,7        400 + 400 + 100 = 900/s
Bay at 24,8
each head → Bay(24,8)  Coal
Bay(24,8) → Depot(30,8) Coal
```

The third head only gets the 100/s the seam has left — a seam is a budget, not
a socket, and the inspector says so. This costs nothing in footprint (still 384
tiles) and is what every other room burns.

Finishing Coal Basin hands over **motor, gearbox, clutch** and opens *both*
Iron Valley and the Power Station.

---

## 4. Iron Valley — 24,000 ore powder

112×112, and the constraint is electricity. The local coal seam is worth 35/s
and a compact plant burns 40.

**Open the coal lane first.** Map view → **shipping** → the `Coal Basin →
Iron Valley` lane → **Train**. 30,000 a load, 50 seconds each way. Nothing
arrives until the Basin's depot is actually shipping (§3).

| what | where |
|---|---|
| Ore Head (inlet ×4, `draws` **Iron Ore**, → solid) | 4,6 |
| Coal Head | 4,34 |
| Water Intake | 4,46 |
| Bay ore | 16,6 |
| Bay coal | 16,34 |
| Bay water | 16,46 |
| Yard power | 44,40 |
| Bay powder | 84,10 |
| Compact Steam Plant ×2 | 30,50 · 30,54 |
| Powder Line | 30,10 |

The Ore Head is your `Coal Head` with the inlets retuned — place it from the
shelf, open it, take a draft, change four `draws` to Iron Ore, commit, and shelf
that as `Ore Head`. It lifts 400/s in principle and 120/s in practice, because
that is what the seam has.

```text
Ore Head   → Bay ore        IronOre
Coal Head  → Bay coal       Coal
Intake     → Bay water      Water
Coal Yard(4,58)  → Plant A  Coal      # the import yard, fed by the train
Bay coal(16,34)  → Plant B  Coal      # the thin local seam
Bay water  → both plants    Water
both plants→ Yard power     Power
Bay ore    → Powder Line    IronOre
Yard power → Powder Line    Power
Powder Line→ Bay powder     OrePowder
Bay powder → Depot(100,10)  OrePowder
```

The Powder Line eats 135 ore and 270 MW every two seconds. Plant B runs at
about 87% forever, because 35 coal a second will not keep a 40-coal boiler
alight — that is the room, not a bug.

Hands over **separator, preheater, condenser**, which is what makes a Steam
Crusher placeable.

---

## 5. Power Station — 320 MW held for 45s

80×80. Water to spare, no fuel at all.

Open `Coal Basin → Power Station` on a **Train** first.

| what | where |
|---|---|
| Water Intake | 4,8 |
| Bay water | 20,8 |
| Yard power | 52,8 |
| Compact Steam Plant ×3 | 36,8 · 36,12 · 36,16 |

```text
Intake             → Bay water    Water
Coal Yard(4,20)    → each plant   Coal      # straight off the import yard
Bay water          → each plant   Water
each plant         → Yard power   Power
Yard power         → Grid(68,10)  Power
```

324 MW against a 320 bar. It is deliberately tight: the interesting failure is
not the average, it is what happens between trains. If the objective keeps
slipping, the yard is running dry — raise the lane's cap with **faster** in the
shipping panel, or add a fourth plant and a second bay.

Hands over **furnace, rollmill, press, crank** → the Stamping Line.

---

## 6. Manufacturing — 45 gears/s for 45s

72×72. Nothing is local except the billet.

Open two lanes: `basin → works` **Coal** by convoy, and `station → works`
**Power** by convoy.

| what | where |
|---|---|
| Caster Head | 4,8 |
| Bay billet | 20,8 |
| Stamping Line | 32,8 |
| Bay gear | 48,12 |

The Caster Head is four inlets set to **Iron** (not "IronBillet" — the
substance is `Iron`, and a fresh one is already in billet form) wired to two
outlets' `solid` ports. 400/s in principle, 120/s from this ground.

```text
Caster Head       → Bay billet     IronBillet
Bay billet        → Stamping Line  IronBillet
Coal Yard(4,20)   → Stamping Line  Coal
Power Yard(4,34)  → Stamping Line  Power
Stamping Line     → Bay gear       Gear
Bay gear          → Depot(60,12)   Gear
```

One stamping line makes 49 gears/s on 121 MW and 12 coal/s. Both of those
arrive in lumps from two rooms that are also busy. Watch **this room's supply**
on the right — if gears stall, it is a yard between trains, not the press.

Hands over **lathe**.

---

## 7. Back to Iron Valley — concentrate

Final Works is judged on concentrate, and nothing in the campaign makes any
yet. Walk back to Iron Valley (the powder line has been running the whole time)
and add the crusher the separator just unlocked:

| what | where |
|---|---|
| Ore Head | 4,20 (the second ore body) |
| Bay ore 2 | 16,20 |
| Steam Crusher | 30,20 |
| Bay concentrate | 84,24 |

```text
Ore Head(4,20)   → Bay ore 2       IronOre
Bay ore 2        → Steam Crusher   IronOre
Coal Yard(4,58)  → Steam Crusher   Coal
Bay water(16,46) → Steam Crusher   Water
Steam Crusher    → Bay concentrate Concentrate
Bay concentrate  → Depot(100,24)   Concentrate
```

The Steam Crusher makes its own power out of coal: 37 concentrate/s from 93
ore/s. It needs no grid connection, which is why it fits in a room whose
problem was electricity.

---

## 8. Final Works — the load that will not sit still

88×88, and the objective is four clauses at once:

```text
for 75 seconds:  never below 110 MW
                 over 380 MW at least once in every 10 seconds
                 above 240 MW for no more than 2 seconds in any 10
and:             30 gears/s and 20 concentrate/s together, for 45 seconds
```

Three lanes: `basin → final` Coal by train, `works → final` Gear by convoy,
`valley → final` Concentrate by convoy.

| what | where |
|---|---|
| Water Intake | 4,8 |
| Bay water | 20,8 |
| Yard power | 56,8 |
| Compact Steam Plant ×2 | 36,8 · 36,12 |
| Pulse Plant | 36,18 |

```text
Intake            → Bay water    Water
Coal Yard(4,20)   → both plants + pulse   Coal
Bay water         → both plants + pulse   Water
all three         → Yard power   Power
Yard power        → Grid(74,8)   Power
Gear Yard(4,34)   → Depot(74,22) Gear
Conc Yard(4,48)   → Depot(74,36) Concentrate
```

Two flat plants give a 216 MW floor — comfortably over 110, deliberately under
240. The Pulse Plant fills a buffer quietly and dumps 362 MW every seven
seconds, which clears 380 without ever sitting above 240 for long. Four plants
held wide open fail this: the surge is required *and so is the quiet between
them*.

The gears and concentrate are not made here at all. Both yards are wired
straight through to their depots — Final Works is judged on whether the rest of
your campaign is still running.

---

## Troubleshooting, by the exact words on screen

| the room says | it means |
|---|---|
| `nothing has been designed inside it yet` | an empty chassis. Open it, take a draft, draw something, commit. |
| `it is standing on a coal seam and is not designed to draw coal` | the head's inlets are set to the wrong substance. Retune `draws`. |
| `it is not standing on anything` | the head missed the ground, or a redesign grew its footprint off the seam. |
| `its coal has nowhere to go` | the output is not wired to a bay or a machine. |
| `its coal is wired to 2 bays` | one output, one destination. Delete a wire. |
| `nothing delivers coal to a bay wired into it` | the depot or grid has a bay but nothing fills it. |
| `IN1 is a material inlet and Water is a fluid` | inlet vs. pump. Material Inlet takes Coal / Iron Ore / Iron; Fluid Inlet takes Water / Crude. |
| `an Extraction Head has to stand on ground, and there is none here` | you clicked off the hatching. |
| `that does not fit there` | overlap, or off the plot. Note that committing a bigger design *grows* a machine, so a head that was 2×2 can collide with its neighbour. |
| `a stamping line is refused: …` | its parts are not unlocked. Check the map view's **components** list. |
| `Iron Valley is not open yet` | its prerequisite room has not been finished. |
| a lane shows `n would not fit in the yard` | the destination yard is full. Lower the cap, or build more capacity behind the yard. |

Deleting leaves a **ghost** you can restore. It restores the building; whether
it restores every connection depends on whether the route is still free, and
the toast says which.

---

## Making testing bearable

**Use the shelf ruthlessly.** Design each of these exactly once, keep it, and
place from the shelf ever after:

```text
Coal Head              4 material inlets (coal)  → 2 outlets.solid
Ore Head               the same, draws iron ore
Caster Head            the same, draws iron
Water Intake           4 pumps → 4 outlets.liquid
Compact Steam Plant    designs/03-compact.machine
```

Copies remember where they came from, so a variant is a derived design rather
than a rewrite.

**Skip drawing entirely while testing a later room.** The server exposes the
book's designs at `/api/reference`, and a placement command may carry a design.
From the browser console, in the room you are standing in:

```js
const seat = localStorage.getItem('temporal-rooms/seat');
const me   = await (await fetch('/api/enter', {method:'POST',
  headers:{'content-type':'application/json'},
  body: JSON.stringify({key: seat, back: true})})).json();

const ref  = await (await fetch('/api/reference')).json();
const pick = (proto, draws) =>
  ref.designs.find(d => d.proto === proto && (!draws || d.draws === draws)).design;

const put = (proto, x, y, design) => fetch('/api/cmd', {method:'POST',
  headers:{'content-type':'application/json'},
  body: JSON.stringify({code: me.at, player: me.player, type: 'PlaceMachine',
    payload: {proto, x, y, face: 0, design}})}).then(r => r.json());

await put('head', 2, 2, pick('head', 'Coal'));       // an already-drawn head
await put('steamplant', 14, 2, pick('steamplant'));  // and a working plant
```

`ref.designs` carries every stock machine plus heads for `Coal`, `Water`,
`IronOre`, `IronBillet` and `Crude`. This is a *test* shortcut — the palette
will never do this, on purpose — but it turns "check whether Final Works can be
passed" from forty minutes of drawing into two.

**Read `-Camp play` when something disagrees.** It plays all five rooms in about
four seconds and prints what it built, what moved between rooms, what got
unlocked, and whether every replica agreed. If your room will not finish and the
script's does, the difference is in `src/camp/play.rs`.
