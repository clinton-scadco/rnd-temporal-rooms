// Wiring. Everything interesting is in the other four files; this one decides
// when to ask the server a question, and moves one clock.

import {
  state, onChange, changed, catalogue, listDesigns, openDesign, save,
  seek, compile, verify, rename, num,
} from './doc.js';
import { initCanvas, ui, invalidate, focusAll, setTool, select } from './canvas.js';
import {
  renderPalette, markTool, renderScore, renderInspector, renderHolding,
  renderMacro, renderWave, renderSource,
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
  renderPalette(kind => setTool(ui.place === kind ? null : kind));
  onChange(() => {
    if (state.dirty) seek(Math.floor(state.renderTime), true);
    refresh();
    invalidate();
  });

  transport();
  fields();
  buttons();
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
