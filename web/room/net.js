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
  onHealth: [],
};

/// How the *connection* is doing, as opposed to how the room is doing.
///
/// These two used to be the same number and they are not the same question.
/// `view.sync` says whether this player's reconstruction of the room agrees
/// with the host's, which is the experiment. This says whether the screen is
/// being told about it at all, which is the difference between a factory that
/// stopped and a browser that stopped watching one -- and on screen those look
/// identical, which is exactly how a play session ends up reporting a freeze
/// that never happened.
export const health = {
  lag: 0,       // ms since a frame last arrived
  rtt: 0,       // how long the last poll took to answer
  misses: 0,    // polls in a row that did not answer
  behind: 0,    // ticks the room says this replica trails its own clock by
  live: false,  // whether the picture on screen is current
};

// ------------------------------------------------------------------- a seat
//
// A player id is a seat in one room, handed out by the host. It lived in this
// module and nowhere else, which meant a refresh -- or a click on a link in
// the left-hand menu -- threw it away and the next join took a *second* seat
// in a room where the first one still held everything that player had built.
// Two seats, one person, and a screen that no longer owned its own factory.
//
// So the browser keeps a token of its own. The token is not the identity the
// host uses -- it is the thing that proves which identity to hand back.

const SEAT = 'temporal-rooms/seat';
const LAST = 'temporal-rooms/room';

// Every access is guarded. `localStorage` throws rather than returns null in a
// private window and does not exist at all in the node test harness, and none
// of that is a reason for the game to fail to start.
function stored(k) {
  try { return localStorage.getItem(k); } catch { return null; }
}
function store(k, v) {
  try { localStorage.setItem(k, v); } catch { /* nothing to remember with */ }
}
function drop(k) {
  try { localStorage.removeItem(k); } catch { /* already gone */ }
}

let fallbackSeat = null;

/// This browser's token, minted once and kept.
export function seat() {
  const held = stored(SEAT);
  if (held) return held;
  const fresh = 'seat-' + Math.random().toString(36).slice(2, 10) + Date.now().toString(36);
  store(SEAT, fresh);
  // If there was nowhere to write it, at least be stable for this page: a
  // token that changes per request would be worse than not having one.
  if (stored(SEAT) !== fresh) return (fallbackSeat ||= fresh);
  return fresh;
}

/// The last room this browser was in, if it was in one.
export const lastRoom = () => stored(LAST);
const remember = code => store(LAST, code);
export const forget = () => drop(LAST);

// ------------------------------------------------------------------ the wire
//
// Every request answers `{ok}` or `{ok: false, error}` and never throws. It
// used to throw: `r.json()` on a connection the server dropped rejects, and
// nothing in `app.js` catches, so one refused socket killed whichever click
// handler was half-way through and left the screen holding a state the host
// had never agreed to. The poll loop below always had its own try/catch and so
// kept running, which is exactly why the failure looked like a freeze rather
// than an error -- the clock went on ticking and nothing else worked.

async function req(path, init, timeout = 0) {
  // A socket that is never going to answer must not be waited on forever. It
  // is not a hypothetical: the server reads a request head with a thirty
  // second timeout on it, and thirty seconds is long enough for a player to
  // conclude the game is broken, close the tab, and be right.
  const ctl = timeout && typeof AbortController === 'function' ? new AbortController() : null;
  const bell = ctl ? setTimeout(() => ctl.abort(), timeout) : null;
  try {
    const r = await fetch(path, ctl ? { ...init, signal: ctl.signal } : init);
    if (!r.ok) return { ok: false, error: `${path} answered ${r.status} ${r.statusText}` };
    return await r.json();
  } catch (e) {
    const gave_up = e && (e.name === 'AbortError' || e.name === 'TimeoutError');
    return {
      ok: false,
      error: gave_up
        ? `${path} did not answer within ${timeout} ms`
        : `${path} did not answer: ${(e && e.message) || e}`,
    };
  } finally {
    if (bell) clearTimeout(bell);
  }
}

const post = (path, body) => req(path, {
  method: 'POST',
  headers: { 'content-type': 'application/json' },
  body: JSON.stringify(body || {}),
});

const get = path => req(path);

// A failed answer is never cached: the catalogue is asked for once, and once
// used to mean once ever, including the once that failed.
export async function catalogue() {
  if (!state.catalogue) {
    const res = await get('/api/catalogue');
    if (!res.ok) return res;
    state.catalogue = res;
  }
  return state.catalogue;
}

export async function parts() {
  if (!state.parts) {
    const res = await get('/api/parts');
    if (!res.ok) return res;
    state.parts = res;
  }
  return state.parts;
}

export const goals = () => get('/api/goals');
export const rooms = () => get('/api/rooms');

function seated(res) {
  if (res.ok) {
    state.code = res.code;
    state.player = res.player;
    remember(res.code);
  }
  return res;
}

export async function host(name, seed, template) {
  return seated(await post('/api/host', { name, seed: seed || undefined, template, key: seat() }));
}

export async function join(code, name) {
  return seated(await post('/api/join', { code, name, key: seat() }));
}

/// Come back to the room this browser was last in.
///
/// The same route as a join, because the client cannot tell the difference: it
/// knows its token and the last code, and whether that is a seat or a stranger
/// is the host's question. `res.rejoined` is the host's answer. The name is
/// left off deliberately -- it was in the page that went away, and the room
/// still has it.
export async function rejoin() {
  const code = lastRoom();
  if (!code) return { ok: false, error: 'this browser has not been in a room' };
  // `back` says this is a reload and not a new player: without it a code left
  // in storage would take a fresh seat in whatever room still answers to it,
  // and a room reopened on the same seed answers to the same code.
  const res = seated(await post('/api/join', { code, key: seat(), back: true }));
  if (!res.ok) forget();
  return res;
}

/// Give up the seat this browser is holding, so the lobby is the lobby again.
export function leave() {
  stop();
  forget();
  state.code = null;
  state.player = 0;
  state.view = null;
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

// ------------------------------------------------------------------ the poll
//
// A chain of `setTimeout`s, and it is worth being precise about what is now
// wrong with that and what is not.
//
// What is *not* wrong: the room falling behind. It does not any more. The
// server beats every room and every replica in it forward four times a second
// on a thread of its own, so a browser that stops asking costs the room
// nothing, and a browser that starts asking again gets a frame that is already
// current instead of triggering a minute of catch-up under a lock the other
// player is waiting on. That was the freeze, it was the *other* player's
// freeze, and it is fixed on the server.
//
// What is still wrong is this end of it. A browser is allowed to starve a
// timer: a background tab gets one a minute, a frozen page gets none until it
// is thawed, and a request on a dead socket can sit there until the server's
// own thirty-second read timeout gives up. In every one of those the room is
// fine and the picture is stale, which on screen is indistinguishable from the
// room being broken.
//
// So: no poll ever waits forever, no two are ever in flight at once, a second
// timer watches the first, coming back to the tab polls immediately rather
// than waiting out a throttled timeout, and how stale the picture is, is a
// number the screen can show rather than something a player has to guess at.

/// How long to wait for one frame before deciding this one is not coming.
/// Generous next to a 180 ms poll, because a frame is worth waiting for; short
/// next to thirty seconds, because a player is not.
const POLL_TIMEOUT = 5000;

/// How often the watchdog looks at the chain. Not a poll rate -- polls happen
/// on the chain -- just often enough that the number on screen moves while
/// nothing else is happening.
const WATCH = 1000;

const stamp = () => (typeof performance !== 'undefined' ? performance.now() : Date.now());

let timer = null;     // the chain
let watch = null;     // the watchdog over it
let period = 180;
let inflight = false;
let lastOk = 0;

/// One frame, or one reason there is not one.
///
/// Never two at once: a slow answer used to be followed by another request on
/// top of it, and a browser that fell behind would queue polls faster than the
/// server could answer them -- which is a way of making a stall permanent.
async function poll() {
  if (inflight || !state.code) return;
  inflight = true;
  const began = stamp();
  try {
    const v = await req(
      `/api/state?code=${state.code}&player=${state.player}`,
      undefined,
      POLL_TIMEOUT,
    );
    health.rtt = Math.round(stamp() - began);
    if (v.ok) {
      lastOk = stamp();
      health.misses = 0;
      health.behind = (v.sync && v.sync.behind) || 0;
      state.view = v;
      state.err = null;
      for (const f of state.onFrame) f(v);
    } else {
      health.misses++;
      state.err = v.error;
    }
  } catch (e) {
    // `req` does not throw, and a listener in `onFrame` might. A renderer that
    // fails must not stop the loop that would have drawn the next frame.
    health.misses++;
    state.err = String((e && e.message) || e);
  } finally {
    inflight = false;
    report();
  }
}

/// Say how stale the picture is, whether or not anything arrived.
function report() {
  health.lag = lastOk ? Math.round(stamp() - lastOk) : 0;
  health.live = !!lastOk && health.misses === 0 && health.lag < period * 6;
  for (const f of state.onHealth) f(health);
}

/// The tab came back, or the network did. Ask now.
function wake() {
  if (typeof document !== 'undefined' && document.visibilityState === 'hidden') return;
  poll();
}

function listen(on) {
  const verb = on ? 'addEventListener' : 'removeEventListener';
  if (typeof document !== 'undefined' && document[verb]) {
    document[verb]('visibilitychange', wake);
  }
  if (typeof window !== 'undefined' && window[verb]) {
    window[verb]('focus', wake);
    window[verb]('online', wake);
  }
}

export function start(p = 180) {
  stop();
  period = p;
  lastOk = 0;
  health.misses = 0;
  const chain = async () => {
    await poll();
    timer = setTimeout(chain, period);
  };
  chain();
  // The watchdog exists for the chain, not for the room: if the chain has been
  // throttled or a request is wedged, it asks again, and either way it keeps
  // the staleness on screen honest while nothing is arriving.
  watch = setInterval(() => {
    report();
    if (!inflight && stamp() - lastOk > period * 4) poll();
  }, WATCH);
  listen(true);
}

export function stop() {
  if (timer) clearTimeout(timer);
  if (watch) clearInterval(watch);
  timer = null;
  watch = null;
  listen(false);
}

export function onFrame(f) { state.onFrame.push(f); }
export function onRefusal(f) { state.onRefusal.push(f); }
export function onHealth(f) { state.onHealth.push(f); }

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
