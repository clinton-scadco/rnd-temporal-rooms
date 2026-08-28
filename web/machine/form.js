// Experiment 08: the plant, drawn.
//
// The renderer is the shape of the claim it is testing. It holds twenty-five
// meshes, uploaded once, and one instance buffer per (mesh, material) pair.
// A plant of four thousand pieces is thirty-odd draw calls, and the number of
// draw calls does not grow when the plant does -- only the instance counts do.
//
// It also does the level-of-detail trick the way the note asks for it. Every
// batch's instances arrive sorted so that the ones that survive furthest are
// first, together with three counts; drawing "medium" is drawing a prefix of
// exactly the same buffer. Nothing is re-uploaded, nothing is re-sorted, and
// the simulation representation is of course identical at every level, because
// the simulator has never heard of any of this.
//
// The one thing worth being careful about is the frame: `basis()` below is the
// same construction as `frame_of` in Rust and `axes` in the .obj writer. If
// those three disagree, elbows point into space.
//
// Experiment 10 made this window an *editor* rather than a viewer. Clicking a
// machine selects it, dragging it slides it across the storey it stands on,
// and two keys lift it and turn it. None of that touches the geometry: the
// mouse produces a change to the document, the document is posted, and a whole
// new plant comes back. That is slower than moving a mesh and it is the only
// version of the feature worth having -- what the player is manipulating is
// the machine, and the pipework regenerating around the thing they moved is
// the entire point of the experiment.
//
// Picking is done on the CPU, against the per-component boxes the server sends
// with the scene, rather than with a colour-buffer readback. A plant is a few
// dozen boxes and a ray test is a few dozen comparisons; a readback is a
// pipeline stall.

const VERT = `#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
layout(location=1) in vec3 aNrm;
layout(location=2) in vec3 iAt;
layout(location=3) in vec3 iDir;
layout(location=4) in vec3 iSize;
layout(location=5) in float iSpin;
layout(location=6) in float iTint;
layout(location=7) in float iOf;

uniform mat4 uViewProj;
uniform float uPick;

out vec3 vNrm;
out vec3 vWorld;
out float vTint;
out float vPick;

mat3 basis(vec3 d, float spin) {
  vec3 up = normalize(d);
  vec3 rf = abs(up.y) > 0.99 ? vec3(0.0, 0.0, 1.0) : vec3(0.0, 1.0, 0.0);
  vec3 right = normalize(cross(up, rf));
  vec3 fwd = cross(right, up);
  if (spin > 2.5)      { vec3 t = right; right = -fwd; fwd = t; }
  else if (spin > 1.5) { right = -right; fwd = -fwd; }
  else if (spin > 0.5) { vec3 t = right; right = fwd; fwd = -t; }
  return mat3(right, up, fwd);
}

void main() {
  mat3 m = basis(iDir, iSpin);
  vec3 local = aPos * iSize;
  vec3 world = iAt + m * local;
  // Non-uniform scale, so the normal takes the reciprocal.
  vec3 n = m * (aNrm / max(iSize, vec3(1e-4)));
  vNrm = normalize(n);
  vWorld = world;
  vTint = iTint;
  vPick = (uPick >= 0.0 && abs(iOf - uPick) < 0.5) ? 1.0 : 0.0;
  gl_Position = uViewProj * vec4(world, 1.0);
}`;

const FRAG = `#version 300 es
precision highp float;
in vec3 vNrm;
in vec3 vWorld;
in float vTint;
in float vPick;

uniform vec3 uColour;
uniform float uRough;
uniform float uMetal;
uniform vec3 uEye;

out vec4 outColour;

void main() {
  vec3 n = normalize(vNrm);
  vec3 sun = normalize(vec3(0.45, 0.82, 0.35));
  float lam = max(dot(n, sun), 0.0);
  // A sky that is brighter than the ground, which is most of what makes an
  // untextured plant read as an object rather than a diagram.
  float sky = 0.5 + 0.5 * n.y;
  vec3 amb = mix(vec3(0.20, 0.21, 0.24), vec3(0.46, 0.51, 0.58), sky);

  vec3 base = uColour * (1.0 - vTint * 0.035);
  vec3 col = base * (amb + lam * 0.85);

  vec3 view = normalize(uEye - vWorld);
  vec3 h = normalize(view + sun);
  float spec = pow(max(dot(n, h), 0.0), mix(90.0, 8.0, uRough)) * (0.04 + 0.5 * uMetal);
  col += vec3(spec) * lam;

  // Depth haze: a plant seen across a yard.
  float d = length(uEye - vWorld);
  col = mix(col, vec3(0.72, 0.76, 0.80), clamp((d - 40.0) / 220.0, 0.0, 0.55));

  col = mix(col, vec3(1.0, 0.86, 0.35), vPick * 0.45);
  outColour = vec4(pow(col, vec3(0.4545)), 1.0);
}`;

// The boxes drawn over the plant when the player is placing things: a
// component's solid, the room it needs to be serviced in, and the verdict on
// it. Lines rather than faces, because the whole purpose is to see the plant
// *through* them.
const LINE_VERT = `#version 300 es
precision highp float;
layout(location=0) in vec3 aPos;
uniform mat4 uViewProj;
uniform vec3 uLo;
uniform vec3 uHi;
void main() {
  vec3 p = mix(uLo, uHi, aPos);
  gl_Position = uViewProj * vec4(p, 1.0);
}`;

const LINE_FRAG = `#version 300 es
precision highp float;
uniform vec4 uColour;
out vec4 outColour;
void main() { outColour = uColour; }`;

/// The unit cube's twelve edges, as pairs of corners in 0..1.
const CUBE = (() => {
  const c = [];
  for (let i = 0; i < 8; i++) {
    for (const bit of [1, 2, 4]) {
      const j = i ^ bit;
      if (j > i) c.push(i & 1, (i >> 1) & 1, (i >> 2) & 1, j & 1, (j >> 1) & 1, (j >> 2) & 1);
    }
  }
  return new Float32Array(c);
})();

const VERDICT = {
  clear: [0.30, 0.85, 0.45, 0.75],
  watch: [0.98, 0.78, 0.22, 0.85],
  bad: [0.98, 0.32, 0.30, 0.95],
};

export const view = {
  lod: 0,
  style: 'works',
  // Experiment 09: which of the four looks to ask for. The default is the one
  // the designer ships; the other three exist so that the comparison can be
  // made in the same window rather than in four screenshots.
  grade: 'full',
  seed: 0,
  pick: -1,
  stats: null,
  shell: '',
  hash: '',
  // Experiment 10: whether the placement overlay is on, and what the last
  // build thought of the plant.
  boxes: true,
  units: [],
  issues: [],
  runs: [],
  levels: 1,
};

let gl = null;
let prog = null;
let uni = {};
let lineProg = null;
let lineUni = {};
let lineVao = null;
let kit = null;      // { meshes: {tag: {vao-parts}}, mats: {tag: {...}} }
let scene = null;    // { batches: [...], bounds }
let canvas = null;
let cam = { yaw: 0.7, pitch: 0.5, dist: 60, at: [0, 4, 0] };
let need = true;
let names = [];
// Experiment 10: what the pointer is allowed to do to the document, handed in
// by `app.js` so that this file still knows nothing about the document.
let edit = null;   // { onPick, onMove, onLift, onTurn, tile }
let held = null;   // the component under the pointer, mid-drag

export function ready() { return !!gl; }

/// What the pointer may do to the document. `app.js` hands this in; this file
/// still has no idea what a component is called or how one is moved.
export function authoring(hooks) {
  edit = hooks;
}

export async function initForm(el) {
  canvas = el;
  gl = canvas.getContext('webgl2', { antialias: true, alpha: false });
  if (!gl) return false;

  prog = link(VERT, FRAG);
  for (const n of ['uViewProj', 'uColour', 'uRough', 'uMetal', 'uEye', 'uPick']) {
    uni[n] = gl.getUniformLocation(prog, n);
  }
  lineProg = link(LINE_VERT, LINE_FRAG);
  for (const n of ['uViewProj', 'uColour', 'uLo', 'uHi']) {
    lineUni[n] = gl.getUniformLocation(lineProg, n);
  }
  lineVao = gl.createVertexArray();
  gl.bindVertexArray(lineVao);
  const lb = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, lb);
  gl.bufferData(gl.ARRAY_BUFFER, CUBE, gl.STATIC_DRAW);
  gl.enableVertexAttribArray(0);
  gl.vertexAttribPointer(0, 3, gl.FLOAT, false, 0, 0);
  gl.bindVertexArray(null);

  gl.enable(gl.DEPTH_TEST);
  gl.enable(gl.CULL_FACE);
  gl.cullFace(gl.BACK);
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.SRC_ALPHA, gl.ONE_MINUS_SRC_ALPHA);

  const res = await fetch('/api/kit').then(r => r.json());
  kit = { meshes: {}, mats: {} };
  for (const m of res.meshes) kit.meshes[m.tag] = upload(m);
  for (const m of res.mats) kit.mats[m.tag] = m;

  drag();
  requestAnimationFrame(frame);
  return true;
}

function link(vs, fs) {
  const c = (type, src) => {
    const s = gl.createShader(type);
    gl.shaderSource(s, src);
    gl.compileShader(s);
    if (!gl.getShaderParameter(s, gl.COMPILE_STATUS)) throw new Error(gl.getShaderInfoLog(s));
    return s;
  };
  const p = gl.createProgram();
  gl.attachShader(p, c(gl.VERTEX_SHADER, vs));
  gl.attachShader(p, c(gl.FRAGMENT_SHADER, fs));
  gl.linkProgram(p);
  if (!gl.getProgramParameter(p, gl.LINK_STATUS)) throw new Error(gl.getProgramInfoLog(p));
  return p;
}

/// One canonical mesh, uploaded once and instanced for the rest of time.
function upload(m) {
  const pos = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, pos);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(m.pos), gl.STATIC_DRAW);
  const nrm = gl.createBuffer();
  gl.bindBuffer(gl.ARRAY_BUFFER, nrm);
  gl.bufferData(gl.ARRAY_BUFFER, new Float32Array(m.nrm), gl.STATIC_DRAW);
  const idx = gl.createBuffer();
  gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, idx);
  gl.bufferData(gl.ELEMENT_ARRAY_BUFFER, new Uint32Array(m.idx), gl.STATIC_DRAW);
  return { pos, nrm, idx, count: m.idx.length, tris: m.tris };
}

/// A scene, as batches. Each one gets a VAO whose per-vertex attributes point
/// at the shared mesh and whose per-instance attributes point at its own
/// buffer, which is the whole of the GPU-side story.
export function show(json, refit) {
  if (!gl || !json || !json.batches) return;
  free();
  scene = { batches: [], bounds: json.bounds, proxy: json.proxy };
  view.stats = json.stats;
  view.shell = json.shell;
  view.hash = json.hash;
  // Experiment 10: everything the placement overlay draws and the inspector
  // reads, worked out by the same pass that built the plant so that the two
  // can never disagree about what is wrong with it.
  view.units = json.units || [];
  view.issues = json.issues || [];
  view.runs = json.runs || [];
  view.levels = json.levels || 1;
  const paint = json.paint || [110, 120, 130];

  // Pieces arrive owned by a *thing* -- a component, a run, the steel under
  // one of them. Collapse those to owning names, so that selecting a
  // generator lights up its plinth and its shaft as well as its body: that is
  // the whole point of the pieces knowing where they came from.
  names = [...new Set(json.owners.map(o => o.name))];
  const byName = json.owners.map(o => names.indexOf(o.name));

  for (const b of json.batches) {
    const mesh = kit.meshes[b.mesh];
    if (!mesh) continue;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    bind(mesh.pos, 0, 3);
    bind(mesh.nrm, 1, 3);
    const data = new Float32Array(b.inst);
    for (let i = 11; i < data.length; i += 12) data[i] = byName[data[i]] ?? -1;
    const inst = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, inst);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
    const S = 12 * 4;
    attrib(2, 3, S, 0);
    attrib(3, 3, S, 12);
    attrib(4, 3, S, 24);
    attrib(5, 1, S, 36);
    attrib(6, 1, S, 40);
    attrib(7, 1, S, 44);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, mesh.idx);
    gl.bindVertexArray(null);

    const mat = kit.mats[b.mat] || { colour: [128, 128, 128], rough: 60, metal: false };
    const colour = b.mat === 'paint' ? paint : mat.colour;
    scene.batches.push({
      vao, inst, count: mesh.count, keep: b.keep, n: b.n,
      colour: colour.map(v => Math.pow(v / 255, 2.2)),
      rough: mat.rough / 100, metal: mat.metal ? 1 : 0,
    });
  }
  // An edit moves the plant, not the viewer: the camera is only reset when a
  // different design is opened.
  if (refit) fit();
  need = true;
}

/// Light up everything belonging to one named component or connection.
export function pick(name) {
  view.pick = name ? names.indexOf(name) : -1;
  need = true;
}

function bind(buf, loc, size) {
  gl.bindBuffer(gl.ARRAY_BUFFER, buf);
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, size, gl.FLOAT, false, 0, 0);
}

function attrib(loc, size, stride, offset) {
  gl.enableVertexAttribArray(loc);
  gl.vertexAttribPointer(loc, size, gl.FLOAT, false, stride, offset);
  gl.vertexAttribDivisor(loc, 1);
}

function free() {
  if (!scene) return;
  for (const b of scene.batches) {
    gl.deleteVertexArray(b.vao);
    gl.deleteBuffer(b.inst);
  }
  if (scene.proxyBatch) gl.deleteVertexArray(scene.proxyBatch.vao);
  scene = null;
}

export function fit() {
  if (!scene || !scene.bounds) return;
  const { lo, hi } = scene.bounds;
  cam.at = [(lo[0] + hi[0]) / 2, (lo[1] + hi[1]) / 2, (lo[2] + hi[2]) / 2];
  const span = Math.max(hi[0] - lo[0], hi[2] - lo[2], hi[1] - lo[1]);
  cam.dist = span * 1.4 + 12;
  need = true;
}

export function invalidate() { need = true; }

// ------------------------------------------------------------------ camera

function drag() {
  let down = null;
  canvas.addEventListener('pointerdown', e => {
    down = { x: e.clientX, y: e.clientY, b: e.button, ox: e.clientX, oy: e.clientY };
    canvas.setPointerCapture(e.pointerId);
    // Left button on a machine picks it up. Anything else is the camera, so
    // the orbit controls this window has always had are untouched.
    held = null;
    if (edit && e.button === 0 && !e.shiftKey) {
      const hit = under(e);
      if (hit) {
        edit.onPick(hit.name);
        // Prototype 2 hands in no `onMove`, which is how a window built for
        // dragging becomes a window where nothing moves: a committed
        // component is placed or deleted, and never slid.
        if (edit.onMove) held = { name: hit.name, from: [hit.x, hit.y, hit.z], moved: false };
      } else if (edit.onGround) {
        // Empty floor, at whichever storey the caller is working on. This is
        // the other half of place-and-delete: the pointer names a tile, the
        // caller turns it into a command.
        const at = onPlane(e, edit.level ? edit.level() : 0);
        const t = at && edit.tile(at);
        if (t) edit.onGround(t.x, t.y);
      }
    }
  });
  canvas.addEventListener('pointerup', e => {
    down = null;
    held = null;
    canvas.releasePointerCapture(e.pointerId);
  });
  canvas.addEventListener('pointermove', e => {
    if (!down) return;
    // Dragging a held machine slides it across the storey it stands on. The
    // ground plane of *its own level*, not of the yard: a component six metres
    // up follows the pointer at six metres up, which is the difference between
    // moving something and dropping it.
    if (held) {
      if (!edit.onMove) return;
      const at = onPlane(e, held.from[2]);
      if (at) {
        const t = edit.tile(at);
        if (t && edit.onMove(held.name, t.x, t.y)) held.moved = true;
      }
      return;
    }
    const dx = e.clientX - down.x, dy = e.clientY - down.y;
    down = { x: e.clientX, y: e.clientY, b: down.b, ox: down.ox, oy: down.oy };
    if (down.b === 2 || e.shiftKey) {
      // On the ground plane, screen-right is (cos yaw, -sin yaw) and
      // up-the-screen is -(sin yaw, cos yaw): the same pair the eye is built
      // from below. Transposing them rotates the pan by twice the yaw, which
      // at any yaw but zero reads as the two drag axes having been swapped.
      const k = cam.dist / 900;
      const s = Math.sin(cam.yaw), c = Math.cos(cam.yaw);
      cam.at[0] -= (dx * c + dy * s) * k;
      cam.at[2] -= (dy * c - dx * s) * k;
    } else {
      // A quarter of a degree per pixel. The first number here was three
      // times this, which put the camera on the other side of the plant
      // before the mouse had crossed it.
      cam.yaw -= dx * 0.0045;
      cam.pitch = Math.min(1.5, Math.max(-0.15, cam.pitch + dy * 0.0035));
    }
    need = true;
  });
  canvas.addEventListener('wheel', e => {
    e.preventDefault();
    cam.dist = Math.min(900, Math.max(4, cam.dist * Math.exp(e.deltaY * 0.001)));
    need = true;
  }, { passive: false });
  canvas.addEventListener('contextmenu', e => e.preventDefault());

  // Two keys, and they are the two verbs the experiment is about.
  canvas.tabIndex = 0;
  canvas.addEventListener('keydown', e => {
    if (!edit) return;
    const k = e.key.toLowerCase();
    if (k === 'r') { edit.onTurn(e.shiftKey ? -1 : 1); e.preventDefault(); }
    else if (k === 'pageup' || k === 'e') { edit.onLift(1); e.preventDefault(); }
    else if (k === 'pagedown' || k === 'q') { edit.onLift(-1); e.preventDefault(); }
    else if (k === 'b') { view.boxes = !view.boxes; need = true; e.preventDefault(); }
  });
}

// ---------------------------------------------------------------- picking

/// Where the pointer is, in clip space.
function ndc(e) {
  const r = canvas.getBoundingClientRect();
  return [((e.clientX - r.left) / r.width) * 2 - 1, 1 - ((e.clientY - r.top) / r.height) * 2];
}

function eyeAt() {
  return [
    cam.at[0] + cam.dist * Math.cos(cam.pitch) * Math.sin(cam.yaw),
    cam.at[1] + cam.dist * Math.sin(cam.pitch),
    cam.at[2] + cam.dist * Math.cos(cam.pitch) * Math.cos(cam.yaw),
  ];
}

/// The ray under the pointer, in world space. Built by inverting the same two
/// matrices the frame was drawn with rather than by a second projection of its
/// own, so that what is picked is what was seen.
function ray(e) {
  const w = canvas.clientWidth || 1, h = canvas.clientHeight || 1;
  const eye = eyeAt();
  const [nx, ny] = ndc(e);
  // Camera basis: the same one `lookAt` builds.
  const z = norm([eye[0] - cam.at[0], eye[1] - cam.at[1], eye[2] - cam.at[2]]);
  const x = norm(cross([0, 1, 0], z));
  const y = cross(z, x);
  const tan = Math.tan(0.9 / 2);
  const ax = nx * tan * (w / h), ay = ny * tan;
  const d = norm([
    x[0] * ax + y[0] * ay - z[0],
    x[1] * ax + y[1] * ay - z[1],
    x[2] * ax + y[2] * ay - z[2],
  ]);
  return { o: eye, d };
}

/// The nearest component the pointer is over, from the boxes the server sent.
function under(e) {
  if (!view.units.length) return null;
  const { o, d } = ray(e);
  let best = null;
  for (const u of view.units) {
    const t = hitBox(o, d, u.solid.lo, u.solid.hi);
    if (t !== null && (!best || t < best.t)) best = { t, ...u, ...{ x: u.x, y: u.y, z: u.z } };
  }
  return best;
}

/// Slab test. Returns the near intersection distance, or null.
function hitBox(o, d, lo, hi) {
  let t0 = 0.001, t1 = 1e9;
  for (let i = 0; i < 3; i++) {
    if (Math.abs(d[i]) < 1e-9) {
      if (o[i] < lo[i] || o[i] > hi[i]) return null;
      continue;
    }
    let a = (lo[i] - o[i]) / d[i];
    let b = (hi[i] - o[i]) / d[i];
    if (a > b) { const t = a; a = b; b = t; }
    t0 = Math.max(t0, a);
    t1 = Math.min(t1, b);
    if (t0 > t1) return null;
  }
  return t0;
}

/// Where the pointer meets the floor of one storey, in metres.
function onPlane(e, level) {
  const { o, d } = ray(e);
  const y = level * 2;   // one tile up is two metres, and the grid is cubic
  if (Math.abs(d[1]) < 1e-6) return null;
  const t = (y - o[1]) / d[1];
  if (t <= 0) return null;
  return [o[0] + d[0] * t, y, o[2] + d[2] * t];
}

function frame() {
  requestAnimationFrame(frame);
  if (!gl || !need) return;
  need = false;
  draw();
}

function draw() {
  const dpr = Math.min(2, window.devicePixelRatio || 1);
  const w = Math.max(1, Math.round(canvas.clientWidth * dpr));
  const h = Math.max(1, Math.round(canvas.clientHeight * dpr));
  if (canvas.width !== w || canvas.height !== h) { canvas.width = w; canvas.height = h; }
  gl.viewport(0, 0, w, h);
  gl.clearColor(0.80, 0.84, 0.88, 1);
  gl.clear(gl.COLOR_BUFFER_BIT | gl.DEPTH_BUFFER_BIT);
  if (!scene) return;

  const eye = eyeAt();
  const vp = mul(perspective(0.9, w / h, 0.4, 4000), lookAt(eye, cam.at));

  gl.useProgram(prog);
  gl.uniformMatrix4fv(uni.uViewProj, false, vp);
  gl.uniform3fv(uni.uEye, eye);
  gl.uniform1f(uni.uPick, view.pick);

  // The very far level is one box, and drawing it is the point: it is what an
  // installation costs when it is a smudge on the horizon.
  if (view.lod >= 3) {
    proxy();
    return;
  }
  for (const b of scene.batches) {
    const n = b.keep[Math.min(view.lod, 2)];
    if (!n) continue;
    gl.uniform3fv(uni.uColour, b.colour);
    gl.uniform1f(uni.uRough, b.rough);
    gl.uniform1f(uni.uMetal, b.metal);
    gl.bindVertexArray(b.vao);
    gl.drawElementsInstanced(gl.TRIANGLES, b.count, gl.UNSIGNED_INT, 0, n);
  }
  gl.bindVertexArray(null);
  if (view.boxes) overlay(vp);
}

/// Green, yellow, red -- round every component, and round the room the
/// selected one needs to be serviced in.
///
/// The note asked for exactly this and it is worth being literal about it:
/// spatial optimisation only becomes gameplay if the player can *see* the
/// space. Drawn last, without depth writes, so the boxes read as annotation
/// over the plant rather than as more plant.
function overlay(vp) {
  if (!view.units.length) return;
  gl.useProgram(lineProg);
  gl.uniformMatrix4fv(lineUni.uViewProj, false, vp);
  gl.bindVertexArray(lineVao);
  gl.depthMask(false);
  const chosen = view.pick >= 0 ? names[view.pick] : null;
  for (const u of view.units) {
    const on = u.name === chosen;
    // Everything that is not clear, plus whatever is selected. A plant with
    // nothing wrong with it should look like a plant, not like a wireframe.
    if (u.verdict === 'clear' && !on) continue;
    const c = VERDICT[u.verdict] || VERDICT.clear;
    gl.uniform4fv(lineUni.uColour, on ? [c[0], c[1], c[2], 1.0] : c);
    box(u.solid.lo, u.solid.hi);
    if (on && u.service) {
      gl.uniform4fv(lineUni.uColour, [0.45, 0.70, 0.98, 0.55]);
      box(u.service.lo, u.service.hi);
    }
  }
  gl.depthMask(true);
  gl.bindVertexArray(null);
}

function box(lo, hi) {
  gl.uniform3fv(lineUni.uLo, lo);
  gl.uniform3fv(lineUni.uHi, hi);
  gl.drawArrays(gl.LINES, 0, 24);
}

/// One box, from the scene's own proxy volume: what an installation costs when
/// it is a smudge on the horizon.
function proxy() {
  if (!scene.proxy || !kit.meshes.box) return;
  const { lo, hi } = scene.proxy;
  if (!scene.proxyBatch) {
    const mesh = kit.meshes.box;
    const vao = gl.createVertexArray();
    gl.bindVertexArray(vao);
    bind(mesh.pos, 0, 3);
    bind(mesh.nrm, 1, 3);
    const inst = gl.createBuffer();
    gl.bindBuffer(gl.ARRAY_BUFFER, inst);
    gl.bufferData(gl.ARRAY_BUFFER, new Float32Array([
      (lo[0] + hi[0]) / 2, lo[1], (lo[2] + hi[2]) / 2,
      0, 1, 0,
      hi[0] - lo[0], hi[1] - lo[1], hi[2] - lo[2],
      0, 0, -1,
    ]), gl.STATIC_DRAW);
    const S = 12 * 4;
    attrib(2, 3, S, 0); attrib(3, 3, S, 12); attrib(4, 3, S, 24);
    attrib(5, 1, S, 36); attrib(6, 1, S, 40); attrib(7, 1, S, 44);
    gl.bindBuffer(gl.ELEMENT_ARRAY_BUFFER, mesh.idx);
    gl.bindVertexArray(null);
    scene.proxyBatch = { vao, count: mesh.count };
  }
  gl.uniform3fv(uni.uColour, [0.20, 0.22, 0.24]);
  gl.uniform1f(uni.uRough, 0.9);
  gl.uniform1f(uni.uMetal, 0);
  gl.bindVertexArray(scene.proxyBatch.vao);
  gl.drawElementsInstanced(gl.TRIANGLES, scene.proxyBatch.count, gl.UNSIGNED_INT, 0, 1);
  gl.bindVertexArray(null);
}

/// How much is actually being drawn, for the panel that says so.
export function drawn() {
  if (!scene) return { calls: 0, instances: 0 };
  if (view.lod >= 3) return { calls: 1, instances: 1 };
  let calls = 0, instances = 0;
  for (const b of scene.batches) {
    const n = b.keep[Math.min(view.lod, 2)];
    if (n) { calls++; instances += n; }
  }
  return { calls, instances };
}

// ------------------------------------------------------------------- maths

function perspective(fov, aspect, near, far) {
  const f = 1 / Math.tan(fov / 2);
  return new Float32Array([
    f / aspect, 0, 0, 0,
    0, f, 0, 0,
    0, 0, (far + near) / (near - far), -1,
    0, 0, (2 * far * near) / (near - far), 0,
  ]);
}

function lookAt(eye, at) {
  const z = norm([eye[0] - at[0], eye[1] - at[1], eye[2] - at[2]]);
  const x = norm(cross([0, 1, 0], z));
  const y = cross(z, x);
  return new Float32Array([
    x[0], y[0], z[0], 0,
    x[1], y[1], z[1], 0,
    x[2], y[2], z[2], 0,
    -dot(x, eye), -dot(y, eye), -dot(z, eye), 1,
  ]);
}

function mul(a, b) {
  const o = new Float32Array(16);
  for (let i = 0; i < 4; i++) {
    for (let j = 0; j < 4; j++) {
      let s = 0;
      for (let k = 0; k < 4; k++) s += a[k * 4 + j] * b[i * 4 + k];
      o[i * 4 + j] = s;
    }
  }
  return o;
}

const dot = (a, b) => a[0] * b[0] + a[1] * b[1] + a[2] * b[2];
const cross = (a, b) => [a[1] * b[2] - a[2] * b[1], a[2] * b[0] - a[0] * b[2], a[0] * b[1] - a[1] * b[0]];
function norm(v) {
  const l = Math.hypot(v[0], v[1], v[2]) || 1;
  return [v[0] / l, v[1] / l, v[2] / l];
}
