// The ridge in three dimensions: a mesh raised from the elevation model, any of the
// guide's maps draped over it, and tools to ask the ground questions.
import * as THREE from "three";
import { OrbitControls } from "three/addons/controls/OrbitControls.js";

const T = window.CawTerrain;
const C = window.CAW3D;
const $ = (s) => document.querySelector(s);
const stage = $("#view3d");
const R = 6378137;

function fail(msg) {
  const p = document.createElement("p");
  p.className = "nogl";
  p.textContent = msg;
  stage.appendChild(p);
}

let renderer;
try {
  renderer = new THREE.WebGLRenderer({ antialias: true, powerPreference: "high-performance" });
} catch (e) {
  fail("This device could not start WebGL, so the 3D view is not available. The flat maps and the explorer hold the same information.");
  throw e;
}
renderer.setPixelRatio(Math.min(window.devicePixelRatio || 1, 2));
renderer.outputColorSpace = THREE.SRGBColorSpace;
renderer.shadowMap.enabled = false;
renderer.shadowMap.type = THREE.PCFSoftShadowMap;
stage.appendChild(renderer.domElement);

const scene = new THREE.Scene();
const camera = new THREE.PerspectiveCamera(45, 1, 5, 400000);
const controls = new OrbitControls(camera, renderer.domElement);
controls.enableDamping = true;
controls.dampingFactor = 0.12;
controls.maxPolarAngle = Math.PI * 0.495;
controls.screenSpacePanning = false;
controls.zoomSpeed = 1.1;

const ambient = new THREE.AmbientLight(0xffffff, 0.55);
const sunLight = new THREE.DirectionalLight(0xfff4e0, 2.6);
sunLight.castShadow = true;
scene.add(ambient, sunLight, sunLight.target);
ambient.visible = false;
sunLight.visible = false;

const state = {
  area: "close",
  layer: 0,
  exag: 1.5,
  sun: false,
  minutes: 0,
  date: new Date(),
  stand: null,
  yaw: 0,
  pitch: 0,
  measureFrom: null,
  picked: null,
  savedExag: 1.5,
  savedView: null,
  dirty: true
};
let area = null, mesh = null, basic = null, lit = null, marks = [], lines = new THREE.Group(), measureLine = null, pickDot = null;
scene.add(lines);

function themeColours() {
  const dark = document.documentElement.getAttribute("data-theme") === "dark" ||
    (!document.documentElement.getAttribute("data-theme") && window.matchMedia("(prefers-color-scheme: dark)").matches);
  const bg = new THREE.Color(dark ? 0x10130f : 0xcfd8dc);
  scene.background = bg;
  scene.fog = new THREE.Fog(bg, 30000, 140000);
  state.dirty = true;
}

// ---------------------------------------------------------------- the ground
function setupArea(name) {
  const spec = C.areas[name];
  const g = T.grid(spec.elev);
  const b = spec.bbox;
  const mid = (b.s + b.n) / 2 * Math.PI / 180;
  const W = R * (b.e - b.w) * Math.PI / 180 * Math.cos(mid);
  const H = R * (T.merc(b.n) - T.merc(b.s)) * Math.cos(mid);
  // About a quarter of a million vertices: all a 30 m model can use, and light on a phone.
  const step = Math.max(1, Math.ceil(Math.sqrt(g.w * g.h / 260000)));
  const nx = Math.floor((g.w - 1) / step) + 1, ny = Math.floor((g.h - 1) / step) + 1;
  const pos = new Float32Array(nx * ny * 3), uv = new Float32Array(nx * ny * 2);
  let lo = 1e9, hi = -1e9;
  for (let j = 0; j < ny; j++) {
    for (let i = 0; i < nx; i++) {
      const gx = Math.min(g.w - 1, i * step), gy = Math.min(g.h - 1, j * step);
      const z = T.elevPixel(spec.elev, gx, gy);
      const u = (gx + 0.5) / g.w, v = (gy + 0.5) / g.h;
      const k = j * nx + i;
      pos[k * 3] = (u - 0.5) * W;
      pos[k * 3 + 1] = z;
      pos[k * 3 + 2] = (v - 0.5) * H;
      uv[k * 2] = u;
      uv[k * 2 + 1] = 1 - v;
      if (z < lo) { lo = z; }
      if (z > hi) { hi = z; }
    }
  }
  const idx = new Uint32Array((nx - 1) * (ny - 1) * 6);
  let n = 0;
  for (let j = 0; j < ny - 1; j++) {
    for (let i = 0; i < nx - 1; i++) {
      const a = j * nx + i, c = a + nx;
      idx[n++] = a; idx[n++] = c; idx[n++] = a + 1;
      idx[n++] = a + 1; idx[n++] = c; idx[n++] = c + 1;
    }
  }
  const geo = new THREE.BufferGeometry();
  geo.setAttribute("position", new THREE.BufferAttribute(pos, 3));
  geo.setAttribute("uv", new THREE.BufferAttribute(uv, 2));
  geo.setIndex(new THREE.BufferAttribute(idx, 1));
  geo.computeVertexNormals();
  if (mesh) { scene.remove(mesh); mesh.geometry.dispose(); }
  basic = basic || new THREE.MeshBasicMaterial({ color: 0xffffff });
  lit = lit || new THREE.MeshLambertMaterial({ color: 0xffffff });
  mesh = new THREE.Mesh(geo, state.sun ? lit : basic);
  mesh.castShadow = true;
  mesh.receiveShadow = true;
  mesh.scale.y = state.exag;
  scene.add(mesh);
  area = { name, b, W, H, lo, hi, cell: W / g.w * step };
  state.area = name;
  document.querySelectorAll("[data-area]").forEach((el) => el.setAttribute("aria-pressed", el.getAttribute("data-area") === name ? "true" : "false"));
  const say = $("[data-mesh]");
  if (say) { say.textContent = (W / 1000).toFixed(1) + " by " + (H / 1000).toFixed(1) + " km, one vertex every " + Math.round(area.cell) + " m, " + (idx.length / 3).toLocaleString("en-CA") + " triangles"; }
  loadLayer(state.layer);
  placeMarks();
  placeLines();
  state.dirty = true;
}

function inside(lat, lon) {
  return area && lon >= area.b.w && lon <= area.b.e && lat >= area.b.s && lat <= area.b.n;
}
function toWorld(lat, lon, lift) {
  const b = area.b;
  const u = (lon - b.w) / (b.e - b.w), v = (T.merc(b.n) - T.merc(lat)) / (T.merc(b.n) - T.merc(b.s));
  const h = T.height(lat, lon);
  return new THREE.Vector3((u - 0.5) * area.W, ((h === null ? area.lo : h) + (lift || 0)) * state.exag, (v - 0.5) * area.H);
}
function fromWorld(x, z) {
  const b = area.b;
  const u = x / area.W + 0.5, v = z / area.H + 0.5;
  return [T.unmerc(T.merc(b.n) - v * (T.merc(b.n) - T.merc(b.s))), b.w + u * (b.e - b.w)];
}
function groundAt(x, z) {
  const ll = fromWorld(x, z);
  if (!inside(ll[0], ll[1])) { return null; }
  const h = T.height(ll[0], ll[1]);
  return h === null ? null : h * state.exag;
}

// ------------------------------------------------------------------- the map
const loader = new THREE.ImageLoader();
let loading = 0;
function loadLayer(i) {
  state.layer = i;
  const spec = C.layers[i];
  const src = spec[state.area];
  const ticket = ++loading;
  const note = $("[data-loading]");
  if (note) { note.textContent = "Loading the map"; }
  loader.load(src, (img) => {
    if (ticket !== loading) { return; }
    const max = Math.min(renderer.capabilities.maxTextureSize, 4096);
    let source = img;
    if (img.width > max || img.height > max) {
      const k = Math.min(max / img.width, max / img.height);
      const c = document.createElement("canvas");
      c.width = Math.floor(img.width * k); c.height = Math.floor(img.height * k);
      c.getContext("2d").drawImage(img, 0, 0, c.width, c.height);
      source = c;
    }
    const tex = new THREE.Texture(source);
    tex.colorSpace = THREE.SRGBColorSpace;
    tex.anisotropy = renderer.capabilities.getMaxAnisotropy();
    tex.minFilter = THREE.LinearMipmapLinearFilter;
    tex.needsUpdate = true;
    [basic, lit].forEach((m) => { if (m.map) { m.map.dispose(); } m.map = tex; m.needsUpdate = true; });
    if (note) { note.textContent = ""; }
    const blurb = $("[data-blurb]");
    if (blurb) { blurb.textContent = spec.blurb; }
    state.dirty = true;
  }, undefined, () => { if (note) { note.textContent = "That map could not be loaded."; } });
}

// ------------------------------------------------------------------ markers
function label(text, fill, ink, shape) {
  const pad = 14, fs = 34;
  const c = document.createElement("canvas");
  const ctx = c.getContext("2d");
  ctx.font = "600 " + fs + "px 'Barlow Semi Condensed', 'Barlow', sans-serif";
  const tw = Math.ceil(ctx.measureText(text).width);
  const icon = 34;
  c.width = tw + pad * 2 + icon + 8; c.height = fs + pad * 2 + 22;
  ctx.font = "600 " + fs + "px 'Barlow Semi Condensed', 'Barlow', sans-serif";
  const bh = fs + pad * 1.4;
  ctx.fillStyle = "rgba(20,20,18,0.86)";
  ctx.beginPath();
  ctx.roundRect(0, 0, c.width, bh, 10);
  ctx.fill();
  ctx.beginPath();
  ctx.moveTo(c.width / 2 - 10, bh - 1); ctx.lineTo(c.width / 2, bh + 20); ctx.lineTo(c.width / 2 + 10, bh - 1);
  ctx.fill();
  ctx.fillStyle = fill; ctx.strokeStyle = ink; ctx.lineWidth = 3;
  const cx = pad + icon / 2 - 2, cy = bh / 2;
  ctx.beginPath();
  if (shape === "diamond") { ctx.moveTo(cx, cy - 14); ctx.lineTo(cx + 14, cy); ctx.lineTo(cx, cy + 14); ctx.lineTo(cx - 14, cy); ctx.closePath(); }
  else if (shape === "triangle") { ctx.moveTo(cx, cy - 14); ctx.lineTo(cx + 14, cy + 11); ctx.lineTo(cx - 14, cy + 11); ctx.closePath(); }
  else if (shape === "gap") { ctx.moveTo(cx - 13, cy - 13); ctx.quadraticCurveTo(cx - 2, cy, cx - 13, cy + 13); ctx.moveTo(cx + 13, cy - 13); ctx.quadraticCurveTo(cx + 2, cy, cx + 13, cy + 13); }
  else { ctx.arc(cx, cy, 13, 0, Math.PI * 2); }
  if (shape !== "gap") { ctx.fill(); }
  ctx.stroke();
  ctx.fillStyle = "#fbf8ef";
  ctx.textBaseline = "middle";
  ctx.fillText(text, pad + icon + 4, bh / 2 + 2);
  const tex = new THREE.CanvasTexture(c);
  tex.colorSpace = THREE.SRGBColorSpace;
  const s = new THREE.Sprite(new THREE.SpriteMaterial({ map: tex, sizeAttenuation: false, depthTest: true, transparent: true }));
  const k = 0.00052;
  s.scale.set(c.width * k, c.height * k, 1);
  s.userData.base = [c.width * k, c.height * k];
  s.center.set(0.5, 0);
  return s;
}
const STYLE = {
  waypoint: ["#d7261e", "#ffffff", "dot"],
  ridge: ["#ffffff", "#d7261e", "dot"],
  glassing: ["#ffd21f", "#1d1d1b", "diamond"],
  summit: ["#fbf8ef", "#1d1d1b", "triangle"],
  saddle: ["#1d1d1b", "#fbf8ef", "gap"],
  road: ["#1d1d1b", "#ffffff", "dot"],
  camp: ["#1f6a30", "#ffffff", "triangle"],
  service: ["#2a78d6", "#ffffff", "dot"]
};
function placeMarks() {
  marks.forEach((m) => { scene.remove(m.sprite); m.sprite.material.map.dispose(); m.sprite.material.dispose(); });
  marks = [];
  const on = {};
  document.querySelectorAll("[data-show]").forEach((el) => { on[el.getAttribute("data-show")] = el.checked; });
  C.places.forEach((p) => {
    if (!inside(p.lat, p.lon)) { return; }
    const group = p.kind === "summit" || p.kind === "saddle" ? "shape" : (p.kind === "waypoint" || p.kind === "ridge" || p.kind === "glassing" ? "points" : "other");
    if (on[group] === false) { return; }
    const st = STYLE[p.kind] || STYLE.road;
    const s = label(p.short || p.name, st[0], st[1], st[2]);
    s.position.copy(toWorld(p.lat, p.lon, 4));
    s.userData = Object.assign({ base: s.userData.base }, p);
    scene.add(s);
    marks.push({ sprite: s, place: p });
  });
  state.dirty = true;
}
function placeLines() {
  lines.clear();
  const show = $("[data-show=routes]");
  if (show && !show.checked) { state.dirty = true; return; }
  C.routes.forEach((r) => {
    const pts = [];
    for (let i = 0; i < r.pts.length - 1; i++) {
      const a = r.pts[i], b = r.pts[i + 1];
      const n = Math.max(1, Math.ceil(T.haversine([a[1], a[0]], [b[1], b[0]]) / 25));
      for (let k = 0; k < n; k++) {
        const t = k / n, lat = a[1] + (b[1] - a[1]) * t, lon = a[0] + (b[0] - a[0]) * t;
        if (inside(lat, lon)) { pts.push(toWorld(lat, lon, 5)); } else if (pts.length > 1) { addLine(pts.splice(0), r.kind); } else { pts.length = 0; }
      }
    }
    if (pts.length > 1) { addLine(pts, r.kind); }
  });
  state.dirty = true;
}
function addLine(pts, kind) {
  const g = new THREE.BufferGeometry().setFromPoints(pts);
  const walk = kind === "walk";
  const m = walk ? new THREE.LineDashedMaterial({ color: 0xff3b30, dashSize: 40, gapSize: 30 }) : new THREE.LineBasicMaterial({ color: 0xff8a3d });
  const l = new THREE.Line(g, m);
  if (walk) { l.computeLineDistances(); }
  lines.add(l);
}

// Where labels would sit on top of one another, the more important one is kept.
const RANK = { waypoint: 0, ridge: 0, glassing: 1, saddle: 2, summit: 3, road: 4, camp: 5, service: 6 };
function declutter() {
  const w = renderer.domElement.clientWidth, h = renderer.domElement.clientHeight;
  const v = new THREE.Vector3();
  const kept = [];
  camera.updateMatrixWorld();
  // Labels keep their size on screen when the glass is raised.
  const zoom = Math.tan(camera.fov * Math.PI / 360) / Math.tan(45 * Math.PI / 360);
  marks.forEach((m) => m.sprite.scale.set(m.sprite.userData.base[0] * zoom, m.sprite.userData.base[1] * zoom, 1));
  const rank = (m) => (m.place.kind in RANK ? RANK[m.place.kind] : 9);
  marks.slice().sort((a, b) => rank(a) - rank(b)).forEach((m) => {
    v.copy(m.sprite.position).project(camera);
    if (v.z > 1 || v.z < -1) { m.sprite.visible = false; return; }
    const x = (v.x + 1) / 2 * w, y = (1 - v.y) / 2 * h;
    // A sprite that ignores distance is sized as a share of the view's height.
    const sh = m.sprite.scale.y * h / (2 * Math.tan(camera.fov * Math.PI / 360)), sw = sh * m.sprite.scale.x / m.sprite.scale.y;
    const box = [x - sw / 2, y - sh, x + sw / 2, y];
    const clash = kept.some((k) => box[0] < k[2] && box[2] > k[0] && box[1] < k[3] && box[3] > k[1]);
    m.sprite.visible = !clash;
    if (!clash) { kept.push(box); }
  });
}

// ---------------------------------------------------------------------- sun
function albertaOffset(d) {
  const y = d.getUTCFullYear();
  const firstSunday = (m) => { const f = new Date(Date.UTC(y, m, 1)); return 1 + (7 - f.getUTCDay()) % 7; };
  const start = Date.UTC(y, 2, firstSunday(2) + 7, 9), end = Date.UTC(y, 10, firstSunday(10), 8);
  return d.getTime() >= start && d.getTime() < end ? -6 : -7;
}
function sunPosition(date, lat, lon) {
  const jd = date.getTime() / 86400000 + 2440587.5, t = (jd - 2451545) / 36525, rad = Math.PI / 180;
  const l0 = (280.46646 + t * (36000.76983 + t * 0.0003032)) % 360;
  const m = 357.52911 + t * (35999.05029 - 0.0001537 * t);
  const e = 0.016708634 - t * (0.000042037 + 0.0000001267 * t);
  const c = Math.sin(m * rad) * (1.914602 - t * (0.004817 + 0.000014 * t)) + Math.sin(2 * m * rad) * (0.019993 - 0.000101 * t) + Math.sin(3 * m * rad) * 0.000289;
  const omega = 125.04 - 1934.136 * t;
  const lambda = l0 + c - 0.00569 - 0.00478 * Math.sin(omega * rad);
  const eps = 23 + (26 + (21.448 - t * (46.815 + t * (0.00059 - t * 0.001813))) / 60) / 60 + 0.00256 * Math.cos(omega * rad);
  const decl = Math.asin(Math.sin(eps * rad) * Math.sin(lambda * rad));
  const y = Math.tan(eps * rad / 2) ** 2;
  const eq = 4 / rad * (y * Math.sin(2 * l0 * rad) - 2 * e * Math.sin(m * rad) + 4 * e * y * Math.sin(m * rad) * Math.cos(2 * l0 * rad) - 0.5 * y * y * Math.sin(4 * l0 * rad) - 1.25 * e * e * Math.sin(2 * m * rad));
  const utcMin = date.getUTCHours() * 60 + date.getUTCMinutes() + date.getUTCSeconds() / 60;
  const tst = (((utcMin + eq + 4 * lon) % 1440) + 1440) % 1440;
  const ha = (tst / 4 - 180) * rad, p = lat * rad;
  const cosz = Math.sin(p) * Math.sin(decl) + Math.cos(p) * Math.cos(decl) * Math.cos(ha);
  const alt = 90 - Math.acos(Math.max(-1, Math.min(1, cosz))) / rad;
  const az = ((Math.atan2(Math.sin(ha), Math.cos(ha) * Math.sin(p) - Math.tan(decl) * Math.cos(p)) / rad + 180) % 360 + 360) % 360;
  return { az, alt };
}
function sunMoment() {
  const d = state.date, off = albertaOffset(d);
  const base = Date.UTC(d.getFullYear(), d.getMonth(), d.getDate(), -off, 0, 0);
  return { when: new Date(base + state.minutes * 60000), off };
}
function applySun() {
  const on = state.sun;
  ambient.visible = on;
  sunLight.visible = on;
  renderer.shadowMap.enabled = on;
  if (mesh) { mesh.material = on ? lit : basic; }
  const out = $("[data-sun-says]");
  if (on && area) {
    const m = sunMoment();
    const mid = [(area.b.s + area.b.n) / 2, (area.b.w + area.b.e) / 2];
    const s = sunPosition(m.when, mid[0], mid[1]);
    const a = s.az * Math.PI / 180, h = Math.max(s.alt, 0.3) * Math.PI / 180;
    const dir = new THREE.Vector3(Math.sin(a) * Math.cos(h), Math.sin(h), -Math.cos(a) * Math.cos(h));
    const span = Math.max(area.W, area.H);
    sunLight.target.position.set(0, area.lo * state.exag, 0);
    sunLight.position.copy(dir.multiplyScalar(span * 1.5)).add(sunLight.target.position);
    const cam = sunLight.shadow.camera;
    cam.left = -span * 0.75; cam.right = span * 0.75; cam.top = span * 0.75; cam.bottom = -span * 0.75;
    cam.near = 10; cam.far = span * 3.2;
    cam.updateProjectionMatrix();
    const size = Math.min(renderer.capabilities.maxTextureSize, window.innerWidth < 800 ? 2048 : 4096);
    if (sunLight.shadow.mapSize.x !== size) { sunLight.shadow.mapSize.set(size, size); if (sunLight.shadow.map) { sunLight.shadow.map.dispose(); sunLight.shadow.map = null; } }
    sunLight.shadow.bias = -0.0006;
    sunLight.shadow.normalBias = area.cell * 0.6;
    const up = s.alt > 0;
    // Light units here are physical: a surface square to a light of strength pi shows
    // its own colour. The sky fills the shade; the sun adds to it.
    sunLight.intensity = up ? Math.PI * (0.75 + 0.45 * Math.min(1, s.alt / 20)) : 0.0;
    ambient.intensity = Math.PI * (up ? 0.5 : 0.3);
    sunLight.color.setHSL(0.09, up ? Math.max(0.1, 0.75 - s.alt / 40) : 0, 0.92);
    if (out) {
      out.textContent = T.clock(state.minutes / 60) + (m.off === -6 ? " MDT" : " MST") + ". " + (up ? "Sun " + s.alt.toFixed(0) + "° up in the " + T.DIR8[Math.round(s.az / 45) % 8] + " (" + Math.round(s.az) + "°)." : "The sun is below the horizon.");
    }
  } else if (out) { out.textContent = "The map's own shading, lit from the north-west."; }
  document.querySelectorAll("[data-sun-only]").forEach((el) => { el.hidden = !on; });
  state.dirty = true;
}

// --------------------------------------------------------------- asking the ground
const ray = new THREE.Raycaster();
function pick(clientX, clientY) {
  const r = renderer.domElement.getBoundingClientRect();
  ray.setFromCamera(new THREE.Vector2((clientX - r.left) / r.width * 2 - 1, -((clientY - r.top) / r.height) * 2 + 1), camera);
  // A marker under the finger wins.
  const hit = ray.intersectObjects(marks.map((m) => m.sprite), false)[0];
  if (hit) { return { lat: hit.object.userData.lat, lon: hit.object.userData.lon, place: hit.object.userData }; }
  // Otherwise march the ray across the height grid until it goes under.
  const o = ray.ray.origin, d = ray.ray.direction;
  let t = 0, prev = 0, step = Math.max(8, area.cell * 0.5);
  const far = Math.max(area.W, area.H) * 3;
  let started = false;
  while (t < far) {
    const x = o.x + d.x * t, y = o.y + d.y * t, z = o.z + d.z * t;
    const g = groundAt(x, z);
    if (g !== null) {
      started = true;
      if (y <= g) {
        let a = prev, b = t;
        for (let i = 0; i < 18; i++) {
          const m = (a + b) / 2, gm = groundAt(o.x + d.x * m, o.z + d.z * m);
          if (gm !== null && o.y + d.y * m <= gm) { b = m; } else { a = m; }
        }
        const ll = fromWorld(o.x + d.x * b, o.z + d.z * b);
        return { lat: ll[0], lon: ll[1] };
      }
    } else if (started) { return null; }
    prev = t;
    t += step;
    step = Math.max(8, Math.min(60, t * 0.004 + area.cell * 0.4));
  }
  return null;
}

const info = $("#info"), infoWrap = $("#infowrap");
function showPoint(p) {
  state.picked = p;
  const q = T.query(p.lat, p.lon);
  const rows = T.describe(q);
  if (p.place && p.place.note) { rows.unshift({ k: "About", v: p.place.note, note: "" }); }
  const acts = [
    { label: "Stand here", run: () => stand(p.lat, p.lon) },
    { label: state.measureFrom ? "Measure to here" : "Measure from here", run: () => measure(p) },
    { label: "Flat map", run: () => { location.href = "explore.html?lat=" + p.lat.toFixed(5) + "&lon=" + p.lon.toFixed(5); } }
  ];
  if (state.measureFrom) { acts.push({ label: "Clear measure", run: clearMeasure }); }
  T.render(info, p.place ? p.place.name : "This ground", rows, acts);
  infoWrap.hidden = false;
  if (!pickDot) {
    pickDot = new THREE.Mesh(new THREE.SphereGeometry(1, 20, 14), new THREE.MeshBasicMaterial({ color: 0xffffff }));
    scene.add(pickDot);
  }
  pickDot.position.copy(toWorld(p.lat, p.lon, 1));
  sizeDot();
  pickDot.visible = true;
  state.dirty = true;
}
function sizeDot() {
  if (!pickDot || !pickDot.visible) { return; }
  const d = camera.position.distanceTo(pickDot.position);
  pickDot.scale.setScalar(Math.max(2, d * Math.tan(camera.fov * Math.PI / 360) * 0.012));
}

function clearMeasure() {
  state.measureFrom = null;
  if (measureLine) { scene.remove(measureLine); measureLine.geometry.dispose(); measureLine = null; }
  $("#measure").hidden = true;
  if (state.picked) { showPoint(state.picked); }
  state.dirty = true;
}
function measure(p) {
  if (!state.measureFrom) {
    state.measureFrom = { lat: p.lat, lon: p.lon };
    const box = $("#measure");
    box.hidden = false;
    box.textContent = "";
    const h = document.createElement("h3"); h.textContent = "Measuring"; box.appendChild(h);
    const s = document.createElement("p"); s.textContent = "First point set. Tap a second point, then press Measure to here."; box.appendChild(s);
    showPoint(p);
    return;
  }
  const a = [state.measureFrom.lat, state.measureFrom.lon], b = [p.lat, p.lon];
  const d = T.haversine(a, b);
  const n = Math.max(20, Math.min(400, Math.round(d / 20)));
  const prof = T.profile(a, b, n).filter((q) => q.z !== null);
  if (prof.length < 2) { return; }
  const za = prof[0].z, zb = prof[prof.length - 1].z;
  let up = 0, down = 0, path = 0;
  for (let i = 1; i < prof.length; i++) {
    const dz = prof[i].z - prof[i - 1].z, dd = prof[i].d - prof[i - 1].d;
    if (dz > 0) { up += dz; } else { down -= dz; }
    path += Math.hypot(dd, dz);
  }
  const brg = T.bearing(a, b), mag = (brg - C.declination + 360) % 360;
  const los = T.sight(a, b, C.eye, C.animal);
  const rows = [
    { k: "Distance", v: T.dist(d), note: "over the ground " + T.dist(path) },
    { k: "Bearing", v: Math.round(brg) + "° true", note: T.DIR16[Math.round(brg / 22.5) % 16] + ", " + Math.round(mag) + "° magnetic" },
    { k: "Height", v: (zb >= za ? "up " : "down ") + T.num(Math.abs(zb - za)) + " m", note: T.num(za) + " m to " + T.num(zb) + " m" },
    { k: "Climb and descent", v: "+" + T.num(up) + " m, -" + T.num(down) + " m", note: "along the straight line" },
    { k: "Angle of the shot", v: (Math.atan2(zb - za, d) * 180 / Math.PI).toFixed(1) + "°", note: zb >= za ? "uphill" : "downhill" },
    { k: "Line of sight", v: los === null ? "unknown" : (los.clear ? "clear" : "blocked"), note: los && !los.clear ? "by ground " + T.dist(los.at.d) + " out, at " + T.num(los.at.z) + " m" : "eye 1.7 m, target 1.0 m, bare ground" }
  ];
  const box = $("#measure");
  T.render(box, "Between the two points", rows, [{ label: "Clear the measure", run: clearMeasure }]);
  box.insertBefore(chart(prof, los), box.querySelector(".acts"));
  box.hidden = false;
  if (measureLine) { scene.remove(measureLine); measureLine.geometry.dispose(); }
  measureLine = new THREE.Line(new THREE.BufferGeometry().setFromPoints(prof.map((q) => toWorld(q.lat, q.lon, 4))), new THREE.LineBasicMaterial({ color: 0xffffff }));
  measureLine.userData = prof;
  scene.add(measureLine);
  state.measureFrom = null;
  showPoint(p);
}
function chart(prof, los) {
  const NS = "http://www.w3.org/2000/svg", W = 300, H = 110, L = 38, B = 18;
  const svg = document.createElementNS(NS, "svg");
  svg.setAttribute("viewBox", "0 0 " + W + " " + H);
  svg.setAttribute("class", "mini");
  svg.setAttribute("role", "img");
  svg.setAttribute("aria-label", "Height along the line between the two points");
  let lo = 1e9, hi = -1e9;
  prof.forEach((q) => { lo = Math.min(lo, q.z); hi = Math.max(hi, q.z); });
  const pad = Math.max(10, (hi - lo) * 0.12); lo -= pad; hi += pad;
  const dmax = prof[prof.length - 1].d;
  const X = (d) => L + d / dmax * (W - L - 6), Y = (z) => H - B - (z - lo) / (hi - lo) * (H - B - 8);
  const el = (tag, attrs, text) => { const e = document.createElementNS(NS, tag); Object.keys(attrs).forEach((k) => e.setAttribute(k, attrs[k])); if (text !== undefined) { e.textContent = text; } svg.appendChild(e); return e; };
  let d = "";
  prof.forEach((q, i) => { d += (i ? "L" : "M") + X(q.d).toFixed(1) + " " + Y(q.z).toFixed(1); });
  el("path", { d: d + "L" + X(dmax) + " " + (H - B) + "L" + L + " " + (H - B) + "Z", class: "wash" });
  el("path", { d, class: "line" });
  el("line", { x1: L, x2: W - 6, y1: H - B, y2: H - B, class: "base" });
  el("line", { x1: X(0), y1: Y(prof[0].z + C.eye), x2: X(dmax), y2: Y(prof[prof.length - 1].z + C.animal), class: los && los.clear ? "sight clear" : "sight" });
  el("text", { x: L - 5, y: Y(hi - pad) + 4, "text-anchor": "end", class: "tick" }, T.num(hi - pad));
  el("text", { x: L - 5, y: Y(lo + pad) + 4, "text-anchor": "end", class: "tick" }, T.num(lo + pad));
  el("text", { x: L, y: H - 4, "text-anchor": "start", class: "tick" }, "0");
  el("text", { x: W - 6, y: H - 4, "text-anchor": "end", class: "tick" }, T.dist(dmax));
  return svg;
}

// ------------------------------------------------------------ standing on the ground
function stand(lat, lon) {
  if (!state.stand) {
    state.savedExag = state.exag;
    state.savedView = { p: camera.position.clone(), t: controls.target.clone(), fov: camera.fov, layer: state.layer };
    // Contour lines a metre wide make poor ground to stand on: use the satellite image.
    const sat = C.layers.findIndex((l) => l.id === "sat");
    if (sat >= 0 && C.layers[state.layer].id === "topo") { $("#layer").value = sat; loadLayer(sat); }
    // Look the way the camera was already looking.
    const f = new THREE.Vector3();
    camera.getWorldDirection(f);
    state.yaw = Math.atan2(f.x, -f.z);
  }
  state.stand = { lat, lon };
  setExag(1);
  state.pitch = -0.04;
  camera.fov = 60;
  camera.near = 0.5;
  camera.updateProjectionMatrix();
  controls.enabled = false;
  const p = toWorld(lat, lon, C.eye);
  camera.position.copy(p);
  if (pickDot) { pickDot.visible = false; }
  infoWrap.hidden = true;
  $("#measure").hidden = true;
  $("#standbar").hidden = false;
  $("#panel").classList.add("folded");
  $("[data-fold]").setAttribute("aria-expanded", "false");
  aim();
}
function leave() {
  if (!state.stand) { return; }
  state.stand = null;
  controls.enabled = true;
  camera.near = 5;
  camera.fov = state.savedView.fov;
  camera.position.copy(state.savedView.p);
  controls.target.copy(state.savedView.t);
  camera.updateProjectionMatrix();
  setExag(state.savedExag);
  if (state.savedView.layer !== state.layer && C.layers[state.layer].id === "sat") { $("#layer").value = state.savedView.layer; loadLayer(state.savedView.layer); }
  $("#standbar").hidden = true;
  $("#panel").classList.remove("folded");
  $("[data-fold]").setAttribute("aria-expanded", "true");
  state.dirty = true;
}
function aim() {
  state.pitch = Math.max(-1.2, Math.min(1.2, state.pitch));
  camera.rotation.order = "YXZ";
  camera.rotation.set(state.pitch, -state.yaw, 0);
  const deg = ((state.yaw * 180 / Math.PI) % 360 + 360) % 360;
  const mag = (deg - C.declination + 360) % 360;
  const power = 60 / camera.fov;
  $("[data-heading]").textContent = "Facing " + Math.round(deg) + "° true (" + T.DIR16[Math.round(deg / 22.5) % 16] + "), " + Math.round(mag) + "° magnetic" + (power > 1.05 ? ". Glass at " + power.toFixed(power < 10 ? 1 : 0) + " power" : "");
  const h = T.height(state.stand.lat, state.stand.lon);
  $("[data-standing]").textContent = "Standing at " + T.num(h) + " m, eye " + C.eye.toFixed(1) + " m above the ground";
  state.dirty = true;
}

// ---------------------------------------------------------------- the controls
function setExag(v) {
  state.exag = v;
  if (mesh) { mesh.scale.y = v; }
  const r = $("#exag");
  if (r) { r.value = v; }
  const o = $("[data-exag]");
  if (o) { o.textContent = v === 1 ? "True to scale" : v.toFixed(1) + " times"; }
  marks.forEach((m) => m.sprite.position.copy(toWorld(m.place.lat, m.place.lon, 4)));
  placeLines();
  if (pickDot && state.picked) { pickDot.position.copy(toWorld(state.picked.lat, state.picked.lon, 1)); }
  if (measureLine) { measureLine.geometry.setFromPoints(measureLine.userData.map((q) => toWorld(q.lat, q.lon, 4))); }
  if (state.sun) { applySun(); }
  state.dirty = true;
}
function view(kind, focus) {
  const f = focus && inside(focus[0], focus[1]) ? focus : (inside(C.waypoint[0], C.waypoint[1]) ? C.waypoint : [(area.b.s + area.b.n) / 2, (area.b.w + area.b.e) / 2]);
  const t = toWorld(f[0], f[1], 0);
  const span = Math.max(area.W, area.H);
  const from = { south: [0, 0.34, 0.62], north: [0, 0.34, -0.62], east: [0.62, 0.34, 0], west: [-0.62, 0.34, 0], above: [0, 0.95, 0.01], near: [0.05, 0.1, 0.17] }[kind] || [0, 0.34, 0.62];
  controls.target.copy(t);
  camera.position.set(t.x + from[0] * span, t.y + from[1] * span, t.z + from[2] * span);
  controls.update();
  state.dirty = true;
}

$("#layer").addEventListener("change", (e) => loadLayer(Number(e.target.value)));
$("#exag").addEventListener("input", (e) => setExag(Number(e.target.value)));
document.querySelectorAll("[data-area]").forEach((el) => el.addEventListener("click", () => {
  if (state.stand) { leave(); }
  clearMeasure();
  setupArea(el.getAttribute("data-area"));
  view("south", state.picked ? [state.picked.lat, state.picked.lon] : null);
  if (state.picked && inside(state.picked.lat, state.picked.lon)) { showPoint(state.picked); } else { infoWrap.hidden = true; if (pickDot) { pickDot.visible = false; } }
}));
document.querySelectorAll("[data-view]").forEach((el) => el.addEventListener("click", () => { if (state.stand) { leave(); } view(el.getAttribute("data-view"), state.picked ? [state.picked.lat, state.picked.lon] : null); }));
document.querySelectorAll("[data-show]").forEach((el) => el.addEventListener("change", () => { placeMarks(); placeLines(); }));
$("#sun").addEventListener("change", (e) => {
  state.sun = e.target.checked;
  if (state.sun && !state.stand && state.exag !== 1) { setExag(1); }
  applySun();
});
$("#time").addEventListener("input", (e) => { state.minutes = Number(e.target.value); applySun(); });
$("#date").addEventListener("change", (e) => {
  const p = e.target.value.split("-").map(Number);
  if (p.length === 3 && p[0]) { state.date = new Date(p[0], p[1] - 1, p[2]); applySun(); }
});
$("[data-leave]").addEventListener("click", leave);
$("[data-fold]").addEventListener("click", () => { const p = $("#panel"); p.classList.toggle("folded"); $("[data-fold]").setAttribute("aria-expanded", p.classList.contains("folded") ? "false" : "true"); });
$("[data-close-info]").addEventListener("click", () => { infoWrap.hidden = true; if (pickDot) { pickDot.visible = false; } state.dirty = true; });
new MutationObserver(themeColours).observe(document.documentElement, { attributes: true, attributeFilter: ["data-theme"] });

// Taps and drags on the view.
const el = renderer.domElement;
let down = null;
const fingers = new Map();
let pinch = 0;
el.addEventListener("pointerdown", (e) => {
  down = { x: e.clientX, y: e.clientY, t: performance.now(), moved: 0 };
  fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  pinch = 0;
  if (state.stand) { el.setPointerCapture(e.pointerId); }
});
el.addEventListener("pointermove", (e) => {
  if (down) { down.moved = Math.max(down.moved, Math.hypot(e.clientX - down.x, e.clientY - down.y)); }
  if (!state.stand || !fingers.has(e.pointerId)) { return; }
  const last = fingers.get(e.pointerId);
  if (fingers.size === 1) {
    const k = camera.fov * Math.PI / 180 / el.clientHeight;
    state.yaw -= (e.clientX - last.x) * k;
    state.pitch += (e.clientY - last.y) * k;
    aim();
  }
  fingers.set(e.pointerId, { x: e.clientX, y: e.clientY });
  if (fingers.size === 2) {
    const a = Array.from(fingers.values()), d = Math.hypot(a[0].x - a[1].x, a[0].y - a[1].y);
    if (pinch) { camera.fov = Math.max(2, Math.min(75, camera.fov * pinch / d)); camera.updateProjectionMatrix(); aim(); }
    pinch = d;
  }
});
function up(e) {
  fingers.delete(e.pointerId);
  pinch = 0;
  if (down && down.moved < 6 && performance.now() - down.t < 500) {
    const p = pick(e.clientX, e.clientY);
    if (p) { showPoint(p); }
  }
  down = null;
}
el.addEventListener("pointerup", up);
el.addEventListener("pointercancel", (e) => { fingers.delete(e.pointerId); down = null; });
el.addEventListener("wheel", (e) => {
  if (!state.stand) { return; }
  e.preventDefault();
  camera.fov = Math.max(2, Math.min(75, camera.fov * Math.exp(e.deltaY * 0.0012)));
  camera.updateProjectionMatrix();
  aim();
}, { passive: false });
window.addEventListener("keydown", (e) => {
  if (e.target && /INPUT|SELECT|TEXTAREA/.test(e.target.tagName)) { return; }
  if (e.key === "Escape" && state.stand) { leave(); return; }
  const turn = 0.06;
  if (state.stand) {
    if (e.key === "ArrowLeft") { state.yaw -= turn; aim(); e.preventDefault(); }
    if (e.key === "ArrowRight") { state.yaw += turn; aim(); e.preventDefault(); }
    if (e.key === "ArrowUp") { state.pitch += turn / 2; aim(); e.preventDefault(); }
    if (e.key === "ArrowDown") { state.pitch -= turn / 2; aim(); e.preventDefault(); }
    return;
  }
  const off = camera.position.clone().sub(controls.target);
  const sph = new THREE.Spherical().setFromVector3(off);
  let used = true;
  if (e.key === "ArrowLeft") { sph.theta -= turn; }
  else if (e.key === "ArrowRight") { sph.theta += turn; }
  else if (e.key === "ArrowUp") { sph.phi = Math.max(0.05, sph.phi - turn); }
  else if (e.key === "ArrowDown") { sph.phi = Math.min(controls.maxPolarAngle, sph.phi + turn); }
  else if (e.key === "+" || e.key === "=") { sph.radius *= 0.85; }
  else if (e.key === "-") { sph.radius /= 0.85; }
  else { used = false; }
  if (used) { camera.position.copy(controls.target).add(new THREE.Vector3().setFromSpherical(sph)); controls.update(); state.dirty = true; e.preventDefault(); }
});
controls.addEventListener("change", () => { state.dirty = true; });

function resize() {
  const w = stage.clientWidth, h = stage.clientHeight;
  renderer.setSize(w, h, false);
  camera.aspect = w / Math.max(1, h);
  camera.updateProjectionMatrix();
  state.dirty = true;
}
window.addEventListener("resize", resize);

const needle = $("[data-needle]");
function frame() {
  requestAnimationFrame(frame);
  if (!state.stand) {
    controls.update();
    // Keep the camera out of the mountain.
    const g = groundAt(camera.position.x, camera.position.z);
    if (g !== null && camera.position.y < g + 25) { camera.position.y = g + 25; state.dirty = true; }
  }
  if (!state.dirty) { return; }
  state.dirty = false;
  sizeDot();
  declutter();
  let heading;
  if (state.stand) { heading = state.yaw; } else { const f = new THREE.Vector3(); camera.getWorldDirection(f); heading = Math.atan2(f.x, -f.z); }
  if (needle) { needle.style.transform = "rotate(" + (-heading * 180 / Math.PI).toFixed(1) + "deg)"; }
  renderer.render(scene, camera);
}

T.ready.then(() => {
  const q = new URLSearchParams(location.search);
  const want = q.get("layer");
  C.layers.forEach((l, i) => {
    const o = document.createElement("option");
    o.value = i; o.textContent = l.name;
    $("#layer").appendChild(o);
    if (want && (l.ridge.indexOf(want) >= 0 || l.close.indexOf(want) >= 0 || l.id === want)) { state.layer = i; }
  });
  $("#layer").value = state.layer;
  const today = new Date();
  state.date = today;
  $("#date").value = today.getFullYear() + "-" + String(today.getMonth() + 1).padStart(2, "0") + "-" + String(today.getDate()).padStart(2, "0");
  state.minutes = 9 * 60;
  $("#time").value = state.minutes;
  themeColours();
  resize();
  const lat = Number(q.get("lat")), lon = Number(q.get("lon"));
  const given = lat && lon;
  const inClose = (la, lo) => { const b = C.areas.close.bbox; return lo >= b.w && lo <= b.e && la >= b.s && la <= b.n; };
  setupArea(q.get("area") === "ridge" || (given && !inClose(lat, lon)) ? "ridge" : "close");
  setExag(state.exag);
  applySun();
  view("south", given ? [lat, lon] : null);
  if (given && inside(lat, lon)) {
    showPoint({ lat, lon });
    if (q.get("stand")) { stand(lat, lon); }
  }
  $("[data-loading]").textContent = "";
  frame();
}).catch((e) => { fail("The terrain data could not be loaded. " + e.message); });
