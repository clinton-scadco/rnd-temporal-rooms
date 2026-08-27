// Drawing a snapshot.
//
// The renderer never simulates. Every number on this canvas came out of the
// last snapshot: a wire's thickness is `flow / rate`, a component's tint is its
// utilisation, a tank's fill is its level. The only thing derived from the
// wall clock is the crawl of the dashes along a flowing wire, which is a
// decoration and is allowed to be one -- it carries no state and nothing reads
// it back.

import { state, part, box, toNum } from './doc.js';

/// One tile, in pixels at scale 1. The grid is the unit of footprint, so it is
/// the unit of the drawing too.
export const TILE = 26;

/// How far a port square sits inside its component's edge.
const PORT_INSET = 5;
/// How close a click has to be to count as landing on one. Smaller than the
/// inset gap between two touching components, so each of a facing pair can be
/// grabbed on its own.
const PORT_GRAB = 7;

const STATUS = {
  RUNNING: '--accent',
  WARMING: '--signal',
  FILLING: '--fluid',
  STARVED: '--signal',
  VENTING: '--signal',
  BLOCKED: '--bad',
  STALLED: '--bad',
  // New in experiment 07, and the one worth spotting from across the canvas: a
  // component that is not short of anything and is refusing what it was given.
  REFUSED: '--bad',
  IDLE: '--muted',
};

let css = null;
export function colour(name) {
  if (!css) css = getComputedStyle(document.documentElement);
  return css.getPropertyValue(name).trim();
}
export function portColour(type) { return colour('--' + type); }
export function statusColour(s) { return colour(STATUS[s] || '--muted'); }

// ---------------------------------------------------------------- layout

export function layout() {
  const out = new Map();
  for (const u of state.design.units) {
    const b = box(u);
    // Experiment 10: a component on an upper storey is drawn a little up and
    // to the left of the tiles it occupies, so that a stack reads as a stack
    // rather than as one component hiding another.
    //
    // A plan of a building with two floors in it has to do *something*, and
    // the two honest options are to draw one floor at a time or to offset
    // them. Offsetting keeps the whole machine on one page, which is what the
    // plan is for -- and the plan is no longer the only view, so it does not
    // have to carry the third dimension by itself any more.
    const up = (u.z || 0) * SHIFT;
    out.set(u.name, {
      u,
      level: u.z || 0,
      x: b.x * TILE - up, y: b.y * TILE - up,
      w: b.w * TILE, h: b.h * TILE,
      cx: (b.x + b.w / 2) * TILE - up, cy: (b.y + b.h / 2) * TILE - up,
    });
  }
  return out;
}

/// How far one storey shifts a footprint on the plan. A fifth of a tile: far
/// enough that two boxes on the same tiles are two boxes, near enough that the
/// plan is still a plan.
const SHIFT = 0.2 * 60;

/// Where port `i` of a component sits: inputs down the left edge, outputs down
/// the right. Hit testing uses this same function, so the square you click is
/// provably the square you saw.
export function portAt(boxes, name, i) {
  const b = boxes.get(name);
  if (!b) return null;
  const ports = part(b.u.kind).ports;
  const side = ports[i].dir === 'in' ? 'in' : 'out';
  const peers = ports.map((p, k) => ({ p, k })).filter(q => q.p.dir === ports[i].dir);
  const at = peers.findIndex(q => q.k === i);
  return {
    // Set in from the edge, because two components that touch would otherwise
    // put the facing pair of ports on exactly the same pixel -- one square
    // where there are two, and only ever the first of them clickable.
    x: side === 'in' ? b.x + PORT_INSET : b.x + b.w - PORT_INSET,
    y: b.y + (b.h * (at + 1)) / (peers.length + 1),
    port: ports[i],
    i,
    unit: b.u,
  };
}

export function eachPort(boxes, fn) {
  for (const b of boxes.values()) {
    const ports = part(b.u.kind).ports;
    for (let i = 0; i < ports.length; i++) fn(portAt(boxes, b.u.name, i));
  }
}

export function hitUnit(boxes, wx, wy) {
  let found = null;
  for (const b of boxes.values()) {
    if (wx < b.x || wx > b.x + b.w || wy < b.y || wy > b.y + b.h) continue;
    // Where two storeys overlap, the click lands on the higher one -- it is
    // the one drawn on top, and clicking what you can see is the whole
    // contract of a hit test.
    if (!found || b.level >= found.level) found = b;
  }
  return found;
}

export function hitPort(boxes, wx, wy) {
  let best = null, bd = PORT_GRAB;
  eachPort(boxes, p => {
    const d = Math.hypot(wx - p.x, wy - p.y);
    if (d < bd) { bd = d; best = p; }
  });
  return best;
}

/// The curve a connection is drawn along. Everything that needs to know where
/// a wire *is* -- drawing it, labelling it, clicking it -- goes through here.
function curve(a, b) {
  const dx = Math.max(30, Math.abs(b.x - a.x) * 0.5);
  return [a.x, a.y, a.x + dx, a.y, b.x - dx, b.y, b.x, b.y];
}

function pointOn(c, t) {
  const m = 1 - t;
  return {
    x: m * m * m * c[0] + 3 * m * m * t * c[2] + 3 * m * t * t * c[4] + t * t * t * c[6],
    y: m * m * m * c[1] + 3 * m * m * t * c[3] + 3 * m * t * t * c[5] + t * t * t * c[7],
  };
}

export function hitWire(boxes, wx, wy) {
  let best = -1, bd = 7;
  state.design.wires.forEach((w, i) => {
    const a = endpoints(boxes, w);
    if (!a) return;
    const c = curve(a.from, a.to);
    for (let k = 0; k <= 24; k++) {
      const p = pointOn(c, k / 24);
      const d = Math.hypot(wx - p.x, wy - p.y);
      if (d < bd) { bd = d; best = i; }
    }
  });
  return best;
}

export function endpoints(boxes, w) {
  const a = state.design.units.find(u => u.name === w.from);
  const b = state.design.units.find(u => u.name === w.to);
  if (!a || !b) return null;
  const ai = part(a.kind).ports.findIndex(p => p.name === w.fromPort);
  const bi = part(b.kind).ports.findIndex(p => p.name === w.toPort);
  if (ai < 0 || bi < 0) return null;
  const from = portAt(boxes, a.name, ai), to = portAt(boxes, b.name, bi);
  return from && to ? { from, to } : null;
}

// ------------------------------------------------------------------ draw

export function draw(ctx, view, ui) {
  const { width: W, height: H, dpr } = view;
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, W, H);
  ctx.fillStyle = colour('--ground');
  ctx.fillRect(0, 0, W, H);
  if (!state.cat) return;

  ctx.save();
  ctx.translate(view.ox, view.oy);
  ctx.scale(view.scale, view.scale);

  grid(ctx, view, W, H);

  const boxes = layout();
  const snap = state.snapshot;
  const byName = new Map();
  const wireFlow = new Map();
  if (snap) {
    for (const u of snap.units) byName.set(u.name, u);
    for (const w of snap.wires) wireFlow.set(key(w), w);
  }

  for (let i = 0; i < state.design.wires.length; i++) {
    wire(ctx, boxes, state.design.wires[i], wireFlow.get(key(state.design.wires[i])), ui, i);
  }
  for (const b of boxes.values()) unit(ctx, b, byName.get(b.u.name), ui);
  if (ui.wiring) pending(ctx, boxes, ui);
  if (ui.place) ghost(ctx, ui);

  ctx.restore();
}

const key = w => `${w.from}.${w.fromPort}>${w.to}.${w.toPort}`;

function grid(ctx, view, W, H) {
  const x0 = Math.floor(-view.ox / view.scale / TILE) * TILE;
  const y0 = Math.floor(-view.oy / view.scale / TILE) * TILE;
  const x1 = x0 + W / view.scale + TILE, y1 = y0 + H / view.scale + TILE;
  ctx.lineWidth = 1 / view.scale;
  ctx.strokeStyle = 'rgba(255,255,255,.03)';
  ctx.beginPath();
  for (let x = x0; x < x1; x += TILE) { ctx.moveTo(x, y0); ctx.lineTo(x, y1); }
  for (let y = y0; y < y1; y += TILE) { ctx.moveTo(x0, y); ctx.lineTo(x1, y); }
  ctx.stroke();
  // The origin, so that "0,0" in the file means somewhere in particular.
  ctx.strokeStyle = 'rgba(255,255,255,.10)';
  ctx.beginPath();
  ctx.moveTo(0, y0); ctx.lineTo(0, y1);
  ctx.moveTo(x0, 0); ctx.lineTo(x1, 0);
  ctx.stroke();
}

// ------------------------------------------------------------------ wires

function wire(ctx, boxes, w, flow, ui, index) {
  const e = endpoints(boxes, w);
  if (!e) return;
  const c = curve(e.from, e.to);
  const type = e.from.port.type;
  const rate = flow ? flow.rate : 1;
  const q = flow ? toNum(flow.flow) : 0;
  const frac = Math.min(1, q / Math.max(1, rate));
  const on = q > 0;
  const selected = state.selected && state.selected.what === 'wire' && state.selected.i === index;

  ctx.save();
  ctx.strokeStyle = portColour(type);
  ctx.globalAlpha = on ? 0.55 + 0.45 * frac : 0.22;
  ctx.lineWidth = 1.2 + 4 * frac;
  ctx.lineCap = 'round';
  if (on) {
    ctx.setLineDash([7, 6]);
    // Cosmetic only: nothing reads this back, and the snapshot does not know
    // it happened.
    ctx.lineDashOffset = -(ui.renderTime * 0.9) % 13;
  }
  ctx.beginPath();
  ctx.moveTo(c[0], c[1]);
  ctx.bezierCurveTo(c[2], c[3], c[4], c[5], c[6], c[7]);
  ctx.stroke();
  ctx.setLineDash([]);
  if (selected) {
    ctx.globalAlpha = 1;
    ctx.lineWidth = 1 + 4 * frac + 3;
    ctx.strokeStyle = colour('--ink');
    ctx.globalAlpha = 0.35;
    ctx.stroke();
  }
  ctx.restore();

  if (ui.flowLabels && flow) {
    const m = pointOn(c, 0.5);
    ctx.save();
    ctx.font = `9px ${mono()}`;
    ctx.textAlign = 'center';
    ctx.fillStyle = on ? portColour(type) : colour('--muted');
    ctx.globalAlpha = on ? 0.95 : 0.5;
    const label = `${q}/${rate}`;
    const wpx = ctx.measureText(label).width + 6;
    ctx.fillStyle = colour('--ground');
    ctx.globalAlpha = 0.85;
    ctx.fillRect(m.x - wpx / 2, m.y - 6, wpx, 11);
    ctx.globalAlpha = 1;
    ctx.fillStyle = on ? portColour(type) : colour('--muted');
    ctx.fillText(label, m.x, m.y + 3);
    ctx.restore();
  }
}

function pending(ctx, boxes, ui) {
  const from = portAt(boxes, ui.wiring.name, ui.wiring.port);
  if (!from) return;
  ctx.save();
  ctx.strokeStyle = portColour(from.port.type);
  ctx.setLineDash([5, 4]);
  ctx.lineWidth = 2;
  ctx.beginPath();
  ctx.moveTo(from.x, from.y);
  ctx.lineTo(ui.pointer.x, ui.pointer.y);
  ctx.stroke();
  ctx.restore();
}

// ------------------------------------------------------------- components

function unit(ctx, b, snap, ui) {
  const p = part(b.u.kind);
  const status = snap ? snap.status : 'IDLE';
  const col = statusColour(status);
  const util = snap ? snap.util / 100 : 0;
  const selected = state.selected && state.selected.what === 'unit' && state.selected.name === b.u.name;
  const bad = state.faults.some(f => f.unit === b.u.name);

  ctx.save();
  round(ctx, b.x + 1, b.y + 1, b.w - 2, b.h - 2, 3);
  ctx.fillStyle = colour('--panel');
  ctx.fill();

  // Utilisation, as a fill from the bottom. It is the one number worth seeing
  // without clicking anything.
  if (util > 0) {
    ctx.save();
    ctx.clip();
    ctx.globalAlpha = 0.20;
    ctx.fillStyle = col;
    ctx.fillRect(b.x, b.y + b.h - (b.h - 2) * util, b.w, (b.h - 2) * util);
    ctx.restore();
  }
  // A store shows what is actually in it, because that is its whole job -- and
  // in the colour of whatever domain it is holding.
  if (snap && snap.detail && snap.detail.cap && snap.detail.level !== undefined) {
    const f = snap.detail.level / snap.detail.cap;
    ctx.save();
    ctx.clip();
    ctx.globalAlpha = 0.32;
    ctx.fillStyle = portColour(p.ports[0].type);
    ctx.fillRect(b.x, b.y + b.h - (b.h - 2) * f, b.w, (b.h - 2) * f);
    ctx.restore();
  }

  ctx.lineWidth = selected ? 2 : 1.2;
  ctx.strokeStyle = bad ? colour('--bad') : selected ? colour('--ink') : col;
  ctx.globalAlpha = snap ? 1 : 0.5;
  ctx.stroke();
  ctx.globalAlpha = 1;

  // Text, only when there is room for it to be read.
  const scale = ui.scale || 1;
  if (b.h >= 40 && scale > 0.45) {
    ctx.fillStyle = colour('--ink');
    ctx.font = `600 11px ${ui.font}`;
    ctx.fillText(b.u.name, b.x + 7, b.y + 15);
    ctx.fillStyle = colour('--muted');
    ctx.font = `9px ${mono()}`;
    ctx.fillText(shortKind(b.u.kind), b.x + 7, b.y + 26);
    if (b.h >= 60) {
      ctx.fillStyle = col;
      ctx.fillText(status, b.x + 7, b.y + b.h - 8);
    }
  } else if (scale > 0.45) {
    ctx.fillStyle = colour('--ink');
    ctx.font = `600 10px ${ui.font}`;
    ctx.fillText(b.u.name, b.x + 6, b.y + b.h / 2 + 3.5);
  }
  ctx.restore();

  // Ports last, so nothing draws over them.
  const boxes = new Map([[b.u.name, b]]);
  for (let i = 0; i < p.ports.length; i++) {
    const pt = portAt(boxes, b.u.name, i);
    const live = snap ? snap.ports[i] : null;
    const wired = live ? live.wired : false;
    const lit = ui.wiring && ui.compatible && ui.compatible.has(`${b.u.name}.${i}`);
    ctx.save();
    ctx.beginPath();
    ctx.rect(pt.x - 4, pt.y - 4, 8, 8);
    ctx.fillStyle = wired || pt.port.external ? portColour(pt.port.type) : colour('--sunk');
    ctx.fill();
    ctx.lineWidth = lit ? 2 : 1;
    ctx.strokeStyle = lit ? colour('--ink') : portColour(pt.port.type);
    ctx.stroke();
    ctx.restore();
  }
}

function ghost(ctx, ui) {
  const p = part(ui.place);
  if (!p) return;
  ctx.save();
  ctx.globalAlpha = ui.placeOk ? 0.5 : 0.25;
  ctx.strokeStyle = ui.placeOk ? colour('--accent') : colour('--bad');
  ctx.setLineDash([4, 3]);
  ctx.lineWidth = 1.5;
  round(ctx, ui.placeAt.x * TILE + 1, ui.placeAt.y * TILE + 1, p.w * TILE - 2, p.h * TILE - 2, 3);
  ctx.stroke();
  ctx.restore();
}

function round(ctx, x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}

function mono() {
  return '"Cascadia Mono", "SF Mono", Consolas, monospace';
}

function shortKind(k) {
  return {
    heatpipe: 'heat pipe', steampipe: 'gas pipe', fluidpipe: 'fluid pipe',
    exchanger: 'exchanger', rollmill: 'rolling mill', separator: 'separator',
  }[k] || k;
}

// ------------------------------------------------------- the orbit strip

/// The waveform: output per tick across the transient and one full period.
///
/// This is the picture that makes "same average, different machine" visible,
/// so it draws the boundary between the two rather than a tidy curve.
export function drawWave(canvas, compiled, note) {
  const ctx = canvas.getContext('2d');
  const dpr = window.devicePixelRatio || 1;
  const r = canvas.getBoundingClientRect();
  canvas.width = Math.max(1, Math.round(r.width * dpr));
  canvas.height = Math.max(1, Math.round(r.height * dpr));
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, r.width, r.height);
  ctx.fillStyle = colour('--sunk');
  ctx.fillRect(0, 0, r.width, r.height);

  if (!compiled || !compiled.wave || !compiled.wave.length) {
    ctx.fillStyle = colour('--muted');
    ctx.font = `11px ${mono()}`;
    ctx.fillText(note || '', 10, r.height / 2);
    return;
  }

  const w = compiled.wave;
  const hi = Math.max(1, ...w);
  const pad = 8;
  const px = i => pad + (i * (r.width - pad * 2)) / Math.max(1, w.length - 1);
  const py = v => r.height - pad - (v / hi) * (r.height - pad * 2);

  // Where the startup stops and the loop begins.
  const stride = compiled.stride || 1;
  const boundary = compiled.settled ? compiled.transient / stride : -1;
  if (boundary >= 0 && boundary < w.length) {
    ctx.fillStyle = 'rgba(224,160,92,.10)';
    ctx.fillRect(px(boundary), 0, r.width - pad - px(boundary), r.height);
    ctx.strokeStyle = colour('--signal');
    ctx.setLineDash([3, 3]);
    ctx.beginPath();
    ctx.moveTo(px(boundary), 0);
    ctx.lineTo(px(boundary), r.height);
    ctx.stroke();
    ctx.setLineDash([]);
    ctx.fillStyle = colour('--signal');
    ctx.font = `9px ${mono()}`;
    ctx.fillText(`orbit: ${compiled.period} ticks, forever`, px(boundary) + 5, 12);
    ctx.fillText(`transient: ${compiled.transient} ticks`, pad + 2, 12);
  }

  ctx.strokeStyle = colour('--accent');
  ctx.lineWidth = 1.5;
  ctx.beginPath();
  w.forEach((v, i) => (i ? ctx.lineTo(px(i), py(v)) : ctx.moveTo(px(i), py(v))));
  ctx.stroke();

  ctx.fillStyle = colour('--muted');
  ctx.font = `9px ${mono()}`;
  ctx.fillText(`${hi} ${compiled.unit || ''}`.trim(), pad + 2, py(hi) - 3);
}
