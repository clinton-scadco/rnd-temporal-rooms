// The browser half of experiment 15, checked without a browser.
//
//     node tests/slice_web.mjs             (with `slice serve` running)
//     node tests/slice_web.mjs 8797        (on another port)
//
// `room_web.mjs` already runs the world view, the machine bench and the
// inspector against live frames, and this experiment does not fork any of them
// -- so this harness deliberately does not do that again. It checks the half
// that is new, and "new" here means three things no earlier client had:
//
//   the century   walking through a fracture reprices two palettes and
//                 repaints the ground, and none of that is the room view's
//   the fracture  a panel that can report a failure which is not about a
//                 factory: somebody else's grid, a hundred and fifty years away
//   provenance    what everything is made of, by the century it came out of
//
// What it catches is the seam the Rust tests cannot see: a field renamed on one
// side of the wire, a panel reading `shipping.interfaces` when the server sends
// `gates`, a terrain layer that throws on a face with no colour.

import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const port = process.argv[2] || '8797';
const base = `http://127.0.0.1:${port}`;
const here = dirname(fileURLToPath(import.meta.url));

let failures = 0;
let checks = 0;
const ok = (cond, what) => {
  checks++;
  if (!cond) { console.log(`  FAIL  ${what}`); failures++; }
  return cond;
};

// ------------------------------------------------------- a browser, sort of

function makeText(t) {
  return { tagName: '#text', textContent: String(t), children: [], parent: null };
}

function stubCtx() {
  const rec = [];
  const noop = () => {};
  return new Proxy(
    {
      canvas: { width: 900, height: 600 },
      _rec: rec,
      measureText: t => ({ width: String(t).length * 6 }),
      createLinearGradient: () => ({ addColorStop: noop }),
      getImageData: () => ({ data: new Uint8ClampedArray(4) }),
      setTransform: noop,
    },
    {
      get(t, k) {
        if (k in t) return t[k];
        return (...a) => { rec.push([String(k), ...a]); };
      },
      set(t, k, v) { t[k] = v; return true; },
    }
  );
}

function makeEl(tagName) {
  const e = {
    tagName: String(tagName).toUpperCase(),
    className: '', textContent: '', value: '', hidden: false, title: '',
    disabled: false,
    options: [],
    style: { cssText: '', setProperty() {}, visibility: '' },
    dataset: {},
    children: [], parent: null,
    _html: '',
    _classes: new Set(),
    classList: {
      toggle(c, on) { on ? e._classes.add(c) : e._classes.delete(c); },
      add(c) { e._classes.add(c); },
      remove(c) { e._classes.delete(c); },
      contains: c => e._classes.has(c),
    },
    on: {},
    addEventListener(k, f) { (e.on[k] ||= []).push(f); },
    removeEventListener() {},
    appendChild(c) { c.parent = e; e.children.push(c); return c; },
    append(...cs) { for (const c of cs) e.appendChild(typeof c === 'object' ? c : makeText(c)); },
    remove() {},
    querySelector(sel) { return e.querySelectorAll(sel)[0] || null; },
    querySelectorAll(sel) {
      const cache = (e._q ||= new Map());
      if (cache.has(sel)) return cache.get(sel);
      const m = /^\[([-\w]+)\]$/.exec(sel);
      if (!m) return [];
      const attr = m[1];
      const key = attr.replace(/^data-/, '').replace(/-(\w)/g, (_, c) => c.toUpperCase());
      const out = [];
      const re = new RegExp(`${attr}="([^"]*)"`, 'g');
      let hit;
      while ((hit = re.exec(e._html))) {
        const el = makeEl('button');
        el.dataset[key] = hit[1];
        out.push(el);
      }
      cache.set(sel, out);
      return out;
    },
    getBoundingClientRect() { return { left: 0, top: 0, width: 900, height: 600 }; },
    getContext(kind) {
      if (String(kind).startsWith('webgl')) return null;
      return (e._ctx ||= stubCtx());
    },
  };
  Object.defineProperty(e, 'innerHTML', {
    get() { return e._html; },
    set(v) { e._html = String(v); e.children = []; e._q = new Map(); },
  });
  return e;
}

const els = new Map();
const TAGS = { family: 'select', seed: 'input', whoami: 'input' };
const el = id => {
  if (!els.has(id)) els.set(id, makeEl(TAGS[id] || 'div'));
  return els.get(id);
};

// The two palettes the price badges are written onto. They are found by
// `document.querySelector('#palette button[data-proto="x"]')`, so the stub has
// to answer that shape rather than the bracket-only one an element answers.
const palette = new Map();
const partBtn = new Map();
function paletteButton(store, key) {
  if (!store.has(key)) store.set(key, makeEl('button'));
  return store.get(key);
}

globalThis.window = {
  devicePixelRatio: 1,
  addEventListener() {},
  dispatchEvent() {},
  CustomEvent: class {},
};
globalThis.document = {
  getElementById: el,
  createElement: makeEl,
  querySelector(sel) {
    let m = /^#palette button\[data-proto="([^"]+)"\]$/.exec(sel);
    if (m) return paletteButton(palette, m[1]);
    m = /^#parts button\[data-kind="([^"]+)"\]$/.exec(sel);
    if (m) return paletteButton(partBtn, m[1]);
    return null;
  },
  querySelectorAll: () => [],
  addEventListener() {},
  body: makeEl('body'),
};
Object.defineProperty(globalThis, 'navigator', {
  value: { clipboard: null },
  configurable: true,
});
globalThis.prompt = () => 'Mk2';
globalThis.alert = () => {};
globalThis.addEventListener = () => {};
globalThis.removeEventListener = () => {};
globalThis.requestAnimationFrame = f => setTimeout(f, 0);
globalThis.cancelAnimationFrame = id => clearTimeout(id);
globalThis.CustomEvent = class CustomEvent {
  constructor(type, init) { this.type = type; this.detail = init && init.detail; }
};
globalThis.dispatchEvent = () => {};
// `world.js` reaches for the bare global the way a page has one.
globalThis.devicePixelRatio = 1;

// ------------------------------------------------------------------ the run

const post = async (path, body) => {
  const r = await fetch(base + path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body || {}),
  });
  return r.json();
};
const get = async path => (await fetch(base + path)).json();

const bare = globalThis.fetch;
globalThis.fetch = (u, o) => bare(String(u).startsWith('http') ? u : base + u, o);

async function main() {
  console.log(`experiment 15's client, against ${base}`);

  // ---- entering
  const me = await post('/api/enter', { name: 'harness' });
  ok(me.ok, 'a player can enter the slice');
  ok(typeof me.code === 'string' && me.code.length === 6, 'the slice has a code');
  ok(me.at === 'valley', 'you start in 1890');
  await post('/api/start', {});

  // ---- the modules, as the page loads them
  const shell = await import(url('web/slice/shell.js'));
  const map = await import(url('web/slice/map.js'));
  const terrain = await import(url('web/slice/terrain.js'));

  const statics = await get('/api/regions');
  const land = await get('/api/land');
  ok(statics.ok && statics.regions.length === 3, 'three regions, from the server');
  ok(statics.fractures.length === 2, 'two fractures');
  ok(land.ok && land.features.length > 10, 'the land arrives with its features');
  ok(
    land.features.every(f => (f.faces || []).length === 3),
    'every feature has a face in all three centuries'
  );
  ok(
    land.features.every(f => f.faces.every(x => /^#[0-9a-f]{6}$/.test(x.colour || ''))),
    'every face has a colour the terrain layer can paint'
  );

  let v = await get(`/api/slice?player=${me.player}`);
  ok(v.ok, 'the slice frame arrives');
  ok(v.regions.length === 3 && v.crossings.length === 2, 'it carries regions and crossings');
  ok(v.at === 'valley', 'it says which century you are standing in');

  // ---- the panels, against a live frame
  shell.renderWhere(v, () => {});
  const where = el('wherebox').innerHTML;
  ok(/1890/.test(where) && /2037/.test(where) && /2070/.test(where), 'the switcher has all three');
  ok(!/disabled/.test(where), 'no century is locked: there is no ladder in this experiment');

  shell.renderCrates(v, 'valley');
  ok(/none landed/.test(el('crates').textContent), '1890 starts with no imported plant');

  shell.renderRegion(v, 'valley', () => {});
  const card = el('roomcard').innerHTML;
  ok(/Kestrel Reach/.test(card), 'the region card names what is under the ground');
  ok(/objective/.test(card), 'and what it has been asked for');

  shell.renderRegion(v, 'district', () => {});
  const d = el('roomcard').innerHTML;
  ok(
    /somebody already built here/.test(d) && /Kestrel Reach/.test(d),
    'in 2037 the card says somebody built on the ore body'
  );

  shell.renderCentury(v, v.phases, 'valley');
  const cent = el('century').innerHTML;
  ok(/a grid to use/.test(cent), 'the century panel says whether there is a grid');
  ok(/mains|motor|generator/.test(cent), 'and lists what the century has not got');

  // ---- the fracture panel: the one failure that is not about a factory
  shell.renderGates(v, { open() {}, close() {} });
  let gates = el('gates').innerHTML;
  ok(/deep fracture/.test(gates) && /near fracture/.test(gates), 'both fractures are listed');
  ok(/no interface/.test(gates), 'the deep one starts bare, and the panel says so');
  ok(/build an interface/.test(gates), 'and offers the one command that puts one there');

  const opened = await post('/api/gate', { do: 'open', player: me.player, fracture: 'deep' });
  ok(opened.ok, 'an interface can be built on the deep fracture');
  ok(
    !(await post('/api/gate', { do: 'open', player: me.player, fracture: 'deep' })).ok,
    'and a second one on the same fracture is refused'
  );

  v = await get(`/api/slice?player=${me.player}`);
  shell.renderGates(v, { open() {}, close() {} });
  gates = el('gates').innerHTML;
  ok(
    /dark/.test(gates) || /holding/.test(gates),
    'once it is there the panel reports whether it is holding'
  );
  ok(
    /2037 Industrial District is delivering/.test(gates),
    'and names the region whose grid is paying for it'
  );

  // ---- the shipping board, and the fleets a century can field
  shell.renderLanes(v, { open() {}, close() {}, cap() {} });
  const lanes = el('lanes').innerHTML;
  ok(/1890 → 2037/.test(lanes), 'lanes are labelled by the centuries they join');
  ok(/Mineral Tramway|Wagon Road/.test(lanes), '1890 is offered the haulage 1890 has');
  ok(
    !/Automated Hauler/.test(lanes.split('2037 → 2070')[0]),
    'and not the haulage it has not'
  );

  // ---- provenance, before anything has crossed
  shell.renderProvenance(v);
  ok(
    /nothing has crossed/.test(el('provenance').innerHTML),
    'the provenance board says so when nothing has'
  );

  shell.renderRegionIO(v, 'valley');
  ok(/no route/.test(el('roomio').innerHTML), 'the io panel names ports nobody is using');

  shell.renderNews(v);
  ok(el('news').innerHTML.length > 0, 'the feed renders');

  // ---- the world map, against the stub canvas
  const atlas = el('atlascanvas');
  map.init(atlas, { onPick() {}, onGate() {} });
  map.setStatics(statics);
  map.setLand(land);
  map.resize();
  map.show(v);
  const drew = atlas.getContext('2d')._rec;
  ok(drew.length > 0, 'the world map draws something');
  ok(
    drew.some(c => c[0] === 'fillText' && /1890 Mining Valley/.test(String(c[1]))),
    'and puts the centuries on it'
  );
  ok(
    drew.some(c => c[0] === 'fillText' && /years/.test(String(c[1]))),
    'and says how far apart they are'
  );

  // ---- the terrain layer
  const tcv = el('terrain');
  terrain.init(tcv);
  terrain.setLand(land);
  terrain.setPhase('1890');
  terrain.resize();
  const painted = tcv.getContext('2d')._rec.filter(c => c[0] === 'fillRect');
  ok(painted.length > 5, 'the terrain layer paints the ground under the plot');
  const in1890 = terrain.here();
  terrain.setPhase('2037');
  terrain.resize();
  const in2037 = terrain.here();
  ok(in1890.length > 0 && in2037.length > 0, 'the terrain key lists what is on the ground');
  ok(
    JSON.stringify(in1890) !== JSON.stringify(in2037),
    'and it is a different valley on the other side of a fracture'
  );
  ok(
    in2037.some(f => f.taken) && !in1890.some(f => f.name === 'Kestrel Reach' && f.taken),
    'Kestrel Reach is open ground in 1890 and somebody else’s works in 2037'
  );
  shell.renderTerrain(in2037);
  ok(
    /nothing may be built here/.test(el('terrainkey').innerHTML),
    'and the panel beside the plot says which tiles are spoken for'
  );

  // ---- the prices: the whole argument of the experiment, on two palettes
  const cat = await get('/api/catalogue?code=valley');
  ok(cat.phase === '1890', 'the catalogue is priced for the century you are in');
  shell.markPrices(cat);
  const plant = paletteButton(palette, 'steamplant');
  ok(plant._classes.has('costly'), 'a compact steam plant is not free in 1890');
  ok(/gears of imported machinery/.test(plant.title), 'and its tooltip says what it costs');
  const line = paletteButton(palette, 'powderline');
  ok(line._classes.has('illegal'), 'the stock powder line cannot be legal in 1890 at any price');
  ok(!line.disabled, 'and it is still not greyed out of existence');

  const parts1890 = await get('/api/parts?code=valley');
  const motor = parts1890.parts.find(p => p.kind === 'motor');
  ok(motor && !motor.native && motor.gears === 1600, 'a motor in 1890 is 1,600 gears');
  ok(motor.madeIn === '2037', 'and the palette can say which century makes one');
  const mains = parts1890.parts.find(p => p.kind === 'mains');
  ok(mains && !mains.crateable, 'a grid connection is the one thing no crate helps with');
  const crusher = parts1890.parts.find(p => p.kind === 'crusher');
  ok(
    crusher && (crusher.frames || []).some(f => f.tag === 'iron' && f.here),
    'a crusher in 1890 is free on cast iron, and the bench is told so'
  );

  const parts2037 = await get('/api/parts?code=district');
  const motor37 = parts2037.parts.find(p => p.kind === 'motor');
  ok(motor37 && motor37.native, 'and in 2037 a motor is just a motor');

  // ---- walking through a fracture
  const walk = await post('/api/travel', { player: me.player, region: 'district' });
  ok(walk.ok && walk.at === 'district', 'a player can walk into 2037');
  ok(walk.phase === '2037', 'and the server says which century that is');
  v = await get(`/api/slice?player=${me.player}`);
  ok(v.at === 'district', 'the frame follows them');
  shell.renderCrates(v, 'district');
  shell.renderRegion(v, 'district', () => {});
  shell.renderCentury(v, v.phases, 'district');
  ok(/a grid to use<\/b><span>yes/.test(el('century').innerHTML), '2037 has a grid and 1890 had not');

  // ---- one intention, in one century
  const state = await get(`/api/state?code=district&player=${me.player}`);
  ok(state.ok, 'a region frame arrives');
  const seam = (state.world.deposits || []).find(d => d.item === 'Water');
  ok(!!seam, '2037 still has the mill race');
  const built = await post('/api/cmd', {
    code: 'district',
    player: me.player,
    type: 'PlaceStorage',
    payload: { proto: 'bay', x: 2, y: 62, face: 0 },
  });
  ok(built.ok, 'a bay can be placed on open ground');
  const onWorks = await post('/api/cmd', {
    code: 'district',
    player: me.player,
    type: 'PlaceStorage',
    payload: { proto: 'bay', x: 10, y: 8, face: 0 },
  });
  ok(!onWorks.ok && onWorks.refused, 'and not on the foundry somebody built in 1951');
  ok(
    /1890 it is open ground/.test(onWorks.error || ''),
    'and the refusal says what the same tiles are in the century that has them'
  );

  console.log(`\n${checks - failures} of ${checks} checks passed`);
  // Explicit, because `terrain.js` starts a render loop the way a page does,
  // and a render loop keeps Node's event loop alive forever. A harness that
  // drives a module with a heartbeat in it has to be the thing that stops.
  process.exit(failures ? 1 : 0);
}

function url(rel) {
  return 'file://' + join(here, '..', rel).replace(/\\/g, '/');
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
