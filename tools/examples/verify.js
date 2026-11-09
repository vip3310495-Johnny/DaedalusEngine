// Runs every generated example with daedalus-cli and evaluates its theoretical checks (defs.js).
// usage: node tools/examples/verify.js [key ...]      (default: all)
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const ROOT = path.resolve(__dirname, '..', '..');
const CLI = path.join(ROOT, 'target/release/daedalus-cli.exe');
const OUT = path.join(ROOT, 'tests/examples');
fs.mkdirSync(OUT, { recursive: true });

// ---------- helpers given to the checks ----------
function helpers(cols) {
  const T = cols.Time;
  const h = {};
  h.t = T;
  h.col = n => { if (!cols[n]) throw new Error('no probe ' + n + ' (have ' + Object.keys(cols).join(',') + ')'); return cols[n]; };
  h.at = (n, t) => {
    const y = h.col(n);
    let lo = 0, hi = T.length - 1;
    if (t <= T[0]) return y[0];
    if (t >= T[hi]) return y[hi];
    while (hi - lo > 1) { const m = (lo + hi) >> 1; if (T[m] <= t) lo = m; else hi = m; }
    return y[lo] + (y[hi] - y[lo]) * (t - T[lo]) / (T[hi] - T[lo]);
  };
  h.last = n => h.col(n)[h.col(n).length - 1];
  const idx = (t0, t1) => { const r = []; T.forEach((t, i) => { if (t >= t0 - 1e-15 && t <= t1 + 1e-15) r.push(i); }); return r; };
  h.avg = (n, t0, t1) => { const y = h.col(n), I = idx(t0, t1); let s = 0; for (let k = 1; k < I.length; k++) s += (y[I[k]] + y[I[k - 1]]) / 2 * (T[I[k]] - T[I[k - 1]]); return s / (T[I[I.length - 1]] - T[I[0]]); };
  h.rms = (n, t0, t1) => { const y = h.col(n), I = idx(t0, t1); let s = 0; for (let k = 1; k < I.length; k++) s += (y[I[k]] ** 2 + y[I[k - 1]] ** 2) / 2 * (T[I[k]] - T[I[k - 1]]); return Math.sqrt(s / (T[I[I.length - 1]] - T[I[0]])); };
  h.max = (n, t0 = -Infinity, t1 = Infinity) => Math.max(...idx(Math.max(t0, T[0]), Math.min(t1, T[T.length - 1])).map(i => h.col(n)[i]));
  h.min = (n, t0 = -Infinity, t1 = Infinity) => Math.min(...idx(Math.max(t0, T[0]), Math.min(t1, T[T.length - 1])).map(i => h.col(n)[i]));
  // least-squares y ≈ dc + A·sin(ωt + φ) on [t0, t1]
  h.fit = (n, f, t0, t1) => {
    const y = h.col(n), I = idx(t0, t1), w = 2 * Math.PI * f;
    const M = [[0, 0, 0], [0, 0, 0], [0, 0, 0]], v = [0, 0, 0];
    for (const i of I) { const b = [1, Math.sin(w * T[i]), Math.cos(w * T[i])]; for (let r = 0; r < 3; r++) { v[r] += b[r] * y[i]; for (let c = 0; c < 3; c++) M[r][c] += b[r] * b[c]; } }
    const x = solve3(M, v);
    return { dc: x[0], A: Math.hypot(x[1], x[2]), ph: Math.atan2(x[2], x[1]) * 180 / Math.PI };
  };
  // max |y − fn(t)| over samples (optionally skipping points near discontinuities)
  h.err = (n, fn, t0 = -Infinity, t1 = Infinity, skip = []) => {
    const y = h.col(n); let e = 0, at = 0;
    T.forEach((t, i) => { if (t < t0 || t > t1 || skip.some(s => Math.abs(t - s) < 1.5 * (T[1] - T[0]))) return; const d = Math.abs(y[i] - fn(t)); if (d > e) { e = d; at = t; } });
    return { e, at };
  };
  h.cross = (n, lvl, dir = 0) => { const y = h.col(n), o = []; for (let i = 1; i < y.length; i++) { const a = y[i - 1] - lvl, b = y[i] - lvl; if (a * b < 0 || (b === 0 && a !== 0)) { if (dir > 0 && a > 0) continue; if (dir < 0 && a < 0) continue; o.push(T[i - 1] + (T[i] - T[i - 1]) * a / (a - b)); } } return o; };
  h.near = (a, b, rel, abs = 0) => Math.abs(a - b) <= Math.max(rel * Math.abs(b), abs);
  h.dphase = (a, b) => ((a - b + 540) % 360) - 180;
  return h;
}
// AC sweep results: Freq column and <probe>_re / <probe>_im
function acHelpers(cols) {
  return {
    col: n => { if (!cols[n]) throw new Error('no column ' + n + ' (have ' + Object.keys(cols).join(',') + ')'); return cols[n]; },
    near: (a, b, rel, abs = 0) => Math.abs(a - b) <= Math.max(rel * Math.abs(b), abs),
  };
}
function solve3(M, v) {
  const a = M.map((r, i) => [...r, v[i]]);
  for (let c = 0; c < 3; c++) {
    let p = c; for (let r = c + 1; r < 3; r++) if (Math.abs(a[r][c]) > Math.abs(a[p][c])) p = r;
    [a[c], a[p]] = [a[p], a[c]];
    for (let r = 0; r < 3; r++) if (r !== c) { const f = a[r][c] / a[c][c]; for (let k = c; k < 4; k++) a[r][k] -= f * a[c][k]; }
  }
  return a.map((r, i) => r[3] / r[i]);
}

function readCsv(f) {
  const L = fs.readFileSync(f, 'utf8').trim().split(/\r?\n/);
  const h = L[0].split(',');
  const c = {};
  h.forEach(k => (c[k] = []));
  for (let i = 1; i < L.length; i++) { const v = L[i].split(','); for (let k = 0; k < h.length; k++) c[h[k]].push(+v[k]); }
  return c;
}

const fmt = x => (typeof x !== 'number' ? String(x) : !isFinite(x) ? String(x) : Math.abs(x) >= 1e4 || (Math.abs(x) < 1e-3 && x !== 0) ? x.toExponential(4) : (+x.toPrecision(6)).toString());

function main() {
  const defs = require('./defs.js');
  const want = process.argv.slice(2).filter(a => !a.startsWith('-'));
  let bad = 0, total = 0, skipped = 0;
  const summary = [];
  for (const ex of defs) {
    if (want.length && !want.includes(ex.key)) continue;
    const ps = path.join(ROOT, 'crates/daedalus-app/assets/examples', ex.key + '.psimx');
    const csv = path.join(OUT, ex.key + '.csv');
    const t0 = Date.now();
    let st;
    try { st = JSON.parse(execFileSync(CLI, ex.ac ? [ps, csv, '--ac'] : [ps, csv], { maxBuffer: 1 << 28 }).toString().trim().split('\n').pop()); }
    catch (e) { console.log(`FAIL | ${ex.key} | 模擬失敗：${String(e.stderr || e.message).split('\n')[0]}`); bad++; total++; continue; }
    const ms = Date.now() - t0;
    const cols = readCsv(csv);
    const h = ex.ac ? acHelpers(cols) : helpers(cols);
    h.stats = st;
    let rows;
    try { rows = ex.checks(h); } catch (e) { rows = [{ item: 'check error', th: '', sim: e.message, ok: false }]; }
    // a row with skip = n stands for n checks that could not run (e.g. no ngspice): never counted as passed
    let exBad = 0, exRun = 0, exSkip = 0;
    for (const r of rows) {
      if (r.skip) {
        exSkip += r.skip;
        console.log(`SKIP | ${ex.key.padEnd(18)} | ${r.item}（${r.skip} 項）| ${r.note || ''}`);
        continue;
      }
      total++;
      exRun++;
      if (!r.ok) { bad++; exBad++; }
      console.log(`${r.ok ? ' OK ' : 'FAIL'} | ${ex.key.padEnd(18)} | ${r.item} | 理論 ${fmt(r.th)} | 模擬 ${fmt(r.sim)}${r.note ? ' | ' + r.note : ''}`);
    }
    skipped += exSkip;
    summary.push(`${ex.key}: ${exRun - exBad}/${exRun}${exSkip ? `，略過 ${exSkip}` : ''}（${ms} ms${st.warnings && st.warnings.length ? '，警告 ' + st.warnings.length : ''}）`);
    if (st.warnings && st.warnings.length && process.argv.includes('-w')) console.log('   warnings: ' + st.warnings.slice(0, 3).join(' / '));
  }
  console.log('\n' + summary.join('\n'));
  console.log(`\n${total - bad}/${total} checks passed${skipped ? `（另有 ${skipped} 項因缺少參考工具而略過，未計入）` : ''}`);
  // --strict (CI release builds): a skipped check is a failure
  process.exit(bad || (skipped && process.argv.includes('--strict')) ? 1 : 0);
}
main();
