// The client half of the protocol, which is deliberately three verbs long.
//
//   send(type, payload)   one intention. Accepted or refused, and told which.
//   poll()                one player's whole view of one frame.
//   presence(...)         a cursor. Allowed to be lost.
//
// There is no prediction here. A client that guessed at the result of its own
// command would be a second authority, and the entire experiment is about
// there being one -- so the ghost under the pointer is a picture, the command
// is a request, and what appears on the plot is whatever the host says
// appeared on the plot. At sixty ticks a second and a poll every 180 ms that
// is a fifth of a second of honesty, which is cheaper than a rollback.

export const state = {
  code: null,
  player: 0,
  catalogue: null,
  parts: null,
  view: null,       // the last frame the host sent
  err: null,
  onFrame: [],
  onRefusal: [],
};

async function post(path, body) {
  const r = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body || {}),
  });
  return r.json();
}

async function get(path) {
  const r = await fetch(path);
  return r.json();
}

export async function catalogue() {
  if (!state.catalogue) state.catalogue = await get('/api/catalogue');
  return state.catalogue;
}

export async function parts() {
  if (!state.parts) state.parts = await get('/api/parts');
  return state.parts;
}

export const goals = () => get('/api/goals');
export const rooms = () => get('/api/rooms');

export async function host(name, seed, template) {
  const res = await post('/api/host', { name, seed: seed || undefined, template });
  if (res.ok) { state.code = res.code; state.player = res.player; }
  return res;
}

export async function join(code, name) {
  const res = await post('/api/join', { code, name });
  if (res.ok) { state.code = res.code; state.player = res.player; }
  return res;
}

/// One intention. The answer is small on purpose: everything the world looks
/// like afterwards arrives on the next poll, from the same reconstruction the
/// other player is being shown.
export async function send(type, payload) {
  const res = await post('/api/cmd', { code: state.code, player: state.player, type, payload });
  if (!res.ok) {
    state.err = res.error;
    for (const f of state.onRefusal) f(res.error, type);
  }
  return res;
}

/// Begin the clock. There is no matching stop.
export function begin() {
  return post('/api/start', { code: state.code });
}

export async function presence(cursor, selection, editing, view) {
  return post('/api/presence', {
    code: state.code, player: state.player, cursor, editing, view,
    // A selected wire is a string key rather than an installation id, and
    // presence is about buildings. Sending it as null is the honest answer.
    selection: typeof selection === 'number' ? selection : null,
  });
}

/// A design, built as a plant, for the 3D window. The document comes from the
/// room rather than from this browser: a client that could post any design
/// would be drawing something nobody else can see.
export function form(id, draft) {
  return post('/api/form', { code: state.code, id, draft: !!draft });
}

/// What every component in one machine is doing, at the phase the room's
/// clock puts its orbit in.
export function inside(id, draft) {
  return post('/api/inside', { code: state.code, id, draft: !!draft });
}

let timer = null;

export function start(period = 180) {
  stop();
  const tick = async () => {
    try {
      const v = await get(`/api/state?code=${state.code}&player=${state.player}`);
      if (v.ok) {
        state.view = v;
        for (const f of state.onFrame) f(v);
      } else {
        state.err = v.error;
      }
    } catch (e) {
      state.err = String(e);
    }
    timer = setTimeout(tick, period);
  };
  tick();
}

export function stop() {
  if (timer) clearTimeout(timer);
  timer = null;
}

export function onFrame(f) { state.onFrame.push(f); }
export function onRefusal(f) { state.onRefusal.push(f); }

// ------------------------------------------------------------------ helpers

export function installs() {
  return (state.view && state.view.world.installs) || [];
}

export function byId(id) {
  return installs().find(i => i.id === id) || null;
}

export function proto(tag) {
  return (state.catalogue && state.catalogue.protos.find(p => p.tag === tag)) || null;
}

/// The plant's opinion of one installation, by name. The world document and
/// the simulation snapshot are joined here and nowhere else: names are the
/// only identity that survives a recompile, which is exactly why the
/// simulator carries state by them.
/// A wire has no `id` of its own -- it *is* the (from, to, item) triple that
/// made it -- so a selected one is spelled as a string and an installation
/// stays a number. Everything that consumes a selection can therefore go on
/// taking one argument.
export const wireKey = c => `w:${c.from}:${c.to}:${c.item}`;
export const wireOf = k =>
  typeof k === 'string' && k.startsWith('w:')
    ? (([, from, to, ...rest]) => ({ from: +from, to: +to, item: rest.join(':') }))(k.split(':'))
    : null;

export function plantOf(name) {
  const p = state.view && state.view.plant;
  if (!p || !p.classes) return null;
  return p.classes.find(c => c.name === name)
      || (p.storages || []).find(s => s.name === name)
      || (p.links || []).find(l => l.name === name)
      || null;
}

export function clock(t) {
  const s = Math.floor(t / 60);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
}

export function num(n) {
  if (n === null || n === undefined) return '--';
  if (Math.abs(n) >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (Math.abs(n) >= 1e4) return (n / 1e3).toFixed(1) + 'k';
  return Number.isInteger(n) ? String(n) : n.toFixed(1);
}
