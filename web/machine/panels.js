// Everything that is words rather than pixels.
//
// The scoreboard is the argument of the experiment made visible: eight numbers,
// none of them a score. A design that beats another on power and loses on
// footprint has not lost, and the panel refuses to pretend otherwise by adding
// them up.

import { state, part, unitOf, retune, unwire, num, toNum } from './doc.js';
import { statusColour, portColour, drawWave } from './render.js';
import { select } from './canvas.js';

const $ = s => document.querySelector(s);
const el = (tag, cls, text) => {
  const e = document.createElement(tag);
  if (cls) e.className = cls;
  if (text !== undefined) e.textContent = text;
  return e;
};

// ---------------------------------------------------------------- palette

export function renderPalette(onPick) {
  const box = $('#palette');
  box.replaceChildren();
  for (const kind of state.cat.order) {
    const p = state.cat.parts[kind];
    const b = el('button');
    b.dataset.kind = kind;
    b.title = `${p.title} — ${p.blurb}`;
    const pips = el('span', 'pips');
    for (const q of p.ports) {
      const s = el('span');
      s.style.background = portColour(q.type);
      s.style.opacity = q.dir === 'in' ? 0.45 : 1;
      s.title = `${q.dir} ${q.type} ${q.rate}/tick`;
      pips.appendChild(s);
    }
    const label = el('span');
    label.appendChild(el('b', null, p.title));
    label.appendChild(document.createElement('br'));
    label.appendChild(el('i', null, `${p.w}x${p.h}`));
    b.append(pips, label);
    b.addEventListener('click', () => onPick(kind));
    box.appendChild(b);
  }
}

export function markTool(kind) {
  document.querySelectorAll('#palette button')
    .forEach(b => b.classList.toggle('on', b.dataset.kind === kind));
}

// ------------------------------------------------------------- scoreboard

export function renderScore() {
  const snap = state.snapshot;
  const v = $('#verdict');
  const tiles = $('#tiles');
  tiles.replaceChildren();
  if (!snap) {
    v.className = 'verdict';
    v.textContent = state.error ? 'will not run' : '—';
    return;
  }
  const r = snap.report;
  const now = snap.now;
  v.className = 'verdict ' + (r.met ? 'met' : 'missed');
  v.textContent = r.met
    ? `${r.targetMw} MW · MET`
    : r.power.value >= r.targetMw
      ? 'not to the brief'
      : `${(r.targetMw - r.power.value).toFixed(0)} MW short`;

  const tile = (label, value, note, cls) => {
    const d = el('div');
    d.appendChild(el('dt', null, label));
    const dd = el('dd', cls);
    dd.textContent = value;
    if (note) dd.appendChild(el('small', null, note));
    d.appendChild(dd);
    tiles.appendChild(d);
  };
  // Steady state first, with the current tick beside it: the difference
  // between the two is the transient, and the transient is the point.
  tile('electrical out', r.power.value.toFixed(2), `MW · now ${now.power}`, r.met ? 'hi' : '');
  tile('fuel', r.fuel.value.toFixed(1), '/tick');
  tile('water', r.water.value.toFixed(1), '/tick', r.water.value > 0 ? '' : 'hi');
  tile('heat wasted', r.wasted.value.toFixed(1), '/tick', r.wasted.value > 0 ? 'warn' : 'hi');
  tile('steam vented', r.vented.value.toFixed(1), '/tick', r.vented.value > 0 ? 'warn' : 'hi');
  tile('footprint', `${r.width}x${r.height}`, `${r.area} tiles · ${(r.density * 100).toFixed(0)}% full`);
  tile('components', r.components, '');
  tile('utilisation', r.utilisation.value.toFixed(0) + '%', '',
    r.utilisation.value > 75 ? 'hi' : r.utilisation.value < 40 ? 'warn' : '');
  // A machine that produces nothing has no per-megawatt anything, and the
  // server says so with a null rather than an invented number. Calling
  // `.toFixed` on that used to throw here -- which aborted the whole refresh,
  // so the inspector never redrew and every click looked dead.
  const per = v => (typeof v === 'number' && isFinite(v) ? v.toFixed(2) : '—');
  tile('per megawatt', per(r.per.areaPerMw),
    r.per.waterPerMw === null ? 'no power, no ratio' : `tiles · ${per(r.per.waterPerMw)} water`);
}

// -------------------------------------------------------------- inspector

/// What the inspector is showing, as a value that can be compared.
///
/// The pane is allowed to skip a rebuild, but only ever for the *same* thing --
/// which is what this exists to decide.
export function paneKey(sel) {
  if (!sel) return 'none';
  return sel.what === 'wire' ? `wire:${sel.i}` : `unit:${sel.name}`;
}

/// What `#detail` currently holds.
let showing = 'none';

export function renderInspector() {
  const box = $('#detail');
  const key = paneKey(state.selected);
  // Rebuilding this pane while a slider inside it is being dragged would take
  // the slider out from under the pointer on the first frame -- but only while
  // it is still the same component's slider.
  //
  // The first version of this guard left out the `key === showing`, and a
  // canvas is not focusable, so clicking a tank's pulse checkbox and then
  // clicking anything else left the focus sitting in a checkbox forever and
  // the inspector frozen on the tank. Every subsequent click looked dead.
  const busy = box.contains(document.activeElement) &&
    document.activeElement.tagName === 'INPUT';
  if (busy && key === showing) return;
  showing = key;
  box.replaceChildren();
  const sel = state.selected;
  if (!sel) {
    box.appendChild(el('p', 'hint', 'click a component, or a connection'));
    return;
  }
  if (sel.what === 'wire') return wirePane(box, sel.i);

  const u = unitOf(sel.name);
  if (!u) {
    box.appendChild(el('p', 'hint', 'gone'));
    return;
  }
  const p = part(u.kind);
  const snap = state.snapshot && state.snapshot.units.find(x => x.name === sel.name);

  const who = el('div', 'who');
  who.appendChild(el('b', null, u.name));
  who.appendChild(el('span', null, p.title));
  box.appendChild(who);

  if (snap) {
    const badge = el('span', 'status ' + severity(snap.status), snap.status);
    badge.style.borderColor = statusColour(snap.status);
    badge.style.color = statusColour(snap.status);
    box.appendChild(badge);

    const bar = el('div', 'bar' + (snap.util < 40 ? ' warn' : ''));
    const fill = el('i');
    fill.style.width = Math.min(100, snap.util) + '%';
    bar.appendChild(fill);
    box.appendChild(bar);

    const why = el('div', 'why');
    for (const line of snap.why) why.appendChild(el('div', null, line));
    box.appendChild(why);
  } else {
    box.appendChild(el('p', 'hint', 'not part of a machine that runs yet'));
  }

  tunables(box, u, p);

  // Ports, with what is in them and what crossed them.
  const t = el('table', 'rows');
  const head = el('tr');
  ['port', 'held', 'in', 'out'].forEach(h => head.appendChild(el('th', null, h)));
  t.appendChild(head);
  p.ports.forEach((q, i) => {
    const live = snap ? snap.ports[i] : null;
    const tr = el('tr');
    const name = el('td');
    const dot = el('span', 'dot');
    dot.style.background = portColour(q.type);
    name.append(dot, document.createTextNode(`${q.name}`));
    tr.appendChild(name);
    tr.appendChild(cell(live ? `${live.level}/${q.cap}` : `–/${q.cap}`, 'n k'));
    tr.appendChild(cell(live ? String(live.got || live.made || 0) : '–', 'n'));
    tr.appendChild(cell(live ? String(live.sent || live.used || 0) : '–', 'n'));
    t.appendChild(tr);
  });
  box.appendChild(t);

  box.appendChild(el('p', 'hint', p.blurb));
}

function cell(text, cls) { return el('td', cls, text); }

function severity(status) {
  if (status === 'RUNNING' || status === 'FILLING') return 'ok';
  if (status === 'BLOCKED' || status === 'STALLED') return 'bad';
  if (status === 'IDLE') return '';
  return 'warn';
}

/// The two components with a decision in them. Everything else is what it is.
function tunables(box, u, p) {
  if (u.kind === 'reactor') {
    const min = state.cat.constants.minThrottle;
    const row = el('div', 'field');
    const label = el('label', null, `throttle ${u.throttle}%`);
    row.appendChild(label);
    const note = el('p', 'hint');
    const say = v => {
      label.textContent = `throttle ${v}%`;
      note.textContent =
        `${state.cat.constants.reactorHeat * v / 100} heat/tick, ` +
        `${state.cat.constants.reactorFuel * v / 100} fuel/tick`;
    };
    const r = document.createElement('input');
    r.type = 'range';
    r.min = min; r.max = 100; r.step = 1; r.value = u.throttle;
    r.addEventListener('input', () => { say(Number(r.value)); retune(u.name, { throttle: Number(r.value) }); });
    row.appendChild(r);
    box.appendChild(row);
    say(u.throttle);
    box.appendChild(note);
  }
  if (u.kind === 'tank') {
    const check = el('label', 'check');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!u.pulse;
    cb.addEventListener('change', () => retune(u.name, { pulse: cb.checked }));
    check.append(cb, document.createTextNode(' pulse instead of passing through'));
    box.appendChild(check);
    if (u.pulse) {
      for (const [k, label] of [['high', 'fill to'], ['low', 'empty to']]) {
        const row = el('div', 'field');
        row.appendChild(el('label', null, label));
        const inp = document.createElement('input');
        inp.type = 'number';
        inp.min = 0; inp.max = part('tank').ports[0].cap; inp.value = u[k];
        inp.addEventListener('change', () => retune(u.name, { [k]: Number(inp.value) }));
        row.appendChild(inp);
        box.appendChild(row);
      }
    }
  }
}

function wirePane(box, i) {
  const w = state.design.wires[i];
  if (!w) return;
  const live = state.snapshot && state.snapshot.wires[i];
  const who = el('div', 'who');
  who.appendChild(el('b', null, `${w.from}.${w.fromPort}`));
  who.appendChild(el('span', null, '→ ' + `${w.to}.${w.toPort}`));
  box.appendChild(who);
  if (live) {
    const frac = Math.min(1, toNum(live.flow) / Math.max(1, live.rate));
    const bar = el('div', 'bar');
    const fill = el('i');
    fill.style.width = frac * 100 + '%';
    fill.style.background = portColour(live.type);
    bar.appendChild(fill);
    box.appendChild(bar);
    const why = el('div', 'why');
    why.appendChild(el('div', null, `${live.type}: ${live.flow} of ${live.rate} per tick`));
    why.appendChild(el('div', null, `${(frac * 100).toFixed(1)}% of what this connection can carry`));
    why.appendChild(el('div', null, `${live.gap} tiles apart, of ${state.cat.constants.reach} reachable`));
    box.appendChild(why);
  }
  const b = el('button', null, 'remove this connection');
  b.addEventListener('click', () => { unwire(i); select(null); });
  box.appendChild(b);
}

// ------------------------------------------------------- holding it back

export function renderHolding() {
  const box = $('#holding');
  box.replaceChildren();
  const snap = state.snapshot;
  if (!snap) {
    for (const f of state.faults) {
      const row = el('div', 'holdrow bad');
      row.appendChild(el('b', null, f.unit || 'the design'));
      row.appendChild(el('span', null, f.what));
      box.appendChild(row);
    }
    if (!state.faults.length) box.appendChild(el('p', 'hint', state.error || 'nothing to run yet'));
    return;
  }
  if (!snap.holding.length) {
    box.appendChild(el('p', 'hint ok', 'every component is doing its job'));
    return;
  }
  for (const h of snap.holding) {
    const row = el('div', 'holdrow' + (h.status === 'STALLED' || h.status === 'BLOCKED' ? ' bad' : ''));
    const b = el('b', null, `${h.name} · ${h.status}`);
    b.style.color = statusColour(h.status);
    row.appendChild(b);
    row.appendChild(el('span', null, h.why));
    row.addEventListener('click', () => select({ what: 'unit', name: h.name }));
    row.style.cursor = 'pointer';
    box.appendChild(row);
  }
}

// --------------------------------------------------------- the compiled

export function renderMacro(verifyResult) {
  const box = $('#macro');
  box.replaceChildren();
  const c = state.compiled;
  const m = c ? c.macro : state.macro;
  if (!m) {
    box.appendChild(el('p', 'hint', 'press compile'));
    return;
  }
  const line = (label, value, lead) => {
    const d = el('div', lead ? 'lead' : null);
    d.appendChild(el('b', null, label + ' '));
    d.appendChild(document.createTextNode(value));
    box.appendChild(d);
  };
  line(m.name, '', true);
  for (const i of m.externalInputs) line('  in  ', `${i.what} ${i.rate.value.toFixed(2)}/tick`);
  for (const o of m.externalOutputs) line('  out ', `${o.what} ${o.rate.value.toFixed(2)}/tick`);
  line('  plot', m.footprint + ` · ${m.internalComponents} parts inside`);
  line('  loop', m.note);
  line('  state', `${m.internalStateBytes} bytes to resume it`);
  if (c && c.settled) {
    box.appendChild(el('div', null,
      `  tick 10^9 costs ${num(c.transient + c.period)} steps, not 10^9`));
  }
  if (verifyResult) {
    const d = el('div', verifyResult.agrees ? 'lead' : null);
    d.textContent = verifyResult.agrees
      ? `  checked at ${verifyResult.checks.length} ticks: the compiled answer and a straight run agree`
      : '  the compiled answer and a straight run DISAGREE';
    box.appendChild(d);
  }
}

export function renderWave() {
  const note = !state.snapshot
    ? 'nothing runs yet'
    : 'press compile to find the orbit';
  drawWave($('#wave'), state.compiled, note);
}

// -------------------------------------------------------------- the file

export function renderSource() {
  const pre = $('#src');
  if (pre.hidden) return;
  pre.textContent = emit();
}

/// The same file the server would write, composed here so that ticking "the
/// file" does not need a round trip. Exported because a second implementation
/// of a file format is a liability unless something compares the two, and
/// `tests/machine_web.mjs` does.
export function emit() {
  const d = state.design;
  const wide = Math.max(4, ...d.units.map(u => u.name.length));
  let s = `machine "${d.name}"\n\n`;
  for (const u of d.units) {
    s += `${u.kind.padEnd(9)} ${u.name.padEnd(wide)} at ${u.x},${u.y}`;
    if (u.kind === 'reactor' && u.throttle !== 100) s += `  throttle ${u.throttle}`;
    if (u.kind === 'tank' && u.pulse) s += `  pulse ${u.high} ${u.low}`;
    s += '\n';
  }
  if (d.wires.length) s += '\n';
  for (const w of d.wires) s += `wire ${w.from}.${w.fromPort} -> ${w.to}.${w.toPort}\n`;
  return s;
}
