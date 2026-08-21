// The document: a factory as placed nodes and wires, and the commands that
// change it. Nothing here knows any physics. Every question about what a
// plant *does* goes over the wire to the solver, and the answer comes back as
// a snapshot of one tick.

export const state = {
  graph: { name: 'Sketch', items: [], nodes: [], edges: [], deploy: 1, stagger: 0 },
  plant: null,        // shape of the compiled plant
  snapshot: null,     // state at snapshot.tick
  source: '',         // the DSL this document emitted
  error: null,
  tick: 0,            // the tick we last asked about
  renderTime: 0,      // where the view is, which may be between ticks
  playing: false,
  speed: 1,
  selected: null,     // { kind: 'node' | 'link', name }
  dirty: true,
};

const undo = [];
const listeners = [];

export function onChange(fn) { listeners.push(fn); }
function changed(structural) {
  if (structural) { state.dirty = true; }
  listeners.forEach(fn => fn());
}

// ------------------------------------------------------------- commands
//
// Every edit goes through here. It is the obvious place for undo, and it is
// also the extension point a replay, a second player or a scripted test would
// need -- a plant is then a list of commands rather than a blob.

export function apply(cmd) {
  markUndo();
  cmd(state.graph);
  changed(true);
}

/// Remember the current document without changing it. Dragging a node is one
/// undo step, not one per pixel, and it is not an edit to the plant at all --
/// a factory is the same factory wherever it is drawn.
export function markUndo() {
  undo.push(JSON.parse(JSON.stringify(state.graph)));
  if (undo.length > 200) undo.shift();
}

export function undoLast() {
  const g = undo.pop();
  if (!g) return;
  state.graph = g;
  changed(true);
}

export function uniqueName(kind) {
  const stem = { source: 'Source', storage: 'Bay', process: 'Machine', sink: 'Sink', link: 'Link' }[kind];
  for (let i = 1; ; i++) {
    const name = stem + i;
    if (!state.graph.nodes.some(n => n.name === name)) return name;
  }
}

export function ensureItem() {
  if (state.graph.items.length === 0) state.graph.items.push('Widget');
  return state.graph.items[0];
}

export function newNode(kind, x, y) {
  const item = ensureItem();
  const other = state.graph.items[1] || item;
  const n = { name: uniqueName(kind), kind, count: 1, shared: false, x, y };
  if (kind === 'storage') {
    n.capacity = 10000;
    n.policy = 'round_robin';
    n.priority = [];
    n.initial = [];
  } else {
    n.inputs = [];
    n.outputs = [];
    n.duration = 60;
    n.returns = 0;
    n.geometry = null;
    if (kind === 'source') n.outputs = [{ item, qty: 100 }];
    if (kind === 'sink') n.inputs = [{ item, qty: 100 }];
    if (kind === 'process') {
      n.inputs = [{ item, qty: 10 }];
      n.outputs = [{ item: other, qty: 10 }];
      n.duration = 20;
    }
    if (kind === 'link') {
      n.inputs = [{ item, qty: 1000 }];
      n.outputs = [{ item, qty: 1000 }];
      n.duration = 300;
      n.returns = 300;
    }
  }
  return n;
}

export function nodeOf(name) { return state.graph.nodes.find(n => n.name === name); }

/// A wire is legal only between a machine and a storage. Machines never touch
/// each other, and neither do bays: the compiler says so, and finding that out
/// after a compile round trip is worse than not being able to draw it.
export function canWire(a, b) {
  if (!a || !b || a === b) return false;
  return (a.kind === 'storage') !== (b.kind === 'storage');
}

export function connect(a, b) {
  apply(g => {
    if (!g.edges.some(e => e.from === a.name && e.to === b.name)) {
      g.edges.push({ from: a.name, to: b.name, item: null });
    }
  });
}

export function removeNode(name) {
  apply(g => {
    g.nodes = g.nodes.filter(n => n.name !== name);
    g.edges = g.edges.filter(e => e.from !== name && e.to !== name);
  });
}

export function removeEdge(from, to) {
  apply(g => { g.edges = g.edges.filter(e => !(e.from === from && e.to === to)); });
}

// ----------------------------------------------------------- the server

let inflight = null;
let queued = null;

/// Ask what the plant looks like at `tick`. One request at a time: dragging a
/// timeline can outrun any server, and the only answer that matters is the
/// last one asked for.
export function seek(tick, force) {
  tick = Math.max(0, Math.round(tick));
  if (!force && !state.dirty && state.snapshot && state.snapshot.tick === tick) return;
  if (inflight) { queued = tick; return; }
  inflight = fetch(`/api/state?t=${tick}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ graph: state.graph }),
  })
    .then(r => r.json())
    .then(res => {
      if (res.ok) {
        state.plant = res.plant;
        state.snapshot = res.snapshot;
        state.source = res.source;
        state.tick = tick;
        state.error = null;
        state.dirty = false;
      } else {
        state.error = res;
        state.snapshot = null;
      }
      // Asked and answered, whatever the answer. A plant that does not compile
      // must not leave the view asking again forever.
      state.dirty = false;
      changed(false);
    })
    .catch(e => { state.error = { error: String(e) }; state.dirty = false; changed(false); })
    .finally(() => {
      inflight = null;
      if (queued !== null) { const t = queued; queued = null; seek(t, true); }
    });
}

export async function fetchTimetable(tick) {
  const r = await fetch(`/api/trace?t=${Math.max(1, Math.round(tick))}`, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ graph: state.graph }),
  });
  const res = await r.json();
  return res.ok ? res.timetable : null;
}

export async function listPlants() {
  const r = await fetch('/api/configs');
  return r.json();
}

export async function openPlant(name) {
  const r = await fetch('/api/config?name=' + encodeURIComponent(name));
  const res = await r.json();
  if (res.ok) {
    state.graph = res.graph;
    state.selected = null;
    state.dirty = true;
    changed(true);
  } else {
    state.error = res;
    changed(false);
  }
  return res;
}

export async function savePlant(name) {
  const r = await fetch('/api/save?name=' + encodeURIComponent(name), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ graph: state.graph }),
  });
  return r.json();
}

// ------------------------------------------------------------ formatting

export function num(n) {
  if (n === null || n === undefined) return '--';
  const v = typeof n === 'string' ? BigInt(n) : BigInt(Math.round(n));
  return v.toLocaleString('en-GB');
}

/// Counts past 2^53 arrive as strings, so everything that does arithmetic on
/// one has to go through here rather than trusting `+`.
export function toNum(n) {
  if (n === null || n === undefined) return 0;
  return typeof n === 'string' ? Number(n) : n;
}

export function ticks(t) {
  if (t === null || t === undefined) return '--';
  return num(t) + (Math.abs(toNum(t)) === 1 ? ' tick' : ' ticks');
}
