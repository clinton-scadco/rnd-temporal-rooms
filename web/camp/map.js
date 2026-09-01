// The map: five rooms, seven lanes, and whatever is between them right now.
//
// It draws exactly what the campaign frame says and computes nothing. A train
// on this canvas is at the position its `due` says it is at, interpolated
// between the tick it left and the tick it lands -- which is a picture of the
// authority's arithmetic rather than a second copy of it. If the drawing and
// the simulation ever disagree, the drawing is wrong, and that is the only
// arrangement in which the disagreement is findable.

const COL = {
  ink: '#dfe7f2',
  dim: '#8b98a8',
  line: '#233042',
  shut: '#2a323d',
  open: '#7cc4ff',
  done: '#8ef0a0',
  here: '#ffe066',
  load: '#ffb457',
  plate: '#131a24',
};

let cv = null;
let ctx = null;
let frame = null;
let sites = null;
let hot = null;
let onPick = () => {};
let dpr = 1;

export function init(canvas, opts = {}) {
  cv = canvas;
  ctx = cv.getContext('2d');
  onPick = opts.onPick || (() => {});
  cv.addEventListener('mousemove', e => {
    const was = hot;
    hot = at(e);
    if (was !== hot) draw();
  });
  cv.addEventListener('mouseleave', () => { hot = null; draw(); });
  cv.addEventListener('click', e => {
    const t = at(e);
    if (t) onPick(t);
  });
  window.addEventListener('resize', resize);
}

export function setSites(s) {
  sites = s;
}

export function show(v) {
  frame = v;
  draw();
}

export function resize() {
  if (!cv) return;
  const r = cv.getBoundingClientRect();
  dpr = window.devicePixelRatio || 1;
  cv.width = Math.max(1, Math.round(r.width * dpr));
  cv.height = Math.max(1, Math.round(r.height * dpr));
  draw();
}

// The five rooms are laid out on a small integer grid the server authored, so
// the map is the same shape on every screen and in every screenshot.
function boxes() {
  if (!frame) return [];
  const r = cv.getBoundingClientRect();
  const w = Math.max(320, r.width);
  const h = Math.max(240, r.height);
  const cols = 5;
  const rows = 3;
  const bw = Math.min(190, (w - 60) / cols - 26);
  const bh = 88;
  const gx = (w - 40) / cols;
  const gy = Math.min(150, (h - 60) / rows);
  return frame.rooms.map(room => ({
    room,
    x: 30 + room.x * gx + (gx - bw) / 2,
    y: 30 + room.y * gy,
    w: bw,
    h: bh,
  }));
}

function at(e) {
  const r = cv.getBoundingClientRect();
  const x = e.clientX - r.left;
  const y = e.clientY - r.top;
  const b = boxes().find(b => x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h);
  return b ? b.room.tag : null;
}

function draw() {
  if (!ctx || !frame) return;
  const r = cv.getBoundingClientRect();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, r.width, r.height);
  ctx.fillStyle = '#0d1117';
  ctx.fillRect(0, 0, r.width, r.height);

  const bs = boxes();
  const find = tag => bs.find(b => b.room.tag === tag);
  const routes = (frame.shipping && frame.shipping.routes) || [];
  const lanes = (frame.shipping && frame.shipping.lanes) || (sites && sites.lanes) || [];

  // ---- lanes, under everything
  for (const l of lanes) {
    const a = find(l.from);
    const b = find(l.to);
    if (!a || !b) continue;
    const open = routes.filter(r => r.from === l.from && r.to === l.to && r.item === l.item);
    const p0 = anchor(a, b);
    const p1 = anchor(b, a);
    ctx.strokeStyle = open.length ? COL.open : COL.line;
    ctx.lineWidth = open.length ? 2 : 1;
    ctx.setLineDash(open.length ? [] : [4, 5]);
    ctx.beginPath();
    ctx.moveTo(p0.x, p0.y);
    ctx.lineTo(p1.x, p1.y);
    ctx.stroke();
    ctx.setLineDash([]);
    arrow(p0, p1, open.length ? COL.open : COL.line);

    const mx = (p0.x + p1.x) / 2;
    const my = (p0.y + p1.y) / 2;
    ctx.font = '10px ui-monospace, monospace';
    ctx.textAlign = 'center';
    ctx.fillStyle = open.length ? COL.dim : COL.shut;
    ctx.fillText(l.itemTitle || l.item, mx, my - 5);

    // Whatever is in the air, where it actually is.
    for (const rt of open) {
      for (const d of rt.due || []) {
        const left = d.at - rt.trip;
        const span = Math.max(1, rt.trip);
        const k = Math.min(1, Math.max(0, (frame.tick - left) / span));
        const x = p0.x + (p1.x - p0.x) * k;
        const y = p0.y + (p1.y - p0.y) * k;
        ctx.fillStyle = COL.load;
        ctx.beginPath();
        ctx.arc(x, y, 3.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  // ---- the rooms
  for (const b of bs) {
    const room = b.room;
    const state = room.done ? 'done' : room.open ? 'open' : 'shut';
    const edge = state === 'done' ? COL.done : state === 'open' ? COL.open : COL.shut;
    const here = frame.at === room.tag;
    plate(b.x, b.y, b.w, b.h, COL.plate, here ? COL.here : edge, hot === room.tag || here ? 2 : 1);

    ctx.textAlign = 'left';
    ctx.fillStyle = state === 'shut' ? COL.dim : COL.ink;
    ctx.font = '600 12px ui-sans-serif, system-ui, sans-serif';
    ctx.fillText(room.title, b.x + 10, b.y + 20);

    ctx.font = '10px ui-monospace, monospace';
    ctx.fillStyle = edge;
    ctx.fillText(state === 'shut' ? 'locked' : state === 'done' ? 'producing' : 'open', b.x + 10, b.y + 35);

    ctx.fillStyle = COL.dim;
    const p = room.goal && room.goal.progress;
    if (state !== 'shut' && p) {
      const met = p.lines.filter(l => l.met).length;
      ctx.fillText(`${met}/${p.lines.length} met`, b.x + 10, b.y + 50);
      bar(b.x + 10, b.y + 56, b.w - 20, 3, p.lines.length ? met / p.lines.length : 0, edge);
    }
    ctx.fillText(`${room.machines} machines`, b.x + 10, b.y + 72);

    const who = (room.here || []).join(', ');
    if (who) {
      ctx.textAlign = 'right';
      ctx.fillStyle = COL.here;
      ctx.fillText(who, b.x + b.w - 10, b.y + 20);
      ctx.textAlign = 'left';
    }
  }
}

function anchor(from, to) {
  const cx = from.x + from.w / 2;
  const cy = from.y + from.h / 2;
  const tx = to.x + to.w / 2;
  const ty = to.y + to.h / 2;
  const dx = tx - cx;
  const dy = ty - cy;
  // Meet the edge of the plate rather than its centre, so a line does not run
  // under the words.
  const sx = dx === 0 ? Infinity : from.w / 2 / Math.abs(dx);
  const sy = dy === 0 ? Infinity : from.h / 2 / Math.abs(dy);
  const s = Math.min(sx, sy);
  return { x: cx + dx * s, y: cy + dy * s };
}

function arrow(p0, p1, colour) {
  const a = Math.atan2(p1.y - p0.y, p1.x - p0.x);
  ctx.fillStyle = colour;
  ctx.beginPath();
  ctx.moveTo(p1.x, p1.y);
  ctx.lineTo(p1.x - 8 * Math.cos(a - 0.35), p1.y - 8 * Math.sin(a - 0.35));
  ctx.lineTo(p1.x - 8 * Math.cos(a + 0.35), p1.y - 8 * Math.sin(a + 0.35));
  ctx.closePath();
  ctx.fill();
}

function plate(x, y, w, h, fill, edge, width) {
  ctx.fillStyle = fill;
  ctx.strokeStyle = edge;
  ctx.lineWidth = width;
  round(x, y, w, h, 6);
  ctx.fill();
  ctx.stroke();
}

function bar(x, y, w, h, k, colour) {
  ctx.fillStyle = '#1d2733';
  ctx.fillRect(x, y, w, h);
  ctx.fillStyle = colour;
  ctx.fillRect(x, y, w * Math.min(1, Math.max(0, k)), h);
}

function round(x, y, w, h, r) {
  ctx.beginPath();
  ctx.moveTo(x + r, y);
  ctx.arcTo(x + w, y, x + w, y + h, r);
  ctx.arcTo(x + w, y + h, x, y + h, r);
  ctx.arcTo(x, y + h, x, y, r);
  ctx.arcTo(x, y, x + w, y, r);
  ctx.closePath();
}
