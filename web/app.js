// Wiring. Everything interesting is in the other four files; this one decides
// when to ask the solver a question.

import {
  state, seek, onChange, listPlants, openPlant, openScenario, savePlant, fetchTimetable,
  addItem, setPlantName, setDeploy, num,
} from './doc.js';
import { initCanvas, onSelect, ui, invalidate, focusAll } from './canvas.js';
import { renderPlant, renderItems, renderInspector, drawTimetable } from './panels.js';
import { renderMission, renderConstraints, renderScrap, renderLog, initVerify } from './play.js';

const $ = s => document.querySelector(s);

// A timeline that reaches 10^9 without making the first thousand ticks
// invisible. The slider is logarithmic; the number under it is not.
const MAX_TICK = 1e9;
const toTick = v => Math.round(Math.pow(10, (v / 1000) * 9) - 1);
const toSlider = t => Math.round((Math.log10(Math.max(0, t) + 1) / 9) * 1000);

// `x1` is a tick every frame at 60fps, which makes a 60-tick cycle take a
// second. Everything above that is the same simulation, asked less often.
const TICKS_PER_SECOND = 60;
let last = performance.now();
let lastAsk = 0;
let timetable = null;
let timetableFor = -1;

function main() {
  initCanvas($('#c'), {
    onPlaced: () => { setTool(null); refreshPanels(); },
    onChanged: refreshPanels,
    onRefused: (a, b) => {
      // The compiler would say this too, but not until you had drawn it.
      hint(a.kind === 'storage'
        ? `${a.name} and ${b.name} are both storages — put a machine between them`
        : `${a.name} and ${b.name} are both machines — route them through a storage`, true);
    },
  });
  onSelect(renderInspector);
  onChange(() => {
    // An edit to the plant is a question the solver has not been asked yet.
    if (state.dirty) { seek(Math.floor(ui.renderTime), true); return; }
    // While playing, the view is deliberately ahead of the last answer; only
    // a paused view snaps back to the tick it was told about.
    if (!state.playing && state.snapshot) {
      clockTo(Number(state.snapshot.tick));
    }
    refreshPanels();
    invalidate();
    // A settled view is one that can afford to ask the scheduler what it did.
    // A playing one cannot: the answer would change every frame.
    if (!state.playing && state.snapshot) maybeTimetable(Number(state.snapshot.tick));
  });

  palette();
  transport();
  plantFields();
  plants();
  toggles();
  editMode();
  initVerify();

  requestAnimationFrame(loop);
}

// ------------------------------------------------------------------ tools

function palette() {
  document.querySelectorAll('[data-place]').forEach(b => {
    b.addEventListener('click', () => setTool(ui.place === b.dataset.place ? null : b.dataset.place));
  });
  $('#additem').addEventListener('submit', e => {
    e.preventDefault();
    const input = e.target.querySelector('input');
    const name = input.value.trim().replace(/[^A-Za-z0-9_]/g, '');
    if (name && !state.graph.items.includes(name)) addItem(name);
    input.value = '';
  });
}

function setTool(kind) {
  ui.place = kind;
  document.querySelectorAll('[data-place]').forEach(b => b.classList.toggle('on', b.dataset.place === kind));
  $('#c').classList.toggle('placing', !!kind);
  hint(kind ? `click the canvas to place a ${kind}` : 'click a tool, then the canvas');
}

function hint(text, bad) {
  const el = $('#hint');
  el.textContent = text;
  el.style.color = bad ? 'var(--bad)' : '';
}

// -------------------------------------------------------------- transport

function transport() {
  $('#play').addEventListener('click', () => setPlaying(!state.playing));
  document.querySelectorAll('[data-step]').forEach(b => {
    b.addEventListener('click', () => {
      setPlaying(false);
      goto(Number(state.snapshot ? state.snapshot.tick : 0) + Number(b.dataset.step));
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
    if (e.key === 'f') focusAll();
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
  $('#seek').value = toSlider(tick);
  $('#tick').textContent = num(Math.round(tick));
  seek(tick);
  invalidate();
}

// ------------------------------------------------------------------- loop
//
// There is one clock, and both the canvas and the document have to be looking
// at it. `ui.renderTime` is where the view is; `state.renderTime` is the tick
// a new command would land on. They were two fields once, which is a bug
// waiting for its first player to place a machine at the wrong moment.

function clockTo(t) {
  ui.renderTime = t;
  state.renderTime = t;
  $('#tick').textContent = num(Math.round(t));
  $('#seek').value = toSlider(t);
}


function loop(now) {
  const dt = Math.min(0.25, (now - last) / 1000);
  last = now;

  if (state.playing) {
    clockTo(ui.renderTime + dt * TICKS_PER_SECOND * state.speed);
    if (ui.renderTime >= MAX_TICK) { clockTo(MAX_TICK); setPlaying(false); }
    invalidate();

    // Interpolating past the next event would be making things up, so that is
    // where the view stops guessing and asks. Between events it is exact.
    const snap = state.snapshot;
    const stale = !snap
      || (snap.nextEvent !== null && ui.renderTime >= Number(snap.nextEvent))
      || ui.renderTime - Number(snap.tick) > 5000;
    if (stale && now - lastAsk > 60) {
      lastAsk = now;
      seek(Math.floor(ui.renderTime));
    }
  }

  drawTimetable($('#tt'), timetable, ui.renderTime, timetableNote());
  requestAnimationFrame(loop);
}

/// Why there is nothing to draw, which is never the same reason twice.
function timetableNote() {
  const regions = state.plant ? state.plant.regions : 0;
  if (!state.plant) return 'no plant compiled';
  if (regions < 2) return 'one region, one clock — there is nothing to schedule';
  const t = Math.floor(ui.renderTime);
  if (t < 1) return 'nothing has happened yet — move the clock';
  if (t > 200000) return `the run to t=${num(t)} makes more advances than are worth drawing`;
  return 'asking the scheduler…';
}

/// The timetable is a whole second run of the scheduler, so it is asked for
/// once the view has settled rather than on every frame of a drag -- and never
/// for a horizon whose answer nobody could read.
async function maybeTimetable(t) {
  t = Math.floor(t);
  if (t === timetableFor) return;
  timetableFor = t;
  if (t < 1 || t > 200000) { timetable = null; return; }
  const got = await fetchTimetable(t);
  // A slower answer to an older question is not an answer.
  if (timetableFor === t) timetable = got;
}

// ------------------------------------------------------------------ panels

function refreshPanels() {
  if (document.activeElement !== $('#plantname')) $('#plantname').value = state.graph.name;
  if (document.activeElement !== $('#deploy')) $('#deploy').value = state.graph.deploy;
  renderPlant();
  renderItems();
  renderInspector();
  renderMission();
  renderConstraints();
  renderScrap();
  renderLog();
  const e = $('#err');
  // A refusal is about a command that is no longer on the log, so it outranks
  // a compile error about a plant that is: it is the more recent news.
  if (state.refused) {
    e.hidden = false;
    e.textContent = 'refused — ' + state.refused;
  } else if (state.error) {
    e.hidden = false;
    const where = state.error.at !== null && state.error.at !== undefined
      ? `t=${num(state.error.at)}` : state.error.line ? `line ${state.error.line}` : '';
    e.textContent = [where, state.error.node, state.error.error].filter(Boolean).join(' · ');
  } else {
    e.hidden = true;
  }
  const src = $('#src');
  if (!src.hidden) src.textContent = state.source || '';
}

/// The two facts about a plant that are not about any one node: what it is
/// called, and how many of it there are.
function plantFields() {
  $('#plantname').addEventListener('change', e => {
    const name = e.target.value.trim().replace(/[^A-Za-z0-9_]/g, '') || 'Sketch';
    e.target.value = name;
    setPlantName(name);
  });
  $('#deploy').addEventListener('change', e => {
    const n = Math.max(1, Math.round(Number(e.target.value) || 1));
    e.target.value = n;
    setDeploy(n);
  });
}

/// Where an edit lands. The whole of the difference between designing a
/// factory and playing one is which tick a command carries.
function editMode() {
  const box = $('#liveedits');
  const show = () => {
    box.checked = state.liveEdits;
    $('#editwhen').textContent = state.liveEdits
      ? 'commands land on the clock'
      : 'commands land at tick 0';
  };
  box.addEventListener('change', e => { state.liveEdits = e.target.checked; show(); });
  onChange(show);
  show();
}

function toggles() {
  $('#overlay').addEventListener('change', e => { ui.overlay = e.target.checked; invalidate(); });
  $('#detail').addEventListener('change', e => { ui.detail = e.target.checked; invalidate(); });
  $('#showsrc').addEventListener('change', e => {
    $('#src').hidden = !e.target.checked;
    $('#src').textContent = state.source || '';
  });
}

// ------------------------------------------------------------------ files

async function plants() {
  const list = await listPlants();
  const sel = $('#open');
  const group = (label, names) => {
    if (!names.length) return;
    const g = document.createElement('optgroup');
    g.label = label;
    names.forEach(n => {
      const o = document.createElement('option');
      o.value = n;
      o.textContent = n.replace('.factory', '');
      g.appendChild(o);
    });
    sel.appendChild(g);
  };
  group('scenarios', list.scenarios || []);
  group('configs', list.configs || []);
  group('sketches', list.sketches || []);

  const open = async name => {
    if (name.endsWith('.scenario')) await openScenario(name);
    else await openPlant(name);
    goto(0);
    setTimeout(focusAll, 0);
  };

  sel.addEventListener('change', async () => {
    if (!sel.value) return;
    const name = sel.value;
    sel.selectedIndex = 0;
    await open(name);
  });

  $('#save').addEventListener('click', async () => {
    const res = await savePlant(state.graph.name.toLowerCase());
    hint(res.ok ? `saved ${res.path}` : res.error, !res.ok);
  });

  // `?plant=11-railchain&t=8000&detail=1` opens a particular plant at a
  // particular tick. A deterministic simulator makes that a real address:
  // the link names a state, not a session.
  const q = new URLSearchParams(location.search);
  const all = (list.scenarios || []).concat(list.configs || [], list.sketches || []);
  const asked = q.get('scenario') || q.get('plant');
  const named = asked && all
    .find(n => n === asked || n === asked + '.factory' || n === asked + '.scenario' || n.includes(asked));
  const first = named
    || (list.configs || []).find(n => n.includes('railchain'))
    || (list.configs || [])[0];
  if (first) {
    if (first.endsWith('.scenario')) await openScenario(first);
    else await openPlant(first);
    if (q.has('detail')) { $('#detail').checked = true; ui.detail = true; }
    if (q.has('source')) { $('#showsrc').checked = true; $('#src').hidden = false; }
    if (q.has('overlay')) { $('#overlay').checked = q.get('overlay') !== '0'; ui.overlay = $('#overlay').checked; }
    goto(Number(q.get('t')) || 0);
    setTimeout(focusAll, 0);
  }
}

main();
