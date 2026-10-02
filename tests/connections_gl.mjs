// Exercise the real testbed UI and WebGL. Start machine serve plus headless
// Chromium with --remote-debugging-port=9228, then:
// node tests/connections_gl.mjs 8799 9228 [screenshot.png]
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';
const port = process.argv[2] || '8799', debug = process.argv[3] || '9228';
const tabs = await fetch(`http://127.0.0.1:${debug}/json/list`).then(r => r.json());
const tab = tabs.find(t => t.type === 'page'); assert(tab, 'Chromium needs a page');
const ws = new WebSocket(tab.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let id = 0; const pending = new Map(), errors = [];
ws.onmessage = e => {
  const message = JSON.parse(e.data);
  if (message.id) { const p = pending.get(message.id); pending.delete(message.id); message.error ? p.reject(message.error) : p.resolve(message.result); }
  if (message.method === 'Runtime.exceptionThrown') errors.push(message.params.exceptionDetails);
};
function send(method, params = {}) {
  return new Promise((resolve, reject) => { const key = ++id; pending.set(key, { resolve, reject }); ws.send(JSON.stringify({ id: key, method, params })); });
}
async function evaluate(expression) {
  const r = await send('Runtime.evaluate', { expression, awaitPromise: true, returnByValue: true });
  assert(!r.exceptionDetails, JSON.stringify(r.exceptionDetails)); return r.result.value;
}
try {
  await send('Runtime.enable'); await send('Page.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 1600, height: 1100, deviceScaleFactor: 1, mobile: false });
  await send('Page.navigate', { url: `http://127.0.0.1:${port}/connections` });
  for (let i = 0; i < 100; i++) {
    if (await evaluate(`!!document.querySelector('#stats')?.textContent.includes('hash')`)) break;
    await new Promise(r => setTimeout(r, 100));
  }
  const result = await evaluate(`(async () => {
    const renderer = await import('/form.js');
    const { CASES } = await import('/connections-cases.js');
    const $ = s => document.querySelector(s);
    const wait = async fn => { for (let i=0; i<200; i++) { if (fn()) return; await new Promise(r => setTimeout(r, 25)); } throw new Error('UI wait timed out: ' + $('#stats').textContent + ' / ' + $('#error').textContent); };
    const change = (id, value) => { $(id).value=value; $(id).dispatchEvent(new Event('change')); };
    const check = (condition, text) => { if (!condition) throw new Error(text); };
    const frames = () => new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
    const gl = $('#gl').getContext('webgl2'); check(gl, 'WebGL2');
    check($('#cases').children.length === CASES.length, 'All presets available');
    for (let i=0; i<CASES.length; i++) {
      $('#cases').children[i].click();
      await wait(() => $('#stats').textContent.includes('hash'));
      check($('#error').hidden, CASES[i].id + ': build error');
      for (const mode of ['pipes', 'equipment', 'all']) {
        change('#geometry', mode);
        for (const lod of ['0', '1', '2']) { change('#lod', lod); await frames(); check(gl.getError() === 0, CASES[i].id + ': GL error'); }
      }
      if ($('#routes').children.length) { $('#routes').children[0].click(); await frames(); $('#routes').children[0].click(); }
    }
    $('#cases').children[0].click(); await wait(() => $('#stats').textContent.includes('hash'));
    const original = $('#stats').textContent;
    $('#x').value='1'; $('#apply-position').click(); await wait(() => $('#stats').textContent.includes('hash'));
    check($('#source').value.includes('at 1,0,0'), 'Position control updates source');
    check($('#stats').textContent !== original, 'Position regenerates geometry');
    $('#source').value='invalid machine'; $('#rebuild').click(); await wait(() => !$('#error').hidden);
    check(renderer.drawn().instances === 0, 'Failed build clears stale geometry');
    check($('#export-report').disabled, 'Failed build cannot export stale diagnostics');
    $('#reset').click(); await wait(() => $('#stats').textContent.includes('hash'));
    $('#run-all').click(); await wait(() => !$('#run-all').disabled);
    check($('#suite').textContent.includes('0 build errors'), 'Batch check succeeds');
    // Leave the visually interesting branch preset up for a screenshot.
    $('#cases').children[CASES.findIndex(c => c.id==='shared-out')].click();
    await wait(() => $('#stats').textContent.includes('hash'));
    change('#geometry','equipment'); change('#lod','0');
    for (const id of ['#paths','#ports']) { $(id).checked=true; $(id).dispatchEvent(new Event('change')); }
    for (const name of ['iso','top','front','side']) { $('[data-camera="'+name+'"]').click(); await frames(); check(gl.getError()===0, 'Camera ' + name); }
    $('[data-camera="iso"]').click(); await frames();
    return { presets: CASES.length, summary: $('#suite').textContent, drawn: renderer.drawn(), canvas: [$('#gl').width, $('#gl').height] };
  })()`);
  assert.equal(errors.length, 0, JSON.stringify(errors));
  console.log('Connection testbed UI, real WebGL, modes, LODs, overlays, edits and failures passed:', result);
  if (process.argv[4]) {
    const shot = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(process.argv[4], Buffer.from(shot.data, 'base64'));
  }
} finally { ws.close(); }
