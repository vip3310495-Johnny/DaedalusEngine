// Example generator: builds .psimx schematics from compact circuit definitions (tools/examples/defs.js).
// Layout: parts on a grid; every pin gets a short wire stub ending in a net label (or a ground symbol),
// so nets are joined by name and no long wires can touch by accident.
// usage: node tools/examples/gen.js            (writes crates/daedalus-app/assets/examples, reference/examples,
//                                               crates/daedalus-app/src/examples_list.rs)
const fs = require('fs');
const path = require('path');
const ROOT = path.resolve(__dirname, '..', '..');
const LIB = JSON.parse(fs.readFileSync(path.join(ROOT, 'crates/daedalus-core/assets/library.json'), 'utf8'));
const DEF = Object.fromEntries(LIB.map(d => [d.type, d]));
const VERSION = 'V' + /version = "([^"]+)"/.exec(fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8'))[1];

function defaults(type) {
  const d = DEF[type];
  if (!d) throw new Error('unknown element type ' + type);
  const p = {};
  for (const q of d.params || []) p[q.k] = q.d;
  return p;
}
function pinsOf(type, params) {
  if (type === 'CBLOCK') {
    const n = Math.ceil(+params.nin || 0), m = Math.ceil(+params.nout || 0);
    const tn = -Math.floor((n - 1) / 2), tm = -Math.floor((m - 1) / 2);
    const out = [];
    for (let i = 0; i < n; i++) out.push({ x: -2, y: tn + i, k: 'ci', n: 'x' + (i + 1) });
    for (let i = 0; i < m; i++) out.push({ x: 2, y: tm + i, k: 'co', n: 'y' + (i + 1) });
    return out;
  }
  return DEF[type].pins;
}
function bbOf(type, params) {
  if (type === 'CBLOCK') {
    const n = Math.max(+params.nin || 0, +params.nout || 0);
    return [-2, -Math.max(1.6, n / 2 + 0.6), 2, Math.max(1.6, n / 2 + 0.6)];
  }
  return DEF[type].bb;
}

// outward stub direction of a pin (element at rot 0)
function stubDir(p, bb) {
  if (p.x === 0 && p.y === 0) return [0, 1];
  const ex = [Math.abs(p.x - bb[0]), Math.abs(p.x - bb[2]), Math.abs(p.y - bb[1]), Math.abs(p.y - bb[3])];
  const m = Math.min(...ex);
  if (m === ex[0] && p.x <= 0) return [-1, 0];
  if (m === ex[1] && p.x >= 0) return [1, 0];
  if (m === ex[2] && p.y <= 0) return [0, -1];
  if (m === ex[3] && p.y >= 0) return [0, 1];
  return Math.abs(p.x) >= Math.abs(p.y) ? [Math.sign(p.x) || 1, 0] : [0, Math.sign(p.y) || 1];
}

function build(ex) {
  const els = [], wires = [];
  let id = 1;
  const usedNames = new Set();
  const counters = {};
  const nameFor = (type, want) => {
    if (want) { usedNames.add(want); return want; }
    const pre = DEF[type].pre || type;
    let k = (counters[pre] || 0) + 1, nm;
    do { nm = pre + k; k++; } while (usedNames.has(nm));
    counters[pre] = k - 1;
    usedNames.add(nm);
    return nm;
  };
  const noname = t => (DEF[t].flags || []).includes('noname');
  const add = (type, x, y, params, name) => {
    const p = Object.assign(defaults(type), params || {});
    for (const k of Object.keys(p)) p[k] = String(p[k]);
    els.push({ id: id++, type, x, y, rot: 0, mir: false, dis: false, name: noname(type) ? '' : nameFor(type, name), params: p });
  };
  // header: description, simulation control, analysis elements
  let hx = -20;
  add('TEXT', hx, -8, { text: ex.text || ex.name });
  add('SIMCTRL', hx, -4, ex.sc);
  hx += 8;
  for (const [type, params] of ex.extra || []) { add(type, hx, -4, params); hx += 8; }
  if (ex.params) add('PARAMS', hx, -4, { text: ex.params });
  // parts on a grid
  const ROWW = 64;
  let cx = -20, cy = 8, rowH = 0;
  for (const [type, name, params, conn] of ex.parts) {
    const p = Object.assign(defaults(type), params || {});
    const bb = bbOf(type, p);
    const w = bb[2] - bb[0] + 12, h = bb[3] - bb[1] + 10;
    if (cx + w > -20 + ROWW) { cx = -20; cy += rowH; rowH = 0; }
    const x = Math.round(cx - bb[0] + 6), y = Math.round(cy - bb[1] + 4);
    add(type, x, y, params, name);
    const pins = pinsOf(type, p);
    for (const pn of pins) {
      const net = conn[pn.n];
      if (net === undefined) throw new Error(`${ex.key}: ${type} ${name || ''} pin ${pn.n} not connected`);
      if (net === null) continue; // deliberately open
      const [dx, dy] = stubDir(pn, bb);
      const px = x + pn.x, py = y + pn.y;
      const L = 2;
      const qx = px + dx * L, qy = py + dy * L;
      wires.push({ id: id++, x1: px, y1: py, x2: qx, y2: qy });
      if (net === '0') add('GND', qx, qy, {});
      else add('LABEL', qx, qy, { text: net });
    }
    for (const k of Object.keys(conn)) if (!pins.find(q => q.n === k)) throw new Error(`${ex.key}: ${type} has no pin ${k}`);
    cx += w;
    rowH = Math.max(rowH, h);
  }
  const doc = { elements: els, wires, nid: id };
  const out = { app: 'psimweb', version: VERSION, doc };
  if (ex.screens) out.screens = ex.screens;
  return out;
}

function main() {
  const defs = require('./defs.js');
  const outA = path.join(ROOT, 'crates/daedalus-app/assets/examples');
  const outR = path.join(ROOT, 'reference/examples');
  const index = JSON.parse(fs.readFileSync(path.join(outA, 'index.json'), 'utf8')).filter(e => !e.generated);
  for (const ex of defs) {
    const j = build(ex);
    const txt = JSON.stringify(j, null, 1);
    fs.writeFileSync(path.join(outA, ex.key + '.psimx'), txt);
    fs.writeFileSync(path.join(outR, ex.key + '.psimx'), txt);
    index.push({ key: ex.key, name: ex.name, cat: ex.cat, generated: true });
  }
  fs.writeFileSync(path.join(outA, 'index.json'), JSON.stringify(index, null, 1));
  fs.writeFileSync(path.join(outR, 'index.json'), JSON.stringify(index, null, 1));
  // Rust list for the app
  const cats = { buck: '電力轉換', buckac: '電力轉換', doublepulse: '功率元件', bridge: '電力轉換', rlc: '被動電路與濾波', tl431: '類比與控制 IC', supervisor: '類比與控制 IC' };
  const rs = ['// generated by tools/examples/gen.js — do not edit', 'pub const EXAMPLES: &[Example] = &['];
  for (const e of index) rs.push(`    Example { key: ${JSON.stringify(e.key)}, name: ${JSON.stringify(e.name)}, cat: ${JSON.stringify(e.cat || cats[e.key] || '其他')}, json: include_str!("../assets/examples/${e.key}.psimx") },`);
  rs.push('];', '');
  fs.writeFileSync(path.join(ROOT, 'crates/daedalus-app/src/examples_list.rs'), rs.join('\n'));
  console.log(`${defs.length} generated examples, ${index.length} in total`);
}
if (require.main === module) main();
module.exports = { build, DEF };
