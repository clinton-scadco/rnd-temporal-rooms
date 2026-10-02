# Machine look study

## Rounded casting kit pass

The kit now has 30 meshes and 12 materials. `Volute` adds a reusable closed
scroll casting for pumps and future blowers: an asymmetric tangential shoulder,
rounded axial edges and smooth cast-shell normals, with flat cover faces.
Its 612 triangles stay inside the canonical instance envelope and the existing
800-triangle limit. Both renderers and OBJ exports consume the same geometry.

Full-detail `Pump` and `MechPump` now use the casting with smaller bolted access
covers, their existing separate drives, bedplates and guards. Fluid connection
necks overlap the casting's conservative inner core, while external shafts
remain attached to the drive housing. Joint weathering is enabled consistently
in the software and WebGL renderers. Earlier grades, footprints, ports and
simulation continue to use the existing design.

Latest previews under `artifacts/machine-look/`: `pump-volute-studio.png`,
`pump-volute-rear.png`, `mechpump-volute-studio.png`, `kit-volute-studio.png`,
`refinery-volute.png` and `refinery-volute-studio.png`. Older assembly and
material studies remain in the gallery.

Verification: 76 targeted Rust tests passed across assembly, geometry,
readability, routing, surface filtering and shadow/contact occlusion. Affected
geometry, palette and stair-clearance checks were rerun after the final shell
normal and material updates. The DOM suite passed, followed by real WebGL
checks for both `10-refinery.machine` and `22-radiator.machine`: shader
compilation, shadow framebuffer, pump selection, all LODs and shaded readback.
Chrome used SwiftShader because its sandboxed GPU process could not start in
this environment. Browser screenshots are `browser-refinery-volute.png` and
`browser-mechpump-volute.png`.

```powershell
.\run.ps1 -Machine form designs/10-refinery.machine --unit P1 --studio --png artifacts/machine-look/pump-volute-studio.png
.\run.ps1 -Machine form designs/22-radiator.machine --unit MP1 --studio --png artifacts/machine-look/mechpump-volute-studio.png
.\run.ps1 -Machine kit --studio --png artifacts/machine-look/kit-volute-studio.png
```

The component assembly pass starts with pumps. At full detail, `Pump` has a
compact bolted process casing, a smaller finned drive housing, an exposed
coupling under an open yellow guard, separate seats on the bedplate, a pressure
gauge and a lifting eye. `MechPump` uses a rear bearing housing and its existing
external drive rather than gaining an electric motor. Fluid necks attach to
the process casing; drive necks attach to the rear housing and remain steel.
The earlier grades, logical footprints, interfaces and simulation remain intact.

The earlier component pass used 29 meshes. The rounded volute described above
is now the thirtieth. Flexible hose geometry and a perforated guard remain
candidates for repeated use across component families. Nameplates, lifting eyes, fasteners and instrument
brackets can first be assembled from the existing kit.

New previews: `pump-component-studio.png`, `pump-component-rear.png`,
`mechpump-component-studio.png`, `refinery-components.png` and
`refinery-components-studio.png` under
`artifacts/machine-look/`. Earlier material-only previews are retained.

The component pass passed 68 targeted assembly, geometry, readability and
routing tests, followed by the DOM and real WebGL checks. The pump regression
also checks casing height, aligned drive axes, process-side fluid connections
and the distinction between internal and external drives.

The reference is `fuel-refinery-preview-static.png`: a dense industrial asset,
with worn enamel, machined steel edges, bolted joints and strong depth between
layers. This pass improves the existing generated machinery and its renderers.

The refinement pass removes the broad, high-contrast paint blotches. Damage
is sparse and concentrates at casing joints; small chips fade with their pixel
footprint instead of becoming large patches at a distance. Grime is a faint
vertical stain. Concrete and galvanised zinc have quieter colour variation.
Both renderers now add a broad sky/ground reflection to steel and a softer
dielectric sheen to enamel. These are analytic reflections, not an environment
map or full indirect lighting.

Vessels have closed manway covers, narrow head seams and external level
instruments with two shell taps and retaining collars. The added hardware is
dressing; the logical ports, routes and simulation are unchanged. Small exposed
metal fittings retain their material through the paint pass.

PNG exports sample at twice the output width and height, then average in linear
light. Thin railings, grating and collars retain coverage. Diagnostic renders
keep their original sampling so existing pixel measurements remain comparable.
The browser already requests multisample antialiasing. Studio framing aims
lower on a wide yard to keep the factory's visible mass nearer the image centre.

Latest comparisons: `refinery-refined.png`, `refinery-refined-studio.png`, and
the `*-refined-studio.png` component close-ups in `artifacts/machine-look/`.
The earlier renders remain available in the gallery for comparison.

```powershell
.\run.ps1 -Machine form designs/10-refinery.machine --style yard --pitch 0.75 --zoom 0.7 --png artifacts/machine-look/refinery-refined.png
.\run.ps1 -Machine form designs/10-refinery.machine --style yard --studio --pitch 0.65 --zoom 0.76 --png artifacts/machine-look/refinery-refined-studio.png
.\run.ps1 -Machine form designs/10-refinery.machine --style yard --unit CO1 --studio --png artifacts/machine-look/column-refined-studio.png
```

The earlier rendering pass used 29 meshes and 12 materials. Large barrels, domes, cones
and bends use 48 sides; every canonical mesh remains below 800 triangles.
Boxes and cylinder ends have bevels. Articulated rotating machinery has steel
rim collars and fastening circles on its end bells.

Both renderers have a filtered sun shadow map, warm direct light, cool ambient
light, metal highlights, local paint chips, oxide and dirt streaks. Gauge faces
have contrasting dials and needles. The software preview also has contact
occlusion. Surface grain uses the existing shared mipmapped texture array;
macro wear follows component-local metres and is filtered at distance.
The browser reuses its shadow map while orbiting and picking, and rebuilds it
when geometry or detail level changes. Haze no longer washes out a large plant
just because the camera was moved back to fit it.

`--studio` makes a 1600 × 1200 preview against charcoal, with a tighter camera
and no haze. `--unit NAME` isolates an existing component with its mounting
frame and refits the bounds. Both switches are inspection options: they do not
change the design, interfaces, routes or simulation.

```powershell
# Existing kit, with the new rendering.
.\run.ps1 -Machine kit --png artifacts/machine-look/kit-after.png
.\run.ps1 -Machine kit --studio --png artifacts/machine-look/kit-studio.png

# Installed machine close-ups.
.\run.ps1 -Machine form designs/10-refinery.machine --unit P1 --studio --png artifacts/machine-look/pump-studio.png
.\run.ps1 -Machine form designs/12-onemotor.machine --unit MO1 --studio --png artifacts/machine-look/motor-studio.png
.\run.ps1 -Machine form designs/03-compact.machine --unit T1 --studio --png artifacts/machine-look/turbine-studio.png

# Whole plant, or browser editor.
.\run.ps1 -Machine form designs/10-refinery.machine --style yard --pitch 0.75 --zoom 0.7 --png artifacts/machine-look/refinery-after.png
.\run.ps1 -Machine serve
```

Open `artifacts/machine-look/index.html` for the reference, before/after
plant views and component close-ups. The earlier PNGs are retained for comparison.

The reference remains ahead in authored shape complexity, layered hoses and
auxiliary hardware, high-resolution surface detail and reflected illumination.
This is a stronger procedural kit, not a claim of matching that asset's fidelity.

The refinement passed 74 targeted Rust tests: assembly, geometry, readability,
routing, surface filtering and the two shadow/contact-occlusion regressions.
The browser DOM checks and real WebGL shader/framebuffer/picking/LOD checks also
passed. The new surface regressions require calmer broad enamel, concentration
of wear at joints, and fading of subpixel chips.

Verification includes the geometry, assembly, routing, material and readability
suites, plus sun-shadow and contact-occlusion regressions. `tests/machine_gl.mjs`
checks the real WebGL shaders, framebuffer, picking and all LODs using a running
machine server and Chromium debug port; the existing DOM checks do not compile
shaders. The full suite has a campaign power-objective failure already documented
in the README; that simulation issue is outside this visual pass.

The 320px readability comparison allows 5% variation in occupied colour bins:
filtered shadows and the smaller seated casings can move a bin across its
pixel-coverage threshold. It still checks palette retention, repaint chroma,
and comparable screen coverage. The grey grade omits macro weather so it
remains a useful material baseline. All 83 targeted tests passed.
