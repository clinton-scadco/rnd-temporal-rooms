// The region view's controls, drawn as things rather than paragraphs.
//
//   the dock       every tool, as an icon with its key on it. One column, no
//                  scrolling, and nothing on it that does the same job as
//                  something else on it: the eight machine chassis the catalogue
//                  lists are one "machine", because an empty chassis is an empty
//                  chassis whatever the catalogue called it
//   the cursor     what is in your hand, at the bottom of the plot, with an x
//   the card       what is under the pointer, next to the pointer. The old
//                  inspector was a panel you had to scroll to while hovering,
//                  which is a thing a hand cannot do
//   the bar        what you can do to the thing you selected, on the thing
//
// Every one of them is a function of the frame and the tool. None of them keep
// state the room or `world.js` does not already have.

import * as net from '../room/net.js';
import * as world from '../room/world.js';

const $ = id => document.getElementById(id);

// ------------------------------------------------------------------- icons

const svg = body =>
  `<svg viewBox="0 0 20 20" fill="none" stroke="currentColor" stroke-width="1.6" ` +
  `stroke-linecap="round" stroke-linejoin="round" aria-hidden="true">${body}</svg>`;

export const ICON = {
  pick: svg('<path d="M5 3l10 6-4.5 1.2L13 16l-2 1-2.5-5.6L5 14z"/>'),
  pipette: svg('<path d="M12.2 4.3a2.3 2.3 0 013.5 3.5L14 9.5 10.5 6z"/><path d="M11.5 7L4.5 14v1.5H6l7-7"/><path d="M9.5 5l5.5 5.5"/><circle cx="3.6" cy="16.4" r=".6" fill="currentColor"/>'),
  head: svg('<path d="M4 16h12"/><path d="M7 16V9h6v7"/><path d="M10 9V3"/><path d="M8 5h4"/><path d="M10 16v2"/>'),
  machine: svg('<rect x="3" y="4" width="14" height="12" rx="1.5"/><circle cx="10" cy="10" r="2.6"/><path d="M10 5.8v1.6M10 12.6v1.6M5.8 10h1.6M12.6 10h1.6"/>'),
  bay: svg('<rect x="4" y="4" width="12" height="12" rx="1"/><path d="M4 9h12M9 4v5"/>'),
  yard: svg('<rect x="2" y="6" width="16" height="10" rx="1"/><path d="M2 10h16M7 6v4M13 6v4"/>'),
  depot: svg('<rect x="3" y="6" width="9" height="8" rx="1"/><path d="M12 10h5M14.5 7.5L17 10l-2.5 2.5"/>'),
  grid: svg('<path d="M11 2L5 11h5l-1 7 6-9h-5z"/>'),
  connect: svg('<circle cx="4.5" cy="5.5" r="2"/><circle cx="15.5" cy="14.5" r="2"/><path d="M6.5 5.5H10v9h3.5"/>'),
  belt: svg('<rect x="2.5" y="7" width="15" height="6" rx="3"/><circle cx="5.5" cy="10" r="1"/><circle cx="14.5" cy="10" r="1"/>'),
  rail: svg('<path d="M6 2l-2 16M14 2l2 16M5.5 6h9M5 10.5h10M4.5 15h11"/>'),
  delete: svg('<path d="M4 6h12M8 6V4h4v2M6 6l1 11h6l1-11"/>'),
  map: svg('<path d="M3 5l4.5-2 5 2L17 3v12l-4.5 2-5-2L3 17z"/><path d="M7.5 3v12M12.5 5v12"/>'),
  plot: svg('<rect x="3" y="3" width="14" height="14" rx="1"/><path d="M3 8h14M8 3v14"/>'),
  gear: svg('<circle cx="10" cy="10" r="2.5"/><path d="M10 2.5v2.2M10 15.3v2.2M2.5 10h2.2M15.3 10h2.2M4.7 4.7l1.6 1.6M13.7 13.7l1.6 1.6M4.7 15.3l1.6-1.6M13.7 6.3l1.6-1.6"/>'),
  crate: svg('<path d="M3 6.5L10 3l7 3.5v7L10 17l-7-3.5z"/><path d="M3 6.5L10 10l7-3.5M10 10v7"/>'),
  clock: svg('<circle cx="10" cy="10" r="7"/><path d="M10 6v4l2.5 2"/>'),
  copy: svg('<rect x="7" y="7" width="10" height="10" rx="1.5"/><path d="M13 7V4.5A1.5 1.5 0 0011.5 3h-7A1.5 1.5 0 003 4.5v7A1.5 1.5 0 004.5 13H7"/>'),
  info: svg('<circle cx="10" cy="10" r="7"/><path d="M10 9v5M10 6.2v.1"/>'),
  portal: svg('<ellipse cx="10" cy="10" rx="4" ry="7"/><path d="M2 10h4M14 10h4"/>'),
  x: svg('<path d="M5 5l10 10M15 5L5 15"/>'),
  back: svg('<path d="M9 5l-5 5 5 5M4 10h12"/>'),
  pencil: svg('<path d="M4 16l1-4 8-8 3 3-8 8z"/><path d="M11.5 5.5l3 3"/>'),
  in: svg('<path d="M3 10h10M9 6l4 4-4 4M16 4v12"/>'),
  out: svg('<path d="M4 4v12M7 10h10M13 6l4 4-4 4"/>'),
};

/// Fill every `<i data-icon>` under a node with its drawing.
export function icons(root = document) {
  root.querySelectorAll('i[data-icon]').forEach(i => {
    if (!i.firstChild) i.innerHTML = ICON[i.dataset.icon] || '';
  });
}

// -------------------------------------------------------------------- dock

/// Everything a hand can hold on the plot.
///
/// `proto` is what gets placed. The machine chassis is `machining` only because
/// something has to be named: every machine prototype places an empty chassis,
/// and its footprint becomes the design's the moment something is designed in.
export const TOOLS = [
  { id: 'pick', key: 'V', label: 'Select', tip: 'hover to read, click to pick, double-click to design' },
  { id: 'pipette', key: 'Q', label: 'Pipette', tip: 'Q over a building picks up another one like it — design and all. Q over empty ground lets go.' },
  null,
  { id: 'head', key: '1', proto: 'head', label: 'Extractor', tip: 'stands on ore, coal, water or wood' },
  { id: 'machine', key: '2', proto: 'machining', label: 'Machine', tip: 'an empty chassis — double-click it to design what it does' },
  { id: 'bay', key: '3', proto: 'bay', label: 'Bay', tip: 'holds 20,000 of one item' },
  { id: 'yard', key: '4', proto: 'yard', label: 'Yard', tip: 'holds 120,000 — for trains and fractures' },
  { id: 'depot', key: '5', proto: 'depot', label: 'Depot', tip: 'ships what it is wired to; goals count it' },
  { id: 'grid', key: '6', proto: 'grid', label: 'Grid', tip: 'sells electricity to the grid', needsGrid: true },
  null,
  { id: 'connect', key: 'W', label: 'Wire', tip: 'click what makes it, then what takes it' },
  { id: 'belt', key: 'B', label: 'Belt', tip: 'bay to bay, short distances' },
  { id: 'rail', key: 'T', label: 'Rail', tip: 'bay to bay, long distances' },
  null,
  { id: 'delete', key: 'X', label: 'Delete', tip: 'click anything; it leaves a ghost you can restore' },
];

export const toolByKey = k => TOOLS.find(t => t && t.key.toLowerCase() === k);
export const toolById = id => TOOLS.find(t => t && t.id === id);

let dockGrid = null;

/// Draw the dock. Redrawn only when whether this century has a grid changes.
export function renderDock(hasGrid, onPick) {
  const box = $('dock');
  if (!box || dockGrid === hasGrid) return;
  dockGrid = hasGrid;
  box.innerHTML = TOOLS.map(t => {
    if (!t) return '<span class="sep"></span>';
    if (t.needsGrid && !hasGrid) return '';
    return (
      `<button data-tool="${t.id}" aria-label="${t.label}">${ICON[t.id]}` +
      `<kbd>${t.key}</kbd>` +
      `<span class="tip"><b>${t.label}</b><kbd>${t.key}</kbd><em>${t.tip}</em></span></button>`
    );
  }).join('');
  box.querySelectorAll('[data-tool]').forEach(b => {
    b.onclick = () => onPick(b.dataset.tool);
  });
  markDock(world.tool);
}

/// Which dock button the tool in hand belongs to.
export function toolIdOf(tool) {
  if (tool.mode !== 'place') return tool.mode;
  const exact = TOOLS.find(t => t && t.proto === tool.proto);
  if (exact) return exact.id;
  const p = net.proto(tool.proto);
  return p && p.role === 'machine' ? 'machine' : null;
}

export function markDock(tool) {
  const on = toolIdOf(tool);
  document.querySelectorAll('#dock [data-tool]').forEach(b =>
    b.classList.toggle('on', b.dataset.tool === on));
  renderCursor(tool);
}

// ------------------------------------------------------------------ cursor

const HINT = {
  place: '<kbd>R</kbd> rotate',
  connect: 'click the source, then where it goes',
  belt: 'click a bay, then another bay',
  rail: 'click a bay, then another bay',
  delete: 'click what to remove',
  pipette: 'click a building to pick up one like it',
};

/// The chip at the bottom of the plot: what is in your hand, and the x.
export function renderCursor(tool, label) {
  const box = $('cursor');
  if (!box) return;
  if (tool.mode === 'pick') { box.hidden = true; return; }
  const id = toolIdOf(tool);
  const t = toolById(id) || {};
  let name = t.label || tool.mode;
  if (tool.mode === 'place' && tool.design) name += ' · copy';
  if (tool.ship) name += ` · ${tool.ship}`;
  if (tool.mode === 'connect' && tool.item) name += ` · ${tool.item}`;
  box.hidden = false;
  box.className = tool.mode === 'delete' ? 'danger' : '';
  $('cursoricon').innerHTML = ICON[id] || ICON.pick;
  $('cursorname').textContent = label || name;
  $('cursorhint').innerHTML = HINT[tool.mode] || '';
}

// ------------------------------------------------------------------- cards

const esc = s => String(s === null || s === undefined ? '' : s)
  .replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

const num = n => net.num(n);

/// What the thing under the pointer is, in plot terms: a building, a wire, a
/// transport, a patch of ground, or something somebody else built in this
/// century. Null for nothing at all.
export function targetAt(p, features) {
  const v = net.state.view;
  if (!p || !v) return null;
  const [x, y] = p.raw;
  const id = world.thingAt(x, y);
  if (id !== null && id !== undefined) return { id };
  const d = (v.world.deposits || []).find(d =>
    x >= d.x && x < d.x + d.w && y >= d.y && y < d.y + d.h);
  if (d) return { deposit: d };
  const f = (features || []).find(f =>
    f.x !== undefined && x >= f.x && x < f.x + f.w && y >= f.y && y < f.y + f.h);
  if (f) return { feature: f };
  return null;
}

/// The hover card, placed beside the pointer and kept inside the stage.
export function renderHover(target, px, py) {
  const box = $('hovercard');
  if (!box) return;
  const html = target ? cardHtml(target) : '';
  if (!html) { box.hidden = true; return; }
  if (box._html !== html) { box.innerHTML = html; box._html = html; }
  box.hidden = false;
  const stage = $('stage').getBoundingClientRect();
  const w = box.offsetWidth, h = box.offsetHeight;
  let x = px + 18, y = py + 18;
  if (x + w > stage.width - 8) x = px - w - 14;
  if (y + h > stage.height - 8) y = Math.max(8, stage.height - h - 8);
  box.style.transform = `translate(${Math.max(8, x)}px, ${y}px)`;
}

function cardHtml(t) {
  if (t.deposit) return depositCard(t.deposit);
  if (t.feature) return featureCard(t.feature);
  const wire = net.wireOf(t.id);
  if (wire) return wireCard(wire);
  const i = net.byId(t.id);
  if (i) return installCard(i);
  const h = ((net.state.view || {}).world || { hauls: [] }).hauls.find(h => h.id === t.id);
  return h ? haulCard(h) : '';
}

/// A building's state as one word and one colour.
export function stateOf(i) {
  const p = net.plantOf(i.name);
  if (i.designed && !i.macro) return ['empty', 'empty'];
  const st = p && p.why ? p.why.state : (i.running ? 'running' : 'idle');
  const cls = st === 'running' ? 'run' : st === 'blocked' ? 'warn' : 'bad';
  return [st, cls];
}

function installCard(i) {
  const p = net.plantOf(i.name);
  const [st, cls] = stateOf(i);
  let html =
    `<div class="hc-head"><span class="sw" style="background:var(--${i.role})"></span>` +
    `<b>${esc(i.name)}</b><span class="pill ${cls}">${esc(st)}</span></div>`;
  if (i.designed && !i.macro) {
    html += `<div class="hc-cta">${ICON.pencil}<span>double-click to design</span></div>`;
  } else if (!i.running && i.idle) {
    html += `<div class="hc-why">${esc(i.idle)}</div>`;
  } else if (p && p.why && p.why.state !== 'running' && p.why.headline) {
    html += `<div class="hc-why">${esc(p.why.headline)}</div>`;
  }
  html += portRows(i, p);
  if (p && p.held && p.capacity) {
    const k = Math.min(100, (100 * (p.used || 0)) / p.capacity);
    html +=
      `<div class="hc-fill"><div class="bar"><div style="width:${k.toFixed(0)}%"></div></div>` +
      `<span>${num(p.used)} / ${num(p.capacity)}</span></div>` +
      (p.held.length
        ? `<div class="hc-held">${p.held.map(q => `<span>${num(q.qty)} ${esc(q.item)}</span>`).join('')}</div>`
        : '');
  }
  if (i.macro) {
    html += `<div class="hc-foot">${ICON.clock}<span>${i.macro.cycleSeconds.toFixed(1)}s cycle · ` +
      `${i.macro.components} parts</span></div>`;
  }
  return html;
}

/// One row per port: a filled pip if something is on the other end of it, a
/// hollow one if nothing is, which is how the input nobody fed is found.
function portRows(i, p) {
  const v = net.state.view;
  const ports = i.ports || [];
  if (!ports.length) return '';
  const partner = {};
  for (const c of v.world.conns) {
    if (c.from === i.id) partner['o' + c.item] = c.to;
    if (c.to === i.id) partner['i' + c.item] = c.from;
  }
  for (const h of v.world.hauls) {
    if (h.from === i.id) partner['o' + h.item] = h.to;
    if (h.to === i.id) partner['i' + h.item] = h.from;
  }
  const short = new Set(((p && p.why && p.why.needs) || []).filter(n => n.short).map(n => n.item));
  const seen = new Set();
  const row = pt => {
    const key = (pt.out ? 'o' : 'i') + pt.item;
    if (seen.has(key)) return '';
    seen.add(key);
    const other = partner[key] !== undefined ? net.byId(partner[key]) : null;
    const on = partner[key] !== undefined;
    const bad = !pt.out && (!on || short.has(pt.item));
    return (
      `<div class="pr${bad ? ' bad' : ''}">` +
      `<i class="pip${on ? '' : ' hollow'}" style="--d:var(--${pt.domain})"></i>` +
      `<span class="it">${esc(pt.title || pt.item)}</span>` +
      `<em>${num(pt.perSecond)}${pt.domain === 'electrical' ? ' MW' : '/s'}</em>` +
      `<span class="to">${on ? (pt.out ? '→ ' : '← ') + esc(other ? other.name : '') : 'not wired'}</span></div>`
    );
  };
  const ins = ports.filter(pt => !pt.out).map(row).join('');
  const outs = ports.filter(pt => pt.out).map(row).join('');
  return (
    (ins ? `<div class="hc-ports"><span class="lbl">${ICON.in}</span><div>${ins}</div></div>` : '') +
    (outs ? `<div class="hc-ports"><span class="lbl">${ICON.out}</span><div>${outs}</div></div>` : '')
  );
}

function wireCard(w) {
  const a = net.byId(w.from), b = net.byId(w.to);
  if (!a || !b) return '';
  const c = (net.state.view.world.conns || []).find(
    c => c.from === w.from && c.to === w.to && c.item === w.item) || w;
  const buf = c.buffer && net.plantOf(c.buffer);
  const held = buf && buf.held && buf.held.length ? buf.held[0].qty : 0;
  return (
    `<div class="hc-head"><i class="pip" style="--d:var(--${c.domain || 'material'})"></i>` +
    `<b>${esc(c.title || w.item)}</b></div>` +
    `<div class="hc-route">${esc(a.name)} <span>→</span> ${esc(b.name)}</div>` +
    (c.buffer && c.capacity
      ? `<div class="hc-fill"><div class="bar"><div style="width:${Math.min(100, (100 * held) / c.capacity).toFixed(0)}%"></div></div>` +
        `<span>${num(held)} / ${num(c.capacity)}</span></div>`
      : '')
  );
}

function haulCard(h) {
  const a = net.byId(h.from), b = net.byId(h.to);
  const p = net.plantOf(h.name);
  const g = h.geometry || {};
  return (
    `<div class="hc-head"><span class="sw" style="background:var(--storage)"></span>` +
    `<b>${esc(h.title)}</b><span class="pill ${h.running ? 'run' : 'bad'}">${h.running ? 'running' : 'waiting'}</span></div>` +
    `<div class="hc-route">${esc(a ? a.name : '?')} <span>→</span> ${esc(b ? b.name : '?')} · ${esc(h.item)}</div>` +
    `<div class="hc-foot">${ICON.clock}<span>${(g.seconds || 0).toFixed(1)}s each way · ` +
    `${num(g.load)} × ${num(g.vehicles)}${p ? ` · ${(p.rate * 60).toFixed(1)}/s` : ''}</span></div>` +
    (h.idle ? `<div class="hc-why">${esc(h.idle)}</div>` : '')
  );
}

function depositCard(d) {
  const k = d.yields ? (100 * (d.yields - d.spare)) / d.yields : 0;
  return (
    `<div class="hc-head"><i class="pip" style="--d:var(--${d.domain || 'material'})"></i>` +
    `<b>${esc(d.title)}</b><span class="pill ${d.spare ? 'run' : 'bad'}">${d.spare ? 'open' : 'taken'}</span></div>` +
    `<div class="hc-fill"><div class="bar"><div style="width:${k.toFixed(0)}%"></div></div>` +
    `<span>${num(d.spare)} of ${num(d.yields)}/s ${esc(d.itemTitle || d.item)} free</span></div>` +
    `<div class="hc-cta">${ICON.head}<span>put an extractor on it <kbd>1</kbd></span></div>`
  );
}

function featureCard(f) {
  return (
    `<div class="hc-head"><span class="sw" style="background:${esc(f.colour)}"></span>` +
    `<b>${esc(f.name)}</b>${f.taken ? '<span class="pill bad">built on</span>' : ''}</div>` +
    `<div class="hc-why">${esc(f.what)}${f.yields ? ` · ${num(f.yields.perSecond)}/s` : ''}</div>`
  );
}

// ------------------------------------------------------------ selection bar

/// The actions of the selected thing, hung above it on the plot.
///
/// Designing a machine used to be: click it, find the inspector, scroll to the
/// bottom of the inspector, press the button. It is now the first button on
/// the building (or a double-click on it).
export function renderSelbar(id, acts) {
  const box = $('selbar');
  if (!box) return;
  const v = net.state.view;
  if (id === null || id === undefined || !v) { box.hidden = true; box._sig = ''; return; }
  const i = net.byId(id);
  const wire = !i && net.wireOf(id);
  const haul = !i && !wire && (v.world.hauls || []).find(h => h.id === id);
  if (!i && !wire && !haul) { box.hidden = true; box._sig = ''; return; }

  const sig = JSON.stringify([id, i && [i.macro ? 1 : 0, i.ports, i.name]]);
  if (box._sig !== sig) {
    box._sig = sig;
    let html = '';
    if (i) {
      const [st, cls] = stateOf(i);
      html += `<span class="name"><span class="dot ${cls}"></span>${esc(i.name)}</span>`;
      if (i.designed) {
        html += `<button data-act="open" class="primary" title="open the bench">${ICON.pencil}<span>${i.macro ? 'Open' : 'Design'}</span></button>`;
      }
      html += `<button data-act="copy" class="icon" title="pick up another one like it (Q)">${ICON.pipette}</button>`;
      const ports = [];
      const seen = new Set();
      for (const p of i.ports || []) {
        const key = (p.out ? 'o' : 'i') + p.item;
        if (seen.has(key)) continue;
        seen.add(key);
        ports.push(p);
      }
      if (ports.length) {
        html += '<span class="rule"></span>' + ports.map(p =>
          `<button data-act="connect" data-item="${esc(p.item)}" class="portbtn" ` +
          `title="wire ${p.out ? 'out of' : 'into'} this: ${esc(p.title || p.item)}">` +
          `<i class="pip" style="--d:var(--${p.domain})"></i>${p.out ? ICON.out : ICON.in}</button>`).join('');
      }
      html += `<span class="rule"></span><button data-act="delete" class="icon danger" title="delete">${ICON.delete}</button>`;
    } else {
      const title = wire
        ? `${(net.byId(wire.from) || {}).name} → ${(net.byId(wire.to) || {}).name} · ${wire.item}`
        : `${haul.title} · ${haul.item}`;
      html += `<span class="name">${esc(title)}</span>` +
        `<button data-act="${wire ? 'unwire' : 'unlink'}" class="icon danger" title="delete">${ICON.delete}</button>`;
    }
    box.innerHTML = html;
    box.querySelectorAll('[data-act]').forEach(b => {
      b.onclick = e => {
        e.stopPropagation();
        const a = acts[b.dataset.act];
        if (!a) return;
        if (i) a(i, b.dataset.item);
        else if (wire) a(wire);
        else a(haul);
      };
    });
  }
  box.hidden = false;
  placeSelbar(box, i);
}

/// Keep the bar above its building while the plot pans and zooms under it.
export function placeSelbar(box, i) {
  box = box || $('selbar');
  if (!box || box.hidden) return;
  const stage = $('stage').getBoundingClientRect();
  const w = box.offsetWidth, h = box.offsetHeight;
  let x, y;
  if (i) {
    const r = world.rectOf(i);
    x = r.x + r.w / 2 - w / 2;
    y = r.y - h - 10;
    if (y < 8) y = r.y + r.h + 10;
  } else {
    x = stage.width / 2 - w / 2;
    y = 12;
  }
  x = Math.max(8, Math.min(stage.width - w - 8, x));
  y = Math.max(8, Math.min(stage.height - h - 8, y));
  box.style.transform = `translate(${x}px, ${y}px)`;
}
