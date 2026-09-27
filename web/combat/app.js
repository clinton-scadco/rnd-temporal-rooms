// Experiment 16's page: poll a frame, draw it sixty times a second.
//
// The server is asked for state a few times a second; everything in between
// is `draw.js` interpolating that state to the page's own estimate of the
// tick. Commands go straight to the server and come back as state on the next
// poll -- the page never guesses what a wave will do.

import { drawScene, estimateTick, fmt, makeView } from './draw.js';

const $ = id => document.getElementById(id);
const POLL_MS = 200;

let layout = null;
let frame = null;
let receivedAt = 0;
let polling = false;

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
  lines.push(`<span class="h">COMBAT DOMAIN</span>${lpad(o.open ? 'OPEN' : 'DORMANT', 23)}`);
  lines.push('');
  lines.push(`Nominal entities: <span class="n">${lpad(fmt(o.nominal), 12)}</span>`);
  lines.push(`<span class="muted">  ${fmt(o.attackers)} attackers · ${o.turrets} turrets · ${fmt(o.shells)} shells</span>`);
  lines.push(`<span class="muted">  ${o.structures} structures · ${fmt(o.machines)} machines</span>`);
  lines.push('');
  lines.push('Compressed state:');
  lines.push(`${pad('Enemy cohorts', 20)}<span class="c">${lpad(o.cohorts, 9)}</span>`);
  lines.push(`${pad('Turret populations', 20)}<span class="c">${lpad(o.batteries, 9)}</span>`);
  lines.push(`${pad('Projectiles', 20)}<span class="c">${lpad(o.volleys, 9)}</span>`);
  lines.push(`${pad('Unique structures', 20)}<span class="c">${lpad(o.structures, 9)}</span>`);
  lines.push(`${pad('Awake sector cells', 20)}<span class="c">${lpad(o.sectorCells, 9)}</span>`);
  lines.push('');
  lines.push(`${pad('Events processed/s', 20)}${lpad(fmt(o.eventsPerSec || 0), 9)}`);
  lines.push(`<span class="muted">${pad('events so far', 20)}${lpad(fmt(o.events), 9)}</span>`);
  lines.push(`<span class="muted">${pad('cohort records made', 20)}${lpad(fmt(o.created), 9)}</span>`);
  lines.push(`<span class="muted">${pad('splits / merges', 20)}${lpad(`${fmt(o.splits)} / ${fmt(o.merges)}`, 9)}</span>`);
  lines.push(`<span class="muted">${pad('killed / through', 20)}${lpad(`${fmt(o.killed)} / ${fmt(o.leaked)}`, 9)}</span>`);
  return lines.join('\n');
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
    .map(s => {
      const mode = s.mode === 'awake' ? 'awake' : 'closed';
      const detail = s.mode === 'awake'
        ? `stepped ${fmt(s.stepped)} rounds on the fight's clock`
        : `orbit ${s.period}t${s.frozen ? ' (frozen)' : ''}, ${fmt(s.ratePerMin || 0)} ${s.product}/min`;
      const changed = s.changed.length ? `<br><span class="bad">${s.changed.join(', ')}</span>` : '';
      return `<div class="item">
        <span class="name">${s.name} <span class="muted small">${s.inside ? 'inside' : 'outside'}</span></span>
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
      const frac = s.max ? s.hp / s.max : 1;
      const tie = layout.structures[i].tie;
      const hurt = s.hp < s.max;
      return `<div class="item">
        <span class="name">${s.name}</span>
        ${hurt ? `<button data-repair="${s.name}">repair</button>` : '<span></span>'}
        <div class="bar"><i class="${s.band}" style="width:${(frac * 100).toFixed(1)}%"></i></div>
        <span class="meta">${s.band}${tie ? ` · feeds ${tie}` : ''}</span>
      </div>`;
    })
    .join('');
}

function batsPanel(f) {
  return f.batteries
    .map((b, i) => {
      const def = layout.batteries[i];
      return `<div class="item">
        <span class="name">${b.name} <span class="muted small">${def.gun} ×${def.count}</span></span>
        <button data-hold="${b.name}" data-on="${b.hold ? 0 : 1}" ${b.alive ? '' : 'disabled'}>${b.hold ? 'release' : 'hold'}</button>
        <span class="meta">${b.alive ? b.duty : '<span class="bad">pit destroyed</span>'} · ${fmt(b.volleys)} volleys</span>
      </div>`;
    })
    .join('');
}

function notesPanel(f) {
  return f.notes.map(n => `<li><b>${fmt(Math.floor(n.at / 3600))}:${String(Math.floor(n.at / 60) % 60).padStart(2, '0')}</b>${n.text}</li>`).join('');
}

function panels(f) {
  $('overlay').innerHTML = overlayText(f);
  $('log').innerHTML = logPanel(f);
  $('sectors').innerHTML = sectorsPanel(f);
  $('structs').innerHTML = structsPanel(f);
  $('bats').innerHTML = batsPanel(f);
  $('notes').innerHTML = notesPanel(f);
  $('clock').textContent = f.clock;
  const chip = $('domainchip');
  chip.textContent = f.domain.open ? 'combat domain open' : 'dormant';
  chip.className = `chip ${f.domain.open ? 'open' : 'closed'}`;
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
    drawScene(ctx, layout, frame, tick, makeView(layout, w, h, 18 * dpr));
    $('tick').textContent = `t=${fmt(Math.floor(tick))}`;
  }
  requestAnimationFrame(paint);
}

// ------------------------------------------------------------------ controls

function bind() {
  document.addEventListener('click', async e => {
    const t = e.target.closest('button');
    if (!t) return;
    if (t.dataset.wave) await api('/api/wave', { n: Number(t.dataset.wave) });
    else if (t.dataset.repair) await api('/api/repair', { s: t.dataset.repair });
    else if (t.dataset.hold) await api('/api/hold', { b: t.dataset.hold, on: t.dataset.on === '1' });
    else if (t.dataset.speed) await api('/api/speed', { x: Number(t.dataset.speed), paused: false });
    else if (t.dataset.pause) await api('/api/speed', { paused: !(frame && frame.paused) });
    else if (t.id === 'send') {
      const n = Number(String($('custom').value).replace(/[,\s_]/g, ''));
      if (n > 0) await api('/api/wave', { n });
    } else if (t.id === 'reset') await api('/api/reset', {});
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
  $('custom').addEventListener('keydown', e => {
    if (e.key === 'Enter') $('send').click();
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
