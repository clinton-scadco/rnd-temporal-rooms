// Prototype 2, wired together: a lobby, a poll, and two windows onto the same
// room.
//
// The whole client is a loop with no memory. Every 180 ms it asks the host
// what the room is, and draws that. It never advances a clock of its own,
// never applies its own command before the host has, and never holds a
// document that the host has not seen -- because the moment a client believes
// something the host does not, the experiment is over and nobody has been
// told.

import * as net from './net.js';
import * as world from './world.js';
import * as bench from './bench.js';
import {
  renderGoal, renderWho, renderSync, renderFeed, renderGhosts, renderInspector,
  renderPalette, renderLink, markTool, toast,
} from './panels.js';

const $ = id => document.getElementById(id);
let mode = 'world';
let lastGoal = null;

// -------------------------------------------------------------------- lobby

async function lobby() {
  // A browser that was in a room goes back to it before the lobby is ever
  // drawn. A reload is not a decision to leave.
  const back = await net.rejoin();
  if (back.ok && back.rejoined) {
    await enter(back.host);
    toast(`back in ${back.code} as ${back.name || 'yourself'}`);
    return;
  }

  const goals = await net.goals();
  if (!goals.ok) return ($('lobbyerr').textContent = goals.error);
  $('template').innerHTML = '<option value="">rolled from the seed</option>' +
    goals.templates.map(t => `<option value="${t.id}">${t.title} — ${t.family}</option>`).join('');
  refreshRooms();
  setInterval(() => { if (!net.state.code) refreshRooms(); }, 3000);

  $('host').onclick = async () => {
    const res = await net.host($('hostname').value, $('seed').value.trim(), $('template').value);
    if (!res.ok) return ($('lobbyerr').textContent = res.error);
    await enter(true);
  };
  $('join').onclick = async () => {
    const code = $('joincode').value.trim().toUpperCase();
    const res = await net.join(code, $('joinname').value);
    if (!res.ok) return ($('lobbyerr').textContent = res.error);
    await enter(false);
  };
  $('joincode').addEventListener('keydown', e => { if (e.key === 'Enter') $('join').click(); });
}

async function refreshRooms() {
  const res = await net.rooms();
  const box = $('openrooms');
  if (!res.ok || !res.rooms.length) { box.innerHTML = 'no rooms open yet.'; return; }
  box.innerHTML = 'open: ' + res.rooms.map(r =>
    `<a data-code="${r.code}">${r.code}</a> ${r.goal} · ${r.players}p · ${net.clock(r.tick)}`).join('<br>');
  box.querySelectorAll('a').forEach(a => {
    a.onclick = () => { $('joincode').value = a.dataset.code; };
  });
}

// --------------------------------------------------------------------- game

async function enter(host) {
  $('lobby').hidden = true;
  $('game').hidden = false;
  for (const id of ['roombox', 'clockbox', 'views']) $(id).hidden = false;
  $('code').textContent = net.state.code;
  $('copy').onclick = () => navigator.clipboard && navigator.clipboard.writeText(net.state.code);
  // A reload now comes straight back here, so there has to be a door out that
  // is not the address bar. Leaving forgets the room, not the seat: the room
  // goes on running and the code still gets you back into the same one.
  $('leave').onclick = () => { net.leave(); location.reload(); };

  const cat = await net.catalogue();
  if (!cat.ok) return toast(cat.error);
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
    onHover: id => renderInspector(id === null ? world.selection : id, inspectorActions),
    onSelect: id => {
      renderInspector(id, inspectorActions);
      net.presence(null, id, bench.bench.id, mode);
    },
  });
  await bench.init({ onLeave: () => show('world') });

  $('viewworld').onclick = () => show('world');
  $('viewbench').onclick = () => {
    // The machine window needs a machine. Without one chosen, take the first
    // one anybody has built.
    if (!bench.bench.id) {
      const first = (net.state.view.world.installs || []).find(i => i.role === 'machine');
      if (!first) return toast('there is no machine to open yet');
      bench.open(first.id);
    }
    show('bench');
  };
  $('wonclose').onclick = () => { $('won').hidden = true; };

  net.onRefusal(e => toast(e));
  net.onFrame(frame);
  // Health arrives whether or not a frame does, which is the whole point of
  // it: a screen that has stopped being told about the room has to be able to
  // say so, and it cannot say so in a handler that only runs when it is told.
  net.onHealth(renderLink);
  net.start();
  setTimeout(() => world.focus(), 400);

  // Section 19: the objective is on screen before anybody builds, and that is
  // the only pause this game has.
  $('briefing').hidden = false;
  $('briefstart').hidden = !host;
  $('briefwait').hidden = host;
  $('briefstart').onclick = async () => {
    await net.begin();
    $('briefing').hidden = true;
  };
}

const inspectorActions = {
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
  // A wire and a transport are the two things a player draws that have no
  // building to select, and until they could be taken back a wrong one was
  // permanent -- the factory said what was wrong with it and there was no way
  // to act on the answer.
  unwire: w => { net.send('DeleteConnection', w); world.select(null); },
  unlink: h => { net.send('DeleteWorldLink', { id: h.id }); world.select(null); },
  // Every placed machine owns its design, so a duplicate is a placement
  // carrying a copy of it. From the moment it lands the two are strangers.
  duplicate: async i => {
    const res = await net.form(i.id, false);
    if (!res.ok) return toast(res.error);
    world.setTool('place', i.proto, res.design);
    markTool('place', i.proto);
    toast(`a copy of ${i.name} — click where it goes`);
  },
};

function show(which) {
  mode = which;
  $('bench').hidden = which !== 'bench';
  $('world').style.visibility = which === 'bench' ? 'hidden' : 'visible';
  $('ghosts').style.visibility = which === 'bench' ? 'hidden' : 'visible';
  $('viewworld').classList.toggle('on', which === 'world');
  $('viewbench').classList.toggle('on', which === 'bench');
  if (which === 'bench') bench.resize(); else world.invalidate();
  net.presence(null, world.selection, which === 'bench' ? bench.bench.id : null, which);
}

/// One frame the host sent. Everything on the screen is a function of this.
let lastSig = '';
function frame(v) {
  $('clock').textContent = net.clock(v.tick);
  // A joiner sits on the briefing until the host starts the clock; after that
  // nothing ever puts it back.
  if (v.started) $('briefing').hidden = true;
  else {
    $('brieftitle').textContent = v.goal.title;
    $('briefbrief').textContent = v.goal.brief;
    $('briefnote').textContent = v.goal.note;
  }
  renderGoal(v);
  renderWho(v);
  renderSync(v);
  renderFeed(v);
  renderGhosts(v, project);
  world.invalidate();
  if (world.selection) renderInspector(world.selection, inspectorActions);

  const machines = v.world.installs.filter(i => i.role === 'machine');
  $('viewbench').disabled = !machines.length;
  if (mode === 'bench' && bench.bench.id) {
    // Only rebuild the 3D window when the document under it has changed --
    // it is a whole plant regenerated from a document, not a mesh nudged.
    const me = v.world.installs.find(i => i.id === bench.bench.id);
    const sig = me ? JSON.stringify([me.editor, me.hasDraft, me.macro]) : '';
    if (sig !== lastSig) { lastSig = sig; bench.refresh(false); }
  }
  const done = v.goal.progress.done;
  if (done && lastGoal !== done.at) {
    lastGoal = done.at;
    won(v, done);
  }
}

/// Where a world tile lands on the screen, for the ghosts that float over it.
function project(x, y, w, h) {
  const s = world.view.scale * 7;
  return [world.view.ox + x * s, world.view.oy + y * s, w * s, h * s];
}

function won(v, done) {
  $('won').hidden = false;
  const shipped = done.shipped.map(s => `${net.num(s.qty)} ${s.item}`).join(', ');
  const drawn = done.drawn.map(s => `${net.num(s.qty)} ${s.item}`).join(', ');
  $('wondetail').innerHTML =
    `<b>completed at</b><span>${net.clock(done.at)} (tick ${done.at})</span>` +
    `<b>shipped</b><span>${shipped || 'nothing'}</span>` +
    `<b>drawn</b><span>${drawn || 'nothing'}</span>` +
    `<b>installations</b><span>${done.installs}</span>` +
    `<b>machines</b><span>${done.machines}, ${done.designs} of them designed</span>` +
    `<b>footprint</b><span>${net.num(done.footprint)} tiles</span>`;
}

lobby();
