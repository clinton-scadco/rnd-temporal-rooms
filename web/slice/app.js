// Experiment 15, wired together: three centuries, two fractures, and two polls.
//
// The client is still a loop with no memory. Every 180 ms it asks what one
// region is, every 600 ms it asks what the *slice* is, and it draws both. It
// never advances a clock, never applies its own command before the authority
// has, and never holds a document the authority has not seen.
//
// What is new is what changes when you walk. Prototype 3's `go()` changed one
// string and you were in another room. This one changes the same string and you
// are in another *century*: the palette reprices itself, the bench reprices
// itself, and the ground under the plot repaints — the wood becomes a street,
// the ore body becomes somebody else's foundry. The room renderer underneath is
// not told any of that, because it does not need to be, and it is served here
// unforked for the third time.

import * as net from '../room/net.js';
import * as world from '../room/world.js';
import * as bench from '../room/bench.js';
import { renderGoal, renderWho, renderFeed, renderGhosts, renderLink, toast }
  from '../room/panels.js';
import * as map from './map.js';
import * as shell from './shell.js';
import * as terrain from './terrain.js';
import * as hud from './hud.js';

const $ = id => document.getElementById(id);

let view = 'map';
let picked = null;      // the century whose card is open on the world view
let slice = null;       // the last slice frame
let statics = null;     // regions, fractures, lanes, fleets: none of them change
let land = null;        // fifteen features, three faces each
let done = new Set();   // regions we have already announced
let hasGrid = false;    // whether the century you are standing in has one

// ------------------------------------------------------------------- lobby

async function lobby() {
  hud.icons();
  const again = await post('/api/enter', { key: net.seat(), back: true });
  if (again.ok && again.rejoined) {
    net.state.code = again.at;
    net.state.player = again.player;
    await enter(again);
    toast(`back in ${again.code} as ${again.name || 'yourself'}`);
    return;
  }

  $('enter').onclick = async () => {
    const res = await post('/api/enter', {
      name: $('whoami').value,
      key: net.seat(),
      seed: $('seed').value.trim() ? Number($('seed').value.trim()) : undefined,
    });
    if (!res.ok) return ($('lobbyerr').textContent = res.error);
    net.state.code = res.at;
    net.state.player = res.player;
    await enter(res);
  };
  $('whoami').addEventListener('keydown', e => { if (e.key === 'Enter') $('enter').click(); });
}

async function post(path, body) {
  const r = await fetch(path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body || {}),
  });
  return r.json();
}

// -------------------------------------------------------------------- game

async function enter(res) {
  $('lobby').hidden = true;
  $('game').hidden = false;
  // The layout depends on the view, and every canvas below is sized from the
  // layout it is first drawn into.
  document.body.dataset.view = 'map';
  for (const id of ['roombox', 'clockbox', 'views']) $(id).hidden = false;
  $('code').textContent = res.code;
  $('copy').onclick = () => navigator.clipboard && navigator.clipboard.writeText(res.code);

  statics = await fetch('/api/regions').then(r => r.json());
  land = await fetch('/api/land').then(r => r.json());

  await repalette();

  world.init($('world'), {
    onSelect: id => {
      hud.renderSelbar(id, actions);
      net.presence(null, id, bench.bench.id, view);
    },
    onTool: t => hud.markDock(t),
    onOpen: id => {
      const i = net.byId(id);
      if (i && i.designed) actions.open(i);
    },
    onPipette: id => pipette(id),
  });
  // The ground is `terrain.js`'s, painted underneath.
  world.view.ground = false;
  hovering();
  keys();
  $('cursorclear').onclick = () => world.clearTool();
  $('goalmore').onclick = () => { $('goalwhy').hidden = !$('goalwhy').hidden; };
  // The ground, behind the plot. It reads `world.view` and is read by nobody.
  terrain.init($('terrain'));
  terrain.setLand(land);
  await bench.init({ onLeave: () => show('world') });

  map.init($('atlascanvas'), {
    onPick: tag => { picked = tag; paint(); },
    onGate: tag => gateActions.toggle(tag),
  });
  map.setStatics(statics);
  map.setLand(land);

  $('viewmap').onclick = () => show('map');
  $('viewworld').onclick = () => {
    show('world');
    if (!fitted.has(net.state.code)) fitPlot();
  };
  $('viewbench').onclick = () => {
    if (!bench.bench.id) {
      const first = (net.state.view.world.installs || []).find(i => i.designed);
      if (!first) return toast('there is no machine to open yet');
      bench.open(first.id);
    }
    show('bench');
  };
  $('wonclose').onclick = () => { $('won').hidden = true; };

  net.onRefusal(e => toast(e));
  net.onFrame(frame);
  net.onHealth(renderLink);
  net.start();
  pump();

  $('briefing').hidden = false;
  $('briefbrief').textContent =
    '1890 has the ore and no electricity. 2037 has the grid and no ore. 2070 has the best '
    + 'process on the map and nothing to put in it. The clock does not pause.';
  $('briefstart').onclick = async () => {
    await post('/api/start', {});
    $('briefing').hidden = true;
  };
  show('map');
}

/// The slice poll. Slower than the region's, because a world map does not need
/// to be redrawn sixty times a minute.
function pump(period = 600) {
  const tick = async () => {
    try {
      const v = await fetch(`/api/slice?player=${net.state.player}`, {
        signal: AbortSignal.timeout ? AbortSignal.timeout(5000) : undefined,
      }).then(r => r.json());
      if (v.ok) {
        slice = v;
        if (v.started) $('briefing').hidden = true;
        paint();
        announce(v);
      }
    } catch (e) {
      // A slice that cannot be reached is a slice carrying on without us; the
      // next poll will say so.
    }
    setTimeout(tick, period);
  };
  tick();
}

function paint() {
  if (!slice) return;
  $('clock').textContent = shell.clock(slice.tick);
  const at = (slice.regions || []).find(r => r.tag === slice.at);
  const ph = at && (slice.phases || []).find(p => p.tag === at.phase);
  hasGrid = !!(ph && ph.grid);
  hud.renderDock(hasGrid, pickTool);
  document.body.dataset.era = at ? at.phase : '';
  shell.renderWhere(slice, go);
  shell.renderCrates(slice, slice.at);
  shell.renderRegion(slice, picked || slice.at, go);
  shell.renderGates(slice, gateActions);
  shell.renderCentury(slice, slice.phases, slice.at);
  shell.renderLanes(slice, laneActions);
  shell.renderProvenance(slice);
  shell.renderNews(slice);
  shell.renderRegionIO(slice, slice.at);
  // The century the plot is standing in. Changing this is what a fracture
  // crossing looks like from the ground.
  const here = (slice.regions || []).find(r => r.tag === slice.at);
  if (here) terrain.setPhase(here.phase);
  if (view === 'map') map.show(slice);
  repriceIfMoved();
}

/// The palette and the bench are repriced whenever the century changes or the
/// machinery ledger moves, and at no other time.
///
/// Both are functions of *where you are standing*, which is the one thing about
/// this front end that Prototype 3's did not have to think about: camp's
/// catalogue changed when something unlocked, and this one changes when you
/// walk through a hole in time.
let priceSig = '';
async function repriceIfMoved() {
  if (!slice) return;
  const r = (slice.regions || []).find(r => r.tag === slice.at);
  const sig = `${slice.at}|${r && r.machinery ? r.machinery.free : 0}`;
  if (sig === priceSig) return;
  priceSig = sig;
  await repalette();
}

async function repalette() {
  const code = net.state.code || 'valley';
  const cat = await fetch(`/api/catalogue?code=${encodeURIComponent(code)}`).then(r => r.json());
  net.state.catalogue = cat;
  // The bench's component list is priced the same way, by the same century.
  const parts = await fetch(`/api/parts?code=${encodeURIComponent(code)}`).then(r => r.json());
  net.state.parts = parts;
  markPartPrices(parts);
  const c = $('benchcentury');
  if (c) c.textContent = parts.phase || '';
}

/// The price of every component, on the bench's own palette.
///
/// The bench is Prototype 2's, unforked, so this decorates its buttons after it
/// has drawn them rather than asking it to draw them differently. A component
/// nobody in this century can make gets the number it costs to have one carried
/// back; the one that no crate would help gets told so.
function markPartPrices(parts) {
  for (const p of parts.parts || []) {
    const tag = p.kind;
    const b = document.querySelector(`#parts button[data-kind="${tag}"]`);
    if (!b) continue;
    b.classList.toggle('costly', !p.native && p.crateable);
    b.classList.toggle('illegal', !p.crateable && !p.native);
    let badge = b.querySelector('.price');
    if (!badge) {
      badge = document.createElement('i');
      badge.className = 'price';
      b.appendChild(badge);
    }
    if (p.native) {
      badge.textContent = '';
      b.title = b.title || '';
    } else if (!p.crateable) {
      badge.textContent = 'no';
      b.title = p.why || 'not possible in this century';
    } else {
      badge.textContent = shell.num(p.gears);
      const frames = (p.frames || []).filter(f => f.here).map(f => f.title);
      b.title =
        `${shell.num(p.gears)} gears of imported machinery, out of ${p.madeIn}` +
        (p.why ? ` — ${p.why}` : '') +
        (frames.length ? `. Native on ${frames.join(' or ')}.` : '');
    }
  }
}

/// Walk into another century. One string.
async function go(tag) {
  const res = await post('/api/travel', { player: net.state.player, region: tag });
  if (!res.ok) return toast(res.error);
  net.state.code = tag;
  picked = tag;
  world.select(null);
  world.clearTool();
  bench.bench.id = null;
  const r = statics && (statics.regions || []).find(r => r.tag === tag);
  if (r) terrain.setPhase(r.phase);
  await repalette();
  show('world');
  fitPlot();
}

/// The whole plot, centred in the stage. Done on arrival in a region, so a
/// century opens as a place rather than a corner of one.
const fitted = new Set();
function fitPlot() {
  const cat = net.state.catalogue;
  const plot = (cat && cat.plot) || 128;
  const r = $('world').getBoundingClientRect();
  if (!r.width || !r.height) return;
  const s = Math.max(0.35, Math.min(4, Math.min(r.width - 80, r.height - 80) / (plot * 7)));
  world.view.scale = s;
  world.view.ox = (r.width - plot * 7 * s) / 2;
  world.view.oy = (r.height - plot * 7 * s) / 2;
  world.invalidate();
  fitted.add(net.state.code);
}

function show(which) {
  view = which;
  document.body.dataset.view = which;
  $('bench').hidden = which !== 'bench';
  $('atlas').hidden = which !== 'map';
  if (which !== 'world') $('hovercard').hidden = true;
  for (const id of ['world', 'ghosts', 'terrain']) {
    const el = $(id);
    if (el) el.style.visibility = which === 'world' ? 'visible' : 'hidden';
  }
  $('viewmap').classList.toggle('on', which === 'map');
  $('viewworld').classList.toggle('on', which === 'world');
  $('viewbench').classList.toggle('on', which === 'bench');
  // Every canvas sizes itself from its box on a resize, and the boxes have
  // just changed shape.
  dispatchEvent(new Event('resize'));
  if (which === 'bench') bench.resize();
  else if (which === 'map') map.resize();
  else { world.invalidate(); terrain.resize(); }
  net.presence(null, world.selection, which === 'bench' ? bench.bench.id : null, which);
}

/// One frame of one region. Everything in the region view is a function of this.
let lastSig = '';
function frame(v) {
  renderGoal(v);
  renderWho(v);
  renderSync(v);
  renderFeed(v);
  renderGhosts(v, project);
  world.invalidate();
  hud.renderSelbar(world.selection, actions);
  refreshHover();

  const machines = v.world.installs.filter(i => i.designed);
  $('viewbench').disabled = !machines.length;
  if (view === 'bench' && bench.bench.id) {
    const me = v.world.installs.find(i => i.id === bench.bench.id);
    const sig = me ? JSON.stringify([me.editor, me.hasDraft, me.draftHash, me.macro]) : '';
    if (sig !== lastSig) { lastSig = sig; bench.refresh(false); owed(); }
  }
}

/// What the machine on the bench owes in imported plant, in the century it is
/// being drawn in.
async function owed() {
  const el = $('benchowed');
  if (!el || !bench.bench.id) return;
  const res = await post('/api/inside', {
    code: net.state.code,
    id: bench.bench.id,
    draft: true,
  });
  if (!res.ok) return;
  el.textContent = res.crated
    ? `${shell.num(res.crated)} gears of imported plant`
    : 'every component of this is local';
  el.className = res.crated ? 'owed costly' : 'owed';
  el.title = (res.imports || []).map(i => `${i.unit}: ${i.title}, ${shell.num(i.gears)}`).join('\n');
}

/// A region that has just met its objective, announced once.
function announce(v) {
  for (const r of v.regions) {
    if (!r.done || done.has(r.tag)) continue;
    done.add(r.tag);
    if (!v.started) continue;
    $('won').hidden = false;
    $('wontitle').textContent = `${r.title} met its objective`;
    $('wondetail').innerHTML =
      `<b>met at</b><span>${shell.clock(r.doneAt)}</span>` +
      `<b>installations</b><span>${r.installs}, ${r.machines} of them machines</span>` +
      `<b>footprint</b><span>${shell.num(r.footprint)} tiles</span>` +
      `<b>on the grid</b><span>${shell.num(r.gridMW)} MW</span>` +
      (r.machinery && r.machinery.standing
        ? `<b>imported plant</b><span>${shell.num(r.machinery.standing)} gears standing</span>`
        : '');
  }
}

function project(x, y, w, h) {
  const s = world.view.scale * 7;
  return [world.view.ox + x * s, world.view.oy + y * s, w * s, h * s];
}

// ----------------------------------------------------------------- actions

const actions = {
  open: i => { bench.open(i.id); show('bench'); owed(); },
  connect: (i, item) => {
    world.connectFrom(i.id, item);
    hud.markDock(world.tool);
  },
  copy: i => pipette(i.id),
  delete: i => {
    net.send(i.role === 'storage' ? 'DeleteStorage' : 'DeleteMachine', { id: i.id });
    world.select(null);
  },
  unwire: w => { net.send('DeleteConnection', w); world.select(null); },
  unlink: h => { net.send('DeleteWorldLink', { id: h.id }); world.select(null); },
};

// ------------------------------------------------------------------- tools

/// A dock button, or its key. Pressing the tool you are already holding puts
/// it down, which is the third way to let go after right-click and the x.
function pickTool(id) {
  const t = hud.toolById(id);
  if (!t || (t.needsGrid && !hasGrid)) return;
  if (hud.toolIdOf(world.tool) === id && id !== 'pick') return world.clearTool();
  if (t.proto) world.setTool('place', t.proto);
  else world.setTool(id);
  hud.markDock(world.tool);
}

/// Pick up another one of whatever is under the pointer.
///
/// A machine comes with its design -- the design *is* what kind of machine it
/// is -- and a depot comes with the item it ships. Nothing under the pointer
/// means let go, as Q does in the factory games this borrows it from.
async function pipette(id) {
  const i = id === null || id === undefined ? null : net.byId(id);
  if (!i) return world.clearTool();
  let design = null;
  if (i.designed && i.macro) {
    const res = await net.form(i.id, false);
    if (!res.ok) return toast(res.error);
    design = res.design;
  }
  world.setTool('place', i.proto, design);
  world.tool.face = i.face || 0;
  if (i.role === 'sink' && i.item && (net.proto(i.proto) || {}).choosesItem) world.tool.ship = i.item;
  hud.markDock(world.tool);
  hud.renderCursor(world.tool, `${i.name}${design ? ' · copy' : ''}`);
}

function keys() {
  addEventListener('keydown', e => {
    const tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
    if (e.ctrlKey || e.metaKey || e.altKey) return;
    const k = String(e.key || '').toLowerCase();
    if (k === 'm' && view !== 'bench') return show(view === 'map' ? 'world' : 'map');
    if (view !== 'world') return;
    if (k === 'q') {
      const p = world.pointer();
      return pipette(p ? world.thingAt(p.raw[0], p.raw[1]) : null);
    }
    const t = hud.toolByKey(k);
    if (t) { e.preventDefault(); pickTool(t.id); }
  });
}

// ------------------------------------------------------------------- hover

/// The card under the pointer, and the bar over the selection, kept where
/// they belong while the plot moves under them.
let pointerPx = null;
function hovering() {
  const cv = $('world');
  cv.addEventListener('pointermove', e => {
    const r = cv.getBoundingClientRect();
    pointerPx = [e.clientX - r.left, e.clientY - r.top];
    refreshHover();
  });
  cv.addEventListener('pointerleave', () => { pointerPx = null; refreshHover(); });
  const tick = () => {
    const sel = world.selection;
    hud.placeSelbar(null, sel === null ? null : net.byId(sel));
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

function refreshHover() {
  const quiet = view !== 'world' || !pointerPx || world.tool.mode === 'place';
  const target = quiet ? null : hud.targetAt(world.pointer(), terrain.here());
  hud.renderHover(target, pointerPx ? pointerPx[0] : 0, pointerPx ? pointerPx[1] : 0);
}

/// The replica check, as one dot. The hashes are its tooltip.
function renderSync(v) {
  const box = $('sync');
  if (!box) return;
  const s = v.sync || {};
  box.className = 'sync ' + (s.agrees === null || s.agrees === undefined ? '' : s.agrees ? 'ok' : 'no');
  box.title = `replicas ${s.agrees === null || s.agrees === undefined ? 'not yet compared'
    : s.agrees ? 'agree' : 'DIVERGED'}\nyou  ${s.hash || '--'}\nhost ${s.hostHash || '--'}`;
}

const laneActions = {
  open: async (from, to, item, fleet) => {
    const res = await post('/api/route', {
      do: 'open', player: net.state.player, from, to, item, fleet,
    });
    if (!res.ok) toast(res.error);
  },
  close: async id => {
    const res = await post('/api/route', { do: 'close', route: id });
    if (!res.ok) toast(res.error);
  },
  cap: async (id, cap) => {
    const res = await post('/api/route', { do: 'cap', route: id, cap });
    if (!res.ok) toast(res.error);
  },
};

/// Putting an interface on a fracture, and taking one down.
///
/// The one command in this experiment with no counterpart in any earlier
/// prototype, and it is one button. What it does not do is pay for anything:
/// whether the fracture *holds* is decided every five simulated seconds by
/// whether the region behind it is delivering the megawatts.
const gateActions = {
  open: async tag => {
    const res = await post('/api/gate', { do: 'open', player: net.state.player, fracture: tag });
    toast(res.ok ? `an interface goes up on the ${tag} fracture` : res.error);
  },
  close: async tag => {
    const res = await post('/api/gate', { do: 'close', fracture: tag });
    if (!res.ok) toast(res.error);
  },
  toggle: tag => {
    const g = ((slice && slice.shipping && slice.shipping.interfaces) || []).find(
      g => g.fracture === tag
    );
    return g ? gateActions.close(tag) : gateActions.open(tag);
  },
};

lobby();
