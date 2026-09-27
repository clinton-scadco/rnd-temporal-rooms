// Experiment 16: the district, drawn from compact state.
//
// Nothing here simulates. A frame says how much strain a site holds and how
// fast it is changing, which road a cohort is on and when it set off, where a
// volley was fired from and where it lands. This module turns those into
// pictures at whatever tick the page asks for, sixty times a second, between
// polls.
//
// The ground is experiment 15's valley -- the same fold of the same fifteen
// features -- painted in the district's century. A bleed is that same fold
// painted in *another* century and clipped to the tear: when the gantry tears
// into 1890, Oakshaw's trees are standing where the A-road was.
//
// Pure functions of (layout, frame, tick, view). No DOM: an optional
// `opts.canvas(w, h)` lets the page cache the three centuries' ground, and
// without it every layer paints directly, so tests/combat_web.mjs can drive
// the whole thing against a canvas that records instead of painting.

const TURN = 65536;
const TAU = Math.PI * 2;

export const ERA = {
  1890: { ink: '#f2b36b', glow: 'rgba(242,179,107,', tint: 'rgba(255,196,120,', name: 'echo' },
  2037: { ink: '#8fd3c0', glow: 'rgba(143,211,192,', tint: 'rgba(143,211,192,', name: '' },
  2070: { ink: '#7fe3ff', glow: 'rgba(127,227,255,', tint: 'rgba(150,235,255,', name: 'glint' },
};

export const COLORS = {
  night: '#07090a',
  ink: '#e4ece8',
  muted: '#8a9a94',
  halo: 'rgba(5,8,7,0.85)',
  awake: '#e0a05c',
  closed: 'rgba(90,200,170,0.55)',
  domain: '#e8646a',
  steel: '#a9bcc6',
  tracer: '#ffe08a',
  shell: '#f5f0e0',
  flash: '#fff4c2',
  hurt: '#ff7b54',
};

// ------------------------------------------------------------------ helpers

function hash32(n) {
  n = (n ^ 61) ^ (n >>> 16);
  n = (n + (n << 3)) | 0;
  n ^= n >>> 4;
  n = Math.imul(n, 0x27d4eb2d);
  n ^= n >>> 15;
  return (n >>> 0) / 4294967296;
}

function hex(c) {
  const m = /^#?([0-9a-f]{6})$/i.exec(c || '');
  if (!m) return [128, 128, 128];
  const n = parseInt(m[1], 16);
  return [(n >> 16) & 255, (n >> 8) & 255, n & 255];
}

function rgb(a, k = 1) {
  return `rgb(${Math.round(a[0] * k)},${Math.round(a[1] * k)},${Math.round(a[2] * k)})`;
}

function mix(a, b, f) {
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f, a[2] + (b[2] - a[2]) * f];
}

/** A radial gradient, or `fallback` where the canvas has none to give. */
function radial(ctx, x, y, r, stops, fallback) {
  const g = ctx.createRadialGradient && ctx.createRadialGradient(x, y, 0, x, y, Math.max(1, r));
  if (!g || !g.addColorStop) return fallback;
  for (const [o, c] of stops) g.addColorStop(o, c);
  return g;
}

function label(ctx, text, x, y, color, size, weight = 600, align = 'left') {
  ctx.font = `${weight} ${size}px ui-monospace, "Cascadia Mono", Consolas, monospace`;
  ctx.textAlign = align;
  ctx.lineJoin = 'round';
  ctx.lineWidth = Math.max(2, size * 0.35);
  ctx.strokeStyle = COLORS.halo;
  ctx.strokeText(text, x, y);
  ctx.fillStyle = color;
  ctx.fillText(text, x, y);
  ctx.textAlign = 'left';
}

export function fmt(n) {
  const v = typeof n === 'string' ? Number(n) : n;
  if (!Number.isFinite(v)) return String(n);
  return Math.round(v).toLocaleString('en-US');
}

/** 1.2k, 43M: a count that has to fit beside a dot. */
export function short(n) {
  const v = Number(n);
  if (v < 1000) return String(Math.round(v));
  if (v < 1e6) return `${(v / 1e3).toFixed(v < 1e4 ? 1 : 0)}k`;
  if (v < 1e9) return `${(v / 1e6).toFixed(v < 1e7 ? 1 : 0)}M`;
  return `${(v / 1e9).toFixed(1)}G`;
}

// ------------------------------------------------------------------ geometry

/** Fit the district into a canvas of w x h pixels, with a margin. */
export function makeView(layout, w, h, margin = 16) {
  const fw = layout.w * layout.mt;
  const fh = layout.h * layout.mt;
  const scale = Math.min((w - 2 * margin) / fw, (h - 2 * margin) / fh);
  return { scale, ox: (w - fw * scale) / 2, oy: (h - fh * scale) / 2, w, h, tile: layout.mt * scale };
}

export const sx = (v, p) => v.ox + p[0] * v.scale;
export const sy = (v, p) => v.oy + p[1] * v.scale;

function lerp(a, b, f) {
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
}

const nodeAt = (layout, n) => layout.nodes[n].at;
function edgeLen(layout, a, b) {
  const p = nodeAt(layout, a);
  const q = nodeAt(layout, b);
  return Math.hypot(q[0] - p[0], q[1] - p[1]);
}

/**
 * Where a cohort is at `tick`: the server's closed form, walked on toward its
 * goal past the node it is heading for, so it does not visibly stop at every
 * corner while the page waits for the next poll.
 */
export function cohortPos(layout, c, tick) {
  if (c.site != null) return nodeAt(layout, c.node);
  let a = c.from;
  let b = c.to;
  let u = Math.max(0, tick - c.since) * c.speed;
  for (let i = 0; i < layout.nodes.length; i++) {
    const len = edgeLen(layout, a, b);
    if (u <= len) return lerp(nodeAt(layout, a), nodeAt(layout, b), len ? u / len : 1);
    if (c.goal == null || c.goal === b) return nodeAt(layout, b);
    u -= len;
    a = b;
    b = layout.next[b][c.goal];
  }
  return nodeAt(layout, b);
}

/** Which way a cohort is walking, in radians, or null when it is not. */
function cohortDir(layout, c) {
  if (c.site != null) return null;
  const p = nodeAt(layout, c.from);
  const q = nodeAt(layout, c.to);
  return Math.atan2(q[1] - p[1], q[0] - p[0]);
}

/** A site's strain at `tick`: its two levels, carried on at their rates. */
export function strainAt(site, tick) {
  const dt = Math.max(0, tick - site.since);
  return [0, 1].map(c => Math.max(0, Number(site.strain[c]) + Number(site.rate[c]) * dt));
}

/** A battery's heading at `tick`, in radians. */
export function heading(b, tick) {
  let a = b.heading;
  if (b.duty === 'laying' && b.fire > b.since) {
    let d = (((b.to - b.from) % TURN) + TURN) % TURN;
    if (d > TURN / 2) d -= TURN;
    const f = Math.min(1, Math.max(0, (tick - b.since) / (b.fire - b.since)));
    a = b.from + d * f;
  }
  return (a / TURN) * TAU;
}

/** A volley at `tick`: where it is, how far through its flight, how high. */
export function volleyAt(v, tick) {
  const f = Math.min(1, Math.max(0, (tick - v.fired) / Math.max(1, v.lands - v.fired)));
  const pos = lerp(v.from, v.to, f);
  const d = Math.hypot(v.to[0] - v.from[0], v.to[1] - v.from[1]);
  const lob = v.gun === 'howitzer' ? 4 * f * (1 - f) * d * 0.28 : 0;
  return { pos, f, lob };
}

/** How many figures stand for a cohort: every one of a few, a crowd of many. */
export function dotsFor(count) {
  if (count <= 14) return count;
  return Math.min(90, Math.round(14 + 9 * Math.log10(count / 14 + 1) * 2.4));
}

/** A point along a polyline, `f` of the way. */
function along(path, f) {
  const lens = [];
  let total = 0;
  for (let i = 0; i + 1 < path.length; i++) {
    const l = Math.hypot(path[i + 1][0] - path[i][0], path[i + 1][1] - path[i][1]);
    lens.push(l);
    total += l;
  }
  let d = f * total;
  for (let i = 0; i < lens.length; i++) {
    if (d <= lens[i]) return lerp(path[i], path[i + 1], lens[i] ? d / lens[i] : 0);
    d -= lens[i];
  }
  return path[path.length - 1];
}

// ------------------------------------------------------------------- ground

const PHASES = ['1890', '2037', '2070'];

/** One glyph per tile per century: experiment 15's fold, later rows winning. */
export function faceGrid(layout) {
  if (layout._grid) return layout._grid;
  const W = layout.w;
  const H = layout.h;
  const grid = {};
  PHASES.forEach((ph, pi) => {
    const g = new Array(W * H).fill(null);
    for (const f of layout.land || []) {
      const face = f.faces[pi];
      for (let y = Math.max(0, f.y); y < Math.min(H, f.y + f.h); y++) {
        for (let x = Math.max(0, f.x); x < Math.min(W, f.x + f.w); x++) g[y * W + x] = face;
      }
    }
    grid[ph] = g;
  });
  layout._grid = grid;
  return grid;
}

/** What open ground is, by century: meadow, dust, pale made ground. */
const BARE = { 1890: [52, 68, 40], 2037: [44, 44, 42], 2070: [52, 56, 60] };

function tileColour(face, phase, x, y) {
  const n = hash32(x * 7919 + y * 104729 + phase.charCodeAt(2));
  const k = 0.9 + 0.2 * n;
  if (!face || face.glyph === '.') return rgb(BARE[phase], k);
  const base = mix(hex(face.colour), BARE[phase], 0.42);
  return rgb(base, k * 0.72);
}

/** Paint one century's ground into `ctx` under view `v`. */
function paintGround(ctx, v, layout, phase) {
  const g = faceGrid(layout)[phase];
  const W = layout.w;
  const t = v.tile;
  ctx.fillStyle = rgb(BARE[phase], 0.8);
  ctx.fillRect(v.ox, v.oy, W * t, layout.h * t);
  for (let y = 0; y < layout.h; y++) {
    for (let x = 0; x < W; x++) {
      const face = g[y * W + x];
      ctx.fillStyle = tileColour(face, phase, x, y);
      ctx.fillRect(v.ox + x * t, v.oy + y * t, t + 0.6, t + 0.6);
    }
  }
  // Detail, by what is on the tile.
  for (let y = 0; y < layout.h; y++) {
    for (let x = 0; x < W; x++) {
      const face = g[y * W + x];
      if (!face) {
        if (phase === '1890' && hash32(x * 31 + y * 17) < 0.18) {
          ctx.fillStyle = 'rgba(120,150,80,0.35)';
          ctx.fillRect(v.ox + (x + hash32(x + y)) * t, v.oy + (y + 0.5) * t, t * 0.14, t * 0.3);
        }
        continue;
      }
      const px = v.ox + x * t;
      const py = v.oy + y * t;
      const h = hash32(x * 9973 + y * 31337);
      const up = y > 0 ? g[(y - 1) * W + x] : null;
      const left = x > 0 ? g[y * W + x - 1] : null;
      switch (face.glyph) {
        case 'T': // oak wood: canopies
          for (let k = 0; k < 2; k++) {
            const cx = px + t * (0.25 + 0.5 * hash32(h * 1e6 + k));
            const cy = py + t * (0.25 + 0.5 * hash32(h * 2e6 + k));
            ctx.fillStyle = k ? 'rgba(20,48,24,0.9)' : 'rgba(64,112,58,0.85)';
            ctx.beginPath();
            ctx.arc(cx, cy, t * (0.32 + 0.18 * hash32(h * 3e6 + k)), 0, TAU);
            ctx.fill();
          }
          break;
        case '"': // scrub
          ctx.fillStyle = 'rgba(150,160,100,0.35)';
          ctx.fillRect(px + h * t * 0.7, py + t * 0.4, t * 0.25, t * 0.18);
          break;
        case '#': // the A-road: a centre line
          if (up && up.glyph === '#' && x % 3 !== 0) {
            ctx.fillStyle = 'rgba(230,220,170,0.45)';
            ctx.fillRect(px, py - t * 0.06, t * 0.7, t * 0.12);
          }
          break;
        case '-': // a cart track: ruts
          ctx.fillStyle = 'rgba(80,64,40,0.5)';
          ctx.fillRect(px, py + t * 0.3, t, t * 0.08);
          ctx.fillRect(px, py + t * 0.62, t, t * 0.08);
          break;
        case '&': // Tarrant Street: terraces either side of a road
          if ((y % 4) !== 1) {
            ctx.fillStyle = h < 0.5 ? 'rgba(120,96,86,0.9)' : 'rgba(100,84,80,0.9)';
            ctx.fillRect(px + t * 0.05, py + t * 0.1, t * 0.9, t * 0.8);
            ctx.fillStyle = 'rgba(0,0,0,0.25)';
            ctx.fillRect(px + t * 0.05, py + t * 0.72, t * 0.9, t * 0.18);
          }
          break;
        case 'W': // standing works: sawtooth roofs
          ctx.fillStyle = 'rgba(0,0,0,0.28)';
          if (x % 2 === 0) ctx.fillRect(px, py, t * 0.4, t);
          break;
        case 'H': // sheds: corrugated
          ctx.fillStyle = 'rgba(255,255,255,0.06)';
          ctx.fillRect(px, py + (y % 2) * t * 0.5, t, t * 0.2);
          break;
        case 'A': // a tower
          if (!left || left.glyph !== 'A') {
            ctx.fillStyle = 'rgba(0,0,0,0.35)';
            ctx.beginPath();
            ctx.arc(px + t * 1.8, py + t * 1.8, t * 1.3, 0, TAU);
            ctx.fill();
            ctx.fillStyle = 'rgba(210,205,196,0.9)';
            ctx.beginPath();
            ctx.arc(px + t * 1.5, py + t * 1.5, t * 1.1, 0, TAU);
            ctx.fill();
            ctx.fillStyle = '#222';
            ctx.beginPath();
            ctx.arc(px + t * 1.5, py + t * 1.5, t * 0.5, 0, TAU);
            ctx.fill();
          }
          break;
        case 'Y': // the pylon line
          ctx.strokeStyle = 'rgba(214,196,108,0.5)';
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(px, py + t * 0.5);
          ctx.lineTo(px + t, py + t * 0.5);
          ctx.stroke();
          if (x % 6 === 0) {
            ctx.fillStyle = 'rgba(230,214,140,0.9)';
            ctx.fillRect(px + t * 0.3, py + t * 0.2, t * 0.4, t * 0.6);
          }
          break;
        case 'O': // ore at the surface
        case 'c':
          for (let k = 0; k < 3; k++) {
            ctx.fillStyle = face.glyph === 'O' ? 'rgba(190,100,60,0.7)' : 'rgba(200,200,220,0.25)';
            ctx.fillRect(px + t * hash32(h * 7e5 + k), py + t * hash32(h * 9e5 + k), t * 0.18, t * 0.18);
          }
          break;
        case 'x': // a spent body: pits
          if (h < 0.3) {
            ctx.fillStyle = 'rgba(0,0,0,0.35)';
            ctx.beginPath();
            ctx.arc(px + t * 0.5, py + t * 0.5, t * 0.3, 0, TAU);
            ctx.fill();
          }
          break;
        case 's': // slag
          ctx.fillStyle = 'rgba(0,0,0,0.25)';
          ctx.beginPath();
          ctx.arc(px + t * h, py + t * 0.6, t * 0.45, Math.PI, TAU);
          ctx.fill();
          break;
        case '=': // a casting table: ripples
          ctx.strokeStyle = 'rgba(255,220,160,0.18)';
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(px, py + t * (0.3 + 0.4 * h));
          ctx.lineTo(px + t, py + t * (0.3 + 0.4 * h));
          ctx.stroke();
          break;
        case '*': // the natural fracture
          ctx.fillStyle = `rgba(190,140,230,${0.25 + 0.3 * h})`;
          ctx.fillRect(px, py, t, t);
          break;
        case '@': // the gantry's apron
          ctx.strokeStyle = 'rgba(40,30,20,0.6)';
          ctx.lineWidth = 1;
          ctx.strokeRect(px + 0.5, py + 0.5, t - 1, t - 1);
          break;
        default:
          break;
      }
    }
  }
}

/** The river, which moves, painted over whichever century is showing. */
function river(ctx, v, layout, phase, tick) {
  const g = faceGrid(layout)[phase];
  const t = v.tile;
  ctx.strokeStyle = 'rgba(180,220,255,0.10)';
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let y = 0; y < layout.h; y++) {
    for (let x = 0; x < layout.w; x++) {
      const f = g[y * layout.w + x];
      if (!f || f.glyph !== '~') continue;
      if (hash32(x * 13 + y * 7) > 0.35) continue;
      const off = ((tick * 0.02 + hash32(x + y * 99)) % 1) * t;
      ctx.moveTo(v.ox + x * t + off, v.oy + (y + 0.5) * t);
      ctx.lineTo(v.ox + x * t + off + t * 0.5, v.oy + (y + 0.5) * t);
    }
  }
  ctx.stroke();
}

/**
 * One century's ground, as an image: cached by the page when it can make an
 * offscreen canvas, painted straight onto `ctx` when it cannot.
 */
function ground(ctx, v, layout, phase, opts) {
  if (!opts || !opts.canvas) {
    paintGround(ctx, v, layout, phase);
    return;
  }
  const cache = (opts.cache = opts.cache || {});
  const key = `${phase}|${v.w}|${v.h}|${v.scale}`;
  if (!cache[phase] || cache[phase].key !== key) {
    const cv = opts.canvas(v.w, v.h);
    paintGround(cv.getContext('2d'), v, layout, phase);
    cache[phase] = { key, cv };
  }
  ctx.drawImage(cache[phase].cv, 0, 0);
}

// ------------------------------------------------------------------- layers

/** A wobbling circle: the edge of a bleed is never quite still. */
function wobble(ctx, x, y, r, tick, seed) {
  ctx.beginPath();
  const n = 48;
  for (let i = 0; i <= n; i++) {
    const a = (i / n) * TAU;
    const w = 1 + 0.06 * Math.sin(a * 5 + tick * 0.05 + seed) + 0.04 * Math.sin(a * 11 - tick * 0.08 + seed * 3);
    const px = x + Math.cos(a) * r * w;
    const py = y + Math.sin(a) * r * w;
    i ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
  }
  ctx.closePath();
}

function bleeds(ctx, v, layout, frame, tick, opts) {
  let n = 0;
  (frame.sites || []).forEach((s, i) => {
    if (!s.bleed || !s.source) return;
    n++;
    const at = layout.sites[i].at;
    const x = sx(v, at);
    const y = sy(v, at);
    // It opens out from nothing over a second, rather than snapping.
    const grow = Math.min(1, Math.max(0.05, (tick - (s.torn || 0)) / 50));
    const r = s.bleed * v.scale * grow;
    const era = ERA[s.source] || ERA[2037];
    ctx.save();
    wobble(ctx, x, y, r, tick, i * 1.7);
    ctx.clip();
    ground(ctx, v, layout, s.source, opts);
    river(ctx, v, layout, s.source, tick);
    ctx.fillStyle = radial(ctx, x, y, r, [[0, `${era.tint}0.05)`], [0.75, `${era.tint}0.14)`], [1, `${era.tint}0.32)`]], `${era.tint}0.14)`);
    ctx.fillRect(x - r * 1.2, y - r * 1.2, r * 2.4, r * 2.4);
    // Motes: dust in 1890's air, scan-glitter in 2070's.
    for (let k = 0; k < 26; k++) {
      const a = hash32(k * 97 + i) * TAU;
      const d = r * Math.sqrt(hash32(k * 131 + i * 7));
      const drift = (tick * (0.15 + hash32(k) * 0.2)) % (v.tile * 3);
      ctx.fillStyle = `${era.glow}${0.25 + 0.35 * hash32(k * 5 + Math.floor(tick / 8))})`;
      ctx.fillRect(x + Math.cos(a) * d, y + Math.sin(a) * d - drift, s.source === '2070' ? 3 : 2, s.source === '2070' ? 1 : 2);
    }
    ctx.restore();
    ctx.save();
    ctx.shadowColor = era.ink;
    ctx.shadowBlur = 14;
    ctx.strokeStyle = `${era.glow}0.8)`;
    ctx.lineWidth = 2;
    wobble(ctx, x, y, r, tick, i * 1.7);
    ctx.stroke();
    ctx.restore();
    label(ctx, `${s.source} BLEEDING THROUGH`, x, y - r - 6, era.ink, Math.max(9, v.tile * 0.8), 700, 'center');
  });
  return n;
}

function sectors(ctx, v, layout, frame, tick) {
  layout.sectors.forEach((s, i) => {
    const live = (frame.sectors && frame.sectors[i]) || {};
    const awake = live.mode === 'awake';
    const x = sx(v, [s.rect.x0, 0]);
    const y = sy(v, [0, s.rect.y0]);
    const w = (s.rect.x1 - s.rect.x0) * v.scale;
    const h = (s.rect.y1 - s.rect.y0) * v.scale;
    const c = awake ? COLORS.awake : COLORS.closed;
    ctx.strokeStyle = c;
    ctx.lineWidth = awake ? 1.6 : 1;
    if (awake) {
      if (ctx.setLineDash) ctx.setLineDash([6, 5]);
      ctx.lineDashOffset = -tick * 0.4;
      ctx.strokeRect(x, y, w, h);
      if (ctx.setLineDash) ctx.setLineDash([]);
      ctx.fillStyle = 'rgba(224,160,92,0.05)';
      ctx.fillRect(x, y, w, h);
    } else {
      // Corners only: a closed sector is a closed form, not a fence.
      const k = Math.min(w, h, v.tile * 2);
      ctx.beginPath();
      for (const [cx, cy, dx, dy] of [[x, y, 1, 1], [x + w, y, -1, 1], [x, y + h, 1, -1], [x + w, y + h, -1, -1]]) {
        ctx.moveTo(cx + dx * k, cy);
        ctx.lineTo(cx, cy);
        ctx.lineTo(cx, cy + dy * k);
      }
      ctx.stroke();
    }
    const size = Math.max(9, v.tile * 0.75);
    label(ctx, s.name.toUpperCase(), x + 5, y + size + 3, c, size, 700);
    const sub = awake
      ? `AWAKE · ${fmt(live.stepped || 0)} rounds stepped`
      : `closed form · orbit ${live.period || '?'}t`;
    label(ctx, sub, x + 5, y + size * 2 + 5, COLORS.muted, size * 0.85, 500);
  });
}

function domain(ctx, v, layout, frame, tick) {
  if (!frame.domain || !frame.domain.open) return;
  const R = layout.bleedR[layout.bleedR.length - 1];
  (frame.sites || []).forEach((s, i) => {
    if (s.torn == null) return;
    const at = layout.sites[i].at;
    ctx.strokeStyle = 'rgba(232,100,106,0.45)';
    ctx.lineWidth = 1;
    if (ctx.setLineDash) ctx.setLineDash([3, 7]);
    ctx.lineDashOffset = tick * 0.3;
    ctx.beginPath();
    ctx.arc(sx(v, at), sy(v, at), R * v.scale, 0, TAU);
    ctx.stroke();
    if (ctx.setLineDash) ctx.setLineDash([]);
  });
}

/** What is crossing: a stream of loads along each lane, coloured by century. */
function streams(ctx, v, layout, frame, tick) {
  let drawn = 0;
  (layout.lanes || []).forEach((l, i) => {
    const live = (frame.lanes && frame.lanes[i]) || { rate: 0, flow: 0 };
    const rate = Number(live.rate);
    const flow = Number(live.flow) / 1000;
    const launder = l.launderPath && frame.laundering;
    const path = launder ? l.launderPath : l.path;
    const era = ERA[l.origin];
    const pts = path.map(p => [sx(v, p), sy(v, p)]);
    ctx.lineCap = 'round';
    ctx.lineJoin = 'round';
    ctx.strokeStyle = rate > 0 ? `${era.glow}${flow > 0 ? 0.28 : 0.1})` : 'rgba(255,255,255,0.05)';
    ctx.lineWidth = v.tile * 0.55;
    ctx.beginPath();
    pts.forEach((p, k) => (k ? ctx.lineTo(p[0], p[1]) : ctx.moveTo(p[0], p[1])));
    ctx.stroke();
    if (flow <= 0) return;
    drawn++;
    const n = Math.min(40, 6 + Math.round(6 * Math.log10(flow + 1)));
    const speed = 0.004 + 0.0015 * Math.log10(flow + 1);
    ctx.fillStyle = era.ink;
    for (let k = 0; k < n; k++) {
      const f = (k / n + tick * speed) % 1;
      const p = along(path, f);
      ctx.fillRect(sx(v, p) - 1.5, sy(v, p) - 1.5, 3, 3);
      // Laundered ore changes century at the crusher: the last stretch is 2037.
    }
    const mid = along(path, 0.55);
    label(ctx, `${l.item} ${fmt(flow)} t/s`, sx(v, mid) + 8, sy(v, mid) + (i ? 14 : -6), era.ink, Math.max(8, v.tile * 0.62), 600);
    if (launder) {
      const end = path[path.length - 1];
      label(ctx, '→ 2037 concentrate', sx(v, end) + v.tile * 3.4, sy(v, end) + 4, ERA[2037].ink, Math.max(8, v.tile * 0.6), 600);
    }
  });
  return drawn;
}

function anchors(ctx, v, layout, frame, tick) {
  (layout.anchors || []).forEach((a, i) => {
    const live = (frame.anchors && frame.anchors[i]) || {};
    const x = sx(v, a.at);
    const y = sy(v, a.at);
    const r = a.range * v.scale;
    if (live.live) {
      ctx.fillStyle = radial(ctx, x, y, r, [[0, 'rgba(143,211,192,0.10)'], [0.85, 'rgba(143,211,192,0.05)'], [1, 'rgba(143,211,192,0.18)']], 'rgba(143,211,192,0.06)');
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      ctx.fill();
      ctx.strokeStyle = 'rgba(143,211,192,0.55)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      // A hexagonal ward, turning slowly: time held still.
      for (let k = 0; k <= 6; k++) {
        const ang = (k / 6) * TAU + tick * 0.004;
        const px = x + Math.cos(ang) * r;
        const py = y + Math.sin(ang) * r;
        k ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
      }
      ctx.stroke();
    } else {
      ctx.strokeStyle = 'rgba(143,211,192,0.12)';
      if (ctx.setLineDash) ctx.setLineDash([2, 6]);
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      ctx.stroke();
      if (ctx.setLineDash) ctx.setLineDash([]);
    }
    // The pylon.
    const s = v.tile * 0.9;
    ctx.fillStyle = 'rgba(0,0,0,0.5)';
    ctx.fillRect(x - s + 3, y - s + 3, s * 2, s * 2);
    ctx.fillStyle = live.live ? '#2b4a42' : '#2a2f2e';
    ctx.fillRect(x - s, y - s, s * 2, s * 2);
    ctx.strokeStyle = live.live ? '#8fd3c0' : '#56615d';
    ctx.lineWidth = 1.5;
    ctx.strokeRect(x - s, y - s, s * 2, s * 2);
    if (live.live) {
      const p = 0.5 + 0.5 * Math.sin(tick * 0.12 + i);
      ctx.fillStyle = `rgba(190,255,235,${0.5 + 0.5 * p})`;
      ctx.beginPath();
      ctx.arc(x, y, s * 0.45, 0, TAU);
      ctx.fill();
    }
    label(ctx, `${a.name}${live.on && !live.live ? ' (down)' : ''}`, x, y + s + v.tile * 0.9, live.live ? '#8fd3c0' : COLORS.muted, Math.max(8, v.tile * 0.6), 600, 'center');
  });
}

function block(ctx, x, y, w, h, roof, band) {
  ctx.fillStyle = 'rgba(0,0,0,0.45)';
  ctx.fillRect(x + 4, y + 4, w, h);
  ctx.fillStyle = roof;
  ctx.fillRect(x, y, w, h);
  ctx.fillStyle = 'rgba(255,255,255,0.08)';
  ctx.fillRect(x, y, w, 2);
  if (band === 'damaged' || band === 'critical') {
    ctx.strokeStyle = 'rgba(0,0,0,0.6)';
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let k = 0; k < (band === 'damaged' ? 3 : 7); k++) {
      const px = x + w * hash32(k * 7 + w);
      const py = y + h * hash32(k * 11 + h);
      ctx.moveTo(px, py);
      ctx.lineTo(px + (hash32(k + 2) - 0.5) * w * 0.4, py + (hash32(k + 3) - 0.5) * h * 0.4);
    }
    ctx.stroke();
  }
}

function structures(ctx, v, layout, frame, tick) {
  layout.structures.forEach((s, i) => {
    if (s.kind === 'anchor' || s.kind === 'pit') return;
    const st = (frame.structures && frame.structures[i]) || { hp: s.hp, max: s.hp, band: 'intact' };
    const cx = sx(v, s.at);
    const cy = sy(v, s.at);
    const w = s.w * v.tile;
    const h = s.h * v.tile;
    const x = cx - w / 2;
    const y = cy - h / 2;
    const site = layout.sites.findIndex(d => d.structure === i);
    const live = site >= 0 && frame.sites ? frame.sites[site] : null;
    const strain = live ? strainAt(live, tick) : [0, 0];
    if (st.band === 'destroyed') {
      ctx.fillStyle = '#2e2926';
      for (let k = 0; k < 22; k++) {
        const r = v.tile * (0.2 + 0.4 * hash32(k + i * 5));
        ctx.fillRect(cx + (hash32(i * 97 + k) - 0.5) * w * 1.2 - r, cy + (hash32(i * 97 + k + 40) - 0.5) * h * 1.1 - r, r * 2, r * 2);
      }
      label(ctx, `${s.name} · rubble`, cx, y + h + v.tile, COLORS.muted, Math.max(8, v.tile * 0.6), 600, 'center');
      return;
    }
    if (s.kind === 'gantry') {
      // A steel frame over the Rift: lit when it is holding, dark when not.
      const lit = !frame.dark;
      ctx.fillStyle = 'rgba(0,0,0,0.5)';
      ctx.fillRect(x + 5, y + 5, w, h);
      ctx.strokeStyle = lit ? '#c9a46a' : '#5b5249';
      ctx.lineWidth = Math.max(2, v.tile * 0.35);
      ctx.strokeRect(x, y, w, h);
      ctx.lineWidth = 1.2;
      ctx.beginPath();
      ctx.moveTo(x, y);
      ctx.lineTo(x + w, y + h);
      ctx.moveTo(x + w, y);
      ctx.lineTo(x, y + h);
      ctx.moveTo(x + w / 2, y);
      ctx.lineTo(x + w / 2, y + h);
      ctx.stroke();
      for (let k = 0; k < 4; k++) {
        const on = lit && Math.floor(tick / 20 + k) % 2 === 0;
        ctx.fillStyle = on ? '#ffd27a' : '#3a342c';
        const lx = k % 2 ? x + w : x;
        const ly = k < 2 ? y : y + h;
        ctx.beginPath();
        ctx.arc(lx, ly, v.tile * 0.3, 0, TAU);
        ctx.fill();
      }
      label(ctx, lit ? 'GANTRY' : 'GANTRY · DARK', cx, y - v.tile * 0.6, lit ? '#e8c889' : COLORS.domain, Math.max(9, v.tile * 0.7), 700, 'center');
    } else if (s.kind === 'yard') {
      block(ctx, x, y, w, h, '#3b3a36', st.band);
      // Stockpiles of 1890 ore, as many as there is strain in them, glowing
      // faintly with how far out of their time they are.
      const n = Math.min(9, Math.ceil(strain[0] / (layout.sites[site].open / 4)));
      for (let k = 0; k < n; k++) {
        const px = x + w * (0.15 + 0.7 * ((k % 3) / 2));
        const py = y + h * (0.25 + 0.5 * (Math.floor(k / 3) / 2));
        ctx.fillStyle = 'rgba(160,82,48,0.95)';
        ctx.beginPath();
        ctx.arc(px, py, v.tile * 0.75, Math.PI, TAU);
        ctx.fill();
        ctx.fillStyle = `${ERA[1890].glow}${0.15 + 0.1 * Math.sin(tick * 0.08 + k)})`;
        ctx.beginPath();
        ctx.arc(px, py, v.tile * 1.1, 0, TAU);
        ctx.fill();
      }
      label(ctx, s.name, cx, y + h + v.tile * 0.9, COLORS.ink, Math.max(8, v.tile * 0.62), 600, 'center');
    } else if (s.kind === 'shop') {
      block(ctx, x, y, w, h, '#40444a', st.band);
      // The 2070 lathes, visible through the roof lights.
      const glow = Math.min(1, strain[1] / layout.sites[site].open);
      for (let k = 0; k < 5; k++) {
        const on = 0.4 + 0.6 * Math.abs(Math.sin(tick * 0.07 + k * 1.3));
        ctx.fillStyle = `${ERA[2070].glow}${(0.15 + 0.7 * glow) * on})`;
        ctx.fillRect(x + w * (0.1 + 0.17 * k), y + h * 0.3, w * 0.1, h * 0.4);
      }
      label(ctx, s.name, cx, y + h + v.tile * 0.9, COLORS.ink, Math.max(8, v.tile * 0.62), 600, 'center');
    } else if (s.kind === 'crusher') {
      block(ctx, x, y, w, h, frame.laundering ? '#4a4538' : '#3a3833', st.band);
      if (frame.laundering) {
        ctx.fillStyle = 'rgba(200,190,160,0.25)';
        for (let k = 0; k < 4; k++) {
          const a = (tick * 0.03 + k) % 1;
          ctx.beginPath();
          ctx.arc(x + w * 0.7 + a * v.tile, y - a * v.tile * 2, v.tile * (0.4 + a), 0, TAU);
          ctx.fill();
        }
      }
      label(ctx, s.name, cx, y + h + v.tile * 0.9, COLORS.ink, Math.max(8, v.tile * 0.62), 600, 'center');
    } else if (s.kind === 'works') {
      label(ctx, `${s.name} · outside every reach`, cx, y - v.tile * 0.5, COLORS.muted, Math.max(8, v.tile * 0.62), 600, 'center');
    }
    if (st.band === 'critical') {
      for (let k = 0; k < 3; k++) {
        const fl = 0.6 + 0.4 * Math.sin(tick * 0.5 + k * 2);
        ctx.fillStyle = `rgba(255,138,60,${0.35 * fl})`;
        ctx.beginPath();
        ctx.arc(cx + (hash32(i + k * 3) - 0.5) * w * 0.8, cy + (hash32(i + k * 7) - 0.5) * h * 0.8, v.tile * (0.6 + 0.3 * fl), 0, TAU);
        ctx.fill();
      }
    }
    const frac = st.max ? st.hp / st.max : 1;
    if (frac < 0.999) {
      const bw = Math.max(w, v.tile * 3);
      ctx.fillStyle = 'rgba(0,0,0,0.7)';
      ctx.fillRect(cx - bw / 2, y - 7, bw, 3);
      ctx.fillStyle = frac > 0.75 ? '#7ed9a0' : frac > 0.4 ? '#e0a05c' : '#e06c6c';
      ctx.fillRect(cx - bw / 2, y - 7, bw * frac, 3);
    }
  });
}

/**
 * Strain at a site, before it tears: a ring filling toward the threshold in
 * the colours of the centuries it is made of, and the light going wrong.
 */
function strains(ctx, v, layout, frame, tick) {
  (frame.sites || []).forEach((s, i) => {
    const def = layout.sites[i];
    const [a, b] = strainAt(s, tick);
    const total = a + b;
    if (total <= 0 && s.torn == null) return;
    const x = sx(v, def.at);
    const y = sy(v, def.at);
    const f = total / def.open;
    const R = v.tile * 3.6;
    const src = a >= b ? '1890' : '2070';
    // Haze: the air over a strained site shimmers before anything tears.
    const haze = Math.min(1, f);
    ctx.fillStyle = radial(ctx, x, y, R * (1.2 + haze), [[0, `${ERA[src].glow}${0.22 * haze})`], [1, `${ERA[src].glow}0)`]], `${ERA[src].glow}${0.08 * haze})`);
    ctx.beginPath();
    ctx.arc(x, y, R * (1.2 + haze), 0, TAU);
    ctx.fill();
    // The gauge: 1890 then 2070, clockwise from the top, as far as the tear.
    ctx.lineWidth = 3;
    ctx.strokeStyle = 'rgba(0,0,0,0.5)';
    ctx.beginPath();
    ctx.arc(x, y, R, 0, TAU);
    ctx.stroke();
    const start = -Math.PI / 2;
    const fa = Math.min(1, a / def.open);
    const fb = Math.min(1 - fa, b / def.open);
    const flick = f > 0.85 && s.torn == null ? 0.6 + 0.4 * Math.sin(tick * 0.9) : 1;
    ctx.strokeStyle = ERA[1890].ink;
    ctx.globalAlpha = flick;
    ctx.beginPath();
    ctx.arc(x, y, R, start, start + fa * TAU);
    ctx.stroke();
    ctx.strokeStyle = ERA[2070].ink;
    ctx.beginPath();
    ctx.arc(x, y, R, start + fa * TAU, start + (fa + fb) * TAU);
    ctx.stroke();
    ctx.globalAlpha = 1;
    // Past the tear, rings for the bleed bands it has reached.
    for (let k = 1; k < layout.bleedX.length; k++) {
      if (f >= layout.bleedX[k]) {
        ctx.strokeStyle = `${ERA[src].glow}0.5)`;
        ctx.lineWidth = 1;
        ctx.beginPath();
        ctx.arc(x, y, R + 4 * k, 0, TAU);
        ctx.stroke();
      }
    }
    const size = Math.max(8, v.tile * 0.58);
    label(ctx, `${fmt(total / 1000)} ty · ${Math.round(f * 100)}%`, x, y + R + size + 2, f >= 1 ? COLORS.domain : COLORS.muted, size, 600, 'center');
  });
}

/** A jagged line from `a` to `b`, re-rolled every few ticks. */
function crack(ctx, x0, y0, x1, y1, seed, n = 9, jitter = 0.35) {
  ctx.beginPath();
  ctx.moveTo(x0, y0);
  const dx = x1 - x0;
  const dy = y1 - y0;
  const len = Math.hypot(dx, dy) || 1;
  for (let k = 1; k < n; k++) {
    const f = k / n;
    const off = (hash32(seed * 131 + k) - 0.5) * len * jitter;
    ctx.lineTo(x0 + dx * f + (-dy / len) * off, y0 + dy * f + (dx / len) * off);
  }
  ctx.lineTo(x1, y1);
}

function ruptures(ctx, v, layout, frame, tick) {
  let n = 0;
  (frame.sites || []).forEach((s, i) => {
    if (s.torn == null) return;
    n++;
    const at = layout.sites[i].at;
    const x = sx(v, at);
    const y = sy(v, at);
    const era = ERA[s.source] || ERA[2037];
    const t = v.tile;
    const pulse = 0.7 + 0.3 * Math.sin(tick * 0.2 + i);
    const age = tick - s.torn;
    const open = Math.min(1, Math.max(0.1, age / 30));
    // Light leaking out of it.
    ctx.fillStyle = radial(ctx, x, y, t * 5 * open, [[0, `${era.glow}${0.55 * pulse})`], [0.4, `${era.glow}0.18)`], [1, `${era.glow}0)`]], `${era.glow}0.2)`);
    ctx.beginPath();
    ctx.arc(x, y, t * 5 * open, 0, TAU);
    ctx.fill();
    // Rays.
    ctx.strokeStyle = `${era.glow}${0.25 * pulse})`;
    ctx.lineWidth = 1;
    ctx.beginPath();
    for (let k = 0; k < 10; k++) {
      const a = hash32(k * 17 + i) * TAU + tick * 0.003;
      const l = t * (2 + 3 * hash32(k * 23 + Math.floor(tick / 10)));
      ctx.moveTo(x + Math.cos(a) * t * 0.8, y + Math.sin(a) * t * 0.8);
      ctx.lineTo(x + Math.cos(a) * l * open, y + Math.sin(a) * l * open);
    }
    ctx.stroke();
    // The tear itself: a crack in the air, white at its heart.
    const seed = Math.floor(tick / 5) + i * 1000;
    const h = t * 3.2 * open;
    ctx.save();
    ctx.shadowColor = era.ink;
    ctx.shadowBlur = 18;
    ctx.strokeStyle = era.ink;
    ctx.lineWidth = Math.max(3, t * 0.55);
    crack(ctx, x - t * 0.4, y - h, x + t * 0.3, y + h, seed);
    ctx.stroke();
    ctx.restore();
    ctx.strokeStyle = '#fffaf0';
    ctx.lineWidth = Math.max(1, t * 0.18);
    crack(ctx, x - t * 0.4, y - h, x + t * 0.3, y + h, seed);
    ctx.stroke();
    if (s.pinned) {
      // Held in place: the anchor's lattice closes round it.
      ctx.strokeStyle = 'rgba(143,211,192,0.8)';
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      for (let k = 0; k <= 6; k++) {
        const a = (k / 6) * TAU - tick * 0.01;
        const px = x + Math.cos(a) * t * 2.6;
        const py = y + Math.sin(a) * t * 2.6;
        k ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
      }
      ctx.stroke();
    }
    label(ctx, `${layout.sites[i].name.toUpperCase()} TORN · ${s.source}${s.pinned ? ' · PINNED' : ''}`, x, y - h - t * 0.8, era.ink, Math.max(9, t * 0.75), 800, 'center');
  });
  return n;
}

function batteries(ctx, v, layout, frame, tick) {
  let flashes = 0;
  layout.batteries.forEach((b, i) => {
    const live = (frame.batteries && frame.batteries[i]) || { heading: 0, duty: 'idle', alive: true, powered: true };
    const cx = sx(v, b.at);
    const cy = sy(v, b.at);
    const working = live.alive && live.powered !== false;
    if (working) {
      ctx.strokeStyle = live.duty === 'idle' ? 'rgba(200,210,205,0.07)' : 'rgba(255,224,138,0.18)';
      ctx.lineWidth = 1;
      if (ctx.setLineDash) ctx.setLineDash([2, 6]);
      ctx.beginPath();
      ctx.arc(cx, cy, b.range * v.scale, 0, TAU);
      ctx.stroke();
      if (ctx.setLineDash) ctx.setLineDash([]);
    }
    // The pit.
    ctx.fillStyle = 'rgba(0,0,0,0.55)';
    ctx.beginPath();
    ctx.arc(cx, cy, v.tile * 1.5, 0, TAU);
    ctx.fill();
    ctx.strokeStyle = working ? '#6f7c76' : '#3a3a3a';
    ctx.lineWidth = 2;
    ctx.stroke();
    const a = heading(live, tick);
    const n = b.count;
    const ring = n > 1 ? v.tile * 0.8 : 0;
    const howitzer = b.gun === 'howitzer';
    const since = live.lastFire != null ? tick - live.lastFire : Infinity;
    for (let k = 0; k < n; k++) {
      const ka = (k / n) * TAU + 0.6;
      const tx = cx + Math.cos(ka) * ring;
      const ty = cy + Math.sin(ka) * ring;
      const len = v.tile * (howitzer ? 1.5 : 1.1);
      const kick = since < 8 ? (1 - since / 8) * v.tile * 0.35 : 0;
      const bx = tx + Math.cos(a) * (len - kick);
      const by = ty + Math.sin(a) * (len - kick);
      ctx.fillStyle = working ? (howitzer ? '#7c8a84' : COLORS.steel) : '#3a3a3a';
      ctx.beginPath();
      ctx.arc(tx, ty, v.tile * (howitzer ? 0.5 : 0.38), 0, TAU);
      ctx.fill();
      ctx.strokeStyle = working ? '#dfe8e4' : '#474747';
      ctx.lineWidth = Math.max(1.5, v.tile * (howitzer ? 0.3 : 0.18));
      ctx.beginPath();
      ctx.moveTo(tx, ty);
      ctx.lineTo(bx, by);
      ctx.stroke();
      if (working && since < 6) {
        flashes++;
        const r = v.tile * (howitzer ? 1.3 : 0.8) * (1 - since / 6);
        ctx.fillStyle = COLORS.flash;
        ctx.beginPath();
        for (let s = 0; s < 8; s++) {
          const sa = a + (s / 8) * TAU;
          const rr = s % 2 ? r * 0.4 : r;
          const px = bx + Math.cos(a) * r * 0.5 + Math.cos(sa) * rr;
          const py = by + Math.sin(a) * r * 0.5 + Math.sin(sa) * rr;
          s ? ctx.lineTo(px, py) : ctx.moveTo(px, py);
        }
        ctx.fill();
      }
    }
    const note = !live.alive ? ' · pit lost' : live.powered === false ? ' · NO GRID' : live.hold ? ' · hold' : '';
    label(ctx, `${b.name}${note}`, cx, cy + v.tile * 2.4, live.powered === false ? ERA[1890].ink : working ? COLORS.ink : COLORS.muted, Math.max(8, v.tile * 0.58), 600, 'center');
  });
  return flashes;
}

function volleys(ctx, v, frame, tick) {
  let drawn = 0;
  for (const vo of frame.volleys || []) {
    const { pos, f, lob } = volleyAt(vo, tick);
    if (f >= 1) continue;
    drawn++;
    const x = sx(v, pos);
    const y = sy(v, pos);
    if (vo.gun === 'howitzer') {
      ctx.fillStyle = 'rgba(0,0,0,0.45)';
      ctx.beginPath();
      ctx.ellipse ? ctx.ellipse(x, y, 3, 1.5, 0, 0, TAU) : ctx.arc(x, y, 2, 0, TAU);
      ctx.fill();
      const back = volleyAt(vo, tick - 4);
      ctx.strokeStyle = 'rgba(245,240,224,0.35)';
      ctx.lineWidth = 1.5;
      ctx.beginPath();
      ctx.moveTo(sx(v, back.pos), sy(v, back.pos) - back.lob * v.scale);
      ctx.lineTo(x, y - lob * v.scale);
      ctx.stroke();
      ctx.fillStyle = COLORS.shell;
      for (let k = 0; k < vo.shells; k++) {
        ctx.beginPath();
        ctx.arc(x + (k - (vo.shells - 1) / 2) * 3, y - lob * v.scale, 2.2, 0, TAU);
        ctx.fill();
      }
    } else {
      const back = volleyAt(vo, tick - 3);
      ctx.strokeStyle = COLORS.tracer;
      ctx.lineWidth = 1.6;
      ctx.beginPath();
      const dx = sx(v, back.pos) - x;
      const dy = sy(v, back.pos) - y;
      const nx = -dy / (Math.hypot(dx, dy) || 1);
      const ny = dx / (Math.hypot(dx, dy) || 1);
      for (let k = 0; k < vo.shells; k++) {
        const o = (k - (vo.shells - 1) / 2) * 3;
        ctx.moveTo(x + nx * o, y + ny * o);
        ctx.lineTo(x + dx + nx * o, y + dy + ny * o);
      }
      ctx.stroke();
    }
  }
  return drawn;
}

/**
 * Manifestations. An echo is a figure of 1890 smoke, drifting, trailing; a
 * glint is a shard of something not yet built, turning. A cohort is drawn as
 * a crowd of representative figures around its one position.
 */
function cohorts(ctx, v, layout, frame, tick) {
  let dots = 0;
  for (const c of frame.cohorts || []) {
    const p = cohortPos(layout, c, tick);
    const cx = sx(v, p);
    const cy = sy(v, p);
    const count = Number(c.count);
    const n = dotsFor(count);
    const radius = Math.max(v.tile * 0.6, c.spread * v.scale);
    const hurt = c.hp < c.hpMax;
    const era = ERA[c.origin] || ERA[1890];
    const dir = cohortDir(layout, c);
    const assault = c.site != null;
    ctx.fillStyle = radial(ctx, cx, cy, radius * 1.3, [[0, `${era.glow}0.22)`], [1, `${era.glow}0)`]], `${era.glow}0.08)`);
    ctx.beginPath();
    ctx.arc(cx, cy, radius * 1.3, 0, TAU);
    ctx.fill();
    const glint = c.kind === 'glint';
    for (let k = 0; k < n; k++) {
      const r = radius * Math.sqrt((k + 0.5) / n) * 0.95;
      const a0 = k * 2.39996 + c.id * 0.7;
      // Each figure wanders on its own small orbit, so a crowd seethes.
      const wander = assault ? 0.02 : 0.006;
      const a = a0 + Math.sin(tick * wander * (1 + (k % 5)) + k) * 0.5;
      const px = cx + Math.cos(a) * r;
      const py = cy + Math.sin(a) * r + (assault ? Math.sin(tick * 0.3 + k) * v.tile * 0.15 : 0);
      if (glint) {
        const s = Math.max(1.5, v.tile * 0.32);
        const rot = tick * 0.08 + k;
        ctx.fillStyle = hurt ? COLORS.hurt : era.ink;
        ctx.beginPath();
        ctx.moveTo(px + Math.cos(rot) * s, py + Math.sin(rot) * s);
        ctx.lineTo(px + Math.cos(rot + 1.9) * s * 0.4, py + Math.sin(rot + 1.9) * s * 0.4);
        ctx.lineTo(px - Math.cos(rot) * s, py - Math.sin(rot) * s);
        ctx.lineTo(px + Math.cos(rot - 1.9) * s * 0.4, py + Math.sin(rot - 1.9) * s * 0.4);
        ctx.closePath();
        ctx.fill();
        if (dir != null) {
          ctx.strokeStyle = `${era.glow}0.35)`;
          ctx.lineWidth = 1;
          ctx.beginPath();
          ctx.moveTo(px, py);
          ctx.lineTo(px - Math.cos(dir) * v.tile * 1.4, py - Math.sin(dir) * v.tile * 1.4);
          ctx.stroke();
        }
      } else {
        const s = Math.max(1.4, v.tile * 0.26);
        if (dir != null) {
          // A smear of smoke behind it.
          ctx.fillStyle = `${era.glow}0.18)`;
          ctx.beginPath();
          ctx.arc(px - Math.cos(dir) * s * 1.6, py - Math.sin(dir) * s * 1.6, s * 1.3, 0, TAU);
          ctx.fill();
        }
        ctx.fillStyle = hurt ? COLORS.hurt : era.ink;
        ctx.beginPath();
        ctx.arc(px, py - s * 0.6, s * 0.55, 0, TAU);
        ctx.fill();
        ctx.fillRect(px - s * 0.45, py - s * 0.1, s * 0.9, s * 1.2);
      }
    }
    dots += n;
    const size = Math.max(9, v.tile * 0.72);
    const text = `${c.kind} ×${short(count)}`;
    label(ctx, text, cx + radius * 0.8, cy - radius * 0.8, era.ink, size, 700);
    const pw = v.tile * 2;
    ctx.fillStyle = 'rgba(0,0,0,0.7)';
    ctx.fillRect(cx + radius * 0.8, cy - radius * 0.8 + 3, pw, 2.5);
    ctx.fillStyle = hurt ? COLORS.hurt : '#7ed9a0';
    ctx.fillRect(cx + radius * 0.8, cy - radius * 0.8 + 3, (pw * c.hp) / c.hpMax, 2.5);
  }
  return dots;
}

function effects(ctx, v, frame, tick) {
  let drawn = 0;
  for (const fx of frame.fx || []) {
    const age = tick - fx.at;
    if (age < 0) continue;
    const x = sx(v, fx.pos);
    const y = sy(v, fx.pos);
    const era = ERA[fx.kind === 1 ? 2070 : 1890];
    if (fx.what === 'impact') {
      if (age > 150) continue;
      drawn++;
      const r = Math.max(4, fx.r * v.scale);
      if (age < 6) {
        ctx.fillStyle = `rgba(255,244,194,${0.8 * (1 - age / 6)})`;
        ctx.beginPath();
        ctx.arc(x, y, r * (0.4 + age / 10), 0, TAU);
        ctx.fill();
      }
      if (age < 30) {
        ctx.strokeStyle = `rgba(255,138,60,${0.7 * (1 - age / 30)})`;
        ctx.lineWidth = 2;
        ctx.beginPath();
        ctx.arc(x, y, r * (0.3 + age / 30), 0, TAU);
        ctx.stroke();
      }
      ctx.fillStyle = `rgba(60,58,52,${0.4 * (1 - age / 150)})`;
      for (let k = 0; k < 3; k++) {
        ctx.beginPath();
        ctx.arc(x + (hash32(fx.at + k) - 0.5) * r + age * 0.08, y - age * 0.12 - k * 3, r * 0.3 + age * 0.05, 0, TAU);
        ctx.fill();
      }
    } else if (fx.what === 'tear' || fx.what === 'seal') {
      if (age > 90) continue;
      drawn++;
      const f = age / 90;
      const src = fx.kind === 1 ? ERA[2070] : ERA[1890];
      const r = v.tile * (fx.what === 'tear' ? 2 + 20 * f : 20 * (1 - f));
      ctx.strokeStyle = `${src.glow}${0.8 * (1 - f)})`;
      ctx.lineWidth = 3 * (1 - f) + 1;
      ctx.beginPath();
      ctx.arc(x, y, r, 0, TAU);
      ctx.stroke();
      if (fx.what === 'tear' && age < 8) {
        ctx.fillStyle = `rgba(255,250,240,${0.6 * (1 - age / 8)})`;
        ctx.fillRect(0, 0, v.w, v.h);
      }
    } else if (fx.what === 'emit') {
      if (age > 40) continue;
      drawn++;
      ctx.fillStyle = `${era.glow}${0.7 * (1 - age / 40)})`;
      for (let k = 0; k < 12; k++) {
        const a = hash32(fx.at * 7 + k) * TAU;
        const d = v.tile * (0.5 + age * 0.12) * (0.5 + hash32(k * 3 + fx.at));
        ctx.fillRect(x + Math.cos(a) * d, y + Math.sin(a) * d, 2, 2);
      }
    } else if (fx.what === 'fade') {
      if (age > 120) continue;
      drawn++;
      const f = age / 120;
      ctx.fillStyle = `${era.glow}${0.55 * (1 - f)})`;
      const r = Math.max(v.tile, fx.r * v.scale);
      for (let k = 0; k < 18; k++) {
        const a = hash32(fx.at * 11 + k) * TAU;
        const d = r * hash32(k * 5 + fx.at);
        ctx.beginPath();
        ctx.arc(x + Math.cos(a) * d, y + Math.sin(a) * d - age * 0.25 * (0.5 + hash32(k)), 1.5 + 2 * (1 - f), 0, TAU);
        ctx.fill();
      }
      if (age < 70) label(ctx, `${short(fx.n)} faded`, x, y - r - age * 0.2, `${era.glow}${1 - age / 70})`, Math.max(9, v.tile * 0.7), 700, 'center');
    } else if (fx.what === 'fall') {
      if (age > 180) continue;
      drawn++;
      ctx.fillStyle = `rgba(120,110,95,${0.5 * (1 - age / 180)})`;
      for (let k = 0; k < 6; k++) {
        ctx.beginPath();
        ctx.arc(x + (hash32(k + fx.at) - 0.5) * v.tile * 5, y + (hash32(k * 3 + fx.at) - 0.5) * v.tile * 5 - age * 0.05, v.tile * (1 + age / 60), 0, TAU);
        ctx.fill();
      }
    }
  }
  return drawn;
}

function vignette(ctx, v, layout, frame) {
  const cx = v.w / 2;
  const cy = v.h / 2;
  const r = Math.hypot(v.w, v.h) / 2;
  ctx.fillStyle = radial(ctx, cx, cy, r, [[0.55, 'rgba(0,0,0,0)'], [1, 'rgba(0,0,0,0.55)']], 'rgba(0,0,0,0)');
  ctx.fillRect(0, 0, v.w, v.h);
  const size = Math.max(12, v.tile * 1.6);
  label(ctx, `${layout.phase} · INDUSTRIAL DISTRICT`, v.ox + v.tile, v.oy + v.h * 0.04 + size, 'rgba(228,236,232,0.55)', size, 800);
  if (frame.dark) {
    label(ctx, 'INTERFACE DARK', v.ox + v.tile, v.oy + v.h * 0.04 + size * 2.1, COLORS.domain, size * 0.7, 800);
  }
}

// -------------------------------------------------------------------- scene

/**
 * Draw the whole district at `tick`. Returns what it drew, counted, so a test
 * can say "a bleed was drawn" without looking at a pixel.
 */
export function drawScene(ctx, layout, frame, tick, v, opts) {
  ctx.fillStyle = COLORS.night;
  ctx.fillRect(0, 0, v.w, v.h);
  ground(ctx, v, layout, layout.phase, opts);
  river(ctx, v, layout, layout.phase, tick);
  const bled = bleeds(ctx, v, layout, frame, tick, opts);
  sectors(ctx, v, layout, frame, tick);
  domain(ctx, v, layout, frame, tick);
  anchors(ctx, v, layout, frame, tick);
  const flowing = streams(ctx, v, layout, frame, tick);
  strains(ctx, v, layout, frame, tick);
  structures(ctx, v, layout, frame, tick);
  const torn = ruptures(ctx, v, layout, frame, tick);
  const flashes = batteries(ctx, v, layout, frame, tick);
  const dots = cohorts(ctx, v, layout, frame, tick);
  const shells = volleys(ctx, v, frame, tick);
  const fx = effects(ctx, v, frame, tick);
  vignette(ctx, v, layout, frame);
  return { cohorts: (frame.cohorts || []).length, dots, flashes, volleys: shells, fx, bleeds: bled, ruptures: torn, streams: flowing };
}

/** Where the page's clock is, between two polls. */
export function estimateTick(frame, receivedAt, now, speed) {
  if (!frame) return 0;
  if (frame.paused) return frame.tick;
  const ahead = ((now - receivedAt) / 1000) * 60 * (speed || 1);
  return frame.tick + Math.min(ahead, 30 * (speed || 1));
}
