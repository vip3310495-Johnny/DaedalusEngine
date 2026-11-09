// Examples 19–22: math function blocks, logic gates / flip-flops / counters, discrete (z-domain)
// control blocks and converters, vectors / memory / multiplexers / lookup tables / sample-and-hold.
// Every check compares the simulated control signal with an independent closed form or an
// independent discrete-event / difference-equation reference written from the block's documentation.
module.exports = (EX, H) => {
  const { PI, row, wave } = H;
  const frac = x => x - Math.floor(x);
  const deg = PI / 180;
  // square source as documented: high (off + Vpp) while frac(f·t − ph/360) < D
  const sq = (f, D = 0.5, ph = 0) => t => (frac(f * t - ph / 360) < D ? 1 : 0);
  // rising / falling edge times of such a square on [0, tt]
  const edges = (f, D, ph, tt) => {
    const r = [], td = ph / 360 / f;
    for (let k = -1; k * (1 / f) <= tt + 1 / f; k++) {
      const a = td + k / f, b = a + D / f;
      if (a > 0 && a <= tt) r.push(a);
      if (b > 0 && b <= tt) r.push(b);
    }
    return r;
  };
  const rising = (f, D, ph, tt) => edges(f, D, ph, tt).filter(t => sq(f, D, ph)(t + 1e-9) === 1);
  // count of samples k/fs ≤ t (sample instants are breakpoints; the value at the instant is the new one)
  const kOf = (t, fs) => Math.floor(t * fs + 1e-7);

  // =================================================================== 19
  {
    const w = 2 * PI * 100, w50 = 2 * PI * 50;
    const x = t => 0.5 + 0.4 * Math.sin(w * t);
    const dg = t => 80 * Math.sin(w * t);
    const rd = t => 3 * Math.sin(w * t);
    const ys = t => Math.sin(w50 * t), xc = t => Math.sin(w50 * t + PI / 2);
    const zc = [0, 5e-3, 10e-3, 15e-3, 20e-3];
    const un = (type, name, inn, out, p = {}) => [type, name, p, { in: inn, out }];
    const pr = (n, net) => ['VP', n, {}, { n: net }];
    const jsRound = v => Math.round(v);
    EX.push({
      key: 'math_blocks', cat: '控制方塊', name: '數學函數方塊（乘除、根號、三角／反三角、指數對數、捨入、運算式）',
      text: '控制訊號：x = 0.5 + 0.4·sin(2π·100t)、θ° = 80·sin(2π·100t)（度）、θr = 3·sin(2π·100t)（弧度）、y = sin(2π·50t)、xc = cos(2π·50t)。\n每個方塊的輸出與 JavaScript Math 的閉合解逐點比較（相對誤差 ≤ 10⁻⁹）：乘、除、√、sin/cos/tan（度／弧度）、asin/acos、atan(虛/實)、atan2、\nk1·k2^x、k1·x^k2、ln、log10、|·|、sign、max/min、四捨五入／截斷（N = 1）及 1/2/3/5/10 輸入運算式方塊。',
      sc: { dt: '10u', tt: '20m' },
      parts: [
        ['VSIN', 'Ux', { Vm: 0.4, f: 100, off: 0.5 }, { '+': 'x', '-': '0' }], ['VSIN', 'Ud', { Vm: 80, f: 100 }, { '+': 'dg', '-': '0' }],
        ['VSIN', 'Ur', { Vm: 3, f: 100 }, { '+': 'rd', '-': '0' }], ['VSIN', 'Uy', { Vm: 1, f: 50 }, { '+': 'ys', '-': '0' }],
        ['VSIN', 'Uc', { Vm: 1, f: 50, ph: 90 }, { '+': 'xc', '-': '0' }], ['CONST', 'K2', { v: 2 }, { out: 'k2' }],
        ['MULT', 'Mul', {}, { in1: 'x', in2: 'dg', out: 'o_mul' }], pr('Y_mul', 'o_mul'),
        ['DIVD', 'Div', {}, { num: 'dg', den: 'x', out: 'o_div' }], pr('Y_div', 'o_div'),
        un('SQRT', 'Sq', 'x', 'o_sqrt'), pr('Y_sqrt', 'o_sqrt'),
        un('SIN', 'Sn', 'dg', 'o_sin'), pr('Y_sin', 'o_sin'), un('SINR', 'Snr', 'rd', 'o_sinr'), pr('Y_sinr', 'o_sinr'),
        un('COS', 'Cs', 'dg', 'o_cos'), pr('Y_cos', 'o_cos'), un('COSR', 'Csr', 'rd', 'o_cosr'), pr('Y_cosr', 'o_cosr'),
        un('TAN', 'Tn', 'dg', 'o_tan'), pr('Y_tan', 'o_tan'),
        un('ASIN', 'As', 'x', 'o_asin'), pr('Y_asin', 'o_asin'), un('ACOS', 'Ac', 'x', 'o_acos'), pr('Y_acos', 'o_acos'),
        ['ATAN', 'At', {}, { real: 'x', imag: 'dg', out: 'o_atan' }], pr('Y_atan', 'o_atan'),
        ['ATAN2', 'At2', {}, { y: 'ys', x: 'xc', out: 'o_atan2' }], pr('Y_atan2', 'o_atan2'),
        un('EXP', 'Ex', 'x', 'o_exp', { k1: 2, k2: '2.718281828' }), pr('Y_exp', 'o_exp'),
        un('POW', 'Pw', 'x', 'o_pow', { k1: 1.5, k2: 3 }), pr('Y_pow', 'o_pow'),
        un('LOG', 'Ln', 'x', 'o_ln'), pr('Y_ln', 'o_ln'), un('LOG10', 'Lg', 'x', 'o_lg'), pr('Y_log10', 'o_lg'),
        un('ABS', 'Ab', 'dg', 'o_abs'), pr('Y_abs', 'o_abs'), un('SIGN', 'Sg', 'dg', 'o_sign'), pr('Y_sign', 'o_sign'),
        ['MAXMIN', 'Mx', { flag: 0 }, { in1: 'x', in2: 'ys', out: 'o_max' }], pr('Y_max', 'o_max'),
        ['MAXMIN', 'Mn', { flag: 1 }, { in1: 'x', in2: 'ys', out: 'o_min' }], pr('Y_min', 'o_min'),
        un('ROUND', 'Rn', 'dg', 'o_rnd', { N: 1, flag: 0 }), pr('Y_round', 'o_rnd'),
        un('ROUND', 'Rt', 'dg', 'o_trn', { N: 1, flag: 1 }), pr('Y_trunc', 'o_trn'),
        ['MATH1', 'F1', { expr: '3*x1^2 - x1 + 1' }, { x1: 'x', out: 'o_f1' }], pr('Y_f1', 'o_f1'),
        ['MATH2', 'F2', { expr: 'x1*x2 + max(x1, x2)' }, { x1: 'x', x2: 'dg', out: 'o_f2' }], pr('Y_f2', 'o_f2'),
        ['MATH3', 'F3', { expr: 'x1 + 2*x2 - x3' }, { x1: 'x', x2: 'dg', x3: 'rd', out: 'o_f3' }], pr('Y_f3', 'o_f3'),
        ['MATH5', 'F5', { expr: 'x1*x2 - x3 + x4*x5' }, { x1: 'x', x2: 'dg', x3: 'rd', x4: 'ys', x5: 'xc', out: 'o_f5' }], pr('Y_f5', 'o_f5'),
        ['MATH10', 'F10', { expr: 'x1+2*x2+3*x3+4*x4+5*x5+6*x6+7*x7+8*x8+9*x9+10*x10' },
          { x1: 'x', x2: 'dg', x3: 'rd', x4: 'ys', x5: 'xc', x6: 'k2', x7: 'x', x8: 'dg', x9: 'rd', x10: 'ys', out: 'o_f10' }], pr('Y_f10', 'o_f10'),
      ],
      screens: [['Y_mul', 'Y_div', 'Y_sqrt'], ['Y_sin', 'Y_cos', 'Y_tan'], ['Y_sinr', 'Y_cosr'], ['Y_asin', 'Y_acos', 'Y_atan', 'Y_atan2'],
        ['Y_exp', 'Y_pow', 'Y_ln', 'Y_log10'], ['Y_abs', 'Y_sign', 'Y_max', 'Y_min'], ['Y_round', 'Y_trunc'], ['Y_f1', 'Y_f2', 'Y_f3', 'Y_f5', 'Y_f10']],
      checks: h => {
        const W = (p, fn, lab, skip = []) => wave(h, p, fn, 0, 20e-3, skip, 1e-9, lab);
        return [
          W('Y_mul', t => x(t) * dg(t), '乘法 x·θ'),
          W('Y_div', t => dg(t) / x(t), '除法 θ/x'),
          W('Y_sqrt', t => Math.sqrt(x(t)), '平方根 √x'),
          W('Y_sin', t => Math.sin(dg(t) * deg), 'sin（度）'), W('Y_cos', t => Math.cos(dg(t) * deg), 'cos（度）'),
          W('Y_tan', t => Math.tan(dg(t) * deg), 'tan（度）'),
          W('Y_sinr', t => Math.sin(rd(t)), 'sin（弧度）'), W('Y_cosr', t => Math.cos(rd(t)), 'cos（弧度）'),
          W('Y_asin', t => Math.asin(x(t)) / deg, 'asin（輸出度）'), W('Y_acos', t => Math.acos(x(t)) / deg, 'acos（輸出度）'),
          W('Y_atan', t => Math.atan(dg(t) / x(t)) / deg, 'atan(虛部/實部)（度）'),
          W('Y_atan2', t => Math.atan2(ys(t), xc(t)) / deg, '四象限 atan2（度）', [10e-3]),
          W('Y_exp', t => 2 * Math.pow(2.718281828, x(t)), '指數 2·e^x'),
          W('Y_pow', t => 1.5 * x(t) ** 3, '冪次 1.5·x³'),
          W('Y_ln', t => Math.log(x(t)), '自然對數'), W('Y_log10', t => Math.log10(x(t)), '常用對數'),
          W('Y_abs', t => Math.abs(dg(t)), '絕對值'),
          W('Y_sign', t => Math.sign(dg(t)), '符號函數', zc),
          W('Y_max', t => Math.max(x(t), ys(t)), '最大值'), W('Y_min', t => Math.min(x(t), ys(t)), '最小值'),
          W('Y_round', t => jsRound(dg(t) * 10) / 10, '四捨五入到 0.1'),
          W('Y_trunc', t => Math.trunc(dg(t) * 10) / 10, '截斷到 0.1'),
          W('Y_f1', t => 3 * x(t) ** 2 - x(t) + 1, '1 輸入運算式 3x² − x + 1'),
          W('Y_f2', t => x(t) * dg(t) + Math.max(x(t), dg(t)), '2 輸入運算式'),
          W('Y_f3', t => x(t) + 2 * dg(t) - rd(t), '3 輸入運算式'),
          W('Y_f5', t => x(t) * dg(t) - rd(t) + ys(t) * xc(t), '5 輸入運算式'),
          W('Y_f10', t => { const v = [x, dg, rd, ys, xc, () => 2, x, dg, rd, ys].map(f => f(t)); return v.reduce((s, q, i) => s + (i + 1) * q, 0); }, '10 輸入運算式'),
        ];
      },
    });
  }

  // =================================================================== 20
  {
    const TT = 20e-3;
    // signals (all start low at t = 0, so no edge is ambiguous at t = 0)
    const SIG = {
      A: [1e3, 0.5, 180], B: [500, 0.5, 180], Cq: [250, 0.5, 180], Dd: [330, 0.3, 180], Kk: [230, 0.5, 180],
      S: [50, 0.015, 221.4], R: [50, 0.015, 273.6],
    };
    const lv = Object.fromEntries(Object.entries(SIG).map(([k, [f, D, ph]]) => [k, sq(f, D, ph)]));
    const allEdges = [].concat(...Object.values(SIG).map(([f, D, ph]) => edges(f, D, ph, TT)));
    const W1 = 0.3e-3, W2 = 0.2e-3;
    const monoEnds = rising(...SIG.A, TT).map(t => t + W1).concat(rising(...SIG.A, TT).map(t => t + W2));
    const skip = allEdges.concat(monoEnds);
    // independent discrete-event reference: fine grid (0.1 µs), edges detected between grid points
    const G = 1e-7, NG = Math.round(TT / G);
    const ref = {};
    {
      const keys = ['dff', 'dffsr', 'jk', 'jksr', 'sre', 'srl', 'mono', 'monoc', 'pwct', 'cnt'];
      for (const k of keys) ref[k] = new Float64Array(NG + 1);
      let p = { A: 0, Dd: 0, Kk: 0, R: 0 };
      let q = { dff: 0, dffsr: 0, jk: 0, jksr: 0, sre: 0, srl: 0, pwct: 0, cnt: 0 };
      let mEnd = -1, mcEnd = -1, pwStart = -1;
      for (let i = 0; i <= NG; i++) {
        const t = i * G;
        const v = Object.fromEntries(Object.keys(lv).map(k => [k, lv[k](t)]));
        const up = k => i > 0 && p[k] === 0 && v[k] === 1;
        const dn = k => i > 0 && p[k] === 1 && v[k] === 0;
        if (up('A')) {
          q.dff = v.Dd;
          if (!v.S && !v.R) q.dffsr = v.Dd;
          const jk = (Q, J, K) => (J && K ? 1 - Q : J ? 1 : K ? 0 : Q);
          q.jk = jk(q.jk, v.Dd, v.Kk);
          if (!v.S && !v.R) q.jksr = jk(q.jksr, v.Dd, v.Kk);
          if (t >= mEnd) mEnd = t + W1;
          if (t >= mcEnd) mcEnd = t + W2;
          q.cnt = (q.cnt + 1) % 256;
        }
        if (up('Dd')) { q.sre = 1; pwStart = t; q.cnt = (q.cnt + 255) % 256; }
        if (dn('Dd') && pwStart >= 0) q.pwct = t - pwStart;
        if (up('Kk')) q.sre = 0;
        if (v.S) { q.dffsr = 1; q.jksr = 1; }
        if (v.R) { q.dffsr = 0; q.jksr = 0; q.cnt = 0; }
        if (v.Dd && !v.Kk) q.srl = 1; else if (!v.Dd && v.Kk) q.srl = 0;
        for (const k of ['dff', 'dffsr', 'jk', 'jksr', 'sre', 'srl', 'pwct', 'cnt']) ref[k][i] = q[k];
        ref.mono[i] = t < mEnd ? 1 : 0;
        ref.monoc[i] = t < mcEnd ? 1 : 0;
        p = v;
      }
    }
    const fall1 = edges(...SIG.Dd, TT).filter(t => lv.Dd(t + 1e-9) === 0)[0];
    const R_ = k => t => ref[k][Math.min(NG, Math.max(0, Math.round(t / G)))];
    const L = (k, c = '') => t => lv[k](t);
    EX.push({
      key: 'logic', cat: '數位邏輯', name: '邏輯閘、正反器、單擊電路、脈寬計數與上下數計數器',
      text: '方波訊號（t = 0 時皆為低電位）：A = 1 kHz、B = 500 Hz、C = 250 Hz；D = 330 Hz（占空比 0.3）、K = 230 Hz；\nS 脈波 12.3 ~ 12.6 ms、R 脈波 15.2 ~ 15.5 ms。閘：AND／AND3／OR／OR3／XOR／NOT／NAND／NOR；\n正反器（時脈 = A）：D、D-SR、JK、JK-SR、SR（邊緣／準位）；單擊 0.3 ms、可控單擊 0.2 ms；脈寬計數器（D 的脈寬 = 0.3/330 s）；\n上/下數計數器（A 加、D 減、R 清除）。與獨立的離散事件參考模型逐點比較（邊緣附近 ±15 µs 不比較）。',
      sc: { dt: '10u', tt: '20m' },
      parts: [
        ...Object.entries(SIG).map(([k, [f, D, ph]]) => ['VSQU', 'U' + k, { Vpp: 1, f, D, ph }, { '+': k.toLowerCase(), '-': '0' }]),
        ['CONST', 'Kw', { v: '0.2m' }, { out: 'wm' }],
        ['AND', 'G1', {}, { in1: 'a', in2: 'b', out: 'g_and' }], ['VP', 'Y_and', {}, { n: 'g_and' }],
        ['AND3', 'G2', {}, { in1: 'a', in2: 'b', in3: 'cq', out: 'g_and3' }], ['VP', 'Y_and3', {}, { n: 'g_and3' }],
        ['OR', 'G3', {}, { in1: 'a', in2: 'b', out: 'g_or' }], ['VP', 'Y_or', {}, { n: 'g_or' }],
        ['OR3', 'G4', {}, { in1: 'a', in2: 'b', in3: 'cq', out: 'g_or3' }], ['VP', 'Y_or3', {}, { n: 'g_or3' }],
        ['XOR', 'G5', {}, { in1: 'a', in2: 'b', out: 'g_xor' }], ['VP', 'Y_xor', {}, { n: 'g_xor' }],
        ['NOT', 'G6', {}, { in1: 'a', out: 'g_not' }], ['VP', 'Y_not', {}, { n: 'g_not' }],
        ['NAND', 'G7', {}, { in1: 'a', in2: 'b', out: 'g_nand' }], ['VP', 'Y_nand', {}, { n: 'g_nand' }],
        ['NOR', 'G8', {}, { in1: 'a', in2: 'b', out: 'g_nor' }], ['VP', 'Y_nor', {}, { n: 'g_nor' }],
        ['DFF', 'F1', {}, { D: 'dd', CLK: 'a', Q: 'q_d', Qn: 'qn_d' }], ['VP', 'Q_dff', {}, { n: 'q_d' }], ['VP', 'Qn_dff', {}, { n: 'qn_d' }],
        ['DFFSR', 'F2', {}, { S: 's', D: 'dd', CLK: 'a', R: 'r', Q: 'q_dsr', Qn: 'qn_dsr' }], ['VP', 'Q_dffsr', {}, { n: 'q_dsr' }],
        ['JKFF', 'F3', {}, { J: 'dd', CLK: 'a', K: 'kk', Q: 'q_jk', Qn: 'qn_jk' }], ['VP', 'Q_jk', {}, { n: 'q_jk' }], ['VP', 'Qn_jk', {}, { n: 'qn_jk' }],
        ['JKFFSR', 'F4', {}, { S: 's', J: 'dd', CLK: 'a', K: 'kk', R: 'r', Q: 'q_jksr', Qn: 'qn_jksr' }], ['VP', 'Q_jksr', {}, { n: 'q_jksr' }],
        ['SRFF', 'F5', { flag: 0 }, { S: 'dd', R: 'kk', Q: 'q_sre', Qn: 'qn_sre' }], ['VP', 'Q_sr_edge', {}, { n: 'q_sre' }],
        ['SRFF', 'F6', { flag: 1 }, { S: 'dd', R: 'kk', Q: 'q_srl', Qn: 'qn_srl' }], ['VP', 'Q_sr_level', {}, { n: 'q_srl' }],
        ['MONO', 'M1', { W: '0.3m' }, { in: 'a', Q: 'q_m', Qn: 'qn_m' }], ['VP', 'Q_mono', {}, { n: 'q_m' }], ['VP', 'Qn_mono', {}, { n: 'qn_m' }],
        ['MONOC', 'M2', {}, { in: 'a', W: 'wm', Q: 'q_mc', Qn: 'qn_mc' }], ['VP', 'Q_monoc', {}, { n: 'q_mc' }],
        ['PWCT', 'Pw', {}, { in: 'dd', out: 'pw' }], ['VP', 'Y_pwct', {}, { n: 'pw' }],
        ['UPDN', 'Ct', { bits: 8 }, { U: 'a', D: 'dd', R: 'r', out: 'cnt' }], ['VP', 'Y_count', {}, { n: 'cnt' }],
      ],
      screens: [['Y_and', 'Y_and3', 'Y_or', 'Y_or3'], ['Y_xor', 'Y_not', 'Y_nand', 'Y_nor'], ['Q_dff', 'Q_dffsr', 'Q_jk', 'Q_jksr'],
        ['Q_sr_edge', 'Q_sr_level'], ['Q_mono', 'Q_monoc'], ['Y_pwct'], ['Y_count']],
      checks: h => {
        const W = (p, fn, lab) => wave(h, p, fn, 0, TT, skip, 1e-9, lab);
        const { A, B, Cq } = { A: L('A'), B: L('B'), Cq: L('Cq') };
        return [
          W('Y_and', t => A(t) & B(t), 'AND'), W('Y_and3', t => A(t) & B(t) & Cq(t), 'AND3'),
          W('Y_or', t => A(t) | B(t), 'OR'), W('Y_or3', t => A(t) | B(t) | Cq(t), 'OR3'),
          W('Y_xor', t => A(t) ^ B(t), 'XOR'), W('Y_not', t => 1 - A(t), 'NOT'),
          W('Y_nand', t => 1 - (A(t) & B(t)), 'NAND'), W('Y_nor', t => 1 - (A(t) | B(t)), 'NOR'),
          W('Q_dff', R_('dff'), 'D 正反器 Q'), W('Qn_dff', t => 1 - R_('dff')(t), 'D 正反器 Q̄'),
          W('Q_dffsr', R_('dffsr'), 'D 正反器（非同步 S/R）'),
          W('Q_jk', R_('jk'), 'JK 正反器 Q'), W('Qn_jk', t => 1 - R_('jk')(t), 'JK 正反器 Q̄'),
          W('Q_jksr', R_('jksr'), 'JK 正反器（非同步 S/R）'),
          W('Q_sr_edge', R_('sre'), 'SR 正反器（邊緣觸發）'), W('Q_sr_level', R_('srl'), 'SR 正反器（準位觸發）'),
          W('Q_mono', R_('mono'), '單擊 0.3 ms'), W('Qn_mono', t => 1 - R_('mono')(t), '單擊 Q̄'),
          W('Q_monoc', R_('monoc'), '可控單擊 0.2 ms'),
          wave(h, 'Y_pwct', t => (t >= fall1 ? 0.3 / 330 : 0), 0, TT, skip, 1e-9, '脈寬計數器'),
          row('脈寬計數器末值（s）', 0.3 / 330, h.last('Y_pwct'), h.near(h.last('Y_pwct'), 0.3 / 330, 1e-9)),
          wave(h, 'Y_count', R_('cnt'), 0, TT, skip, 1e-9, '上/下數計數器'),
          row('計數值 @ 15 ms（16 次上數 − 5 次下數）', R_('cnt')(15e-3), h.at('Y_count', 15e-3), h.at('Y_count', 15e-3) === R_('cnt')(15e-3)),
        ];
      },
    });
  }

  // =================================================================== 21
  {
    const fs = 2e3, T = 1 / fs, TT = 20e-3, N = Math.round(TT * fs);
    const w = 2 * PI * 100;
    const u = t => Math.sin(w * t);
    const xs = t => 0.5 + 0.4 * Math.sin(w * t);
    const uq = t => 0.9 * Math.sin(w * t) + 0.0123;
    const U = Array.from({ length: N + 2 }, (_, k) => u(k * T));
    const at = k => (k >= 0 ? U[k] : 0);
    const seq = f => { const y = []; for (let k = 0; k <= N + 1; k++) y.push(f(k, y)); return y; };
    const yv = (y, k) => (k >= 0 ? y[k] : 0);
    const S = {
      zoh: seq(k => at(k)),
      ud: seq(k => at(k - 1)),
      dd: seq(k => (at(k) - at(k - 1)) / T),
      idT: seq((k, y) => yv(y, k - 1) + T / 2 * (at(k) + at(k - 1))),
      idB: seq((k, y) => yv(y, k - 1) + T * at(k)),
      idF: seq((k, y) => yv(y, k - 1) + T * at(k - 1)),
      fir: seq(k => 0.25 * (at(k) + at(k - 1) + at(k - 2) + at(k - 3))),
      iir: seq((k, y) => 0.0201 * at(k) + 0.0402 * at(k - 1) + 0.0201 * at(k - 2) + 1.561 * yv(y, k - 1) - 0.6414 * yv(y, k - 2)),
      tfz: seq((k, y) => 0.1 * at(k - 1) + 0.9 * yv(y, k - 1)),
    };
    // step input 1 at 0.25 ms (between samples): u(0) = 0, u(k ≥ 1) = 1
    const st = k => (k >= 1 ? 1 : 0);
    const rstK = [7.25e-3, 17.25e-3].map(t => Math.ceil(t * fs));
    S.irstd = seq((k, y) => (rstK.includes(k) ? 0 : yv(y, k - 1) + T * st(k)));
    S.irsti = seq((k, y) => { let v = yv(y, k - 1) + T / 2 * 18000 * (st(k) + st(k - 1)); if (v > 360) v -= 360; if (v < 0) v += 360; return v; });
    const q = (v, rnd) => { const vox = (v + 1) * 127.5; const n = rnd ? Math.round(vox) : Math.floor(vox); return Math.max(0, Math.min(255, n)); };
    S.dig = seq(k => q(uq(k * T), false));
    S.dig2 = seq(k => q(uq(k * T), true));
    const adcT = rising(2e3, 0.5, 180, TT);
    const adc = t => { let c = 0; for (const r of adcT) if (r <= t + 1e-9) c = Math.max(0, Math.min(255, Math.floor(256 * xs(r)))); return c; };
    const Y = n => t => S[n][kOf(t, fs)];
    EX.push({
      key: 'digital_control', cat: '數位控制', name: '離散控制方塊（ZOH、1/z、離散積分／微分、FIR、IIR、z 轉移函數、量化、A/D、D/A）',
      text: '取樣頻率 fs = 2 kHz（T = 0.5 ms，取樣時刻 k·T 為模擬斷點），輸入 u = sin(2π·100t)。\n零階保持、單位延遲、離散微分 (u(n) − u(n−1))/T、離散積分（梯形／後向／前向尤拉）、可重置積分（外部上升緣 7.25 ms、17.25 ms）、\n內部重置積分（18000 °/s，0 ~ 360° 環繞）、FIR（4 點平均）、IIR Butterworth（fc = 1 kHz@10 kHz 係數）、z 轉移函數 0.1/(z − 0.9)、\n量化（8 位元截斷／四捨五入）、8 位元 A/D（Vref = 1、2 kHz 時脈上升緣）接 D/A。與差分方程式逐取樣比較。',
      sc: { dt: '10u', tt: '20m' },
      parts: [
        ['VSIN', 'Uu', { Vm: 1, f: 100 }, { '+': 'u', '-': '0' }], ['VSIN', 'Ux', { Vm: 0.4, f: 100, off: 0.5 }, { '+': 'xs', '-': '0' }],
        ['VSIN', 'Uq', { Vm: 0.9, f: 100, off: 0.0123 }, { '+': 'uq', '-': '0' }],
        ['VSTEP', 'Us', { V: 1, ts: '0.25m' }, { '+': 'st', '-': '0' }], ['VSTEP', 'Uw', { V: 18000, ts: '0.25m' }, { '+': 'sw', '-': '0' }],
        ['VSQU', 'Urst', { Vpp: 1, f: 100, D: 0.1, ph: 261 }, { '+': 'rst', '-': '0' }],
        ['VSQU', 'Uclk', { Vpp: 1, f: '2k', D: 0.5, ph: 180 }, { '+': 'clk', '-': '0' }], ['CONST', 'Kref', { v: 1 }, { out: 'vref' }],
        ['ZOH', 'Zh', { fs: '2k' }, { in: 'u', out: 'o_zoh' }], ['VP', 'Y_zoh', {}, { n: 'o_zoh' }],
        ['UDELAY', 'Ud', { fs: '2k' }, { in: 'u', out: 'o_ud' }], ['VP', 'Y_udelay', {}, { n: 'o_ud' }],
        ['DD', 'Dd', { fs: '2k' }, { in: 'u', out: 'o_dd' }], ['VP', 'Y_dd', {}, { n: 'o_dd' }],
        ['ID', 'It', { alg: 0, fs: '2k' }, { in: 'u', out: 'o_it' }], ['VP', 'Y_id_trap', {}, { n: 'o_it' }],
        ['ID', 'Ib', { alg: 1, fs: '2k' }, { in: 'u', out: 'o_ib' }], ['VP', 'Y_id_be', {}, { n: 'o_ib' }],
        ['ID', 'If', { alg: 2, fs: '2k' }, { in: 'u', out: 'o_if' }], ['VP', 'Y_id_fe', {}, { n: 'o_if' }],
        ['IRSTD', 'Ir', { alg: 1, flag: 0, fs: '2k' }, { in: 'st', reset: 'rst', out: 'o_ir' }], ['VP', 'Y_irstd', {}, { n: 'o_ir' }],
        ['IRSTI', 'Ii', { alg: 0, lo: 0, hi: 360, fs: '2k' }, { in: 'sw', out: 'o_ii' }], ['VP', 'Y_irsti', {}, { n: 'o_ii' }],
        ['FIR', 'Fr', { b: '0.25 0.25 0.25 0.25', fs: '2k' }, { in: 'u', out: 'o_fir' }], ['VP', 'Y_fir', {}, { n: 'o_fir' }],
        ['FILTD', 'Fd', { b: '0.0201 0.0402 0.0201', a: '1 -1.561 0.6414', fs: '2k' }, { in: 'u', out: 'o_iir' }], ['VP', 'Y_iir', {}, { n: 'o_iir' }],
        ['TFZ', 'Tz', { b: '0 0.1', a: '1 -0.9', fs: '2k' }, { in: 'u', out: 'o_tfz' }], ['VP', 'Y_tfz', {}, { n: 'o_tfz' }],
        ['DIGIT', 'Q1', { N: 8, vimin: -1, vimax: 1, vomin: 0, vomax: 255, fs: '2k' }, { in: 'uq', out: 'o_q1' }], ['VP', 'Y_quant', {}, { n: 'o_q1' }],
        ['DIGIT2', 'Q2', { N: 8, vimin: -1, vimax: 1, vomin: 0, vomax: 255, fs: '2k' }, { in: 'uq', out: 'o_q2' }], ['VP', 'Y_quant2', {}, { n: 'o_q2' }],
        ['ADC', 'Ad', { bits: 8 }, { Vin: 'xs', Vref: 'vref', CLK: 'clk', out: 'code' }], ['VP', 'Y_adc', {}, { n: 'code' }],
        ['DAC', 'Da', { bits: 8 }, { Vin: 'code', Vref: 'vref', out: 'o_dac' }], ['VP', 'Y_dac', {}, { n: 'o_dac' }],
      ],
      screens: [['Y_zoh', 'Y_udelay'], ['Y_dd'], ['Y_id_trap', 'Y_id_be', 'Y_id_fe'], ['Y_irstd', 'Y_irsti'], ['Y_fir', 'Y_iir', 'Y_tfz'], ['Y_quant', 'Y_quant2'], ['Y_adc'], ['Y_dac']],
      checks: h => {
        const W = (p, n, lab) => wave(h, p, Y(n), 0, TT, [], 1e-9, lab);
        return [
          W('Y_zoh', 'zoh', '零階保持'), W('Y_udelay', 'ud', '單位延遲 1/z'), W('Y_dd', 'dd', '離散微分'),
          W('Y_id_trap', 'idT', '離散積分（梯形）'), W('Y_id_be', 'idB', '離散積分（後向尤拉）'), W('Y_id_fe', 'idF', '離散積分（前向尤拉）'),
          W('Y_irstd', 'irstd', '外部重置積分（上升緣）'), W('Y_irsti', 'irsti', '內部重置積分（0 ~ 360° 環繞）'),
          W('Y_fir', 'fir', 'FIR 4 點平均'), W('Y_iir', 'iir', 'IIR 二階 Butterworth'), W('Y_tfz', 'tfz', 'z 轉移函數 0.1/(z − 0.9)'),
          W('Y_quant', 'dig', '量化 8 位元（截斷）'), W('Y_quant2', 'dig2', '量化 8 位元（四捨五入）'),
          wave(h, 'Y_adc', adc, 0, TT, adcT, 1e-9, 'A/D 8 位元 floor(256·Vin/Vref)'),
          wave(h, 'Y_dac', t => adc(t) / 256, 0, TT, adcT, 1e-9, 'D/A：Vref/2⁸·碼值'),
        ];
      },
    });
  }

  // =================================================================== 22
  {
    const TT = 20e-3, w = 2 * PI * 100;
    const x = t => 0.5 + 0.4 * Math.sin(w * t);
    const dg = t => 80 * Math.sin(w * t);
    const ramp = t => 1000 * t;
    // circular buffer L = 4 at 1 kHz, samples k = 0, 1, 2, … written to index k mod 4
    const cbuf = off => t => { const k = Math.floor(t * 1e3 + 1e-7); for (let n = k; n >= 0; n--) if (n % 4 === off) return n; return 0; };
    // stack (depth 4): push on rising edges at 0.1 + k ms (value = ramp), pop on rising edges at 2 + 4k ms
    const pushT = rising(1e3, 0.5, 36, TT), popT = rising(250, 0.5, 180, TT);
    const stackOut = t => {
      const ev = pushT.filter(s => s <= t + 1e-9).map(s => [s, 'u']).concat(popT.filter(s => s <= t + 1e-9).map(s => [s, 'o'])).sort((a, b) => a[0] - b[0]);
      const st = []; let out = 0;
      for (const [s, k] of ev) { if (k === 'u') { if (st.length === 4) st.shift(); st.push(ramp(s)); } else st.pop(); if (st.length) out = st[st.length - 1]; }
      return out;
    };
    const s0 = sq(400, 0.5, 180), s1 = sq(200, 0.5, 180), s2 = sq(100, 0.5, 180);
    const selE = [].concat(edges(400, 0.5, 180, TT), edges(200, 0.5, 180, TT), edges(100, 0.5, 180, TT));
    const tri = t => { const ph = frac(100 * t); return -0.5 + 4 * (ph < 0.5 ? 2 * ph : 2 - 2 * ph); };
    const lk = v => { const X = [0, 1, 2, 3], Yv = [0, 10, 15, 17]; if (v <= 0) return 0; if (v >= 3) return 17; const j = Math.floor(v); return Yv[j] + (Yv[j + 1] - Yv[j]) * (v - j); };
    const ang = t => -360 + 720 * frac(50 * t);
    const tz = a => { let v = ((a % 360) + 360) % 360, s = 1; if (v >= 180) { v -= 180; s = -1; } if (v > 90) v = 180 - v; return s * (v < 30 ? 2 * v / 30 : 2); };
    const sqk = a => { let v = ((a % 360) + 360) % 360, s = 1; if (v >= 180) { v -= 180; s = -1; } return Math.abs(v - 90) < 60 ? s : 0; };
    const sqJumps = []; for (let k = 0; k < 2; k++) for (const a of [-330, -210, -150, -30, 30, 150, 210, 330]) sqJumps.push((a + 360) / 720 / 50 + k / 50);
    const sampT = rising(1e3, 0.5, 180, TT);
    const samp = t => { let v = 0; for (const r of sampT) if (r <= t + 1e-9) v = x(r); return v; };
    const consts = [10, 20, 30, 40, 50, 60, 70, 80];
    EX.push({
      key: 'vector_lookup', cat: '控制方塊', name: '向量／記憶體、多工器、查表與取樣保持',
      text: '陣列 [2 4 6 8] 讀位址 2 → 6；摺積 [1 2 3] ⊗ [4 5] = [4 13 22 15]；環形緩衝區（長度 4、1 kHz）寫入斜坡 u = 1000·t；\n堆疊（深度 4）：0.1 ms + k·1 ms 推入斜坡值、2 ms + k·4 ms 取出（滿時擠掉最底層；輸出 = 頂端值）；\n2／4／8 輸入多工器（選擇線 400／200／100 Hz）；一維查表 (0,0)(1,10)(2,15)(3,17)（三角波 −0.5 ~ 3.5 輸入、端點箝位）；\n梯形波查表（θ = 30°、峰值 2）與方波查表（脈寬 120°）以 −360° ~ 360° 鋸齒角度驅動；取樣保持（1 kHz 上升緣）。',
      sc: { dt: '10u', tt: '20m' },
      parts: [
        ['VSIN', 'Ux', { Vm: 0.4, f: 100, off: 0.5 }, { '+': 'x', '-': '0' }], ['VSIN', 'Ud', { Vm: 80, f: 100 }, { '+': 'dg', '-': '0' }],
        ['VPWL2', 'Ur', { pairs: '(0,0) (20m,20)' }, { '+': 'ramp', '-': '0' }],
        ['ARRAY', 'Ar', { L: 4, vals: '2 4 6 8' }, { out: 'arr' }], ['MEMREAD', 'Mr', { off: 2 }, { in: 'arr', out: 'o_arr' }], ['VP', 'Y_array2', {}, { n: 'o_arr' }],
        ['ARRAY', 'Aa', { L: 3, vals: '1 2 3' }, { out: 'va' }], ['ARRAY', 'Ab', { L: 2, vals: '4 5' }, { out: 'vb' }],
        ['CONV', 'Cv', {}, { A: 'va', B: 'vb', out: 'cv' }],
        ['MEMREAD', 'Mc1', { off: 1 }, { in: 'cv', out: 'o_c1' }], ['VP', 'Y_conv1', {}, { n: 'o_c1' }],
        ['MEMREAD', 'Mc3', { off: 3 }, { in: 'cv', out: 'o_c3' }], ['VP', 'Y_conv3', {}, { n: 'o_c3' }],
        ['CBUF', 'Cb', { L: 4, fs: '1k' }, { in: 'ramp', out: 'cb' }],
        ['MEMREAD', 'Mb1', { off: 1 }, { in: 'cb', out: 'o_b1' }], ['VP', 'Y_cbuf1', {}, { n: 'o_b1' }],
        ['MEMREAD', 'Mb3', { off: 3 }, { in: 'cb', out: 'o_b3' }], ['VP', 'Y_cbuf3', {}, { n: 'o_b3' }],
        ['VSQU', 'Upush', { Vpp: 1, f: '1k', D: 0.5, ph: 36 }, { '+': 'push', '-': '0' }], ['VSQU', 'Upop', { Vpp: 1, f: 250, D: 0.5, ph: 180 }, { '+': 'pop', '-': '0' }],
        ['STACK', 'Sk', { depth: 4 }, { Vin: 'ramp', push: 'push', pop: 'pop', out: 'o_stk' }], ['VP', 'Y_stack', {}, { n: 'o_stk' }],
        ['VSQU', 'Us0', { Vpp: 1, f: 400, D: 0.5, ph: 180 }, { '+': 's0', '-': '0' }], ['VSQU', 'Us1', { Vpp: 1, f: 200, D: 0.5, ph: 180 }, { '+': 's1', '-': '0' }],
        ['VSQU', 'Us2', { Vpp: 1, f: 100, D: 0.5, ph: 180 }, { '+': 's2', '-': '0' }],
        ...consts.map((v, i) => ['CONST', 'K' + i, { v }, { out: 'd' + i }]),
        ['MUX2', 'X2', {}, { d0: 'x', d1: 'dg', s0: 's0', Y: 'o_m2' }], ['VP', 'Y_mux2', {}, { n: 'o_m2' }],
        ['MUX4', 'X4', {}, { d0: 'd0', d1: 'd1', d2: 'd2', d3: 'd3', s0: 's0', s1: 's1', Y: 'o_m4' }], ['VP', 'Y_mux4', {}, { n: 'o_m4' }],
        ['MUX8', 'X8', {}, { d0: 'd0', d1: 'd1', d2: 'd2', d3: 'd3', d4: 'd4', d5: 'd5', d6: 'd6', d7: 'd7', s0: 's0', s1: 's1', s2: 's2', Y: 'o_m8' }], ['VP', 'Y_mux8', {}, { n: 'o_m8' }],
        ['VTRI', 'Ut', { Vpp: 4, f: 100, D: 0.5, off: -0.5 }, { '+': 'tri', '-': '0' }],
        ['LKUP', 'Lk', { tab: '0 0 1 10 2 15 3 17' }, { in: 'tri', out: 'o_lk' }], ['VP', 'Y_lkup', {}, { n: 'o_lk' }],
        ['VSAW', 'Ua', { Vpk: 720, f: 50, off: -360 }, { '+': 'ang', '-': '0' }],
        ['LKUPTZ', 'Lt', { th: 30, vpk: 2 }, { in: 'ang', out: 'o_tz' }], ['VP', 'Y_trap', {}, { n: 'o_tz' }],
        ['LKUPSQ', 'Ls', { th: 120 }, { in: 'ang', out: 'o_sq' }], ['VP', 'Y_sqtab', {}, { n: 'o_sq' }],
        ['VSQU', 'Usmp', { Vpp: 1, f: '1k', D: 0.5, ph: 180 }, { '+': 'smp', '-': '0' }],
        ['SAMP', 'Sh', {}, { in: 'x', ctrl: 'smp', out: 'o_sh' }], ['VP', 'Y_samp', {}, { n: 'o_sh' }],
      ],
      screens: [['Y_array2', 'Y_conv1', 'Y_conv3'], ['Y_cbuf1', 'Y_cbuf3'], ['Y_stack'], ['Y_mux2'], ['Y_mux4', 'Y_mux8'], ['Y_lkup'], ['Y_trap', 'Y_sqtab'], ['Y_samp']],
      checks: h => {
        const W = (p, fn, lab, skip = [], tol = 1e-9) => wave(h, p, fn, 0, TT, skip, tol, lab);
        return [
          W('Y_array2', () => 6, '陣列讀位址 2'),
          W('Y_conv1', () => 13, '摺積第 1 項'), W('Y_conv3', () => 15, '摺積第 3 項'),
          W('Y_cbuf1', cbuf(1), '環形緩衝區位置 1', [], 1e-7), W('Y_cbuf3', cbuf(3), '環形緩衝區位置 3', [], 1e-7),
          W('Y_stack', stackOut, '堆疊（深度 4，輸出 = 頂端值）', [], 1e-7),
          W('Y_mux2', t => (s0(t) ? dg(t) : x(t)), '2 輸入多工器', selE),
          W('Y_mux4', t => consts[s0(t) + 2 * s1(t)], '4 輸入多工器', selE),
          W('Y_mux8', t => consts[s0(t) + 2 * s1(t) + 4 * s2(t)], '8 輸入多工器', selE),
          W('Y_lkup', t => lk(tri(t)), '一維查表（線性內插、端點箝位）'),
          W('Y_trap', t => tz(ang(t)), '梯形波查表', [0, 20e-3, 40e-3]),
          W('Y_sqtab', t => sqk(ang(t)), '方波查表', sqJumps.concat([0, 20e-3])),
          W('Y_samp', samp, '取樣保持', sampT),
        ];
      },
    });
  }
};
