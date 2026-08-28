// The plot, drawn, and the four things a player may do to it.
//
//   place    ghost, rotation, snapping, collision, then one command
//   delete   one command, and a ghost left behind
//   connect  a wire between a bay and a machine
//   link     a transport between two bays, whose latency is the distance
//
// Nothing here moves anything. A committed installation has a position and an
// orientation and they are historical facts: wanting it elsewhere is a delete
// and a place, at two different ticks. That is the rule from section 4 of the
// brief, and the reason it is a rule is on the wire rather than on the screen
// -- a drag is a stream of positions with no canonical order, and a place is
// one command with one tick and one sequence number.

import * as net from './net.js';
import { menu, toast } from './panels.js';

const TILE = 7;

export const view = { ox: 20, oy: 20, scale: 1, w: 0, h: 0, dpr: 1 };
export const tool = { mode: 'pick', proto: null, face: 0, from: null, item: null, design: null };
export let selection = null;

let canvas = null, ctx = null, hover = null, need = true, onSelect = () => {};
let lastCursor = 0;

export function init(el, hooks) {
  canvas = el;
  ctx = el.getContext('2d');
  onSelect = hooks.onSelect || (() => {});
  addEventListener('resize', resize);
  resize();
  wire();
  requestAnimationFrame(frame);
}

export function invalidate() { need = true; }

/// `design` is set when what is being placed is a *copy* of a machine that is
/// already standing: every placed machine owns its design outright, so
/// duplicating one is a placement command carrying the design it had at the
/// moment the player pressed the button. Editing either copy afterwards does
/// nothing at all to the other.
export function setTool(mode, proto, design) {
  tool.mode = mode;
  tool.proto = proto || null;
  tool.design = design || null;
  tool.from = null;
  need = true;
}

export function select(id) {
  selection = id;
  onSelect(id);
  need = true;
}

function resize() {
  if (!canvas) return;
  const r = canvas.getBoundingClientRect();
  view.dpr = Math.min(2, devicePixelRatio || 1);
  view.w = r.width;
  view.h = r.height;
  canvas.width = Math.round(r.width * view.dpr);
  canvas.height = Math.round(r.height * view.dpr);
  need = true;
}

// ------------------------------------------------------------------ mapping

const sx = x => view.ox + x * TILE * view.scale;
const sy = y => view.oy + y * TILE * view.scale;
const wx = px => (px - view.ox) / (TILE * view.scale);
const wy = py => (py - view.oy) / (TILE * view.scale);

function at(e) {
  const r = canvas.getBoundingClientRect();
  return [wx(e.clientX - r.left), wy(e.clientY - r.top)];
}

function under(x, y) {
  for (const i of net.installs()) {
    if (x >= i.x && x < i.x + i.w && y >= i.y && y < i.y + i.h) return i;
  }
  return null;
}

/// How far a point is from a segment, in screen pixels.
function offSegment(px, py, ax, ay, bx, by) {
  const dx = bx - ax, dy = by - ay;
  const len = dx * dx + dy * dy;
  const t = len ? Math.max(0, Math.min(1, ((px - ax) * dx + (py - ay) * dy) / len)) : 0;
  return Math.hypot(px - (ax + dx * t), py - (ay + dy * t));
}

/// The transport or wire under the pointer, if any.
///
/// Lines are the only thing on this canvas a player can draw but could not,
/// until now, take back: `under` hit-tests boxes, so a haul was never
/// selectable and a wire had no identity to select. A mis-wired bay was
/// therefore permanent unless you deleted the building at one end of it.
/// Boxes still win the pick, and the tolerance is in pixels rather than tiles
/// so that it stays clickable at every zoom.
const NEAR = 6;
function lineUnder(x, y) {
  const v = net.state.view;
  if (!v) return null;
  const px = sx(x), py = sy(y);
  let best = null, near = NEAR;
  const test = (a, b, what) => {
    if (!a || !b) return;
    const [ax, ay] = centre(a), [bx, by] = centre(b);
    const d = offSegment(px, py, ax, ay, bx, by);
    if (d < near) { near = d; best = what; }
  };
  for (const h of v.world.hauls) test(net.byId(h.from), net.byId(h.to), h.id);
  for (const c of v.world.conns) test(net.byId(c.from), net.byId(c.to), net.wireKey(c));
  return best;
}

/// The size the held prototype would be, turned the way it is being held.
function held() {
  if (!tool.proto) return null;
  const p = net.proto(tool.proto);
  if (!p) return null;
  const [w, h] = tool.face & 1 ? [p.h, p.w] : [p.w, p.h];
  return { p, w, h };
}

function collides(x, y, w, h) {
  const cat = net.state.catalogue;
  const plot = cat ? cat.plot : 128;
  if (x < 0 || y < 0 || x + w > plot || y + h > plot) return true;
  return net.installs().some(i => x < i.x + i.w && i.x < x + w && y < i.y + i.h && i.y < y + h);
}

// ----------------------------------------------------------------- pointer

function wire() {
  canvas.addEventListener('pointermove', e => {
    const [x, y] = at(e);
    hover = { x: Math.floor(x), y: Math.floor(y), raw: [x, y] };
    need = true;
    const now = performance.now();
    if (now - lastCursor > 120) {
      lastCursor = now;
      net.presence({ x, y }, selection, null, 'world');
    }
    if (e.buttons & 4 || (e.buttons & 1 && e.shiftKey)) {
      view.ox += e.movementX;
      view.oy += e.movementY;
    }
  });
  canvas.addEventListener('pointerdown', e => {
    if (e.button !== 0 || e.shiftKey) return;
    const [x, y] = at(e);
    click(Math.floor(x), Math.floor(y), e, x, y);
  });
  canvas.addEventListener('wheel', e => {
    e.preventDefault();
    const r = canvas.getBoundingClientRect();
    const [bx, by] = [e.clientX - r.left, e.clientY - r.top];
    const before = [wx(bx), wy(by)];
    view.scale = Math.min(4, Math.max(0.35, view.scale * Math.exp(-e.deltaY * 0.0012)));
    view.ox = bx - before[0] * TILE * view.scale;
    view.oy = by - before[1] * TILE * view.scale;
    need = true;
  }, { passive: false });
  canvas.addEventListener('contextmenu', e => e.preventDefault());

  addEventListener('keydown', e => {
    if (e.target.tagName === 'INPUT' || e.target.tagName === 'SELECT') return;
    const k = e.key.toLowerCase();
    if (k === 'r' && tool.proto) { tool.face = (tool.face + (e.shiftKey ? 3 : 1)) % 4; need = true; }
    else if (k === 'escape') { setTool('pick'); select(null); }
    else if (k === 'delete' || k === 'backspace') { if (selection) remove(selection); }
  });
}

/// `x, y` is the tile that was clicked; `rx, ry` is where in it the pointer
/// actually was. A building occupies whole tiles and only wants the first, but
/// a line is a few pixels wide and rounding to a tile can throw the pick clean
/// past it at anything but the closest zoom.
function click(x, y, e, rx = x + .5, ry = y + .5) {
  const hit = under(x, y);
  if (tool.mode === 'place') return place(x, y);
  if (tool.mode === 'delete') {
    const target = hit ? hit.id : lineUnder(rx, ry);
    return target === null || target === undefined ? null : remove(target);
  }
  if (tool.mode === 'connect' || tool.mode === 'belt' || tool.mode === 'rail') {
    if (!hit) { tool.from = null; need = true; return; }
    if (!tool.from) { tool.from = hit.id; need = true; return; }
    const from = tool.from;
    tool.from = null;
    return join(from, hit.id, e);
  }
  const line = hit ? null : lineUnder(rx, ry);
  select(hit ? hit.id : (line === null || line === undefined ? null : line));
}

function place(x, y) {
  const h = held();
  if (!h) return;
  if (collides(x, y, h.w, h.h)) return toast('that does not fit there');
  const common = { proto: h.p.tag, x, y, face: tool.face, design: tool.design };
  if (h.p.role === 'storage') return net.send('PlaceStorage', common);
  if (h.p.choosesItem) {
    return menu('ships which item?', itemsOfInterest().map(i => ({
      label: i,
      pick: () => net.send('PlaceMachine', { ...common, item: i }),
    })));
  }
  net.send('PlaceMachine', common);
}

/// The items a delivery depot might be set to: whatever anything in this room
/// actually makes, with the ones the goal counts first.
///
/// Electricity is not among them, because it has its own sink -- a depot full
/// of megawatts would be a lorry full of lightning.
function itemsOfInterest() {
  const wanted = new Set();
  for (const l of ((net.state.view || {}).goal || { progress: { lines: [] } }).progress.lines) {
    for (const i of net.state.catalogue.items) {
      if (l.what.toLowerCase().includes(i.toLowerCase())) wanted.add(i);
    }
  }
  const made = new Set();
  for (const i of net.installs()) for (const m of i.makes) made.add(m);
  const skip = new Set(['Power', 'Heat', 'Torque', 'Stroke']);
  const out = [...net.state.catalogue.items].filter(i => !skip.has(i));
  out.sort((a, b) => rank(a, wanted, made) - rank(b, wanted, made) || a.localeCompare(b));
  return out;
}

function rank(item, wanted, made) {
  if (wanted.has(item)) return 0;
  if (made.has(item)) return 1;
  return 2;
}

function remove(id) {
  const wire = net.wireOf(id);
  if (wire) return net.send('DeleteConnection', wire);
  const i = net.byId(id);
  if (i) return net.send(i.role === 'storage' ? 'DeleteStorage' : 'DeleteMachine', { id });
  const h = (net.state.view.world.hauls || []).find(h => h.id === id);
  if (h) return net.send('DeleteWorldLink', { id });
}

/// Two things the player clicked, one after the other.
function join(fromId, toId, e) {
  const a = net.byId(fromId), b = net.byId(toId);
  if (!a || !b || a.id === b.id) return;
  if (tool.mode === 'connect') {
    if ((a.role === 'storage') === (b.role === 'storage')) {
      return toast(a.role === 'storage'
        ? 'two bays need a transport between them'
        : 'two machines need a bay between them');
    }
    const machine = a.role === 'storage' ? b : a;
    const items = a.role === 'storage' ? machine.wants : machine.makes;
    if (!items.length) return toast(`${machine.name} has nothing to wire there`);
    return choose(items, item => net.send('CreateConnection', { from: a.id, to: b.id, item }), e);
  }
  // A transport. Both ends must be bays, and the item must be something that
  // actually arrives at the loading end.
  if (a.role !== 'storage' || b.role !== 'storage') return toast('a transport runs between two bays');
  const items = arriving(a.id);
  if (!items.length) return toast(`nothing is delivered to ${a.name} yet`);
  choose(items, item => net.send('CreateWorldLink', { proto: tool.mode, from: a.id, to: b.id, item }), e);
}

/// What is delivered into one bay, according to the document.
function arriving(id) {
  const w = net.state.view.world;
  const items = new Set();
  for (const c of w.conns) if (c.to === id) items.add(c.item);
  for (const h of w.hauls) if (h.to === id) items.add(h.item);
  return [...items];
}

function choose(items, then) {
  if (items.length === 1) return then(items[0]);
  menu('which item?', items.map(i => ({ label: i, pick: () => then(i) })));
}

// -------------------------------------------------------------------- draw

function frame() {
  requestAnimationFrame(frame);
  if (!need || !ctx) return;
  need = false;
  draw();
}

const ROLE = {
  source: '--source', storage: '--storage', machine: '--machine',
  sink: '--sink', transport: '--belt',
};
const css = n => getComputedStyle(document.documentElement).getPropertyValue(n).trim();

export function draw() {
  const v = net.state.view;
  ctx.save();
  ctx.scale(view.dpr, view.dpr);
  ctx.clearRect(0, 0, view.w, view.h);
  ctx.fillStyle = css('--sunk');
  ctx.fillRect(0, 0, view.w, view.h);
  if (!v) { ctx.restore(); return; }

  grid();
  const w = v.world;
  for (const h of w.hauls) haul(h);
  for (const c of w.conns) conn(c);
  for (const i of w.installs) box(i);
  ghost();
  cursors();
  ctx.restore();
}

function grid() {
  const cat = net.state.catalogue;
  const plot = cat ? cat.plot : 128;
  const step = 8;
  ctx.strokeStyle = 'rgba(125,144,137,.10)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = 0; x <= plot; x += step) { ctx.moveTo(sx(x), sy(0)); ctx.lineTo(sx(x), sy(plot)); }
  for (let y = 0; y <= plot; y += step) { ctx.moveTo(sx(0), sy(y)); ctx.lineTo(sx(plot), sy(y)); }
  ctx.stroke();
  ctx.strokeStyle = 'rgba(125,144,137,.30)';
  ctx.strokeRect(sx(0), sy(0), plot * TILE * view.scale, plot * TILE * view.scale);
}

function centre(i) {
  return [sx(i.x + i.w / 2), sy(i.y + i.h / 2)];
}

function box(i) {
  const x = sx(i.x), y = sy(i.y);
  const w = i.w * TILE * view.scale, h = i.h * TILE * view.scale;
  const colour = css(ROLE[i.role] || '--muted');
  ctx.fillStyle = i.running ? colour + '28' : 'rgba(125,144,137,.10)';
  ctx.fillRect(x, y, w, h);
  ctx.lineWidth = selection === i.id ? 2 : 1;
  ctx.strokeStyle = selection === i.id ? css('--accent') : colour;
  ctx.strokeRect(x + .5, y + .5, w - 1, h - 1);

  // The status band: what the simulator thinks of it, in one stripe.
  const p = net.plantOf(i.name);
  if (p && p.why) {
    const st = p.why.state;
    ctx.fillStyle = st === 'running' ? css('--good')
      : st === 'blocked' ? css('--signal') : css('--bad');
    ctx.fillRect(x + 1, y + 1, w - 2, 2);
  } else if (!i.running) {
    ctx.fillStyle = css('--muted');
    ctx.fillRect(x + 1, y + 1, w - 2, 2);
  }
  if (i.editor !== null && i.editor !== undefined) {
    ctx.strokeStyle = css('--signal');
    ctx.setLineDash([3, 3]);
    ctx.strokeRect(x - 2.5, y - 2.5, w + 5, h + 5);
    ctx.setLineDash([]);
  }
  if (view.scale > 0.6) {
    ctx.fillStyle = css('--ink');
    ctx.font = `${Math.max(9, 10 * view.scale)}px var(--ui), sans-serif`;
    ctx.textBaseline = 'top';
    ctx.fillText(i.name, x + 4, y + 6);
    if (p && p.held && p.held.length) {
      ctx.fillStyle = css('--muted');
      ctx.fillText(p.held.map(q => `${short(q.qty)} ${q.item}`).join(' '), x + 4, y + 6 + 12 * view.scale);
    } else if (i.item) {
      ctx.fillStyle = css('--muted');
      ctx.fillText(i.item, x + 4, y + 6 + 12 * view.scale);
    }
  }
}

const short = n => n >= 1e6 ? (n / 1e6).toFixed(1) + 'M' : n >= 1e4 ? (n / 1e3).toFixed(0) + 'k' : String(n);

function conn(c) {
  const a = net.byId(c.from), b = net.byId(c.to);
  if (!a || !b) return;
  const on = selection === net.wireKey(c);
  const [ax, ay] = centre(a), [bx, by] = centre(b);
  ctx.strokeStyle = on ? css('--accent') : 'rgba(125,144,137,.5)';
  ctx.lineWidth = on ? 3 : 1;
  ctx.beginPath();
  ctx.moveTo(ax, ay);
  ctx.lineTo(bx, by);
  ctx.stroke();
  arrow(ax, ay, bx, by, 'rgba(125,144,137,.7)');
}

function haul(h) {
  const a = net.byId(h.from), b = net.byId(h.to);
  if (!a || !b) return;
  const [ax, ay] = centre(a), [bx, by] = centre(b);
  const on = selection === h.id;
  ctx.strokeStyle = on ? css('--accent') : h.running ? css('--storage') : css('--muted');
  ctx.lineWidth = (h.proto === 'rail' ? 3 : 2) + (on ? 2 : 0);
  ctx.setLineDash(h.proto === 'rail' ? [8, 4] : []);
  ctx.beginPath();
  ctx.moveTo(ax, ay);
  ctx.lineTo(bx, by);
  ctx.stroke();
  ctx.setLineDash([]);
  arrow(ax, ay, bx, by, css('--storage'));
  if (view.scale > 0.5) {
    ctx.fillStyle = css('--muted');
    ctx.font = '10px var(--mono), monospace';
    const g = h.geometry || {};
    ctx.fillText(`${h.item} ${g.seconds ? g.seconds.toFixed(1) + 's' : ''}`,
      (ax + bx) / 2 + 4, (ay + by) / 2 - 4);
  }
  // What is in the air, as pips along the line.
  const p = net.plantOf(h.name);
  if (p && p.flights && net.state.view) {
    const now = net.state.view.plant.tick;
    for (const f of p.flights) {
      const k = 1 - (f.arrive - now) / Math.max(1, h.geometry ? h.geometry.latency : 1);
      if (k < 0 || k > 1) continue;
      ctx.fillStyle = f.loaded ? css('--source') : 'rgba(125,144,137,.6)';
      ctx.beginPath();
      ctx.arc(ax + (bx - ax) * k, ay + (by - ay) * k, 2.5, 0, 7);
      ctx.fill();
    }
  }
}

function arrow(ax, ay, bx, by, colour) {
  const a = Math.atan2(by - ay, bx - ax);
  const mx = (ax + bx) / 2, my = (ay + by) / 2;
  ctx.fillStyle = colour;
  ctx.beginPath();
  ctx.moveTo(mx + Math.cos(a) * 5, my + Math.sin(a) * 5);
  ctx.lineTo(mx + Math.cos(a + 2.5) * 5, my + Math.sin(a + 2.5) * 5);
  ctx.lineTo(mx + Math.cos(a - 2.5) * 5, my + Math.sin(a - 2.5) * 5);
  ctx.fill();
}

/// The picture under the pointer. Client-side, uncommitted, and never sent.
function ghost() {
  if (tool.mode === 'connect' || tool.mode === 'belt' || tool.mode === 'rail') {
    const a = tool.from ? net.byId(tool.from) : null;
    if (a && hover) {
      const [ax, ay] = centre(a);
      ctx.strokeStyle = css('--accent');
      ctx.setLineDash([4, 3]);
      ctx.beginPath();
      ctx.moveTo(ax, ay);
      ctx.lineTo(sx(hover.raw[0]), sy(hover.raw[1]));
      ctx.stroke();
      ctx.setLineDash([]);
    }
    return;
  }
  const h = held();
  if (!h || !hover || tool.mode !== 'place') return;
  const bad = collides(hover.x, hover.y, h.w, h.h);
  const x = sx(hover.x), y = sy(hover.y);
  const w = h.w * TILE * view.scale, ht = h.h * TILE * view.scale;
  ctx.fillStyle = bad ? 'rgba(224,108,108,.18)' : 'rgba(70,197,165,.16)';
  ctx.fillRect(x, y, w, ht);
  ctx.strokeStyle = bad ? css('--bad') : css('--accent');
  ctx.setLineDash([4, 3]);
  ctx.strokeRect(x + .5, y + .5, w - 1, ht - 1);
  ctx.setLineDash([]);
  // Which way it is facing, so that rotation is visible before it is
  // committed rather than afterwards.
  const [cx, cy] = [x + w / 2, y + ht / 2];
  const a = tool.face * Math.PI / 2;
  ctx.strokeStyle = css('--accent');
  ctx.beginPath();
  ctx.moveTo(cx, cy);
  ctx.lineTo(cx + Math.cos(a) * Math.min(w, ht) * .4, cy + Math.sin(a) * Math.min(w, ht) * .4);
  ctx.stroke();
}

/// Everybody else's pointer. Not deterministic, not ordered, not part of
/// anything -- which is exactly why it may be drawn without asking anybody.
function cursors() {
  const v = net.state.view;
  for (const p of v.players) {
    if (p.id === net.state.player || !p.cursor) continue;
    const [x, y] = [sx(p.cursor[0]), sy(p.cursor[1])];
    ctx.fillStyle = p.colour;
    ctx.beginPath();
    ctx.moveTo(x, y);
    ctx.lineTo(x + 9, y + 4);
    ctx.lineTo(x + 4, y + 9);
    ctx.fill();
    ctx.font = '10px var(--ui), sans-serif';
    ctx.fillText(p.name, x + 11, y + 12);
  }
}

export function focus() {
  const v = net.state.view;
  if (!v) return;
  const [x0, y0, x1, y1] = v.world.extent;
  const w = Math.max(24, x1 - x0 + 12), h = Math.max(24, y1 - y0 + 12);
  view.scale = Math.min(view.w / (w * TILE), view.h / (h * TILE));
  view.ox = -(x0 - 6) * TILE * view.scale;
  view.oy = -(y0 - 6) * TILE * view.scale;
  need = true;
}
