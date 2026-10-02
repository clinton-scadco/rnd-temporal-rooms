# Machine connection testbed

Run `./run.ps1 -Machine serve`, then open
[the testbed](http://127.0.0.1:8788/connections). The machine designer also has
a **connection testbed** link beside its plan/form buttons.

The 21 presets cover all seven routing domains: fluid, heat, gas, rotary,
mechanical, material and electrical. They include straight runs, tight spacing,
all four rotations, raised components, obstacles, shared outlets/inlets,
crossing pipes, rigid drives versus belts, and a jammed assembly with refused
routes. Each document uses the production Rust layout/routing/geometry passes
and the shared machine-view WebGL renderer.

To reproduce a drawing defect:

1. Choose a preset and inspect it from the top, front, side or isometric view.
2. Toggle **Pipes only**, **Route paths**, and **Port stubs**. Blue lines show
   the actual router centreline; amber lines show the required straight section
   leaving each solved port. These overlays are visible through equipment.
3. Change component coordinates, level or facing and apply, or edit/paste a
   `.machine` document and press **Rebuild**. Invalid edits clear the old render
   and show the build error. **Reset preset** restores the original document.
4. Click a connection row to highlight its geometry and restrict overlays to
   that connection. Click it again to show all connections.
5. Export the `.machine` document and diagnostics to retain a reproduction.
   Diagnostics include build settings, scene hash, instances, solved ports,
   semantic route corners, physical shared trunks, supports and spatial issues. Changes are local to this page;
   exporting downloads a file without overwriting designs in the repository.

**Clean**, **tight** and **lost** describe the router's result. They do not
certify that the pipe mesh is visually correct. A lost run has no pipe geometry
and no route centreline. **Check all presets** reports routing results for the
selected seed, style and look; visual inspection is still needed for gaps,
fitting overlap or incorrectly oriented elbows.

Verification:

```powershell
./run.ps1 -Machine check
# Or against an already running server:
node tests/connections_web.mjs 8788
# Real browser checks, with Chromium's debug port enabled:
node tests/connections_gl.mjs 8788 9228 target/connections-testbed.png
```

The API checks verify all presets, all routing domains, port/path agreement,
orthogonal rigid route sections (belts may span diagonally), refused routes without geometry, and exact LOD
prefix preservation when hiding equipment or enclosures. Browser checks render
every preset in all geometry modes and LODs, exercise camera/overlay controls,
edit positions, check failed rebuilds and run the preset batch.

Connection geometry regressions live in `tests/connection_geometry.rs`. They
check seated riser supports, raised equipment frames, solved casing axes,
shared split/merge tees and multi-branch trunks, flat belt loops and pulleys, unused turbine ports,
reactor lifting-arm mounting, and pipe clearance around stairs. The jammed
preset intentionally retains refused connections; their missing geometry is
reported in the connection list.
