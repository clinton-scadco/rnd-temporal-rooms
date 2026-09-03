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

/// What may be placed.
///
/// One button per thing, and anything designed arrives empty: note 7 of the
/// play session was the shortest one in it -- prebuilt machines take the fun
/// out of the game entirely. A machine here is a *chassis*, and what goes
/// inside it is yours.
///
/// There was briefly a second button beside each one that placed the
/// catalogue's worked answer. It went, because a build menu with two of every
/// building in it is a build menu nobody can read, and because a worked
/// example belongs where you are designing rather than where you are choosing
/// what to put down.
export function renderPalette(cat, onPick) {
  const box = $('palette');
  box.innerHTML = '';
  const order = ['source', 'storage', 'machine', 'sink'];
  for (const role of order) {
    for (const p of cat.protos.filter(p => p.role === role)) {
      const b = document.createElement('button');
      b.dataset.proto = p.tag;
      b.title = p.designed
        ? `${p.blurb}\n\nArrives empty. Open it to design what goes inside.`
        : p.blurb;
      b.innerHTML =
        `<span class="swatch" style="background:var(--${role})"></span>` +
        `<span>${p.title}</span>` +
        `<span class="n">${p.needsGround ? 'on ground' : `${p.w}\u00d7${p.h}`}</span>`;
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
    connect: 'click what it comes out of, then what it goes into &middot; ' +
      'no bay needed between two machines',
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

  // Finished, and whether it is still true.
  //
  // These are two different questions and the panel used to have room for one
  // of them. The play session finished a room, unplugged one of the power
  // stations that had finished it, watched the number fall, and could not tell
  // whether anything was wrong -- because the objective panel answers "have
  // you" and they were asking "are you". Both now, and the second one loudly.
  if (p.doneAt !== null && p.doneAt !== undefined) {
    const d = document.createElement('div');
    d.className = 'verdict' + (p.holding ? '' : ' slipped');
    d.innerHTML = `<b>room completed</b><span>at ${net.clock(p.doneAt)}</span>` +
      (p.holding ? '' :
        '<em>the factory is no longer doing it. The room stays passed; ' +
        `what has stopped is: ${p.slipped.join('; ')}.</em>`);
    box.appendChild(d);
  }

  for (const l of p.lines) {
    const at_most = l.unit.includes('most');
    const k = l.need > 0 ? Math.min(1.6, l.have / l.need) : 0;
    const d = document.createElement('div');
    d.className = 'line' + (l.met ? ' met' : '') + (at_most && !l.met ? ' over' : '');
    // An achievement that is met is done with; a live requirement that is met
    // is only met *at the moment*, and saying so is the whole point of the
    // distinction.
    const mark = l.kind === 'state'
      ? `<span class="kind" title="a fact about the factory right now, which can stop being true">now</span>`
      : `<span class="kind" title="a total that only grows: once reached, reached">total</span>`;
    d.innerHTML =
      `<div class="what"><span>${mark}${l.what}</span>` +
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
    // Dimmed for somebody who has stopped *watching*, not somebody whose
    // replica is behind: with the room beating on its own, every replica is at
    // the current tick whether its browser is there or not, so `behind` no
    // longer says anything about whether a person is. `away` does.
    d.className = 'p' + (p.away > 180 ? ' behind' : '');
    d.title = `${p.name} joined at ${net.clock(p.joinedAt)} · ` +
      `${p.agreed} checks agreed, ${p.mismatches} mismatched, ${p.resyncs} resynchronised` +
      (p.away > 180 ? ` · last looked ${p.awaySeconds.toFixed(0)}s ago` : '');
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

/// How current the picture is, which is a different question from whether it
/// is right.
///
/// `renderSync` above answers "does this browser's reconstruction of the room
/// agree with the host's?" -- the experiment. This answers "is this browser
/// being told about it?", and it only has anything to say when the answer is
/// no. A live connection writes nothing at all: a healthy game does not need a
/// green light, and a stale one is the only case anybody has ever needed to be
/// told about.
export function renderLink(h) {
  const box = $('link');
  if (!box) return;
  const secs = (h.lag / 1000).toFixed(0);
  if (h.misses > 2) {
    box.className = 'link out';
    box.textContent = `no answer · ${secs}s`;
  } else if (!h.live) {
    box.className = 'link slow';
    box.textContent = `catching up · ${secs}s`;
  } else {
    box.className = 'link';
    box.textContent = '';
  }
  box.title = h.live
    ? `current, ${h.rtt} ms round trip`
    : `the room is still running; this screen last heard from it ${secs}s ago`;
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
    // The wiring comes back too, and the button says so before it is pressed.
    b.textContent = g.conns ? `restore +${g.conns}` : 'restore';
    b.title = g.conns
      ? `puts ${g.title} back, and reconnects the ${g.conns} connection` +
        `${g.conns === 1 ? '' : 's'} it had. Any that no longer fit are named in the feed.`
      : `puts ${g.title} back`;
    // The command is the room's, not this browser's: assembling it here is how
    // a restored machine used to come back with the catalogue's design instead
    // of the one somebody had built.
    b.onclick = () => net.send(g.restore.type, g.restore.payload);
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
  }
  html += flows(i, why);
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
  html += connectBlock(i);
  html += '<div class="acts">';
  // Anything designed can be opened, including something with nothing in it
  // yet -- which is how an empty chassis stops being empty. `designed` comes
  // from the room, so the panel does not have to know which roles those are.
  if (i.designed) {
    html += `<button data-act="open">${i.macro ? 'open the machine' : 'design it'}</button>`;
    if (i.macro) {
      html += '<button data-act="duplicate" ' +
        'title="a copy of this design, which then goes its own way">duplicate</button>';
    }
  }
  html += '<button data-act="delete">delete</button></div>';
  box.innerHTML = html;
  box.querySelectorAll('[data-act]').forEach(b => {
    b.onclick = () => hooks[b.dataset.act] && hooks[b.dataset.act](i, b.dataset.item);
  });
}

/// What is coming in and what is going out, one row per port.
///
/// The play session's note 3, and the reason it was worth doing before
/// anything prettier: every building could say what it *was* and none of them
/// could say what they were getting. The old panel listed the needs the
/// simulator happened to be blocked on, which is a different set from "the
/// things this machine consumes" -- an input nobody had wired at all did not
/// appear, because a machine with an unwired input is not commissioned and has
/// no opinion about anything.
///
/// So the spine is the ports, which exist whether or not anybody has connected
/// them, and the simulator's numbers are hung off them where there are any.
function flows(i, why) {
  const ports = i.ports || [];
  if (!ports.length) return '';
  const v = net.state.view;
  const need = {};
  for (const n of (why && why.needs) || []) need[n.item] = n;
  const holding = {};
  for (const n of (why && why.holding) || []) holding[n.item] = n;

  // Who is on the other end of each item, and how fast they can push or pull.
  const from = {}, to = {};
  for (const c of v.world.conns) {
    if (c.to === i.id) from[c.item] = c;
    if (c.from === i.id) to[c.item] = c;
  }
  for (const h of v.world.hauls) {
    if (h.to === i.id) from[h.item] = h;
    if (h.from === i.id) to[h.item] = h;
  }

  const rateOut = (id, item) => {
    const o = net.byId(id);
    const port = o && (o.ports || []).find(p => p.out && p.item === item);
    return port ? port.perSecond : null;
  };
  // What is sitting in whatever feeds or drains this port right now.
  const stock = c => {
    if (!c) return null;
    const name = c.buffer || (net.byId(c.from === i.id ? c.to : c.from) || {}).name;
    const held = name && net.plantOf(name);
    const q = held && held.held && held.held.find(h => h.item === c.item);
    return q ? q.qty : (held ? 0 : null);
  };

  const seen = new Set();
  const row = pt => {
    const key = (pt.out ? 'o' : 'i') + pt.item;
    if (seen.has(key)) return '';
    seen.add(key);
    const c = pt.out ? to[pt.item] : from[pt.item];
    const other = c ? net.byId(pt.out ? c.to : c.from) : null;
    const have = stock(c);
    const n = need[pt.item];
    const short = !pt.out && (!c || (n && n.short));
    const incoming = !pt.out && c ? rateOut(c.from, pt.item) : null;
    let right;
    if (!c) right = 'not connected';
    else if (pt.out) right = `${other ? other.name : '?'}${have === null ? '' : ` &middot; ${net.num(have)} waiting`}`;
    else right = `${have === null ? '--' : net.num(have)} available` +
      (incoming ? ` &middot; ${net.num(incoming)}/s in` : '');
    return `<div class="row${short ? ' short' : ''}">` +
      `<span><i class="pip" style="--d:var(--${pt.domain})"></i>` +
      `${pt.title || pt.item} <em>${net.num(pt.perSecond)}${pt.domain === 'electrical' ? ' MW' : '/s'}</em></span>` +
      `<span>${right}</span></div>`;
  };

  const ins = ports.filter(pt => !pt.out).map(row).join('');
  const outs = ports.filter(pt => pt.out).map(row).join('');
  let html = '';
  if (ins) html += `<h2>taking</h2>${ins}`;
  if (outs) html += `<h2>giving</h2>${outs}`;
  // The one line the old panel had that this would otherwise lose: a machine
  // whose output has nowhere to go is blocked, and it should say so here.
  for (const k in holding) {
    html += `<div class="row"><span>backed up</span><span>${holding[k].bay} ${holding[k].full}% full</span></div>`;
  }
  return html;
}

/// The connection points this building has, and which of them are joined to
/// anything.
///
/// The play session asked for this twice over: note 6 wanted the wire tools
/// out of a scrolling side menu and onto the thing being wired, and note 3
/// wanted every building to say what it takes and gives. They are the same
/// panel. Clicking a port starts a connection from it, which means the second
/// click is a destination and there is no menu at all -- note 10's "never ask
/// a question whose answer is already determined", answered at the source.
///
/// A filled dot is connected; a hollow one is not. A machine with a hollow
/// input is the machine that is not running, and this is where you see it
/// without reading a paragraph.
function connectBlock(i) {
  const v = net.state.view;
  const ports = i.ports || [];
  if (!ports.length) {
    return i.role === 'storage'
      ? '<h2>connect</h2><p class="muted">nothing has been wired into this bay yet, ' +
        'so it does not know what it holds. Wire something in, or draw from it ' +
        'to a machine that needs one thing.</p>'
      : '';
  }
  const wired = new Set();
  const partner = {};
  const note = (key, name) => { wired.add(key); (partner[key] ||= []).push(name); };
  for (const c of v.world.conns) {
    if (c.from === i.id) note('out:' + c.item, nameOf(c.to));
    if (c.to === i.id) note('in:' + c.item, nameOf(c.from));
  }
  for (const h of v.world.hauls) {
    if (h.from === i.id) note('out:' + h.item, nameOf(h.to));
    if (h.to === i.id) note('in:' + h.item, nameOf(h.from));
  }
  const seen = new Set();
  const row = p => {
    const key = (p.out ? 'out:' : 'in:') + p.item;
    if (seen.has(key)) return '';
    seen.add(key);
    const on = wired.has(key);
    const to = (partner[key] || []).join(', ');
    const rate = p.perSecond ? `${net.num(p.perSecond)}/s` : '';
    return `<button class="port${on ? ' on' : ''}" data-act="connect" ` +
      `data-item="${p.item}" title="${p.out ? 'out' : 'in'} &middot; ${p.domain}` +
      `${on ? ' &middot; ' + to : ' &middot; not connected'}">` +
      `<span class="dot" style="--d:var(--${p.domain})"></span>` +
      `<span class="what">${p.title || p.item}</span>` +
      `<span class="n">${on ? to : rate || '&mdash;'}</span></button>`;
  };
  const ins = ports.filter(p => !p.out).map(row).join('');
  const outs = ports.filter(p => p.out).map(row).join('');
  let html = '<h2>connect</h2>';
  if (ins) html += `<div class="ports"><span class="lbl">in</span>${ins}</div>`;
  if (outs) html += `<div class="ports"><span class="lbl">out</span>${outs}</div>`;
  return html;
}

const nameOf = id => (net.byId(id) || {}).name || `#${id}`;

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
  // The document's own line, which carries the domain and -- when the two
  // ends are machines -- the buffer the compiler put between them.
  const c = (net.state.view.world.conns || []).find(
    c => c.from === wire.from && c.to === wire.to && c.item === wire.item
  ) || wire;
  const buf = c.buffer && net.plantOf(c.buffer);
  const held = buf && buf.held && buf.held.length ? buf.held[0].qty : null;
  box.innerHTML =
    `<div class="name">${a.name} &rarr; ${b.name}</div>` +
    `<div class="sub">${c.domain || 'material'} &middot; ${c.title || wire.item}</div>` +
    `<p class="muted">${
      a.role === 'storage' ? `${b.name} draws its ${c.title || wire.item} from this bay.`
      : b.role === 'storage' ? `${a.name} posts its ${c.title || wire.item} into this bay.`
      // The whole of experiment 13's first change, said in one sentence where
      // the player is looking at the thing it changed.
      : `${a.name} feeds ${b.name} directly. No bay needed: the buffer between ` +
        `them is part of the connection.`}</p>` +
    (c.buffer
      ? '<dl>' +
        `<dt>buffered</dt><dd>${held === null ? '--' : net.num(held)} of ${net.num(c.capacity)}</dd>` +
        '</dl>'
      : '') +
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
