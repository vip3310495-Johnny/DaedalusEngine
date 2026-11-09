/* =====================================================================
 * 電路模擬器 — schematic editor
 * Original author: Johnny.ch.Tsai
 * ===================================================================== */
'use strict';
// pristine page source (captured before init mutates the DOM) for the pop-out waveform window
const BOOT_HTML = (() => { try { return document.documentElement.outerHTML; } catch (e) { return ''; } })();
const APP = { version: 'V3.2.1', author: 'Johnny.ch.Tsai', contributors: '' };
const $ = (s, r) => (r || document).querySelector(s);
const $$ = (s, r) => Array.from((r || document).querySelectorAll(s));
const esc = s => String(s == null ? '' : s).replace(/[&<>"']/g, c => ({ '&': '&amp;', '<': '&lt;', '>': '&gt;', '"': '&quot;', "'": '&#39;' }[c]));
function h(tag, attrs, ...kids) {
  const e = document.createElement(tag);
  if (attrs) for (const k in attrs) { const v = attrs[k]; if (v == null || v === false) continue; if (k === 'class') e.className = v; else if (k.startsWith('on')) e.addEventListener(k.slice(2), v); else if (k === 'html') e.innerHTML = v; else e.setAttribute(k, v === true ? '' : v); }
  for (const k of kids.flat()) if (k != null && k !== false) e.append(k.nodeType ? k : document.createTextNode(String(k)));
  return e;
}
function siFmt(v, unit, dig) {
  if (v === 0) return '0' + (unit ? ' ' + unit : '');
  if (!isFinite(v)) return String(v);
  const a = Math.abs(v); const P = [[1e9, 'G'], [1e6, 'M'], [1e3, 'k'], [1, ''], [1e-3, 'm'], [1e-6, 'µ'], [1e-9, 'n'], [1e-12, 'p'], [1e-15, 'f']];
  for (const [m, s] of P) if (a >= m * 0.9999999) return +(v / m).toPrecision(dig || 4) + ' ' + s + (unit || '');
  return v.toExponential(3) + (unit ? ' ' + unit : '');
}
const store = {
  get(k) { try { return localStorage.getItem(k); } catch (e) { return null; } },
  set(k, v) { try { localStorage.setItem(k, v); } catch (e) { /* storage unavailable */ } }
};

/* ------------------------------------------------------------------
 * Geometry
 * ------------------------------------------------------------------ */
function tpt(el, x, y) {
  if (el.mir) x = -x;
  switch (el.rot & 3) { case 0: return [el.x + x, el.y + y]; case 1: return [el.x - y, el.y + x]; case 2: return [el.x - x, el.y - y]; default: return [el.x + y, el.y - x]; }
}
const defOf = el => LIB.D[el.type];
const pinsOf = el => defOf(el).pinsFor(el);
function wpins(el) { return pinsOf(el).map(p => { const [x, y] = tpt(el, p.x, p.y); return { x, y, k: p.k, n: p.n }; }); }
function wbox(el) {
  const d = defOf(el); let [x0, y0, x1, y1] = d.bb;
  if (d.type === 'LABEL') x1 = Math.max(1.6, 0.62 * String(el.params.text || '').length + 0.9);
  if (d.type === 'TEXT') { const lines = String(el.params.text || '').split('\n'); x1 = Math.max(1, Math.max.apply(null, lines.map(l => l.length)) * 0.62 + 0.4); y1 = 0.4 + (lines.length - 1) * 1.1; }
  if (d.type === 'CBLOCK') { const n = +el.params.nin || 1, m = +el.params.nout || 1; const hh = Math.max(n, m) / 2 + 0.4; y0 = -hh; y1 = hh; }
  const c = [tpt(el, x0, y0), tpt(el, x1, y0), tpt(el, x0, y1), tpt(el, x1, y1)];
  return { x0: Math.min(...c.map(p => p[0])), y0: Math.min(...c.map(p => p[1])), x1: Math.max(...c.map(p => p[0])), y1: Math.max(...c.map(p => p[1])) };
}
function onlyCtl(el, c) {
  const ps = wpins(el); const pk = pinsOf(el); if (!ps.length || !pk.some(p => p.k === 'a')) return false;
  let any = false;
  for (let i = 0; i < ps.length; i++) { const r = c.find(c.key(ps[i].x, ps[i].y)); if (c.isCtlRoot(r)) { any = true; continue; } if (c.isGndRoot(r)) continue; return false; }
  return any;
}
const isCtlEl = el => { const ps = pinsOf(el); return ps.length > 0 && ps.every(p => p.k === 'ci' || p.k === 'co' || p.k === 'g'); };

/* ------------------------------------------------------------------
 * Document state, undo, autosave
 * ------------------------------------------------------------------ */
const ED = {
  doc: { elements: [], wires: [], nid: 1 },
  sel: new Set(), mode: 'select', ghost: null, wire: null, drag: null, band: null, pan: null,
  vb: { x: -10, y: -8, w: 60, h: 36 }, mouse: [0, 0], undo: [], redo: [], dirty: false,
  errEls: new Set(), conn: null, libSel: 'Elements/Power/RLC Branches', lastRes: null, lastAC: null, fileName: '未命名電路',
  pinMarks: store.get('psimweb.pinMarks') !== '0', pinTol: 0
};
function snapshot() { return JSON.stringify(ED.doc); }
function pushUndo() { ED.undo.push(snapshot()); if (ED.undo.length > 120) ED.undo.shift(); ED.redo = []; }
function changed() { ED.dirty = true; ED.conn = null; ED.errEls.clear(); render(); saveLocal(); }
let saveTimer = 0;
function saveLocal() { clearTimeout(saveTimer); saveTimer = setTimeout(() => store.set('psimweb.autosave', JSON.stringify({ doc: ED.doc, name: ED.fileName })), 400); }
function undo() { if (!ED.undo.length) return; ED.redo.push(snapshot()); ED.doc = JSON.parse(ED.undo.pop()); ED.sel.clear(); changed(); }
function redo() { if (!ED.redo.length) return; ED.undo.push(snapshot()); ED.doc = JSON.parse(ED.redo.pop()); ED.sel.clear(); changed(); }
const elById = id => ED.doc.elements.find(e => e.id === id);
const wireById = id => ED.doc.wires.find(w => w.id === id);

function defaultParams(type) { const o = {}; LIB.D[type].params.forEach(p => { o[p.k] = p.d; }); return o; }
function uniqueName(pre) {
  const used = new Set(ED.doc.elements.map(e => e.name));
  for (let i = 1; ; i++) { const n = pre + i; if (!used.has(n)) return n; }
}
function newElement(type, x, y, rot, mir) {
  const d = LIB.D[type];
  const el = { id: ED.doc.nid++, type, x, y, rot: rot || 0, mir: !!mir, dis: false, name: d.noname ? '' : uniqueName(d.pre), params: defaultParams(type) };
  if (type === 'ACSWEEP') el.name = 'ACSWEEP';
  if (type === 'SIMCTRL') el.name = 'Simulation Control';
  return el;
}
function addWire(x1, y1, x2, y2) { if (x1 === x2 && y1 === y2) return null; const w = { id: ED.doc.nid++, x1, y1, x2, y2 }; ED.doc.wires.push(w); return w; }

/* ------------------------------------------------------------------
 * Connectivity (nets, junctions, open pins)
 * ------------------------------------------------------------------ */
function connectivity() {
  if (ED.conn) return ED.conn;
  const doc = ED.doc; const key = (x, y) => x + ',' + y;
  const par = new Map(); const find = k => { let r = k; while (par.get(r) !== r) r = par.get(r); let c = k; while (par.get(c) !== r) { const n = par.get(c); par.set(c, r); c = n; } return r; };
  const add = k => { if (!par.has(k)) par.set(k, k); };
  const uni = (a, b) => { add(a); add(b); const ra = find(a), rb = find(b); if (ra !== rb) par.set(ra, rb); };
  const cnt = new Map(); const bump = k => cnt.set(k, (cnt.get(k) || 0) + 1);
  const pts = []; // all connection points
  const elPins = [];
  for (const el of doc.elements) {
    const wp = wpins(el); elPins.push({ el, wp });
    wp.forEach(p => { const k = key(p.x, p.y); add(k); bump(k); pts.push([p.x, p.y]); });
  }
  for (const w of doc.wires) { const a = key(w.x1, w.y1), b = key(w.x2, w.y2); uni(a, b); bump(a); bump(b); pts.push([w.x1, w.y1], [w.x2, w.y2]); }
  // T junctions: a point lying strictly inside a wire segment
  for (const w of doc.wires) {
    const minx = Math.min(w.x1, w.x2), maxx = Math.max(w.x1, w.x2), miny = Math.min(w.y1, w.y2), maxy = Math.max(w.y1, w.y2);
    for (const [x, y] of pts) {
      if ((x === w.x1 && y === w.y1) || (x === w.x2 && y === w.y2)) continue;
      if (w.y1 === w.y2 ? (y === w.y1 && x > minx && x < maxx) : (x === w.x1 && y > miny && y < maxy)) {
        const k = key(x, y); if (!w._tj) w._tj = new Set(); if (!w._tj.has(k)) { w._tj.add(k); uni(k, key(w.x1, w.y1)); bump(k); bump(k); }
      }
    }
    delete w._tj;
  }
  // labels
  const lbl = new Map();
  for (const { el, wp } of elPins) if (el.type === 'LABEL' && !el.dis) { const t = String(el.params.text || '').trim(); if (!t) continue; const k = key(wp[0].x, wp[0].y); if (lbl.has(t)) uni(k, lbl.get(t)); else lbl.set(t, k); }
  const junc = []; const seen = new Set();
  for (const [k, c] of cnt) if (c >= 3 && !seen.has(k)) { seen.add(k); const [x, y] = k.split(',').map(Number); junc.push([x, y]); }
  // net membership counts (for open pins)
  const members = new Map();
  for (const { el, wp } of elPins) wp.forEach(p => { const r = find(key(p.x, p.y)); members.set(r, (members.get(r) || 0) + 1); });
  for (const w of doc.wires) { const r = find(key(w.x1, w.y1)); members.set(r, (members.get(r) || 0) + 1); }
  const open = [];
  for (const { el, wp } of elPins) { if (el.type === 'LABEL' || el.type === 'GND') continue; wp.forEach(p => { if ((members.get(find(key(p.x, p.y))) || 0) <= 1) open.push([p.x, p.y]); }); }
  const kinds = new Map();
  for (const { el, wp } of elPins) { if (el.dis) continue; wp.forEach(p => { const r = find(key(p.x, p.y)); let s = kinds.get(r); if (!s) kinds.set(r, s = new Set()); s.add(p.k); }); }
  const isCtlRoot = r => { const s = kinds.get(r); return !!s && !s.has('p') && !s.has('gnd') && (s.has('ci') || s.has('co') || s.has('g')); };
  const isGndRoot = r => { const s = kinds.get(r); return !!s && s.has('gnd'); };
  ED.conn = { find, key, junc, open, elPins, isCtlRoot, isGndRoot };
  return ED.conn;
}

/* ------------------------------------------------------------------
 * Rendering
 * ------------------------------------------------------------------ */
const SVGNS = 'http://www.w3.org/2000/svg';
let svg, gGrid, gWires, gEls, gTop;
function initSvg() {
  svg = $('#sch');
  svg.innerHTML = `<defs><pattern id="gp" width="1" height="1" patternUnits="userSpaceOnUse"><circle cx="0" cy="0" r="0.055" fill="var(--grid)"/><circle cx="1" cy="0" r="0.055" fill="var(--grid)"/><circle cx="0" cy="1" r="0.055" fill="var(--grid)"/><circle cx="1" cy="1" r="0.055" fill="var(--grid)"/></pattern>
  <pattern id="gp5" width="5" height="5" patternUnits="userSpaceOnUse"><circle cx="0" cy="0" r="0.16" fill="var(--grid)"/><circle cx="5" cy="0" r="0.16" fill="var(--grid)"/><circle cx="0" cy="5" r="0.16" fill="var(--grid)"/><circle cx="5" cy="5" r="0.16" fill="var(--grid)"/></pattern></defs>
  <rect id="gridRect" fill="url(#gp)"/><g id="gW"></g><g id="gE"></g><g id="gT"></g>`;
  gGrid = $('#gridRect'); gWires = $('#gW'); gEls = $('#gE'); gTop = $('#gT');
}
function applyView() {
  const r = svg.getBoundingClientRect(); const vb = ED.vb;
  vb.h = vb.w * (r.height / Math.max(1, r.width));
  svg.setAttribute('viewBox', `${vb.x} ${vb.y} ${vb.w} ${vb.h}`);
  const ppu = r.width / vb.w;
  gGrid.setAttribute('fill', ppu < 7 ? 'url(#gp5)' : 'url(#gp)');
  gGrid.setAttribute('x', Math.floor(vb.x) - 1); gGrid.setAttribute('y', Math.floor(vb.y) - 1); gGrid.setAttribute('width', vb.w + 3); gGrid.setAttribute('height', vb.h + 3);
}
function symMarkup(el, cls) {
  const d = defOf(el); let s;
  try { s = d.sym(el); } catch (e) { s = { g: '' }; }
  const tr = `translate(${el.x} ${el.y}) rotate(${(el.rot & 3) * 90}) scale(${el.mir ? -1 : 1} 1)`;
  let out = `<g class="sym ${cls}" transform="${tr}">${s.g}</g>`;
  // texts drawn upright
  const dx = tpt(el, 1, 0); const dirX = dx[0] - el.x;
  (s.t || []).forEach(t => {
    const yc = t.y - t.sz * 0.36;
    const [x, y] = tpt(el, t.x, yc);
    let a = t.a; if (a !== 'middle') { if (dirX < 0) a = a === 'start' ? 'end' : 'start'; else if (dirX === 0) a = 'middle'; }
    out += `<text class="it${cls.includes('ctl') ? ' ctl' : ''}" x="${x}" y="${y}" font-size="${t.sz}" text-anchor="${a}" dominant-baseline="central"${cls.includes('dis') ? ' opacity=".5"' : ''}>${esc(t.s)}</text>`;
  });
  return out;
}
function labelLines(el) {
  const d = defOf(el); const lines = [];
  const disp = el.disp || {};
  if (!d.noname && (disp.name !== false) && el.name) lines.push(el.name);
  const showSet = new Set(d.show || []);
  d.params.forEach(p => { const on = disp[p.k] !== undefined ? disp[p.k] : showSet.has(p.k); if (on) lines.push(p.k + '=' + el.params[p.k] + (p.u && /^[-+0-9.eE]+$/.test(String(el.params[p.k])) ? p.u : '')); });
  return lines;
}
const N3 = v => +v.toFixed(3);
const PIN_KIND = { p: '功率', ci: '控制輸入', g: '閘極（控制輸入）', co: '控制輸出', a: '功率或控制', gnd: '接地', lbl: '節點標籤' };
// inward unit direction (local coordinates) of a pin: from the nearest edge of the symbol box towards the body
function pinDir(d, el, p) {
  let [x0, y0, x1, y1] = d.bb;
  if (d.type === 'CBLOCK') { const n = +el.params.nin || 1, m = +el.params.nout || 1; const hh = Math.max(n, m) / 2 + 0.4; y0 = -hh; y1 = hh; }
  const e = 1e-6; let dx = 0, dy = 0;
  if (p.x <= x0 + e) dx = 1; else if (p.x >= x1 - e) dx = -1; else if (p.y <= y0 + e) dy = 1; else if (p.y >= y1 - e) dy = -1;
  else { const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2; if (Math.abs(p.x - cx) >= Math.abs(p.y - cy)) dx = p.x < cx ? 1 : -1; else dy = p.y < cy ? 1 : -1; }
  return [dx, dy];
}
// pin-type markers drawn on each lead: ■ power, ▶ control input (pointing in), ▶ control output (pointing out), ◇ either
function pinMarks(el, cls) {
  const d = defOf(el); if (d.label || d.text) return '';
  let s = '';
  for (const p of pinsOf(el)) {
    if (p.k === 'gnd' || p.k === 'lbl') continue;
    const [dx, dy] = pinDir(d, el, p);
    const [px, py] = tpt(el, p.x, p.y), [qx, qy] = tpt(el, p.x + dx, p.y + dy); const ux = qx - px, uy = qy - py;
    const cx = px + ux * 0.42, cy = py + uy * 0.42;
    if (p.k === 'p') s += `<rect class="pm pw" x="${N3(cx - 0.11)}" y="${N3(cy - 0.11)}" width="0.22" height="0.22"/>`;
    else if (p.k === 'a') s += `<path class="pm pa" d="M${N3(cx)} ${N3(cy - 0.15)}L${N3(cx + 0.15)} ${N3(cy)}L${N3(cx)} ${N3(cy + 0.15)}L${N3(cx - 0.15)} ${N3(cy)}Z"/>`;
    else {
      const o = p.k === 'co' ? -1 : 1, vx = ux * o, vy = uy * o;
      s += `<path class="pm pc" d="M${N3(cx + vx * 0.16)} ${N3(cy + vy * 0.16)}L${N3(cx - vx * 0.16 - vy * 0.15)} ${N3(cy - vy * 0.16 + vx * 0.15)}L${N3(cx - vx * 0.16 + vy * 0.15)} ${N3(cy - vy * 0.16 - vx * 0.15)}Z"/>`;
    }
  }
  return s ? `<g class="pms${cls ? ' ' + cls : ''}">${s}</g>` : '';
}
// node label: flag + text, following the element's rotation/mirror (text stays readable)
function labelMarkup(el, ghost) {
  const t = String(el.params.text || ''); const wd = Math.max(1.6, 0.62 * t.length + 0.9);
  const pts = [[0, 0], [0.5, -0.55], [wd, -0.55], [wd, 0.55], [0.5, 0.55]].map(([a, b]) => tpt(el, a, b));
  const d = 'M' + pts.map(q => N3(q[0]) + ' ' + N3(q[1])).join('L') + 'Z';
  const [cx, cy] = tpt(el, (0.5 + wd) / 2 + 0.05, 0); const q = tpt(el, 1, 0); const uy = q[1] - el.y;
  const ang = uy > 0.5 ? 90 : (uy < -0.5 ? -90 : 0);
  return `<path${ghost ? ' fill="none"' : ' class="lblflag"'} d="${d}"/><text${ghost ? '' : ' class="lbltxt"'} x="${N3(cx)}" y="${N3(cy)}" font-size="0.85" text-anchor="middle" dominant-baseline="central"${ang ? ` transform="rotate(${ang} ${N3(cx)} ${N3(cy)})"` : ''}>${esc(t)}</text>`;
}
let labelRects = []; // clickable name / parameter texts of the last render (world coordinates)
function labelKeys(el) {
  const d = defOf(el); const disp = el.disp || {}; const keys = [];
  if (!d.noname && (disp.name !== false) && el.name) keys.push('__name');
  const showSet = new Set(d.show || []);
  d.params.forEach(p => { const on = disp[p.k] !== undefined ? disp[p.k] : showSet.has(p.k); if (on) keys.push(p.k); });
  return keys;
}
function hitLabel(x, y) { for (let i = labelRects.length - 1; i >= 0; i--) { const r = labelRects[i]; if (x >= r.x0 && x <= r.x1 && y >= r.y0 && y <= r.y1) return r; } return null; }
function render() {
  if (!svg) return;
  applyView(); labelRects = [];
  const doc = ED.doc; const c = connectivity();
  // wires
  let w = '';
  for (const wi of doc.wires) {
    const sel = ED.sel.has('w:' + wi.id);
    const ctl = c.isCtlRoot(c.find(c.key(wi.x1, wi.y1)));
    w += `<line class="wire${sel ? ' sel' : ''}${ctl ? ' ctl' : ''}" x1="${wi.x1}" y1="${wi.y1}" x2="${wi.x2}" y2="${wi.y2}"/>`;
  }
  c.junc.forEach(([x, y]) => { const ctl = c.isCtlRoot(c.find(c.key(x, y))); w += `<circle class="jdot" cx="${x}" cy="${y}" r="0.2"${ctl ? ' style="fill:var(--ctl)"' : ''}/>`; });
  gWires.innerHTML = w;
  // elements
  let e = '';
  for (const el of doc.elements) {
    const d = defOf(el); const sel = ED.sel.has('e:' + el.id);
    const cls = (isCtlEl(el) || onlyCtl(el, c) ? 'ctl ' : '') + (el.dis ? 'dis ' : '') + (sel ? 'selx ' : '') + (ED.errEls.has(el.id) ? 'errx' : '');
    if (d.label) {
      e += `<g class="${el.dis ? 'sym dis' : ''}">${labelMarkup(el, false)}</g>`;
      if (findHit(el)) { const b = wbox(el); e += `<rect class="findhit" x="${b.x0 - 0.45}" y="${b.y0 - 0.45}" width="${b.x1 - b.x0 + 0.9}" height="${b.y1 - b.y0 + 0.9}" rx="0.3"/>`; }
      if (sel) { const b = wbox(el); e += `<rect class="hl" x="${b.x0 - 0.3}" y="${b.y0 - 0.3}" width="${b.x1 - b.x0 + 0.6}" height="${b.y1 - b.y0 + 0.6}"/>`; }
      continue;
    }
    if (d.text) {
      const lines = String(el.params.text || '').split('\n');
      e += lines.map((l, i) => `<text x="${el.x}" y="${el.y + i * 1.1}" font-size="0.9" fill="${sel ? 'var(--sel)' : 'var(--muted)'}">${esc(l)}</text>`).join('');
      if (sel) { const b = wbox(el); e += `<rect class="hl" x="${b.x0 - 0.3}" y="${b.y0 - 0.3}" width="${b.x1 - b.x0 + 0.6}" height="${b.y1 - b.y0 + 0.6}"/>`; }
      continue;
    }
    e += symMarkup(el, cls);
    if (ED.pinMarks) e += pinMarks(el, el.dis ? 'dis' : '');
    const b = wbox(el);
    if (findHit(el)) e += `<rect class="findhit" x="${b.x0 - 0.45}" y="${b.y0 - 0.45}" width="${b.x1 - b.x0 + 0.9}" height="${b.y1 - b.y0 + 0.9}" rx="0.3"/>`;
    const lines = labelLines(el), keys = labelKeys(el);
    if (lines.length) {
      const wide = (b.x1 - b.x0) >= (b.y1 - b.y0);
      const fs = 0.78, lh = 0.95;
      const off = d.lblRight && !(el.rot & 1) ? 0.5 : 0; // symbols with a top pin: keep the name clear of the lead
      const hot = ED.hotLabel && ED.hotLabel.id === el.id ? ED.hotLabel.key : null;
      lines.forEach((l, i) => {
        let x, y, anchor;
        if (wide) { x = (b.x0 + b.x1) / 2 + off; y = b.y0 - 0.35 - (lines.length - 1 - i) * lh; anchor = off ? 'start' : 'middle'; }
        else { x = b.x1 + 0.35; y = (b.y0 + b.y1) / 2 - (lines.length - 1) * lh / 2 + i * lh; anchor = 'start'; }
        const wd = 0.52 * l.length + 0.3, x0 = anchor === 'middle' ? x - wd / 2 : x;
        labelRects.push({ id: el.id, key: keys[i], x0, x1: x0 + wd, y0: y - (wide ? 0.75 : 0.45), y1: y + (wide ? 0.15 : 0.45) });
        e += `<text class="nm${hot === keys[i] ? ' hot' : ''}" x="${x}" y="${y}" font-size="${fs}" text-anchor="${anchor}"${wide ? '' : ' dominant-baseline="central"'}${el.dis ? ' opacity=".5"' : ''}>${esc(l)}</text>`;
      });
    }
    if (sel) e += `<rect class="hl" x="${b.x0 - 0.3}" y="${b.y0 - 0.3}" width="${b.x1 - b.x0 + 0.6}" height="${b.y1 - b.y0 + 0.6}"/>`;
    if (ED.errEls.has(el.id)) e += `<rect class="errbox" x="${b.x0 - 0.4}" y="${b.y0 - 0.4}" width="${b.x1 - b.x0 + 0.8}" height="${b.y1 - b.y0 + 0.8}" rx="0.3"/>`;
  }
  c.open.forEach(([x, y]) => { e += `<circle class="open" cx="${x}" cy="${y}" r="0.2"/>`; });
  gEls.innerHTML = e;
  renderTop();
  updateStatus();
  updateWelcome();
}
// start panel on an empty sheet
function updateWelcome() {
  const w = $('#welcome'); if (!w) return;
  const empty = !ED.doc.elements.some(el => el.type !== 'SIMCTRL') && !ED.doc.wires.length;
  w.hidden = !empty || ED.mode === 'place' || ED.mode === 'paste';
}
function renderTop() {
  let t = '';
  const [mx, my] = ED.mouse;
  if (ED.ghost && ED.ghostOn) {
    if (ED.ghost.type) { const el = Object.assign({}, ED.ghost, { x: mx, y: my, params: ED.ghost.params || defaultParams(ED.ghost.type), name: '' }); t += `<g class="ghost">${ghostMarkup(el)}</g>`; }
    else if (ED.ghost.group) {
      const g = ED.ghost.group; let s = '';
      g.elements.forEach(el => { s += ghostMarkup(Object.assign({}, el, { x: el.x + mx, y: el.y + my })); });
      g.wires.forEach(w => { s += `<line class="wprev" x1="${w.x1 + mx}" y1="${w.y1 + my}" x2="${w.x2 + mx}" y2="${w.y2 + my}"/>`; });
      t += `<g class="ghost">${s}</g>`;
    }
  }
  if (ED.wire) {
    const pts = ED.wire.pts.concat(lPath(ED.wire.pts[ED.wire.pts.length - 1], [mx, my]).slice(1));
    t += `<polyline class="wprev" points="${pts.map(p => p.join(',')).join(' ')}"/>`;
  }
  if (ED.mode === 'wire' || ED.wire) t += `<circle class="snap" cx="${mx}" cy="${my}" r="0.32"/>`;
  else if (ED.hoverPin) t += `<circle class="pinhov" cx="${ED.hoverPin.x}" cy="${ED.hoverPin.y}" r="0.42"/>`;
  if (ED.band) { const b = ED.band; t += `<rect class="selbox" x="${Math.min(b.x0, b.x1)}" y="${Math.min(b.y0, b.y1)}" width="${Math.abs(b.x1 - b.x0)}" height="${Math.abs(b.y1 - b.y0)}"/>`; }
  gTop.innerHTML = t;
}
function ghostMarkup(el) {
  const d = defOf(el);
  if (d.label) return labelMarkup(el, true);
  if (d.text) return `<text x="${el.x}" y="${el.y}" font-size="0.9">${esc(el.params.text)}</text>`;
  return symMarkup(el, '') + (ED.pinMarks ? pinMarks(el, '') : '');
}
function lPath(a, b) {
  if (a[0] === b[0] || a[1] === b[1]) return [a, b];
  const hFirst = Math.abs(b[0] - a[0]) >= Math.abs(b[1] - a[1]);
  return hFirst ? [a, [b[0], a[1]], b] : [a, [a[0], b[1]], b];
}

/* ------------------------------------------------------------------
 * View helpers
 * ------------------------------------------------------------------ */
function toWorld(ev) {
  const r = svg.getBoundingClientRect(); const vb = ED.vb;
  return [vb.x + (ev.clientX - r.left) * vb.w / r.width, vb.y + (ev.clientY - r.top) * vb.h / r.height];
}
const snapv = v => Math.round(v);
function zoomAt(f, wx, wy) {
  const vb = ED.vb; const nw = Math.min(600, Math.max(6, vb.w * f)); const k = nw / vb.w;
  vb.x = wx - (wx - vb.x) * k; vb.y = wy - (wy - vb.y) * k; vb.w = nw; render();
}
function fitView() {
  const els = ED.doc.elements; if (!els.length && !ED.doc.wires.length) { ED.vb = { x: -10, y: -8, w: 60, h: 36 }; render(); return; }
  let x0 = Infinity, y0 = Infinity, x1 = -Infinity, y1 = -Infinity;
  els.forEach(el => { const b = wbox(el); x0 = Math.min(x0, b.x0); y0 = Math.min(y0, b.y0 - 2); x1 = Math.max(x1, b.x1 + 3); y1 = Math.max(y1, b.y1); });
  ED.doc.wires.forEach(w => { x0 = Math.min(x0, w.x1, w.x2); x1 = Math.max(x1, w.x1, w.x2); y0 = Math.min(y0, w.y1, w.y2); y1 = Math.max(y1, w.y1, w.y2); });
  const r = svg.getBoundingClientRect(); const asp = r.height / Math.max(1, r.width);
  let w = Math.max(48, (x1 - x0) + 6), hh = (y1 - y0) + 6; if (hh / asp > w) w = hh / asp;
  ED.vb = { x: (x0 + x1) / 2 - w / 2, y: (y0 + y1) / 2 - w * asp / 2, w, h: w * asp };
  render();
}

/* ------------------------------------------------------------------
 * Hit testing
 * ------------------------------------------------------------------ */
function hitElement(x, y) {
  const els = ED.doc.elements;
  for (let i = els.length - 1; i >= 0; i--) { const b = wbox(els[i]); if (x >= b.x0 - 0.25 && x <= b.x1 + 0.25 && y >= b.y0 - 0.25 && y <= b.y1 + 0.25) return els[i]; }
  return null;
}
function hitWire(x, y, tol) {
  tol = tol || ED.pinTol || 0.35; const ws = ED.doc.wires;
  for (let i = ws.length - 1; i >= 0; i--) {
    const w = ws[i]; const minx = Math.min(w.x1, w.x2) - tol, maxx = Math.max(w.x1, w.x2) + tol, miny = Math.min(w.y1, w.y2) - tol, maxy = Math.max(w.y1, w.y2) + tol;
    if (x >= minx && x <= maxx && y >= miny && y <= maxy) return w;
  }
  return null;
}
function nearPin(x, y) {
  const tol = ED.pinTol || 0.35; let best = null, bd = Infinity;
  for (const el of ED.doc.elements) for (const p of wpins(el)) { const dd = Math.max(Math.abs(p.x - x), Math.abs(p.y - y)); if (dd < tol && dd < bd) { bd = dd; best = { el, p }; } }
  return best;
}
function isConnPoint(x, y) {
  for (const el of ED.doc.elements) for (const p of wpins(el)) if (p.x === x && p.y === y) return true;
  for (const w of ED.doc.wires) {
    if ((w.x1 === x && w.y1 === y) || (w.x2 === x && w.y2 === y)) return true;
    if (w.y1 === w.y2 && y === w.y1 && x > Math.min(w.x1, w.x2) && x < Math.max(w.x1, w.x2)) return true;
    if (w.x1 === w.x2 && x === w.x1 && y > Math.min(w.y1, w.y2) && y < Math.max(w.y1, w.y2)) return true;
  }
  return false;
}

/* ------------------------------------------------------------------
 * Editing operations
 * ------------------------------------------------------------------ */
function selectedEls() { return ED.doc.elements.filter(e => ED.sel.has('e:' + e.id)); }
function selectedWires() { return ED.doc.wires.filter(w => ED.sel.has('w:' + w.id)); }
function rotateSel(dir) {
  if (ED.ghost) { if (ED.ghost.type) ED.ghost.rot = (ED.ghost.rot + (dir || 1) + 4) & 3; else rotGroup(ED.ghost.group, dir); renderTop(); return; }
  const els = selectedEls(); const ws = selectedWires(); if (!els.length && !ws.length) return;
  pushUndo();
  if (els.length === 1 && !ws.length) { els[0].rot = (els[0].rot + (dir || 1) + 4) & 3; }
  else {
    // rotate group about its center (snapped)
    const pts = []; els.forEach(e => pts.push([e.x, e.y])); ws.forEach(w => pts.push([w.x1, w.y1], [w.x2, w.y2]));
    const cx = Math.round(pts.reduce((s, p) => s + p[0], 0) / pts.length), cy = Math.round(pts.reduce((s, p) => s + p[1], 0) / pts.length);
    const R = (x, y) => (dir || 1) > 0 ? [cx - (y - cy), cy + (x - cx)] : [cx + (y - cy), cy - (x - cx)];
    els.forEach(e => { [e.x, e.y] = R(e.x, e.y); e.rot = (e.rot + (dir || 1) + 4) & 3; });
    ws.forEach(w => { [w.x1, w.y1] = R(w.x1, w.y1); [w.x2, w.y2] = R(w.x2, w.y2); });
  }
  changed();
}
function rotGroup(g, dir) {
  const R = (x, y) => (dir || 1) > 0 ? [-y, x] : [y, -x];
  g.elements.forEach(e => { [e.x, e.y] = R(e.x, e.y); e.rot = (e.rot + (dir || 1) + 4) & 3; });
  g.wires.forEach(w => { [w.x1, w.y1] = R(w.x1, w.y1); [w.x2, w.y2] = R(w.x2, w.y2); });
}
// vertical: false = flip left/right, true = flip top/bottom
function mirrorSel(vertical) {
  const flipEl = e => { e.mir = !e.mir; if (vertical) e.rot = (e.rot + 2) & 3; };
  if (ED.ghost) { if (ED.ghost.type) flipEl(ED.ghost); else { ED.ghost.group.elements.forEach(e => { flipEl(e); if (vertical) e.y = -e.y; else e.x = -e.x; }); ED.ghost.group.wires.forEach(w => { if (vertical) { w.y1 = -w.y1; w.y2 = -w.y2; } else { w.x1 = -w.x1; w.x2 = -w.x2; } }); } renderTop(); return; }
  const els = selectedEls(); if (!els.length) return;
  pushUndo(); els.forEach(flipEl); changed();
}
function toggleDisable() {
  if (ED.ghost && ED.ghost.type) { ED.ghost.dis = !ED.ghost.dis; renderTop(); return; }
  const els = selectedEls(); if (!els.length) return;
  pushUndo(); const to = !els.every(e => e.dis); els.forEach(e => { e.dis = to; }); changed();
  toast(to ? '已停用 ' + els.length + ' 個元件（不參與模擬）' : '已啟用 ' + els.length + ' 個元件');
}
function deleteSel() {
  if (!ED.sel.size) return;
  pushUndo();
  const ne = ED.doc.elements.length, nw = ED.doc.wires.length;
  ED.doc.elements = ED.doc.elements.filter(e => !ED.sel.has('e:' + e.id));
  ED.doc.wires = ED.doc.wires.filter(w => !ED.sel.has('w:' + w.id));
  ED.sel.clear(); changed();
  toast('已刪除 ' + (ne - ED.doc.elements.length) + ' 個元件、' + (nw - ED.doc.wires.length) + ' 條導線（Ctrl+Z 可復原）');
}
let CLIP = null;
function copySel(cut) {
  const els = selectedEls(), ws = selectedWires(); if (!els.length && !ws.length) return;
  const pts = []; els.forEach(e => pts.push([e.x, e.y])); ws.forEach(w => pts.push([w.x1, w.y1]));
  const cx = Math.round(Math.min(...pts.map(p => p[0]))), cy = Math.round(Math.min(...pts.map(p => p[1])));
  CLIP = { elements: els.map(e => Object.assign(JSON.parse(JSON.stringify(e)), { x: e.x - cx, y: e.y - cy })), wires: ws.map(w => ({ x1: w.x1 - cx, y1: w.y1 - cy, x2: w.x2 - cx, y2: w.y2 - cy })) };
  if (cut) deleteSel(); else toast('已複製 ' + els.length + ' 個元件、' + ws.length + ' 條導線');
}
function pasteClip() {
  if (!CLIP) return;
  setMode('paste'); ED.ghost = { group: JSON.parse(JSON.stringify(CLIP)) }; ED.ghostOn = true; renderTop();
}
function placeGroup(g, mx, my) {
  pushUndo(); ED.sel.clear();
  g.elements.forEach(e0 => {
    const el = JSON.parse(JSON.stringify(e0)); el.id = ED.doc.nid++; el.x += mx; el.y += my;
    const d = LIB.D[el.type]; if (!d.noname && el.type !== 'SIMCTRL' && el.type !== 'ACSWEEP') { if (ED.doc.elements.some(o => o.name === el.name)) el.name = uniqueName(d.pre); }
    if (d.single && ED.doc.elements.some(o => o.type === el.type)) return;
    ED.doc.elements.push(el); ED.sel.add('e:' + el.id);
  });
  g.wires.forEach(w => { const nw = addWire(w.x1 + mx, w.y1 + my, w.x2 + mx, w.y2 + my); if (nw) ED.sel.add('w:' + nw.id); });
  changed();
}
function placeElement(proto, x, y) {
  const d = LIB.D[proto.type];
  if (d.single && ED.doc.elements.some(e => e.type === proto.type)) { toast('每個電路只能有一個「' + d.name + '」元件'); return null; }
  pushUndo();
  const el = newElement(proto.type, x, y, proto.rot, proto.mir); el.dis = !!proto.dis;
  if (proto.params) el.params = Object.assign(el.params, proto.params);
  ED.doc.elements.push(el); ED.sel.clear(); ED.sel.add('e:' + el.id); changed();
  return el;
}
// split wires so that their endpoints touch moved pins (rubber banding)
function moveSelection(dx, dy, base) {
  const doc = ED.doc;
  const movedPins = new Set(); const key = (x, y) => x + ',' + y;
  base.elements.forEach(e0 => { if (!ED.sel.has('e:' + e0.id)) return; const e = Object.assign({}, e0); wpins(e).forEach(p => movedPins.add(key(p.x, p.y))); });
  base.wires.forEach(w => { if (ED.sel.has('w:' + w.id)) { movedPins.add(key(w.x1, w.y1)); movedPins.add(key(w.x2, w.y2)); } });
  doc.elements = base.elements.map(e0 => { const e = Object.assign({}, e0); if (ED.sel.has('e:' + e.id)) { e.x += dx; e.y += dy; } return e; });
  const out = [];
  base.wires.forEach(w0 => {
    const w = Object.assign({}, w0);
    if (ED.sel.has('w:' + w.id)) { w.x1 += dx; w.x2 += dx; w.y1 += dy; w.y2 += dy; out.push(w); return; }
    const m1 = movedPins.has(key(w.x1, w.y1)), m2 = movedPins.has(key(w.x2, w.y2));
    if (!m1 && !m2) { out.push(w); return; }
    const horiz = w0.y1 === w0.y2;
    let a = [w.x1, w.y1], b = [w.x2, w.y2];
    if (m1) a = [a[0] + dx, a[1] + dy]; if (m2) b = [b[0] + dx, b[1] + dy];
    if (a[0] === b[0] || a[1] === b[1]) { out.push(Object.assign(w, { x1: a[0], y1: a[1], x2: b[0], y2: b[1] })); return; }
    // keep the fixed end's direction, add a corner
    const fixed = m1 ? b : a, mov = m1 ? a : b;
    const corner = horiz ? [mov[0], fixed[1]] : [fixed[0], mov[1]];
    if (m1 && m2) { out.push(Object.assign(w, { x1: a[0], y1: a[1], x2: b[0], y2: b[1] })); return; }
    out.push(Object.assign(w, { x1: fixed[0], y1: fixed[1], x2: corner[0], y2: corner[1] }));
    out.push({ id: w.id + 0.5, x1: corner[0], y1: corner[1], x2: mov[0], y2: mov[1], _tmp: true });
  });
  doc.wires = out;
}
function finalizeWires() {
  let nid = ED.doc.nid;
  ED.doc.wires = ED.doc.wires.filter(w => !(w.x1 === w.x2 && w.y1 === w.y2)).map(w => { if (w._tmp || w.id % 1) { const n = { id: nid++, x1: w.x1, y1: w.y1, x2: w.x2, y2: w.y2 }; return n; } return w; });
  ED.doc.nid = nid;
}

/* ------------------------------------------------------------------
 * Modes and pointer handling
 * ------------------------------------------------------------------ */
function setMode(m, ghost) {
  ED.mode = m; ED.wire = null; ED.band = null;
  ED.ghost = ghost || null; ED.ghostOn = !!ghost;
  $$('.tb[data-mode]').forEach(b => b.classList.toggle('on', b.dataset.mode === m));
  svg.style.cursor = m === 'wire' ? 'crosshair' : (m === 'place' || m === 'paste' ? 'copy' : 'default');
  updateHint(); renderTop();
}
function startPlace(type) {
  const d = LIB.D[type]; if (!d) return;
  noteRecent(type);
  if (d.single && ED.doc.elements.some(e => e.type === type)) { const el = ED.doc.elements.find(e => e.type === type); ED.sel.clear(); ED.sel.add('e:' + el.id); render(); openProps(el); return; }
  setMode('place', { type, rot: 0, mir: false, dis: false, params: defaultParams(type) });
}
function noteRecent(type) {
  const d = LIB.D[type]; if (!d || d.single || d.nosim && !d.probe && type !== 'LABEL' && type !== 'GND') { /* keep */ }
  let r = []; try { r = JSON.parse(store.get('psimweb.recent') || '[]'); } catch (e) { r = []; }
  r = [type].concat(r.filter(t => t !== type)).slice(0, 8); store.set('psimweb.recent', JSON.stringify(r));
  if (typeof refreshQuick === 'function') refreshQuick();
}
function updateHint() {
  const m = { select: '點選元件或導線，拖曳框選；雙擊元件開啟屬性、雙擊空白處新增元件；點名稱或參數文字可直接修改；從接腳拖曳可拉線。', place: '左鍵放置元件　Space 旋轉　X 左右鏡像　Y 上下鏡像　D 停用/啟用　Esc 取消', paste: '左鍵貼上　Space 旋轉　Esc 取消', wire: '左鍵開始/轉折，點到接腳或導線即完成；雙擊或右鍵結束　Esc 取消' }[ED.mode] || '';
  $('#hint').textContent = m;
}
function onDown(ev) {
  // pan: middle button or right-button drag (a right click without dragging still opens the context menu)
  if (ev.button === 1 || ev.button === 2) { closeCtx(); ED.pan = { x: ev.clientX, y: ev.clientY, vb: Object.assign({}, ED.vb), right: ev.button === 2, moved: false }; svg.setPointerCapture(ev.pointerId); if (ev.button !== 2) ev.preventDefault(); return; }
  if (ev.button !== 0) return;
  closeCtx();
  const [wx, wy] = toWorld(ev); const sx = snapv(wx), sy = snapv(wy);
  svg.setPointerCapture(ev.pointerId);
  if (ED.mode === 'place') { placeElement(ED.ghost, sx, sy); if (!ev.shiftKey) setMode('select'); else render(); return; }
  if (ED.mode === 'paste') { placeGroup(ED.ghost.group, sx, sy); setMode('select'); return; }
  if (ED.mode === 'wire') { wireClick(sx, sy, true); return; }
  // select mode
  const pin = nearPin(wx, wy);
  const el = hitElement(wx, wy); const wi = el ? null : hitWire(wx, wy);
  if (pin && !(el && ED.sel.has('e:' + el.id))) {
    // a node label's only pin is its tip: a click (no drag) there selects the label, a drag draws a wire
    const lbl = pin.el.type === 'LABEL' ? pin.el : (el && el.type === 'LABEL' ? el : null);
    ED.wireFromSelect = true; wireClick(pin.p.x, pin.p.y, true); if (lbl && ED.wire) ED.wire.lbl = lbl.id;
    if (ev.pointerType === 'touch') toast('接腳 ' + pin.el.name + '.' + (pin.p.n || '') + '：' + (PIN_KIND[pin.p.k] || ''));
    return;
  }
  const lr = !el && !wi && !ev.shiftKey && !ev.ctrlKey && !ev.metaKey ? hitLabel(wx, wy) : null;
  if (el || wi || lr) {
    const lel = lr ? elById(lr.id) : null;
    const k = el ? 'e:' + el.id : (wi ? 'w:' + wi.id : 'e:' + lel.id);
    if (ev.shiftKey || ev.ctrlKey || ev.metaKey) { if (ED.sel.has(k)) ED.sel.delete(k); else ED.sel.add(k); render(); return; }
    if (!ED.sel.has(k)) { ED.sel.clear(); ED.sel.add(k); }
    ED.drag = { sx, sy, dx: 0, dy: 0, base: JSON.parse(JSON.stringify({ elements: ED.doc.elements, wires: ED.doc.wires })), moved: false, label: lr ? { id: lr.id, key: lr.key } : null };
    render(); showLibDescFor(el ? el.type : (lel ? lel.type : null)); return;
  }
  if (!ev.shiftKey) ED.sel.clear();
  ED.band = { x0: wx, y0: wy, x1: wx, y1: wy }; render();
}
function onMove(ev) {
  const [wx, wy] = toWorld(ev); const sx = snapv(wx), sy = snapv(wy);
  if (ED.pan) { if (!ED.pan.moved && Math.abs(ev.clientX - ED.pan.x) + Math.abs(ev.clientY - ED.pan.y) > 4) { ED.pan.moved = true; svg.classList.add('panning'); } if (!ED.pan.moved) return; const r = svg.getBoundingClientRect(); const k = ED.pan.vb.w / r.width; ED.vb.x = ED.pan.vb.x - (ev.clientX - ED.pan.x) * k; ED.vb.y = ED.pan.vb.y - (ev.clientY - ED.pan.y) * k; applyView(); return; }
  ED.mouse = [sx, sy]; ED.lastClient = [ev.clientX, ev.clientY];
  if (ev.pointerType !== 'touch') hoverAt(wx, wy, ev);
  if (ED.drag) {
    const dx = sx - ED.drag.sx, dy = sy - ED.drag.sy;
    if (dx !== ED.drag.dx || dy !== ED.drag.dy) { ED.drag.dx = dx; ED.drag.dy = dy; ED.drag.moved = true; moveSelection(dx, dy, ED.drag.base); ED.conn = null; render(); }
    return;
  }
  if (ED.band) { ED.band.x1 = wx; ED.band.y1 = wy; renderTop(); return; }
  if (ED.wire && ED.wire.press) { const d = Math.abs(sx - ED.wire.pts[0][0]) + Math.abs(sy - ED.wire.pts[0][1]); if (d > 0) ED.wire.dragged = true; }
  if (ED.ghost) ED.ghostOn = true;
  renderTop();
}
// hover feedback in select mode: cursor shape, pin ring, status text and a delayed tooltip
const HOV = { pin: null, el: null, lbl: null, t: 0 };
function hoverAt(wx, wy, ev) {
  const sel = ED.mode === 'select' && !ED.drag && !ED.band && !ED.pan;
  const pn = (ED.mode === 'select' || ED.mode === 'wire') ? nearPin(wx, wy) : null;
  const el = sel && !pn ? hitElement(wx, wy) : null;
  const lr = sel && !pn && !el ? hitLabel(wx, wy) : null;
  const wi = sel && !pn && !el && !lr ? hitWire(wx, wy) : null;
  $('#coord').textContent = `x ${ED.mouse[0]}, y ${ED.mouse[1]}` + (pn ? '　接腳 ' + pn.el.name + '.' + (pn.p.n || '') + '：' + (PIN_KIND[pn.p.k] || '') : '');
  if (sel) svg.style.cursor = pn ? 'crosshair' : (lr ? 'text' : (el ? (ED.sel.has('e:' + el.id) ? 'move' : 'pointer') : (wi ? 'pointer' : 'default')));
  const hp = pn ? { x: pn.p.x, y: pn.p.y } : null;
  if (!!hp !== !!ED.hoverPin || (hp && (hp.x !== ED.hoverPin.x || hp.y !== ED.hoverPin.y))) { ED.hoverPin = hp; renderTop(); }
  const hl = lr ? { id: lr.id, key: lr.key } : null;
  if (!!hl !== !!ED.hotLabel || (hl && (hl.id !== ED.hotLabel.id || hl.key !== ED.hotLabel.key))) { ED.hotLabel = hl; render(); }
  // tooltip: element under the cursor (after a short pause)
  const target = pn ? null : (el || (lr ? elById(lr.id) : null));
  clearTimeout(HOV.t); const tip = $('#tip');
  if (!target || pn) { if (tip) tip.hidden = true; HOV.el = null; return; }
  if (HOV.el === target && tip && !tip.hidden) { tip.style.left = (ev.clientX + 14) + 'px'; tip.style.top = (ev.clientY + 16) + 'px'; return; }
  HOV.el = target;
  HOV.t = setTimeout(() => { if (HOV.el !== target || ED.drag || ED.band || ED.pan || $('.modal') || $('#inlineEdit')) return; showTip(target, ev.clientX, ev.clientY); }, 450);
}
function showTip(el, cx, cy) {
  let tip = $('#tip'); if (!tip) { tip = h('div', { id: 'tip', role: 'tooltip' }); document.body.append(tip); }
  const d = defOf(el); const pr = d.params.filter(p => p.type !== 'code' && String(el.params[p.k] || '') !== '').slice(0, 6).map(p => `<span><i>${esc(p.k)}</i> ${esc(el.params[p.k])}${p.u && /^[-+0-9.eE]+[a-zA-Zµ]*$/.test(String(el.params[p.k])) ? esc(p.u) : ''}</span>`).join('');
  const pins = pinsOf(el).filter(p => p.k !== 'lbl' && p.k !== 'gnd').map(p => `<span class="pk pk-${p.k === 'g' ? 'ci' : p.k}"><i></i>${esc(p.n || '')}</span>`).join('');
  tip.innerHTML = `<b>${esc(el.name || d.name)}</b><small>${esc(d.name)}　${esc(d.zh)}</small>${pr ? `<div class="tp">${pr}</div>` : ''}${pins ? `<div class="pinlist">${pins}</div>` : ''}<em>${el.dis ? '已停用 · ' : ''}雙擊編輯屬性，點名稱或參數文字可直接修改</em>`;
  tip.hidden = false; tip.style.left = (cx + 14) + 'px'; tip.style.top = (cy + 16) + 'px';
  const r = tip.getBoundingClientRect(); if (r.right > innerWidth - 8) tip.style.left = Math.max(4, cx - r.width - 10) + 'px'; if (r.bottom > innerHeight - 8) tip.style.top = Math.max(4, cy - r.height - 10) + 'px';
}
function hideTip() { clearTimeout(HOV.t); const t = $('#tip'); if (t) t.hidden = true; HOV.el = null; if (ED.hoverPin) { ED.hoverPin = null; renderTop(); } if (ED.hotLabel) { ED.hotLabel = null; render(); } }
// inline editing of a name or parameter shown on the schematic
function inlineEdit(id, key) {
  const el = elById(id); if (!el) return; const d = defOf(el);
  const p = key === '__name' ? null : d.params.find(q => q.k === key); if (key !== '__name' && !p) return;
  if (p && (p.type === 'choice' || p.type === 'code')) { openProps(el); return; }
  const rect = labelRects.find(r => r.id === id && r.key === key); if (!rect) { openProps(el); return; }
  hideTip(); closeInline();
  const sr = svg.getBoundingClientRect(); const vb = ED.vb; const sx = sr.left + (rect.x0 - vb.x) / vb.w * sr.width, sy = sr.top + (rect.y0 - vb.y) / vb.h * sr.height;
  const cur = key === '__name' ? el.name : String(el.params[key] == null ? '' : el.params[key]);
  const inp = h('input', { id: 'inlineEdit', type: 'text', value: cur, spellcheck: 'false', autocomplete: 'off', 'aria-label': key === '__name' ? '元件名稱' : (p.l || key) });
  const lab = h('label', { for: 'inlineEdit' }, key === '__name' ? '名稱' : key + (p.u ? ' (' + p.u + ')' : ''));
  const box = h('div', { id: 'inlineBox' }, lab, inp);
  document.body.append(box);
  box.style.left = Math.min(innerWidth - 230, Math.max(4, sx - 6)) + 'px'; box.style.top = Math.min(innerHeight - 40, Math.max(4, sy - 6)) + 'px';
  const syms = paramSyms();
  const commit = () => {
    const v = inp.value.trim();
    if (key === '__name') {
      if (!v) { toast('名稱不可空白'); inp.focus(); return false; }
      if (v !== el.name && ED.doc.elements.some(o => o !== el && o.name === v)) { toast('名稱「' + v + '」已被使用'); inp.classList.add('bad'); inp.focus(); return false; }
      if (v !== el.name) { pushUndo(); el.name = v; changed(); }
    } else {
      const err = validateParam(p, v, syms, el);
      if (err) { toast(key + '：' + err); inp.classList.add('bad'); inp.focus(); return false; }
      if (v !== String(el.params[key])) { pushUndo(); el.params[key] = v; changed(); }
    }
    closeInline(); return true;
  };
  inp.addEventListener('keydown', e => { e.stopPropagation(); if (e.key === 'Enter') { e.preventDefault(); commit(); } else if (e.key === 'Escape') { e.preventDefault(); closeInline(); } else if (e.key === 'Tab') { e.preventDefault(); if (commit()) { const keys = labelKeys(el); const i = keys.indexOf(key); const nk = keys[(i + (e.shiftKey ? -1 : 1) + keys.length) % keys.length]; if (nk && nk !== key) inlineEdit(id, nk); } } });
  inp.addEventListener('input', () => { if (p) inp.classList.toggle('bad', !!validateParam(p, inp.value, syms, el)); else inp.classList.remove('bad'); });
  inp.addEventListener('blur', () => { setTimeout(() => { if ($('#inlineEdit') === inp) { if (!commit()) closeInline(); } }, 80); });
  inp.focus(); inp.select();
}
function closeInline() { const b = $('#inlineBox'); if (b) b.remove(); }
function onUp(ev) {
  if (ED.pan) { const p = ED.pan; ED.pan = null; svg.classList.remove('panning'); if (p.right) { if (p.moved) ED.rpanEnd = performance.now(); else if (p.ctx) ctxAt(p.ctx); } render(); return; }
  const [wx, wy] = toWorld(ev); const sx = snapv(wx), sy = snapv(wy);
  if (ED.drag) {
    const d = ED.drag; ED.drag = null;
    if (d.moved) { const after = ED.doc; ED.doc = JSON.parse(JSON.stringify(d.base)); pushUndo(); ED.doc = after; finalizeWires(); changed(); }
    else if (d.label && ev.pointerType !== 'touch') inlineEdit(d.label.id, d.label.key);
    return;
  }
  if (ED.band) {
    const b = ED.band; ED.band = null;
    const x0 = Math.min(b.x0, b.x1), x1 = Math.max(b.x0, b.x1), y0 = Math.min(b.y0, b.y1), y1 = Math.max(b.y0, b.y1);
    if (x1 - x0 > 0.2 || y1 - y0 > 0.2) {
      ED.doc.elements.forEach(el => { const bb = wbox(el); if (bb.x0 >= x0 && bb.x1 <= x1 && bb.y0 >= y0 && bb.y1 <= y1) ED.sel.add('e:' + el.id); });
      ED.doc.wires.forEach(w => { if (Math.min(w.x1, w.x2) >= x0 && Math.max(w.x1, w.x2) <= x1 && Math.min(w.y1, w.y2) >= y0 && Math.max(w.y1, w.y2) <= y1) ED.sel.add('w:' + w.id); });
    }
    render(); return;
  }
  if (ED.wire && ED.wire.press) {
    ED.wire.press = false;
    if (!ED.wire.dragged && ED.wire.lbl && ED.wire.pts.length === 1) { const id = ED.wire.lbl; ED.wire = null; ED.wireFromSelect = false; setMode('select'); ED.sel.clear(); ED.sel.add('e:' + id); render(); return; }
    if (ED.wire.dragged) { wireClick(sx, sy, false); if (ED.wire) finishWire(); }
  }
}
function wireClick(x, y, press) {
  if (!ED.wire) { ED.wire = { pts: [[x, y]], press, dragged: false }; if (ED.mode !== 'wire') { ED.prevMode = ED.mode; ED.mode = 'wire'; svg.style.cursor = 'crosshair'; updateHint(); } renderTop(); return; }
  const last = ED.wire.pts[ED.wire.pts.length - 1];
  if (last[0] === x && last[1] === y) { if (ED.wire.pts.length > 1) finishWire(); return; }
  const seg = lPath(last, [x, y]).slice(1);
  ED.wire.pts.push(...seg);
  if (isConnPoint(x, y)) finishWire(); else renderTop();
}
function finishWire() {
  const w = ED.wire; ED.wire = null;
  if (w && w.pts.length > 1) {
    pushUndo();
    for (let i = 1; i < w.pts.length; i++) addWire(w.pts[i - 1][0], w.pts[i - 1][1], w.pts[i][0], w.pts[i][1]);
    changed();
  }
  if (ED.wireFromSelect) { ED.wireFromSelect = false; setMode('select'); } else renderTop();
}
function onDbl(ev) {
  const [wx, wy] = toWorld(ev);
  if (ED.mode === 'wire') { if (ED.wire) finishWire(); return; }
  const el = hitElement(wx, wy); if (el) { openProps(el); return; }
  const lr = hitLabel(wx, wy); if (lr) { openProps(elById(lr.id)); return; }
  if (ED.mode === 'select' && !hitWire(wx, wy)) openQuickAdd(ev.clientX, ev.clientY, [snapv(wx), snapv(wy)]);
}
function onWheel(ev) {
  ev.preventDefault(); const [wx, wy] = toWorld(ev);
  zoomAt(ev.deltaY > 0 ? 1.15 : 1 / 1.15, wx, wy);
}
function onCtx(ev) {
  ev.preventDefault();
  const at = { clientX: ev.clientX, clientY: ev.clientY, w: toWorld(ev) };
  if (ED.pan && ED.pan.right) { ED.pan.ctx = at; return; } // button still held: decide on release (drag = pan, click = menu)
  if (performance.now() - (ED.rpanEnd || 0) < 400) return;  // menu event after a right-drag pan (Windows fires it on release)
  ctxAt(at);
}
function ctxAt(ev) {
  if (ED.mode === 'wire') { if (ED.wire) finishWire(); else setMode('select'); return; }
  if (ED.mode === 'place' || ED.mode === 'paste') { setMode('select'); return; }
  const [wx, wy] = ev.w; const el = hitElement(wx, wy); const wi = el ? null : hitWire(wx, wy);
  if (el && !ED.sel.has('e:' + el.id)) { ED.sel.clear(); ED.sel.add('e:' + el.id); render(); }
  if (wi && !ED.sel.has('w:' + wi.id)) { ED.sel.clear(); ED.sel.add('w:' + wi.id); render(); }
  showCtx(ev.clientX, ev.clientY, el);
}
function showCtx(x, y, el) {
  closeCtx();
  const items = el ? [
    ['屬性…', 'Enter', () => openProps(el)], ['旋轉 90°', 'Space', () => rotateSel(1)], ['左右鏡像', 'X', () => mirrorSel(false)], ['上下鏡像', 'Y', () => mirrorSel(true)],
    [el.dis ? '啟用' : '停用', 'D', toggleDisable], null, ['複製', 'Ctrl+C', () => copySel(false)], ['剪下', 'Ctrl+X', () => copySel(true)], ['刪除', 'Del', deleteSel]
  ] : (ED.sel.size ? [['刪除', 'Del', deleteSel], ['複製', 'Ctrl+C', () => copySel(false)]] : [['貼上', 'Ctrl+V', pasteClip], ['畫導線', 'W', () => setMode('wire')], ['全部顯示', 'F', fitView]]);
  const m = h('div', { id: 'ctx' });
  items.forEach(it => { if (!it) { m.append(h('hr')); return; } m.append(h('button', { onclick: () => { closeCtx(); it[2](); } }, h('span', null, it[0]), h('span', { class: 'kbd' }, it[1]))); });
  document.body.append(m);
  const r = m.getBoundingClientRect(); m.style.left = Math.min(x, innerWidth - r.width - 8) + 'px'; m.style.top = Math.min(y, innerHeight - r.height - 8) + 'px';
}
function closeCtx() { const m = $('#ctx'); if (m) m.remove(); }

/* ------------------------------------------------------------------
 * Keyboard
 * ------------------------------------------------------------------ */
function onKey(ev) {
  if ($('.modal') || $('.viewer:not([hidden]):not(.float)')) { if (ev.key === 'Escape') { const v = $('.viewer:not([hidden]):not(.float)'); if (!$('.modal') && v) { if (v.id === 'wave' && typeof WV !== 'undefined' && WV.tool) { WV.setTool(WV.tool); return; } if (window.__WAVE_POPUP__) window.close(); else v.hidden = true; } } return; }
  const t = ev.target; if (t && (t.tagName === 'INPUT' || t.tagName === 'TEXTAREA' || t.tagName === 'SELECT')) return;
  const k = ev.key, ctrl = ev.ctrlKey || ev.metaKey;
  if (k === ' ') { ev.preventDefault(); const a = document.activeElement; if (a && a.tagName === 'BUTTON') a.blur(); if (!ev.repeat) rotateSel(ev.shiftKey ? -1 : 1); return; }
  if (ctrl) {
    const lk = k.toLowerCase();
    if (lk === 'z' && !ev.shiftKey) { undo(); ev.preventDefault(); }
    else if (lk === 'y' || (lk === 'z' && ev.shiftKey)) { redo(); ev.preventDefault(); }
    else if (lk === 'c') { copySel(false); ev.preventDefault(); }
    else if (lk === 'x') { copySel(true); ev.preventDefault(); }
    else if (lk === 'v') { pasteClip(); ev.preventDefault(); }
    else if (lk === 'a') { ED.doc.elements.forEach(e => ED.sel.add('e:' + e.id)); ED.doc.wires.forEach(w => ED.sel.add('w:' + w.id)); render(); ev.preventDefault(); }
    else if (lk === 's') { saveDialog(); ev.preventDefault(); }
    else if (lk === 'o') { openFile(); ev.preventDefault(); }
    else if (lk === 'f') { if (ev.shiftKey) { if (innerWidth <= 1100 && !$('#main').classList.contains('libOpen')) toggleLib(); $('#libSearch').focus(); } else openFind(); ev.preventDefault(); }
    else if (lk === 'd') { copySel(false); pasteClip(); ev.preventDefault(); }
    return;
  }
  switch (k) {
    case '/': case 'Insert': { ev.preventDefault(); const r = svg.getBoundingClientRect(); const [mx, my] = ED.mouse; const inside = ED.lastClient && ED.lastClient[0] >= r.left && ED.lastClient[0] <= r.right && ED.lastClient[1] >= r.top && ED.lastClient[1] <= r.bottom; openQuickAdd(inside ? ED.lastClient[0] : r.left + r.width / 2, inside ? ED.lastClient[1] : r.top + r.height / 2, inside ? [mx, my] : null); break; }
    case 'r': case 'R': rotateSel(ev.shiftKey ? -1 : 1); break;
    case 'x': case 'X': mirrorSel(false); break;
    case 'y': case 'Y': mirrorSel(true); break;
    case 'd': case 'D': toggleDisable(); break;
    case 'w': case 'W': setMode('wire'); break;
    case 'l': case 'L': startPlace('LABEL'); break;
    case 'g': case 'G': startPlace('GND'); break;
    case 'Delete': case 'Backspace': deleteSel(); break;
    case 'Escape': if (ED.wire) { ED.wire = null; if (ED.wireFromSelect) { ED.wireFromSelect = false; setMode('select'); } renderTop(); } else { setMode('select'); ED.sel.clear(); render(); } break;
    case 'Enter': { const s = selectedEls(); if (s.length === 1) openProps(s[0]); break; }
    case 'F5': ev.preventDefault(); runSim(false); break;
    case 'F6': ev.preventDefault(); openLastResult(); break;
    case 'F7': ev.preventDefault(); openSimCtrl(); break;
    case 'f': case 'F': fitView(); break;
    case '+': case '=': zoomAt(1 / 1.25, ED.vb.x + ED.vb.w / 2, ED.vb.y + ED.vb.h / 2); break;
    case '-': case '_': zoomAt(1.25, ED.vb.x + ED.vb.w / 2, ED.vb.y + ED.vb.h / 2); break;
    case 'ArrowLeft': case 'ArrowRight': case 'ArrowUp': case 'ArrowDown': {
      if (!ED.sel.size) return; ev.preventDefault();
      const dx = k === 'ArrowLeft' ? -1 : k === 'ArrowRight' ? 1 : 0, dy = k === 'ArrowUp' ? -1 : k === 'ArrowDown' ? 1 : 0;
      const base = JSON.parse(JSON.stringify({ elements: ED.doc.elements, wires: ED.doc.wires })); pushUndo(); moveSelection(dx, dy, base); finalizeWires(); changed(); break;
    }
    default: return;
  }
}

/* ------------------------------------------------------------------
 * Library browser
 * ------------------------------------------------------------------ */
function iconSvg(type, w, hgt) {
  const d = LIB.D[type]; const el = { type, x: 0, y: 0, rot: 0, mir: false, params: defaultParams(type) };
  let [x0, y0, x1, y1] = d.bb; if (d.label) { x1 = 3; } if (d.text) return `<svg viewBox="-0.5 -1.6 6 3" width="${w}" height="${hgt}"><text x="0" y="0.3" font-size="1.4" fill="var(--muted)">Abc</text></svg>`;
  if (d.label) return `<svg viewBox="-0.5 -1.2 4.5 2.4" width="${w}" height="${hgt}"><path d="M0 0L0.5 -0.55L3.2 -0.55L3.2 0.55L0.5 0.55Z" fill="var(--accent-soft)" stroke="var(--accent)" stroke-width="0.12"/><text x="0.7" y="0.3" font-size="0.8" fill="var(--accent)">Vo</text></svg>`;
  const pad = 0.5; const bw = x1 - x0 + 2 * pad, bh = y1 - y0 + 2 * pad;
  const s = Math.max(bw / (w / hgt), bh);
  const vw = s * (w / hgt), vh = s;
  const cx = (x0 + x1) / 2, cy = (y0 + y1) / 2;
  return `<svg viewBox="${cx - vw / 2} ${cy - vh / 2} ${vw} ${vh}" width="${w}" height="${hgt}" class="ico"><g class="sym${isCtlEl(el) ? ' ctl' : ''}">${symMarkup(el, isCtlEl(el) ? 'ctl' : '').replace(/^<g class="sym [^"]*"/, '<g')}</g></svg>`;
}
function buildLib() {
  const root = LIB.tree();
  const tree = $('#libTree'); tree.innerHTML = '';
  const open = new Set(JSON.parse(store.get('psimweb.libOpen') || '["Elements","Elements/Power","Elements/Control","Elements/Other","Elements/Sources"]'));
  const walk = (node, depth) => {
    const has = node.kids.length > 0;
    const row = h('div', { class: 'tn' + (ED.libSel === node.path ? ' sel' : ''), style: `padding-left:${4 + depth * 12}px`, 'data-path': node.path },
      h('span', { class: 'tw' }, has ? (open.has(node.path) ? '▾' : '▸') : ''), node.name);
    row.onclick = (e) => {
      if (has && (e.target.classList.contains('tw') || ED.libSel === node.path)) { if (open.has(node.path)) open.delete(node.path); else open.add(node.path); store.set('psimweb.libOpen', JSON.stringify([...open])); }
      ED.libSel = node.path; $('#libSearch').value = ''; buildLib(); showList(node);
    };
    tree.append(row);
    if (has && open.has(node.path)) node.kids.forEach(k => walk(k, depth + 1));
  };
  walk(root, 0);
  if (!ED.libListShown) { const n = findNode(root, ED.libSel) || root; showList(n); ED.libListShown = true; }
}
function findNode(n, p) { if (n.path === p) return n; for (const k of n.kids) { const r = findNode(k, p); if (r) return r; } return null; }
function showList(node, types) {
  const list = $('#libList'); list.innerHTML = '';
  let items = types || node.items;
  if (!types && !items.length) { const all = []; const rec = n => { all.push(...n.items); n.kids.forEach(rec); }; rec(node); items = all; }
  if (!items.length) { list.append(h('div', { class: 'libEmpty' }, '沒有符合的元件')); return; }
  items.forEach(t => {
    const d = LIB.D[t];
    const it = h('div', { class: 'li', draggable: 'true', title: d.name + '（' + d.zh + '）', 'data-type': t, html: iconSvg(t, 44, 30) },
      h('div', { class: 'nm' }, d.name, h('small', null, d.zh)));
    it.onclick = () => { $$('.li.sel').forEach(x => x.classList.remove('sel')); it.classList.add('sel'); showLibDesc(t); startPlace(t); touchPlaceHint(); };
    it.onmouseenter = () => showLibDesc(t);
    it.ondragstart = (e) => { e.dataTransfer.setData('text/plain', 'psim:' + t); e.dataTransfer.effectAllowed = 'copy'; ED.dragType = t; showLibDesc(t); };
    it.ondragend = () => { ED.dragType = null; ED.ghost = null; ED.ghostOn = false; renderTop(); };
    list.append(it);
  });
}
function pinListHtml(t) {
  const d = LIB.D[t]; if (!d) return '';
  const el = { type: t, x: 0, y: 0, rot: 0, mir: false, params: defaultParams(t) };
  const ps = d.pinsFor(el).filter(p => p.k !== 'lbl'); if (!ps.length) return '';
  return '<div class="pinlist">接腳：' + ps.map(p => `<span class="pk pk-${p.k === 'g' ? 'ci' : p.k}"><i></i>${esc(p.n || '')} <small>${esc(PIN_KIND[p.k] || '')}</small></span>`).join('') + '</div>';
}
function showLibDesc(t) {
  const d = LIB.D[t]; if (!d) return;
  $('#libDesc').innerHTML = `<b>${esc(d.name)}</b><span class="zh">${esc(d.zh)}</span>${pinListHtml(t)}<p>${esc(d.desc)}</p>`;
}
function showLibDescFor(t) { if (t) showLibDesc(t); }
function libSearch() {
  const q = $('#libSearch').value.trim().toLowerCase();
  if (!q) { ED.libSel = ED.libSel || 'Elements'; buildLib(); const n = findNode(LIB.tree(), ED.libSel); showList(n); return; }
  const words = q.split(/\s+/);
  const res = LIB.ORDER.filter(t => { const d = LIB.D[t]; const hay = (t + ' ' + d.name + ' ' + d.zh + ' ' + d.desc + ' ' + d.cat).toLowerCase(); return words.every(w => hay.includes(w)); });
  ED.libSel = ''; $$('.tn.sel').forEach(x => x.classList.remove('sel'));
  showList(null, res);
  $('#libDesc').innerHTML = `<b>搜尋結果</b><span class="zh">${res.length} 個元件</span><p>點選元件後移到電路圖上放置，或直接拖曳到電路圖。</p>`;
}

/* ------------------------------------------------------------------
 * Dialog helper
 * ------------------------------------------------------------------ */
function modal(title, body, buttons, opts) {
  opts = opts || {};
  const close = () => { wrap.remove(); if (opts.onClose) opts.onClose(); resetViewport(); };
  const foot = h('footer');
  (buttons || []).forEach(b => foot.append(h('button', { class: 'btn' + (b.pri ? ' pri' : '') + (b.danger ? ' danger' : ''), onclick: () => { if (b.fn && b.fn() === false) return; if (b.close !== false) close(); } }, b.label)));
  const dlg = h('div', { class: 'dlg' + (opts.wide ? ' wide' : ''), role: 'dialog', 'aria-modal': 'true' },
    h('header', null, h('h3', null, title), h('button', { class: 'x', 'aria-label': '關閉', onclick: close }, '×')),
    h('div', { class: 'body' }, body), foot);
  const wrap = h('div', { class: 'modal' }, dlg);
  wrap.addEventListener('mousedown', e => { if (e.target === wrap && !opts.sticky) close(); });
  wrap.addEventListener('keydown', e => {
    if (e.key === 'Escape') { e.stopPropagation(); close(); }
    if (e.key === 'Enter' && opts.onEnter && e.target.tagName !== 'TEXTAREA' && e.target.tagName !== 'BUTTON') { e.preventDefault(); opts.onEnter(); }
  });
  document.body.append(wrap);
  // on touch screens do not focus a text field automatically (the on-screen keyboard would pop up and scroll the page)
  setTimeout(() => { const coarse = document.documentElement.classList.contains('coarse'); const f = dlg.querySelector(coarse ? 'button.pri' : 'input,select,textarea,button.pri'); if (f) f.focus({ preventScroll: true }); }, 30);
  return { close, wrap, dlg };
}
// mobile browsers scroll the page while the on-screen keyboard is open and do not always scroll back;
// the app is a fixed full-screen layout, so put the page back and re-layout the schematic
function resetViewport() {
  const a = document.activeElement; if (a && a !== document.body && (a.tagName === 'INPUT' || a.tagName === 'TEXTAREA' || a.tagName === 'SELECT')) a.blur();
  const fix = () => { if (window.scrollX || window.scrollY) window.scrollTo(0, 0); document.documentElement.scrollTop = 0; document.body.scrollTop = 0; if (typeof render === 'function') render(); };
  fix(); setTimeout(fix, 120); setTimeout(fix, 450);
}
function toast(msg) {
  const s = $('#statusMsg'); s.textContent = msg; clearTimeout(toast.t); toast.t = setTimeout(() => { s.textContent = ''; }, 4000);
}

/* ------------------------------------------------------------------
 * Properties dialog
 * ------------------------------------------------------------------ */
function paramSyms() {
  let txt = ''; ED.doc.elements.forEach(e => { if (e.type === 'PARAMS' && !e.dis) txt += (e.params.text || '') + '\n'; });
  return SIM.parseParamText(txt).syms;
}
function validateParam(p, v, syms, el) {
  const s = String(v == null ? '' : v);
  try {
    if (!p.type || p.type === 'num') { if (s.trim() === '') return '不可空白'; SIM.evalConst(s, syms); }
    else if (p.type === 'list') { if (s.trim() !== '') SIM.evalList(s, syms); }
    else if (p.type === 'expr') {
      const args = el.type === 'NONI' ? ['v'] : el.type === 'NONV' ? ['i'] : (el.type.startsWith('MATH') ? ['t', 'x'].concat(Array.from({ length: 10 }, (_, i) => 'x' + (i + 1))) : ['t']);
      SIM.compileFn(s, args, syms);
    } else if (p.type === 'text' && p.k === 'pairs') SIM.evalPairs(s, syms);
    else if (p.k === 'spice') { const r = SIM.parseSpiceModel(s); if (r.bad.length) return '無法解析：' + r.bad.join('、'); }
  } catch (e) { return e.message; }
  return null;
}
function openProps(el) {
  const d = defOf(el);
  if (el.type === 'SIMCTRL') { openSimCtrl(el); return; }
  const syms = paramSyms();
  const disp = Object.assign({}, el.disp || {});
  const showSet = new Set(d.show || []);
  const grid = h('div', { class: 'prm' });
  grid.append(h('span', { class: 'hdr' }, '參數'), h('span', { class: 'hdr' }, '數值'), h('span', { class: 'hdr' }, '單位'), h('span', { class: 'hdr', title: '顯示在電路圖上' }, '顯示'));
  const inputs = {};
  if (!d.noname) {
    const inp = h('input', { type: 'text', id: 'p_name', value: el.name, spellcheck: 'false' });
    inputs.__name = inp;
    const cb = h('input', { type: 'checkbox', title: '在電路圖顯示名稱' }); cb.checked = disp.name !== false; cb.onchange = () => { disp.name = cb.checked; };
    grid.append(h('label', { for: 'p_name' }, '名稱 Name', h('small', null, '波形與激勵源以此名稱識別')), inp, h('span'), cb);
  }
  const advBox = h('div', { class: 'prm' });
  d.params.forEach(p => {
    const id = 'p_' + p.k; let inp;
    if (p.type === 'choice') { inp = h('select', { id }); p.opts.forEach(([v, l]) => { const o = h('option', { value: v }, l); if (String(el.params[p.k]) === v) o.selected = true; inp.append(o); }); }
    else if (p.type === 'code') { inp = h('textarea', { id, spellcheck: 'false' }); inp.value = el.params[p.k] == null ? '' : el.params[p.k]; }
    else inp = h('input', { type: 'text', id, value: el.params[p.k] == null ? '' : el.params[p.k], spellcheck: 'false', autocomplete: 'off' });
    inputs[p.k] = inp;
    const cb = h('input', { type: 'checkbox', title: '在電路圖顯示此參數' }); cb.checked = disp[p.k] !== undefined ? disp[p.k] : showSet.has(p.k); cb.onchange = () => { disp[p.k] = cb.checked; };
    const lab = h('label', { for: id, title: p.desc }, p.l, p.desc ? h('small', null, p.desc) : null);
    const target = p.adv ? advBox : grid;
    if (p.type === 'code') target.append(lab, h('span'), h('span'), cb, h('span'), inp);
    else target.append(lab, inp, h('span', { class: 'u' }, p.u || ''), p.type === 'code' ? h('span') : cb);
    if (!p.type || p.type === 'num' || p.type === 'list' || p.type === 'expr') inp.addEventListener('input', () => { const e = validateParam(p, inp.value, syms, el); inp.classList.toggle('bad', !!e); });
  });
  // SPICE model selector: show the selected preset as placeholder, disable the text for ideal models
  const spP = d.params.find(p => p.k === 'spice');
  if (spP && inputs.model && inputs.spice) {
    const mSel = inputs.model, sp = inputs.spice, pres = (SIM.DEV_PRESETS && SIM.DEV_PRESETS[spP.presetKind]) || {};
    const isPreset = v => Object.values(pres).some(t => t === v.trim());
    const upd = () => {
      const m = mSel.value, ideal = m === 'ideal' || m === '3state';
      d.params.forEach(p => { if (p.idealOnly && inputs[p.k]) inputs[p.k].disabled = !ideal; });
      sp.disabled = ideal; sp.placeholder = ideal ? '（理想模型不使用 SPICE 參數）' : (pres[m] ? pres[m].replace(/ /g, '\n') : '例如：\n.model D1 D(IS=2.52n N=1.752 RS=0.568 CJO=4p TT=20n BV=100)');
    };
    mSel.addEventListener('change', () => { if (isPreset(sp.value)) sp.value = ''; upd(); });
    upd();
  }
  const err = h('div', { class: 'prmErr' });
  const body = h('div', null, grid);
  if (advBox.childNodes.length) body.append(h('details', { class: 'adv' }, h('summary', null, '進階參數'), advBox));
  body.append(err, h('div', { class: 'descBox' }, h('b', null, d.name + '（' + d.zh + '）'), h('div', null, d.desc)));
  const ok = () => {
    const errs = [];
    d.params.forEach(p => { if (p.type === 'choice' || (p.type === 'code' && p.k !== 'spice')) return; const e = validateParam(p, inputs[p.k].value, syms, el); inputs[p.k].classList.toggle('bad', !!e); if (e) errs.push(p.l.split(' ')[0] + '：' + e); });
    if (inputs.model && inputs.spice && inputs.model.value === 'custom' && !inputs.spice.value.trim()) { inputs.spice.classList.add('bad'); errs.push('自訂模型需填寫 SPICE 參數'); }
    if (inputs.__name) {
      const nm = inputs.__name.value.trim();
      if (!nm) errs.push('名稱不可空白');
      else if (ED.doc.elements.some(o => o !== el && o.name === nm && !LIB.D[o.type].noname)) errs.push('名稱「' + nm + '」已被其他元件使用');
    }
    if (errs.length) { err.textContent = errs.join('；'); return false; }
    pushUndo();
    if (inputs.__name) el.name = inputs.__name.value.trim();
    d.params.forEach(p => { el.params[p.k] = inputs[p.k].value; });
    el.disp = disp; changed();
    return true;
  };
  const m = modal(d.name + (el.name ? ' — ' + el.name : '') + ' 屬性', body, [
    { label: '取消' }, { label: '確定', pri: true, fn: ok }], { wide: d.params.some(p => p.type === 'code'), onEnter: () => { if (ok() !== false) m.close(); } });
}

/* ------------------------------------------------------------------
 * Simulation control dialog (PSIM layout: Parameters tab)
 * ------------------------------------------------------------------ */
function simCtrlEl(create) {
  let el = ED.doc.elements.find(e => e.type === 'SIMCTRL');
  if (!el && create) {
    pushUndo();
    let x = 0, y = 0; if (ED.doc.elements.length) { const bs = ED.doc.elements.map(wbox); x = Math.round(Math.max(...bs.map(b => b.x1))) + 4; y = Math.round(Math.min(...bs.map(b => b.y0))) + 1; }
    el = newElement('SIMCTRL', x, y); ED.doc.elements.push(el); changed();
  }
  return el;
}
function openSimCtrl(elIn) {
  const el = elIn || simCtrlEl(true); const d = LIB.D.SIMCTRL; const syms = paramSyms();
  const f = (k, label, unit) => { const inp = h('input', { type: 'text', id: 'sc_' + k, value: el.params[k], spellcheck: 'false', autocomplete: 'off' }); return [h('label', { for: 'sc_' + k }, label), inp, h('span', { class: 'u' }, unit), h('span')]; };
  const g = h('div', { class: 'prm' }, ...f('dt', 'Time step 時間步長', 's'), ...f('tt', 'Total time 總時間', 's'), ...f('tp', 'Print time 輸出起始時間', 's'), ...f('ps', 'Print step 輸出間隔', ''));
  const sel = (k, label) => { const p = d.params.find(q => q.k === k); const s = h('select', { id: 'sc_' + k }); p.opts.forEach(([v, l]) => { const o = h('option', { value: v }, l); if (String(el.params[k]) === v) o.selected = true; s.append(o); }); return [h('label', { for: 'sc_' + k }, label), s, h('span'), h('span')]; };
  const adv = h('div', { class: 'prm' }, ...sel('interp', '切換瞬間內插 Switching interpolation'), ...sel('damp', '切換後阻尼 Damping after switching'));
  const info = h('div', { class: 'note', style: 'margin-top:10px' });
  const err = h('div', { class: 'prmErr' });
  const upd = () => {
    try {
      const dt = SIM.evalConst($('#sc_dt').value, syms), tt = SIM.evalConst($('#sc_tt').value, syms), tp = SIM.evalConst($('#sc_tp').value, syms), ps = Math.max(1, Math.round(SIM.evalConst($('#sc_ps').value, syms)));
      const N = tt / dt, nr = Math.floor((tt - tp) / dt / ps) + 1;
      info.innerHTML = `時間步數：<b class="mono">${N.toExponential(3)}</b>　輸出點數：<b class="mono">${nr.toExponential(3)}</b>（每個探棒約 ${siFmt(nr * 8, 'B', 3)}）` + (N > 5e7 ? '<br><span style="color:var(--warn)">步數很多，模擬會需要較長時間，可隨時在進度視窗中止。</span>' : '') + (dt < 1e-12 * 0.999 ? '<br><span style="color:var(--err)">時間步長小於 1 ps。</span>' : '');
      err.textContent = '';
    } catch (e) { info.textContent = ''; err.textContent = e.message; }
  };
  const body = h('div', null,
    h('div', { class: 'tabs' }, h('button', { class: 'on' }, 'Parameters 參數')),
    g, h('details', { class: 'adv', open: true }, h('summary', null, '精度選項'), adv), info, err,
    h('div', { class: 'descBox' }, h('b', null, '說明'), h('div', null, 'Time step 為固定模擬步長，最小可設 1 ps；Print step = N 表示每 N 步儲存一點。開啟「切換瞬間內插」時，二極體、比較器驅動的開關等切換時刻會在步長內精確定位，即使步長較大也能得到正確的占空比與平均值。')));
  const m = modal('Simulation Control 模擬控制', body, [
    { label: '取消' }, { label: '確定', pri: true, fn: () => save() }], { onEnter: () => { if (save() !== false) m.close(); } });
  ['dt', 'tt', 'tp', 'ps'].forEach(k => $('#sc_' + k).addEventListener('input', upd)); upd();
  function save() {
    const vals = {}; let bad = false;
    ['dt', 'tt', 'tp', 'ps'].forEach(k => { const v = $('#sc_' + k).value; try { const x = SIM.evalConst(v, syms); if ((k === 'dt' || k === 'tt') && !(x > 0)) throw new Error('必須大於 0'); if (k === 'tp' && x < 0) throw new Error('不可為負'); if (k === 'ps' && !(x >= 1)) throw new Error('必須 ≥ 1'); $('#sc_' + k).classList.remove('bad'); } catch (e) { bad = true; $('#sc_' + k).classList.add('bad'); err.textContent = k + '：' + e.message; } vals[k] = v; });
    if (bad) return false;
    pushUndo(); Object.assign(el.params, vals, { interp: $('#sc_interp').value, damp: $('#sc_damp').value }); changed(); return true;
  }
}

/* ------------------------------------------------------------------
 * Netlist builder
 * ------------------------------------------------------------------ */
function buildNetlist() {
  const errors = [], warnings = [];
  const els = ED.doc.elements.filter(e => !e.dis);
  const c = connectivityFor(els);
  const nets = [], netOf = new Map();
  const getNet = r => { if (!netOf.has(r)) { netOf.set(r, nets.length); nets.push({ kinds: new Set(), pins: [], kind: null }); } return netOf.get(r); };
  const elems = [];
  els.forEach(el => {
    const d = defOf(el); if (d.nosim) return;
    const wp = wpins(el);
    const pins = wp.map((p, i) => { const r = c.find(c.key(p.x, p.y)); const ni = getNet(r); nets[ni].kinds.add(p.k); nets[ni].pins.push({ el, i, k: p.k }); return ni; });
    elems.push({ el, pins });
  });
  // net members (wires count as members)
  const wireMembers = new Map(); ED.doc.wires.forEach(w => { const r = c.find(c.key(w.x1, w.y1)); wireMembers.set(r, (wireMembers.get(r) || 0) + 1); });
  // classify nets
  nets.forEach(n => {
    if (n.kinds.has('gnd')) n.kind = 'G';
    else if (n.kinds.has('p') && (n.kinds.has('ci') || n.kinds.has('co') || n.kinds.has('g'))) { n.kind = 'X'; }
    else if (n.kinds.has('p')) n.kind = 'P';
    else if (n.kinds.has('ci') || n.kinds.has('co') || n.kinds.has('g')) n.kind = 'C';
    else n.kind = null;
  });
  // propagate power domain through 'any' pins of the same element
  let ch = true;
  while (ch) {
    ch = false;
    elems.forEach(({ el, pins }) => {
      const ps = pinsOf(el); const anyIdx = ps.map((p, i) => p.k === 'a' ? i : -1).filter(i => i >= 0);
      if (!anyIdx.length) return;
      const hasP = anyIdx.some(i => nets[pins[i]].kind === 'P');
      if (hasP) anyIdx.forEach(i => { const n = nets[pins[i]]; if (n.kind === null) { n.kind = 'P'; ch = true; } });
    });
  }
  nets.forEach(n => { if (n.kind === null) n.kind = 'C'; });
  nets.forEach(n => { if (n.kind === 'X') { const ids = new Set(n.pins.map(p => p.el.id)); errors.push({ msg: '功率電路與控制電路直接相連（' + [...ids].map(id => elById(id).name || elById(id).type).join('、') + '）。請透過感測器、開關控制器或 Control-Power Interface 連接。', els: [...ids] }); } });
  // dangling pins: net with single pin and no wires/labels
  const root = new Map(); netOf.forEach((ni, r) => root.set(ni, r));
  const netList = nets.map((n, i) => ({ kind: n.kind === 'X' ? 'P' : n.kind }));
  elems.forEach(e => {
    e.pins = e.pins.map((ni, i) => { const n = nets[ni]; const single = n.pins.length <= 1 && !(wireMembers.get(root.get(ni)) > 0) && n.kind !== 'G'; return single ? -1 : ni; });
  });
  // checks
  const sc = ED.doc.elements.filter(e => e.type === 'SIMCTRL' && !e.dis);
  if (sc.length > 1) errors.push({ msg: '只能有一個 Simulation Control 元件。', els: sc.map(e => e.id) });
  const names = new Map(); ED.doc.elements.forEach(e => { if (e.dis || LIB.D[e.type].noname) return; if (names.has(e.name)) errors.push({ msg: '元件名稱重複：' + e.name, els: [e.id, names.get(e.name)] }); else names.set(e.name, e.id); });
  let paramText = ''; els.forEach(e => { if (e.type === 'PARAMS') paramText += (e.params.text || '') + '\n'; });
  const pt = SIM.parseParamText(paramText); pt.errors.forEach(m => errors.push({ msg: '參數檔：' + m, els: els.filter(e => e.type === 'PARAMS').map(e => e.id) }));
  const net = { nets: netList, elements: elems.map(({ el, pins }) => ({ id: el.id, type: el.type, name: el.name, params: el.params, pins })), paramText };
  return { net, errors, warnings, syms: pt.syms };
}
function connectivityFor(els) {
  const saved = ED.doc.elements; const savedConn = ED.conn;
  ED.doc.elements = els; ED.conn = null; const c = connectivity(); ED.doc.elements = saved; ED.conn = savedConn; return c;
}

/* ------------------------------------------------------------------
 * Status bar & messages
 * ------------------------------------------------------------------ */
function updateStatus() {
  const n = ED.doc.elements.length, w = ED.doc.wires.length;
  let sel = '';
  if (ED.sel.size === 1) { const el = selectedEls()[0]; if (el) { const d = defOf(el); const ps = d.params.filter(p => p.type !== 'code' && (d.show || []).includes(p.k)).slice(0, 3).map(p => p.k + '=' + el.params[p.k]).join(' '); sel = ` · 已選 ${el.name || d.name}${ps ? '（' + ps + '）' : ''}`; } else sel = ' · 已選 1 條導線'; }
  else if (ED.sel.size) sel = ` · 已選 ${ED.sel.size}`;
  $('#count').textContent = `${n} 元件 · ${w} 導線` + sel;
  $('#fname').textContent = ED.fileName + (ED.dirty ? ' *' : '');
}
function showMessages(errors, warnings, title) {
  closeMessages();
  if (!errors.length && !warnings.length) return;
  const box = h('div', { id: 'msgs', role: 'alert' }, h('h4', null, title || (errors.length ? '無法執行模擬' : '模擬警告'), h('button', { class: 'tb', onclick: closeMessages, 'aria-label': '關閉' }, '×')));
  errors.forEach(e => { const m = h('div', { class: 'm err' }, e.msg); if (e.els && e.els.length) { m.dataset.el = e.els.join(','); m.onclick = () => focusEls(e.els); } box.append(m); });
  warnings.forEach(wm => box.append(h('div', { class: 'm warn' }, wm)));
  $('#stage').append(box);
  const pill = $('#msgPill'); pill.hidden = false; pill.className = 'pill ' + (errors.length ? 'err' : 'warn'); pill.textContent = errors.length ? errors.length + ' 個錯誤' : warnings.length + ' 個警告';
  pill.onclick = () => showMessages(errors, warnings, title);
}
function closeMessages() { const m = $('#msgs'); if (m) m.remove(); }
function focusEls(ids) {
  ED.sel.clear(); ids.forEach(id => { if (elById(id)) ED.sel.add('e:' + id); });
  const els = ids.map(elById).filter(Boolean); if (!els.length) return;
  const b = els.map(wbox); const cx = (Math.min(...b.map(q => q.x0)) + Math.max(...b.map(q => q.x1))) / 2, cy = (Math.min(...b.map(q => q.y0)) + Math.max(...b.map(q => q.y1))) / 2;
  ED.vb.x = cx - ED.vb.w / 2; ED.vb.y = cy - ED.vb.h / 2; render();
}

/* ------------------------------------------------------------------
 * Find in schematic (element names, types, node labels, text)
 * ------------------------------------------------------------------ */
const FIND = { q: '', hits: [], i: -1, on: false };
function findHit(el) { return FIND.on && FIND.hits.length > 1 && FIND.hits.includes(el.id) && !ED.sel.has('e:' + el.id); }
function openFind() {
  let bar = $('#findBar');
  if (!bar) {
    bar = h('div', { id: 'findBar', role: 'search' },
      h('input', { id: 'findQ', type: 'search', placeholder: '搜尋元件名稱、類型或標籤', autocomplete: 'off', 'aria-label': '搜尋電路圖' }),
      h('span', { id: 'findCnt', class: 'mono' }),
      h('button', { class: 'tb', title: '上一個 (Shift+Enter)', 'aria-label': '上一個', html: '<svg viewBox="0 0 24 24"><path d="M6 15l6-6 6 6"/></svg>', onclick: () => findStep(-1) }),
      h('button', { class: 'tb', title: '下一個 (Enter)', 'aria-label': '下一個', html: '<svg viewBox="0 0 24 24"><path d="M6 9l6 6 6-6"/></svg>', onclick: () => findStep(1) }),
      h('button', { class: 'tb', title: '關閉 (Esc)', 'aria-label': '關閉搜尋', html: '<svg viewBox="0 0 24 24"><path d="M6 6l12 12M18 6L6 18"/></svg>', onclick: closeFind }));
    $('#stage').append(bar);
    const q = $('#findQ');
    q.addEventListener('input', () => findRun(q.value));
    q.addEventListener('keydown', e => { if (e.key === 'Enter') { e.preventDefault(); findStep(e.shiftKey ? -1 : 1); } else if (e.key === 'Escape') { e.preventDefault(); closeFind(); } e.stopPropagation(); });
  }
  bar.hidden = false; FIND.on = true;
  const q = $('#findQ'); q.focus(); q.select(); if (q.value.trim()) findRun(q.value);
}
function findRun(qs) {
  const q = String(qs || '').trim().toLowerCase(); FIND.q = q; FIND.hits = []; FIND.i = -1;
  if (q) {
    const words = q.split(/\s+/);
    const score = el => {
      const d = defOf(el); const name = String(el.name || '').toLowerCase(); const txt = (d.label || d.text) ? String(el.params.text || '').toLowerCase() : '';
      if (name === q || txt === q) return 4;
      if (name.includes(q) || txt.includes(q)) return 3;
      const hay = name + ' ' + txt + ' ' + (el.type + ' ' + d.name + ' ' + d.zh).toLowerCase();
      return words.every(w => hay.includes(w)) ? 1 : 0;
    };
    FIND.hits = ED.doc.elements.map(el => [el, score(el)]).filter(x => x[1] > 0).sort((a, b) => b[1] - a[1] || a[0].y - b[0].y || a[0].x - b[0].x).map(x => x[0].id);
  }
  if (FIND.hits.length) findStep(1, true);
  else { $('#findCnt').textContent = q ? '找不到' : ''; render(); }
}
function findStep(dir, first) {
  if (!FIND.hits.length) { const q = $('#findQ'); if (q && q.value.trim() && FIND.q !== q.value.trim().toLowerCase()) findRun(q.value); return; }
  FIND.i = first ? 0 : (FIND.i + dir + FIND.hits.length) % FIND.hits.length;
  const el = elById(FIND.hits[FIND.i]); if (!el) return;
  ED.sel.clear(); ED.sel.add('e:' + el.id); centerOn(el);
  $('#findCnt').textContent = (FIND.i + 1) + ' / ' + FIND.hits.length;
  showLibDescFor(el.type);
}
function centerOn(el) {
  const b = wbox(el); const vb = ED.vb; const r = svg.getBoundingClientRect();
  if (vb.w > 90) vb.w = 60;
  const hh = vb.w * r.height / Math.max(1, r.width); const cx = (b.x0 + b.x1) / 2, cy = (b.y0 + b.y1) / 2;
  if (!(cx > vb.x + vb.w * 0.15 && cx < vb.x + vb.w * 0.85 && cy > vb.y + hh * 0.15 && cy < vb.y + hh * 0.85)) { vb.x = cx - vb.w / 2; vb.y = cy - hh / 2; }
  render();
}
function closeFind() { const bar = $('#findBar'); if (bar) bar.hidden = true; FIND.on = false; FIND.hits = []; render(); const a = document.activeElement; if (a && a.id === 'findQ') a.blur(); }

/* ------------------------------------------------------------------
 * Quick add: type an element name, Enter places it (double-click on empty
 * canvas, the / key, or the + button)
 * ------------------------------------------------------------------ */
function openQuickAdd(cx, cy, at) {
  closeQuickAdd(); hideTip(); closeInline();
  const box = h('div', { id: 'qadd', role: 'dialog', 'aria-label': '快速新增元件' });
  const inp = h('input', { type: 'search', placeholder: '輸入元件名稱，例如 mos、比較器、tl431', autocomplete: 'off', spellcheck: 'false' });
  const list = h('div', { class: 'ql' });
  box.append(h('div', { class: 'qh' }, '新增元件', h('span', null, '↑↓ 選擇　Enter 放置　Esc 關閉')), inp, list);
  document.body.append(box);
  box.style.left = Math.min(innerWidth - 380, Math.max(4, cx - 20)) + 'px'; box.style.top = Math.min(innerHeight - 340, Math.max(4, cy - 20)) + 'px';
  let items = [], cur = 0;
  const recent = (() => { try { return JSON.parse(store.get('psimweb.recent') || '[]'); } catch (e) { return []; } })();
  const score = (t, q, words) => {
    const d = LIB.D[t]; const name = (t + ' ' + d.name).toLowerCase(), zh = d.zh.toLowerCase();
    if (t.toLowerCase() === q || d.name.toLowerCase() === q) return 5;
    if (name.startsWith(q) || zh.startsWith(q)) return 4;
    if (name.includes(q) || zh.includes(q)) return 3;
    const hay = name + ' ' + zh + ' ' + d.desc.toLowerCase() + ' ' + d.cat.toLowerCase();
    return words.every(w => hay.includes(w)) ? 1 : 0;
  };
  const refresh = () => {
    const q = inp.value.trim().toLowerCase();
    if (!q) items = recent.filter(t => LIB.D[t]).concat(['R', 'L', 'C', 'DIODE', 'MOSFET', 'VDC', 'VSIN', 'GND', 'VP', 'IP', 'LABEL'].filter(t => !recent.includes(t))).slice(0, 10);
    else { const words = q.split(/\s+/); items = LIB.ORDER.map(t => [t, score(t, q, words)]).filter(x => x[1] > 0).sort((a, b) => b[1] - a[1]).slice(0, 10).map(x => x[0]); }
    cur = 0; draw();
  };
  const draw = () => {
    list.innerHTML = '';
    if (!items.length) { list.append(h('div', { class: 'libEmpty' }, '沒有符合的元件')); return; }
    items.forEach((t, i) => { const d = LIB.D[t]; const it = h('div', { class: 'qi' + (i === cur ? ' on' : ''), html: iconSvg(t, 40, 26) }, h('div', { class: 'nm' }, d.name, h('small', null, d.zh)));
      it.onmouseenter = () => { cur = i; draw(); }; it.onmousedown = e => { e.preventDefault(); cur = i; pick(); }; list.append(it); });
    const on = list.querySelector('.qi.on'); if (on && on.scrollIntoView) on.scrollIntoView({ block: 'nearest' });
  };
  const pick = () => {
    const t = items[cur]; if (!t) return;
    closeQuickAdd();
    if (at) { const d = LIB.D[t]; if (d.single && ED.doc.elements.some(e => e.type === t)) { startPlace(t); return; } noteRecent(t); const g = { type: t, rot: 0, mir: false, dis: false, params: defaultParams(t) }; const el = placeElement(g, at[0], at[1]); if (el) { showLibDesc(t); toast('已放置 ' + (el.name || LIB.D[t].name) + '；Space 旋轉、雙擊編輯屬性'); } }
    else { startPlace(t); touchPlaceHint(); }
  };
  inp.addEventListener('input', refresh);
  inp.addEventListener('keydown', e => {
    e.stopPropagation();
    if (e.key === 'ArrowDown') { e.preventDefault(); cur = Math.min(items.length - 1, cur + 1); draw(); }
    else if (e.key === 'ArrowUp') { e.preventDefault(); cur = Math.max(0, cur - 1); draw(); }
    else if (e.key === 'Enter') { e.preventDefault(); pick(); }
    else if (e.key === 'Escape') { e.preventDefault(); closeQuickAdd(); }
  });
  inp.addEventListener('blur', () => setTimeout(() => { if ($('#qadd') === box && document.activeElement !== inp) closeQuickAdd(); }, 120));
  refresh(); inp.focus();
}
function closeQuickAdd() { const b = $('#qadd'); if (b) b.remove(); }

/* =====================================================================
 * 電路模擬器 — simulation runner, waveform viewer, Bode viewer,
 * file I/O, examples and start-up
 * ===================================================================== */
'use strict';

/* ------------------------------------------------------------------
 * Running a simulation (Web Worker with main-thread fallback)
 * ------------------------------------------------------------------ */
const WORKER_SHIM = `
self.onmessage = function (e) {
  var d = e.data, R;
  try {
    if (d.kind === 'tran') R = new SIM.TransientRunner(d.net, d.sc);
    else if (d.ac.method === 1) R = new SIM.ACLinearRunner(d.net, d.sc, d.ac);
    else R = new SIM.ACInjectRunner(d.net, d.sc, d.ac);
  } catch (err) { self.postMessage({ type: 'error', msg: err.message, elId: err.elId }); return; }
  var step = function () {
    var done;
    try { done = R.run(150); } catch (err) { self.postMessage({ type: 'error', msg: err.message, elId: err.elId }); return; }
    if (done) {
      var res = R.result(); var tr = [];
      if (res.data) res.data = res.data.map(function (a) { var b = new Float64Array(a); tr.push(b.buffer); return b; });
      self.postMessage({ type: 'done', res: res }, tr);
    } else { self.postMessage({ type: 'prog', p: R.progress(), t: R.sim ? R.sim.t : (R.A ? R.A.t : 0), f: R.cur ? R.cur.f : null }); setTimeout(step, 0); }
  };
  step();
};`;
let RUN = null;
function engineSource() { return $('#engine-src').textContent; }
function evalSimCtrl(syms) {
  const el = ED.doc.elements.find(e => e.type === 'SIMCTRL' && !e.dis);
  if (!el) throw { msg: '電路中沒有 Simulation Control 元件，請由「模擬 → 模擬控制」(F7) 設定時間步長與總時間。' };
  const g = k => { try { return SIM.evalConst(el.params[k], syms); } catch (e) { throw { msg: 'Simulation Control：' + k + ' ' + e.message, els: [el.id] }; } };
  const sc = { dt: g('dt'), tTotal: g('tt'), tPrint: g('tp'), printStep: Math.max(1, Math.round(g('ps'))), interp: String(el.params.interp) !== '0', damp: String(el.params.damp) !== '0', lte: String(el.params.lte || '1') !== '0', lteTol: el.params.ltetol ? g('ltetol') : 1e-3 };
  if (!(sc.dt > 0) || !(sc.tTotal > 0)) throw { msg: 'Simulation Control：時間步長與總時間必須大於 0', els: [el.id] };
  if (sc.dt < 1e-12 * 0.9999) throw { msg: 'Simulation Control：時間步長最小為 1 ps', els: [el.id] };
  return sc;
}
function evalAC(syms) {
  const el = ED.doc.elements.find(e => e.type === 'ACSWEEP' && !e.dis); if (!el) return null;
  const P = el.params; const g = k => { try { return SIM.evalConst(P[k], syms); } catch (e) { throw { msg: 'AC Sweep：' + k + ' ' + e.message, els: [el.id] }; } };
  let extra = []; try { extra = SIM.evalList(P.extra || '', syms); } catch (e) { throw { msg: 'AC Sweep：額外頻率點格式錯誤', els: [el.id] }; }
  const ac = { fStart: g('f0'), fEnd: g('f1'), nPts: Math.round(g('n')), flag: +P.flag || 0, src: String(P.src || '').trim(), aStart: g('a0'), aEnd: g('a1'), extra, method: +P.method || 0,
    settle: Math.max(1, g('settle')), mcyc: Math.max(1, Math.round(g('mcyc'))), maxCyc: Math.max(4, g('maxc')), tol: Math.max(1e-7, g('tol') / 100), tPre: g('tpre'), tSettle: g('tset'), baseline: +P.base === 0 ? 0 : 1 };
  if (ac.fEnd > 1e6 * 1.000001) throw { msg: 'AC Sweep：結束頻率上限為 1 MHz', els: [el.id] };
  if (!(ac.fStart > 0) || !(ac.fEnd > ac.fStart)) throw { msg: 'AC Sweep：頻率範圍設定錯誤', els: [el.id] };
  if (!ac.src) throw { msg: 'AC Sweep：請輸入激勵源名稱', els: [el.id] };
  return ac;
}
function runSim(forceTran) {
  if (RUN) return;
  closeMessages(); ED.errEls.clear();
  const nl = buildNetlist();
  let sc, ac = null;
  const fail = (errs) => { errs.forEach(e => (e.els || []).forEach(id => ED.errEls.add(id))); render(); showMessages(errs, []); };
  if (nl.errors.length) return fail(nl.errors);
  try { sc = evalSimCtrl(nl.syms); if (!forceTran) ac = evalAC(nl.syms); } catch (e) { return fail([e]); }
  const kind = ac ? 'ac' : 'tran';
  const payload = { kind, net: nl.net, sc, ac };
  const title = ac ? (ac.method === 1 ? 'AC 掃頻（線性頻域法）' : 'AC 掃頻（小訊號注入法）') : '暫態模擬';
  const t0 = performance.now();
  const bar = h('i'); const pct = h('span', { class: 'mono' }, '0%'); const tinfo = h('span', { class: 'mono' }, ''); const eta = h('span', { class: 'mono' }, '');
  const body = h('div', { class: 'prog' }, h('div', { class: 'bar' }, bar), h('div', { class: 'progInfo' }, pct, tinfo, eta),
    h('p', { class: 'note' }, ac ? '頻率 ' + siFmt(ac.fStart, 'Hz', 3) + ' ~ ' + siFmt(ac.fEnd, 'Hz', 3) + '，' + ac.nPts + ' 點' : '步長 ' + siFmt(sc.dt, 's') + '，總時間 ' + siFmt(sc.tTotal, 's') + '，共 ' + Math.round(sc.tTotal / sc.dt).toLocaleString() + ' 步'));
  let worker = null, cancelled = false, fallback = null;
  const dlg = modal(title + '中…', body, [{ label: '中止', danger: true, fn: () => { cancelled = true; stop(); } }], { sticky: true, onClose: () => { if (RUN) { cancelled = true; stop(); } } });
  const stop = () => { if (worker) worker.terminate(); worker = null; if (fallback) clearTimeout(fallback.t); RUN = null; };
  const onProg = (p, t, f) => {
    const el = (performance.now() - t0) / 1000; bar.style.width = (100 * p).toFixed(1) + '%'; pct.textContent = (100 * p).toFixed(1) + '%';
    tinfo.textContent = f ? 'f = ' + siFmt(f, 'Hz', 4) : 't = ' + siFmt(t || 0, 's', 4);
    eta.textContent = p > 0.005 ? '剩餘約 ' + fmtDur(el * (1 - p) / p) : '已執行 ' + fmtDur(el);
  };
  const onDone = (res) => {
    RUN = null; if (worker) worker.terminate(); worker = null; dlg.close();
    const secs = (performance.now() - t0) / 1000;
    if (res.kind === 'tran') {
      ED.lastRes = res; const st = res.stats || {};
      let info = '模擬完成 ' + fmtDur(secs) + (res.events ? '，切換事件 ' + res.events + ' 次' : '') + (st.refined ? '，自動細分 ' + st.refined + ' 步' : '');
      if (st.refineMax && st.steps && st.refineMax > 0.02 * st.steps) { info += '；建議縮小時間步長'; res.warnings = res.warnings.concat(['有 ' + st.refineMax + ' 個時間步即使細分到 1/16 仍超過誤差容許值（' + (100 * st.refineMax / st.steps).toFixed(1) + '% 的步數），建議縮小 Simulation Control 的時間步長。']); }
      toastPill('ok', info); if (!sendToPopups(res)) openWave(res);
    }
    else { ED.lastAC = res; toastPill('ok', 'AC 掃頻完成 ' + fmtDur(secs)); openBode(res); }
    const warns = (res.warnings || []).slice(0, 50);
    if (warns.length) showMessages([], warns, '模擬完成，但有警告');
  };
  const onErr = (msg, elId) => {
    RUN = null; if (worker) worker.terminate(); worker = null; dlg.close();
    const errs = [{ msg, els: elId != null ? [elId] : [] }]; fail(errs);
  };
  RUN = { stop };
  try {
    const blob = new Blob([engineSource() + '\n' + WORKER_SHIM], { type: 'text/javascript' });
    const url = URL.createObjectURL(blob);
    worker = new Worker(url); URL.revokeObjectURL(url);
    worker.onmessage = e => { const m = e.data; if (cancelled) return; if (m.type === 'prog') onProg(m.p, m.t, m.f); else if (m.type === 'done') onDone(m.res); else if (m.type === 'error') onErr(m.msg, m.elId); };
    worker.onerror = e => { e.preventDefault(); if (!cancelled) { worker.terminate(); worker = null; runMain(); } };
    worker.postMessage(payload);
  } catch (e) { worker = null; runMain(); }
  function runMain() {
    // main-thread fallback in small chunks so the progress bar keeps updating
    let R;
    try { R = kind === 'tran' ? new SIM.TransientRunner(nl.net, sc) : (ac.method === 1 ? new SIM.ACLinearRunner(nl.net, sc, ac) : new SIM.ACInjectRunner(nl.net, sc, ac)); }
    catch (err) { onErr(err.message, err.elId); return; }
    fallback = { t: 0 };
    const step = () => {
      if (cancelled) return; let done;
      try { done = R.run(60); } catch (err) { onErr(err.message, err.elId); return; }
      if (done) onDone(R.result()); else { onProg(R.progress(), R.sim ? R.sim.t : (R.A ? R.A.t : 0), R.cur ? R.cur.f : null); fallback.t = setTimeout(step, 0); }
    };
    step();
  }
}
function fmtDur(s) { if (s < 1) return (s * 1000).toFixed(0) + ' ms'; if (s < 60) return s.toFixed(1) + ' s'; const m = Math.floor(s / 60); return m + ' 分 ' + Math.round(s - 60 * m) + ' 秒'; }
function toastPill(kind, msg) { const p = $('#msgPill'); p.hidden = false; p.className = 'pill ' + kind; p.textContent = msg; p.onclick = null; }
function openLastResult() { if (ED.lastRes) openWave(ED.lastRes); else if (ED.lastAC) openBode(ED.lastAC); else toast('尚無模擬結果，請先按 F5 執行模擬'); }

/* ------------------------------------------------------------------
 * Plot helpers
 * ------------------------------------------------------------------ */
function cssv(n) { return getComputedStyle(document.documentElement).getPropertyValue(n).trim(); }
function curveColor(i) { return cssv('--c' + ((i % 8) + 1)); }
function niceTicks(a, b, n) {
  if (!(b > a)) { const d = Math.abs(a) || 1; a -= d * 0.5; b += d * 0.5; }
  const span = b - a, raw = span / Math.max(1, n), p = Math.pow(10, Math.floor(Math.log10(raw))), m = raw / p;
  const step = (m < 1.5 ? 1 : m < 3 ? 2 : m < 7 ? 5 : 10) * p;
  const out = []; for (let v = Math.ceil(a / step) * step; v <= b + step * 1e-9; v += step) out.push(Math.abs(v) < step * 1e-9 ? 0 : v);
  return { ticks: out, step };
}
function fmtTick(v, step) {
  if (v === 0) return '0';
  const a = Math.max(Math.abs(v), Math.abs(step));
  const P = [[1e9, 'G'], [1e6, 'M'], [1e3, 'k'], [1, ''], [1e-3, 'm'], [1e-6, 'µ'], [1e-9, 'n'], [1e-12, 'p']];
  for (const [m, s] of P) if (a >= m * 0.999) { const dec = Math.max(0, Math.min(6, Math.ceil(-Math.log10(step / m) - 1e-9))); return (v / m).toFixed(dec) + s; }
  return v.toExponential(1);
}
function setupCanvas(cv) {
  const r = cv.getBoundingClientRect(); const dpr = window.devicePixelRatio || 1;
  const W = Math.max(10, Math.round(r.width * dpr)), H = Math.max(10, Math.round(r.height * dpr));
  if (cv.width !== W || cv.height !== H) { cv.width = W; cv.height = H; }
  const ctx = cv.getContext('2d'); ctx.setTransform(dpr, 0, 0, dpr, 0, 0);
  return { ctx, w: r.width, h: r.height };
}

/* ------------------------------------------------------------------
 * Waveform viewer
 * ------------------------------------------------------------------ */
const WV = { res: null, screens: [], act: 0, xa: 0, xb: 1, cur: false, c1: null, c2: null, hover: null };
function openWave(res) {
  WV.res = res;
  const n = res.names.length;
  // keep previous screen layout when the same probes exist
  const prev = WV.layout && WV.layout.names.join('|') === res.names.join('|') ? WV.layout : null;
  const want = !prev && ED.docLayout ? ED.docLayout.map(g => g.map(nm => res.names.indexOf(nm)).filter(i => i >= 0)).filter(g => g.length) : null;
  if (prev) WV.screens = prev.screens.map(s => ({ curves: s.curves.slice(), y: null }));
  else if (want && want.length) { WV.screens = want.map(g => ({ curves: g, y: null })); const seen = new Set(want.flat()); const rest = []; for (let i = 0; i < n; i++) if (!seen.has(i)) rest.push(i); if (rest.length) WV.screens.push({ curves: rest, y: null }); }
  else if (n <= 4) { WV.screens = []; for (let i = 0; i < n; i++) WV.screens.push({ curves: [i], y: null }); if (!WV.screens.length) WV.screens.push({ curves: [], y: null }); }
  else {
    // many probes: group by kind of quantity (name prefix V / I / P), at most 4 screens
    const cls = nm => /^v/i.test(nm) ? 0 : /^i/i.test(nm) ? 1 : /^[pe]/i.test(nm) ? 2 : 3; const g = [[], [], [], []];
    for (let i = 0; i < n; i++) g[cls(res.names[i])].push(i);
    WV.screens = g.filter(x => x.length).map(x => ({ curves: x, y: null }));
  }
  WV.act = 0; WV.xa = res.t0; WV.xb = res.t0 + Math.max(1, res.n - 1) * res.dt; WV.c1 = WV.c2 = null;
  const v = $('#wave'); v.hidden = false; applyWaveStyle(); buildWaveUI(); drawWave();
}
function setAct(si) { if (WV.act === si) return; WV.act = si; $$('#wPlots .scr').forEach((x, j) => x.classList.toggle('act', j === si)); drawMeas(); }
/* ---------- waveform appearance: fonts, line width, per-signal colours (persisted per probe name) ---------- */
const WFONTS = [
  ['mono', '等寬 IBM Plex Mono', '"IBM Plex Mono", Consolas, "SFMono-Regular", ui-monospace, monospace'],
  ['consolas', 'Consolas', 'Consolas, "Courier New", monospace'],
  ['courier', 'Courier New', '"Courier New", Courier, monospace'],
  ['sans', '系統無襯線', 'system-ui, "Segoe UI", Arial, sans-serif'],
  ['jhenghei', '微軟正黑體', '"Microsoft JhengHei", "PingFang TC", "Noto Sans TC", sans-serif'],
  ['arial', 'Arial', 'Arial, Helvetica, sans-serif'],
  ['times', 'Times New Roman', '"Times New Roman", Times, serif']];
const WSTY_DEF = { font: 'mono', size: 11, lw: 1.4, colors: {} };
let WSTY = loadWaveStyle();
function loadWaveStyle() { try { const o = JSON.parse(store.get('psimweb.waveStyle') || 'null'); if (o) return Object.assign({}, WSTY_DEF, o, { colors: Object.assign({}, o.colors || {}) }); } catch (e) { /* ignore */ } return JSON.parse(JSON.stringify(WSTY_DEF)); }
function saveWaveStyle() { store.set('psimweb.waveStyle', JSON.stringify(WSTY)); }
function wFont() { return (WFONTS.find(f => f[0] === WSTY.font) || WFONTS[0])[2]; }
function wColor(ci) { const n = WV.res && WV.res.names[ci]; return (n && WSTY.colors[n]) || curveColor(ci); }
function applyWaveStyle() {
  const v = $('#wave'); if (!v) return;
  v.style.setProperty('--wfont', wFont()); v.style.setProperty('--wsize', WSTY.size + 'px');
  PAD = { l: Math.round(WSTY.size * 5.6 + 0.5), r: 14, t: WSTY.size + 13, b: WSTY.size + 11 };
}
function restyleWave() { applyWaveStyle(); if (WV.res && !$('#wave').hidden) { buildWaveUI(); drawWave(); } }
window.addEventListener('storage', e => { if (e.key === 'psimweb.waveStyle') { WSTY = loadWaveStyle(); restyleWave(); } });
const PALETTE = ['#1c6fd1', '#e8590c', '#2b8a3e', '#c2255c', '#6741d9', '#0c8599', '#b8860b', '#495057',
  '#e03131', '#f08c00', '#37b24d', '#1971c2', '#9c36b5', '#0ca678', '#d6336c', '#5c940d',
  '#364fc7', '#862e9c', '#a61e4d', '#212529', '#74c0fc', '#ffa94d', '#8ce99a', '#faa2c1'];
function closePicker() { const p = $('#cpick'); if (p) p.remove(); }
// colour picker: swatch grid + custom colour + hex; onPick(null) = back to default
function pickColor(anchor, cur, onPick) {
  closePicker();
  const pop = h('div', { id: 'cpick', class: 'cpick', role: 'dialog', 'aria-label': '選擇顏色' });
  const grid = h('div', { class: 'cgrid' });
  PALETTE.forEach(c => grid.append(h('button', { class: 'cs' + (c.toLowerCase() === String(cur).toLowerCase() ? ' on' : ''), title: c, 'aria-label': c, style: 'background:' + c, onclick: () => { onPick(c); closePicker(); } })));
  const nat = h('input', { type: 'color', value: /^#[0-9a-f]{6}$/i.test(cur) ? cur : '#1c6fd1', title: '自訂顏色（調色盤）' });
  const hex = h('input', { type: 'text', value: cur, spellcheck: 'false', 'aria-label': '色碼', class: 'mono' });
  nat.oninput = () => { hex.value = nat.value; onPick(nat.value); };
  hex.onchange = () => { const v = hex.value.trim(); if (/^#?[0-9a-f]{6}$/i.test(v)) { const c = v[0] === '#' ? v : '#' + v; nat.value = c; onPick(c); } };
  pop.append(grid, h('div', { class: 'crow' }, h('label', null, '自訂', nat), hex, h('button', { class: 'btn', onclick: () => { onPick(null); closePicker(); } }, '預設')));
  document.body.append(pop);
  const r = anchor.getBoundingClientRect(), pr = pop.getBoundingClientRect();
  pop.style.left = Math.max(8, Math.min(innerWidth - pr.width - 8, r.left)) + 'px';
  pop.style.top = (r.bottom + pr.height + 8 > innerHeight ? Math.max(8, r.top - pr.height - 6) : r.bottom + 6) + 'px';
  setTimeout(() => { const off = e => { if (!pop.contains(e.target)) { closePicker(); document.removeEventListener('mousedown', off, true); } }; document.addEventListener('mousedown', off, true); }, 0);
}
function colorFor(anchor, ci) {
  const name = WV.res.names[ci];
  pickColor(anchor, wColor(ci), c => { if (c) WSTY.colors[name] = c; else delete WSTY.colors[name]; saveWaveStyle(); anchor.style.background = wColor(ci); buildWaveUI(); drawWave(); });
}
function waveStyleDialog() {
  const fSel = h('select', { id: 'ws_font' }); WFONTS.forEach(([k, l, st]) => { const o = h('option', { value: k, style: 'font-family:' + st }, l); if (k === WSTY.font) o.selected = true; fSel.append(o); });
  const zSel = h('select', { id: 'ws_size' }); [9, 10, 11, 12, 13, 14, 16, 18, 20].forEach(z => { const o = h('option', { value: z }, z + ' px'); if (z === WSTY.size) o.selected = true; zSel.append(o); });
  const lSel = h('select', { id: 'ws_lw' }); [[1, '細 1 px'], [1.4, '標準 1.4 px'], [2, '粗 2 px'], [2.5, '2.5 px'], [3, '特粗 3 px']].forEach(([v, l]) => { const o = h('option', { value: v }, l); if (v === WSTY.lw) o.selected = true; lSel.append(o); });
  const live = () => { WSTY.font = fSel.value; WSTY.size = +zSel.value; WSTY.lw = +lSel.value; saveWaveStyle(); restyleWave(); };
  [fSel, zSel, lSel].forEach(x => x.addEventListener('change', live));
  const list = h('div', { class: 'wsList' });
  const fill = () => {
    list.innerHTML = '';
    (WV.res ? WV.res.names : []).forEach((nm, ci) => {
      const sw = h('button', { class: 'sw big', title: '點選更改顏色', 'aria-label': nm + ' 顏色', style: 'background:' + wColor(ci) });
      sw.onclick = () => pickColor(sw, wColor(ci), c => { if (c) WSTY.colors[nm] = c; else delete WSTY.colors[nm]; saveWaveStyle(); sw.style.background = wColor(ci); restyleWave(); });
      list.append(h('div', { class: 'wsRow' }, sw, h('span', { class: 'mono' }, nm)));
    });
  };
  fill();
  const body = h('div', null,
    h('div', { class: 'prm' }, h('label', { for: 'ws_font' }, '字體'), fSel, h('span'), h('span'), h('label', { for: 'ws_size' }, '字體大小'), zSel, h('span'), h('span'), h('label', { for: 'ws_lw' }, '波形線寬'), lSel, h('span'), h('span')),
    h('h4', { class: 'wsH' }, '波形顏色（點色塊開啟調色盤；顏色依探棒名稱記住）'), list);
  modal('波形外觀', body, [
    { label: '全部還原預設', fn: () => { WSTY = JSON.parse(JSON.stringify(WSTY_DEF)); saveWaveStyle(); fSel.value = WSTY.font; zSel.value = WSTY.size; lSel.value = WSTY.lw; restyleWave(); fill(); return false; } },
    { label: '關閉', pri: true }], { wide: true });
}
function waveSave() { WV.layout = { names: WV.res.names.slice(), screens: WV.screens.map(s => ({ curves: s.curves.slice() })) }; }
function buildWaveUI() {
  const res = WV.res;
  const vars = $('#wVars'); vars.innerHTML = '';
  res.names.forEach((nm, i) => vars.append(h('button', { title: '加入/移除目前畫面', onclick: () => { const s = WV.screens[WV.act]; const k = s.curves.indexOf(i); if (k >= 0) s.curves.splice(k, 1); else s.curves.push(i); s.y = null; waveSave(); buildPlots(); drawWave(); } },
    h('span', { class: 'sw', title: '點選更改顏色', style: `background:${wColor(i)}`, onclick: e => { e.stopPropagation(); colorFor(e.currentTarget, i); } }), nm)));
  buildPlots();
}
function buildPlots() {
  const box = $('#wPlots'); box.innerHTML = '';
  WV.screens.forEach((s, si) => {
    const cv = h('canvas'); const lg = h('div', { class: 'legend' });
    const noSel = e => e.stopPropagation();
    s.curves.forEach(ci => lg.append(h('span', { class: 'lgc', 'data-ci': ci }, h('i', { class: 'sw', title: '點選更改顏色', style: 'background:' + wColor(ci), onmousedown: noSel, onclick: e => { e.stopPropagation(); colorFor(e.currentTarget, ci); } }), h('span', { class: 'lgn' }, WV.res.names[ci]), h('span', { class: 'lgv mono' }), h('b', { title: '從此畫面移除', onclick: (e) => { e.stopPropagation(); s.curves.splice(s.curves.indexOf(ci), 1); s.y = null; waveSave(); buildPlots(); drawWave(); } }, '×'))));
    if (!s.curves.length) lg.append(h('span', { class: 'lgEmpty' }, document.documentElement.classList.contains('coarse') ? '點「量測」面板裡的探棒訊號加入此畫面' : '點左側探棒訊號加入此畫面'));
    const rd = h('span', { class: 'rdT mono', title: '游標位置的時間' });
    const stb = h('div', { class: 'stb', role: 'toolbar', 'aria-label': '畫面 ' + (si + 1) + ' 座標軸' });
    stb.append(h('button', { class: 'stbT', title: '顯示/收起此畫面的座標軸工具', 'aria-label': '座標軸工具', onmousedown: noSel, onclick: e => { e.stopPropagation(); setAct(si); stb.classList.toggle('open'); } }, '⇔ 軸'));
    const SB = (txt, title, fn, cls) => stb.append(h('button', { class: cls || '', title, 'aria-label': title, onmousedown: noSel, onclick: e => { e.stopPropagation(); setAct(si); fn(); } }, txt));
    const SEP = () => stb.append(h('span', { class: 'ssep' }));
    SB('X−', 'X 軸縮小（顯示更長時間）', () => scrZoomX(si, 1.6)); SB('X+', 'X 軸放大', () => scrZoomX(si, 0.625));
    SB('◀', 'X 軸往左平移', () => scrPanX(si, -0.25)); SB('▶', 'X 軸往右平移', () => scrPanX(si, 0.25)); SEP();
    SB('Y−', 'Y 軸縮小', () => scrZoomY(si, 1.6)); SB('Y+', 'Y 軸放大', () => scrZoomY(si, 0.625));
    SB('▲', 'Y 軸往上平移（看較高的值）', () => scrPanY(si, 0.2)); SB('▼', 'Y 軸往下平移', () => scrPanY(si, -0.2)); SEP();
    SB('⤢', '此畫面自動範圍（雙擊波形亦可）', () => scrFit(si));
    SB('▭', '只顯示這個畫面 / 恢復全部畫面', () => { WV.solo = WV.solo === si ? null : si; buildPlots(); drawWave(); }, WV.solo === si ? 'on' : '');
    const head = h('div', { class: 'scrHead' }, h('span', { class: 'scrNo', title: '畫面 ' + (si + 1) + '（紅點＝目前畫面，探棒訊號會加入此畫面）' }, h('i'), String(si + 1)), lg, rd, stb);
    const plot = h('div', { class: 'plot' }, cv);
    const scr = h('div', { class: 'scr' + (si === WV.act ? ' act' : '') + (WV.solo != null && WV.solo !== si ? ' hid' : '') }, head, plot);
    scr.onmousedown = () => setAct(si);
    box.append(scr);
    attachPlotMouse(cv, si);
  });
  const vis = WV.solo != null ? 1 : WV.screens.length;
  box.style.gridTemplateRows = `repeat(${vis}, minmax(${innerWidth <= 760 ? 110 : 150}px, 1fr))`;
}
// readout in the screen header (hover or cursor C1): time and the value of every curve of that screen
function headReadout(si, t) {
  const scr = $$('#wPlots .scr')[si]; if (!scr) return;
  const rd = scr.querySelector('.rdT');
  if (t == null) { rd.textContent = ''; scr.querySelectorAll('.lgv').forEach(e => { e.textContent = ''; }); return; }
  rd.textContent = 't = ' + siFmt(t, 's', 5);
  scr.querySelectorAll('.lgc').forEach(e => { const ci = +e.dataset.ci; e.querySelector('.lgv').textContent = '= ' + siFmt(valAt(ci, t), '', 5); });
}
let PAD = { l: 62, r: 16, t: 10, b: 22 };
function idxRange(xa, xb) { const r = WV.res; const i0 = Math.max(0, Math.floor((xa - r.t0) / r.dt)); const i1 = Math.min(r.n - 1, Math.ceil((xb - r.t0) / r.dt)); return [i0, i1]; }
function yRange(s) {
  if (s.y) return s.y;
  const [xa, xb] = sX(s); const [i0, i1] = idxRange(xa, xb); let lo = Infinity, hi = -Infinity;
  s.curves.forEach(ci => { const d = WV.res.data[ci]; for (let i = i0; i <= i1; i++) { const v = d[i]; if (v < lo) lo = v; if (v > hi) hi = v; } });
  if (!isFinite(lo)) { lo = -1; hi = 1; }
  if (hi - lo < 1e-12 * Math.max(1, Math.abs(hi))) { const d = Math.max(Math.abs(hi) * 0.05, 1e-9); lo -= d; hi += d; }
  const m = (hi - lo) * 0.06; return [lo - m, hi + m];
}
function drawWave() {
  const cvs = $$('#wPlots canvas');
  WV.screens.forEach((s, si) => { if (WV.solo != null && WV.solo !== si) return; drawScreen(cvs[si], s, si === WV.screens.length - 1); if (!(WV.hover && WV.hover.cv === cvs[si])) headReadout(si, WV.cur && WV.c1 != null ? WV.c1 : null); });
  drawMeas();
  $('#wRange').textContent = WV.screens.some(s => s.x) ? '各畫面 X 軸獨立' : siFmt(WV.xa, 's', 4) + ' — ' + siFmt(WV.xb, 's', 4);
}
function drawScreen(cv, s) {
  if (!cv) return;
  const { ctx, w, h: H } = setupCanvas(cv);
  const pw = w - PAD.l - PAD.r, ph = H - PAD.t - PAD.b;
  ctx.clearRect(0, 0, w, H); ctx.fillStyle = cssv('--canvas'); ctx.fillRect(0, 0, w, H);
  const [xa, xb] = sX(s);
  const [ya, yb] = yRange(s); const X = t => PAD.l + (t - xa) / (xb - xa) * pw, Y = v => PAD.t + (yb - v) / (yb - ya) * ph;
  ctx.font = WSTY.size + 'px ' + wFont(); ctx.lineWidth = 1;
  const xt = niceTicks(xa, xb, Math.max(2, Math.floor(pw / 90))), yt = niceTicks(ya, yb, Math.max(2, Math.floor(ph / 32)));
  ctx.strokeStyle = cssv('--grid2'); ctx.fillStyle = cssv('--muted');
  ctx.beginPath(); xt.ticks.forEach(t => { const x = Math.round(X(t)) + 0.5; ctx.moveTo(x, PAD.t); ctx.lineTo(x, PAD.t + ph); }); yt.ticks.forEach(v => { const y = Math.round(Y(v)) + 0.5; ctx.moveTo(PAD.l, y); ctx.lineTo(PAD.l + pw, y); }); ctx.stroke();
  ctx.textAlign = 'center'; ctx.textBaseline = 'top'; xt.ticks.forEach(t => ctx.fillText(fmtTick(t, xt.step), X(t), PAD.t + ph + 4));
  ctx.textAlign = 'right'; ctx.textBaseline = 'middle'; yt.ticks.forEach(v => ctx.fillText(fmtTick(v, yt.step), PAD.l - 6, Y(v)));
  ctx.strokeStyle = cssv('--line'); ctx.strokeRect(PAD.l + 0.5, PAD.t + 0.5, pw, ph);
  // curves
  const r = WV.res; const [i0, i1] = idxRange(xa, xb);
  ctx.save(); ctx.beginPath(); ctx.rect(PAD.l, PAD.t, pw, ph); ctx.clip();
  s.curves.forEach(ci => {
    const d = r.data[ci]; ctx.strokeStyle = wColor(ci); ctx.lineWidth = WSTY.lw; ctx.beginPath();
    const n = i1 - i0 + 1;
    if (n <= pw * 2) { for (let i = i0; i <= i1; i++) { const x = X(r.t0 + i * r.dt), y = Y(d[i]); if (i === i0) ctx.moveTo(x, y); else ctx.lineTo(x, y); } }
    else {
      // min/max per pixel column
      const cols = Math.ceil(pw); let first = true;
      for (let c = 0; c < cols; c++) {
        const ta = xa + (c / pw) * (xb - xa), tb = xa + ((c + 1) / pw) * (xb - xa);
        let a = Math.max(i0, Math.floor((ta - r.t0) / r.dt)), b = Math.min(i1, Math.ceil((tb - r.t0) / r.dt));
        if (b < a) continue; let lo = Infinity, hi = -Infinity; for (let i = a; i <= b; i++) { const v = d[i]; if (v < lo) lo = v; if (v > hi) hi = v; }
        const x = PAD.l + c + 0.5; if (first) { ctx.moveTo(x, Y(d[a])); first = false; }
        ctx.lineTo(x, Y(lo)); ctx.lineTo(x, Y(hi));
      }
    }
    ctx.stroke();
  });
  ctx.restore();
  // cursors
  const coarse = document.documentElement.classList.contains('coarse'); const hw = coarse ? 16 : 10, hh = coarse ? 22 : 15;
  const cur = (t, lab, col, sel) => {
    if (t == null || t < xa || t > xb) return; const x = Math.round(X(t)) + 0.5;
    ctx.strokeStyle = col; ctx.lineWidth = sel ? 2 : 1; ctx.setLineDash([5, 4]); ctx.beginPath(); ctx.moveTo(x, PAD.t); ctx.lineTo(x, PAD.t + ph); ctx.stroke(); ctx.setLineDash([]); ctx.lineWidth = 1;
    // handle (tab) at the top of the line: the drag target
    ctx.fillStyle = col; ctx.beginPath(); ctx.roundRect(x - hw, PAD.t, 2 * hw, hh, [0, 0, 5, 5]); ctx.fill();
    ctx.fillStyle = '#fff'; ctx.font = 'bold ' + Math.min(WSTY.size, hh - 4) + 'px ' + wFont(); ctx.textAlign = 'center'; ctx.textBaseline = 'middle'; ctx.fillText(lab, x, PAD.t + hh / 2);
  };
  if (WV.cur) { cur(WV.c1, 'C1', cssv('--accent'), WV.csel !== 2); cur(WV.c2, 'C2', cssv('--focus'), WV.csel === 2); }
  if (WV.hover && WV.hover.cv === cv) { const x = Math.round(X(WV.hover.t)) + 0.5; ctx.strokeStyle = cssv('--faint'); ctx.beginPath(); ctx.moveTo(x, PAD.t); ctx.lineTo(x, PAD.t + ph); ctx.stroke(); }
  if (WV.zoomBox && WV.zoomBox.cv === cv) {
    const z = WV.zoomBox; ctx.fillStyle = cssv('--accent-soft');
    if (z.rect) { const x = Math.min(z.x0, z.x1), y = Math.min(z.y0, z.y1), bw = Math.abs(z.x1 - z.x0), bh = Math.abs(z.y1 - z.y0); ctx.fillRect(x, y, bw, bh); ctx.strokeStyle = cssv('--accent'); ctx.setLineDash([4, 3]); ctx.strokeRect(x + 0.5, y + 0.5, bw, bh); ctx.setLineDash([]); }
    else ctx.fillRect(Math.min(z.x0, z.x1), PAD.t, Math.abs(z.x1 - z.x0), ph);
  }
}
function valAt(ci, t) { const r = WV.res; const f = (t - r.t0) / r.dt; const i = Math.max(0, Math.min(r.n - 1, Math.floor(f))); const j = Math.min(r.n - 1, i + 1); const a = f - i; return r.data[ci][i] + (r.data[ci][j] - r.data[ci][i]) * Math.max(0, Math.min(1, a)); }
// which cursor (1 / 2) is within tol px of the pointer on this plot, 0 when none; the top handle counts over its full width
function cursorNear(cv, si, cx, cy, tol) {
  if (!WV.cur) return 0; const r = cv.getBoundingClientRect(); const pw = r.width - PAD.l - PAD.r; const [xa, xb] = sX(WV.screens[si]);
  const px = t => r.left + PAD.l + (t - xa) / (xb - xa) * pw; const y = cy - r.top;
  let best = 0, bd = Infinity;
  for (const [w, t] of [[1, WV.c1], [2, WV.c2]]) { if (t == null) continue; const d = Math.abs(cx - px(t)); const lim = y < PAD.t + 26 ? Math.max(tol, 20) : tol; if (d <= lim && d < bd) { bd = d; best = w; } }
  return best;
}
function attachPlotMouse(cv, si) {
  const S = () => WV.screens[si];
  const tAt = e => { const r = cv.getBoundingClientRect(); const pw = r.width - PAD.l - PAD.r; const [xa, xb] = sX(S()); return xa + (e.clientX - r.left - PAD.l) / pw * (xb - xa); };
  const vAt = e => { const r = cv.getBoundingClientRect(); const ph = r.height - PAD.t - PAD.b; const [a, b] = yRange(S()); return b - (e.clientY - r.top - PAD.t) / ph * (b - a); };
  cv.addEventListener('mousedown', e => {
    // pan: middle button or Alt + left drag (X and Y of this screen)
    if (e.button === 1 || (e.button === 0 && e.altKey)) {
      e.preventDefault(); const r = cv.getBoundingClientRect();
      WV.pan = { si, x: e.clientX, y: e.clientY, xr: sX(S()).slice(), yr: yRange(S()).slice(), pw: r.width - PAD.l - PAD.r, ph: r.height - PAD.t - PAD.b }; cv.style.cursor = 'grabbing'; return;
    }
    if (e.button !== 0) return;
    if (WV.cur) {
      const near = cursorNear(cv, si, e.clientX, e.clientY, 10);
      if (near) { WV.cdrag = { si, which: near, tAt }; WV.csel = near; cv.style.cursor = 'ew-resize'; drawWave(); return; }
      if (!WV.tool) { const t = tAt(e); if (e.shiftKey || WV.csel === 2) WV.c2 = t; else WV.c1 = t; drawWave(); return; }
    }
    const r = cv.getBoundingClientRect();
    if (WV.tool === 'hand') {
      e.preventDefault();
      WV.pan = { si, x: e.clientX, y: e.clientY, xr: sX(S()).slice(), yr: yRange(S()).slice(), pw: r.width - PAD.l - PAD.r, ph: r.height - PAD.t - PAD.b }; cv.style.cursor = 'grabbing'; return;
    }
    WV.zoomBox = { cv, si, x0: e.clientX - r.left, x1: e.clientX - r.left, y0: e.clientY - r.top, y1: e.clientY - r.top, t0: tAt(e), v0: vAt(e), rect: WV.tool === 'rect' };
  });
  cv.addEventListener('contextmenu', e => { if (WV.cur) { e.preventDefault(); WV.c2 = tAt(e); drawWave(); } });
  cv.addEventListener('mousemove', e => {
    const r = cv.getBoundingClientRect();
    if (WV.pan) return;
    if (WV.cdrag) { const t = tAt(e); if (WV.cdrag.which === 2) WV.c2 = t; else WV.c1 = t; drawWave(); return; }
    if (WV.cur && !WV.zoomBox) cv.style.cursor = cursorNear(cv, si, e.clientX, e.clientY, 10) ? 'ew-resize' : '';
    if (WV.zoomBox && WV.zoomBox.cv === cv) { const z = WV.zoomBox; z.x1 = e.clientX - r.left; z.y1 = e.clientY - r.top; z.t1 = tAt(e); z.v1 = vAt(e); drawScreen(cv, S()); return; }
    if (WV.tool && !WV.cdrag) cv.style.cursor = WV.tool === 'hand' ? 'grab' : 'crosshair';
    const [xa, xb] = sX(S());
    const t = Math.max(xa, Math.min(xb, tAt(e))); WV.hover = { cv, t };
    const s = S();
    $('#wHover').innerHTML = '<span class="mono">t = ' + siFmt(t, 's', 6) + '</span>' + s.curves.map(ci => `<br><span class="mono" style="color:${wColor(ci)}">${esc(WV.res.names[ci])} = ${siFmt(valAt(ci, t), '', 6)}</span>`).join('');
    headReadout(si, t);
    drawScreen(cv, s);
  });
  cv.addEventListener('mouseleave', () => { WV.hover = null; drawScreen(cv, S()); headReadout(si, WV.cur && WV.c1 != null ? WV.c1 : null); });
  cv.addEventListener('wheel', e => {
    e.preventDefault(); const k = e.deltaY > 0 ? 1.25 : 0.8;
    if (e.shiftKey || e.ctrlKey) { scrZoomY(si, k, vAt(e)); return; }
    scrZoomX(si, k, tAt(e));
  }, { passive: false });
  cv.addEventListener('dblclick', () => scrFit(si));
  attachPlotTouch(cv, si, tAt, vAt);
}
// touch on a plot: 1 finger drag = pan X/Y, pinch = zoom (horizontal spread → X, vertical → Y),
// tap = read values (cursor mode: tap sets C1, long press sets C2), double tap = auto range
function attachPlotTouch(cv, si, tAt, vAt) {
  const tp = new Map(); let st = null, lastTap = null;
  const S = () => WV.screens[si];
  const readout = (e) => {
    const t = tAt(e), s = S(); WV.hover = { cv, t };
    $('#wHover').innerHTML = '<span class="mono">t = ' + siFmt(t, 's', 6) + '</span>' + s.curves.map(ci => `<br><span class="mono" style="color:${wColor(ci)}">${esc(WV.res.names[ci])} = ${siFmt(valAt(ci, t), '', 6)}</span>`).join('');
    headReadout(si, t);
    drawScreen(cv, s);
  };
  const base = () => { const r = cv.getBoundingClientRect(); return { xr: sX(S()).slice(), yr: yRange(S()).slice(), pw: r.width - PAD.l - PAD.r, ph: r.height - PAD.t - PAD.b, r }; };
  cv.addEventListener('pointerdown', e => {
    if (e.pointerType !== 'touch') return;
    e.preventDefault(); try { cv.setPointerCapture(e.pointerId); } catch (er) { /* ignore */ }
    setAct(si); tp.set(e.pointerId, { x: e.clientX, y: e.clientY });
    if (tp.size === 1) {
      const near = WV.cur ? cursorNear(cv, si, e.clientX, e.clientY, 26) : 0;
      if (near) { st = { mode: 'cur', which: near }; WV.csel = near; drawWave(); return; }
      st = Object.assign({ mode: 'pan', x: e.clientX, y: e.clientY, t0: performance.now(), moved: false }, base());
      st.lp = setTimeout(() => { if (st && st.mode === 'pan' && !st.moved && WV.cur) { st.lpDone = true; WV.c2 = tAt(e); drawWave(); } }, 550);
    } else if (tp.size === 2) {
      if (st) clearTimeout(st.lp);
      const [a, b] = [...tp.values()]; const bs = base();
      const ce = { clientX: (a.x + b.x) / 2, clientY: (a.y + b.y) / 2 };
      st = Object.assign({ mode: 'pinch', dx0: Math.abs(a.x - b.x), dy0: Math.abs(a.y - b.y), tc: tAt(ce), vc: vAt(ce) }, bs);
    }
  });
  cv.addEventListener('pointermove', e => {
    if (e.pointerType !== 'touch' || !tp.has(e.pointerId) || !st) return;
    e.preventDefault(); tp.set(e.pointerId, { x: e.clientX, y: e.clientY });
    const s = S();
    if (st.mode === 'cur') { const t = tAt(e); if (st.which === 2) WV.c2 = t; else WV.c1 = t; drawWave(); return; }
    if (st.mode === 'pan') {
      const dxp = e.clientX - st.x, dyp = e.clientY - st.y;
      if (!st.moved && Math.hypot(dxp, dyp) > 8) { st.moved = true; clearTimeout(st.lp); }
      if (!st.moved) return;
      const dx = dxp / st.pw * (st.xr[1] - st.xr[0]), dy = dyp / st.ph * (st.yr[1] - st.yr[0]);
      s.y = [st.yr[0] + dy, st.yr[1] + dy]; setScreenX(si, st.xr[0] - dx, st.xr[1] - dx, true);
    } else if (st.mode === 'pinch' && tp.size >= 2) {
      const [a, b] = [...tp.values()]; const dx = Math.abs(a.x - b.x), dy = Math.abs(a.y - b.y);
      if (st.dy0 > 40 && dy > 10) { const k = st.dy0 / dy; s.y = [st.vc - (st.vc - st.yr[0]) * k, st.vc + (st.yr[1] - st.vc) * k]; }
      if (st.dx0 > 40 && dx > 10) { const k = st.dx0 / dx; setScreenX(si, st.tc - (st.tc - st.xr[0]) * k, st.tc + (st.xr[1] - st.tc) * k, true); }
      else drawWave();
    }
  });
  const end = e => {
    if (e.pointerType !== 'touch' || !tp.has(e.pointerId)) return;
    tp.delete(e.pointerId);
    if (!st) return;
    clearTimeout(st.lp);
    if (st.mode === 'pinch') { if (tp.size === 1) { const [p] = [...tp.values()]; st = Object.assign({ mode: 'pan', x: p.x, y: p.y, t0: 0, moved: true }, base()); } else if (!tp.size) st = null; return; }
    if (st.mode === 'pan' && !st.moved && !st.lpDone && e.type === 'pointerup') {
      const now = performance.now();
      if (lastTap && now - lastTap < 320) { lastTap = null; scrFit(si); }
      else { lastTap = now; if (WV.cur) { if (WV.csel === 2) WV.c2 = tAt(e); else WV.c1 = tAt(e); drawWave(); } else readout(e); }
    }
    if (!tp.size) st = null;
  };
  cv.addEventListener('pointerup', end); cv.addEventListener('pointercancel', end);
}
window.addEventListener('mousemove', e => {
  const p = WV.pan; if (!p) return;
  const dx = (e.clientX - p.x) / p.pw * (p.xr[1] - p.xr[0]), dy = (e.clientY - p.y) / p.ph * (p.yr[1] - p.yr[0]);
  const s = WV.screens[p.si]; if (!s) return;
  s.y = [p.yr[0] + dy, p.yr[1] + dy];
  setScreenX(p.si, p.xr[0] - dx, p.xr[1] - dx, true);
});
window.addEventListener('mouseup', () => {
  if (WV.cdrag) { WV.cdrag = null; $$('#wPlots canvas').forEach(c => { c.style.cursor = ''; }); drawWave(); return; }
  if (WV.pan) { WV.pan = null; $$('#wPlots canvas').forEach(c => { c.style.cursor = ''; }); return; }
  const z = WV.zoomBox; if (!z) return; WV.zoomBox = null;
  if (z.t1 == null || Math.abs(z.x1 - z.x0) <= 4) { drawWave(); return; }
  if (z.rect && Math.abs(z.y1 - z.y0) > 4) { WV.screens[z.si].y = [Math.min(z.v0, z.v1), Math.max(z.v0, z.v1)]; setScreenX(z.si, Math.min(z.t0, z.t1), Math.max(z.t0, z.t1), true); }
  else setScreenX(z.si, Math.min(z.t0, z.t1), Math.max(z.t0, z.t1));
});
/* ---------- per-screen axes (X follows the shared range unless the screen has its own) ---------- */
function sX(s) { return s && s.x ? s.x : [WV.xa, WV.xb]; }
function fullX() { const r = WV.res; return [r.t0, r.t0 + Math.max(1, r.n - 1) * r.dt]; }
function clampX(a, b) {
  const r = WV.res, [lo, hi] = fullX(); let w = b - a;
  if (w < r.dt * 4) { const m = (a + b) / 2; w = r.dt * 4; a = m - w / 2; b = m + w / 2; }
  if (w >= hi - lo) return [lo, hi];
  if (a < lo) { a = lo; b = lo + w; } if (b > hi) { b = hi; a = hi - w; }
  return [a, b];
}
function setScreenX(si, a, b, keepY) {
  [a, b] = clampX(a, b);
  if (WV.link) { WV.xa = a; WV.xb = b; WV.screens.forEach(s => { s.x = null; if (!keepY && s !== WV.screens[si]) s.y = null; }); }
  else WV.screens[si].x = [a, b];
  drawWave();
}
function scrZoomX(si, k, tc) { const [a, b] = sX(WV.screens[si]); const m = tc != null ? tc : (a + b) / 2; setScreenX(si, m - (m - a) * k, m + (b - m) * k, true); }
function scrPanX(si, f) { const [a, b] = sX(WV.screens[si]); const d = (b - a) * f; setScreenX(si, a + d, b + d, true); }
function drawScreenIdx(si) { drawScreen($$('#wPlots canvas')[si], WV.screens[si]); }
function scrZoomY(si, k, vc) { const s = WV.screens[si]; const [a, b] = yRange(s); const m = vc != null ? vc : (a + b) / 2; s.y = [m - (m - a) * k, m + (b - m) * k]; drawScreenIdx(si); }
function scrPanY(si, f) { const s = WV.screens[si]; const [a, b] = yRange(s); const d = (b - a) * f; s.y = [a + d, b + d]; drawScreenIdx(si); }
function scrFit(si) { const s = WV.screens[si]; s.y = null; if (WV.link) { waveFit(); return; } s.x = fullX(); drawWave(); }
function zoomX(a, b) {
  [a, b] = clampX(a, b);
  WV.xa = a; WV.xb = b; WV.screens.forEach(s => { s.y = null; s.x = null; }); drawWave();
}
function waveFit() { const r = WV.res; WV.xa = r.t0; WV.xb = r.t0 + Math.max(1, r.n - 1) * r.dt; WV.screens.forEach(s => { s.y = null; s.x = null; }); drawWave(); }
function stats(ci, a, b) {
  const r = WV.res, d = r.data[ci]; const [i0, i1] = idxRange(a, b);
  let mx = -Infinity, mn = Infinity, s1 = 0, s2 = 0, T = 0;
  for (let i = i0; i <= i1; i++) { const v = d[i]; if (v > mx) mx = v; if (v < mn) mn = v; if (i > i0) { const u = d[i - 1]; s1 += (u + v) / 2 * r.dt; s2 += (u * u + v * v) / 2 * r.dt; T += r.dt; } }
  return { mx, mn, avg: T ? s1 / T : d[i0], rms: T ? Math.sqrt(s2 / T) : Math.abs(d[i0]), pp: mx - mn, integ: s1 };
}
function drawMeas() {
  const box = $('#wMeas'); const s = WV.screens[WV.act]; if (!s) { box.innerHTML = ''; return; }
  const useC = WV.cur && WV.c1 != null && WV.c2 != null; const [sxa, sxb] = sX(s); const a = useC ? Math.min(WV.c1, WV.c2) : sxa, b = useC ? Math.max(WV.c1, WV.c2) : sxb;
  let html = `<div class="note">${useC ? '游標 C1–C2 之間' : '目前顯示範圍'}（${siFmt(b - a, 's', 4)}）</div><table><tr><th></th><th>Max</th><th>Min</th><th>Avg</th><th>RMS</th><th>P-P</th><th title="對時間的積分（功率波形 → 能量）">∫dt</th></tr>`;
  s.curves.forEach(ci => { const st = stats(ci, a, b); html += `<tr><td style="color:${wColor(ci)}">${esc(WV.res.names[ci])}</td><td>${siFmt(st.mx, '', 4)}</td><td>${siFmt(st.mn, '', 4)}</td><td>${siFmt(st.avg, '', 4)}</td><td>${siFmt(st.rms, '', 4)}</td><td>${siFmt(st.pp, '', 4)}</td><td>${siFmt(st.integ, '', 4)}</td></tr>`; });
  html += '</table>';
  if (WV.cur) {
    const coarse = document.documentElement.classList.contains('coarse');
    html += `<div class="note" style="margin-top:6px">C1 = ${WV.c1 != null ? siFmt(WV.c1, 's', 6) : (coarse ? '—（點波形設定）' : '—（左鍵設定）')}<br>C2 = ${WV.c2 != null ? siFmt(WV.c2, 's', 6) : (coarse ? '—（選 C2 後點波形）' : '—（右鍵或 Shift+左鍵）')}${useC ? '<br>Δt = ' + siFmt(WV.c2 - WV.c1, 's', 6) + '，1/Δt = ' + siFmt(1 / Math.abs(WV.c2 - WV.c1), 'Hz', 5) : ''}<br><span class="faint">${coarse ? '拖曳游標線上方的把手可移動游標' : '拖曳游標線可移動游標'}</span></div>`;
    if (WV.c1 != null) {
      html += '<table><tr><th></th><th>@C1</th><th>@C2</th><th>Δ</th></tr>';
      s.curves.forEach(ci => { const v1 = valAt(ci, WV.c1), v2 = WV.c2 != null ? valAt(ci, WV.c2) : NaN; html += `<tr><td style="color:${wColor(ci)}">${esc(WV.res.names[ci])}</td><td>${siFmt(v1, '', 5)}</td><td>${isNaN(v2) ? '—' : siFmt(v2, '', 5)}</td><td>${isNaN(v2) ? '—' : siFmt(v2 - v1, '', 5)}</td></tr>`; });
      html += '</table>';
    }
  }
  box.innerHTML = html;
}
function waveCsv() {
  const r = WV.res; const [i0, i1] = idxRange(...sX(WV.screens[WV.act]));
  const step = Math.max(1, Math.ceil((i1 - i0 + 1) / 200000));
  let s = 'Time,' + r.names.join(',') + '\n';
  for (let i = i0; i <= i1; i += step) { s += (r.t0 + i * r.dt).toPrecision(12); for (let k = 0; k < r.names.length; k++) s += ',' + r.data[k][i].toPrecision(10); s += '\n'; }
  exportText('waveform.csv', s, step > 1 ? '資料點過多，已每 ' + step + ' 點取一點（共約 20 萬列）。' : '');
}

/* ------------------------------------------------------------------
 * Bode viewer
 * ------------------------------------------------------------------ */
const BV = { res: null, hide: new Set(), fa: 1, fb: 10, hover: null };
function openBode(res) {
  BV.res = res; BV.hide = new Set(); BV.fa = res.freqs[0]; BV.fb = res.freqs[res.freqs.length - 1]; if (!(BV.fb > BV.fa)) { BV.fa /= 2; BV.fb *= 2; }
  $('#bode').hidden = false;
  const list = $('#bVars'); list.innerHTML = '';
  res.curves.forEach((c, i) => {
    const cb = h('input', { type: 'checkbox', id: 'bc' + i }); cb.checked = true; cb.onchange = () => { if (cb.checked) BV.hide.delete(i); else BV.hide.add(i); drawBode(); };
    list.append(h('label', { for: 'bc' + i, style: 'display:flex;gap:6px;align-items:center;font:12.5px var(--f-mono)' }, cb, h('span', { style: `width:10px;height:3px;background:${curveColor(i)}` }), c.name));
  });
  const info = $('#bInfo'); info.innerHTML = '';
  res.curves.forEach((c, i) => info.append(h('div', { class: 'card', html: bodeSummary(c, res.freqs, i) })));
  drawBode();
}
function interpX(f, a, b, fa, fb) { return a + (b - a) * (Math.log(f / fa) / Math.log(fb / fa)); }
function bodeSummary(c, fs, i) {
  const out = [`<b style="color:${curveColor(i)}">${esc(c.name)}</b>` + (c.loop ? '（迴路增益）' : '')];
  // gain crossover (0 dB) and phase margin
  for (let k = 1; k < fs.length; k++) {
    const a = c.mag[k - 1], b = c.mag[k];
    if ((a >= 0 && b < 0) || (a < 0 && b >= 0)) {
      const r = a / (a - b); const f = fs[k - 1] * Math.pow(fs[k] / fs[k - 1], r); const ph = c.ph[k - 1] + (c.ph[k] - c.ph[k - 1]) * r;
      let pm = ph + 180; while (pm > 180) pm -= 360; while (pm < -180) pm += 360;
      if (c.loop) out.push(`穿越頻率 fc = ${siFmt(f, 'Hz', 4)}`, `相位 = ${ph.toFixed(2)}°，相位裕度 PM = ${pm.toFixed(2)}°`);
      else out.push(`0 dB 穿越 @ ${siFmt(f, 'Hz', 4)}（相位 ${ph.toFixed(2)}°）`);
      break;
    }
  }
  for (let k = 1; k < fs.length; k++) {
    const a = c.ph[k - 1] + 180, b = c.ph[k] + 180, m = x => ((x % 360) + 540) % 360 - 180;
    const pa = m(a), pb = m(b);
    if (c.loop && Math.abs(pa - pb) < 90 && ((pa >= 0 && pb < 0) || (pa < 0 && pb >= 0))) { const r = pa / (pa - pb); const f = fs[k - 1] * Math.pow(fs[k] / fs[k - 1], r); const g = c.mag[k - 1] + (c.mag[k] - c.mag[k - 1]) * r; out.push(`相位 −180° 於 ${siFmt(f, 'Hz', 4)}，增益裕度 GM = ${(-g).toFixed(2)} dB`); break; }
  }
  let pk = 0; c.mag.forEach((v, k) => { if (v > c.mag[pk]) pk = k; });
  out.push(`低頻增益 ${c.mag[0].toFixed(2)} dB @ ${siFmt(fs[0], 'Hz', 4)}`, `峰值 ${c.mag[pk].toFixed(2)} dB @ ${siFmt(fs[pk], 'Hz', 4)}`);
  if (!c.loop) { const ref = c.mag[0] - 3; for (let k = 1; k < fs.length; k++) if (c.mag[k - 1] >= ref && c.mag[k] < ref) { const r = (c.mag[k - 1] - ref) / (c.mag[k - 1] - c.mag[k]); out.push(`−3 dB 頻寬 ${siFmt(fs[k - 1] * Math.pow(fs[k] / fs[k - 1], r), 'Hz', 4)}`); break; } }
  return out.join('<br>');
}
function drawBode() {
  const res = BV.res; if (!res) return;
  ['#bMag', '#bPh'].forEach((sel, pi) => {
    const cv = $(sel); const { ctx, w, h: H } = setupCanvas(cv);
    const pw = w - PAD.l - PAD.r, ph = H - PAD.t - PAD.b;
    ctx.clearRect(0, 0, w, H); ctx.fillStyle = cssv('--canvas'); ctx.fillRect(0, 0, w, H);
    const key = pi === 0 ? 'mag' : 'ph';
    let lo = Infinity, hi = -Infinity; res.curves.forEach((c, i) => { if (BV.hide.has(i)) return; c[key].forEach((v, k) => { if (res.freqs[k] < BV.fa || res.freqs[k] > BV.fb) return; if (isFinite(v)) { lo = Math.min(lo, v); hi = Math.max(hi, v); } }); });
    if (!isFinite(lo)) { lo = -1; hi = 1; }
    if (pi === 1) { lo = Math.floor(lo / 45) * 45; hi = Math.ceil(hi / 45) * 45; if (hi - lo < 90) { hi += 45; lo -= 45; } }
    else { const m = Math.max(1, (hi - lo) * 0.08); lo -= m; hi += m; }
    const X = f => PAD.l + Math.log(f / BV.fa) / Math.log(BV.fb / BV.fa) * pw, Y = v => PAD.t + (hi - v) / (hi - lo) * ph;
    ctx.font = '11px ' + cssv('--f-mono'); ctx.lineWidth = 1;
    // log grid
    ctx.strokeStyle = cssv('--grid2'); ctx.beginPath();
    for (let d = Math.floor(Math.log10(BV.fa)); d <= Math.ceil(Math.log10(BV.fb)); d++) for (let m = 1; m < 10; m++) { const f = m * Math.pow(10, d); if (f < BV.fa || f > BV.fb) continue; const x = Math.round(X(f)) + 0.5; ctx.moveTo(x, PAD.t); ctx.lineTo(x, PAD.t + ph); }
    const yt = pi === 1 ? { ticks: (() => { const a = []; const st = (hi - lo) > 400 ? 90 : 45; for (let v = lo; v <= hi + 1e-9; v += st) a.push(v); return a; })(), step: 45 } : niceTicks(lo, hi, Math.max(2, Math.floor(ph / 32)));
    yt.ticks.forEach(v => { const y = Math.round(Y(v)) + 0.5; ctx.moveTo(PAD.l, y); ctx.lineTo(PAD.l + pw, y); });
    ctx.stroke();
    if (pi === 0 && lo < 0 && hi > 0) { ctx.strokeStyle = cssv('--faint'); ctx.setLineDash([4, 3]); ctx.beginPath(); const y = Math.round(Y(0)) + 0.5; ctx.moveTo(PAD.l, y); ctx.lineTo(PAD.l + pw, y); ctx.stroke(); ctx.setLineDash([]); }
    if (pi === 1 && lo <= -180 && hi >= -180) { ctx.strokeStyle = cssv('--faint'); ctx.setLineDash([4, 3]); ctx.beginPath(); const y = Math.round(Y(-180)) + 0.5; ctx.moveTo(PAD.l, y); ctx.lineTo(PAD.l + pw, y); ctx.stroke(); ctx.setLineDash([]); }
    ctx.fillStyle = cssv('--muted'); ctx.textAlign = 'right'; ctx.textBaseline = 'middle';
    yt.ticks.forEach(v => ctx.fillText(pi === 1 ? v.toFixed(0) + '°' : fmtTick(v, yt.step) + ' dB', PAD.l - 6, Y(v)));
    ctx.textAlign = 'center'; ctx.textBaseline = 'top';
    for (let d = Math.floor(Math.log10(BV.fa)); d <= Math.ceil(Math.log10(BV.fb)); d++) { const f = Math.pow(10, d); if (f < BV.fa * 0.999 || f > BV.fb * 1.001) continue; ctx.fillText(siFmt(f, 'Hz', 3).replace(' ', ''), X(f), PAD.t + ph + 4); }
    ctx.strokeStyle = cssv('--line'); ctx.strokeRect(PAD.l + 0.5, PAD.t + 0.5, pw, ph);
    ctx.fillStyle = cssv('--muted'); ctx.textAlign = 'left'; ctx.fillText(pi === 0 ? '增益 (dB)' : '相位 (°)', PAD.l + 4, 5);
    ctx.save(); ctx.beginPath(); ctx.rect(PAD.l, PAD.t, pw, ph); ctx.clip();
    res.curves.forEach((c, i) => {
      if (BV.hide.has(i)) return; ctx.strokeStyle = curveColor(i); ctx.lineWidth = 1.6; ctx.beginPath();
      c[key].forEach((v, k) => { const x = X(res.freqs[k]), y = Y(v); if (k === 0) ctx.moveTo(x, y); else ctx.lineTo(x, y); }); ctx.stroke();
      ctx.fillStyle = curveColor(i); if (res.freqs.length <= 80) c[key].forEach((v, k) => { ctx.beginPath(); ctx.arc(X(res.freqs[k]), Y(v), 2, 0, 7); ctx.fill(); });
    });
    if (BV.hover != null) { const x = Math.round(X(BV.hover)) + 0.5; ctx.strokeStyle = cssv('--faint'); ctx.beginPath(); ctx.moveTo(x, PAD.t); ctx.lineTo(x, PAD.t + ph); ctx.stroke(); }
    ctx.restore();
  });
}
function bodeHover(e, cv) {
  const r = cv.getBoundingClientRect(); const pw = r.width - PAD.l - PAD.r;
  const f = BV.fa * Math.pow(BV.fb / BV.fa, (e.clientX - r.left - PAD.l) / pw);
  if (!(f >= BV.fa && f <= BV.fb)) { BV.hover = null; drawBode(); return; }
  BV.hover = f; const res = BV.res;
  let k = 1; while (k < res.freqs.length - 1 && res.freqs[k] < f) k++;
  const f0 = res.freqs[k - 1], f1 = res.freqs[k]; const t = f1 > f0 ? Math.log(f / f0) / Math.log(f1 / f0) : 0;
  $('#bHover').innerHTML = `<span class="mono">f = ${siFmt(f, 'Hz', 5)}</span>` + res.curves.map((c, i) => BV.hide.has(i) ? '' : `<br><span class="mono" style="color:${curveColor(i)}">${esc(c.name)}: ${(c.mag[k - 1] + (c.mag[k] - c.mag[k - 1]) * t).toFixed(3)} dB, ${(c.ph[k - 1] + (c.ph[k] - c.ph[k - 1]) * t).toFixed(2)}°</span>`).join('');
  drawBode();
}
function bodeCsv() {
  const res = BV.res; let s = 'Frequency(Hz)'; res.curves.forEach(c => { s += `,${c.name} |H|(dB),${c.name} phase(deg),${c.name} Re,${c.name} Im`; }); s += '\n';
  res.freqs.forEach((f, k) => { s += f.toPrecision(10); res.curves.forEach(c => { s += ',' + c.mag[k].toPrecision(10) + ',' + c.ph[k].toPrecision(10) + ',' + c.re[k].toPrecision(10) + ',' + c.im[k].toPrecision(10); }); s += '\n'; });
  exportText('bode.csv', s, '');
}

/* ------------------------------------------------------------------
 * File I/O
 * ------------------------------------------------------------------ */
function exportText(name, text, note) {
  const ta = h('textarea', { class: 'big', readonly: true, spellcheck: 'false' }); ta.value = text.length > 3e6 ? text.slice(0, 3e6) + '\n…（內容過長，已截斷顯示；下載檔案為完整內容）' : text;
  const body = h('div', null, note ? h('p', { class: 'note' }, note) : null, h('p', { class: 'note' }, '可直接下載檔案；若瀏覽器封鎖下載（例如在預覽頁中），請改用「複製」。'), ta);
  modal('匯出 ' + name, body, [
    { label: '複製', close: false, fn: () => { const done = () => toast('已複製到剪貼簿'); try { navigator.clipboard.writeText(text).then(done, () => { ta.select(); document.execCommand('copy'); done(); }); } catch (e) { ta.select(); document.execCommand('copy'); done(); } return false; } },
    { label: '下載檔案', pri: true, close: false, fn: () => { const a = h('a', { href: URL.createObjectURL(new Blob([text], { type: 'text/plain' })), download: name }); document.body.append(a); a.click(); a.remove(); return false; } },
    { label: '關閉' }], { wide: true });
}
function saveDialog() {
  // the waveform screen layout (probe names per screen) travels with the file
  const screens = WV.layout && WV.layout.names ? WV.layout.screens.map(sc => sc.curves.map(i => WV.layout.names[i])) : (ED.docLayout || undefined);
  const data = JSON.stringify({ app: 'psimweb', version: APP.version, doc: ED.doc, screens }, null, 1);
  ED.dirty = false; updateStatus();
  exportText((ED.fileName || 'circuit').replace(/[\\/:*?"<>|]/g, '_') + '.psimx', data, '電路檔（JSON）。之後可用「開啟檔案」或將檔案拖曳到電路圖上載入。');
}
function openFile() {
  const inp = h('input', { type: 'file', accept: '.psimx,.json,.psimsch,application/json,text/plain' });
  inp.onchange = () => { const f = inp.files[0]; if (f) readFile(f); };
  inp.click();
}
function readFile(f) {
  if (/\.(psimsch|sch)$/i.test(f.name)) {
    showMessages([{ msg: '無法直接開啟 PSIM 原生電路檔（' + f.name + '）：.psimsch 是 PSIM 的專有格式，沒有公開規格；其他第三方工具（Typhoon HIL、SIMBA）也都是先由 PSIM 本身轉出網表才能讀取。請在 PSIM 以「Simulate → Generate Netlist File (XML)」匯出 XML 網表，提供樣本後即可評估加入匯入功能。' }], [], '無法開啟檔案');
    return;
  }
  const rd = new FileReader();
  rd.onload = () => { try { loadDocText(String(rd.result), f.name.replace(/\.[^.]+$/, '')); } catch (e) { showMessages([{ msg: '無法開啟檔案：' + e.message }], []); } };
  rd.readAsText(f);
}
// a parsed document (own copy) with every parameter filled in from the library defaults
function docFromJson(src) {
  const doc = JSON.parse(JSON.stringify(src));
  if (!Array.isArray(doc.elements) || !Array.isArray(doc.wires)) throw new Error('不是有效的電路檔');
  doc.elements = doc.elements.filter(e => LIB.D[e.type]);
  doc.elements.forEach(e => { e.params = Object.assign(defaultParams(e.type), e.params || {}); e.rot = e.rot || 0; });
  doc.nid = Math.max(doc.nid || 1, 1 + Math.max(0, ...doc.elements.map(e => e.id), ...doc.wires.map(w => w.id)));
  return doc;
}
function loadDocText(txt, name) {
  const o = JSON.parse(txt); const doc = docFromJson(o.doc || o);
  pushUndo(); ED.doc = doc; ED.docLayout = Array.isArray(o.screens) ? o.screens : null; WV.layout = null; ED.sel.clear(); ED.fileName = name || '電路'; ED.lastRes = null; ED.lastAC = null; ED.dirty = false; changed(); fitView();
}
function pasteJsonDialog() {
  const ta = h('textarea', { class: 'big', spellcheck: 'false', placeholder: '在此貼上電路檔 JSON 內容' });
  modal('由文字匯入電路', h('div', null, ta), [{ label: '取消' }, { label: '匯入', pri: true, fn: () => { try { loadDocText(ta.value, '匯入的電路'); } catch (e) { toast('匯入失敗：' + e.message); return false; } } }], { wide: true });
}
function newDoc() {
  pushUndo(); ED.doc = { elements: [], wires: [], nid: 1 }; ED.docLayout = null; WV.layout = null; ED.sel.clear(); ED.fileName = '未命名電路'; ED.lastRes = null; ED.lastAC = null;
  const r = svg.getBoundingClientRect(); const w = 60, hh = w * r.height / Math.max(1, r.width);
  ED.vb = { x: -w / 2, y: -hh / 2, w, h: hh };
  const el = newElement('SIMCTRL', Math.round(w / 2 - 6), Math.round(-hh / 2 + 5)); ED.doc.elements.push(el); changed();
}

/* ------------------------------------------------------------------
 * Examples
 * ------------------------------------------------------------------ */
function makeDoc(build) {
  const doc = { elements: [], wires: [], nid: 1 };
  const E = (type, x, y, rot, params, name, mir) => {
    const d = LIB.D[type]; const el = { id: doc.nid++, type, x, y, rot: rot || 0, mir: !!mir, dis: false, name: name || '', params: Object.assign(defaultParams(type), params || {}) };
    if (!name && !d.noname) { const used = new Set(doc.elements.map(e => e.name)); for (let i = 1; ; i++) if (!used.has(d.pre + i)) { el.name = d.pre + i; break; } }
    doc.elements.push(el); return el;
  };
  const W = (...pts) => { for (let i = 1; i < pts.length; i++) { const [a, b] = [pts[i - 1], pts[i]]; if (a[0] !== b[0] || a[1] !== b[1]) doc.wires.push({ id: doc.nid++, x1: a[0], y1: a[1], x2: b[0], y2: b[1] }); } };
  build(E, W);
  return doc;
}
const EXAMPLES = {
  buck: { name: 'Buck 轉換器（開迴路 PWM）', build: (E, W) => {
    E('VDC', 0, 6, 0, { V: '100' }, 'Vin'); E('GND', 0, 8);
    E('MOSFET', 6, 4, 3, {}, 'Q1'); W([0, 4], [4, 4]);
    E('DIODE', 10, 6, 3, { vf: '0' }, 'D1'); W([8, 4], [11, 4]);
    E('L', 13, 4, 0, { L: '100u' }, 'L1'); E('IP', 17, 4, 0, {}, 'IL');
    E('C', 19, 6, 1, { C: '100u' }, 'Co'); E('R', 25, 6, 1, { R: '5' }, 'RL'); W([19, 4], [25, 4]);
    E('VP', 22, 2, 0, {}, 'Vo');
    W([0, 8], [25, 8]);
    E('VDC', 2, 17, 0, { V: '0.437' }, 'Vref'); E('GND', 2, 19);
    E('VTRI', 2, 23, 0, { Vpp: '1', f: '50k', D: '0.5' }, 'Vtri'); E('GND', 2, 25);
    E('COMP', 13, 18, 0, {}, 'CMP');
    W([2, 15], [10, 15], [10, 17]); W([2, 21], [8, 21], [8, 19], [10, 19]);
    E('ONCTRL', 19, 18, 0, {}, 'ON1'); W([16, 18], [17, 18]);
    W([21, 18], [21, 12], [6, 12], [6, 6]);
    E('VP', 23, 16, 0, {}, 'Vg'); W([21, 18], [23, 18]);
    E('SIMCTRL', 30, 2, 0, { dt: '0.2u', tt: '3m', tp: '0', ps: '1' });
    E('TEXT', 0, -3, 0, { text: 'Buck 轉換器：Vin=100 V、fsw=50 kHz、D=0.437 → 理想 Vo ≈ 43.7 V。按 F5 執行模擬。' });
  } },
  buckac: { name: 'Buck 轉換器 AC 掃頻（小訊號注入）', build: (E, W) => {
    E('VDC', 0, 6, 0, { V: '100' }, 'Vin'); E('GND', 0, 8);
    E('MOSFET', 6, 4, 3, {}, 'Q1'); W([0, 4], [4, 4]);
    E('DIODE', 10, 6, 3, { vf: '0' }, 'D1'); W([8, 4], [11, 4]);
    E('L', 13, 4, 0, { L: '100u' }, 'L1');
    E('C', 17, 6, 1, { C: '100u' }, 'Co'); E('R', 23, 6, 1, { R: '5' }, 'RL'); W([15, 4], [27, 4]);
    E('ACP', 20, 2, 0, {}, 'Vo_ac'); E('VP', 27, 2, 0, {}, 'Vo');
    W([0, 8], [23, 8]);
    E('VDC', -4, 17, 0, { V: '0.437' }, 'Vref'); E('GND', -4, 19);
    E('SUM2', 4, 15, 0, { k1: '1', k2: '1' }, 'SUM1'); W([-4, 15], [2, 15]);
    E('VSIN', 4, 20, 0, { Vm: '0', f: '1k' }, 'Vac'); E('GND', 4, 22); W([4, 17], [4, 18]);
    E('COMP', 13, 17, 0, {}, 'CMP'); W([6, 15], [9, 15], [9, 16], [10, 16]);
    E('VTRI', 10, 23, 0, { Vpp: '1', f: '50k', D: '0.5' }, 'Vtri'); E('GND', 10, 25); W([10, 21], [10, 18]);
    E('ONCTRL', 20, 17, 0, {}, 'ON1'); W([16, 17], [18, 17]);
    W([22, 17], [22, 12], [6, 12], [6, 6]);
    E('SIMCTRL', 34, 1, 0, { dt: '0.2u', tt: '6m', tp: '0', ps: '10' });
    E('ACSWEEP', 34, 8, 0, { f0: '200', f1: '20k', n: '25', src: 'Vac', a0: '0.005', a1: '0.02', method: '0', settle: '5', mcyc: '4', maxc: '80', tol: '0.2', tpre: '-1' });
    E('TEXT', -4, -3, 0, { text: '激勵源 Vac 經加法器注入調變訊號，AC Sweep Probe 量測 Vo/調變 的開迴路響應（含開關動作）。F5 執行掃頻。' });
  } },
  supervisor: { name: 'Supervisor 看門狗比較器（AS393 + 301k 修正）', build: (E, W) => {
    // sources
    E('VPWL', -12, -4, 0, { f: '0', vals: '0 12 12 0', times: '0 1m 70m 80m' }, 'P12V'); E('GND', -12, -2); W([-12, -6], [-12, -8]); E('LABEL', -12, -8, 0, { text: 'VCC' });
    W([-12, -7], [-6, -7]); E('VP', -6, -9, 0, {}, 'Vcc');
    E('VDC', -12, 6, 0, { V: '3.3' }, 'P3V3M'); E('GND', -12, 8); W([-12, 4], [-12, 2]); E('LABEL', -12, 2, 0, { text: '3V3M' });
    E('VPWL', -12, 18, 0, { f: '0', vals: '0 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0 0 3.3 3.3 0', times: '0 5m 5m 5.001m 6m 6.001m 7m 7.001m 8m 8.001m 9m 9.001m 10m 10.001m 11m 11.001m 12m 12.001m 13m 13.001m 14m 14.001m 15m 15.001m 16m 16.001m 17m 17.001m 18m 18.001m 19m 19.001m 20m 20.001m 21m 21.001m 22m 22.001m 23m 23.001m 24m 24.001m 25m 25.001m 26m 26.001m 27m 27.001m 28m 28.001m 29m 29.001m 30m 30.001m 31m 31.001m 32m 32.001m 33m 33.001m 34m 34.001m 35m 35.001m 36m 36.001m 37m 37.001m 38m 38.001m 39m 39.001m 40m 40.001m' }, 'MCU_SRC'); E('GND', -12, 20); W([-12, 16], [-12, 14]); E('LABEL', -12, 14, 0, { text: 'MCU_RDY' });
    W([-12, 15], [-4, 15]); E('VP', -4, 13, 0, {}, 'MCU_RDY');
    // V+ (pin 3): RM318 from VCC, RM315 // CM312 to SG, RM316 hysteresis from output, DM30 clamp to VCC
    E('LABEL', 8, -8, 0, { text: 'VCC' }); W([8, -8], [8, 0]);
    E('DIODE', 12, -6, 2, { model: '1N4148' }, 'DM30'); W([8, -6], [10, -6]);
    E('R', 12, 0, 0, { R: '100k' }, 'RM318'); W([8, 0], [10, 0]);
    W([14, -6], [16, -6]); W([14, 0], [16, 0]); W([16, -10], [16, 2]); W([16, 2], [26, 2]);
    E('R', 20, 4, 1, { R: '31.6k' }, 'RM315'); E('GND', 20, 6);
    E('C', 26, 4, 1, { C: '0.1u' }, 'CM312'); E('GND', 26, 6);
    E('R', 24, -10, 0, { R: '100k' }, 'RM316'); W([16, -10], [22, -10]); W([26, -10], [50, -10]);
    E('VP', 19, -12, 0, {}, 'Vpos');
    // V- (pin 2): RM321 from VCC, CM320 timing cap, new 301k divider, QM31 discharge
    E('LABEL', 8, 10, 0, { text: 'VCC' }); W([8, 10], [8, 14]);
    E('R', 12, 14, 0, { R: '100k' }, 'RM321'); W([8, 14], [10, 14]); W([14, 14], [16, 14]);
    W([16, 14], [16, 22]); W([16, 20], [26, 20]);
    E('C', 21, 22, 1, { C: '0.1u' }, 'CM320'); E('GND', 21, 24);
    E('R', 26, 22, 1, { R: '301k' }, 'RM_301k'); E('GND', 26, 24);
    E('VP', 19, 14, 0, {}, 'Vneg');
    E('R', 16, 24, 1, { R: '49.9' }, 'RM322'); W([16, 26], [16, 28]);
    E('NPN3', 16, 30, 0, { model: '2N4401' }, 'QM31'); E('GND', 16, 32);
    W([14, 30], [6, 30]);
    E('R', 12, 34, 1, { R: '10k' }, 'RM327'); W([12, 30], [12, 32]); E('GND', 12, 36);
    E('DIODE', 8, 34, 3, { model: '1N4148' }, 'DM31'); W([8, 30], [8, 32]); E('GND', 8, 36);
    E('C', 4, 30, 0, { C: '0.1u' }, 'CM319');
    E('R', -2, 30, 0, { R: '1k' }, 'RM324'); W([0, 30], [2, 30]); W([-4, 30], [-8, 30], [-8, 26]); E('LABEL', -8, 26, 0, { text: 'MCU_RDY' });
    // comparator UM52A (AS393, open collector built into the model)
    E('COMPOC', 38, 4, 0, {}, 'UM52A');
    W([16, -2], [30, -2], [30, 3], [35, 3]); W([16, 16], [28, 16], [28, 5], [35, 5]);
    W([38, 2], [38, -1]); E('LABEL', 38, -1, 0, { text: 'VCC' }); E('GND', 38, 6);
    W([41, 4], [50, 4]); W([50, -10], [50, 4]);
    E('VP', 44, -12, 0, {}, 'MCU_OK');
    // LED indicator
    E('R', 54, -4, 0, { R: '301' }, 'RM319'); W([50, -4], [52, -4]); W([56, -4], [58, -4]);
    E('LED', 60, -4, 2, { model: 'LEDRED' }, 'LEDM2'); W([62, -4], [62, -6]); E('LABEL', 62, -6, 0, { text: '3V3M' });
    E('SIMCTRL', 70, -12, 0, { dt: '1u', tt: '0.1', tp: '0', ps: '1' });
    E('TEXT', -12, -20, 0, { text: 'Supervisor 看門狗：MCU_RDY 每 2 ms 一個上升沿，經 CM319 觸發 QM31 放掉 CM320；39 ms 後停止 toggle，V− 充電超過 V+ → UM52A 拉低 *MCU_OK。70 ms 起 12 V 下電。' });
    E('TEXT', -12, -18, 0, { text: '元件皆為 SPICE 物理模型（1N4148/BAS316、MMST4401、紅光 LED、開集極 AS393）。比較改前/改後：選取 RM_301k 按 D 停用後再按 F5；結束後的警告會列出比較器輸入超出共模範圍的累計時間。' });
  } },
  rlc: { name: 'RLC 低通濾波器波德圖（線性頻域法）', build: (E, W) => {
    E('VSIN', 0, 4, 0, { Vm: '1', f: '1k' }, 'Vac'); E('GND', 0, 6);
    W([0, 2], [1, 2]); E('R', 3, 2, 0, { R: '2' }, 'R1'); E('L', 7, 2, 0, { L: '1m' }, 'L1');
    E('C', 9, 4, 1, { C: '10u' }, 'C1'); E('R', 13, 4, 1, { R: '20' }, 'RL'); W([9, 2], [13, 2]); W([0, 6], [13, 6]);
    E('ACP', 11, 0, 0, {}, 'Vout'); E('VP', 15, 0, 0, {}, 'Vo'); W([13, 2], [15, 2]);
    E('ACSWEEP', 20, 2, 0, { f0: '10', f1: '1M', n: '120', src: 'Vac', a0: '1', a1: '1', method: '1' });
    E('SIMCTRL', 20, -2, 0, { dt: '1u', tt: '10m', tp: '0', ps: '1' });
    E('TEXT', 0, -4, 0, { text: '線性頻域法：直接解複數 MNA，速度快、結果為解析解。按 F5 執行。' });
  } },
  bridge: { name: '單相二極體橋式整流（電容濾波）', build: (E, W) => {
    E('VSIN', 0, 5, 0, { Vm: '311', f: '50' }, 'Vs'); E('GND', 0, 7);
    E('IP', 2, 3, 0, {}, 'Is'); E('L', 6, 3, 0, { L: '1m' }, 'Ls');
    E('BDIODE1', 12, 4, 0, { vf: '0.8' }, 'BD1'); W([8, 3], [9, 3]); W([0, 7], [9, 7], [9, 5]);
    E('C', 17, 5, 1, { C: '470u' }, 'Cf'); W([15, 3], [17, 3]); W([15, 5], [15, 7], [17, 7]);
    E('R', 20, 5, 1, { R: '100' }, 'RL'); W([17, 3], [20, 3]); W([17, 7], [20, 7]);
    E('VP2', 22, 5, 0, {}, 'Vdc'); W([20, 3], [22, 3]); W([20, 7], [22, 7]);
    E('SIMCTRL', 27, 1, 0, { dt: '10u', tt: '0.1', tp: '0', ps: '1' });
    E('TEXT', 0, -2, 0, { text: '單相橋式整流：220 Vrms / 50 Hz，470 µF 濾波電容，100 Ω 負載。' });
  } },
  tl431: { name: 'TL431 + 光耦合器回授特性', build: (E, W) => {
    E('VPWL', 0, 4, 0, { f: '0', times: '0 1m 19m 20m', vals: '4.8 4.8 5.2 5.2' }, 'Vs'); E('GND', 0, 6);
    W([0, 2], [2, 2]); W([2, 2], [4, 2]); W([4, 2], [6, 2]); E('VP', 2, 0, 0, {}, 'Vout');
    E('IP', 8, 2, 0, {}, 'Iled'); E('R', 12, 2, 0, { R: '1k' }, 'Rled'); W([14, 2], [14, 3]);
    E('OPTO1', 17, 4, 0, { ctr: '100' }, 'PC1');
    E('TL431', 14, 9, 0, {}, 'U1'); W([14, 5], [14, 7]); E('GND', 14, 11);
    E('R', 4, 5, 1, { R: '10k' }, 'Rup'); W([4, 2], [4, 3]);
    E('R', 4, 11, 1, { R: '10k' }, 'Rdn'); W([4, 7], [4, 8]); W([4, 8], [4, 9]); E('GND', 4, 13);
    W([4, 8], [8, 8]); W([8, 8], [12, 8]); W([12, 8], [12, 9]); E('VP', 8, 6, 0, {}, 'Vref');
    E('VDC', 28, 6, 0, { V: '5' }, 'Vcc'); E('GND', 28, 8);
    E('R', 25, 1, 1, { R: '10k' }, 'Rpu'); W([20, 3], [21, 3]); W([21, 3], [25, 3]); W([25, -1], [28, -1], [28, 4]);
    W([20, 5], [20, 7]); E('GND', 20, 7);
    E('VP', 21, 1, 0, {}, 'Vfb');
    E('SIMCTRL', 34, 1, 0, { dt: '1u', tt: '20m', tp: '0', ps: '5' });
    E('TEXT', 0, -6, 0, { text: 'TL431 + PC817 類光耦合器：Vout 由 4.8 V 慢慢升到 5.2 V，分壓 10k/10k → 約 5.0 V 時 TL431 開始吸電流，\nLED 電流經 CTR 轉到光電晶體，Vfb 由 5 V 拉低。可觀察 Imin 低轉導區與 CTR 對回授增益的影響。' });
  } },
  doublepulse: { name: '雙脈衝測試（非理想 MOSFET + 二極體反向恢復 + 雜散電感）', build: (E, W) => {
    E('VDC', 0, 6, 0, { V: '400' }, 'Vdc'); E('GND', 0, 8);
    W([0, 4], [0, 2], [1, 2]); E('RL', 4, 2, 0, { R: '0.5', L: '30n', i0: '0' }, 'Lσ'); W([7, 2], [8, 2]); W([8, 2], [12, 2]);
    E('DIODE', 8, 4, 3, { model: 'ds', vfd: '1.5', ifd: '20', rd: '15m', qrr: '400n', irr: '20', cjd: '60p', vcj: '100', bvd: '650' }, 'D1');
    E('L', 12, 4, 1, { L: '200u', i0: '0' }, 'L1');
    W([8, 6], [10, 6]); W([10, 6], [12, 6]); W([12, 6], [14, 6]);
    E('VP2', 14, 8, 0, {}, 'Vds'); W([14, 10], [14, 14], [10, 14]);
    E('IP', 10, 8, 1, {}, 'Id');
    E('MOSNI', 10, 12, 0, { qgd: '15n', vq: '400', vcap: '25' }, 'Q1');
    E('RL', 10, 17, 1, { R: '0.02', L: '5n', i0: '0' }, 'Ls'); E('GND', 10, 20);
    E('GATING', 3, 12, 0, { f: '0', n: '4', pts: '2u 12u 17u 20u' }, 'G1'); W([5, 12], [8, 12]);
    E('DEVP', 20, 8, 0, { el: 'Q1', q: 'vg' }, 'Vgs'); E('DEVP', 20, 11, 0, { el: 'Q1', q: 'p' }, 'Pq1'); E('DEVP', 20, 14, 0, { el: 'Q1', q: 'e' }, 'Eq1');
    E('SIMCTRL', 20, 2, 0, { dt: '0.5n', tt: '24u', tp: '0', ps: '2' });
    E('TEXT', 0, -3, 0, { text: '雙脈衝測試：第一個脈衝把電感電流充到約 20 A，第二次導通時可看到二極體反向恢復電流尖峰與米勒平台（Vgs）。\n功率迴路雜散電感 Lσ = 30 nH（含 0.5 Ω 迴路等效電阻）與 Coss / 二極體接面電容造成關斷時的 Vds 過衝與振鈴；共源極電感 Ls = 5 nH 拖慢開通。\nQ1 用資料手冊參數：Qgd = 15 nC @ 400 V（米勒平台長度 = Qgd/Ig）、Ciss/Crss/Coss @ 25 V（Cds 隨 Vds 變化）；D1 用資料手冊參數：Vf 1.5 V @ 20 A、Qrr 400 nC、Cj 60 pF @ 100 V。\nVds 以兩點探棒量 D–S（不含 Ls 的壓降）。Pq1 = Vds·Id 瞬時功率，Eq1 = ∫Pq1 dt；用游標夾住一次開通或關斷，Eq1 的 Δ（或 Pq1 的 ∫dt）就是 Eon / Eoff。' });
  } }
};
function loadExample(k) {
  const ex = EXAMPLES[k]; pushUndo(); ED.doc = ex.doc ? docFromJson(ex.doc) : makeDoc(ex.build); ED.docLayout = ex.screens || null; WV.layout = null; ED.sel.clear(); ED.fileName = ex.name; ED.lastRes = null; ED.lastAC = null; ED.dirty = false; changed(); fitView();
}

/* ------------------------------------------------------------------
 * Help / about
 * ------------------------------------------------------------------ */
function helpDialog() {
  const rows = [['/ 或在空白處雙擊', '快速新增：輸入元件名稱（英文或中文），Enter 放在該處'], ['點選元件庫項目', '灰色元件跟著游標移動，左鍵放置（按住 Shift 可連續放置）'], ['由元件庫拖曳', '直接拖到電路圖上放置'], ['點元件旁的名稱 / 參數文字', '就地修改（Enter 確定、Tab 下一個、Esc 取消）；選單式或程式碼參數會開啟屬性視窗'], ['滑鼠停在元件上', '顯示名稱、參數與接腳類型；停在接腳上會標出接腳並在狀態列顯示類型'], ['Space / Shift+Space', '旋轉 90°（順時針 / 逆時針；R 也可以）'], ['X', '左右鏡像'], ['Y', '上下鏡像'], ['D', '停用 / 啟用（停用的元件以灰色虛線顯示，不參與模擬）'], ['W', '畫導線（從接腳拖曳也可直接拉線）'], ['L', '放置節點標籤'], ['G', '放置接地'], ['Enter / 雙擊', '編輯屬性'], ['Delete', '刪除'], ['Ctrl+Z / Ctrl+Y', '復原 / 重做'], ['Ctrl+C / X / V', '複製 / 剪下 / 貼上'], ['Ctrl+D', '複製並貼上'], ['Ctrl+A', '全選'], ['方向鍵', '移動選取的元件 1 格'], ['滾輪 / + −', '縮放'], ['右鍵拖曳 / 中鍵拖曳', '平移'], ['F', '全部顯示'], ['F5', '執行模擬（有 AC Sweep 元件時執行掃頻）'], ['F6', '開啟上次的模擬結果'], ['F7', 'Simulation Control 模擬控制'], ['Ctrl+S / Ctrl+O', '儲存 / 開啟電路檔'], ['Ctrl+F', '搜尋電路圖（元件名稱、類型、標籤文字），Enter / Shift+Enter 下一個 / 上一個'], ['Ctrl+Shift+F', '搜尋元件庫'], ['Esc', '取消目前動作']];
  const t = h('table', { class: 'keys' }); rows.forEach(r => t.append(h('tr', null, h('td', null, r[0]), h('td', null, r[1]))));
  const legend = h('div', { class: 'pinlist', style: 'margin:0 0 10px', html: '接腳類型標記（每隻腳的導線上）：<span class="pk pk-p"><i></i>功率</span><span class="pk pk-ci"><i></i>控制輸入（三角形指向元件）</span><span class="pk pk-co"><i></i>控制輸出（三角形指向外）</span><span class="pk pk-a"><i></i>功率或控制皆可</span>。功率接腳與控制接腳不能直接相連，需經由電壓/電流感測器（功率→控制）或 On-Off Controller、Control-Power Interface（控制→功率）。「檢視」選單可隱藏標記；滑鼠移到接腳上，狀態列會顯示接腳名稱與類型。' });
  const w = h('div', null, legend, h('p', { class: 'note' }, '波形視窗：拖曳框選放大時間軸、滾輪縮放、Shift+滾輪縮放 Y 軸、雙擊還原；「✋ 平移」工具可用左鍵拖曳平移（任何時候中鍵或 Alt+左鍵亦可）；「⬚ 區域放大」工具框選一塊區域即同時放大 X 與 Y，Esc 離開工具；開啟「游標」後左鍵設 C1、右鍵（或 Shift+左鍵）設 C2。拖曳游標線上方的把手（C1 / C2）可移動游標；工具列的 C1 / C2 決定點選波形時設定哪一支游標。量測表的「∫dt」欄是對時間的積分（功率波形夾住一次切換 = Eon / Eoff）。點選畫面即選為目前畫面（左上角紅色方塊標示），探棒訊號會加入此畫面。每個畫面上方的標題列有圖例（旁邊顯示游標處的數值）與 X−/X+/◀/▶（X 軸縮放、平移）、Y−/Y+/▲/▼（Y 軸縮放、平移）、⤢（自動範圍）、▭（只看這個畫面）；工具都在標題列，不會蓋到波形。鍵盤：← → 平移、+ − 縮放 X、↑ ↓ 平移 Y、Shift+↑↓ 縮放 Y、Home 自動範圍、A 全部顯示、C 游標、數字鍵切換畫面；「側欄」可收起左側面板。在波形上中鍵拖曳或 Alt+左鍵拖曳可同時平移 X/Y，滾輪縮放 X、Shift/Ctrl+滾輪縮放 Y，雙擊恢復該畫面。「X 軸連動」開啟時 X 軸操作同步到所有畫面，關閉時各畫面獨立（Y 軸永遠獨立）。「外觀」可設定字體、字體大小、線寬；點波形旁的色塊開啟色盤更改顏色（依探棒名稱記住）。「⧉ 獨立視窗」把波形移到另一個瀏覽器視窗（可拖到第二個螢幕），之後的模擬結果自動更新到該視窗；瀏覽器不允許開新視窗時改為可拖曳、可調整大小的浮動視窗。電路圖上右鍵拖曳可平移，右鍵單擊仍開啟選單。手機／平板：單指在空白處拖曳平移、雙指縮放；點元件選取、拖曳移動；從接腳拖曳拉線；長按開啟選單；點兩下開啟屬性；底部工具列提供執行、波形、框選、導線、旋轉、鏡像、停用、屬性、複製、貼上、刪除、復原、搜尋、完成（工具列可左右滑動）；點到接腳時會顯示接腳名稱與類型。波形上單指拖曳平移、雙指水平張開縮放 X、垂直張開縮放 Y、點一下讀值、點兩下自動範圍；手機上點各畫面的「⇔ 軸」展開座標軸工具；「量測」按鈕從底部拉出量測表（Max / Min / Avg / RMS / P-P / ∫dt）與游標讀值。'), t);
  modal('操作說明與快捷鍵', w, [{ label: '關閉', pri: true }], { wide: true });
}
function aboutDialog() {
  const b = h('div', null, h('dl', { class: 'kv' },
    h('dt', null, '工具名稱'), h('dd', null, '電路模擬器（PSIM 風格的電力電子電路模擬）'),
    h('dt', null, '版本'), h('dd', { class: 'mono' }, APP.version),
    h('dt', null, '原作者'), h('dd', null, APP.author),
    h('dt', null, '協作者'), h('dd', null, APP.contributors || '—')),
    h('div', { class: 'descBox' }, h('b', null, '求解方法'), h('div', null, '修正節點分析法（MNA）、梯形積分並於切換後以兩步 h/8 後向尤拉抑制數值振盪；稀疏 LU（Markowitz 選主元、靜態重放、疊代精修）；開關以 10 µΩ / 1 MΩ 分段線性模型表示並以狀態疊代求解；二極體與比較器等的切換瞬間先以線性內插定位、再以割線法精修（誤差通常低於步長的 1/100）並分割步長；可選的自動細分步長以三階差分估計局部截斷誤差，超出容許值時將該步對半細分（最多 4 層）；訊號源轉折點、取樣時刻與脈波邊緣皆為模擬斷點。半導體可選 SPICE 物理模型（二極體 level-1、BJT Gummel-Poon、MOSFET level-1、開集極比較器），以牛頓法加接面電壓限制（pnjlim）求解，t = 0 為真正的直流工作點（電感與電壓源串聯或節點僅由電感連接時，初始電流為 0 的電感視為短路求直流穩態）。非理想開關可用「元件內部探棒」DEVP 直接讀取 Vgs、Vds、Id、瞬時功率與累積能量（Eon / Eoff）。AC 掃頻支援小訊號注入法（DFT + 線性趨勢去除 + 漣波抵銷）與線性頻域法。')),
    h('p', { class: 'note' }, '版本規則：重大核心改版進位第一碼；新增或調整小功能進位第二碼；錯誤修正進位第三碼。修改本工具時請保留原作者，並可將自己的名字加入協作者。'));
  modal('關於', b, [{ label: '關閉', pri: true }]);
}

/* ------------------------------------------------------------------
 * Start-up
 * ------------------------------------------------------------------ */
const ICONS = {
  new: '<path d="M6 3h8l4 4v14H6z"/><path d="M14 3v4h4"/>', open: '<path d="M3 7h6l2 2h10v10H3z"/>', save: '<path d="M5 3h12l3 3v15H5z"/><path d="M8 3v5h8V3M8 21v-7h8v7"/>',
  undo: '<path d="M9 7L4 12l5 5"/><path d="M4 12h11a5 5 0 0 1 0 10h-3"/>', redo: '<path d="M15 7l5 5-5 5"/><path d="M20 12H9a5 5 0 0 0 0 10h3"/>',
  rot: '<path d="M20 12a8 8 0 1 1-3-6.2"/><path d="M20 4v5h-5"/>', mirx: '<path d="M12 3v18"/><path d="M9 7L4 12l5 5z"/><path d="M15 7l5 5-5 5z"/>', miry: '<path d="M3 12h18"/><path d="M7 9l5-5 5 5z"/><path d="M7 15l5 5 5-5z"/>',
  dis: '<circle cx="12" cy="12" r="8"/><path d="M6.5 6.5l11 11"/>', wire: '<path d="M4 18h6V6h10"/><circle cx="4" cy="18" r="1.5"/><circle cx="20" cy="6" r="1.5"/>', label: '<path d="M4 12l4-5h12v10H8z"/>', gnd: '<path d="M12 4v8M5 12h14M8 16h8M11 20h2"/>',
  ctrl: '<circle cx="12" cy="12" r="8"/><path d="M12 7v5l3 2"/>', run: '<path d="M7 5l12 7-12 7z"/>', wave: '<path d="M3 12c2-6 4-6 6 0s4 6 6 0 4-6 6 0"/>', fit: '<path d="M4 9V4h5M15 4h5v5M20 15v5h-5M9 20H4v-5"/>',
  zin: '<circle cx="11" cy="11" r="6"/><path d="M20 20l-4.5-4.5M8 11h6M11 8v6"/>', zout: '<circle cx="11" cy="11" r="6"/><path d="M20 20l-4.5-4.5M8 11h6"/>', lib: '<path d="M4 4h6v16H4zM14 4h6v16h-6z"/>', help: '<circle cx="12" cy="12" r="9"/><path d="M9.5 9a2.5 2.5 0 1 1 3.5 2.3c-.7.3-1 .9-1 1.7M12 17h.01"/>',
  theme: '<path d="M20 14.5A8 8 0 1 1 9.5 4a6.5 6.5 0 0 0 10.5 10.5z"/>', find: '<circle cx="10.5" cy="10.5" r="6"/><path d="M20 20l-5-5"/>'
};
const ico = k => `<svg viewBox="0 0 24 24">${ICONS[k]}</svg>`;
function buildTop() {
  const menus = [
    ['檔案', [['新增電路', 'Ctrl+N', newDoc], ['開啟檔案…', 'Ctrl+O', openFile], ['由文字匯入…', '', pasteJsonDialog], ['儲存 / 匯出…', 'Ctrl+S', saveDialog], null,
      ['範例：Buck 轉換器', '', () => loadExample('buck')], ['範例：Buck AC 掃頻', '', () => loadExample('buckac')], ['範例：RLC 波德圖', '', () => loadExample('rlc')], ['範例：橋式整流', '', () => loadExample('bridge')], ['範例：Supervisor 看門狗比較器', '', () => loadExample('supervisor')], ['範例：TL431 + 光耦合器回授', '', () => loadExample('tl431')], ['範例：雙脈衝測試（非理想 MOSFET）', '', () => loadExample('doublepulse')], ['範例：三相併網逆變器（dq + PQ 控制）', '', () => loadExample('gridinv')], ['範例：單相全橋 PFC（數位控制）', '', () => loadExample('pfc')]]],
    ['編輯', [['復原', 'Ctrl+Z', undo], ['重做', 'Ctrl+Y', redo], null, ['剪下', 'Ctrl+X', () => copySel(true)], ['複製', 'Ctrl+C', () => copySel(false)], ['貼上', 'Ctrl+V', pasteClip], ['刪除', 'Del', deleteSel], ['全選', 'Ctrl+A', () => { ED.doc.elements.forEach(e => ED.sel.add('e:' + e.id)); ED.doc.wires.forEach(w => ED.sel.add('w:' + w.id)); render(); }], null,
      ['旋轉', 'Space', () => rotateSel(1)], ['左右鏡像', 'X', () => mirrorSel(false)], ['上下鏡像', 'Y', () => mirrorSel(true)], ['停用 / 啟用', 'D', toggleDisable], null, ['導線', 'W', () => setMode('wire')], ['標籤', 'L', () => startPlace('LABEL')], ['接地', 'G', () => startPlace('GND')], ['文字', '', () => startPlace('TEXT')], null, ['快速新增元件…', '/', () => { const r = svg.getBoundingClientRect(); openQuickAdd(r.left + r.width / 2 - 180, r.top + 60, null); }], ['搜尋電路圖…', 'Ctrl+F', openFind], ['搜尋元件庫', 'Ctrl+Shift+F', () => { if (innerWidth <= 1100 && !$('#main').classList.contains('libOpen')) toggleLib(); $('#libSearch').focus(); }]]],
    ['檢視', [['全部顯示', 'F', fitView], ['放大', '+', () => zoomAt(0.8, ED.vb.x + ED.vb.w / 2, ED.vb.y + ED.vb.h / 2)], ['縮小', '−', () => zoomAt(1.25, ED.vb.x + ED.vb.w / 2, ED.vb.y + ED.vb.h / 2)], null, ['顯示 / 隱藏元件庫', '', toggleLib], ['顯示 / 隱藏接腳類型標記', '', togglePinMarks], ['切換深色 / 淺色', '', toggleTheme]]],
    ['模擬', [['模擬控制 Simulation Control…', 'F7', () => openSimCtrl()], ['放置 AC Sweep', '', () => startPlace('ACSWEEP')], ['放置參數檔', '', () => startPlace('PARAMS')], null, ['執行模擬', 'F5', () => runSim(false)], ['只執行暫態模擬', '', () => runSim(true)], ['開啟上次結果', 'F6', openLastResult]]],
    ['說明', [['操作說明與快捷鍵', '', helpDialog], ['關於', '', aboutDialog]]]
  ];
  const top = $('#menus');
  menus.forEach(([name, items]) => {
    const drop = h('div', { class: 'drop' });
    items.forEach(it => { if (!it) { drop.append(h('hr')); return; } drop.append(h('button', { onclick: () => { closeMenus(); it[2](); } }, h('span', null, it[0]), h('kbd', null, it[1]))); });
    const m = h('div', { class: 'menu' }, h('button', { onclick: (e) => { const open = m.classList.contains('open'); closeMenus(); if (!open) { m.classList.add('open'); placeDrop(m, drop); } e.stopPropagation(); }, onmouseenter: () => { if ($('.menu.open') && !m.classList.contains('open')) { closeMenus(); m.classList.add('open'); placeDrop(m, drop); } } }, name), drop);
    top.append(m);
  });
  document.addEventListener('click', e => { if (!e.target.closest('.menu')) closeMenus(); });
  const tb = $('#tools');
  const B = (k, title, fn, mode) => { const b = h('button', { class: 'tb', title, 'aria-label': title, html: ico(k), onclick: fn }); if (mode) b.dataset.mode = mode; tb.append(b); return b; };
  B('lib', '元件庫', toggleLib); tb.append(h('span', { class: 'sep' }));
  B('new', '新增 (Ctrl+N)', newDoc); B('open', '開啟 (Ctrl+O)', openFile); B('save', '儲存 (Ctrl+S)', saveDialog); tb.append(h('span', { class: 'sep' }));
  B('undo', '復原 (Ctrl+Z)', undo); B('redo', '重做 (Ctrl+Y)', redo); tb.append(h('span', { class: 'sep' }));
  B('rot', '旋轉 (Space)', () => rotateSel(1)); B('mirx', '左右鏡像 (X)', () => mirrorSel(false)); B('miry', '上下鏡像 (Y)', () => mirrorSel(true)); B('dis', '停用/啟用 (D)', toggleDisable); tb.append(h('span', { class: 'sep' }));
  B('wire', '導線 (W)', () => setMode(ED.mode === 'wire' ? 'select' : 'wire'), 'wire'); B('label', '標籤 (L)', () => startPlace('LABEL')); B('gnd', '接地 (G)', () => startPlace('GND')); tb.append(h('span', { class: 'sep' }));
  B('find', '搜尋電路圖 (Ctrl+F)', openFind); tb.append(h('span', { class: 'sep' }));
  B('ctrl', '模擬控制 (F7)', () => openSimCtrl());
  const run = h('button', { class: 'tb run', title: '執行模擬 (F5)', html: ico('run') + '<span>執行</span>', onclick: () => runSim(false) }); tb.append(run);
  B('wave', '波形 / 波德圖 (F6)', openLastResult);
  const tr = $('#topRight');
  tr.append(h('button', { class: 'tb', title: '切換深色/淺色', 'aria-label': '切換深色/淺色', html: ico('theme'), onclick: toggleTheme }), h('button', { class: 'tb', title: '說明', 'aria-label': '說明', html: ico('help'), onclick: helpDialog }));
  // quick element bar
  refreshQuick();
  const z = $('#zoomBox');
  z.append(h('button', { class: 'tb', title: '放大', 'aria-label': '放大', html: ico('zin'), onclick: () => zoomAt(0.8, ED.vb.x + ED.vb.w / 2, ED.vb.y + ED.vb.h / 2) }), h('button', { class: 'tb', title: '縮小', 'aria-label': '縮小', html: ico('zout'), onclick: () => zoomAt(1.25, ED.vb.x + ED.vb.w / 2, ED.vb.y + ED.vb.h / 2) }), h('button', { class: 'tb', title: '全部顯示 (F)', 'aria-label': '全部顯示', html: ico('fit'), onclick: fitView }));
}
const QUICK_FIXED = ['R', 'L', 'C', 'DIODE', 'MOSFET', 'IGBT', 'VDC', 'VSIN', 'VTRI', 'COMPOC', 'COMP', 'ONCTRL', 'VSEN', 'ISEN', 'VP', 'IP', 'GND'];
function refreshQuick() {
  const q = $('#quick'); if (!q) return; q.innerHTML = '';
  const add = h('button', { class: 'tb qplus', title: '新增元件：輸入名稱搜尋（/ 鍵，或在空白處雙擊）', 'aria-label': '新增元件', html: '<svg viewBox="0 0 24 24"><path d="M12 5v14M5 12h14"/></svg>', onclick: e => { const r = e.currentTarget.getBoundingClientRect(); openQuickAdd(r.left, r.bottom + 6, null); } });
  q.append(add, h('span', { class: 'sep' }));
  let recent = []; try { recent = JSON.parse(store.get('psimweb.recent') || '[]'); } catch (e) { recent = []; }
  recent = recent.filter(t => LIB.D[t] && !QUICK_FIXED.includes(t)).slice(0, 6);
  const mk = (t, cls) => {
    const b = h('button', { class: 'tb' + (cls ? ' ' + cls : ''), title: LIB.D[t].name + '（' + LIB.D[t].zh + '）', 'aria-label': LIB.D[t].name, html: iconSvg(t, 30, 22), draggable: 'true' });
    b.onclick = () => { showLibDesc(t); startPlace(t); touchPlaceHint(); };
    b.ondragstart = e => { e.dataTransfer.setData('text/plain', 'psim:' + t); ED.dragType = t; };
    q.append(b);
  };
  QUICK_FIXED.forEach(t => mk(t));
  if (recent.length) { q.append(h('span', { class: 'sep', title: '最近使用' })); recent.forEach(t => mk(t, 'recent')); }
}
function placeDrop(m, drop) { const r = m.getBoundingClientRect(); drop.style.top = r.bottom + 'px'; drop.style.left = Math.max(4, Math.min(innerWidth - drop.offsetWidth - 4, r.left)) + 'px'; }
function closeMenus() { $$('.menu.open').forEach(m => m.classList.remove('open')); }
function toggleLib() { const m = $('#main'); if (innerWidth <= 1100) m.classList.toggle('libOpen'); else m.classList.toggle('nolib'); setTimeout(render, 0); }
function togglePinMarks() { ED.pinMarks = !ED.pinMarks; store.set('psimweb.pinMarks', ED.pinMarks ? '1' : '0'); render(); toast(ED.pinMarks ? '已顯示接腳類型標記' : '已隱藏接腳類型標記'); }
function toggleTheme() {
  const r = document.documentElement; const cur = r.getAttribute('data-theme');
  const isDark = cur ? cur === 'dark' : matchMedia('(prefers-color-scheme: dark)').matches;
  r.setAttribute('data-theme', isDark ? 'light' : 'dark'); store.set('psimweb.theme', isDark ? 'light' : 'dark');
  render(); if (!$('#wave').hidden) drawWave(); if (!$('#bode').hidden) drawBode();
}
function initViewers() {
  const wt = $('#waveTools');
  const T = (label, fn, cls) => { const b = h('button', { class: 'tb' + (cls ? ' ' + cls : ''), onclick: fn }, label); wt.append(b); return b; };
  T('全部顯示', waveFit); T('X 放大', () => { const m = (WV.xa + WV.xb) / 2, d = (WV.xb - WV.xa) / 4; zoomX(m - d, m + d); }); T('X 縮小', () => { const m = (WV.xa + WV.xb) / 2, d = (WV.xb - WV.xa); zoomX(m - d, m + d); });
  const cb = T('游標', () => { WV.cur = !WV.cur; cb.classList.toggle('on', WV.cur); csel.style.display = WV.cur ? '' : 'none'; if (WV.cur && WV.c1 == null) { const [a, b] = sX(WV.screens[WV.act]); WV.c1 = a + (b - a) * 0.3; WV.c2 = a + (b - a) * 0.7; } drawWave(); });
  // which cursor a tap / click places (touch has no right button); dragging a handle moves either one
  const csel = h('span', { class: 'csel', style: 'display:none' });
  const c1b = h('button', { class: 'tb on', title: '點波形時設定 C1' }, 'C1'), c2b = h('button', { class: 'tb', title: '點波形時設定 C2' }, 'C2');
  const pick = w => { WV.csel = w; c1b.classList.toggle('on', w !== 2); c2b.classList.toggle('on', w === 2); drawWave(); };
  c1b.onclick = () => pick(1); c2b.onclick = () => pick(2); csel.append(c1b, c2b); WV.csel = 1;
  wt.append(csel);
  // drag tools: hand = pan X/Y of a screen, rect = zoom into a dragged rectangle (X and Y); off = X-range drag (default)
  WV.tool = null;
  const setTool = t => { WV.tool = WV.tool === t ? null : t; hb.classList.toggle('on', WV.tool === 'hand'); rb.classList.toggle('on', WV.tool === 'rect'); $$('#wPlots canvas').forEach(c => { c.style.cursor = WV.tool === 'hand' ? 'grab' : WV.tool === 'rect' ? 'crosshair' : ''; }); };
  const hb = T('✋ 平移', () => setTool('hand')); hb.title = '手形工具：左鍵拖曳平移該畫面的 X/Y（亦可隨時用中鍵或 Alt+左鍵拖曳）；再按一次或 Esc 關閉';
  const rb = T('⬚ 區域放大', () => setTool('rect')); rb.title = '區域放大：左鍵框選一塊區域，X 與 Y 同時放大到該區塊；雙擊畫面可回到自動範圍；再按一次或 Esc 關閉';
  WV.setTool = setTool;
  WV.link = store.get('psimweb.xlink') === '1';
  const lk = T('X 軸連動', () => { WV.link = !WV.link; lk.classList.toggle('on', WV.link); store.set('psimweb.xlink', WV.link ? '1' : '0'); if (WV.link && WV.res) { const [a, b] = sX(WV.screens[WV.act]); zoomX(a, b); } });
  lk.title = '開啟：任一畫面的 X 軸縮放/平移同步到所有畫面；關閉：各畫面獨立（Y 軸永遠各自獨立）'; lk.classList.toggle('on', WV.link);
  T('外觀', waveStyleDialog).title = '字體、字體大小、線寬與每條波形的顏色';
  T('新增畫面', () => { WV.screens.push({ curves: [], y: null }); WV.act = WV.screens.length - 1; waveSave(); buildPlots(); drawWave(); });
  T('刪除畫面', () => { if (WV.screens.length <= 1) return; WV.screens.splice(WV.act, 1); WV.act = Math.max(0, WV.act - 1); waveSave(); buildPlots(); drawWave(); });
  T('匯出 CSV', waveCsv);
  const sb = T(window.innerWidth <= 760 ? '量測' : '側欄', () => { const v = $('#wave'); v.classList.toggle('noside'); sb.classList.toggle('on', !v.classList.contains('noside')); store.set('psimweb.wside', v.classList.contains('noside') ? '0' : '1'); drawWave(); });
  sb.title = '顯示 / 隱藏左側的探棒清單與量測表（隱藏時波形占滿整個視窗）'; { const ws = store.get('psimweb.wside'); const hide = ws === '0' || (ws == null && window.innerWidth <= 760); if (hide) $('#wave').classList.add('noside'); else sb.classList.add('on'); }
  wt.append(h('span', { id: 'wRange', class: 'mono note', style: 'margin-left:6px' }));
  // keyboard: arrows pan the active screen, +/- zoom X, Shift+up/down zoom Y, Home auto range, A all, C cursors, 1-9 pick the screen
  document.addEventListener('keydown', e => {
    const v = $('#wave'); if (v.hidden || $('.modal') || /^(INPUT|TEXTAREA|SELECT)$/.test((e.target && e.target.tagName) || '')) return;
    if (v.classList.contains('float') && !v.contains(e.target) && e.target !== document.body) return;
    const si = WV.act; let used = true;
    if (e.key === 'ArrowLeft') scrPanX(si, -0.1);
    else if (e.key === 'ArrowRight') scrPanX(si, 0.1);
    else if (e.key === 'ArrowUp') { if (e.shiftKey) scrZoomY(si, 0.8); else scrPanY(si, 0.1); }
    else if (e.key === 'ArrowDown') { if (e.shiftKey) scrZoomY(si, 1.25); else scrPanY(si, -0.1); }
    else if (e.key === '+' || e.key === '=') scrZoomX(si, 0.8);
    else if (e.key === '-' || e.key === '_') scrZoomX(si, 1.25);
    else if (e.key === 'Home') scrFit(si);
    else if (e.key === 'a' || e.key === 'A') waveFit();
    else if (e.key === 'c' || e.key === 'C') cb.click();
    else if (/^[1-9]$/.test(e.key) && +e.key <= WV.screens.length) setAct(+e.key - 1);
    else used = false;
    if (used) e.preventDefault();
  });
  if (!window.__WAVE_POPUP__) wt.append(h('button', { class: 'tb', id: 'wPop', style: 'margin-left:auto', title: '把波形移到獨立的瀏覽器視窗（可拖到另一個螢幕），之後的模擬結果會自動更新到該視窗', onclick: popoutWave }, '⧉ 獨立視窗'));
  wt.append(h('button', { class: 'tb', style: window.__WAVE_POPUP__ ? 'margin-left:auto' : '', onclick: () => { if (window.__WAVE_POPUP__) window.close(); else { const v = $('#wave'); v.hidden = true; v.classList.remove('float'); } } }, '關閉 (Esc)'));
  new ResizeObserver(() => { if (!$('#wave').hidden) drawWave(); }).observe($('#wave'));
  initFloatDrag();
  const bt = $('#bodeTools');
  bt.append(h('button', { class: 'tb', onclick: bodeCsv }, '匯出 CSV'), h('button', { class: 'tb', onclick: () => { if (ED.lastRes) openWave(ED.lastRes); } }, '暫態波形'), h('button', { class: 'tb', style: 'margin-left:auto', onclick: () => { $('#bode').hidden = true; } }, '關閉 (Esc)'));
  ['#bMag', '#bPh'].forEach(s => { const cv = $(s); cv.addEventListener('mousemove', e => bodeHover(e, cv)); cv.addEventListener('mouseleave', () => { BV.hover = null; drawBode(); }); });
  window.addEventListener('resize', () => { if (!$('#wave').hidden) drawWave(); if (!$('#bode').hidden) drawBode(); });
}
/* ------------------------------------------------------------------
 * Waveform pop-out window (falls back to a floating in-page window)
 * ------------------------------------------------------------------ */
let POPUPS = [];
function sendToPopups(res) {
  POPUPS = POPUPS.filter(w => { try { return !w.closed && w.__recvRes; } catch (e) { return false; } });
  let sent = false;
  for (const w of POPUPS) { try { w.__recvRes(res, WV.layout); sent = true; } catch (e) { /* window gone */ } }
  return sent;
}
function popoutWave() {
  const res = WV.res || ED.lastRes; if (!res) return;
  let w = null;
  try {
    w = window.open('', '_blank', 'popup=yes,width=1280,height=820');
    if (w) {
      const src = BOOT_HTML.replace(/<head([^>]*)>/i, '<head$1><script>window.__WAVE_POPUP__=1;<\/script>');
      if (!/__WAVE_POPUP__/.test(src)) throw new Error('no head');
      w.document.open(); w.document.write('<!doctype html>' + src); w.document.close();
      POPUPS.push(w);
      $('#wave').hidden = true; $('#wave').classList.remove('float');
      toast('波形已移到獨立視窗；之後的模擬結果會自動更新到該視窗');
      return;
    }
  } catch (e) { try { if (w) w.close(); } catch (e2) { /* ignore */ } }
  // pop-ups blocked (e.g. sandboxed page): floating window over the schematic instead
  floatWave(true);
  toast('瀏覽器不允許開新視窗，已改為可拖曳的浮動視窗（電路圖仍可操作）');
}
function floatWave(on) {
  const v = $('#wave'); v.classList.toggle('float', on); v.hidden = false;
  const b = $('#wPop'); if (b) b.textContent = on ? '⧉ 獨立視窗' : '⧉ 獨立視窗';
  requestAnimationFrame(drawWave);
}
function initFloatDrag() {
  const v = $('#wave'), bar = $('.vTop', v); let d = null;
  bar.addEventListener('pointerdown', e => {
    if (!v.classList.contains('float') || e.target.closest('button')) return;
    const r = v.getBoundingClientRect(); d = { x: e.clientX, y: e.clientY, l: r.left, t: r.top }; bar.setPointerCapture(e.pointerId);
  });
  bar.addEventListener('pointermove', e => { if (!d) return; v.style.left = Math.max(0, Math.min(innerWidth - 120, d.l + e.clientX - d.x)) + 'px'; v.style.top = Math.max(0, Math.min(innerHeight - 40, d.t + e.clientY - d.y)) + 'px'; });
  bar.addEventListener('pointerup', () => { d = null; });
}
function initPopup() {
  const op = window.opener;
  try { const th = op.document.documentElement.getAttribute('data-theme') || store.get('psimweb.theme'); if (th) document.documentElement.setAttribute('data-theme', th); } catch (e) { /* ignore */ }
  $('#app').hidden = true; initViewers();
  document.addEventListener('keydown', e => { if (e.key === 'Escape') window.close(); });
  window.__psimPopRes = () => WV.res;
  window.__recvRes = (res, layout) => {
    if (layout) WV.layout = JSON.parse(JSON.stringify(layout));
    openWave(res); try { document.title = '波形 — ' + op.__psimMain.name(); window.focus(); } catch (e) { /* ignore */ }
  };
  try { const M = op.__psimMain, res = M.res(); if (res) window.__recvRes(res, M.layout()); } catch (e) { document.body.append(h('p', { class: 'note', style: 'padding:20px' }, '無法讀取主視窗的模擬結果。')); }
}
function init() {
  if (window.__WAVE_POPUP__) { initPopup(); return; }
  // read-only hooks used by pop-out waveform windows (top-level consts are not window properties)
  window.__psimMain = { res: () => WV.res || ED.lastRes, layout: () => WV.layout || null, name: () => ED.fileName, wv: () => WV };
  const th = store.get('psimweb.theme'); if (th) document.documentElement.setAttribute('data-theme', th);
  initSvg(); buildTop(); buildLib(); initViewers();
  showLibDesc('R');
  $('#libSearch').addEventListener('input', libSearch);
  $('#libFindBtn').addEventListener('click', libSearch);
  bindSchematicPointers(); initTouch();
  svg.addEventListener('wheel', onWheel, { passive: false });
  svg.addEventListener('pointerleave', () => { if (ED.ghost) { ED.ghostOn = false; renderTop(); } hideTip(); if (ED.mode === 'select') svg.style.cursor = 'default'; });
  svg.addEventListener('pointerdown', () => { hideTip(); closeInline(); });
  // start panel buttons
  const wb = $('#welcome .wbtns');
  if (wb) {
    wb.append(h('button', { class: 'btn pri', onclick: e => { const r = e.currentTarget.getBoundingClientRect(); openQuickAdd(r.left, r.bottom + 6, null); } }, '＋ 新增元件'),
      h('button', { class: 'btn', onclick: openFile }, '開啟檔案…'),
      h('button', { class: 'btn', onclick: () => loadExample('buck') }, '範例：Buck'),
      h('button', { class: 'btn', onclick: () => loadExample('tl431') }, '範例：TL431 回授'),
      h('button', { class: 'btn', onclick: () => loadExample('doublepulse') }, '範例：雙脈衝'),
      h('button', { class: 'btn', onclick: helpDialog }, '操作說明'));
  }
  // drag & drop from the library and of circuit files
  const stage = $('#stage');
  stage.addEventListener('dragover', e => {
    e.preventDefault(); const t = ED.dragType;
    if (t) { e.dataTransfer.dropEffect = 'copy'; const [wx, wy] = toWorld(e); ED.mouse = [snapv(wx), snapv(wy)]; if (!ED.ghost || ED.ghost.type !== t) ED.ghost = { type: t, rot: 0, mir: false, dis: false, params: defaultParams(t) }; ED.ghostOn = true; renderTop(); }
  });
  stage.addEventListener('dragleave', e => { if (e.target === stage || !stage.contains(e.relatedTarget)) { if (ED.mode !== 'place') { ED.ghost = null; } ED.ghostOn = false; renderTop(); } });
  stage.addEventListener('drop', e => {
    e.preventDefault();
    const txt = e.dataTransfer.getData('text/plain') || '';
    if (txt.startsWith('psim:')) { const t = txt.slice(5); const [wx, wy] = toWorld(e); const g = ED.ghost && ED.ghost.type === t ? ED.ghost : { type: t, rot: 0, mir: false }; placeElement(g, snapv(wx), snapv(wy)); ED.dragType = null; setMode('select'); return; }
    if (e.dataTransfer.files && e.dataTransfer.files[0]) readFile(e.dataTransfer.files[0]);
  });
  document.addEventListener('keydown', onKey);

  document.addEventListener('keydown', e => { if ((e.ctrlKey || e.metaKey) && e.key.toLowerCase() === 'n' && !$('.modal')) { e.preventDefault(); newDoc(); } });
  window.addEventListener('resize', () => render());
  new ResizeObserver(() => render()).observe($('#stage'));
  // restore autosave or open the example
  let restored = false;
  const s = store.get('psimweb.autosave');
  if (s) { try { const o = JSON.parse(s); if (o.doc && o.doc.elements && o.doc.elements.length) { ED.doc = o.doc; ED.doc.elements = ED.doc.elements.filter(e => LIB.D[e.type]); ED.fileName = o.name || '電路'; restored = true; } } catch (e) { /* ignore */ } }
  if (!restored) { ED.doc = makeDoc(EXAMPLES.buck.build); ED.fileName = EXAMPLES.buck.name; }
  ED.conn = null; setMode('select'); requestAnimationFrame(() => { fitView(); });
  $('#ver').textContent = APP.version;
}
document.addEventListener('DOMContentLoaded', init);

/* generated by gen_examples.js from examples/*.psimx — do not edit by hand */
EXAMPLES["gridinv"] = { name: "三相併網逆變器（dq 電流控制 + PQ 命令 + SRF-PLL，類比控制）", screens: [["Ia","Ib","Ic"],["Id","Iq","Idref","Iqref"],["P","Q","Pref_","Qref_"],["Va","Vd","theta"]], doc: {"elements":[{"id":1,"type":"TEXT","x":0,"y":-2,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【主回路 Power Stage】 Vdc 800 V（分裂電容、中點接地）三相兩階 VSI → L 濾波 → 380 V/50 Hz 三線電網（中性點 N 浮接）"}},{"id":2,"type":"TEXT","x":0,"y":-4,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"介面標籤：GA/GB/GC、GAN/GBN/GCN（閘極）、IA/IB/IC（電流感測）、VA/VB/VC、N（PCC 相電壓感測）、TH（PLL 角度）、MD/MQ（調變命令）、TRI（載波）"}},{"id":3,"type":"VDC","x":4,"y":14,"rot":0,"mir":false,"dis":false,"name":"Vdc","params":{"V":"800"}},{"id":8,"type":"RC","x":8,"y":9,"rot":1,"mir":false,"dis":false,"name":"Cdc1","params":{"R":"0.01","C":"0.0022","v0":"400.0"}},{"id":9,"type":"RC","x":8,"y":19,"rot":1,"mir":false,"dis":false,"name":"Cdc2","params":{"R":"0.01","C":"0.0022","v0":"400.0"}},{"id":12,"type":"GND","x":8,"y":14,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":14,"type":"MOSFET","x":20,"y":9,"rot":0,"mir":false,"dis":false,"name":"QAH","params":{}},{"id":15,"type":"MOSFET","x":20,"y":19,"rot":0,"mir":false,"dis":false,"name":"QAL","params":{}},{"id":19,"type":"LABEL","x":16,"y":9,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GA"}},{"id":21,"type":"LABEL","x":16,"y":19,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GAN"}},{"id":23,"type":"MOSFET","x":30,"y":9,"rot":0,"mir":false,"dis":false,"name":"QBH","params":{}},{"id":24,"type":"MOSFET","x":30,"y":19,"rot":0,"mir":false,"dis":false,"name":"QBL","params":{}},{"id":28,"type":"LABEL","x":26,"y":9,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GB"}},{"id":30,"type":"LABEL","x":26,"y":19,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GBN"}},{"id":32,"type":"MOSFET","x":40,"y":9,"rot":0,"mir":false,"dis":false,"name":"QCH","params":{}},{"id":33,"type":"MOSFET","x":40,"y":19,"rot":0,"mir":false,"dis":false,"name":"QCL","params":{}},{"id":37,"type":"LABEL","x":36,"y":9,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GC"}},{"id":39,"type":"LABEL","x":36,"y":19,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GCN"}},{"id":44,"type":"ISEN","x":50,"y":8,"rot":0,"mir":false,"dis":false,"name":"ISA","params":{"k":"1"}},{"id":46,"type":"LABEL","x":50,"y":4,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IA"}},{"id":48,"type":"ISEN","x":50,"y":14,"rot":0,"mir":false,"dis":false,"name":"ISB","params":{"k":"1"}},{"id":50,"type":"LABEL","x":50,"y":10,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IB"}},{"id":54,"type":"ISEN","x":50,"y":20,"rot":0,"mir":false,"dis":false,"name":"ISC","params":{"k":"1"}},{"id":56,"type":"LABEL","x":50,"y":16,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IC"}},{"id":57,"type":"RL3","x":58,"y":14,"rot":0,"mir":false,"dis":false,"name":"Lf","params":{"R":"0.1","L":"0.002"}},{"id":70,"type":"LABEL","x":63,"y":5,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VA"}},{"id":71,"type":"WATT","x":67,"y":8,"rot":0,"mir":false,"dis":false,"name":"Pa","params":{"fc":"10"}},{"id":74,"type":"LABEL","x":67,"y":11,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":79,"type":"LABEL","x":63,"y":11,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VB"}},{"id":80,"type":"WATT","x":67,"y":14,"rot":0,"mir":false,"dis":false,"name":"Pb","params":{"fc":"10"}},{"id":83,"type":"LABEL","x":67,"y":17,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":90,"type":"LABEL","x":63,"y":17,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VC"}},{"id":91,"type":"WATT","x":67,"y":20,"rot":0,"mir":false,"dis":false,"name":"Pc","params":{"fc":"10"}},{"id":94,"type":"LABEL","x":67,"y":23,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":96,"type":"VSIN3","x":76,"y":14,"rot":0,"mir":true,"dis":false,"name":"Grid","params":{"V":"380","f":"50","ph":"0"}},{"id":105,"type":"LABEL","x":80,"y":14,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":106,"type":"TEXT","x":0,"y":30,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 1：電壓感測 + SRF-PLL】 vq = Vm·sin(θ−φ) → PI(負增益) → Δω，θ = ∫(ω0+Δω)dt（經單位延遲 z⁻¹ 斷開代數迴路）；鎖定時 vd = Vm、vq = 0"}},{"id":107,"type":"VSEN","x":6,"y":34,"rot":0,"mir":false,"dis":false,"name":"VSA","params":{"k":"1"}},{"id":108,"type":"LABEL","x":2,"y":33,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VA"}},{"id":110,"type":"LABEL","x":2,"y":35,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":112,"type":"VSEN","x":6,"y":40,"rot":0,"mir":false,"dis":false,"name":"VSB","params":{"k":"1"}},{"id":113,"type":"LABEL","x":2,"y":39,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VB"}},{"id":115,"type":"LABEL","x":2,"y":41,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":117,"type":"VSEN","x":6,"y":46,"rot":0,"mir":false,"dis":false,"name":"VSC","params":{"k":"1"}},{"id":118,"type":"LABEL","x":2,"y":45,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VC"}},{"id":120,"type":"LABEL","x":2,"y":47,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":122,"type":"ABC2DQO","x":20,"y":40,"rot":0,"mir":false,"dis":false,"name":"Vdq","params":{}},{"id":132,"type":"LABEL","x":17,"y":44,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"TH"}},{"id":136,"type":"LABEL","x":24,"y":36,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VD"}},{"id":137,"type":"PI","x":28,"y":40,"rot":0,"mir":false,"dis":false,"name":"PLLpi","params":{"k":"-0.3","T":"0.03"}},{"id":140,"type":"LABEL","x":25,"y":43,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VQ"}},{"id":141,"type":"SUM2","x":34,"y":40,"rot":0,"mir":false,"dis":false,"name":"wsum","params":{"k1":"1","k2":"1"}},{"id":143,"type":"CONST","x":32,"y":44,"rot":0,"mir":false,"dis":false,"name":"w0","params":{"v":"314.1593"}},{"id":145,"type":"I","x":40,"y":40,"rot":0,"mir":false,"dis":false,"name":"thInt","params":{"T":"1","y0":"-1.5707963267948966"}},{"id":147,"type":"UTD","x":46,"y":40,"rot":0,"mir":false,"dis":false,"name":"z1","params":{}},{"id":150,"type":"LABEL","x":50,"y":40,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"TH"}},{"id":151,"type":"VP","x":37,"y":38,"rot":0,"mir":false,"dis":false,"name":"w_pll","params":{}},{"id":152,"type":"VP","x":49,"y":38,"rot":0,"mir":false,"dis":false,"name":"theta","params":{}},{"id":153,"type":"VP","x":22,"y":34,"rot":0,"mir":false,"dis":false,"name":"Vd","params":{}},{"id":155,"type":"VP","x":11,"y":32,"rot":0,"mir":false,"dis":false,"name":"Va","params":{}},{"id":156,"type":"TEXT","x":6,"y":48,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 2：電流 dq 變換】 同一個 θ（PLL）"}},{"id":157,"type":"ABC2DQO","x":20,"y":56,"rot":0,"mir":false,"dis":false,"name":"Idq","params":{}},{"id":158,"type":"LABEL","x":12,"y":52,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IA"}},{"id":162,"type":"LABEL","x":12,"y":55,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IB"}},{"id":164,"type":"LABEL","x":12,"y":59,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IC"}},{"id":168,"type":"LABEL","x":14,"y":60,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TH"}},{"id":172,"type":"LABEL","x":28,"y":55,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"ID"}},{"id":173,"type":"VP","x":26,"y":53,"rot":0,"mir":false,"dis":false,"name":"Id","params":{}},{"id":176,"type":"LABEL","x":24,"y":60,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IQ"}},{"id":177,"type":"VP","x":27,"y":58,"rot":0,"mir":false,"dis":false,"name":"Iq","params":{}},{"id":179,"type":"VP","x":14,"y":50,"rot":0,"mir":false,"dis":false,"name":"Ia","params":{}},{"id":180,"type":"VP","x":14,"y":53,"rot":0,"mir":false,"dis":false,"name":"Ib","params":{}},{"id":181,"type":"VP","x":14,"y":57,"rot":0,"mir":false,"dis":false,"name":"Ic","params":{}},{"id":182,"type":"TEXT","x":0,"y":62,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 3：P*、Q* → id*、iq*】 id* = P*/(1.5·vd)，iq* = Q*/(1.5·vd)\nvd 下限 30 V 防止除以 0；Q > 0 = 輸出感性無效功率（電流落後電壓）"}},{"id":183,"type":"VSTEP2","x":4,"y":68,"rot":0,"mir":false,"dis":false,"name":"Pref","params":{"V1":"0","V2":"50000","ts":"0.02","tr":"0.03"}},{"id":184,"type":"GND","x":4,"y":70,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":185,"type":"MATH2","x":16,"y":67,"rot":0,"mir":false,"dis":false,"name":"idref","params":{"expr":"x1/(1.5*max(x2,30))"}},{"id":189,"type":"LABEL","x":11,"y":68,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VD"}},{"id":192,"type":"LABEL","x":20,"y":67,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IDR"}},{"id":193,"type":"VP","x":19,"y":65,"rot":0,"mir":false,"dis":false,"name":"Idref","params":{}},{"id":194,"type":"VSTEP2","x":4,"y":78,"rot":0,"mir":false,"dis":false,"name":"Qref","params":{"V1":"0","V2":"-20000","ts":"0.1","tr":"0.02"}},{"id":195,"type":"GND","x":4,"y":80,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":196,"type":"MATH2","x":16,"y":77,"rot":0,"mir":false,"dis":false,"name":"iqref","params":{"expr":"x1/(1.5*max(x2,30))"}},{"id":200,"type":"LABEL","x":11,"y":78,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VD"}},{"id":203,"type":"LABEL","x":20,"y":77,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IQR"}},{"id":204,"type":"VP","x":19,"y":75,"rot":0,"mir":false,"dis":false,"name":"Iqref","params":{}},{"id":205,"type":"VP","x":6,"y":64,"rot":0,"mir":false,"dis":false,"name":"Pref_","params":{}},{"id":207,"type":"VP","x":6,"y":74,"rot":0,"mir":false,"dis":false,"name":"Qref_","params":{}},{"id":209,"type":"TEXT","x":34,"y":59,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 4：dq 電流迴路 + 解耦前饋】 vd* = vd + PI(id*−id) + ωL·iq，vq* = vq + PI(iq*−iq) − ωL·id，再除以 Vdc/2 = 400 V"}},{"id":210,"type":"SUB2","x":34,"y":67,"rot":0,"mir":false,"dis":false,"name":"ed","params":{}},{"id":211,"type":"LABEL","x":29,"y":67,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IDR"}},{"id":213,"type":"LABEL","x":34,"y":71,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"ID"}},{"id":215,"type":"PI","x":40,"y":67,"rot":0,"mir":false,"dis":false,"name":"PId","params":{"k":"6","T":"0.005"}},{"id":217,"type":"SUM3","x":46,"y":67,"rot":0,"mir":false,"dis":false,"name":"vdsum","params":{"k1":"1","k2":"1","k3":"0.62832"}},{"id":219,"type":"LABEL","x":46,"y":63,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VD"}},{"id":221,"type":"LABEL","x":46,"y":71,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IQ"}},{"id":223,"type":"P","x":52,"y":67,"rot":0,"mir":false,"dis":false,"name":"md","params":{"k":"0.0025"}},{"id":226,"type":"LABEL","x":56,"y":67,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"MD"}},{"id":227,"type":"SUB2","x":34,"y":77,"rot":0,"mir":false,"dis":false,"name":"eq","params":{}},{"id":228,"type":"LABEL","x":29,"y":77,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IQR"}},{"id":230,"type":"LABEL","x":34,"y":81,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"IQ"}},{"id":232,"type":"PI","x":40,"y":77,"rot":0,"mir":false,"dis":false,"name":"PIq","params":{"k":"6","T":"0.005"}},{"id":234,"type":"SUM3","x":46,"y":77,"rot":0,"mir":false,"dis":false,"name":"vqsum","params":{"k1":"1","k2":"1","k3":"-0.62832"}},{"id":236,"type":"LABEL","x":46,"y":73,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VQ"}},{"id":238,"type":"LABEL","x":46,"y":81,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"ID"}},{"id":240,"type":"P","x":52,"y":77,"rot":0,"mir":false,"dis":false,"name":"mq","params":{"k":"0.0025"}},{"id":243,"type":"LABEL","x":56,"y":77,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"MQ"}},{"id":244,"type":"TEXT","x":66,"y":55,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 5：dq → abc 與 10 kHz 載波 PWM】 上臂 = (m > 載波)，下臂 = NOT（理想開關，無死區）"}},{"id":245,"type":"DQO2ABC","x":76,"y":70,"rot":0,"mir":false,"dis":false,"name":"mabc","params":{}},{"id":246,"type":"LABEL","x":66,"y":65,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"MD"}},{"id":250,"type":"LABEL","x":66,"y":67,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"MQ"}},{"id":254,"type":"CONST","x":65,"y":70,"rot":0,"mir":false,"dis":false,"name":"zero","params":{"v":"0"}},{"id":256,"type":"LABEL","x":66,"y":73,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TH"}},{"id":260,"type":"VTRI","x":72,"y":84,"rot":0,"mir":false,"dis":false,"name":"carrier","params":{"Vpp":"2","f":"10000","D":"0.5","off":"-1","ph":"0"}},{"id":261,"type":"GND","x":72,"y":86,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":263,"type":"LABEL","x":72,"y":80,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"TRI"}},{"id":264,"type":"COMP","x":88,"y":66,"rot":0,"mir":false,"dis":false,"name":"PWMA","params":{}},{"id":269,"type":"LABEL","x":84,"y":67,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TRI"}},{"id":272,"type":"LABEL","x":94,"y":66,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GA"}},{"id":273,"type":"NOT","x":96,"y":69,"rot":0,"mir":false,"dis":false,"name":"NA","params":{}},{"id":277,"type":"LABEL","x":100,"y":69,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GAN"}},{"id":278,"type":"COMP","x":88,"y":74,"rot":0,"mir":false,"dis":false,"name":"PWMB","params":{}},{"id":282,"type":"LABEL","x":84,"y":75,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TRI"}},{"id":285,"type":"LABEL","x":94,"y":74,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GB"}},{"id":286,"type":"NOT","x":96,"y":77,"rot":0,"mir":false,"dis":false,"name":"NB","params":{}},{"id":290,"type":"LABEL","x":100,"y":77,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GBN"}},{"id":291,"type":"COMP","x":88,"y":82,"rot":0,"mir":false,"dis":false,"name":"PWMC","params":{}},{"id":295,"type":"LABEL","x":84,"y":83,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TRI"}},{"id":298,"type":"LABEL","x":94,"y":82,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GC"}},{"id":299,"type":"NOT","x":96,"y":85,"rot":0,"mir":false,"dis":false,"name":"NC","params":{}},{"id":303,"type":"LABEL","x":100,"y":85,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GCN"}},{"id":305,"type":"VP","x":82,"y":64,"rot":0,"mir":false,"dis":false,"name":"ma","params":{}},{"id":306,"type":"TEXT","x":0,"y":86,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【量測：P、Q（dq 計算）】 P = 1.5(vd·id + vq·iq)，Q = 1.5(vd·iq − vq·id)；Pa/Pb/Pc 為各相瓦特計（10 Hz 低通）"}},{"id":307,"type":"MATH5","x":16,"y":92,"rot":0,"mir":false,"dis":false,"name":"calcP","params":{"expr":"1.5*(x1*x3+x2*x4)"}},{"id":308,"type":"LABEL","x":8,"y":87,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VD"}},{"id":312,"type":"LABEL","x":10,"y":89,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VQ"}},{"id":316,"type":"LABEL","x":10,"y":92,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"ID"}},{"id":318,"type":"LABEL","x":10,"y":95,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IQ"}},{"id":322,"type":"LABEL","x":8,"y":97,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TH"}},{"id":327,"type":"LABEL","x":20,"y":92,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"Pcalc"}},{"id":328,"type":"VP","x":19,"y":90,"rot":0,"mir":false,"dis":false,"name":"P","params":{}},{"id":329,"type":"MATH5","x":16,"y":106,"rot":0,"mir":false,"dis":false,"name":"calcQ","params":{"expr":"1.5*(x1*x4-x2*x3)"}},{"id":330,"type":"LABEL","x":8,"y":101,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VD"}},{"id":334,"type":"LABEL","x":10,"y":103,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VQ"}},{"id":338,"type":"LABEL","x":10,"y":106,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"ID"}},{"id":340,"type":"LABEL","x":10,"y":109,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IQ"}},{"id":344,"type":"LABEL","x":8,"y":111,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"TH"}},{"id":349,"type":"LABEL","x":20,"y":106,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"Qcalc"}},{"id":350,"type":"VP","x":19,"y":104,"rot":0,"mir":false,"dis":false,"name":"Q","params":{}},{"id":351,"type":"SIMCTRL","x":88,"y":2,"rot":0,"mir":false,"dis":false,"name":"","params":{"dt":"1u","tt":"0.2","tp":"0","ps":"4"}}],"wires":[{"id":4,"x1":4,"y1":12,"x2":4,"y2":4},{"id":5,"x1":4,"y1":4,"x2":40,"y2":4},{"id":6,"x1":4,"y1":16,"x2":4,"y2":26},{"id":7,"x1":4,"y1":26,"x2":40,"y2":26},{"id":10,"x1":8,"y1":6,"x2":8,"y2":4},{"id":11,"x1":8,"y1":12,"x2":8,"y2":16},{"id":13,"x1":8,"y1":22,"x2":8,"y2":26},{"id":16,"x1":20,"y1":7,"x2":20,"y2":4},{"id":17,"x1":20,"y1":11,"x2":20,"y2":17},{"id":18,"x1":20,"y1":21,"x2":20,"y2":26},{"id":20,"x1":16,"y1":9,"x2":18,"y2":9},{"id":22,"x1":16,"y1":19,"x2":18,"y2":19},{"id":25,"x1":30,"y1":7,"x2":30,"y2":4},{"id":26,"x1":30,"y1":11,"x2":30,"y2":17},{"id":27,"x1":30,"y1":21,"x2":30,"y2":26},{"id":29,"x1":26,"y1":9,"x2":28,"y2":9},{"id":31,"x1":26,"y1":19,"x2":28,"y2":19},{"id":34,"x1":40,"y1":7,"x2":40,"y2":4},{"id":35,"x1":40,"y1":11,"x2":40,"y2":17},{"id":36,"x1":40,"y1":21,"x2":40,"y2":26},{"id":38,"x1":36,"y1":9,"x2":38,"y2":9},{"id":40,"x1":36,"y1":19,"x2":38,"y2":19},{"id":41,"x1":20,"y1":12,"x2":44,"y2":12},{"id":42,"x1":44,"y1":12,"x2":44,"y2":8},{"id":43,"x1":44,"y1":8,"x2":48,"y2":8},{"id":45,"x1":50,"y1":6,"x2":50,"y2":4},{"id":47,"x1":30,"y1":14,"x2":48,"y2":14},{"id":49,"x1":50,"y1":12,"x2":50,"y2":10},{"id":51,"x1":40,"y1":16,"x2":44,"y2":16},{"id":52,"x1":44,"y1":16,"x2":44,"y2":20},{"id":53,"x1":44,"y1":20,"x2":48,"y2":20},{"id":55,"x1":50,"y1":18,"x2":50,"y2":16},{"id":58,"x1":52,"y1":8,"x2":55,"y2":8},{"id":59,"x1":55,"y1":8,"x2":55,"y2":13},{"id":60,"x1":55,"y1":13,"x2":56,"y2":13},{"id":61,"x1":52,"y1":14,"x2":56,"y2":14},{"id":62,"x1":52,"y1":20,"x2":55,"y2":20},{"id":63,"x1":55,"y1":20,"x2":55,"y2":15},{"id":64,"x1":55,"y1":15,"x2":56,"y2":15},{"id":65,"x1":60,"y1":13,"x2":61,"y2":13},{"id":66,"x1":61,"y1":13,"x2":61,"y2":8},{"id":67,"x1":61,"y1":8,"x2":62,"y2":8},{"id":68,"x1":62,"y1":8,"x2":63,"y2":8},{"id":69,"x1":63,"y1":8,"x2":63,"y2":5},{"id":72,"x1":63,"y1":8,"x2":65,"y2":8},{"id":73,"x1":67,"y1":10,"x2":67,"y2":11},{"id":75,"x1":69,"y1":8,"x2":72,"y2":8},{"id":76,"x1":60,"y1":14,"x2":62,"y2":14},{"id":77,"x1":62,"y1":14,"x2":63,"y2":14},{"id":78,"x1":63,"y1":14,"x2":63,"y2":11},{"id":81,"x1":63,"y1":14,"x2":65,"y2":14},{"id":82,"x1":67,"y1":16,"x2":67,"y2":17},{"id":84,"x1":69,"y1":14,"x2":72,"y2":14},{"id":85,"x1":60,"y1":15,"x2":61,"y2":15},{"id":86,"x1":61,"y1":15,"x2":61,"y2":20},{"id":87,"x1":61,"y1":20,"x2":62,"y2":20},{"id":88,"x1":62,"y1":20,"x2":63,"y2":20},{"id":89,"x1":63,"y1":20,"x2":63,"y2":17},{"id":92,"x1":63,"y1":20,"x2":65,"y2":20},{"id":93,"x1":67,"y1":22,"x2":67,"y2":23},{"id":95,"x1":69,"y1":20,"x2":72,"y2":20},{"id":97,"x1":72,"y1":8,"x2":73,"y2":8},{"id":98,"x1":73,"y1":8,"x2":73,"y2":13},{"id":99,"x1":73,"y1":13,"x2":74,"y2":13},{"id":100,"x1":72,"y1":14,"x2":74,"y2":14},{"id":101,"x1":72,"y1":20,"x2":73,"y2":20},{"id":102,"x1":73,"y1":20,"x2":73,"y2":15},{"id":103,"x1":73,"y1":15,"x2":74,"y2":15},{"id":104,"x1":78,"y1":14,"x2":80,"y2":14},{"id":109,"x1":2,"y1":33,"x2":4,"y2":33},{"id":111,"x1":2,"y1":35,"x2":4,"y2":35},{"id":114,"x1":2,"y1":39,"x2":4,"y2":39},{"id":116,"x1":2,"y1":41,"x2":4,"y2":41},{"id":119,"x1":2,"y1":45,"x2":4,"y2":45},{"id":121,"x1":2,"y1":47,"x2":4,"y2":47},{"id":123,"x1":8,"y1":34,"x2":14,"y2":34},{"id":124,"x1":14,"y1":34,"x2":14,"y2":38},{"id":125,"x1":14,"y1":38,"x2":17,"y2":38},{"id":126,"x1":8,"y1":40,"x2":12,"y2":40},{"id":127,"x1":12,"y1":40,"x2":12,"y2":39},{"id":128,"x1":12,"y1":39,"x2":17,"y2":39},{"id":129,"x1":8,"y1":46,"x2":16,"y2":46},{"id":130,"x1":16,"y1":46,"x2":16,"y2":40},{"id":131,"x1":16,"y1":40,"x2":17,"y2":40},{"id":133,"x1":17,"y1":44,"x2":17,"y2":41},{"id":134,"x1":23,"y1":39,"x2":24,"y2":39},{"id":135,"x1":24,"y1":39,"x2":24,"y2":36},{"id":138,"x1":23,"y1":40,"x2":26,"y2":40},{"id":139,"x1":25,"y1":40,"x2":25,"y2":43},{"id":142,"x1":30,"y1":40,"x2":32,"y2":40},{"id":144,"x1":34,"y1":44,"x2":34,"y2":42},{"id":146,"x1":36,"y1":40,"x2":38,"y2":40},{"id":148,"x1":42,"y1":40,"x2":44,"y2":40},{"id":149,"x1":48,"y1":40,"x2":50,"y2":40},{"id":154,"x1":22,"y1":36,"x2":24,"y2":36},{"id":159,"x1":12,"y1":52,"x2":16,"y2":52},{"id":160,"x1":16,"y1":52,"x2":16,"y2":54},{"id":161,"x1":16,"y1":54,"x2":17,"y2":54},{"id":163,"x1":12,"y1":55,"x2":17,"y2":55},{"id":165,"x1":12,"y1":59,"x2":16,"y2":59},{"id":166,"x1":16,"y1":59,"x2":16,"y2":56},{"id":167,"x1":16,"y1":56,"x2":17,"y2":56},{"id":169,"x1":14,"y1":60,"x2":17,"y2":60},{"id":170,"x1":17,"y1":60,"x2":17,"y2":57},{"id":171,"x1":23,"y1":55,"x2":28,"y2":55},{"id":174,"x1":23,"y1":56,"x2":24,"y2":56},{"id":175,"x1":24,"y1":56,"x2":24,"y2":60},{"id":178,"x1":24,"y1":60,"x2":27,"y2":60},{"id":186,"x1":4,"y1":66,"x2":10,"y2":66},{"id":187,"x1":10,"y1":66,"x2":10,"y2":67},{"id":188,"x1":10,"y1":67,"x2":14,"y2":67},{"id":190,"x1":11,"y1":68,"x2":14,"y2":68},{"id":191,"x1":18,"y1":67,"x2":20,"y2":67},{"id":197,"x1":4,"y1":76,"x2":10,"y2":76},{"id":198,"x1":10,"y1":76,"x2":10,"y2":77},{"id":199,"x1":10,"y1":77,"x2":14,"y2":77},{"id":201,"x1":11,"y1":78,"x2":14,"y2":78},{"id":202,"x1":18,"y1":77,"x2":20,"y2":77},{"id":206,"x1":6,"y1":66,"x2":4,"y2":66},{"id":208,"x1":6,"y1":76,"x2":4,"y2":76},{"id":212,"x1":29,"y1":67,"x2":32,"y2":67},{"id":214,"x1":34,"y1":71,"x2":34,"y2":69},{"id":216,"x1":36,"y1":67,"x2":38,"y2":67},{"id":218,"x1":42,"y1":67,"x2":44,"y2":67},{"id":220,"x1":46,"y1":63,"x2":46,"y2":65},{"id":222,"x1":46,"y1":71,"x2":46,"y2":69},{"id":224,"x1":48,"y1":67,"x2":50,"y2":67},{"id":225,"x1":54,"y1":67,"x2":56,"y2":67},{"id":229,"x1":29,"y1":77,"x2":32,"y2":77},{"id":231,"x1":34,"y1":81,"x2":34,"y2":79},{"id":233,"x1":36,"y1":77,"x2":38,"y2":77},{"id":235,"x1":42,"y1":77,"x2":44,"y2":77},{"id":237,"x1":46,"y1":73,"x2":46,"y2":75},{"id":239,"x1":46,"y1":81,"x2":46,"y2":79},{"id":241,"x1":48,"y1":77,"x2":50,"y2":77},{"id":242,"x1":54,"y1":77,"x2":56,"y2":77},{"id":247,"x1":66,"y1":65,"x2":72,"y2":65},{"id":248,"x1":72,"y1":65,"x2":72,"y2":68},{"id":249,"x1":72,"y1":68,"x2":73,"y2":68},{"id":251,"x1":66,"y1":67,"x2":71,"y2":67},{"id":252,"x1":71,"y1":67,"x2":71,"y2":69},{"id":253,"x1":71,"y1":69,"x2":73,"y2":69},{"id":255,"x1":67,"y1":70,"x2":73,"y2":70},{"id":257,"x1":66,"y1":73,"x2":70,"y2":73},{"id":258,"x1":70,"y1":73,"x2":70,"y2":71},{"id":259,"x1":70,"y1":71,"x2":73,"y2":71},{"id":262,"x1":72,"y1":82,"x2":72,"y2":80},{"id":265,"x1":79,"y1":69,"x2":80,"y2":69},{"id":266,"x1":80,"y1":69,"x2":80,"y2":63},{"id":267,"x1":80,"y1":63,"x2":85,"y2":63},{"id":268,"x1":85,"y1":63,"x2":85,"y2":65},{"id":270,"x1":84,"y1":67,"x2":85,"y2":67},{"id":271,"x1":91,"y1":66,"x2":94,"y2":66},{"id":274,"x1":92,"y1":66,"x2":92,"y2":69},{"id":275,"x1":92,"y1":69,"x2":94,"y2":69},{"id":276,"x1":98,"y1":69,"x2":100,"y2":69},{"id":279,"x1":79,"y1":70,"x2":81,"y2":70},{"id":280,"x1":81,"y1":70,"x2":81,"y2":73},{"id":281,"x1":81,"y1":73,"x2":85,"y2":73},{"id":283,"x1":84,"y1":75,"x2":85,"y2":75},{"id":284,"x1":91,"y1":74,"x2":94,"y2":74},{"id":287,"x1":92,"y1":74,"x2":92,"y2":77},{"id":288,"x1":92,"y1":77,"x2":94,"y2":77},{"id":289,"x1":98,"y1":77,"x2":100,"y2":77},{"id":292,"x1":79,"y1":71,"x2":80,"y2":71},{"id":293,"x1":80,"y1":71,"x2":80,"y2":81},{"id":294,"x1":80,"y1":81,"x2":85,"y2":81},{"id":296,"x1":84,"y1":83,"x2":85,"y2":83},{"id":297,"x1":91,"y1":82,"x2":94,"y2":82},{"id":300,"x1":92,"y1":82,"x2":92,"y2":85},{"id":301,"x1":92,"y1":85,"x2":94,"y2":85},{"id":302,"x1":98,"y1":85,"x2":100,"y2":85},{"id":304,"x1":80,"y1":66,"x2":82,"y2":66},{"id":309,"x1":8,"y1":87,"x2":12,"y2":87},{"id":310,"x1":12,"y1":87,"x2":12,"y2":90},{"id":311,"x1":12,"y1":90,"x2":14,"y2":90},{"id":313,"x1":10,"y1":89,"x2":13,"y2":89},{"id":314,"x1":13,"y1":89,"x2":13,"y2":91},{"id":315,"x1":13,"y1":91,"x2":14,"y2":91},{"id":317,"x1":10,"y1":92,"x2":14,"y2":92},{"id":319,"x1":10,"y1":95,"x2":13,"y2":95},{"id":320,"x1":13,"y1":95,"x2":13,"y2":93},{"id":321,"x1":13,"y1":93,"x2":14,"y2":93},{"id":323,"x1":8,"y1":97,"x2":12,"y2":97},{"id":324,"x1":12,"y1":97,"x2":12,"y2":94},{"id":325,"x1":12,"y1":94,"x2":14,"y2":94},{"id":326,"x1":18,"y1":92,"x2":20,"y2":92},{"id":331,"x1":8,"y1":101,"x2":12,"y2":101},{"id":332,"x1":12,"y1":101,"x2":12,"y2":104},{"id":333,"x1":12,"y1":104,"x2":14,"y2":104},{"id":335,"x1":10,"y1":103,"x2":13,"y2":103},{"id":336,"x1":13,"y1":103,"x2":13,"y2":105},{"id":337,"x1":13,"y1":105,"x2":14,"y2":105},{"id":339,"x1":10,"y1":106,"x2":14,"y2":106},{"id":341,"x1":10,"y1":109,"x2":13,"y2":109},{"id":342,"x1":13,"y1":109,"x2":13,"y2":107},{"id":343,"x1":13,"y1":107,"x2":14,"y2":107},{"id":345,"x1":8,"y1":111,"x2":12,"y2":111},{"id":346,"x1":12,"y1":111,"x2":12,"y2":108},{"id":347,"x1":12,"y1":108,"x2":14,"y2":108},{"id":348,"x1":18,"y1":106,"x2":20,"y2":106}],"nid":352} };
EXAMPLES["pfc"] = { name: "單相全橋 PFC（數位控制：取樣電壓/電流迴路）", screens: [["Vac","Vbus","Vref_"],["IL","Iref","ILf"],["Pin","Pout","Pcmd"],["Duty","Pwm"],["THD_i"],["Vrms","Irms_","I1"]], doc: {"elements":[{"id":1,"type":"TEXT","x":0,"y":-8,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【主回路 Power Stage】 圖騰柱無橋 PFC（全橋 4 開關）：快臂 QH/QL 40 kHz PWM，慢臂 QNH/QNL 隨市電極性切換；Vbus 400 V，2 kW"}},{"id":2,"type":"TEXT","x":0,"y":-6,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"市電 = 1 V 正弦 × 包絡線（VPWL）：230 Vrms → 0.20 s 驟降 150 Vrms → 0.24 s 恢復 → 0.28 s 突升 265 Vrms。介面標籤：GH/GL/GNH/GNL、VAC、VBUS、IL、N"}},{"id":3,"type":"VSIN","x":4,"y":10,"rot":0,"mir":false,"dis":false,"name":"Vunit","params":{"Vm":"1","f":"50.0","ph":"0"}},{"id":4,"type":"GND","x":4,"y":12,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":5,"type":"VPWL","x":4,"y":18,"rot":0,"mir":false,"dis":false,"name":"Venv","params":{"f":"0","times":"0 0.2 0.201 0.24 0.241 0.28 0.281 0.4","vals":"325.27 325.27 212.13 212.13 325.27 325.27 374.77 374.77"}},{"id":6,"type":"GND","x":4,"y":20,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":7,"type":"VNONM","x":12,"y":14,"rot":0,"mir":false,"dis":false,"name":"Vsrc","params":{"k":"1"}},{"id":15,"type":"WATT","x":18,"y":6,"rot":0,"mir":false,"dis":false,"name":"Pin","params":{"fc":"10"}},{"id":21,"type":"L","x":24,"y":6,"rot":0,"mir":false,"dis":false,"name":"Lb","params":{"L":"0.0006","i0":"0"}},{"id":23,"type":"ISEN","x":28,"y":6,"rot":0,"mir":false,"dis":false,"name":"ISL","params":{"k":"1"}},{"id":25,"type":"LABEL","x":28,"y":2,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IL"}},{"id":27,"type":"MOSFET","x":36,"y":2,"rot":0,"mir":false,"dis":false,"name":"QH","params":{}},{"id":28,"type":"MOSFET","x":36,"y":12,"rot":0,"mir":false,"dis":false,"name":"QL","params":{}},{"id":33,"type":"LABEL","x":32,"y":2,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GH"}},{"id":35,"type":"LABEL","x":32,"y":12,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GL"}},{"id":37,"type":"MOSFET","x":48,"y":10,"rot":0,"mir":false,"dis":false,"name":"QNH","params":{}},{"id":38,"type":"MOSFET","x":48,"y":18,"rot":0,"mir":false,"dis":false,"name":"QNL","params":{}},{"id":42,"type":"LABEL","x":44,"y":10,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GNH"}},{"id":44,"type":"LABEL","x":44,"y":18,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"GNL"}},{"id":50,"type":"LABEL","x":14,"y":20,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"N"}},{"id":53,"type":"GND","x":40,"y":22,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":54,"type":"RC","x":54,"y":10,"rot":1,"mir":false,"dis":false,"name":"Cbus","params":{"R":"0.05","C":"0.001","v0":"325"}},{"id":57,"type":"WATT","x":60,"y":4,"rot":1,"mir":false,"dis":false,"name":"Pout","params":{"fc":"10"}},{"id":59,"type":"R","x":60,"y":14,"rot":1,"mir":false,"dis":false,"name":"Rload","params":{"R":"80.0"}},{"id":65,"type":"VSEN","x":68,"y":10,"rot":0,"mir":false,"dis":false,"name":"VSbus","params":{"k":"1"}},{"id":74,"type":"LABEL","x":72,"y":10,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VBUS"}},{"id":75,"type":"VSEN","x":20,"y":14,"rot":0,"mir":false,"dis":false,"name":"VSac","params":{"k":"1"}},{"id":81,"type":"LABEL","x":24,"y":14,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VAC"}},{"id":82,"type":"TEXT","x":0,"y":30,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 1：取樣（ADC，fs = 40 kHz = 開關頻率）與市電極性】 vac、vbus、iL 經 ZOH 取樣；pol = (vac_s > 0)；|vac|、|iL| 給整流型迴路"}},{"id":83,"type":"ZOH","x":12,"y":34,"rot":0,"mir":false,"dis":false,"name":"ZOH1","params":{"fs":"40000.0"}},{"id":84,"type":"LABEL","x":4,"y":34,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VAC"}},{"id":87,"type":"LABEL","x":16,"y":34,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VACs"}},{"id":88,"type":"ZOH","x":12,"y":38,"rot":0,"mir":false,"dis":false,"name":"ZOH2","params":{"fs":"40000.0"}},{"id":89,"type":"LABEL","x":4,"y":38,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VBUS"}},{"id":92,"type":"LABEL","x":16,"y":38,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VBUSs"}},{"id":93,"type":"ZOH","x":12,"y":42,"rot":0,"mir":false,"dis":false,"name":"ZOH3","params":{"fs":"40000.0"}},{"id":94,"type":"LABEL","x":4,"y":42,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IL"}},{"id":97,"type":"LABEL","x":16,"y":42,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"ILs"}},{"id":98,"type":"ABS","x":28,"y":34,"rot":0,"mir":false,"dis":false,"name":"absV","params":{}},{"id":99,"type":"LABEL","x":24,"y":34,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VACs"}},{"id":102,"type":"LABEL","x":32,"y":34,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VACA"}},{"id":103,"type":"ABS","x":28,"y":42,"rot":0,"mir":false,"dis":false,"name":"absI","params":{}},{"id":104,"type":"LABEL","x":24,"y":42,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"ILs"}},{"id":107,"type":"LABEL","x":32,"y":42,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"ILA"}},{"id":108,"type":"COMP","x":42,"y":38,"rot":0,"mir":false,"dis":false,"name":"pol","params":{}},{"id":109,"type":"LABEL","x":37,"y":37,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VACs"}},{"id":111,"type":"CONST","x":37,"y":41,"rot":0,"mir":false,"dis":false,"name":"zero","params":{"v":"0"}},{"id":114,"type":"LABEL","x":48,"y":38,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"POL"}},{"id":115,"type":"MATH1","x":58,"y":34,"rot":0,"mir":false,"dis":false,"name":"sq","params":{"expr":"x*x"}},{"id":116,"type":"LABEL","x":54,"y":34,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VACs"}},{"id":118,"type":"FIR","x":64,"y":34,"rot":0,"mir":false,"dis":false,"name":"MArms","params":{"b":"0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025 0.0025","fs":"40000.0"}},{"id":121,"type":"LABEL","x":68,"y":34,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"VRMS2"}},{"id":122,"type":"MATH1","x":58,"y":40,"rot":0,"mir":false,"dis":false,"name":"rms","params":{"expr":"sqrt(max(x,0))"}},{"id":123,"type":"LABEL","x":54,"y":40,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VRMS2"}},{"id":126,"type":"VP","x":62,"y":38,"rot":0,"mir":false,"dis":false,"name":"Vrms","params":{}},{"id":127,"type":"TEXT","x":50,"y":43,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"Vrms² = 半週期（10 ms，400 點）滑動平均( vac_s² )，供電流命令的前饋 1/Vrms²（無漣波、延遲 10 ms）"}},{"id":128,"type":"VP","x":7,"y":32,"rot":0,"mir":false,"dis":false,"name":"Vac","params":{}},{"id":129,"type":"VP","x":7,"y":36,"rot":0,"mir":false,"dis":false,"name":"Vbus","params":{}},{"id":130,"type":"VP","x":7,"y":40,"rot":0,"mir":false,"dis":false,"name":"IL","params":{}},{"id":131,"type":"TEXT","x":0,"y":48,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 2：電壓迴路（數位 PI + 反飽和，輸出 = 輸入功率命令 P*）】 vbus_s → 2 階 Butterworth 20 Hz → e = Vref − vbus_f → PI(z)：Kp = 30 W/V，Ki = 900 W/(V·s)，限幅 0–4 kW；Vref 軟啟動 0 → 325 V (40 ms) → 400 V (120 ms)"}},{"id":132,"type":"FILTD","x":8,"y":54,"rot":0,"mir":false,"dis":false,"name":"LPbus","params":{"b":"2.46193005e-06 4.92386009e-06 2.46193005e-06","a":"1 -1.99555712 0.995566972","fs":"40000.0"}},{"id":133,"type":"LABEL","x":4,"y":54,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VBUSs"}},{"id":135,"type":"SUB2","x":18,"y":52,"rot":0,"mir":false,"dis":false,"name":"ev","params":{}},{"id":140,"type":"VPWL","x":8,"y":62,"rot":0,"mir":false,"dis":false,"name":"Vref","params":{"f":"0","times":"0 0.04 0.12 1","vals":"0 325 400 400"}},{"id":141,"type":"GND","x":8,"y":64,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":146,"type":"VP","x":11,"y":57,"rot":0,"mir":false,"dis":false,"name":"Vref_","params":{}},{"id":148,"type":"LABEL","x":22,"y":52,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"EV"}},{"id":149,"type":"P","x":32,"y":52,"rot":0,"mir":false,"dis":false,"name":"KpV","params":{"k":"30"}},{"id":150,"type":"LABEL","x":28,"y":52,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"EV"}},{"id":152,"type":"MATH2","x":32,"y":57,"rot":0,"mir":false,"dis":false,"name":"KiV","params":{"expr":"900*x1+30*x2"}},{"id":153,"type":"LABEL","x":28,"y":57,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"EV"}},{"id":155,"type":"LABEL","x":26,"y":60,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"AWV"}},{"id":159,"type":"ID","x":38,"y":57,"rot":0,"mir":false,"dis":false,"name":"IV","params":{"alg":"1","y0":"0","fs":"40000.0"}},{"id":161,"type":"SUM2","x":44,"y":52,"rot":0,"mir":false,"dis":false,"name":"uV","params":{"k1":"1","k2":"1"}},{"id":165,"type":"LIM","x":56,"y":52,"rot":0,"mir":false,"dis":false,"name":"satV","params":{"lo":"0","hi":"4000"}},{"id":168,"type":"LABEL","x":60,"y":52,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"PCMD"}},{"id":169,"type":"SUB2","x":62,"y":58,"rot":0,"mir":false,"dis":false,"name":"awV","params":{}},{"id":175,"type":"UDELAY","x":68,"y":58,"rot":0,"mir":false,"dis":false,"name":"zV","params":{"fs":"40000.0"}},{"id":178,"type":"LABEL","x":72,"y":58,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"AWV"}},{"id":179,"type":"VP","x":59,"y":50,"rot":0,"mir":false,"dis":false,"name":"Pcmd","params":{}},{"id":180,"type":"TEXT","x":0,"y":66,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 3：電流命令與電流迴路（數位 PI + 反飽和 + 工作週期前饋）】 iref = P*·|vac|/max(Vrms², 141²)；d = sat( 1 − |vac|/vbus + PI(z)(iref − |iL|) , 0…0.98 )；Kp = 0.0377，Ki = 94.7"}},{"id":181,"type":"MATH3","x":10,"y":72,"rot":0,"mir":false,"dis":false,"name":"iref","params":{"expr":"x1*x2/max(x3,20000)"}},{"id":182,"type":"LABEL","x":2,"y":69,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"PCMD"}},{"id":186,"type":"LABEL","x":4,"y":72,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VACA"}},{"id":188,"type":"LABEL","x":2,"y":75,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VRMS2"}},{"id":192,"type":"SUB2","x":18,"y":72,"rot":0,"mir":false,"dis":false,"name":"ei","params":{}},{"id":194,"type":"VP","x":14,"y":70,"rot":0,"mir":false,"dis":false,"name":"Iref","params":{}},{"id":195,"type":"LABEL","x":18,"y":76,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"ILA"}},{"id":198,"type":"LABEL","x":22,"y":72,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"EI"}},{"id":199,"type":"MATH2","x":10,"y":80,"rot":0,"mir":false,"dis":false,"name":"dff","params":{"expr":"1-x1/max(x2,100)"}},{"id":200,"type":"LABEL","x":6,"y":80,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VACA"}},{"id":202,"type":"LABEL","x":4,"y":82,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"VBUSs"}},{"id":206,"type":"LABEL","x":14,"y":80,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"DFF"}},{"id":207,"type":"P","x":32,"y":72,"rot":0,"mir":false,"dis":false,"name":"KpI","params":{"k":"0.0376991118"}},{"id":208,"type":"LABEL","x":28,"y":72,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"EI"}},{"id":210,"type":"MATH2","x":32,"y":77,"rot":0,"mir":false,"dis":false,"name":"KiI","params":{"expr":"94.7482023*x1+2513.27412*x2"}},{"id":211,"type":"LABEL","x":28,"y":77,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"EI"}},{"id":213,"type":"LABEL","x":26,"y":80,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"AWI"}},{"id":217,"type":"ID","x":38,"y":77,"rot":0,"mir":false,"dis":false,"name":"II","params":{"alg":"1","y0":"0","fs":"40000.0"}},{"id":219,"type":"SUM2","x":44,"y":72,"rot":0,"mir":false,"dis":false,"name":"uI","params":{"k1":"1","k2":"1"}},{"id":223,"type":"SUM2","x":50,"y":72,"rot":0,"mir":false,"dis":false,"name":"rawI","params":{"k1":"1","k2":"1"}},{"id":225,"type":"LABEL","x":50,"y":76,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"DFF"}},{"id":227,"type":"LIM","x":56,"y":72,"rot":0,"mir":false,"dis":false,"name":"satI","params":{"lo":"0","hi":"0.98"}},{"id":230,"type":"LABEL","x":60,"y":72,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"DUTY"}},{"id":231,"type":"SUB2","x":62,"y":78,"rot":0,"mir":false,"dis":false,"name":"awI","params":{}},{"id":237,"type":"UDELAY","x":68,"y":78,"rot":0,"mir":false,"dis":false,"name":"zI","params":{"fs":"40000.0"}},{"id":240,"type":"LABEL","x":72,"y":78,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"AWI"}},{"id":241,"type":"VP","x":59,"y":70,"rot":0,"mir":false,"dis":false,"name":"Duty","params":{}},{"id":242,"type":"TEXT","x":0,"y":86,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【控制區 4：PWM 與閘極邏輯（含二極體模擬）】 PWM = (d > 三角波 40 kHz，取樣點在載波谷 = 脈波中點 → 取到平均電流)；升壓開關 = PWM，同步開關 = NOT PWM AND (iL·sign(pol) > 0.5 A，二極體模擬)；正半週升壓開關 = QL，負半週 = QH；慢臂 GNL = pol、GNH = NOT pol"}},{"id":243,"type":"VTRI","x":4,"y":94,"rot":0,"mir":false,"dis":false,"name":"tri","params":{"Vpp":"1","f":"40000.0","D":"0.5","off":"0","ph":"0"}},{"id":244,"type":"GND","x":4,"y":96,"rot":0,"mir":false,"dis":false,"name":"","params":{}},{"id":245,"type":"COMP","x":12,"y":91,"rot":0,"mir":false,"dis":false,"name":"PWM","params":{}},{"id":246,"type":"LABEL","x":6,"y":90,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"DUTY"}},{"id":250,"type":"LABEL","x":18,"y":91,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"PWM"}},{"id":251,"type":"VP","x":16,"y":89,"rot":0,"mir":false,"dis":false,"name":"Pwm","params":{}},{"id":252,"type":"NOT","x":28,"y":89,"rot":0,"mir":false,"dis":false,"name":"nPWM","params":{}},{"id":253,"type":"LABEL","x":24,"y":89,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"PWM"}},{"id":255,"type":"MATH2","x":12,"y":96,"rot":0,"mir":false,"dis":false,"name":"ilsg","params":{"expr":"x1*(2*x2-1)"}},{"id":256,"type":"LABEL","x":9,"y":96,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"ILs"}},{"id":258,"type":"LABEL","x":9,"y":99,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"POL"}},{"id":261,"type":"COMP","x":28,"y":94,"rot":0,"mir":false,"dis":false,"name":"synEn","params":{}},{"id":265,"type":"CONST","x":21,"y":98,"rot":0,"mir":false,"dis":false,"name":"ith","params":{"v":"0.5"}},{"id":268,"type":"AND","x":36,"y":92,"rot":0,"mir":false,"dis":false,"name":"sync","params":{}},{"id":276,"type":"LABEL","x":41,"y":92,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"SYNC"}},{"id":277,"type":"MUX2","x":50,"y":89,"rot":0,"mir":false,"dis":false,"name":"muxH","params":{}},{"id":278,"type":"LABEL","x":46,"y":88,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"PWM"}},{"id":280,"type":"LABEL","x":46,"y":89,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"SYNC"}},{"id":282,"type":"LABEL","x":50,"y":93,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"POL"}},{"id":285,"type":"LABEL","x":54,"y":89,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GH"}},{"id":286,"type":"MUX2","x":50,"y":97,"rot":0,"mir":false,"dis":false,"name":"muxL","params":{}},{"id":287,"type":"LABEL","x":46,"y":96,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"SYNC"}},{"id":289,"type":"LABEL","x":46,"y":97,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"PWM"}},{"id":291,"type":"LABEL","x":50,"y":101,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"POL"}},{"id":294,"type":"LABEL","x":54,"y":97,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GL"}},{"id":295,"type":"NOT","x":66,"y":89,"rot":0,"mir":false,"dis":false,"name":"nPOL","params":{}},{"id":296,"type":"LABEL","x":62,"y":89,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"POL"}},{"id":299,"type":"LABEL","x":70,"y":89,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GNH"}},{"id":300,"type":"LABEL","x":62,"y":94,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"POL"}},{"id":302,"type":"LABEL","x":70,"y":94,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"GNL"}},{"id":303,"type":"TEXT","x":0,"y":105,"rot":0,"mir":false,"dis":false,"name":"","params":{"text":"【量測】 THD：iL 經 2 kHz 二階低通（相當於 EMI 濾波後的輸入電流）再算 50 Hz 基頻的 THD；Irms（移動均方根）；Pin / Pout（10 Hz 低通瓦特計）；PF = Pin /(Vrms·Irms)"}},{"id":304,"type":"LP2","x":8,"y":109,"rot":0,"mir":false,"dis":false,"name":"LPi","params":{"k":"1","z":"0.7071","fc":"2000"}},{"id":305,"type":"LABEL","x":4,"y":109,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IL"}},{"id":307,"type":"THD","x":16,"y":109,"rot":0,"mir":false,"dis":false,"name":"THDi","params":{"f":"50.0","fb":"25"}},{"id":309,"type":"VP","x":13,"y":107,"rot":0,"mir":false,"dis":false,"name":"ILf","params":{}},{"id":311,"type":"VP","x":20,"y":106,"rot":0,"mir":false,"dis":false,"name":"THD_i","params":{}},{"id":315,"type":"VP","x":22,"y":110,"rot":0,"mir":false,"dis":false,"name":"I1","params":{}},{"id":316,"type":"RMS","x":30,"y":109,"rot":0,"mir":false,"dis":false,"name":"Irms","params":{"fb":"50.0"}},{"id":317,"type":"LABEL","x":26,"y":109,"rot":2,"mir":false,"dis":false,"name":"","params":{"text":"IL"}},{"id":320,"type":"VP","x":34,"y":107,"rot":0,"mir":false,"dis":false,"name":"Irms_","params":{}},{"id":321,"type":"SIMCTRL","x":76,"y":-4,"rot":0,"mir":false,"dis":false,"name":"","params":{"dt":"1u","tt":"0.34","tp":"0","ps":"4"}}],"wires":[{"id":8,"x1":4,"y1":8,"x2":4,"y2":6},{"id":9,"x1":4,"y1":6,"x2":8,"y2":6},{"id":10,"x1":8,"y1":6,"x2":8,"y2":13},{"id":11,"x1":8,"y1":13,"x2":10,"y2":13},{"id":12,"x1":4,"y1":16,"x2":8,"y2":16},{"id":13,"x1":8,"y1":16,"x2":8,"y2":15},{"id":14,"x1":8,"y1":15,"x2":10,"y2":15},{"id":16,"x1":12,"y1":12,"x2":12,"y2":6},{"id":17,"x1":12,"y1":6,"x2":16,"y2":6},{"id":18,"x1":18,"y1":8,"x2":18,"y2":9},{"id":19,"x1":18,"y1":9,"x2":26,"y2":9},{"id":20,"x1":26,"y1":9,"x2":26,"y2":20},{"id":22,"x1":20,"y1":6,"x2":22,"y2":6},{"id":24,"x1":28,"y1":4,"x2":28,"y2":2},{"id":26,"x1":30,"y1":6,"x2":36,"y2":6},{"id":29,"x1":36,"y1":0,"x2":36,"y2":-2},{"id":30,"x1":36,"y1":4,"x2":36,"y2":6},{"id":31,"x1":36,"y1":6,"x2":36,"y2":10},{"id":32,"x1":36,"y1":14,"x2":36,"y2":22},{"id":34,"x1":32,"y1":2,"x2":34,"y2":2},{"id":36,"x1":32,"y1":12,"x2":34,"y2":12},{"id":39,"x1":48,"y1":8,"x2":48,"y2":-2},{"id":40,"x1":48,"y1":12,"x2":48,"y2":16},{"id":41,"x1":48,"y1":20,"x2":48,"y2":22},{"id":43,"x1":44,"y1":10,"x2":46,"y2":10},{"id":45,"x1":44,"y1":18,"x2":46,"y2":18},{"id":46,"x1":12,"y1":16,"x2":12,"y2":20},{"id":47,"x1":12,"y1":20,"x2":47,"y2":20},{"id":48,"x1":47,"y1":20,"x2":47,"y2":14},{"id":49,"x1":47,"y1":14,"x2":48,"y2":14},{"id":51,"x1":36,"y1":-2,"x2":60,"y2":-2},{"id":52,"x1":36,"y1":22,"x2":60,"y2":22},{"id":55,"x1":54,"y1":7,"x2":54,"y2":-2},{"id":56,"x1":54,"y1":13,"x2":54,"y2":22},{"id":58,"x1":60,"y1":-2,"x2":60,"y2":2},{"id":60,"x1":60,"y1":6,"x2":60,"y2":12},{"id":61,"x1":60,"y1":16,"x2":60,"y2":22},{"id":62,"x1":58,"y1":4,"x2":62,"y2":4},{"id":63,"x1":62,"y1":4,"x2":62,"y2":22},{"id":64,"x1":62,"y1":22,"x2":60,"y2":22},{"id":66,"x1":66,"y1":9,"x2":64,"y2":9},{"id":67,"x1":64,"y1":9,"x2":64,"y2":-2},{"id":68,"x1":64,"y1":-2,"x2":60,"y2":-2},{"id":69,"x1":66,"y1":11,"x2":66,"y2":13},{"id":70,"x1":66,"y1":13,"x2":64,"y2":13},{"id":71,"x1":64,"y1":13,"x2":64,"y2":22},{"id":72,"x1":64,"y1":22,"x2":60,"y2":22},{"id":73,"x1":70,"y1":10,"x2":72,"y2":10},{"id":76,"x1":18,"y1":13,"x2":16,"y2":13},{"id":77,"x1":16,"y1":13,"x2":16,"y2":6},{"id":78,"x1":18,"y1":15,"x2":16,"y2":15},{"id":79,"x1":16,"y1":15,"x2":16,"y2":20},{"id":80,"x1":22,"y1":14,"x2":24,"y2":14},{"id":85,"x1":4,"y1":34,"x2":10,"y2":34},{"id":86,"x1":14,"y1":34,"x2":16,"y2":34},{"id":90,"x1":4,"y1":38,"x2":10,"y2":38},{"id":91,"x1":14,"y1":38,"x2":16,"y2":38},{"id":95,"x1":4,"y1":42,"x2":10,"y2":42},{"id":96,"x1":14,"y1":42,"x2":16,"y2":42},{"id":100,"x1":24,"y1":34,"x2":26,"y2":34},{"id":101,"x1":30,"y1":34,"x2":32,"y2":34},{"id":105,"x1":24,"y1":42,"x2":26,"y2":42},{"id":106,"x1":30,"y1":42,"x2":32,"y2":42},{"id":110,"x1":37,"y1":37,"x2":39,"y2":37},{"id":112,"x1":39,"y1":41,"x2":39,"y2":39},{"id":113,"x1":45,"y1":38,"x2":48,"y2":38},{"id":117,"x1":54,"y1":34,"x2":56,"y2":34},{"id":119,"x1":60,"y1":34,"x2":62,"y2":34},{"id":120,"x1":66,"y1":34,"x2":68,"y2":34},{"id":124,"x1":54,"y1":40,"x2":56,"y2":40},{"id":125,"x1":60,"y1":40,"x2":62,"y2":40},{"id":134,"x1":4,"y1":54,"x2":6,"y2":54},{"id":136,"x1":10,"y1":54,"x2":13,"y2":54},{"id":137,"x1":13,"y1":54,"x2":13,"y2":56},{"id":138,"x1":13,"y1":56,"x2":18,"y2":56},{"id":139,"x1":18,"y1":56,"x2":18,"y2":54},{"id":142,"x1":8,"y1":60,"x2":8,"y2":59},{"id":143,"x1":8,"y1":59,"x2":15,"y2":59},{"id":144,"x1":15,"y1":59,"x2":15,"y2":52},{"id":145,"x1":15,"y1":52,"x2":16,"y2":52},{"id":147,"x1":20,"y1":52,"x2":22,"y2":52},{"id":151,"x1":28,"y1":52,"x2":30,"y2":52},{"id":154,"x1":28,"y1":57,"x2":30,"y2":57},{"id":156,"x1":26,"y1":60,"x2":29,"y2":60},{"id":157,"x1":29,"y1":60,"x2":29,"y2":58},{"id":158,"x1":29,"y1":58,"x2":30,"y2":58},{"id":160,"x1":34,"y1":57,"x2":36,"y2":57},{"id":162,"x1":34,"y1":52,"x2":42,"y2":52},{"id":163,"x1":40,"y1":57,"x2":44,"y2":57},{"id":164,"x1":44,"y1":57,"x2":44,"y2":54},{"id":166,"x1":46,"y1":52,"x2":54,"y2":52},{"id":167,"x1":58,"y1":52,"x2":60,"y2":52},{"id":170,"x1":59,"y1":52,"x2":59,"y2":58},{"id":171,"x1":59,"y1":58,"x2":60,"y2":58},{"id":172,"x1":53,"y1":52,"x2":53,"y2":62},{"id":173,"x1":53,"y1":62,"x2":62,"y2":62},{"id":174,"x1":62,"y1":62,"x2":62,"y2":60},{"id":176,"x1":64,"y1":58,"x2":66,"y2":58},{"id":177,"x1":70,"y1":58,"x2":72,"y2":58},{"id":183,"x1":2,"y1":69,"x2":6,"y2":69},{"id":184,"x1":6,"y1":69,"x2":6,"y2":71},{"id":185,"x1":6,"y1":71,"x2":8,"y2":71},{"id":187,"x1":4,"y1":72,"x2":8,"y2":72},{"id":189,"x1":2,"y1":75,"x2":6,"y2":75},{"id":190,"x1":6,"y1":75,"x2":6,"y2":73},{"id":191,"x1":6,"y1":73,"x2":8,"y2":73},{"id":193,"x1":12,"y1":72,"x2":16,"y2":72},{"id":196,"x1":18,"y1":76,"x2":18,"y2":74},{"id":197,"x1":20,"y1":72,"x2":22,"y2":72},{"id":201,"x1":6,"y1":80,"x2":8,"y2":80},{"id":203,"x1":4,"y1":82,"x2":8,"y2":82},{"id":204,"x1":8,"y1":82,"x2":8,"y2":81},{"id":205,"x1":12,"y1":80,"x2":14,"y2":80},{"id":209,"x1":28,"y1":72,"x2":30,"y2":72},{"id":212,"x1":28,"y1":77,"x2":30,"y2":77},{"id":214,"x1":26,"y1":80,"x2":29,"y2":80},{"id":215,"x1":29,"y1":80,"x2":29,"y2":78},{"id":216,"x1":29,"y1":78,"x2":30,"y2":78},{"id":218,"x1":34,"y1":77,"x2":36,"y2":77},{"id":220,"x1":34,"y1":72,"x2":42,"y2":72},{"id":221,"x1":40,"y1":77,"x2":44,"y2":77},{"id":222,"x1":44,"y1":77,"x2":44,"y2":74},{"id":224,"x1":46,"y1":72,"x2":48,"y2":72},{"id":226,"x1":50,"y1":76,"x2":50,"y2":74},{"id":228,"x1":52,"y1":72,"x2":54,"y2":72},{"id":229,"x1":58,"y1":72,"x2":60,"y2":72},{"id":232,"x1":59,"y1":72,"x2":59,"y2":78},{"id":233,"x1":59,"y1":78,"x2":60,"y2":78},{"id":234,"x1":53,"y1":72,"x2":53,"y2":82},{"id":235,"x1":53,"y1":82,"x2":62,"y2":82},{"id":236,"x1":62,"y1":82,"x2":62,"y2":80},{"id":238,"x1":64,"y1":78,"x2":66,"y2":78},{"id":239,"x1":70,"y1":78,"x2":72,"y2":78},{"id":247,"x1":6,"y1":90,"x2":9,"y2":90},{"id":248,"x1":4,"y1":92,"x2":9,"y2":92},{"id":249,"x1":15,"y1":91,"x2":18,"y2":91},{"id":254,"x1":24,"y1":89,"x2":26,"y2":89},{"id":257,"x1":9,"y1":96,"x2":10,"y2":96},{"id":259,"x1":9,"y1":99,"x2":10,"y2":99},{"id":260,"x1":10,"y1":99,"x2":10,"y2":97},{"id":262,"x1":14,"y1":96,"x2":20,"y2":96},{"id":263,"x1":20,"y1":96,"x2":20,"y2":93},{"id":264,"x1":20,"y1":93,"x2":25,"y2":93},{"id":266,"x1":23,"y1":98,"x2":25,"y2":98},{"id":267,"x1":25,"y1":98,"x2":25,"y2":95},{"id":269,"x1":30,"y1":89,"x2":31,"y2":89},{"id":270,"x1":31,"y1":89,"x2":31,"y2":91},{"id":271,"x1":31,"y1":91,"x2":33,"y2":91},{"id":272,"x1":31,"y1":94,"x2":32,"y2":94},{"id":273,"x1":32,"y1":94,"x2":32,"y2":93},{"id":274,"x1":32,"y1":93,"x2":33,"y2":93},{"id":275,"x1":39,"y1":92,"x2":41,"y2":92},{"id":279,"x1":46,"y1":88,"x2":48,"y2":88},{"id":281,"x1":46,"y1":89,"x2":48,"y2":89},{"id":283,"x1":50,"y1":93,"x2":50,"y2":91},{"id":284,"x1":52,"y1":89,"x2":54,"y2":89},{"id":288,"x1":46,"y1":96,"x2":48,"y2":96},{"id":290,"x1":46,"y1":97,"x2":48,"y2":97},{"id":292,"x1":50,"y1":101,"x2":50,"y2":99},{"id":293,"x1":52,"y1":97,"x2":54,"y2":97},{"id":297,"x1":62,"y1":89,"x2":64,"y2":89},{"id":298,"x1":68,"y1":89,"x2":70,"y2":89},{"id":301,"x1":62,"y1":94,"x2":70,"y2":94},{"id":306,"x1":4,"y1":109,"x2":6,"y2":109},{"id":308,"x1":10,"y1":109,"x2":14,"y2":109},{"id":310,"x1":18,"y1":108,"x2":20,"y2":108},{"id":312,"x1":18,"y1":110,"x2":20,"y2":110},{"id":313,"x1":20,"y1":110,"x2":20,"y2":112},{"id":314,"x1":20,"y1":112,"x2":22,"y2":112},{"id":318,"x1":26,"y1":109,"x2":28,"y2":109},{"id":319,"x1":32,"y1":109,"x2":34,"y2":109}],"nid":322} };

/* =====================================================================
 * 電路模擬器 — touch support (phones / tablets)
 * Schematic: 1 finger on empty space = pan, on a part = select/drag,
 * on a pin = draw wire; 2 fingers = pinch zoom + pan; long press = context
 * menu; double tap = properties / finish wire. A floating action bar gives
 * the functions that otherwise need a keyboard (R, X, D, Del, Esc …).
 * Mouse and pen keep the desktop behaviour.
 * ===================================================================== */
'use strict';
const TOUCH = { pts: new Map(), pinch: null, one: null, lastTap: null, band: false, dblAt: 0 };
const isCoarse = () => { try { return matchMedia('(pointer: coarse)').matches; } catch (e) { return false; } };
const LP_MS = 550, MOVE_TOL = 9;

function fakeEv(src, extra) {
  return Object.assign({ clientX: src.clientX, clientY: src.clientY, button: 0, pointerId: src.pointerId, pointerType: 'touch', shiftKey: false, ctrlKey: false, metaKey: false, altKey: false, preventDefault() {}, stopPropagation() {} }, extra || {});
}
function cancelOneFinger() {
  const o = TOUCH.one; if (!o) return;
  clearTimeout(o.lp);
  if (ED.drag) { const d = ED.drag; ED.drag = null; if (d.moved) { ED.doc.elements = d.base.elements; ED.doc.wires = d.base.wires; ED.conn = null; } }
  if (ED.band) ED.band = null;
  if (ED.wire && ED.wireFromSelect && ED.wire.pts && ED.wire.pts.length <= 1) { ED.wire = null; ED.wireFromSelect = false; setMode('select'); }
  ED.pan = null; svg.classList.remove('panning');
  TOUCH.one = null; render();
}
function tDown(ev) {
  ev.preventDefault();
  TOUCH.pts.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  try { svg.setPointerCapture(ev.pointerId); } catch (e) { /* ignore */ }
  closeCtx(); closeMenus();
  if (TOUCH.pts.size >= 2) {
    cancelOneFinger();
    const [a, b] = [...TOUCH.pts.values()];
    const cx = (a.x + b.x) / 2, cy = (a.y + b.y) / 2;
    TOUCH.pinch = { d0: Math.hypot(a.x - b.x, a.y - b.y) || 1, c: toWorld({ clientX: cx, clientY: cy }), vb: Object.assign({}, ED.vb) };
    return;
  }
  if (TOUCH.pinch) return;
  const o = TOUCH.one = { id: ev.pointerId, x: ev.clientX, y: ev.clientY, t: performance.now(), moved: false, kind: null, lpFired: false };
  const [wx, wy] = toWorld(ev);
  // finger-sized hit tolerance for pins and wires (about 16 px, never less than the mouse tolerance)
  { const r = svg.getBoundingClientRect(); ED.pinTol = Math.min(1.2, Math.max(0.35, 16 * ED.vb.w / Math.max(1, r.width))); }
  o.lp = setTimeout(() => {
    if (!TOUCH.one || TOUCH.one !== o || o.moved) return;
    o.lpFired = true; const at = { clientX: o.x, clientY: o.y, w: [wx, wy] };
    // undo whatever the press started, then open the context menu
    if (ED.drag && !ED.drag.moved) ED.drag = null;
    if (ED.band) { ED.band = null; }
    if (ED.wire && ED.wireFromSelect) { ED.wire = null; ED.wireFromSelect = false; setMode('select'); }
    ED.pan = null; renderTop();
    if (navigator.vibrate) try { navigator.vibrate(15); } catch (e) { /* ignore */ }
    ctxAt(at);
  }, LP_MS);
  if (ED.mode === 'place' || ED.mode === 'paste') { o.kind = 'place'; ED.mouse = [snapv(wx), snapv(wy)]; ED.ghostOn = true; renderTop(); return; }
  if (ED.mode === 'wire') { o.kind = 'std'; onDown(fakeEv(ev)); return; }
  const onSomething = nearPin(wx, wy) || hitElement(wx, wy) || hitWire(wx, wy);
  if (onSomething || TOUCH.band) { o.kind = 'std'; onDown(fakeEv(ev)); return; }
  o.kind = 'pan'; ED.pan = { x: ev.clientX, y: ev.clientY, vb: Object.assign({}, ED.vb), right: false, moved: false };
}
function tMove(ev) {
  if (!TOUCH.pts.has(ev.pointerId)) return;
  ev.preventDefault();
  TOUCH.pts.set(ev.pointerId, { x: ev.clientX, y: ev.clientY });
  if (TOUCH.pinch) {
    if (TOUCH.pts.size < 2) return;
    const [a, b] = [...TOUCH.pts.values()]; const P = TOUCH.pinch;
    const d = Math.hypot(a.x - b.x, a.y - b.y) || 1, cx = (a.x + b.x) / 2, cy = (a.y + b.y) / 2;
    const r = svg.getBoundingClientRect();
    const w = Math.min(600, Math.max(6, P.vb.w * P.d0 / d)), hh = w * r.height / Math.max(1, r.width);
    ED.vb.w = w; ED.vb.x = P.c[0] - (cx - r.left) / r.width * w; ED.vb.y = P.c[1] - (cy - r.top) / r.height * hh;
    applyView(); return;
  }
  const o = TOUCH.one; if (!o || o.id !== ev.pointerId || o.lpFired) return;
  if (!o.moved && Math.hypot(ev.clientX - o.x, ev.clientY - o.y) > MOVE_TOL) { o.moved = true; clearTimeout(o.lp); }
  if (o.kind === 'place') {
    // dragging in place mode pans; the ghost follows the finger once it stops
    if (o.moved) { if (!ED.pan) ED.pan = { x: o.x, y: o.y, vb: Object.assign({}, ED.vb), right: false, moved: true }; onMove(fakeEv(ev)); }
    return;
  }
  onMove(fakeEv(ev));
}
function tUp(ev) {
  if (!TOUCH.pts.has(ev.pointerId)) return;
  TOUCH.pts.delete(ev.pointerId);
  if (TOUCH.pinch) { if (TOUCH.pts.size === 0) { TOUCH.pinch = null; render(); } return; }
  const o = TOUCH.one; if (!o || o.id !== ev.pointerId) return;
  clearTimeout(o.lp); TOUCH.one = null;
  if (o.lpFired) { ED.pan = null; return; }
  if (o.kind === 'place') {
    if (ED.pan) { ED.pan = null; svg.classList.remove('panning'); render(); return; }
    const [wx, wy] = toWorld(ev); ED.mouse = [snapv(wx), snapv(wy)];
    onDown(fakeEv(ev)); onUp(fakeEv(ev));
    return;
  }
  const tap = !o.moved && performance.now() - o.t < 400;
  onUp(fakeEv(ev));
  if (o.kind === 'pan' && tap) { if (ED.sel.size) { ED.sel.clear(); render(); } }
  // double tap
  if (tap) {
    const now = performance.now(), lt = TOUCH.lastTap;
    if (lt && now - lt.t < 320 && Math.hypot(lt.x - ev.clientX, lt.y - ev.clientY) < 24) { TOUCH.lastTap = null; TOUCH.dblAt = now; onDbl(fakeEv(ev)); }
    else TOUCH.lastTap = { t: now, x: ev.clientX, y: ev.clientY };
  }
}
function tCancel(ev) { TOUCH.pts.delete(ev.pointerId); if (TOUCH.pinch && !TOUCH.pts.size) { TOUCH.pinch = null; render(); } if (TOUCH.one && TOUCH.one.id === ev.pointerId) cancelOneFinger(); }

// pointer routing: touch goes through the touch layer, mouse/pen to the desktop handlers
function closeLibDrawer() { const m = $('#main'); if (innerWidth <= 1100 && m.classList.contains('libOpen')) { m.classList.remove('libOpen'); setTimeout(render, 0); return true; } return false; }
function bindSchematicPointers() {
  svg.addEventListener('pointerdown', ev => { if (closeLibDrawer()) return; if (ev.pointerType === 'touch') tDown(ev); else { ED.pinTol = 0; onDown(ev); } });
  svg.addEventListener('pointermove', ev => { if (ev.pointerType === 'touch') tMove(ev); else onMove(ev); });
  svg.addEventListener('pointerup', ev => { if (ev.pointerType === 'touch') tUp(ev); else onUp(ev); });
  svg.addEventListener('pointercancel', ev => { if (ev.pointerType === 'touch') tCancel(ev); else if (ED.pan) { ED.pan = null; render(); } });
  svg.addEventListener('dblclick', ev => { if (performance.now() - TOUCH.dblAt < 600) return; onDbl(ev); });
  svg.addEventListener('contextmenu', ev => { if (TOUCH.pts.size || performance.now() - TOUCH.dblAt < 600 || (ev.pointerType === 'touch')) { ev.preventDefault(); return; } onCtx(ev); });
}

/* ---------- touch action bar ---------- */
function cancelAction() {
  if (ED.wire) { finishWire(); return; }
  if (ED.mode !== 'select') { setMode('select'); return; }
  ED.sel.clear(); render();
}
function propsSel() {
  const els = ED.doc.elements.filter(e => ED.sel.has('e:' + e.id));
  if (els.length === 1) openProps(els[0]); else toast(els.length ? '請只選取一個元件' : '請先點選一個元件');
}
function buildTouchBar() {
  const bar = h('div', { id: 'tbar', role: 'toolbar', 'aria-label': '觸控操作' });
  const B = (k, label, fn, id) => { const b = h('button', { class: 'tb', id: id || null, title: label, 'aria-label': label, html: ico(k) + '<span>' + label + '</span>', onclick: e => { e.stopPropagation(); fn(b); } }); bar.append(b); return b; };
  B('run', '執行', () => runSim(false)).classList.add('tbRun');
  B('wave', '波形', openLastResult);
  B('band', '框選', b => { TOUCH.band = !TOUCH.band; b.classList.toggle('on', TOUCH.band); toast(TOUCH.band ? '框選模式：在空白處拖曳以框選多個元件' : '平移模式：在空白處拖曳以移動畫面'); }, 'tbBand');
  B('wire', '導線', () => setMode(ED.mode === 'wire' ? 'select' : 'wire')).dataset.mode = 'wire';
  B('rot', '旋轉', () => rotateSel(1));
  B('mirx', '鏡像', () => mirrorSel(false));
  B('dis', '停用', toggleDisable);
  B('props', '屬性', propsSel);
  B('copy', '複製', () => copySel(false));
  B('paste', '貼上', () => { pasteClip(); touchPlaceHint(); });
  B('del', '刪除', deleteSel);
  B('undo', '復原', undo);
  B('find', '搜尋', openFind);
  B('esc', '完成', cancelAction);
  $('#stage').append(bar);
}
// on touch there is no hover, so show the ghost at the centre of the view and tell the user what to do
function touchPlaceHint() {
  if (!isCoarse() && innerWidth > 1100) return;
  closeLibDrawer();
  if (ED.mode === 'place' || ED.mode === 'paste') {
    ED.mouse = [snapv(ED.vb.x + ED.vb.w / 2), snapv(ED.vb.y + ED.vb.h / 2)]; ED.ghostOn = true; renderTop();
    toast('點一下電路圖放置；可先按「旋轉」「鏡像」，按「完成」取消');
  }
}
function initTouch() {
  Object.assign(ICONS, {
    band: '<rect x="4" y="5" width="16" height="13" rx="1" stroke-dasharray="3 2"/>',
    props: '<path d="M4 6h10M4 12h16M4 18h8"/><circle cx="17" cy="6" r="2"/><circle cx="15" cy="18" r="2"/>',
    copy: '<rect x="8" y="8" width="12" height="12" rx="1"/><path d="M16 8V4H4v12h4"/>',
    paste: '<path d="M8 4h8v3H8z"/><path d="M6 6H5v15h14V6h-1"/>',
    del: '<path d="M5 7h14M9 7V4h6v3M7 7l1 13h8l1-13"/>',
    esc: '<path d="M5 12l5 5 9-10"/>'
  });
  buildTouchBar();
  document.documentElement.classList.toggle('coarse', isCoarse());
  // when the on-screen keyboard closes, mobile browsers may leave the page scrolled (toolbars pushed off-screen)
  if (window.visualViewport) visualViewport.addEventListener('resize', () => { const a = document.activeElement; if (!a || !/^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName)) { if (window.scrollY || window.scrollX) window.scrollTo(0, 0); } });
  window.addEventListener('scroll', () => { if (!$('.modal') && (window.scrollY || window.scrollX)) { const a = document.activeElement; if (!a || !/^(INPUT|TEXTAREA|SELECT)$/.test(a.tagName)) window.scrollTo(0, 0); } }, { passive: true });
  try { matchMedia('(pointer: coarse)').addEventListener('change', () => document.documentElement.classList.toggle('coarse', isCoarse())); } catch (e) { /* old Safari */ }
}

