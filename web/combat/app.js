// Experiment 16's page: poll a frame, draw it sixty times a second.
//
// The server is asked for state a few times a second; everything in between
// is `draw.js` interpolating that state to the page's own estimate of the
// tick. Commands go straight to the server and come back as state on the next
// poll -- the page never guesses what a crossing will do.

import { drawScene, estimateTick, fmt, makeView, strainAt, ERA } from './draw.js';

const $ = id => document.getElementById(id);
const POLL_MS = 200;
const RATES = [0, 10, 40, 150, 600];

let layout = null;
let frame = null;
let receivedAt = 0;
let polling = false;
// The three centuries' ground, painted once per canvas size.
const paintOpts = {
  canvas: (w, h) => {
    const c = document.createElement('canvas');
    c.width = w;
    c.height = h;
    return c;
  },
};

async function api(path, body) {
  const r = await fetch(path, body === undefined ? {} : {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body),
  });
  return r.json();
}

// -------------------------------------------------------------------- panels

const pad = (s, n) => String(s).padEnd(n);
const lpad = (s, n) => String(s).padStart(n);

export function overlayText(f) {
  const o = f.overlay;
  const lines = [];
  lines.push(`<span class="h">DISTURBANCE DOMAIN</span>${lpad(o.open ? 'OPEN' : 'DORMANT', 18)}`);
  lines.push('');
  lines.push(`Nominal entities: <span class="n">${lpad(fmt(o.nominal), 12)}</span>`);
  lines.push(`<span class="muted">  ${fmt(o.manifest)} manifest · ${o.turrets} turrets · ${fmt(o.shells)} shells</span>`);
  lines.push(`<span class="muted">  ${o.structures} structures · ${fmt(o.machines)} machines awake</span>`);
  lines.push(`<span class="muted">  ${fmt(o.tonnes)} t crossed, each one a strain</span>`);
  lines.push('');
  lines.push('Compressed state:');
  lines.push(`${pad('Manifest cohorts', 20)}<span class="c">${lpad(o.cohorts, 9)}</span>`);
  lines.push(`${pad('Open ruptures', 20)}<span class="c">${lpad(o.ruptures, 9)}</span>`);
  lines.push(`${pad('Strain records', 20)}<span class="c">${lpad(o.strainRecords, 9)}</span>`);
  lines.push(`${pad('Turret populations', 20)}<span class="c">${lpad(o.batteries, 9)}</span>`);
  lines.push(`${pad('Projectiles', 20)}<span class="c">${lpad(o.volleys, 9)}</span>`);
  lines.push(`${pad('Awake sector cells', 20)}<span class="c">${lpad(o.sectorCells, 9)}</span>`);
  lines.push('');
  lines.push(`${pad('Events processed/s', 20)}${lpad(fmt(o.eventsPerSec || 0), 9)}`);
  lines.push(`<span class="muted">${pad('events so far', 20)}${lpad(fmt(o.events), 9)}</span>`);
  lines.push(`<span class="muted">${pad('tears', 20)}${lpad(fmt(o.tears), 9)}</span>`);
  lines.push(`<span class="muted">${pad('splits / merges', 20)}${lpad(`${fmt(o.splits)} / ${fmt(o.merges)}`, 9)}</span>`);
  lines.push(`<span class="muted">${pad('destroyed / faded', 20)}${lpad(`${fmt(o.killed)} / ${fmt(o.faded)}`, 9)}</span>`);
  return lines.join('\n');
}

function lanesPanel(f) {
  return layout.lanes
    .map((l, i) => {
      const live = f.lanes[i];
      const rate = Number(live.rate);
      const flow = Number(live.flow) / 1000;
      const era = ERA[l.origin];
      const buttons = RATES.map(r => `<button data-lane="${l.tag}" data-rate="${r}" class="${r === rate ? 'on' : ''}">${r ? r : 'close'}</button>`).join('');
      const state = rate === 0
        ? '<span class="muted">closed</span>'
        : flow === 0
          ? '<span class="bad">dark: nothing crossing</span>'
          : `<b style="color:${era.ink}">${fmt(flow)} t/s</b> crossing${flow < rate ? ` <span class="warn">(asked ${fmt(rate)})</span>` : ''}`;
      return `<div class="lane">
        <div class="lanehead"><span class="swatch" style="background:${era.ink}"></span>
          <b>${l.name}</b> <span class="muted small">${l.item} · ${l.years} years · ${l.mw} MW · into the ${l.dest.toLowerCase()}</span></div>
        <div class="row wrap group">${buttons}</div>
        <div class="small">${state} · ${fmt(Number(live.crossed) / 1000)} t so far</div>
      </div>`;
    })
    .join('');
}

function gridPanel(f) {
  const g = f.grid;
  const pct = Math.min(100, (g.demand / g.supply) * 100);
  const over = g.demand > g.supply;
  return `<div class="gridbar"><i class="${over ? 'over' : ''}" style="width:${pct.toFixed(1)}%"></i></div>
    grid ${g.demand} / ${g.supply} MW${over ? ` · <span class="warn">brownout: the lanes get ${(g.factor / 10).toFixed(0)}%</span>` : ''}`;
}

function responsesPanel(f) {
  const rows = layout.anchors.map((a, i) => {
    const live = f.anchors[i];
    const covers = layout.sites.filter(s => Math.hypot(s.at[0] - a.at[0], s.at[1] - a.at[1]) <= a.range).map(s => s.name);
    return `<div class="item">
      <span class="name">${a.name} <span class="muted small">${a.mw} MW · covers ${covers.join(', ')}</span></span>
      <button data-anchor="${a.name}" data-on="${live.on ? 0 : 1}" class="${live.on ? 'on' : ''}">${live.on ? 'powered' : 'power'}</button>
      ${live.on && !live.live ? '<span class="meta bad">the anchor is down</span>' : ''}
    </div>`;
  });
  rows.push(`<div class="item">
    <span class="name">Launder the ore <span class="muted small">${layout.grid.launderMw} MW · needs the crusher house</span></span>
    <button data-launder="${f.launder ? 0 : 1}" class="${f.launder ? 'on' : ''}">${f.launder ? 'laundering' : 'launder'}</button>
    ${f.launder && !f.laundering ? '<span class="meta bad">the crusher house is down</span>' : ''}
  </div>`);
  return rows.join('');
}

function sitesPanel(f, tick) {
  return layout.sites
    .map((d, i) => {
      const s = f.sites[i];
      const [a, b] = strainAt(s, tick);
      const fa = Math.min(100, (a / d.open) * 100);
      const fb = Math.min(100 - fa, (b / d.open) * 100);
      const status = s.torn != null
        ? `<span class="bad">torn into ${s.source}</span>${s.pinned ? ' · <span class="good">pinned</span>' : s.bleed ? ` · bleeding ${Math.round(s.bleed / layout.mt)} tiles` : ''} · ${s.emits} vents`
        : s.pinned
          ? '<span class="good">pinned</span>'
          : '<span class="muted">holding</span>';
      const rate = (Number(s.rate[0]) + Number(s.rate[1])) * 60 / 1000;
      return `<div class="item">
        <span class="name">${d.name} <span class="muted small">${d.holds ? `holds ${d.holds}` : 'a door: holds nothing'}</span></span>
        <span class="meta mono">${fmt((a + b) / 1000)} / ${fmt(d.open / 1000)} ty</span>
        <div class="bar strain"><i class="e1890" style="width:${fa.toFixed(1)}%"></i><i class="e2070" style="width:${fb.toFixed(1)}%"></i></div>
        <span class="meta">${status} · ${rate >= 0 ? '+' : ''}${fmt(rate)} ty/s</span>
      </div>`;
    })
    .join('');
}

function logPanel(f) {
  const l = f.log;
  const kb = l.checkpointBytes ? `${(l.checkpointBytes / 1024).toFixed(1)} KB` : '-';
  return [
    `<b>${l.commands}</b> commands, <b>${fmt(l.bytes)}</b> bytes of log`,
    `<b>${fmt(l.derived)}</b> derived events, <b>${l.serializedEvents}</b> of them serialized`,
    `<b>${l.checkpoints}</b> checkpoints, the latest ${kb}${l.lastCheckpoint != null ? ` at t=${fmt(l.lastCheckpoint)}` : ''}`,
  ].join('\n');
}

function sectorsPanel(f) {
  return f.sectors
    .map((s, i) => {
      const mode = s.mode === 'awake' ? 'awake' : 'closed';
      const detail = s.mode === 'awake'
        ? `stepped ${fmt(s.stepped)} rounds on the disturbance's clock`
        : `orbit ${s.period}t${s.frozen ? ' (frozen)' : ''}, ${fmt(s.ratePerMin || 0)} ${s.product}/min`;
      const changed = s.changed.length ? `<br><span class="bad">${s.changed.join(', ')}</span>` : '';
      const by = layout.sectors[i].reachedBy;
      return `<div class="item">
        <span class="name">${s.name} <span class="muted small">${by.length ? `reached by ${by.join(', ').toLowerCase()}` : 'out of every reach'}</span></span>
        <span class="chip ${mode}">${mode === 'awake' ? 'awake' : 'closed form'}</span>
        <span class="meta">${fmt(s.machines)} machines in ${s.states} cells · ${detail}<br>
        woke ${s.woke} · recompiled ${s.recompiles} · ${fmt(s.evals)} evaluations${changed}</span>
      </div>`;
    })
    .join('');
}

function structsPanel(f) {
  return f.structures
    .map((s, i) => {
      const def = layout.structures[i];
      if (def.kind === 'works') return '';
      const frac = s.max ? s.hp / s.max : 1;
      const hurt = s.hp < s.max;
      return `<div class="item">
        <span class="name">${s.name}</span>
        ${hurt ? `<button data-repair="${s.name}">repair</button>` : '<span></span>'}
        <div class="bar"><i class="${s.band}" style="width:${(frac * 100).toFixed(1)}%"></i></div>
        <span class="meta">${s.band}${def.tie ? ` · feeds ${def.tie}` : ''}</span>
      </div>`;
    })
    .join('');
}

function batsPanel(f) {
  return f.batteries
    .map((b, i) => {
      const def = layout.batteries[i];
      const state = !b.alive ? '<span class="bad">pit destroyed</span>' : !b.powered ? '<span class="warn">no grid: standing in 1890</span>' : b.duty;
      return `<div class="item">
        <span class="name">${b.name} <span class="muted small">${def.gun} ×${def.count}</span></span>
        <button data-hold="${b.name}" data-on="${b.hold ? 0 : 1}" ${b.alive ? '' : 'disabled'}>${b.hold ? 'release' : 'hold'}</button>
        <span class="meta">${state} · ${fmt(b.volleys)} volleys</span>
      </div>`;
    })
    .join('');
}

function notesPanel(f) {
  return f.notes
    .map(n => `<li><b>${fmt(Math.floor(n.at / 3600))}:${String(Math.floor(n.at / 60) % 60).padStart(2, '0')}</b>${n.text}</li>`)
    .join('');
}

function panels(f) {
  $('overlay').innerHTML = overlayText(f);
  $('lanes').innerHTML = lanesPanel(f);
  $('grid').innerHTML = gridPanel(f);
  $('responses').innerHTML = responsesPanel(f);
  $('sites').innerHTML = sitesPanel(f, f.tick);
  $('log').innerHTML = logPanel(f);
  $('sectors').innerHTML = sectorsPanel(f);
  $('structs').innerHTML = structsPanel(f);
  $('bats').innerHTML = batsPanel(f);
  $('notes').innerHTML = notesPanel(f);
  $('clock').textContent = f.clock;
  const chip = $('domainchip');
  const torn = f.sites.filter(s => s.torn != null).length;
  chip.textContent = f.domain.open ? `disturbance · ${torn} torn` : 'dormant';
  chip.className = `chip ${f.domain.open ? 'open' : 'closed'}`;
  $('darkchip').hidden = !f.dark;
  for (const b of document.querySelectorAll('#speed [data-speed]')) {
    b.classList.toggle('on', Number(b.dataset.speed) === f.speed && !f.paused);
  }
  const pause = document.querySelector('#speed [data-pause]');
  if (pause) pause.classList.toggle('on', !!f.paused);
}

// --------------------------------------------------------------------- loop

async function poll() {
  if (polling) return;
  polling = true;
  try {
    const f = await api('/api/frame');
    if (f.ok) {
      frame = f;
      receivedAt = performance.now();
      panels(f);
    }
  } catch (e) {
    // The server went away; the next poll will say whether it came back.
  } finally {
    polling = false;
  }
}

function paint() {
  const canvas = $('view');
  const dpr = window.devicePixelRatio || 1;
  const rect = canvas.getBoundingClientRect();
  const w = Math.max(1, Math.floor(rect.width * dpr));
  const h = Math.max(1, Math.floor(rect.height * dpr));
  if (canvas.width !== w || canvas.height !== h) {
    canvas.width = w;
    canvas.height = h;
  }
  const ctx = canvas.getContext('2d');
  if (layout && frame) {
    const tick = estimateTick(frame, receivedAt, performance.now(), frame.speed);
    ctx.setTransform(1, 0, 0, 1, 0, 0);
    drawScene(ctx, layout, frame, tick, makeView(layout, w, h, 10 * dpr), paintOpts);
    $('tick').textContent = `t=${fmt(Math.floor(tick))}`;
  }
  requestAnimationFrame(paint);
}

// ------------------------------------------------------------------ controls

function bind() {
  document.addEventListener('click', async e => {
    const t = e.target.closest('button');
    if (!t) return;
    const d = t.dataset;
    if (d.lane) await api('/api/stream', { lane: d.lane, rate: Number(d.rate) });
    else if (d.anchor) await api('/api/anchor', { a: d.anchor, on: d.on === '1' });
    else if (d.launder) await api('/api/launder', { on: d.launder === '1' });
    else if (d.repair) await api('/api/repair', { s: d.repair });
    else if (d.hold) await api('/api/hold', { b: d.hold, on: d.on === '1' });
    else if (d.speed) await api('/api/speed', { x: Number(d.speed), paused: false });
    else if (d.pause) await api('/api/speed', { paused: !(frame && frame.paused) });
    else if (t.id === 'reset') await api('/api/reset', {});
    else if (t.id === 'verify') {
      const v = $('verdict');
      v.textContent = 'replaying...';
      const r = await api('/api/verify', {});
      v.className = `mono small ${r.match ? 'good' : 'bad'}`;
      v.textContent = [
        r.match ? 'all three agree' : 'THEY DISAGREE',
        `live     ${r.live}`,
        `replay   ${r.replay}   (${r.commands} commands, ${r.derived} events rederived)`,
        `resumed  ${r.resumed ?? '-'}${r.from != null ? `   (from t=${fmt(r.from)})` : ''}`,
        `${r.ms.toFixed(1)} ms`,
      ].join('\n');
    } else return;
    poll();
  });
}

async function main() {
  layout = await api('/api/layout');
  bind();
  await poll();
  setInterval(poll, POLL_MS);
  requestAnimationFrame(paint);
}

main();
