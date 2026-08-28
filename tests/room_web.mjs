// The browser half of prototype 2, checked without a browser.
//
//     node tests/room_web.mjs             (with `room serve` running)
//     node tests/room_web.mjs 8791        (on another port)
//
// Two players, one room, and no Chrome. The harness plays a whole session
// through the same HTTP the front end uses -- host, join, place, wire,
// redesign, commit, delete, restore -- and then runs the real `world.js` and
// `panels.js` against the frames that come back, with a canvas that records
// instead of painting.
//
// What that catches is the half of the client the Rust tests cannot see: a
// field renamed on one side of the wire, a panel that reads `progress.lines`
// when the server sends `lines`, a renderer that throws on a factory with a
// transport in it. The determinism proof lives in `tests/mp.rs`; this is the
// proof that the thing on the screen is looking at it.

import { readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const port = process.argv[2] || '8790';
const base = `http://127.0.0.1:${port}`;
const here = dirname(fileURLToPath(import.meta.url));
const web = join(here, '..', 'web', 'room');

let failures = 0;
const ok = (cond, what) => {
  if (!cond) { console.log(`  FAIL  ${what}`); failures++; }
  return cond;
};

// ------------------------------------------------------------ a browser, sort of

function makeText(t) {
  return { tagName: '#text', textContent: String(t), children: [], parent: null };
}

function makeEl(tagName) {
  const e = {
    tagName: String(tagName).toUpperCase(),
    className: '', textContent: '', value: '', hidden: false, title: '',
    style: { cssText: '', setProperty() {} }, dataset: {},
    children: [], parent: null,
    classList: { toggle() {}, add() {}, remove() {} },
    // Handlers are kept rather than dropped, so a test can click the canvas
    // the way a player does instead of calling the function behind it.
    on: {},
    addEventListener(k, f) { (e.on[k] ||= []).push(f); },
    removeEventListener() {},
    appendChild(c) { c.parent = e; e.children.push(c); return c; },
    append(...cs) { for (const c of cs) e.appendChild(typeof c === 'object' ? c : makeText(c)); },
    remove() {},
    // Enough of a query to find the buttons a panel just wrote, which is the
    // only selector anything in the client asks this for. The same objects
    // come back every time, or the handler a panel hangs on one would be hung
    // on something nobody can press.
    querySelectorAll(sel) { return sel === '[data-act]' ? e._acts || [] : []; },
    getBoundingClientRect() { return { left: 0, top: 0, width: 900, height: 600 }; },
    getContext() { return stubCtx(); },
    contains() { return false; },
    set innerHTML(v) {
      e._html = v;
      e.children = [];
      e._acts = [...String(v).matchAll(/data-act="([a-z]+)"/g)].map(m => {
        const b = makeEl('button');
        b.dataset.act = m[1];
        return b;
      });
    },
    get innerHTML() { return e._html || ''; },
  };
  return e;
}

function stubCtx() {
  const calls = [];
  return new Proxy({ calls }, {
    get(t, k) {
      if (k === 'calls') return calls;
      if (k === 'measureText') return s => ({ width: String(s).length * 6 });
      if (k === 'canvas') return { width: 900, height: 600 };
      return (...a) => { calls.push([k, ...a]); };
    },
    set() { return true; },
  });
}

const panes = {};
const el = id => (panes[id] ||= makeEl('div'));
globalThis.document = {
  documentElement: {},
  body: makeEl('body'),
  getElementById: el,
  querySelector: () => null,
  querySelectorAll: () => [],
  createElement: makeEl,
  createTextNode: makeText,
};
globalThis.window = { devicePixelRatio: 1 };
globalThis.devicePixelRatio = 1;
globalThis.addEventListener = () => {};
globalThis.requestAnimationFrame = () => 0;
globalThis.performance = globalThis.performance || { now: () => Date.now() };
globalThis.getComputedStyle = () => ({ getPropertyValue: () => '#46C5A5' });
const real = globalThis.fetch;
globalThis.fetch = (url, init) => real(String(url).startsWith('/') ? base + url : url, init);

const pathToUrl = p => 'file:///' + p.replace(/\\/g, '/');
const net = await import(pathToUrl(join(web, 'net.js')));
const world = await import(pathToUrl(join(web, 'world.js')));
const panels = await import(pathToUrl(join(web, 'panels.js')));

const post = (path, body) =>
  fetch(base + path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  }).then(r => r.json());
const get = path => fetch(base + path).then(r => r.json());

async function reachable() {
  try { return (await fetch(base + '/api/catalogue')).ok; } catch { return false; }
}
if (!(await reachable())) {
  console.log(`nothing is serving on ${base}. Start it with:\n  .\\run.ps1 -Room`);
  process.exit(2);
}

// ------------------------------------------------------------- served assets

console.log('served assets');
for (const [url, file] of [
  ['/', 'index.html'],
  ['/room.css', 'room.css'],
  ['/app.js', 'app.js'],
  ['/net.js', 'net.js'],
  ['/world.js', 'world.js'],
  ['/bench.js', 'bench.js'],
  ['/panels.js', 'panels.js'],
]) {
  const served = await fetch(base + url).then(r => r.text());
  const onDisk = readFileSync(join(web, file), 'utf8');
  ok(served === onDisk, `${url} is the ${file} on disk` +
    (served === onDisk ? '' : ' -- the binary is stale, rebuild it'));
}
{
  const served = await fetch(base + '/machine/form.js').then(r => r.text());
  const onDisk = readFileSync(join(here, '..', 'web', 'machine', 'form.js'), 'utf8');
  ok(served === onDisk, "/machine/form.js is experiment 10's renderer, unforked");
}

// ---------------------------------------------------------------- catalogue

console.log('catalogue');
const cat = await net.catalogue();
ok(cat.ok && cat.protos.length > 10, `${cat.protos.length} things can be placed`);
ok(cat.tickRate === 60, 'sixty ticks to the second');
ok(cat.plot > 32, 'the plot has a size');
for (const p of cat.protos) {
  ok(p.w > 0 && p.h > 0, `${p.tag} has a footprint`);
  ok(!!p.title && !!p.role && !!p.note, `${p.tag} can be drawn in the palette`);
  if (p.role === 'machine') {
    ok(!!p.macro, `${p.tag} carries the recipe its design compiles to`);
    ok(p.macro.cycleSeconds > 0 && p.macro.gives.length > 0, `${p.tag} makes something`);
    ok(p.macro.width === p.w && p.macro.height === p.h, `${p.tag}'s palette size is its real size`);
  }
}
const parts = await net.parts();
ok(parts.parts.length > 30, `${parts.parts.length} components to design with`);
const templates = (await get('/api/goals')).templates;
ok(templates.length >= 15, `${templates.length} goal templates`);

// ------------------------------------------------------------------ a room

console.log('hosting');
const seed = String(Date.now() % 1_000_000_007);
const hosted = await net.host('Ada', seed, 'first-gears');
ok(hosted.ok && /^[A-Z0-9]{6}$/.test(hosted.code), `room ${hosted.code || hosted.error}`);
if (!hosted.ok) process.exit(1);
ok(hosted.goal.brief.includes('gears'), `goal: ${hosted.goal.brief}`);
const code = hosted.code;
const ada = hosted.player;

// Section 19: the objective is on screen, and nothing is running yet.
const idle = await get(`/api/state?code=${code}&player=${ada}`);
ok(idle.ok && !idle.started && idle.tick === 0, 'the clock has not started');
const refused = await post('/api/cmd', {
  code, player: ada, type: 'PlaceStorage', payload: { proto: 'bay', x: 70, y: 70, face: 0 },
});
ok(!refused.ok, `and nothing can be built yet: ${refused.error}`);
const begun = await post('/api/start', { code });
ok(begun.ok, 'the host starts the room, and there is no matching stop');

const joined = await post('/api/join', { code, name: 'Bee' });
ok(joined.ok, `Bee joined at tick ${joined.joinedAt}`);
const bee = joined.player;

const frame = async player => get(`/api/state?code=${code}&player=${player}`);
let v = await frame(ada);
ok(v.ok, 'a frame arrived');
ok(v.tick >= 0 && v.tickRate === 60, 'it has a clock');
ok(v.world.installs.length >= 4, `the plot starts with ${v.world.installs.length} things on it`);
ok(v.players.length === 2, 'two players');
ok(!!v.goal.progress, 'and an objective with progress on it');

// The command path, including the refusals.
console.log('commands');
const cmd = (player, type, payload) => post('/api/cmd', { code, player, type, payload });
const find = tag => v.world.installs.find(i => i.proto === tag);

const caster = find('billetcaster'), coal = find('coalpit'), water = find('waterpump');
const depot = find('depot');
const bays = v.world.installs.filter(i => i.proto === 'bay');
ok(!!caster && !!coal && !!depot, 'the goal furnished the plot with what it is about');

const placed = await cmd(ada, 'PlaceMachine', { proto: 'machining', x: 40, y: 8, face: 0 });
ok(placed.ok, 'a machining cell is placed');
ok(placed.command.tick >= 0 && placed.command.seq > 0, 'stamped with a tick and a sequence');
v = await frame(ada);
const cell = v.world.installs.find(i => i.proto === 'machining');
ok(!!cell && cell.macro.gives.some(g => g.item === 'Gear'), 'and it knows it makes gears');

const clash = await cmd(bee, 'PlaceStorage', { proto: 'bay', x: 40, y: 8, face: 0 });
ok(!clash.ok && clash.refused && /overlap/.test(clash.error), `refused: ${clash.error}`);
const nonsense = await cmd(bee, 'CreateConnection', { from: bays[0].id, to: bays[1].id, item: 'Coal' });
ok(!nonsense.ok && /bays/.test(nonsense.error), `refused: ${nonsense.error}`);

console.log('wiring');
const gearbay = await cmd(bee, 'PlaceStorage', { proto: 'bay', x: 62, y: 8, face: 0 });
ok(gearbay.ok, 'a bay for the gears');
v = await frame(bee);
const gears = v.world.installs[v.world.installs.length - 1];
const plant = await cmd(ada, 'PlaceMachine', { proto: 'steamplant', x: 40, y: 30, face: 0 });
ok(plant.ok, 'a steam plant');
v = await frame(ada);
const power = v.world.installs[v.world.installs.length - 1];
const yard = await cmd(ada, 'PlaceStorage', { proto: 'yard', x: 62, y: 30, face: 0 });
ok(yard.ok, 'a yard for the electricity');
v = await frame(ada);
const powerbay = v.world.installs[v.world.installs.length - 1];

for (const [from, to, item] of [
  [caster.id, bays[0].id, 'IronBillet'],
  [coal.id, bays[1].id, 'Coal'],
  [water.id, bays[2].id, 'Water'],
  [bays[0].id, cell.id, 'IronBillet'],
  [cell.id, gears.id, 'Gear'],
  [gears.id, depot.id, 'Gear'],
  [bays[1].id, power.id, 'Coal'],
  [bays[2].id, power.id, 'Water'],
  [power.id, powerbay.id, 'Power'],
  [powerbay.id, cell.id, 'Power'],
]) {
  const r = await cmd(ada, 'CreateConnection', { from, to, item });
  ok(r.ok, `wired ${item}` + (r.ok ? '' : `: ${r.error}`));
}
const belt = await cmd(bee, 'CreateWorldLink', {
  proto: 'belt', from: bays[0].id, to: gears.id, item: 'IronBillet',
});
ok(belt.ok, 'a belt, whose latency nobody typed');
v = await frame(bee);
const haul = v.world.hauls[0];
ok(haul && haul.geometry.seconds > 0, `the belt takes ${haul.geometry.seconds.toFixed(1)}s each way`);
await cmd(bee, 'DeleteWorldLink', { id: haul.id });

v = await frame(ada);
ok(v.world.installs.filter(i => i.running).length >= 6, 'most of the factory is commissioned');
ok(v.plant && v.plant.classes.length > 0, 'and the simulation has something to say about it');

// ------------------------------------------------------------- the machine

console.log('the machine, from the inside');
const opened = await cmd(ada, 'OpenDesign', { id: cell.id });
ok(opened.ok, 'Ada takes out a draft');
const contested = await cmd(bee, 'OpenDesign', { id: cell.id });
ok(!contested.ok, `and Bee cannot: ${contested.error}`);
const scene = await post('/api/form', { code, id: cell.id, draft: true });
ok(scene.ok && scene.batches.length > 0, `the draft builds into ${scene.pieces} pieces`);
ok(scene.units && scene.units.length > 0, 'with a box round every component, for picking');
const before = cell.macro.cycleSeconds;
const comp = await cmd(ada, 'PlaceComponent', { id: cell.id, kind: 'motor', x: 0, y: 12, z: 0 });
ok(comp.ok, 'a motor goes into the draft' + (comp.ok ? '' : `: ${comp.error}`));
v = await frame(ada);
const live = v.world.installs.find(i => i.id === cell.id);
ok(live.macro.cycleSeconds === before, 'and the running machine has not changed');
ok(live.hasDraft && live.editor === ada, 'the lock and the draft are in the document');
const draft = await post('/api/form', { code, id: cell.id, draft: true });
const committed = await cmd(ada, 'CommitMachineDesign', { id: cell.id, design: draft.design });
ok(committed.ok, 'the commit is one command' + (committed.ok ? '' : `: ${committed.error}`));
v = await frame(ada);
const after = v.world.installs.find(i => i.id === cell.id);
ok(!after.hasDraft && after.editor === null, 'the draft is gone');

const insides = await net.inside(cell.id, false);
ok(insides.ok && insides.units.length > 0, `${insides.units.length} components, each with a status`);
ok(insides.units.every(u => !!u.status && Array.isArray(u.ports)), 'and ports with levels on them');
ok(typeof insides.phase === 'number', `read at phase ${insides.phase} of its orbit`);

const copy = await cmd(bee, 'PlaceMachine', {
  proto: 'machining', x: 40, y: 44, face: 0, design: draft.design,
});
ok(copy.ok, 'and it can be duplicated, design and all' + (copy.ok ? '' : `: ${copy.error}`));

// ------------------------------------------------------------ ghosts, sync

console.log('ghosts');
await cmd(bee, 'DeleteStorage', { id: powerbay.id });
v = await frame(bee);
ok(v.ghosts.length === 1, 'a ghost is left where the yard was');
const g = v.ghosts[0];
ok(g.fades > 0 && g.w > 0, `it fades in ${g.fades.toFixed(1)}s`);
const restored = await cmd(bee, 'PlaceStorage', { proto: g.proto, x: g.x, y: g.y, face: g.face });
ok(restored.ok, 'and restoring it is a new placement');

console.log('synchronisation');
await new Promise(r => setTimeout(r, 1400));
const va = await frame(ada);
const vb = await frame(bee);
ok(va.sync.hash && va.sync.hostHash, 'a hash on both sides');
ok(va.sync.agrees !== false, `Ada agrees with the host: ${va.sync.hash}`);
ok(vb.sync.agrees !== false, `Bee agrees with the host: ${vb.sync.hash}`);
ok(va.players.every(p => p.mismatches === 0), 'nobody has diverged');
ok(va.events.length > 0, `${va.events.length} construction events to show`);

// ------------------------------------------------------------- the drawing

console.log('the front end, run for real');
net.state.view = va;
net.state.code = code;
net.state.player = ada;
world.init(el('world'), { onSelect() {} });
world.draw();
const ctx = el('world').getContext();
ok(true, 'the world canvas drew without throwing');

panels.renderGoal(va);
ok(/gears/.test(el('goalbrief').textContent + el('goalbrief').innerHTML), 'the objective is on screen');
panels.renderWho(va);
panels.renderSync(va);
ok(/[0-9a-f]{6}/.test(el('sync').innerHTML), 'the hashes are on screen');
panels.renderFeed(va);
ok(el('feed').innerHTML !== undefined, 'the feed rendered');
panels.renderGhosts(va, (x, y, w, h) => [x, y, w, h]);
panels.renderPalette(cat, () => {});
panels.markTool('place', 'bay');
panels.renderInspector(cell.id, {});
const said = el('inspect').innerHTML;
ok(/Cell|machining|cycle/i.test(said), 'the inspector says what the machine is doing');
panels.renderInspector(bays[1].id, {});
ok(el('inspect').innerHTML.length > 40, 'and what a bay is holding');

// A wire and a transport are the two things a player draws that have no
// building to click, and for a while that meant a mis-wired bay was permanent:
// `under` hit-tests boxes, so a haul was never selectable and a wire had no
// identity to select at all. The factory would say what was wrong with it and
// there was no way to act on the answer.
console.log('taking a wire back');
{
  const canvas = el('world');
  let asked = null;
  const acts = {
    unwire: w => { asked = ['DeleteConnection', w]; },
    unlink: h => { asked = ['DeleteWorldLink', { id: h.id }]; },
  };
  world.init(canvas, { onSelect: id => panels.renderInspector(id, acts) });
  world.focus();
  // Click halfway along a wire, in the pixels the canvas would have got.
  const click = (a, b) => {
    const s = world.view.scale * 7;
    const wx = (a.x + a.w / 2 + b.x + b.w / 2) / 2;
    const wy = (a.y + a.h / 2 + b.y + b.h / 2) / 2;
    for (const f of canvas.on.pointerdown || []) {
      f({ button: 0, shiftKey: false, clientX: world.view.ox + wx * s, clientY: world.view.oy + wy * s });
    }
  };
  const install = id => va.world.installs.find(i => i.id === id);
  click(install(cell.id), install(gears.id));
  ok(typeof world.selection === 'string' && world.selection.startsWith('w:'),
    `clicking a wire selects it: ${world.selection}`);
  el('inspect').querySelectorAll('[data-act]').forEach(b => b.onclick());
  ok(asked && asked[0] === 'DeleteConnection' && asked[1].from === cell.id
     && asked[1].to === gears.id && asked[1].item === 'Gear',
    `and the inspector can delete it: ${JSON.stringify(asked)}`);

  const undone = await cmd(ada, 'DeleteConnection', { from: cell.id, to: gears.id, item: 'Gear' });
  ok(undone.ok, 'the host accepts it' + (undone.ok ? '' : `: ${undone.error}`));
  const after = await frame(ada);
  ok(after.world.conns.length === va.world.conns.length - 1, 'and the wire is gone');
  ok(!!after.world.installs.find(i => i.id === cell.id).idle,
    `the cell now says: ${after.world.installs.find(i => i.id === cell.id).idle}`);
  await cmd(ada, 'CreateConnection', { from: cell.id, to: gears.id, item: 'Gear' });
}

// The same, for a transport -- and the one number that makes a rail look
// broken when it is only slow.
console.log('a rail, and what it is waiting for');
{
  const spare = await cmd(ada, 'PlaceStorage', { proto: 'bay', x: 92, y: 8, face: 0 });
  ok(spare.ok, 'a bay at the far end of the plot');
  const far = (await frame(ada)).world.installs.slice(-1)[0];
  const rail = await cmd(ada, 'CreateWorldLink', {
    proto: 'rail', from: gears.id, to: far.id, item: 'Gear',
  });
  ok(rail.ok, 'a rail between two bays' + (rail.ok ? '' : `: ${rail.error}`));
  const w = await frame(ada);
  net.state.view = w;
  const h = w.world.hauls[w.world.hauls.length - 1];
  ok(h.geometry.load === 5000, `it carries ${h.geometry.load} a trip`);
  const canvas = el('world');
  let asked = null;
  world.init(canvas, { onSelect: id => panels.renderInspector(id, { unlink: x => { asked = x.id; } }) });
  world.focus();
  const a = w.world.installs.find(i => i.id === h.from);
  const b = w.world.installs.find(i => i.id === h.to);
  const s = world.view.scale * 7;
  const wx = (a.x + a.w / 2 + b.x + b.w / 2) / 2, wy = (a.y + a.h / 2 + b.y + b.h / 2) / 2;
  for (const f of canvas.on.pointerdown || []) {
    f({ button: 0, shiftKey: false, clientX: world.view.ox + wx * s, clientY: world.view.oy + wy * s });
  }
  ok(world.selection === h.id, `clicking the rail selects it: ${world.selection}`);
  ok(/full/.test(el('inspect').innerHTML),
    'and the inspector says it leaves with a full load and not before');
  el('inspect').querySelectorAll('[data-act]').forEach(x => x.onclick());
  ok(asked === h.id, 'the rail can be deleted from there too');
  await cmd(ada, 'DeleteWorldLink', { id: h.id });
  net.state.view = va;
}

// Last: does the page's own entry point load at all? `app.js` wires the lobby
// to the DOM on import, and a typo in it is a blank screen that no amount of
// testing the modules underneath would find.
console.log('the page itself');
try {
  const app = await import(pathToUrl(join(web, 'app.js')));
  ok(!!app, 'app.js loads and builds the lobby');
} catch (e) {
  ok(false, `app.js threw on load: ${e}`);
}

console.log(failures ? `\n${failures} failed` : '\nall of it agrees');
process.exit(failures ? 1 : 0);
