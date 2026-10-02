// Run against machine serve: node tests/connections_web.mjs [port]
import assert from 'node:assert/strict';
import { readFileSync } from 'node:fs';
const moduleSource = readFileSync(new URL('../web/machine/connections-cases.js', import.meta.url), 'utf8');
const { CASES, inspectionScene } = await import(`data:text/javascript;base64,${Buffer.from(moduleSource).toString('base64')}`);
const base = `http://127.0.0.1:${process.argv[2] || 8788}`;
async function post(path, body) {
  const response = await fetch(base + path, { method: 'POST', body });
  assert.equal(response.status, 200);
  const json = await response.json(); assert(json.ok, JSON.stringify(json)); return json;
}
for (const asset of ['/connections', '/connections.css', '/connections.js', '/connections-cases.js']) {
  assert.equal((await fetch(base + asset)).status, 200, asset);
}
const same = (a, b) => a.every((v, i) => Math.abs(v - b[i]) < 1e-8);
const domains = new Set();
let routes = 0, lost = 0;
for (const preset of CASES) {
  const { design } = await post('/api/open', preset.source);
  const scene = await post('/api/form?style=yard&seed=0&diagnostics=1', JSON.stringify({ design }));
  assert.equal(scene.runs.length, design.wires.length, preset.id);
  const diagnostics = scene.diagnostics;
  assert.equal(diagnostics.paths.length, scene.runs.length);
  for (const run of scene.runs) {
    routes++; domains.add(run.domain);
    const points = diagnostics.paths.find(p => p.name === run.name).points;
    const owner = scene.owners.findIndex(o => o.class === 'run' && o.name === run.name);
    assert(owner >= 0);
    if (run.tier === 'lost') {
      lost++; assert.equal(points.length, 0, run.name);
      for (const batch of scene.batches) {
        assert(!batch.inst.some((v, i) => i % 12 === 11 && v === owner), `Lost run fabricated geometry: ${run.name}`);
      }
      continue;
    }
    assert(points.length >= 2, `${preset.id}: ${run.name}`);
    const [from, to] = run.name.split(' -> ');
    const a = diagnostics.sockets.find(p => p.name === from), b = diagnostics.sockets.find(p => p.name === to);
    assert(a && b, run.name);
    assert(same(points[0], a.at), `${run.name}: source mismatch`);
    assert(same(points.at(-1), b.at), `${run.name}: destination mismatch`);
    for (let i = 1; !run.belt && i < points.length; i++) {
      assert.equal(points[i].filter((v, axis) => Math.abs(v - points[i - 1][axis]) > 1e-8).length, 1, `${run.name}: non-orthogonal path`);
    }
    if (preset.expected.straight) assert.equal(run.bends, 0, `${preset.id}: should align straight`);
  }
  if (preset.expected.issue) assert(scene.issues.length > 0, `${preset.id}: missing spatial warning`);
  if (preset.expected.lost) assert(scene.runs.some(r => r.tier === 'lost'), `${preset.id}: missing refused route`);
  for (const mode of ['equipment', 'pipes', 'all']) {
    const visible = inspectionScene(scene, mode);
    for (const batch of visible.batches) {
      assert.equal(batch.inst.length, batch.n * 12);
      assert(batch.keep.every(n => n <= batch.n));
      assert(batch.keep[0] >= batch.keep[1] && batch.keep[1] >= batch.keep[2]);
      for (let i = 0; i < batch.n; i++) {
        const kind = scene.owners[batch.inst[i * 12 + 11]].class;
        if (mode === 'pipes') assert.equal(kind, 'run');
        if (mode === 'equipment') assert.notEqual(kind, 'shell');
      }
      // Filtering must retain the exact surviving prefix at every LOD.
      const original = scene.batches.find(b => b.mesh === batch.mesh && b.mat === batch.mat);
      for (let lod = 0; lod < 3; lod++) {
        const expected = [];
        for (let i = 0; i < original.keep[lod]; i++) {
          const kind = scene.owners[original.inst[i * 12 + 11]].class;
          if (mode === 'all' || (mode === 'pipes' ? kind === 'run' : kind !== 'shell')) expected.push(...original.inst.slice(i * 12, (i + 1) * 12));
        }
        assert.deepEqual(batch.inst.slice(0, batch.keep[lod] * 12), expected);
      }
    }
  }
  console.log(`${preset.id}: ${scene.runs.map(r => `${r.tier}/${r.bends} bends`).join(', ')}`);
}
assert.equal(domains.size, 7, 'Connection presets should cover every routing domain');
const normal = await post('/api/form', JSON.stringify((await post('/api/open', CASES[0].source)).design));
assert.equal(normal.diagnostics, undefined, 'Diagnostics must be opt-in');
console.log(`${CASES.length} presets, ${routes} routes, ${domains.size} domains checked (${lost} lost routes exposed).`);
