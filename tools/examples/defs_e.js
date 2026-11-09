// Examples 23–26: three-phase measurement chain (voltage / current sensors, abc↔dqo, FFT, RMS, THD,
// control-to-power interface, simplified C block) a Monte Carlo tolerance analysis and a loop-gain measurement (AC sweep, loop probe).
module.exports = (EX, H) => {
  const { PI, C, div, add, mul, cabs, carg, jw, row, wave, phasor } = H;
  const frac = x => x - Math.floor(x);

  // =================================================================== 23
  {
    const f = 50, w = 2 * PI * f, Vm = 380 * Math.SQRT2 / Math.sqrt(3), TT = 0.5;
    const Z = add(C(10), jw(w, 20e-3)), Im = Vm / cabs(Z), phI = -carg(Z);
    const va = t => Vm * Math.sin(w * t), vb = t => Vm * Math.sin(w * t - 2 * PI / 3), vc = t => Vm * Math.sin(w * t + 2 * PI / 3);
    const th = t => 2 * PI * frac(f * t);
    const sig = t => Math.sin(w * t) + 0.2 * Math.sin(5 * w * t);
    const wraps = Array.from({ length: Math.round(TT * f) + 1 }, (_, k) => k / f);
    EX.push({
      key: 'measurement_dq', cat: '三相系統', name: '三相量測鏈：電壓／電流感測器、abc↔dqo、FFT、RMS、THD、控制-功率介面、C 程式方塊',
      text: `三相 380 V／50 Hz 供電給 Y 接三相 RL 負載（10 Ω、20 mH，中性點浮接）。\n電壓感測器量 va、vb、vc，θ = 2π·frac(50t)；abc→dqo：vd = 0、vq = Vm = ${Vm.toFixed(3)} V、vo = 0；dqo→abc 還原 va、vb、vc。\n電流感測器量 ia，FFT（64 點）振幅 ${Im.toFixed(4)} A、相位 ${phI.toFixed(3)}°；RMS 方塊 = Vm/√2；\n訊號 sin(ωt) + 0.2·sin(5ωt) 的 THD = 20 %、基本波 v1 = sin(ωt)；控制-功率介面以前一步的控制值驅動 1 kΩ；\nC 方塊 y1 = x1·x2 + 1、y2 = ∫x1 dt（x1 = 2、x2 = θ）。`,
      sc: { dt: '20u', tt: '0.5' },
      parts: [
        ['VSIN3', 'Vs', { V: 380, f: 50 }, { n: '0', a: 'a', b: 'b', c: 'c' }],
        ['ISEN', 'CTa', { k: 1 }, { in: 'a', out: 'a2', o: 'ia' }],
        ['RL3', 'Load', { R: 10, L: '20m' }, { A: 'a2', B: 'b', C: 'c', a: 'st', b: 'st', c: 'st' }],
        ['VSEN', 'PTa', { k: 1 }, { '+': 'a', '-': '0', out: 'va' }], ['VSEN', 'PTb', { k: 1 }, { '+': 'b', '-': '0', out: 'vb' }],
        ['VSEN', 'PTc', { k: 1 }, { '+': 'c', '-': '0', out: 'vc' }],
        ['VSAW', 'Uth', { Vpk: 2 * PI, f: 50 }, { '+': 'th', '-': '0' }],
        ['ABC2DQO', 'Tdq', {}, { a: 'va', b: 'vb', c: 'vc', 'θ': 'th', d: 'vd', q: 'vq', o: 'vo' }],
        ['VP', 'Vd', {}, { n: 'vd' }], ['VP', 'Vq', {}, { n: 'vq' }], ['VP', 'Vo', {}, { n: 'vo' }],
        ['DQO2ABC', 'Tabc', {}, { d: 'vd', q: 'vq', o: 'vo', 'θ': 'th', a: 'ra', b: 'rb', c: 'rc' }],
        ['VP', 'Va_r', {}, { n: 'ra' }], ['VP', 'Vb_r', {}, { n: 'rb' }], ['VP', 'Vc_r', {}, { n: 'rc' }],
        ['VP', 'Ia', {}, { n: 'ia' }],
        ['FFT', 'Ff', { N: 64, fb: 50 }, { in: 'ia', amp: 'famp', phase: 'fph' }], ['VP', 'FFT_amp', {}, { n: 'famp' }], ['VP', 'FFT_ph', {}, { n: 'fph' }],
        ['RMS', 'Rm', { fb: 50 }, { in: 'va', out: 'vrms' }], ['VP', 'Va_rms', {}, { n: 'vrms' }],
        ['VSIN', 'U1', { Vm: 1, f: 50 }, { '+': 'h1', '-': '0' }], ['VSIN', 'U5', { Vm: 1, f: 250 }, { '+': 'h5', '-': '0' }],
        ['MATH2', 'Mx', { expr: 'x1 + 0.2*x2' }, { x1: 'h1', x2: 'h5', out: 'sig' }], ['VP', 'Sig', {}, { n: 'sig' }],
        ['THD', 'Th', { f: 50, fb: 10 }, { in: 'sig', THD: 'thd', v1: 'v1' }], ['VP', 'THD', {}, { n: 'thd' }], ['VP', 'V1', {}, { n: 'v1' }],
        ['CTOP', 'Ip', {}, { in: 'sig', out: 'pw' }], ['R', 'Rp', { R: '1k' }, { 1: 'pw', 2: '0' }], ['VP', 'V_ctop', {}, { n: 'pw' }],
        ['CONST', 'K2', { v: 2 }, { out: 'k2' }],
        ['CBLOCK', 'Cb', { nin: 2, nout: 2, glob: 'static double acc = 0;', code: 'y1 = x1*x2 + 1;\nacc = acc + x1*delt;\ny2 = acc;' }, { x1: 'k2', x2: 'th', y1: 'cy1', y2: 'cy2' }],
        ['VP', 'C_y1', {}, { n: 'cy1' }], ['VP', 'C_y2', {}, { n: 'cy2' }],
      ],
      screens: [['Vd', 'Vq', 'Vo'], ['Va_r', 'Vb_r', 'Vc_r'], ['Ia', 'FFT_amp'], ['FFT_ph'], ['Va_rms'], ['Sig', 'V1'], ['THD'], ['V_ctop'], ['C_y1', 'C_y2']],
      checks: h => {
        const T = h.t, last = T.length - 1;
        const s = h.col('V_ctop'), u = h.col('Sig');
        // previous internal step: the previous output row or a breakpoint in between (FFT samples k/(64·50) s)
        let ec = 0; for (let i = 2; i <= last; i++) { const kb = Math.ceil(T[i] * 3200 - 1e-6) - 1, tb = kb / 3200; const tp = tb > T[i - 1] + 1e-9 ? tb : T[i - 1]; ec = Math.max(ec, Math.abs(s[i] - sig(tp))); }
        const thd = h.last('THD');
        return [
          row('abc→dqo：vd = 0（最大 |vd|）', 0, Math.max(h.max('Vd'), -h.min('Vd')), Math.max(h.max('Vd'), -h.min('Vd')) < 1e-12 * Vm),
          row('abc→dqo：vq = Vm', Vm, h.max('Vq', 0, TT), h.near(h.max('Vq', 0, TT), Vm, 1e-12) && h.near(h.min('Vq', 0, TT), Vm, 1e-12)),
          row('abc→dqo：vo = 0（最大 |vo|）', 0, Math.max(h.max('Vo'), -h.min('Vo')), Math.max(h.max('Vo'), -h.min('Vo')) < 1e-12 * Vm),
          wave(h, 'Va_r', va, 0, TT, wraps, 1e-12, 'dqo→abc：va'), wave(h, 'Vb_r', vb, 0, TT, wraps, 1e-12, 'dqo→abc：vb'),
          wave(h, 'Vc_r', vc, 0, TT, wraps, 1e-12, 'dqo→abc：vc'),
          ...phasor(h, 'Ia', f, 0.46, 0.5, Im, phI, 1e-5, 0.005, '電流感測器 ia'),
          row('FFT 基本波振幅（A）', Im, h.last('FFT_amp'), h.near(h.last('FFT_amp'), Im, 1e-5)),
          row('FFT 相位（°，sin 為 0°）', phI, h.last('FFT_ph'), Math.abs(h.last('FFT_ph') - phI) < 0.01),
          row('RMS 方塊 = Vm/√2', Vm / Math.SQRT2, h.last('Va_rms'), h.near(h.last('Va_rms'), Vm / Math.SQRT2, 1e-5)),
          row('THD（穩定後，收斂 > 3/fb = 0.3 s）', 0.2, thd, h.near(thd, 0.2, 0.01)),
          ...phasor(h, 'V1', f, 0.46, 0.5, 1, 0, 2e-3, 0.2, 'THD 基本波輸出 v1'),
          row('控制-功率介面 = 前一步控制值（最大偏差）', 0, ec, ec < 1e-9),
          wave(h, 'C_y1', t => 2 * th(t) + 1, 0, TT, wraps, 1e-12, 'C 方塊 y1 = x1·x2 + 1'),
          wave(h, 'C_y2', t => 2 * t, 0, TT, [], 1e-9, 'C 方塊 y2 = ∫x1 dt（Σ x1·delt）'),
        ];
      },
    });
  }

  // =================================================================== 24
  {
    const N = 200, TT = 10e-3, t1 = 1e-3;
    EX.push({
      key: 'montecarlo_rc', cat: '分析功能', name: '蒙地卡羅容差分析：RC 分壓器（兩個電阻 ±5 % 常態、電容 ±10 % 均勻）',
      text: '10 V 經 Ra 對 Rb ∥ Cf 充電：Vo(∞) = 10·Rb/(Ra + Rb)、τ = (Ra ∥ Rb)·Cf。蒙地卡羅 200 次（#0 為標稱值），種子 7。\n容差的兩種寫法：Rb.R、Cf.C 直接指定元件參數；Ra 的 R 欄填參數檔變數 R1，再對 R1 設定容差。\nRa、Rb 為常態分佈（容差 5 % = 3σ，抽樣截斷在 ±5 % 內），Cf 為 ±10 % 均勻分佈。\n逐次驗證每一次模擬：Vo(1 ms) = Vo(∞)·(1 − e^(−1ms/τ))、Vo(10 ms) = Vo(∞)；統計：σ(Vo)/Vo ≈ 0.5·√2·(5 %/3) = 1.18 %。',
      sc: { dt: '2u', tt: '10m' },
      params: 'R1 = 1k',
      extra: [['MONTECARLO', { n: N, seed: 7, vars: 'R1 5% gauss\nRb.R 5% gauss\nCf.C 10% uniform' }]],
      parts: [
        ['VDC', 'Vin', { V: 10 }, { '+': 'in', '-': '0' }], ['R', 'Ra', { R: 'R1' }, { 1: 'in', 2: 'o' }],
        ['R', 'Rb', { R: '1k' }, { 1: 'o', 2: '0' }], ['C', 'Cf', { C: '1u' }, { 1: 'o', 2: '0' }], ['VP', 'Vo', {}, { n: 'o' }],
      ],
      screens: [['Vo']],
      checks: h => {
        const mc = h.stats.mc;
        if (!mc) return [row('蒙地卡羅結果', '', '沒有 mc 資料', false)];
        const S = mc.samples, names = mc.vars;
        const iR1 = names.indexOf('R1'), iR2 = names.indexOf('Rb.R'), iC = names.indexOf('Cf.C');
        const cols = Object.keys(h).length && S.map((_, k) => `Vo [${k === 0 ? '#0 標稱' : '#' + k}]`);
        let e1 = 0, e2 = 0;
        const vinf = [];
        S.forEach((s, k) => {
          const [R1, R2, C1] = [s[iR1], s[iR2], s[iC]];
          const Vi = 10 * R2 / (R1 + R2), tau = R1 * R2 / (R1 + R2) * C1;
          vinf.push(Vi);
          e1 = Math.max(e1, Math.abs(h.at(cols[k], t1) - Vi * (1 - Math.exp(-t1 / tau))) / Vi);
          e2 = Math.max(e2, Math.abs(h.last(cols[k]) - Vi * (1 - Math.exp(-TT / tau))) / Vi);
        });
        const mean = a => a.reduce((x, y) => x + y, 0) / a.length;
        const sd = a => { const m = mean(a); return Math.sqrt(a.reduce((x, y) => x + (y - m) ** 2, 0) / (a.length - 1)); };
        const rel = i => S.slice(1).map(s => s[i] / S[0][i] - 1);
        const st = mc.stats.find(x => x.probe === 'Vo' && x.metric === '末值');
        const fin = S.map((_, k) => h.last(cols[k]));
        const sR = sd(rel(iR1)), sR2 = sd(rel(iR2)), cU = rel(iC);
        const se = s => s / Math.sqrt(2 * (N - 2));
        // Gaussian truncated at ±3σ: σ = 0.98658·tol/3
        const TR = 0.98658, sR0 = 0.05 / 3 * TR;
        const sig = 5 * 0.5 * Math.SQRT2 * sR0;
        const rR = [...rel(iR1), ...rel(iR2)];
        return [
          row('執行次數（含 #0 標稱）', N, S.length, S.length === N || S.length === N + 1),
          row('#0 為標稱值 R1 = Rb.R = 1k、Cf.C = 1µ', 1, +(S[0][iR1] === 1e3 && S[0][iR2] === 1e3 && Math.abs(S[0][iC] - 1e-6) < 1e-18), S[0][iR1] === 1e3 && S[0][iR2] === 1e3 && Math.abs(S[0][iC] - 1e-6) < 1e-18),
          row('每次 Vo(1 ms) 對解析解最大相對誤差', 0, e1, e1 < 1e-5),
          row('每次 Vo(10 ms) 對解析解最大相對誤差', 0, e2, e2 < 1e-6),
          row('R1（參數檔變數）抽樣 σ/R（應 = 0.98658·5 %/3）', sR0, sR, Math.abs(sR - sR0) < 4 * se(sR0)),
          row('Rb.R（元件參數）抽樣 σ/R（應 = 0.98658·5 %/3）', sR0, sR2, Math.abs(sR2 - sR0) < 4 * se(sR0)),
          row('常態分佈截斷：R1、Rb.R 全部在 ±5 % 內', 0.05, Math.max(...rR.map(Math.abs)), Math.max(...rR.map(Math.abs)) <= 0.05 + 1e-12),
          row('Cf.C 均勻分佈範圍 ⊂ ±10 %', 0.1, Math.max(...cU.map(Math.abs)), Math.max(...cU.map(Math.abs)) <= 0.1 + 1e-12),
          row('Cf.C 均勻分佈 σ（應 = 10 %/√3）', 0.1 / Math.sqrt(3), sd(cU), Math.abs(sd(cU) - 0.1 / Math.sqrt(3)) < 4 * se(0.1 / Math.sqrt(3))),
          row('統計「末值」平均 = 各次末值平均', mean(fin), st && st.mean, !!st && h.near(st.mean, mean(fin), 1e-12)),
          row('統計「末值」σ = 各次末值 σ', sd(fin), st && st.std, !!st && h.near(st.std, sd(fin), 1e-9)),
          row('統計 最小／最大', `${Math.min(...fin).toFixed(4)}／${Math.max(...fin).toFixed(4)}`, st && `${st.min.toFixed(4)}／${st.max.toFixed(4)}`, !!st && st.min === Math.min(...fin) && st.max === Math.max(...fin)),
          row('Vo 平均 ≈ 5 V（4σ/√N 內）', 5, mean(vinf), Math.abs(mean(vinf) - 5) < 4 * sig / Math.sqrt(N)),
          row('σ(Vo) ≈ 0.5·√2·σ(R)·5 V（一階誤差傳遞）', sig, sd(vinf), Math.abs(sd(vinf) - sig) < 4 * se(sig)),
        ];
      },
    });
  }

  // =================================================================== 25
  {
    const R1 = 2, Lf = 1e-3, Cf = 10e-6, RL = 20, ks = 0.5, kp = 0.8, Ti = 0.5e-3;
    const s_ = f => C(0, 2 * PI * f);
    const Gp = f => { const s = s_(f); const Zp = div(C(RL), add(C(1), mul(s, C(RL * Cf)))); return div(Zp, add(add(C(R1), mul(s, C(Lf))), Zp)); };
    const PIc = f => mul(C(kp), add(C(1), div(C(1), mul(s_(f), C(Ti)))));
    const Lg = f => mul(mul(PIc(f), C(ks)), Gp(f));
    const neg = z => C(-z.re, -z.im);
    EX.push({
      key: 'loop_gain', cat: '分析功能', name: '迴路增益量測（AC 掃頻、迴路探棒、閉迴路 RLC 電壓控制）',
      ac: true,
      text: 'PI 控制器（k = 0.8、T = 0.5 ms）經控制-功率介面驅動 RLC 濾波器（2 Ω、1 mH、10 µF、20 Ω 負載），電壓感測 0.5 回授。\n激勵源 Vac 經加法器注入控制器輸出：迴路探棒 Tloop（極性 1）量測 T = −V(u)/V(u+v) = PI(s)·0.5·Gp(s)，\nHloop（極性 0）量測 H = V(u)/V(u+v) = −T；AC 探棒 Vo = Gp/(1 + T)。以線性頻域法掃 10 Hz ~ 100 kHz（80 點）與解析轉移函數逐點比較。',
      sc: { dt: '1u', tt: '10m' },
      extra: [['ACSWEEP', { f0: 10, f1: '100k', n: 80, flag: 0, src: 'Vac', a0: 1, a1: 1, method: 1 }]],
      parts: [
        ['CONST', 'Ref', { v: 1 }, { out: 'ref' }], ['SUB2', 'Err', {}, { 'in+': 'ref', 'in-': 'y', out: 'e' }],
        ['PI', 'Ctrl', { k: kp, T: '0.5m' }, { in: 'e', out: 'u' }],
        ['VSIN', 'Vac', { Vm: 0, f: '1k' }, { '+': 'v', '-': '0' }], ['SUM2', 'Inj', { k1: 1, k2: 1 }, { in1: 'u', in2: 'v', out: 'ui' }],
        ['ACP2', 'Tloop', { neg: 1 }, { dot: 'ui', x: 'u' }], ['ACP2', 'Hloop', { neg: 0 }, { dot: 'ui', x: 'u' }],
        ['CTOP', 'Drv', {}, { in: 'ui', out: 'p1' }], ['R', 'Rs', { R: R1 }, { 1: 'p1', 2: 'p2' }], ['L', 'Lf', { L: '1m' }, { 1: 'p2', 2: 'o' }],
        ['C', 'Cf', { C: '10u' }, { 1: 'o', 2: '0' }], ['R', 'RL', { R: RL }, { 1: 'o', 2: '0' }],
        ['VSEN', 'Sen', { k: ks }, { '+': 'o', '-': '0', out: 'y' }], ['ACP', 'Vo', {}, { n: 'o' }],
      ],
      checks: h => {
        const F = h.col('Freq');
        const cmp = (name, fn, lab) => {
          let e = 0, at = 0;
          F.forEach((f, i) => { const z = fn(f), m = C(h.col(name + '_re')[i], h.col(name + '_im')[i]); const d = cabs(add(m, neg(z))) / cabs(z); if (d > e) { e = d; at = f; } });
          return row(lab, 0, e, e < 1e-9, `最大相對誤差 @ ${at.toFixed(1)} Hz（${F.length} 點）`);
        };
        // crossover |T| = 1 and phase margin: analytic (bisection) vs the sweep (log-interpolated between points)
        let lo = 10, hi = 1e5; for (let k = 0; k < 200; k++) { const m = Math.sqrt(lo * hi); if (cabs(Lg(m)) > 1) lo = m; else hi = m; }
        const fc = lo, pm = 180 + carg(Lg(fc));
        const mag = i => Math.hypot(h.col('Tloop_re')[i], h.col('Tloop_im')[i]);
        let fcs = NaN, pms = NaN;
        for (let i = 1; i < F.length; i++) if (mag(i - 1) > 1 && mag(i) <= 1) {
          const a = Math.log(mag(i - 1)), b = Math.log(mag(i)), r = a / (a - b);
          fcs = Math.exp(Math.log(F[i - 1]) + r * (Math.log(F[i]) - Math.log(F[i - 1])));
          const p0 = Math.atan2(h.col('Tloop_im')[i - 1], h.col('Tloop_re')[i - 1]), p1 = Math.atan2(h.col('Tloop_im')[i], h.col('Tloop_re')[i]);
          pms = 180 + (p0 + r * (p1 - p0)) * 180 / PI;
        }
        return [
          cmp('Tloop', Lg, '迴路探棒（極性 1）T = PI·0.5·Gp'),
          cmp('Hloop', f => neg(Lg(f)), '迴路探棒（極性 0）H = −T'),
          cmp('Vo', f => div(Gp(f), add(C(1), Lg(f))), 'AC 探棒 Vo = Gp/(1 + T)'),
          row('穿越頻率（Hz，掃頻點間對數內插）', fc, fcs, h.near(fcs, fc, 0.02)),
          row('相位裕度（°）', pm, pms, Math.abs(pms - pm) < 1),
        ];
      },
    });
  }

  // =================================================================== 26
  {
    const frac = x => x - Math.floor(x);
    const ws = 2 * PI * 50, Vm = 100;
    const vs = t => Vm * Math.sin(ws * t);
    const deg = t => 360 * frac(50 * t);
    const hi1k = t => frac(1000 * t) < 0.5;
    const edges1k = Array.from({ length: 81 }, (_, k) => k * 0.5e-3);
    const ac = [0, 10e-3, 20e-3, 30e-3, 40e-3, 10e-3 / 3, 20e-3 / 3 + 10e-3, 23.3333333e-3, 10e-3 / 3 + 30e-3];
    const g1k = t => frac(1000 * t) < 0.3;
    EX.push({
      key: 'power_gate', cat: '功率元件', name: '基極／閘極接功率電路：NPN、PNP、SCR、TRIAC、GTO',
      text: '開關型電晶體與閘流體的基極／閘極直接接功率電路（接面為 0.7 V 二極體，會汲取電流）：\nNPN：5 V 方波經 1 kΩ 驅動基極 → 高電位時 Ib = (5 − 0.7)/1k = 4.3 mA、C-E 導通；PNP：射極 12 V，基極經 1 kΩ 拉低時導通；\nSCR／TRIAC：100 V／50 Hz，閘極脈衝（脈衝變壓器式、參考陰極／MT1）在 60° 觸發；GTO：有閘極電流時導通、電流停止即關斷。',
      sc: { dt: '2u', tt: '40m' },
      parts: [
        ['VSQU', 'Ub1', { Vpp: 5, f: '1k', D: 0.5 }, { '+': 'b1s', '-': '0' }], ['IP', 'Ib_npn', {}, { in: 'b1s', out: 'b1x' }], ['R', 'Rb1', { R: '1k' }, { 1: 'b1x', 2: 'b1' }],
        ['VDC', 'Vcc1', { V: 12 }, { '+': 'vcc1', '-': '0' }], ['R', 'Rc1', { R: 100 }, { 1: 'vcc1', 2: 'c1' }],
        ['NPN', 'Q1', {}, { C: 'c1', E: '0', B: 'b1' }], ['VP', 'Vc_npn', {}, { n: 'c1' }],
        ['VDC', 'Vcc2', { V: 12 }, { '+': 'e2', '-': '0' }], ['PNP', 'Q2', {}, { C: 'c2', E: 'e2', B: 'b2' }],
        ['R', 'Rb2', { R: '1k' }, { 1: 'b2', 2: 'b2s' }], ['VSQU', 'Ub2', { Vpp: 12, f: '1k', D: 0.5 }, { '+': 'b2s', '-': '0' }],
        ['R', 'Rc2', { R: 100 }, { 1: 'c2', 2: '0' }], ['VP', 'Vc_pnp', {}, { n: 'c2' }],
        ['VSIN', 'Vac', { Vm, f: 50 }, { '+': 'ac', '-': '0' }],
        ['THY', 'S1', {}, { A: 'ac', K: 'k3', Gate: 'g3' }], ['R', 'Rl3', { R: 10 }, { 1: 'k3', 2: '0' }], ['VP', 'Vload_scr', {}, { n: 'k3' }],
        ['VSQU', 'Ug3', { Vpp: 5, f: 50, D: 0.05, ph: 60 }, { '+': 'g3s', '-': 'k3' }], ['R', 'Rg3', { R: 100 }, { 1: 'g3s', 2: 'g3' }],
        ['TRIAC', 'T1', {}, { 1: 'ac', 2: 'k4', Gate: 'g4' }], ['R', 'Rl4', { R: 10 }, { 1: 'k4', 2: '0' }], ['VP', 'Vload_triac', {}, { n: 'k4' }],
        ['VSQU', 'Ug4', { Vpp: 5, f: 100, D: 0.1, ph: 120 }, { '+': 'g4s', '-': 'k4' }], ['R', 'Rg4', { R: 100 }, { 1: 'g4s', 2: 'g4' }],
        ['VDC', 'Vdc5', { V: 50 }, { '+': 'v5', '-': '0' }], ['R', 'Rl5', { R: 10 }, { 1: 'v5', 2: 'a5' }],
        ['GTO', 'G1', {}, { A: 'a5', K: '0', Gate: 'g5' }], ['VP', 'Va_gto', {}, { n: 'a5' }],
        ['VSQU', 'Ug5', { Vpp: 5, f: '1k', D: 0.3 }, { '+': 'g5s', '-': '0' }], ['R', 'Rg5', { R: 100 }, { 1: 'g5s', 2: 'g5' }],
      ],
      screens: [['Vc_npn', 'Vc_pnp'], ['Ib_npn'], ['Vload_scr', 'Vload_triac'], ['Va_gto']],
      checks: h => {
        const W = (p, fn, skip, tol, lab) => wave(h, p, fn, 0, 40e-3, skip, tol, lab);
        const gEdges = [].concat(...Array.from({ length: 41 }, (_, k) => [k * 1e-3, k * 1e-3 + 0.3e-3]));
        const scrOn = t => { const d = deg(t); return d >= 60 && d < 180; };
        const triOn = t => { const d = deg(t); return (d >= 60 && d < 180) || (d >= 240 && d < 360); };
        return [
          W('Vc_npn', t => (hi1k(t) ? 0 : 12), edges1k, 2e-4, 'NPN：基極高電位時導通（Vc = 0）、低電位時關斷（Vc = 12 V）'),
          W('Ib_npn', t => (hi1k(t) ? 4.3e-3 : 0), edges1k, 2e-4, 'NPN 基極電流 = (5 − 0.7)/1 kΩ'),
          W('Vc_pnp', t => (hi1k(t) ? 0 : 12), edges1k, 2e-4, 'PNP：基極拉低時導通（Vc = 12 V）'),
          W('Vload_scr', t => (scrOn(t) ? vs(t) : 0), ac, 1e-4, 'SCR：60° 觸發，導通至電流過零'),
          W('Vload_triac', t => (triOn(t) ? vs(t) : 0), ac, 1e-4, 'TRIAC：每半週 60° 觸發（雙向）'),
          W('Va_gto', t => (g1k(t) ? 0 : 50), gEdges, 2e-4, 'GTO：閘極有電流時導通、停止即關斷'),
        ];
      },
    });
  }
};
