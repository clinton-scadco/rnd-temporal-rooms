// The browser half of experiment 16, checked without a browser.
//
//     node tests/combat_web.mjs            (with `combat serve` running)
//     node tests/combat_web.mjs 8801       (on another port)
//
// The renderer is pure functions of (layout, frame, tick), so this drives it
// against a canvas that records instead of painting, with frames from a live
// server partway through a real disturbance. What it catches is the seam the Rust
// tests cannot see: a field renamed on one side of the wire, an interpolation
// that puts a shell somewhere other than where the server says it lands, a
// layer that throws on a structure with no hp left.

import { fileURLToPath, pathToFileURL } from 'node:url';
import { dirname, join } from 'node:path';

const port = process.argv[2] || '8800';
const base = `http://127.0.0.1:${port}`;
const here = dirname(fileURLToPath(import.meta.url));
const url = p => pathToFileURL(join(here, '..', p)).href;

let failures = 0;
let checks = 0;
const ok = (cond, what) => {
  checks++;
  if (!cond) {
    console.log(`  FAIL  ${what}`);
    failures++;
  }
  return cond;
};

function stubCtx() {
  const rec = [];
  const noop = () => {};
  return new Proxy(
    { _rec: rec, measureText: t => ({ width: String(t).length * 6 }), setLineDash: noop, setTransform: noop },
    {
      get(t, k) {
        if (k in t) return t[k];
        return (...a) => {
          for (const x of a) if (typeof x === 'number' && !Number.isFinite(x)) rec.push(['NaN', String(k)]);
          rec.push([String(k), ...a]);
        };
      },
      set(t, k, v) {
        t[k] = v;
        return true;
      },
    }
  );
}

const get = async p => (await fetch(base + p)).json();
const post = async (p, b) =>
  (await fetch(base + p, { method: 'POST', headers: { 'content-type': 'application/json' }, body: JSON.stringify(b || {}) })).json();
const sleep = ms => new Promise(r => setTimeout(r, ms));

async function main() {
  console.log(`experiment 16's client, against ${base}`);
  const draw = await import(url('web/combat/draw.js'));

  await post('/api/reset', { seed: 16 });
  const layout = await get('/api/layout');
  ok(layout.ok && layout.phase === '2037', 'the district stands in 2037');
  ok(layout.land.length >= 15, "experiment 15's land came with it");
  ok(layout.lanes.map(l => l.origin).join() === '1890,2070', 'two lanes: 1890 forward, 2070 back');
  ok(layout.sites.length === 3 && layout.anchors.length === 2, 'three sites, two anchors');
  ok(layout.sectors.find(s => s.name === 'Foundry').reachedBy.length === 0, 'the foundry is out of every reach');
  const grid = draw.faceGrid(layout);
  ok(grid['1890'].length === layout.w * layout.h, 'a face for every tile, in every century');
  const at = (ph, x, y) => (grid[ph][y * layout.w + x] || { glyph: '.' }).glyph;
  ok(at('1890', 45, 20) === 'T' && at('2037', 45, 20) === '&', 'Long Wood in 1890 is Tarrant Street in 2037');

  let f = await get('/api/frame');
  ok(f.ok && f.domain.open === false, 'the district starts dormant');
  ok(f.sectors.every(s => s.mode === 'closed'), 'every sector starts in closed form');
  ok(f.log.commands === 0, 'nothing written down yet');

  const v = draw.makeView(layout, 900, 600);
  let ctx = stubCtx();
  let drew = draw.drawScene(ctx, layout, f, f.tick, v);
  ok(drew.cohorts === 0 && drew.bleeds === 0 && drew.streams === 0, 'a quiet district draws nothing moving');
  ok(!ctx._rec.some(r => r[0] === 'NaN'), 'no NaN reaches the canvas at rest');

  // ---- open both fractures, at eight times speed
  await post('/api/speed', { x: 8, paused: false });
  ok((await post('/api/stream', { lane: 'deep', rate: 60 })).ok, 'the deep fracture can be opened');
  ok((await post('/api/stream', { lane: 'near', rate: 30 })).ok, 'the near corridor can be opened');
  ok((await post('/api/stream', { lane: 'nowhere', rate: 30 })).ok === false, 'a lane that does not exist is refused');

  let sawTear = false;
  let sawBleed = false;
  let sawDark = false;
  let sawVolley = false;
  let sawStream = false;
  let sawAwake = false;
  let sawSplit = false;
  let anchored = false;
  let maxCohorts = 0;
  for (let i = 0; i < 90; i++) {
    await sleep(250);
    f = await get('/api/frame');
    if (f.sectors.some(s => s.mode === 'awake')) sawAwake = true;
    if (f.dark) sawDark = true;
    maxCohorts = Math.max(maxCohorts, f.cohorts.length);
    if (f.cohorts.some(c => c.hp < c.hpMax)) sawSplit = true;
    for (const dt of [0, 3, 9]) {
      ctx = stubCtx();
      drew = draw.drawScene(ctx, layout, f, f.tick + dt, v);
      if (ctx._rec.some(r => r[0] === 'NaN')) ok(false, `NaN drawn at t=${f.tick + dt}`);
      if (drew.volleys > 0) sawVolley = true;
      if (drew.ruptures > 0) sawTear = true;
      if (drew.bleeds > 0) sawBleed = true;
      if (drew.streams > 0) sawStream = true;
    }
    // Strain carried on at its rate is what the server says at its own tick.
    for (const s of f.sites) {
      const [a, b] = draw.strainAt(s, s.since);
      if (a !== Number(s.strain[0]) || b !== Number(s.strain[1])) ok(false, 'strain interpolates from its level');
    }
    for (const vo of f.volleys) {
      const a = draw.volleyAt(vo, vo.fired);
      const b = draw.volleyAt(vo, vo.lands);
      if (a.pos[0] !== vo.from[0] || b.pos[0] !== vo.to[0] || b.lob !== 0) {
        ok(false, 'a volley interpolates from its origin to its target');
      }
    }
    for (const c of f.cohorts) {
      const p = draw.cohortPos(layout, c, f.tick);
      if (Math.abs(p[0] - c.pos[0]) > 60 || Math.abs(p[1] - c.pos[1]) > 60) {
        ok(false, `cohort #${c.id} drawn ${Math.round(p[0] - c.pos[0])},${Math.round(p[1] - c.pos[1])} from where it is`);
      }
    }
    // Once it has torn and bled, answer it: anchor, launder, close the lanes.
    if (!anchored && sawBleed && sawVolley) {
      anchored = true;
      ok((await post('/api/anchor', { a: 'North anchor', on: true })).ok, 'the north anchor can be powered');
      ok((await post('/api/launder', { on: true })).ok, 'the ore can be laundered');
      ok((await post('/api/stream', { lane: 'deep', rate: 0 })).ok, 'the deep fracture can be closed');
      ok((await post('/api/stream', { lane: 'near', rate: 0 })).ok, 'the near corridor can be closed');
    }
    if (anchored && !f.domain.open && f.cohorts.length === 0 && f.overlay.events > 0) break;
  }
  ok(sawTear, 'a rupture was drawn');
  ok(sawBleed, 'a bleed was drawn: another century showing through');
  ok(sawDark, 'the interface went dark');
  ok(sawStream, 'crossings were drawn');
  ok(sawAwake, 'the sectors a rupture reaches woke');
  ok(sawVolley, 'volleys in flight were drawn');
  ok(sawSplit, 'a cohort was seen damaged: a split reached the wire');
  ok(maxCohorts > 0 && maxCohorts < 60, `cohorts stayed a handful (${maxCohorts} at most)`);
  ok(f.sectors.find(s => s.name === 'Foundry').woke === 0, 'the foundry never woke');
  ok(f.log.serializedEvents === 0 && f.log.derived > 100, 'hundreds of events, none written down');

  const o = f.overlay;
  for (const k of ['nominal', 'cohorts', 'ruptures', 'strainRecords', 'batteries', 'volleys', 'eventsPerSec']) {
    ok(o[k] !== undefined, `the overlay reports ${k}`);
  }
  ok(draw.fmt(12480) === '12,480', 'counts are written with commas');
  ok(draw.short(43338694) === '43M', 'big crowds are written short');

  // ---- repair, and three reconstructions
  const lost = f.structures.filter(s => s.hp < s.max);
  if (lost.length) {
    const r = await post('/api/repair', { s: lost[0].name });
    ok(r.ok, `${lost[0].name} can be repaired`);
  }
  const dup = await post('/api/repair', { s: 'Crusher house' });
  ok(dup.ok === false, 'repairing something intact is refused');
  const ver = await post('/api/verify', {});
  ok(ver.match, `live, replayed and resumed agree (${ver.live} / ${ver.replay} / ${ver.resumed})`);

  const log = await get('/api/log');
  ok(log.ok && log.log.commands.length >= 6, 'the log holds the commands');
  ok(!JSON.stringify(log.log).includes('impact') && !JSON.stringify(log.log).includes('tear'), 'the log holds no events');

  await post('/api/speed', { x: 1, paused: false });
  console.log(`${checks - failures} of ${checks} checks passed`);
  process.exit(failures ? 1 : 0);
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
