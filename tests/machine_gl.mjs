// Real WebGL smoke check. Run machine serve and a headless Chromium with
// --remote-debugging-port=9228, then: node tests/machine_gl.mjs 8799 9228
// The DOM tests cannot catch shader compilation or framebuffer failures.
import assert from 'node:assert/strict';
import { writeFileSync } from 'node:fs';

const port = process.argv[2] || '8799';
const debug = process.argv[3] || '9228';
const designName = process.argv[5] || '10-refinery.machine';
const tabs = await fetch(`http://127.0.0.1:${debug}/json/list`).then(r => r.json());
const tab = tabs.find(t => t.type === 'page');
assert(tab, 'Chromium needs a page target');
const ws = new WebSocket(tab.webSocketDebuggerUrl);
await new Promise((resolve, reject) => { ws.onopen = resolve; ws.onerror = reject; });
let id = 0;
const pending = new Map();
const errors = [];
ws.onmessage = e => {
  const msg = JSON.parse(e.data);
  if (msg.id) { const p = pending.get(msg.id); pending.delete(msg.id); msg.error ? p.reject(msg.error) : p.resolve(msg.result); }
  if (msg.method === 'Runtime.exceptionThrown') errors.push(msg.params.exceptionDetails);
};
function send(method, params = {}) {
  return new Promise((resolve, reject) => {
    const key = ++id;
    pending.set(key, { resolve, reject });
    ws.send(JSON.stringify({ id: key, method, params }));
  });
}
try {
  await send('Runtime.enable');
  await send('Page.enable');
  await send('Emulation.setDeviceMetricsOverride', { width: 1400, height: 1000, deviceScaleFactor: 1, mobile: false });
  // A same-origin blank surface avoids running the designer's UI alongside
  // the renderer being checked. Navigation to a source asset is read-only.
  await send('Page.navigate', { url: `http://127.0.0.1:${port}/form.js` });
  for (let attempt = 0; attempt < 50; attempt++) {
    const r = await send('Runtime.evaluate', { expression: 'document.readyState' });
    if (r.result.value === 'complete') break;
    await new Promise(r => setTimeout(r, 100));
  }
  const result = await send('Runtime.evaluate', { awaitPromise: true, returnByValue: true, expression: `
    (async () => {
      document.body.innerHTML = '<canvas id="qa" style="width:1400px;height:1000px"></canvas>';
      document.body.style.margin = '0';
      const renderer = await import('/form.js');
      const canvas = document.getElementById('qa');
      if (!await renderer.initForm(canvas)) throw new Error('WebGL2 unavailable');
      const opened = await fetch('/api/design?name=' + encodeURIComponent(${JSON.stringify(designName)})).then(r => r.json());
      const built = await fetch('/api/form?style=yard&seed=0', {
        method:'POST', headers:{'content-type':'application/json'}, body:JSON.stringify({design:opened.design})
      }).then(r => r.json());
      if (!built.ok) throw new Error(JSON.stringify(built));
      if (!built.batches.some(b => b.mesh === 'volute')) throw new Error('Pump casting missing');
      const pump = built.units.find(u => u.kind === 'pump' || u.kind === 'mechpump');
      if (!pump) throw new Error('No pump to exercise picking');
      renderer.view.boxes = false;
      renderer.show(built, true);
      const gl = canvas.getContext('webgl2');
      const levels = [];
      for (const lod of [0, 1, 2, 3, 0]) {
        renderer.view.lod = lod;
        renderer.pick(lod === 0 ? pump.name : null);
        await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
        const error = gl.getError();
        if (error) throw new Error('GL error at LOD ' + lod + ': ' + error);
        levels.push(lod);
      }
      renderer.pick(null);
      await new Promise(r => requestAnimationFrame(() => requestAnimationFrame(r)));
      // Read within a RAF because the default buffer is not preserved.
      const colours = await new Promise(resolve => requestAnimationFrame(() => {
        renderer.pick(null);
        requestAnimationFrame(() => {
          const px = new Uint8Array(canvas.width * canvas.height * 4);
          gl.readPixels(0, 0, canvas.width, canvas.height, gl.RGBA, gl.UNSIGNED_BYTE, px);
          const set = new Set();
          for (let i = 0; i < px.length; i += 64) set.add(px.slice(i, i + 3).join(','));
          resolve(set.size);
        });
      }));
      return { levels, colours, pieces:built.stats.pieces };
    })()
  ` });
  assert(!result.exceptionDetails, JSON.stringify(result.exceptionDetails));
  assert.equal(errors.length, 0, JSON.stringify(errors));
  assert(result.result.value.colours > 50, 'the canvas must contain shaded geometry');
  console.log('WebGL shaders, shadow framebuffer, picking and all LODs passed:', result.result.value);
  if (process.argv[4]) {
    const shot = await send('Page.captureScreenshot', { format: 'png' });
    writeFileSync(process.argv[4], Buffer.from(shot.data, 'base64'));
  }
} finally { ws.close(); }
