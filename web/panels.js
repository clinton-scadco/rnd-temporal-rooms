// The panels around the canvas: what the plant is, what one thing in it is
// doing right now, and what the scheduler did to get here.

import { state, apply, num, toNum, ticks } from './doc.js';
import { compact, regionColour } from './render.js';

const $ = s => document.querySelector(s);

export function renderPlant() {
  const p = state.plant;
  const el = $('#plant');
  if (!p) { el.innerHTML = '<div><dt>not compiled</dt><dd>--</dd></div>'; return; }
  const rows = [
    ['objects', num(p.objects), true],
    ['classes', num(p.classes)],
    ['storages', num(p.storages)],
    ['pop cells', num(p.cells), true],
    ['regions', num(p.regions), p.regions > 1],
    ['fused', num(p.fused)],
    ['min slack', p.minSlack === null ? 'unbounded' : num(p.minSlack)],
    ['base period', num(p.basePeriod)],
  ];
  el.innerHTML = rows
    .map(([k, v, hi]) => `<div><dt>${k}</dt><dd class="${hi ? 'hi' : ''}">${v}</dd></div>`)
    .join('');
}

export function renderItems() {
  $('#items').innerHTML = state.graph.items.map(i => `<span>${esc(i)}</span>`).join('') ||
    '<span class="hint">none yet</span>';
}

// --------------------------------------------------------------- inspector

export function renderInspector() {
  const el = $('#detailpane');
  const sel = state.selected;
  if (!sel) { el.innerHTML = '<p class="hint">click anything on the canvas</p>'; return; }
  const node = state.graph.nodes.find(n => n.name === sel.name);
  if (!node) { el.innerHTML = '<p class="hint">gone</p>'; return; }

  const snap = state.snapshot;
  const cls = snap && snap.classes.find(c => c.name === node.name);
  const store = snap && snap.storages.find(s => s.name === node.name);
  const link = snap && snap.links.find(l => l.name === node.name);

  el.innerHTML = `
    <div class="field"><label>name</label><input data-p="name" value="${esc(node.name)}"></div>
    ${node.kind === 'storage' ? storageForm(node) : machineForm(node)}
    <h2>state${snap ? ` <span class="hint">at t=${num(snap.tick)}</span>` : ''}</h2>
    ${cls ? classState(cls, link) : store ? storeState(store) : '<p class="hint">not compiled</p>'}
  `;
  wireForm(el, node);
}

function machineForm(n) {
  const item = sel => `<select data-p="${sel.path}">${state.graph.items
    .map(i => `<option ${i === sel.value ? 'selected' : ''}>${esc(i)}</option>`).join('')}</select>`;
  let html = `
    <div class="field"><label>population</label><input data-p="count" type="number" min="1" value="${n.count}"></div>
    <div class="field"><label>${n.kind === 'link' ? 'out leg' : n.kind === 'process' ? 'takes' : 'every'}</label>
      <input data-p="duration" type="number" min="1" value="${n.duration}"></div>`;
  if (n.kind === 'link') {
    html += `<div class="field"><label>trip home</label><input data-p="returns" type="number" min="0" value="${n.returns}"></div>`;
    html += `<div class="field"><label>batch</label><input data-p="in0qty" type="number" min="1" value="${n.inputs[0] ? n.inputs[0].qty : 0}"></div>`;
    html += `<div class="field"><label>moves</label>${item({ path: 'in0item', value: n.inputs[0] && n.inputs[0].item })}</div>`;
  } else {
    if (n.inputs.length) {
      html += `<div class="field"><label>consumes</label><input data-p="in0qty" type="number" min="1" value="${n.inputs[0].qty}"></div>`;
      html += `<div class="field"><label>of</label>${item({ path: 'in0item', value: n.inputs[0].item })}</div>`;
    }
    if (n.outputs.length) {
      html += `<div class="field"><label>produces</label><input data-p="out0qty" type="number" min="1" value="${n.outputs[0].qty}"></div>`;
      html += `<div class="field"><label>of</label>${item({ path: 'out0item', value: n.outputs[0].item })}</div>`;
    }
  }
  html += `<label class="check"><input type="checkbox" data-p="shared" ${n.shared ? 'checked' : ''}> shared across the deployment</label>`;
  return html;
}

function storageForm(n) {
  return `
    <div class="field"><label>capacity</label><input data-p="capacity" type="number" min="1" value="${n.capacity}"></div>
    <div class="field"><label>policy</label>
      <select data-p="policy">
        ${['index', 'round_robin', 'priority'].map(p =>
          `<option ${p === n.policy ? 'selected' : ''}>${p}</option>`).join('')}
      </select></div>
    <label class="check"><input type="checkbox" data-p="shared" ${n.shared ? 'checked' : ''}> shared across the deployment</label>`;
}

function wireForm(el, node) {
  el.querySelectorAll('[data-p]').forEach(input => {
    input.addEventListener('change', () => {
      const p = input.dataset.p;
      const v = input.type === 'checkbox' ? input.checked
        : input.type === 'number' ? Math.max(0, Number(input.value)) : input.value.trim();
      apply(g => {
        const n = g.nodes.find(x => x.name === node.name);
        if (!n) return;
        if (p === 'name') {
          const old = n.name;
          const name = String(v).replace(/[^A-Za-z0-9_]/g, '') || old;
          n.name = name;
          g.edges.forEach(e => {
            if (e.from === old) e.from = name;
            if (e.to === old) e.to = name;
          });
          state.selected = { name };
        } else if (p === 'in0qty') { if (n.inputs[0]) n.inputs[0].qty = v || 1; if (n.kind === 'link' && n.outputs[0]) n.outputs[0].qty = v || 1; }
        else if (p === 'out0qty') { if (n.outputs[0]) n.outputs[0].qty = v || 1; }
        else if (p === 'in0item') { if (n.inputs[0]) n.inputs[0].item = v; if (n.kind === 'link' && n.outputs[0]) n.outputs[0].item = v; }
        else if (p === 'out0item') { if (n.outputs[0]) n.outputs[0].item = v; }
        else if (p === 'duration') { n.duration = Math.max(1, v); if (n.geometry) n.geometry = null; }
        else if (p === 'returns') { n.returns = v; if (n.geometry) n.geometry = null; }
        else n[p] = v;
      });
    });
  });
}

function bar(parts, total) {
  const seg = parts
    .filter(([n]) => n > 0)
    .map(([n, cls]) => `<i class="${cls}" style="width:${(n / total) * 100}%"></i>`)
    .join('');
  return `<div class="bar">${seg}</div>`;
}

function classState(c, link) {
  const total = toNum(c.count) || 1;
  const home = c.returning.reduce((a, r) => a + toNum(r.n), 0);
  const busy = toNum(c.busy), idle = toNum(c.idle), blocked = toNum(c.blocked);
  const region = c.region === null
    ? '<span class="tag">lifted across a boundary</span>'
    : `<span class="tag region" style="border-color:${regionColour(c.region)};color:${regionColour(c.region)}">region ${c.region}</span>`;

  let html = `
    ${region}
    ${bar([[busy, 'busy'], [home, 'home'], [idle, 'idle'], [blocked, 'blocked']], total)}
    <div class="legend">
      <span><b>${num(busy)}</b> ${link ? 'in transit' : 'working'}</span>
      ${home ? `<span><b>${num(home)}</b> homebound</span>` : ''}
      <span><b>${num(idle)}</b> ${link ? 'waiting to load' : 'idle'}</span>
      <span><b>${num(blocked)}</b> ${link ? 'waiting to unload' : 'blocked'}</span>
    </div>
    <table class="rows">
      <tr><td class="k">population</td><td>${num(c.count)}</td></tr>
      <tr><td class="k">cycles done</td><td>${num(c.cycles)}</td></tr>
      <tr><td class="k">distinct states</td><td>${num(c.states)}</td></tr>
      <tr><td class="k">cycle</td><td>${ticks(c.duration)}</td></tr>
    </table>`;

  if (link) {
    html += `
      <h2>transport</h2>
      <table class="rows">
        <tr><td class="k">route</td><td>${esc(link.from || '?')} → ${esc(link.to || '?')}</td></tr>
        <tr><td class="k">batch</td><td>${num(link.batch)} ${esc(link.item || '')}</td></tr>
        <tr><td class="k">out / home</td><td>${num(link.latency)} / ${num(link.returns)}</td></tr>
        <tr><td class="k">throughput</td><td>${link.rate.toFixed(4)} /tick</td></tr>
        <tr><td class="k">lifted</td><td>${link.channel ? `region ${link.srcRegion} → ${link.dstRegion}` : 'no'}</td></tr>
      </table>
      <h2>in the air</h2>
      <table class="rows">
        ${link.flights.length
          ? link.flights.map(f =>
            `<tr><td class="k">${f.loaded ? 'loaded' : 'empty'} ×${num(f.n)}</td>
                 <td>${num(f.depart)} → ${num(f.arrive)}</td></tr>`).join('')
          : '<tr><td class="k">nothing in transit</td><td></td></tr>'}
      </table>`;
  } else if (c.working.length) {
    html += `<h2>due</h2><table class="rows">${c.working
      .map(w => `<tr><td class="k">×${num(w.n)}</td><td>t=${num(w.at)} (+${num(w.left)})</td></tr>`)
      .join('')}</table>`;
  }
  return html;
}

function storeState(s) {
  const used = toNum(s.used), cap = toNum(s.capacity);
  const region = `<span class="tag region" style="border-color:${regionColour(s.region)};color:${regionColour(s.region)}">region ${s.region}</span>`;
  return `
    ${region}
    ${bar([[used, 'busy'], [cap - used, 'idle']], cap || 1)}
    <div class="legend"><span><b>${num(s.used)}</b> of ${num(s.capacity)}</span>
      <span>${((used / (cap || 1)) * 100).toFixed(1)}% full</span></div>
    <table class="rows">
      ${s.held.map(h => `<tr><td class="k">${esc(h.item)}</td><td>${num(h.qty)}</td></tr>`).join('')}
      <tr><td class="k">policy</td><td>${esc(s.policy)}</td></tr>
      ${s.shared ? '<tr><td class="k">shared</td><td>one for the deployment</td></tr>' : ''}
    </table>`;
}

// --------------------------------------------------------------- timetable
//
// Place down the side, time across: the same picture a railway timetable is,
// and the same one causal decomposition turns out to be.

export function drawTimetable(canvas, tt, now, note) {
  const dpr = window.devicePixelRatio || 1;
  const r = canvas.getBoundingClientRect();
  canvas.width = Math.round(r.width * dpr);
  canvas.height = Math.round(r.height * dpr);
  const ctx = canvas.getContext('2d');
  ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  ctx.clearRect(0, 0, r.width, r.height);
  if (!tt || !tt.advances.length) {
    ctx.fillStyle = '#7D9089';
    ctx.font = '11px Cascadia Mono, Consolas, monospace';
    ctx.fillText(note || 'nothing scheduled yet', 8, 20);
    return;
  }

  const regions = Math.max(...tt.advances.map(a => a.region)) + 1;
  const t1 = Math.max(...tt.advances.map(a => toNum(a.to))) || 1;
  const left = 54, right = r.width - 8;
  const laneH = Math.min(16, (r.height - 16) / regions);
  const X = t => left + (toNum(t) / t1) * (right - left);

  ctx.font = '9px Cascadia Mono, Consolas, monospace';
  for (let i = 0; i < regions; i++) {
    const y = 6 + i * laneH;
    ctx.fillStyle = '#7D9089';
    ctx.fillText('region ' + i, 4, y + laneH * 0.72);
    ctx.fillStyle = 'rgba(255,255,255,.04)';
    ctx.fillRect(left, y, right - left, laneH - 2);
  }
  for (const a of tt.advances) {
    const y = 6 + a.region * laneH;
    const x0 = X(a.from), x1 = X(a.to);
    ctx.fillStyle = regionColour(a.region);
    ctx.globalAlpha = a.blocked ? 0.95 : 0.55;
    ctx.fillRect(x0, y, Math.max(1, x1 - x0 - 1), laneH - 2);
    ctx.globalAlpha = 1;
  }
  // Where the view is sitting now.
  if (now !== undefined && now <= t1) {
    ctx.strokeStyle = '#E0A05C';
    ctx.lineWidth = 1;
    ctx.beginPath();
    ctx.moveTo(X(now), 2);
    ctx.lineTo(X(now), 6 + regions * laneH);
    ctx.stroke();
  }
  ctx.fillStyle = '#7D9089';
  ctx.fillText(
    `${compact(tt.steps)} advances · ${compact(tt.messages)} messages · ${compact(tt.rendezvous)} waits · widest skew ${compact(tt.maxSkew)}`,
    left, r.height - 2,
  );
}

function esc(s) {
  return String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}
