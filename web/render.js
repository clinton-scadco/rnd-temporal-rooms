// Drawing a snapshot.
//
// The only clock in this file is `renderTime`, which is a float and means
// nothing to the simulator. Everything that moves is derived from a pair of
// ticks the solver already knew:
//
//   a vehicle    progress = (renderTime - depart) / (arrive - depart)
//   a machine    progress = 1 - (deadline - renderTime) / duration
//
// No position is stored, integrated or synchronised. If the view is asked for
// a tick it has no snapshot for, the honest thing is to ask the solver, not to
// keep stepping something of its own.

import { state, toNum } from './doc.js';

export const NODE_W = 158;
export const NODE_H = 74;
const REGION_COLOURS = ['--r0', '--r1', '--r2', '--r3', '--r4', '--r5', '--r6', '--r7'];

let css = null;
function colour(name) {
  if (!css) css = getComputedStyle(document.documentElement);
  return css.getPropertyValue(name).trim();
}
export function regionColour(r) {
  return colour(REGION_COLOURS[(r ?? 0) % REGION_COLOURS.length]);
}

export function compact(n) {
  n = toNum(n);
  if (n < 1000) return String(n);
  const u = [[1e9, 'B'], [1e6, 'M'], [1e3, 'k']];
  for (const [d, s] of u) {
    if (n >= d) {
      const v = n / d;
      return (v >= 100 ? v.toFixed(0) : v.toFixed(1).replace(/\.0$/, '')) + s;
    }
  }
  return String(n);
}

// --------------------------------------------------------------- layout

/// Where everything is, in world coordinates. Shared with hit testing, so the
/// thing you click is provably the thing you saw.
export function layout() {
  const boxes = new Map();
  for (const n of state.graph.nodes) {
    boxes.set(n.name, {
      node: n,
      x: n.x, y: n.y,
      w: n.kind === 'link' ? 92 : NODE_W,
      h: n.kind === 'link' ? 40 : NODE_H,
      cx: n.x + (n.kind === 'link' ? 46 : NODE_W / 2),
      cy: n.y + (n.kind === 'link' ? 20 : NODE_H / 2),
    });
  }
  return boxes;
}

/// The little circle you drag a wire out of. Hit testing uses this same
/// function, so the target is exactly the thing that was drawn.
export function handleAt(b) {
  return { x: b.x + b.w, y: b.cy };
}

export function hit(boxes, wx, wy) {
  let found = null;
  for (const b of boxes.values()) {
    if (wx >= b.x && wx <= b.x + b.w && wy >= b.y && wy <= b.y + b.h) found = b;
  }
  return found;
}

/// Where a line between two boxes should stop, so arrows land on an edge
/// rather than in the middle of a name.
function edgePoint(from, to) {
  const dx = to.cx - from.cx, dy = to.cy - from.cy;
  const sx = from.w / 2 + 4, sy = from.h / 2 + 4;
  const s = Math.min(Math.abs(dx) > 1e-6 ? sx / Math.abs(dx) : 1e9,
                     Math.abs(dy) > 1e-6 ? sy / Math.abs(dy) : 1e9);
  return { x: from.cx + dx * s, y: from.cy + dy * s };
}

// ---------------------------------------------------------------- draw

export function draw(ctx, view, opts) {
  const { width: W, height: H, dpr } = view;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = colour('--ground');
  ctx.fillRect(0, 0, W, H);

  ctx.save();
  ctx.translate(view.ox, view.oy);
  ctx.scale(view.scale, view.scale);

  grid(ctx, view, W, H);

  const boxes = layout();
  const snap = state.snapshot;
  const byName = new Map();
  if (snap) {
    for (const s of snap.storages) byName.set(s.name, { kind: 'storage', d: s });
    for (const c of snap.classes) byName.set(c.name, { kind: 'class', d: c });
  }

  if (opts.overlay && snap) regions(ctx, boxes, byName, snap);
  wires(ctx, boxes);
  if (snap) rails(ctx, boxes, snap, opts);
  for (const b of boxes.values()) node(ctx, b, byName.get(b.node.name), opts, view);
  if (opts.wiringFrom) pending(ctx, boxes, opts);

  ctx.restore();
}

function grid(ctx, view, W, H) {
  const step = 40;
  const x0 = Math.floor(-view.ox / view.scale / step) * step;
  const y0 = Math.floor(-view.oy / view.scale / step) * step;
  const x1 = x0 + W / view.scale + step, y1 = y0 + H / view.scale + step;
  ctx.strokeStyle = 'rgba(255,255,255,.028)';
  ctx.lineWidth = 1 / view.scale;
  ctx.beginPath();
  for (let x = x0; x < x1; x += step) { ctx.moveTo(x, y0); ctx.lineTo(x, y1); }
  for (let y = y0; y < y1; y += step) { ctx.moveTo(x0, y); ctx.lineTo(x1, y); }
  ctx.stroke();
}

// ------------------------------------------------------------- regions

function regions(ctx, boxes, byName, snap) {
  const bounds = new Map();
  for (const b of boxes.values()) {
    const info = byName.get(b.node.name);
    const r = info && info.d.region;
    if (r === null || r === undefined) continue;
    const e = bounds.get(r) || { x0: 1e9, y0: 1e9, x1: -1e9, y1: -1e9 };
    e.x0 = Math.min(e.x0, b.x); e.y0 = Math.min(e.y0, b.y);
    e.x1 = Math.max(e.x1, b.x + b.w); e.y1 = Math.max(e.y1, b.y + b.h);
    bounds.set(r, e);
  }
  const pad = 26;
  for (const [r, e] of bounds) {
    const info = snap.regions[r];
    const col = regionColour(r);
    ctx.save();
    ctx.strokeStyle = col;
    ctx.globalAlpha = 0.5;
    ctx.setLineDash([7, 5]);
    ctx.lineWidth = 1.25;
    round(ctx, e.x0 - pad, e.y0 - pad - 16, e.x1 - e.x0 + pad * 2, e.y1 - e.y0 + pad * 2 + 16, 10);
    ctx.stroke();
    ctx.globalAlpha = 0.06;
    ctx.fillStyle = col;
    ctx.fill();
    ctx.restore();

    if (!info) continue;
    ctx.save();
    ctx.fillStyle = col;
    ctx.globalAlpha = 0.9;
    ctx.font = '600 11px Cascadia Mono, Consolas, monospace';
    const slack = info.slack === null ? 'slack unbounded' : 'slack ' + compact(info.slack);
    ctx.fillText(
      `region ${r}  ·  t=${toNum(info.clock).toLocaleString('en-GB')}  ·  ${slack}  ·  ${info.mode}`,
      e.x0 - pad + 4, e.y0 - pad - 22,
    );
    ctx.restore();
  }
}

// --------------------------------------------------------------- wires

function wires(ctx, boxes) {
  ctx.save();
  ctx.strokeStyle = 'rgba(200,225,215,.30)';
  ctx.fillStyle = 'rgba(200,225,215,.30)';
  ctx.lineWidth = 1.5;
  for (const e of state.graph.edges) {
    const a = boxes.get(e.from), b = boxes.get(e.to);
    if (!a || !b) continue;
    if (a.node.kind === 'link' || b.node.kind === 'link') continue;  // drawn as rails
    const p = edgePoint(a, b), q = edgePoint(b, a);
    ctx.beginPath();
    ctx.moveTo(p.x, p.y);
    ctx.lineTo(q.x, q.y);
    ctx.stroke();
    arrow(ctx, p, q);
  }
  ctx.restore();
}

function arrow(ctx, p, q) {
  const a = Math.atan2(q.y - p.y, q.x - p.x);
  const s = 7;
  ctx.beginPath();
  ctx.moveTo(q.x, q.y);
  ctx.lineTo(q.x - s * Math.cos(a - 0.4), q.y - s * Math.sin(a - 0.4));
  ctx.lineTo(q.x - s * Math.cos(a + 0.4), q.y - s * Math.sin(a + 0.4));
  ctx.closePath();
  ctx.fill();
}

function pending(ctx, boxes, opts) {
  const a = boxes.get(opts.wiringFrom);
  if (!a) return;
  ctx.save();
  ctx.strokeStyle = colour('--accent');
  ctx.setLineDash([5, 4]);
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  ctx.moveTo(a.cx, a.cy);
  ctx.lineTo(opts.pointer.x, opts.pointer.y);
  ctx.stroke();
  ctx.restore();
}

// --------------------------------------------------------------- rails
//
// A link is not a box with two wires. It is a track between two bays, and the
// vehicles on it are the four buckets of its class: waiting to load, in
// transit, waiting to unload, on the way home.

function railPath(boxes, link) {
  const from = state.graph.edges.find(e => e.to === link.name);
  const to = state.graph.edges.find(e => e.from === link.name);
  const a = from && boxes.get(from.from);
  const b = to && boxes.get(to.to);
  const m = boxes.get(link.name);
  if (!a || !b || !m) return null;
  return [{ x: a.cx, y: a.cy }, { x: m.cx, y: m.cy }, { x: b.cx, y: b.cy }];
}

function along(path, t, offset) {
  const segs = [];
  let total = 0;
  for (let i = 1; i < path.length; i++) {
    const d = Math.hypot(path[i].x - path[i - 1].x, path[i].y - path[i - 1].y);
    segs.push(d); total += d;
  }
  let want = Math.max(0, Math.min(1, t)) * total;
  for (let i = 0; i < segs.length; i++) {
    if (want <= segs[i] || i === segs.length - 1) {
      const f = segs[i] === 0 ? 0 : want / segs[i];
      const p = path[i], q = path[i + 1];
      const x = p.x + (q.x - p.x) * f, y = p.y + (q.y - p.y) * f;
      const a = Math.atan2(q.y - p.y, q.x - p.x);
      return { x: x + Math.sin(a) * offset, y: y - Math.cos(a) * offset, a };
    }
    want -= segs[i];
  }
  return { x: path[0].x, y: path[0].y, a: 0 };
}

function rails(ctx, boxes, snap, opts) {
  const now = opts.renderTime;
  for (const link of snap.links) {
    const node = state.graph.nodes.find(n => n.name === link.name);
    if (!node) continue;
    const path = railPath(boxes, node);
    if (!path) continue;

    ctx.save();
    ctx.strokeStyle = link.channel ? colour('--signal') : 'rgba(200,225,215,.35)';
    ctx.globalAlpha = link.channel ? 0.55 : 0.5;
    ctx.lineWidth = 2.5;
    ctx.beginPath();
    ctx.moveTo(path[0].x, path[0].y);
    for (let i = 1; i < path.length; i++) ctx.lineTo(path[i].x, path[i].y);
    ctx.stroke();
    ctx.restore();

    for (const f of link.flights) {
      const span = toNum(f.arrive) - toNum(f.depart);
      const p = span <= 0 ? 1 : (now - toNum(f.depart)) / span;
      // Loaded vehicles run out along the track; empty ones come back on a
      // parallel one, because the trip home is a real leg with its own time.
      const pos = f.loaded ? along(path, p, -5) : along(path, 1 - p, 5);
      // No count beside each leg: fifty wagons on one track means fifty labels
      // on top of each other, and the inspector lists every one exactly.
      vehicle(ctx, pos, f.loaded, 0, link);
    }
    // Vehicles with nowhere to go sit at the end they are stuck at.
    if (toNum(link.waitingToLoad) > 0) {
      vehicle(ctx, along(path, 0, -5), true, toNum(link.waitingToLoad), link, true);
    }
    if (toNum(link.waitingToUnload) > 0) {
      vehicle(ctx, along(path, 1, -5), true, toNum(link.waitingToUnload), link, true);
    }
  }
}

function vehicle(ctx, pos, loaded, n, link, stalled) {
  ctx.save();
  ctx.translate(pos.x, pos.y);
  ctx.rotate(pos.a);
  const w = 13, h = 7;
  ctx.beginPath();
  ctx.rect(-w / 2, -h / 2, w, h);
  if (loaded) {
    ctx.fillStyle = stalled ? colour('--bad') : colour('--accent');
    ctx.fill();
  } else {
    ctx.strokeStyle = colour('--signal');
    ctx.lineWidth = 1.4;
    ctx.stroke();
  }
  ctx.restore();
  if (n > 1) {
    ctx.save();
    ctx.fillStyle = colour('--muted');
    ctx.font = '9px Cascadia Mono, Consolas, monospace';
    ctx.fillText('x' + compact(n), pos.x + 8, pos.y - 6);
    ctx.restore();
  }
}

// ---------------------------------------------------------------- nodes

function node(ctx, b, info, opts, view) {
  const n = b.node;
  const sel = state.selected && state.selected.name === n.name;
  const region = info && info.d.region;
  const accent = region === null || region === undefined ? colour('--muted') : regionColour(region);

  ctx.save();
  ctx.fillStyle = colour('--panel');
  ctx.strokeStyle = sel ? colour('--accent') : colour('--rule');
  ctx.lineWidth = sel ? 2 : 1;
  round(ctx, b.x, b.y, b.w, b.h, n.kind === 'storage' ? 3 : 8);
  ctx.fill();
  ctx.stroke();

  // A stripe in the region's colour: the cheapest way to see a decomposition.
  ctx.fillStyle = accent;
  ctx.globalAlpha = 0.85;
  ctx.fillRect(b.x, b.y, 3, b.h);
  ctx.globalAlpha = 1;

  ctx.fillStyle = colour('--ink');
  ctx.font = '600 12px Segoe UI, system-ui, sans-serif';
  ctx.fillText(n.name, b.x + 10, b.y + 17);

  if (n.kind !== 'storage' && n.count > 1) {
    ctx.fillStyle = colour('--muted');
    ctx.font = '10px Cascadia Mono, Consolas, monospace';
    const label = '×' + compact(n.count);
    ctx.fillText(label, b.x + b.w - ctx.measureText(label).width - 9, b.y + 17);
  }

  ctx.fillStyle = colour('--muted');
  ctx.font = '9px Cascadia Mono, Consolas, monospace';
  ctx.fillText(n.kind, b.x + 10, b.y + b.h - 8);

  if (info && info.kind === 'storage') storageBody(ctx, b, info.d);
  else if (info && info.kind === 'class') classBody(ctx, b, info.d, opts, view);

  const h = handleAt(b);
  ctx.beginPath();
  ctx.arc(h.x, h.y, 4, 0, Math.PI * 2);
  ctx.fillStyle = colour('--ground');
  ctx.strokeStyle = sel ? colour('--accent') : colour('--rule');
  ctx.lineWidth = 1.5;
  ctx.fill();
  ctx.stroke();

  ctx.restore();
}

function storageBody(ctx, b, s) {
  const used = toNum(s.used), cap = toNum(s.capacity);
  const f = cap === 0 ? 0 : Math.min(1, used / cap);
  const x = b.x + 10, y = b.y + 26, w = b.w - 20, h = 8;
  ctx.fillStyle = colour('--sunk');
  ctx.fillRect(x, y, w, h);
  ctx.fillStyle = f > 0.98 ? colour('--bad') : colour('--accent');
  ctx.fillRect(x, y, w * f, h);
  ctx.strokeStyle = colour('--rule');
  ctx.lineWidth = 1;
  ctx.strokeRect(x + .5, y + .5, w - 1, h - 1);

  ctx.fillStyle = colour('--ink');
  ctx.font = '10px Cascadia Mono, Consolas, monospace';
  const held = s.held.filter(i => toNum(i.qty) > 0);
  const text = held.length
    ? held.map(i => compact(i.qty) + ' ' + i.item).join('  ')
    : 'empty';
  ctx.fillText(clip(ctx, text, b.w - 20), x, y + 22);

  ctx.fillStyle = colour('--muted');
  const pc = (f * 100).toFixed(f > 0 && f < 0.01 ? 2 : 0) + '%';
  ctx.fillText(pc, b.x + b.w - ctx.measureText(pc).width - 9, b.y + b.h - 8);
}

function classBody(ctx, b, c, opts, view) {
  const total = toNum(c.count) || 1;
  const busy = toNum(c.busy), idle = toNum(c.idle), blocked = toNum(c.blocked);
  const home = c.returning.reduce((a, r) => a + toNum(r.n), 0);
  const x = b.x + 10, y = b.y + 26, w = b.w - 20, h = 8;

  // Far away, a population is a bar: this many working, this many starved,
  // this many blocked. That is the whole state, whether it stands for four
  // machines or a billion.
  let cx = x;
  const seg = (n, col) => {
    const dw = (n / total) * w;
    if (dw <= 0) return;
    ctx.fillStyle = col;
    ctx.fillRect(cx, y, dw, h);
    cx += dw;
  };
  ctx.fillStyle = colour('--sunk');
  ctx.fillRect(x, y, w, h);
  seg(busy, colour('--accent'));
  seg(home, colour('--signal'));
  seg(idle, '#3B4A45');
  seg(blocked, colour('--bad'));
  ctx.strokeStyle = colour('--rule');
  ctx.lineWidth = 1;
  ctx.strokeRect(x + .5, y + .5, w - 1, h - 1);

  ctx.fillStyle = colour('--muted');
  ctx.font = '10px Cascadia Mono, Consolas, monospace';
  const util = ((busy / total) * 100).toFixed(0) + '%';
  ctx.fillText(util + ' busy', x, y + 22);

  if (opts.detail && view.scale > 0.9) machines(ctx, b, c, opts);
}

/// Close up, a population becomes machines again -- but only as pixels. The
/// states are dealt out in the proportions the snapshot reports and scattered
/// by a fixed permutation, so the floor looks like a floor and holds still
/// between frames. There is no object behind any of these.
function machines(ctx, b, c, opts) {
  const total = toNum(c.count);
  if (total <= 1) return;
  const cap = 1800;
  const shown = Math.min(total, cap);
  const cols = Math.ceil(Math.sqrt(shown * 2.2));
  const rows = Math.ceil(shown / cols);
  const cell = Math.min(5, Math.max(2, 110 / cols));
  const gw = cols * cell, gh = rows * cell;
  const x0 = b.x + b.w / 2 - gw / 2, y0 = b.y + b.h + 8;

  ctx.save();
  ctx.fillStyle = 'rgba(0,0,0,.35)';
  ctx.fillRect(x0 - 3, y0 - 3, gw + 6, gh + 6);

  const buckets = [];
  const push = (n, col) => { if (n > 0) buckets.push([Math.round((n / total) * shown), col]); };
  push(toNum(c.idle), '#3B4A45');
  push(toNum(c.blocked), colour('--bad'));
  for (const w of c.working) push(toNum(w.n), shade(colour('--accent'), phase(w, c, opts)));
  for (const r of c.returning) push(toNum(r.n), colour('--signal'));

  let k = 0;
  for (const [n, col] of buckets) {
    ctx.fillStyle = col;
    for (let i = 0; i < n && k < shown; i++, k++) {
      const p = scatter(k, shown);
      ctx.fillRect(x0 + (p % cols) * cell, y0 + Math.floor(p / cols) * cell, cell - 1, cell - 1);
    }
  }
  if (total > cap) {
    ctx.fillStyle = colour('--muted');
    ctx.font = '9px Cascadia Mono, Consolas, monospace';
    ctx.fillText(`${compact(total)} machines, ${cap} drawn`, x0, y0 + gh + 11);
  }
  ctx.restore();
}

/// How far through its cycle a working machine is, from the tick its cycle
/// ends and how long a cycle takes.
function phase(w, c, opts) {
  const d = toNum(c.duration) || 1;
  const left = toNum(w.at) - opts.renderTime;
  return Math.max(0, Math.min(1, 1 - left / d));
}

function shade(hex, t) {
  const n = parseInt(hex.replace('#', ''), 16);
  const f = 0.45 + 0.55 * t;
  const r = Math.round(((n >> 16) & 255) * f);
  const g = Math.round(((n >> 8) & 255) * f);
  const b = Math.round((n & 255) * f);
  return `rgb(${r},${g},${b})`;
}

/// A fixed odd-stride walk over the grid: every cell exactly once, in an order
/// that looks unstructured and never changes.
function scatter(i, n) {
  const stride = 2 * Math.floor(n / 7) + 1;
  return (i * stride + (i % 13)) % n;
}

// -------------------------------------------------------------- helpers

function round(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function clip(ctx, text, w) {
  if (ctx.measureText(text).width <= w) return text;
  let s = text;
  while (s.length > 1 && ctx.measureText(s + '…').width > w) s = s.slice(0, -1);
  return s + '…';
}
