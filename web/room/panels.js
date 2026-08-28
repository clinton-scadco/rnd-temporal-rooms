// Everything that is words rather than pixels: the objective, the inspector,
// the feed, the ghosts, and the one panel that is the whole experiment --
// three hashes, compared, once a simulated second.

import * as net from './net.js';

const $ = id => document.getElementById(id);

// ------------------------------------------------------------ small things

export function toast(msg) {
  let el = document.querySelector('.toast');
  if (!el) {
    el = document.createElement('div');
    el.className = 'toast';
    el.style.cssText =
      'position:fixed;left:50%;bottom:34px;transform:translateX(-50%);z-index:40;' +
      'background:#33191A;color:#E06C6C;border:1px solid #E06C6C;border-radius:4px;' +
      'padding:.5em .9em;font:12px var(--ui);pointer-events:none;transition:opacity .25s';
    document.body.appendChild(el);
  }
  el.textContent = msg;
  el.style.opacity = '1';
  clearTimeout(el._t);
  el._t = setTimeout(() => { el.style.opacity = '0'; }, 2600);
}

let pointer = { x: 200, y: 200 };
addEventListener('pointermove', e => { pointer = { x: e.clientX, y: e.clientY }; });

/// A short list of things the player could have meant. Used wherever a
/// command needs one more fact than a click carries -- which item a wire is
/// about, what a depot ships.
export function menu(title, options) {
  document.querySelectorAll('.pop').forEach(e => e.remove());
  if (!options.length) return;
  const el = document.createElement('div');
  el.className = 'pop';
  el.style.cssText =
    `position:fixed;left:${pointer.x + 8}px;top:${pointer.y + 8}px;z-index:50;` +
    'background:#131B18;border:1px solid #38493F;border-radius:4px;padding:6px;' +
    'min-width:150px;box-shadow:0 8px 24px rgba(0,0,0,.5)';
  const h = document.createElement('div');
  h.textContent = title;
  h.style.cssText = 'font:10px var(--ui);letter-spacing:.12em;text-transform:uppercase;color:#7D9089;margin:2px 4px 6px';
  el.appendChild(h);
  for (const o of options) {
    const b = document.createElement('button');
    b.textContent = o.label;
    b.style.cssText = 'display:block;width:100%;text-align:left;margin:2px 0';
    b.onclick = () => { el.remove(); o.pick(); };
    el.appendChild(b);
  }
  document.body.appendChild(el);
  setTimeout(() => addEventListener('pointerdown', function off(e) {
    if (!el.contains(e.target)) { el.remove(); removeEventListener('pointerdown', off); }
  }), 0);
}

// -------------------------------------------------------------- the palette

export function renderPalette(cat, onPick) {
  const box = $('palette');
  box.innerHTML = '';
  const order = ['source', 'storage', 'machine', 'sink'];
  for (const role of order) {
    for (const p of cat.protos.filter(p => p.role === role)) {
      const b = document.createElement('button');
      b.dataset.proto = p.tag;
      b.title = p.blurb;
      b.innerHTML =
        `<span class="swatch" style="background:var(--${role})"></span>` +
        `<span>${p.title}</span><span class="n">${p.w}&times;${p.h}</span>`;
      b.onclick = () => onPick(p.tag);
      box.appendChild(b);
    }
  }
}

export function markTool(mode, proto) {
  document.querySelectorAll('#palette button').forEach(b =>
    b.classList.toggle('on', mode === 'place' && b.dataset.proto === proto));
  document.querySelectorAll('.tools button').forEach(b =>
    b.classList.toggle('on', b.dataset.mode === mode));
  const hint = {
    place: 'click the plot &middot; <b>R</b> turns it &middot; <b>Esc</b> puts it down',
    connect: 'click a bay, then a machine (or the other way round)',
    belt: 'click the bay it leaves, then the bay it arrives at',
    rail: 'click the bay it leaves, then the bay it arrives at',
    delete: 'click anything. It leaves a ghost you can restore',
    pick: 'pick something, then click the plot &middot; <b>R</b> turns it &middot; <b>Esc</b> puts it down',
  };
  $('tool').innerHTML = hint[mode] || hint.pick;
}

// --------------------------------------------------------------- the goal

export function renderGoal(v) {
  const g = v.goal, p = g.progress;
  $('goaltitle').textContent = g.title;
  $('goalbrief').textContent = g.brief;
  $('goalnote').textContent = g.note;
  const box = $('goallines');
  box.innerHTML = '';
  for (const l of p.lines) {
    const at_most = l.unit.includes('most');
    const k = l.need > 0 ? Math.min(1.6, l.have / l.need) : 0;
    const d = document.createElement('div');
    d.className = 'line' + (l.met ? ' met' : '') + (at_most && !l.met ? ' over' : '');
    d.innerHTML =
      `<div class="what"><span>${l.what}</span>` +
      `<span class="have">${net.num(l.have)} / ${net.num(l.need)} ${l.unit.replace('at most', '')}</span></div>` +
      `<div class="track"><div class="fill" style="width:${Math.min(100, k * 100).toFixed(0)}%"></div></div>`;
    box.appendChild(d);
  }
  if (p.warming) {
    const d = document.createElement('p');
    d.className = 'note';
    d.textContent = 'a rate needs a window: the room has not run long enough to have one yet.';
    box.appendChild(d);
  }
}

// ------------------------------------------------------------ who, and sync

export function renderWho(v) {
  const box = $('who');
  box.innerHTML = '';
  for (const p of v.players) {
    const d = document.createElement('div');
    d.className = 'p' + (p.behind > 120 ? ' behind' : '');
    d.title = `${p.name} joined at ${net.clock(p.joinedAt)} · ` +
      `${p.agreed} checks agreed, ${p.mismatches} mismatched, ${p.resyncs} resynchronised`;
    d.innerHTML = `<span class="dot" style="background:${p.colour}"></span>${p.name}` +
      (p.editing ? ' <span style="color:var(--signal)">&#9998;</span>' : '');
    box.appendChild(d);
  }
}

/// The experiment, in three lines.
export function renderSync(v) {
  const s = v.sync;
  const you = v.players.find(p => p.id === v.you);
  const agrees = s.agrees;
  const mark = agrees === null ? '<b style="color:var(--muted)">--</b>'
    : agrees ? '<b>agrees</b>' : '<b class="no">DIVERGED</b>';
  $('sync').innerHTML =
    `t+${s.probeSeconds.toFixed(0)}s ${mark}<br>` +
    `you  ${s.hash || '----------------'}<br>` +
    `host ${s.hostHash || '----------------'}` +
    (you ? `<br>${you.agreed} checks &middot; ${you.resyncs} resync` : '');
}

export function renderFeed(v) {
  const box = $('feed');
  box.innerHTML = '';
  for (const e of v.events) {
    const who = v.players.find(p => p.id === e.by);
    const d = document.createElement('div');
    d.innerHTML = `<span class="t">${net.clock(e.at)}</span> ` +
      `<span style="color:${who ? who.colour : 'var(--muted)'}">${who ? who.name : 'the room'}</span> ` +
      `${e.what}`;
    box.appendChild(d);
  }
}

// ------------------------------------------------------------- the ghosts

/// A deleted thing, still faintly there, with a Restore on it.
///
/// Restore is a new placement command at the tick it is pressed. The seconds
/// the thing was missing really happened, and the factory really did run
/// without it.
export function renderGhosts(v, project) {
  const box = $('ghosts');
  box.innerHTML = '';
  for (const g of v.ghosts) {
    const [x, y, w, h] = project(g.x, g.y, g.w, g.h);
    const d = document.createElement('div');
    d.className = 'g';
    d.style.cssText = `left:${x}px;top:${y}px;width:${w}px;height:${h}px`;
    const who = v.players.find(p => p.id === g.by);
    d.innerHTML = `<div>${g.title}<br><span style="opacity:.7">${who ? who.name : ''} &middot; ${g.fades.toFixed(0)}s</span></div>`;
    const b = document.createElement('button');
    b.textContent = 'restore';
    b.onclick = () => {
      const payload = { proto: g.proto, x: g.x, y: g.y, face: g.face, item: g.item };
      net.send(g.proto === 'bay' || g.proto === 'yard' ? 'PlaceStorage' : 'PlaceMachine', payload);
    };
    d.appendChild(b);
    box.appendChild(d);
  }
}

// ---------------------------------------------------------- the inspector

/// Why a thing is doing what it is doing, in seconds, in the words the
/// simulator already had for it.
export function renderInspector(id, hooks) {
  const box = $('inspect');
  const v = net.state.view;
  if (!v) return;
  const wire = net.wireOf(id);
  if (wire) return inspectWire(box, wire, hooks);
  const i = net.byId(id);
  const haul = !i && (v.world.hauls || []).find(h => h.id === id);
  if (!i && !haul) {
    box.innerHTML = '<p class="muted">click anything.</p>';
    return;
  }
  if (haul) return inspectHaul(box, haul, hooks);

  const p = net.plantOf(i.name);
  const why = p && p.why;
  const state = why ? why.state : (i.running ? 'running' : 'not commissioned');
  const cls = state === 'running' ? '' : (state === 'blocked' ? ' warn' : ' bad');
  let html =
    `<div class="name">${i.name}</div>` +
    `<div class="sub">${i.title}${i.item ? ' &middot; ' + i.item : ''} &middot; ${i.w}&times;${i.h} tiles at ${i.x},${i.y}</div>` +
    `<span class="state${cls}">${state}</span>`;
  if (i.idle) html += `<p class="muted">${i.idle}</p>`;
  if (why) {
    html += `<p>${why.headline}</p><dl>` +
      `<dt>utilisation</dt><dd>${(why.utilisation * 100).toFixed(0)}%</dd>` +
      `<dt>busy / idle / blocked</dt><dd>${why.busy} / ${why.idle} / ${why.blockedCount}</dd>`;
    if (why.nextDeliveryIn !== null && why.nextDeliveryIn !== undefined) {
      html += `<dt>next delivery</dt><dd>${(why.nextDeliveryIn / 60).toFixed(1)}s` +
        (why.nextDeliveryBy ? ` (${why.nextDeliveryBy})` : '') + '</dd>';
    }
    html += '</dl>';
    if (why.needs && why.needs.length) {
      html += '<h2>needs</h2>';
      for (const n of why.needs) {
        html += `<div class="row${n.short ? ' short' : ''}"><span>${n.item} &times;${net.num(n.perCycle)}</span>` +
          `<span>${net.num(n.available)} in ${n.bay}</span></div>`;
      }
    }
    if (why.holding && why.holding.length) {
      html += '<h2>holding</h2>';
      for (const n of why.holding) {
        html += `<div class="row"><span>${n.item}</span><span>${n.bay} ${n.full}% full</span></div>`;
      }
    }
  }
  if (p && p.held) {
    html += '<h2>contents</h2>';
    for (const q of p.held) {
      html += `<div class="row"><span>${q.item}</span><span>${net.num(q.qty)}</span></div>`;
    }
    html += `<div class="row muted"><span>capacity</span><span>${net.num(p.used)} / ${net.num(p.capacity)}</span></div>`;
  }
  if (i.macro) {
    const m = i.macro;
    const line = v => v.map(a => `${net.num(a.qty)} ${a.item}`).join(', ') || 'nothing';
    html += '<h2>the machine inside</h2><dl>' +
      `<dt>cycle</dt><dd>${m.cycleSeconds.toFixed(1)}s</dd>` +
      `<dt>takes</dt><dd>${line(m.takes)}</dd>` +
      `<dt>gives</dt><dd>${line(m.gives)}</dd>` +
      `<dt>components</dt><dd>${m.components}</dd>` +
      `<dt>orbit</dt><dd>${m.settled ? 'exact' : 'unsettled'}</dd>` +
      '</dl>';
    if (m.cycleSeconds > 30) {
      html += `<p class="muted">a ${m.cycleSeconds.toFixed(0)}-second orbit runs in enormous batches. ` +
        `It will need a bay that can hold one.</p>`;
    }
  }
  html += '<div class="acts">';
  if (i.role === 'machine') {
    html += '<button data-act="open">open the machine</button>' +
      '<button data-act="duplicate" title="a copy of this design, which then goes its own way">duplicate</button>';
  }
  html += '<button data-act="delete">delete</button></div>';
  box.innerHTML = html;
  box.querySelectorAll('[data-act]').forEach(b => {
    b.onclick = () => hooks[b.dataset.act] && hooks[b.dataset.act](i);
  });
}

/// One wire, and the button that takes it back.
///
/// A wire is the one thing a player can draw that has no building to click,
/// so without this panel a mis-wired bay could only be fixed by deleting the
/// bay.
function inspectWire(box, wire, hooks) {
  const a = net.byId(wire.from), b = net.byId(wire.to);
  if (!a || !b) {
    box.innerHTML = '<p class="muted">click anything.</p>';
    return;
  }
  box.innerHTML =
    `<div class="name">${a.name} &rarr; ${b.name}</div>` +
    `<div class="sub">a wire &middot; ${wire.item}</div>` +
    `<p class="muted">${a.role === 'storage'
      ? `${b.name} draws its ${wire.item} from this bay.`
      : `${a.name} posts its ${wire.item} into this bay.`}</p>` +
    '<div class="acts"><button data-act="unwire">delete this wire</button></div>';
  box.querySelectorAll('[data-act]').forEach(btn => {
    btn.onclick = () => hooks.unwire && hooks.unwire(wire);
  });
}

function inspectHaul(box, h, hooks) {
  const p = net.plantOf(h.name);
  const g = h.geometry || {};
  // A transport departs with a full load and not before, so a rail with a
  // 5,000 batch sitting on a bay that fills at 48 a second looks broken for
  // its first hundred seconds. Saying what it is waiting for is the
  // difference between a slow link and a dead one.
  const from = net.byId(h.from);
  const bay = from && net.plantOf(from.name);
  const level = bay && (bay.held || []).find(q => q.item === h.item);
  const short = g.load && (!level || level.qty < g.load);
  box.innerHTML =
    `<div class="name">${h.name}</div>` +
    `<div class="sub">${h.title} &middot; ${h.item}</div>` +
    (h.idle ? `<p class="muted">${h.idle}</p>` : '') +
    '<dl>' +
    `<dt>each way</dt><dd>${(g.seconds || 0).toFixed(1)}s</dd>` +
    `<dt>load</dt><dd>${net.num(g.load)} &times; ${net.num(g.vehicles)}</dd>` +
    (p ? `<dt>throughput</dt><dd>${(p.rate * 60).toFixed(1)}/s</dd>` +
         `<dt>waiting</dt><dd>${p.waitingToLoad} to load, ${p.waitingToUnload} to unload</dd>` : '') +
    '</dl>' +
    (short
      ? `<p class="muted">It leaves with a full ${net.num(g.load)} and not before. ` +
        `${from.name} holds ${net.num(level ? level.qty : 0)}.</p>`
      : '') +
    '<div class="acts"><button data-act="unlink">delete</button></div>';
  box.querySelectorAll('[data-act]').forEach(btn => {
    btn.onclick = () => hooks.unlink && hooks.unlink(h);
  });
}
