// The document, and the one question it asks the server.
//
//     state(design, t)
//
// The browser owns the design and nothing else. It does not know that a heat
// pipe leaks 2%, what a turbine does below its threshold, or how long a tank
// takes to fill; it posts the document, gets back the machine at a tick, and
// draws it.
//
// What it *does* own a copy of is the catalogue -- footprints, port names,
// domains, the reach limit -- because refusing to draw an illegal wire is a
// thing that has to happen while the pointer is still moving. That copy is
// fetched from the server at startup rather than typed in here, so the
// thirty-eight components still have exactly one definition and it is in Rust.
//
// Experiment 07 added a second thing worth saying about the boundary: what a
// wire carries is now a *stuff* rather than a number, and the browser is told
// what it is rather than working it out. `Iron Ore (powder, 82% pure)` is
// composed in Rust, arrives as a string, and is printed.

export const state = {
  design: { name: 'Machine', brief: 'power', units: [], wires: [] },
  cat: null,            // { parts: {tag: part}, order: [tag], constants }
  snapshot: null,       // the machine at `tick`
  macro: null,          // what it looks like from outside
  totals: null,         // everything it had done by `tick`
  equivalentTick: 0,    // the tick actually simulated to answer
  compiled: null,       // orbit + waveform, when asked for
  faults: [],
  error: null,
  tick: 0,
  renderTime: 0,
  playing: false,
  speed: 1,
  selected: null,       // { what: 'unit', name } | { what: 'wire', i }
  family: '',           // which palette family is showing, '' for all
  dirty: true,
};

const listeners = [];
export function onChange(fn) { listeners.push(fn); }
export function changed(structural) {
  if (structural) { state.dirty = true; state.compiled = null; }
  listeners.forEach(fn => fn());
}

// ------------------------------------------------------------- catalogue

export function part(kind) { return state.cat && state.cat.parts[kind]; }
export function portsOf(kind, dir) {
  const p = part(kind);
  return p ? p.ports.map((q, i) => ({ ...q, i })).filter(q => !dir || q.dir === dir) : [];
}
export function unitOf(name) { return state.design.units.find(u => u.name === name); }

// Experiment 10: a footprint turns with the component, so `w` and `h` are the
// placed dimensions rather than the catalogue's. Only an *authored* rotation
// turns it -- an inferred one is the visual pipeline's business, and the
// browser never sees it.
export function box(u) {
  const p = part(u.kind);
  const t = turned(u);
  return {
    x: u.x,
    y: u.y,
    z: u.z || 0,
    w: p ? (t ? p.h : p.w) : 1,
    h: p ? (t ? p.w : p.h) : 1,
    d: p ? (p.storeys || 1) : 1,
  };
}

export function turned(u) {
  return u.face !== null && u.face !== undefined && (u.face & 1) === 1;
}

/// Clear tiles between two footprints, which is what the reach rule measures.
/// Three dimensions since experiment 10: stacking a component on top of the
/// one it feeds is a legitimate way of being next to it.
export function gap(a, b) {
  const A = box(a), B = box(b);
  const dx = Math.max(B.x - (A.x + A.w), A.x - (B.x + B.w), 0);
  const dy = Math.max(B.y - (A.y + A.h), A.y - (B.y + B.h), 0);
  const dz = Math.max(B.z - (A.z + A.d), A.z - (B.z + B.d), 0);
  return dx + dy + dz;
}

/// Whether a component put at these tiles would be inside another one.
/// Sharing a footprint is not a clash -- it is a stack, which is the whole
/// point of the third tile. Sharing a footprint at the same height is.
export function overlaps(u, x, y, z, ignore) {
  const me = typeof u === 'string' ? { kind: u, face: null } : u;
  const A = box({ ...me, x, y, z: z || 0 });
  return state.design.units.some(o => {
    if (o.name === ignore) return false;
    const B = box(o);
    return A.x < B.x + B.w && B.x < A.x + A.w &&
           A.y < B.y + B.h && B.y < A.y + A.h &&
           A.z < B.z + B.d && B.z < A.z + A.d;
  });
}

/// Exactly the rule the compiler uses, so nothing can be drawn that will then
/// be refused -- and when it cannot be drawn, the reason is the one the server
/// would have given.
export function wireProblem(a, ai, b, bi) {
  if (!a || !b || a === b) return 'a component cannot be wired to itself';
  const pa = part(a.kind).ports[ai], pb = part(b.kind).ports[bi];
  if (!pa || !pb) return 'no such port';
  if (pa.dir !== 'out' || pb.dir !== 'in') return 'an output goes to an input';
  // Experiment 06 refused to wire a boundary port at all. Experiment 07 does
  // not: a generator that runs a conveyor motor and exports the difference is
  // a design, so the only rule left is the domain.
  if (pa.type !== pb.type) return `${pa.name} carries ${pa.type}, ${pb.name} takes ${pb.type}`;
  const g = gap(a, b);
  const reach = state.cat.constants.reach;
  if (g > reach) {
    return `${g} tiles apart — a connection reaches ${reach}. Move them together, stack them, or put a pipe in between`;
  }
  if (state.design.wires.some(w =>
      w.from === a.name && w.fromPort === pa.name && w.to === b.name && w.toPort === pb.name)) {
    return 'already wired';
  }
  return null;
}

/// The first port on `b` that `a.ai` could legally be wired to, so that
/// dropping a connection on a component rather than on one of its four little
/// squares still does the obvious thing.
export function firstCompatible(a, ai, b) {
  const pa = part(a.kind).ports[ai];
  const ports = part(b.kind).ports;
  for (let i = 0; i < ports.length; i++) {
    if (!wireProblem(a, ai, b, i)) return i;
  }
  // Nothing legal: hand back whichever port shares the type, so the refusal
  // names something the player was plausibly aiming at.
  const same = ports.findIndex(q => q.type === pa.type && q.dir === 'in');
  return same >= 0 ? same : 0;
}

// --------------------------------------------------------------- editing

export function uniqueName(kind) {
  const stem = {
    reactor: 'R', burner: 'BN', heater: 'EH', mains: 'M', pump: 'W', inlet: 'I',
    outlet: 'O', skip: 'SK', radiator: 'RD',
    heatpipe: 'HP', steampipe: 'SP', fluidpipe: 'FP', chute: 'CH', screw: 'SC',
    shaft: 'SH', cable: 'CB',
    hopper: 'HO', tank: 'TK', drum: 'DR', flywheel: 'FW',
    valve: 'V', clutch: 'CL',
    exchanger: 'HX', preheater: 'PH', condenser: 'CD', furnace: 'F',
    turbine: 'T', generator: 'G', motor: 'MO', gearbox: 'GB', crank: 'CR',
    crusher: 'C', mill: 'MI', separator: 'S', rollmill: 'RM', press: 'P',
    lathe: 'L', column: 'CO',
  }[kind] || 'U';
  for (let i = 1; ; i++) {
    const name = stem + i;
    if (!unitOf(name)) return name;
  }
}

export function place(kind, x, y, z) {
  const u = {
    name: uniqueName(kind), kind, x, y, z: z || 0, face: null,
    throttle: 100, pulse: false, high: 1200, low: 0,
    draws: kind === 'inlet' ? 'ore' : 'water', ratio: 4, limit: 100, stages: 2,
  };
  state.design.units.push(u);
  changed(true);
  return u;
}

export function move(name, x, y, z) {
  const u = unitOf(name);
  if (!u) return;
  u.x = x; u.y = y;
  if (z !== undefined) u.z = Math.max(0, z);
  // Moving is a structural edit here, unlike in the workbench: distance is a
  // rule, so dragging a component out of reach really does break the wire.
  changed(true);
}

/// Experiment 10: up a storey, down a storey. Refused at the floor and
/// wherever the component would end up inside another one, because the
/// document already refuses that and a control that offers an illegal move is
/// a control that lies.
export function lift(name, by) {
  const u = unitOf(name);
  if (!u) return false;
  const z = Math.max(0, (u.z || 0) + by);
  if (z === (u.z || 0) || overlaps(u, u.x, u.y, z, name)) return false;
  u.z = z;
  changed(true);
  return true;
}

/// And a quarter turn. The first one takes the component from "wherever the
/// flow points it" to a decision the player owns; after that it just turns.
export function turn(name, by) {
  const u = unitOf(name);
  if (!u) return false;
  const was = u.face === null || u.face === undefined ? 0 : u.face;
  const face = (was + by + 4) & 3;
  const saved = u.face;
  u.face = face;
  // A turn changes the footprint, so it can be refused for exactly the reason
  // a move can.
  if (overlaps(u, u.x, u.y, u.z || 0, name)) {
    u.face = saved;
    return false;
  }
  changed(true);
  return true;
}

/// Back to letting the flow decide, which is where every component starts.
export function freeface(name) {
  const u = unitOf(name);
  if (!u) return;
  u.face = null;
  changed(true);
}

export function remove(name) {
  state.design.units = state.design.units.filter(u => u.name !== name);
  state.design.wires = state.design.wires.filter(w => w.from !== name && w.to !== name);
  // Any selection at all, not just this component's: a wire is selected by its
  // index in a list that has just been shortened.
  state.selected = null;
  changed(true);
}

export function connect(a, ai, b, bi) {
  state.design.wires.push({
    from: a.name, fromPort: part(a.kind).ports[ai].name,
    to: b.name, toPort: part(b.kind).ports[bi].name,
  });
  changed(true);
}

export function unwire(i) {
  state.design.wires.splice(i, 1);
  if (state.selected && state.selected.what === 'wire') state.selected = null;
  changed(true);
}

export function retune(name, patch) {
  const u = unitOf(name);
  if (!u) return;
  Object.assign(u, patch);
  changed(true);
}

export function rename(name) {
  state.design.name = name;
  changed(true);
}

/// Which of the four briefs this machine is judged against. A structural edit,
/// because everything on the scoreboard changes.
export function setBrief(tag) {
  state.design.brief = tag;
  changed(true);
}

export function brief() {
  return (state.cat.briefs || []).find(b => b.tag === state.design.brief) || null;
}

// ------------------------------------------------------------ the server

let inflight = null;
let queued = null;

/// Ask what the machine looks like at `tick`. One request at a time: dragging
/// a timeline can outrun any server, and only the last answer matters.
export function seek(tick, force) {
  tick = Math.max(0, Math.round(tick));
  if (!force && !state.dirty && state.snapshot && state.snapshot.tick === tick) return;
  if (inflight) { queued = tick; return; }
  inflight = post(`/api/state?t=${tick}`)
    .then(res => {
      if (res.ok) {
        state.snapshot = res.snapshot;
        state.macro = res.macro;
        state.totals = res.totals;
        state.equivalentTick = res.equivalentTick;
        state.faults = [];
        state.error = null;
      } else {
        // A document that cannot be simulated is still a document you can look
        // at, and a component you have just placed and not yet wired is
        // exactly that.
        state.snapshot = null;
        state.faults = res.faults || [];
        state.error = res.error;
      }
      state.tick = tick;
      state.dirty = false;
      changed(false);
    })
    .catch(e => { state.error = String(e); state.dirty = false; changed(false); })
    .finally(() => {
      inflight = null;
      if (queued !== null) { const t = queued; queued = null; seek(t, true); }
    });
}

export async function compile() {
  const res = await post('/api/compile');
  state.compiled = res.ok ? res : null;
  if (!res.ok) state.error = res.error;
  changed(false);
  return res;
}

export async function verify(tick) {
  return post(`/api/verify?t=${Math.max(1, Math.round(tick))}`);
}

/// Experiment 08. The same document, to a pass that has never heard of a tick:
/// nothing about the answer depends on `state.tick`, and nothing about
/// `state.snapshot` depends on this.
export async function form(style, seed, grade) {
  return post(
    `/api/form?style=${encodeURIComponent(style)}&seed=${seed | 0}` +
    `&grade=${encodeURIComponent(grade || 'full')}`
  );
}

function post(url) {
  return fetch(url, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ design: state.design }),
  }).then(r => r.json());
}

export async function catalogue() {
  const res = await fetch('/api/catalogue').then(r => r.json());
  state.cat = {
    parts: Object.fromEntries(res.parts.map(p => [p.kind, p])),
    order: res.parts.map(p => p.kind),
    portKinds: res.portKinds,
    substances: res.substances || [],
    briefs: res.briefs || [],
    families: [...new Set(res.parts.map(p => p.family))],
    constants: res.constants,
  };
  return state.cat;
}

export async function listDesigns() {
  const res = await fetch('/api/designs').then(r => r.json());
  return res.designs || [];
}

export async function openDesign(name) {
  const res = await fetch('/api/design?name=' + encodeURIComponent(name)).then(r => r.json());
  if (res.ok) {
    adopt(res.design);
  } else {
    state.error = res.error;
    changed(false);
  }
  return res;
}

export function adopt(design) {
  // The wire format spells tunables out per unit whatever their kind, which is
  // what keeps `retune` from having to know which fields a kind cares about.
  state.design = {
    name: design.name,
    brief: design.brief || 'power',
    units: design.units.map(u => ({ ...u })),
    wires: design.wires.map(w => ({ ...w })),
  };
  state.selected = null;
  state.compiled = null;
  changed(true);
}

export async function save(name) {
  const r = await fetch('/api/save?name=' + encodeURIComponent(name), {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ design: state.design }),
  });
  return r.json();
}

// ------------------------------------------------------------ formatting

export function num(n) {
  if (n === null || n === undefined) return '--';
  const v = typeof n === 'string' ? BigInt(n) : BigInt(Math.round(n));
  return v.toLocaleString('en-GB');
}

export function toNum(n) {
  if (n === null || n === undefined) return 0;
  return typeof n === 'string' ? Number(n) : n;
}

/// Big numbers, short. A scoreboard that wraps is a scoreboard nobody reads.
export function compact(n) {
  n = toNum(n);
  const neg = n < 0;
  n = Math.abs(n);
  let s;
  if (n >= 1e12) s = (n / 1e12).toFixed(1) + 'T';
  else if (n >= 1e9) s = (n / 1e9).toFixed(1) + 'B';
  else if (n >= 1e6) s = (n / 1e6).toFixed(1) + 'M';
  else if (n >= 1e4) s = (n / 1e3).toFixed(0) + 'k';
  else s = num(n);
  return (neg ? '-' : '') + s.replace('.0', '');
}
