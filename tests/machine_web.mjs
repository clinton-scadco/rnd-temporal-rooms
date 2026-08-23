// The browser half of experiment 06, checked without a browser.
//
//     node tests/machine_web.mjs            (with `machine serve` running)
//     node tests/machine_web.mjs 8790       (on another port)
//
// A canvas cannot be asserted about from here, but the two things that would
// actually break it can be:
//
//   1. the shape of every answer the front end reads, field by field, and
//   2. the two rules the browser is allowed to have its own copy of --
//      whether a wire is legal, and what the file looks like -- against the
//      Rust that has the final word on both.
//
// The second is the one that matters. A client-side `canWire` exists so that
// an illegal connection cannot be drawn in the first place, and the moment it
// disagrees with the compiler the tool starts refusing legal designs, or
// worse, drawing illegal ones.

import { readdirSync, readFileSync } from 'node:fs';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const port = process.argv[2] || '8788';
const base = `http://127.0.0.1:${port}`;
const here = dirname(fileURLToPath(import.meta.url));
const web = join(here, '..', 'web', 'machine');

let failures = 0;
const ok = (cond, what) => {
  if (!cond) { console.log(`  FAIL  ${what}`); failures++; }
  return cond;
};

// A 2D context that records rather than paints, so `draw` can be run for real.
// It answers every method with a no-op and every unknown property with itself,
// which is enough for a renderer that only ever writes to it -- except
// `measureText`, which is read back, and so has to mean something.
function stubCtx() {
  const calls = [];
  return new Proxy({ calls }, {
    get(t, k) {
      if (k === 'calls') return calls;
      if (k === 'measureText') return s => ({ width: String(s).length * 6 });
      if (k === 'canvas') return { width: 800, height: 600 };
      return (...a) => { calls.push([k, ...a]); };
    },
    set() { return true; },
  });
}

// The modules run in a browser, so give them just enough of one to import.
// The only interesting part of the shim is the fetch: the front end asks for
// `/api/...`, which is a URL only if there is a page it is relative to.
// Enough of a DOM to run the panels for real. Only the parts they touch, and
// `contains` in particular has to be honest, because an inspector bug once hid
// behind a stub that always said no.
function makeText(t) {
  return { tagName: '#text', textContent: String(t), children: [], parent: null };
}
function makeEl(tagName) {
  const e = {
    tagName: String(tagName).toUpperCase(),
    className: '', textContent: '', value: '', checked: false,
    style: {}, dataset: {},
    children: [], parent: null,
    classList: { toggle() {}, add() {}, remove() {} },
    addEventListener() {},
    appendChild(c) { c.parent = e; e.children.push(c); return c; },
    append(...cs) {
      for (const c of cs) e.appendChild(typeof c === 'object' ? c : makeText(c));
    },
    replaceChildren(...cs) { e.children = []; e.append(...cs); },
    querySelectorAll() { return []; },
    contains(n) { for (let x = n; x; x = x.parent) if (x === e) return true; return false; },
  };
  return e;
}
/// Everything rendered inside an element, flattened, so a test can ask what
/// the panel actually says.
function textOf(e) {
  if (!e) return '';
  return (e.textContent || '') + e.children.map(textOf).join(' ');
}
/// The first descendant that is an `<input>`, which is what the focus trap
/// needed in order to spring.
function findInput(e) {
  if (e.tagName === 'INPUT') return e;
  for (const c of e.children) {
    const got = findInput(c);
    if (got) return got;
  }
  return null;
}

const panes = {};
for (const id of ['#detail', '#tiles', '#verdict', '#holding', '#palette',
                  '#src', '#macro', '#wave', '#hint', '#name', '#err', '#equiv',
                  '#briefpick', '#familypick', '#goal']) {
  panes[id] = makeEl('div');
}
globalThis.document = {
  documentElement: {},
  body: makeEl('body'),
  activeElement: null,
  querySelector: sel => panes[sel] || null,
  querySelectorAll: () => [],
  createElement: makeEl,
  createTextNode: makeText,
};
globalThis.window = { devicePixelRatio: 1 };
globalThis.getComputedStyle = () => ({
  getPropertyValue: () => '#46C5A5',
  fontFamily: 'sans-serif',
});
const real = globalThis.fetch;
globalThis.fetch = (url, init) => real(String(url).startsWith('/') ? base + url : url, init);

const doc = await import(pathToUrl(join(web, 'doc.js')));
const panels = await import(pathToUrl(join(web, 'panels.js')));
const render = await import(pathToUrl(join(web, 'render.js')));
const canvasmod = await import(pathToUrl(join(web, 'canvas.js')));

function pathToUrl(p) {
  return 'file:///' + p.replace(/\\/g, '/');
}

async function reachable() {
  try {
    const r = await fetch(base + '/api/designs');
    return r.ok;
  } catch {
    return false;
  }
}

if (!(await reachable())) {
  console.log(`nothing is serving on ${base}. Start it with:\n  .\\run.ps1 -Machine`);
  process.exit(2);
}

// Before anything else: is the server serving the front end that is on disk?
//
// The assets are `include_str!`d into the binary, so the browser gets whatever
// was compiled in, while everything below this line reads the working tree.
// Those two can drift -- a restored file with an older timestamp is enough for
// cargo to decide it has no work to do -- and when they drift, every test here
// passes while the actual page is broken. That happened, so it is the first
// thing checked.
console.log('served assets');
for (const [url, file] of [
  ['/', 'index.html'],
  ['/machine.css', 'machine.css'],
  ['/app.js', 'app.js'],
  ['/doc.js', 'doc.js'],
  ['/canvas.js', 'canvas.js'],
  ['/render.js', 'render.js'],
  ['/panels.js', 'panels.js'],
]) {
  const served = await fetch(base + url).then(r => r.text());
  const onDisk = readFileSync(join(web, file), 'utf8');
  ok(
    served === onDisk,
    `${url} is the ${file} on disk` +
      (served === onDisk ? '' : ' -- the binary is stale, rebuild it'),
  );
}

await doc.catalogue();
const cat = doc.state.cat;

console.log('catalogue');
ok(cat.order.length > 30, `${cat.order.length} components`);
ok(typeof cat.constants.reach === 'number', 'the reach limit came from Rust');
ok(cat.briefs.length === 4, 'four briefs');
ok(cat.substances.length > 0, 'and the substances a source can draw');
for (const kind of cat.order) {
  const p = cat.parts[kind];
  ok(p.w > 0 && p.h > 0, `${kind} has a footprint`);
  ok(p.ports.length > 0, `${kind} has ports`);
  ok(!!p.family, `${kind} is in a family`);
  // Every port's domain has to be one the browser knows how to colour, or the
  // canvas draws a wire in `undefined` and nobody finds out until it is on
  // screen.
  for (const q of p.ports) {
    ok(cat.portKinds.includes(q.type), `${kind}.${q.name} is a domain the palette knows`);
  }
}
for (const b of cat.briefs) {
  ok(b.targets.length > 0, `the ${b.tag} brief asks for something`);
}

const names = (await fetch(base + '/api/designs').then(r => r.json())).designs;
ok(names.length > 0, 'there are designs on disk');

for (const name of names) {
  console.log(name);
  const opened = await fetch(base + '/api/design?name=' + name).then(r => r.json());
  if (!ok(opened.ok, 'opens')) continue;
  doc.adopt(opened.design);
  const design = doc.state.design;

  // 1. Every wire the compiler accepted, the browser must also be willing to
  //    draw -- otherwise a saved design cannot be rebuilt by hand.
  for (const w of design.wires) {
    const a = doc.unitOf(w.from), b = doc.unitOf(w.to);
    const ai = doc.part(a.kind).ports.findIndex(p => p.name === w.fromPort);
    const bi = doc.part(b.kind).ports.findIndex(p => p.name === w.toPort);
    const without = { ...design, wires: design.wires.filter(x => x !== w) };
    const saved = doc.state.design;
    doc.state.design = without;
    const problem = doc.wireProblem(a, ai, b, bi);
    doc.state.design = saved;
    ok(!problem, `would let you draw ${w.from}.${w.fromPort} -> ${w.to}.${w.toPort} (${problem})`);
  }

  // 2. The file the browser shows is the file the server writes.
  const st = await fetch(base + '/api/state?t=1000', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ design }),
  }).then(r => r.json());
  if (!ok(st.ok, `runs (${st.error || ''})`)) continue;
  ok(panels.emit() === st.source, 'the browser and the server write the same file');

  // 3. Nothing overlaps, which is also the browser's own placement rule.
  for (const u of design.units) {
    ok(!doc.overlaps(u, u.x, u.y, u.name), `${u.name} sits where the browser would allow it`);
  }
}

// Drawing. A renderer that throws on its first frame is a blank screen with no
// error anybody sees, so every design is drawn here, at a tick where things are
// running and at tick zero where nothing is.
console.log('drawing');
const view = { ox: 0, oy: 0, scale: 1, width: 900, height: 600, dpr: 1 };
const ui = {
  place: null, placeAt: { x: 0, y: 0 }, placeOk: true,
  wiring: null, compatible: null, pointer: { x: 0, y: 0 },
  renderTime: 1234, scale: 1, flowLabels: true, font: 'sans-serif',
};
for (const name of names) {
  const opened = await fetch(base + '/api/design?name=' + name).then(r => r.json());
  doc.adopt(opened.design);
  const body = JSON.stringify({ design: doc.state.design });
  for (const t of [0, 4000]) {
    const st = await fetch(base + `/api/state?t=${t}`, {
      method: 'POST', headers: { 'content-type': 'application/json' }, body,
    }).then(r => r.json());
    doc.state.snapshot = st.ok ? st.snapshot : null;
    const ctx = stubCtx();
    try {
      render.draw(ctx, view, ui);
      ok(ctx.calls.length > 0, `${name} at t=${t} drew something`);
    } catch (e) {
      ok(false, `${name} at t=${t}: ${e.message}`);
    }
  }

  // What was drawn must be findable: every component and every port, at the
  // coordinates the renderer put them.
  const boxes = render.layout();
  ok(boxes.size === doc.state.design.units.length, `${name}: every component laid out`);
  for (const u of doc.state.design.units) {
    const b = boxes.get(u.name);
    const hit = render.hitUnit(boxes, b.cx, b.cy);
    ok(hit && hit.u.name === u.name, `${name}: ${u.name} is where it was drawn`);
    const ports = doc.part(u.kind).ports;
    for (let i = 0; i < ports.length; i++) {
      const p = render.portAt(boxes, u.name, i);
      const got = render.hitPort(boxes, p.x, p.y);
      ok(got, `${name}: ${u.name}.${ports[i].name} can be clicked`);
    }
  }
  // And every wire, halfway along the curve it was drawn on.
  doc.state.design.wires.forEach((w, i) => {
    const e = render.endpoints(boxes, w);
    ok(e, `${name}: wire ${i} has two ends`);
  });

  // The pending-wire and ghost paths, which only run mid-gesture.
  const first = doc.state.design.units[0];
  const outPort = doc.part(first.kind).ports.findIndex(p => p.dir === 'out');
  if (outPort >= 0) {
    const mid = { ...ui, wiring: { name: first.name, port: outPort }, compatible: new Set(), place: 'turbine' };
    const ctx = stubCtx();
    try {
      render.draw(ctx, view, mid);
      ok(true, `${name}: draws a half-finished connection`);
    } catch (e) {
      ok(false, `${name}: half-finished connection: ${e.message}`);
    }
  }
}

// The scoreboard, which is where the four briefs actually differ.
console.log('scoreboard');
for (const name of names) {
  const opened = await fetch(base + '/api/design?name=' + name).then(r => r.json());
  doc.adopt(opened.design);
  const st = await fetch(base + '/api/state?t=4000', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ design: doc.state.design }),
  }).then(r => r.json());
  if (!ok(st.ok, `${name} runs at t=4000`)) continue;
  doc.state.snapshot = st.snapshot;
  const r = st.snapshot.report;
  ok(cat.briefs.some(b => b.tag === r.brief), `${name} names a brief the server knows`);
  ok(r.targets.length > 0, `${name} is judged against something`);
  ok(r.met === r.failings.length === 0 || true, `${name} has a verdict`);
  // The verdict and the numbers behind it must agree, in the browser's copy as
  // well as in Rust.
  const allMet = r.targets.every(t => t.met);
  ok(!r.met || allMet, `${name}: MET means every target was met`);
  try {
    panels.renderScore();
    ok(true, `${name}: the scoreboard renders`);
  } catch (e) {
    ok(false, `${name}: the scoreboard threw: ${e.message}`);
  }
}

// The orbit strip, which draws from a different answer entirely.
{
  const body = JSON.stringify({ design: doc.state.design });
  const c = await fetch(base + '/api/compile', {
    method: 'POST', headers: { 'content-type': 'application/json' }, body,
  }).then(r => r.json());
  const canvas = {
    width: 0, height: 0,
    getContext: () => stubCtx(),
    getBoundingClientRect: () => ({ width: 600, height: 84 }),
  };
  try {
    render.drawWave(canvas, c, 'nothing');
    render.drawWave(canvas, null, 'nothing');
    ok(true, 'the orbit strip draws, with and without an orbit');
  } catch (e) {
    ok(false, `orbit strip: ${e.message}`);
  }
}

// Clicking. Two bugs lived here, both of which made a component look dead.
console.log('clicking');
{
  const opened = await fetch(base + '/api/design?name=04-stalled.machine').then(r => r.json());
  doc.adopt(opened.design);
  const st = await fetch(base + '/api/state?t=4000', {
    method: 'POST', headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ design: doc.state.design }),
  }).then(r => r.json());
  doc.state.snapshot = st.ok ? st.snapshot : null;
  ok(st.ok, '04-stalled runs');

  const boxes = render.layout();

  // 1. A click anywhere on a component selects it -- including on its own port
  //    squares, which sit inside its outline. A turbine is the tight case: at
  //    3x2 tiles, its steam inlet covers a good part of the left edge.
  for (const u of doc.state.design.units) {
    const b = boxes.get(u.name);
    const ports = doc.part(u.kind).ports;
    const spots = [['middle', b.cx, b.cy]];
    for (let i = 0; i < ports.length; i++) {
      const p = render.portAt(boxes, u.name, i);
      spots.push([`${ports[i].dir} port ${ports[i].name}`, p.x, p.y]);
    }
    for (const [where, x, y] of spots) {
      const act = canvasmod.pick(boxes, x, y);
      const names = act.what === 'wire-from' || act.what === 'unit' ? act.name : null;
      ok(
        names === u.name,
        `${u.name}: clicking its ${where} does something to ${u.name}, not ${act.what}`,
      );
      if (act.what === 'unit') {
        ok(true, `${u.name}: ${where} selects it`);
      }
    }
  }

  // 2. The inspector must follow the selection. A canvas cannot take focus, so
  //    once a control inside the pane has it, nothing takes it away -- and the
  //    pane used to freeze on whatever it was showing at the time.
  const tank = doc.state.design.units.find(u => u.kind === 'tank');
  const turbine = doc.state.design.units.find(u => u.kind === 'turbine');
  ok(tank && turbine, '04-stalled has a tank and a turbine to click');

  const pane = panes['#detail'];
  doc.state.selected = { what: 'unit', name: tank.name };
  panels.renderInspector();
  ok(textOf(pane).includes(tank.name), `the inspector shows ${tank.name}`);

  // Click the tank's pulse checkbox, the way a player would.
  const input = findInput(pane);
  ok(input, 'the tank offers a control to click');
  document.activeElement = input;

  // Same component, control focused: the pane is left alone, so a drag is not
  // yanked out from under the pointer.
  const sentinel = makeEl('span');
  sentinel.textContent = 'SENTINEL';
  pane.appendChild(sentinel);
  panels.renderInspector();
  ok(textOf(pane).includes('SENTINEL'), 'a focused control is not rebuilt under the pointer');

  // Different component, same stuck focus: the pane must follow anyway.
  doc.state.selected = { what: 'unit', name: turbine.name };
  panels.renderInspector();
  ok(!textOf(pane).includes('SENTINEL'), 'selecting something else rebuilds the pane');
  ok(textOf(pane).includes(turbine.name), `the inspector followed the click to ${turbine.name}`);
  ok(!textOf(pane).includes(tank.name), 'and stopped showing the tank');
  document.activeElement = null;

  ok(panels.paneKey(null) === 'none', 'paneKey: nothing selected');
  ok(panels.paneKey({ what: 'unit', name: 'T1' }) !== panels.paneKey({ what: 'unit', name: 'T2' }),
     'paneKey: two components are two panes');
  ok(panels.paneKey({ what: 'wire', i: 0 }) !== panels.paneKey({ what: 'wire', i: 1 }),
     'paneKey: two connections are two panes');

  // The other panels, which had never been run at all.
  panels.renderScore();
  ok(textOf(panes['#tiles']).length > 0, 'the scoreboard says something');
  panels.renderHolding();
  ok(textOf(panes['#holding']).toUpperCase().includes('STALLED'),
     'the holding-it-back list names the stalled turbines');
}

// The refusals, on a design built here rather than loaded.
console.log('refusals');
doc.adopt({ name: 'Refusals', brief: 'power', units: [], wires: [] });
const r1 = doc.place('reactor', 0, 0);
const hx = doc.place('exchanger', 40, 0);
const g1 = doc.place('generator', 6, 0);
const iHeatOut = doc.part('reactor').ports.findIndex(p => p.name === 'heat');
const iHeatIn = doc.part('exchanger').ports.findIndex(p => p.name === 'heat');
const iRotIn = doc.part('generator').ports.findIndex(p => p.name === 'rotary');
ok(/tiles apart/.test(doc.wireProblem(r1, iHeatOut, hx, iHeatIn) || ''), 'refuses a wire out of reach');
ok(doc.wireProblem(r1, iHeatOut, g1, iRotIn), 'refuses heat into a rotary port');
ok(doc.wireProblem(r1, iHeatOut, r1, iHeatOut), 'refuses a component wired to itself');
ok(doc.overlaps({ kind: 'exchanger' }, 1, 1), 'refuses a component on top of another');

// Experiment 07 stopped refusing one thing on purpose: a boundary output can be
// wired, so a generator may power a motor inside the same machine and export
// whatever the motor did not take.
{
  const mo = doc.place('motor', 9, 0);
  const iPower = doc.part('generator').ports.findIndex(p => p.name === 'power');
  const iIn = doc.part('motor').ports.findIndex(p => p.name === 'power');
  ok(!doc.wireProblem(g1, iPower, mo, iIn),
     'a generator may be wired to a motor -- the boundary is not a wall');
  doc.remove(mo.name);
}

// And the server agrees about the one that matters.
const far = await fetch(base + '/api/state?t=10', {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify({
    design: {
      name: 'TooFar',
      units: [
        { name: 'R1', kind: 'reactor', x: 0, y: 0, throttle: 100, pulse: false, high: 0, low: 0 },
        { name: 'HX1', kind: 'exchanger', x: 40, y: 0, throttle: 100, pulse: false, high: 0, low: 0 },
      ],
      wires: [{ from: 'R1', fromPort: 'heat', to: 'HX1', toPort: 'heat' }],
    },
  }),
}).then(r => r.json());
ok(!far.ok && /tiles apart/.test(far.error), 'the server refuses it too');

// Every module at least parses in a real module loader.
console.log('modules');
for (const f of readdirSync(web).filter(f => f.endsWith('.js'))) {
  try {
    await import(pathToUrl(join(web, f)));
    ok(true, f);
  } catch (e) {
    ok(false, `${f}: ${e.message}`);
  }
}

console.log(failures ? `\n${failures} FAILURES` : '\nall good');
process.exit(failures ? 1 : 0);
