// Experiment 16: the encounter, drawn from compact state.
//
// Nothing here simulates. A frame from the server says where a cohort set off
// and when, where a volley was fired from and where it lands and at which two
// ticks, which heading a battery is turning from and to and between which two
// ticks. This module turns those into pictures at whatever tick the page asks
// for, sixty times a second, between polls -- which is the experiment's claim
// about rendering: explosions are a consequence of four numbers, not a stream.
//
// Pure functions of (layout, frame, tick). No DOM, so tests/combat_web.mjs can
// drive every one of them against a canvas that records instead of painting.

const TURN = 65536;
const TAU = Math.PI * 2;

export const COLORS = {
  ground: '#0c1210',
  grid: 'rgba(125,144,137,0.06)',
  route: '#2a2620',
  routeEdge: '#3a342a',
  sectorClosed: 'rgba(70,197,165,0.07)',
  sectorClosedEdge: 'rgba(70,197,165,0.55)',
  sectorAwake: 'rgba(224,160,92,0.10)',
  sectorAwakeEdge: '#e0a05c',
  domainOpen: '#e06c6c',
  domainShut: 'rgba(125,144,137,0.35)',
  ink: '#d8e3de',
  muted: '#7d9089',
  runner: '#f0b35e',
  brute: '#e0565f',
  hurt: '#ff7b54',
  stone: '#8b9690',
  steel: '#9fb4c0',
  belt: '#56a8e8',
  tracer: '#ffe08a',
  shell: '#f5f0e0',
  flash: '#fff4c2',
  fire: '#ff8a3c',
  smoke: 'rgba(40,40,36,0.55)',
};

// ------------------------------------------------------------------ geometry

/** Fit the field into a canvas of w x h pixels, with a margin. */
export function makeView(layout, w, h, margin = 16) {
  const fw = layout.w * layout.mt;
  const fh = layout.h * layout.mt;
  const scale = Math.min((w - 2 * margin) / fw, (h - 2 * margin) / fh);
  return {
    scale,
    ox: (w - fw * scale) / 2,
    oy: (h - fh * scale) / 2,
    w,
    h,
    tile: layout.mt * scale,
  };
}

export const sx = (v, p) => v.ox + p[0] * v.scale;
export const sy = (v, p) => v.oy + p[1] * v.scale;

function legLen(route, leg) {
  const a = route[leg];
  const b = route[leg + 1];
  return Math.hypot(b[0] - a[0], b[1] - a[1]);
}

function lerp(a, b, f) {
  return [a[0] + (b[0] - a[0]) * f, a[1] + (b[1] - a[1]) * f];
}

/** Which route nodes have something standing on them, from a frame. */
export function blocked(layout, frame) {
  const out = new Set();
  layout.structures.forEach((s, i) => {
    const st = frame.structures && frame.structures[i];
    if (s.node != null && st && st.hp > 0) out.add(s.node);
  });
  return out;
}

/**
 * Where a cohort is at `tick`: the same closed form the server uses, walked
 * on past nodes nothing stands on, so a cohort does not visibly stop at every
 * corner while the page waits for the next poll.
 */
export function cohortPos(layout, c, tick, stops) {
  const route = layout.route;
  if (c.node != null) return route[c.node];
  let leg = c.leg;
  let u = Math.max(0, tick - c.since) * c.speed;
  for (;;) {
    const len = legLen(route, leg);
    if (u <= len) return lerp(route[leg], route[leg + 1], len ? u / len : 1);
    const node = leg + 1;
    if (node + 1 >= route.length || (stops && stops.has(node))) return route[node];
    u -= len;
    leg = node;
  }
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

/**
 * A volley at `tick`: where it is, how far through its flight, and how high a
 * lobbed shell is above the ground under it.
 */
export function volleyAt(v, tick) {
  const f = Math.min(1, Math.max(0, (tick - v.fired) / Math.max(1, v.lands - v.fired)));
  const pos = lerp(v.from, v.to, f);
  const d = Math.hypot(v.to[0] - v.from[0], v.to[1] - v.from[1]);
  const lob = v.gun === 'howitzer' ? 4 * f * (1 - f) * d * 0.28 : 0;
  return { pos, f, lob };
}

/** How many dots stand for a cohort: every one of a few, a handful of many. */
export function dotsFor(count) {
  if (count <= 12) return count;
  return Math.min(64, Math.round(12 + 7 * Math.log10(count / 12 + 1) * 2.2));
}

function hash32(n) {
  n = (n ^ 61) ^ (n >>> 16);
  n = (n + (n << 3)) | 0;
  n ^= n >>> 4;
  n = Math.imul(n, 0x27d4eb2d);
  n ^= n >>> 15;
  return (n >>> 0) / 4294967296;
}

export function fmt(n) {
  const v = typeof n === 'string' ? Number(n) : n;
  if (!Number.isFinite(v)) return String(n);
  return Math.round(v).toLocaleString('en-US');
}

// ------------------------------------------------------------------- layers

function ground(ctx, v, layout) {
  ctx.fillStyle = COLORS.ground;
  ctx.fillRect(0, 0, v.w, v.h);
  ctx.strokeStyle = COLORS.grid;
  ctx.lineWidth = 1;
  ctx.beginPath();
  for (let x = 0; x <= layout.w; x += 4) {
    const px = v.ox + x * v.tile;
    ctx.moveTo(px, v.oy);
    ctx.lineTo(px, v.oy + layout.h * v.tile);
  }
  for (let y = 0; y <= layout.h; y += 4) {
    const py = v.oy + y * v.tile;
    ctx.moveTo(v.ox, py);
    ctx.lineTo(v.ox + layout.w * v.tile, py);
  }
  ctx.stroke();
}

function rectOf(v, r) {
  return [sx(v, [r.x0, 0]), sy(v, [0, r.y0]), (r.x1 - r.x0) * v.scale, (r.y1 - r.y0) * v.scale];
}

function sectors(ctx, v, layout, frame, tick) {
  layout.sectors.forEach((s, i) => {
    const live = (frame.sectors && frame.sectors[i]) || {};
    const awake = live.mode === 'awake';
    const [x, y, w, h] = rectOf(v, s.rect);
    ctx.fillStyle = awake ? COLORS.sectorAwake : COLORS.sectorClosed;
    ctx.fillRect(x, y, w, h);
    // Buildings: a fixed scatter per sector. A closed sector pulses at its
    // orbit's own period, so the picture of "one period of evaluation away"
    // is literally periodic.
    const period = live.period || 240;
    const phase = ((tick % period) / period) * TAU;
    const seed = s.name.length * 7919;
    const cols = Math.max(2, Math.floor(w / (v.tile * 3.2)));
    const rows = Math.max(2, Math.floor(h / (v.tile * 3.2)));
    for (let r = 0; r < rows; r++) {
      for (let c = 0; c < cols; c++) {
        const k = seed + r * 31 + c * 17;
        if (hash32(k) < 0.28) continue;
        const bx = x + (c + 0.2) * (w / cols);
        const by = y + (r + 0.25) * (h / rows);
        const bw = (w / cols) * (0.45 + 0.35 * hash32(k + 1));
        const bh = (h / rows) * (0.4 + 0.3 * hash32(k + 2));
        const glow = awake ? 0.5 + 0.5 * Math.sin(tick * 0.2 + k) : 0.5 + 0.5 * Math.sin(phase + k);
        ctx.fillStyle = awake
          ? `rgba(224,160,92,${0.12 + 0.18 * glow})`
          : `rgba(70,197,165,${0.08 + 0.12 * glow})`;
        ctx.fillRect(bx, by, bw, bh);
      }
    }
    ctx.lineWidth = awake ? 2 : 1;
    ctx.strokeStyle = awake ? COLORS.sectorAwakeEdge : COLORS.sectorClosedEdge;
    if (awake && ctx.setLineDash) {
      ctx.setLineDash([6, 4]);
      ctx.lineDashOffset = -tick * 0.4;
    }
    ctx.strokeRect(x, y, w, h);
    if (ctx.setLineDash) ctx.setLineDash([]);
    ctx.fillStyle = awake ? COLORS.sectorAwakeEdge : COLORS.sectorClosedEdge;
    ctx.font = `600 ${Math.max(10, v.tile * 0.9)}px ui-monospace, Consolas, monospace`;
    ctx.textBaseline = 'top';
    ctx.fillText(s.name.toUpperCase(), x + 5, y + 4);
    ctx.font = `${Math.max(9, v.tile * 0.7)}px ui-monospace, Consolas, monospace`;
    ctx.fillStyle = COLORS.muted;
    const mode = awake
      ? `AWAKE · stepped ${fmt(live.stepped || 0)} rounds`
      : `CLOSED FORM · orbit ${live.period || '?'}t`;
    ctx.fillText(mode, x + 5, y + 4 + v.tile * 1.1);
    if (live.machines != null) {
      ctx.fillText(`${fmt(live.machines)} machines · ${live.states} cells`, x + 5, y + 4 + v.tile * 2);
    }
    if (live.changed && live.changed.length) {
      ctx.fillStyle = COLORS.domainOpen;
      ctx.fillText(live.changed.join(', '), x + 5, y + h - v.tile * 1.2);
    }
  });
}

function domain(ctx, v, layout, frame, tick) {
  const open = frame.domain && frame.domain.open;
  const [x, y, w, h] = rectOf(v, layout.domain);
  ctx.lineWidth = open ? 2 : 1;
  ctx.strokeStyle = open ? COLORS.domainOpen : COLORS.domainShut;
  if (ctx.setLineDash) {
    ctx.setLineDash(open ? [10, 6] : [3, 6]);
    ctx.lineDashOffset = open ? tick * 0.6 : 0;
  }
  ctx.strokeRect(x, y, w, h);
  if (ctx.setLineDash) ctx.setLineDash([]);
  ctx.font = `700 ${Math.max(10, v.tile * 0.8)}px ui-monospace, Consolas, monospace`;
  ctx.textBaseline = 'bottom';
  ctx.fillStyle = open ? COLORS.domainOpen : COLORS.domainShut;
  ctx.fillText(open ? 'COMBAT DOMAIN · EXPLICIT' : 'DISTURBANCE BOUNDARY · DORMANT', x + 6, y - 3);
}

function route(ctx, v, layout) {
  const pts = layout.route;
  for (const [w, c] of [
    [v.tile * 2.6, COLORS.routeEdge],
    [v.tile * 2.0, COLORS.route],
  ]) {
    ctx.strokeStyle = c;
    ctx.lineWidth = w;
    ctx.lineJoin = 'round';
    ctx.lineCap = 'round';
    ctx.beginPath();
    pts.forEach((p, i) => (i ? ctx.lineTo(sx(v, p), sy(v, p)) : ctx.moveTo(sx(v, p), sy(v, p))));
    ctx.stroke();
  }
  // Direction of travel, faintly.
  ctx.fillStyle = 'rgba(224,108,108,0.25)';
  for (let i = 0; i + 1 < pts.length; i++) {
    const m = lerp(pts[i], pts[i + 1], 0.5);
    const a = Math.atan2(pts[i + 1][1] - pts[i][1], pts[i + 1][0] - pts[i][0]);
    const r = v.tile * 0.6;
    ctx.beginPath();
    ctx.moveTo(sx(v, m) + Math.cos(a) * r, sy(v, m) + Math.sin(a) * r);
    ctx.lineTo(sx(v, m) + Math.cos(a + 2.5) * r, sy(v, m) + Math.sin(a + 2.5) * r);
    ctx.lineTo(sx(v, m) + Math.cos(a - 2.5) * r, sy(v, m) + Math.sin(a - 2.5) * r);
    ctx.fill();
  }
}

function belt(ctx, v, layout, frame, tick) {
  const i = layout.structures.findIndex(s => s.kind === 'belt');
  const st = frame.structures && frame.structures[i];
  const down = st && st.band === 'destroyed';
  const [a, b] = layout.belt;
  const x = sx(v, a);
  const y0 = sy(v, a);
  const y1 = sy(v, b);
  const node = layout.route[layout.structures[i].node];
  const gapY = sy(v, node);
  ctx.lineWidth = v.tile * 0.7;
  ctx.strokeStyle = down ? '#2d3b44' : COLORS.belt;
  ctx.globalAlpha = down ? 0.9 : 0.55;
  ctx.beginPath();
  if (down) {
    ctx.moveTo(x, y0);
    ctx.lineTo(x, gapY - v.tile * 2.5);
    ctx.moveTo(x, gapY + v.tile * 2.5);
    ctx.lineTo(x, y1);
  } else {
    ctx.moveTo(x, y0);
    ctx.lineTo(x, y1);
  }
  ctx.stroke();
  ctx.globalAlpha = 1;
  if (!down) {
    // Ore on the belt, moving south: drawn only while the conveyor stands.
    ctx.fillStyle = '#c9d8e0';
    const step = v.tile * 1.4;
    const off = (tick * 0.35) % step;
    for (let y = y0 + off; y < y1; y += step) ctx.fillRect(x - 1.5, y, 3, 3);
  } else {
    ctx.fillStyle = '#56a8e8';
    for (let k = 0; k < 7; k++) {
      ctx.fillRect(x + (hash32(k) - 0.5) * v.tile * 3, gapY + (hash32(k + 9) - 0.5) * v.tile * 4, 3, 2);
    }
  }
}

function structures(ctx, v, layout, frame, tick, assaulted) {
  layout.structures.forEach((s, i) => {
    if (s.kind === 'belt') return;
    const st = (frame.structures && frame.structures[i]) || { hp: s.hp, max: s.hp, band: 'intact' };
    const cx = sx(v, s.at);
    const cy = sy(v, s.at);
    const w = s.w * v.tile;
    const h = s.h * v.tile;
    const frac = st.max ? st.hp / st.max : 1;
    if (st.band === 'destroyed') {
      ctx.fillStyle = '#3b3530';
      for (let k = 0; k < 18; k++) {
        const rx = (hash32(i * 97 + k) - 0.5) * w * 1.4;
        const ry = (hash32(i * 97 + k + 40) - 0.5) * h * 1.2;
        const r = v.tile * (0.2 + 0.35 * hash32(k + i));
        ctx.fillRect(cx + rx - r, cy + ry - r, r * 2, r * 2);
      }
      ctx.fillStyle = COLORS.muted;
      ctx.font = `${Math.max(8, v.tile * 0.6)}px ui-monospace, Consolas, monospace`;
      ctx.textBaseline = 'top';
      ctx.fillText('rubble', cx - w / 2, cy + h / 2 + 2);
      return;
    }
    const tint =
      st.band === 'intact' ? COLORS.stone : st.band === 'damaged' ? '#a8866a' : '#b4584a';
    if (s.kind === 'pit') {
      ctx.strokeStyle = tint;
      ctx.lineWidth = Math.max(2, v.tile * 0.5);
      ctx.beginPath();
      ctx.arc(cx, cy, w / 2, 0, TAU);
      ctx.stroke();
      ctx.fillStyle = 'rgba(20,24,22,0.9)';
      ctx.beginPath();
      ctx.arc(cx, cy, w / 2 - ctx.lineWidth / 2, 0, TAU);
      ctx.fill();
    } else {
      ctx.fillStyle = tint;
      ctx.fillRect(cx - w / 2, cy - h / 2, w, h);
      if (s.kind === 'hall' || s.kind === 'shed') {
        ctx.strokeStyle = 'rgba(0,0,0,0.35)';
        ctx.lineWidth = 1;
        ctx.beginPath();
        for (let k = 1; k < 4; k++) {
          ctx.moveTo(cx - w / 2, cy - h / 2 + (h * k) / 4);
          ctx.lineTo(cx + w / 2, cy - h / 2 + (h * k) / 4);
        }
        ctx.stroke();
      }
    }
    // Cracks and fire, by band.
    if (st.band !== 'intact') {
      ctx.strokeStyle = 'rgba(0,0,0,0.6)';
      ctx.lineWidth = 1;
      ctx.beginPath();
      const n = st.band === 'damaged' ? 3 : 6;
      for (let k = 0; k < n; k++) {
        const px = cx + (hash32(i * 13 + k) - 0.5) * w;
        const py = cy + (hash32(i * 13 + k + 5) - 0.5) * h;
        ctx.moveTo(px, py);
        ctx.lineTo(px + (hash32(k + 2) - 0.5) * v.tile * 1.6, py + (hash32(k + 3) - 0.5) * v.tile * 1.6);
      }
      ctx.stroke();
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
    // Under assault: sparks on the face the route arrives at.
    if (s.node != null && assaulted.has(s.node)) {
      ctx.fillStyle = COLORS.flash;
      for (let k = 0; k < 6; k++) {
        const r = hash32(Math.floor(tick / 3) * 11 + k + i * 5);
        ctx.fillRect(cx + w / 2 + r * v.tile * 1.2, cy + (hash32(r * 1e6 + k) - 0.5) * h, 2, 2);
      }
    }
    // An hp bar, once there is something to show.
    if (frac < 0.999) {
      const bw = Math.max(w, v.tile * 3);
      ctx.fillStyle = 'rgba(0,0,0,0.6)';
      ctx.fillRect(cx - bw / 2, cy - h / 2 - 6, bw, 3);
      ctx.fillStyle = frac > 0.75 ? '#7ed9a0' : frac > 0.4 ? '#e0a05c' : '#e06c6c';
      ctx.fillRect(cx - bw / 2, cy - h / 2 - 6, bw * frac, 3);
    }
  });
}

function batteries(ctx, v, layout, frame, tick) {
  let flashes = 0;
  layout.batteries.forEach((b, i) => {
    const live = (frame.batteries && frame.batteries[i]) || { heading: 0, duty: 'idle', alive: true };
    const cx = sx(v, b.at);
    const cy = sy(v, b.at);
    if (live.alive) {
      ctx.strokeStyle = live.duty === 'idle' ? 'rgba(125,144,137,0.10)' : 'rgba(70,197,165,0.18)';
      ctx.lineWidth = 1;
      if (ctx.setLineDash) ctx.setLineDash([2, 5]);
      ctx.beginPath();
      ctx.arc(cx, cy, b.range * v.scale, 0, TAU);
      ctx.stroke();
      if (ctx.setLineDash) ctx.setLineDash([]);
    }
    const a = heading(live, tick);
    const n = b.count;
    const ring = n > 1 ? v.tile * 0.9 : 0;
    const howitzer = b.gun === 'howitzer';
    const since = live.lastFire != null ? tick - live.lastFire : Infinity;
    for (let k = 0; k < n; k++) {
      const ka = (k / n) * TAU + 0.6;
      const tx = cx + Math.cos(ka) * ring;
      const ty = cy + Math.sin(ka) * ring;
      const len = v.tile * (howitzer ? 1.6 : 1.2);
      // Recoil: the barrel kicks back for a few ticks after a volley.
      const kick = since < 8 ? (1 - since / 8) * v.tile * 0.35 : 0;
      const bx = tx + Math.cos(a) * (len - kick);
      const by = ty + Math.sin(a) * (len - kick);
      ctx.fillStyle = live.alive ? (howitzer ? '#6f7f78' : COLORS.steel) : '#3a3a3a';
      ctx.beginPath();
      ctx.arc(tx, ty, v.tile * (howitzer ? 0.55 : 0.42), 0, TAU);
      ctx.fill();
      ctx.strokeStyle = live.alive ? '#dfe8e4' : '#444';
      ctx.lineWidth = Math.max(1.5, v.tile * (howitzer ? 0.32 : 0.2));
      ctx.beginPath();
      ctx.moveTo(tx, ty);
      ctx.lineTo(bx, by);
      ctx.stroke();
      if (live.alive && since < 6) {
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
    ctx.font = `${Math.max(8, v.tile * 0.6)}px ui-monospace, Consolas, monospace`;
    ctx.textBaseline = 'top';
    ctx.fillStyle = live.alive ? (live.hold ? COLORS.muted : COLORS.ink) : '#555';
    ctx.fillText(`${b.name} ×${n}${live.hold ? ' (hold)' : ''}`, cx - v.tile * 2, cy + v.tile * 2.1);
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
      // The shadow on the ground, and the shell above it.
      ctx.fillStyle = 'rgba(0,0,0,0.45)';
      ctx.beginPath();
      ctx.ellipse
        ? ctx.ellipse(x, y, 3, 1.5, 0, 0, TAU)
        : ctx.arc(x, y, 2, 0, TAU);
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

function effects(ctx, v, frame, tick) {
  let drawn = 0;
  for (const fx of frame.fx || []) {
    const age = tick - fx.at;
    if (age < 0) continue;
    const x = sx(v, fx.pos);
    const y = sy(v, fx.pos);
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
      // Smoke drifts and thins.
      ctx.fillStyle = `rgba(60,58,52,${0.4 * (1 - age / 150)})`;
      for (let k = 0; k < 3; k++) {
        ctx.beginPath();
        ctx.arc(x + (hash32(fx.at + k) - 0.5) * r + age * 0.08, y - age * 0.12 - k * 3, r * 0.3 + age * 0.05, 0, TAU);
        ctx.fill();
      }
      if (fx.n && Number(fx.n) > 0 && age < 50) {
        ctx.fillStyle = `rgba(224,108,108,${1 - age / 50})`;
        ctx.font = `600 ${Math.max(9, v.tile * 0.7)}px ui-monospace, Consolas, monospace`;
        ctx.textBaseline = 'bottom';
        ctx.fillText(`−${fmt(fx.n)}`, x + r * 0.5, y - r * 0.5 - age * 0.3);
      }
    } else if (fx.what === 'fall') {
      if (age > 180) continue;
      drawn++;
      ctx.fillStyle = `rgba(120,110,95,${0.5 * (1 - age / 180)})`;
      for (let k = 0; k < 6; k++) {
        ctx.beginPath();
        ctx.arc(x + (hash32(k + fx.at) - 0.5) * v.tile * 5, y + (hash32(k * 3 + fx.at) - 0.5) * v.tile * 5 - age * 0.05, v.tile * (1 + age / 60), 0, TAU);
        ctx.fill();
      }
    } else if (fx.what === 'leak') {
      if (age > 90) continue;
      drawn++;
      ctx.fillStyle = `rgba(224,108,108,${1 - age / 90})`;
      ctx.font = `700 ${Math.max(9, v.tile * 0.8)}px ui-monospace, Consolas, monospace`;
      ctx.textBaseline = 'middle';
      ctx.fillText(`${fmt(fx.n)} through ⟵`, x - v.tile * 7 - age * 0.2, y);
    }
  }
  return drawn;
}

function cohorts(ctx, v, layout, frame, tick, stops) {
  let dots = 0;
  const list = frame.cohorts || [];
  for (const c of list) {
    const p = cohortPos(layout, c, tick, stops);
    const cx = sx(v, p);
    const cy = sy(v, p);
    const count = Number(c.count);
    const n = dotsFor(count);
    const radius = Math.max(v.tile * 0.5, c.spread * v.scale);
    const hurt = c.hp < c.hpMax;
    const brute = c.kind === 'brute';
    const base = hurt ? COLORS.hurt : brute ? COLORS.brute : COLORS.runner;
    const assaulting = c.node != null;
    // A faint ground under the whole cohort: its extent, which is also what a
    // shell has to land inside to hit it.
    ctx.fillStyle = hurt ? 'rgba(255,123,84,0.08)' : brute ? 'rgba(224,86,95,0.08)' : 'rgba(240,179,94,0.07)';
    ctx.beginPath();
    ctx.arc(cx, cy, radius, 0, TAU);
    ctx.fill();
    ctx.fillStyle = base;
    const size = Math.max(1.5, v.tile * (brute ? 0.34 : 0.24));
    for (let k = 0; k < n; k++) {
      // A sunflower: even at any count, deterministic per cohort.
      const r = radius * Math.sqrt((k + 0.5) / n) * 0.92;
      const a = k * 2.39996 + c.id * 0.7;
      const bob = assaulting
        ? Math.sin(tick * 0.9 + k) * v.tile * 0.25
        : Math.sin(tick * 0.35 + k * 1.3) * v.tile * 0.12;
      const px = cx + Math.cos(a) * r + (assaulting ? bob : 0);
      const py = cy + Math.sin(a) * r + (assaulting ? 0 : bob);
      if (brute) ctx.fillRect(px - size, py - size, size * 2, size * 2);
      else {
        ctx.beginPath();
        ctx.arc(px, py, size, 0, TAU);
        ctx.fill();
      }
    }
    dots += n;
    // The count, and a pip bar of health.
    ctx.font = `600 ${Math.max(9, v.tile * 0.75)}px ui-monospace, Consolas, monospace`;
    ctx.textBaseline = 'bottom';
    ctx.fillStyle = COLORS.ink;
    const label = `×${fmt(count)}`;
    ctx.fillText(label, cx + radius * 0.7, cy - radius * 0.7);
    const pw = v.tile * 1.8;
    ctx.fillStyle = 'rgba(0,0,0,0.6)';
    ctx.fillRect(cx + radius * 0.7, cy - radius * 0.7 + 1, pw, 2.5);
    ctx.fillStyle = hurt ? COLORS.hurt : '#7ed9a0';
    ctx.fillRect(cx + radius * 0.7, cy - radius * 0.7 + 1, (pw * c.hp) / c.hpMax, 2.5);
  }
  return dots;
}

// -------------------------------------------------------------------- scene

/**
 * Draw the whole encounter at `tick`. Returns what it drew, counted, so a test
 * can say "a volley in flight was drawn" without looking at a pixel.
 */
export function drawScene(ctx, layout, frame, tick, v) {
  const stops = blocked(layout, frame);
  const assaulted = new Set((frame.cohorts || []).filter(c => c.node != null).map(c => c.node));
  ground(ctx, v, layout);
  sectors(ctx, v, layout, frame, tick);
  domain(ctx, v, layout, frame, tick);
  route(ctx, v, layout);
  belt(ctx, v, layout, frame, tick);
  structures(ctx, v, layout, frame, tick, assaulted);
  const dots = cohorts(ctx, v, layout, frame, tick, stops);
  const flashes = batteries(ctx, v, layout, frame, tick);
  const shells = volleys(ctx, v, frame, tick);
  const fx = effects(ctx, v, frame, tick);
  return { cohorts: (frame.cohorts || []).length, dots, flashes, volleys: shells, fx };
}

/** Where the page's clock is, between two polls. */
export function estimateTick(frame, receivedAt, now, speed) {
  if (!frame) return 0;
  if (frame.paused) return frame.tick;
  const ahead = ((now - receivedAt) / 1000) * 60 * (speed || 1);
  return frame.tick + Math.min(ahead, 30 * (speed || 1));
}
