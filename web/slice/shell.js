// The shell around Prototype 2's region view: three centuries, two fractures,
// a machinery ledger and a provenance board.
//
// Every panel here is a pure function of one slice frame. None of them compute
// anything the server did not already say, which is the rule the whole front
// end of this project has been built on since Prototype 0 — if a panel and the
// simulation disagree, the panel is wrong, and that is findable.
//
// They say it with bars, pips and chips where they can, and keep the sentences
// the server sends for the tooltip: a board you glance at mid-build is read in
// shapes first and words second.

const $ = id => document.getElementById(id);

export const num = n => {
  if (n === null || n === undefined) return '--';
  if (Math.abs(n) >= 1e6) return (n / 1e6).toFixed(1) + 'M';
  if (Math.abs(n) >= 1e4) return (n / 1e3).toFixed(1) + 'k';
  return Number.isInteger(n) ? String(n) : n.toFixed(1);
};

export const clock = t => {
  const s = Math.floor(t / 60);
  return `${Math.floor(s / 60)}:${String(s % 60).padStart(2, '0')}`;
};

const pct = (a, b) => (b > 0 ? Math.max(0, Math.min(100, (100 * a) / b)) : 0);
const bar = (k, cls = '') =>
  `<span class="meter ${cls}"><span style="width:${k.toFixed(0)}%"></span></span>`;

// ------------------------------------------------------------- the switcher

/// The three centuries, as chips in their own colours.
///
/// All three are always enabled: there is no ladder here. The district cannot
/// get ore until 1890 is mining and 1890 cannot get a motor until somebody later
/// makes one, and neither of those is a lock.
export function renderWhere(v, go) {
  const box = $('wherebox');
  if (!box) return;
  box.hidden = false;
  const html = v.regions
    .map(r => {
      const p = r.goal && r.goal.progress;
      const k = p && p.lines.length
        ? p.lines.reduce((s, l) => s + Math.min(1, l.need ? l.have / l.need : 0), 0) / p.lines.length
        : 0;
      return (
        `<button data-tag="${r.tag}" class="e${esc(r.phase)}${r.tag === v.at ? ' on' : ''}${r.done ? ' done' : ''}" ` +
        `title="${esc(r.title)} — ${esc(r.problem)}"><b>${esc(r.phase)}</b>` +
        `<span>${esc(shortName(r.title))}</span>` +
        `<i class="prog" style="width:${(k * 100).toFixed(0)}%"></i></button>`
      );
    })
    .join('');
  if (box._html === html) return;
  box._html = html;
  box.innerHTML = html;
  box.querySelectorAll('[data-tag]').forEach(b => {
    b.onclick = () => go(b.dataset.tag);
  });
}

function shortName(title) {
  return String(title).replace(/^\d{4}\s+/, '');
}

/// The imported-plant readout in the header.
export function renderCrates(v, tag) {
  const box = $('cratebox');
  if (!box) return;
  const r = (v.regions || []).find(r => r.tag === tag);
  const m = (r && r.machinery) || { landed: 0, standing: 0, free: 0 };
  box.hidden = false;
  box.classList.toggle('none', !m.landed);
  $('crates').textContent = m.landed
    ? `${num(m.free)} free of ${num(m.landed)}`
    : 'none landed';
  box.title = m.landed
    ? `imported plant: ${num(m.standing)} gears of machinery standing in ${r.title}, ` +
      `${num(m.free)} still to spend`
    : `imported plant: nothing has been shipped backwards into ${r ? r.title : 'here'} yet`;
}

// ----------------------------------------------------------- the world view

/// One century's card, on the world view.
export function renderRegion(v, tag, go) {
  const r = (v.regions || []).find(r => r.tag === tag);
  const box = $('roomcard');
  if (!box) return;
  if (!r) { box.innerHTML = '<p class="muted">click a century.</p>'; return; }
  $('roomtitle').textContent = r.title;

  const ground = (r.ground || [])
    .map(
      g =>
        `<div class="row${g.spent ? ' spent' : ''}" title="${esc(g.where)}"><span>${esc(g.where)}</span>` +
        `<b>${num(g.perSecond)}/s ${esc(g.itemTitle)}</b></div>`
    )
    .join('');

  // What somebody else already built on: the same ground, another century.
  const taken = (r.builtOver || [])
    .map(
      b =>
        `<div class="took" title="${esc(b.became)}"><b>${esc(b.name)}</b><span>${esc(b.what)}</span></div>`
    )
    .join('');

  const p = r.goal && r.goal.progress;
  const lines = p
    ? p.lines
        .map(
          l =>
            `<div class="gl${l.met ? ' met' : ''}"><span>${esc(l.what)}</span>` +
            `${bar(pct(l.have, l.need), l.met ? 'good' : '')}<em>${num(l.have)}/${num(l.need)}</em></div>`
        )
        .join('')
    : '';
  box.innerHTML =
    `<p class="problem" title="${esc(r.note)}">${esc(r.problem)}</p>` +
    `<div class="tiles">` +
    `<div><b>${num(r.gridMW)}</b><span>MW grid</span></div>` +
    `<div><b>${r.machines}</b><span>machines</span></div>` +
    `<div><b>${num(r.footprint)}</b><span>tiles</span></div>` +
    (r.machinery && r.machinery.landed
      ? `<div class="imp"><b>${num(r.machinery.free)}</b><span>plant free</span></div>`
      : '') +
    `</div>` +
    `<h3>objective</h3>${lines || `<p class="muted">${esc((r.goal || {}).brief || '')}</p>`}` +
    (ground ? `<h3>under it</h3>${ground}` : '') +
    (taken ? `<h3>somebody already built here</h3>${taken}` : '') +
    `<button class="go" data-go="${r.tag}">stand in ${esc(r.phase)}</button>`;
  const b = box.querySelector('[data-go]');
  if (b) b.onclick = () => go(r.tag);
}

/// The fractures, and what is holding them open.
///
/// A meter per fracture: how much of the power it wants is arriving. A dark
/// fracture is 2037's grid being short, seen from 1890.
export function renderGates(v, acts) {
  const box = $('gates');
  if (!box) return;
  const s = v.shipping || {};
  const gates = s.interfaces || [];
  const crossings = v.crossings || [];
  box.innerHTML = crossings
    .map(f => {
      const g = gates.find(g => g.fracture === f.tag);
      const held = (v.regions || []).find(r => r.tag === f.heldBy);
      const who = esc(held ? held.title : f.heldBy);
      const state = !g ? 'none' : g.lit ? 'lit' : 'dark';
      const badge = !g ? 'no interface' : g.lit ? 'holding' : 'dark';
      const want = g ? g.wantMW : f.holdMW;
      const have = g ? g.haveMW : 0;
      return (
        `<div class="gate ${state}" title="${esc(f.why)}">` +
        `<div class="head"><b>${esc(f.tag)} fracture</b><span class="badge">${badge}</span></div>` +
        `<div class="span">${f.years} years</div>` +
        `<div class="power">${bar(pct(have, want), state === 'dark' ? 'bad' : state === 'lit' ? 'lit' : '')}` +
        `<em>${num(have)} / ${num(want)} MW</em></div>` +
        `<p class="why">${g ? `${who} is delivering ${num(have)} of ${num(want)} MW` : `held open by ${who}`}</p>` +
        (g
          ? `<div class="why">${num(g.carried)} carried` +
            (g.darkSeconds > 0 ? ` · dark ${Math.round(g.darkSeconds)}s` : '') + `</div>`
          : '') +
        `<div class="open">` +
        (g
          ? `<button data-gate-close="${f.tag}">take it down</button>`
          : `<button class="primary" data-gate-open="${f.tag}">build an interface</button>`) +
        `</div></div>`
      );
    })
    .join('');
  box.querySelectorAll('[data-gate-open]').forEach(b => {
    b.onclick = () => acts.open(b.dataset.gateOpen);
  });
  box.querySelectorAll('[data-gate-close]').forEach(b => {
    b.onclick = () => acts.close(b.dataset.gateClose);
  });
}

/// What the century you are standing in can and cannot make.
export function renderCentury(v, phases, tag) {
  const box = $('century');
  if (!box) return;
  const r = (v.regions || []).find(r => r.tag === tag);
  const p = (phases || []).find(p => r && p.tag === r.phase);
  const c = $('palcentury');
  if (c) c.textContent = p ? p.tag : '';
  if (!p) { box.innerHTML = '<p class="muted">&mdash;</p>'; return; }
  const lacks = (p.lacks || [])
    .map(
      l =>
        `<span class="lack" title="${esc(l.why || '')}"><b>${esc(l.part)}</b>` +
        `${l.arrives ? `<em>${esc(l.arrives)}</em>` : ''}</span>`
    )
    .join('');
  box.innerHTML =
    `<p class="blurb">${esc(p.blurb)}</p>` +
    `<div class="stat"><b>a grid to use</b><span>${p.grid ? 'yes' : 'no'}</span></div>` +
    `<div class="stat"><b>frames</b><span>${(p.materials || []).join(', ')}</span></div>` +
    `<div class="stat"><b>components</b><span>${p.holds} of 37</span></div>` +
    (lacks ? `<h3>has not got</h3><div class="lacks">${lacks}</div>` : '');
}

// ------------------------------------------------------------- the shipping

/// The routes, open ones first, then every lane that could be one.
///
/// A lane that is not running is one row with a fleet picker and a button,
/// rather than a paragraph and a button per fleet.
export function renderLanes(v, acts) {
  const s = v.shipping || { routes: [], lanes: [], fleets: [] };
  const box = $('lanes');
  if (!box) return;
  const fleets = s.fleets || [];
  const phase = tag => ((v.regions || []).find(r => r.tag === tag) || {}).phase || '';
  const head = l =>
    `<span class="dir">${esc(phase(l.from))} → ${esc(phase(l.to))}</span>` +
    `<span class="it">${esc(l.itemTitle || l.item)}</span>`;

  const running = [];
  const idle = [];
  for (const l of s.lanes || []) {
    const open = (s.routes || []).filter(
      r => r.from === l.from && r.to === l.to && r.item === l.item
    );
    if (!open.length) {
      // A fleet belongs to a century: the list a lane offers is the list its
      // *origin* can field.
      const from = (v.regions || []).find(r => r.tag === l.from) || {};
      const can = fleets.filter(f => !f.from || Number(f.from) <= Number(from.year || 0));
      idle.push(
        `<div class="lane idle" title="${esc(l.why)}"><div class="head">${head(l)}</div>` +
        `<div class="open"><select data-fleet="${l.from}|${l.to}|${l.item}">` +
        can.map(f => `<option value="${f.tag}" title="${esc(f.blurb)}">${esc(f.title)}</option>`).join('') +
        `</select><button class="primary" data-open="${l.from}|${l.to}|${l.item}">open</button></div></div>`
      );
      continue;
    }
    for (const r of open) {
      const made = (r.madeOf || []).map(m => `${num(m.qty)} out of ${m.origin}`).join(', ');
      running.push(
        `<div class="lane live"><div class="head">${head(l)}</div>` +
        `<div class="stats"><span title="fleet">${esc(r.fleetTitle)}</span>` +
        `<span title="each way">${Math.round(r.tripSeconds)}s</span>` +
        `<span title="cap">≤${num(r.cap)}/s</span>` +
        `<span title="moved in ${num(r.trips)} trips">${num(r.moved)} moved</span></div>` +
        (made ? `<span class="made">waiting: ${esc(made)}</span>` : '') +
        (r.heldBack > 0 ? `<span class="spill">${num(r.heldBack)} turned away at the fracture</span>` : '') +
        (r.spilled > 0 ? `<span class="spill">${num(r.spilled)} would not fit in the yard</span>` : '') +
        `<div class="open">` +
        `<button data-cap="${r.id}|${Math.max(1, Math.round(r.cap / 2))}" title="halve the cap">−</button>` +
        `<button data-cap="${r.id}|${Math.round(r.cap * 2)}" title="double the cap">+</button>` +
        `<button data-close="${r.id}" title="close the route">close</button>` +
        `</div></div>`
      );
    }
  }
  box.innerHTML =
    (running.length ? running.join('') : '<p class="muted">no routes open.</p>') +
    (idle.length ? `<h3>could open</h3>${idle.join('')}` : '');
  box.querySelectorAll('[data-open]').forEach(b => {
    const [from, to, item] = b.dataset.open.split('|');
    b.onclick = () => {
      const sel = b.parentNode && b.parentNode.querySelector && b.parentNode.querySelector('select');
      const fleet = sel ? sel.value : ((fleets[0] || {}).tag);
      acts.open(from, to, item, fleet);
    };
  });
  box.querySelectorAll('[data-cap]').forEach(b => {
    const [id, cap] = b.dataset.cap.split('|');
    b.onclick = () => acts.cap(Number(id), Number(cap));
  });
  box.querySelectorAll('[data-close]').forEach(b => {
    b.onclick = () => acts.close(Number(b.dataset.close));
  });
}

/// What every region is made of, by the century it came out of: one stacked
/// bar per item, a segment per century.
export function renderProvenance(v) {
  const box = $('provenance');
  if (!box) return;
  const rows = ((v.shipping || {}).provenance || []).filter(p => (p.sentFrom || []).length);
  if (!rows.length) {
    box.innerHTML = '<p class="muted">nothing has crossed a fracture yet.</p>';
    return;
  }
  const name = tag => ((v.regions || []).find(r => r.tag === tag) || {}).phase || tag;
  box.innerHTML = rows
    .map(p => {
      const total = p.sentFrom.reduce((s, f) => s + f.qty, 0) || 1;
      return (
        `<div class="prov"><div class="head"><span>${esc(p.itemTitle)} in ${esc(name(p.region))}` +
        `</span><span>${num(p.holding)} held</span></div>` +
        `<div class="stack">` +
        p.sentFrom
          .map(f => `<span class="e${esc(f.phase)}" style="flex:${f.qty / total}" title="${num(f.qty)} out of ${esc(f.phase)}"></span>`)
          .join('') +
        `</div>` +
        p.sentFrom
          .map(f => `<div class="from"><b class="t${esc(f.phase)}">${esc(f.phase)}</b><span>${num(f.qty)}</span></div>`)
          .join('') +
        `</div>`
      );
    })
    .join('');
}

export function renderNews(v) {
  const box = $('news');
  if (!box) return;
  const items = (v.news || []).concat([]).slice(0, 18);
  box.innerHTML = items.length
    ? items
        .map(
          n =>
            `<div class="news ${esc(n.kind)}"><span class="at">${clock(n.at)}</span>` +
            `<span>${esc(n.what)}</span></div>`
        )
        .join('')
    : '<p class="muted">nothing has happened yet.</p>';
}

// ------------------------------------------------------------ region panels

/// What crosses this region's boundary, as one row per item.
///
/// Arrow, item, rate, and a bar for the yard it lands in. The sentences the old
/// panel carried are the row's tooltip.
export function renderRegionIO(v, tag) {
  const box = $('roomio');
  if (!box) return;
  const region = (v.regions || []).find(r => r.tag === tag);
  const io = region && region.io;
  if (!io) { box.innerHTML = '<p class="muted">nothing crosses yet.</p>'; return; }

  const flow = (r, importing) => {
    const tip =
      `${num(r.atSource)} at ${r.from} · ${num(r.inTransit)} in transit` +
      (r.years ? ` · ${r.years} years out of its time` : '') +
      (r.nextIn !== null && r.nextIn !== undefined ? ` · next in ${r.nextIn.toFixed(0)}s by ${r.fleet}` : '') +
      (r.heldBack > 0 ? ` · ${num(r.heldBack)} turned away` : '') +
      (r.blocked ? ` · ${r.blocked}` : '');
    const full = importing && r.bayFull !== null && r.bayFull !== undefined ? r.bayFull : null;
    return (
      `<div class="io${r.blocked ? ' stuck' : ''}" data-route="${r.route}" title="${esc(tip)}">` +
      `<span class="dir ${importing ? 'in' : 'out'}">${importing ? '←' : '→'}</span>` +
      `<i class="pip" style="--d:var(--${r.domain})"></i>` +
      `<span class="it">${esc(r.itemTitle)}</span>` +
      `<span class="n">${(r.rate || 0).toFixed(1)}/s</span>` +
      `<span class="where">${esc(importing ? r.from : r.to)}</span>` +
      (full !== null ? bar(full, full > 90 ? 'bad' : '') : '<span></span>') +
      '</div>'
    );
  };

  const idle = (ports, routes, importing) =>
    (ports || [])
      .filter(p => !routes.some(r => r.item === p.item))
      .map(
        p =>
          `<div class="io idle" title="this region can ${importing ? 'receive' : 'ship'} ` +
          `${esc(p.itemTitle.toLowerCase())}${p.at ? ` at ${esc(p.at)}` : ''}; open a route on the world view">` +
          `<span class="dir ${importing ? 'in' : 'out'}">${importing ? '←' : '→'}</span>` +
          `<i class="pip" style="--d:var(--${p.domain})"></i>` +
          `<span class="it">${esc(p.itemTitle)}</span>` +
          `<span class="n">no route</span><span></span><span></span></div>`
      )
      .join('');

  const html =
    io.imports.map(r => flow(r, true)).join('') +
    idle(io.takes, io.imports, true) +
    io.exports.map(r => flow(r, false)).join('') +
    idle(io.gives, io.exports, false);
  box.innerHTML = html || '<p class="muted">nothing crosses here.</p>';
}

function esc(s) {
  return String(s === null || s === undefined ? '' : s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
