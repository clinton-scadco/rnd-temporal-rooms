// Everything that is words rather than pixels.
//
// The scoreboard is the argument of the experiment made visible: a row of
// numbers, none of them a score. A design that beats another on output and
// loses on footprint has not lost, and the panel refuses to pretend otherwise
// by adding them up.
//
// Experiment 07 changed what is on it. There are four briefs now, each asking
// for something different, so the first tiles are the brief's own targets --
// `30/tick of Iron Ore, powder or finer, 80%+ pure` -- and the costs after them
// are the same for all four.

import { state, part, unitOf, retune, unwire, num, toNum, brief } from './doc.js';
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
  // Thirty-eight buttons in one column is a list nobody reads, so they come out
  // in the families they were designed in -- and the family headings are the
  // argument of the experiment: sources, sinks, transport, stores, control,
  // heat, mechanical, process, and nothing that is only ever one machine.
  let shown = null;
  for (const kind of state.cat.order) {
    const p = state.cat.parts[kind];
    if (state.family && p.family !== state.family) continue;
    if (p.family !== shown) {
      shown = p.family;
      box.appendChild(el('div', 'family', p.family));
    }
    const b = el('button');
    b.dataset.kind = kind;
    b.title = describe(p);
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

/// A component's whole behaviour as a tooltip: what it takes, what it refuses,
/// and what it does to what passes through. Composed from the recipe the server
/// sends, so a component that changes in Rust changes here.
export function describe(p) {
  const lines = [`${p.title} — ${p.blurb}`];
  if (p.recipe) {
    for (const d of p.recipe.draws) {
      lines.push(`takes ${d.qty} ${d.port}/tick` + (d.needs.length ? `  (${d.needs.join(', ')})` : ''));
    }
    for (const m of p.recipe.makes) {
      lines.push(`makes ${m.qty} ${m.port}/tick` + (m.does.length ? `  (${m.does.join(', ')})` : ''));
    }
  }
  return lines.join('\n');
}

export function renderFamilies(onPick) {
  const sel = $('#familypick');
  if (!sel) return;
  sel.replaceChildren();
  for (const [value, label] of [['', 'every family'], ...state.cat.families.map(f => [f, f])]) {
    const o = document.createElement('option');
    o.value = value;
    o.textContent = label;
    sel.appendChild(o);
  }
  sel.value = state.family;
  sel.addEventListener('change', () => onPick(sel.value));
}

export function renderBriefPicker(onPick) {
  const sel = $('#briefpick');
  sel.replaceChildren();
  for (const b of state.cat.briefs) {
    const o = document.createElement('option');
    o.value = b.tag;
    o.textContent = b.title;
    sel.appendChild(o);
  }
  sel.addEventListener('change', () => onPick(sel.value));
}

export function renderBrief() {
  const b = brief();
  $('#briefpick').value = state.design.brief;
  $('#goal').textContent = b ? b.goal : '';
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
  const missed = r.targets.filter(t => !t.met);
  v.className = 'verdict ' + (r.met ? 'met' : 'missed');
  v.textContent = r.met
    ? 'MET'
    : missed.length === 0
      ? 'not to the brief'
      : missed
          .map(t => `${(t.need - t.got.value).toFixed(1)} ${t.label} short`)
          .join(' · ');

  const tile = (label, value, note, cls) => {
    const d = el('div');
    d.appendChild(el('dt', null, label));
    const dd = el('dd', cls);
    dd.textContent = value;
    if (note) dd.appendChild(el('small', null, note));
    d.appendChild(dd);
    tiles.appendChild(d);
  };
  // What the brief asked for first, one tile each, with the target under it:
  // steady state above, the requirement below, and the difference is the whole
  // verdict.
  for (const t of r.targets) {
    tile(t.label, t.got.value.toFixed(2), `of ${t.need} · ${t.wanted}`, t.met ? 'hi' : 'warn');
  }
  // Then the costs, which are the same four questions whatever the brief.
  if (r.brief !== 'power') {
    tile('grid draw', r.grid.value.toFixed(1), 'MW/tick', r.grid.value > 0 ? 'warn' : 'hi');
  } else {
    tile('electrical out', r.power.value.toFixed(2), `MW · now ${now.power}`, r.met ? 'hi' : '');
  }
  tile('fuel', r.fuel.value.toFixed(1), '/tick');
  tile('water', r.water.value.toFixed(1), '/tick', r.water.value > 0 ? '' : 'hi');
  tile('heat wasted', r.wasted.value.toFixed(1), '/tick', r.wasted.value > 0 ? 'warn' : 'hi');
  tile('thrown away', r.vented.value.toFixed(1), '/tick', r.vented.value > 0 ? 'warn' : 'hi');
  tile('footprint', `${r.width}x${r.height}`, `${r.area} tiles · ${(r.density * 100).toFixed(0)}% full`);
  tile('components', r.components, '');
  tile('utilisation', r.utilisation.value.toFixed(0) + '%', '',
    r.utilisation.value > 75 ? 'hi' : r.utilisation.value < 40 ? 'warn' : '');
  // A machine that produces nothing has no per-unit anything, and the server
  // says so with a null rather than an invented number. Calling `.toFixed` on
  // that used to throw here -- which aborted the whole refresh, so the
  // inspector never redrew and every click looked dead.
  const per = v => (typeof v === 'number' && isFinite(v) ? v.toFixed(2) : '—');
  tile('per unit made', per(r.per.areaPerOut),
    r.per.waterPerOut === null ? 'nothing made, no ratio' : `tiles · ${per(r.per.waterPerOut)} water`);
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
    // What is actually in the port, which in experiment 07 is the interesting
    // half: a rolling mill that is full of the right amount of the wrong thing
    // looks exactly like one that is working until you read this line.
    if (live && live.holding) name.appendChild(el('small', 'stuff', live.holding));
    if (q.external) name.appendChild(el('small', 'stuff', 'the machine boundary'));
    tr.appendChild(name);
    tr.appendChild(cell(live ? `${live.level}/${q.cap}` : `–/${q.cap}`, 'n k'));
    tr.appendChild(cell(live ? String(live.got || live.made || 0) : '–', 'n'));
    tr.appendChild(cell(live ? String(live.sent || live.used || live.shipped || 0) : '–', 'n'));
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

/// The components with a decision in them. Everything else is what it is.
///
/// Nine kinds have a tunable and twenty-nine do not, so the shape of this
/// function is a switch rather than a form: a slider for a throttle, a select
/// for a substance, a number for a threshold. The catalogue says which kinds
/// have one at all, so a component that stops being tunable in Rust stops
/// offering a box here.
function tunables(box, u, p) {
  if (!p.tunable) return;
  const field = (label, node) => {
    const row = el('div', 'field');
    row.appendChild(el('label', null, label));
    row.appendChild(node);
    box.appendChild(row);
    return row;
  };
  const number = (key, min, max) => {
    const inp = document.createElement('input');
    inp.type = 'number';
    inp.min = min; inp.max = max; inp.value = u[key];
    inp.addEventListener('change', () => retune(u.name, { [key]: Number(inp.value) }));
    return inp;
  };

  if (u.kind === 'reactor') {
    const min = state.cat.constants.minThrottle;
    const label = el('label', null, `throttle ${u.throttle}%`);
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
    const row = el('div', 'field');
    row.append(label, r);
    box.appendChild(row);
    say(u.throttle);
    box.appendChild(note);
  }

  if (u.kind === 'pump' || u.kind === 'inlet') {
    // A source is the only place a substance enters the machine, so this is
    // the one control that decides what the whole design is *about*.
    const want = p.ports[0].type;
    const sel = document.createElement('select');
    for (const sub of state.cat.substances.filter(x => x.domain === want)) {
      const o = document.createElement('option');
      o.value = sub.tag;
      o.textContent = sub.title;
      sel.appendChild(o);
    }
    sel.value = u.draws;
    sel.addEventListener('change', () => retune(u.name, { draws: sel.value }));
    field('draws', sel);
    const sub = state.cat.substances.find(x => x.tag === u.draws);
    if (sub && sub.hardness > 0) {
      box.appendChild(el('p', 'hint', `hardness ${sub.hardness} — a crusher rates 8`));
    }
  }

  if (u.kind === 'gearbox') {
    const sel = document.createElement('select');
    for (const r of [-8, -4, -2, 1, 2, 4, 8]) {
      const o = document.createElement('option');
      o.value = String(r);
      o.textContent = r === 1 ? 'straight through'
        : r < 0 ? `${-r}:1 up — faster, lighter`
        : `1:${r} down — slower, heavier`;
      sel.appendChild(o);
    }
    sel.value = String(u.ratio);
    sel.addEventListener('change', () => retune(u.name, { ratio: Number(sel.value) }));
    field('ratio', sel);
    const snap = state.snapshot && state.snapshot.units.find(x => x.name === u.name);
    if (snap && snap.detail) {
      box.appendChild(el('p', 'hint',
        `speed ${snap.detail.inSpeed} in, speed ${snap.detail.outSpeed} out`));
    }
  }

  if (u.kind === 'valve' || u.kind === 'clutch') {
    field(u.kind === 'valve' ? 'pass at most' : 'engage at', number('limit', 0, p.ports[0].rate));
    box.appendChild(el('p', 'hint', u.kind === 'valve'
      ? `of ${p.ports[0].rate}/tick`
      : 'it gathers this much, then passes everything until it is empty'));
  }

  if (u.kind === 'column') {
    field('stages', number('stages', 1, state.cat.constants.columnStages));
    const snap = state.snapshot && state.snapshot.units.find(x => x.name === u.name);
    if (snap && snap.detail && snap.detail.light !== undefined) {
      box.appendChild(el('p', 'hint',
        `per 10 fed: ${snap.detail.light} light, ${snap.detail.middle} middle, ` +
        `${snap.detail.heavy} heavy, for ${snap.detail.heatPerBatch} heat`));
    }
  }

  if (['tank', 'drum', 'flywheel', 'hopper'].includes(u.kind)) {
    const check = el('label', 'check');
    const cb = document.createElement('input');
    cb.type = 'checkbox';
    cb.checked = !!u.pulse;
    cb.addEventListener('change', () => retune(u.name, { pulse: cb.checked }));
    check.append(cb, document.createTextNode(' pulse instead of passing through'));
    box.appendChild(check);
    if (u.pulse) {
      field('fill to', number('high', 0, p.ports[0].cap));
      field('empty to', number('low', 0, p.ports[0].cap));
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
    if (live.carrying) why.appendChild(el('div', null, `carrying ${live.carrying}`));
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
  for (const w of m.waste || []) line('  waste', `${w.what} ${w.rate.value.toFixed(2)}/tick`);
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
  let s = `machine "${d.name}"\nbrief ${d.brief}\n\n`;
  for (const u of d.units) {
    s += `${u.kind.padEnd(9)} ${u.name.padEnd(wide)} at ${u.x},${u.y}`;
    if (u.kind === 'reactor' && u.throttle !== 100) s += `  throttle ${u.throttle}`;
    if (u.kind === 'pump' && u.draws !== 'water') s += `  draws ${u.draws}`;
    if (u.kind === 'inlet' && u.draws !== 'ore') s += `  draws ${u.draws}`;
    if (u.kind === 'gearbox' && u.ratio !== 4) s += `  ratio ${u.ratio}`;
    if ((u.kind === 'valve' || u.kind === 'clutch') && u.limit !== 100) s += `  limit ${u.limit}`;
    if (u.kind === 'column' && u.stages !== 2) s += `  stages ${u.stages}`;
    if (u.pulse) s += `  pulse ${u.high} ${u.low}`;
    s += '\n';
  }
  if (d.wires.length) s += '\n';
  for (const w of d.wires) s += `wire ${w.from}.${w.fromPort} -> ${w.to}.${w.toPort}\n`;
  return s;
}
