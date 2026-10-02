# Experiments 14–16: back to the factory

The camp is the campaign spine; the machine designer is the engineering workspace. The slice keeps its existing region dock, inspector, shipping controls, palette and cooperative room. New experiments extend those interactions.

## 14 — construction that can be read

The designer's **era workshop** (`/eras`) compares the three shipped ore lines side by side, with pictures generated from their actual documents. Open either Plan or Form directly from an example. Timber frames have heavy beam sections and bracing; cast iron has cast beds and ribs; fabricated steel keeps the existing frame vocabulary. Water wheels have wooden paddles and spokes, and steam engines have cylinders, piston rods and flywheels. Ports, footprint and simulation remain design authority.

The shared live bench in camp, room and slice now offers **Plan / Form**, construction material controls, and **belt / shaft** connections. Materials can coexist inside a machine. Draft edits still leave the installed machine running until commit.

## 15 — one spatially fractured world

There is one 216 × 72 valley. The three regions own disjoint, stepped patches of it, with global coordinates and crossings on their shared edges. The overview and region view draw the same landscape. Buildings remain visible in neighbouring patches, and shipping is drawn across the physical crossings. A region selection focuses the camera and its tools.

Placement and redesign validate the entire footprint against that patch. Changing era does not reveal another playable copy of the same coordinates. The old face table remains an authoring source for local terrain and the command-line history study; it no longer defines overlapping browser worlds.

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

The integrated slice playthrough exercises damage, repair and replica agreement. `tests/realigned.rs` covers unique spatial ownership, material-dependent structure with unchanged interfaces, the factory encounter, and belt preservation through draft commands. Existing era, form, routing, replay and browser checks remain applicable. The existing camp headless campaign test still fails at its Power Station objective; this predates the realignment.
