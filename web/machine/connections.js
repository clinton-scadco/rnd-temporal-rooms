import * as plant from './form.js';
import { CASES, inspectionScene } from './connections-cases.js';

const $ = s => document.querySelector(s);
const faces = ['east', 'south', 'west', 'north'];
let current = CASES[0], design = null, scene = null, selected = null;
let generation = 0, builtSource = '', caseButtons = new Map();
const results = new Map();

async function api(path, body) {
  const response = await fetch(path, { method: 'POST', headers: { 'content-type': 'application/json' }, body });
  if (!response.ok) throw new Error(`Request failed (${response.status})`);
  const json = await response.json();
  if (!json.ok) throw new Error(json.error || 'The machine could not be built');
  return json;
}
function settings() {
  return new URLSearchParams({ style: $('#style').value, grade: $('#grade').value, seed: $('#seed').value, diagnostics: '1' });
}
async function build(source, query = settings()) {
  const opened = await api('/api/open', source);
  const formed = await api(`/api/form?${query}`, JSON.stringify({ design: opened.design }));
  return { design: opened.design, scene: formed };
}
function error(message) {
  $('#error').textContent = message || '';
  $('#error').hidden = !message;
}
function resultFor(s) {
  const counts = { clean: 0, tight: 0, lost: 0 };
  for (const r of s.runs) counts[r.tier]++;
  return counts;
}
function markResult(id, value) {
  results.set(id, value);
  const badge = caseButtons.get(id).querySelector('.result');
  badge.className = `result ${value.error ? 'failed' : value.lost ? 'lost' : value.tight ? 'tight' : 'clean'}`;
  badge.textContent = value.error ? 'error' : value.lost ? `${value.lost} lost` : value.tight ? `${value.tight} tight` : 'clean';
}
function clearScene() {
  scene = null; design = null; selected = null;
  // Clear the previous geometry immediately so a failed edit cannot be
  // mistaken for a successful render of the new document.
  plant.clear();
  plant.mark({ wires: [], targets: [], ghost: null, link: null });
  $('#routes').replaceChildren(); $('#issues').replaceChildren(); $('#stats').textContent = '';
  $('#unit').replaceChildren();
  $('#export-report').disabled = true;
  $('#apply-position').disabled = true;
}
async function rebuild(refit = false) {
  const token = ++generation, preset = current, source = $('#source').value;
  clearScene(); error(null);
  $('#stats').textContent = 'Building connections…';
  try {
    const built = await build(source);
    if (token !== generation) return;
    design = built.design; scene = built.scene; builtSource = source;
    if (source === preset.source) markResult(preset.id, resultFor(scene));
    render(refit); report(); units();
    $('#export-report').disabled = false; $('#apply-position').disabled = false;
  } catch (e) {
    if (token !== generation) return;
    error(e.message); $('#stats').textContent = 'Build failed';
    if (source === preset.source) markResult(preset.id, { error: e.message });
  }
}
function choose(id) {
  current = CASES.find(c => c.id === id) || CASES[0];
  history.replaceState(null, '', `#${current.id}`);
  for (const [id, button] of caseButtons) button.setAttribute('aria-current', String(id === current.id));
  $('#title').textContent = current.title; $('#note').textContent = current.note;
  $('#source').value = current.source;
  plant.cameraPreset('iso');
  return rebuild(true);
}
function render(refit = false) {
  if (!scene) return;
  plant.view.lod = Number($('#lod').value);
  plant.view.boxes = $('#boxes').checked;
  plant.show(inspectionScene(scene, $('#geometry').value), refit);
  if (refit) plant.fit({ zoom: .6 });
  plant.pick(selected);
  const paths = scene.diagnostics.paths.filter(p => !selected || p.name === selected);
  const wires = $('#paths').checked ? paths.flatMap(p => p.points.slice(1).map((v, i) => [p.points[i], v, [0.40, 0.72, 1, 1]])) : [];
  const sockets = scene.diagnostics.sockets.filter(p => p.connected && (!selected || selected.split(' -> ').includes(p.name)));
  const targets = [];
  if ($('#ports').checked) for (const p of sockets) {
    wires.push([p.at, p.at.map((v, i) => v + p.out[i] * p.stub), [1, 0.73, 0.26, 1]]);
    targets.push({ lo: p.at.map(v => v - .09), hi: p.at.map(v => v + .09) });
  }
  plant.mark({ wires, targets }); plant.invalidate();
}
function report() {
  const c = resultFor(scene);
  $('#stats').textContent = `${scene.runs.length} connections · ${c.clean} clean · ${c.tight} tight · ${c.lost} lost · ${scene.stats.pieces} pieces · hash ${scene.hash}`;
  $('#routes').replaceChildren();
  for (const r of scene.runs) {
    const button = document.createElement('button'); button.className = 'route';
    const name = document.createElement('span'); name.textContent = r.name;
    const detail = document.createElement('small'); detail.className = r.tier;
    detail.textContent = `${r.domain} · ${r.tier} · ${r.bends} bends · ${r.metres} m · Ø ${r.bore} mm`;
    button.append(name, detail);
    button.addEventListener('click', () => {
      selected = selected === r.name ? null : r.name;
      for (const b of $('#routes').children) b.classList.toggle('active', b === button && !!selected);
      render();
    });
    $('#routes').append(button);
  }
  $('#issues').replaceChildren();
  if (!scene.issues.length) $('#issues').textContent = 'No spatial issues reported.';
  for (const issue of scene.issues) {
    const p = document.createElement('p'); p.className = issue.bad ? 'bad' : '';
    p.textContent = `${issue.of} · ${issue.rule}: ${issue.what}`; $('#issues').append(p);
  }
}
function units() {
  $('#unit').replaceChildren(...design.units.map(u => {
    const option = document.createElement('option'); option.value = u.name; option.textContent = `${u.name} (${u.kind})`; return option;
  }));
  unitFields();
}
function unitFields() {
  const u = design?.units.find(u => u.name === $('#unit').value);
  if (!u) return;
  for (const axis of ['x', 'y', 'z']) $(`#${axis}`).value = u[axis] || 0;
  $('#face').value = u.face == null ? '' : faces[u.face];
}
function applyPosition() {
  if (!design) return;
  if ($('#source').value !== builtSource) { error('Rebuild the edited document before applying position controls.'); return; }
  const coords = ['x', 'y', 'z'].map(axis => Number($(`#${axis}`).value));
  if (!coords.every(Number.isSafeInteger) || coords[2] < 0) { error('Use whole tile coordinates and a non-negative level.'); return; }
  const u = design.units.find(u => u.name === $('#unit').value);
  const pattern = new RegExp(`^(\\s*${u.kind}\\s+${u.name}\\s+.*?\\bat\\s+)\\S+(.*)$`, 'm');
  const face = $('#face').value;
  $('#source').value = $('#source').value.replace(pattern, (_, before, after) => {
    const tail = after.replace(/\s+face\s+\S+/, '');
    const comment = tail.indexOf('#');
    const rotation = face ? ` face ${face}` : '';
    return `${before}${coords.join(',')}${comment < 0 ? tail + rotation : tail.slice(0, comment) + rotation + ' ' + tail.slice(comment)}`;
  });
  rebuild();
}
function download(name, content, type) {
  const url = URL.createObjectURL(new Blob([content], { type }));
  const a = document.createElement('a'); a.href = url; a.download = name; a.click();
  setTimeout(() => URL.revokeObjectURL(url), 1000);
}
async function runAll() {
  const button = $('#run-all'); button.disabled = true;
  const query = settings(); let failed = 0, lost = 0, tight = 0;
  try {
    for (const [i, preset] of CASES.entries()) {
      $('#suite').textContent = `Checking ${i + 1}/${CASES.length}…`;
      try {
        const { scene } = await build(preset.source, query), counts = resultFor(scene);
        markResult(preset.id, counts); lost += counts.lost; tight += counts.tight;
      } catch (e) { markResult(preset.id, { error: e.message }); failed++; }
    }
    $('#suite').textContent = `${CASES.length} checked · ${failed} build errors · ${tight} tight / ${lost} lost routes`;
  } finally { button.disabled = false; }
}
async function main() {
  $('#count').textContent = CASES.length;
  for (const preset of CASES) {
    const button = document.createElement('button');
    const title = document.createElement('span'); title.textContent = preset.title;
    const domain = document.createElement('small'); domain.textContent = preset.domain;
    const badge = document.createElement('span'); badge.className = 'result';
    button.append(title, domain, badge); button.addEventListener('click', () => choose(preset.id));
    caseButtons.set(preset.id, button); $('#cases').append(button);
  }
  if (!await plant.initForm($('#gl'))) throw new Error('WebGL2 is required to render the connection testbed.');
  $('#rebuild').addEventListener('click', () => rebuild());
  $('#reset').addEventListener('click', () => choose(current.id));
  $('#source').addEventListener('input', () => { $('#export-report').disabled = true; });
  $('#unit').addEventListener('change', unitFields);
  $('#apply-position').addEventListener('click', applyPosition);
  $('#run-all').addEventListener('click', runAll);
  $('#previous').addEventListener('click', () => choose(CASES[(CASES.indexOf(current) + CASES.length - 1) % CASES.length].id));
  $('#next').addEventListener('click', () => choose(CASES[(CASES.indexOf(current) + 1) % CASES.length].id));
  for (const id of ['grade', 'seed', 'style']) $(`#${id}`).addEventListener('change', () => rebuild());
  for (const id of ['geometry', 'lod', 'paths', 'ports', 'boxes']) $(`#${id}`).addEventListener('change', () => render());
  for (const b of document.querySelectorAll('[data-camera]')) b.addEventListener('click', () => plant.cameraPreset(b.dataset.camera));
  $('#fit').addEventListener('click', () => plant.fit({ zoom: .6 }));
  $('#export-source').addEventListener('click', () => download(`connection-${current.id}.machine`, $('#source').value, 'text/plain'));
  $('#export-report').addEventListener('click', () => {
    if (!scene || $('#source').value !== builtSource) return;
    download(`connection-${current.id}.json`, JSON.stringify({ preset: current.id, source: builtSource, settings: Object.fromEntries(settings()), scene }, null, 2), 'application/json');
  });
  await choose(location.hash.slice(1));
}
main().catch(e => error(e.message));
