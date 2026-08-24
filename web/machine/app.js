// Wiring. Everything interesting is in the other four files; this one decides
// when to ask the server a question, and moves one clock.

import {
  state, onChange, changed, catalogue, listDesigns, openDesign, save,
  seek, compile, verify, rename, setBrief, num, form,
} from './doc.js';
import * as plant from './form.js';
import { initCanvas, ui, invalidate, focusAll, setTool, select } from './canvas.js';
import {
  renderPalette, markTool, renderScore, renderInspector, renderHolding,
  renderMacro, renderWave, renderSource, renderFamilies, renderBriefPicker,
  renderBrief,
} from './panels.js';

const $ = s => document.querySelector(s);

// A machine settles in hundreds of ticks and then repeats forever, so the
// timeline reaches 10^9 to make the point that the far end is free. The slider
// is logarithmic; the number under it is not.
const MAX_TICK = 1e9;
const toTick = v => Math.round(Math.pow(10, (v / 1000) * 9) - 1);
const toSlider = t => Math.round((Math.log10(Math.max(0, t) + 1) / 9) * 1000);

const TICKS_PER_SECOND = 60;
let last = performance.now();
let lastAsk = 0;
let lastVerify = null;

async function main() {
  await catalogue();
  ui.font = getComputedStyle(document.body).fontFamily;

  initCanvas($('#c'), {
    onTool: markTool,
    onSay: hint,
  });
  const palette = () => renderPalette(kind => setTool(ui.place === kind ? null : kind));
  palette();
  renderFamilies(family => { state.family = family; palette(); });
  renderBriefPicker(tag => setBrief(tag));
  onChange(() => {
    if (state.dirty) seek(Math.floor(state.renderTime), true);
    refresh();
    invalidate();
    // Experiment 08 rebuilds on every edit rather than on a button, because
    // the property being demonstrated is reactivity: move a component and the
    // steel under it moves, in the same gesture.
    if (showing === 'form') refreshForm();
  });

  transport();
  fields();
  buttons();
  views();
  await designs();

  requestAnimationFrame(loop);
}

// -------------------------------------------------------------- transport

function transport() {
  $('#play').addEventListener('click', () => setPlaying(!state.playing));
  document.querySelectorAll('[data-step]').forEach(b => {
    b.addEventListener('click', () => {
      setPlaying(false);
      goto(Math.floor(state.renderTime) + Number(b.dataset.step));
    });
  });
  document.querySelectorAll('.speed').forEach(b => {
    b.addEventListener('click', () => {
      state.speed = Number(b.dataset.speed);
      document.querySelectorAll('.speed').forEach(x => x.classList.toggle('on', x === b));
    });
  });
  document.querySelector('.speed').classList.add('on');

  $('#seek').addEventListener('input', e => {
    setPlaying(false);
    goto(toTick(Number(e.target.value)));
  });

  window.addEventListener('keydown', e => {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT') return;
    if (e.code === 'Space') { setPlaying(!state.playing); e.preventDefault(); }
  });
}

function setPlaying(on) {
  state.playing = on;
  $('#play').textContent = on ? '❚❚' : '▶';
  $('#play').classList.toggle('on', on);
  last = performance.now();
}

function goto(tick) {
  tick = Math.max(0, Math.min(MAX_TICK, tick));
  clockTo(tick);
  seek(tick);
  invalidate();
}

function clockTo(t) {
  state.renderTime = t;
  ui.renderTime = t;
  $('#tick').textContent = num(Math.round(t));
  $('#seek').value = toSlider(t);
}

function loop(now) {
  const dt = Math.min(0.25, (now - last) / 1000);
  last = now;
  if (state.playing) {
    clockTo(state.renderTime + dt * TICKS_PER_SECOND * state.speed);
    if (state.renderTime >= MAX_TICK) { clockTo(MAX_TICK); setPlaying(false); }
    invalidate();
    // Unlike a factory, a machine has something new to show on almost every
    // tick, so the view asks as fast as the server will answer rather than
    // waiting for a scheduled event.
    if (now - lastAsk > 40) {
      lastAsk = now;
      seek(Math.floor(state.renderTime));
    }
  }
  requestAnimationFrame(loop);
}

// ----------------------------------------------------------------- panels

function refresh() {
  renderBrief();
  renderScore();
  renderInspector();
  renderHolding();
  renderSource();
  renderWave();
  renderMacro(lastVerify);
  if (document.activeElement !== $('#name')) $('#name').value = state.design.name;

  const e = $('#err');
  if (state.error) {
    e.hidden = false;
    e.textContent = state.error;
  } else {
    e.hidden = true;
  }

  // The one line that says the compilation is doing something: the tick the
  // player asked for, and the much smaller tick that answered it.
  const eq = $('#equiv');
  if (state.snapshot && state.equivalentTick !== state.tick) {
    eq.textContent = `t=${num(state.tick)} answered by simulating t=${num(state.equivalentTick)}`;
  } else {
    eq.textContent = '';
  }
}

function hint(text, bad) {
  const h = $('#hint');
  h.textContent = text || (ui.place ? `click the plot to put down a ${ui.place}` : 'pick a component, then click the plot');
  h.classList.toggle('bad', !!bad);
}

function fields() {
  $('#name').addEventListener('change', e => {
    const n = e.target.value.trim() || 'Machine';
    e.target.value = n;
    rename(n);
  });
  $('#showsrc').addEventListener('change', e => {
    $('#src').hidden = !e.target.checked;
    renderSource();
  });
  $('#showflow').addEventListener('change', e => {
    ui.flowLabels = e.target.checked;
    invalidate();
  });
}

function buttons() {
  $('#compile').addEventListener('click', async () => {
    lastVerify = null;
    const res = await compile();
    if (!res.ok) hint(res.error, true);
  });
  $('#verify').addEventListener('click', async () => {
    const res = await verify(Math.max(1000, Math.floor(state.renderTime)));
    lastVerify = res.ok ? res : null;
    if (!res.ok) hint(res.error, true);
    renderMacro(lastVerify);
  });
}

// ------------------------------------------- experiment 08: the two views

// Which of the two things the middle of the screen is: the document, or the
// plant the document builds. They are the same design and they are never out
// of step, because the second one is a pure function of the first.
let showing = 'plan';
let building = false;
let stale = false;

function views() {
  const to = async where => {
    showing = where;
    $('#viewplan').classList.toggle('on', where === 'plan');
    $('#viewform').classList.toggle('on', where === 'form');
    $('#c').hidden = where !== 'plan';
    $('#gl').hidden = where !== 'form';
    $('#formbar').hidden = where !== 'form';
    if (where !== 'form') return;
    if (!plant.ready() && !(await plant.initForm($('#gl')))) {
      hint('this browser has no WebGL 2, so there is nothing to draw the plant with', true);
      return;
    }
    await rebuild(true);
  };
  $('#viewplan').addEventListener('click', () => to('plan'));
  $('#viewform').addEventListener('click', () => to('form'));

  $('#formstyle').addEventListener('change', e => { plant.view.style = e.target.value; rebuild(true); });
  $('#formseed').addEventListener('change', e => { plant.view.seed = Number(e.target.value) || 0; rebuild(true); });
  $('#formlod').addEventListener('change', e => {
    plant.view.lod = Number(e.target.value) || 0;
    plant.invalidate();
    stats();
  });
}

/// Clicking a component is not an edit, so it does not rebuild a plant. It
/// only lights one up.
let lastDoc = '';
function refreshForm() {
  const doc = JSON.stringify(state.design);
  if (doc === lastDoc) {
    plant.pick(state.selected && state.selected.what === 'unit' ? state.selected.name : null);
    return;
  }
  lastDoc = doc;
  rebuild(false);
}

async function rebuild(refit) {
  if (showing !== 'form' || !plant.ready()) return;
  if (building) { stale = true; return; }
  building = true;
  lastDoc = JSON.stringify(state.design);
  const res = await form(plant.view.style, plant.view.seed);
  building = false;
  if (res.ok) {
    plant.show(res, refit);
    plant.pick(state.selected && state.selected.what === 'unit' ? state.selected.name : null);
    stats();
  } else {
    hint(res.error, true);
  }
  if (stale) { stale = false; rebuild(false); }
}

function stats() {
  const s = plant.view.stats;
  if (!s) return;
  const d = plant.drawn();
  $('#formstats').textContent =
    `${num(s.units)} components · ${num(s.runs)} runs, ${num(s.runMetres)} m · ` +
    `${num(s.pieces)} pieces from ${s.meshes} meshes · ` +
    `drawing ${num(d.instances)} in ${d.calls} calls · ${plant.view.shell} · ${plant.view.hash}`;
}

// ------------------------------------------------------------------ files

async function designs() {
  const names = await listDesigns();
  const sel = $('#open');
  for (const n of names) {
    const o = document.createElement('option');
    o.value = n;
    o.textContent = n.replace('.machine', '');
    sel.appendChild(o);
  }
  const open = async name => {
    await openDesign(name);
    select(null);
    goto(4000);
    setTimeout(focusAll, 0);
    await compile();
    // A different machine deserves a different camera.
    if (showing === 'form') await rebuild(true);
  };
  sel.addEventListener('change', async () => {
    if (!sel.value) return;
    const name = sel.value;
    sel.selectedIndex = 0;
    await open(name);
  });

  $('#save').addEventListener('click', async () => {
    const res = await save(state.design.name.toLowerCase().replace(/[^a-z0-9_-]+/g, '-'));
    hint(res.ok ? `saved ${res.path}` : res.error, !res.ok);
  });

  // `?design=03-compact&t=4000` names a state rather than a session, which a
  // deterministic simulator is allowed to do.
  const q = new URLSearchParams(location.search);
  const asked = q.get('design');
  const named = asked && names.find(n => n === asked || n === asked + '.machine' || n.includes(asked));
  const first = named || names.find(n => n.includes('compact')) || names[0];
  if (first) {
    await openDesign(first);
    if (q.has('src')) { $('#showsrc').checked = true; $('#src').hidden = false; }
    goto(Number(q.get('t')) || 4000);
    setTimeout(focusAll, 0);
    await compile();
  } else {
    changed(true);
  }
}

main();
