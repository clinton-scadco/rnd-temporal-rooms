// The browser half of experiment 16, checked without a browser.
//
//     node tests/combat_web.mjs            (with `combat serve` running)
//     node tests/combat_web.mjs 8801       (on another port)
//
// The renderer is pure functions of (layout, frame, tick), so this drives it
// against a canvas that records instead of painting, with frames from a live
// server partway through a real wave. What it catches is the seam the Rust
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
  ok(layout.ok && layout.route.length > 2, 'the layout has a route');
  ok(layout.batteries.length === 6, 'six batteries');
  ok(layout.sectors.filter(s => s.inside).map(s => s.name).join() === 'Smelting,Yard', 'two sectors inside the domain');

  let f = await get('/api/frame');
  ok(f.ok && f.domain.open === false, 'the domain starts dormant');
  ok(f.sectors.every(s => s.mode === 'closed'), 'every sector starts in closed form');
  ok(f.log.commands === 0, 'nothing written down yet');

  const v = draw.makeView(layout, 900, 600);
  let ctx = stubCtx();
  let drew = draw.drawScene(ctx, layout, f, f.tick, v);
  ok(drew.cohorts === 0, 'an empty field draws no cohorts');
  ok(!ctx._rec.some(r => r[0] === 'NaN'), 'no NaN reaches the canvas at rest');

  // ---- a wave, at eight times speed
  await post('/api/speed', { x: 8, paused: false });
  const w = await post('/api/wave', { n: 3000 });
  ok(w.ok, 'a wave can be sent');
  const bad = await post('/api/wave', { n: 0 });
  ok(bad.ok === false, 'a wave of nobody is refused');

  let sawVolley = false;
  let sawFlash = false;
  let sawAwake = false;
  let sawSplit = false;
  let maxCohorts = 0;
  for (let i = 0; i < 60; i++) {
    await sleep(250);
    f = await get('/api/frame');
    if (f.sectors.some(s => s.mode === 'awake')) sawAwake = true;
    maxCohorts = Math.max(maxCohorts, f.cohorts.length);
    if (f.cohorts.some(c => c.hp < c.hpMax)) sawSplit = true;
    for (const dt of [0, 3, 9]) {
      ctx = stubCtx();
      drew = draw.drawScene(ctx, layout, f, f.tick + dt, v);
      if (ctx._rec.some(r => r[0] === 'NaN')) ok(false, `NaN drawn at t=${f.tick + dt}`);
      if (drew.volleys > 0) sawVolley = true;
      if (drew.flashes > 0) sawFlash = true;
    }
    // A volley is where its four numbers say, at both ends.
    for (const vo of f.volleys) {
      const a = draw.volleyAt(vo, vo.fired);
      const b = draw.volleyAt(vo, vo.lands);
      if (a.pos[0] !== vo.from[0] || b.pos[0] !== vo.to[0] || b.lob !== 0) {
        ok(false, 'a volley interpolates from its origin to its target');
      }
    }
    // A marching cohort is where the server says, at the server's tick.
    for (const c of f.cohorts) {
      const p = draw.cohortPos(layout, c, f.tick, draw.blocked(layout, f));
      if (Math.abs(p[0] - c.pos[0]) > 60 || Math.abs(p[1] - c.pos[1]) > 60) {
        ok(false, `cohort #${c.id} drawn ${Math.round(p[0] - c.pos[0])},${Math.round(p[1] - c.pos[1])} from where it is`);
      }
    }
    if (f.overlay.events > 0 && !f.domain.open && sawAwake) break;
  }
  ok(sawAwake, 'the sectors inside the domain woke');
  ok(sawVolley, 'volleys in flight were drawn');
  ok(sawFlash, 'muzzle flashes were drawn');
  ok(sawSplit, 'a cohort was seen damaged: a split reached the wire');
  ok(maxCohorts > 0 && maxCohorts < 60, `cohorts stayed a handful (${maxCohorts} at most)`);
  ok(f.sectors.find(s => s.name === 'Quarry').woke === 0, 'the Quarry never woke');
  ok(f.sectors.find(s => s.name === 'Works').woke === 0, 'the Works never woke');
  ok(f.log.serializedEvents === 0 && f.log.derived > 100, 'hundreds of events, none written down');
  ok(f.log.commands === 1, 'one command written down');

  // ---- the overlay says what the brief asks for
  const o = f.overlay;
  for (const k of ['nominal', 'cohorts', 'batteries', 'volleys', 'structures', 'eventsPerSec']) {
    ok(o[k] !== undefined, `the overlay reports ${k}`);
  }

  // ---- the page's panels, rendered from the same frame
  globalThis.document = { getElementById: () => null, addEventListener() {}, querySelectorAll: () => [], querySelector: () => null };
  const text = draw.fmt(12480);
  ok(text === '12,480', 'counts are written with commas');

  // ---- repair, and three reconstructions
  const lost = f.structures.filter(s => s.hp < s.max);
  if (lost.length) {
    const r = await post('/api/repair', { s: lost[0].name });
    ok(r.ok, `${lost[0].name} can be repaired`);
  }
  const dup = await post('/api/repair', { s: 'Yard sheds' });
  ok(dup.ok === false, 'repairing something intact is refused');
  const ver = await post('/api/verify', {});
  ok(ver.match, `live, replayed and resumed agree (${ver.live} / ${ver.replay} / ${ver.resumed})`);

  const log = await get('/api/log');
  ok(log.ok && log.log.commands.length >= 2, 'the log holds the commands');
  ok(!JSON.stringify(log.log).includes('impact'), 'the log holds no events');

  await post('/api/speed', { x: 1, paused: false });
  console.log(`${checks - failures} of ${checks} checks passed`);
  process.exit(failures ? 1 : 0);
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
