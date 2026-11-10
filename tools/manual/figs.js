// Demonstration circuits of the user manual: one figure per element type (tools/manual/gen.js).
// part: [type, name, {params}, {pin: net}]   net '0' = ground; nets are joined by label name (examples/gen.js).
// figure: { sc, parts, plots: [{ y: [probe…], unit, label }], or xy: { x, y: [...] }, or ac + bode: [probe…],
//           cap (caption, HTML), lab (legend names), t: [t0, t1] (window), variants (several runs),
//           params (parameter file), extra (analysis elements), differs (known model difference to the
//           reference engine: compared, reported, not failed), nocmp (not compared, with the reason) }
'use strict';
const F = {};

const vp = (name, net) => ['VP', name, {}, { n: net }];
const vp2 = (name, a, b) => ['VP2', name, {}, { '+': a, '-': b }];
const ip = (name, a, b) => ['IP', name, {}, { in: a, out: b }];
const src = (type, name, params, p, n = '0') => [type, name, params, { '+': p, '-': n }];
const R = (name, a, b, r) => ['R', name, { R: r }, { 1: a, 2: b }];
// logic square wave 0 / 1
const sq = (name, net, f, D = 0.5, ph = 0) => src('VSQU', name, { Vpp: 1, f, D, ph }, net);
const sine = (name, net, Vm, f, more = {}) => src('VSIN', name, Object.assign({ Vm, f }, more), net);
const ramp = (name, net, v0, v1, t1) => src('VPWL', name, { vals: `${v0} ${v1}`, times: `0 ${t1}` }, net);
const dc = (name, net, v) => src('VDC', name, { V: v }, net);
const cst = (name, net, v) => ['CONST', name, { v }, { out: net }];

/* ====================================================================== voltage sources */
const vsrc = (type, a, b, labA, labB, sc, cap, more = {}) => Object.assign({
  sc,
  parts: [src(type, 'S1', a, 'a'), vp('v1', 'a')].concat(b ? [src(type, 'S2', b, 'b'), vp('v2', 'b')] : []),
  plots: [{ y: b ? ['v1', 'v2'] : ['v1'], unit: 'V' }],
  lab: { v1: labA, v2: labB },
  cap,
}, more);

F.VDC = vsrc('VDC', {}, { V: -5 }, 'V = 10 V（預設）', 'V = −5 V', { dt: '10u', tt: '10m' }, '直流源輸出固定電壓，不隨時間改變。');
F.VBAT = vsrc('VBAT', {}, null, 'V = 10 V', null, { dt: '10u', tt: '10m' }, '直流電池：與直流電壓源相同的理想直流源，只是符號不同。');
F.VGDC = { sc: { dt: '10u', tt: '10m' }, parts: [['VGDC', 'S1', {}, { n: 'a' }], vp('v1', 'a')], plots: [{ y: ['v1'], unit: 'V' }], lab: { v1: 'V = 10 V' }, cap: '單端直流源：接腳對地輸出固定電壓，不需另外接地。' };
F.VGDCT = { sc: { dt: '10u', tt: '10m' }, parts: [['VGDCT', 'S1', { V: 5 }, { n: 'a' }], vp('v1', 'a')], plots: [{ y: ['v1'], unit: 'V' }], lab: { v1: 'V = 5 V' }, cap: '單端直流源（T 形符號）：接腳對地輸出固定電壓。' };
F.VSIN = vsrc('VSIN', {}, { ph: 90, off: 5 }, 'Vm = 10 V、f = 60 Hz（預設）', '相位 90°、偏移 5 V', { dt: '20u', tt: '33.34m' }, '正弦波 v = Vm·sin(2πft + θ) + Voffset：兩個週期，第二條曲線加上 90° 相位與 5 V 直流偏移。');
F.VTRI = vsrc('VTRI', {}, { D: 0.2 }, 'D = 0.5（預設）', 'D = 0.2', { dt: '0.1u', tt: '0.2m' }, '三角波（Vpp = 1 V、10 kHz）：D 為上升時間占週期的比例，D = 0.2 時上升快、下降慢。');
F.VSAW = vsrc('VSAW', {}, { off: -0.5 }, 'Vpk = 1 V（預設）', '偏移 −0.5 V', { dt: '0.1u', tt: '0.2m' }, '鋸齒波（10 kHz）：每個週期由 Voffset 線性上升到 Voffset + Vpeak 後立即回到起點。');
F.VSQU = vsrc('VSQU', {}, { D: 0.25, off: -1, Vpp: 2 }, 'Vpp = 1 V、D = 0.5（預設）', 'Vpp = 2 V、D = 0.25、偏移 −1 V', { dt: '0.1u', tt: '0.2m' }, '方波（10 kHz）：週期開始時為高電位 Voffset + Vpp，經過 D·T 後轉為低電位 Voffset。');
F.VSTEP = vsrc('VSTEP', {}, { V: -0.5, ts: '2m' }, 'V = 1 V、Tstep = 1 ms（預設）', 'V = −0.5 V、Tstep = 2 ms', { dt: '5u', tt: '3m' }, '步階源：Tstep 之前為 0，之後為 Vstep。');
F.VSTEP2 = vsrc('VSTEP2', {}, { V1: 0.2, V2: 0.8, tr: '1m' }, '0 → 1 V、轉換時間 0（預設）', '0.2 → 0.8 V、轉換時間 1 ms', { dt: '5u', tt: '3m' }, '雙準位步階：在 Tstep 時由 V1 轉到 V2；設定轉換時間時以直線斜坡轉換。');
F.VPWL = vsrc('VPWL', {}, { f: 250, vals: '0 1 0.5 0.5' }, '預設（不重複）', '0 1 0.5 0.5，f = 250 Hz 重複', { dt: '5u', tt: '8m' }, '分段線性源：把 (Ti, Vi) 各點連成折線（預設 0、1、1、0 V 於 0、1、2、3 ms）；設定頻率時每個週期重複。');
F.VPWL2 = vsrc('VPWL2', {}, { pairs: '(0,0) (0.5m,1) (1m,-1) (2m,0)' }, '預設 (0,0) (1m,1) (2m,1) (3m,0)', '(0,0) (0.5m,1) (1m,−1) (2m,0)', { dt: '5u', tt: '4m' }, '分段線性源（成對輸入）：以 (時間, 數值) 成對寫出各點。');
F.VRAND = { sc: { dt: '20u', tt: '2m' }, parts: [src('VRAND', 'S1', {}, 'a'), vp('v1', 'a')], plots: [{ y: ['v1'], unit: 'V' }], lab: { v1: 'Vpp = 1 V、seed = 1' }, cap: '亂數源：每個時間步輸出 Vpp·n + Voffset，n 為 0 到 1 的均勻亂數；相同的種子得到相同的序列。', nocmp: '亂數序列' };
F.VMATH = { sc: { dt: '20u', tt: '33.34m' }, parts: [src('VMATH', 'S1', {}, 'a'), vp('v1', 'a'), src('VMATH', 'S2', { expr: 'sin(2*pi*60*t) + 0.3*sin(2*pi*180*t+pi)' }, 'b'), vp('v2', 'b')], plots: [{ y: ['v1', 'v2'], unit: 'V' }], lab: { v1: 'sin(2*pi*60*t)（預設）', v2: 'sin(2*pi*60*t) + 0.3*sin(2*pi*180*t+pi)' }, cap: '數學函數源：輸出為時間 t 的任意運算式（函數見「運算式語法」），例如加上三次諧波。' };
F.VSIN3 = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', {}, { n: 'n', a: 'a', b: 'b', c: 'c' }], R('Ra', 'a', 'n', 1000), R('Rb', 'b', 'n', 1000), R('Rc', 'c', 'n', 1000), vp2('va', 'a', 'n'), vp2('vb', 'b', 'n'), vp2('vc', 'c', 'n')], plots: [{ y: ['va', 'vb', 'vc'], unit: 'V' }], cap: '三相 Y 接正弦源（V = 380 V 線電壓有效值，60 Hz）：相電壓峰值 380·√2/√3 = 310 V，B、C 相分別落後、超前 A 相 120°。' };
// controlled sources: the control quantity and the output
F.VVCVS = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vc', 'c', 1, 100), ['VVCVS', 'E1', { k: 5 }, { '+': 'o', '-': '0', 'in+': 'c', 'in-': '0' }], R('RL', 'o', '0', 10), vp('vin', 'c'), vp('vout', 'o')], plots: [{ y: ['vin', 'vout'], unit: 'V' }], lab: { vin: '控制電壓 vin', vout: '輸出（k = 5）' }, cap: '電壓控制電壓源：輸出 = k·(v(in+) − v(in−))，此例 k = 5。' };
F.VVCVSV = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'c1', 1, 500), ramp('V2', 'c2', 0, 2, '20m'), ['VVCVSV', 'E1', {}, { '+': 'o', '-': '0', vin1: 'c1', vin2: 'c2' }], R('RL', 'o', '0', 10), vp('vin1', 'c1'), vp('vin2', 'c2'), vp('vout', 'o')], plots: [{ y: ['vin1', 'vin2'], unit: 'V' }, { y: ['vout'], unit: 'V', c: 2 }], cap: '可變增益電壓控制電壓源：輸出 = (k·vin2)·vin1。vin2 由 0 緩慢升到 2，輸出振幅隨之變大。' };
F.VNONM = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'c1', 1, 500), sine('V2', 'c2', 1, 50), ['VNONM', 'E1', {}, { '+': 'o', '-': '0', vin1: 'c1', vin2: 'c2' }], R('RL', 'o', '0', 10), vp('vin1', 'c1'), vp('vin2', 'c2'), vp('vout', 'o')], plots: [{ y: ['vin1', 'vin2'], unit: 'V' }, { y: ['vout'], unit: 'V', c: 2 }], cap: '非線性電壓源（乘法）：輸出 = k·vin1·vin2，兩個正弦相乘得到調變波形。' };
F.VNOND = { sc: { dt: '10u', tt: '10m' }, parts: [dc('V1', 'c1', 1), ramp('V2', 'c2', 0.2, 2, '10m'), ['VNOND', 'E1', {}, { '+': 'o', '-': '0', vin1: 'c1', vin2: 'c2' }], R('RL', 'o', '0', 10), vp('vin2', 'c2'), vp('vout', 'o')], xy: { x: 'vin2', y: ['vout'], xunit: 'V', yunit: 'V', xlabel: 'vin2（vin1 = 1 V）' }, lab: { vout: '輸出 = k·vin1/vin2' }, cap: '非線性電壓源（除法）：vin1 固定 1 V、vin2 由 0.2 V 掃到 2 V，輸出 = k·vin1/vin2 為雙曲線。' };
F.VNONSQ = { sc: { dt: '10u', tt: '10m' }, parts: [ramp('V1', 'c1', 0, 10, '10m'), ['VNONSQ', 'E1', {}, { '+': 'o', '-': '0', vin1: 'c1' }], R('RL', 'o', '0', 10), vp('vin1', 'c1'), vp('vout', 'o')], xy: { x: 'vin1', y: ['vout'], xunit: 'V', yunit: 'V', xlabel: 'vin1' }, lab: { vout: '輸出 = k·√vin1' }, cap: '非線性電壓源（平方根）：輸出 = k·√vin1。' };
F.VPOWERS = { sc: { dt: '10u', tt: '10m' }, parts: [ramp('V1', 'c1', -2, 2, '10m'), ['VPOWERS', 'E1', {}, { '+': 'o', '-': '0', vin: 'c1' }], R('RL', 'o', '0', 10), vp('vin', 'c1'), vp('vout', 'o')], xy: { x: 'vin', y: ['vout'], xunit: 'V', yunit: 'V', xlabel: 'vin' }, lab: { vout: 'sign(vin)·k·(k1·|vin|)^k2' }, cap: '冪次電壓源（k2 = 2）：輸出 = sign(vin)·k·(k1·|vin|)^k2，保留輸入的正負號。' };
// current-controlled: the control current flows in a resistor branch
F.VCCVS = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vs', 's', 1, 100), ip('ic', 's', 'm'), R('Rc', 'm', 'k', 1), R('Rk', 'k', '0', 1), ['VCCVS', 'H1', { k: 2 }, { '+': 'o', '-': '0', 'c+': 'm', 'c-': 'k' }], R('RL', 'o', '0', 10), vp('vout', 'o')], plots: [{ y: ['ic'], unit: 'A' }, { y: ['vout'], unit: 'V', c: 1 }], lab: { ic: '控制電流（Rc 的電流）', vout: '輸出（k = 2 Ω）' }, cap: '電流控制電壓源：控制端跨接在 Rc 兩端，輸出 = k·i(Rc)，此例 k = 2（Rc、Rk 各 1 Ω）。' };
F.VCCVS1 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vs', 's', 1, 100), ['VCCVS1', 'H1', { k: 2 }, { '+': 'o', '-': '0', in: 's', out: 'm' }], ip('ic', 'm', 'm2'), R('Rc', 'm2', '0', 1), R('RL', 'o', '0', 10), vp('vout', 'o')], plots: [{ y: ['ic'], unit: 'A' }, { y: ['vout'], unit: 'V', c: 1 }], lab: { ic: '控制電流', vout: '輸出（k = 2 Ω）' }, cap: '電流控制電壓源（串接式）：控制電流由 in 流入、out 流出，輸出 = k·i。' };

/* ====================================================================== current sources */
// every current source drives a 1 Ω resistor through a current probe
const isrc = (type, a, b, labA, labB, sc, cap, more = {}) => Object.assign({
  sc,
  parts: [src(type, 'S1', a, 'a'), ip('i1', 'a', 'a2'), R('R1', 'a2', '0', 1)].concat(b ? [src(type, 'S2', b, 'b'), ip('i2', 'b', 'b2'), R('R2', 'b2', '0', 1)] : []),
  plots: [{ y: b ? ['i1', 'i2'] : ['i1'], unit: 'A' }],
  lab: { i1: labA, i2: labB },
  cap,
}, more);
const iNote = '電流由 + 端流出、經 1 Ω 電阻流回 − 端。';
F.IDC = isrc('IDC', {}, { V: -5 }, 'I = 10 A（預設）', 'I = −5 A', { dt: '10u', tt: '10m' }, '直流電流源：輸出固定電流。' + iNote);
F.ISIN = isrc('ISIN', {}, { ph: 90, off: 5 }, 'Im = 10 A、60 Hz（預設）', '相位 90°、偏移 5 A', { dt: '20u', tt: '33.34m' }, '正弦電流源 i = Im·sin(2πft + θ) + Ioffset。' + iNote);
F.ITRI = isrc('ITRI', {}, { D: 0.2 }, 'D = 0.5（預設）', 'D = 0.2', { dt: '0.1u', tt: '0.2m' }, '三角波電流源（1 A、10 kHz）。' + iNote);
F.ISQU = isrc('ISQU', {}, { D: 0.25 }, 'D = 0.5（預設）', 'D = 0.25', { dt: '0.1u', tt: '0.2m' }, '方波電流源（1 A、10 kHz）。' + iNote);
F.ISTEP = isrc('ISTEP', {}, { V: 0.5, ts: '2m' }, '1 A、1 ms（預設）', '0.5 A、2 ms', { dt: '5u', tt: '3m' }, '步階電流源。' + iNote);
F.ISTEP2 = isrc('ISTEP2', {}, { V1: 0.2, V2: 0.8, tr: '1m' }, '0 → 1 A（預設）', '0.2 → 0.8 A、轉換 1 ms', { dt: '5u', tt: '3m' }, '雙準位步階電流源。' + iNote);
F.IPWL = isrc('IPWL', {}, { f: 250, vals: '0 1 0.5 0.5' }, '預設（不重複）', 'f = 250 Hz 重複', { dt: '5u', tt: '8m' }, '分段線性電流源。' + iNote);
F.IPWL2 = isrc('IPWL2', {}, { pairs: '(0,0) (0.5m,1) (1m,-1) (2m,0)' }, '預設', '(0,0) (0.5m,1) (1m,−1) (2m,0)', { dt: '5u', tt: '4m' }, '分段線性電流源（成對輸入）。' + iNote);
F.IRAND = Object.assign(isrc('IRAND', {}, null, 'Ipp = 1 A、seed = 1', null, { dt: '20u', tt: '2m' }, '亂數電流源：每個時間步輸出 Ipp·n + Ioffset。' + iNote), { nocmp: '亂數序列' });
F.IMATH = isrc('IMATH', {}, { expr: '2*exp(-t/5m)*sin(2*pi*300*t)' }, 'sin(2*pi*60*t)（預設）', '2*exp(-t/5m)*sin(2*pi*300*t)', { dt: '20u', tt: '20m' }, '數學函數電流源：輸出為時間 t 的運算式。' + iNote);
F.IVCCS = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vc', 'c', 1, 100), ['IVCCS', 'G1', { k: 2 }, { '+': 'o', '-': '0', 'in+': 'c', 'in-': '0' }], ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1), vp('vin', 'c')], plots: [{ y: ['vin'], unit: 'V' }, { y: ['iout'], unit: 'A', c: 1 }], lab: { vin: '控制電壓', iout: '輸出電流（k = 2 A/V）' }, cap: '電壓控制電流源：輸出 = k·(v(in+) − v(in−))。' };
F.IVCCSV = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'c1', 1, 500), ramp('V2', 'c2', 0, 2, '20m'), ['IVCCSV', 'G1', {}, { '+': 'o', '-': '0', vin1: 'c1', vin2: 'c2' }], ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1), vp('vin1', 'c1'), vp('vin2', 'c2')], plots: [{ y: ['vin1', 'vin2'], unit: 'V' }, { y: ['iout'], unit: 'A', c: 2 }], cap: '可變增益電壓控制電流源：輸出 = (k·vin2)·vin1。' };
F.INONM = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'c1', 1, 500), sine('V2', 'c2', 1, 50), ['INONM', 'G1', {}, { '+': 'o', '-': '0', vin1: 'c1', vin2: 'c2' }], ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1), vp('vin1', 'c1'), vp('vin2', 'c2')], plots: [{ y: ['vin1', 'vin2'], unit: 'V' }, { y: ['iout'], unit: 'A', c: 2 }], cap: '非線性電流源（乘法）：輸出 = k·vin1·vin2。' };
F.INOND = { sc: { dt: '10u', tt: '10m' }, parts: [dc('V1', 'c1', 1), ramp('V2', 'c2', 0.2, 2, '10m'), ['INOND', 'G1', {}, { '+': 'o', '-': '0', vin1: 'c1', vin2: 'c2' }], ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1), vp('vin2', 'c2')], xy: { x: 'vin2', y: ['iout'], xunit: 'V', yunit: 'A', xlabel: 'vin2（vin1 = 1 V）' }, lab: { iout: '輸出 = k·vin1/vin2' }, cap: '非線性電流源（除法）：輸出 = k·vin1/vin2。' };
F.INONSQ = { sc: { dt: '10u', tt: '10m' }, parts: [ramp('V1', 'c1', 0, 10, '10m'), ['INONSQ', 'G1', {}, { '+': 'o', '-': '0', vin1: 'c1' }], ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1), vp('vin1', 'c1')], xy: { x: 'vin1', y: ['iout'], xunit: 'V', yunit: 'A', xlabel: 'vin1' }, lab: { iout: '輸出 = k·√vin1' }, cap: '非線性電流源（平方根）：輸出 = k·√vin1。' };
F.ICCCS = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vs', 's', 1, 100), ip('ic', 's', 'm'), R('Rc', 'm', 'k', 1), R('Rk', 'k', '0', 1), ['ICCCS', 'F1', { k: 3 }, { '+': 'o', '-': '0', 'c+': 'm', 'c-': 'k' }], ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1)], plots: [{ y: ['ic', 'iout'], unit: 'A' }], lab: { ic: '控制電流（Rc）', iout: '輸出電流（k = 3）' }, cap: '電流控制電流源：控制端跨接在 Rc 兩端，輸出 = k·i(Rc)。' };
F.ICCCS1 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vs', 's', 1, 100), ['ICCCS1', 'F1', { k: 3 }, { '+': 'o', '-': '0', in: 's', out: 'm' }], ip('ic', 'm', 'm2'), R('Rc', 'm2', '0', 1), ip('iout', 'o', 'o2'), R('RL', 'o2', '0', 1)], plots: [{ y: ['ic', 'iout'], unit: 'A' }], lab: { ic: '控制電流', iout: '輸出電流（k = 3）' }, cap: '電流控制電流源（串接式）：控制電流由 in 流入、out 流出，輸出 = k·i。' };

/* ====================================================================== RLC branches */
const vsrcIn = (name, net, Vm, f) => sine(name, net, Vm, f);
F.R = { sc: { dt: '20u', tt: '40m' }, parts: [vsrcIn('Vs', 'a', 10, 50), ip('i', 'a', 'b'), ['R', 'R1', { R: 10 }, { 1: 'b', 2: '0' }], vp('v', 'b')], plots: [{ y: ['v'], unit: 'V' }, { y: ['i'], unit: 'A', c: 1 }], lab: { v: '電壓 v', i: '電流 i' }, cap: 'R = 10 Ω 接 10 V、50 Hz 正弦電壓：i = v/R，電流與電壓同相。' };
F.L = { sc: { dt: '2u', tt: '4m' }, parts: [src('VSQU', 'Vs', { Vpp: 10, off: -5, f: '1k' }, 'a'), ip('i', 'a', 'b'), ['L', 'L1', {}, { 1: 'b', 2: '0' }], vp('v', 'b')], plots: [{ y: ['v'], unit: 'V' }, { y: ['i'], unit: 'A', c: 1 }], lab: { v: '電壓 v（±5 V 方波）', i: '電流 i' }, cap: 'L = 1 mH 接 ±5 V、1 kHz 方波：v = L·di/dt，電流以 ±5000 A/s 的斜率上下變化成三角波（由 0 開始）。' };
F.C = { sc: { dt: '2u', tt: '4m' }, parts: [src('ISQU', 'Is', { Vpp: 2, off: -1, f: '1k' }, 'a'), ip('i', 'a', 'b'), ['C', 'C1', {}, { 1: 'b', 2: '0' }], vp('v', 'b')], plots: [{ y: ['i'], unit: 'A' }, { y: ['v'], unit: 'V', c: 1 }], lab: { i: '電流 i（±1 A 方波）', v: '電容電壓 v' }, cap: 'C = 100 µF 由 ±1 A、1 kHz 方波電流充放電：i = C·dv/dt，電壓以 ±10⁴ V/s 的斜率變化成三角波（由 0 開始）。' };
F.RL = { sc: { dt: '5u', tt: '6m' }, parts: [src('VSTEP', 'Vs', { V: 10 }, 'a'), ip('i', 'a', 'b'), ['RL', 'B1', {}, { 1: 'b', 2: '0' }]], plots: [{ y: ['i'], unit: 'A' }], lab: { i: '電流 i' }, cap: 'R-L 支路（1 Ω、1 mH）在 1 ms 加上 10 V 步階：i = 10·(1 − e^(−t/τ))，τ = L/R = 1 ms。' };
F.RC = { sc: { dt: '1u', tt: '1m' }, parts: [src('VSTEP', 'Vs', { V: 10, ts: '0.2m' }, 'a'), ip('i', 'a', 'b'), ['RC', 'B1', {}, { 1: 'b', 2: '0' }]], plots: [{ y: ['i'], unit: 'A' }], lab: { i: '電流 i' }, cap: 'R-C 支路（1 Ω、100 µF）在 0.2 ms 加上 10 V 步階：i = 10·e^(−t/τ)，τ = RC = 0.1 ms。' };
F.LC = { sc: { dt: '2u', tt: '6m' }, parts: [src('VSTEP', 'Vs', { V: 10 }, 'a'), ip('i', 'a', 'b'), ['LC', 'B1', {}, { 1: 'b', 2: '0' }]], plots: [{ y: ['i'], unit: 'A' }], lab: { i: '電流 i' }, cap: 'L-C 支路（1 mH、100 µF）加上 10 V 步階：無阻尼振盪，f = 1/(2π√LC) = 503 Hz，峰值 10·√(C/L) = 3.16 A。' };
F.RLC = { sc: { dt: '2u', tt: '10m' }, parts: [src('VSTEP', 'Vs', { V: 10 }, 'a'), ip('i', 'a', 'b'), ['RLC', 'B1', {}, { 1: 'b', 2: '0' }]], plots: [{ y: ['i'], unit: 'A' }], lab: { i: '電流 i' }, cap: 'R-L-C 支路（1 Ω、1 mH、100 µF）加上 10 V 步階：欠阻尼振盪，衰減常數 α = R/2L = 500 s⁻¹。' };
const three = (type, params, cap) => ({
  sc: { dt: '20u', tt: '33.34m' },
  parts: [['VSIN3', 'S1', { V: 100 }, { n: '0', a: 'a', b: 'b', c: 'c' }], ip('ia', 'a', 'a2'), ip('ib', 'b', 'b2'), ip('ic', 'c', 'c2'), [type, 'B1', params, { A: 'a2', B: 'b2', C: 'c2', a: 'y', b: 'y', c: 'y' }], vp('va', 'a')],
  plots: [{ y: ['va'], unit: 'V' }, { y: ['ia', 'ib', 'ic'], unit: 'A', c: 1 }],
  lab: { va: 'A 相電壓' },
  cap,
});
F.R3 = three('R3', { R: 10 }, '三相電阻（每相 10 Ω，Y 接）接 100 V 三相電源：各相電流與該相電壓同相。');
F.RL3 = three('RL3', { R: 5, L: '20m' }, '三相 R-L 支路（5 Ω、20 mH）：電流落後電壓 arctan(ωL/R) = 56°，開始時含直流暫態。');
F.RC3 = three('RC3', { R: 5, C: '300u' }, '三相 R-C 支路（5 Ω、300 µF）：電流超前電壓 arctan(1/(ωRC)) = 60.5°。');
F.RLC3 = three('RLC3', { R: 2, L: '20m', C: '350u' }, '三相 R-L-C 支路（2 Ω、20 mH、350 µF，接近 60 Hz 諧振）：電流與電壓幾乎同相。');
F.RHEO = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vs', 'k', 10, 60), ['RHEO', 'P1', { tap: 0.3 }, { k: 'k', m: '0', t: 't' }], vp('vk', 'k'), vp('vt', 't')], plots: [{ y: ['vk', 'vt'], unit: 'V' }], lab: { vk: 'k 端', vt: '抽頭 t（Tap = 0.3）' }, cap: '可變電阻當分壓器：k–m 之間接 10 V，抽頭 t 對 m 的電壓為 (1 − Tap) 倍（t–m 之間的電阻 R·(1 − Tap)）。' };
F.LSAT = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 4.5, 50, { ph: 90 }), ip('i', 'a', 'b'), ['LSAT', 'L1', {}, { 1: 'b', 2: '0' }], vp('v', 'b')], plots: [{ y: ['v'], unit: 'V' }, { y: ['i'], unit: 'A', c: 1 }], lab: { v: '電壓', i: '電流' }, cap: '飽和電感（表格 i–L：1 A 10 mH、2 A 6 mH、5 A 3 mH）接 4.5 V、50 Hz：磁通接近飽和時電感變小，電流出現尖峰。', differs: 'BE 重新起步' };
F.NONI = { sc: { dt: '1u', tt: '1m' }, parts: [ramp('Vs', 'a', 0, 0.75, '1m'), ip('i', 'a', 'b'), ['NONI', 'D1', {}, { 1: 'b', 2: '0' }], vp('v', 'b')], xy: { x: 'v', y: ['i'], xunit: 'V', yunit: 'A', xlabel: '元件電壓 v' }, lab: { i: 'i = 1e-14·(e^(40v) − 1)' }, cap: '電導型非線性元件（預設為二極體式 i = f(v)）：電壓由 0 掃到 0.75 V 的 i–v 特性。' };
F.NONV = { sc: { dt: '1u', tt: '1m' }, parts: [src('IPWL', 'Is', { vals: '-10 10', times: '0 1m' }, 'a'), ip('i', 'a', 'b'), ['NONV', 'X1', {}, { 1: 'b', 2: '0' }], vp('v', 'b')], xy: { x: 'i', y: ['v'], xunit: 'A', yunit: 'V', xlabel: '元件電流 i' }, lab: { v: 'v = 10i + 0.1i³' }, cap: '電阻型非線性元件（預設 v = 10i + 0.1i³）：電流由 −10 A 掃到 10 A 的 v–i 特性。' };

/* ====================================================================== switches */
const gate = (name, net, f, D = 0.5) => src('VSQU', name, { Vpp: 1, f, D }, net);
const LOGIC = [-0.2, 1.2];
F.DIODE = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 10, 50), ['DIODE', 'D1', { vf: 0.7 }, { A: 'a', K: 'k' }], ip('i', 'k', 'k2'), R('RL', 'k2', '0', 10), vp('vin', 'a'), vp('vout', 'k')], plots: [{ y: ['vin', 'vout'], unit: 'V' }, { y: ['i'], unit: 'A', c: 2 }], lab: { vin: '輸入 10 V、50 Hz', vout: '負載電壓', i: '二極體電流' }, cap: '半波整流：二極體（導通壓降設為 0.7 V）串接 10 Ω 負載。輸入超過 0.7 V 才導通，逆偏時截止。' };
F.LED = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 8, 50), R('Rs', 'a', 'b', 100), ['LED', 'D1', {}, { A: 'b', K: '0' }], R('Rs2', 'a', 'c', 100), ['LED', 'D2', { model: 'custom', spice: 'IS=93.2p RS=42m N=3.73 CJO=2.97p VJ=0.75 M=0.333 TT=4.32u' }, { A: 'c', K: '0' }], vp('vin', 'a'), vp('vled', 'b'), vp('vspice', 'c')], plots: [{ y: ['vin', 'vled', 'vspice'], unit: 'V' }], lab: { vin: '輸入 ±8 V', vled: '理想模型（Vth = 2 V）', vspice: 'SPICE 紅光 LED' }, cap: '兩顆發光二極體各經 100 Ω 接 ±8 V 正弦：順偏時理想模型箝在 2 V，SPICE 紅光 LED 約 1.8～2 V；逆偏時兩者都不導通，LED 兩端承受全部 −8 V（不模擬逆向崩潰，假設元件不會損壞）。' };
F.ZENER = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 10, 50), R('Rs', 'a', 'z', 100), ['ZENER', 'Z1', {}, { A: '0', K: 'z' }], vp('vin', 'a'), vp('vz', 'z')], plots: [{ y: ['vin', 'vz'], unit: 'V' }], lab: { vin: '輸入 10 V', vz: '稽納電壓 VKA' }, cap: '稽納二極體（VB = 5.1 V）經 100 Ω 接 10 V 正弦：逆偏超過 5.1 V 時箝位；正半週的反方向（順偏）導通，電壓約為 0。' };
F.DIAC = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 50, 50), R('Rs', 'a', 'd', 1000), ['DIAC', 'X1', {}, { 1: 'd', 2: '0' }], vp('vin', 'a'), vp('vd', 'd')], plots: [{ y: ['vin', 'vd'], unit: 'V' }], lab: { vin: '輸入 50 V', vd: 'DIAC 電壓' }, cap: '雙向觸發二極體（轉折電壓 30 V、導通壓降 5 V）經 1 kΩ 接 50 V 正弦：電壓到 ±30 V 時崩潰導通、降到 ±5 V，電流過零後恢復阻斷。' };
F.THY = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 100, 50), ['THY', 'T1', {}, { A: 'a', K: 'k', Gate: 'g' }], R('RL', 'k', '0', 10), ['GATING', 'G1', { f: 50, n: 2, pts: '60 80' }, { gate: 'g' }], vp('vin', 'a'), vp('vout', 'k'), vp('gate', 'g')], plots: [{ y: ['vin', 'vout'], unit: 'V' }, { y: ['gate'], h: 46, yr: LOGIC, c: 2 }], lab: { vin: '輸入 100 V、50 Hz', vout: '負載電壓', gate: '閘極（60°～80°）' }, cap: '閘流體半波可控整流：每個週期在 60° 觸發，觸發脈波結束後仍維持導通，直到電流在 180° 過零才關斷。' };
F.TRIAC = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 100, 50), ['TRIAC', 'T1', {}, { 1: 'a', 2: 'k', Gate: 'g' }], R('RL', 'k', '0', 10), ['GATING', 'G1', { f: 50, n: 4, pts: '60 80 240 260' }, { gate: 'g' }], vp('vin', 'a'), vp('vout', 'k'), vp('gate', 'g')], plots: [{ y: ['vin', 'vout'], unit: 'V' }, { y: ['gate'], h: 46, yr: LOGIC, c: 2 }], lab: { vin: '輸入 100 V', vout: '負載電壓', gate: '閘極（60°、240°）' }, cap: '雙向閘流體交流相位控制：正、負半週各在觸發角 60° 導通，電流過零時關斷（調光器原理）。' };
F.GTO = { sc: { dt: '2u', tt: '4m' }, parts: [dc('Vs', 'a', 100), R('RL', 'a', 'k', 10), ['GTO', 'T1', {}, { A: 'k', K: '0', Gate: 'g' }], gate('Vg', 'g', '1k', 0.3), vp('vak', 'k'), vp('gate', 'g')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vak'], unit: 'V', c: 1 }], lab: { gate: '閘極（D = 0.3）', vak: '陽極–陰極電壓' }, cap: '閘關閘流體接 100 V、10 Ω：閘極為高時導通（電壓降為 0），閘極轉低即關斷，不必等電流過零。' };
const lowSide = (type, pins, cap, more = {}) => Object.assign({ sc: { dt: '1u', tt: '3m' }, parts: [dc('Vs', 'a', 10), R('RL', 'a', 'c', 10), [type, 'Q1', {}, pins('c', '0', 'g')], gate('Vg', 'g', '1k'), vp('gate', 'g'), vp('vsw', 'c')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vsw'], unit: 'V', c: 1 }], lab: { gate: '控制訊號', vsw: '開關兩端電壓' }, cap }, more);
F.NPN = lowSide('NPN', (c, e, g) => ({ C: c, E: e, B: g }), '開關模式 NPN（基極接控制訊號）接 10 V、10 Ω：基極為高時 C-E 導通（電壓降為 0），為低時關斷（電壓回到 10 V）。');
F.SSWI = { sc: { dt: '10u', tt: '40m' }, parts: [sine('Vs', 'a', 10, 50), ['SSWI', 'S1', {}, { 1: 'a', 2: 'b', Gate: 'g' }], R('RL', 'b', '0', 10), gate('Vg', 'g', 200, 0.5), vp('vin', 'a'), vp('vout', 'b'), vp('gate', 'g')], plots: [{ y: ['vin', 'vout'], unit: 'V' }, { y: ['gate'], h: 46, yr: LOGIC, c: 2 }], lab: { vin: '輸入 10 V', vout: '負載電壓', gate: '閘極 200 Hz' }, cap: '雙向開關：閘極為高時不論電壓極性都導通，負載得到被切割的交流波形。' };
F.PNP = { sc: { dt: '1u', tt: '3m' }, parts: [dc('Vs', 'e', 10), ['PNP', 'Q1', {}, { E: 'e', C: 'c', B: 'g' }], R('RL', 'c', '0', 10), gate('Vg', 'g', '1k'), vp('gate', 'g'), vp('vload', 'c')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vload'], unit: 'V', c: 1 }], lab: { gate: '控制訊號', vload: '負載電壓' }, cap: 'PNP 高側開關：基極為低時 E-C 導通，負載得到 10 V；基極為高時關斷。' };
F.PMOS = { sc: { dt: '1u', tt: '3m' }, parts: [dc('Vs', 's', 10), ['PMOS', 'Q1', {}, { S: 's', D: 'd', G: 'g' }], R('RL', 'd', '0', 10), gate('Vg', 'g', '1k'), vp('gate', 'g'), vp('vload', 'd')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vload'], unit: 'V', c: 1 }], lab: { gate: '閘極訊號', vload: '負載電壓' }, cap: 'P 通道 MOSFET 高側開關：閘極訊號為低時導通，負載得到 10 V。' };
// chopper with an R-L load and a free-wheeling diode
const chopper = (type, pins, cap) => ({ sc: { dt: '0.1u', tt: '1m' }, parts: [dc('Vs', 'top', 100), ['RL', 'Ld', { R: 5, L: '1m' }, { 1: 'top', 2: 'x' }], ip('iL', 'x', 'c'), ['DIODE', 'Df', {}, { A: 'c', K: 'top' }], [type, 'Q1', {}, pins('c', '0', 'g')], gate('Vg', 'g', '10k'), vp('gate', 'g'), vp('vsw', 'c')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vsw'], unit: 'V', c: 1 }, { y: ['iL'], unit: 'A', c: 2 }], lab: { gate: '閘極 10 kHz、D = 0.5', vsw: '開關電壓', iL: '負載電流' }, cap });
F.IGBT = chopper('IGBT', (c, e, g) => ({ C: c, E: e, G: g }), 'IGBT 截波器：100 V 經 R-L 負載（5 Ω、1 mH），二極體續流。閘極為高時 IGBT 導通、負載電流上升；關斷後電流經二極體續流，開關電壓升到 100 V。');
F.MOSFET = chopper('MOSFET', (c, e, g) => ({ D: c, S: e, G: g }), 'MOSFET 截波器（電路同 IGBT 的例子）：閘極高於 0.5 時通道導通，負載電流以 τ = L/R 的指數逐步建立。');
F.PBSW = { sc: { dt: '10u', tt: '8m' }, parts: [dc('Vs', 'a', 10), ['PBSW', 'S1', { times: '2m 6m' }, { 1: 'a', 2: 'b' }], ip('i', 'b', 'c'), R('RL', 'c', '0', 10)], plots: [{ y: ['i'], unit: 'A' }], lab: { i: '電流（2 ms 按下、6 ms 放開）' }, cap: '按鈕開關：初始為斷開，依「切換時間」在 2 ms 閉合、6 ms 斷開，時刻精確對齊。' };
F.SSWI3 = { sc: { dt: '20u', tt: '40m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: '0', a: 'a', b: 'b', c: 'c' }], ['SSWI3', 'S2', {}, { A: 'a', B: 'b', C: 'c', a: 'a2', b: 'b2', c: 'c2', Gate: 'g' }], ip('ia', 'a2', 'a3'), ip('ib', 'b2', 'b3'), ip('ic', 'c2', 'c3'), ['R3', 'RL', { R: 10 }, { A: 'a3', B: 'b3', C: 'c3', a: 'y', b: 'y', c: 'y' }], src('VSTEP', 'Vg', { V: 1, ts: '12m' }, 'g'), vp('gate', 'g')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['ia', 'ib', 'ic'], unit: 'A', c: 1 }], lab: { gate: '閘極（12 ms 轉高）' }, cap: '三相雙向開關：同一個閘極訊號在 12 ms 同時接通三相，Y 接 10 Ω 負載開始有電流。' };
F.SPDT = { sc: { dt: '10u', tt: '6m' }, parts: [dc('V1', 'p', 10), dc('V2', 'n', -5), ['SPDT', 'S1', {}, { COM: 'o', 1: 'p', 2: 'n', Gate: 'g' }], R('RL', 'o', '0', 10), gate('Vg', 'g', 500), vp('gate', 'g'), vp('vcom', 'o')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vcom'], unit: 'V', c: 1 }], lab: { gate: '閘極', vcom: 'COM 電壓' }, cap: '單刀雙擲開關：閘極為高時 COM 接到 1（+10 V），為低時接到 2（−5 V）。' };
F.DPST = { sc: { dt: '10u', tt: '6m' }, parts: [dc('V1', 'p', 10), dc('V2', 'q', 5), ['DPST', 'S1', {}, { '1a': 'p', '2a': 'x', '1b': 'q', '2b': 'y', Gate: 'g' }], ip('ia', 'x', 'x2'), R('Ra', 'x2', '0', 10), ip('ib', 'y', 'y2'), R('Rb', 'y2', '0', 10), gate('Vg', 'g', 500, 0.3), vp('gate', 'g')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['ia', 'ib'], unit: 'A', c: 1 }], lab: { gate: '閘極（D = 0.3）', ia: '第 a 組（10 V / 10 Ω）', ib: '第 b 組（5 V / 10 Ω）' }, cap: '雙刀單擲開關：兩組開關由同一閘極同時導通、同時斷開。' };
F.DPDT = { sc: { dt: '10u', tt: '6m' }, parts: [dc('V1', 'p', 10), ['DPDT', 'S1', {}, { COMa: 'x', a1: 'p', a2: '0', COMb: 'y', b1: '0', b2: 'p', Gate: 'g' }], R('RL', 'x', 'y', 10), gate('Vg', 'g', 500), vp('gate', 'g'), vp2('vload', 'x', 'y')], plots: [{ y: ['gate'], h: 46, yr: LOGIC }, { y: ['vload'], unit: 'V', c: 1 }], lab: { gate: '閘極', vload: '負載電壓' }, cap: '雙刀雙擲開關做極性反轉：閘極為高時負載得到 +10 V，為低時兩端對調得到 −10 V。' };
// SPICE transistors: output characteristics, one transistor per bias point on the same swept supply
const outChar = (type, nm, bias, cap, opts) => {
  const parts = [ramp('Vce', 's', 0, opts.vmax, '1m'), vp(opts.xname, 's')];
  if (opts.neg) parts.splice(0, 2, ramp('Vce', 's', 0, -opts.vmax, '1m'), vp2(opts.xname, '0', 's'));
  bias.forEach((b, k) => {
    const pins = opts.pins('c' + k, 'b' + k);
    parts.push(opts.drive(k, b), [type, 'Q' + k, {}, pins], opts.neg ? ip(`${nm}${k}`, 'c' + k, 's') : ip(`${nm}${k}`, 's', 'c' + k));
  });
  const lab = {};
  bias.forEach((b, k) => (lab[`${nm}${k}`] = opts.label(b)));
  return { sc: { dt: '1u', tt: '1m' }, parts, xy: { x: opts.xname, y: bias.map((_, k) => `${nm}${k}`), xunit: 'V', yunit: 'A', xlabel: opts.xlabel }, lab, cap };
};
F.NPN3 = outChar('NPN3', 'ic', ['20u', '50u', '100u'], 'NPN 電晶體（SPICE Gummel-Poon，預設 2N3904）的輸出特性：基極電流 20、50、100 µA，集極電壓由 0 掃到 5 V；可看到飽和區、主動區與 Early 效應造成的斜率。', { vmax: 5, xname: 'vce', xlabel: 'VCE', pins: (c, b) => ({ C: c, E: '0', B: b }), drive: (k, b) => src('IDC', 'Ib' + k, { V: b }, 'b' + k), label: b => `IB = ${b.replace('u', ' µ')}A` });
F.PNP3 = outChar('PNP3', 'ic', ['20u', '50u', '100u'], 'PNP 電晶體（SPICE，預設 2N3906）的輸出特性：基極流出 20、50、100 µA，射極接地、集極由 0 掃到 −5 V。', { vmax: 5, neg: true, xname: 'vec', xlabel: 'VEC', pins: (c, b) => ({ C: c, E: '0', B: b }), drive: (k, b) => src('IDC', 'Ib' + k, { V: b }, '0', 'b' + k), label: b => `IB = ${b.replace('u', ' µ')}A` });
F.MOS3 = outChar('MOS3', 'id', [3, 4, 5], 'N 通道 MOSFET（SPICE level-1，預設 GENERIC_N）的輸出特性：VGS = 3、4、5 V，汲極電壓由 0 掃到 10 V；線性區之後進入飽和區。', { vmax: 10, xname: 'vds', xlabel: 'VDS', pins: (c, b) => ({ D: c, S: '0', G: b }), drive: (k, b) => dc('Vg' + k, 'b' + k, b), label: b => `VGS = ${b} V` });
F.PMOS3 = outChar('PMOS3', 'id', [-3, -4, -5], 'P 通道 MOSFET（SPICE level-1，預設 GENERIC_P）的輸出特性：VGS = −3、−4、−5 V，源極接地、汲極由 0 掃到 −10 V。', { vmax: 10, neg: true, xname: 'vsd', xlabel: 'VSD', pins: (c, b) => ({ D: c, S: '0', G: b }), drive: (k, b) => dc('Vg' + k, 'b' + k, b), label: b => `VGS = ${b} V` });
// bridges
F.BDIODE1 = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 100, 50), ['BDIODE1', 'B1', {}, { 'A+': 'a', 'A-': '0', 'DC+': 'p', 'DC-': 'n' }], R('RL', 'p', 'n', 10), vp('vin', 'a'), vp2('vdc', 'p', 'n')], plots: [{ y: ['vin', 'vdc'], unit: 'V' }], lab: { vin: '交流輸入', vdc: '直流輸出（DC+ − DC−）' }, cap: '單相全橋二極體整流：輸出為輸入的絕對值（全波整流），每個週期兩個脈波。' };
F.BDIODE3 = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: 'nn', a: 'a', b: 'b', c: 'c' }], ['BDIODE3', 'B1', {}, { A: 'a', B: 'b', C: 'c', 'DC+': 'p', 'DC-': '0' }], R('RL', 'p', '0', 10), vp2('vab', 'a', 'b'), vp2('vbc', 'b', 'c'), vp2('vca', 'c', 'a'), vp('vdc', 'p')], plots: [{ y: ['vab', 'vbc', 'vca'], unit: 'V' }, { y: ['vdc'], unit: 'V', c: 3 }], lab: { vdc: '直流輸出' }, cap: '三相全橋二極體整流（100 V 線電壓）：輸出跟隨最大的線電壓，每個週期 6 個脈波。' };
F.BTHY1 = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 100, 50), ['BTHY1', 'B1', {}, { 'A+': 'a', 'A-': '0', 'DC+': 'p', 'DC-': 'n', Ct: 'g' }], R('RL', 'p', 'n', 10), ['GATING', 'G1', { f: 50, n: 2, pts: '60 80' }, { gate: 'g' }], vp('vin', 'a'), vp2('vdc', 'p', 'n')], plots: [{ y: ['vin', 'vdc'], unit: 'V' }], lab: { vin: '交流輸入', vdc: '直流輸出' }, cap: '單相全橋閘流體整流，觸發角 60°（Gating Block 50 Hz 提供開關 1 的觸發，開關 3、4 自動延遲 180°）。' };
F.BTHY3 = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: 'nn', a: 'a', b: 'b', c: 'c' }], ['BTHY3', 'B1', {}, { A: 'a', B: 'b', C: 'c', 'DC+': 'p', 'DC-': '0', Ct: 'g' }], R('RL', 'p', '0', 10), ['GATING', 'G1', { f: 60, n: 2, pts: '60 75' }, { gate: 'g' }], vp2('vab', 'a', 'b'), vp('vdc', 'p')], plots: [{ y: ['vab'], unit: 'V' }, { y: ['vdc'], unit: 'V', c: 3 }], lab: { vab: '線電壓 vab', vdc: '直流輸出' }, cap: '三相全橋閘流體整流，觸發角 α = 30°（開關 1 在 A 相電壓 60° 觸發；自然換相點為 30°）。' };
F.BTHY3H = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: '0', a: 'a', b: 'b', c: 'c' }], ['BTHY3H', 'B1', {}, { A: 'a', B: 'b', C: 'c', N: 'k', Ct: 'g' }], R('RL', 'k', '0', 10), ['GATING', 'G1', { f: 60, n: 2, pts: '60 75' }, { gate: 'g' }], vp('va', 'a'), vp('vout', 'k')], plots: [{ y: ['va', 'vout'], unit: 'V' }], lab: { va: 'A 相電壓', vout: '輸出' }, cap: '三相半波閘流體整流，觸發角 α = 30°：每個週期 3 個脈波。' };
F.BTHY6H = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: '0', a: 'a1', b: 'a3', c: 'a5' }], ['VSIN3', 'S2', { V: 100, ph: -60 }, { n: '0', a: 'a2', b: 'a4', c: 'a6' }], ['BTHY6H', 'B1', {}, { A1: 'a1', A2: 'a2', A3: 'a3', A4: 'a4', A5: 'a5', A6: 'a6', N: 'k', Ct: 'g' }], R('RL', 'k', '0', 10), ['GATING', 'G1', { f: 60, n: 2, pts: '90 100' }, { gate: 'g' }], vp('va1', 'a1'), vp('vout', 'k')], plots: [{ y: ['va1', 'vout'], unit: 'V' }], lab: { va1: '第 1 相電壓', vout: '輸出' }, cap: '六相半波閘流體整流（兩組相差 60° 的三相電源），觸發角 α = 30°：每個週期 6 個脈波。' };
// non-ideal switches: switching transients with a resistive load
F.MOSNI = { sc: { dt: '1n', tt: '2u' }, parts: [dc('Vs', 'a', 100), R('RL', 'a', 'd', 10), ['MOSNI', 'Q1', {}, { D: 'd', S: '0', G: 'g' }], ['GATING', 'G1', { f: 0, n: 2, pts: '0.2u 1.2u' }, { gate: 'g' }], ['DEVP', 'vgs', { el: 'Q1', q: 'vg' }, {}], ['DEVP', 'id', { el: 'Q1', q: 'id' }, {}], vp('vds', 'd')], plots: [{ y: ['vgs'], unit: 'V' }, { y: ['vds'], unit: 'V', c: 1 }, { y: ['id'], unit: 'A', c: 2 }], lab: { vgs: '內部閘極電壓 Vgs', vds: 'Vds', id: '汲極電流' }, cap: '非理想 N 通道 MOSFET（100 V、10 Ω 負載）在 0.2 µs 導通、1.2 µs 關斷：閘極經 Rg 對 Ciss 充電，Vgs 在米勒平台停留時 Vds 下降，可看到有限的切換時間。', differs: 'Cgd 以 Vgd = Vth 為中心（米勒平台修正）' };
F.PMOSNI = { sc: { dt: '1n', tt: '2u' }, parts: [dc('Vs', 's', 100), ['PMOSNI', 'Q1', {}, { S: 's', D: 'd', G: 'g' }], R('RL', 'd', '0', 10), src('VPWL', 'Vg', { vals: '1 1 0 0 1 1', times: '0 0.2u 0.2u 1.2u 1.2u 2u' }, 'g'), ['DEVP', 'vgs', { el: 'Q1', q: 'vg' }, {}], vp('vload', 'd')], plots: [{ y: ['vgs'], unit: 'V' }, { y: ['vload'], unit: 'V', c: 1 }], lab: { vgs: '內部閘極電壓 Vgs', vload: '負載電壓' }, cap: '非理想 P 通道 MOSFET 高側開關：控制訊號在 0.2～1.2 µs 為低，驅動器輸出負的 Vgs 使其導通。', differs: 'Cgd 以 Vgd = Vth 為中心（米勒平台修正）' };
F.IGBTNI = { sc: { dt: '1n', tt: '3.5u' }, parts: [dc('Vs', 'a', 300), R('RL', 'a', 'c', 15), ['IGBTNI', 'Q1', {}, { C: 'c', E: '0', G: 'g' }], ['GATING', 'G1', { f: 0, n: 2, pts: '0.3u 1.8u' }, { gate: 'g' }], ['DEVP', 'vge', { el: 'Q1', q: 'vg' }, {}], ['DEVP', 'ic', { el: 'Q1', q: 'id' }, {}], vp('vce', 'c')], plots: [{ y: ['vge'], unit: 'V' }, { y: ['vce'], unit: 'V', c: 1 }, { y: ['ic'], unit: 'A', c: 2 }], lab: { vge: '內部閘極電壓 Vge', vce: 'Vce', ic: '集極電流' }, cap: '非理想 IGBT（300 V、15 Ω 負載）：導通後 Vce 降到 Vce(sat)；關斷時集極電流先快速下降，再以拖尾電流慢慢消失。', differs: 'Cgd 以 Vgd = Vth 為中心（米勒平台修正）' };

/* ====================================================================== transformers, coupled inductors */
F.TFI = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 10, 50), ['TFI', 'T1', { np: 1, ns: 2 }, { 'p+': 'a', 'p-': '0', 's+': 's', 's-': '0' }], R('RL', 's', '0', 100), vp('vp', 'a'), vp('vs', 's')], plots: [{ y: ['vp', 'vs'], unit: 'V' }], lab: { vp: '一次側', vs: '二次側' }, cap: '理想變壓器 Np : Ns = 1 : 2：vs = 2·vp，電流反比。' };
F.TF1 = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 10, 50), ip('ip', 'a', 'a2'), ['TF1', 'T1', { np: 1, ns: 0.5, lm: '0.1', lp: '1m', rp: '0.05' }, { 'p+': 'a2', 'p-': '0', 's+': 's', 's-': '0' }], R('RL', 's', '0', 2), vp('vp', 'a'), vp('vs', 's')], plots: [{ y: ['vp', 'vs'], unit: 'V' }, { y: ['ip'], unit: 'A', c: 2 }], lab: { vp: '一次側', vs: '二次側', ip: '一次側電流' }, cap: '單相變壓器（Np : Ns = 1 : 0.5、Lm = 0.1 H、Lp = 1 mH、Rp = 0.05 Ω）接 2 Ω 負載：二次側約一半電壓，漏感與電阻造成小幅壓降與相移。' };
F.TF3W = { sc: { dt: '20u', tt: '40m' }, parts: [sine('Vs', 'a', 10, 50), ['TF3W', 'T1', { ns: 1, nt: 0.5 }, { 'p+': 'a', 'p-': '0', 's+': 's', 's-': '0', 't+': 't', 't-': '0' }], R('Rs', 's', '0', 10), R('Rt', 't', '0', 10), vp('vp', 'a'), vp('vs', 's'), vp('vt', 't')], plots: [{ y: ['vp', 'vs', 'vt'], unit: 'V' }], lab: { vp: '一次側', vs: '二次側 s（1 : 1）', vt: '二次側 t（1 : 0.5）' }, cap: '單相三繞組變壓器：兩個二次側各自依匝數比得到電壓。' };
F.TF3YY = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: '0', a: 'a', b: 'b', c: 'c' }], ['TF3YY', 'T1', { ns: 0.5 }, { A: 'a', B: 'b', C: 'c', N: '0', a: 'x', b: 'y', c: 'z', n: 'nn' }], ['R3', 'RL', { R: 10 }, { A: 'x', B: 'y', C: 'z', a: 'nn', b: 'nn', c: 'nn' }], vp('vA', 'a'), vp2('va', 'x', 'nn'), vp2('vb', 'y', 'nn'), vp2('vc', 'z', 'nn')], plots: [{ y: ['vA'], unit: 'V' }, { y: ['va', 'vb', 'vc'], unit: 'V', c: 1 }], lab: { vA: '一次側 A 相' }, cap: '三相 Y/Y 變壓器（Np : Ns = 1 : 0.5）：二次側各相電壓為一次側的一半，相位相同。' };
F.TF3YD = { sc: { dt: '20u', tt: '33.34m' }, parts: [['VSIN3', 'S1', { V: 100 }, { n: '0', a: 'a', b: 'b', c: 'c' }], ['TF3YD', 'T1', { rs: 0.01, ls: '0.1m' }, { A: 'a', B: 'b', C: 'c', N: '0', a: 'x', b: 'y', c: 'z' }], ['R3', 'RL', { R: 10 }, { A: 'x', B: 'y', C: 'z', a: '0', b: '0', c: '0' }], vp2('vAB', 'a', 'b'), vp2('vab', 'x', 'y')], plots: [{ y: ['vAB', 'vab'], unit: 'V' }], lab: { vAB: '一次側線電壓 vAB', vab: '二次側線電壓 vab' }, cap: '三相 Y/Δ 變壓器（匝數比 1 : 1，二次側含 0.01 Ω、0.1 mH；Δ 繞組形成迴路，至少要有一點漏感或電阻）：二次側線電壓等於一次側相電壓（為線電壓的 1/√3），並與一次側線電壓相差 30°。' };
F.MUT2 = { sc: { dt: '1u', tt: '3m' }, parts: [sine('Vs', 'a', 10, '1k'), ip('i1', 'a', 'a2'), ['MUT2', 'M1', {}, { '1+': 'a2', '1-': '0', '2+': 's', '2-': '0' }], R('RL', 's', '0', 10), vp('v1', 'a'), vp('v2', 's')], plots: [{ y: ['v1', 'v2'], unit: 'V' }, { y: ['i1'], unit: 'A', c: 2 }], lab: { v1: '繞組 1', v2: '繞組 2（10 Ω 負載）', i1: '繞組 1 電流' }, cap: '兩繞組耦合電感（L11 = L22 = 1 mH、L12 = 0.9 mH，耦合係數 0.9）當變壓器用：漏感使二次側電壓低於一次側。' };
F.MUT3 = { sc: { dt: '1u', tt: '3m' }, parts: [sine('Vs', 'a', 10, '1k'), ['MUT3', 'M1', {}, { '1+': 'a', '1-': '0', '2+': 's', '2-': '0', '3+': 't', '3-': '0' }], R('R2', 's', '0', 10), R('R3', 't', '0', 100), vp('v1', 'a'), vp('v2', 's'), vp('v3', 't')], plots: [{ y: ['v1', 'v2', 'v3'], unit: 'V' }], lab: { v1: '繞組 1', v2: '繞組 2（10 Ω）', v3: '繞組 3（100 Ω）' }, cap: '三繞組耦合電感：負載較重的繞組因漏感壓降較大。' };

/* ====================================================================== other power elements */
F.OPAMP = { sc: { dt: '1u', tt: '2m' }, parts: [sine('Vs', 'a', 4, '1k'), R('R1', 'a', 'm', '1k'), R('Rf', 'm', 'o', '5k'), ['OPAMP', 'U1', {}, { 'V-': 'm', 'V+': '0', Vo: 'o' }], vp('vin', 'a'), vp('vout', 'o')], plots: [{ y: ['vin', 'vout'], unit: 'V' }], lab: { vin: '輸入 4 V', vout: '輸出' }, cap: '反相放大器（Rf/R1 = 5）：輸出 = −5·輸入，但被電源 ±15 V 箝位，所以峰值被削平。' };
F.OPAMP1 = { sc: { dt: '1u', tt: '2m' }, parts: [sine('Vs', 'a', 20, '1k'), dc('Vr', 'r', 5), ['OPAMP1', 'U1', {}, { 'V-': 'o', 'V+': 'a', Vo: 'o', gnd: 'r' }], R('RL', 'o', '0', '10k'), vp('vin', 'a'), vp('vout', 'o')], plots: [{ y: ['vin', 'vout'], unit: 'V' }], lab: { vin: '輸入 20 V', vout: '輸出（參考地接 5 V）' }, cap: '電壓隨耦器，模型的參考地接腳接到 5 V：輸出的上下限 Vs+、Vs− 以參考地為準，所以變成 5 + 15 = 20 V 與 5 − 15 = −10 V。' };
F.COMPOC = { sc: { dt: '0.2u', tt: '2m' }, parts: [sine('Vs', 'i', 1, '1k'), dc('Vcc', 'v5', 5), ['COMPOC', 'U1', {}, { 'IN+': 'i', 'IN-': '0', OUT: 'o', 'V+': 'v5', GND: '0' }], R('Rpu', 'v5', 'o', '10k'), vp('vin', 'i'), vp('vout', 'o')], plots: [{ y: ['vin'], unit: 'V' }, { y: ['vout'], unit: 'V', c: 1 }], lab: { vin: 'IN+（IN− 接地）', vout: '輸出（10 kΩ 上拉到 5 V）' }, cap: '開集極比較器：IN+ > IN− 時輸出電晶體關閉，輸出由上拉電阻拉到 5 V；反之拉到接近 0 V。可看到傳遞延遲與上拉電阻造成的上升時間。' };
F.TL431 = { sc: { dt: '1u', tt: '3m' }, parts: [src('VSTEP2', 'Vs', { V1: 0, V2: 12, ts: '0.2m', tr: '2m' }, 'a'), R('Rs', 'a', 'k', '1k'), ['TL431', 'U1', {}, { K: 'k', A: '0', REF: 'r' }], R('Ra', 'k', 'r', '10k'), R('Rb', 'r', '0', '10k'), vp('vin', 'a'), vp('vk', 'k')], plots: [{ y: ['vin', 'vk'], unit: 'V' }], lab: { vin: '輸入（0 → 12 V 斜坡）', vk: '陰極電壓' }, cap: 'TL431 並聯穩壓（REF 分壓 1 : 1）：輸入低時陰極電壓跟著上升，超過 2·Vref ≈ 5 V 後被穩定住，多餘電流由 TL431 吸收。' };
F.OPTO1 = { sc: { dt: '0.5u', tt: '3m' }, parts: [src('VSQU', 'Vs', { Vpp: 5, f: '1k' }, 'a'), R('Rled', 'a', 'b', 390), ip('if', 'b', 'b2'), ['OPTO1', 'U1', {}, { A: 'b2', K: '0', C: 'c', E: '0' }], dc('Vcc', 'v5', 5), R('RL', 'v5', 'c', '1k'), vp('vc', 'c')], plots: [{ y: ['if'], unit: 'A' }, { y: ['vc'], unit: 'V', c: 1 }], lab: { if: 'LED 電流', vc: '光電晶體集極（1 kΩ 上拉）' }, cap: '光耦合器（CTR 100 %）：LED 有電流時光電晶體導通，集極被拉低；可看到有限的上升與下降時間。' };
F.OPTO2 = { sc: { dt: '0.5u', tt: '3m' }, parts: [src('VSQU', 'Vs', { Vpp: 5, f: '1k' }, 'a'), R('Rled', 'a', 'b', 390), ['OPTO2', 'U1', { ctr2: 50 }, { A: 'b', K: '0', C1: 'c1', E1: '0', C2: 'c2', E2: '0' }], dc('Vcc', 'v5', 5), R('R1', 'v5', 'c1', '1k'), R('R2', 'v5', 'c2', '1k'), vp('vc1', 'c1'), vp('vc2', 'c2')], plots: [{ y: ['vc1', 'vc2'], unit: 'V' }], lab: { vc1: '輸出 1（CTR 100 %）', vc2: '輸出 2（CTR 50 %）' }, cap: '一個 LED 照射兩個光電晶體：CTR 較低的一組在相同負載下拉低的程度較小。' };
F.DVDT = { sc: { dt: '1u', tt: '3m' }, parts: [src('VTRI', 'Vs', { Vpp: 1, f: '1k' }, 'a'), R('Ra', 'a', '0', 100), ['DVDT', 'D1', {}, { in: 'a', out: 'o' }], vp('vin', 'a'), vp('vout', 'o')], plots: [{ y: ['vin'], unit: 'V' }, { y: ['vout'], unit: 'V/s', c: 1 }], lab: { vin: '輸入三角波 1 V、1 kHz', vout: '輸出 dv/dt' }, cap: '功率電路微分方塊：輸入三角波的斜率為 ±2000 V/s，輸出即為對應的方波。' };

/* ====================================================================== control blocks */
// one-input block: the input and the output on one screen (or two when the units differ)
const blk = (type, params, input, cap, more = {}) => Object.assign({
  sc: { dt: '10u', tt: '20m' },
  parts: [input('x'), [type, 'B1', params, { in: 'x', out: 'y' }], vp('in', 'x'), vp('out', 'y')],
  plots: more.split ? [{ y: ['in'] }, { y: ['out'], c: 1 }] : [{ y: ['in', 'out'] }],
  lab: { in: '輸入', out: '輸出' },
  cap,
}, more);
const xyblk = (type, params, v0, v1, cap, more = {}) => Object.assign({
  sc: { dt: '5u', tt: '10m' },
  parts: [ramp('Vin', 'x', v0, v1, '10m'), [type, 'B1', params, { in: 'x', out: 'y' }], vp('in', 'x'), vp('out', 'y')],
  xy: { x: 'in', y: ['out'], xlabel: '輸入', xunit: more.xunit || '', yunit: more.yunit || '' },
  lab: { out: '輸出' },
  cap,
}, more);
const sinIn = (Vm = 1, f = 100, more = {}) => net => sine('Vin', net, Vm, f, more);
F.CONST = { sc: { dt: '10u', tt: '10m' }, parts: [cst('K1', 'a', 1), cst('K2', 'b', -0.5), vp('k1', 'a'), vp('k2', 'b')], plots: [{ y: ['k1', 'k2'] }], lab: { k1: 'v = 1（預設）', k2: 'v = −0.5' }, cap: '常數方塊輸出固定的控制訊號。' };
F.P = blk('P', { k: 2 }, sinIn(), '比例控制器 k = 2：輸出 = 2·輸入。');
F.I = blk('I', {}, net => src('VSQU', 'Vin', { Vpp: 2, off: -1, f: 250 }, net), '積分器 G(s) = 1/(sT)，T = 1 ms：±1 的方波積分成三角波（每 2 ms 變化 2）。');
F.RESETI = { sc: { dt: '10u', tt: '10m' }, parts: [cst('Vin', 'x', 1), sq('Vr', 'r', 300, 0.1), ['RESETI', 'B1', {}, { in: 'x', out: 'y', reset: 'r' }], vp('reset', 'r'), vp('out', 'y')], plots: [{ y: ['reset'], h: 46, yr: LOGIC }, { y: ['out'], c: 1 }], lab: { reset: '重置訊號', out: '輸出' }, cap: '可重置積分器（邊緣重置）：輸入 1、T = 1 ms，輸出以 1000/s 上升，重置訊號每個上升緣歸零。' };
F.DIFF = blk('DIFF', {}, net => src('VTRI', 'Vin', { Vpp: 1, f: 250 }, net), '微分器 G(s) = sT，T = 1 ms：三角波（斜率 ±500 /s）微分成 ±0.5 的方波。');
F.PI = { sc: { dt: '10u', tt: '4m' }, parts: [src('VSTEP', 'Vin', { V: 1 }, 'x'), ['PI', 'B1', {}, { in: 'x', out: 'y' }], vp('in', 'x'), vp('out', 'y')], plots: [{ y: ['in', 'out'] }], lab: { in: '輸入（1 ms 步階）', out: '輸出' }, cap: 'PI 控制器 k = 1、T = 1 ms 的步階響應：先跳到 k，再以 k/T = 1000/s 的斜率上升。' };
F.SUM2 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'a', 1, 100), cst('V2', 'b', 0.5), ['SUM2', 'B1', {}, { in1: 'a', in2: 'b', out: 'y' }], vp('in1', 'a'), vp('in2', 'b'), vp('out', 'y')], plots: [{ y: ['in1', 'in2', 'out'] }], lab: { out: 'in1 + in2' }, cap: '加法器（k1 = k2 = 1）：正弦加上 0.5 的常數。' };
F.SUB2 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'a', 1, 100), cst('V2', 'b', 0.5), ['SUB2', 'B1', {}, { 'in+': 'a', 'in-': 'b', out: 'y' }], vp('inp', 'a'), vp('inn', 'b'), vp('out', 'y')], plots: [{ y: ['inp', 'inn', 'out'] }], lab: { inp: 'in+', inn: 'in−', out: 'in+ − in−' }, cap: '減法器：輸出 = in+ − in−，常用來算誤差（參考值 − 回授值）。' };
F.SUM3 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'a', 1, 100), cst('V2', 'b', 0.5), sq('V3', 'c', 50), ['SUM3', 'B1', {}, { in1: 'a', in2: 'b', in3: 'c', out: 'y' }], vp('in1', 'a'), vp('in3', 'c'), vp('out', 'y')], plots: [{ y: ['in1', 'in3', 'out'] }], lab: { out: 'in1 + in2 + in3（in2 = 0.5）' }, cap: '三輸入加法器：正弦、常數 0.5 與方波相加。' };
F.COMP = { sc: { dt: '1u', tt: '20m' }, parts: [sine('V1', 'a', 0.8, 50), src('VTRI', 'Vc', { Vpp: 2, off: -1, f: '500' }, 'b'), ['COMP', 'B1', {}, { 'in+': 'a', 'in-': 'b', out: 'y' }], vp('inp', 'a'), vp('inn', 'b'), vp('out', 'y')], plots: [{ y: ['inp', 'inn'] }, { y: ['out'], h: 50, yr: LOGIC, c: 2 }], lab: { inp: 'in+：正弦調變訊號', inn: 'in−：三角載波 500 Hz', out: '輸出' }, cap: '邏輯比較器做正弦 PWM：in+ > in− 時輸出 1，否則 0；交越時刻以內插精確求得。' };
F.LIM = blk('LIM', {}, sinIn(2), '限制器（下限 −1、上限 1）：超出範圍的部分被削平。');
// filters: Bode plots from the AC sweep (linear frequency-domain method)
const acsw = { f0: 10, f1: '100k', n: 40, flag: 0, src: 'Vac', a0: 1, a1: 1, method: 1 };
const bode = (type, params, cap) => ({ ac: true, sc: { dt: '10u', tt: '10m' }, extra: [['ACSWEEP', acsw]], parts: [src('VSIN', 'Vac', { Vm: 0, f: '1k' }, 'x'), [type, 'B1', params, { in: 'x', out: 'y' }], ['ACP', 'H', {}, { n: 'y' }]], bode: ['H'], lab: { H: 'H(f) = 輸出 / 輸入' }, cap });
F.LP1 = bode('LP1', {}, '一階低通濾波器（fc = 1 kHz）的波德圖：fc 處 −3 dB、相位 −45°，之後每十倍頻下降 20 dB。');
F.HP1 = bode('HP1', {}, '一階高通濾波器（fc = 1 kHz）：低頻每十倍頻 +20 dB，fc 處 −3 dB、相位 +45°。');
F.LP2 = bode('LP2', {}, '二階低通濾波器（fc = 1 kHz、ξ = 0.707，Butterworth）：之後每十倍頻下降 40 dB，相位趨近 −180°。');
F.HP2 = bode('HP2', {}, '二階高通濾波器（fc = 1 kHz、ξ = 0.707）。');
F.BP2 = bode('BP2', {}, '二階帶通濾波器（中心 1 kHz、頻寬 100 Hz）：只讓中心頻率附近通過，中心處增益 k、相位 0。');
F.BS2 = bode('BS2', {}, '二階帶拒濾波器（中心 1 kHz、頻寬 100 Hz）：中心頻率被深度衰減。');
// computational blocks
F.MULT = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'a', 1, 250), ramp('V2', 'b', -1, 1, '20m'), ['MULT', 'B1', {}, { in1: 'a', in2: 'b', out: 'y' }], vp('in1', 'a'), vp('in2', 'b'), vp('out', 'y')], plots: [{ y: ['in1', 'in2'] }, { y: ['out'], c: 2 }], lab: { out: 'in1 × in2' }, cap: '乘法器：正弦乘上由 −1 升到 1 的斜坡，輸出的振幅與正負隨之改變。' };
F.DIVD = { sc: { dt: '5u', tt: '10m' }, parts: [cst('N', 'a', 1), ramp('Vd', 'b', 0.2, 2, '10m'), ['DIVD', 'B1', {}, { num: 'a', den: 'b', out: 'y' }], vp('den', 'b'), vp('out', 'y')], xy: { x: 'den', y: ['out'], xlabel: '除數（被除數 = 1）' }, lab: { out: '1 / 除數' }, cap: '除法器：被除數 1、除數由 0.2 掃到 2。' };
F.SQRT = xyblk('SQRT', {}, 0, 10, '平方根：輸出 = √輸入（負輸入以 0 計算）。');
F.SIN = xyblk('SIN', {}, 0, 360, '正弦（輸入單位為度）：輸入 0～360°。', { xunit: '°' });
F.SINR = xyblk('SINR', {}, 0, 6.2832, '正弦（輸入單位為弧度）：輸入 0～2π。');
F.ASIN = xyblk('ASIN', {}, -1, 1, '反正弦：輸出單位為度（−90°～90°）。', { yunit: '°' });
F.COS = xyblk('COS', {}, 0, 360, '餘弦（輸入單位為度）。', { xunit: '°' });
F.COSR = xyblk('COSR', {}, 0, 6.2832, '餘弦（輸入單位為弧度）。');
F.ACOS = xyblk('ACOS', {}, -1, 1, '反餘弦：輸出單位為度（0°～180°）。', { yunit: '°' });
F.TAN = xyblk('TAN', {}, -80, 80, '正切（輸入單位為度）：輸入 −80°～80°。', { xunit: '°' });
F.ATAN = { sc: { dt: '5u', tt: '10m' }, parts: [cst('Re', 'a', 1), ramp('Vi', 'b', -5, 5, '10m'), ['ATAN', 'B1', {}, { real: 'a', imag: 'b', out: 'y' }], vp('imag', 'b'), vp('out', 'y')], xy: { x: 'imag', y: ['out'], xlabel: '虛部（實部 = 1）', yunit: '°' }, lab: { out: 'tan⁻¹(虛部/實部)' }, cap: '反正切：輸出 = tan⁻¹(虛部/實部)，單位為度。' };
F.ATAN2 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vy', 'a', 1, 100), sine('Vx', 'b', 1, 100, { ph: 90 }), ['ATAN2', 'B1', {}, { y: 'a', x: 'b', out: 'o' }], vp('y', 'a'), vp('x', 'b'), vp('angle', 'o')], plots: [{ y: ['y', 'x'] }, { y: ['angle'], unit: '°', c: 2 }], lab: { angle: 'atan2(y, x)' }, cap: '四象限反正切：y = sin、x = cos（向量以 100 Hz 旋轉），輸出角度在 −180°～180° 之間循環。' };
F.EXP = xyblk('EXP', {}, -2, 2, '指數函數 k1·k2^輸入（k1 = 1、k2 = e）。');
F.POW = xyblk('POW', {}, 0, 3, '冪次函數 k1·輸入^k2（k2 = 2）。');
F.LOG = xyblk('LOG', {}, 0.1, 10, '自然對數。');
F.LOG10 = xyblk('LOG10', {}, 0.1, 100, '常用對數（以 10 為底）。');
F.RMS = blk('RMS', {}, sinIn(1, 60), '均方根值（基頻 60 Hz）：輸入 1 V 正弦，每個週期結束時更新一次，讀值 0.707。', { sc: { dt: '20u', tt: '50m' } });
F.ABS = blk('ABS', {}, sinIn(), '絕對值。');
F.SIGN = blk('SIGN', {}, sinIn(), '符號函數：正 → 1、負 → −1。');
F.MAXMIN = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V1', 'a', 1, 100), sine('V2', 'b', 1, 100, { ph: 90 }), ['MAXMIN', 'B1', {}, { in1: 'a', in2: 'b', out: 'y' }], vp('in1', 'a'), vp('in2', 'b'), vp('out', 'y')], plots: [{ y: ['in1', 'in2'] }, { y: ['out'], c: 2 }], lab: { out: '最大值（Flag = 0）' }, cap: '最大／最小值：兩個相差 90° 的正弦取較大者。' };
// other function blocks
F.SAMP = { sc: { dt: '10u', tt: '20m' }, parts: [sine('Vin', 'x', 1, 100), sq('Vc', 'c', 1000), ['SAMP', 'B1', {}, { in: 'x', ctrl: 'c', out: 'y' }], vp('in', 'x'), vp('out', 'y')], plots: [{ y: ['in', 'out'] }], lab: { in: '輸入', out: '輸出（1 kHz 取樣）' }, cap: '取樣保持：控制訊號每個上升緣取樣一次，保持到下一次取樣。' };
F.LKUP = xyblk('LKUP', {}, -0.5, 3.5, '一維查表（預設表格 0→0、1→10、2→15、3→17）：兩點之間線性內插，超出範圍時箝位於端點值。');
F.LKUPTZ = xyblk('LKUPTZ', {}, -360, 360, '梯形波查表（上升角 30°、峰值 1）：輸入角度 −360°～360°。', { xunit: '°' });
F.LKUPSQ = xyblk('LKUPSQ', {}, -360, 360, '方波查表（每半週期中央寬 120°）。', { xunit: '°' });
F.FFT = { sc: { dt: '20u', tt: '50m' }, parts: [sine('Vin', 'x', 1, 60, { ph: 30 }), ['FFT', 'B1', {}, { in: 'x', amp: 'a', phase: 'p' }], vp('in', 'x'), vp('amp', 'a'), vp('phase', 'p')], plots: [{ y: ['in', 'amp'] }, { y: ['phase'], unit: '°', c: 2 }], lab: { in: '輸入 sin(ωt + 30°)', amp: '基本波峰值', phase: '基本波相位' }, cap: '基本波分析（N = 64、60 Hz）：輸入振幅 1、相位 30° 的正弦，一個週期後輸出振幅 1 與相位 30°。' };
F.TFS = { sc: { dt: '10u', tt: '15m' }, parts: [src('VSTEP', 'Vin', { V: 1 }, 'x'), ['TFS', 'B1', { k: 1.5, num: '0 0 400e3', den: '1 1200 400e3' }, { in: 'x', out: 'y' }], vp('in', 'x'), vp('out', 'y')], plots: [{ y: ['in', 'out'] }], lab: { in: '輸入（1 ms 步階）', out: '輸出' }, cap: 's 域轉移函數 1.5·400e3/(s² + 1200s + 400e3) 的步階響應：穩態 1.5，欠阻尼。' };
F.TFS1 = { sc: { dt: '10u', tt: '6m' }, parts: [src('VSTEP', 'Vin', { V: 1, ts: '2m' }, 'x'), ['TFS1', 'B1', { x0: 1 }, { in: 'x', out: 'y' }], ['TFS', 'B2', {}, { in: 'x', out: 'z' }], vp('in', 'x'), vp('out', 'y'), vp('ref', 'z')], plots: [{ y: ['in', 'out', 'ref'] }], lab: { in: '輸入（2 ms 步階）', out: '含初值 x0 = 1', ref: '同一轉移函數、初值 0' }, cap: 's 域轉移函數（含初值）1/(1e-3·s + 1)：狀態初值使輸出由初始狀態開始衰減，之後與一般轉移函數的步階響應相同。' };
F.TDELAY = blk('TDELAY', {}, sinIn(1, 250), '時間延遲（Td = 1 ms）：輸出為 1 ms 前的輸入（t < Td 時輸出 0）。');
F.UTD = { sc: { dt: '0.1m', tt: '3m' }, parts: [sq('Vin', 'x', 1000), ['UTD', 'B1', {}, { in: 'x', out: 'y' }], vp('in', 'x'), vp('out', 'y')], plots: [{ y: ['in', 'out'], yr: LOGIC }], lab: { in: '輸入（1 kHz）', out: '輸出' }, cap: '單位時間延遲：輸出為上一個時間步的輸入（此例時間步 0.1 ms，所以延遲 0.1 ms）。' };
F.THD = { sc: { dt: '20u', tt: '0.3' }, parts: [src('VMATH', 'Vin', { expr: 'sin(2*pi*60*t) + 0.2*sin(2*pi*180*t)' }, 'x'), ['THD', 'B1', {}, { in: 'x', THD: 'h', v1: 'v' }], vp('in', 'x'), vp('v1', 'v'), vp('thd', 'h')], plots: [{ y: ['in', 'v1'] }, { y: ['thd'], c: 2, yr: [0, 0.5] }], lab: { in: '輸入（含 20 % 三次諧波）', v1: '基本波 v1', thd: 'THD' }, cap: '總諧波失真：基本波 60 Hz 加上 20 % 三次諧波，帶通濾波器收斂（約 3/fb = 0.15 s）後 THD 讀值 0.2。' };
F.ROUND = xyblk('ROUND', {}, 0, 5, '捨入（N = 0）：四捨五入到整數。');
F.LIMDVDT = blk('LIMDVDT', { L: 1000 }, net => sq('Vin', net, 100), '變化率限制器（1000 /s）：方波的跳變被限制成每毫秒變化 1 的斜坡。');
F.MUX2 = { sc: { dt: '10u', tt: '20m' }, parts: [sine('V0', 'a', 1, 100), cst('V1', 'b', -0.5), sq('Vs', 's', 40), ['MUX2', 'B1', {}, { d0: 'a', d1: 'b', s0: 's', Y: 'y' }], vp('s0', 's'), vp('out', 'y')], plots: [{ y: ['s0'], h: 46, yr: LOGIC }, { y: ['out'], c: 1 }], lab: { s0: '選擇 s0', out: '輸出（d0：正弦、d1：−0.5）' }, cap: '2 輸入多工器：s0 為 0 時輸出 d0，為 1 時輸出 d1。' };
F.MUX4 = { sc: { dt: '10u', tt: '8m' }, parts: [cst('D0', 'a', 0), cst('D1', 'b', 1), cst('D2', 'c', 2), cst('D3', 'd', 3), sq('S0', 's0', 1000, 0.5, 180), sq('S1', 's1', 500, 0.5, 180), ['MUX4', 'B1', {}, { d0: 'a', d1: 'b', d2: 'c', d3: 'd', s0: 's0', s1: 's1', Y: 'y' }], vp('s0', 's0'), vp('s1', 's1'), vp('out', 'y')], plots: [{ y: ['s0'], h: 40, yr: LOGIC }, { y: ['s1'], h: 40, yr: LOGIC, c: 1 }, { y: ['out'], c: 2 }], lab: { out: '輸出（d0～d3 = 0、1、2、3）' }, cap: '4 輸入多工器：(s1, s0) 依序為 00、01、10、11 時輸出 d0、d1、d2、d3。' };
F.MUX8 = { sc: { dt: '10u', tt: '8m' }, parts: [0, 1, 2, 3, 4, 5, 6, 7].map(k => cst('D' + k, 'd' + k, k)).concat([sq('S0', 's0', 2000, 0.5, 180), sq('S1', 's1', 1000, 0.5, 180), sq('S2', 's2', 500, 0.5, 180), ['MUX8', 'B1', {}, { d0: 'd0', d1: 'd1', d2: 'd2', d3: 'd3', d4: 'd4', d5: 'd5', d6: 'd6', d7: 'd7', s0: 's0', s1: 's1', s2: 's2', Y: 'y' }], vp('s2', 's2'), vp('out', 'y')]), plots: [{ y: ['s2'], h: 40, yr: LOGIC }, { y: ['out'], c: 1 }], lab: { out: '輸出（d0～d7 = 0～7）' }, cap: '8 輸入多工器：選擇訊號 s2 s1 s0 由 000 數到 111，輸出依序為 d0～d7。' };
const abc = (f = 60) => [sine('Va', 'a', 1, f), sine('Vb', 'b', 1, f, { ph: -120 }), sine('Vc', 'c', 1, f, { ph: 120 }), src('VSAW', 'Vth', { Vpk: 6.283185307, f }, 'th')];
F.ABC2DQO = { sc: { dt: '20u', tt: '33.34m' }, parts: abc().concat([['ABC2DQO', 'B1', {}, { a: 'a', b: 'b', c: 'c', 'θ': 'th', d: 'd', q: 'q', o: 'o' }], vp('a', 'a'), vp('b', 'b'), vp('c', 'c'), vp('d', 'd'), vp('q', 'q'), vp('o', 'o')]), plots: [{ y: ['a', 'b', 'c'] }, { y: ['d', 'q', 'o'] }], cap: 'abc → dqo 轉換：三相 60 Hz 正弦與同步旋轉的角度 θ（0～2π 鋸齒波）轉成直流的 d、q 量，o 為零序量。' };
F.DQO2ABC = { sc: { dt: '20u', tt: '33.34m' }, parts: [cst('D', 'd', 0), cst('Q', 'q', 1), cst('O', 'o', 0), src('VSAW', 'Vth', { Vpk: 6.283185307, f: 60 }, 'th'), ['DQO2ABC', 'B1', {}, { d: 'd', q: 'q', o: 'o', 'θ': 'th', a: 'a', b: 'b', c: 'c' }], vp('theta', 'th'), vp('a', 'a'), vp('b', 'b'), vp('c', 'c')], plots: [{ y: ['theta'] }, { y: ['a', 'b', 'c'], c: 1 }], lab: { theta: 'θ（0～2π）' }, cap: 'dqo → abc 反轉換：d = 0、q = 1、o = 0 與 60 Hz 的 θ 產生三相正弦。' };

/* ====================================================================== logic */
const LH = 40;
const lplot = names => names.map((n, k) => ({ y: [n], h: LH, yr: LOGIC, c: k }));
const gate2 = (type, cap, three) => {
  const ins = three ? ['in1', 'in2', 'in3'] : ['in1', 'in2'];
  const parts = [sq('A', 'a', 1000), sq('B', 'b', 500)];
  const pins = { in1: 'a', in2: 'b', out: 'y' };
  if (three) { parts.push(sq('C', 'c', 250)); pins.in3 = 'c'; }
  parts.push([type, 'U1', {}, pins], vp('in1', 'a'), vp('in2', 'b'), vp('out', 'y'));
  if (three) parts.push(vp('in3', 'c'));
  return { sc: { dt: '5u', tt: '4m' }, parts, plots: lplot(ins.concat(['out'])), cap };
};
F.AND = gate2('AND', '及閘：兩個輸入都為 1 時輸出 1。');
F.AND3 = gate2('AND3', '三輸入及閘：三個輸入都為 1 時輸出 1。', true);
F.OR = gate2('OR', '或閘：任一輸入為 1 時輸出 1。');
F.OR3 = gate2('OR3', '三輸入或閘。', true);
F.XOR = gate2('XOR', '互斥或閘：兩個輸入不同時輸出 1。');
F.NAND = gate2('NAND', '反及閘：及閘的反相。');
F.NOR = gate2('NOR', '反或閘：或閘的反相。');
F.NOT = { sc: { dt: '5u', tt: '4m' }, parts: [sq('A', 'a', 1000), ['NOT', 'U1', {}, { in1: 'a', out: 'y' }], vp('in', 'a'), vp('out', 'y')], plots: lplot(['in', 'out']), cap: '反閘：輸出為輸入的反相。' };
// pulses as piecewise-linear sources: rising at each start, width w
const pulses = (name, net, starts, w) => {
  const tv = [[0, 0]];
  for (const s of starts) tv.push([s, 0], [s, 1], [s + w, 1], [s + w, 0]);
  return src('VPWL', name, { vals: tv.map(p => p[1]).join(' '), times: tv.map(p => p[0]).join(' ') }, net);
};
F.SRFF = { sc: { dt: '5u', tt: '10m' }, parts: [pulses('S', 's', [1e-3, 3e-3, 6e-3], 2e-4), pulses('R', 'r', [2e-3, 4.5e-3, 8e-3], 2e-4), ['SRFF', 'U1', {}, { S: 's', R: 'r', Q: 'q', Qn: null }], vp('S', 's'), vp('R', 'r'), vp('Q', 'q')], plots: lplot(['S', 'R', 'Q']), cap: 'SR 正反器（邊緣觸發）：S 的上升緣使 Q = 1，R 的上升緣使 Q = 0，其餘時間保持。' };
F.JKFF = { sc: { dt: '5u', tt: '8m' }, parts: [sq('Ck', 'k', 1000, 0.5, 180), sq('J', 'j', 250), sq('K', 'kk', 125), ['JKFF', 'U1', {}, { J: 'j', CLK: 'k', K: 'kk', Q: 'q', Qn: null }], vp('J', 'j'), vp('K', 'kk'), vp('CLK', 'k'), vp('Q', 'q')], plots: lplot(['J', 'K', 'CLK', 'Q']), cap: 'JK 正反器：時脈上升緣時 J=K=0 保持、J=1/K=0 設定、J=0/K=1 清除、J=K=1 反轉。' };
F.JKFFSR = { sc: { dt: '5u', tt: '8m' }, parts: [sq('Ck', 'k', 1000, 0.5, 180), cst('J', 'j', 1), cst('K', 'kk', 1), pulses('S', 's', [5.2e-3], 1.2e-3), pulses('R', 'r', [2.2e-3], 1.2e-3), ['JKFFSR', 'U1', {}, { S: 's', J: 'j', CLK: 'k', K: 'kk', R: 'r', Q: 'q', Qn: null }], vp('CLK', 'k'), vp('S', 's'), vp('R', 'r'), vp('Q', 'q')], plots: lplot(['CLK', 'S', 'R', 'Q']), cap: '含設定／重置的 JK 正反器（J = K = 1，每個時脈反轉）：R 為高時強制 Q = 0、S 為高時強制 Q = 1，不受時脈影響。' };
F.DFF = { sc: { dt: '5u', tt: '8m' }, parts: [sq('Ck', 'k', 1000, 0.5, 180), sq('D', 'd', 300), ['DFF', 'U1', {}, { D: 'd', CLK: 'k', Q: 'q', Qn: null }], vp('D', 'd'), vp('CLK', 'k'), vp('Q', 'q')], plots: lplot(['D', 'CLK', 'Q']), cap: 'D 正反器：時脈上升緣把 D 的值存入 Q。' };
F.DFFSR = { sc: { dt: '5u', tt: '8m' }, parts: [sq('Ck', 'k', 1000, 0.5, 180), sq('D', 'd', 300), pulses('S', 's', [5.2e-3], 1.2e-3), pulses('R', 'r', [1.7e-3], 1.2e-3), ['DFFSR', 'U1', {}, { S: 's', D: 'd', CLK: 'k', R: 'r', Q: 'q', Qn: null }], vp('D', 'd'), vp('CLK', 'k'), vp('S', 's'), vp('R', 'r'), vp('Q', 'q')], plots: lplot(['D', 'CLK', 'S', 'R', 'Q']), cap: '含設定／重置的 D 正反器：S、R 為高時立即設定或清除 Q，其餘時間依時脈上升緣鎖入 D。' };
F.MONO = { sc: { dt: '5u', tt: '10m' }, parts: [sq('In', 'a', 200, 0.1), ['MONO', 'U1', {}, { in: 'a', Q: 'q', Qn: 'qn' }], vp('in', 'a'), vp('Q', 'q'), vp('Qn', 'qn')], plots: lplot(['in', 'Q', 'Qn']), cap: '單擊電路（W = 1 ms）：輸入每個上升緣觸發一個 1 ms 的脈波。' };
F.MONOC = { sc: { dt: '5u', tt: '10m' }, parts: [sq('In', 'a', 200, 0.1), cst('W', 'w', 0.002), ['MONOC', 'U1', {}, { in: 'a', W: 'w', Q: 'q', Qn: null }], vp('in', 'a'), vp('Q', 'q')], plots: lplot(['in', 'Q']), cap: '可控單擊電路：脈寬由下方輸入決定（此例 0.002 = 2 ms）。' };
F.PWCT = { sc: { dt: '5u', tt: '10m' }, parts: [src('VPWL', 'In', { vals: '0 0 1 1 0 0 1 1 0 0 1 1 0', times: '0 1m 1m 1.5m 1.5m 3m 3m 4.2m 4.2m 6m 6m 8m 8m' }, 'a'), ['PWCT', 'U1', {}, { in: 'a', out: 'y' }], vp('in', 'a'), vp('width', 'y')], plots: [{ y: ['in'], h: LH, yr: LOGIC }, { y: ['width'], unit: 's', c: 1 }], lab: { width: '輸出（脈寬，秒）' }, cap: '脈寬計數器：每個下降緣輸出剛結束的脈波寬度（0.5、1.2、2 ms）並保持。' };
F.UPDN = { sc: { dt: '5u', tt: '10m' }, parts: [sq('U', 'u', 1000, 0.5, 180), pulses('D', 'dn', [6.3e-3, 6.8e-3, 7.3e-3], 1e-4), pulses('R', 'r', [8.6e-3], 3e-4), ['UPDN', 'U1', {}, { U: 'u', D: 'dn', R: 'r', out: 'y' }], vp('U', 'u'), vp('D', 'dn'), vp('R', 'r'), vp('count', 'y')], plots: lplot(['U', 'D', 'R']).concat([{ y: ['count'], c: 3 }]), lab: { count: '計數值' }, cap: '上／下數計數器：UP 每個上升緣加 1、DN 減 1，R 為高時清除為 0。' };
F.ADC = { sc: { dt: '5u', tt: '10m' }, parts: [sine('Vin', 'x', 2.5, 100, { off: 2.4 }), cst('Vref', 'r', 5), sq('Ck', 'k', 2000), ['ADC', 'U1', { bits: 4 }, { Vin: 'x', Vref: 'r', CLK: 'k', out: 'y' }], vp('in', 'x'), vp('code', 'y')], plots: [{ y: ['in'], unit: 'V' }, { y: ['code'], c: 1 }], lab: { in: '輸入 2.4 V ± 2.5 V', code: '輸出碼（4 位元）' }, cap: 'A/D 轉換器（此例 4 位元、Vref = 5 V、2 kHz 時脈）：輸出 = floor(2^N/Vref·Vin)，限制在 0～15（低於 0 V 的輸入輸出 0）。' };
F.DAC = { sc: { dt: '5u', tt: '8m' }, parts: [src('VPWL', 'Code', { vals: '0 0 5 5 10 10 15 15', times: '0 2m 2m 4m 4m 6m 6m 8m' }, 'x'), cst('Vref', 'r', 5), ['DAC', 'U1', { bits: 4 }, { Vin: 'x', Vref: 'r', out: 'y' }], vp('code', 'x'), vp('out', 'y')], plots: [{ y: ['code'] }, { y: ['out'], unit: 'V', c: 1 }], lab: { code: '輸入碼', out: '輸出電壓' }, cap: 'D/A 轉換器（4 位元、Vref = 5 V）：輸出 = Vref/2^N·碼（0、5、10、15 → 0、1.5625、3.125、4.6875 V）。' };

/* ====================================================================== digital control */
const dsp = (type, params, input, cap, more = {}) => blk(type, params, input, cap, Object.assign({ sc: { dt: '10u', tt: '20m' } }, more));
F.ZOH = dsp('ZOH', { fs: '1k' }, sinIn(1, 100), '零階保持（fs = 1 kHz）：每個取樣時刻取樣並保持。');
F.UDELAY = dsp('UDELAY', { fs: '1k' }, sinIn(1, 100), '單位延遲 1/z（fs = 1 kHz）：輸出為前一個取樣時刻的輸入。');
F.ID = dsp('ID', { fs: '1k' }, net => src('VSQU', 'Vin', { Vpp: 2, off: -1, f: 100 }, net), '離散積分器（梯形法，fs = 1 kHz）：±1 方波積分成階梯狀的三角波。');
F.IRSTD = { sc: { dt: '10u', tt: '20m' }, parts: [cst('Vin', 'x', 1), sq('Vr', 'r', 200, 0.1), ['IRSTD', 'B1', { fs: '2k' }, { in: 'x', reset: 'r', out: 'y' }], vp('reset', 'r'), vp('out', 'y')], plots: [{ y: ['reset'], h: LH, yr: LOGIC }, { y: ['out'], c: 1 }], lab: { reset: '重置', out: '輸出' }, cap: '外部重置離散積分器（輸入 1、fs = 2 kHz）：重置訊號的上升緣使輸出歸零。' };
F.IRSTI = { sc: { dt: '10u', tt: '40m' }, parts: [cst('Vin', 'x', 21600), ['IRSTI', 'B1', {}, { in: 'x', out: 'y' }], vp('angle', 'y')], plots: [{ y: ['angle'], unit: '°' }], lab: { angle: '輸出（0～360°）' }, cap: '內部重置積分器：輸入 360·60 = 21600 °/s，輸出在 0～360° 之間環繞，即 60 Hz 的角度訊號。' };
F.DD = dsp('DD', { fs: '2k' }, net => src('VTRI', 'Vin', { Vpp: 1, f: 100 }, net), '離散微分器（fs = 2 kHz）：三角波（斜率 ±200 /s）微分成 ±200 的方波。', { split: true });
F.FIR = dsp('FIR', {}, net => sq('Vin', net, 500), 'FIR 濾波器（預設 4 點移動平均，fs = 10 kHz）：方波的每個跳變分成 4 個取樣週期完成。', { sc: { dt: '5u', tt: '4m' } });
F.FILTD = dsp('FILTD', {}, net => sq('Vin', net, 200), '數位濾波器（預設為 fc = 1 kHz、fs = 10 kHz 的二階 Butterworth）：方波的步階響應。', { sc: { dt: '5u', tt: '10m' } });
F.TFZ = dsp('TFZ', {}, net => sq('Vin', net, 200), 'z 域轉移函數（預設係數同數位濾波器，fc = 1 kHz、fs = 10 kHz）。', { sc: { dt: '5u', tt: '10m' } });
F.DIGIT = dsp('DIGIT', { N: 3, vomax: 7, fs: '2k' }, sinIn(2.5, 100, { off: 2.4 }), '量化方塊（N = 3，0～5 V 對應 0～7）：先線性映射再向下截斷到整數階層。', { split: true });
F.DIGIT2 = dsp('DIGIT2', { N: 3, vomax: 7, fs: '2k' }, sinIn(2.5, 100, { off: 2.4 }), '量化方塊（含 ½ LSB 偏移，N = 3）：四捨五入到最接近的階層。', { split: true });
const reads = (n, from) => Array.from({ length: n }, (_, k) => [['MEMREAD', 'M' + k, { off: k }, { in: from, out: 'm' + k }], vp('m' + k, 'm' + k)]).flat();
F.CBUF = { sc: { dt: '10u', tt: '10m' }, parts: [ramp('Vin', 'x', 0, 10, '10m'), ['CBUF', 'B1', { fs: '1k' }, { in: 'x', out: 'v' }], vp('in', 'x')].concat(reads(4, 'v')), plots: [{ y: ['in', 'm0', 'm1', 'm2', 'm3'] }], lab: { m0: '位置 0', m1: '位置 1', m2: '位置 2', m3: '位置 3' }, cap: '環形緩衝區（長度 4、fs = 1 kHz）：斜坡輸入的取樣依序寫入 4 個位置，寫滿後從頭覆寫；以記憶體讀取方塊讀出各位置。' };
F.ARRAY = { sc: { dt: '10u', tt: '5m' }, parts: [['ARRAY', 'A1', {}, { out: 'v' }]].concat(reads(4, 'v')), plots: [{ y: ['m0', 'm1', 'm2', 'm3'] }], lab: { m0: '[0] = 2', m1: '[1] = 4', m2: '[2] = 6', m3: '[3] = 8' }, cap: '資料陣列（2 4 6 8）：以記憶體讀取方塊讀出各元素。' };
F.CONV = { sc: { dt: '10u', tt: '5m' }, parts: [['ARRAY', 'A1', { L: 3, vals: '1 2 3' }, { out: 'a' }], ['ARRAY', 'A2', { L: 2, vals: '4 5' }, { out: 'b' }], ['CONV', 'C1', {}, { A: 'a', B: 'b', out: 'v' }]].concat(reads(4, 'v')), plots: [{ y: ['m0', 'm1', 'm2', 'm3'] }], lab: { m0: '[0] = 4', m1: '[1] = 13', m2: '[2] = 22', m3: '[3] = 15' }, cap: '摺積：[1 2 3] ⊗ [4 5] = [4 13 22 15]。' };
F.MEMREAD = { sc: { dt: '10u', tt: '5m' }, parts: [['ARRAY', 'A1', {}, { out: 'v' }], ['MEMREAD', 'M1', { off: 2 }, { in: 'v', out: 'y' }], vp('out', 'y')], plots: [{ y: ['out'] }], lab: { out: '陣列 [2 4 6 8] 偏移 2 → 6' }, cap: '記憶體讀取：讀出向量中指定位置的值。' };
F.STACK = { sc: { dt: '5u', tt: '10m' }, parts: [ramp('Vin', 'x', 0, 10, '10m'), pulses('Push', 'pu', [1e-3, 2e-3, 3e-3], 2e-4), pulses('Pop', 'po', [6e-3, 7e-3, 8e-3], 2e-4), ['STACK', 'S1', {}, { Vin: 'x', push: 'pu', pop: 'po', out: 'y' }], vp('in', 'x'), vp('push', 'pu'), vp('pop', 'po'), vp('top', 'y')], plots: [{ y: ['push'], h: LH, yr: LOGIC }, { y: ['pop'], h: LH, yr: LOGIC, c: 1 }, { y: ['in', 'top'], c: 2 }], lab: { in: '輸入（斜坡）', top: '輸出（堆疊頂端）' }, cap: '堆疊（先進後出）：1、2、3 ms 推入當時的輸入 1、2、3；6、7、8 ms 依序取出，頂端值變成 2、1，最後保持。' };

/* ====================================================================== analysis and simulation settings */
const rlcStep = [src('VSTEP', 'Vs', { V: 10, ts: '0.5m' }, 'a'), ip('i', 'a', 'b'), ['RLC', 'B1', {}, { 1: 'b', 2: '0' }]];
F.SIMCTRL = { sc: { tt: '8m' }, parts: rlcStep, variants: [{ id: 'coarse', label: 'dt = 200 µs', sc: { dt: '200u' } }, { id: 'fine', label: 'dt = 5 µs', sc: { dt: '5u' } }], plots: [{ y: ['i [dt = 200 µs]', 'i [dt = 5 µs]'], unit: 'A' }], cap: '同一個 RLC 步階響應以兩種時間步長模擬：步長決定輸出點的間隔（粗步長的曲線由較少的點連成），自動細分步長讓每一點仍維持準確。' };
F.ACSWEEP = { ac: true, sc: { dt: '10u', tt: '10m' }, extra: [['ACSWEEP', acsw]], parts: [src('VSIN', 'Vac', { Vm: 0, f: '1k' }, 'a'), R('Rs', 'a', 'b', 1), ['L', 'L1', { L: '1m' }, { 1: 'b', 2: 'o' }], ['C', 'C1', { C: '100u' }, { 1: 'o', 2: '0' }], R('RL', 'o', '0', 10), ['ACP', 'Vo', {}, { n: 'o' }]], bode: ['Vo'], lab: { Vo: 'Vo / Vac' }, cap: 'AC 掃頻（線性頻域法，10 Hz～100 kHz）：LC 低通濾波器（1 mH、100 µF、10 Ω 負載）在 503 Hz 諧振，之後每十倍頻下降 40 dB。' };
F.PSWEEP = { sc: { dt: '2u', tt: '6m' }, params: 'Rx = 1k', extra: [['PSWEEP', { var: 'Rx', mode: 0, vals: '500 1k 2k' }]], nativeRuns: true, parts: [src('VSTEP', 'Vs', { V: 1, ts: '0.5m' }, 'a'), R('R1', 'a', 'o', 'Rx'), ['C', 'C1', { C: '1u' }, { 1: 'o', 2: '0' }], vp('vo', 'o')], variants: [['500', 'Rx = 500'], ['1k', 'Rx = 1k'], ['2k', 'Rx = 2k']].map(([v, p]) => ({ id: v, label: `Rx=${v}`, params: p })), plots: [{ y: ['vo [Rx=500]', 'vo [Rx=1k]', 'vo [Rx=2k]'], unit: 'V' }], cap: '參數掃描：參數檔變數 Rx 依序代入 500、1k、2k，各執行一次 RC 步階響應，結果疊在同一個畫面（τ = Rx·1 µF）。' };
const mcRuns = (() => {
  // the reference engine repeats the circuit with these values; DaedalusEngine draws its own samples
  let s = 7;
  const rnd = () => ((s = (s * 16807) % 2147483647) / 2147483647);
  const gauss = () => { for (;;) { const u = rnd() * 2 - 1, v = rnd() * 2 - 1, r = u * u + v * v; if (r > 0 && r < 1) { const z = u * Math.sqrt(-2 * Math.log(r) / r); if (Math.abs(z) <= 3) return z; } } };
  return Array.from({ length: 12 }, (_, k) => ({ id: 'mc' + k, label: k ? `#${k}` : '#0 標稱', r: k ? 1000 * (1 + 0.1 / 3 * gauss()) : 1000 }));
})();
F.MONTECARLO = { sc: { dt: '2u', tt: '4m' }, extra: [['MONTECARLO', { n: 12, seed: 7, vars: 'R1.R 10% gauss' }]], nativeRuns: true, parts: [src('VSTEP', 'Vs', { V: 1, ts: '0.5m' }, 'a'), R('R1', 'a', 'o', '1k'), ['C', 'C1', { C: '1u' }, { 1: 'o', 2: '0' }], vp('vo', 'o')], variants: mcRuns.map(m => ({ id: m.id, label: m.label, parts: parts => parts.map(p => (p[1] === 'R1' ? R('R1', 'a', 'o', m.r.toFixed(3)) : p)) })), plots: [{ y: mcRuns.map(m => `vo [${m.label}]`), unit: 'V' }], lab: Object.fromEntries(mcRuns.map((m, k) => [`vo [${m.label}]`, k ? '' : '#0 標稱值'])), compare: ['vo [#0 標稱]'], cap: '蒙地卡羅分析：R1 = 1 kΩ ±10 %（常態分佈，容差 = 3σ）執行 12 次 RC 步階響應，第 0 次為標稱值；各次結果疊在同一個畫面，可看出 τ 的分佈。' };

/* ====================================================================== switch controllers, sensors, probes */
F.ONCTRL = { sc: { dt: '1u', tt: '3m' }, parts: [sq('Vc', 'c', 1000, 0.3), ['ONCTRL', 'X1', {}, { in: 'c', gate: 'g' }], dc('Vs', 'a', 10), R('RL', 'a', 'd', 10), ['MOSFET', 'Q1', {}, { D: 'd', S: '0', G: 'g' }], vp('ctrl', 'c'), vp('vds', 'd')], plots: [{ y: ['ctrl'], h: LH, yr: LOGIC }, { y: ['vds'], unit: 'V', c: 1 }], lab: { ctrl: '控制訊號', vds: 'MOSFET 電壓' }, cap: '開關控制器把控制電路的 0/1 訊號送到功率開關的閘極：此例驅動 MOSFET 切換 10 V、10 Ω 負載。' };
F.ACTRL = { sc: { dt: '10u', tt: '40m' }, parts: [sq('Sync', 's', 50), cst('Alpha', 'al', 45), cst('En', 'en', 1), ['ACTRL', 'X1', { f: 50, pw: 30 }, { alpha: 'al', sync: 's', enable: 'en', gate: 'g' }], vp('sync', 's'), vp('gate', 'g')], plots: lplot(['sync', 'gate']), cap: '觸發角控制器（50 Hz、α = 45°、脈寬 30°）：同步訊號上升緣為 α = 0，延遲 45°（2.5 ms）後輸出觸發脈波。' };
F.GATING = { sc: { dt: '5u', tt: '1m' }, parts: [['GATING', 'G1', {}, { gate: 'g1' }], ['GATING', 'G2', { n: 6, pts: '35 92 175 187 345 357' }, { gate: 'g2' }], vp('g1', 'g1'), vp('g2', 'g2')], plots: lplot(['g1', 'g2']), lab: { g1: '預設：2 kHz、0° 與 180°', g2: '6 點：35 92 175 187 345 357°' }, cap: '閘極脈波產生器：在每個週期的指定角度切換輸出（由低電位開始）。' };
F.VSEN = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vs', 'a', 100, 60), R('RL', 'a', '0', 10), ['VSEN', 'S1', { k: 0.01 }, { '+': 'a', '-': '0', out: 'y' }], vp('v', 'a'), vp('out', 'y')], plots: [{ y: ['v'], unit: 'V' }, { y: ['out'], c: 1 }], lab: { v: '功率電路電壓', out: '感測器輸出（k = 0.01）' }, cap: '電壓感測器：把功率電路的電壓乘上增益送進控制電路（理想、無負載效應）。' };
F.ISEN = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vs', 'a', 10, 60), ['ISEN', 'S1', { k: 2 }, { in: 'a', out: 'b', o: 'y' }], ['RL', 'Z1', { R: 2, L: '10m' }, { 1: 'b', 2: '0' }], vp('out', 'y'), vp('v', 'a')], plots: [{ y: ['v'], unit: 'V' }, { y: ['out'], c: 1 }], lab: { v: '電源電壓', out: '感測器輸出（k = 2）' }, cap: '電流感測器串接在 R-L 負載支路：輸出 = 2 × 支路電流，可看出電流落後電壓。' };
F.VP = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vs', 'a', 10, 60), R('R1', 'a', 'b', 1), R('R2', 'b', '0', 1), vp('Vb', 'b')], plots: [{ y: ['Vb'], unit: 'V' }], lab: { Vb: '節點 b 對地電壓' }, cap: '電壓探棒量測節點對地電壓，探棒名稱就是波形名稱（此例為兩個相等電阻的中點）。' };
F.VP2 = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vs', 'a', 10, 60), R('R1', 'a', 'b', 1), R('R2', 'b', '0', 1), vp('Va', 'a'), vp('Vb', 'b'), vp2('Vab', 'a', 'b')], plots: [{ y: ['Va', 'Vb', 'Vab'], unit: 'V' }], lab: { Vab: '兩點電壓 a − b' }, cap: '兩點電壓探棒：量測 + 端相對 − 端的電壓（此例為 R1 兩端的電壓）。' };
F.IP = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vs', 'a', 10, 60), ip('I1', 'a', 'b'), ['RL', 'Z1', { R: 5, L: '10m' }, { 1: 'b', 2: '0' }]], plots: [{ y: ['I1'], unit: 'A' }], lab: { I1: '支路電流' }, cap: '電流探棒串接在支路中，量測由 in 流向 out 的電流（此例為 5 Ω、10 mH 負載的電流，含開始時的直流暫態）。' };
const meterIn = (net, off) => sine('Vs', net, 10, 60, { off });
F.VDCM = { sc: { dt: '50u', tt: '0.5' }, parts: [meterIn('a', 3), R('RL', 'a', '0', 10), ['VDCM', 'M1', {}, { '+': 'a', '-': '0' }], vp('v', 'a')], plots: [{ y: ['v', 'M1'], unit: 'V' }], lab: { v: '輸入（10 V 正弦 + 3 V 直流）', M1: '直流電壓表讀值' }, cap: '直流電壓表（fc = 10 Hz）：以低通濾波取平均值，約 0.3 s 後收斂到 3 V。' };
F.VACM = { sc: { dt: '50u', tt: '0.5' }, parts: [meterIn('a', 3), R('RL', 'a', '0', 10), ['VACM', 'M1', {}, { '+': 'a', '-': '0' }], vp('v', 'a')], plots: [{ y: ['v', 'M1'], unit: 'V' }], lab: { v: '輸入（10 V 正弦 + 3 V 直流）', M1: '交流電壓表讀值' }, cap: '交流電壓表（60 Hz）：去除直流後計算移動均方根，收斂到 10/√2 = 7.07 V。', differs: '均方根改為精確計算' };
F.ADCM = { sc: { dt: '50u', tt: '0.5' }, parts: [src('ISIN', 'Is', { Vm: 10, f: 60, off: 3 }, 'a'), ['ADCM', 'M1', {}, { in: 'a', out: 'b' }], R('RL', 'b', '0', 1)], plots: [{ y: ['M1'], unit: 'A' }], lab: { M1: '直流電流表讀值' }, cap: '直流電流表串接在支路中（電流 10 A 正弦 + 3 A 直流）：讀值收斂到 3 A。' };
F.AACM = { sc: { dt: '50u', tt: '0.5' }, parts: [src('ISIN', 'Is', { Vm: 10, f: 60, off: 3 }, 'a'), ['AACM', 'M1', {}, { in: 'a', out: 'b' }], R('RL', 'b', '0', 1)], plots: [{ y: ['M1'], unit: 'A' }], lab: { M1: '交流電流表讀值' }, cap: '交流電流表（電流 10 A 正弦 + 3 A 直流）：讀值收斂到 7.07 A。', differs: '均方根改為精確計算' };
F.WATT = { sc: { dt: '50u', tt: '0.5' }, parts: [sine('Vs', 'a', 10, 60), ['WATT', 'W1', {}, { 'I+': 'a', 'I-': 'b', 'V-': '0' }], R('RL', 'b', '0', 10)], plots: [{ y: ['W1'], unit: 'W' }], lab: { W1: '瓦特計讀值' }, cap: '瓦特計：10 V 峰值正弦接 10 Ω，平均功率 = Vm²/(2R) = 5 W。' };
F.DEVP = { sc: { dt: '1n', tt: '2u' }, parts: [dc('Vs', 'a', 100), R('RL', 'a', 'd', 10), ['MOSNI', 'Q1', {}, { D: 'd', S: '0', G: 'g' }], ['GATING', 'G1', { f: 0, n: 2, pts: '0.2u 1.2u' }, { gate: 'g' }], ['DEVP', 'p', { el: 'Q1', q: 'p' }, {}], ['DEVP', 'e', { el: 'Q1', q: 'e' }, {}]], plots: [{ y: ['p'], unit: 'W' }, { y: ['e'], unit: 'J', c: 1 }], lab: { p: '瞬時功率 v·i', e: '累積能量 ∫v·i dt' }, cap: '元件內部探棒讀取非理想 MOSFET（電路同 MOSNI 的例子）的瞬時功率與累積能量：每次切換的能量（Eon、Eoff）可用游標量能量曲線的差值。', differs: 'Cgd 以 Vgd = Vth 為中心（米勒平台修正）' };
F.ACP = { ac: true, sc: { dt: '10u', tt: '10m' }, extra: [['ACSWEEP', acsw]], parts: [src('VSIN', 'Vac', { Vm: 0, f: '1k' }, 'a'), R('R1', 'a', 'o', '1k'), ['C', 'C1', { C: '159n' }, { 1: 'o', 2: '0' }], ['ACP', 'Vo', {}, { n: 'o' }]], bode: ['Vo'], lab: { Vo: 'Vo / Vac' }, cap: 'AC 掃頻探棒量測節點相對於激勵源的頻率響應：RC 低通（1 kΩ、159 nF，fc = 1 kHz）。' };
F.ACP2 = { ac: true, sc: { dt: '10u', tt: '10m' }, extra: [['ACSWEEP', acsw]], parts: [cst('Ref', 'ref', 0), ['SUB2', 'E1', {}, { 'in+': 'ref', 'in-': 'y', out: 'e' }], ['PI', 'C1', { k: 2, T: '1m' }, { in: 'e', out: 'u' }], src('VSIN', 'Vac', { Vm: 0, f: '1k' }, 'v'), ['SUM2', 'J1', {}, { in1: 'u', in2: 'v', out: 'ui' }], ['LP1', 'P1', { fc: 200 }, { in: 'ui', out: 'y' }], ['ACP2', 'T', {}, { dot: 'ui', x: 'u' }]], bode: ['T'], lab: { T: '迴路增益 T' }, cap: '迴路增益探棒：激勵源經加法器注入 PI 控制器輸出，探棒圓點接注入之後、另一端接注入之前，量到 T = PI(s)·受控體(s)；|T| = 1 的穿越頻率與該處相位決定相位裕度。' };
F.CTOP = { sc: { dt: '20u', tt: '33.34m' }, parts: [sine('Vc', 'c', 5, 60), ['CTOP', 'X1', {}, { in: 'c', out: 'p' }], R('RL', 'p', '0', 10), vp('ctl', 'c'), vp('pw', 'p')], plots: [{ y: ['ctl', 'pw'], unit: 'V' }], lab: { ctl: '控制訊號', pw: '功率電路電壓' }, cap: '控制-功率介面：把控制訊號變成功率電路中對地的電壓源（此例接 10 Ω 負載）。' };

/* ====================================================================== function blocks */
const mathIn = k => [sine('X1', 'x1', 1, 100), sq('X2', 'x2', 50), src('VTRI', 'X3', { Vpp: 2, off: -1, f: 200 }, 'x3'), sine('X4', 'x4', 0.5, 300), cst('X5', 'x5', 0.2)].slice(0, k);
const mathPins = (k, n) => Object.assign({ out: 'y' }, Object.fromEntries(Array.from({ length: n }, (_, i) => [`x${i + 1}`, i < k ? `x${i + 1}` : 'zero'])));
F.MATH1 = { sc: { dt: '5u', tt: '10m' }, parts: [ramp('X1', 'x1', -2, 2, '10m'), ['MATH1', 'F1', {}, { x1: 'x1', out: 'y' }], vp('x', 'x1'), vp('out', 'y')], xy: { x: 'x', y: ['out'], xlabel: 'x' }, lab: { out: 'x*x（預設）' }, cap: '1 輸入數學函數方塊：輸出為 x 的任意運算式（預設 x*x）。' };
F.MATH2 = { sc: { dt: '10u', tt: '20m' }, parts: mathIn(2).concat([['MATH2', 'F1', {}, mathPins(2, 2)], vp('x1', 'x1'), vp('x2', 'x2'), vp('out', 'y')]), plots: [{ y: ['x1', 'x2'] }, { y: ['out'], c: 2 }], lab: { out: 'x1*x2（預設）' }, cap: '2 輸入數學函數方塊：預設 x1*x2，此例為正弦乘上方波。' };
F.MATH3 = { sc: { dt: '10u', tt: '20m' }, parts: mathIn(3).concat([['MATH3', 'F1', { expr: 'max(x1, x3) + 0.5*x2' }, mathPins(3, 3)], vp('x1', 'x1'), vp('x3', 'x3'), vp('out', 'y')]), plots: [{ y: ['x1', 'x3'] }, { y: ['out'], c: 2 }], lab: { out: 'max(x1, x3) + 0.5*x2' }, cap: '3 輸入數學函數方塊：運算式可用函數與比較，例如 max(x1, x3) + 0.5*x2。' };
F.MATH5 = { sc: { dt: '10u', tt: '20m' }, parts: mathIn(5).concat([['MATH5', 'F1', { expr: 'x2 > 0.5 ? x1 : x3 + x4 + x5' }, mathPins(5, 5)], vp('x2', 'x2'), vp('out', 'y')]), plots: [{ y: ['x2'], h: LH, yr: LOGIC }, { y: ['out'], c: 1 }], lab: { out: 'x2 > 0.5 ? x1 : x3 + x4 + x5' }, cap: '5 輸入數學函數方塊：條件運算子 ?: 依 x2 選擇兩個運算式之一。' };
F.MATH10 = { sc: { dt: '10u', tt: '20m' }, parts: mathIn(3).concat([cst('Z0', 'zero', 0), ['MATH10', 'F1', {}, mathPins(3, 10)], vp('out', 'y')]), plots: [{ y: ['out'] }], lab: { out: 'x1+x2+x3（預設）' }, cap: '10 輸入數學函數方塊：預設 x1+x2+x3（此例 x4～x10 接常數 0）。' };
F.CBLOCK = { sc: { dt: '10u', tt: '5m' }, parts: [src('VSTEP', 'X1', { V: 1 }, 'x'), ['CBLOCK', 'C1', {}, { x1: 'x', y1: 'y' }], vp('x1', 'x'), vp('y1', 'y')], plots: [{ y: ['x1', 'y1'] }], lab: { x1: '輸入 x1（1 ms 步階）', y1: '輸出 y1' }, cap: '簡化 C 程式方塊（預設程式：acc += x1*delt; y1 = 2*x1 + 100*acc;）：相當於比例 2、積分增益 100 的 PI，步階後先跳到 2 再以 100 /s 上升。' };

// three-phase watt-hour meter (not in the reference engine: there the same circuit with probes, the energy
// computed from them; CI simulates the meter itself and compares)
{
  const s3 = ['VSIN3', 'S1', { V: 380 }, { n: '0', a: 'a', b: 'b', c: 'c' }];
  const load = [['SSWI3', 'K1', {}, { A: 'a2', B: 'b2', C: 'c2', a: 'a3', b: 'b3', c: 'c3', Gate: 'g' }], src('VSTEP', 'Vg', { V: 1, ts: '20m' }, 'g'), ['R3', 'RL', { R: 10 }, { A: 'a3', B: 'b3', C: 'c3', a: '0', b: '0', c: '0' }]];
  F.WH3 = {
    sc: { dt: '20u', tt: '0.1' },
    parts: [s3, ['WH3', 'E', {}, { 'A+': 'a', 'A-': 'a1', 'B+': 'b', 'B-': 'b1', 'C+': 'c', 'C-': 'c1', N: '0' }], ip('ia', 'a1', 'a2'), ip('ib', 'b1', 'b2'), ip('ic', 'c1', 'c2')].concat(load),
    jsParts: [s3, ip('ia', 'a', 'a2'), ip('ib', 'b', 'b2'), ip('ic', 'c', 'c2'), vp('va', 'a'), vp('vb', 'b'), vp('vc', 'c')].concat(load),
    jsPost: c => {
      const T = c.Time, e = [0];
      const p = i => c.va[i] * c.ia[i] + c.vb[i] * c.ib[i] + c.vc[i] * c.ic[i];
      for (let i = 1; i < T.length; i++) e.push(e[i - 1] + 0.5 * (T[i] - T[i - 1]) * (p(i) + p(i - 1)) / 3600);
      c.E = e;
    },
    plots: [{ y: ['ia', 'ib', 'ic'], unit: 'A' }, { y: ['E'], unit: 'Wh', c: 3 }],
    lab: { E: '瓦時計讀值（Wh）' },
    cap: '三相瓦時計：380 V、60 Hz 三相電源經三相開關在 20 ms 接上 Y 接 10 Ω 負載（14.44 kW），讀值由 0 開始以 14.44 kW × t 增加（0.1 s 時約 0.32 Wh）。',
  };
}

module.exports = F;
