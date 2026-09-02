// The panels around the map: the room you clicked, the shelf, the twelve
// components, the shipping board and the campaign's news.
//
// Every one of them is a pure function of the last campaign frame. None of
// them holds state, none of them decides anything, and none of them predicts
// what a button will do -- pressing one posts an intention and the next frame
// says whether it happened. Same rule as Prototype 2's panels, one altitude up.

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

/// The five rooms, as buttons. A shut one says why rather than vanishing.
export function renderWhere(v, go) {
  const box = $('wherebox');
  box.hidden = false;
  box.innerHTML = v.rooms
    .map(
      r =>
        `<button data-tag="${r.tag}" class="${r.tag === v.at ? 'on' : ''}${r.done ? ' done' : ''}"` +
        `${r.open ? '' : ' disabled'} title="${esc(r.open ? r.problem : r.gate || 'not open yet')}">${r.title}</button>`
    )
    .join('');
  box.querySelectorAll('[data-tag]').forEach(b => {
    b.onclick = () => go(b.dataset.tag);
  });
}

// --------------------------------------------------------------- one room

export function renderRoom(v, tag, go) {
  const r = v.rooms.find(x => x.tag === tag) || v.rooms.find(x => x.tag === v.at);
  const box = $('roomcard');
  if (!r) {
    box.innerHTML = '<p class="muted">click a room.</p>';
    return;
  }
  $('roomtitle').textContent = r.title;
  const p = (r.goal && r.goal.progress) || { lines: [] };
  const lines = p.lines
    .map(
      l =>
        `<div class="line"><b>${esc(l.what)}</b>` +
        `<span class="${l.met ? 'met' : ''}">${num(l.have)} / ${num(l.need)} ${esc(l.unit || '')}</span></div>`
    )
    .join('');
  box.innerHTML =
    `<p class="problem">${esc(r.problem)}</p>` +
    (r.open
      ? `<div class="line"><b>objective</b><span></span></div>` +
        `<p class="problem">${esc(r.goal ? r.goal.brief : '')}</p>` +
        lines +
        `<div class="line"><b>installations</b><span>${r.installs}, ${r.machines} of them machines</span></div>` +
        `<div class="line"><b>footprint</b><span>${num(r.footprint)} tiles</span></div>` +
        (r.imports.length
          ? `<div class="line"><b>arrives here</b><span>${r.imports.join(', ')}</span></div>`
          : '') +
        (r.exports.length
          ? `<div class="line"><b>ships from here</b><span>${r.exports.join(', ')}</span></div>`
          : '') +
        (r.done ? `<div class="line"><b>producing since</b><span>${clock(r.doneAt)}</span></div>` : '') +
        `<p class="note">${esc(r.note)}</p>` +
        (r.gives.length
          ? `<p class="note">finishing it hands over: ${r.gives.map(g => esc(g.title)).join(', ')}</p>`
          : '') +
        `<button data-go="${r.tag}">${r.tag === v.at ? 'you are here' : 'go there'}</button>`
      : `<p class="shut">not open yet — ${esc(r.gate || 'something else has to come first')}</p>` +
        `<p class="note">${esc(r.note)}</p>`);
  const b = box.querySelector('[data-go]');
  if (b) b.onclick = () => go(r.tag);
}

// ---------------------------------------------------------------- the shelf

export function renderShelf(v, acts) {
  const box = $('shelf');
  const items = (v.shelf && v.shelf.designs) || [];
  if (!items.length) {
    box.innerHTML =
      '<p class="muted">nothing yet. open a machine, and keep the design that worked.</p>';
    return;
  }
  box.innerHTML = items
    .map(
      s =>
        `<div class="saved"><b>${esc(s.name)}</b>` +
        (s.fromName ? `<span class="from">from ${esc(s.fromName)}</span>` : '') +
        `<span class="note">${esc(s.note)}</span>` +
        `<span class="note">kept in ${esc(s.site)} at ${clock(s.at)}</span>` +
        `<div class="row">` +
        `<button data-copy="${s.id}">copy</button>` +
        `<button data-place="${s.id}">place</button>` +
        `<button data-forget="${s.id}">forget</button>` +
        `</div></div>`
    )
    .join('');
  box.querySelectorAll('[data-copy]').forEach(b => {
    b.onclick = () => acts.copy(+b.dataset.copy);
  });
  box.querySelectorAll('[data-place]').forEach(b => {
    b.onclick = () => acts.place(+b.dataset.place);
  });
  box.querySelectorAll('[data-forget]').forEach(b => {
    b.onclick = () => acts.forget(+b.dataset.forget);
  });
}

/// The shelf, again, as a palette in the room view: the whole point of a
/// library is that it is where you build from.
export function renderShelfPalette(v, pick) {
  const box = $('shelfpalette');
  const items = (v.shelf && v.shelf.designs) || [];
  if (!items.length) {
    box.innerHTML = '<p class="muted">nothing kept yet.</p>';
    return;
  }
  box.innerHTML = items
    .map(
      s =>
        `<button data-saved="${s.id}" title="${esc(s.note)}">${esc(s.name)}` +
        (s.fromName ? `<span class="from">from ${esc(s.fromName)}</span>` : '') +
        `</button>`
    )
    .join('');
  box.querySelectorAll('[data-saved]').forEach(b => {
    b.onclick = () => pick(+b.dataset.saved);
  });
}

// ----------------------------------------------------------------- the tech

export function renderTech(v) {
  const t = v.tech || { unlocks: [], earned: 0, total: 0 };
  $('techcount').textContent = `${t.earned} of ${t.total} unlocked · a component, never a percentage`;
  $('tech').innerHTML = t.unlocks
    .map(
      u =>
        `<div class="part ${u.got ? 'got' : 'locked'}"><b>${esc(u.title)}</b>` +
        `<span class="opens">${esc(u.opens)}</span></div>`
    )
    .join('');
}

// -------------------------------------------------------------- the lanes

export function renderLanes(v, acts) {
  const s = v.shipping || { routes: [], lanes: [], fleets: [] };
  const box = $('lanes');
  const fleets = s.fleets || [];
  box.innerHTML = (s.lanes || [])
    .map(l => {
      const open = (s.routes || []).filter(
        r => r.from === l.from && r.to === l.to && r.item === l.item
      );
      const rooms = tag => (v.rooms.find(r => r.tag === tag) || {}).title || tag;
      const head =
        `<div class="head"><span>${esc(rooms(l.from))} → ${esc(rooms(l.to))}</span>` +
        `<span>${esc(l.itemTitle || l.item)}</span></div>`;
      if (!open.length) {
        const canOpen =
          (v.rooms.find(r => r.tag === l.from) || {}).open &&
          (v.rooms.find(r => r.tag === l.to) || {}).open;
        return (
          `<div class="lane">${head}<span class="why">${esc(l.why)}</span>` +
          (canOpen
            ? `<div class="open">` +
              fleets
                .map(
                  f =>
                    `<button data-open="${l.from}|${l.to}|${l.item}|${f.tag}" ` +
                    `title="${esc(f.blurb)}">${esc(f.title)}</button>`
                )
                .join('') +
              `</div>`
            : '') +
          `</div>`
        );
      }
      return open
        .map(r => {
          const full = r.load ? Math.min(1, r.hold / r.load) : 0;
          return (
            `<div class="lane">${head}` +
            `<span class="why">${esc(r.fleetTitle)} · ${num(r.load)} a load · ` +
            `${Math.round(r.tripSeconds)}s each way · up to ${num(r.cap)}/s</span>` +
            `<div class="bar"><i style="width:${(full * 100).toFixed(0)}%"></i></div>` +
            `<span class="why">${num(r.moved)} moved in ${r.trips} trips · ` +
            `${r.inFlight} in the air</span>` +
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
    b.onclick = () => acts.cap(+id, +cap);
  });
  box.querySelectorAll('[data-close]').forEach(b => {
    b.onclick = () => acts.close(+b.dataset.close);
  });
}

// ------------------------------------------------------------------ news

/// What this room imports and exports, and where anything that is not moving
/// has stopped.
///
/// Notes 7, 10 and 16 are one panel. A room could not say what it was being
/// sent, and when something could not be delivered the message said so without
/// saying where it had gone. Nothing disappears in this game -- a load is at
/// its source, in the air, or waiting at its destination -- and the panel's
/// whole job is to name which.
export function renderRoomIO(v, tag) {
  const box = document.getElementById('roomio');
  if (!box) return;
  const room = (v.rooms || []).find(r => r.tag === tag);
  const io = room && room.io;
  if (!io) { box.innerHTML = '<p class="muted">nothing crosses the boundary yet.</p>'; return; }

  const n = x => (x >= 1e6 ? (x / 1e6).toFixed(1) + 'M'
    : x >= 1e4 ? (x / 1e3).toFixed(0) + 'k' : String(Math.round(x)));

  const flow = (r, importing) => {
    // The three places, always all three, so that a zero is an answer rather
    // than an omission.
    const where =
      `<span title="waiting at ${r.from}">${n(r.atSource)} at source</span>` +
      `<span title="between rooms">${n(r.inTransit)} in transit</span>` +
      (importing && r.bay
        ? `<span title="the yard it lands in">${r.bay}${
            r.bayFull === null || r.bayFull === undefined ? '' : ` ${r.bayFull.toFixed(0)}% full`}</span>`
        : '');
    return `<div class="io${r.blocked ? ' stuck' : ''}" data-route="${r.route}">` +
      `<div class="io-head"><span><i class="pip" style="--d:var(--${r.domain})"></i>` +
      `${r.itemTitle}</span>` +
      `<span class="n">${r.rate.toFixed(1)}/s ${importing ? 'from ' + r.from : 'to ' + r.to}</span></div>` +
      `<div class="io-where">${where}</div>` +
      (r.nextIn !== null && r.nextIn !== undefined
        ? `<div class="io-note">next arrival in ${r.nextIn.toFixed(0)}s &middot; ${r.fleet}</div>` : '') +
      (r.spilled > 0
        ? `<div class="io-note bad">${n(r.spilled)} could not be unloaded and stayed where it was</div>` : '') +
      (r.blocked ? `<div class="io-note bad">${r.blocked}</div>` : '') +
      '</div>';
  };

  // A port with no route on it is the thing that was invisible: the room can
  // take coal, and nobody is sending any.
  const idle = (ports, routes, verb) => ports
    .filter(p => !routes.some(r => r.item === p.item))
    .map(p => `<div class="io idle"><div class="io-head">` +
      `<span><i class="pip" style="--d:var(--${p.domain})"></i>${p.itemTitle}</span>` +
      `<span class="n">no route</span></div>` +
      `<div class="io-note">this room can ${verb} ${p.itemTitle.toLowerCase()}` +
      `${p.at ? ` at ${p.at}` : ''}, and nothing is.</div></div>`).join('');

  let html = '';
  html += '<h3>in</h3>' + (io.imports.map(r => flow(r, true)).join('') +
    idle(io.takes, io.imports, 'receive') || '<p class="muted">nothing arrives here.</p>');
  html += '<h3>out</h3>' + (io.exports.map(r => flow(r, false)).join('') +
    idle(io.gives, io.exports, 'ship') || '<p class="muted">nothing leaves here.</p>');
  box.innerHTML = html;
}

export function renderNews(v) {
  const items = (v.news || []).concat(
    (v.moves || []).map(m => ({ at: m.at, kind: m.arriving ? 'in' : 'out', what: m.what }))
  );
  items.sort((a, b) => b.at - a.at);
  $('news').innerHTML = items
    .slice(0, 22)
    .map(
      n =>
        `<div class="item ${esc(n.kind)}"><span class="at">${clock(n.at)}</span>` +
        `<span class="what">${esc(n.what)}</span></div>`
    )
    .join('');
}

// --------------------------------------------------------------- the room

/// Grey out the prototypes whose components have not arrived, and say which
/// one is missing. A palette that hid them would be a progression nobody could
/// look forward to.
export function markLocks(cat) {
  for (const p of cat.protos || []) {
    const b = document.querySelector(`#palette button[data-proto="${p.tag}"]`);
    if (!b) continue;
    if (p.locked) {
      b.classList.add('locked');
      b.dataset.needs = 'needs ' + (p.needs || []).map(n => n.title).join(', ');
      b.disabled = true;
    } else {
      b.classList.remove('locked');
      b.disabled = false;
    }
  }
}

function esc(s) {
  return String(s === null || s === undefined ? '' : s)
    .replace(/&/g, '&amp;')
    .replace(/</g, '&lt;')
    .replace(/>/g, '&gt;')
    .replace(/"/g, '&quot;');
}
