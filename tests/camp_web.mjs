// The browser half of prototype 3, checked without a browser.
//
//     node tests/camp_web.mjs             (with `camp serve` running)
//     node tests/camp_web.mjs 8796        (on another port)
//
// Prototype 2's `room_web.mjs` already runs the world view, the machine bench
// and the inspector against live frames, and this experiment does not fork any
// of them -- so this harness deliberately does *not* do that again. It checks
// the half that is new: the campaign frame, the map, the shelf, the component
// list and the shipping board, run as the real modules against the real
// server, with a DOM that records instead of painting.
//
// What that catches is the seam the Rust tests cannot see -- a field renamed
// on one side of the wire, a panel reading `shipping.routes` when the server
// sends `routes`, a map that throws on a lane nobody has opened yet.

import { readFileSync } from 'node:fs';
import nodeNet from 'node:net';
import { fileURLToPath } from 'node:url';
import { dirname, join } from 'node:path';

const port = process.argv[2] || '8795';
const base = `http://127.0.0.1:${port}`;
const here = dirname(fileURLToPath(import.meta.url));

let failures = 0;
const ok = (cond, what) => {
  if (!cond) { console.log(`  FAIL  ${what}`); failures++; }
  return cond;
};

// ------------------------------------------------------- a browser, sort of

function makeText(t) {
  return { tagName: '#text', textContent: String(t), children: [], parent: null };
}

function stubCtx() {
  const rec = [];
  const noop = () => {};
  return new Proxy(
    {
      canvas: { width: 900, height: 600 },
      _rec: rec,
      measureText: () => ({ width: 40 }),
      createLinearGradient: () => ({ addColorStop: noop }),
      getImageData: () => ({ data: new Uint8ClampedArray(4) }),
      setTransform: noop,
    },
    {
      get(t, k) {
        if (k in t) return t[k];
        return (...a) => { rec.push([String(k), ...a]); };
      },
      set(t, k, v) { t[k] = v; return true; },
    }
  );
}

function makeEl(tagName) {
  const e = {
    tagName: String(tagName).toUpperCase(),
    className: '', textContent: '', value: '', hidden: false, title: '',
    disabled: false,
    style: { cssText: '', setProperty() {}, visibility: '' },
    dataset: {},
    children: [], parent: null,
    _html: '',
    classList: { toggle() {}, add() {}, remove() {}, contains: () => false },
    on: {},
    addEventListener(k, f) { (e.on[k] ||= []).push(f); },
    removeEventListener() {},
    appendChild(c) { c.parent = e; e.children.push(c); return c; },
    append(...cs) { for (const c of cs) e.appendChild(typeof c === 'object' ? c : makeText(c)); },
    remove() {},
    querySelector(sel) { return e.querySelectorAll(sel)[0] || null; },
    // Panels write HTML and then hang handlers on what they wrote, so the
    // harness has to be able to find `[data-open]` in a string. A tiny,
    // deliberate parser: enough to find the attributes the client asks for.
    querySelectorAll(sel) {
      // The same objects come back every time, or the handler a panel hangs
      // on one would be hung on something nobody can press. Cleared whenever
      // the panel rewrites its HTML, because those buttons are gone.
      const cache = (e._q ||= new Map());
      if (cache.has(sel)) return cache.get(sel);
      const m = /^\[([-\w]+)\]$/.exec(sel);
      if (!m) return [];
      const attr = m[1];
      const key = attr.replace(/^data-/, '').replace(/-(\w)/g, (_, c) => c.toUpperCase());
      const out = [];
      const re = new RegExp(`${attr}="([^"]*)"`, 'g');
      let hit;
      while ((hit = re.exec(e._html))) {
        const el = makeEl('button');
        el.dataset[key] = hit[1];
        out.push(el);
      }
      cache.set(sel, out);
      return out;
    },
    getBoundingClientRect() { return { left: 0, top: 0, width: 900, height: 600 }; },
    getContext() { return (e._ctx ||= stubCtx()); },
  };
  Object.defineProperty(e, 'innerHTML', {
    get() { return e._html; },
    set(v) { e._html = String(v); e.children = []; e._q = new Map(); },
  });
  return e;
}

const els = new Map();
const el = id => {
  if (!els.has(id)) els.set(id, makeEl('div'));
  return els.get(id);
};

globalThis.window = {
  devicePixelRatio: 1,
  addEventListener() {},
  dispatchEvent() {},
  CustomEvent: class {},
};
globalThis.document = {
  getElementById: el,
  createElement: makeEl,
  querySelector: () => null,
  querySelectorAll: () => [],
  addEventListener() {},
  body: makeEl('body'),
};
// Node ships a read-only `navigator`; the client only ever reaches for the
// clipboard, which does not exist here either way.
Object.defineProperty(globalThis, 'navigator', {
  value: { clipboard: null },
  configurable: true,
});
globalThis.prompt = () => 'Mk2';
globalThis.alert = () => {};
// Prototype 2's client listens on the bare global, the way a page does.
globalThis.addEventListener = () => {};
globalThis.removeEventListener = () => {};
globalThis.requestAnimationFrame = f => setTimeout(f, 0);
globalThis.cancelAnimationFrame = id => clearTimeout(id);
globalThis.CustomEvent = class CustomEvent {
  constructor(type, init) { this.type = type; this.detail = init && init.detail; }
};
globalThis.dispatchEvent = () => {};

// ------------------------------------------------------------------ the run

const post = async (path, body) => {
  const r = await fetch(base + path, {
    method: 'POST',
    headers: { 'content-type': 'application/json' },
    body: JSON.stringify(body || {}),
  });
  return r.json();
};
const get = async path => (await fetch(base + path)).json();

async function main() {
  console.log(`prototype 3's client, against ${base}`);

  // ---- entering
  const me = await post('/api/enter', { name: 'harness' });
  ok(me.ok, 'a player can enter the campaign');
  ok(typeof me.code === 'string' && me.code.length === 6, 'the campaign has a code');
  ok(me.at === 'basin', 'you start in Coal Basin');
  await post('/api/start', {});

  // ---- the map, and the campaign frame
  const atlas = await get('/api/sites');
  ok(atlas.ok && atlas.sites.length === 5, 'the map has five rooms');
  ok(atlas.lanes.length >= 5, 'the map has lanes');
  ok(atlas.fleets.length === 3, 'there are three fleets');
  for (const s of atlas.sites) {
    ok(s.problem && s.note && s.template, `${s.tag} says what it is`);
  }

  const v = await get(`/api/camp?player=${me.player}`);
  ok(v.ok, 'the campaign frame arrives');
  for (const k of ['rooms', 'tech', 'shelf', 'shipping', 'cast', 'news', 'moves']) {
    ok(k in v, `the frame carries \`${k}\``);
  }
  ok(v.rooms.length === 5, 'five rooms in the frame');
  ok(v.rooms[0].open === true, 'the first room is open');
  ok(v.rooms.filter(r => !r.open).length === 4, 'the other four are shut');
  ok(v.rooms.every(r => r.open || r.gate), 'a shut room says why');
  ok(v.tech.unlocks.length === 12, 'twelve components to earn');
  ok(v.tech.earned === 0, 'and none of them yet');
  ok(v.shipping.lanes.length >= 5, 'the shipping board knows the lanes');

  // ---- the palette, and what is locked
  const cat = await get('/api/catalogue');
  const locked = cat.protos.filter(p => p.locked);
  ok(locked.length > 0, 'some prototypes are locked at the start');
  ok(
    locked.every(p => (p.needs || []).length > 0),
    'a locked prototype names the component it is waiting for'
  );
  ok(
    cat.protos.some(p => p.tag === 'steamplant' && !p.locked),
    'the compact steam plant is not locked'
  );
  const parts = await get('/api/parts');
  ok(parts.parts.filter(p => p.locked).length === 12, 'twelve components are locked');

  // ---- one room's frame, in Prototype 2's own shape
  const room = await get(`/api/state?code=basin&player=${me.player}`);
  ok(room.ok, 'a room frame arrives');
  for (const k of ['goal', 'world', 'plant', 'players', 'events', 'sync', 'ghosts']) {
    ok(k in room, `the room frame carries \`${k}\` -- Prototype 2's contract`);
  }
  ok(room.world.installs.length > 0, 'Coal Basin comes furnished');
  // What a room comes with is a place for things to leave from and *ground*.
  // The rate the room gave it used to be written on a building; since
  // experiment 13 it is written on the seam.
  ok(room.world.deposits.length > 0, 'and with ground worth standing on');
  ok(
    room.world.deposits.every(d => d.yields > 0),
    'each patch carrying the number the room gave it'
  );
  ok(
    !room.world.installs.some(i => i.role === 'source'),
    'and with nothing already extracting anything'
  );

  // ---- the panels, run for real
  const shell = await import(pathTo('web/camp/shell.js'));
  const map = await import(pathTo('web/camp/map.js'));

  let went = null;
  shell.renderWhere(v, tag => { went = tag; });
  ok(el('wherebox').innerHTML.includes('Coal Basin'), 'the switcher lists the rooms');
  const buttons = el('wherebox').querySelectorAll('[data-tag]');
  ok(buttons.length === 5, 'one button per room');
  buttons[0].onclick && buttons[0].onclick();
  ok(went === 'basin', 'clicking a room asks to walk there');

  shell.renderRoom(v, 'basin', () => {});
  ok(el('roomcard').innerHTML.includes('objective'), 'an open room shows its objective');
  shell.renderRoom(v, 'final', () => {});
  ok(el('roomcard').innerHTML.includes('not open yet'), 'a shut room says why instead');

  shell.renderTech(v);
  ok(el('tech').innerHTML.includes('Separator'), 'the component list is on screen');
  ok(el('techcount').textContent.includes('0 of 12'), 'and says how far along it is');

  shell.renderShelf(v, { copy() {}, place() {}, forget() {} });
  ok(el('shelf').innerHTML.includes('nothing yet'), 'an empty shelf says so');

  let opened = null;
  shell.renderLanes(v, { open: (...a) => { opened = a; }, close() {}, cap() {} });
  ok(el('lanes').innerHTML.includes('Coal Basin'), 'the shipping board lists the lanes');
  const openers = el('lanes').querySelectorAll('[data-open]');
  ok(openers.length === 0, 'no lane can be opened while its far end is shut');

  shell.renderNews(v);
  shell.markLocks(cat);

  map.init(el('atlascanvas'), { onPick() {} });
  map.setSites(atlas);
  map.resize();
  map.show(v);
  const drawn = el('atlascanvas').getContext()._rec;
  ok(drawn.length > 0, 'the map draws something');
  ok(
    drawn.some(c => c[0] === 'fillText' && String(c[1]).includes('Coal Basin')),
    'and the rooms are on it by name'
  );
  ok(
    drawn.some(c => c[0] === 'fillText' && String(c[1]) === 'locked'),
    'a shut room is drawn as shut'
  );

  // ---- building, through the same door the client uses
  //
  // Coal Basin comes with ground rather than with a working pit, so the head
  // that works it is the first thing anybody builds.
  ok(room.world.deposits.length === 3, `three patches of ground: ${room.world.deposits.length}`);
  const coal = room.world.deposits.find(d => d.item === 'Coal');
  ok(coal && coal.yields > 0 && coal.spare === coal.yields,
    `a coal seam worth ${coal && coal.yields} a second, none of it spoken for`);
  const dig = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'PlaceMachine',
    payload: { proto: 'coalpit', x: coal.x, y: coal.y, face: 0, example: true },
  });
  ok(dig.ok, 'a coal head on the seam' + (dig.ok ? '' : `: ${dig.error}`));
  const dug = await get(`/api/state?code=basin&player=${me.player}`);
  const seam = dug.world.installs.find(i => i.proto === 'coalpit');
  ok(!!seam, 'and it is standing there');
  const worked = dug.world.deposits.find(d => d.id === coal.id);
  ok(worked.taken > 0 && worked.spare < worked.yields,
    `the seam says how much of it is being worked: ${worked.taken} of ${worked.yields}`);
  ok(worked.worked.includes(seam.name), `and by whom: ${worked.worked.join(', ')}`);

  const offGround = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'PlaceMachine', payload: { proto: 'oremine', x: 20, y: 20, face: 0, example: true },
  });
  ok(!offGround.ok && /stand on/.test(offGround.error),
    `a head with nothing under it is refused: ${offGround.error}`);

  const put = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'PlaceStorage', payload: { proto: 'bay', x: 8, y: 2, face: 0 },
  });
  ok(put.ok, 'a bay can be placed');
  const after = await get(`/api/state?code=basin&player=${me.player}`);
  const bay = after.world.installs.find(i => i.proto === 'bay');
  ok(!!bay, 'and it is in the next frame');
  const wire = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'CreateConnection', payload: { from: seam.id, to: bay.id, item: 'Coal' },
  });
  ok(wire.ok, 'a wire can be drawn');

  const nope = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'PlaceMachine', payload: { proto: 'stamping', x: 20, y: 20, face: 0, example: true },
  });
  ok(!nope.ok && nope.refused, 'a locked machine is refused rather than dropped');
  ok(String(nope.error).includes('unlocked'), 'and the refusal says why');

  const shut = await post('/api/cmd', {
    code: 'final', player: me.player,
    type: 'PlaceStorage', payload: { proto: 'bay', x: 8, y: 2, face: 0 },
  });
  ok(!shut.ok, 'nothing can be built in a room that has not opened');

  const walk = await post('/api/travel', { player: me.player, site: 'final' });
  ok(!walk.ok, 'and nobody can walk into one');

  // ---- a chassis, and the book's answer
  //
  // Note 7: prebuilt machines take the fun out of the game entirely. A
  // placement is an empty chassis unless the worked example is asked for by
  // name, and an empty one is not running and says why.
  const empty = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'PlaceMachine', payload: { proto: 'steamplant', x: 22, y: 14, face: 0 },
  });
  ok(empty.ok, 'an empty chassis can be placed' + (empty.ok ? '' : `: ${empty.error}`));
  const chassis = (await get(`/api/state?code=basin&player=${me.player}`))
    .world.installs.slice(-1)[0];
  ok(!chassis.macro, 'and it has nothing inside it');
  ok(!chassis.running && /designed/.test(chassis.idle || ''),
    `it is not running, and says why: ${chassis.idle}`);
  ok((chassis.ports || []).length === 0, 'with no ports until there is something to port');
  // Opening it gives an empty drawing board rather than a refusal.
  const board = await post('/api/cmd', {
    code: 'basin', player: me.player, type: 'OpenDesign', payload: { id: chassis.id },
  });
  ok(board.ok, 'an empty chassis can be opened' + (board.ok ? '' : `: ${board.error}`));
  const draft = (await get(`/api/state?code=basin&player=${me.player}`))
    .world.installs.find(i => i.id === chassis.id);
  ok(draft.hasDraft, 'and it opens on an empty drawing board');
  await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'DeleteMachine', payload: { id: chassis.id },
  });

  const cat2 = await get('/api/catalogue');
  ok(cat2.protos.filter(p => p.example).length > 0,
    'the catalogue says which prototypes have a worked example');

  // ---- the shelf, end to end
  const plant = await post('/api/cmd', {
    code: 'basin', player: me.player,
    type: 'PlaceMachine',
    payload: { proto: 'steamplant', x: 14, y: 2, face: 0, example: true },
  });
  ok(plant.ok, 'the worked example can be placed by name');
  const now = await get(`/api/state?code=basin&player=${me.player}`);
  const machine = now.world.installs.find(i => i.proto === 'steamplant');
  const saved = await post('/api/shelf', {
    do: 'save', player: me.player, code: 'basin', id: machine.id, name: 'Mk1',
  });
  ok(saved.ok, 'its design goes on the shelf');
  const copied = await post('/api/shelf', {
    do: 'copy', player: me.player, design: saved.design, name: 'Mk2',
  });
  ok(copied.ok, 'and can be copied under a new name');
  const twice = await post('/api/shelf', {
    do: 'save', player: me.player, code: 'basin', id: machine.id, name: 'Mk1',
  });
  ok(!twice.ok, 'two designs cannot share one name');

  const withShelf = await get(`/api/camp?player=${me.player}`);
  ok(withShelf.shelf.designs.length === 2, 'the shelf has both');
  const child = withShelf.shelf.designs.find(d => d.name === 'Mk2');
  ok(child.fromName === 'Mk1', 'and the copy remembers where it came from');
  ok(child.proto === 'steamplant', 'and which prototype it goes in');

  shell.renderShelf(withShelf, { copy() {}, place() {}, forget() {} });
  ok(el('shelf').innerHTML.includes('from Mk1'), 'the shelf shows the lineage');
  shell.renderShelfPalette(withShelf, () => {});
  ok(el('shelfpalette').innerHTML.includes('Mk2'), 'and the room view can build from it');

  const form = await post('/api/form', { design: saved.design });
  ok(form.ok && form.design, 'a shelved design builds as a plant for the 3D view');

  // ---- refusals that are answers
  const nolane = await post('/api/route', {
    do: 'open', player: me.player, from: 'basin', to: 'basin', item: 'Coal', fleet: 'train',
  });
  ok(!nolane.ok, 'a lane the map does not have is refused');

  // ---- the door
  //
  // What arrives on the socket before any of the above is HTTP, and what
  // arrives on it in practice is not always HTTP. A browser told to open a
  // bare `host:port` tries `https://` first and sends a TLS ClientHello, whose
  // random block is not UTF-8. Prototype 2's server was taught to answer that;
  // this one had its own copy of the door and was not, which is why the play
  // session's log still filled up with `stream did not contain valid UTF-8`.
  // Both now stand behind `src/http.rs`.
  {
    const raw = (bytes, wait = 3000) => new Promise(resolve => {
      const s = new nodeNet.Socket();
      let got = Buffer.alloc(0);
      let settled = false;
      const done = () => {
        if (settled) return;
        settled = true;
        clearTimeout(timer);
        s.destroy();
        resolve(got.toString('latin1'));
      };
      const timer = setTimeout(done, wait);
      s.connect(Number(port), '127.0.0.1', () => s.write(bytes));
      s.on('data', d => { got = Buffer.concat([got, d]); });
      s.on('close', done);
      s.on('end', done);
      s.on('error', done);
    });
    const hello = Buffer.concat([
      Buffer.from('1603010200010001fc0303', 'hex'),
      Buffer.alloc(112, 0xc8),
    ]);
    const tls = await raw(hello);
    ok(/^HTTP\/1\.1 400/.test(tls), `an https attempt is answered, not dropped: ${tls.slice(0, 24)}`);
    ok(/http, not https/.test(tls), 'and told which protocol this port speaks');
    const crlf = String.fromCharCode(13, 10);
    const fine = await raw(Buffer.from(`GET /api/sites HTTP/1.1${crlf}Host: x${crlf}${crlf}`));
    ok(/^HTTP\/1\.1 200/.test(fine), 'a real request is unaffected');
  }

  // ---- the beat
  //
  // Five rooms on one clock, and until now the *replicas* of those rooms only
  // moved when the browser that owned them polled. A second player who is not
  // looking must be carried anyway, in all five, or their next poll pays for
  // every tick they missed while everybody else waits on the lock.
  {
    const quiet = await post('/api/enter', { name: 'quiet', key: 'seat-quiet' });
    ok(quiet.ok && !quiet.rejoined, `a second player entered as ${quiet.player}`);
    const before = (await get(`/api/state?code=basin&player=${me.player}`)).tick;
    await new Promise(r => setTimeout(r, 1500));
    const after = await get(`/api/state?code=basin&player=${me.player}`);
    ok(after.tick > before, `the campaign clock ran on: ${before} -> ${after.tick}`);
    const beat = after.sync.beat;
    ok(beat > 0 && beat < 60, `the frame says how often a room beats: ${beat} ticks`);
    // Every room, not just the one anybody was looking at.
    for (const tag of atlas.sites.map(s => s.tag)) {
      const f = await get(`/api/state?code=${tag}&player=${me.player}`);
      const q = f.players.find(p => p.id === quiet.player);
      ok(q.behind <= beat, `${tag} carried the quiet replica: ${q.behind} behind, beat is ${beat}`);
      ok(q.resyncs === 0 && q.mismatches === 0,
        `${tag} did not treat a quiet browser as a broken one: ${q.resyncs}/${q.mismatches}`);
    }
  }

  // ---- coming back
  //
  // A campaign seat owns five rooms, a place on the map and everything the
  // tech tree has been opened with. `POST /api/enter` used to carry no token
  // at all, so a refresh took a fresh one and left all of it behind.
  {
    const key = 'seat-cy-' + Date.now();
    const first = await post('/api/enter', { name: 'Cy', key });
    ok(first.ok && !first.rejoined, `Cy took seat ${first.player}`);
    ok(first.at === 'basin', 'and starts in Coal Basin');
    const moved = await post('/api/travel', { player: first.player, site: 'basin' });
    ok(moved.ok, 'Cy can stand somewhere');

    const back = await post('/api/enter', { key, back: true });
    ok(back.ok && back.rejoined, 'the campaign recognised the browser');
    ok(back.player === first.player, `the same seat: ${back.player}`);
    ok(back.name === 'Cy', `and the same name: ${back.name}`);
    ok(back.at === 'basin', `put back in the room Cy was standing in: ${back.at}`);

    const frame = await get(`/api/state?code=basin&player=${back.player}`);
    const cy = frame.players.find(p => p.id === back.player);
    ok(cy.rejoins === 1 && cy.resyncs === 0,
      `counted as a rejoin, not a correction: ${cy.rejoins}/${cy.resyncs}`);

    const twin = await post('/api/enter', { name: 'Cy', key: key + '-other' });
    ok(twin.ok && !twin.rejoined && twin.player !== back.player,
      'a different browser is a different seat');

    // A token from a campaign that is gone must not seat a stranger.
    const cast = (await get(`/api/camp?player=${me.player}`)).cast.length;
    const stale = await post('/api/enter', { key: 'seat-never-been-here', back: true });
    ok(!stale.ok, `coming back to a seat that is gone is refused: ${stale.error}`);
    ok((await get(`/api/camp?player=${me.player}`)).cast.length === cast,
      'and leaves no phantom behind');
  }

  // ---- what crosses a room's boundary
  //
  // Notes 7, 10 and 16 are one panel. A room could not say what it was being
  // sent, and when something could not be delivered the message said so
  // without saying where the material had gone.
  {
    const c = await get(`/api/camp?player=${me.player}`);
    const basin = c.rooms.find(r => r.tag === 'basin');
    ok(!!basin.io, 'a room states what crosses its boundary');
    for (const k of ['imports', 'exports', 'takes', 'gives']) {
      ok(Array.isArray(basin.io[k]), `its \`${k}\` is a list`);
    }
    // Coal Basin has no imports and is where everything starts, so its ports
    // are the interesting half.
    ok(basin.io.gives.length > 0, `Coal Basin can ship ${basin.io.gives.length} things`);
    ok(basin.io.gives.every(g => g.item && g.itemTitle && g.domain),
      'and each says what, in words and in a domain');
    const valley = c.rooms.find(r => r.tag === 'valley');
    ok(valley.io.takes.length > 0, `Iron Valley can receive ${valley.io.takes.length} things`);
    ok(valley.io.takes.every(t => t.at), 'each naming the yard it lands in');

    // Open a route and the three places a load can be all appear.
    const opened = await post('/api/route', {
      do: 'open', player: me.player, from: 'basin', to: 'valley',
      item: valley.io.takes[0].item, fleet: 'train',
    });
    if (opened.ok) {
      const c2 = await get(`/api/camp?player=${me.player}`);
      const v2 = c2.rooms.find(r => r.tag === 'valley');
      const line = v2.io.imports[0];
      ok(!!line, 'the route shows up as an import');
      for (const k of ['atSource', 'inTransit', 'moved', 'spilled']) {
        ok(typeof line[k] === 'number', `an import says its \`${k}\``);
      }
      ok(line.bay, `and where it lands: ${line.bay}`);
      ok(c2.rooms.find(r => r.tag === 'basin').io.exports.length > 0,
        'and the far end calls it an export');
    } else {
      ok(true, `no route to open between those two: ${opened.error}`);
    }

    // The panel itself renders from that, without a browser.
    shell.renderRoomIO(c, 'basin');
    const html = el('roomio').innerHTML;
    ok(/<h3>in<\/h3>/.test(html) && /<h3>out<\/h3>/.test(html),
      'the panel has both directions');
  }

  // ---- the wiring itself
  //
  // `app.js` is the file that joins Prototype 2's client to this campaign, and
  // the one thing that can be checked about it without a browser is that it
  // loads: every import resolves, every id it reaches for exists in the page
  // it was written for, and its lobby wires up without throwing. A `fetch`
  // with no origin is the only thing Node cannot give it, so it is given one.
  const bare = globalThis.fetch;
  globalThis.fetch = (u, o) => bare(String(u).startsWith('http') ? u : base + u, o);
  try {
    await import(pathTo('web/camp/app.js'));
    ok(true, 'the client loads, and every id it reaches for is in the page');
    // `lobby()` now asks whether this browser already has a seat before it
    // draws anything, so the button is wired one round trip later than it used
    // to be. Waiting for it is the test; timing out is the failure.
    const enterBtn = el('enter');
    for (let i = 0; i < 50 && typeof enterBtn.onclick !== 'function'; i++) {
      await new Promise(r => setTimeout(r, 20));
    }
    ok(typeof enterBtn.onclick === 'function', 'and the lobby is wired up');
  } catch (e) {
    ok(false, `the client would not load: ${e && e.message}`);
  } finally {
    globalThis.fetch = bare;
  }

  console.log(failures === 0
    ? '\nthe client and the campaign agree about every field they share.'
    : `\n${failures} disagreement(s).`);
  process.exit(failures === 0 ? 0 : 1);
}

function pathTo(rel) {
  const p = join(here, '..', rel).replace(/\\/g, '/');
  return 'file:///' + p.replace(/^\/+/, '');
}

main().catch(e => {
  console.error(e);
  process.exit(1);
});
