// Camera and pointer. The canvas owns a viewport, a selection and whichever
// drag is half finished, and nothing else.
//
// One rule worth stating: a component is placed, moved and wired in *tiles*.
// The pixel positions in `render` are a presentation of the tile grid, never
// the other way round, because tiles are what the footprint metric counts.

import {
  state, part, place, move, remove, connect, unwire, overlaps,
  wireProblem, firstCompatible, unitOf, changed,
} from './doc.js';
import { TILE, draw, layout, hitUnit, hitPort, hitWire } from './render.js';

export const view = { ox: 60, oy: 40, scale: 1, width: 0, height: 0, dpr: 1 };
export const ui = {
  place: null,
  placeAt: { x: 0, y: 0 },
  placeOk: true,
  wiring: null,          // { name, port }
  compatible: null,      // Set of "unit.portIndex" a pending wire could land on
  pointer: { x: 0, y: 0 },
  renderTime: 0,
  scale: 1,
  flowLabels: true,
  // Experiment 10: which storey the plan is being drawn on. The plan view is
  // still a plan -- it shows the whole machine at once -- but a new component
  // is placed on this level, so a player who has climbed to the mezzanine in
  // the 3D view goes on adding to it from here.
  level: 0,
  font: '"Segoe UI", system-ui, sans-serif',
};

let canvas, ctx, drag = null, hooks = {}, needsDraw = true;

export function initCanvas(el, h) {
  canvas = el;
  hooks = h || {};
  ctx = canvas.getContext('2d');
  resize();
  new ResizeObserver(resize).observe(canvas.parentElement);

  canvas.addEventListener('pointerdown', e => {
    canvas.setPointerCapture(e.pointerId);
    // A canvas cannot take focus, so without this the focus stays wherever it
    // last was -- typically a slider in the inspector, which freezes the
    // inspector and silently disables every keyboard shortcut, because the
    // keydown handler below ignores anything aimed at an input.
    if (document.activeElement && document.activeElement !== document.body) {
      document.activeElement.blur();
    }
    const w = toWorld(e);
    const boxes = layout();

    if (ui.place) {
      const at = tileFor(ui.place, w);
      if (overlaps({ kind: ui.place, face: null }, at.x, at.y, ui.level)) {
        say('there is something there already', true);
        return;
      }
      const u = place(ui.place, at.x, at.y, ui.level);
      setTool(null);
      select({ what: 'unit', name: u.name });
      return;
    }

    const act = pick(boxes, w.x, w.y);
    if (act.hint) say(act.hint, true);
    if (act.what === 'wire-from') {
      startWiring(act.name, act.port);
      return;
    }
    if (act.what === 'unit') {
      select({ what: 'unit', name: act.name });
      if (act.drag) {
        const b = boxes.get(act.name);
        drag = {
          kind: 'unit', name: act.name,
          dx: w.x / TILE - b.u.x, dy: w.y / TILE - b.u.y,
          ox: b.u.x, oy: b.u.y,
        };
      }
      return;
    }
    if (act.what === 'wire') {
      select({ what: 'wire', i: act.i });
      return;
    }

    select(null);
    drag = { kind: 'pan', x: e.clientX, y: e.clientY, ox: view.ox, oy: view.oy };
  });

  canvas.addEventListener('pointermove', e => {
    const w = toWorld(e);
    ui.pointer = w;
    if (ui.place) {
      ui.placeAt = tileFor(ui.place, w);
      ui.placeOk = !overlaps({ kind: ui.place, face: null }, ui.placeAt.x, ui.placeAt.y, ui.level);
      invalidate();
    }
    if (!drag) {
      if (ui.wiring) invalidate();
      const boxes = layout();
      const p = hitPort(boxes, w.x, w.y);
      canvas.style.cursor = ui.place ? 'crosshair'
        : p ? 'alias'
        : hitUnit(boxes, w.x, w.y) ? 'grab'
        : 'default';
      return;
    }
    if (drag.kind === 'pan') {
      view.ox = drag.ox + (e.clientX - drag.x);
      view.oy = drag.oy + (e.clientY - drag.y);
      invalidate();
    } else if (drag.kind === 'unit') {
      const x = Math.max(0, Math.round(w.x / TILE - drag.dx));
      const y = Math.max(0, Math.round(w.y / TILE - drag.dy));
      const u = unitOf(drag.name);
      if (u && (u.x !== x || u.y !== y) && !overlaps(u, x, y, u.z || 0, u.name)) {
        move(drag.name, x, y);
      }
    }
  });

  canvas.addEventListener('pointerup', e => {
    const w = toWorld(e);
    if (ui.wiring) {
      finishWiring(w);
    }
    drag = null;
    invalidate();
  });

  canvas.addEventListener('wheel', e => {
    e.preventDefault();
    const w = toWorld(e);
    const k = Math.exp(-e.deltaY * 0.0012);
    const next = Math.max(0.25, Math.min(3, view.scale * k));
    view.ox -= w.x * next - w.x * view.scale;
    view.oy -= w.y * next - w.y * view.scale;
    view.scale = next;
    invalidate();
  }, { passive: false });

  window.addEventListener('keydown', e => {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT') return;
    if (e.key === 'Delete' || e.key === 'Backspace') {
      if (state.selected && state.selected.what === 'unit') remove(state.selected.name);
      else if (state.selected && state.selected.what === 'wire') unwire(state.selected.i);
      else return;
      select(null);
      e.preventDefault();
    } else if (e.key === 'Escape') {
      setTool(null);
      ui.wiring = null;
      ui.compatible = null;
      invalidate();
    } else if (e.key === 'f') {
      focusAll();
    }
  });

  requestAnimationFrame(frame);
}

/// What a click at this point in the world means.
///
/// Lifted out of the event handler because it is policy rather than plumbing,
/// and because the interesting case is easy to get wrong: a port square sits
/// *inside* its component's outline, so a click that lands on one has to do
/// something sensible about the component too. The first version returned
/// early on an input port and selected nothing, which made the whole left edge
/// of a turbine feel like a dead spot.
export function pick(boxes, wx, wy) {
  const p = hitPort(boxes, wx, wy);
  if (p && p.port.dir === 'out' && !p.port.external) {
    return { what: 'wire-from', name: p.unit.name, port: p.i };
  }
  if (p && p.port.dir === 'in') {
    // Wiring backwards is what everybody tries at least once. Say so, and
    // select anyway -- but do not start a drag, or nudging a port would move
    // the component instead of explaining it.
    return {
      what: 'unit',
      name: p.unit.name,
      drag: false,
      hint: 'start at an output and finish at an input',
    };
  }
  const b = hitUnit(boxes, wx, wy);
  if (b) return { what: 'unit', name: b.u.name, drag: true };
  const wi = hitWire(boxes, wx, wy);
  if (wi >= 0) return { what: 'wire', i: wi };
  return { what: 'pan' };
}

// ------------------------------------------------------------------ tools

export function setTool(kind) {
  ui.place = kind;
  canvas.classList.toggle('placing', !!kind);
  hooks.onTool && hooks.onTool(kind);
  invalidate();
}

function tileFor(kind, w) {
  const p = part(kind);
  return {
    x: Math.max(0, Math.round(w.x / TILE - p.w / 2)),
    y: Math.max(0, Math.round(w.y / TILE - p.h / 2)),
  };
}

function startWiring(name, port) {
  ui.wiring = { name, port };
  const from = unitOf(name);
  ui.compatible = new Set();
  for (const u of state.design.units) {
    part(u.kind).ports.forEach((q, i) => {
      if (!wireProblem(from, port, u, i)) ui.compatible.add(`${u.name}.${i}`);
    });
  }
  if (!ui.compatible.size) {
    say('nothing on the plot takes that, in reach', true);
  }
  invalidate();
}

function finishWiring(w) {
  const boxes = layout();
  const from = unitOf(ui.wiring.name);
  const fi = ui.wiring.port;
  ui.wiring = null;
  ui.compatible = null;
  let target = hitPort(boxes, w.x, w.y);
  let to = target && target.unit;
  let ti = target && target.i;
  if (!to) {
    const b = hitUnit(boxes, w.x, w.y);
    if (b) { to = b.u; ti = firstCompatible(from, fi, to); }
  }
  if (!to) return;
  const problem = wireProblem(from, fi, to, ti);
  if (problem) {
    say(problem, true);
    return;
  }
  connect(from, fi, to, ti);
  say(null);
}

function say(text, bad) {
  hooks.onSay && hooks.onSay(text, bad);
}

// -------------------------------------------------------------- selection

export function select(sel) {
  state.selected = sel;
  invalidate();
  changed(false);
}

export function focusAll() {
  const us = state.design.units;
  if (!us.length) return;
  const boxes = layout();
  let x0 = 1e9, y0 = 1e9, x1 = -1e9, y1 = -1e9;
  for (const b of boxes.values()) {
    x0 = Math.min(x0, b.x); y0 = Math.min(y0, b.y);
    x1 = Math.max(x1, b.x + b.w); y1 = Math.max(y1, b.y + b.h);
  }
  const pad = 40;
  const s = Math.min(view.width / (x1 - x0 + pad * 2), view.height / (y1 - y0 + pad * 2), 1.6);
  view.scale = Math.max(0.25, s);
  view.ox = -x0 * view.scale + (view.width - (x1 - x0) * view.scale) / 2;
  view.oy = -y0 * view.scale + (view.height - (y1 - y0) * view.scale) / 2;
  invalidate();
}

export function invalidate() { needsDraw = true; }

function frame() {
  if (needsDraw) {
    needsDraw = false;
    ui.scale = view.scale;
    draw(ctx, view, ui);
  }
  requestAnimationFrame(frame);
}

function resize() {
  const r = canvas.parentElement.getBoundingClientRect();
  view.dpr = window.devicePixelRatio || 1;
  view.width = r.width;
  view.height = r.height;
  canvas.width = Math.round(r.width * view.dpr);
  canvas.height = Math.round(r.height * view.dpr);
  invalidate();
}

function toWorld(e) {
  const r = canvas.getBoundingClientRect();
  return {
    x: (e.clientX - r.left - view.ox) / view.scale,
    y: (e.clientY - r.top - view.oy) / view.scale,
  };
}
