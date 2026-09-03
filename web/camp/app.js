// Prototype 3, wired together: a map, five rooms, and two polls.
//
// The client is still a loop with no memory. Every 180 ms it asks the campaign
// what one room is, every 600 ms it asks what the *campaign* is, and it draws
// both. It never advances a clock, never applies its own command before the
// authority has, and never holds a document the authority has not seen.
//
// What is genuinely new is which room it is asking about. Walking from Iron
// Valley to Manufacturing changes one string in this file. It does not pause
// the room you left, it does not unload it, and it does not restart it: the
// campaign has been running all five the whole time, and the replica of the
// room you were in has been fed every command and every arrival that happened
// while you were gone. That is the experiment, and this is the two lines of
// client code that spend it.

import * as net from '../room/net.js';
import * as world from '../room/world.js';
import * as bench from '../room/bench.js';
import { renderGoal, renderWho, renderSync, renderFeed, renderGhosts, renderInspector,
         renderPalette, renderLink, markTool, toast } from '../room/panels.js';
import * as map from './map.js';
import * as shell from './shell.js';

const $ = id => document.getElementById(id);

let view = 'map';
let picked = null;      // the room whose card is open on the map
let camp = null;        // the last campaign frame
let done = new Set();   // rooms we have already announced

// ------------------------------------------------------------------- lobby

async function lobby() {
  // A browser that was in the campaign goes back to it before the lobby is
  // ever drawn. A reload is not a decision to leave -- and a campaign seat
  // owns five rooms, a place on the map, and everything the tech tree has been
  // opened with, so losing one to a refresh was the worst version of that bug
  // this project had.
  //
  // `back` says this is a reload and not a new player: without it a token left
  // over from a campaign that has since been thrown away would take a fresh
  // seat in whatever campaign is running now.
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

  const cat = await net.catalogue();
  // A palette click places an empty chassis; the small `example` button
  // beside it places the catalogue's worked answer, and says so.
  renderPalette(cat, (tag, example) => {
    world.setTool('place', tag, null, example);
    markTool('place', tag, example);
  });
  document.querySelectorAll('.tools button').forEach(b => {
    b.onclick = () => {
      const m = world.tool.mode === b.dataset.mode ? 'pick' : b.dataset.mode;
      world.setTool(m);
      markTool(m);
    };
  });
  markTool('pick');

  world.init($('world'), {
    // Hovering shows; clicking pins. A pinned selection comes back the moment
    // the pointer leaves the thing it wandered onto, so reading the room never
    // costs you the building you were working on.
    onHover: id => renderInspector(id === null ? world.selection : id, actions),
    onSelect: id => {
      renderInspector(id, actions);
      net.presence(null, id, bench.bench.id, view);
    },
  });
  await bench.init({ onLeave: () => show('world') });
  $('benchkeep').onclick = keep;

  map.init($('atlascanvas'), { onPick: tag => { picked = tag; paint(); } });
  const atlas = await fetch('/api/sites').then(r => r.json());
  map.setSites(atlas);

  $('viewmap').onclick = () => show('map');
  $('viewworld').onclick = () => show('world');
  $('viewbench').onclick = () => {
    if (!bench.bench.id) {
      // Including one with nothing in it yet, which is exactly the one you
      // want to open.
      const first = (net.state.view.world.installs || []).find(i => i.designed);
      if (!first) return toast('there is no machine to open yet');
      bench.open(first.id);
    }
    show('bench');
  };
  $('wonclose').onclick = () => { $('won').hidden = true; };

  net.onRefusal(e => toast(e));
  net.onFrame(frame);
  // Health arrives whether or not a frame does: a screen that has stopped
  // being told about the campaign has to be able to say so, and it cannot say
  // so in a handler that only runs when it is told.
  net.onHealth(renderLink);
  net.start();
  pump();

  // The objective is on screen before anybody builds -- five of them, in fact
  // -- and that is the only pause this game has.
  $('briefing').hidden = false;
  $('briefbrief').textContent =
    'Five rooms on one clock. Coal Basin is open; the other four are waiting on it.';
  $('briefnote').textContent =
    'Finish a room and it starts supplying the ones after it, and hands over components that '
    + 'change what a machine can be. Nothing you leave behind stops.';
  $('briefstart').onclick = async () => {
    await post('/api/start', {});
    $('briefing').hidden = true;
  };
  show('map');
}

/// The campaign poll. Slower than the room's, because a map does not need to
/// be redrawn sixty times a minute.
function pump(period = 600) {
  const tick = async () => {
    try {
      // Bounded, for the same reason the room's poll is: a socket that is
      // never going to answer must not stop the loop that would have asked
      // again. The campaign keeps running either way -- it beats on a thread
      // of its own now -- so a missed pump costs a stale map and nothing else.
      const v = await fetch(`/api/camp?player=${net.state.player}`, {
        signal: AbortSignal.timeout ? AbortSignal.timeout(5000) : undefined,
      }).then(r => r.json());
      if (v.ok) {
        camp = v;
        if (v.started) $('briefing').hidden = true;
        paint();
        announce(v);
      }
    } catch (e) {
      // A campaign that cannot be reached is a campaign that carries on
      // without us; the next poll will say so.
    }
    setTimeout(tick, period);
  };
  tick();
}

function paint() {
  if (!camp) return;
  $('clock').textContent = shell.clock(camp.tick);
  shell.renderWhere(camp, go);
  shell.renderRoom(camp, picked || camp.at, go);
  shell.renderShelf(camp, shelfActions);
  shell.renderShelfPalette(camp, fromShelf);
  shell.renderTech(camp);
  shell.renderLanes(camp, laneActions);
  shell.renderNews(camp);
  // The room you are standing in, not the one whose card is open on the map:
  // this panel is about the factory in front of you.
  shell.renderRoomIO(camp, camp.at);
  if (view === 'map') map.show(camp);
  // The palette follows the components: a prototype becomes placeable the
  // moment the last of its parts arrives, and nobody has to be told twice.
  net.catalogue().then(() => refreshLocks());
}

let lockSig = '';
async function refreshLocks() {
  const sig = camp && camp.tech ? String(camp.tech.earned) : '';
  if (sig === lockSig) return;
  lockSig = sig;
  // The catalogue's locks are a function of the campaign, so it is re-fetched
  // rather than cached across an unlock.
  const cat = await fetch('/api/catalogue').then(r => r.json());
  net.state.catalogue = cat;
  shell.markLocks(cat);
}

/// Walk to another room. One string.
async function go(tag) {
  const res = await post('/api/travel', { player: net.state.player, site: tag });
  if (!res.ok) return toast(res.error);
  net.state.code = tag;
  picked = tag;
  world.select(null);
  bench.bench.id = null;
  show('world');
}

function show(which) {
  view = which;
  document.body.dataset.view = which;
  $('bench').hidden = which !== 'bench';
  $('atlas').hidden = which !== 'map';
  for (const id of ['world', 'ghosts']) {
    $(id).style.visibility = which === 'world' ? 'visible' : 'hidden';
  }
  $('viewmap').classList.toggle('on', which === 'map');
  $('viewworld').classList.toggle('on', which === 'world');
  $('viewbench').classList.toggle('on', which === 'bench');
  if (which === 'bench') bench.resize();
  else if (which === 'map') map.resize();
  else world.invalidate();
  net.presence(null, world.selection, which === 'bench' ? bench.bench.id : null, which);
}

/// One frame of one room. Everything in the room view is a function of this.
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
    const sig = me ? JSON.stringify([me.editor, me.hasDraft, me.macro]) : '';
    if (sig !== lastSig) { lastSig = sig; bench.refresh(false); }
  }
}

/// A room that has just come online, announced once.
function announce(v) {
  for (const r of v.rooms) {
    if (!r.done || done.has(r.tag)) continue;
    done.add(r.tag);
    if (!v.started) continue;
    $('won').hidden = false;
    $('wontitle').textContent = `${r.title} is producing`;
    const gives = r.gives.map(g => `<b>${g.title}</b><span>${g.opens}</span>`).join('');
    $('wondetail').innerHTML =
      `<b>completed at</b><span>${shell.clock(r.doneAt)}</span>` +
      `<b>installations</b><span>${r.installs}, ${r.machines} of them machines</span>` +
      `<b>footprint</b><span>${shell.num(r.footprint)} tiles</span>` +
      (gives || '<b>hands over</b><span>nothing new</span>');
  }
}

function project(x, y, w, h) {
  const s = world.view.scale * 7;
  return [world.view.ox + x * s, world.view.oy + y * s, w * s, h * s];
}

// ----------------------------------------------------------------- actions

const actions = {
  open: i => { bench.open(i.id); show('bench'); },
  // Note 6: a connection starts on the building it comes out of, not in a
  // tool list below the fold. The port has already named the item, so the next
  // click is a destination and nothing else is asked.
  connect: (i, item) => {
    world.connectFrom(i.id, item);
    markTool('connect');
    toast(`${i.name} \u00b7 ${item} \u2014 click where it goes`);
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

const shelfActions = {
  copy: async id => {
    const name = prompt('a name for the copy');
    if (!name) return;
    const res = await post('/api/shelf', { do: 'copy', player: net.state.player, design: id, name });
    if (!res.ok) toast(res.error);
  },
  forget: async id => {
    const res = await post('/api/shelf', { do: 'forget', design: id });
    if (!res.ok) toast(res.error);
  },
  place: fromShelf,
};

/// Take a design off the shelf and put it under the pointer.
///
/// The same path a duplicate takes in Prototype 2: the design comes back from
/// the authority, rides in the placement command, and the campaign checks
/// every component in it against what has been unlocked before it lands.
async function fromShelf(id) {
  const saved = ((camp && camp.shelf && camp.shelf.designs) || []).find(s => s.id === id);
  if (!saved) return toast('that design is not on the shelf');
  const res = await fetch('/api/form', {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify({ design: id }),
  }).then(r => r.json());
  if (!res.ok) return toast(res.error);
  show('world');
  world.setTool('place', saved.proto, res.design);
  markTool('place', saved.proto);
  toast(`${saved.name} — click where it goes`);
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

/// Keep the machine that is open on the shelf, so the next room can start from
/// it rather than from nothing.
async function keep() {
  if (!bench.bench.id) return toast('open a machine first');
  const name = prompt('a name for this design');
  if (!name) return;
  const res = await post('/api/shelf', {
    do: 'save',
    player: net.state.player,
    code: net.state.code,
    id: bench.bench.id,
    name,
    draft: false,
  });
  toast(res.ok ? `${name} is on the shelf` : res.error);
}

lobby();
