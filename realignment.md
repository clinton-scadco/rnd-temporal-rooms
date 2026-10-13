# Experiments 14–16: back to the factory

The camp is the campaign spine; the machine designer is the engineering workspace. The slice keeps its existing region dock, inspector, shipping controls, palette and cooperative room. New experiments extend those interactions.

## 14 — construction that can be read

The designer's **era workshop** (`/eras`) compares the three shipped ore lines side by side, with pictures generated from their actual documents. Open either Plan or Form directly from an example. Timber frames have heavy beam sections and bracing; cast iron has cast beds and ribs; fabricated steel keeps the existing frame vocabulary. Water wheels have wooden paddles and spokes, and steam engines have cylinders, piston rods and flywheels. Ports, footprint and simulation remain design authority.

The shared live bench in camp, room and slice now offers **Plan / Form**, construction material controls, and **belt / shaft** connections. Materials can coexist inside a machine. Draft edits still leave the installed machine running until commit.

## 15 — one spatially fractured world

There is one 216 × 72 valley. The three regions own disjoint, stepped patches of it, with global coordinates and crossings on their shared edges. The overview and region view draw the same landscape. Buildings remain visible in neighbouring patches, and shipping is drawn across the physical crossings. A region selection focuses the camera and its tools.

Placement and redesign validate the entire footprint against that patch. Changing era does not reveal another playable copy of the same coordinates. The playable landscape has 29 individually positioned features: western seams and a broad river, a northern industrial district with a vertical canal, and an eastern campus with a culvert and depleted ground. Roads, buildings and resources are not shifted copies of the historical face table. That table remains only for the command-line history study.

## Building from components

The slice has one **Build** tool. Click the ground to place a blank works plot; the designer opens immediately in Plan. Its Start here panel offers Extract / Process / Store / Ship as guides on the same plot. They select no building type and place no prefabricated layout.

For a miner: Build on a seam → Mining drill → Product outlet → connect the drill output to the solid input → Commit. On water, use Water intake pump and the outlet’s liquid input. Belt and pipe intakes request delivered goods; they do not extract from the ground. Mining can share a plot with processing and its drive.

A storage design connects Belt / Pipe intake → Bulk container / Liquid tank → Product outlet, Sorter outlet or Shipping dock. Intakes and sorters select actual goods, including concentrate and gears. Connected bulk containers add 20,000 capacity each; tanks add 12,000. Loading interfaces have finite budgets, shared across mixed goods and outside connections. Adding connected interfaces changes world throughput, including belt and rail loading. A shipping dock can fulfil orders directly from the designed store.

Containers on one plot pool their inventory. Separate plots are currently needed for independent stocks. Persistent pooled storage applies to logistics layouts; containers inside a processing design remain process buffers. Shipping docks fulfil orders in logistics layouts. The region’s fixed arrival/shipping infrastructure and legacy prototypes remain for saves and campaign scenarios; player construction in the slice uses the shared plot designer.

## 16 — a disturbance in the factory

The normal Experiment 16 browser entry opens the factory slice. Ore delivered through the deep crossing accumulates strain. With an ore feed connected in the district, 40,000 imported units trigger one threatened connection and a 25-second warning. Inspect the feed, close the ore route or disconnect it to prevent impact. Otherwise that feed breaks; repair restores it. A 120-second recovery period prevents repeated attacks from overwhelming the ordinary building loop.

The overlay uses one approach marker, a dashed route and a target outline. Only the affected room receives an ordinary connection command; other regions continue running. This is a small integration pilot, not the original turret/cohort combat model: that engine, deterministic replay checks and scale studies remain available in the explicit **lab**.

## Run

```powershell
.\run.ps1 -Machine serve       # designer; open its era workshop
.\run.ps1 -Camp                # campaign with the shared Plan/Form bench
.\run.ps1 -Slice               # spatial valley and local disturbance, :8797
.\run.ps1 -Combat              # same integrated factory entry, :8800
.\run.ps1 -Combat lab          # original combat diagnostic scene
.\run.ps1 -Combat play         # original compact simulation acceptance run
```

The integrated slice playthrough exercises damage, repair and replica agreement. `tests/realigned.rs` covers unique spatial ownership, material-dependent structure with unchanged interfaces, the factory encounter, and belt preservation through draft commands. `tests/works.rs` checks designed extraction, mixed storage, tanks, shipping, inventory/replay and loading throughput. Existing era, form, routing, replay and browser checks remain applicable. The existing camp headless campaign test still fails at its Power Station objective; this predates the realignment.
