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

export const bench = { id: null, level: 0, face: null, holding: null, sel: null, draft: false };

let ready = false;
let design = null;      // the document in the window, live or draft
let state = null;       // what every component in it is doing, and why
let onLeave = () => {};

export async function init(hooks) {
  onLeave = hooks.onLeave || (() => {});
  await net.parts();
  plant.authoring({
    // Place-and-delete: there is no onMove, so form.js cannot slide anything.
    onPick: name => { bench.sel = name; refresh(); },
    onGround: (x, y) => put(x, y),
    level: () => bench.level,
    tile: at => ({ x: Math.round(at[0] / 2), y: Math.round(at[2] / 2) }),
    onTurn: d => { bench.face = (((bench.face ?? 0) + d) % 4 + 4) % 4; hint(); },
    onLift: d => { bench.level = Math.max(0, bench.level + d); $('benchlevel').textContent = bench.level; },
  });
  ready = await plant.initForm($('plant3d'));
  palette();
  $('benchedit').onclick = () => net.send('OpenDesign', { id: bench.id });
  $('benchcommit').onclick = commit;
  $('benchdiscard').onclick = () => net.send('CloseDesign', { id: bench.id, keep: false });
  $('benchclose').onclick = () => onLeave();
  $('benchup').onclick = () => { bench.level++; $('benchlevel').textContent = bench.level; };
  $('benchdown').onclick = () => { bench.level = Math.max(0, bench.level - 1); $('benchlevel').textContent = bench.level; };
  $('family').onchange = palette;
}

export async function open(id) {
  bench.id = id;
  bench.sel = null;
  bench.holding = null;
  await refresh(true);
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
  state = await net.inside(bench.id, i.hasDraft);
  $('benchname').textContent = i.name + ' · ' + i.title + (i.hasDraft ? ' · draft' : '');
  $('benchedit').disabled = !!i.editor;
  $('benchcommit').disabled = !mine;
  $('benchdiscard').disabled = !mine;
  $('benchlock').textContent = i.editor === null || i.editor === undefined
    ? ''
    : (mine ? 'you are editing this' : editorName(i.editor) + ' is editing this');
  info(i);
}

function editorName(id) {
  const p = net.state.view.players.find(p => p.id === id);
  return p ? p.name : `player ${id}`;
}

// --------------------------------------------------------------- the palette

function palette() {
  const cat = net.state.parts;
  if (!cat) return;
  const fam = $('family');
  if (!fam.options.length) {
    const fams = [...new Set(cat.parts.map(p => p.family))].sort();
    fam.innerHTML = fams.map(f => `<option>${f}</option>`).join('');
  }
  const box = $('parts');
  box.innerHTML = '';
  for (const p of cat.parts.filter(p => p.family === fam.value)) {
    const b = document.createElement('button');
    b.title = p.blurb;
    b.innerHTML = `<span>${p.title}</span><span class="n">${p.w}&times;${p.h}</span>`;
    b.onclick = () => {
      bench.holding = bench.holding === p.kind ? null : p.kind;
      bench.face = null;
      hint();
      [...box.children].forEach(c => c.classList.toggle('on', c === b && bench.holding));
    };
    box.appendChild(b);
  }
}

function hint() {
  const k = bench.holding;
  $('benchinfo').dataset.hint = k || '';
  if (k) {
    toast(`${k} · click the floor at level ${bench.level} · R turns it`);
  }
}

/// One component, placed at the tile under the pointer.
function put(x, y) {
  if (!bench.holding) return;
  if (!bench.draft) return toast('open a draft first');
  net.send('PlaceComponent', {
    id: bench.id, kind: bench.holding, x, y, z: bench.level, face: bench.face,
  });
}

async function commit() {
  const i = net.byId(bench.id);
  if (!i || !i.hasDraft) return toast('there is nothing to commit');
  const res = await net.form(bench.id, true);
  if (!res.ok) return toast(res.error);
  const out = await net.send('CommitMachineDesign', { id: bench.id, design: res.design });
  if (out.ok) toast('committed: the machine changes at one tick, everywhere');
}

// -------------------------------------------------------------- the panels

function info(i) {
  const box = $('benchinfo');
  const m = i.macro || {};
  const sel = bench.sel && design ? design.units.find(u => u.name === bench.sel) : null;
  const live = state && state.ok ? state.units.find(u => u.name === bench.sel) : null;
  let html =
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
    if (bench.draft) {
      html += '<div class="acts" style="display:flex;gap:4px;margin-top:8px">' +
        '<button id="bconnect">connect</button><button id="bdelete">delete</button></div>';
      if (p && p.tunable) html += tuner(sel, p);
    }
  }
  box.innerHTML = html;
  if ($('bdelete')) {
    $('bdelete').onclick = () =>
      net.send('DeleteComponent', { id: bench.id, unit: bench.sel });
  }
  if ($('bconnect')) $('bconnect').onclick = () => startWire(sel);
  box.querySelectorAll('[data-tune]').forEach(el => {
    el.onchange = () => net.send('TuneComponent', {
      id: bench.id, unit: sel.name, field: el.dataset.tune, value: String(el.value),
    });
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

/// Semantic snapping, as a menu rather than as a magnet: only the pairs the
/// rules would accept are offered, and the rules are the same ones the host
/// will apply to the command.
function startWire(from) {
  const cat = net.state.parts.parts;
  const a = cat.find(p => p.kind === from.kind);
  if (!a) return;
  const outs = a.ports.filter(q => q.dir === 'out');
  if (!outs.length) return toast(`${from.name} has nothing to give`);
  const options = [];
  for (const u of design.units) {
    if (u.name === from.name) continue;
    const b = cat.find(p => p.kind === u.kind);
    if (!b) continue;
    for (const o of outs) {
      for (const q of b.ports.filter(q => q.dir === 'in' && q.type === o.type)) {
        options.push({
          label: `${o.name} → ${u.name}.${q.name}`,
          pick: () => net.send('ConnectComponent', {
            id: bench.id, from: from.name, fromPort: o.name, to: u.name, toPort: q.name,
          }),
        });
      }
    }
  }
  if (!options.length) return toast('nothing here takes what it makes');
  menu('connect to', options.slice(0, 14));
}

export function resize() {
  if (ready) plant.fit();
}
