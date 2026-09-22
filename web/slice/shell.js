// The shell around Prototype 2's region view: three centuries, two fractures,
// a machinery ledger and a provenance board.
//
// Every panel here is a pure function of one slice frame. None of them compute
// anything the server did not already say, which is the rule the whole front
// end of this project has been built on since Prototype 0 — if a panel and the
// simulation disagree, the panel is wrong, and that is findable.

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

// ------------------------------------------------------------- the switcher

/// The three centuries, as buttons.
///
/// All three are always enabled, which is the one visible difference from
/// Prototype 3's switcher and is the whole design: there is no ladder here.
/// The district cannot get ore until 1890 is mining and 1890 cannot get a motor
/// until somebody later makes one, and neither of those is a lock — it is a
/// supply chain with a century and a half in the middle of it.
export function renderWhere(v, go) {
  const box = $('wherebox');
  if (!box) return;
  box.hidden = false;
  box.innerHTML = v.regions
    .map(
      r =>
        `<button data-tag="${r.tag}" class="${r.tag === v.at ? 'on' : ''}${r.done ? ' done' : ''}" ` +
        `title="${esc(r.problem)}"><b>${esc(r.phase)}</b><span>${esc(shortName(r.title))}</span></button>`
    )
    .join('');
  box.querySelectorAll('[data-tag]').forEach(b => {
    b.onclick = () => go(b.dataset.tag);
  });
}

/// "1890 Mining Valley" is the title; the button only has room for the half
/// that is not already the year on it.
function shortName(title) {
  return String(title).replace(/^\d{4}\s+/, '');
}

/// The imported-plant readout in the header.
///
/// In 2037 and 2070 this is zero forever and says so quietly. In 1890 it is the
/// number that decides what the region is allowed to be, so it is on screen at
/// all times rather than behind a panel.
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
    ? `${num(m.standing)} gears of imported machinery standing in ${r.title}, ` +
      `${num(m.free)} still to spend`
    : `nothing has been shipped backwards into ${r ? r.title : 'here'} yet`;
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
        `<div class="row${g.spent ? ' spent' : ''}"><span>${esc(g.where)}</span>` +
        `<b>${num(g.perSecond)}/s ${esc(g.itemTitle)}</b></div>`
    )
    .join('');

  // What somebody else already built on. The panel that has to make a player
  // understand that this is the same ground they were standing on a moment ago.
  const taken = (r.builtOver || [])
    .map(
      b =>
        `<div class="took"><b>${esc(b.name)}</b><span>${esc(b.what)}</span>` +
        `<p>${esc(b.became)}</p></div>`
    )
    .join('');

  const p = r.goal && r.goal.progress;
  box.innerHTML =
    `<p class="problem">${esc(r.problem)}</p>` +
    `<p class="note">${esc(r.note)}</p>` +
    `<div class="stat"><b>objective</b><span>${esc((r.goal || {}).brief || '')}</span></div>` +
    (p
      ? `<div class="stat"><b>progress</b><span>${p.lines.filter(l => l.met).length}` +
        ` of ${p.lines.length} met</span></div>`
      : '') +
    `<div class="stat"><b>on the grid</b><span>${num(r.gridMW)} MW</span></div>` +
    `<div class="stat"><b>built</b><span>${r.machines} machines, ${num(r.footprint)} tiles</span></div>` +
    (r.machinery && r.machinery.landed
      ? `<div class="stat"><b>imported plant</b><span>${num(r.machinery.standing)} standing, ` +
        `${num(r.machinery.free)} free</span></div>`
      : '') +
    (ground ? `<h3>under it</h3>${ground}` : '') +
    (taken ? `<h3>somebody already built here</h3>${taken}` : '') +
    `<button class="go" data-go="${r.tag}">stand in ${esc(r.phase)}</button>`;
  const b = box.querySelector('[data-go]');
  if (b) b.onclick = () => go(r.tag);
}

/// The fractures, and what is holding them open.
///
/// The only panel in this project that can report a failure which is not about
/// a factory: a dark fracture is 2037's grid being short, seen from 1890.
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
      const state = !g ? 'none' : g.lit ? 'lit' : 'dark';
      const words = !g
        ? `no interface. ${esc(held ? held.title : f.heldBy)} would hold it open ` +
          `with ${f.holdMW} MW.`
        : g.lit
          ? `holding ${g.holding} years open on ${g.wantMW} MW, ` +
            `and ${esc(held ? held.title : f.heldBy)} is delivering ${g.haveMW}.`
          : `<b>dark.</b> it wants ${g.wantMW} MW to hold ${g.holding} years and ` +
            `${esc(held ? held.title : f.heldBy)} is delivering ${g.haveMW}.`;
      return (
        `<div class="gate ${state}">` +
        `<div class="head"><span>${esc(f.tag)} fracture</span>` +
        `<span>${f.years} years</span></div>` +
        `<p class="why">${words}</p>` +
        (g
          ? `<div class="why">${num(g.carried)} carried · gauge ${num(g.gauge)}/s` +
            (g.darkSeconds > 0 ? ` · dark for ${Math.round(g.darkSeconds)}s so far` : '') +
            `</div>`
          : '') +
        `<p class="why muted">${esc(f.why)}</p>` +
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
  if (!p) { box.innerHTML = '<p class="muted">&mdash;</p>'; return; }
  const lacks = (p.lacks || [])
    .map(
      l =>
        `<div class="lack"><b>${esc(l.part)}</b><span>${esc(l.arrives || '')}</span>` +
        `<p>${esc(l.why || '')}</p></div>`
    )
    .join('');
  box.innerHTML =
    `<p class="blurb">${esc(p.blurb)}</p>` +
    `<div class="stat"><b>a grid to use</b><span>${p.grid ? 'yes' : 'no'}</span></div>` +
    `<div class="stat"><b>frames it makes</b><span>${(p.materials || []).join(', ')}</span></div>` +
    `<div class="stat"><b>components</b><span>${p.holds} of 37</span></div>` +
    (lacks ? `<h3>and the ones it has not got</h3>${lacks}` : '');
}

// ------------------------------------------------------------- the shipping

export function renderLanes(v, acts) {
  const s = v.shipping || { routes: [], lanes: [], fleets: [] };
  const box = $('lanes');
  if (!box) return;
  const fleets = s.fleets || [];
  const name = tag => ((v.regions || []).find(r => r.tag === tag) || {}).title || tag;
  const phase = tag => ((v.regions || []).find(r => r.tag === tag) || {}).phase || '';
  box.innerHTML = (s.lanes || [])
    .map(l => {
      const open = (s.routes || []).filter(
        r => r.from === l.from && r.to === l.to && r.item === l.item
      );
      const head =
        `<div class="head"><span>${esc(phase(l.from))} → ${esc(phase(l.to))}</span>` +
        `<span>${esc(l.itemTitle || l.item)}</span></div>`;
      if (!open.length) {
        // A fleet belongs to a century: the wagon list a lane offers is the
        // list its *origin* can field, which is section 2's "modern logistics"
        // spent rather than asserted.
        const from = (v.regions || []).find(r => r.tag === l.from) || {};
        const can = fleets.filter(f => !f.from || Number(f.from) <= Number(from.year || 0));
        return (
          `<div class="lane">${head}<span class="why">${esc(l.why)}</span>` +
          `<div class="open">` +
          can
            .map(
              f =>
                `<button data-open="${l.from}|${l.to}|${l.item}|${f.tag}" ` +
                `title="${esc(f.blurb)}">${esc(f.title)}</button>`
            )
            .join('') +
          `</div></div>`
        );
      }
      return open
        .map(r => {
          const made = (r.madeOf || [])
            .map(m => `${num(m.qty)} out of ${m.origin}`)
            .join(', ');
          return (
            `<div class="lane">${head}` +
            `<span class="why">${esc(r.fleetTitle)} · ${Math.round(r.tripSeconds)}s each way ` +
            `· up to ${num(r.cap)}/s</span>` +
            `<span class="why">${num(r.moved)} moved in ${num(r.trips)} trips · ` +
            `${r.inFlight} in the fracture</span>` +
            (made ? `<span class="made">waiting: ${esc(made)}</span>` : '') +
            (r.heldBack > 0
              ? `<span class="spill">${num(r.heldBack)} turned away at the fracture</span>`
              : '') +
            (r.spilled > 0
              ? `<span class="spill">${num(r.spilled)} would not fit in the yard</span>`
              : '') +
            `<div class="open">` +
            `<button data-cap="${r.id}|${Math.max(1, Math.round(r.cap / 2))}">slower</button>` +
            `<button data-cap="${r.id}|${Math.round(r.cap * 2)}">faster</button>` +
            `<button data-close="${r.id}">close</button>` +
            `</div></div>`
          );
        })
        .join('');
    })
    .join('');
  box.querySelectorAll('[data-open]').forEach(b => {
    const [from, to, item, fleet] = b.dataset.open.split('|');
    b.onclick = () => acts.open(from, to, item, fleet);
  });
  box.querySelectorAll('[data-cap]').forEach(b => {
    const [id, cap] = b.dataset.cap.split('|');
    b.onclick = () => acts.cap(Number(id), Number(cap));
  });
  box.querySelectorAll('[data-close]').forEach(b => {
    b.onclick = () => acts.close(Number(b.dataset.close));
  });
}

/// What every region is made of, by the century it came out of.
///
/// The panel this experiment exists to be able to draw. Ore in 2037 that says
/// *out of 1890* is the success criterion, on screen, without anybody having to
/// be told that it is.
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
    .map(
      p =>
        `<div class="prov"><div class="head"><span>${esc(p.itemTitle)} in ${esc(name(p.region))}` +
        `</span><span>${num(p.holding)} held</span></div>` +
        (p.sentFrom || [])
          .map(f => `<div class="from"><b>${esc(f.phase)}</b><span>${num(f.qty)}</span></div>`)
          .join('') +
        `</div>`
    )
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

/// What crosses this region's boundary, in both directions, with the years on
/// it.
export function renderRegionIO(v, tag) {
  const box = $('roomio');
  if (!box) return;
  const region = (v.regions || []).find(r => r.tag === tag);
  const io = region && region.io;
  if (!io) { box.innerHTML = '<p class="muted">nothing crosses the boundary yet.</p>'; return; }

  const flow = (r, importing) => {
    const where =
      `<span title="waiting at ${esc(r.from)}">${num(r.atSource)} at source</span>` +
      `<span title="inside the fracture">${num(r.inTransit)} in transit</span>` +
      (importing && r.bay
        ? `<span title="the yard it lands in">${esc(r.bay)}${
            r.bayFull === null || r.bayFull === undefined
              ? ''
              : ` ${r.bayFull.toFixed(0)}% full`}</span>`
        : '');
    return (
      `<div class="io${r.blocked ? ' stuck' : ''}" data-route="${r.route}">` +
      `<div class="io-head"><span><i class="pip" style="--d:var(--${r.domain})"></i>` +
      `${esc(r.itemTitle)}</span>` +
      `<span class="n">${(r.rate || 0).toFixed(1)}/s ${importing ? 'from' : 'to'} ` +
      `${esc(importing ? r.from : r.to)}</span></div>` +
      (r.years ? `<div class="io-years">${r.years} years out of its time</div>` : '') +
      `<div class="io-where">${where}</div>` +
      (r.nextIn !== null && r.nextIn !== undefined
        ? `<div class="io-note">next arrival in ${r.nextIn.toFixed(0)}s &middot; ${esc(r.fleet)}</div>`
        : '') +
      (r.heldBack > 0
        ? `<div class="io-note bad">${num(r.heldBack)} turned away at the fracture</div>`
        : '') +
      (r.blocked ? `<div class="io-note bad">${esc(r.blocked)}</div>` : '') +
      '</div>'
    );
  };

  const idle = (ports, routes, verb) =>
    (ports || [])
      .filter(p => !routes.some(r => r.item === p.item))
      .map(
        p =>
          `<div class="io idle"><div class="io-head">` +
          `<span><i class="pip" style="--d:var(--${p.domain})"></i>${esc(p.itemTitle)}</span>` +
          `<span class="n">no route</span></div>` +
          `<div class="io-note">this region can ${verb} ${esc(p.itemTitle.toLowerCase())}` +
          `${p.at ? ` at ${esc(p.at)}` : ''}, and nothing is.</div>` +
          ((p.sentFrom || []).length
            ? `<div class="io-note">ever sent: ` +
              p.sentFrom.map(f => `${num(f.qty)} out of ${f.phase}`).join(', ') +
              `</div>`
            : '') +
          `</div>`
      )
      .join('');

  let html = '';
  html +=
    '<h3>in</h3>' +
    (io.imports.map(r => flow(r, true)).join('') + idle(io.takes, io.imports, 'receive') ||
      '<p class="muted">nothing arrives here.</p>');
  html +=
    '<h3>out</h3>' +
    (io.exports.map(r => flow(r, false)).join('') + idle(io.gives, io.exports, 'ship') ||
      '<p class="muted">nothing leaves here.</p>');
  box.innerHTML = html;
}

/// What is standing on the ground in this century, beside the plot.
export function renderTerrain(list) {
  const box = $('terrainkey');
  if (!box) return;
  if (!list || !list.length) { box.innerHTML = '<p class="muted">&mdash;</p>'; return; }
  box.innerHTML = list
    .map(
      f =>
        `<div class="terr${f.taken ? ' taken' : ''}">` +
        `<i style="background:${esc(f.colour)}"></i>` +
        `<b>${esc(f.name)}</b><span>${esc(f.what)}` +
        `${f.yields ? ` · ${num(f.yields.perSecond)}/s` : ''}` +
        `${f.taken ? ' · nothing may be built here' : ''}</span></div>`
    )
    .join('');
}

// ------------------------------------------------------------- the palette

/// Put the price on the palette.
///
/// Nothing is greyed out for being from the wrong century — the one exception
/// is a design that could not be legal at any price, which is a grid connection
/// in a century with no grid. Everything else carries what the book's version
/// of it would cost to stand up here, because that is the question a player has.
export function markPrices(cat) {
  for (const p of cat.protos || []) {
    const b = document.querySelector(`#palette button[data-proto="${p.tag}"]`);
    if (!b) continue;
    b.classList.toggle('illegal', !!p.illegal);
    b.classList.toggle('costly', !p.illegal && p.stockCost > 0);
    b.disabled = false;
    const badge = b.querySelector('.price') || Object.assign(document.createElement('i'), { className: 'price' });
    if (p.illegal) {
      badge.textContent = 'no';
      b.title = p.why || 'not possible in this century';
    } else if (p.stockCost > 0) {
      badge.textContent = num(p.stockCost);
      b.title =
        `the standard one costs ${num(p.stockCost)} gears of imported machinery here` +
        ((p.imports || []).length
          ? ` — ${p.imports.map(i => i.title).join(', ')}`
          : '') +
        (p.short > 0 ? `; ${num(p.short)} short` : '');
    } else {
      badge.textContent = '';
    }
    if (!badge.parentNode) b.appendChild(badge);
  }
  const c = $('palcentury');
  if (c) c.textContent = cat.phase || '';
}

function esc(s) {
  return String(s === null || s === undefined ? '' : s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
