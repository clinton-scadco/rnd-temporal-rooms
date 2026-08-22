// Camera and pointer. The canvas owns no factory state either -- it owns a
// viewport, a selection and whichever drag is half finished.

import { state, place, moveNode, nodeOf, canWire, connect, removeNode, undoLast, onChange } from './doc.js';
import { draw, layout, hit, handleAt } from './render.js';

export const view = { ox: 40, oy: 40, scale: 1, width: 0, height: 0, dpr: 1 };
export const ui = { place: null, overlay: true, detail: false, renderTime: 0, wiringFrom: null, pointer: { x: 0, y: 0 } };

let canvas, ctx, drag = null;
let needsDraw = true;

export function initCanvas(el, hooks) {
  canvas = el;
  ctx = canvas.getContext('2d');
  resize();
  new ResizeObserver(resize).observe(canvas.parentElement);

  canvas.addEventListener('pointerdown', e => {
    canvas.setPointerCapture(e.pointerId);
    const w = toWorld(e);
    const boxes = layout();
    const b = hit(boxes, w.x, w.y);

    if (ui.place) {
      const kind = ui.place;
      ui.place = null;
      // The node does not exist until the server agrees that it does, so the
      // selection is by name rather than by object.
      const proposed = place(kind, Math.round(w.x / 20) * 20 - 70, Math.round(w.y / 20) * 20 - 30);
      select(proposed.name);
      hooks.onPlaced && hooks.onPlaced();
      return;
    }

    if (b) {
      const h = handleAt(b);
      if (Math.hypot(w.x - h.x, w.y - h.y) < 9 || e.shiftKey) {
        ui.wiringFrom = b.node.name;
        drag = { kind: 'wire' };
        select(b.node.name);
        return;
      }
      select(b.node.name);
      drag = { kind: 'node', name: b.node.name, dx: w.x - b.node.x, dy: w.y - b.node.y, moved: false };
      return;
    }

    select(null);
    drag = { kind: 'pan', x: e.clientX, y: e.clientY, ox: view.ox, oy: view.oy };
  });

  canvas.addEventListener('pointermove', e => {
    const w = toWorld(e);
    ui.pointer = w;
    if (!drag) {
      const b = hit(layout(), w.x, w.y);
      canvas.style.cursor = ui.place ? 'crosshair' : b ? 'grab' : 'default';
      if (ui.wiringFrom) invalidate();
      return;
    }
    if (drag.kind === 'pan') {
      view.ox = drag.ox + (e.clientX - drag.x);
      view.oy = drag.oy + (e.clientY - drag.y);
      invalidate();
    } else if (drag.kind === 'node') {
      // Moving is not a structural edit: the plant is the same plant wherever
      // it is drawn, so it does not become a command and does not recompile.
      drag.moved = true;
      moveNode(drag.name, Math.round((w.x - drag.dx) / 10) * 10, Math.round((w.y - drag.dy) / 10) * 10);
      invalidate();
    } else if (drag.kind === 'wire') {
      invalidate();
    }
  });

  canvas.addEventListener('pointerup', e => {
    const w = toWorld(e);
    if (drag && drag.kind === 'wire') {
      const b = hit(layout(), w.x, w.y);
      const from = nodeOf(ui.wiringFrom);
      if (b && canWire(from, b.node)) connect(from, b.node);
      else if (b && b.node !== from) hooks.onRefused && hooks.onRefused(from, b.node);
      ui.wiringFrom = null;
    }
    drag = null;
    invalidate();
    hooks.onChanged && hooks.onChanged();
  });

  canvas.addEventListener('wheel', e => {
    e.preventDefault();
    const w = toWorld(e);
    const k = Math.exp(-e.deltaY * 0.0012);
    const next = Math.max(0.12, Math.min(4, view.scale * k));
    // Zoom about the pointer, so the thing under the cursor stays there.
    view.ox -= (w.x * next - w.x * view.scale);
    view.oy -= (w.y * next - w.y * view.scale);
    view.scale = next;
    invalidate();
  }, { passive: false });

  window.addEventListener('keydown', e => {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT') return;
    if ((e.key === 'Delete' || e.key === 'Backspace') && state.selected) {
      removeNode(state.selected.name);
      select(null);
      e.preventDefault();
    } else if (e.key === 'z' && (e.ctrlKey || e.metaKey)) {
      undoLast();
      e.preventDefault();
    } else if (e.key === 'Escape') {
      ui.place = null;
      ui.wiringFrom = null;
      invalidate();
    }
  });

  onChange(invalidate);
  requestAnimationFrame(frame);
}

let selectHook = null;
export function onSelect(fn) { selectHook = fn; }

function select(name) {
  state.selected = name ? { name } : null;
  invalidate();
  selectHook && selectHook();
}

export function focusAll() {
  const ns = state.graph.nodes;
  if (!ns.length) return;
  const x0 = Math.min(...ns.map(n => n.x)) - 60, x1 = Math.max(...ns.map(n => n.x)) + 220;
  const y0 = Math.min(...ns.map(n => n.y)) - 80, y1 = Math.max(...ns.map(n => n.y)) + 140;
  const s = Math.min(view.width / (x1 - x0), view.height / (y1 - y0), 1.6);
  view.scale = Math.max(0.12, s);
  view.ox = -x0 * view.scale + (view.width - (x1 - x0) * view.scale) / 2;
  view.oy = -y0 * view.scale + (view.height - (y1 - y0) * view.scale) / 2;
  invalidate();
}

export function invalidate() { needsDraw = true; }

function frame() {
  if (needsDraw) {
    needsDraw = false;
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
