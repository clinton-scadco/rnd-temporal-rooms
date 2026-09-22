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
import { renderGoal, renderWho, renderSync, renderFeed, renderGhosts, renderInspector,
         renderPalette, renderLink, markTool, toast } from '../room/panels.js';
import * as map from './map.js';
import * as shell from './shell.js';
import * as terrain from './terrain.js';

const $ = id => document.getElementById(id);

let view = 'map';
let picked = null;      // the century whose card is open on the world view
let slice = null;       // the last slice frame
let statics = null;     // regions, fractures, lanes, fleets: none of them change
let land = null;        // fifteen features, three faces each
let done = new Set();   // regions we have already announced

// ------------------------------------------------------------------- lobby

async function lobby() {
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
  for (const id of ['roombox', 'clockbox', 'views']) $(id).hidden = false;
  $('code').textContent = res.code;
  $('copy').onclick = () => navigator.clipboard && navigator.clipboard.writeText(res.code);

  statics = await fetch('/api/regions').then(r => r.json());
  land = await fetch('/api/land').then(r => r.json());

  await repalette();
  document.querySelectorAll('.tools button').forEach(b => {
    b.onclick = () => {
      const m = world.tool.mode === b.dataset.mode ? 'pick' : b.dataset.mode;
      world.setTool(m);
      markTool(m);
    };
  });
  markTool('pick');

  world.init($('world'), {
    onHover: id => renderInspector(id === null ? world.selection : id, actions),
    onSelect: id => {
      renderInspector(id, actions);
      net.presence(null, id, bench.bench.id, view);
    },
  });
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
  $('viewworld').onclick = () => show('world');
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
    'Three regions of one valley, a hundred and eighty years apart, all running at once. '
    + '1890 has the ore and no electricity. 2037 has the grid and no ore. 2070 has the best '
    + 'process on the map and nothing to put in it.';
  $('briefnote').textContent =
    'Nothing is researched and nothing is locked. A motor in 1890 is a price in gears that '
    + 'somebody later has to make and ship backwards — through a fracture that only stays open '
    + 'while somebody’s grid is holding it.';
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
  shell.renderWhere(slice, go);
  shell.renderCrates(slice, slice.at);
  shell.renderRegion(slice, picked || slice.at, go);
  shell.renderGates(slice, gateActions);
  shell.renderCentury(slice, slice.phases, slice.at);
  shell.renderLanes(slice, laneActions);
  shell.renderProvenance(slice);
  shell.renderNews(slice);
  shell.renderRegionIO(slice, slice.at);
  shell.renderTerrain(terrain.here());
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
  renderPalette(cat, (tag, example) => {
    world.setTool('place', tag, null, example);
    markTool('place', tag, example);
  });
  shell.markPrices(cat);
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
  bench.bench.id = null;
  const r = statics && (statics.regions || []).find(r => r.tag === tag);
  if (r) terrain.setPhase(r.phase);
  await repalette();
  show('world');
  if (r) toast(`${r.title} — ${r.problem}`);
}

function show(which) {
  view = which;
  document.body.dataset.view = which;
  $('bench').hidden = which !== 'bench';
  $('atlas').hidden = which !== 'map';
  for (const id of ['world', 'ghosts', 'terrain']) {
    const el = $(id);
    if (el) el.style.visibility = which === 'world' ? 'visible' : 'hidden';
  }
  $('viewmap').classList.toggle('on', which === 'map');
  $('viewworld').classList.toggle('on', which === 'world');
  $('viewbench').classList.toggle('on', which === 'bench');
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
  if (world.selection) renderInspector(world.selection, actions);

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
    markTool('connect');
    toast(`${i.name} · ${item} — click where it goes`);
  },
  delete: i => net.send(i.role === 'storage' ? 'DeleteStorage' : 'DeleteMachine', { id: i.id }),
  unwire: w => { net.send('DeleteConnection', w); world.select(null); },
  unlink: h => { net.send('DeleteWorldLink', { id: h.id }); world.select(null); },
  duplicate: async i => {
    const res = await net.form(i.id, false);
    if (!res.ok) return toast(res.error);
    world.setTool('place', i.proto, res.design);
    markTool('place', i.proto);
    toast(`a copy of ${i.name} — click where it goes`);
  },
};

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
