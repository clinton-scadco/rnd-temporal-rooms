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
export const tool = {
  mode: 'pick', proto: null, face: 0, from: null, item: null, design: null,
  /// Place the catalogue's worked example rather than an empty chassis. Only
  /// ever set by asking for one by name.
  example: false,
};
export let selection = null;

let canvas = null, ctx = null, hover = null, need = true, onSelect = () => {};
let onHover = () => {};
let lastCursor = 0;

export function init(el, hooks) {
  canvas = el;
  ctx = el.getContext('2d');
  onSelect = hooks.onSelect || (() => {});
  onHover = hooks.onHover || (() => {});
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
export function setTool(mode, proto, design, example) {
  tool.mode = mode;
  tool.proto = proto || null;
  tool.design = design || null;
  tool.example = !!example;
  tool.from = null;
  // A port chosen for a connection that was never finished must not be
  // waiting inside the next one.
  tool.item = null;
  need = true;
}

/// What the pointer is over, which is a different thing from what is selected.
/// Hovering shows; clicking pins.
let hovered = null;
export const peek = () => hovered;

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

// ------------------------------------------------------------------ ports
//
// A connection now starts and ends somewhere in particular. It used to be
// drawn centre to centre, which is what you draw when a connection is an edge
// in a graph -- and the play session's note 13 is what it feels like when the
// thing on screen is an edge in a graph: lines crossing buildings, no way to
// tell what any of them carried, and no sense that the factory occupied space.
//
// So a machine's ports are laid out on its edges: what it takes on the left,
// what it gives on the right, in the order the design lists them. That order
// is derived from the design inside the machine and is stable, so a port stays
// where it was between frames.

/// Which side of a building a port sits on, and where along it.
function portAt(i, item, out) {
  const list = (i.ports || []).filter(p => !!p.out === out);
  let k = list.findIndex(p => p.item === item);
  if (k < 0) k = 0;
  const n = Math.max(1, list.length);
  const along = (k + 1) / (n + 1);
  return [out ? sx(i.x + i.w) : sx(i.x), sy(i.y + i.h * along)];
}

/// The route a connection takes, as a list of points.
///
/// Orthogonal, because a belt goes round corners and a diagonal through three
/// other buildings does not. Two shapes: a `Z` when the consumer is to the
/// right of the producer, which is the common case and reads left to right;
/// and a loop around the outside when it is not, because a line that went
/// straight back through its own producer would be worse than a long way
/// round.
///
/// Deliberately not a solver. Experiment 13 asks for routes that consume space
/// and avoid collisions, and that is a document change -- a route would have to
/// be part of the world, part of the hash, and part of what refuses a
/// placement. This is the drawing, done honestly: it starts at a real port,
/// ends at a real port, and turns square corners in between.
const STUB = 10;
function routeOf(a, b, item) {
  const [ax, ay] = portAt(a, item, true);
  const [bx, by] = portAt(b, item, false);
  const gap = bx - ax;
  if (gap > STUB * 2) {
    const mx = ax + gap / 2;
    return [[ax, ay], [mx, ay], [mx, by], [bx, by]];
  }
  // Round the outside: out of the producer, along a lane below both of them,
  // and back in to the consumer.
  const lane = Math.max(sy(a.y + a.h), sy(b.y + b.h)) + STUB * 1.6;
  const out = ax + STUB, back = bx - STUB;
  return [[ax, ay], [out, ay], [out, lane], [back, lane], [back, by], [bx, by]];
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
  // A wire is a polyline now, so every leg of it has to be clickable -- a
  // connection you can see and cannot select is exactly the wire that turns
  // out to be the wrong one.
  for (const c of v.world.conns) {
    const a = net.byId(c.from), b = net.byId(c.to);
    if (!a || !b) continue;
    const pts = routeOf(a, b, c.item);
    for (let k = 1; k < pts.length; k++) {
      const d = offSegment(px, py, pts[k - 1][0], pts[k - 1][1], pts[k][0], pts[k][1]);
      if (d < near) { near = d; best = net.wireKey(c); }
    }
  }
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

/// Whether the thing being held would have ground to stand on here.
///
/// A head off its seam is a refusal the server will make anyway; showing it
/// under the pointer means the player never has to earn that refusal.
function onGround(x, y, w, h) {
  const p = net.proto(tool.proto);
  if (!p || !p.extracts) return true;
  const v = net.state.view;
  return ((v && v.world.deposits) || []).some(d =>
    d.item === p.extracts && d.spare > 0
    && x < d.x + d.w && d.x < x + w && y < d.y + d.h && d.y < y + h);
}

// ----------------------------------------------------------------- pointer

function wire() {
  canvas.addEventListener('pointermove', e => {
    const [x, y] = at(e);
    hover = { x: Math.floor(x), y: Math.floor(y), raw: [x, y] };
    need = true;
    // Note 3: the inspector should follow the pointer, not wait for a click.
    // Reading a factory means sweeping across it, and a panel that costs a
    // click per building is a panel nobody reads twice.
    if (tool.mode === 'pick' || tool.mode === 'connect') {
      const it = under(x, y);
      const over = it ? it.id : lineUnder(x, y);
      if (over !== hovered) {
        hovered = over;
        onHover(over);
      }
    }
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
  // A placement is a *chassis* unless it is carrying a design: one off the
  // shelf, a copy of something already standing, or -- asked for by name --
  // the catalogue's worked example. Note 7 of the play session: prebuilt
  // machines take the fun out of the game entirely.
  const common = {
    proto: h.p.tag,
    x,
    y,
    face: tool.face,
    design: tool.design,
    example: !tool.design && !!tool.example,
  };
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
    // The one pairing still refused, and not for want of a buffer: two bays
    // are joined by a transport, which has a length and a latency.
    if (a.role === 'storage' && b.role === 'storage') {
      return toast('two bays need a transport between them');
    }
    // A connection was started from a particular port, so there is nothing
    // left to ask.
    if (tool.item) {
      const item = tool.item;
      tool.item = null;
      return net.send('CreateConnection', { from: a.id, to: b.id, item });
    }
    const items = mating(a, b);
    if (!items.length) {
      return toast(`${a.name} has nothing ${b.name} takes`);
    }
    return choose(items, item => net.send('CreateConnection', { from: a.id, to: b.id, item }), e);
  }
  // A transport. Both ends must be bays, and the item must be something that
  // actually arrives at the loading end.
  if (a.role !== 'storage' || b.role !== 'storage') return toast('a transport runs between two bays');
  const items = arriving(a.id);
  if (!items.length) return toast(`nothing is delivered to ${a.name} yet`);
  choose(items, item => net.send('CreateWorldLink', { proto: tool.mode, from: a.id, to: b.id, item }), e);
}

/// What could cross from one building to another: an output port on the near
/// end meeting an input port on the far end.
///
/// This is note 10 -- never ask a question whose answer is already determined.
/// A bay holding one item wired to a machine that takes it produces a
/// one-element list, and `choose` below sends the command without a menu.
export function mating(a, b) {
  const outs = portItems(a, true), ins = portItems(b, false);
  const both = outs.filter(i => ins.includes(i));
  if (both.length) return both;
  // A bay has no ports of its own until the room gives it some, so an empty
  // one falls back to what the other end can handle. Two machines with nothing
  // in common get an empty list and are told so.
  if (a.role === 'storage' && !outs.length) return ins;
  if (b.role === 'storage' && !ins.length) return outs;
  return both;
}

export function portItems(i, out) {
  return [...new Set((i.ports || []).filter(p => !!p.out === out).map(p => p.item))];
}

/// Start a connection from one named port, so the second click finishes it.
///
/// The contextual palette calls this: a player who clicked `IronOre OUT` on a
/// machine has already answered the only question a wire needs, and should be
/// asked for a destination and nothing else.
export function connectFrom(id, item) {
  tool.mode = 'connect';
  tool.proto = null;
  tool.from = id;
  tool.item = item;
  need = true;
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
  // Terrain first, under everything: it is what the room *is*, and a head
  // stands on top of it.
  for (const d of w.deposits || []) ground(d);
  for (const h of w.hauls) haul(h);
  for (const c of w.conns) conn(c);
  for (const i of w.installs) box(i);
  for (const i of w.installs) ports(i);
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

/// A patch of ground worth standing on.
///
/// Drawn as ground rather than as a building: hatched, unbordered on three
/// sides, sitting under whatever has been built on it. Experiment 13's note 1
/// was that a mine which produced ore because the catalogue said so was the
/// last magical object in the world; the answer has to *look* like an
/// opportunity rather than like another box.
function ground(d) {
  const x = sx(d.x), y = sy(d.y);
  const w = d.w * TILE * view.scale, h = d.h * TILE * view.scale;
  const colour = css('--' + (d.domain || 'material')) || css('--muted');
  const spent = d.spare === 0;
  // Something is being placed that would work this ground: say so before the
  // click rather than after it.
  const wanted = tool.mode === 'place' && net.proto(tool.proto)
    && net.proto(tool.proto).extracts === d.item;

  ctx.save();
  ctx.beginPath();
  ctx.rect(x, y, w, h);
  ctx.clip();
  ctx.fillStyle = colour + (wanted ? '2a' : '14');
  ctx.fillRect(x, y, w, h);
  // Hatching, so ground never reads as a floor you could build a bay on by
  // accident -- you can, and it is a waste, and it should look like one.
  ctx.strokeStyle = colour + (spent ? '18' : '38');
  ctx.lineWidth = 1;
  ctx.beginPath();
  const step = 7 * view.scale;
  for (let k = -h; k < w; k += step) {
    ctx.moveTo(x + k, y + h);
    ctx.lineTo(x + k + h, y);
  }
  ctx.stroke();
  ctx.restore();

  ctx.strokeStyle = colour + (wanted ? 'cc' : '55');
  ctx.setLineDash(wanted ? [] : [4, 3]);
  ctx.lineWidth = wanted ? 2 : 1;
  ctx.strokeRect(x + .5, y + .5, w - 1, h - 1);
  ctx.setLineDash([]);

  if (view.scale > 0.5) {
    ctx.fillStyle = colour + (spent ? '77' : 'dd');
    ctx.font = `${Math.max(9, 10 * view.scale)}px var(--ui), sans-serif`;
    ctx.textBaseline = 'bottom';
    ctx.fillText(d.title, x + 4, y + h - 5);
    ctx.font = '9px var(--mono), monospace';
    ctx.fillStyle = css('--muted');
    // What is left of it, which is the only number that decides whether
    // another head here is worth the floor.
    ctx.fillText(
      spent ? `${short(d.yields)}/s, all spoken for` : `${short(d.spare)} of ${short(d.yields)}/s free`,
      x + 4, y + h + 9);
    ctx.textBaseline = 'top';
  }
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

/// One connection, drawn as the physical thing it now is: from a real port,
/// round square corners, into a real port, in the colour of what it carries.
function conn(c) {
  const a = net.byId(c.from), b = net.byId(c.to);
  if (!a || !b) return;
  const on = selection === net.wireKey(c);
  const pts = routeOf(a, b, c.item);
  const colour = css('--' + (c.domain || 'material')) || css('--belt');
  ctx.strokeStyle = on ? css('--accent') : colour;
  ctx.globalAlpha = on ? 1 : .75;
  ctx.lineWidth = on ? 3 : 2;
  ctx.lineJoin = 'round';
  // Electricity is a cable rather than a belt, and reads better as one.
  ctx.setLineDash(c.domain === 'electrical' ? [6, 3] : []);
  ctx.beginPath();
  ctx.moveTo(pts[0][0], pts[0][1]);
  for (let k = 1; k < pts.length; k++) ctx.lineTo(pts[k][0], pts[k][1]);
  ctx.stroke();
  ctx.setLineDash([]);
  ctx.globalAlpha = 1;
  const [px, py] = pts[pts.length - 2], [qx, qy] = pts[pts.length - 1];
  arrow(px, py, qx, qy, colour);
  // What it carries, on the longest leg, so a room full of wires says what is
  // in each of them without being clicked.
  if (view.scale > 0.55) {
    let best = 0, bx = 0, by = 0;
    for (let k = 1; k < pts.length; k++) {
      const len = Math.hypot(pts[k][0] - pts[k - 1][0], pts[k][1] - pts[k - 1][1]);
      if (len > best) {
        best = len;
        bx = (pts[k][0] + pts[k - 1][0]) / 2;
        by = (pts[k][1] + pts[k - 1][1]) / 2;
      }
    }
    if (best > 26) {
      ctx.fillStyle = on ? css('--accent') : colour;
      ctx.font = '9px var(--mono), monospace';
      ctx.textBaseline = 'bottom';
      ctx.textAlign = 'center';
      ctx.fillText(c.title || c.item, bx, by - 3);
      ctx.textAlign = 'left';
      ctx.textBaseline = 'top';
    }
  }
}

/// The sockets on the outside of one building.
///
/// Small, and worth the pixels: they are the thing that makes a machine look
/// like it has an inside. An unconnected port is hollow, which is how a player
/// finds the input nobody has fed.
function ports(i) {
  if (view.scale < 0.5) return;
  const v = net.state.view;
  const wired = new Set();
  for (const c of v.world.conns) {
    if (c.from === i.id) wired.add('out:' + c.item);
    if (c.to === i.id) wired.add('in:' + c.item);
  }
  for (const h of v.world.hauls) {
    if (h.from === i.id) wired.add('out:' + h.item);
    if (h.to === i.id) wired.add('in:' + h.item);
  }
  const seen = new Set();
  for (const p of i.ports || []) {
    const key = (p.out ? 'out:' : 'in:') + p.item;
    if (seen.has(key)) continue;
    seen.add(key);
    const [x, y] = portAt(i, p.item, !!p.out);
    const colour = css('--' + p.domain) || css('--muted');
    ctx.beginPath();
    ctx.arc(x, y, 2.6, 0, Math.PI * 2);
    if (wired.has(key)) {
      ctx.fillStyle = colour;
      ctx.fill();
    } else {
      ctx.strokeStyle = colour;
      ctx.lineWidth = 1;
      ctx.stroke();
    }
  }
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
  const bad = collides(hover.x, hover.y, h.w, h.h) || !onGround(hover.x, hover.y, h.w, h.h);
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
