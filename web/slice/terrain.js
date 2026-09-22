// The ground, under the factory.
//
// This is the one module in the slice's front end that does something no
// earlier prototype's client does, and the way it does it is the point: it
// paints on a canvas *behind* `#world`, using the projection `world.js`
// exports, and `world.js` is never told it exists.
//
// That matters because `web/room/` has now been served unforked by three
// different servers, and the moment this experiment reached into it to add a
// terrain layer, that claim would have stopped being true. A canvas behind
// another canvas, reading `view.ox`, `view.oy` and `view.scale`, is the whole
// mechanism. The renderer draws the factory; this draws the century it is
// standing in; neither knows about the other.
//
// It is the same fold as `slice map` and `slice map --png` — one list of
// features, each carrying three faces — so a screen, a terminal and a PNG
// cannot end up disagreeing about what the valley looked like in 1890.

import { view } from '../room/world.js';

const TILE = 7;

let cv = null;
let ctx = null;
let land = null;        // { plot, features: [...] }
let phase = null;       // which century we are painting
let last = '';          // the projection we last painted at

export function init(canvas) {
  cv = canvas;
  ctx = cv.getContext('2d');
  window.addEventListener('resize', resize);
  // The plot pans and zooms under a renderer this module does not drive, so
  // rather than hooking it, the terrain checks once a frame whether the
  // projection moved. Fifteen rectangles is not a budget worth defending.
  const tick = () => {
    const sig = `${view.ox}|${view.oy}|${view.scale}|${view.w}|${view.h}|${phase}`;
    if (sig !== last) { last = sig; draw(); }
    requestAnimationFrame(tick);
  };
  requestAnimationFrame(tick);
}

export function setLand(l) {
  land = l;
  last = '';
}

/// Which century to paint. Changing it is what a fracture crossing looks like.
export function setPhase(p) {
  phase = p;
}

export function resize() {
  if (!cv) return;
  const r = cv.getBoundingClientRect();
  const dpr = Math.min(2, (window && window.devicePixelRatio) || 1);
  cv.width = Math.max(1, Math.round(r.width * dpr));
  cv.height = Math.max(1, Math.round(r.height * dpr));
  last = '';
  draw();
}

/// Everything standing on this century's ground, for the panel beside the plot.
export function here() {
  if (!land || !phase) return [];
  return land.features
    .map(f => ({ name: f.name, became: f.became, ...face(f) }))
    .filter(f => f.what);
}

function face(f) {
  return (f.faces || []).find(x => x.phase === phase) || {};
}

function draw() {
  if (!ctx || !cv) return;
  const r = cv.getBoundingClientRect();
  const dpr = Math.min(2, (window && window.devicePixelRatio) || 1);
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, r.width, r.height);
  if (!land || !phase) return;

  const s = TILE * view.scale;
  const sx = x => view.ox + x * s;
  const sy = y => view.oy + y * s;

  // The plot itself, so the ground reads as ground rather than as the page.
  ctx.fillStyle = '#b6ac98';
  ctx.globalAlpha = 0.20;
  ctx.fillRect(sx(0), sy(0), land.plot * s, land.plot * s);
  ctx.globalAlpha = 1;

  for (const f of land.features) {
    const fc = face(f);
    if (!fc.colour) continue;
    const x = sx(f.x), y = sy(f.y), w = f.w * s, h = f.h * s;

    // A deposit is drawn by `world.js` already, hatched, as an opportunity. So
    // this layer paints it faintly and lets the renderer put the real thing on
    // top — otherwise the same ore body would be drawn twice, in two
    // vocabularies, and the second one would win an argument it should not be
    // having.
    const isGround = !!fc.yields;
    ctx.globalAlpha = isGround ? 0.13 : fc.taken ? 0.62 : 0.34;
    ctx.fillStyle = fc.colour;
    ctx.fillRect(x, y, w, h);
    ctx.globalAlpha = 1;

    // Something somebody else built. It is not yours, you cannot build on it,
    // and it should look like a wall rather than like a texture.
    if (fc.taken) {
      ctx.strokeStyle = fc.colour;
      ctx.lineWidth = 1.5;
      ctx.strokeRect(x + 0.75, y + 0.75, w - 1.5, h - 1.5);
      ctx.save();
      ctx.beginPath();
      ctx.rect(x, y, w, h);
      ctx.clip();
      ctx.strokeStyle = 'rgba(12,14,18,.34)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      const step = Math.max(5, 6 * view.scale);
      for (let k = -h; k < w; k += step) {
        ctx.moveTo(x + k, y + h);
        ctx.lineTo(x + k + h, y);
      }
      ctx.stroke();
      ctx.restore();
    }

    if (view.scale > 0.55 && !isGround) {
      ctx.fillStyle = 'rgba(232,238,246,.72)';
      ctx.font = `${Math.max(9, 9.5 * view.scale)}px ui-sans-serif, system-ui, sans-serif`;
      ctx.textBaseline = 'top';
      ctx.fillText(f.name, x + 4, y + 4);
    }
  }
}
