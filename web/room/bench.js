// The machine, from the inside: experiment 10's window, driven by prototype
// 2's rules.
//
// Two things are different from the designer that window was built for, and
// both of them are the brief:
//
//   nothing is dragged      a component is placed or deleted, never moved
//   nothing is live         edits go to a *draft*; the machine keeps running
//
// The second is the one that matters. Opening a placed machine does not stop
// it: the plant outside goes on consuming, producing and backing up while the
// draft is edited, and the design that is running does not change until
// somebody presses commit -- at which point it changes at one canonical tick,
// as one command, on every client at once.

import * as net from './net.js';
import * as plant from '../machine/form.js';
import { menu, toast } from './panels.js';

const $ = id => document.getElementById(id);

export const bench = {
  id: null,
  level: 0,
  face: null,
  holding: null,
  sel: null,
  draft: false,
  // Experiment 14: the wire, as a mode rather than as a menu. `wiring` is the
  // tool being on; `wire` is the component the material would leave from,
  // once one has been clicked.
  wiring: false,
  wire: null,
};

let ready = false;
let design = null;      // the document in the window, live or draft
let state = null;       // what every component in it is doing, and why
let cursor = null;      // where the pointer is: { hit, tile, at }
let onLeave = () => {};

export async function init(hooks) {
  onLeave = hooks.onLeave || (() => {});
  await net.parts();
  plant.authoring({
    // Place-and-delete: there is no onMove, so form.js cannot slide anything.
    onPick: name => picked(name),
    onGround: (x, y) => put(x, y),
    onHover: h => { cursor = h; paint(); },
    placing: () => !!bench.holding,
    onCancel: () => cancel(),
    level: () => bench.level,
    tile: at => ({ x: Math.round(at[0] / 2), y: Math.round(at[2] / 2) }),
    onTurn: d => { bench.face = (((bench.face ?? 0) + d) % 4 + 4) % 4; hint(); paint(); },
    onLift: d => { bench.level = Math.max(0, bench.level + d); level(); },
  });
  ready = await plant.initForm($('plant3d'));
  palette();
  $('benchedit').onclick = () => draft();
  $('benchcommit').onclick = commit;
  $('benchdiscard').onclick = async () => {
    const res = await net.send('CloseDesign', { id: bench.id, keep: false });
    if (res.ok) { bench.sel = null; cancel(); await refresh(false); }
  };
  $('benchclose').onclick = () => onLeave();
  $('benchup').onclick = () => { bench.level++; level(); };
  $('benchdown').onclick = () => { bench.level = Math.max(0, bench.level - 1); level(); };
  $('family').onchange = palette;
  if ($('benchwire')) $('benchwire').onclick = () => wiring(!bench.wiring);
  // The two keys the bar advertises, from anywhere in the window rather than
  // only from a canvas that happens to have the focus -- a player who has just
  // pressed a palette button has the focus on the palette button, and `R turns
  // it` is a lie if the plant has to be clicked first for it to be true.
  //
  // `defaultPrevented` is the handover: form.js takes these keys when the
  // pointer is in the plant, and says so, and this does not take them twice.
  addEventListener('keydown', e => {
    if (e.defaultPrevented || !bench.id || $('bench').hidden) return;
    const tag = (e.target && e.target.tagName) || '';
    if (tag === 'INPUT' || tag === 'SELECT' || tag === 'TEXTAREA') return;
    const k = String(e.key || '').toLowerCase();
    if (k === 'escape') cancel();
    else if (k === 'r' && bench.holding) {
      bench.face = (((bench.face ?? 0) + (e.shiftKey ? -1 : 1)) % 4 + 4) % 4;
      hint();
      paint();
    } else return;
    e.preventDefault();
  });
}

export async function open(id) {
  bench.id = id;
  bench.sel = null;
  cancel();
  await refresh(true);
}

function level() {
  $('benchlevel').textContent = bench.level;
  hint();   // the storey is in the sentence, so the sentence moves with it
  paint();
}

/// Put down whatever is being held, and stop drawing whatever wire was being
/// drawn. One key for both, because from the player's side they are one state:
/// the pointer is busy.
function cancel() {
  bench.holding = null;
  bench.wiring = false;
  bench.wire = null;
  document.querySelectorAll('#parts button.on').forEach(b => b.classList.remove('on'));
  hint();
  paint();
}

/// Rebuild the window from whatever the room says this machine is now.
///
/// Called on every frame the room reports a change, which is what makes two
/// players in the same machine see the same draft: the second one is not
/// watching the first one's mouse, they are both watching the document.
export async function refresh(refit) {
  if (!bench.id || !ready) return;
  const i = net.byId(bench.id);
  if (!i) return onLeave();
  const mine = i.editor === net.state.player;
  bench.draft = i.hasDraft && mine;
  const res = await net.form(bench.id, i.hasDraft);
  if (!res.ok) return toast(res.error);
  design = res.design;
  plant.show(res, refit);
  // A rebuild renumbers the owners the renderer highlights by, so the
  // selection is re-stated against the plant that just arrived rather than
  // left pointing at whatever now holds its old index.
  plant.pick(design && design.units.some(u => u.name === bench.sel) ? bench.sel : null);
  state = await net.inside(bench.id, i.hasDraft);
  $('benchname').textContent = i.name + ' · ' + i.title + (i.hasDraft ? ' · draft' : '');
  $('benchedit').disabled = !!i.editor;
  $('benchcommit').disabled = !mine;
  $('benchdiscard').disabled = !mine;
  $('benchlock').textContent = i.editor === null || i.editor === undefined
    ? ''
    : (mine ? 'you are editing this' : editorName(i.editor) + ' is editing this');
  if ($('benchedit')) $('benchedit').textContent = bench.draft ? 'draft open' : 'open a draft';
  info(i);
  // The overlay is drawn from the document, so a document that has just
  // changed redraws it: a wire that has just been made appears as a line
  // between the two things it joined.
  paint();
}

function editorName(id) {
  const p = net.state.view.players.find(p => p.id === id);
  return p ? p.name : `player ${id}`;
}

// --------------------------------------------------------------- the palette

/// The first entry of the family filter, and its default: the whole
/// vocabulary, under one heading per family.
///
/// It used to default to whichever family sorted first, which is `control` --
/// so opening an empty chassis showed a valve and a clutch and nothing else.
/// That reads exactly like a bug, and for the machine experiment 13 added it
/// *was* one: a player who puts an extraction head on a seam and opens it to
/// draw a mining head is looking for an inlet and an outlet, and the two
/// things on screen were a valve and a clutch. Nothing may be a dropdown away
/// when it is the only way to make the game's first machine.
const EVERYTHING = 'everything';

function palette() {
  const cat = net.state.parts;
  if (!cat) return;
  const fam = $('family');
  const fams = [...new Set(cat.parts.map(p => p.family))].sort();
  if (!fam.options.length) {
    fam.innerHTML = [EVERYTHING, ...fams].map(f => `<option>${f}</option>`).join('');
    fam.value = EVERYTHING;
  }
  const box = $('parts');
  box.innerHTML = '';
  const showing = fam.value === EVERYTHING ? fams : [fam.value];
  for (const f of showing) {
    const parts = cat.parts.filter(p => p.family === f);
    if (!parts.length) continue;
    // A heading per family, so the whole vocabulary being on screen at once
    // is still readable rather than thirty-eight buttons in a column.
    if (showing.length > 1) {
      const h = document.createElement('h3');
      h.className = 'fam';
      h.textContent = f;
      box.appendChild(h);
    }
    for (const p of parts) {
      const b = document.createElement('button');
      b.title = p.locked
        ? p.blurb + '\n\nnot unlocked yet -- ' + (p.opens || 'it is not available here')
        : p.blurb;
      b.dataset.kind = p.kind;
      // A locked component is shown rather than hidden, the same way a locked
      // prototype is in the room's palette: a progression nobody can look
      // forward to is a progression nobody notices.
      if (p.locked) b.classList.add('locked');
      b.innerHTML = `<span>${p.title}</span><span class="n">${p.w}&times;${p.h}</span>`;
      b.onclick = () => {
        if (p.locked) return toast(`${p.title} has not been unlocked yet`);
        bench.holding = bench.holding === p.kind ? null : p.kind;
        bench.face = null;
        // Holding a component and drawing a wire are two pointers, and there
        // is one pointer.
        if (bench.holding) { bench.wiring = false; bench.wire = null; }
        hint();
        paint();
        [...box.children].forEach(c => c.classList.toggle('on', c === b && bench.holding));
      };
      box.appendChild(b);
    }
  }
}

const partOf = kind => (net.state.parts ? net.state.parts.parts.find(p => p.kind === kind) : null);

/// What the pointer is for, in the bar, in a sentence.
///
/// This used to be a toast -- the same red box a refusal comes back in, at the
/// bottom of the screen, for two and a half seconds. So the one line telling
/// you how to place the thing you had just picked up looked like an error and
/// was gone before you had moved the mouse.
function hint() {
  const el = $('benchhint');
  $('benchinfo').dataset.hint = bench.holding || '';
  if (!el) return;
  const p = partOf(bench.holding);
  if (p) {
    el.textContent = `${p.title} — click the floor at level ${bench.level} · R turns it · Esc puts it down`;
  } else if (bench.wiring && bench.wire) {
    el.textContent = `wiring from ${bench.wire} — click what it should feed · Esc cancels`;
  } else if (bench.wiring) {
    el.textContent = 'wiring — click the component the material leaves from';
  } else {
    el.textContent = '';
  }
  el.classList.toggle('on', !!(p || bench.wiring));
  if ($('benchwire')) $('benchwire').classList.toggle('on', bench.wiring);
}

// ------------------------------------------------------------------ the draft
//
// A draft is bookkeeping, not a decision. It exists so that the plant outside
// can keep running while the design is edited, and the player's half of it is
// `commit` -- the moment the running machine changes. Making them *open* one
// first put a modal step in front of every edit whose only content was the
// word yes, and the failure mode was silent: pick a component, click the
// floor, and nothing happens but a red box saying `open a draft first`.
//
// So the first edit takes the draft, and commit and discard stay exactly where
// they were.

/// The draft this player may edit, taken now if there is not one already.
async function draft() {
  if (bench.draft) return true;
  const i = net.byId(bench.id);
  if (!i) return false;
  if (i.editor !== null && i.editor !== undefined && i.editor !== net.state.player) {
    toast(`${editorName(i.editor)} is editing this`);
    return false;
  }
  const res = await net.send('OpenDesign', { id: bench.id });
  if (!res.ok) return false;   // `net` has already said why, in the same place everything else does
  // The next poll is up to 180 ms away and the edit that asked for this draft
  // is about to be sent. Patching the frame this window reads keeps the two in
  // step; the poll then confirms it, from the host, like everything else.
  i.hasDraft = true;
  i.editor = net.state.player;
  bench.draft = true;
  await refresh(false);
  return true;
}

/// One component, placed at the tile under the pointer.
async function put(x, y) {
  if (!bench.holding) return;
  if (!(await draft())) return;
  const res = await net.send('PlaceComponent', {
    id: bench.id, kind: bench.holding, x, y, z: bench.level, face: bench.face,
  });
  if (res.ok) await refresh(false);
}

async function commit() {
  const i = net.byId(bench.id);
  if (!i || !i.hasDraft) return toast('there is nothing to commit');
  const res = await net.form(bench.id, true);
  if (!res.ok) return toast(res.error);
  const out = await net.send('CommitMachineDesign', { id: bench.id, design: res.design });
  if (out.ok) {
    toast('committed: the machine changes at one tick, everywhere');
    cancel();
    await refresh(false);
  }
}

// -------------------------------------------------------------- the panels

function info(i) {
  const box = $('benchinfo');
  const m = i.macro || {};
  const sel = bench.sel && design ? design.units.find(u => u.name === bench.sel) : null;
  const live = state && state.ok ? state.units.find(u => u.name === bench.sel) : null;
  let html = '';
  // An empty chassis is the one thing on screen that says nothing about
  // itself, and since experiment 13 it is also the *first* thing anybody
  // opens: a room comes with ground rather than with a working pit. So it
  // says what it is short of.
  if (design && !design.units.length) {
    html += '<div class="muted" style="font-size:11px;margin-bottom:8px">' +
      (i.role === 'source'
        ? 'Nothing in here yet. A head that works the ground under it is an '
          + '<b>inlet</b> (or a <b>pump</b>, for water) set to draw what is down there, '
          + 'wired to an <b>outlet</b> for it to leave by.'
        : 'Nothing in here yet. Pick a component on the left and click the floor; '
          + 'then <b>wire</b> them together.')
      + '<br><span class="muted">Placing anything takes a draft on its own. '
      + 'The machine outside keeps running until you commit.</span></div>';
  }
  html +=
    `<div class="row"><span>components</span><span>${design ? design.units.length : 0}</span></div>` +
    `<div class="row"><span>connections</span><span>${design ? design.wires.length : 0}</span></div>` +
    `<div class="row"><span>cycle</span><span>${(m.cycleSeconds || 0).toFixed(1)}s</span></div>` +
    `<div class="row"><span>footprint</span><span>${m.designWidth}&times;${m.designHeight}</span></div>`;
  // Section 21, at the inner altitude: what is stopping this machine, in the
  // machine's own words. The outer inspector says whether the world is
  // feeding it; this says whether it could use more if it were.
  if (state && state.ok && state.holding && state.holding.length) {
    html += '<h2>holding it back</h2>';
    for (const h of state.holding.slice(0, 6)) {
      html += `<div class="row"><span>${h.name}</span>` +
        `<span style="color:var(--signal)">${h.status}</span></div>` +
        `<div class="muted" style="font-size:10px;margin:-2px 0 4px">${h.why || ''}</div>`;
    }
  }
  if (sel) {
    const p = net.state.parts.parts.find(p => p.kind === sel.kind);
    html += `<h2>${sel.name}</h2><div class="row"><span>${p ? p.title : sel.kind}</span>` +
      `<span>${sel.x},${sel.y},${sel.z || 0}</span></div>`;
    if (live) {
      html += `<div class="row"><span>doing</span>` +
        `<span style="color:var(--${live.status === 'running' ? 'good' : 'signal'})">${live.status}</span></div>` +
        `<div class="row"><span>utilisation</span><span>${(live.util || 0).toFixed(0)}%</span></div>`;
      for (const q of live.ports) {
        html += `<div class="row"><span>${q.dir === 'in' ? '&larr;' : '&rarr;'} ${q.name}</span>` +
          `<span style="color:var(--${q.type})">${q.level}/${q.cap}</span></div>`;
      }
      for (const l of (live.why || []).slice(0, 3)) {
        html += `<div class="muted" style="font-size:10px">${l}</div>`;
      }
    } else if (p) {
      for (const q of p.ports) {
        html += `<div class="row"><span>${q.dir === 'in' ? '&larr;' : '&rarr;'} ${q.name}</span>` +
          `<span style="color:var(--${q.type})">${q.type}</span></div>`;
      }
    }
    // What this one is already joined to, and a way to take it apart again.
    // The count at the top of this panel has always said `connections 2`; who
    // they were between was not written down anywhere on screen, and there was
    // no way at all to undo one.
    const mine = (design ? design.wires : []).filter(w => w.from === sel.name || w.to === sel.name);
    html += '<h2>connections</h2>';
    if (!mine.length) {
      html += '<div class="muted" style="font-size:10px">none yet — <b>wire</b> joins this to '
        + 'something that takes what it makes</div>';
    }
    for (const [k, w] of mine.entries()) {
      const out = w.from === sel.name;
      html += `<div class="row"><span>${out ? '&rarr;' : '&larr;'} ` +
        `${out ? w.to + '.' + w.toPort : w.from + '.' + w.fromPort}</span>` +
        `<button class="x" data-cut="${k}" title="disconnect">&times;</button></div>`;
    }
    // These used to appear only once a draft was open, which is the wrong way
    // round: they are how a player *asks* for one.
    html += '<div class="acts" style="display:flex;gap:4px;margin-top:8px">' +
      '<button id="bconnect">wire</button><button id="bdelete">delete</button></div>';
    if (p && p.tunable) html += tuner(sel, p);
  }
  box.innerHTML = html;
  if ($('bdelete')) {
    $('bdelete').onclick = async () => {
      if (!(await draft())) return;
      const res = await net.send('DeleteComponent', { id: bench.id, unit: bench.sel });
      if (res.ok) { bench.sel = null; await refresh(false); }
    };
  }
  if ($('bconnect')) $('bconnect').onclick = () => wiring(true, sel.name);
  box.querySelectorAll('[data-cut]').forEach(el => {
    const w = (design ? design.wires : []).filter(w => w.from === sel.name || w.to === sel.name)[+el.dataset.cut];
    el.onclick = async () => {
      if (!w || !(await draft())) return;
      const res = await net.send('DisconnectComponent', {
        id: bench.id, from: w.from, fromPort: w.fromPort, to: w.to, toPort: w.toPort,
      });
      if (res.ok) await refresh(false);
    };
  });
  box.querySelectorAll('[data-tune]').forEach(el => {
    el.onchange = async () => {
      if (!(await draft())) return;
      const res = await net.send('TuneComponent', {
        id: bench.id, unit: sel.name, field: el.dataset.tune, value: String(el.value),
      });
      if (res.ok) await refresh(false);
    };
  });
}

function tuner(u, p) {
  const t = u.tune || {};
  const field = (name, label, value, type = 'number') =>
    `<div class="row"><span>${label}</span>` +
    `<input data-tune="${name}" type="${type}" value="${value}" style="width:80px"></div>`;
  switch (u.kind) {
    case 'reactor': return '<h2>tune</h2>' + field('throttle', 'throttle %', t.throttle ?? 100);
    case 'gearbox': return '<h2>tune</h2>' + field('ratio', 'ratio', t.ratio ?? 4);
    case 'valve': case 'clutch': return '<h2>tune</h2>' + field('limit', 'limit', t.limit ?? 100);
    case 'column': return '<h2>tune</h2>' + field('stages', 'stages', t.stages ?? 2);
    case 'pump': case 'inlet':
      return '<h2>tune</h2><div class="row"><span>draws</span><select data-tune="subst">' +
        net.state.parts.substances.filter(s => s.source !== false).map(s =>
          `<option value="${s.tag}"${s.tag === t.subst ? ' selected' : ''}>${s.title}</option>`).join('') +
        '</select></div>';
    default: return '';
  }
}

// ------------------------------------------------------------------- the wire
//
// Connecting two components was one button, in one panel, that only appeared
// once a component was selected *and* a draft was open, and it opened a list of
// sentences like `steam → TU1.in`. Everything about that is correct and none of
// it is visible: the guide says "wire them together" and there is nothing on
// screen that looks like wiring.
//
// So it is a tool, and it is the same tool as the world view's `connect`: turn
// it on, click what makes, click what takes, and while you are between those
// two clicks a line follows the pointer and everything the rules would accept
// is boxed in blue. The menu of port pairs survives, for the case it was
// actually good at: two components that could be joined in more than one way.

/// Turn the wire tool on or off, optionally with the component it starts from.
function wiring(on, from) {
  bench.wiring = !!on;
  bench.wire = on ? (from || null) : null;
  if (on) {
    bench.holding = null;
    document.querySelectorAll('#parts button.on').forEach(b => b.classList.remove('on'));
  }
  hint();
  paint();
}

/// Every way one component could legally feed another: an out port and an in
/// port of the same domain. The same rule the host will apply to the command,
/// which is why the menu never offers a pair that is then refused.
function pairs(from, to) {
  const cat = net.state.parts ? net.state.parts.parts : [];
  const u = design && design.units.find(u => u.name === from);
  const v = design && design.units.find(u => u.name === to);
  if (!u || !v || from === to) return [];
  const a = cat.find(p => p.kind === u.kind);
  const b = cat.find(p => p.kind === v.kind);
  if (!a || !b) return [];
  const has = (f, fp, t, tp) =>
    design.wires.some(w => w.from === f && w.fromPort === fp && w.to === t && w.toPort === tp);
  const out = [];
  for (const o of a.ports.filter(q => q.dir === 'out')) {
    for (const q of b.ports.filter(q => q.dir === 'in' && q.type === o.type)) {
      if (!has(from, o.name, to, q.name)) out.push([o.name, q.name]);
    }
  }
  return out;
}

/// The second click. One way to join them is a connection; several is the
/// question the menu was always for.
async function join(from, to) {
  const opts = pairs(from, to);
  if (!opts.length) {
    toast(`${to} does not take what ${from} makes`);
    return;
  }
  const send = async ([fp, tp]) => {
    if (!(await draft())) return;
    const res = await net.send('ConnectComponent',
      { id: bench.id, from, fromPort: fp, to, toPort: tp });
    if (res.ok) await refresh(false);
  };
  wiring(false);
  if (opts.length === 1) return send(opts[0]);
  menu('connect', opts.slice(0, 14).map(o => ({
    label: `${from}.${o[0]} → ${to}.${o[1]}`,
    pick: () => send(o),
  })));
}

/// A click in the plant. In the wire tool it is one of the two ends; otherwise
/// it is a selection, the way it always was.
function picked(name) {
  if (bench.wiring) {
    if (!bench.wire) {
      bench.wire = name;
      bench.sel = name;
      plant.pick(name);
      hint();
      paint();
      refresh(false);
      return;
    }
    if (bench.wire === name) return wiring(true, null);   // clicking it again lets go
    return join(bench.wire, name);
  }
  bench.sel = name;
  plant.pick(name);
  paint();
  refresh(false);
}

// ------------------------------------------------------------------ the ghost

/// One tile is two metres, and the solid a component gets is inset from the
/// tiles it stands on -- the same 250 mm the layout pass uses, so that the box
/// under the pointer is the box that appears when the click lands.
const TILE = 2, INSET = 0.25;

/// Where a component of this kind would be, if it were put down here.
function ghostBox(p, x, y, z, face) {
  const turned = face !== null && face !== undefined && (face & 1) === 1;
  const [w, h] = turned ? [p.h, p.w] : [p.w, p.h];
  const base = z * TILE;
  return {
    lo: [x * TILE + INSET, base, y * TILE + INSET],
    hi: [(x + w) * TILE - INSET, base + (p.height || 2000) / 1000, (y + h) * TILE - INSET],
    base,
    face: face ?? null,
  };
}

/// Whether the rules would take it, as far as this browser can tell.
///
/// Overlapping something is the refusal a player is about to earn nine times
/// out of ten, and it is the one that can be answered here, from the boxes the
/// server already sent. The host still decides -- this only decides what colour
/// the ghost is.
function fits(g) {
  if (g.lo[0] < 0 || g.lo[2] < 0) return false;
  const e = 0.02;
  return !plant.view.units.some(u =>
    g.lo[0] < u.solid.hi[0] - e && g.hi[0] > u.solid.lo[0] + e &&
    g.lo[1] < u.solid.hi[1] - e && g.hi[1] > u.solid.lo[1] + e &&
    g.lo[2] < u.solid.hi[2] - e && g.hi[2] > u.solid.lo[2] + e);
}

const solidOf = name => (plant.view.units.find(u => u.name === name) || {}).solid || null;

function centreOf(name) {
  const s = solidOf(name);
  return s ? [(s.lo[0] + s.hi[0]) / 2, (s.lo[1] + s.hi[1]) / 2, (s.lo[2] + s.hi[2]) / 2] : null;
}

/// Everything the pointer is about to do, handed to the renderer as a picture.
/// Nothing here is sent anywhere.
function paint() {
  if (!ready) return;
  const m = { ghost: null, link: null, targets: [], wires: [] };

  // The connections that are already there, as lines between what makes and
  // what takes. The pipework draws them properly; this draws them legibly,
  // through the plant, so that `connections 3` is three things you can see.
  for (const w of (design ? design.wires : [])) {
    const a = centreOf(w.from), b = centreOf(w.to);
    if (a && b) m.wires.push([a, b]);
  }

  const p = partOf(bench.holding);
  if (p && cursor && cursor.tile) {
    const g = ghostBox(p, cursor.tile.x, cursor.tile.y, bench.level, bench.face);
    m.ghost = { ...g, ok: fits(g) };
  }

  if (bench.wiring && bench.wire) {
    const from = centreOf(bench.wire);
    if (from) {
      for (const u of (design ? design.units : [])) {
        if (pairs(bench.wire, u.name).length) {
          const s = solidOf(u.name);
          if (s) m.targets.push(s);
        }
      }
      const over = cursor && cursor.hit ? cursor.hit.name : null;
      const to = (over && centreOf(over)) || (cursor && cursor.at) || from;
      m.link = { from, to, ok: !!(over && pairs(bench.wire, over).length) };
    }
  }
  plant.mark(m);
}

export function resize() {
  if (ready) plant.fit();
}
