// Example circuits with their theoretical verification.
// part: [type, name, {params}, {pin: net}]   net '0' = ground; nets joined by label name.
// checks(h) returns rows {item, th, sim, ok, note}; see verify.js for the helpers.
const PI = Math.PI;
const C = (re, im = 0) => ({ re, im });
const add = (a, b) => C(a.re + b.re, a.im + b.im);
const sub = (a, b) => C(a.re - b.re, a.im - b.im);
const mul = (a, b) => C(a.re * b.re - a.im * b.im, a.re * b.im + a.im * b.re);
const div = (a, b) => { const d = b.re * b.re + b.im * b.im; return C((a.re * b.re + a.im * b.im) / d, (a.im * b.re - a.re * b.im) / d); };
const par = (a, b) => div(mul(a, b), add(a, b));
const cabs = a => Math.hypot(a.re, a.im);
const carg = a => Math.atan2(a.im, a.re) * 180 / PI;
const jw = (w, L) => C(0, w * L);
const zc = (w, Cap) => C(0, -1 / (w * Cap));
// complex Gaussian elimination (small systems)
function csolve(A, b) {
  const n = A.length, M = A.map((r, i) => [...r, b[i]]);
  for (let c = 0; c < n; c++) {
    let p = c; for (let r = c + 1; r < n; r++) if (cabs(M[r][c]) > cabs(M[p][c])) p = r;
    [M[c], M[p]] = [M[p], M[c]];
    for (let r = 0; r < n; r++) if (r !== c) { const f = div(M[r][c], M[c][c]); for (let k = c; k <= n; k++) M[r][k] = sub(M[r][k], mul(f, M[c][k])); }
  }
  return M.map((r, i) => div(r[n], r[i]));
}
const row = (item, th, sim, ok, note) => ({ item, th, sim, ok, note });
// waveform check against a closed form: relative to the waveform's peak
function wave(h, probe, fn, t0, t1, skip, tol, label) {
  const { e, at } = h.err(probe, fn, t0, t1, skip);
  const pk = Math.max(Math.abs(h.max(probe, t0, t1)), Math.abs(h.min(probe, t0, t1)));
  return row(label || `${probe} 全波形 vs 解析解`, 0, e / pk, e / pk <= tol, `最大偏差 ${e.toExponential(3)} @ ${at.toExponential(4)} s（相對峰值）`);
}
// phasor check of the last cycles: amplitude and phase
function phasor(h, probe, f, t0, t1, A, ph, tolA, tolPh, label) {
  const r = h.fit(probe, f, t0, t1);
  return [
    row(`${label || probe} 振幅`, A, r.A, h.near(r.A, A, tolA)),
    row(`${label || probe} 相位（°）`, ph, r.ph, Math.abs(h.dphase(r.ph, ph)) <= tolPh),
  ];
}

const EX = [];

// ======================================================================= 1
EX.push({
  key: 'branches', cat: '被動電路與濾波', name: '串聯支路步階響應（RC／LC／RLC、斜坡電源）',
  text: '串聯支路的步階與斜坡響應：VSTEP 在 1 ms 加上 10 V。\nRC（100 Ω、10 µF）：i = 0.1·e^(−t/1ms)；LC（1 mH、10 µF）：i = 1 A·sin(10⁴t)；\nRLC（5 Ω、1 mH、10 µF）：欠阻尼振盪 α = 2500 s⁻¹、ωd = 9682 rad/s。VSTEP2 以 1 ms 斜坡升到 10 V 驅動同一 RC。',
  sc: { dt: '1u', tt: '6m' },
  parts: [
    ['VSTEP', 'Vs1', { V: 10, ts: '1m' }, { '+': 's1', '-': '0' }], ['IP', 'I_rc', {}, { in: 's1', out: 'a1' }], ['RC', 'Brc', { R: 100, C: '10u' }, { 1: 'a1', 2: '0' }],
    ['VSTEP', 'Vs2', { V: 10, ts: '1m' }, { '+': 's2', '-': '0' }], ['IP', 'I_lc', {}, { in: 's2', out: 'a2' }], ['LC', 'Blc', { L: '1m', C: '10u' }, { 1: 'a2', 2: '0' }],
    ['VSTEP', 'Vs3', { V: 10, ts: '1m' }, { '+': 's3', '-': '0' }], ['IP', 'I_rlc', {}, { in: 's3', out: 'a3' }], ['RLC', 'Brlc', { R: 5, L: '1m', C: '10u' }, { 1: 'a3', 2: '0' }],
    ['VSTEP2', 'Vs4', { V1: 0, V2: 10, ts: '1m', tr: '1m' }, { '+': 's4', '-': '0' }], ['IP', 'I_ramp', {}, { in: 's4', out: 'a4' }], ['RC', 'Brc2', { R: 100, C: '10u' }, { 1: 'a4', 2: '0' }],
  ],
  screens: [['I_rc', 'I_ramp'], ['I_lc'], ['I_rlc']],
  checks: h => {
    const u = t => t - 1e-3;
    const al = 2500, wd = Math.sqrt(1e8 - al * al);
    const ramp = t => { const s = u(t); if (s <= 0) return 0; if (s <= 1e-3) return 0.1 * (1 - Math.exp(-s / 1e-3)); return 0.1 * (1 - Math.exp(-1)) * Math.exp(-(s - 1e-3) / 1e-3); };
    return [
      wave(h, 'I_rc', t => (u(t) > 0 ? 0.1 * Math.exp(-u(t) / 1e-3) : 0), 0, 6e-3, [1e-3], 1e-4),
      wave(h, 'I_lc', t => (u(t) > 0 ? Math.sin(1e4 * u(t)) : 0), 0, 6e-3, [1e-3], 1e-3),
      wave(h, 'I_rlc', t => (u(t) > 0 ? 10 / (1e-3 * wd) * Math.exp(-al * u(t)) * Math.sin(wd * u(t)) : 0), 0, 6e-3, [1e-3], 1e-3),
      wave(h, 'I_ramp', ramp, 0, 6e-3, [1e-3, 2e-3], 1e-4),
    ];
  },
});

// ======================================================================= 2
{
  const f = 50, w = 2 * PI * f, Vm = 380 * Math.SQRT2 / Math.sqrt(3);
  const Zrc = add(C(20), zc(w, 200e-6)), Zrlc = add(add(C(10), jw(w, 20e-3)), zc(w, 500e-6));
  const Z = add(add(C(1.5), jw(w, 10e-3)), par(Zrc, Zrlc));
  const Im = Vm / cabs(Z), phi = -carg(Z);
  EX.push({
    key: 'threephase_load', cat: '三相系統', name: '三相 RL 線路與 RC／RLC 負載（交流電表、瓦特計）',
    text: `三相 380 V／50 Hz 經三相 R、RL 線路供電給 Y 接 RC 與 RLC 負載（中性點浮接，平衡）。\n每相阻抗 Z = 1.5 Ω + j·ω·10 mH + (20 Ω − j/ωC₁) ∥ (10 Ω + jω·20 mH − j/ωC₂)。\n交流電流表、交流電壓表、瓦特計讀值與相量計算比較：Ia,rms = ${(Im / Math.SQRT2).toFixed(3)} A、P = ${(Im * Im / 2 * Z.re).toFixed(1)} W。`,
    sc: { dt: '20u', tt: '0.4' },
    parts: [
      ['VSIN3', 'Vs', { V: 380, f: 50 }, { n: '0', a: 'a', b: 'b', c: 'c' }],
      ['AACM', 'Ia_rms', { f: 50, fc: 10 }, { in: 'a', out: 'a1' }],
      ['WATT', 'Pa', { fc: 10 }, { 'I+': 'a1', 'I-': 'a2', 'V-': '0' }],
      ['IP', 'Ia', {}, { in: 'a2', out: 'a3' }],
      ['VACM', 'Va_rms', { f: 50, fc: 10 }, { '+': 'a', '-': '0' }],
      ['R3', 'Rline', { R: 1 }, { A: 'a3', B: 'b', C: 'c', a: 'a4', b: 'b4', c: 'c4' }],
      ['RL3', 'Lline', { R: 0.5, L: '10m' }, { A: 'a4', B: 'b4', C: 'c4', a: 'a5', b: 'b5', c: 'c5' }],
      ['RC3', 'Zrc', { R: 20, C: '200u' }, { A: 'a5', B: 'b5', C: 'c5', a: 's', b: 's', c: 's' }],
      ['RLC3', 'Zrlc', { R: 10, L: '20m', C: '500u' }, { A: 'a5', B: 'b5', C: 'c5', a: 'u', b: 'u', c: 'u' }],
      ['VP', 'Vs_star', {}, { n: 's' }],
    ],
    screens: [['Ia'], ['Ia_rms', 'Va_rms'], ['Pa'], ['Vs_star']],
    checks: h => [
      ...phasor(h, 'Ia', f, 0.36, 0.4, Im, phi, 1e-4, 0.01, 'A 相電流'),
      row('交流電流表 Ia,rms', Im / Math.SQRT2, h.last('Ia_rms'), h.near(h.last('Ia_rms'), Im / Math.SQRT2, 1e-4)),
      row('交流電壓表 Va,rms', 380 / Math.sqrt(3), h.last('Va_rms'), h.near(h.last('Va_rms'), 380 / Math.sqrt(3), 1e-5)),
      row('瓦特計 Pa（讀值最後一週期平均）', Im * Im / 2 * Z.re, h.avg('Pa', 0.38, 0.4), h.near(h.avg('Pa', 0.38, 0.4), Im * Im / 2 * Z.re, 1e-3), '瓦特計為 v·i 的低通濾波值，含 100 Hz 殘留漣波'),
      row('Y 接負載中性點電壓（平衡應為 0）', 0, h.max('Vs_star', 0.3, 0.4) - h.min('Vs_star', 0.3, 0.4), h.max('Vs_star', 0.3, 0.4) - h.min('Vs_star', 0.3, 0.4) < 1e-6 * Vm),
    ],
  });
}

// ======================================================================= 3
{
  // NONI operating point: (5 − v)/1000 = 1e-14·(e^(40v) − 1)
  let v = 0.6;
  for (let i = 0; i < 100; i++) { const f = (5 - v) / 1000 - 1e-14 * (Math.exp(40 * v) - 1), df = -1e-3 - 40e-14 * Math.exp(40 * v); v -= f / df; }
  const vd = v;
  EX.push({
    key: 'nonlinear', cat: '被動電路與濾波', name: '非線性元件（i=f(v)、v=f(i)、飽和電感、可變電阻、dv/dt）',
    text: '非線性元件：i = f(v) 的指數二極體（1 kΩ、5 V）、v = 10i + 0.1i³（2 A）、\n飽和電感（磁通鏈分段線性，電流以 1000 A/s 斜坡上升：v = dλ/dt = 10、2、1 V）、\n可變電阻抽頭 30 %（10 V → 7 V）、功率電路微分方塊（1 kHz 正弦）。',
    sc: { dt: '1u', tt: '6m' },
    parts: [
      ['VDC', 'V1', { V: 5 }, { '+': 'v1', '-': '0' }], ['R', 'R1', { R: '1k' }, { 1: 'v1', 2: 'd' }], ['NONI', 'Dn', {}, { 1: 'd', 2: '0' }], ['VP', 'Vd', {}, { n: 'd' }],
      ['IDC', 'I2', { V: 2 }, { '+': 'nv', '-': '0' }], ['NONV', 'Nv', {}, { 1: 'nv', 2: '0' }], ['VP', 'Vnv', {}, { n: 'nv' }],
      ['IPWL', 'I3', { vals: '0 5 5', times: '0 5m 1' }, { '+': 'l', '-': '0' }], ['LSAT', 'Ls', { tab: '1 10m 2 6m 5 3m' }, { 1: 'l', 2: '0' }], ['VP', 'Vls', {}, { n: 'l' }],
      ['VDC', 'V4', { V: 10 }, { '+': 'k', '-': '0' }], ['RHEO', 'Rh', { R: 10, tap: 0.3 }, { k: 'k', m: '0', t: 'tp' }], ['VP', 'Vtap', {}, { n: 'tp' }],
      ['VSIN', 'V5', { Vm: 1, f: '1k' }, { '+': 's5', '-': '0' }], ['DVDT', 'Dv', {}, { in: 's5', out: 'dv' }], ['VP', 'Vdv', {}, { n: 'dv' }],
    ],
    screens: [['Vd', 'Vtap'], ['Vnv'], ['Vls'], ['Vdv']],
    checks: h => {
      const w = 2 * PI * 1e3, dt = 1e-6;
      return [
        row('i = f(v) 工作點電壓', vd, h.last('Vd'), h.near(h.last('Vd'), vd, 1e-6)),
        row('v = f(i) @ 2 A', 20.8, h.last('Vnv'), h.near(h.last('Vnv'), 20.8, 1e-9)),
        row('飽和電感 0~1 A 段 v', 10, h.at('Vls', 0.5e-3), h.near(h.at('Vls', 0.5e-3), 10, 1e-6)),
        row('飽和電感 1~2 A 段 v', 2, h.at('Vls', 1.5e-3), h.near(h.at('Vls', 1.5e-3), 2, 1e-6)),
        row('飽和電感 2~5 A 段 v', 1, h.at('Vls', 3.5e-3), h.near(h.at('Vls', 3.5e-3), 1, 1e-6)),
        row('飽和電感 電流停止上升後 v', 0, h.at('Vls', 5.5e-3), Math.abs(h.at('Vls', 5.5e-3)) < 1e-9),
        row('可變電阻抽頭電壓', 7, h.last('Vtap'), h.near(h.last('Vtap'), 7, 1e-9)),
        wave(h, 'Vdv', t => w * Math.cos(w * t), 2 * dt, 6e-3, [], 4e-3, 'dv/dt 方塊 vs 解析導數（後向差分誤差 ≈ ωΔt/2）'),
      ];
    },
  });
}

// ======================================================================= 4
{
  const f50 = 50, w = 2 * PI * f50, w1k = 2 * PI * 1e3;
  // TF1 referred to the primary: Vp → rp + jωlp → Lm ∥ (rs + jωls + R'/1)
  const Zs = add(add(C(0.1), jw(w, 1e-3)), C(100 / 4));
  const Zm = jw(w, 0.5);
  const Zin = add(add(C(0.1), jw(w, 1e-3)), par(Zm, Zs));
  const Vm = div(par(Zm, Zs), Zin); // node voltage per volt of source
  const Vsec = mul(div(C(25), Zs), Vm); // across R' (per volt)
  const vo2 = mul(Vsec, C(2)); // actual secondary
  // MUT2: [R1 + jωL11, jωL12; jωL12, RL + jωL22]·[i1; i2'] = [V; 0] with i2' = −i2 (load current)
  const L = [[1e-3, 0.9e-3], [0.9e-3, 1e-3]];
  const m2 = csolve([[add(C(1), jw(w1k, L[0][0])), jw(w1k, L[0][1])], [jw(w1k, L[1][0]), add(C(10), jw(w1k, L[1][1]))]], [C(10), C(0)]);
  const vo4 = mul(m2[1], C(-10)); // v2 = −R·i2'  → with i2' entering the dot: v(o4) = −10·i2'
  const L3 = [[1e-3, 0.9e-3, 0.8e-3], [0.9e-3, 1e-3, 0.85e-3], [0.8e-3, 0.85e-3, 1e-3]];
  const R3 = [1, 10, 20];
  const m3 = csolve(L3.map((r, i) => r.map((x, j) => add(jw(w1k, x), C(i === j ? R3[i] : 0)))), [C(10), C(0), C(0)]);
  EX.push({
    key: 'transformers', cat: '變壓器與磁性元件', name: '單相變壓器與耦合電感（理想、含漏感、三繞組、2／3 繞組耦合）',
    text: '理想變壓器 2:1；單相變壓器（Rp=Rs=0.1 Ω、Lp=Ls=1 mH、Lm=0.5 H、1:2、負載 100 Ω）；\n三繞組變壓器 1:1:0.5；兩繞組與三繞組耦合電感（1 kHz、L12 = 0.9 mH 等）。\n二次側電壓的振幅與相位與相量計算比較。',
    sc: { dt: '5u', tt: '0.1' },
    parts: [
      ['VSIN', 'Vt1', { Vm: 100, f: 50 }, { '+': 'p1', '-': '0' }], ['TFI', 'T1', { np: 2, ns: 1 }, { 'p+': 'p1', 'p-': '0', 's+': 'o1', 's-': '0' }], ['R', 'Rl1', { R: 10 }, { 1: 'o1', 2: '0' }], ['VP', 'Vo1', {}, { n: 'o1' }],
      ['VSIN', 'Vt2', { Vm: 100, f: 50 }, { '+': 'p2', '-': '0' }], ['TF1', 'T2', { rp: 0.1, rs: 0.1, lp: '1m', ls: '1m', lm: 0.5, np: 1, ns: 2 }, { 'p+': 'p2', 'p-': '0', 's+': 'o2', 's-': '0' }], ['R', 'Rl2', { R: 100 }, { 1: 'o2', 2: '0' }], ['VP', 'Vo2', {}, { n: 'o2' }],
      ['VSIN', 'Vt3', { Vm: 100, f: 50 }, { '+': 'p3', '-': '0' }], ['TF3W', 'T3', { lm: 1, np: 1, ns: 1, nt: 0.5 }, { 'p+': 'p3', 'p-': '0', 's+': 'o3s', 's-': '0', 't+': 'o3t', 't-': '0' }], ['R', 'Rl3s', { R: 10 }, { 1: 'o3s', 2: '0' }], ['R', 'Rl3t', { R: 5 }, { 1: 'o3t', 2: '0' }], ['VP', 'Vo3s', {}, { n: 'o3s' }], ['VP', 'Vo3t', {}, { n: 'o3t' }],
      ['VSIN', 'Vt4', { Vm: 10, f: '1k' }, { '+': 'p4', '-': '0' }], ['R', 'Rs4', { R: 1 }, { 1: 'p4', 2: 'm4' }], ['MUT2', 'M2', { l11: '1m', l12: '0.9m', l22: '1m' }, { '1+': 'm4', '1-': '0', '2+': 'o4', '2-': '0' }], ['R', 'Rl4', { R: 10 }, { 1: 'o4', 2: '0' }], ['VP', 'Vo4', {}, { n: 'o4' }],
      ['VSIN', 'Vt5', { Vm: 10, f: '1k' }, { '+': 'p5', '-': '0' }], ['R', 'Rs5', { R: 1 }, { 1: 'p5', 2: 'm5' }], ['MUT3', 'M3', { l11: '1m', l12: '0.9m', l13: '0.8m', l22: '1m', l23: '0.85m', l33: '1m' }, { '1+': 'm5', '1-': '0', '2+': 'o5b', '2-': '0', '3+': 'o5c', '3-': '0' }], ['R', 'Rl5b', { R: 10 }, { 1: 'o5b', 2: '0' }], ['R', 'Rl5c', { R: 20 }, { 1: 'o5c', 2: '0' }], ['VP', 'Vo5b', {}, { n: 'o5b' }], ['VP', 'Vo5c', {}, { n: 'o5c' }],
    ],
    screens: [['Vo1', 'Vo2'], ['Vo3s', 'Vo3t'], ['Vo4'], ['Vo5b', 'Vo5c']],
    checks: h => [
      ...phasor(h, 'Vo1', 50, 0.06, 0.1, 50, 0, 1e-6, 1e-3, '理想變壓器 Vs'),
      ...phasor(h, 'Vo2', 50, 0.06, 0.1, 100 * cabs(vo2), carg(vo2), 1e-4, 0.01, '單相變壓器 Vs'),
      ...phasor(h, 'Vo3s', 50, 0.06, 0.1, 100, 0, 1e-6, 1e-3, '三繞組 s'),
      ...phasor(h, 'Vo3t', 50, 0.06, 0.1, 50, 0, 1e-6, 1e-3, '三繞組 t'),
      ...phasor(h, 'Vo4', 1e3, 0.095, 0.1, cabs(vo4), carg(vo4), 1e-3, 0.05, '耦合電感（2）V2'),
      ...phasor(h, 'Vo5b', 1e3, 0.095, 0.1, 10 * cabs(m3[1]), carg(mul(m3[1], C(-1))), 1e-3, 0.05, '耦合電感（3）V2'),
      ...phasor(h, 'Vo5c', 1e3, 0.095, 0.1, 20 * cabs(m3[2]), carg(mul(m3[2], C(-1))), 1e-3, 0.05, '耦合電感（3）V3'),
    ],
  });
}

// ======================================================================= 5
EX.push({
  key: 'rectifier3', cat: '電力轉換', name: '三相變壓器＋三相二極體橋（Y/Y、Y/Δ、直流表）',
  text: '400 V／50 Hz 經 Y/Y 變壓器（1:0.5）與三相二極體橋供電給 10 Ω：\n理想六脈波平均 Vdc = (3√2/π)·200 = 270.1 V、Idc = 27.01 A（直流電壓表、直流電流表）。\nY/Δ 變壓器（1:1）Δ 側 A 相繞組接在 a–b：Vab 與 Van 同相、峰值 326.6 V。',
  sc: { dt: '10u', tt: '0.3' },
  parts: [
    ['VSIN3', 'V1', { V: 400, f: 50 }, { n: '0', a: 'a', b: 'b', c: 'c' }],
    ['TF3YY', 'T1', { np: 1, ns: 0.5, lm: 10 }, { A: 'a', B: 'b', C: 'c', N: '0', a: 'x', b: 'y', c: 'z', n: '0' }],
    ['BDIODE3', 'B1', {}, { A: 'x', B: 'y', C: 'z', 'DC+': 'p', 'DC-': 'q' }],
    ['ADCM', 'Idc', { fc: 10 }, { in: 'p', out: 'p2' }], ['R', 'Rl', { R: 10 }, { 1: 'p2', 2: 'q' }],
    ['VDCM', 'Vdc_avg', { fc: 10 }, { '+': 'p2', '-': 'q' }], ['VP2', 'Vdc', {}, { '+': 'p2', '-': 'q' }],
    ['VSIN3', 'V2', { V: 400, f: 50 }, { n: '0', a: 'a2', b: 'b2', c: 'c2' }],
    ['TF3YD', 'T2', { np: 1, ns: 1, lm: 10, rs: '1m' }, { A: 'a2', B: 'b2', C: 'c2', N: '0', a: 'ya', b: 'yb', c: 'yc' }],
    ['R3', 'Rl3', { R: 50 }, { A: 'ya', B: 'yb', C: 'yc', a: '0', b: '0', c: '0' }],
    ['VP2', 'Vab', {}, { '+': 'ya', '-': 'yb' }], ['VP', 'Van', {}, { n: 'a2' }],
  ],
  screens: [['Vdc', 'Vdc_avg'], ['Idc'], ['Vab', 'Van']],
  checks: h => {
    const vd = 3 * Math.SQRT2 / PI * 200, vm = 400 * Math.SQRT2 / Math.sqrt(3);
    return [
      row('Vdc 平均（最後一週期）', vd, h.avg('Vdc', 0.28, 0.3), h.near(h.avg('Vdc', 0.28, 0.3), vd, 2e-3), '變壓器激磁電流使輸出略低'),
      row('Vdc 漣波峰對峰（六脈波 = √2·VLL·(1 − cos30°)）', Math.SQRT2 * 200 * (1 - Math.cos(PI / 6)), h.max('Vdc', 0.28, 0.3) - h.min('Vdc', 0.28, 0.3), h.near(h.max('Vdc', 0.28, 0.3) - h.min('Vdc', 0.28, 0.3), Math.SQRT2 * 200 * (1 - Math.cos(PI / 6)), 5e-3)),
      row('直流電壓表讀值（最後一週期平均）', vd, h.avg('Vdc_avg', 0.28, 0.3), h.near(h.avg('Vdc_avg', 0.28, 0.3), vd, 2e-3)),
      row('直流電流表讀值（最後一週期平均）', vd / 10, h.avg('Idc', 0.28, 0.3), h.near(h.avg('Idc', 0.28, 0.3), vd / 10, 2e-3)),
      ...phasor(h, 'Vab', 50, 0.26, 0.3, vm, 0, 1e-3, 0.1, 'Y/Δ 二次側 Vab'),
      ...phasor(h, 'Van', 50, 0.26, 0.3, vm, 0, 1e-9, 1e-6, '一次側 Van'),
    ];
  },
});

// ======================================================================= 6
{
  const Vm = 311, w = 2 * PI * 50;
  EX.push({
    key: 'phase_control', cat: '電力轉換', name: '單相相控：閘流體橋、TRIAC 調光、SCR 半波、DIAC 觸發',
    text: '單相相控（311 V 峰值、50 Hz、電阻負載）：\n閘流體全橋 α = 60°（觸發角控制器）：Vdc = (Vm/π)(1 + cos α) = 148.5 V；\nTRIAC α = 90°：Vrms = (Vm/√2)·√(1 − α/π + sin2α/2π) = 155.5 V；SCR 半波 α = 30°：Vavg = Vm(1 + cos α)/2π = 92.4 V；\nDIAC（30 V 轉折、5 V 導通）串 1.1 kΩ：|v| 超過 30 V 時導通，電流 = (v − 5)/1.1k。',
    sc: { dt: '5u', tt: '0.1' },
    parts: [
      ['VSIN', 'Vac1', { Vm, f: 50 }, { '+': 'ac1', '-': '0' }], ['BTHY1', 'Bt', {}, { 'A+': 'ac1', 'A-': '0', 'DC+': 'd1', 'DC-': 'd1n', Ct: 'g1' }],
      ['R', 'Rl1', { R: 10 }, { 1: 'd1', 2: 'd1n' }], ['VP2', 'Vbridge', {}, { '+': 'd1', '-': 'd1n' }],
      ['ACTRL', 'Act', { f: 50, pw: 90 }, { alpha: 'al1', sync: 'sy', enable: 'en', gate: 'g1' }],
      ['CONST', 'Alpha', { v: 60 }, { out: 'al1' }], ['CONST', 'Enable', { v: 1 }, { out: 'en' }], ['VSQU', 'Sync', { Vpp: 1, f: 50, D: 0.5 }, { '+': 'sy', '-': '0' }],
      ['VSIN', 'Vac2', { Vm, f: 50 }, { '+': 'ac2', '-': '0' }], ['TRIAC', 'Tr', {}, { 1: 'ac2', 2: 't2', Gate: 'gt' }], ['R', 'Rl2', { R: 10 }, { 1: 't2', 2: '0' }], ['VP', 'Vtriac', {}, { n: 't2' }],
      ['GATING', 'Gt', { f: 50, n: 4, pts: '90 180 270 360' }, { gate: 'gt' }],
      ['VSIN', 'Vac3', { Vm, f: 50 }, { '+': 'ac3', '-': '0' }], ['THY', 'Scr', {}, { A: 'ac3', K: 'k3', Gate: 'gs' }], ['R', 'Rl3', { R: 10 }, { 1: 'k3', 2: '0' }], ['VP', 'Vscr', {}, { n: 'k3' }],
      ['GATING', 'Gs', { f: 50, n: 2, pts: '30 90' }, { gate: 'gs' }],
      ['VSIN', 'Vac4', { Vm: 100, f: 50 }, { '+': 'ac4', '-': '0' }], ['R', 'Rd1', { R: '1k' }, { 1: 'ac4', 2: 'd4' }], ['DIAC', 'Dc', { vbo: 30, vbb: 5 }, { 1: 'd4', 2: 'd5' }],
      ['IP', 'Idiac', {}, { in: 'd5', out: 'd6' }], ['R', 'Rd2', { R: 100 }, { 1: 'd6', 2: '0' }],
    ],
    screens: [['Vbridge'], ['Vtriac'], ['Vscr'], ['Idiac']],
    checks: h => {
      const a1 = PI / 3, a2 = PI / 2, a3 = PI / 6;
      const ton = Math.asin(30 / 100) / w, toff = (PI - Math.asin((5 + 5e-4 * 1100) / 100)) / w; // 0.5 mA threshold
      const on = h.cross('Idiac', 5e-4, 1), off = h.cross('Idiac', 5e-4, -1);
      return [
        row('閘流體橋 Vdc 平均', Vm / PI * (1 + Math.cos(a1)), h.avg('Vbridge', 0.08, 0.1), h.near(h.avg('Vbridge', 0.08, 0.1), Vm / PI * (1 + Math.cos(a1)), 1e-3), '平均以取樣點梯形積分，切換跳變造成約 2e-4 的量測誤差'),
        row('TRIAC 輸出 Vrms', Vm / Math.SQRT2 * Math.sqrt(1 - a2 / PI + Math.sin(2 * a2) / (2 * PI)), h.rms('Vtriac', 0.08, 0.1), h.near(h.rms('Vtriac', 0.08, 0.1), Vm / Math.SQRT2 * Math.sqrt(1 - a2 / PI + Math.sin(2 * a2) / (2 * PI)), 1e-3)),
        row('SCR 半波 Vavg', Vm / (2 * PI) * (1 + Math.cos(a3)), h.avg('Vscr', 0.08, 0.1), h.near(h.avg('Vscr', 0.08, 0.1), Vm / (2 * PI) * (1 + Math.cos(a3)), 1e-3)),
        row('DIAC 第一次導通時刻', ton, on[0], Math.abs(on[0] - ton) < 10e-6),
        row('DIAC 第一次關斷時刻（電流降到 0.5 mA）', toff, off[0], Math.abs(off[0] - toff) < 10e-6),
        row('DIAC 導通電流（t = 5 ms）', (100 - 5) / 1100, h.at('Idiac', 5e-3), h.near(h.at('Idiac', 5e-3), 95 / 1100, 1e-4)),
      ];
    },
  });
}

// further examples live in defs_b.js, defs_c.js, ... (each exports (EX, H) => void)
const H = { PI, C, add, sub, mul, div, par, cabs, carg, jw, zc, csolve, row, wave, phasor };
for (const f of ['defs_b.js', 'defs_c.js', 'defs_d.js', 'defs_e.js']) {
  const p = require('path').join(__dirname, f);
  if (require('fs').existsSync(p)) require(p)(EX, H);
}
module.exports = EX;
