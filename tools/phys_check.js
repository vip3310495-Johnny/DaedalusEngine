// Physical / theoretical verification of the example circuits.
// Independent reference solutions (exact matrix exponentials of each linear mode, events found by
// bisection) plus closed-form theory values; nothing here shares code with the simulator.
// usage: node tools/phys_check.js <dir with example csv files>
const fs = require('fs');
const dir = process.argv[2] || 'tests/phys';

function readCsv(f) {
  const lines = fs.readFileSync(f, 'utf8').trim().split(/\r?\n/);
  const head = lines[0].split(',');
  const cols = {};
  head.forEach(h => (cols[h] = []));
  for (let i = 1; i < lines.length; i++) {
    const v = lines[i].split(',');
    head.forEach((h, k) => cols[h].push(parseFloat(v[k])));
  }
  return cols;
}

// ---------- small dense linear algebra ----------
const eye = n => Array.from({ length: n }, (_, i) => Array.from({ length: n }, (_, j) => (i === j ? 1 : 0)));
const mmul = (a, b) => a.map(r => b[0].map((_, j) => r.reduce((s, x, k) => s + x * b[k][j], 0)));
const madd = (a, b, s = 1) => a.map((r, i) => r.map((x, j) => x + s * b[i][j]));
const mscale = (a, s) => a.map(r => r.map(x => x * s));
const mvec = (a, v) => a.map(r => r.reduce((s, x, k) => s + x * v[k], 0));
const norm1 = a => Math.max(...a[0].map((_, j) => a.reduce((s, r) => s + Math.abs(r[j]), 0)));
function solve(a, b) {
  const n = a.length, m = a.map((r, i) => [...r, ...b[i]]);
  for (let c = 0; c < n; c++) {
    let p = c;
    for (let r = c + 1; r < n; r++) if (Math.abs(m[r][c]) > Math.abs(m[p][c])) p = r;
    [m[c], m[p]] = [m[p], m[c]];
    for (let r = 0; r < n; r++) if (r !== c) { const f = m[r][c] / m[c][c]; for (let k = c; k < m[r].length; k++) m[r][k] -= f * m[c][k]; }
  }
  return m.map((r, i) => r.slice(n).map(x => x / m[i][i]));
}
// matrix exponential: scaling and squaring with a [8/8] Pade approximant (double accurate)
function expm(a) {
  const n = a.length;
  let s = Math.max(0, Math.ceil(Math.log2(norm1(a) / 0.5)) + 1);
  const as = mscale(a, Math.pow(2, -s));
  const c = [1];
  const q = 8;
  for (let k = 1; k <= q; k++) c.push(c[k - 1] * (q - k + 1) / (k * (2 * q - k + 1)));
  let x = eye(n), N = eye(n), D = eye(n);
  for (let k = 1; k <= q; k++) {
    x = mmul(as, x);
    N = madd(N, x, c[k]);
    D = madd(D, x, (k % 2 ? -1 : 1) * c[k]);
  }
  let e = solve(D, N);
  for (; s > 0; s--) e = mmul(e, e);
  return e;
}
// cache of exp(A h) per (mode, h)
const ecache = new Map();
function step(key, A, h, x) {
  const k = key + '|' + h;
  let E = ecache.get(k);
  if (!E) { E = expm(mscale(A, h)); ecache.set(k, E); }
  return mvec(E, x);
}

function compare(name, t, sim, ref, scale) {
  let emax = 0, at = 0;
  for (let i = 0; i < t.length; i++) {
    const e = Math.abs(sim[i] - ref[i]);
    if (e > emax) { emax = e; at = t[i]; }
  }
  return { name, maxErr: emax, rel: emax / scale, at };
}

const report = [];
const row = (ex, item, theory, sim, unit, ok, note = '') => report.push({ ex, item, theory, sim, unit, ok, note });
const fmt = x => (typeof x === 'number' ? (Math.abs(x) >= 1e4 || (Math.abs(x) < 1e-3 && x !== 0) ? x.toExponential(4) : (+x.toPrecision(6)).toString()) : x);
const near = (a, b, rel, abs = 0) => Math.abs(a - b) <= Math.max(rel * Math.abs(b), abs);
function avgOver(t, y, t0, t1) {
  let s = 0, w = 0;
  for (let i = 1; i < t.length; i++) {
    if (t[i] <= t0 || t[i - 1] >= t1) continue;
    const a = Math.max(t0, t[i - 1]), b = Math.min(t1, t[i]);
    const ya = y[i - 1] + (y[i] - y[i - 1]) * (a - t[i - 1]) / (t[i] - t[i - 1]);
    const yb = y[i - 1] + (y[i] - y[i - 1]) * (b - t[i - 1]) / (t[i] - t[i - 1]);
    s += (ya + yb) / 2 * (b - a); w += b - a;
  }
  return s / w;
}
const range = (t, y, t0, t1) => { let lo = Infinity, hi = -Infinity; t.forEach((x, i) => { if (x >= t0 && x <= t1) { lo = Math.min(lo, y[i]); hi = Math.max(hi, y[i]); } }); return [lo, hi]; };

// ================= Buck (open loop) =================
function buck() {
  const d = readCsv(dir + '/buck.csv');
  const Vin = 100, L = 100e-6, C = 100e-6, R = 5, ron = 10e-6, D = 0.437, T = 20e-6;
  // states [iL, vC, 1]  (the constant input is the 3rd state)
  const Aon = [[-ron / L, -1 / L, Vin / L], [1 / C, -1 / (R * C), 0], [0, 0, 0]];
  const Aoff = [[-ron / L, -1 / L, 0], [1 / C, -1 / (R * C), 0], [0, 0, 0]];
  const Adcm = [[0, 0, 0], [0, -1 / (R * C), 0], [0, 0, 0]];
  // gate: comparator Vref(0.437) > Vtri ; Vtri rises 0->1 over T/2 then falls
  const edges = [];
  for (let k = 0; k * T < 3e-3 + T; k++) { edges.push([k * T + D * T / 2, 0]); edges.push([(k + 1) * T - D * T / 2, 1]); }
  const out = { iL: [], vC: [] };
  let x = [0, 0, 1], t = 0, on = 1, dcm = false, ei = 0;
  const prop = (x, h) => {
    if (on) return step('on', Aon, h, x);
    if (dcm) return step('dcm', Adcm, h, x);
    return step('off', Aoff, h, x);
  };
  const tt = d.Time;
  for (let i = 0; i < tt.length; i++) {
    const target = tt[i];
    while (t < target - 1e-18) {
      const tn = Math.min(target, edges[ei][0]);
      let h = tn - t;
      // diode turn-off (DCM entry) inside this interval?
      if (!on && !dcm) {
        const xe = prop(x, h);
        if (xe[0] < 0) {
          let a = 0, b = h;
          for (let it = 0; it < 80; it++) { const m = (a + b) / 2; (prop(x, m)[0] < 0 ? (b = m) : (a = m)); }
          x = prop(x, a); x[0] = 0; t += a; dcm = true; continue;
        }
      }
      x = prop(x, h); t = tn;
      if (Math.abs(t - edges[ei][0]) < 1e-18) { on = edges[ei][1]; if (on) dcm = false; ei++; }
    }
    out.iL.push(x[0]); out.vC.push(x[1]);
  }
  const c1 = compare('Vo', tt, d.Vo, out.vC, 43.7), c2 = compare('IL', tt, d.IL, out.iL, 8.74);
  row('buck', 'Vo 波形 vs 精確分段解（最大誤差）', '0', fmt(c1.maxErr) + ' V @' + fmt(c1.at), '', c1.rel < 1e-3, '相對 ' + fmt(c1.rel));
  row('buck', 'IL 波形 vs 精確分段解（最大誤差）', '0', fmt(c2.maxErr) + ' A @' + fmt(c2.at), '', c2.rel < 1e-3, '相對 ' + fmt(c2.rel));
  // steady-state theory: 20 ms run (the 3 ms example still rings, envelope e^(-t/2RC), 2RC = 1 ms)
  const dl = readCsv(dir + '/buck_long.csv');
  const t1 = 20e-3, t0 = t1 - 5 * T;
  const vo = avgOver(dl.Time, dl.Vo, t0, t1), il = avgOver(dl.Time, dl.IL, t0, t1);
  const [ilo, ihi] = range(dl.Time, dl.IL, t0, t1), [vlo, vhi] = range(dl.Time, dl.Vo, t0, t1);
  const voTh = D * Vin; // ideal CCM
  const dIth = (Vin - voTh) * D * T / L, dVth = dIth / (8 * C / T);
  row('buck', '穩態 Vo 平均（D·Vin，20 ms）', voTh, vo, 'V', near(vo, voTh, 0.002));
  row('buck', '穩態 IL 平均（Vo/R）', vo / R, il, 'A', near(il, vo / R, 0.002));
  row('buck', 'IL 漣波峰對峰（(Vin−Vo)·D·T/L）', dIth, ihi - ilo, 'A', near(ihi - ilo, dIth, 0.01));
  row('buck', 'Vo 漣波峰對峰（ΔI·T/8C）', dVth, vhi - vlo, 'V', near(vhi - vlo, dVth, 0.05));
  // LC ringing of the start-up: f0 = 1/(2π√LC), overshoot for ζ = √(L/C)/(2R)
  const zeta = Math.sqrt(L / C) / (2 * R), os = Math.exp(-Math.PI * zeta / Math.sqrt(1 - zeta * zeta));
  const vmax = Math.max(...out.vC);
  const simMax = Math.max(...d.Vo);
  row('buck', '啟動過衝峰值（精確解）', vmax, simMax, 'V', near(simMax, vmax, 2e-3), '二階理論估計 ' + fmt(voTh * (1 + os)) + ' V（ζ=' + fmt(zeta) + '，平均模型）');
}

// ================= RLC (transient, 1 kHz sine) =================
function rlc() {
  const d = readCsv(dir + '/rlc.csv');
  const R1 = 2, L = 1e-3, C = 10e-6, RL = 20, w = 2 * Math.PI * 1e3;
  // states [iL, vC, s, c]  with s = sin(wt), c = cos(wt)
  const A = [[-R1 / L, -1 / L, 1 / L, 0], [1 / C, -1 / (RL * C), 0, 0], [0, 0, 0, w], [0, 0, -w, 0]];
  let x = [0, 0, 0, 1], t = 0;
  const ref = [];
  for (const tt of d.Time) { x = step('rlc' + (tt - t), A, tt - t, x); t = tt; ref.push(x[1]); }
  const c = compare('Vo', d.Time, d.Vo, ref, 1);
  row('rlc', 'Vo 波形 vs 解析解（最大誤差）', '0', fmt(c.maxErr) + ' V @' + fmt(c.at), '', c.rel < 1e-3, '相對 ' + fmt(c.rel));
  // steady-state amplitude |H(jw)|
  const re = (a, b) => [a, b];
  const zc = re(RL / (1 + (w * RL * C) ** 2), -w * RL * RL * C / (1 + (w * RL * C) ** 2));
  const den = [R1 + zc[0], w * L + zc[1]];
  const mag = Math.hypot(...zc) / Math.hypot(...den);
  const [lo, hi] = range(d.Time, d.Vo, 8e-3, 10e-3);
  row('rlc', '穩態振幅 |H(j2π·1k)|', mag, (hi - lo) / 2, 'V', near((hi - lo) / 2, mag, 2e-3));
}

// ================= Bridge rectifier =================
function bridge() {
  const d = readCsv(dir + '/bridge.csv');
  const Vm = 311, w = 2 * Math.PI * 50, L = 1e-3, C = 470e-6, R = 100, vf = 0.8, ron = 10e-6;
  // states [i, vC, s, c, 1]
  const Ap = [[-2 * ron / L, -1 / L, Vm / L, 0, -2 * vf / L], [1 / C, -1 / (R * C), 0, 0, 0], [0, 0, 0, w, 0], [0, 0, -w, 0, 0], [0, 0, 0, 0, 0]];
  const An = [[-2 * ron / L, 1 / L, Vm / L, 0, 2 * vf / L], [-1 / C, -1 / (R * C), 0, 0, 0], [0, 0, 0, w, 0], [0, 0, -w, 0, 0], [0, 0, 0, 0, 0]];
  const A0 = [[0, 0, 0, 0, 0], [0, -1 / (R * C), 0, 0, 0], [0, 0, 0, w, 0], [0, 0, -w, 0, 0], [0, 0, 0, 0, 0]];
  const P = (m, h, x) => step('br' + m, m > 0 ? Ap : m < 0 ? An : A0, h, x);
  const fwd = x => Vm * x[2] - (x[1] + 2 * vf); // positive half drive
  const rev = x => -Vm * x[2] - (x[1] + 2 * vf);
  const solveRef = times => {
  let x = [0, 0, 0, 1, 1], t = 0, mode = 0; // 0 off, 1 positive, -1 negative
  const Is = [], Vdc = [];
  const sub = 1e-6;
  for (const tt of times) {
    while (t < tt - 1e-15) {
      const h = Math.min(sub, tt - t);
      const xn = P(mode, h, x);
      const ev = mode === 0 ? (fwd(xn) > 0 ? 1 : rev(xn) > 0 ? -1 : 0) : (mode * xn[0] < 0 ? 2 : 0);
      if (!ev) { x = xn; t += h; continue; }
      let a = 0, b = h;
      const cond = mode === 0 ? (xx => (ev > 0 ? fwd(xx) : rev(xx)) > 0) : (xx => mode * xx[0] < 0);
      for (let it = 0; it < 70; it++) { const m = (a + b) / 2; cond(P(mode, m, x)) ? (b = m) : (a = m); }
      x = P(mode, b, x); t += b;
      if (mode === 0) mode = ev; else { x[0] = 0; mode = 0; }
    }
    Is.push(x[0]); Vdc.push(x[1]);
  }
  return { Is, Vdc };
  };
  const { Is, Vdc } = solveRef(d.Time);
  const c1 = compare('Vdc', d.Time, d.Vdc, Vdc, 300), c2 = compare('Is', d.Time, d.Is, Is, 30);
  row('bridge', 'Vdc 波形 vs 精確分段解（最大誤差）', '0', fmt(c1.maxErr) + ' V @' + fmt(c1.at), '', c1.rel < 2e-3, '相對 ' + fmt(c1.rel) + '（參考解忽略 1 MΩ 關斷電阻）');
  row('bridge', 'Is 波形 vs 精確分段解（最大誤差）', '0', fmt(c2.maxErr) + ' A @' + fmt(c2.at), '', c2.rel < 2e-3, '相對 ' + fmt(c2.rel));
  const pk0 = Math.max(...d.Vdc), pkRef = Math.max(...Vdc);
  row('bridge', '啟動湧入過衝（LC 諧振，精確解）', pkRef, pk0, 'V', near(pk0, pkRef, 1e-4), '0.1 s 時仍以 RC=47 ms 衰減');
  const dl = readCsv(dir + '/bridge_long.csv');
  const [lo, hi] = range(dl.Time, dl.Vdc, 0.56, 0.6);
  const rl = solveRef(dl.Time);
  const c3 = compare('Vdc', dl.Time, dl.Vdc, rl.Vdc, 300);
  row('bridge', '0.6 s 全程 Vdc vs 精確分段解', '0', fmt(c3.maxErr) + ' V @' + fmt(c3.at), '', c3.rel < 2e-3, '相對 ' + fmt(c3.rel));
  const hiRef = Math.max(...rl.Vdc.filter((_, i) => dl.Time[i] >= 0.56));
  row('bridge', '穩態直流峰值（LC 輸入：電感續推電荷，可高於 Vm−2Vf）', hiRef, hi, 'V', near(hi, hiRef, 1e-3), 'Vm−2Vf = ' + fmt(Vm - 2 * vf) + ' V；√(L/C)=1.46 Ω');
  const avg = avgOver(dl.Time, dl.Vdc, 0.56, 0.6);
  // the capacitor discharges for (T/2 − conduction time) each half cycle: ΔV ≈ Iavg·(1/(2f) − tc)/C
  let ncond = 0;
  dl.Time.forEach((x, i) => { if (x > 0.56 && x <= 0.6 && Math.abs(dl.Is[i]) > 1e-3) ncond++; });
  const tc = ncond * 1e-5 / 4;
  const est = avg / R * (1 / 100 - tc) / C;
  row('bridge', '穩態漣波 ≈ Iavg·(T/2 − t導通)/C', est, hi - lo, 'V', near(hi - lo, est, 0.1), '每半週導通 ' + fmt(tc) + ' s');
}

// ================= TL431 + optocoupler =================
function tl431() {
  const d = readCsv(dir + '/tl431.csv');
  const n = d.Time.length;
  let worst = 0;
  for (let i = 0; i < n; i++) worst = Math.max(worst, Math.abs(d.Vref[i] - (d.Vout[i] / 2 - 2e-6 * 5e3)));
  row('tl431', 'REF 分壓 Vout/2 − Iref·(Rup∥Rdn)', 0, worst, 'V（最大偏差）', worst < 2e-3);
  // turn-on threshold: Iled reaches 0.4 mA (Imin) near Vref = 2.495 V
  let k = d.Iled.findIndex(x => x > 0.4e-3);
  row('tl431', 'Iled 達 Imin(0.4 mA) 時的 Vref', '≈2.49', k >= 0 ? d.Vref[k] : NaN, 'V', k >= 0 && near(d.Vref[k], 2.49, 0.01));
  // regulation: above 1 mA the TL431 sets Vref within a few mV of 2.495 → Iled limited only by Rled/LED/Vka
  const dk = readCsv(dir + '/tl431_k.csv');
  const n2 = dk.Time.length - 1;
  const vka = dk.Vk[n2], vled = dk.Va[n2] - dk.Vk[n2];
  row('tl431', '飽和時 KCL：Iled = (Vout − Va)/Rled', (dk.Vout[n2] - dk.Va[n2]) / 1e3, dk.Iled[n2], 'A', near(dk.Iled[n2], (dk.Vout[n2] - dk.Va[n2]) / 1e3, 1e-6), 'Vled=' + fmt(vled) + ' V');
  row('tl431', 'TL431 飽和 Vka（資料手冊未規定）', '模型假設', vka, 'V', vka > 1.2 && vka < 2.2, 'Vka 約 1.6 V 以下輸出飽和');
  // phototransistor: below saturation Ic = CTR·IF → Vfb = 5 − 10k·Iled
  let e = 0, cnt = 0;
  for (let i = 0; i < n; i++) if (d.Iled[i] > 0.02e-3 && d.Vfb[i] > 1.0) { e = Math.max(e, Math.abs(d.Vfb[i] - (5 - 1e4 * d.Iled[i] * 1.0))); cnt++; }
  row('tl431', '光耦線性區 Vfb = 5 − Rpu·CTR·IF', 0, e, 'V（最大偏差，' + cnt + ' 點）', e < 0.15, 'CTR 隨 IF 變化（PC817 特性），容許 ±0.15 V');
  const vsat = Math.min(...d.Vfb);
  row('tl431', '光電晶體飽和 Vce(sat)', '0.1~0.3', vsat, 'V', vsat > 0.03 && vsat < 0.4);
}

// ================= Double pulse =================
function doublepulse() {
  const d = readCsv(dir + '/doublepulse.csv');
  const t = d.Time, at = x => { let i = t.findIndex(v => v >= x); return i; };
  // first pulse: di/dt = Vdc / L1
  const i1 = d.Id[at(6e-6)], i2 = d.Id[at(10e-6)];
  // series resistance: Lσ 0.5 Ω + Ls 0.02 Ω + Rds(on) 0.05 Ω → i(t) = V/Rt·(1 − e^(−t·Rt/L))
  const Rt = 0.57, ith = x => 400 / Rt * (1 - Math.exp(-x * Rt / 200e-6));
  row('doublepulse', '第一脈衝 i(6µs)（RL 充電，Rt=0.57 Ω）', ith(4e-6), i1, 'A', near(i1, ith(4e-6), 0.01));
  const slope = (i2 - i1) / 4e-6, sth = (ith(8e-6) - ith(4e-6)) / 4e-6;
  row('doublepulse', '第一脈衝平均 di/dt', sth, slope, 'A/s', near(slope, sth, 0.01));
  const ipk1 = Math.max(...d.Id.filter((_, i) => t[i] > 11e-6 && t[i] < 12.2e-6));
  row('doublepulse', '關斷前電流（RL 充電 10 µs）', ith(10e-6), ipk1, 'A', near(ipk1, ith(10e-6), 0.01));
  // turn-off overshoot and ringing
  const seg = t.map((x, i) => [x, d.Vds[i]]).filter(p => p[0] > 12e-6 && p[0] < 13.5e-6);
  const vpk = Math.max(...seg.map(p => p[1]));
  row('doublepulse', '關斷 Vds 過衝（> Vdc + Vf）', '> 401.5', vpk, 'V', vpk > 401.5 && vpk < 700);
  // ringing frequency from zero-crossings of (Vds − 400) after the peak
  const sg = seg.filter(p => p[0] > seg[seg.findIndex(q => q[1] === vpk)][0]);
  const zc = [];
  for (let i = 1; i < sg.length; i++) if ((sg[i - 1][1] - 401.5) * (sg[i][1] - 401.5) < 0) zc.push(sg[i][0]);
  const fr = zc.length >= 3 ? (zc.length - 1) / 2 / (zc[zc.length - 1] - zc[0]) : NaN;
  const Ceq = 150e-6 / 1e6; // placeholder replaced below
  const fth = 1 / (2 * Math.PI * Math.sqrt(30e-9 * (60e-12 + 40e-12)));
  row('doublepulse', '關斷振鈴頻率 1/(2π√(Lσ·Ceq))', '≈' + fmt(fth) + '（Ceq≈Coss+Cj@400V≈100p）', fr, 'Hz', fr > fth / 2.5 && fr < fth * 2.5, 'Coss、Cj 隨電壓變化，數量級比對');
  // Miller plateau: Vgs flat while Vds swings; duration ≈ Qgd / Ig, Ig = (Von − Vpl)/Rg
  // gate charge removed while Vgs sits on the plateau, until Vds reaches Vdc → should equal Qgd
  const g = t.map((x, i) => [x, d.Vgs[i], d.Vds[i]]).filter(p => p[0] >= 12e-6 && p[0] < 12.2e-6);
  const k200 = g.findIndex(p => p[2] >= 200), k400 = g.findIndex(p => p[2] >= 400);
  const vpl = g[k200][1];
  let k0 = k200;
  while (k0 > 0 && g[k0 - 1][1] < vpl + 0.6) k0--;
  let qpl = 0;
  for (let i = k0 + 1; i <= k400; i++) qpl += (g[i][1] + g[i - 1][1]) / 2 / 5 * (g[i][0] - g[i - 1][0]);
  row('doublepulse', '關斷米勒平台電荷 ∫Ig dt = Qgd', 15e-9, qpl, 'C', near(qpl, 15e-9, 0.25), 'Vpl=' + fmt(vpl) + ' V，平台 ' + fmt(g[k400][0] - g[k0][0]) + ' s，Ig≈Vgs/Rg');
  // second turn-on: reverse-recovery current peak ≈ IL + Irr
  // second turn-on: charge control, Qrr ≈ Irr²/(2·di/dt) (snappy) … Irr²/(di/dt) (soft)
  const w2 = t.map((x, i) => [x, d.Id[i]]).filter(p => p[0] > 17e-6 && p[0] < 17.2e-6);
  const ilf = ipk1 - 1.5 * 5e-6 / 200e-6; // freewheeling: decays by Vf·5µs/L1
  const kpk = w2.reduce((b, p, i) => (p[1] > w2[b][1] ? i : b), 0);
  const ka = w2.findIndex(p => p[1] > 0.2 * ilf), kb = w2.findIndex(p => p[1] > 0.8 * ilf);
  const didt = (w2[kb][1] - w2[ka][1]) / (w2[kb][0] - w2[ka][0]);
  const irr = w2[kpk][1] - ilf;
  const lo = Math.sqrt(400e-9 * didt), hi = Math.sqrt(2 * 400e-9 * didt);
  row('doublepulse', '反向恢復 Irr（由 Qrr 與實際 di/dt）', fmt(lo) + '~' + fmt(hi), irr, 'A', irr > 0.6 * lo && irr < 1.15 * hi, 'di/dt=' + fmt(didt / 1e9) + ' A/ns；資料手冊 Irr=20 A 對應 500 A/µs');
  const i3 = d.Id[at(19.9e-6)];
  const ie = ilf + (400 - Rt * 22) / 200e-6 * 2.9e-6;
  row('doublepulse', '第二脈衝末電流 ≈ IL + (Vdc−I·Rt)·2.9µs/L1', ie, i3, 'A', near(i3, ie, 0.02));
}

// ================= Supervisor =================
function supervisor() {
  const d = readCsv(dir + '/supervisor.csv');
  const t = d.Time;
  // V− charges through 100k toward VCC·301k/401k with τ = (100k∥301k)·0.1µ after the last MCU_RDY edge
  const vinf = 12 * 301 / 401, tau = (100e3 * 301e3 / 401e3) * 0.1e-6;
  // find the last discharge (V− minimum) before 50 ms
  let i0 = 0, vmin = Infinity;
  t.forEach((x, i) => { if (x > 30e-3 && x < 50e-3 && d.Vneg[i] <= vmin + 1e-6) { vmin = Math.min(vmin, d.Vneg[i]); i0 = i; } });
  const tA = t[i0] + 1e-3, tB = t[i0] + 4e-3;
  const vA = d.Vneg[t.findIndex(x => x >= tA)], vB = d.Vneg[t.findIndex(x => x >= tB)];
  const tauSim = (tB - tA) / Math.log((vinf - vA) / (vinf - vB));
  row('supervisor', 'V− 充電時間常數 (100k∥301k)·0.1µF', tau, tauSim, 's', near(tauSim, tau, 0.02));
  // trip: MCU_OK falls when V− crosses V+
  const kTrip = t.findIndex((x, i) => x > t[i0] && d.MCU_OK[i] < 1.0);
  const kCross = t.findIndex((x, i) => x > t[i0] && d.Vneg[i] > d.Vpos[i]);
  row('supervisor', 'V− 超過 V+ 到 MCU_OK 下降（tpd 1.3 µs + 上升時間）', '≈1.3e-6', t[kTrip] - t[kCross], 's', t[kTrip] - t[kCross] >= 0 && t[kTrip] - t[kCross] < 20e-6, 'V−=' + fmt(d.Vneg[kCross]) + ' V，跨越於 ' + fmt(t[kCross]) + ' s');
  // V+ divider check at 20 ms (VCC = 12 V, output high): V+ between the plain divider and the diode-clamped value
  const k20 = t.findIndex(x => x >= 20e-3);
  const vdiv = 12 * 31.6 / 131.6;
  row('supervisor', 'V+ 分壓（12V·31.6k/131.6k，含 100k 遲滯）', '≈' + fmt(vdiv), d.Vpos[k20], 'V', near(d.Vpos[k20], vdiv, 0.15));
  // power-down: DM30 clamps V+ to VCC + Vf while VCC falls
  let worst = -Infinity;
  t.forEach((x, i) => { if (x > 70e-3 && x < 80e-3) worst = Math.max(worst, d.Vpos[i] - d.Vcc[i]); });
  row('supervisor', '下電時 V+ ≤ VCC + Vf(1N4148)', '≤ ~0.7', worst, 'V', worst < 0.8);
}

const ex = { buck, rlc, bridge, tl431, doublepulse, supervisor };
for (const [k, f] of Object.entries(ex)) {
  try { f(); } catch (e) { row(k, 'ERROR', '', e.message, '', false); }
}
let bad = 0;
for (const r of report) {
  if (!r.ok) bad++;
  console.log((r.ok ? ' OK ' : 'FAIL') + ' | ' + r.ex.padEnd(11) + ' | ' + r.item + ' | 理論 ' + fmt(r.theory) + ' | 模擬 ' + fmt(r.sim) + ' ' + r.unit + (r.note ? ' | ' + r.note : ''));
}
console.log(bad ? bad + ' check(s) failed' : 'all checks passed');
process.exit(bad ? 1 : 0);
