// The document: a factory, and everything that has happened to it. Nothing
// here knows any physics. Every question about what a plant *does* goes over
// the wire to the solver, and the answer comes back as a snapshot of one tick.
//
// Prototype 1 changed what "the document" is. It used to be a graph the
// browser owned and mutated; it is now a base plant plus a list of commands,
// and the browser owns neither the result of applying them nor the rules about
// which ones are allowed.
//
//     log = { base, commands: [ { at, op, ... } ] }
//     graph = whatever the server says log means at the tick we asked about
//
// So the browser does not apply an edit. It *proposes* one: append the
// command, ask again, and take the graph that comes back. A refusal -- two
// bays wired together, a name that is taken, a recipe that does not compile --
// arrives as an error naming the tick, and the command is taken back off the
// end of the log.
//
// This is a slower way to draw a rectangle and a much better way to stay
// honest. There is exactly one implementation of what an edit means, it is in
// Rust, and it is the one a replaying client would use.

export const state = {
  log: { base: emptyGraph(), commands: [] },
  graph: emptyGraph(),  // the plant at `tick`, as the server understands it
  plant: null,          // shape of the compiled plant
  snapshot: null,       // state at snapshot.tick
  scrapped: [],         // what the edits along the way destroyed
  play: null,           // scenario progress, when one is loaded
  scenario: null,       // name of the loaded scenario file
  source: '',           // the DSL this document emitted
  error: null,
  refused: null,       // the last edit the server would not accept, and why
  tick: 0,              // the tick we last asked about
  renderTime: 0,        // where the view is, which may be between ticks
  playing: false,
  speed: 1,
  selected: null,       // { name }
  // Where new commands land. `false` puts them at tick 0, which is designing
  // a factory; `true` puts them at the tick on the clock, which is playing
  // one. The distinction is entirely in this one flag -- a design edit is a
  // command at tick 0 and nothing else.
  liveEdits: false,
  dirty: true,
};

/// The shortest true thing that can be said about a command.
export function describeOp(c) {
  switch (c.op) {
    case 'place': return `place ${c.node.kind} ${c.node.name}`;
    case 'retune': return `retune ${c.node.name}`;
    case 'remove': return `remove ${c.name}`;
    case 'wire': return `wire ${c.from} → ${c.to}`;
    case 'unwire': return `unwire ${c.from} → ${c.to}`;
    case 'item': return `item ${c.name}`;
    case 'name': return `name ${c.name}`;
    case 'deploy': return `deploy x${c.count}`;
    default: return c.op;
  }
}

function emptyGraph() {
  return { name: 'Sketch', items: [], nodes: [], edges: [], deploy: 1, stagger: 0 };
}

const listeners = [];
export function onChange(fn) { listeners.push(fn); }
function changed(structural) {
  if (structural) state.dirty = true;
  listeners.forEach(fn => fn());
}

// ------------------------------------------------------------- commands

/// The tick a new command lands on.
export function editTick() {
  return state.liveEdits ? Math.max(0, Math.floor(state.renderTime)) : 0;
}

/// Propose an edit. Optimism is not involved: the command goes on the log, the
/// server is asked what the plant looks like now, and if the answer is that
/// the command was illegal it comes straight back off again.
export function record(edit) {
  const at = editTick();
  state.refused = null;
  const last = state.log.commands[state.log.commands.length - 1];
  if (last && last.at > at) {
    // Editing the past would rewrite history that has already been played,
    // and the log is required to be in order.
    state.refused = `the clock is at ${at} but the last command was at ${last.at}`;
    changed(false);
    return false;
  }
  state.log.commands.push({ at, ...edit });
  seek(Math.max(at, Math.floor(state.renderTime)), true);
  return true;
}

/// Take the last command back. A log makes undo trivial and exact: the state
/// after undoing is not a remembered copy of the document, it is the same pure
/// function of a shorter log.
export function undoLast() {
  if (!state.log.commands.length) return;
  state.log.commands.pop();
  seek(Math.floor(state.renderTime), true);
}

export function uniqueName(kind) {
  const stem = { source: 'Source', storage: 'Bay', process: 'Machine', sink: 'Sink', link: 'Link' }[kind];
  for (let i = 1; ; i++) {
    const name = stem + i;
    if (!state.graph.nodes.some(n => n.name === name)) return name;
  }
}

export function firstItem() {
  return state.graph.items[0] || 'Widget';
}

export function newNode(kind, x, y) {
  const item = firstItem();
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

export function place(kind, x, y) {
  const n = newNode(kind, x, y);
  if (state.graph.items.length === 0) record({ op: 'item', name: 'Widget' });
  record({ op: 'place', node: n });
  return n;
}

/// Change a node into what the given function makes of a copy of it.
export function retune(name, mutate) {
  const n = nodeOf(name);
  if (!n) return;
  const next = JSON.parse(JSON.stringify(n));
  mutate(next);
  record({ op: 'retune', node: next });
}

export function nodeOf(name) { return state.graph.nodes.find(n => n.name === name); }

/// A wire is legal only between a machine and a storage. The compiler says so
/// too, but finding that out after drawing it is worse than not being able to
/// draw it.
export function canWire(a, b) {
  if (!a || !b || a === b) return false;
  return (a.kind === 'storage') !== (b.kind === 'storage');
}

export function connect(a, b) {
  record({ op: 'wire', from: a.name, to: b.name, item: null });
}

export function removeNode(name) {
  record({ op: 'remove', name });
}

export function removeEdge(from, to) {
  record({ op: 'unwire', from, to });
}

export function addItem(name) {
  record({ op: 'item', name });
}

export function setPlantName(name) {
  record({ op: 'name', name });
}

export function setDeploy(count) {
  record({ op: 'deploy', count });
}

/// Where a box is drawn is not an edit. A factory is the same factory wherever
/// it is drawn, so a drag writes straight through to whichever part of the log
/// holds that node's coordinates and never adds a command -- and the server
/// ignores positions when it decides whether it has seen this plant before.
export function moveNode(name, x, y) {
  const live = nodeOf(name);
  if (live) { live.x = x; live.y = y; }
  const inBase = state.log.base.nodes.find(n => n.name === name);
  if (inBase) { inBase.x = x; inBase.y = y; return; }
  // Only a `place` carries a position that survives: a retune keeps whatever
  // the node was already drawn at, so writing into one would move the box on
  // this screen and nowhere else.
  for (let i = state.log.commands.length - 1; i >= 0; i--) {
    const c = state.log.commands[i];
    if (c.op === 'place' && c.node.name === name) {
      c.node.x = x;
      c.node.y = y;
      return;
    }
  }
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
  const asked = state.log.commands.length;
  const url = `/api/state?t=${tick}` +
    (state.scenario ? `&scenario=${encodeURIComponent(state.scenario)}` : '');
  inflight = fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ log: state.log }),
  })
    .then(r => r.json())
    .then(res => {
      if (res.ok) {
        state.plant = res.plant;
        state.snapshot = res.snapshot;
        state.graph = res.graph;
        state.source = res.source;
        state.scrapped = res.scrapped || [];
        state.play = res.play || null;
        state.tick = tick;
        state.error = null;
      } else {
        // A plant that does not compile is still a plant you can look at. A
        // machine you have just placed and not yet wired up is exactly that,
        // and it has to appear on the canvas or there is no way to wire it.
        if (res.graph) state.graph = res.graph;
        if (res.source) state.source = res.source;
        state.snapshot = null;
        state.plant = null;

        // Only a command that can *never* work comes back off the log.
        if (res.refused && asked === state.log.commands.length && state.log.commands.length) {
          const dropped = state.log.commands.pop();
          // The refusal has to outlive the corrective request, or the player
          // sees the plant snap back with no explanation.
          state.refused = `${describeOp(dropped)} at t=${dropped.at}: ${res.error}`;
          state.dirty = false;
          changed(false);
          seek(tick, true);
          return;
        }
        state.error = res;
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
  const res = await post(`/api/trace?t=${Math.max(1, Math.round(tick))}`);
  return res.ok ? res.timetable : null;
}

/// The networking proof, rehearsed. Ask the server whether this tick reached
/// from the beginning and this tick reached from a snapshot halfway through
/// are the same tick.
export async function verify(tick) {
  return post(`/api/verify?t=${Math.max(1, Math.round(tick))}`);
}

function post(url) {
  return fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ log: state.log }),
  }).then(r => r.json());
}

export async function listPlants() {
  const r = await fetch('/api/configs');
  return r.json();
}

function adopt(graph) {
  state.log = { base: graph, commands: [] };
  state.graph = graph;
  state.selected = null;
  state.scrapped = [];
  state.refused = null;
  state.error = null;
  state.dirty = true;
}

export async function openPlant(name) {
  const r = await fetch('/api/config?name=' + encodeURIComponent(name));
  const res = await r.json();
  if (res.ok) {
    state.scenario = null;
    state.play = null;
    state.liveEdits = false;
    adopt(res.graph);
    changed(true);
  } else {
    state.error = res;
    changed(false);
  }
  return res;
}

export async function openScenario(name) {
  const r = await fetch('/api/scenario?name=' + encodeURIComponent(name));
  const res = await r.json();
  if (res.ok) {
    state.scenario = name;
    // A scenario is a factory you are given and asked to fix, so edits land on
    // the clock rather than at the beginning of time.
    state.liveEdits = true;
    adopt(res.graph);
    changed(true);
  } else {
    state.error = res;
    changed(false);
  }
  return res;
}

export async function savePlant(name) {
  const r = await fetch(
    `/api/save?name=${encodeURIComponent(name)}&t=${Math.floor(state.renderTime)}`,
    {
      method: 'POST',
      headers: { 'content-type': 'application/json' },
      body: JSON.stringify({ log: state.log }),
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
