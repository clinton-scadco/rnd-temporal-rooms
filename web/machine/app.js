// Wiring. Everything interesting is in the other four files; this one decides
// when to ask the server a question, and moves one clock.

import {
  state, onChange, changed, catalogue, listDesigns, openDesign, save,
  seek, compile, verify, rename, setBrief, num, form,
  move, lift, turn, unitOf, overlaps, part,
} from './doc.js';
import * as plant from './form.js';
import { initCanvas, ui, invalidate, focusAll, setTool, select } from './canvas.js';
import {
  renderPalette, markTool, renderScore, renderInspector, renderHolding,
  renderMacro, renderWave, renderSource, renderFamilies, renderBriefPicker,
  renderBrief, renderSpace,
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
  authoring();
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
    $('#formhelp').hidden = where !== 'form';
    // The spatial report is the plant's opinion of the plant, so it goes away
    // with the plant rather than sitting stale under the plan.
    if (where !== 'form') $('#space').replaceChildren();
    if (where === 'form') $('#gl').focus();
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
  // Experiment 09. Rebuilding at a different grade is the same design through
  // the same pipeline, so the camera stays exactly where it was: the point of
  // the control is to see one plant change its clothes, not to be shown four
  // plants.
  $('#formgrade').addEventListener('change', e => { plant.view.grade = e.target.value; rebuild(false); });
  $('#formlod').addEventListener('change', e => {
    plant.view.lod = Number(e.target.value) || 0;
    plant.invalidate();
    stats();
  });
  // Experiment 10: the clearance overlay. On by default, because the boxes are
  // what turn "the plant looks wrong" into "that one is red".
  const boxes = $('#formboxes');
  if (boxes) {
    boxes.checked = plant.view.boxes;
    boxes.addEventListener('change', e => {
      plant.view.boxes = e.target.checked;
      plant.invalidate();
    });
  }
}

// ------------------------------------------------------ experiment 10: 3D
//
// The plant view is an editor. What it is allowed to do to the document is
// exactly what the plan view is allowed to do, plus the two verbs the plan
// view has no axis for -- and every one of them goes through `doc.js`, so the
// 3D window still knows nothing about components.
function authoring() {
  plant.authoring({
    onPick: name => {
      select({ what: 'unit', name });
      // The plan follows the player upstairs: whatever they last touched in
      // the 3D view sets the storey a new component is placed on.
      const u = unitOf(name);
      if (u) ui.level = u.z || 0;
      refresh();
      invalidate();
    },
    // Metres to tiles. The pointer is over the middle of the machine, so the
    // footprint is centred on it rather than hung off its corner -- dragging
    // by a corner is a thing nobody has ever meant to do.
    tile: at => {
      const sel = state.selected;
      if (!sel || sel.what !== 'unit') return null;
      const u = unitOf(sel.name);
      if (!u) return null;
      const p = part(u.kind);
      const t = (u.face & 1) === 1;
      const w = t ? p.h : p.w, h = t ? p.w : p.h;
      return {
        x: Math.max(0, Math.round(at[0] / 2 - w / 2)),
        y: Math.max(0, Math.round(at[2] / 2 - h / 2)),
      };
    },
    onMove: (name, x, y) => {
      const u = unitOf(name);
      if (!u || (u.x === x && u.y === y)) return false;
      // Refused rather than clamped: a component that slides through the one
      // next to it and stops on the far side of it is a component the player
      // has lost track of.
      if (overlaps(u, x, y, u.z || 0, name)) return false;
      move(name, x, y);
      return true;
    },
    onLift: by => {
      const sel = state.selected;
      if (!sel || sel.what !== 'unit') return hint('pick a component to lift');
      if (!lift(sel.name, by)) {
        hint(by > 0 ? 'there is something in the way up there' : 'it is already on the slab');
      }
    },
    onTurn: by => {
      const sel = state.selected;
      if (!sel || sel.what !== 'unit') return hint('pick a component to turn');
      if (!turn(sel.name, by)) hint('it does not fit turned that way', true);
    },
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
  const res = await form(plant.view.style, plant.view.seed, plant.view.grade);
  building = false;
  if (res.ok) {
    plant.show(res, refit);
    plant.pick(state.selected && state.selected.what === 'unit' ? state.selected.name : null);
    stats();
    renderSpace();
  } else {
    hint(res.error, true);
  }
  if (stale) { stale = false; rebuild(false); }
}

function stats() {
  const s = plant.view.stats;
  if (!s) return;
  const d = plant.drawn();
  // Experiment 10 puts the routing result first, because it is the one number
  // on this line that can be *bad*: a plant with a connection in it that could
  // not be made is a plant with a hole in it, and the player should not have
  // to go looking.
  const routed = s.lost
    ? `${s.lost} of ${s.runs} runs have no valid route`
    : s.tight
      ? `${s.runs} runs, ${s.tight} of them tight`
      : `${s.runs} runs, all clean`;
  $('#formstats').textContent =
    `${num(s.units)} components on ${s.levels} storeys · ${routed}, ${num(s.runMetres)} m · ` +
    `${num(s.pieces)} pieces from ${s.meshes} meshes in ${s.mats} materials · ` +
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
