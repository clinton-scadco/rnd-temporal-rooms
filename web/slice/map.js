// The world view: three centuries of one valley, stacked, with the fractures
// between them and whatever is inside one right now.
//
// It draws exactly what the slice frame says and computes nothing. A load on
// this canvas is where its `due` says it is, interpolated between the tick it
// left and the tick it lands — a picture of the authority's arithmetic rather
// than a second copy of it. If the drawing and the simulation ever disagree,
// the drawing is wrong, and that is the only arrangement in which the
// disagreement is findable.
//
// Two things here are not in Prototype 3's map, and both are the experiment:
//
//   the fracture   drawn as the join itself, with what it is holding, what it
//                  costs and whether the grid behind it is managing — because a
//                  dark fracture is the one failure in this world that is not
//                  about a factory
//   the terrain    each century's ground as a thumbnail on its own plate, from
//                  the same fold the plot and the PNG use, so the eye can see
//                  the wood become a street without entering either region

const COL = {
  ink: '#dfe7f2',
  dim: '#8b98a8',
  line: '#233042',
  open: '#7cc4ff',
  done: '#8ef0a0',
  here: '#ffe066',
  load: '#ffb457',
  lit: '#a98bff',
  dark: '#e0565f',
  plate: '#131a24',
};

let cv = null;
let ctx = null;
let frame = null;
let land = null;
let statics = null;
let hot = null;
let onPick = () => {};
let onGate = () => {};
let dpr = 1;
let gateHit = [];

export function init(canvas, opts = {}) {
  cv = canvas;
  ctx = cv.getContext('2d');
  onPick = opts.onPick || (() => {});
  onGate = opts.onGate || (() => {});
  cv.addEventListener('mousemove', e => {
    const was = hot;
    hot = at(e);
    cv.style.cursor = hot || gateAt(e) ? 'pointer' : 'default';
    if (was !== hot) draw();
  });
  cv.addEventListener('mouseleave', () => { hot = null; draw(); });
  cv.addEventListener('click', e => {
    const g = gateAt(e);
    if (g) return onGate(g);
    const t = at(e);
    if (t) onPick(t);
  });
  window.addEventListener('resize', resize);
}

export function setStatics(s) { statics = s; }
export function setLand(l) { land = l; }

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

// Stacked rather than spread, because the one relationship that matters here
// is *earlier and later*, and a map that put 1890 to the left of 2037 would be
// making it look like a journey rather than a drop.
function boxes() {
  if (!frame) return [];
  const r = cv.getBoundingClientRect();
  const w = Math.max(360, r.width);
  const h = Math.max(300, r.height);
  const bw = Math.min(520, w - 80);
  const bh = Math.min(120, (h - 80) / 3 - 26);
  const gy = Math.min(170, (h - 40) / 3);
  return frame.regions.map((region, i) => ({
    region,
    x: (w - bw) / 2,
    y: 24 + i * gy,
    w: bw,
    h: bh,
  }));
}

function at(e) {
  const r = cv.getBoundingClientRect();
  const x = e.clientX - r.left;
  const y = e.clientY - r.top;
  const b = boxes().find(b => x >= b.x && x <= b.x + b.w && y >= b.y && y <= b.y + b.h);
  return b ? b.region.tag : null;
}

function gateAt(e) {
  const r = cv.getBoundingClientRect();
  const x = e.clientX - r.left;
  const y = e.clientY - r.top;
  const g = gateHit.find(g => x >= g.x && x <= g.x + g.w && y >= g.y && y <= g.y + g.h);
  return g ? g.tag : null;
}

function draw() {
  if (!ctx || !frame) return;
  const r = cv.getBoundingClientRect();
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, r.width, r.height);
  ctx.fillStyle = '#0d1117';
  ctx.fillRect(0, 0, r.width, r.height);

  const bs = boxes();
  const find = tag => bs.find(b => b.region.tag === tag);
  const shipping = frame.shipping || {};
  const routes = shipping.routes || [];
  const gates = shipping.interfaces || [];
  const fractures = (frame.crossings || []).length
    ? frame.crossings
    : (statics && statics.fractures) || [];

  gateHit = [];

  // ---- the fractures, between the plates
  for (const f of fractures) {
    const a = find(f.early);
    const b = find(f.late);
    if (!a || !b) continue;
    const g = gates.find(g => g.fracture === f.tag);
    const x = a.x + a.w / 2;
    const y0 = a.y + a.h;
    const y1 = b.y;
    const colour = !g ? COL.line : g.lit ? COL.lit : COL.dark;

    ctx.strokeStyle = colour;
    ctx.lineWidth = g ? 3 : 1.5;
    ctx.setLineDash(g ? [] : [5, 5]);
    ctx.beginPath();
    ctx.moveTo(x, y0);
    ctx.lineTo(x, y1);
    ctx.stroke();
    ctx.setLineDash([]);
    arrow({ x, y: y0 }, { x, y: y1 }, colour);

    // The plaque: what it is holding, what it costs, and whether the region
    // behind it is managing. Clickable, because putting an interface up is the
    // one command in this experiment nothing else can do.
    const words = !g
      ? `no interface — click to build one`
      : g.lit
        ? `holding ${g.holding} years · ${g.wantMW} MW of ${g.haveMW}`
        : `DARK — wants ${g.wantMW} MW, has ${g.haveMW}`;
    ctx.font = '10px ui-monospace, monospace';
    const tw = ctx.measureText(words).width + 18;
    const px = x + 16;
    const py = (y0 + y1) / 2 - 9;
    plate(px, py, tw, 18, COL.plate, colour, 1);
    ctx.textAlign = 'left';
    ctx.fillStyle = g && !g.lit ? COL.dark : COL.dim;
    ctx.fillText(words, px + 9, py + 12);
    gateHit.push({ tag: f.tag, x: px, y: py, w: tw, h: 18 });

    ctx.fillStyle = COL.dim;
    ctx.textAlign = 'right';
    ctx.fillText(`${f.years} years`, x - 12, (y0 + y1) / 2 + 4);
    ctx.textAlign = 'left';

    // Whatever is inside the fracture, where it actually is.
    for (const rt of routes) {
      if (rt.fracture !== f.tag) continue;
      const down = rt.from === f.early;
      for (const d of rt.due || []) {
        const trip = Math.max(1, rt.tripSeconds * 60);
        const k = Math.min(1, Math.max(0, (frame.tick - (d.at - trip)) / trip));
        const t = down ? k : 1 - k;
        ctx.fillStyle = COL.load;
        ctx.beginPath();
        ctx.arc(x, y0 + (y1 - y0) * t, 3.5, 0, Math.PI * 2);
        ctx.fill();
      }
    }
  }

  // ---- the three centuries
  for (const b of bs) {
    const region = b.region;
    const here = frame.at === region.tag;
    const edge = region.done ? COL.done : COL.open;
    plate(b.x, b.y, b.w, b.h, COL.plate, here ? COL.here : edge, hot === region.tag || here ? 2 : 1);

    // The century's own ground, as a strip on its plate. Same features, same
    // colours, same fold as the plot and the PNG.
    thumb(b, region.phase);

    ctx.textAlign = 'left';
    ctx.fillStyle = COL.ink;
    ctx.font = '600 13px ui-sans-serif, system-ui, sans-serif';
    ctx.fillText(region.title, b.x + 12, b.y + 22);

    ctx.font = '10px ui-monospace, monospace';
    ctx.fillStyle = edge;
    ctx.fillText(region.done ? 'objective met' : 'running', b.x + 12, b.y + 38);

    ctx.fillStyle = COL.dim;
    const p = region.goal && region.goal.progress;
    if (p && p.lines) {
      const met = p.lines.filter(l => l.met).length;
      ctx.fillText(`${met}/${p.lines.length} met`, b.x + 12, b.y + 54);
      bar(b.x + 12, b.y + 60, 150, 3, p.lines.length ? met / p.lines.length : 0, edge);
    }
    ctx.fillText(`${region.machines} machines · ${region.gridMW} MW on the grid`, b.x + 12, b.y + 78);

    // The one number 1890 is really playing against.
    const m = region.machinery || {};
    if (m.landed) {
      ctx.fillStyle = COL.lit;
      ctx.fillText(`${num(m.standing)} of ${num(m.landed)} gears of imported plant`, b.x + 12, b.y + 94);
    }

    const who = (region.here || []).join(', ');
    if (who) {
      ctx.textAlign = 'right';
      ctx.fillStyle = COL.here;
      ctx.font = '10px ui-monospace, monospace';
      ctx.fillText(who, b.x + b.w - 12, b.y + 22);
      ctx.textAlign = 'left';
    }
  }
}

/// One century's ground, drawn small on the right of its plate.
///
/// Cheap and literal: the same fifteen rectangles the terrain layer paints
/// under the plot, scaled into a strip. Three of these side by side is the
/// experiment's whole visual claim, on one screen, without entering anything.
function thumb(b, phase) {
  if (!land) return;
  const side = b.h - 16;
  const x0 = b.x + b.w - side - 10;
  const y0 = b.y + 8;
  const s = side / land.plot;
  ctx.save();
  ctx.beginPath();
  ctx.rect(x0, y0, side, side);
  ctx.clip();
  ctx.fillStyle = '#b6ac98';
  ctx.fillRect(x0, y0, side, side);
  for (const f of land.features) {
    const fc = (f.faces || []).find(x => x.phase === phase);
    if (!fc || !fc.colour) continue;
    ctx.fillStyle = fc.colour;
    ctx.fillRect(x0 + f.x * s, y0 + f.y * s, Math.max(1, f.w * s), Math.max(1, f.h * s));
  }
  ctx.restore();
  ctx.strokeStyle = COL.line;
  ctx.lineWidth = 1;
  ctx.strokeRect(x0 + 0.5, y0 + 0.5, side - 1, side - 1);
}

const num = n => (n || 0).toLocaleString('en-GB');

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
