// User manual generator: writes crates/daedalus-app/assets/manual.html, which is built into the program
// (說明 › 使用說明書, F1). Every element of the library gets its symbol, description, parameters and a figure
// simulated from a small circuit (tools/manual/figs.js); the guide chapters are in tools/manual/text.js.
//
// usage: node tools/manual/gen.js
//            figures simulated with the reference JS engine (reference/harness.js), for a local preview;
//            also writes tools/manual/figdata.json (the plotted data)
//        node tools/manual/gen.js --cli target/release/daedalus-cli.exe [--strict]
//            figures simulated with DaedalusEngine itself (CI, before the program is built, so the shipped
//            manual shows exactly what the program computes); every figure is compared with figdata.json and
//            an unexpected difference is reported (--strict, release builds: fails the run)
//        node tools/manual/gen.js ... --only VSIN,LP1   only these figures (preview; nothing else is written)
'use strict';
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const ROOT = path.resolve(__dirname, '..', '..');
const LIB = JSON.parse(fs.readFileSync(path.join(ROOT, 'crates/daedalus-core/assets/library.json'), 'utf8'));
const DEF = Object.fromEntries(LIB.map(d => [d.type, d]));
const VERSION = 'V' + /version = "([^"]+)"/.exec(fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8'))[1];
const { build } = require('../examples/gen.js');
const FIGS = require('./figs.js');
const TEXT = require('./text.js');
const OUT = path.join(ROOT, 'crates/daedalus-app/assets/manual.html');
const DATA = path.join(__dirname, 'figdata.json');
const TMP = path.join(ROOT, 'tests/manual');

const argv = process.argv.slice(2);
const cliIdx = argv.indexOf('--cli');
const CLI = cliIdx >= 0 ? path.resolve(argv[cliIdx + 1]) : null;
const onlyIdx = argv.indexOf('--only');
const ONLY = onlyIdx >= 0 ? new Set(argv[onlyIdx + 1].split(',')) : null;

/* ------------------------------------------------------------------ simulation */
function readCsv(f) {
  const L = fs.readFileSync(f, 'utf8').trim().split(/\r?\n/);
  const h = L[0].split(',');
  const c = {};
  h.forEach(k => (c[k] = []));
  for (let i = 1; i < L.length; i++) {
    const v = L[i].split(',');
    for (let k = 0; k < h.length; k++) c[h[k]].push(+v[k]);
  }
  return c;
}

const PROBES = new Set(['VP', 'VP2', 'IP', 'DEVP', 'ACP', 'ACP2', 'VDCM', 'VACM', 'ADCM', 'AACM', 'WATT']);
// one circuit → columns (Time or Freq + probes)
function runOne(key, fig, variant) {
  const ex = { key, name: key, text: fig.title || key, sc: Object.assign({}, fig.sc, variant && variant.sc), params: variant && variant.params !== undefined ? variant.params : fig.params, extra: fig.extra, parts: !CLI && fig.jsParts ? fig.jsParts : fig.parts };
  if (variant && variant.parts) ex.parts = variant.parts(fig.parts);
  // element names are unique regardless of case: a source or block named like a probe (Vs / vs) is renamed
  const pn = new Set(ex.parts.filter(p => PROBES.has(p[0])).map(p => p[1].toLowerCase()));
  ex.parts = ex.parts.map(p => (!PROBES.has(p[0]) && p[1] && pn.has(p[1].toLowerCase()) ? [p[0], 'X_' + p[1], p[2], p[3]] : p));
  const doc = build(ex);
  fs.mkdirSync(TMP, { recursive: true });
  const tag = key + (variant ? '_' + variant.id : '');
  const ps = path.join(TMP, tag + '.psimx');
  const csv = path.join(TMP, tag + '.csv');
  fs.writeFileSync(ps, JSON.stringify(doc));
  try {
    if (CLI) execFileSync(CLI, fig.ac ? [ps, csv, '--ac'] : [ps, csv], { stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 1 << 28 });
    else execFileSync(process.execPath, [path.join(ROOT, 'reference/harness.js'), 'run', ps, csv].concat(fig.ac ? ['ac'] : []), { stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 1 << 28 });
  } catch (e) {
    throw new Error(`${tag}: 模擬失敗 ${String(e.stderr || e.message).split('\n').slice(0, 3).join(' ')}`);
  }
  const cols = readCsv(csv);
  if (!CLI && fig.jsPost) fig.jsPost(cols);
  return cols;
}

// a figure's data: one run, or several runs (variants) merged as "probe [label]"
function simulate(key, fig) {
  // the parameter sweep and Monte Carlo elements run inside DaedalusEngine only: the JS engine repeats the
  // circuit for each value instead (same column names)
  const native = CLI && fig.nativeRuns;
  const variants = native ? null : fig.variants;
  if (!variants) return runOne(key, fig, native ? null : undefined);
  let out = null;
  for (const v of variants) {
    const c = runOne(key, fig, v);
    const X = fig.ac ? 'Freq' : 'Time';
    if (!out) out = { [X]: c[X], __T: {} };
    // runs with other time steps keep their own time axis
    for (const k of Object.keys(c)) if (k !== X) { out[`${k} [${v.label}]`] = c[k]; out.__T[`${k} [${v.label}]`] = c[X]; }
  }
  return out;
}

/* ------------------------------------------------------------------ numbers */
const PRE = [[1e9, 'G'], [1e6, 'M'], [1e3, 'k'], [1, ''], [1e-3, 'm'], [1e-6, 'µ'], [1e-9, 'n'], [1e-12, 'p'], [1e-15, 'f']];
function si(v, unit = '', dig = 4) {
  if (v === 0) return '0' + (unit ? ' ' + unit : '');
  if (!isFinite(v)) return String(v);
  const a = Math.abs(v);
  for (const [m, s] of PRE) if (a >= m * 0.9999999) return `${+(v / m).toPrecision(dig)} ${s}${unit}`.trim();
  return v.toExponential(2) + ' ' + unit;
}
function niceTicks(a, b, n) {
  if (!(b > a)) { const d = Math.abs(a) || 1; a -= d * 0.5; b += d * 0.5; }
  const raw = (b - a) / Math.max(n, 1);
  const p = Math.pow(10, Math.floor(Math.log10(raw)));
  const m = raw / p;
  const step = (m < 1.5 ? 1 : m < 3 ? 2 : m < 7 ? 5 : 10) * p;
  const out = [];
  for (let v = Math.ceil(a / step) * step, g = 0; v <= b + step * 1e-9 && g < 100; v += step, g++) out.push(Math.abs(v) < step * 1e-9 ? 0 : v);
  return { ticks: out, step };
}
function fmtTick(v, step, unit) {
  if (v === 0) return '0';
  const a = Math.max(Math.abs(v), Math.abs(step));
  for (const [m, s] of PRE) {
    if (a >= m * 0.999) {
      const dec = Math.min(6, Math.max(0, Math.ceil(-Math.log10(step / m) - 1e-9)));
      return (v / m).toFixed(dec) + s + unit;
    }
  }
  return v.toExponential(1);
}
const esc = s => String(s).replace(/&/g, '&amp;').replace(/</g, '&lt;').replace(/>/g, '&gt;').replace(/"/g, '&quot;');

/* ------------------------------------------------------------------ plotting */
// min / max per bucket over the time window (edges and spikes survive), at most ~2 points per pixel pair
function decimate(T, Y, t0, t1, buckets = 170) {
  const idx = [];
  for (let i = 0; i < T.length; i++) if (T[i] >= t0 - 1e-15 && T[i] <= t1 + 1e-15) idx.push(i);
  if (idx.length <= 2 * buckets) return idx.map(i => [T[i], Y[i]]);
  const out = [];
  const per = idx.length / buckets;
  for (let b = 0; b < buckets; b++) {
    const a = Math.floor(b * per), e = Math.min(idx.length, Math.floor((b + 1) * per));
    let mn = -1, mx = -1;
    for (let k = a; k < e; k++) {
      const v = Y[idx[k]];
      if (!isFinite(v)) continue;
      if (mn < 0 || v < Y[idx[mn]]) mn = k;
      if (mx < 0 || v > Y[idx[mx]]) mx = k;
    }
    if (mn < 0) continue;
    const [p, q] = mn <= mx ? [mn, mx] : [mx, mn];
    out.push([T[idx[p]], Y[idx[p]]]);
    if (q !== p) out.push([T[idx[q]], Y[idx[q]]]);
  }
  return out;
}

const W = 600, ML = 62, MR = 14, MT = 8, GAP = 12, XLAB = 22;
// half-pixel coordinates keep the page small
const r1 = v => Math.round(v * 2) / 2;

function axisY(lo, hi, h) {
  const pad = (hi - lo) * 0.08 || Math.abs(hi) * 0.1 || 1;
  return [lo - pad, hi + pad];
}

// stacked panels sharing the X axis: series = [{ x: [...], y: [...], name, cls }] per panel
function svgPanels(panels, xr, opts) {
  const heights = panels.map(p => p.h || opts.h || 120);
  // a legend row above every panel that names its curves
  const LEG = 17;
  const legs = panels.map(p => (p.series.some(c => c.name) || p.label ? LEG : 0));
  const XL = opts.xlabel ? 15 : 0;
  const H = MT + heights.reduce((a, b) => a + b, 0) + legs.reduce((a, b) => a + b, 0) + GAP * (panels.length - 1) + XLAB + XL;
  const pw = W - ML - MR;
  let s = `<svg class="plot" viewBox="0 0 ${W} ${H}" role="img" aria-label="${esc(opts.label || '')}">`;
  const xs = opts.xlog ? x => ML + (Math.log10(x) - Math.log10(xr[0])) / (Math.log10(xr[1]) - Math.log10(xr[0])) * pw : x => ML + (x - xr[0]) / (xr[1] - xr[0]) * pw;
  let y0 = MT;
  const xt = opts.xlog ? logTicks(xr[0], xr[1]) : niceTicks(xr[0], xr[1], 6);
  panels.forEach((p, k) => {
    const h = heights[k];
    const ly = y0 + 12;
    y0 += legs[k];
    let lo = Infinity, hi = -Infinity;
    for (const c of p.series) for (const v of c.y) if (isFinite(v)) { lo = Math.min(lo, v); hi = Math.max(hi, v); }
    if (!isFinite(lo)) { lo = -1; hi = 1; }
    if (p.yr) [lo, hi] = p.yr;
    else if (hi - lo < 1e-12 * Math.max(Math.abs(hi), 1e-30)) { const d = Math.abs(hi) * 0.1 || 1; lo -= d; hi += d; }
    else [lo, hi] = axisY(lo, hi);
    const ys = v => y0 + h - (v - lo) / (hi - lo) * h;
    s += `<rect class="pa" x="${ML}" y="${y0}" width="${pw}" height="${h}"/>`;
    const yt = niceTicks(lo, hi, Math.max(2, Math.round(h / 34)));
    for (const v of yt.ticks) {
      const y = r1(ys(v));
      s += `<line class="gr" x1="${ML}" x2="${ML + pw}" y1="${y}" y2="${y}"/><text class="tk" x="${ML - 5}" y="${y + 3.5}" text-anchor="end">${esc(fmtTick(v, yt.step, p.unit || ''))}</text>`;
    }
    for (const v of xt.ticks) {
      const x = r1(xs(v));
      s += `<line class="gr" x1="${x}" x2="${x}" y1="${y0}" y2="${y0 + h}"/>`;
      if (k === panels.length - 1) s += `<text class="tk" x="${x}" y="${y0 + h + 14}" text-anchor="middle">${esc(opts.xlog ? fmtLog(v) : fmtTick(v, xt.step, opts.xunit || ''))}</text>`;
    }
    if (lo < 0 && hi > 0) s += `<line class="zero" x1="${ML}" x2="${ML + pw}" y1="${r1(ys(0))}" y2="${r1(ys(0))}"/>`;
    p.series.forEach(c => {
      let d = '';
      let pen = false;
      for (let i = 0; i < c.x.length; i++) {
        if (!isFinite(c.y[i]) || !isFinite(c.x[i])) { pen = false; continue; }
        const X = r1(xs(c.x[i])), Y = r1(Math.max(y0 - 2, Math.min(y0 + h + 2, ys(c.y[i]))));
        d += (pen ? 'L' : 'M') + X + ' ' + Y;
        pen = true;
      }
      s += `<path class="cv ${c.cls}" d="${d}"/>`;
    });
    // legend
    let lx = ML;
    p.series.forEach(c => {
      if (!c.name) return;
      s += `<rect class="${c.cls}" x="${lx}" y="${ly - 7}" width="14" height="3"/><text class="lg" x="${lx + 18}" y="${ly}">${esc(c.name)}</text>`;
      lx += 30 + 6.3 * [...c.name].reduce((a, ch) => a + (ch.charCodeAt(0) > 0x2e80 ? 1.85 : 1), 0);
    });
    if (p.label) s += `<text class="pl" x="${ML + pw}" y="${ly}" text-anchor="end">${esc(p.label)}</text>`;
    y0 += h + GAP;
  });
  if (opts.xlabel) s += `<text class="tk xl" x="${ML + pw / 2}" y="${H - 3}" text-anchor="middle">${esc(opts.xlabel)}</text>`;
  return s + '</svg>';
}
function logTicks(a, b) {
  const out = [];
  for (let e = Math.floor(Math.log10(a)); e <= Math.ceil(Math.log10(b)); e++) { const v = Math.pow(10, e); if (v >= a * 0.999 && v <= b * 1.001) out.push(v); }
  return { ticks: out, step: 1 };
}
const fmtLog = v => si(v, 'Hz', 3).replace(' ', '');

const CLS = ['c0', 'c1', 'c2', 'c3', 'c4', 'c5'];
// the figure's SVG and the plotted (decimated) data
function plotFig(key, fig, cols) {
  const lab = n => (fig.lab && n in fig.lab ? fig.lab[n] : n);
  const data = {};
  if (fig.ac) {
    const F = cols.Freq;
    const series = fig.bode.map((n, k) => {
      const re = cols[n + '_re'], im = cols[n + '_im'];
      if (!re) throw new Error(`${key}: no AC column ${n}`);
      const mag = re.map((r, i) => 20 * Math.log10(Math.hypot(r, im[i])));
      let ph = re.map((r, i) => Math.atan2(im[i], r) * 180 / Math.PI);
      // unwrap the phase
      for (let i = 1; i < ph.length; i++) { while (ph[i] - ph[i - 1] > 180) ph[i] -= 360; while (ph[i] - ph[i - 1] < -180) ph[i] += 360; }
      data[n + ' |H|'] = F.map((f, i) => [f, mag[i]]);
      data[n + ' ∠H'] = F.map((f, i) => [f, ph[i]]);
      return { mag, ph, name: lab(n), cls: CLS[k % 6] };
    });
    const svg = svgPanels([
      { series: series.map(s => ({ x: F, y: s.mag, name: s.name, cls: s.cls })), unit: 'dB', label: '大小', h: 130 },
      { series: series.map(s => ({ x: F, y: s.ph, name: '', cls: s.cls })), unit: '°', label: '相位', h: 110 },
    ], [F[0], F[F.length - 1]], { xlog: true, label: fig.cap, xlabel: '頻率' });
    return { svg, data };
  }
  const T = cols.Time;
  const t0 = fig.t ? fig.t[0] : T[0], t1 = fig.t ? fig.t[1] : T[T.length - 1];
  const col = n => { if (!cols[n]) throw new Error(`${key}: no probe ${n} (have ${Object.keys(cols).join(', ')})`); return cols[n]; };
  if (fig.xy) {
    // transfer characteristic: y against x over the window
    const xs = col(fig.xy.x);
    const panels = [{ series: fig.xy.y.map((n, k) => { const d = decimate(T, col(n), t0, t1, 400); data[n] = d; return null; }) }];
    const idx = []; for (let i = 0; i < T.length; i++) if (T[i] >= t0 && T[i] <= t1) idx.push(i);
    data[fig.xy.x] = decimate(T, xs, t0, t1, 400);
    const series = fig.xy.y.map((n, k) => ({ x: idx.map(i => xs[i]), y: idx.map(i => col(n)[i]), name: lab(n), cls: CLS[k % 6] }));
    let lo = Infinity, hi = -Infinity; for (const i of idx) { lo = Math.min(lo, xs[i]); hi = Math.max(hi, xs[i]); }
    // thin the drawn points (smooth characteristics)
    const step = Math.max(1, Math.floor(idx.length / 260));
    for (const sr of series) { sr.x = sr.x.filter((_, i) => i % step === 0 || i === sr.x.length - 1); sr.y = sr.y.filter((_, i) => i % step === 0 || i === sr.y.length - 1); }
    panels[0] = { series, unit: fig.xy.yunit || '', h: fig.h || 220, yr: fig.xy.yr };
    const svg = svgPanels(panels, fig.xy.xr || [lo, hi], { label: fig.cap, xunit: fig.xy.xunit || '', xlabel: fig.xy.xlabel });
    return { svg, data };
  }
  const panels = fig.plots.map(p => ({
    series: p.y.map((n, k) => {
      const d = decimate((cols.__T && cols.__T[n]) || T, col(n), t0, t1);
      data[n] = d;
      return { x: d.map(q => q[0]), y: d.map(q => q[1]), name: lab(n), cls: CLS[(p.c !== undefined ? p.c + k : k) % 6] };
    }),
    unit: p.unit || '',
    label: p.label,
    h: p.h || fig.h,
    yr: p.yr,
  }));
  const svg = svgPanels(panels, [t0, t1], { label: fig.cap, xunit: 's', h: fig.h || (fig.plots.length > 2 ? 78 : 120) });
  return { svg, data };
}

/* ------------------------------------------------------------------ comparison (CI) */
// each JS-engine point against the program's curve near the same time (edges may move by a sample):
// per series, the 98th percentile of the deviation must stay below 1 % of the plotted range
function compare(ref, got) {
  const bad = [];
  const rows = [];
  for (const n of Object.keys(ref)) {
    const a = ref[n], b = got[n];
    if (!b) { bad.push(`${n}：缺少資料`); continue; }
    let lo = Infinity, hi = -Infinity;
    for (const [, v] of a) if (isFinite(v)) { lo = Math.min(lo, v); hi = Math.max(hi, v); }
    const range = Math.max(hi - lo, Math.abs(hi) * 1e-6, 1e-12);
    const span = (a[a.length - 1][0] - a[0][0]) || 1;
    const win = span / 220 * 1.5;
    const dev = a.map(([t, v]) => {
      let m = Infinity;
      for (const [u, w] of b) if (Math.abs(u - t) <= win) m = Math.min(m, Math.abs(w - v));
      if (m === Infinity) { // no point near: interpolate
        let j = b.findIndex(q => q[0] >= t);
        if (j <= 0) j = 1;
        const [u0, w0] = b[j - 1], [u1, w1] = b[Math.min(j, b.length - 1)];
        m = Math.abs(w0 + (w1 - w0) * (u1 > u0 ? (t - u0) / (u1 - u0) : 0) - v);
      }
      return (isFinite(m) ? m : 0) / range;
    }).sort((x, y) => x - y);
    const p98 = dev[Math.floor(dev.length * 0.98)] || 0, mx = dev[dev.length - 1] || 0;
    rows.push(`${n}: 98% ${(p98 * 100).toFixed(3)} %，最大 ${(mx * 100).toFixed(2)} %`);
    if (p98 > 0.01) bad.push(`${n}：偏差 ${(p98 * 100).toFixed(2)} %（98 百分位）`);
  }
  return { bad, rows };
}

/* ------------------------------------------------------------------ symbols */
function symbolSvg(d) {
  let pins = d.pins;
  let bb = d.bb;
  if (d.type === 'CBLOCK') {
    pins = [{ x: -2, y: 0, k: 'ci', n: 'x1' }, { x: 2, y: 0, k: 'co', n: 'y1' }];
  }
  const [x0, y0, x1, y1] = bb;
  const m = 0.7;
  const vb = [x0 - m, y0 - m, x1 - x0 + 2 * m, y1 - y0 + 2 * m];
  const h = Math.min(96, Math.max(46, vb[3] * 15));
  let s = `<svg class="sym" viewBox="${vb.map(v => +v.toFixed(2)).join(' ')}" style="height:${h.toFixed(0)}px" role="img" aria-label="${esc(d.zh)} 符號">`;
  s += `<g class="sg">${d.sym.g}</g>`;
  for (const t of d.sym.t || []) s += `<text x="${t.x}" y="${t.y}" font-size="${t.sz}" text-anchor="${t.a || 'start'}" class="st">${esc(t.s)}</text>`;
  if (d.type === 'CBLOCK') s += `<text x="0" y="0.3" font-size="0.9" text-anchor="middle" class="st">C</text>`;
  for (const p of pins) {
    const c = p.k === 'p' ? 'pp' : p.k === 'a' ? 'pa2' : 'pc';
    if (p.k === 'p') s += `<rect class="${c}" x="${p.x - 0.13}" y="${p.y - 0.13}" width="0.26" height="0.26"/>`;
    else if (p.k === 'a') s += `<rect class="${c}" x="${p.x - 0.12}" y="${p.y - 0.12}" width="0.24" height="0.24" transform="rotate(45 ${p.x} ${p.y})"/>`;
    else if (p.k === 'gnd' || p.k === 'lbl') continue;
    else {
      // direction of the pin: from the pin towards the body (inputs) or away from it (outputs)
      const cx = (bb[0] + bb[2]) / 2, cy = (bb[1] + bb[3]) / 2;
      let dx = 0, dy = 0;
      if (Math.abs(p.x - cx) >= Math.abs(p.y - cy)) dx = Math.sign(cx - p.x) || 1; else dy = Math.sign(cy - p.y) || 1;
      if (p.k === 'co') { dx = -dx; dy = -dy; }
      const tip = [p.x + dx * 0.22, p.y + dy * 0.22], b1 = [p.x - dx * 0.12 - dy * 0.17, p.y - dy * 0.12 + dx * 0.17], b2 = [p.x - dx * 0.12 + dy * 0.17, p.y - dy * 0.12 - dx * 0.17];
      s += `<path class="${c}" d="M${tip.map(v => +v.toFixed(2)).join(' ')}L${b1.map(v => +v.toFixed(2)).join(' ')}L${b2.map(v => +v.toFixed(2)).join(' ')}Z"/>`;
    }
  }
  return s + '</svg>';
}
const KIND = { p: '■ 功率', a: '◇ 功率或控制', ci: '▶ 控制輸入', g: '▶ 閘極（控制訊號）', co: '▶ 控制輸出（指向外）', gnd: '接地', lbl: '節點' };
function pinRole(t, n, k) {
  const R = {
    'WATT I+': '電流由此流入；量測電壓為 I+ 相對 V−', 'WATT I-': '電流由此流出', 'WATT V-': '電壓的參考端（負端）',
    'WH3 A+': '該相電流流入；電壓為此點相對 N', 'WH3 B+': '該相電流流入；電壓為此點相對 N', 'WH3 C+': '該相電流流入；電壓為此點相對 N',
    'WH3 A-': '該相電流流出', 'WH3 B-': '該相電流流出', 'WH3 C-': '該相電流流出', 'WH3 N': '電壓參考點（三相三線可接任一點）',
    'DVDT in': '輸入電壓', 'DVDT out': '輸出 dv/dt',
  };
  if (R[t + ' ' + n]) return R[t + ' ' + n];
  if (k === 'p' && n === 'in') return '電流流入';
  if (k === 'p' && n === 'out') return '電流流出';
  return '';
}
function pinList(d) {
  if (d.type === 'CBLOCK') return '輸入 x1…xn（▶ 控制輸入）、輸出 y1…ym（◀ 控制輸出），數量由參數決定';
  if (!d.pins.length) return '';
  return d.pins.map(p => { const r = pinRole(d.type, p.n, p.k); return `<span class="pin"><b>${esc(p.n)}</b> ${KIND[p.k] || p.k}${r ? '：' + esc(r) : ''}</span>`; }).join('');
}
function paramTable(d) {
  const ps = d.params || [];
  if (!ps.length) return '<p class="muted">沒有參數。</p>';
  let s = '<table class="pt"><thead><tr><th>參數</th><th>預設值</th><th>說明</th></tr></thead><tbody>';
  for (const p of ps) {
    let def = p.d === '' ? '（空白）' : String(p.d);
    if (p.type === 'code' || p.type === 'expr' || /\n/.test(def)) def = `<pre>${esc(def)}</pre>`;
    else def = `<code>${esc(def)}</code>${p.u ? ' ' + esc(p.u) : ''}`;
    let desc = esc(p.desc || '');
    if (p.opts) desc += (desc ? '<br>' : '') + p.opts.map(o => esc(o[1])).join('；');
    s += `<tr><td><b>${esc(p.l)}</b><br><code class="k">${esc(p.k)}</code>${p.adv ? ' <span class="adv">進階</span>' : ''}</td><td>${def}</td><td>${desc}</td></tr>`;
  }
  return s + '</tbody></table>';
}

/* ------------------------------------------------------------------ page */
const CAT_ZH = {
  'Elements/Power/RLC Branches': '功率元件 › RLC 支路', 'Elements/Power/Switches': '功率元件 › 開關', 'Elements/Power/Non-ideal Switches': '功率元件 › 非理想開關',
  'Elements/Power/Transformers': '功率元件 › 變壓器', 'Elements/Power/Other': '功率元件 › 其他', 'Elements/Control': '控制元件', 'Elements/Control/Filters': '控制元件 › 濾波器',
  'Elements/Control/Computational Blocks': '控制元件 › 運算方塊', 'Elements/Control/Other Function Blocks': '控制元件 › 其他功能方塊', 'Elements/Control/Logic Elements': '控制元件 › 邏輯元件',
  'Elements/Control/Digital Control Module': '控制元件 › 數位控制模組', 'Elements/Other': '其他 › 分析與模擬設定', 'Elements/Other/Switch Controllers': '其他 › 開關控制器', 'Elements/Other/Sensors': '其他 › 感測器',
  'Elements/Other/Probes': '其他 › 探棒與量表', 'Elements/Other/Function Blocks': '其他 › 函數方塊', 'Elements/Sources/Voltage': '電源 › 電壓源', 'Elements/Sources/Current': '電源 › 電流源', 'Elements/Symbols': '符號',
};
const catId = c => 'cat-' + c.replace(/^Elements\//, '').replace(/[^A-Za-z]+/g, '-').toLowerCase();

function page(cards, chapters) {
  const cats = [];
  for (const d of LIB) if (!cats.includes(d.cat)) cats.push(d.cat);
  // sources first (as in the library tree they are last, but most circuits start with them)
  const order = cats.filter(c => c.startsWith('Elements/Sources')).concat(cats.filter(c => !c.startsWith('Elements/Sources')));
  let toc = '<nav class="toc" id="toc"><div class="tt">目錄</div><ol>';
  for (const ch of chapters) toc += `<li><a href="#${ch.id}">${esc(ch.title)}</a></li>`;
  toc += '<li><a href="#elements">元件參考</a><ol class="sub">';
  for (const c of order) toc += `<li><a href="#${catId(c)}">${esc(CAT_ZH[c] || c)}</a></li>`;
  toc += '</ol></li></ol></nav>';
  let main = '';
  for (const ch of chapters) main += `<section class="ch" id="${ch.id}"><h2>${esc(ch.title)}</h2>${ch.html}</section>`;
  main += '<section class="ch" id="elements"><h2>元件參考</h2>' + TEXT.elementsIntro;
  for (const c of order) {
    main += `<section class="cat" id="${catId(c)}"><h3>${esc(CAT_ZH[c] || c)}</h3><div class="grid">`;
    for (const d of LIB.filter(x => x.cat === c)) main += cards[d.type];
    main += '</div></section>';
  }
  main += '</section>';
  return `<!doctype html>
<html lang="zh-Hant">
<head>
<meta charset="utf-8">
<meta name="viewport" content="width=device-width, initial-scale=1">
<title>DaedalusEngine 使用說明書 ${VERSION}</title>
<style>${CSS}</style>
</head>
<body>
<header class="top"><button class="menu" id="menu" aria-label="目錄">☰</button><div class="brand">DaedalusEngine 使用說明書 <span class="ver">${VERSION}</span></div>
<input id="q" type="search" placeholder="搜尋元件、參數或功能（例如 MOSFET、PWM、THD）" autocomplete="off"><span id="hits" class="muted"></span><button class="theme" id="theme" title="切換淺色／深色">◐ 跟隨系統</button></header>
${toc}
<main id="main">
${main}
<footer class="foot">DaedalusEngine ${VERSION} · 本說明書由程式內建，元件的波形圖皆以 DaedalusEngine 本身模擬示範電路產生。</footer>
</main>
<script>${JS}</script>
</body>
</html>
`;
}

const LIGHT = '--bg:#f6f8fa;--panel:#fff;--text:#18202a;--muted:#5d6877;--line:#d6dce4;--accent:#e8590c;--link:#1c6fd1;--code:#eef2f6;--sym:#1f48b8;--ctl:#0b7a75;--plot:#fbfcfd;--grid:#e3e8ee;--zero:#b9c3cf;--c0:#1c6fd1;--c1:#e8590c;--c2:#2b8a3e;--c3:#c2255c;--c4:#6741d9;--c5:#0c8599;';
const DARK = '--bg:#0f1319;--panel:#151a21;--text:#e4e9ef;--muted:#9aa6b4;--line:#2a323d;--accent:#ff922b;--link:#7aa7ff;--code:#1d242e;--sym:#7aa7ff;--ctl:#4fd1c5;--plot:#0b0f14;--grid:#1f2731;--zero:#3a4554;--c0:#5aa2ff;--c1:#ff922b;--c2:#69db7c;--c3:#f783ac;--c4:#b197fc;--c5:#3bc9db;';
const CSS = `
:root{${LIGHT}color-scheme:light}
@media (prefers-color-scheme:dark){:root:not([data-theme=light]){${DARK}color-scheme:dark}}
:root[data-theme=dark]{${DARK}color-scheme:dark}
*{box-sizing:border-box}html{scroll-padding-top:64px}
body{margin:0;background:var(--bg);color:var(--text);font:15px/1.65 "Microsoft JhengHei UI","Microsoft JhengHei","Noto Sans TC","PingFang TC",system-ui,sans-serif}
a{color:var(--link);text-decoration:none}a:hover{text-decoration:underline}
code,pre{font-family:Consolas,"Cascadia Mono",monospace;font-size:.9em;background:var(--code);border-radius:4px}
code{padding:0 .3em}pre{padding:.6em .8em;overflow:auto;margin:.4em 0;white-space:pre-wrap}
.top{position:sticky;top:0;z-index:5;display:flex;gap:12px;align-items:center;padding:10px 18px;background:var(--panel);border-bottom:1px solid var(--line)}
.brand{font-weight:700;font-size:17px;white-space:nowrap}.ver{color:var(--muted);font-weight:400;font-size:14px}
#q{flex:1;max-width:520px;padding:7px 12px;border:1px solid var(--line);border-radius:8px;background:var(--bg);color:var(--text);font:inherit}
.theme{border:1px solid var(--line);background:var(--bg);color:var(--text);border-radius:6px;padding:4px 10px;font:inherit;font-size:13px;cursor:pointer;white-space:nowrap;margin-left:auto}
.menu{display:none;border:1px solid var(--line);background:var(--bg);color:var(--text);border-radius:6px;font-size:18px;padding:2px 9px}
.toc{position:fixed;top:56px;bottom:0;left:0;width:260px;overflow:auto;padding:14px 10px 30px 18px;border-right:1px solid var(--line);background:var(--panel);font-size:14px}
.toc .tt{font-weight:700;color:var(--muted);margin-bottom:6px}.toc ol{margin:0;padding-left:1.2em}.toc li{margin:3px 0}.toc .sub{font-size:13px}
main{margin-left:260px;padding:8px 28px 40px;max-width:1260px}
h2{font-size:24px;margin:34px 0 12px;padding-bottom:6px;border-bottom:2px solid var(--line)}
h3{font-size:19px;margin:28px 0 10px;color:var(--accent)}h4{margin:0;font-size:17px}
.ch p,.ch li{max-width:980px}
table{border-collapse:collapse;margin:8px 0;font-size:14px;background:var(--panel)}
th,td{border:1px solid var(--line);padding:5px 9px;vertical-align:top;text-align:left}th{background:var(--code)}
.grid{display:grid;grid-template-columns:repeat(auto-fill,minmax(500px,1fr));gap:16px}
.el{background:var(--panel);border:1px solid var(--line);border-radius:10px;padding:14px 16px;min-width:0}
.eh{display:flex;gap:14px;align-items:center;margin-bottom:6px}
.en{color:var(--muted);font-weight:400;font-size:14px;margin-left:6px}.meta{color:var(--muted);font-size:13px}
.sym{width:auto;max-width:150px;flex:none;background:var(--plot);border:1px solid var(--line);border-radius:8px;padding:4px}
.sg path,.sg circle,.sg rect{fill:none;stroke:var(--sym);stroke-width:.09;stroke-linecap:round;stroke-linejoin:round}
.sg .f{fill:var(--sym)}.sg .thin{stroke-width:.05}.sg .dash{stroke-dasharray:.2 .15}
.st{fill:var(--sym);font-family:Consolas,monospace}
.pp{fill:var(--sym)}.pc{fill:var(--ctl)}.pa2{fill:none;stroke:var(--sym);stroke-width:.05}
.desc{margin:6px 0 8px;white-space:pre-line}
figure{margin:8px 0 10px}figcaption{color:var(--muted);font-size:13.5px;margin-top:2px}
.plot{width:100%;height:auto;display:block;background:var(--panel)}
.pa{fill:var(--plot);stroke:var(--line)}.gr{stroke:var(--grid);stroke-width:1}.zero{stroke:var(--zero);stroke-width:1}
.tk{fill:var(--muted);font:10.5px Consolas,monospace}.xl{font-family:inherit}
.cv{fill:none;stroke-width:1.7;stroke-linejoin:round}.lg{font-size:11.5px;fill:var(--text)}.pl{fill:var(--muted);font-size:11px}
.c0{stroke:var(--c0);fill:var(--c0)}.c1{stroke:var(--c1);fill:var(--c1)}.c2{stroke:var(--c2);fill:var(--c2)}.c3{stroke:var(--c3);fill:var(--c3)}.c4{stroke:var(--c4);fill:var(--c4)}.c5{stroke:var(--c5);fill:var(--c5)}
.cv.c0,.cv.c1,.cv.c2,.cv.c3,.cv.c4,.cv.c5{fill:none}.lg.c0,.lg.c1,.lg.c2,.lg.c3,.lg.c4,.lg.c5{stroke:none}
.pt{width:100%}.pt pre{margin:0;font-size:12.5px}.pt td:first-child{width:30%}.pt td:nth-child(2){width:22%;word-break:break-all}
.k{font-size:12px}.adv{font-size:11px;color:var(--muted);border:1px solid var(--line);border-radius:4px;padding:0 4px}
.pins{font-size:13px;color:var(--muted);margin:4px 0}.pin{display:inline-block;margin-right:12px}
.muted{color:var(--muted)}.note{border-left:3px solid var(--accent);padding:4px 12px;background:var(--code);border-radius:0 6px 6px 0}
details>summary{cursor:pointer;color:var(--link);font-size:14px;margin:4px 0}
.kbd{display:inline-block;border:1px solid var(--line);border-bottom-width:2px;border-radius:4px;padding:0 5px;font-size:13px;background:var(--panel)}
.hide{display:none!important}.foot{margin-top:40px;color:var(--muted);font-size:13px}
@media (max-width:900px){.toc{transform:translateX(-100%);transition:transform .2s;z-index:6;width:280px}.toc.open{transform:none}main{margin-left:0;padding:8px 14px 40px}.menu{display:block}.grid{grid-template-columns:1fr}.brand{font-size:15px}}
@media print{.top,.toc{display:none}main{margin:0}.el{break-inside:avoid}}
`;

const JS = `
const q=document.getElementById('q'),hits=document.getElementById('hits');
const cards=[...document.querySelectorAll('.el')],cats=[...document.querySelectorAll('.cat')],chs=[...document.querySelectorAll('.ch:not(#elements)')];
q.addEventListener('input',()=>{const t=q.value.trim().toLowerCase();let n=0;
cards.forEach(c=>{const ok=!t||c.dataset.k.includes(t)||c.textContent.toLowerCase().includes(t);c.classList.toggle('hide',!ok);if(ok&&t)n++;});
cats.forEach(s=>s.classList.toggle('hide',!!t&&!s.querySelector('.el:not(.hide)')));
chs.forEach(s=>s.classList.toggle('hide',!!t&&!s.textContent.toLowerCase().includes(t)));
hits.textContent=t?n+' 個元件':'';});
const th=document.getElementById('theme'),root=document.documentElement,TH=[['','◐ 跟隨系統'],['light','☀ 淺色'],['dark','☾ 深色']];
let ti=0;try{ti=Math.max(0,TH.findIndex(x=>x[0]===(localStorage.getItem('de-theme')||'')));}catch(e){}
const setTheme=i=>{ti=i;const v=TH[i][0];if(v)root.setAttribute('data-theme',v);else root.removeAttribute('data-theme');th.textContent=TH[i][1];try{localStorage.setItem('de-theme',v);}catch(e){}};
setTheme(ti);th.addEventListener('click',()=>setTheme((ti+1)%TH.length));
document.getElementById('menu').addEventListener('click',()=>document.getElementById('toc').classList.toggle('open'));
document.querySelectorAll('.toc a').forEach(a=>a.addEventListener('click',()=>document.getElementById('toc').classList.remove('open')));
`;

/* ------------------------------------------------------------------ main */
function main() {
  const ref = fs.existsSync(DATA) ? JSON.parse(fs.readFileSync(DATA, 'utf8')) : {};
  const outData = {};
  const cards = {};
  let fails = 0, compared = 0;
  const missing = LIB.filter(d => !FIGS[d.type] && !TEXT.noFigure[d.type]).map(d => d.type);
  if (missing.length) console.log('沒有示範圖：' + missing.join(' '));
  for (const d of LIB) {
    if (ONLY && !ONLY.has(d.type)) continue;
    const fig = FIGS[d.type];
    let figHtml = '';
    if (fig) {
      const cols = simulate(d.type, fig);
      const { svg, data } = plotFig(d.type, fig, cols);
      outData[d.type] = data;
      if (argv.includes('--stats')) console.log(d.type.padEnd(9) + Object.entries(data).map(([n, p]) => { const v = p.map(q => q[1]).filter(isFinite); return `${n}[${si(Math.min(...v))}…${si(Math.max(...v))}]`; }).join(' '));
      figHtml = `<figure>${svg}<figcaption>${fig.cap}</figcaption></figure>`;
      if (CLI) {
        if (fig.nocmp) console.log(`  -  | ${d.type.padEnd(10)} | 不比較（${fig.nocmp}）`);
        else if (!ref[d.type]) { console.log(`FAIL | ${d.type.padEnd(10)} | figdata.json 沒有此圖（請在本機重新產生說明書）`); fails++; }
        else {
          const r = compare(fig.compare ? Object.fromEntries(Object.entries(ref[d.type]).filter(([k]) => fig.compare.includes(k))) : ref[d.type], data);
          compared++;
          const known = fig.differs;
          if (r.bad.length && !known) fails++;
          console.log(`${r.bad.length ? (known ? 'DIFF' : 'FAIL') : ' OK '} | ${d.type.padEnd(10)} | ${r.bad.length ? r.bad.join('；') + (known ? `（已知差異：${known}）` : '') : r.rows.join('；')}`);
        }
      }
    } else if (TEXT.noFigure[d.type]) {
      figHtml = `<div class="note">${TEXT.noFigure[d.type]}</div>`;
    }
    const extra = (TEXT.extra && TEXT.extra[d.type]) || '';
    const kw = [d.type, d.name, d.zh, d.pre || ''].join(' ').toLowerCase();
    cards[d.type] = `<article class="el" id="el-${d.type}" data-k="${esc(kw)}">
<div class="eh">${symbolSvg(d)}<div><h4>${esc(d.zh)}<span class="en">${esc(d.name)}</span></h4><div class="meta"><code>${esc(d.type)}</code>${d.pre ? ` · 名稱前綴 ${esc(d.pre)}` : ''} · ${esc(CAT_ZH[d.cat] || d.cat)}</div><div class="pins">${pinList(d)}</div></div></div>
<p class="desc">${esc(d.desc)}</p>${figHtml}${extra}
${paramTable(d)}
</article>`;
  }
  if (ONLY) {
    const f = path.join(TMP, 'preview.html');
    fs.mkdirSync(TMP, { recursive: true });
    fs.writeFileSync(f, page(Object.assign(Object.fromEntries(LIB.map(d => [d.type, ''])), cards), argv.includes('--chapters') ? TEXT.chapters({ VERSION, LIB }) : []));
    console.log('預覽：' + f);
    return;
  }
  const html = page(cards, TEXT.chapters({ VERSION, LIB }));
  fs.writeFileSync(OUT, html);
  if (!CLI) {
    // plotted data, 4 significant digits (the CI comparison allows 1 %)
    const round = v => (isFinite(v) ? +v.toPrecision(5) : null);
    const rd = Object.fromEntries(Object.entries(outData).map(([k, s]) => [k, Object.fromEntries(Object.entries(s).map(([n, pts]) => [n, pts.map(([t, v]) => [round(t), round(v)])]))]));
    fs.writeFileSync(DATA, JSON.stringify(rd));
  }
  const nfig = Object.keys(outData).length;
  console.log(`說明書：${(html.length / 1024).toFixed(0)} KB，${LIB.length} 個元件、${nfig} 張示範圖 → ${path.relative(ROOT, OUT)}`);
  if (CLI) {
    console.log(`\n${compared - fails}/${compared} 張圖與參考引擎一致${fails ? `，${fails} 張不一致` : ''}`);
    // branch builds report a difference, release builds (--strict) fail on it
    if (fails && !argv.includes('--strict')) console.log(`::warning title=說明書示範圖::${fails} 張圖與參考引擎不一致（見記錄）`);
    process.exit(fails && argv.includes('--strict') ? 1 : 0);
  }
}
main();
