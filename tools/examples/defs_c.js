// Examples 13–18: controlled sources (linear / nonlinear), source catalogue (voltage / current),
// basic control blocks, analog filters.
module.exports = (EX, H) => {
  const { PI, C, add, sub, mul, div, cabs, carg, jw, row, wave, phasor } = H;
  const w1k = 2 * PI * 1e3;
  const S = t => Math.sin(w1k * t);

  // =================================================================== 13
  EX.push({
    key: 'ctrl_sources', cat: '電源與受控源', name: '線性受控源（CCVS、VCVS、CCCS、VCCS 及可變增益型）',
    text: '控制支路：10 V／10 Ω → 1 A；串接式控制電流 3 V／3 Ω → 1 A；控制電壓 = 1 kHz、1 V 正弦（輸入端接 1 kΩ 負載 → 功率節點，當步同解）。\nCCVS k=5 → 5 V；串接 CCVS k=2 → 2 V；VCVS k=3 → 3·sin；可變增益 VCVS (0.5·2 V)·vin → vin；\nCCCS k=0.5 → 0.5 A·10 Ω = 5 V；串接 CCCS k=0.1 → 1 V；VCCS k=0.01·vin·100 Ω → vin；可變增益 VCCS (0.25·4 V)·vin·1 Ω → vin。',
    sc: { dt: '1u', tt: '3m' },
    parts: [
      ['VDC', 'V10', { V: 10 }, { '+': 'n1', '-': '0' }], ['R', 'Rc', { R: 10 }, { 1: 'n1', 2: '0' }],
      ['VDC', 'V3', { V: 3 }, { '+': 'n3', '-': '0' }],
      ['VSIN', 'Vs', { Vm: 1, f: '1k' }, { '+': 'vs', '-': '0' }], ['VDC', 'V2', { V: 2 }, { '+': 'v2', '-': '0' }], ['VDC', 'V4', { V: 4 }, { '+': 'v4', '-': '0' }],
      ['R', 'Rvs', { R: '1k' }, { 1: 'vs', 2: '0' }], ['R', 'Rv2', { R: '1k' }, { 1: 'v2', 2: '0' }], ['R', 'Rv4', { R: '1k' }, { 1: 'v4', 2: '0' }],
      ['VCCVS', 'E1', { k: 5 }, { '+': 'o1', '-': '0', 'c+': 'n1', 'c-': '0' }], ['R', 'R1', { R: 100 }, { 1: 'o1', 2: '0' }], ['VP', 'V_ccvs', {}, { n: 'o1' }],
      ['VCCVS1', 'E2', { k: 2 }, { '+': 'o2', '-': '0', in: 'n3', out: 'n3b' }], ['R', 'R3c', { R: 3 }, { 1: 'n3c', 2: '0' }], ['R', 'R2', { R: 100 }, { 1: 'o2', 2: '0' }], ['VP', 'V_ccvs1', {}, { n: 'o2' }],
      ['VVCVS', 'E3', { k: 3 }, { '+': 'o3', '-': '0', 'in+': 'vs', 'in-': '0' }], ['R', 'R3', { R: 100 }, { 1: 'o3', 2: '0' }], ['VP', 'V_vcvs', {}, { n: 'o3' }],
      ['VVCVSV', 'E4', { k: 0.5 }, { '+': 'o4', '-': '0', vin1: 'vs', vin2: 'v2' }], ['R', 'R4', { R: 100 }, { 1: 'o4', 2: '0' }], ['VP', 'V_vcvsv', {}, { n: 'o4' }],
      ['ICCCS', 'F1', { k: 0.5 }, { '+': 'o5', '-': '0', 'c+': 'n1', 'c-': '0' }], ['R', 'R5', { R: 10 }, { 1: 'o5', 2: '0' }], ['VP', 'V_cccs', {}, { n: 'o5' }],
      ['ICCCS1', 'F2', { k: 0.1 }, { '+': 'o6', '-': '0', in: 'n3b', out: 'n3c' }], ['R', 'R6', { R: 10 }, { 1: 'o6', 2: '0' }], ['VP', 'V_cccs1', {}, { n: 'o6' }],
      ['IVCCS', 'G1', { k: 0.01 }, { '+': 'o7', '-': '0', 'in+': 'vs', 'in-': '0' }], ['R', 'R7', { R: 100 }, { 1: 'o7', 2: '0' }], ['VP', 'V_vccs', {}, { n: 'o7' }],
      ['IVCCSV', 'G2', { k: 0.25 }, { '+': 'o8', '-': '0', vin1: 'vs', vin2: 'v4' }], ['R', 'R8', { R: 1 }, { 1: 'o8', 2: '0' }], ['VP', 'V_vccsv', {}, { n: 'o8' }],
    ],
    screens: [['V_ccvs', 'V_ccvs1', 'V_cccs', 'V_cccs1'], ['V_vcvs', 'V_vcvsv', 'V_vccs', 'V_vccsv']],
    checks: h => [
      row('CCVS：k·Ic = 5·1 A', 5, h.last('V_ccvs'), h.near(h.last('V_ccvs'), 5, 1e-9)),
      row('串接 CCVS：k·I = 2·1 A', 2, h.last('V_ccvs1'), h.near(h.last('V_ccvs1'), 2, 1e-6)),
      wave(h, 'V_vcvs', t => 3 * S(t), 0, 3e-3, [], 1e-10, 'VCVS = 3·vin'),
      wave(h, 'V_vcvsv', t => S(t), 0, 3e-3, [], 1e-10, '可變增益 VCVS = (0.5·2)·vin'),
      row('CCCS：0.5·1 A·10 Ω', 5, h.last('V_cccs'), h.near(h.last('V_cccs'), 5, 1e-9)),
      row('串接 CCCS：0.1·1 A·10 Ω', 1, h.last('V_cccs1'), h.near(h.last('V_cccs1'), 1, 1e-6)),
      wave(h, 'V_vccs', t => S(t), 0, 3e-3, [], 1e-10, 'VCCS = 0.01·vin·100 Ω'),
      wave(h, 'V_vccsv', t => S(t), 0, 3e-3, [], 1e-10, '可變增益 VCCS = (0.25·4)·vin·1 Ω'),
    ],
  });

  // =================================================================== 14
  {
    const A = t => 2 + Math.sin(w1k * t);
    EX.push({
      key: 'nonlinear_sources', cat: '電源與受控源', name: '非線性受控源（乘、除、平方根、冪次）',
      text: 'vA = 2 + sin(2π·1k·t)、vB = 2 V（輸入接 1 kΩ → 功率節點，牛頓法當步求解）。\n電壓源：0.5·vA·vB、vA/vB、2·√vA、sign(vA)·|vA|²；\n電流源（注入 100 Ω）：0.01·vA·vB、0.01·vA/vB、0.01·√vA → 電壓 = vA·vB、vA/vB、√vA。',
      sc: { dt: '1u', tt: '3m' },
      parts: [
        ['VSIN', 'Va', { Vm: 1, f: '1k', off: 2 }, { '+': 'a', '-': '0' }], ['VDC', 'Vb', { V: 2 }, { '+': 'b', '-': '0' }],
        ['R', 'Rla', { R: '1k' }, { 1: 'a', 2: '0' }], ['R', 'Rlb', { R: '1k' }, { 1: 'b', 2: '0' }],
        ['VNONM', 'N1', { k: 0.5 }, { '+': 'o1', '-': '0', vin1: 'a', vin2: 'b' }], ['R', 'R1', { R: 100 }, { 1: 'o1', 2: '0' }], ['VP', 'V_mul', {}, { n: 'o1' }],
        ['VNOND', 'N2', { k: 1 }, { '+': 'o2', '-': '0', vin1: 'a', vin2: 'b' }], ['R', 'R2', { R: 100 }, { 1: 'o2', 2: '0' }], ['VP', 'V_div', {}, { n: 'o2' }],
        ['VNONSQ', 'N3', { k: 2 }, { '+': 'o3', '-': '0', vin1: 'a' }], ['R', 'R3', { R: 100 }, { 1: 'o3', 2: '0' }], ['VP', 'V_sqrt', {}, { n: 'o3' }],
        ['VPOWERS', 'N4', { k: 1, k1: 1, k2: 2 }, { '+': 'o4', '-': '0', vin: 'a' }], ['R', 'R4', { R: 100 }, { 1: 'o4', 2: '0' }], ['VP', 'V_pow', {}, { n: 'o4' }],
        ['INONM', 'N5', { k: 0.01 }, { '+': 'o5', '-': '0', vin1: 'a', vin2: 'b' }], ['R', 'R5', { R: 100 }, { 1: 'o5', 2: '0' }], ['VP', 'V_imul', {}, { n: 'o5' }],
        ['INOND', 'N6', { k: 0.01 }, { '+': 'o6', '-': '0', vin1: 'a', vin2: 'b' }], ['R', 'R6', { R: 100 }, { 1: 'o6', 2: '0' }], ['VP', 'V_idiv', {}, { n: 'o6' }],
        ['INONSQ', 'N7', { k: 0.01 }, { '+': 'o7', '-': '0', vin1: 'a' }], ['R', 'R7', { R: 100 }, { 1: 'o7', 2: '0' }], ['VP', 'V_isqrt', {}, { n: 'o7' }],
      ],
      screens: [['V_mul', 'V_imul'], ['V_div', 'V_idiv'], ['V_sqrt', 'V_isqrt'], ['V_pow']],
      checks: h => [
        wave(h, 'V_mul', t => 0.5 * A(t) * 2, 0, 3e-3, [], 1e-10, '電壓源 0.5·vA·vB'),
        wave(h, 'V_div', t => A(t) / 2, 0, 3e-3, [], 1e-10, '電壓源 vA/vB'),
        wave(h, 'V_sqrt', t => 2 * Math.sqrt(A(t)), 0, 3e-3, [], 1e-10, '電壓源 2·√vA'),
        wave(h, 'V_pow', t => A(t) ** 2, 0, 3e-3, [], 1e-10, '電壓源 |vA|²'),
        wave(h, 'V_imul', t => A(t) * 2, 0, 3e-3, [], 1e-10, '電流源 0.01·vA·vB·100 Ω'),
        wave(h, 'V_idiv', t => A(t) / 2, 0, 3e-3, [], 1e-10, '電流源 0.01·vA/vB·100 Ω'),
        wave(h, 'V_isqrt', t => Math.sqrt(A(t)), 0, 3e-3, [], 1e-10, '電流源 0.01·√vA·100 Ω'),
      ],
    });
  }

  // =================================================================== 15
  {
    const frac = x => x - Math.floor(x);
    const pwl = (pts, t) => { if (t <= pts[0][0]) return pts[0][1]; for (let i = 1; i < pts.length; i++) if (t <= pts[i][0]) return pts[i - 1][1] + (pts[i][1] - pts[i - 1][1]) * (t - pts[i - 1][0]) / (pts[i][0] - pts[i - 1][0]); return pts[pts.length - 1][1]; };
    EX.push({
      key: 'voltage_sources', cat: '電源與受控源', name: '電壓源總覽（鋸齒、成對 PWL、電池、亂數、數學式）',
      text: '鋸齒波 1 kHz（偏移 1 V、峰值 2 V）、成對 PWL (0,0)(1m,1)(2m,1)(3m,−1)、12 V 電池、\n亂數源（0 ~ 1 V，種子 3，每步一個值）、數學式 sin(2π·500t) + 0.3·cos(2π·1500t)。',
      sc: { dt: '1u', tt: '4m' },
      parts: [
        ['VSAW', 'Vsaw', { Vpk: 2, f: '1k', off: 1 }, { '+': 'a', '-': '0' }], ['R', 'Ra', { R: '1k' }, { 1: 'a', 2: '0' }], ['VP', 'V_saw', {}, { n: 'a' }],
        ['VPWL2', 'Vpwl', { pairs: '(0,0) (1m,1) (2m,1) (3m,-1)' }, { '+': 'b', '-': '0' }], ['R', 'Rb', { R: '1k' }, { 1: 'b', 2: '0' }], ['VP', 'V_pwl2', {}, { n: 'b' }],
        ['VBAT', 'Bat', { V: 12 }, { '+': 'c', '-': '0' }], ['R', 'Rcc', { R: '1k' }, { 1: 'c', 2: '0' }], ['VP', 'V_bat', {}, { n: 'c' }],
        ['VRAND', 'Vr', { Vpp: 1, off: 0, seed: 3 }, { '+': 'd', '-': '0' }], ['R', 'Rd', { R: '1k' }, { 1: 'd', 2: '0' }], ['VP', 'V_rand', {}, { n: 'd' }],
        ['VMATH', 'Vm', { expr: 'sin(2*pi*500*t)+0.3*cos(2*pi*1500*t)' }, { '+': 'e', '-': '0' }], ['R', 'Re', { R: '1k' }, { 1: 'e', 2: '0' }], ['VP', 'V_math', {}, { n: 'e' }],
      ],
      screens: [['V_saw'], ['V_pwl2', 'V_bat'], ['V_rand'], ['V_math']],
      checks: h => {
        const y = h.col('V_rand').slice(1), n = y.length, m = y.reduce((s, v) => s + v, 0) / n;
        const sd = Math.sqrt(y.reduce((s, v) => s + (v - m) ** 2, 0) / (n - 1));
        return [
          wave(h, 'V_saw', t => 1 + 2 * frac(t * 1e3 + 1e-12), 0, 4e-3, [1e-3, 2e-3, 3e-3, 4e-3], 1e-10, '鋸齒波 off + Vpk·frac(f·t)'),
          wave(h, 'V_pwl2', t => pwl([[0, 0], [1e-3, 1], [2e-3, 1], [3e-3, -1]], t), 0, 4e-3, [], 1e-10, '成對 PWL'),
          row('電池電壓', 12, h.last('V_bat'), h.near(h.last('V_bat'), 12, 1e-12)),
          row('亂數源範圍 [0, 1]', '[0, 1]', `[${h.min('V_rand').toFixed(4)}, ${h.max('V_rand').toFixed(4)}]`, h.min('V_rand') >= 0 && h.max('V_rand') <= 1),
          row('亂數源平均（均勻分佈 0.5）', 0.5, m, Math.abs(m - 0.5) < 4 * 0.2887 / Math.sqrt(n)),
          row('亂數源標準差（1/√12）', 1 / Math.sqrt(12), sd, h.near(sd, 1 / Math.sqrt(12), 0.02)),
          wave(h, 'V_math', t => Math.sin(2 * PI * 500 * t) + 0.3 * Math.cos(2 * PI * 1500 * t), 0, 4e-3, [], 1e-10, '數學式電壓源'),
        ];
      },
    });
  }

  // =================================================================== 16
  {
    const frac = x => x - Math.floor(x);
    const pwl = (pts, t) => { if (t <= pts[0][0]) return pts[0][1]; for (let i = 1; i < pts.length; i++) if (t <= pts[i][0]) return pts[i - 1][1] + (pts[i][1] - pts[i - 1][1]) * (t - pts[i - 1][0]) / (pts[i][0] - pts[i - 1][0]); return pts[pts.length - 1][1]; };
    const tri = t => { const ph = frac(t * 1e3 + 1e-12); return -1 + 2 * (ph < 0.3 ? ph / 0.3 : (1 - ph) / 0.7); };
    const squ = t => (frac(t * 2e3 + 1e-12) < 0.25 ? 1 : 0);
    const R = 10;
    const load = (k, name, type, p) => [[type, name, p, { '+': 'n' + k, '-': '0' }], ['R', 'R' + k, { R }, { 1: 'n' + k, 2: '0' }], ['VP', 'V_' + name, {}, { n: 'n' + k }]];
    EX.push({
      key: 'current_sources', cat: '電源與受控源', name: '電流源總覽（正弦、三角、方波、步階、雙準位、成對 PWL、亂數、數學式）',
      text: '各電流源注入 10 Ω，量到的電壓 = 10·i：\n正弦 1 A 1 kHz 30° + 0.5 A、三角 2 Ap-p 1 kHz（上升比 0.3、偏移 −1 A）、方波 1 A 2 kHz（D = 0.25）、\n步階 2 A @1 ms、雙準位 1 → −1 A（1 ms 起 0.5 ms 斜坡）、成對 PWL、亂數 0 ~ 1 A、數學式 2·e^(−t/1ms)。',
      sc: { dt: '1u', tt: '4m' },
      parts: [
        ...load(1, 'isin', 'ISIN', { Vm: 1, f: '1k', ph: 30, off: 0.5 }),
        ...load(2, 'itri', 'ITRI', { Vpp: 2, f: '1k', D: 0.3, off: -1 }),
        ...load(3, 'isqu', 'ISQU', { Vpp: 1, f: '2k', D: 0.25, off: 0 }),
        ...load(4, 'istep', 'ISTEP', { V: 2, ts: '1m' }),
        ...load(5, 'istep2', 'ISTEP2', { V1: 1, V2: -1, ts: '1m', tr: '0.5m' }),
        ...load(6, 'ipwl2', 'IPWL2', { pairs: '(0,0) (1m,2) (2m,2) (2.5m,0)' }),
        ...load(7, 'irand', 'IRAND', { Vpp: 1, off: 0, seed: 5 }),
        ...load(8, 'imath', 'IMATH', { expr: '2*exp(-t/1e-3)' }),
      ],
      screens: [['V_isin'], ['V_itri', 'V_isqu'], ['V_istep', 'V_istep2', 'V_ipwl2'], ['V_irand'], ['V_imath']],
      checks: h => {
        const y = h.col('V_irand').slice(1).map(v => v / R), n = y.length, m = y.reduce((s, v) => s + v, 0) / n;
        return [
          wave(h, 'V_isin', t => R * (Math.sin(w1k * t + PI / 6) + 0.5), 0, 4e-3, [], 1e-10, '正弦電流源'),
          wave(h, 'V_itri', t => R * tri(t), 0, 4e-3, [], 1e-10, '三角波電流源（上升比 0.3）'),
          wave(h, 'V_isqu', t => R * squ(t), 0, 4e-3, [0.125e-3, 0.5e-3, 0.625e-3, 1e-3, 1.125e-3, 1.5e-3, 1.625e-3, 2e-3, 2.125e-3, 2.5e-3, 2.625e-3, 3e-3, 3.125e-3, 3.5e-3, 3.625e-3, 4e-3], 1e-10, '方波電流源（D = 0.25）'),
          wave(h, 'V_istep', t => (t >= 1e-3 ? 2 * R : 0), 0, 4e-3, [1e-3], 1e-10, '步階電流源'),
          wave(h, 'V_istep2', t => R * (t < 1e-3 ? 1 : t < 1.5e-3 ? 1 - 2 * (t - 1e-3) / 0.5e-3 : -1), 0, 4e-3, [], 1e-10, '雙準位步階（0.5 ms 斜坡）'),
          wave(h, 'V_ipwl2', t => R * pwl([[0, 0], [1e-3, 2], [2e-3, 2], [2.5e-3, 0]], t), 0, 4e-3, [], 1e-10, '成對 PWL 電流源'),
          row('亂數電流源平均（0.5 A）', 0.5, m, Math.abs(m - 0.5) < 4 * 0.2887 / Math.sqrt(n)),
          wave(h, 'V_imath', t => R * 2 * Math.exp(-t / 1e-3), 0, 4e-3, [], 1e-10, '數學式電流源'),
        ];
      },
    });
  }

  // =================================================================== 17
  {
    const w = 2 * PI * 100, dt = 10e-6;
    const u2 = t => Math.sin(w * t);
    EX.push({
      key: 'control_blocks', cat: '控制方塊', name: '控制方塊基礎（比例、積分、PI、微分、重置積分、加減、限制、延遲）',
      text: '輸入：u1 = 1 ms 時的單位步階、u2 = 100 Hz 正弦、常數 0.5。\n比例 3·u2；積分 1/(s·1ms) → 斜坡；PI k=2、T=1ms → 2 + 2(t−1ms)/1ms；微分 T = 1 ms → 0.628·cos；\n可重置積分（每 5 ms 歸零）→ 鋸齒；減法、三輸入加法；限制 ±0.5；變化率限制 100/s；單位延遲；2 ms 延遲。',
      sc: { dt: '10u', tt: '20m' },
      parts: [
        ['VSTEP', 'U1', { V: 1, ts: '1m' }, { '+': 'u1', '-': '0' }], ['VSIN', 'U2', { Vm: 1, f: 100 }, { '+': 'u2', '-': '0' }], ['CONST', 'K05', { v: 0.5 }, { out: 'c' }], ['CONST', 'K1', { v: 1 }, { out: 'one' }],
        ['VSQU', 'Rst', { Vpp: 1, f: 200, D: 0.5 }, { '+': 'rst', '-': '0' }],
        ['P', 'Pk', { k: 3 }, { in: 'u2', out: 'y_p' }], ['VP', 'Y_p', {}, { n: 'y_p' }],
        ['I', 'Int', { T: '1m' }, { in: 'u1', out: 'y_i' }], ['VP', 'Y_i', {}, { n: 'y_i' }],
        ['PI', 'Pi', { k: 2, T: '1m' }, { in: 'u1', out: 'y_pi' }], ['VP', 'Y_pi', {}, { n: 'y_pi' }],
        ['DIFF', 'Df', { T: '1m' }, { in: 'u2', out: 'y_d' }], ['VP', 'Y_d', {}, { n: 'y_d' }],
        ['RESETI', 'Ri', { T: '1m', flag: 0 }, { in: 'one', out: 'y_ri', reset: 'rst' }], ['VP', 'Y_ri', {}, { n: 'y_ri' }],
        ['SUB2', 'Sb', {}, { 'in+': 'u2', 'in-': 'c', out: 'y_sub' }], ['VP', 'Y_sub', {}, { n: 'y_sub' }],
        ['SUM3', 'S3', { k1: 1, k2: 2, k3: -1 }, { in1: 'u1', in2: 'u2', in3: 'c', out: 'y_sum' }], ['VP', 'Y_sum', {}, { n: 'y_sum' }],
        ['LIM', 'Lm', { lo: -0.5, hi: 0.5 }, { in: 'u2', out: 'y_lim' }], ['VP', 'Y_lim', {}, { n: 'y_lim' }],
        ['LIMDVDT', 'Ld', { L: 100 }, { in: 'u1', out: 'y_ld' }], ['VP', 'Y_ld', {}, { n: 'y_ld' }],
        ['UTD', 'Ud', {}, { in: 'u2', out: 'y_ud' }], ['VP', 'Y_ud', {}, { n: 'y_ud' }],
        ['TDELAY', 'Td', { Td: '2m' }, { in: 'u2', out: 'y_td' }], ['VP', 'Y_td', {}, { n: 'y_td' }],
      ],
      screens: [['Y_i', 'Y_pi', 'Y_ld'], ['Y_p', 'Y_d', 'Y_lim'], ['Y_ri'], ['Y_sub', 'Y_sum'], ['Y_ud', 'Y_td']],
      checks: h => {
        const frac = x => x - Math.floor(x);
        return [
          wave(h, 'Y_p', t => 3 * u2(t), 0, 20e-3, [], 1e-10, '比例 3·u2'),
          wave(h, 'Y_i', t => (t > 1e-3 ? (t - 1e-3) / 1e-3 : 0), 0, 20e-3, [1e-3], 1e-3, '積分器（梯形法）'),
          wave(h, 'Y_pi', t => (t > 1e-3 ? 2 + 2 * (t - 1e-3) / 1e-3 : 0), 0, 20e-3, [1e-3], 1e-3, 'PI 步階響應'),
          wave(h, 'Y_d', t => 1e-3 * w * Math.cos(w * t), 2 * dt, 20e-3, [], 2 * w * dt, '微分器 T·du/dt（後向差分誤差 ≈ ωΔt/2）'),
          wave(h, 'Y_ri', t => { const s = frac(t * 200 + 1e-9) / 200; return s / 1e-3; }, 0.1e-3, 20e-3, [5e-3, 10e-3, 15e-3, 20e-3], 2e-3, '可重置積分（上升緣歸零）'),
          wave(h, 'Y_sub', t => u2(t) - 0.5, 0, 20e-3, [], 1e-10, '減法 u2 − 0.5'),
          wave(h, 'Y_sum', t => (t >= 1e-3 ? 1 : 0) + 2 * u2(t) - 0.5, 0, 20e-3, [1e-3], 1e-10, '三輸入加法 u1 + 2u2 − 0.5'),
          wave(h, 'Y_lim', t => Math.max(-0.5, Math.min(0.5, u2(t))), 0, 20e-3, [], 1e-10, '限制器 ±0.5'),
          wave(h, 'Y_ld', t => (t < 1e-3 ? 0 : Math.min(1, 100 * (t - 1e-3))), 0, 20e-3, [1e-3, 11e-3], 2e-3, '變化率限制 100 /s'),
          wave(h, 'Y_ud', t => u2(t - dt), dt, 20e-3, [], 1e-10, '單位時間延遲 = u2(t − Δt)'),
          wave(h, 'Y_td', t => (t > 2e-3 ? u2(t - 2e-3) : 0), 0, 20e-3, [], 1e-10, '2 ms 時間延遲'),
        ];
      },
    });
  }

  // =================================================================== 18
  {
    const f = 1e3, s = C(0, 2 * PI * f), wc = 2 * PI * 1e3;
    const Hlp1 = div(C(wc), add(s, C(wc))), Hhp1 = div(s, add(s, C(wc)));
    const den2 = add(add(mul(s, s), mul(C(2 * 0.7071 * wc), s)), C(wc * wc));
    const Hlp2 = div(C(wc * wc), den2), Hhp2 = div(mul(s, s), den2);
    const B = 2 * PI * 100, wo = 2 * PI * 1e3;
    const Hbp = div(mul(C(B), s), add(add(mul(s, s), mul(C(B), s)), C(wo * wo)));
    const s15 = C(0, 2 * PI * 1.5e3);
    const Hbs = div(add(mul(s15, s15), C(wo * wo)), add(add(mul(s15, s15), mul(C(B), s15)), C(wo * wo)));
    const Htf = div(C(4e6), add(add(mul(s, s), mul(C(2000), s)), C(4e6)));
    const Htf1 = div(C(1), add(mul(C(1e-3), s), C(1)));
    const ph = (h, n, Hc, fq, lab) => phasor(h, n, fq, 0.036, 0.04, cabs(Hc), carg(Hc), 1e-4, 0.02, lab);
    EX.push({
      key: 'analog_filters', cat: '控制方塊', name: '類比濾波器與 s 域轉移函數（一階、二階、帶通、帶拒）',
      text: '1 kHz、1 V 正弦輸入（帶拒用 1.5 kHz），穩態振幅與相位對照 |H(jω)|、∠H(jω)：\n一階低通／高通（fc = 1 kHz）、二階低通／高通（ζ = 0.7071）、帶通（fo = 1 kHz、頻寬 100 Hz）、帶拒、\ns 域轉移函數 4·10⁶/(s² + 2000s + 4·10⁶)、含初值的轉移函數 1/(10⁻³s + 1)（初值 2）。',
      sc: { dt: '1u', tt: '40m' },
      parts: [
        ['VSIN', 'Vin', { Vm: 1, f: '1k' }, { '+': 'u', '-': '0' }], ['VSIN', 'Vin15', { Vm: 1, f: '1.5k' }, { '+': 'u15', '-': '0' }],
        ['LP1', 'Lp1', { fc: '1k' }, { in: 'u', out: 'y1' }], ['VP', 'Y_lp1', {}, { n: 'y1' }],
        ['HP1', 'Hp1', { fc: '1k' }, { in: 'u', out: 'y2' }], ['VP', 'Y_hp1', {}, { n: 'y2' }],
        ['LP2', 'Lp2', { fc: '1k', z: 0.7071 }, { in: 'u', out: 'y3' }], ['VP', 'Y_lp2', {}, { n: 'y3' }],
        ['HP2', 'Hp2', { fc: '1k', z: 0.7071 }, { in: 'u', out: 'y4' }], ['VP', 'Y_hp2', {}, { n: 'y4' }],
        ['BP2', 'Bp', { fo: '1k', fb: 100 }, { in: 'u', out: 'y5' }], ['VP', 'Y_bp', {}, { n: 'y5' }],
        ['BS2', 'Bs', { fo: '1k', fb: 100 }, { in: 'u15', out: 'y6' }], ['VP', 'Y_bs', {}, { n: 'y6' }],
        ['TFS', 'Tf', { num: '0 0 4e6', den: '1 2000 4e6' }, { in: 'u', out: 'y7' }], ['VP', 'Y_tfs', {}, { n: 'y7' }],
        ['TFS1', 'Tf1', { num: '0 1', den: '1e-3 1', x0: '2' }, { in: 'u', out: 'y8' }], ['VP', 'Y_tfs1', {}, { n: 'y8' }],
      ],
      screens: [['Y_lp1', 'Y_hp1'], ['Y_lp2', 'Y_hp2'], ['Y_bp', 'Y_bs'], ['Y_tfs', 'Y_tfs1']],
      checks: h => [
        ...ph(h, 'Y_lp1', Hlp1, f, '一階低通'), ...ph(h, 'Y_hp1', Hhp1, f, '一階高通'),
        ...ph(h, 'Y_lp2', Hlp2, f, '二階低通'), ...ph(h, 'Y_hp2', Hhp2, f, '二階高通'),
        ...ph(h, 'Y_bp', Hbp, f, '帶通（中心頻率）'), ...ph(h, 'Y_bs', Hbs, 1.5e3, '帶拒（1.5 kHz）'),
        ...ph(h, 'Y_tfs', Htf, f, 's 域轉移函數'), ...ph(h, 'Y_tfs1', Htf1, f, '含初值轉移函數（穩態）'),
        row('含初值轉移函數 y(0)', 2, h.col('Y_tfs1')[0], Math.abs(h.col('Y_tfs1')[0] - 2) < 1e-3),
      ],
    });
  }
};
