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
import nodeNet from 'node:net';

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

// A machine is placed empty and designed. That is the game, and it is checked
// below by designing an extraction head out of two components and a wire.
//
// It is not, however, something a harness can do nine times: a machining cell
// is nine components and twelve wires, and drawing it here would be a test of
// the designer rather than of the client-and-server contract this file is
// about. So the finished designs come off `/api/reference`, which exists for
// exactly this and which nothing in the game reads.
const book = await get('/api/reference');
ok(book.ok && book.designs.length > 0, `${book.designs.length} reference designs, for the harness`);
const designOf = tag => (book.designs.find(d => d.proto === tag) || {}).design;
const find = tag => v.world.installs.find(i => i.proto === tag);

const depot = find('depot');
const bays = v.world.installs.filter(i => i.proto === 'bay');
ok(!!depot, 'the goal furnished the plot with what it is about');

// A room comes with ground, not with working mines: note 1 of the play
// session. The first thing anybody does is put a head on each seam.
ok(v.world.deposits.length > 0, `${v.world.deposits.length} patches of ground to work`);
ok(v.world.deposits.every(d => d.item && d.title && d.yields > 0),
  'each says what it is and what it is worth');
// One chassis for all of them: what a head draws is one word inside its
// design. There is no catalogue answer to ask for any more, so the harness
// does what a player does -- puts an empty head on the seam, opens it, and
// draws two components and a wire.
//
// It is longer than handing over a finished design and it is the point: this
// is the loop experiment 13's third section is about, and if it did not work
// the game would have no way to get material out of the ground at all.
const DRAWS = { IronOre: 'ore', Coal: 'coal', Water: 'water', IronBillet: 'iron', Crude: 'crude' };
async function head(item) {
  const w = (await frame(ada)).world;
  const d = w.deposits.find(g => g.item === item && g.spare > 0);
  ok(!!d, `there is ${item} in the ground`);
  const put = await cmd(ada, 'PlaceMachine', { proto: 'head', x: d.x, y: d.y, face: 0 });
  ok(put.ok, `an empty head on the ${item} ground` + (put.ok ? '' : `: ${put.error}`));
  const id = (await frame(ada)).world.installs.slice(-1)[0].id;

  // Open it, and draw. A fluid comes in through a pump and leaves as liquid;
  // a solid comes in through an inlet and leaves as solid.
  const fluid = item === 'Water' || item === 'Crude';
  const src = fluid ? 'pump' : 'inlet';
  const step = async (type, payload, what) => {
    const r = await cmd(ada, type, payload);
    ok(r.ok, `${what} on the ${item} head` + (r.ok ? '' : `: ${r.error}`));
    return r;
  };
  await step('OpenDesign', { id }, 'a drawing board');
  await step('PlaceComponent', { id, kind: src, x: 0, y: 0, z: 0 }, `an ${src}`);
  await step('PlaceComponent', { id, kind: 'outlet', x: 4, y: 0, z: 0 }, 'an outlet');
  const draft = (await net.form(id, true)).design;
  const inlet = draft.units.find(u => u.kind === src).name;
  const outlet = draft.units.find(u => u.kind === 'outlet').name;
  await step('TuneComponent', { id, unit: inlet, field: 'subst', value: DRAWS[item] },
    `it set to draw ${DRAWS[item]}`);
  await step('ConnectComponent', {
    id,
    from: inlet, fromPort: fluid ? 'water' : 'out',
    to: outlet, toPort: fluid ? 'liquid' : 'solid',
  }, 'a wire to the outlet');
  const done = (await net.form(id, true)).design;
  await step('CommitMachineDesign', { id, design: done }, 'the design committed');

  const built = (await frame(ada)).world.installs.find(i => i.id === id);
  ok(built.makes.includes(item), `and it makes ${item}: ${JSON.stringify(built.makes)}`);
  ok(!built.wants.includes(item), 'without asking a bay for what it is digging');
  return id;
}
const caster = { id: await head('IronBillet') };
const coal = { id: await head('Coal') };
const water = { id: await head('Water') };

// And a head that has nowhere to stand is refused where it can be seen.
const nowhere = await cmd(ada, 'PlaceMachine', { proto: 'head', x: 60, y: 60, face: 0 });
ok(!nowhere.ok && /stand on ground/.test(nowhere.error), `refused: ${nowhere.error}`);

const placed = await cmd(ada, 'PlaceMachine', {
  proto: 'machining', x: 40, y: 8, face: 0, design: designOf('machining'),
});
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
const plant = await cmd(ada, 'PlaceMachine', {
  proto: 'steamplant', x: 40, y: 30, face: 0, design: designOf('steamplant'),
});
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

// -------------------------------------------------------------- real ports
//
// Experiment 13's first change, over the wire the client actually reads. A
// machine's connection points are derived from the design inside it, they
// carry a domain, and two machines may be joined without a shed between them.
console.log('ports, and a wire with no bay in it');
{
  const f = await frame(ada);
  const cellNow = f.world.installs.find(i => i.id === cell.id);
  ok(Array.isArray(cellNow.ports) && cellNow.ports.length > 0,
    `the cell has ${(cellNow.ports || []).length} ports`);
  const power = cellNow.ports.find(p => p.item === 'Power' && !p.out);
  ok(!!power, 'including an electrical input');
  ok(power.domain === 'electrical', `which knows its domain: ${power && power.domain}`);
  ok(power.perSecond > 0, `and its rate: ${power && power.perSecond}`);
  const gear = cellNow.ports.find(p => p.item === 'Gear' && p.out);
  ok(gear && gear.domain === 'material', 'and a material output');
  // Ports are derived, so every one of them is an item the machine really
  // handles -- there is nothing in the list the recipe does not mention.
  ok(cellNow.ports.filter(p => !p.out).every(p => cellNow.wants.includes(p.item)),
    'every input port is something it consumes');
  ok(cellNow.ports.filter(p => p.out).every(p => cellNow.makes.includes(p.item)),
    'every output port is something it produces');

  // A bay has the ports the room gave it, and no others.
  const bay = f.world.installs.find(i => i.id === bays[0].id);
  ok((bay.ports || []).some(p => p.item === 'IronBillet'),
    'a bay offers what has been wired into it');

  // Two machines, joined directly. This was a refusal until now.
  const p2 = await cmd(ada, 'PlaceMachine', {
    proto: 'steamplant', x: 4, y: 60, face: 0, design: designOf('steamplant'),
  });
  ok(p2.ok, 'a second steam plant' + (p2.ok ? '' : `: ${p2.error}`));
  const plantB = (await frame(ada)).world.installs.slice(-1)[0];
  const c2 = await cmd(ada, 'PlaceMachine', {
    proto: 'machining', x: 30, y: 60, face: 0, design: designOf('machining'),
  });
  ok(c2.ok, 'and a second machining cell' + (c2.ok ? '' : `: ${c2.error}`));
  const cellB = (await frame(ada)).world.installs.slice(-1)[0];

  const direct = await cmd(ada, 'CreateConnection', {
    from: plantB.id, to: cellB.id, item: 'Power',
  });
  ok(direct.ok, 'a generator powers a machine directly' + (direct.ok ? '' : `: ${direct.error}`));

  const w = (await frame(ada)).world;
  const wire = w.conns.find(c => c.from === plantB.id && c.to === cellB.id);
  ok(!!wire, 'and the document has it');
  ok(wire.domain === 'electrical', `the wire knows what it carries: ${wire.domain}`);
  ok(wire.unit === 'MW', `and the unit to say it in: ${wire.unit}`);
  ok(!!wire.buffer && wire.capacity > 0,
    `with a buffer the player never placed: ${wire.buffer} holding ${wire.capacity}`);
  // A wire that goes through a bay has no buffer of its own: the bay is one.
  const viaBay = w.conns.find(c => c.from === cell.id && c.to === gears.id);
  ok(viaBay && !viaBay.buffer, 'a wire into a bay does not invent a second one');

  // Nothing was added to the document to hold it.
  const named = new Set(w.installs.map(i => i.name));
  ok(!named.has(wire.buffer), 'the derived buffer is not a building');

  // The refusals that are left, in the words the player sees.
  const backwards = await cmd(ada, 'CreateConnection', {
    from: cellB.id, to: plantB.id, item: 'Power',
  });
  ok(!backwards.ok && /no power output/.test(backwards.error), `refused: ${backwards.error}`);
  const twice = await cmd(ada, 'CreateConnection', {
    from: power2Source(w, cellB.id), to: cellB.id, item: 'Power',
  });
  ok(!twice.ok, `a second supply of one item is refused: ${twice.error}`);
}

function power2Source(w, notThis) {
  const p = w.installs.find(i => i.proto === 'steamplant' && i.id !== notThis);
  return p ? p.id : 0;
}

// ------------------------------------------------- connecting from a port
//
// Note 6 wanted the wire tools out of a scrolling side menu and onto the
// building being wired; note 10 wanted the game to stop asking questions whose
// answers are already determined. They are the same panel.
console.log('the connect panel');
{
  net.state.view = await frame(ada);
  const cellNow = net.byId(cell.id);

  panels.renderInspector(cell.id, {});
  const html = el('inspect').innerHTML;
  ok(/data-act="connect"/.test(html), 'the inspector offers the ports as buttons');
  ok(/data-item="Power"/.test(html), 'including the electrical one');
  ok(/class="ports"/.test(html), 'grouped into in and out');

  // Clicking one arms a connection from that port, so the next click on the
  // canvas is a destination and no menu is shown at all.
  world.setTool('pick');
  world.connectFrom(cell.id, 'Gear');
  ok(world.tool.mode === 'connect' && world.tool.from === cell.id && world.tool.item === 'Gear',
    'clicking a port starts a wire from it');
  // Changing tool must not leave a half-chosen port behind.
  world.setTool('pick');
  ok(world.tool.item === null && world.tool.from === null, 'and switching tools forgets it');

  // What could cross between two things, which is what decides whether a menu
  // is shown at all.
  const bay0 = net.byId(bays[0].id);
  const both = world.mating(bay0, cellNow);
  ok(both.length === 1 && both[0] === 'IronBillet',
    `a bay with one item in it needs no menu: ${JSON.stringify(both)}`);
  const plantNow = net.byId(power.id);
  ok(world.mating(plantNow, cellNow).includes('Power'),
    'a generator and a machine agree about electricity');
  ok(world.mating(cellNow, plantNow).length === 0,
    'and a machine has nothing the generator that feeds it wants');

  // The wire panel says what the connection is, in the vocabulary it now has.
  const direct = net.state.view.world.conns.find(c => c.buffer);
  panels.renderInspector(net.wireKey(direct), {});
  const wireHtml = el('inspect').innerHTML;
  ok(/electrical|material|fluid/.test(wireHtml), 'a wire says which domain it is');
  ok(/buffered/.test(wireHtml), 'and what is sitting in it');

  world.draw();
  ok(true, 'the canvas draws ports and routed wires without throwing');
}

// ------------------------------------------------------------- legibility
//
// Notes 3, 8 and 12: a building that says what it is taking and giving, an
// objective that distinguishes what you have done from what you are doing, and
// a restore that brings the wiring back with the building.
console.log('reading the room');
{
  net.state.view = await frame(ada);
  const v = net.state.view;

  // ---- the objective, live and latched
  ok(v.goal.progress.lines.every(l => l.kind === 'achievement' || l.kind === 'state'),
    'every requirement says which sort it is');
  ok(typeof v.goal.progress.holding === 'boolean',
    'and the room says whether it is doing it right now');
  ok(Array.isArray(v.goal.progress.slipped), 'naming anything that has stopped');
  panels.renderGoal(v);
  // The panel appends one element per requirement, so the marks are on the
  // children rather than in the box's own markup.
  const marks = (el('goallines').children || []).map(c => c.innerHTML || '').join('');
  ok(/class="kind"/.test(marks), `the panel marks each requirement: ${marks.slice(0, 80)}`);

  // ---- the inspector says what is coming in and going out
  panels.renderInspector(cell.id, {});
  const html = el('inspect').innerHTML;
  ok(/<h2>taking<\/h2>/.test(html), 'the inspector says what the cell takes');
  ok(/<h2>giving<\/h2>/.test(html), 'and what it gives');
  ok(/available|not connected/.test(html), 'with the state of each input');

  // Hovering populates it, and does not need a click.
  let hoveredAs = null;
  world.init(el('world'), {
    onHover: id => { hoveredAs = id; },
    onSelect() {},
  });
  world.focus();
  world.setTool('pick');
  const target = v.world.installs.find(i => i.id === cell.id);
  const sc = world.view.scale * 7;
  for (const f of el('world').on.pointermove || []) {
    f({
      clientX: world.view.ox + (target.x + target.w / 2) * sc,
      clientY: world.view.oy + (target.y + target.h / 2) * sc,
    });
  }
  ok(hoveredAs === cell.id, `hovering a building offers it to the inspector: ${hoveredAs}`);
}

// -------------------------------------------------------- restoring wiring
console.log('a tombstone, not a headstone');
{
  const before = (await frame(ada)).world;
  const victim = before.installs.find(i => i.id === cell.id);
  const wires = before.conns.filter(c => c.from === victim.id || c.to === victim.id).length;
  ok(wires >= 2, `the cell is wired ${wires} ways`);

  await cmd(ada, 'DeleteMachine', { id: victim.id });
  const gone = await frame(ada);
  const g = gone.ghosts.find(x => x.proto === 'machining');
  ok(!!g, 'a ghost is left where the cell was');
  ok(g.conns === wires, `the ghost remembers all ${wires} connections: ${g.conns}`);
  ok(g.restore && g.restore.type === 'Restore', 'and carries the command that puts it back');
  ok(!!g.restore.payload.design, 'design and all -- which the browser used to drop');

  const put = await post('/api/cmd', {
    code, player: ada, type: g.restore.type, payload: g.restore.payload,
  });
  ok(put.ok, 'restoring is one command' + (put.ok ? '' : `: ${put.error}`));
  const after = (await frame(ada)).world;
  const back = after.installs.find(i => i.x === g.x && i.y === g.y);
  ok(!!back, 'the cell is back');
  ok(back.id !== victim.id, 'as a new placement rather than a rollback');
  const rewired = after.conns.filter(c => c.from === back.id || c.to === back.id).length;
  ok(rewired === wires, `and every one of its ${wires} connections came with it: ${rewired}`);
  ok(!!back.macro, 'with its design intact');

  const feed = (await frame(ada)).events;
  ok(feed.some(e => e.verb === 'restore'), 'and the feed says what came back');
}

// ---------------------------------------------------------------- the beat
//
// The room keeps its own time. A browser that stops asking used to stop its
// replica with it, and the poll that eventually came back had to carry the
// whole gap in one call -- holding the room's lock, with the *other* player's
// poll queued behind it. The player who froze was never the one who walked
// away.
console.log('the beat');
{
  // Nobody polls for Bee for a second and a half, which is six beats.
  const before = (await frame(ada)).tick;
  await new Promise(r => setTimeout(r, 1500));
  const after = await frame(ada);
  ok(after.tick > before, `the clock ran on: ${before} -> ${after.tick}`);

  // Never more than one beat: whatever it trails by is the gap between the
  // last beat and the instant the frame was cut, and that is the bound the
  // beat exists to put on it. It used to be the whole ninety.
  const beat = after.sync.beat;
  ok(beat > 0 && beat < 60, `the frame says how often the room beats: ${beat} ticks`);
  const quiet = after.players.find(p => p.id === bee);
  ok(quiet.behind <= beat,
    `Bee's replica was carried anyway: ${quiet.behind} ticks behind, one beat is ${beat}`);
  ok(quiet.away > 60, `and the room knows Bee is not watching: away ${quiet.awaySeconds}s`);
  const watching = after.players.find(p => p.id === ada);
  ok(watching.away < 60, `while Ada, who is, is not marked away: ${watching.away}`);

  // The poll Bee eventually sends is a read, not a catch-up.
  const back = await frame(bee);
  ok(back.ok && back.sync.behind <= beat,
    `Bee's poll had at most a beat to catch up: ${back.sync.behind}`);
  ok((await frame(ada)).players.find(p => p.id === bee).away < 60, 'coming back clears away');

  // Being away is not diverging, and must not be counted as one.
  const bees = (await frame(bee)).players.find(p => p.id === bee);
  ok(bees.resyncs === 0 && bees.mismatches === 0,
    `a quiet browser was not treated as a broken one: ${bees.resyncs}/${bees.mismatches}`);
}

// ---------------------------------------------------------------- the picture
//
// The other half: whether this screen is being *told* about a room that is
// fine. Those look identical to a player, which is how a play session reports
// a freeze that never happened.
console.log('how current the picture is');
{
  ok(net.health && typeof net.health.lag === 'number', 'the client reports its own health');

  // A poll that answers: live, with a round trip on it, and the header stays
  // out of the way.
  const beats = [];
  net.onHealth(h => beats.push({ ...h }));
  net.start(180);
  await new Promise(r => setTimeout(r, 500));
  net.stop();
  ok(beats.length > 0, `${beats.length} health reports`);
  const last = beats[beats.length - 1];
  ok(last.live && last.misses === 0, `the connection is live: ${JSON.stringify(last)}`);
  ok(last.rtt >= 0 && last.lag < 2000, `and current: ${last.rtt} ms round trip, ${last.lag} ms stale`);
  panels.renderLink(last);
  ok(el('link').textContent === '', 'a healthy connection says nothing');

  // A stale one does. The panel is the thing under test here, not the network:
  // a browser that has been throttled for a minute is not something a test can
  // wait for, and the number it would produce is the one being handed over.
  panels.renderLink({ lag: 9000, rtt: 40, misses: 0, behind: 0, live: false });
  ok(/catching up/.test(el('link').textContent), `a stale one says so: ${el('link').textContent}`);
  panels.renderLink({ lag: 30000, rtt: 0, misses: 9, behind: 0, live: false });
  ok(/no answer/.test(el('link').textContent), `and a dead one says that: ${el('link').textContent}`);

  // The header dims somebody who has stopped watching, and not somebody whose
  // replica is behind -- with the beat running, nobody's is.
  const v2 = await frame(ada);
  panels.renderWho({ ...v2, players: [{ ...v2.players[0], away: 600, awaySeconds: 10, behind: 0 }] });
  ok(/behind/.test(el('who').children[0].className), 'a player who is away is dimmed');
}

// ------------------------------------------------------------------ the door
//
// What arrives on the socket before any of the above is HTTP, and what arrives
// on it in practice is not always HTTP. A browser told to open a bare
// `host:port` tries `https://` first and sends a TLS ClientHello, which the
// server used to read with `read_line`, fail on as invalid UTF-8, and drop
// without an answer -- one indistinguishable log line per attempt, and a
// socket left hanging until the browser gave up.
console.log('the door');
{
  // Read to the close rather than to the first packet: the answer is
  // `Connection: close`, and a header and its body can arrive separately.
  const raw = (bytes, wait = 3000) => new Promise(resolve => {
    const s = new nodeNet.Socket();
    let got = Buffer.alloc(0);
    let settled = false;
    const done = () => {
      if (settled) return;
      settled = true;
      clearTimeout(timer);
      s.destroy();
      resolve(got.toString('latin1'));
    };
    const timer = setTimeout(done, wait);
    s.connect(Number(port), '127.0.0.1', () => s.write(bytes));
    s.on('data', d => { got = Buffer.concat([got, d]); });
    s.on('close', done);
    s.on('end', done);
    s.on('error', done);
  });

  // A TLS record begins 0x16 and its random block is not UTF-8.
  const hello = Buffer.concat([
    Buffer.from('1603010200010001fc0303', 'hex'),
    Buffer.alloc(112, 0xc8),
  ]);
  const tls = await raw(hello);
  ok(/^HTTP\/1\.1 400/.test(tls), `an https attempt is answered, not dropped: ${tls.slice(0, 24)}`);
  ok(/http, not https/.test(tls), 'and told which protocol this port speaks');

  const junk = await raw(Buffer.from([0x00, 0x01, 0x02, 0x03, 0x0d, 0x0a, 0x0d, 0x0a]));
  ok(/^HTTP\/1\.1 400/.test(junk), 'so is anything else that is not a request');

  // And the door still works.
  const fine = await raw(Buffer.from('GET /api/catalogue HTTP/1.1\r\nHost: x\r\n\r\n'));
  ok(/^HTTP\/1\.1 200/.test(fine), 'a real request is unaffected');
}

// --------------------------------------------------------------- coming back
//
// A seat is everything a player owns. A refresh used to take a second one,
// leaving the first holding the factory that the person in front of the screen
// had just built.
console.log('coming back');
{
  const key = 'seat-test-' + seed;
  const first = await post('/api/join', { code, name: 'Cy', key });
  ok(first.ok && !first.rejoined, `Cy took seat ${first.player}`);
  // A replica's sequence must never run ahead of the log. When it did, this
  // seat silently skipped the next command it was sent -- see
  // `a_refusal_leaves_no_trace` in tests/mp.rs.
  const atJoin = await frame(first.player);
  const seated = atJoin.players.find(p => p.id === first.player);
  ok(seated.seq <= atJoin.commands,
    `a fresh seat is not ahead of the log: seq ${seated.seq}, log ${atJoin.commands}`);
  const built = await cmd(first.player, 'PlaceStorage', { proto: 'bay', x: 74, y: 24, face: 0 });
  ok(built.ok, `Cy can build` + (built.ok ? '' : `: ${built.error}`));
  const fr = await frame(first.player);
  const mine = fr.world.installs.find(i => i.x === 74 && i.y === 24);
  ok(!!mine, 'and built something on it');

  // The reload: the same token, and no memory of the name or the id.
  const back = await post('/api/join', { code, key });
  ok(back.ok && back.rejoined, 'the room recognised the browser');
  ok(back.player === first.player, `the same seat: ${back.player}`);
  ok(back.name === 'Cy', `and the same name: ${back.name}`);
  ok(back.host === false, 'which is not the host seat');
  const after = (await frame(back.player)).world.installs.find(i => i.id === mine.id);
  ok(!!after, 'with everything the seat had built still on it');

  const me = (await frame(back.player)).players.find(p => p.id === back.player);
  ok(me.rejoins === 1 && me.resyncs === 0, `counted as a rejoin, not a correction: ${me.rejoins}/${me.resyncs}`);

  // A token is the identity; a name is not.
  const twin = await post('/api/join', { code, name: 'Cy', key: key + '-other' });
  ok(twin.ok && !twin.rejoined && twin.player !== back.player, 'a different browser is a different seat');

  // The host seat is the one with the start button, and says so, so that a
  // host who reloaded before starting the clock gets it back.
  const hostBack = await post('/api/join', { code, key: net.seat(), back: true });
  ok(hostBack.ok, 'the harness itself can come back');
  ok(hostBack.player === ada && hostBack.host === true, 'to seat one, marked as the host');

  // A code left in storage must not seat a stranger. Rooms are named from
  // their seed, so the same code comes round again on a server that restarted.
  const stale = await post('/api/join', { code, key: 'seat-never-been-here', back: true });
  ok(!stale.ok, `coming back to a seat that is gone is refused: ${stale.error}`);
  const players = (await frame(ada)).players.length;
  await post('/api/join', { code, key: 'seat-never-been-here-either', back: true });
  ok((await frame(ada)).players.length === players, 'and leaves no phantom behind');
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
