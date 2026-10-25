// The route preview. Its ground is the game's own city map: the game program builds it with
// the city map's code and palette (`map_ground`, `omsi_navigator::MapGround`) and it is drawn
// here as the game draws it, with WebGL - the land, water and woods, every paved surface and
// carriageway, the tracks and the houses, their details coming and going at the city map's
// scales. Over it the trip's route in blue on the very lanes the game drives, its stops with
// their times, and the entry points - one clicked is where the bus starts. Dragged to move,
// the wheel to zoom where the mouse is, eased as the game's map eases. (Without WebGL, or a
// game program that does not build the ground, the page draws the map's layers itself.)

import { html, useEffect, useRef, useState } from '../vendor/preact-htm.js';
import { call } from './api.js';
import { t, plain } from './i18n.js';
import { Spinner } from './ui.js';

const cache = new Map();

/** The stop objects stand beside the road; their dots go onto the route itself: the nearest
 * point of the route's lanes (within 40 m - a stop the route misses stays where it is). */
function onRoute(d) {
  for (const s of d.stops) {
    let best = null;
    let bd = 40 * 40;
    for (const p of d.route) {
      for (let i = 0; i + 3 < p.length; i += 2) {
        const [ax, ay, bx, by] = [p[i], p[i + 1], p[i + 2], p[i + 3]];
        const vx = bx - ax;
        const vy = by - ay;
        const l = vx * vx + vy * vy || 1;
        const k = Math.max(0, Math.min(1, ((s.at[0] - ax) * vx + (s.at[1] - ay) * vy) / l));
        const x = ax + vx * k;
        const y = ay + vy * k;
        const dd = (x - s.at[0]) ** 2 + (y - s.at[1]) ** 2;
        if (dd < bd) (bd = dd), (best = [x, y]);
      }
    }
    if (best) s.at = best;
  }
  return d;
}



// The picture under the route is drawn into a canvas of its own, a little larger than the view,
// a slice of a few milliseconds at a time between frames: moving and zooming only scale and
// shift the last finished one, so a frame never waits for the whole city to be drawn again.
// Everything is cut into squares of the map, and only the squares in view are drawn.
const CHUNK = 1024;
const CELL = 400; // metres
const MARGIN = 1.5; // the picture's size, by the view's
const SLICE = 8; // ms of drawing between two frames

/** Triangles (x, y, x, y, x, y) in runs of CHUNK, each with its bounds. */
function tris(t) {
  const out = [];
  for (let s = 0; s + 5 < t.length; s += CHUNK * 6) {
    const e = Math.min(s + CHUNK * 6, t.length - 5);
    const lo = [Infinity, Infinity];
    const hi = [-Infinity, -Infinity];
    for (let i = s; i < e; i += 2) {
      const x = t[i];
      const y = t[i + 1];
      if (x < lo[0]) lo[0] = x;
      if (x > hi[0]) hi[0] = x;
      if (y < lo[1]) lo[1] = y;
      if (y > hi[1]) hi[1] = y;
    }
    out.push({ t, s, e, lo, hi, path: null });
  }
  return out;
}

function pathOf(c) {
  if (c.path) return c.path;
  const { t, s, e } = c;
  const path = new Path2D();
  for (let i = s; i < e; i += 6) {
    path.moveTo(t[i], t[i + 1]);
    path.lineTo(t[i + 2], t[i + 3]);
    path.lineTo(t[i + 4], t[i + 5]);
    path.closePath();
  }
  return (c.path = path);
}

/** A line (or a ring) into the Path2D of the map square it starts in (`cells`: square -> path). */
function into(cells, p, ring = false, dx = 0, dy = 0) {
  if (p.length < 2) return;
  const k = `${Math.floor(p[0] / CELL)},${Math.floor(p[1] / CELL)}`;
  let c = cells.get(k);
  if (!c) cells.set(k, (c = { path: new Path2D(), lo: [Infinity, Infinity], hi: [-Infinity, -Infinity] }));
  for (let i = 0; i + 1 < p.length; i += 2) {
    const x = p[i] + dx;
    const y = p[i + 1] + dy;
    if (i) c.path.lineTo(x, y);
    else c.path.moveTo(x, y);
    if (x < c.lo[0]) c.lo[0] = x;
    if (x > c.hi[0]) c.hi[0] = x;
    if (y < c.lo[1]) c.lo[1] = y;
    if (y > c.hi[1]) c.hi[1] = y;
  }
  if (ring) c.path.closePath();
}

// The map package's surface (`d.package`, see docs/MAP_PACKAGE.md): its triangles and lines in
// map metres, made once and drawn under a transform that follows the view.
const FILL_KEYS = ['water', 'green', 'built', 'grass', 'forest', 'sport', 'farm']; // by class
const FILL_LOOK = ['water', 'park', 'built', 'grass', 'forest', 'sport', 'field'];
const FILL_ORDER = [2, 6, 3, 1, 4, 5, 0]; // under everything: built-up land first, water last

function surfacePaths(pkg) {
  if (pkg.paths) return pkg.paths;
  const fills = pkg.fills.map(tris);
  // a river's width tells river from stream; the railway and the borders are one path each
  const lines = { river: new Path2D(), stream: new Path2D(), rail: new Path2D(), border: new Path2D() };
  for (const l of pkg.lines) {
    const path = l.c === 0 ? (l.w >= 5 ? lines.river : lines.stream) : l.c === 1 ? lines.rail : l.c === 2 ? lines.border : null;
    if (!path) continue;
    for (let i = 0; i + 1 < l.p.length; i += 2) (i ? path.lineTo : path.moveTo).call(path, l.p[i], l.p[i + 1]);
  }
  return (pkg.paths = { fills, lines });
}

/** Map metres (y up) onto the canvas: widths are then pixels times `mpp`. */
function toMap(g, v, W, H) {
  g.translate(W / 2 - v.cx / v.mpp, H / 2 + v.cy / v.mpp);
  g.scale(1 / v.mpp, -1 / v.mpp);
  g.lineCap = g.lineJoin = 'round';
}

/** The surface under the roads: ground, areas, then rivers, rail and borders. */
function* drawSurface(g, pkg, v, W, H, seen) {
  const c = LOOK === LOOKS.light && pkg.colors;
  const { fills, lines } = surfacePaths(pkg);
  g.save();
  toMap(g, v, W, H);
  for (const k of FILL_ORDER) {
    g.fillStyle = g.strokeStyle = c ? c[FILL_KEYS[k]] : LOOK[FILL_LOOK[k]];
    g.lineWidth = 0.8 * v.mpp; // (no seams between neighbouring triangles)
    for (const ch of seen(fills[k])) {
      g.fill(pathOf(ch));
      g.stroke(pathOf(ch));
      yield;
    }
  }
  const stroke = (path, color, px) => {
    g.strokeStyle = color;
    g.lineWidth = px * v.mpp;
    g.stroke(path);
  };
  stroke(lines.river, c ? c.river : LOOK.river, 6);
  stroke(lines.stream, c ? c.river : LOOK.river, 2.4);
  yield;
  stroke(lines.rail, c ? c.rail_edge : LOOK.rail, 3.4);
  stroke(lines.rail, c ? c.rail : LOOK.railDash, 1.6);
  g.globalAlpha = 0.7;
  stroke(lines.border, c ? c.border : LOOK.border, 1.2);
  g.restore();
}

const LOOKS = {
  light: {
    land: '#d8ecc8', grass: '#d8ecc8', forest: '#c3e0ae', field: '#eef0d8', paved: '#eceae4', sand: '#f3ead2', water: '#a6d3f5',
    park: '#d3eac1', built: '#efebe3', sport: '#cfe9c8', river: '#a6d3f5', border: '#b9a9c9',
    road: '#ffffff', roadEdge: '#dcd8cf', main: '#fbe08a', mainEdge: '#e5bd55', foot: '#f8f7f3', footEdge: '#e4e1d9', area: '#efedea', areaEdge: '#dedbd6',
    roof: '#e9e6df', roofEdge: '#d6d1c7', shadow: 'rgba(60,64,52,0.10)', rail: '#b2afa8', railDash: '#ffffff', tram: '#a59fb2',
    route: '#0a84ff', routeEdge: '#0060df', pin: '#ffffff', card: 'rgba(255,255,255,0.96)', cardShadow: 'rgba(0,0,0,0.16)', text: '#1c1c1e', dim: '#6e6e73', entry: '#ff9f0a', end: '#1c1c1e',
  },
  dark: {
    land: '#22322a', grass: '#22322a', forest: '#1f3527', field: '#2b2f29', paved: '#2e3033', sand: '#36332b', water: '#1d3550',
    park: '#213628', built: '#2c2d31', sport: '#22362b', river: '#1d3550', border: '#6b5d7a',
    road: '#4a4d53', roadEdge: '#323438', main: '#8a7a4e', mainEdge: '#5c5236', foot: '#34363a', footEdge: '#2c2e31', area: '#313337', areaEdge: '#2a2c2f',
    roof: '#3a3c41', roofEdge: '#46494f', shadow: 'rgba(0,0,0,0.28)', rail: '#5d5f64', railDash: '#2f3134', tram: '#6a6478',
    route: '#0a84ff', routeEdge: '#0a5bd1', pin: '#f2f2f7', card: 'rgba(44,44,46,0.96)', cardShadow: 'rgba(0,0,0,0.5)', text: '#f2f2f7', dim: '#98989d', entry: '#ff9f0a', end: '#1c1c1e',
  },
};
let LOOK = LOOKS.light;
// land classes in the order they are laid: grass, field, sand, paved, forest, water
const LAND = [[0, 'grass'], [2, 'field'], [4, 'sand'], [3, 'paved'], [1, 'forest'], [5, 'water']];

/** The layers' triangles, houses and tracks in map metres, made once. */
function layerPaths(l) {
  if (l.paths) return l.paths;
  const houses = new Map();
  const shadows = new Map();
  for (const h of l.houses) {
    into(houses, h.p, true);
    // (the shadow falls south-east, longer for a taller house)
    const k = Math.min(h.h, 40) * 0.12;
    into(shadows, h.p, true, k, -k);
  }
  const rail = new Path2D();
  const tram = new Path2D();
  for (const x of l.lines) {
    const path = x.c === 0 ? rail : x.c === 1 ? tram : null;
    if (!path) continue;
    for (let i = 0; i + 1 < x.p.length; i += 2) (i ? path.lineTo : path.moveTo).call(path, x.p[i], x.p[i + 1]);
  }
  return (l.paths = { land: l.land.map(tris), paved: l.paved.map(tris), houses: [...houses.values()], shadows: [...shadows.values()], rail, tram });
}

/** The ground, the paved surfaces, the tracks and the houses, under the roads' middles. */
function* drawLayers(g, l, v, W, H, seen) {
  const P = layerPaths(l);
  g.save();
  toMap(g, v, W, H);
  const fill = function* (chunks, color, seam = 0.8) {
    g.fillStyle = g.strokeStyle = color;
    g.lineWidth = seam * v.mpp; // (no seams between neighbouring triangles)
    for (const c of seen(chunks)) {
      g.fill(pathOf(c));
      g.stroke(pathOf(c));
      yield;
    }
  };
  for (const [k, key] of LAND) yield* fill(P.land[k], LOOK[key]);
  const near = v.mpp < 2.5;
  // pavements and yards (near), carriageways: an edge, then the surface (the edge only near: from
  // afar the roads are drawn over them with edges of their own, and edging every triangle of a
  // whole city took seconds)
  const paved = near ? [[1, 'foot', 'footEdge'], [2, 'area', 'areaEdge'], [0, 'road', 'roadEdge']] : [[2, 'area', 'areaEdge'], [0, 'road', 'roadEdge']];
  for (const [k, f, e] of paved) {
    if (near) {
      g.strokeStyle = LOOK[e];
      g.lineWidth = 2 * v.mpp;
      for (const c of seen(P.paved[k])) {
        g.stroke(pathOf(c));
        yield;
      }
    }
    yield* fill(P.paved[k], LOOK[f]);
  }
  g.strokeStyle = LOOK.rail;
  g.lineWidth = Math.max(3.4, 3.6 / v.mpp) * v.mpp;
  g.stroke(P.rail);
  if (near) {
    g.setLineDash([9, 9]);
    g.strokeStyle = LOOK.railDash;
    g.lineWidth = Math.max(1.4, 1.3 / v.mpp) * v.mpp;
    g.stroke(P.rail);
    g.setLineDash([]);
  }
  g.strokeStyle = LOOK.tram;
  g.lineWidth = 1.3 * v.mpp;
  g.stroke(P.tram);
  yield;
  if (v.mpp < 6) {
    g.fillStyle = LOOK.shadow;
    for (const c of seen(P.shadows)) g.fill(c.path);
    yield;
    g.fillStyle = LOOK.roof;
    g.strokeStyle = LOOK.roofEdge;
    g.lineWidth = 0.8 * v.mpp;
    for (const c of seen(P.houses)) {
      g.fill(c.path);
      g.stroke(c.path);
    }
    yield;
  }
  g.restore();
}

// --- The ground as the game builds it (`map_ground`)

/** The city map's ground around `at` (metres from the page's origin) at `mpp`: the head, and
 * the flat and the widened triangles' vertices (see `map_ground` in the game's launcher service). */
async function readGround(map, date, at, reach, mpp, light) {
  const buf = await call('map_ground', { map, date, x: at[0], y: at[1], reach, mpp, light });
  const n = new DataView(buf).getUint32(0, true);
  const head = JSON.parse(new TextDecoder().decode(new Uint8Array(buf, 4, n)));
  return { head, flat: buf.slice(4 + n, 4 + n + head.flat), wide: buf.slice(4 + n + head.flat) };
}

// the game's own vertex rule (omsi-ui's `ui.wgsl`): a vertex moves out along its extrusion
// by the larger of its width in metres and its width in pixels at this scale (positions come
// in steps of `step` metres, the extrusion in 1/4096, the widths in 1/256). The flat triangles,
// which have no extrusion, have a program of their own.
const VS = (wide) => `attribute vec2 p; attribute vec4 c; ${wide ? 'attribute vec2 e; attribute vec2 w;' : ''}
uniform vec2 centre; uniform float mpp; uniform vec2 hsize; uniform float step;
varying vec4 v;
void main() {
  vec2 q = p * step${wide ? ' + e / 4096.0 * max(w.x, w.y * mpp) / 256.0' : ''};
  gl_Position = vec4((q - centre) / mpp / hsize, 0.0, 1.0);
  v = c;
}`;
const FS = `precision mediump float; varying vec4 v; void main() { gl_FragColor = vec4(v.rgb * v.a, v.a); }`;
// where each attribute is, in both programs
const AT = { p: 0, c: 1, e: 2, w: 3 };

/** A WebGL picture of the ground on `canvas`; none where the browser has no WebGL. */
function groundGl(canvas) {
  const gl = canvas.getContext('webgl', { antialias: true, premultipliedAlpha: true, alpha: false });
  if (!gl) return null;
  const sh = (type, src) => {
    const o = gl.createShader(type);
    gl.shaderSource(o, src);
    gl.compileShader(o);
    return o;
  };
  const program = (wide) => {
    const prog = gl.createProgram();
    gl.attachShader(prog, sh(gl.VERTEX_SHADER, VS(wide)));
    gl.attachShader(prog, sh(gl.FRAGMENT_SHADER, FS));
    for (const [n, i] of Object.entries(AT)) gl.bindAttribLocation(prog, i, n);
    gl.linkProgram(prog);
    if (!gl.getProgramParameter(prog, gl.LINK_STATUS)) return null;
    const un = (n) => gl.getUniformLocation(prog, n);
    return { prog, u: { centre: un('centre'), mpp: un('mpp'), hsize: un('hsize'), step: un('step') } };
  };
  const progs = [program(false), program(true)];
  if (!progs[0] || !progs[1]) return null;
  return { gl, progs, bufs: [gl.createBuffer(), gl.createBuffer()], runs: [] };
}

/** Upload a ground the game built. */
function uploadGround(g, ground) {
  const { gl } = g;
  [ground.flat, ground.wide].forEach((data, i) => {
    gl.bindBuffer(gl.ARRAY_BUFFER, g.bufs[i]);
    gl.bufferData(gl.ARRAY_BUFFER, data, gl.STATIC_DRAW);
  });
  g.runs = ground.head.runs;
  g.step = ground.head.step;
  g.anchor = ground.head.anchor;
}

/** Draw the uploaded ground with the view `v` (centre and metres a pixel) on a W x H page:
 * its runs of flat and widened triangles in order, each with its own program and buffer. */
function drawGround(g, v, W, H, dpr, land) {
  const { gl } = g;
  gl.viewport(0, 0, Math.round(W * dpr), Math.round(H * dpr));
  const [r, gr, b] = land;
  gl.clearColor(r, gr, b, 1);
  gl.clear(gl.COLOR_BUFFER_BIT);
  if (!g.runs.length) return;
  gl.enable(gl.BLEND);
  gl.blendFunc(gl.ONE, gl.ONE_MINUS_SRC_ALPHA);
  const from = [0, 0];
  for (const [wide, n] of g.runs) {
    const { prog, u } = g.progs[wide];
    gl.useProgram(prog);
    gl.uniform2f(u.centre, v.cx - g.anchor[0], v.cy - g.anchor[1]);
    gl.uniform1f(u.mpp, v.mpp);
    gl.uniform2f(u.hsize, W / 2, H / 2);
    gl.uniform1f(u.step, g.step);
    const size = wide ? 16 : 8;
    gl.bindBuffer(gl.ARRAY_BUFFER, g.bufs[wide]);
    gl.enableVertexAttribArray(AT.p);
    gl.vertexAttribPointer(AT.p, 2, gl.SHORT, false, size, 0);
    gl.enableVertexAttribArray(AT.c);
    gl.vertexAttribPointer(AT.c, 4, gl.UNSIGNED_BYTE, true, size, size - 4);
    for (const [k, at] of [['e', 4], ['w', 8]]) {
      if (wide) {
        gl.enableVertexAttribArray(AT[k]);
        gl.vertexAttribPointer(AT[k], 2, gl.SHORT, false, size, at);
      } else gl.disableVertexAttribArray(AT[k]);
    }
    gl.drawArrays(gl.TRIANGLES, from[wide], n);
    from[wide] += n;
  }
}

/** A CSS colour - `#rgb`, `#rrggbb` or `rgb(a)(r,g,b,...)` - as 0..1 numbers. (The page's
 * own colours are hex; read as `rgba(...)`, `#22322a` gave pure red before the game's
 * palette came.) */
const rgb = (css) => {
  const s = (css || '').trim();
  const hex = /^#([0-9a-f]{3}|[0-9a-f]{6})$/i.exec(s);
  if (hex) {
    const h = hex[1].length === 3 ? [...hex[1]].map((c) => c + c).join('') : hex[1];
    return [0, 2, 4].map((i) => parseInt(h.slice(i, i + 2), 16) / 255);
  }
  return (s.match(/[\d.]+/g) || [0, 0, 0]).slice(0, 3).map((x) => x / 255);
};

/** Whether this browser draws WebGL (then the game builds the ground and the page needs no
 * surface of its own, unless the game cannot). */
let webglHere = null;
const hasWebgl = () => (webglHere ??= !!document.createElement('canvas').getContext('webgl'));

/** The city map's level of detail at `mpp` (`city_ground`'s scales): a new ground is built
 * when it changes. */
const detail = (mpp) => (mpp < 0.7 ? 1 : 0) | (mpp < 2.2 ? 2 : 0) | (mpp < 3.5 ? 4 : 0) | (mpp < 6 ? 8 : 0);

const surfaces = new Map();
let drawn = 0;

export async function readRouteMap(map, date, trip, surface = true, mode = '') {
  return JSON.parse(new TextDecoder().decode(await call('route_map', { map, date, trip, surface, editor: mode === 'editor', route_only: mode === 'route' })));
}

const surfaceNow = new Map();

function surfaceOf(map, date) {
  const at = `${map}|${date}`;
  if (!surfaces.has(at)) {
    surfaces.set(at, readRouteMap(map, date, '', true).then(
      (d) => {
        const s = { package: d.package, layers: d.layers };
        surfaceNow.set(at, s);
        return s;
      },
      () => (surfaces.delete(at), null),
    ));
  }
  return surfaces.get(at);
}

function load(map, date, trip, mode = '') {
  const key = `${map}|${date}|${trip}|${mode}`;
  if (!cache.has(key)) cache.set(key, readRouteMap(map, date, trip, false, mode).then(onRoute, (e) => ({ error: e.message })));
  return cache.get(key);
}

const NO_ROUTE = { route: [], stops: [] };

/** The map as the timetable editor draws it: the roads, and every stop of the timetable with its
 * place (`busstops`). The editor's own map and the page ask for the same one. */
export const editorMap = (map, date) => load(map, date, '', 'editor');

const withSurface = (d, s) => ({ ...d, package: s?.package ?? d.package, layers: s?.layers ?? d.layers, id: ++drawn });

function roadPaths(d) {
  if (d.roadPaths) return d.roadPaths;
  const groups = new Map();
  for (const r of d.roads) {
    const w = Math.round(r.w * 2) / 2;
    const k = `${r.main ? 1 : 0}|${w}`;
    if (!groups.has(k)) groups.set(k, { main: !!r.main, w, cells: new Map() });
    into(groups.get(k).cells, r.p);
  }
  return (d.roadPaths = [...groups.values()].map((g) => ({ ...g, cells: [...g.cells.values()] })));
}

function* drawRoads(g, d, v, W, H, seen) {
  const paved = !!d.layers && v.mpp < 2.5;
  g.save();
  toMap(g, v, W, H);
  for (let pass = 0; pass < 4; pass++) {
    const main = pass % 2 === 1;
    const top = pass > 1;
    const gold = main && !paved;
    g.strokeStyle = top ? (gold ? LOOK.main : LOOK.road) : gold ? LOOK.mainEdge : LOOK.roadEdge;
    for (const r of roadPaths(d)) {
      if (r.main !== main) continue;
      const w = paved ? Math.max(r.w - 1.2, 1.5) : r.w;
      g.lineWidth = Math.max((w + (top ? 0 : paved ? 0 : 1.2)) / v.mpp, top ? (main ? 2.2 : 1.4) : main ? 3.4 : 2.6) * v.mpp;
      for (const c of seen(r.cells, g.lineWidth)) g.stroke(c.path);
      yield;
    }
  }
  g.restore();
}

/** Everything under the route, for view `v` on a W x H canvas (steps: see `sliced`). */
function* drawBase(g, d, v, W, H, dpr) {
  g.setTransform(dpr, 0, 0, dpr, 0, 0);
  g.fillStyle = LOOK.land;
  g.fillRect(0, 0, W, H);
  const x0 = v.cx - (W / 2 + 4) * v.mpp;
  const x1 = v.cx + (W / 2 + 4) * v.mpp;
  const y0 = v.cy - (H / 2 + 4) * v.mpp;
  const y1 = v.cy + (H / 2 + 4) * v.mpp;
  const seen = (chunks, pad = 0) => (chunks || []).filter((c) => c.hi[0] + pad >= x0 && c.lo[0] - pad <= x1 && c.hi[1] + pad >= y0 && c.lo[1] - pad <= y1);
  if (d.package) yield* drawSurface(g, d.package, v, W, H, seen);
  if (d.layers) yield* drawLayers(g, d.layers, v, W, H, seen);
  yield* drawRoads(g, d, v, W, H, seen);
}

/** Run a drawing a slice at a time, one slice a frame; `done` when it is finished. Returns
 * what stops it. */
function sliced(steps, look, done) {
  let stopped = false;
  const step = () => {
    if (stopped) return;
    const end = performance.now() + SLICE;
    const was = LOOK;
    LOOK = look;
    try {
      while (performance.now() < end) {
        if (steps.next().done) {
          stopped = true;
          done();
          return;
        }
      }
    } finally {
      LOOK = was;
    }
    // (a hidden window gets no frames: the timer goes on then)
    let next = 0;
    const go = () => {
      if (next++) return;
      setTimeout(step, 0);
    };
    requestAnimationFrame(go);
    setTimeout(go, 100);
  };
  setTimeout(step, 0);
  return () => (stopped = true);
}

/** The route in map metres, made once. */
function routePath(d) {
  if (d.routePath) return d.routePath;
  const path = new Path2D();
  for (const p of d.route) for (let i = 0; i + 1 < p.length; i += 2) (i ? path.lineTo : path.moveTo).call(path, p[i], p[i + 1]);
  return (d.routePath = path);
}

/** The view that shows `d`: the trip's route when there is one, else the map, framed in the
 * part of the W x H page `inset` leaves free. */
function fit(d, W, H, inset, el) {
  let lo = d.lo;
  let hi = d.hi;
  if (d.route.length) {
    lo = [Infinity, Infinity];
    hi = [-Infinity, -Infinity];
    for (const p of d.route) for (let i = 0; i < p.length; i += 2) {
      lo = [Math.min(lo[0], p[i]), Math.min(lo[1], p[i + 1])];
      hi = [Math.max(hi[0], p[i]), Math.max(hi[1], p[i + 1])];
    }
  }
  const k = inset?.(el) || { l: 0, t: 0, r: 0, b: 0 };
  const fw = Math.max(W - k.l - k.r, W / 3);
  const fh = Math.max(H - k.t - k.b, H / 3);
  const span = Math.max((hi[0] - lo[0]) / Math.max(fw - (inset ? 220 : 80), fw / 2), (hi[1] - lo[1]) / Math.max(fh - (inset ? 90 : 60), fh / 2), 0.2);
  const ox = Math.min(k.l, W - fw) + fw / 2 - W / 2;
  const oy = Math.min(k.t, H - fh) + fh / 2 - H / 2;
  return { cx: (lo[0] + hi[0]) / 2 - ox * span, cy: (lo[1] + hi[1]) / 2 + oy * span, mpp: span };
}

/** The timetable editor's stops over the map: every stop a small dot (a depot a square), the stops
 * the next click may go to ringed, the trip under way numbered in order, and the names where
 * there is room for them. `editor`: { chain: [ids], reach: Set | null, depots: Set }. */
function drawEditorStops(g, d, editor, hover, X, Y, W, H, v, taken, free, P) {
  const chain = new Map((editor.chain || []).map((id, i) => [id, i]));
  const reach = editor.reach || null;
  const depots = editor.depots || new Set();
  const dim = !!reach && reach.size > 0;
  g.font = '600 11.5px Inter, sans-serif';
  const label = (x, y, text, strong) => {
    const w = g.measureText(text).width + 14;
    const lx = [x + 11, x - 11 - w].find((lx) => free(lx, y - 10, w));
    if (lx === undefined) return;
    taken.push([lx, y - 10, w]);
    g.save();
    g.shadowColor = LOOK.cardShadow;
    g.shadowBlur = 6;
    g.fillStyle = P?.card || LOOK.card;
    g.beginPath();
    g.roundRect(lx, y - 10, w, 20, 7);
    g.fill();
    g.restore();
    g.fillStyle = strong ? LOOK.route : P?.card_text || LOOK.text;
    g.fillText(text, lx + 7, y + 4);
  };
  const shown = [];
  d.busstops.forEach((b, i) => {
    const x = X(b.at[0]);
    const y = Y(b.at[1]);
    if (x < -20 || x > W + 20 || y < -20 || y > H + 20) return;
    shown.push([b, i, x, y]);
  });
  // (the plain ones first, then the depots, the reachable ones, the chosen ones over them)
  const rank = ([b, i]) => (chain.has(b.id) ? 4 : reach?.has(b.id) ? 3 : i === hover ? 2 : depots.has(b.id) ? 1 : 0);
  shown.sort((a, b) => rank(a) - rank(b));
  const small = v.mpp > 14;
  for (const [b, i, x, y] of shown) {
    const k = chain.get(b.id);
    const r = rank([b, i]);
    if (k !== undefined) {
      g.fillStyle = LOOK.route;
      g.beginPath();
      g.arc(x, y, 9, 0, Math.PI * 2);
      g.fill();
      g.strokeStyle = LOOK.pin;
      g.lineWidth = 2;
      g.stroke();
      g.fillStyle = '#ffffff';
      g.textAlign = 'center';
      g.fillText(String(k + 1), x, y + 4);
      g.textAlign = 'left';
    } else if (depots.has(b.id)) {
      const s = i === hover ? 11 : 9;
      g.fillStyle = LOOK.entry;
      g.beginPath();
      g.roundRect(x - s / 2, y - s / 2, s, s, 2.5);
      g.fill();
      g.strokeStyle = LOOK.pin;
      g.lineWidth = 1.6;
      g.stroke();
    } else {
      g.fillStyle = LOOK.pin;
      g.globalAlpha = dim && r < 3 && i !== hover ? 0.55 : 1;
      g.beginPath();
      g.arc(x, y, i === hover ? 6 : small ? 2 : 3.4, 0, Math.PI * 2);
      g.fill();
      g.strokeStyle = 'rgba(20,22,26,0.55)';
      g.lineWidth = 1;
      g.stroke();
      g.globalAlpha = 1;
    }
    if (r === 3 && k === undefined) {
      g.strokeStyle = LOOK.route;
      g.lineWidth = 2.4;
      g.beginPath();
      g.arc(x, y, 9, 0, Math.PI * 2);
      g.stroke();
    }
  }
  // names: the ones that matter first (the trip's, the reachable, the hovered), then the rest
  // as far as there is room, close enough to read
  const named = [...shown].sort((a, b) => rank(b) - rank(a));
  for (const [b, i, x, y] of named) {
    const r = rank([b, i]);
    if (r === 0 && v.mpp > 3.2) continue;
    if (r === 1 && v.mpp > 9) continue;
    label(x, y, plain(b.name), r >= 3 || i === hover);
  }
}

const MIN_MPP = 0.15;
const MAX_MPP = 200;

export function RouteMap({ map, date, trip, stops = [], entry = -1, onEntry, height = 340, inset, editor }) {
  const box = useRef();
  const cv = useRef();
  const gcv = useRef();
  const [data, setData] = useState(null);
  // the view drawn now, and the one it eases to (the game's map eases its zoom the same way)
  const view = useRef(null);
  // the finished pictures: of the view (`base`) and of the whole map (`whole`, for what lies
  // beyond the view's picture while it is moved or zoomed out), and the one being drawn
  const base = useRef(null);
  const whole = useRef(null);
  const job = useRef(null);
  const spare = useRef(null);
  const busy = useRef(false);
  // the view being eased to, a new trip's flight to it pending, and the easing's frame
  const aim = useRef(null);
  const refit = useRef(false);
  const glide = useRef(0);
  const settle = useRef(0);
  const frame = useRef(0);
  const [hover, setHover] = useState(-1);
  // the timetable editor: the route of the trip (or of the stops being chosen) comes apart from
  // the map, which is read once, and the stop under the pointer
  const [rt, setRt] = useState(null);
  const [hoverStop, setHoverStop] = useState(-1);
  const ed = useRef(editor);
  ed.current = editor;
  // the ground the game built (null: not asked yet, false: this browser or game cannot)
  const gl = useRef(null);
  const ground = useRef({ built: null, asking: null, failed: false });

  useEffect(() => {
    if (!map) return;
    let alive = true;
    // (the surface only where the page draws the map itself: WebGL draws the game's ground)
    const surface = hasWebgl() ? null : surfaceOf(map, date);
    load(map, date, editor ? '' : trip, editor ? 'editor' : '').then((d) => {
      if (!alive) return;
      const now = surfaceNow.get(`${map}|${date}`);
      setData(d.error ? d : withSurface(d, now));
      // (a new trip: the view flies to it rather than jumping)
      refit.current = true;
      if (d.error || now || !surface) return;
      surface.then((s) => alive && s && setData((cur) => (cur && !cur.error ? withSurface(cur, s) : cur)));
    });
    return () => (alive = false);
  }, [map, date, editor ? '' : trip, !!editor]);

  // (the editor's route: lighter, and asked for again at every change of the trip)
  useEffect(() => {
    if (!editor || !map) return undefined;
    if (!trip) {
      setRt(NO_ROUTE);
      return undefined;
    }
    let alive = true;
    load(map, date, trip, 'route').then((r) => {
      if (!alive) return;
      setRt(r.error ? NO_ROUTE : r);
      if (!r.error && r.route?.length && ed.current?.fit !== false) refit.current = true;
    });
    return () => (alive = false);
  }, [map, date, trip, !!editor]);

  /** Start drawing the picture `v` needs, unless it is there or under way. */
  const refresh = (d, v, W, H, dpr) => {
    const key = `${d.id}|${LOOK === LOOKS.light}|${W}x${H}@${dpr}`;
    const same = (x) => x && x.key === key && x.view.cx === v.cx && x.view.cy === v.cy && x.view.mpp === v.mpp;
    if (same(base.current)) return wholeMap(d, W, H);
    const j = job.current;
    // (a picture of another zoom is no use any more; one of another place still is, for a while)
    if (j && (j.key !== key || j.view.mpp !== v.mpp)) {
      j.stop();
      job.current = null;
      if (j.key === 'whole') whole.current = { ...whole.current, making: null };
    }
    if (busy.current || job.current) return;
    const BW = Math.round(W * MARGIN);
    const BH = Math.round(H * MARGIN);
    const canvas = spare.current || document.createElement('canvas');
    spare.current = null;
    Object.assign(canvas, { width: Math.round(BW * dpr), height: Math.round(BH * dpr) });
    const at = { ...v };
    const look = LOOK;
    const made = { canvas, key, view: at, W: BW, H: BH, map };
    job.current = {
      key,
      view: at,
      stop: sliced(drawBase(canvas.getContext('2d'), d, at, BW, BH, dpr), look, () => {
        job.current = null;
        if (base.current && base.current.canvas !== whole.current?.canvas) spare.current = base.current.canvas;
        base.current = made;
        request();
      }),
    };
  };

  /** The whole map, once, at a scale that fits it in about 1600 pixels. */
  const wholeMap = (d, W, H) => {
    const key = `${d.id}|${LOOK === LOOKS.light}`;
    if (whole.current?.key === key || whole.current?.making === key || job.current) return;
    const span = Math.max(d.hi[0] - d.lo[0], d.hi[1] - d.lo[1], 1) * 1.2;
    const mpp = span / 1600;
    const OW = Math.ceil((d.hi[0] - d.lo[0]) * 1.2 / mpp) + 2;
    const OH = Math.ceil((d.hi[1] - d.lo[1]) * 1.2 / mpp) + 2;
    const canvas = document.createElement('canvas');
    Object.assign(canvas, { width: OW, height: OH });
    const at = { cx: (d.lo[0] + d.hi[0]) / 2, cy: (d.lo[1] + d.hi[1]) / 2, mpp };
    whole.current = { ...whole.current, making: key };
    job.current = {
      key: 'whole',
      view: { mpp: NaN },
      stop: sliced(drawBase(canvas.getContext('2d'), d, at, OW, OH, 1), LOOK, () => {
        job.current = null;
        whole.current = { canvas, key, view: at, W: OW, H: OH, map };
        request();
      }),
    };
  };

  // a new map: its own ground
  useEffect(() => {
    ground.current = { built: null, asking: null, failed: false };
    if (gl.current) gl.current.runs = [];
  }, [map, date]);

  /** Ask the game for the ground around the view when the one there is does not cover it,
   * or is of another level of detail - as the city map asks for its own. */
  const wantGround = (v, W, H, light) => {
    const G = ground.current;
    if (G.failed || G.asking || !map) return;
    const reach = Math.max(500, Math.max(W, H) * v.mpp * 0.8);
    const b = G.built;
    const fresh = b && b.light === light && detail(b.mpp) === detail(v.mpp) && Math.hypot(b.at[0] - v.cx, b.at[1] - v.cy) <= b.reach * 0.35 && b.mpp / v.mpp <= 1.7 && v.mpp / b.mpp <= 1.7;
    // (while it moves, the ground there is kept as long as it covers the view)
    const covers = b && b.light === light && Math.abs(b.at[0] - v.cx) + (W / 2) * v.mpp <= b.reach && Math.abs(b.at[1] - v.cy) + (H / 2) * v.mpp <= b.reach;
    if (fresh || (busy.current && covers)) return;
    const ask = { at: [v.cx, v.cy], reach, mpp: v.mpp, light };
    G.asking = ask;
    readGround(map, date, ask.at, reach, v.mpp, light).then(
      (got) => {
        if (ground.current !== G) return;
        G.asking = null;
        if (gl.current) uploadGround(gl.current, got);
        G.built = ask;
        G.palette = got.head.palette;
        latest.current();
      },
      () => {
        if (ground.current !== G) return;
        G.asking = null;
        G.failed = true;
        latest.current();
        // (the page draws the map itself now: with the map's surface too)
        surfaceOf(map, date).then((s) => s && setData((cur) => (cur && !cur.error ? withSurface(cur, s) : cur)));
      },
    );
  };

  // the picture: the game's ground (or, without it, the finished pictures scaled to the
  // view), the route, the stops and the entry points over them
  const draw = () => {
    const c = cv.current;
    const d = data;
    if (!c || !d || d.error) return;
    const dpr = window.devicePixelRatio || 1;
    const light = document.documentElement.dataset.theme === 'light';
    LOOK = light ? LOOKS.light : LOOKS.dark;
    // (the timetable editor draws the route it is asked for, which is not the map's own)
    const R = editor ? rt || NO_ROUTE : d;
    const W = box.current.clientWidth;
    const H = box.current.clientHeight;
    if (!W || !H) return;
    for (const el of [c, gcv.current]) {
      if (el && (el.width !== Math.round(W * dpr) || el.height !== Math.round(H * dpr))) Object.assign(el, { width: Math.round(W * dpr), height: Math.round(H * dpr) });
    }
    if (!view.current) {
      view.current = fit(editor ? { lo: d.lo, hi: d.hi, route: R.route } : d, W, H, inset, box.current);
      aim.current = { ...view.current };
      refit.current = false;
    } else if (refit.current) {
      refit.current = false;
      aim.current = fit(editor ? { lo: d.lo, hi: d.hi, route: R.route } : d, W, H, inset, box.current);
      aim.current.zoomAt = null;
      animate();
    }
    const v = view.current;
    if (gl.current === null && gcv.current) gl.current = groundGl(gcv.current) || false;
    const G = ground.current;
    const onGl = !!gl.current && !G.failed;
    // (until the game's ground has come, the page's own picture stands in for it)
    const groundUp = onGl && !!G.built;
    const g = c.getContext('2d');
    g.setTransform(dpr, 0, 0, dpr, 0, 0);
    if (onGl) {
      wantGround(v, W, H, LOOK === LOOKS.light);
      drawGround(gl.current, v, W, H, dpr, rgb(G.palette?.land || LOOK.land));
    }
    if (groundUp) {
      g.clearRect(0, 0, W, H);
    } else {
      refresh(d, v, W, H, dpr);
      g.fillStyle = LOOK.land;
      g.fillRect(0, 0, W, H);
      for (const b of [whole.current, base.current]) {
        if (!b?.canvas || b.map !== map) continue;
        const k = b.view.mpp / v.mpp;
        g.drawImage(b.canvas, W / 2 + (b.view.cx - v.cx) / v.mpp - (b.W / 2) * k, H / 2 - (b.view.cy - v.cy) / v.mpp - (b.H / 2) * k, b.W * k, b.H * k);
      }
    }
    // the route and the cards in the game's colours where it gave them
    const P = (groundUp && G.palette) || null;
    const routeColor = P?.route || LOOK.route;
    const X = (x) => W / 2 + (x - v.cx) / v.mpp;
    const Y = (y) => H / 2 - (y - v.cy) / v.mpp;
    g.save();
    toMap(g, v, W, H);
    for (const [color, wide] of [[P?.route_edge || LOOK.routeEdge, 7], [routeColor, 4.6]]) {
      g.strokeStyle = color;
      g.lineWidth = wide * v.mpp;
      g.stroke(routePath(R));
    }
    g.restore();
    g.lineCap = g.lineJoin = 'round';
    // stops: white pins with the bus's blue, a card with the time and name beside
    g.font = '600 11.5px Inter, sans-serif';
    // (a label that would cover one already written goes to the left of its stop, else away)
    const taken = [];
    const free = (x, y, w) => !taken.some((r) => x < r[0] + r[2] && x + w > r[0] && y < r[1] + 20 && y + 20 > r[1]);
    for (const s of R.stops) {
      const x = X(s.at[0]);
      const y = Y(s.at[1]);
      if (x < -300 || x > W + 300 || y < -30 || y > H + 30) continue;
      const end = stops.length && (s.k === 0 || s.k === stops.length - 1);
      g.fillStyle = LOOK.pin;
      g.beginPath();
      g.arc(x, y, end ? 7 : 5.5, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = end ? LOOK.end : routeColor;
      g.beginPath();
      g.arc(x, y, end ? 5 : 3.6, 0, Math.PI * 2);
      g.fill();
      if (end && s.k === 0) {
        g.fillStyle = LOOK.pin;
        g.beginPath();
        g.arc(x, y, 2.4, 0, Math.PI * 2);
        g.fill();
      }
      const st = stops[s.k];
      if (st) {
        const label = `${st.time}  ${st.name}`;
        const w = g.measureText(label).width + 14;
        const lx = [x + 10, x - 10 - w].find((lx) => free(lx, y - 10, w));
        if (lx === undefined) continue;
        taken.push([lx, y - 10, w]);
        g.save();
        g.shadowColor = LOOK.cardShadow;
        g.shadowBlur = 6;
        g.fillStyle = P?.card || LOOK.card;
        g.beginPath();
        g.roundRect(lx, y - 10, w, 20, 7);
        g.fill();
        g.restore();
        g.fillStyle = routeColor;
        g.fillText(plain(st.time), lx + 7, y + 4);
        g.fillStyle = P?.card_text || LOOK.text;
        g.fillText(plain(st.name), lx + 7 + g.measureText(`${st.time}  `).width, y + 4);
      }
    }
    if (editor && d.busstops) drawEditorStops(g, d, editor, hoverStop, X, Y, W, H, v, taken, free, P);
    // entry points: amber, the hovered one ringed grey, the chosen one white
    (editor ? [] : d.entries).forEach((e, i) => {
      const x = X(e.at[0]);
      const y = Y(e.at[1]);
      if (x < -20 || x > W + 20 || y < -20 || y > H + 20) return;
      g.fillStyle = LOOK.pin;
      g.beginPath();
      g.arc(x, y, 5.5, 0, Math.PI * 2);
      g.fill();
      g.fillStyle = LOOK.entry;
      g.beginPath();
      g.arc(x, y, 3.8, 0, Math.PI * 2);
      g.fill();
      if (i === hover || e.index === entry) {
        g.strokeStyle = e.index === entry ? routeColor : '#8e8e93';
        g.lineWidth = 2;
        g.beginPath();
        g.arc(x, y, e.index === entry ? 10 : 7.5, 0, Math.PI * 2);
        g.stroke();
      }
    });
    if (hover >= 0 && d.entries[hover]) {
      const e = d.entries[hover];
      g.font = '500 12px Inter, sans-serif';
      const w = g.measureText(e.name).width + 12;
      g.fillStyle = 'rgba(28,30,33,0.94)';
      g.beginPath();
      g.roundRect(X(e.at[0]) - w / 2, Y(e.at[1]) - 34, w, 22, 8);
      g.fill();
      g.fillStyle = '#ffffff';
      g.fillText(plain(e.name), X(e.at[0]) - w / 2 + 6, Y(e.at[1]) - 19);
    }
  };
  const latest = useRef(draw);
  latest.current = draw;
  /** Ease the view to `aim` frame by frame (a zoom keeps the point under the mouse where it
   * is, as the city map's wheel does), then draw it once more. */
  const animate = () => {
    if (glide.current) return;
    let last = performance.now();
    const step = (now) => {
      const v = view.current;
      const a = aim.current;
      if (!v || !a) return (glide.current = 0);
      const k = 1 - Math.exp(-Math.min(now - last, 100) / 1000 * 14);
      last = now;
      const mpp = Math.exp(Math.log(v.mpp) + (Math.log(a.mpp) - Math.log(v.mpp)) * k);
      if (a.zoomAt) {
        const { px, py, wx, wy, W, H } = a.zoomAt;
        v.mpp = mpp;
        v.cx = wx - (px - W / 2) * mpp;
        v.cy = wy + (py - H / 2) * mpp;
      } else {
        v.mpp = mpp;
        v.cx += (a.cx - v.cx) * k;
        v.cy += (a.cy - v.cy) * k;
      }
      const done = Math.abs(Math.log(v.mpp / a.mpp)) < 0.002 && (a.zoomAt || Math.hypot(a.cx - v.cx, a.cy - v.cy) < v.mpp * 0.3);
      if (done) {
        v.mpp = a.mpp;
        if (!a.zoomAt) [v.cx, v.cy] = [a.cx, a.cy];
        glide.current = 0;
        request();
        return;
      }
      // (moving: no new picture of the map is started until the view stands still)
      moving();
      glide.current = requestAnimationFrame(step);
    };
    glide.current = requestAnimationFrame(step);
  };

  /** Draw on the next frame (once, however many moves come before it). */
  const request = () => {
    if (frame.current) return;
    const go = () => {
      if (!frame.current) return;
      frame.current = 0;
      latest.current();
    };
    frame.current = requestAnimationFrame(go);
    // (a hidden window gets no frames)
    setTimeout(go, 100);
  };
  /** Moving or zooming: no new picture is started until it has stopped for a moment. */
  const moving = () => {
    busy.current = true;
    clearTimeout(settle.current);
    settle.current = setTimeout(() => {
      busy.current = !!drag.current;
      request();
    }, 150);
    request();
  };
  useEffect(() => request(), [data, rt, hover, hoverStop, editor, entry, stops.map((x) => `${x.time} ${x.name}`).join('|')]);
  useEffect(() => () => {
    clearTimeout(settle.current);
    cancelAnimationFrame(frame.current);
    cancelAnimationFrame(glide.current);
    job.current?.stop();
  }, []);
  // a lost WebGL context (a GPU reset, too many pages open): the page draws the map itself
  // until the context comes back, then the ground is asked for and built again
  useEffect(() => {
    const el = gcv.current;
    if (!el) return undefined;
    const lost = (e) => {
      e.preventDefault();
      gl.current = false;
      latest.current();
    };
    const back = () => {
      gl.current = null;
      ground.current = { built: null, asking: null, failed: false };
      latest.current();
    };
    el.addEventListener('webglcontextlost', lost);
    el.addEventListener('webglcontextrestored', back);
    return () => {
      el.removeEventListener('webglcontextlost', lost);
      el.removeEventListener('webglcontextrestored', back);
    };
  }, []);
  useEffect(() => {
    const redraw = () => latest.current();
    const ro = new ResizeObserver(redraw);
    ro.observe(box.current);
    document.fonts?.ready.then(redraw);
    const theme = new MutationObserver(redraw);
    theme.observe(document.documentElement, { attributes: true, attributeFilter: ['data-theme'] });
    const scale = matchMedia(`(resolution: ${window.devicePixelRatio || 1}dppx)`);
    scale.addEventListener('change', redraw);
    return () => {
      ro.disconnect();
      theme.disconnect();
      scale.removeEventListener('change', redraw);
    };
  }, []);

  const at = (e) => {
    const r = cv.current.getBoundingClientRect();
    return [e.clientX - r.left, e.clientY - r.top];
  };
  const entryAt = ([px, py]) => {
    const d = data;
    const v = view.current;
    if (!d?.entries || !v) return -1;
    const W = box.current.clientWidth;
    const H = box.current.clientHeight;
    let best = -1;
    let bd = 12 * 12;
    d.entries.forEach((e, i) => {
      const dx = W / 2 + (e.at[0] - v.cx) / v.mpp - px;
      const dy = H / 2 - (e.at[1] - v.cy) / v.mpp - py;
      if (dx * dx + dy * dy < bd) (bd = dx * dx + dy * dy), (best = i);
    });
    return best;
  };
  /** The timetable stop under a pointer position (within 10 pixels), as its place in the list. */
  const stopAt = ([px, py]) => {
    const d = data;
    const v = view.current;
    if (!d?.busstops || !v) return -1;
    const W = box.current.clientWidth;
    const H = box.current.clientHeight;
    let best = -1;
    let bd = 11 * 11;
    d.busstops.forEach((b, i) => {
      const dx = W / 2 + (b.at[0] - v.cx) / v.mpp - px;
      const dy = H / 2 - (b.at[1] - v.cy) / v.mpp - py;
      if (dx * dx + dy * dy < bd) (bd = dx * dx + dy * dy), (best = i);
    });
    return best;
  };
  // the editor sends the view somewhere (a stop chosen from a list): it flies there
  useEffect(() => {
    const f = editor?.focus;
    if (!f || !view.current) return;
    aim.current = { cx: f.x, cy: f.y, mpp: f.mpp ?? Math.min(view.current.mpp, 2.5), zoomAt: null };
    animate();
  }, [editor?.focus]);
  const drag = useRef(null);
  return html`<div class="routemap" ref=${box} style=${(height ? `height:${height}px;` : '') + (hoverStop >= 0 ? 'cursor:pointer' : '')}
      onPointerDown=${(e) => {
        drag.current = { p: at(e), moved: 0 };
        // (a drag takes the map where it is: an easing in progress stops there)
        cancelAnimationFrame(glide.current);
        glide.current = 0;
        if (view.current) aim.current = { ...view.current, zoomAt: null };
        e.currentTarget.setPointerCapture(e.pointerId);
      }}
      onPointerMove=${(e) => {
        const p = at(e);
        const d = drag.current;
        if (d && view.current) {
          const dx = p[0] - d.p[0];
          const dy = p[1] - d.p[1];
          d.moved += Math.abs(dx) + Math.abs(dy);
          d.p = p;
          view.current.cx -= dx * view.current.mpp;
          view.current.cy += dy * view.current.mpp;
          aim.current = { ...view.current, zoomAt: null };
          moving();
        } else if (ed.current) {
          const h = stopAt(p);
          if (h !== hoverStop) setHoverStop(h);
        } else {
          const h = entryAt(p);
          if (h !== hover) setHover(h);
        }
      }}
      onPointerUp=${(e) => {
        const d = drag.current;
        drag.current = null;
        if (d?.moved) {
          clearTimeout(settle.current);
          busy.current = false;
          request();
        }
        if (d && d.moved < 5) {
          if (ed.current) {
            const i = stopAt(at(e));
            if (i >= 0) ed.current.onStop?.(data.busstops[i].id, e);
          } else {
            const i = entryAt(at(e));
            if (i >= 0) onEntry?.(data.entries[i].index);
          }
        }
      }}
      onPointerLeave=${() => (setHover(-1), setHoverStop(-1))}
      onWheel=${(e) => {
        e.preventDefault();
        const v = view.current;
        if (!v) return;
        const [px, py] = at(e);
        const W = box.current.clientWidth;
        const H = box.current.clientHeight;
        // the point under the mouse stays there; a trackpad's small steps zoom little
        const a = aim.current?.zoomAt ? aim.current : { ...v };
        const wx = v.cx + (px - W / 2) * v.mpp;
        const wy = v.cy - (py - H / 2) * v.mpp;
        const k = Math.exp(Math.max(-1, Math.min(1, e.deltaY * (e.deltaMode ? 0.05 : 0.0025))) * 0.6);
        aim.current = { cx: v.cx, cy: v.cy, mpp: Math.min(Math.max(a.mpp * k, MIN_MPP), MAX_MPP), zoomAt: { px, py, wx, wy, W, H } };
        animate();
      }}>
    <canvas ref=${gcv} class="ground"></canvas>
    <canvas ref=${cv} class="over"></canvas>
    ${!data && html`<div class="state"><${Spinner} /> ${t('Reading the map…')}</div>`}
    ${data?.error && html`<div class="state">${data.error}</div>`}
    ${(data?.credit || data?.package?.credit) && html`<div class="credit">${data.credit || data.package.credit}</div>`}
    ${data && !data.error && !editor && html`<div class="legend">${t('{roads} roads | {stops} stops | {entries} entry points', { roads: data.roads.length, stops: data.stops.length, entries: data.entries.length })}</div>`}
  </div>`;
}
