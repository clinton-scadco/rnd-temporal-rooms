// The parts of the screen that only exist because somebody wants something
// out of the factory: the brief, the money, the orders, and the receipt for
// what an edit destroyed.
//
// None of this is simulation. Every number here arrives from the server
// already decided -- `scenario.rs` reads the counters and says whether the
// order is met -- because two clients that disagree about who won have a worse
// problem than two clients that disagree about a pixel.

import { state, num, toNum, verify, describeOp } from './doc.js';

const $ = s => document.querySelector(s);

export function renderMission() {
  const el = $('#mission');
  const p = state.play;
  if (!p || !p.scenario) { el.hidden = true; return; }
  el.hidden = false;

  const sc = p.scenario;
  const spent = toNum(p.spent), budget = toNum(sc.budget);
  const over = p.overspent !== null && p.overspent !== undefined;
  const verdict = p.won ? '<b class="won">delivered</b>'
    : p.lost ? '<b class="lost">failed</b>'
    : '<b class="running">in progress</b>';

  const orders = p.orders.map(o => {
    const frac = Math.max(0, Math.min(1, o.progress));
    const cls = o.met ? 'won' : o.failed ? 'lost' : '';
    return `
      <div class="order ${cls}">
        <div class="order-text">${esc(o.text)}</div>
        <div class="bar"><i class="${o.failed ? 'blocked' : 'busy'}" style="width:${frac * 100}%"></i></div>
        <div class="legend"><span><b>${num(o.have)}</b> of ${num(o.need)}</span>
          <span>${(frac * 100).toFixed(1)}%</span></div>
      </div>`;
  }).join('');

  const bought = (p.purchases || []).slice(-6).reverse().map(b =>
    `<tr><td class="k">t=${num(b.at)}</td><td>${esc(b.what)}</td>
       <td>${num(b.cost)}</td></tr>`).join('');

  el.innerHTML = `
    <h2>${esc(sc.name)} <span class="hint">${verdict}</span></h2>
    <p class="brief">${esc(sc.brief)}</p>
    <div class="money ${over ? 'over' : ''}">
      <div><dt>budget</dt><dd>${num(sc.budget)}</dd></div>
      <div><dt>spent</dt><dd>${num(p.spent)}</dd></div>
      <div><dt>left</dt><dd class="${spent > budget ? 'bad' : 'hi'}">${num(p.remaining)}</dd></div>
    </div>
    ${over ? `<p class="hint bad">over budget from t=${num(p.overspent)}</p>` : ''}
    ${orders}
    ${bought ? `<h2>bought</h2><table class="rows">${bought}</table>` : ''}
  `;
}

/// What is holding the plant back, from the snapshot's own analysis.
export function renderConstraints() {
  const el = $('#constraints');
  const snap = state.snapshot;
  const list = snap && snap.constraints ? snap.constraints : [];
  if (!list.length) {
    el.innerHTML = '<p class="hint">nothing is flat out while something waits on it</p>';
    return;
  }
  el.innerHTML = list.map(c => `
    <div class="constraint">
      <b>${esc(c.name)}</b> <span class="hint">${esc(c.kind)} · ${c.rate.toFixed(3)}/tick</span>
      <div class="hint">starving ${c.starving.map(esc).join(', ')}</div>
    </div>`).join('');
}

/// The receipt. An edit that destroyed something says so, once, and keeps
/// saying it as long as that edit is on the log.
export function renderScrap() {
  const el = $('#scrap');
  const s = state.scrapped || [];
  if (!s.length) { el.hidden = true; return; }
  el.hidden = false;
  el.innerHTML = '<h2>scrapped</h2>' + s.map(x =>
    `<div class="hint"><b>${esc(x.what)}</b> — ${esc(x.detail)}</div>`).join('');
}

/// The command log, which is the document now and is worth being able to see.
export function renderLog() {
  const el = $('#log');
  const cs = state.log.commands;
  if (!cs.length) {
    el.innerHTML = '<p class="hint">nothing has happened to this plant yet</p>';
    return;
  }
  el.innerHTML = `<table class="rows">${cs.slice(-14).map(c =>
    `<tr><td class="k">t=${num(c.at)}</td><td>${esc(describeOp(c))}${
      c.node && c.node.count > 1 ? ` <span class="hint">x${num(c.node.count)}</span>` : ''}</td></tr>`)
    .join('')}</table>` +
    (cs.length > 14 ? `<p class="hint">${cs.length - 14} earlier</p>` : '');
}

/// Ask the server whether this tick reached from the beginning and this tick
/// reached from a snapshot halfway through are the same tick.
///
/// This is the P2 question asked early and cheaply. It proves nothing about
/// two machines -- both answers come from the same process -- but it proves
/// the thing that would break first: that a snapshot plus the rest of a log is
/// worth as much as the whole log.
export function initVerify() {
  $('#verifybtn').addEventListener('click', async () => {
    const out = $('#verified');
    out.textContent = 'replaying…';
    out.className = 'hint';
    const t = Math.max(1, Math.floor(state.renderTime));
    const res = await verify(t);
    if (!res.ok) {
      out.textContent = res.error;
      out.className = 'hint bad';
      return;
    }
    out.className = res.matches ? 'hint ok' : 'hint bad';
    out.innerHTML = res.matches
      ? `t=${num(res.tick)} matches from a join at t=${num(res.joinedAt)}<br>
         <span class="mono">${res.digest}</span> · ${num(res.commands)} commands · ${num(res.bytes)} bytes`
      : `DESYNC at t=${num(res.tick)}<br>
         <span class="mono">${res.digest}</span> vs <span class="mono">${res.joinedDigest}</span>`;
  });
}

function esc(s) {
  return String(s).replace(/[&<>"]/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;' }[c]));
}
