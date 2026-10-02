// Actual machine documents: every preset goes through /api/open and /api/form.
const fixture = (id, title, domain, note, body, expected = {}) => ({
  id, title, domain, note, expected,
  source: `machine "Connection ${id}"\nbrief power\n\n${body.trim()}\n`,
});
const fluid = (x, y, face = 'east', z = 0) => `
pump P1 at 0,0 face east
preheater H1 at ${x},${y},${z} face ${face}
wire P1.water -> H1.in`;

export const CASES = [
  fixture('straight', 'Straight fluid run', 'fluid', 'Two inline valves should connect directly. Inspect the flange seating and short straight section.', `
valve V1 at 0,0 face east
valve V2 at 5,0 face east
wire V1.out -> V2.in`, { straight: true }),
  fixture('short', 'Close neighbours', 'fluid', 'A short gap exposes oversized flanges, reducers and overlapping fittings.', fluid(2, 0)),
  fixture('offset', 'Small lateral offset', 'fluid', 'Look for unnecessary loops when the flange positions are nearly aligned.', fluid(6, 1)),
  fixture('elbow', 'Quarter turn', 'fluid', 'Inspect the bend entry, exit and continuity from above and from the side.', fluid(5, 4, 'south')),
  fixture('reverse', 'Facing away', 'fluid', 'A reversed tube inlet forces the pipe to approach from the far side.', fluid(6, 0, 'west')),
  fixture('north', 'North-facing inlet', 'fluid', 'The fourth rotation helps expose sign errors in elbow orientation.', fluid(5, 4, 'north')),
  fixture('rise', 'Raised receiver', 'fluid', 'A six-metre rise tests vertical elbows, stubs and the connection to a raised component.', fluid(6, 0, 'east', 3)),
  fixture('obstacle', 'Equipment in the corridor', 'fluid', 'The line must avoid the reactor body; compare the path overlay with the pipe mesh.', `${fluid(8, 0)}\nreactor BLOCK at 3,0`),
  fixture('shared-out', 'One outlet, two branches', 'fluid', 'Inspect the shared outlet, tee geometry and separation of the two branches.', `
pump P1 at 0,3
preheater H1 at 5,0 face east
preheater H2 at 5,6 face east
wire P1.water -> H1.in
wire P1.water -> H2.in`),
  fixture('shared-in', 'Two feeds, one inlet', 'fluid', 'A shared destination stresses merging routes and fittings at the receiving flange.', `
pump P1 at 0,0
pump P2 at 0,6
preheater H1 at 6,3 face east
wire P1.water -> H1.in
wire P2.water -> H1.in`),
  fixture('heat', 'Large heat main', 'heat', 'Fat lagged pipes need enough straight length for their elbows and collars.', `
reactor R1 at 0,0
exchanger H1 at 6,3 face east
wire R1.heat -> H1.heat`),
  fixture('gas', 'Steam connection', 'gas', 'Inspect the steam outlet and downward-facing turbine inlet, including the final elbow.', `
exchanger H1 at 0,0 face east
turbine T1 at 5,3 face east
wire H1.steam -> T1.steam`),
  fixture('shaft', 'Straight drive shaft', 'rotary', 'A drive that can align should stay straight, including its couplings.', `
motor M1 at 0,0 face east
gearbox G1 at 5,1 face east
wire M1.rotary -> G1.in`, { straight: true }),
  fixture('misaligned', 'Misaligned drive', 'rotary', 'An off-axis rigid drive should report its problem. Lost routes must have no fabricated pipe.', `
motor M1 at 0,0 face east
gearbox G1 at 5,6 face east
wire M1.rotary -> G1.in`, { issue: true }),
  fixture('belt', 'Belt across an offset', 'rotary', 'Compare the slack belt representation with the rigid drive preset.', `
motor M1 at 0,0 face east
gearbox G1 at 5,6 face east
wire M1.rotary -> G1.in belt`),
  fixture('material', 'Falling material', 'material', 'Check the square chute where the upper inlet drops into a lower crusher.', `
inlet I1 at 0,0,3 draws ore
crusher C1 at 4,0
wire I1.out -> C1.in`),
  fixture('mechanical', 'Mechanical linkage', 'mech', 'A crank feeding a press exercises the rod and mechanical coupling vocabulary.', `
crank K1 at 0,0 face east
press P1 at 5,0 face east
wire K1.stroke -> P1.drive`),
  fixture('electrical', 'Cable tray', 'electrical', 'An electrical run should use conduit and trays, with continuous connections at both ends.', `
mains E1 at 0,0
motor M1 at 7,4
wire E1.power -> M1.power`),
  fixture('crossing', 'Crossing fluid lines', 'fluid', 'Two crossed connections test routing order, pipe clearance and height separation.', `
pump P1 at 0,0 face east
pump P2 at 0,6 face east
preheater H1 at 8,0 face east
preheater H2 at 8,6 face east
wire P1.water -> H2.in
wire P2.water -> H1.in`),
  fixture('jammed', 'Jammed assembly / refused route', 'mixed', 'This crowded layout must expose a refused route. A lost connection should have no pipe mesh or centreline.', `
reactor R1 at 0,0
exchanger H1 at 5,0
turbine T1 at 9,0
generator G1 at 12,0
pump P1 at 3,5
valve V1 at 5,5
drum D1 at 7,4
wire R1.heat -> H1.heat
wire P1.water -> V1.in
wire V1.out -> D1.in
wire D1.out -> H1.water
wire H1.steam -> T1.steam
wire T1.rotary -> G1.rotary`, { lost: true }),
  fixture('mixed', 'Mixed-domain assembly', 'mixed', 'A small real assembly combines heat, water, steam and a rotary coupling in one view.', `
reactor R1 at 0,0
pump P1 at 0,4
exchanger H1 at 5,0 face east
turbine T1 at 9,0 face east
generator G1 at 13,0 face east
wire R1.heat -> H1.heat
wire P1.water -> H1.water
wire H1.steam -> T1.steam
wire T1.rotary -> G1.rotary`),
];

// Filter whole instances, preserving the prefix counts used by each LOD.
// All owners and paths keep their original indices and coordinates.
export function inspectionScene(scene, mode) {
  if (mode === 'all') return scene;
  const allowed = new Set(scene.owners.flatMap((o, i) =>
    (mode === 'pipes' ? o.class === 'run' : o.class !== 'shell') ? [i] : []));
  return { ...scene, batches: scene.batches.flatMap(b => {
    const inst = [], keep = b.keep.map(() => 0);
    for (let i = 0; i < b.n; i++) {
      if (!allowed.has(b.inst[i * 12 + 11])) continue;
      inst.push(...b.inst.slice(i * 12, (i + 1) * 12));
      b.keep.forEach((n, lod) => { if (i < n) keep[lod]++; });
    }
    return inst.length ? [{ ...b, inst, keep, n: inst.length / 12 }] : [];
  }) };
}
