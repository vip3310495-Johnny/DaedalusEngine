/* =====================================================================
 * 電路模擬器 — element library (symbols, pins, parameters, descriptions)
 * Coordinates are in grid units; pins sit on integer grid points.
 * Pin kinds: p = power, ci = control input, co = control output,
 *            g = gate (control input), a = any (power or control), gnd
 * ===================================================================== */
'use strict';
var LIB = (function () {
  const D = {}; const ORDER = [];
  // ---------- SVG helpers (local grid units) ----------
  const f = n => +n.toFixed(3);
  const ln = (x1, y1, x2, y2) => `M${f(x1)} ${f(y1)}L${f(x2)} ${f(y2)}`;
  const pth = (d, cls) => `<path d="${d}"${cls ? ` class="${cls}"` : ''}/>`;
  const cir = (x, y, r, cls) => `<circle cx="${f(x)}" cy="${f(y)}" r="${f(r)}"${cls ? ` class="${cls}"` : ''}/>`;
  const rect = (x0, y0, x1, y1, cls) => `<rect x="${f(x0)}" y="${f(y0)}" width="${f(x1 - x0)}" height="${f(y1 - y0)}"${cls ? ` class="${cls}"` : ''}/>`;
  const T = (x, y, s, sz, a) => ({ x, y, s, sz: sz || 0.85, a: a || 'middle' });
  const dot = (x, y) => cir(x, y, 0.16, 'f');
  // output arrow on a lead: pin (x,y), outward direction (dx,dy); same geometry as the editor's pin-type markers
  const oArr = (x, y, dx, dy) => { const cx = x - dx * 0.42, cy = y - dy * 0.42; return pth(`M${f(cx + dx * 0.16)} ${f(cy + dy * 0.16)}L${f(cx - dx * 0.16 - dy * 0.15)} ${f(cy - dy * 0.16 + dx * 0.15)}L${f(cx - dx * 0.16 + dy * 0.15)} ${f(cy - dy * 0.16 - dx * 0.15)}Z`, 'f'); };

  // resistor zigzag between x0..x1 at y
  function zig(x0, x1, y, amp) {
    amp = amp || 0.4; const n = 6, w = (x1 - x0) / n; let d = `M${f(x0)} ${f(y)}`;
    for (let i = 0; i < n; i++) d += `L${f(x0 + w * (i + 0.5))} ${f(y + (i % 2 ? amp : -amp))}`;
    return d + `L${f(x1)} ${f(y)}`;
  }
  function coil(x0, x1, y, n) { n = n || 4; const w = (x1 - x0) / n, r = w / 2; let d = `M${f(x0)} ${f(y)}`; for (let i = 0; i < n; i++) d += `a${f(r)} ${f(r)} 0 0 1 ${f(w)} 0`; return d; }
  function capH(x, y, gap) { gap = gap || 0.22; return ln(x - gap, y - 0.75, x - gap, y + 0.75) + ln(x + gap, y - 0.75, x + gap, y + 0.75); }
  function diodeH(x, y, s) { s = s || 0.65; return pth(`M${f(x - s)} ${f(y - s)}L${f(x + s)} ${f(y)}L${f(x - s)} ${f(y + s)}Z`, 'f') + pth(ln(x + s, y - s, x + s, y + s)); }
  function diodeV(x, y, s, up) { s = s || 0.6; const d = up ? -1 : 1; return pth(`M${f(x - s)} ${f(y - d * s)}L${f(x)} ${f(y + d * s)}L${f(x + s)} ${f(y - d * s)}Z`, 'f') + pth(ln(x - s, y + d * s, x + s, y + d * s)); }

  // ---------- definition helper ----------
  let curCat = '';
  function cat(c) { curCat = c; }
  function def(type, o) {
    o.type = type; o.cat = o.cat || curCat; o.params = o.params || []; o.pins = o.pins || [];
    D[type] = o; ORDER.push(type);
  }
  const PP = (x, y, n) => ({ x, y, k: 'p', n });
  const CI = (x, y, n) => ({ x, y, k: 'ci', n });
  const CO = (x, y, n) => ({ x, y, k: 'co', n });
  const GT = (x, y, n) => ({ x, y, k: 'g', n: n || 'Gate' });
  const AN = (x, y, n) => ({ x, y, k: 'a', n });
  const prm = (k, l, d, u, desc, o) => Object.assign({ k, l, d: String(d), u: u || '', desc: desc || '' }, o || {});
  const choice = (k, l, d, opts, desc) => ({ k, l, d: String(d), type: 'choice', opts, desc: desc || '' });
  // physical (SPICE) model selector + parameter text; presets live in engine_devices (SIM.DEV_PRESETS)
  const SPICEP = (kind) => prm('spice', 'SPICE 模型參數（空白＝使用所選型號）', '', '', '可直接貼上廠商的 .model 敘述或 KEY=VALUE 參數（SPICE 單位：M = 毫、MEG = 百萬）。有填寫時優先於上方型號。溫度固定 27 °C，溫度係數與高階效應參數會被忽略並提出警告。', { type: 'code', presetKind: kind });
  const MODELP = (opts, d, kind) => Object.assign(choice('model', 'Model 模型', d, opts, '選擇理想（分段線性）模型或 SPICE 物理模型。物理模型以牛頓法求解指數 / 平方律特性與接面電荷，精度與 SPICE 相同但較慢。'), { presetKind: kind });
  const D_OPTS = [['ideal', '理想開關（PSIM 分段線性，最快）'], ['ds', '資料手冊參數（Vf@If、Rd、trr/Qrr、Cj、BV；進階參數）'], ['1N4148', '1N4148 / BAS316 / 1N914 小訊號開關'], ['1N4007', '1N4007 整流'], ['1N5819', '1N5819 蕭特基'], ['custom', '自訂（下方 SPICE 參數）']];
  // datasheet-style physical diode parameters (model = 'ds'); converted to the SPICE level-1 model by the engine
  const D_DS = [prm('vfd', 'Vf 順向壓降 @ If', 0.85, 'V', '資料手冊的順向壓降（模型 = 資料手冊參數時使用）。', { adv: 1 }), prm('ifd', 'If 順向壓降的測試電流', 5, 'A', '', { adv: 1 }),
    prm('rd', 'Rd 動態電阻', '20m', 'Ω', 'Vf–If 曲線在 If 附近的斜率；0 表示純指數。', { adv: 1 }),
    prm('trr', 'trr 逆向恢復時間', 0, 's', '0 = 無逆向恢復。換算 TT = trr（一階近似）；若填了 Qrr 則以 Qrr 為準。', { adv: 1 }),
    prm('qrr', 'Qrr 逆向恢復電荷', 0, 'C', '0 = 用 trr。換算 TT = Qrr / Irr。', { adv: 1 }), prm('irr', 'Irr Qrr 的測試電流', 0, 'A', '0 = 同 If。', { adv: 1 }),
    prm('cjd', 'Cj 接面電容', 0, 'F', '0 = 無。資料手冊在逆向電壓 Vcj 下量得的值，換算為零偏壓 CJO。', { adv: 1 }), prm('vcj', 'Vcj Cj 的測試逆向電壓', 0, 'V', '', { adv: 1 }),
    prm('bvd', 'BV 逆向崩潰電壓', 0, 'V', '0 = 無崩潰。', { adv: 1 })];
  const ROFFP = prm('roff', 'Off Resistance 關斷電阻', '1M', 'Ω', '關斷時的漏電阻。PSIM 固定為 1 MΩ；模擬 100 kΩ 等級的高阻抗類比電路時建議改為 1G 以上，否則漏電流會造成可觀的誤差。', { adv: 1 });
  const INIT = choice('init', 'Initial Position 初始狀態', 0, [['0', '0：關斷 (Off)'], ['1', '1：導通 (On)']], '模擬開始時開關的狀態。');

  // generic 2-terminal horizontal body with leads
  function two(body, half) { half = half || 2; return { lead: pth(ln(-2, 0, -half, 0) + ln(half, 0, 2, 0)), body }; }

  /* ===================================================================
   * POWER — RLC branches
   * =================================================================== */
  cat('Elements/Power/RLC Branches');
  const RLC_SYM = {
    R: () => pth(ln(-2, 0, -1.2, 0) + zig(-1.2, 1.2, 0) + ln(1.2, 0, 2, 0)),
    L: () => pth(ln(-2, 0, -1.2, 0) + coil(-1.2, 1.2, 0) + ln(1.2, 0, 2, 0)),
    C: () => pth(ln(-2, 0, -0.22, 0) + ln(0.22, 0, 2, 0) + capH(0, 0)),
  };
  def('R', { name: 'Resistor', zh: '電阻', pre: 'R', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -0.6, 2, 0.6], show: ['R'],
    sym: () => ({ g: RLC_SYM.R() }),
    params: [prm('R', 'Resistance 電阻', 10, 'Ω', '電阻值，單位歐姆。')],
    desc: '理想線性電阻，v = R·i。電阻值可為 0（視為短路），不可為負值。' });
  def('L', { name: 'Inductor', zh: '電感', pre: 'L', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -0.7, 2, 0.3], show: ['L'],
    sym: () => ({ g: RLC_SYM.L() + dot(-1.6, -0.5) }),
    params: [prm('L', 'Inductance 電感', '1m', 'H', '電感值，單位亨利。'), prm('i0', 'Initial Current 初始電流', 0, 'A', 't=0 時流入有點端（接腳 1）的電流。')],
    desc: '理想線性電感，v = L·di/dt。以梯形積分法離散化，開關動作後的第一步改用後向尤拉法以抑制數值振盪。電流方向以流入接腳 1（圓點端）為正。' });
  def('C', { name: 'Capacitor', zh: '電容', pre: 'C', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -0.8, 2, 0.8], show: ['C'],
    sym: () => ({ g: RLC_SYM.C() + `<path d="${ln(-0.9, -0.75, -0.9, -0.25) + ln(-1.15, -0.5, -0.65, -0.5)}" class="thin"/>` }),
    params: [prm('C', 'Capacitance 電容', '100u', 'F', '電容值，單位法拉。'), prm('v0', 'Initial Cap. Voltage 初始電壓', 0, 'V', 't=0 時接腳 1 相對接腳 2 的電壓。')],
    desc: '理想線性電容，i = C·dv/dt。可設定初始電壓。若與電壓源直接並聯且初始電壓不同，t=0 的初始解會以近似值處理並提出警告。' });
  const brParams = (r, l, c) => [].concat(
    r ? [prm('R', 'Resistance 電阻', 1, 'Ω', '串聯電阻；0 表示無此元件。')] : [],
    l ? [prm('L', 'Inductance 電感', '1m', 'H', '串聯電感；0 表示無此元件。')] : [],
    c ? [prm('C', 'Capacitance 電容', '100u', 'F', '串聯電容；0 表示無此元件（短路）。')] : [],
    l ? [prm('i0', 'Initial Current 初始電流', 0, 'A', '支路初始電流。')] : [],
    c ? [prm('v0', 'Initial Cap. Voltage 初始電壓', 0, 'V', '電容初始電壓。')] : []);
  function brSym(parts) {
    const n = parts.length, span = n * 2.4, x0 = -span / 2; let d = ln(-(n + 1), 0, x0, 0) + ln(-x0, 0, n + 1, 0);
    let g = '';
    parts.forEach((p, i) => {
      const a = x0 + i * 2.4 + 0.2, b = a + 2.0;
      if (p === 'R') d += ln(a - 0.2, 0, a, 0) + zig(a, b, 0) + ln(b, 0, b + 0.2, 0);
      if (p === 'L') d += ln(a - 0.2, 0, a, 0) + coil(a, b, 0) + ln(b, 0, b + 0.2, 0);
      if (p === 'C') { const m = (a + b) / 2; d += ln(a - 0.2, 0, m - 0.22, 0) + ln(m + 0.22, 0, b + 0.2, 0) + capH(m, 0); }
    });
    return pth(d) + g;
  }
  [['RL', 'R-L Branch', 'R-L 串聯支路', ['R', 'L']], ['RC', 'R-C Branch', 'R-C 串聯支路', ['R', 'C']], ['LC', 'L-C Branch', 'L-C 串聯支路', ['L', 'C']], ['RLC', 'R-L-C Branch', 'R-L-C 串聯支路', ['R', 'L', 'C']]].forEach(([t, nm, zh, parts]) => {
    const n = parts.length + 1;
    def(t, { name: nm, zh, pre: t, pins: [PP(-n, 0, '1'), PP(n, 0, '2')], bb: [-n, -0.8, n, 0.8], show: parts,
      sym: () => ({ g: brSym(parts) }),
      params: brParams(parts.includes('R'), parts.includes('L'), parts.includes('C')),
      desc: zh + '：' + parts.join('、') + ' 串聯成單一支路，以單一支路電流變數求解（v = R·i + L·di/dt + vC）。任一參數設 0 表示該元件不存在，但不可全部為 0。' });
  });
  function br3Sym(parts) {
    let d = ''; const n = 3;
    [-1, 0, 1].forEach(y => {
      d += ln(-2, y, -1.3, y) + ln(1.3, y, 2, y);
      const seg = 2.6 / parts.length;
      parts.forEach((p, i) => {
        const a = -1.3 + i * seg + 0.08, b = a + seg - 0.16;
        if (i > 0) d += ln(a - 0.16, y, a, y);
        if (p === 'R') d += zig(a, b, y, 0.22);
        if (p === 'L') d += coil(a, b, y, 3);
        if (p === 'C') { const m = (a + b) / 2; d += ln(a, y, m - 0.12, y) + ln(m + 0.12, y, b, y) + ln(m - 0.12, y - 0.35, m - 0.12, y + 0.35) + ln(m + 0.12, y - 0.35, m + 0.12, y + 0.35); }
      });
    });
    return pth(d) + dot(-1.7, -1.35);
  }
  [['R3', '3-ph Resistor', '三相電阻', ['R']], ['RL3', '3-ph R-L Branch', '三相 R-L 支路', ['R', 'L']], ['RC3', '3-ph R-C Branch', '三相 R-C 支路', ['R', 'C']], ['RLC3', '3-ph R-L-C Branch', '三相 R-L-C 支路', ['R', 'L', 'C']]].forEach(([t, nm, zh, parts]) => {
    def(t, { name: nm, zh, pre: t, pins: [PP(-2, -1, 'A'), PP(-2, 0, 'B'), PP(-2, 1, 'C'), PP(2, -1, 'a'), PP(2, 0, 'b'), PP(2, 1, 'c')], bb: [-2, -1.6, 2, 1.5], show: parts,
      sym: () => ({ g: br3Sym(parts) }),
      params: brParams(parts.includes('R'), parts.includes('L'), parts.includes('C')).filter(p => p.k !== 'i0' && p.k !== 'v0'),
      desc: zh + '：三相對稱支路，每相各為 ' + parts.join('-') + ' 串聯，初始電流與電容電壓皆為 0。有圓點的一相為 A 相（最上方）。' });
  });
  def('RHEO', { name: 'Rheostat', zh: '可變電阻', pre: 'RH', pins: [PP(-2, 0, 'k'), PP(2, 0, 'm'), PP(0, -2, 't')], bb: [-2, -2, 2, 0.6], show: ['R', 'tap'],
    sym: () => ({ g: pth(ln(-2, 0, -1.2, 0) + zig(-1.2, 1.2, 0) + ln(1.2, 0, 2, 0) + ln(0, -2, 0, -0.55)) + pth('M-0.25 -0.95L0 -0.5L0.25 -0.95Z', 'f') }),
    params: [prm('R', 'Total Resistance 總電阻', 10, 'Ω', 'k 與 m 之間的總電阻。'), prm('tap', 'Tap Position (0 to 1)', 0.5, '', '抽頭位置；k 與 t 之間電阻 = R·Tap。')],
    desc: '附抽頭的電阻。k–t 之間為 R·Tap，t–m 之間為 R·(1−Tap)。' });
  def('LSAT', { name: 'Saturable Inductor', zh: '飽和電感', pre: 'LS', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -1, 2, 0.4],
    sym: () => ({ g: pth(ln(-2, 0, -1.2, 0) + coil(-1.2, 1.2, 0) + ln(1.2, 0, 2, 0)) + pth('M-1.2 -0.95L-0.3 -0.95Q0.4 -0.95 1.2 -0.6', 'thin') }),
    params: [prm('tab', 'Current vs. Inductance (i1 L1 i2 L2 ...)', '1 10m 2 6m 5 3m', '', '成對的 (電流, 電感)，L = λ/i；電流必須遞增。λ–i 曲線以分段線性近似。', { type: 'list' })],
    desc: '考慮鐵心飽和的非線性電感。以 (i, L) 點定義磁通鏈 λ = L·i 的分段線性曲線，模擬時以牛頓法求解。' });
  def('NONI', { name: 'Nonlinear Element (i=f(v))', zh: '非線性元件（電導型）', pre: 'NL', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -0.8, 2, 0.8],
    sym: () => ({ g: pth(ln(-2, 0, -1, 0) + ln(1, 0, 2, 0)) + rect(-1, -0.6, 1, 0.6) + pth('M-0.7 0.4L0.7 -0.4', 'thin'), t: [] }),
    params: [prm('f', 'Expression f(v)', '1e-14*(exp(40*v)-1)', '', '電流 i 對電壓 v 的函數，v = v(1) − v(2)。', { type: 'expr' }), prm('df', 'Expression df/dv', '40e-14*exp(40*v)', '', 'f(v) 對 v 的導數，供牛頓法使用。', { type: 'expr' }),
      prm('x0', 'Initial Value vo', 0, 'V', '初始電壓猜值。'), prm('lo', 'Lower Limit of v', -1000, 'V', '迭代時 v 的下限。'), prm('hi', 'Upper Limit of v', 1, 'V', '迭代時 v 的上限。')],
    desc: '電導型非線性元件 i = f(v)。需提供運算式與其導數；良好的初始值與上下限有助收斂。例：二極體 i = 1e-14·(e^(40v) − 1)。' });
  def('NONV', { name: 'Nonlinear Element (v=f(i))', zh: '非線性元件（電阻型）', pre: 'NL', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -0.8, 2, 0.8],
    sym: () => ({ g: pth(ln(-2, 0, -1, 0) + ln(1, 0, 2, 0)) + rect(-1, -0.6, 1, 0.6) + pth('M-0.7 -0.4L0.7 0.4', 'thin') }),
    params: [prm('f', 'Expression f(i)', '10*i+0.1*i^3', '', '電壓 v 對電流 i 的函數，i 由接腳 1 流向接腳 2。', { type: 'expr' }), prm('df', 'Expression df/di', '10+0.3*i^2', '', 'f(i) 對 i 的導數。', { type: 'expr' }),
      prm('x0', 'Initial Value io', 0, 'A', '初始電流猜值。'), prm('lo', 'Lower Limit of i', -1000, 'A', '迭代時 i 的下限。'), prm('hi', 'Upper Limit of i', 1000, 'A', '迭代時 i 的上限。')],
    desc: '電阻型非線性元件 v = f(i)，以牛頓法求解。' });

  /* ===================================================================
   * POWER — Switches
   * =================================================================== */
  cat('Elements/Power/Switches');
  const DIODE_P = [prm('vf', 'Diode Voltage Drop 導通壓降', 0, 'V', '導通時的順向壓降（理想模型）。'), INIT, prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', '導通電阻（PSIM 理想開關為 10 µΩ）。', { adv: 1 }), ROFFP, MODELP(D_OPTS, 'ideal', 'D')].concat(D_DS, [SPICEP('D')]);
  def('DIODE', { name: 'Diode', zh: '二極體', pre: 'D', pins: [PP(-2, 0, 'A'), PP(2, 0, 'K')], bb: [-2, -0.8, 2, 0.8],
    sym: () => ({ g: pth(ln(-2, 0, -0.65, 0) + ln(0.65, 0, 2, 0)) + diodeH(0, 0) }), params: DIODE_P,
    desc: '理想二極體（分段線性模型）：順偏電壓超過導通壓降時導通，電流降到零時關斷。導通電阻 10 µΩ、關斷電阻預設 1 MΩ（與 PSIM 相同，可在進階參數調整）。導通/關斷時刻以線性內插精確定位（切換瞬間內插）。Model 選「資料手冊參數」時用進階參數的 Vf@If、Rd、trr 或 Qrr、Cj@Vcj、BV 建立物理模型：IS = If/exp((Vf − If·Rd)/Vt)、RS = Rd、TT = Qrr/Irr（或 trr）、CJO = Cj·√(1 + Vcj/0.7)、BV。Model 選 SPICE 型號或自訂時改用 SPICE level-1 二極體模型：I = IS·(e^(V/(N·Vt)) − 1)、串聯 RS、逆向崩潰 BV/IBV、接面電容 CJO/VJ/M/FC 與擴散電容 TT（逆向恢復），此時理想模型的參數不使用。' });
  def('LED', { name: 'Light-emitting Diode', zh: '發光二極體', pre: 'LED', pins: [PP(-2, 0, 'A'), PP(2, 0, 'K')], bb: [-2, -1.6, 2, 0.8], show: ['model'],
    sym: () => ({ g: pth(ln(-2, 0, -0.65, 0) + ln(0.65, 0, 2, 0)) + diodeH(0, 0) + pth('M0 -0.9L0.5 -1.5M0.5 -0.9L1 -1.5', 'thin') + pth('M0.5 -1.5l-0.25 0.05l0.15 0.2zM1 -1.5l-0.25 0.05l0.15 0.2z', 'f') }),
    params: [prm('vf', 'Threshold Voltage 導通電壓', 2, 'V', 'LED 順向導通電壓（理想模型）。'), prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', '導通後的串聯電阻（理想模型）。'), ROFFP, MODELP([['ideal', '理想（分段線性）'], ['LEDRED', '紅光 LED（Vf ≈ 1.85 V @ 20 mA）'], ['custom', '自訂（下方 SPICE 參數）']], 'ideal', 'D'), SPICEP('D')],
    desc: '發光二極體。理想模型：v > Vth 時導通，壓降 = Vth + Ron·i，電流降到零時關斷。SPICE 模型：以 IS、N、RS 等參數計算實際的指數 I-V 特性（小電流時 Vf 會明顯低於額定值）。' });
  def('ZENER', { name: 'Zener', zh: '稽納二極體', pre: 'ZD', pins: [PP(-2, 0, 'A'), PP(2, 0, 'K')], bb: [-2, -0.9, 2, 0.9], show: ['vb'],
    sym: () => ({ g: pth(ln(-2, 0, -0.65, 0) + ln(0.65, 0, 2, 0)) + pth('M-0.65 -0.65L0.65 0L-0.65 0.65Z', 'f') + pth('M0.35 -0.85L0.65 -0.65L0.65 0.65L0.95 0.85') }),
    params: [prm('vb', 'Breakdown Voltage 崩潰電壓', 5.1, 'V', '逆向崩潰電壓 VB。'), prm('vf', 'Forward Voltage Drop 順向壓降', 0, 'V', '順向導通壓降。'), prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', '', { adv: 1 }), ROFFP, MODELP([['ideal', '理想（分段線性）'], ['ZENER', '通用稽納（崩潰電壓取 VB 參數）'], ['custom', '自訂（下方 SPICE 參數）']], 'ideal', 'D'), SPICEP('D')],
    desc: '稽納二極體。理想模型：順偏時如一般二極體；逆偏時 VKA 超過 VB 即箝位於 VB（實際為 VB + Ron·I）。SPICE 模型：崩潰區以 BV、IBV 的指數特性表示（膝點軟化），自訂參數未給 BV 時取 VB 參數。' });
  def('DIAC', { name: 'DIAC', zh: '雙向觸發二極體', pre: 'DIAC', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -1.2, 2, 1.2], show: ['vbo'],
    sym: () => ({ g: pth(ln(-2, 0, -0.6, 0) + ln(0.6, 0, 2, 0) + ln(-0.6, -1, -0.6, 1) + ln(0.6, -1, 0.6, 1)) + pth('M-0.6 -0.9L0.6 -0.45L-0.6 0Z', 'f') + pth('M0.6 0L-0.6 0.45L0.6 0.9Z', 'f') }),
    params: [prm('vbo', 'Breakover Voltage 轉折電壓', 30, 'V', '開始導通的轉折電壓。'), prm('vbb', 'Breakback Voltage 導通壓降', 5, 'V', '雪崩導通後的壓降。')],
    desc: '雙向觸發二極體：任一方向電壓超過轉折電壓即雪崩導通，導通壓降為 Breakback Voltage，電流降到零時關斷。' });
  const THY_SYM = (gx) => pth(ln(-2, 0, -0.65, 0) + ln(0.65, 0, 2, 0) + ln(0.65, 0.4, 1, 1.2) + ln(1, 1.2, 1, 2)) + diodeH(0, 0);
  def('THY', { name: 'Thyristor', zh: '閘流體 SCR', pre: 'THY', pins: [PP(-2, 0, 'A'), PP(2, 0, 'K'), GT(1, 2)], bb: [-2, -0.8, 2, 2],
    sym: () => ({ g: THY_SYM() }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '導通壓降。'), prm('ih', 'Holding Current 維持電流', 0, 'A', '電流低於此值即關斷。'), prm('il', 'Latching Current 栓鎖電流', 0, 'A', '觸發脈波移除後維持導通所需的最小電流。'), INIT],
    desc: '閘流體：閘極訊號為高（> 0.5）且順偏時導通；導通後即使閘極訊號消失仍維持導通，直到電流低於維持電流。閘極必須接 Gating Block、On-Off Controller 或 Alpha Controller 等控制訊號。' });
  def('TRIAC', { name: 'TRIAC', zh: '雙向閘流體', pre: 'TRIAC', pins: [PP(-2, 0, '1'), PP(2, 0, '2'), GT(1, 2)], bb: [-2, -1.2, 2, 2],
    sym: () => ({ g: pth(ln(-2, 0, -0.6, 0) + ln(0.6, 0, 2, 0) + ln(-0.6, -1, -0.6, 1) + ln(0.6, -1, 0.6, 1) + ln(0.6, 0.9, 1, 1.3) + ln(1, 1.3, 1, 2)) + pth('M-0.6 -0.9L0.6 -0.45L-0.6 0Z', 'f') + pth('M0.6 0L-0.6 0.45L0.6 0.9Z', 'f') }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '導通壓降。')],
    desc: '雙向閘流體，等同兩個反向並聯的閘流體共用閘極；閘極為高且任一方向順偏即導通，電流過零時關斷（維持與栓鎖電流為 0）。' });
  def('GTO', { name: 'GTO', zh: '閘關閘流體', pre: 'GTO', pins: [PP(-2, 0, 'A'), PP(2, 0, 'K'), GT(1, 2)], bb: [-2, -0.8, 2, 2],
    sym: () => ({ g: THY_SYM() + pth('M0.75 1.05L1.25 1.05M0.75 1.35L1.25 1.35', 'thin') }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '導通壓降。'), INIT],
    desc: '閘關閘流體（對稱型，可阻斷正反向電壓）：閘極為高且順偏時導通；閘極轉低或電流降到零時關斷。' });
  // vertical transistor family: top (0,-2), bottom (0,2), gate (-2,0)
  const BJT_SYM = (pnp) => pth(ln(0, -2, 0, -1.1) + ln(0, 2, 0, 1.1) + ln(-2, 0, -0.6, 0) + ln(-0.6, -0.8, -0.6, 0.8) + ln(-0.6, -0.35, 0, -1.1) + ln(-0.6, 0.35, 0, 1.1)) +
    (pnp ? pth('M-0.55 0.38L-0.05 0.58L-0.2 0.85Z', 'f') : pth('M0 1.1L-0.45 0.95L-0.2 0.7Z', 'f'));
  const MOS_SYM = (p) => pth(ln(0, -2, 0, -0.8) + ln(0, 2, 0, 0.8) + ln(-2, 0, -0.9, 0) + ln(-0.9, -0.8, -0.9, 0.8) + ln(-0.55, -0.9, -0.55, -0.4) + ln(-0.55, -0.25, -0.55, 0.25) + ln(-0.55, 0.4, -0.55, 0.9) + ln(-0.55, -0.65, 0, -0.65) + ln(-0.55, 0.65, 0, 0.65) + ln(-0.55, 0, 0, 0) + ln(0, 0, 0, 0.8) + ln(0, -0.8, 0, -0.65)) +
    (p ? pth('M-0.05 0L-0.45 -0.2L-0.45 0.2Z', 'f') : pth('M-0.55 0L-0.15 -0.2L-0.15 0.2Z', 'f')) + pth(ln(0, -0.8, 0.8, -0.8) + ln(0.8, -0.8, 0.8, 0.8) + ln(0.8, 0.8, 0, 0.8), 'thin') + (p ? pth('M0.5 -0.3L1.1 -0.3L0.8 0.3Z', 'f thin') : pth('M0.5 0.3L1.1 0.3L0.8 -0.3Z', 'f thin'));
  const IGBT_SYM = () => pth(ln(0, -2, 0, -1.1) + ln(0, 2, 0, 1.1) + ln(-2, 0, -0.9, 0) + ln(-0.9, -0.7, -0.9, 0.7) + ln(-0.6, -0.8, -0.6, 0.8) + ln(-0.6, -0.35, 0, -1.1) + ln(-0.6, 0.35, 0, 1.1)) + pth('M0 1.1L-0.45 0.95L-0.2 0.7Z', 'f') + pth(ln(0, -1.4, 0.9, -1.4) + ln(0.9, -1.4, 0.9, 1.4) + ln(0.9, 1.4, 0, 1.4), 'thin') + pth('M0.6 0.3L1.2 0.3L0.9 -0.3Z', 'f thin') + pth(ln(0.6, -0.3, 1.2, -0.3), 'thin');
  def('NPN', { name: 'npn Transistor', zh: 'NPN 電晶體（開關）', pre: 'Q', pins: [PP(0, -2, 'C'), PP(0, 2, 'E'), GT(-2, 0, 'B')], bb: [-2, -2, 0.6, 2],
    sym: () => ({ g: BJT_SYM(false) }), params: [prm('vf', 'Saturation Voltage 飽和壓降', 0, 'V', 'C-E 導通壓降。'), INIT, ROFFP],
    desc: '開關模式 NPN 電晶體（理想開關）：基極（閘極）訊號為高且 VCE > 0 時導通；閘極轉低或電流降到零時關斷。注意：此模型由閘極電壓訊號控制，而非基極電流，且可阻斷反向電壓。' });
  def('PNP', { name: 'pnp Transistor', zh: 'PNP 電晶體（開關）', pre: 'Q', pins: [PP(0, -2, 'C'), PP(0, 2, 'E'), GT(-2, 0, 'B')], bb: [-2, -2, 0.6, 2],
    sym: () => ({ g: BJT_SYM(true) }), params: [prm('vf', 'Saturation Voltage 飽和壓降', 0, 'V', 'E-C 導通壓降。'), INIT, ROFFP],
    desc: '開關模式 PNP 電晶體：閘極訊號為低且負偏（VCE < 0）時導通，電流由 E 流向 C；閘極轉高或電流降到零時關斷。' });
  def('IGBT', { name: 'IGBT', zh: 'IGBT', pre: 'Q', pins: [PP(0, -2, 'C'), PP(0, 2, 'E'), GT(-2, 0, 'G')], bb: [-2, -2, 1.3, 2],
    sym: () => ({ g: IGBT_SYM() }),
    params: [prm('vf', 'Saturation Voltage 飽和壓降', 0, 'V', '主開關導通壓降。'), prm('vd', 'Diode Voltage Drop 二極體壓降', 0, 'V', '反並聯二極體壓降。'), INIT],
    desc: 'IGBT 開關模組：主動開關加反並聯二極體。閘極為高且 VCE > 0 時主開關導通；閘極轉低或電流反向時關斷，反向電流經由二極體流通。電流探棒量到的是整個模組電流。' });
  def('MOSFET', { name: 'MOSFET', zh: 'N 通道 MOSFET', pre: 'Q', pins: [PP(0, -2, 'D'), PP(0, 2, 'S'), GT(-2, 0, 'G')], bb: [-2, -2, 1.2, 2],
    sym: () => ({ g: MOS_SYM(false) }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', '通道導通電阻 Rds(on)。'), prm('vd', 'Diode Voltage Drop 二極體壓降', 0, 'V', '本體二極體壓降。'), INIT, ROFFP],
    desc: 'N 通道 MOSFET 開關模組：主動開關加反並聯本體二極體。閘極為高（> 0.5）且 VDS > 0 時通道導通；閘極轉低時關斷，反向電流經由本體二極體。' });
  def('PMOS', { name: 'p-MOSFET', zh: 'P 通道 MOSFET', pre: 'Q', pins: [PP(0, -2, 'D'), PP(0, 2, 'S'), GT(-2, 0, 'G')], bb: [-2, -2, 1.2, 2],
    sym: () => ({ g: MOS_SYM(true) }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', '通道導通電阻。'), prm('vd', 'Diode Voltage Drop 二極體壓降', 0, 'V', '本體二極體壓降。'), INIT, ROFFP],
    desc: 'P 通道 MOSFET 開關模組：閘極訊號為低且 VDS < 0 時導通（電流由 S 流向 D）；含反並聯本體二極體（D→S）。' });
  def('SSWI', { name: 'Bi-directional Switch', zh: '雙向開關', pre: 'S', pins: [PP(-2, 0, '1'), PP(2, 0, '2'), GT(0, 2)], bb: [-2, -1, 2, 2],
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0.8, 0, 2, 0) + ln(-0.8, 0, 0.7, -0.8) + ln(0, 2, 0, -0.3)) + cir(-0.8, 0, 0.12, 'f') + cir(0.8, 0, 0.12, 'f') }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', ''), INIT],
    desc: '雙向開關：閘極訊號為高即導通，不論電壓極性，可雙向導通；閘極轉低即關斷。' });
  def('PBSW', { name: 'Push Button Switch', zh: '按鈕開關', pre: 'SW', pins: [PP(-2, 0, '1'), PP(2, 0, '2')], bb: [-2, -1.6, 2, 0.4],
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0.8, 0, 2, 0) + ln(-1, -0.6, 1, -0.6) + ln(0, -0.6, 0, -1.5) + ln(-0.4, -1.5, 0.4, -1.5)) + cir(-0.8, 0, 0.12, 'f') + cir(0.8, 0, 0.12, 'f') }),
    params: [INIT, prm('times', 'Toggle Time(s) 切換時間', '', 's', '在這些時間點切換開關狀態（以空白分隔，例如 "5m 12m"）。留白表示整段模擬維持初始狀態。', { type: 'list' })],
    desc: '按鈕開關：以初始狀態加上指定的切換時間點操作（例如 5 ms 按下、12 ms 放開）。切換時刻會被設為模擬斷點，時間精確對齊。' });
  def('SSWI3', { name: '3-ph Bi-directional Switch', zh: '三相雙向開關', pre: 'S', pins: [PP(-2, -1, 'A'), PP(-2, 0, 'B'), PP(-2, 1, 'C'), PP(2, -1, 'a'), PP(2, 0, 'b'), PP(2, 1, 'c'), GT(0, 3)], bb: [-2, -1.6, 2, 3],
    sym: () => ({ g: pth([-1, 0, 1].map(y => ln(-2, y, -0.7, y) + ln(0.7, y, 2, y) + ln(-0.7, y, 0.6, y - 0.6)).join('') + ln(0, 3, 0, 1.4)) + rect(-1.1, -1.5, 1.1, 1.4, 'dash') }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', '')],
    desc: '三個由同一閘極訊號控制的雙向開關（A-a、B-b、C-c）。' });
  const blade = (x0, y0, x1, y1) => ln(x0, y0, x1, y1);
  def('SPDT', { name: 'SPDT Bi-directional Switch', zh: '單刀雙擲雙向開關', pre: 'S', pins: [PP(-2, 0, 'COM'), PP(2, -1, '1'), PP(2, 1, '2'), GT(0, 2)], bb: [-2, -1.5, 2, 2],
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0.8, -1, 2, -1) + ln(0.8, 1, 2, 1) + blade(-0.8, 0, 0.75, 0.75)) + pth(ln(0, 2, 0, 0.45), 'dash') + cir(-0.8, 0, 0.12, 'f') + cir(0.8, -1, 0.12, 'f') + cir(0.8, 1, 0.12, 'f'),
      t: [T(1.35, -1.25, '1', 0.5), T(1.35, 0.75, '2', 0.5)] }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', ''), INIT],
    desc: '單刀雙擲（SPDT）雙向開關：閘極訊號為高時 COM 接到 1、2 斷開；為低時 COM 接到 2、1 斷開（圖上的刀片畫在閘極為低的位置）。兩個接點同時切換（理想，無先斷後接延遲）；初始狀態以 t=0 的閘極訊號為準。' });
  def('DPST', { name: 'DPST Bi-directional Switch', zh: '雙刀單擲雙向開關', pre: 'S', pins: [PP(-2, -1, '1a'), PP(2, -1, '2a'), PP(-2, 1, '1b'), PP(2, 1, '2b'), GT(0, 3)], bb: [-2, -1.8, 2, 3],
    sym: () => ({ g: pth([-1, 1].map(y => ln(-2, y, -0.8, y) + ln(0.8, y, 2, y) + blade(-0.8, y, 0.7, y - 0.8)).join('')) + pth(ln(0, 3, 0, -1.4), 'dash') + [-1, 1].map(y => cir(-0.8, y, 0.12, 'f') + cir(0.8, y, 0.12, 'f')).join('') }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', ''), INIT],
    desc: '雙刀單擲（DPST）雙向開關：兩組雙向開關（1a-2a、1b-2b）由同一閘極訊號控制，閘極為高時兩組同時導通、為低時同時斷開。' });
  def('DPDT', { name: 'DPDT Bi-directional Switch', zh: '雙刀雙擲雙向開關', pre: 'S', pins: [PP(-2, -2, 'COMa'), PP(2, -3, 'a1'), PP(2, -1, 'a2'), PP(-2, 2, 'COMb'), PP(2, 1, 'b1'), PP(2, 3, 'b2'), GT(0, 4)], bb: [-2, -3.5, 2, 4],
    sym: () => ({ g: pth([-2, 2].map(y => ln(-2, y, -0.8, y) + ln(0.8, y - 1, 2, y - 1) + ln(0.8, y + 1, 2, y + 1) + blade(-0.8, y, 0.75, y + 0.75)).join('')) + pth(ln(0, 4, 0, -1.6), 'dash') + [-2, 2].map(y => cir(-0.8, y, 0.12, 'f') + cir(0.8, y - 1, 0.12, 'f') + cir(0.8, y + 1, 0.12, 'f')).join(''),
      t: [T(1.35, -3.25, '1', 0.5), T(1.35, -1.25, '2', 0.5), T(1.35, 0.75, '1', 0.5), T(1.35, 2.75, '2', 0.5)] }),
    params: [prm('ron', 'On Resistance 導通電阻', '10u', 'Ω', ''), INIT],
    desc: '雙刀雙擲（DPDT）雙向開關：兩組 SPDT 由同一閘極訊號控制。閘極為高時 COMa→a1、COMb→b1；為低時 COMa→a2、COMb→b2。常用於極性反轉或兩組電路同時切換。' });
  def('NPN3', { name: 'npn Transistor (3-state)', zh: 'NPN 電晶體（SPICE 模型）', pre: 'Q', show: ['model'], pins: [PP(0, -2, 'C'), PP(0, 2, 'E'), PP(-2, 0, 'B')], bb: [-2, -2, 0.6, 2],
    sym: () => ({ g: BJT_SYM(false) + cir(0.1, 0, 0.15, 'f') }),
    params: [prm('beta', 'Current Gain beta 電流增益', 100, '', 'β = Ic/Ib。'), prm('vr', 'Bias Voltage Vr 基射電壓', 0.7, 'V', '基極-射極順向偏壓。'), prm('vsat', 'Vce,sat 飽和電壓', 0.2, 'V', '飽和區 C-E 電壓。'), ROFFP, MODELP([['2N3904', '2N3904 / MMBT3904'], ['2N4401', '2N4401 / MMST4401 / MMBT4401'], ['2N2222A', '2N2222A / MMBT2222A'], ['custom', '自訂（下方 SPICE 參數）'], ['3state', '舊版三態分段線性（β、Vr、Vsat）']], '2N3904', 'NPN'), SPICEP('NPN')],
    desc: 'NPN 雙極電晶體，基極為功率節點。預設為 SPICE Gummel-Poon 模型：IS、BF、BR、NF、NR、VAF/VAR（Early 效應）、IKF/IKR（高注入）、ISE/NE、ISC/NC（低電流復合）、RB/RC/RE、接面電容 CJE/CJC 與渡越時間 TF/TR（儲存時間、切換延遲），以牛頓法求解，與 SPICE 結果一致。Model 選「舊版三態」時改用分段線性模型（截止 Vbe < Vr；線性 Ic = β·Ib；飽和 Vce = Vce,sat），速度快但忽略指數特性與電荷。' });
  def('PNP3', { name: 'pnp Transistor (3-state)', zh: 'PNP 電晶體（SPICE 模型）', pre: 'Q', show: ['model'], pins: [PP(0, -2, 'C'), PP(0, 2, 'E'), PP(-2, 0, 'B')], bb: [-2, -2, 0.6, 2],
    sym: () => ({ g: BJT_SYM(true) + cir(0.1, 0, 0.15, 'f') }),
    params: [prm('beta', 'Current Gain beta 電流增益', 100, '', ''), prm('vr', 'Bias Voltage Vr 射基電壓', 0.7, 'V', ''), prm('vsat', 'Vec,sat 飽和電壓', 0.2, 'V', ''), ROFFP, MODELP([['2N3906', '2N3906 / MMBT3906'], ['custom', '自訂（下方 SPICE 參數）'], ['3state', '舊版三態分段線性（β、Vr、Vsat）']], '2N3906', 'PNP'), SPICEP('PNP')],
    desc: 'PNP 雙極電晶體，預設為 SPICE Gummel-Poon 模型（參數同 NPN，極性相反，電流由 E 流向 C）。Model 選「舊版三態」時改用分段線性模型。' });
  def('MOS3', { name: 'MOSFET (3-state)', zh: 'N 通道 MOSFET（SPICE 模型）', pre: 'Q', show: ['model'], pins: [PP(0, -2, 'D'), PP(0, 2, 'S'), PP(-2, 0, 'G')], bb: [-2, -2, 1.2, 2],
    sym: () => ({ g: MOS_SYM(false) + cir(-1.4, 0, 0.15, 'f') }),
    params: [prm('vth', 'Threshold Voltage Vth 臨界電壓', 2, 'V', '閘源臨界電壓。'), prm('gm', 'Transconductance gm 轉導', 1, 'A/V', '主動區 Id = gm·(Vgs − Vth)。'), prm('ron', 'On Resistance 導通電阻', '10m', 'Ω', '歐姆區通道電阻。'), prm('vd', 'Diode Voltage Drop 二極體壓降', 0, 'V', '本體二極體壓降。'), MODELP([['GENERIC_N', '通用小訊號 N-MOSFET（Vth 2.1 V，近似 2N7002）'], ['custom', '自訂（下方 SPICE 參數）'], ['3state', '舊版三態分段線性（Vth、gm、Ron）']], 'GENERIC_N', 'NMOS'), SPICEP('NMOS')],
    desc: 'N 通道 MOSFET，閘極為功率節點。預設為 SPICE level-1（Shichman-Hodges）模型：VTO、KP（含 W/L）、LAMBDA（通道長度調變）、RD/RS，閘極電容 CGS/CGD 與 CDS（定值），本體二極體 IS/N/RB/CJO/TT；臨界點以數 mV 寬度平滑處理以利收斂。通用型號參數為近似值，精確分析請貼上廠商模型中對應的參數。Model 選「舊版三態」時改用分段線性模型（Vgs < Vth 截止；Id = gm·(Vgs − Vth)；歐姆區 Ron）。' });
  def('PMOS3', { name: 'p-MOSFET (3-state)', zh: 'P 通道 MOSFET（SPICE 模型）', pre: 'Q', show: ['model'], pins: [PP(0, -2, 'D'), PP(0, 2, 'S'), PP(-2, 0, 'G')], bb: [-2, -2, 1.2, 2],
    sym: () => ({ g: MOS_SYM(true) + cir(-1.4, 0, 0.15, 'f') }),
    params: [prm('vth', 'Threshold Voltage |Vth| 臨界電壓', 2, 'V', 'Vsg 臨界電壓（正值）。'), prm('gm', 'Transconductance gm 轉導', 1, 'A/V', ''), prm('ron', 'On Resistance 導通電阻', '10m', 'Ω', ''), prm('vd', 'Diode Voltage Drop 二極體壓降', 0, 'V', ''), MODELP([['GENERIC_P', '通用小訊號 P-MOSFET（Vth −2 V）'], ['custom', '自訂（下方 SPICE 參數）'], ['3state', '舊版三態分段線性（Vth、gm、Ron）']], 'GENERIC_P', 'PMOS'), SPICEP('PMOS')],
    desc: 'P 通道 MOSFET，預設為 SPICE level-1 模型（VTO 為負值，其餘同 N 通道），含本體二極體。Model 選「舊版三態」時改用分段線性模型（Vsg > Vth 導通，Isd = gm·(Vsg − Vth)）。' });
  // bridges
  const boxPins = (o) => o;
  function bridgeSym(label, h, extra) { return pth(extra || '') + rect(-2.2, -h, 2.2, h) + `<path class="thin" d="${ln(-1.2, -h + 0.5, -1.2, h - 0.5)}"/>`; }
  def('BDIODE1', { name: '1-ph Diode Bridge', zh: '單相二極體橋', pre: 'BD', pins: [PP(-3, -1, 'A+'), PP(-3, 1, 'A-'), PP(3, -1, 'DC+'), PP(3, 1, 'DC-')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: pth(ln(-3, -1, -2.2, -1) + ln(-3, 1, -2.2, 1) + ln(2.2, -1, 3, -1) + ln(2.2, 1, 3, 1)) + rect(-2.2, -1.8, 2.2, 1.8) + diodeV(0, 0, 0.55, true), t: [T(-1.5, -0.7, 'A+', 0.6, 'start'), T(-1.5, 1.3, 'A−', 0.6, 'start'), T(1.5, -0.7, '+', 0.7, 'end'), T(1.5, 1.3, '−', 0.7, 'end')] }),
    params: [prm('vf', 'Diode Voltage Drop 二極體壓降', 0, 'V', '每顆二極體的順向壓降。')],
    desc: '單相全橋二極體整流模組：D1 (A+→DC+)、D3 (A−→DC+)、D4 (DC−→A+)、D2 (DC−→A−)。' });
  def('BDIODE3', { name: '3-ph Diode Bridge', zh: '三相二極體橋', pre: 'BD', pins: [PP(-3, -1, 'A'), PP(-3, 0, 'B'), PP(-3, 1, 'C'), PP(3, -1, 'DC+'), PP(3, 1, 'DC-')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: pth(ln(-3, -1, -2.2, -1) + ln(-3, 0, -2.2, 0) + ln(-3, 1, -2.2, 1) + ln(2.2, -1, 3, -1) + ln(2.2, 1, 3, 1)) + rect(-2.2, -1.8, 2.2, 1.8) + diodeV(0, 0, 0.55, true), t: [T(-1.6, -0.75, 'A', 0.6, 'start'), T(-1.6, 0.25, 'B', 0.6, 'start'), T(-1.6, 1.25, 'C', 0.6, 'start'), T(1.5, -0.7, '+', 0.7, 'end'), T(1.5, 1.3, '−', 0.7, 'end')] }),
    params: [prm('vf', 'Diode Voltage Drop 二極體壓降', 0, 'V', '')],
    desc: '三相全橋二極體整流模組（6 顆二極體），上臂 D1/D3/D5 接 A/B/C 至 DC+，下臂 D4/D6/D2 由 DC− 接至 A/B/C。' });
  const thyBridgeDesc = '閘極 Ct 只需提供開關 1 的觸發訊號，其他開關的觸發訊號依相位差自動推算，因此 Ct 必須接到 Gating Block 或 Alpha Controller（用以取得工作頻率）。';
  def('BTHY1', { name: '1-ph Thyristor Bridge', zh: '單相閘流體橋', pre: 'BT', pins: [PP(-3, -1, 'A+'), PP(-3, 1, 'A-'), PP(3, -1, 'DC+'), PP(3, 1, 'DC-'), GT(0, 3, 'Ct')], bb: [-3, -2, 3, 3],
    sym: () => ({ g: pth(ln(-3, -1, -2.2, -1) + ln(-3, 1, -2.2, 1) + ln(2.2, -1, 3, -1) + ln(2.2, 1, 3, 1) + ln(0, 1.8, 0, 3)) + rect(-2.2, -1.8, 2.2, 1.8) + diodeV(0, -0.2, 0.55, true) + pth(ln(0.55, 0.35, 0.95, 0.8), 'thin'), t: [T(-1.5, -0.7, 'A+', 0.6, 'start'), T(-1.5, 1.3, 'A−', 0.6, 'start'), T(1.5, -0.7, '+', 0.7, 'end'), T(1.5, 1.3, '−', 0.7, 'end')] }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '')],
    desc: '單相全橋閘流體模組。開關 1、2 同時觸發，開關 3、4 延遲 180°。' + thyBridgeDesc });
  def('BTHY3', { name: '3-ph Thyristor Bridge', zh: '三相閘流體橋', pre: 'BT', pins: [PP(-3, -1, 'A'), PP(-3, 0, 'B'), PP(-3, 1, 'C'), PP(3, -1, 'DC+'), PP(3, 1, 'DC-'), GT(0, 3, 'Ct')], bb: [-3, -2, 3, 3],
    sym: () => ({ g: pth(ln(-3, -1, -2.2, -1) + ln(-3, 0, -2.2, 0) + ln(-3, 1, -2.2, 1) + ln(2.2, -1, 3, -1) + ln(2.2, 1, 3, 1) + ln(0, 1.8, 0, 3)) + rect(-2.2, -1.8, 2.2, 1.8) + diodeV(0, -0.2, 0.55, true) + pth(ln(0.55, 0.35, 0.95, 0.8), 'thin'), t: [T(-1.6, -0.75, 'A', 0.6, 'start'), T(-1.6, 0.25, 'B', 0.6, 'start'), T(-1.6, 1.25, 'C', 0.6, 'start'), T(1.5, -0.7, '+', 0.7, 'end'), T(1.5, 1.3, '−', 0.7, 'end')] }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '')],
    desc: '三相全橋閘流體模組（6 脈波）。開關 1~6 依序相差 60°（1:A 上、2:C 下、3:B 上、4:A 下、5:C 上、6:B 下）。' + thyBridgeDesc });
  def('BTHY3H', { name: '3-ph Thyristor Half-Bridge', zh: '三相半波閘流體橋', pre: 'BT', pins: [PP(-3, -1, 'A'), PP(-3, 0, 'B'), PP(-3, 1, 'C'), PP(3, 0, 'N'), GT(0, 3, 'Ct')], bb: [-3, -2, 3, 3],
    sym: () => ({ g: pth(ln(-3, -1, -2.2, -1) + ln(-3, 0, -2.2, 0) + ln(-3, 1, -2.2, 1) + ln(2.2, 0, 3, 0) + ln(0, 1.8, 0, 3)) + rect(-2.2, -1.8, 2.2, 1.8) + diodeH(0, 0, 0.5) + pth(ln(0.5, 0.35, 0.85, 0.8), 'thin'), t: [T(-1.6, -0.75, 'A', 0.6, 'start'), T(-1.6, 0.25, 'B', 0.6, 'start'), T(-1.6, 1.25, 'C', 0.6, 'start')] }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '')],
    desc: '三相半波閘流體模組（3 脈波），三顆閘流體陽極分別接 A/B/C、陰極共接 N，相鄰開關觸發相差 120°。' + thyBridgeDesc });
  def('BTHY6H', { name: '6-ph Thyristor Half-Bridge', zh: '六相半波閘流體橋', pre: 'BT', pins: [PP(-3, -3, 'A1'), PP(-3, -2, 'A2'), PP(-3, -1, 'A3'), PP(-3, 0, 'A4'), PP(-3, 1, 'A5'), PP(-3, 2, 'A6'), PP(3, 0, 'N'), GT(0, 4, 'Ct')], bb: [-3, -3.6, 3, 4],
    sym: () => ({ g: pth([-3, -2, -1, 0, 1, 2].map(y => ln(-3, y, -2.2, y)).join('') + ln(2.2, 0, 3, 0) + ln(0, 2.8, 0, 4)) + rect(-2.2, -3.6, 2.2, 2.8) + diodeH(0, -0.4, 0.5), t: [1, 2, 3, 4, 5, 6].map(i => T(-1.6, i - 3.75, String(i), 0.55, 'start')) }),
    params: [prm('vf', 'Voltage Drop 導通壓降', 0, 'V', '')],
    desc: '六相半波閘流體模組，六顆閘流體陽極接 A1~A6、陰極共接 N，相鄰開關觸發相差 60°。' + thyBridgeDesc });

  /* ===================================================================
   * POWER — Non-ideal switches (gate driven from a control signal through Rg)
   * =================================================================== */
  cat('Elements/Power/Non-ideal Switches');
  // non-linear capacitances of the non-ideal switches: gate-drain charge (Miller plateau length) and the Vds dependence of Cgd / Cds
  const NI_CAPQ = (qgd, vq) => [prm('qgd', 'Qgd 閘-汲極電荷', qgd, 'C', '資料手冊的 Qgd（米勒電荷）：決定米勒平台長度 = Qgd / Ig。0 = Cgd 固定為 Crss（平台很短）。', { adv: 1 }),
    prm('vq', 'Vq Qgd 的測試電壓', vq, 'V', 'Qg 曲線的 Vds 測試條件。', { adv: 1 }),
    prm('vcap', 'Vcap 電容規格的測試電壓', 25, 'V', 'Ciss/Crss/Coss 的 Vds 測試條件。Cds 隨 Vds 以 1/√(1+Vds/1V) 變化（Coss 在低壓時增大）；0 = 固定值。', { adv: 1 })];
  const NI_GATE = (inv) => [
    prm('von', inv ? 'Gate drive on 導通驅動電壓 |Vgs|' : 'Gate drive high 驅動高準位 Vgs', inv ? 10 : 12, 'V', inv ? '閘極訊號為低時，驅動器輸出 Vgs = −|此值|。' : '閘極訊號為高時驅動器輸出的 Vgs。'),
    prm('voff', inv ? 'Gate drive off 關斷驅動電壓 |Vgs|' : 'Gate drive low 驅動低準位 Vgs', 0, 'V', inv ? '閘極訊號為高時的 |Vgs|（通常 0）。' : '閘極訊號為低時的 Vgs（可設負壓，例如 −5）。'),
    prm('rg', 'Rg 閘極迴路電阻', 5, 'Ω', '驅動器輸出電阻 + 外部閘極電阻 + 元件內部閘極電阻的總和，決定切換速度。')];
  const NI_BODY = (vf, iff, tt) => [prm('vfd', 'Diode Vf 二極體順向壓降', vf, 'V', '在下方測試電流時的順向壓降。'), prm('ifd', 'Vf 測試電流', iff, 'A', ''), prm('ttd', 'Diode TT 渡越時間（反向恢復）', tt, 's', 'SPICE 二極體 TT，儲存電荷 Q = TT·I；決定反向恢復電流與時間（trr 約為 TT 的 1~2 倍，依 di/dt 而定）。', { adv: 1 })];
  const R_G = (x0, x1) => zig(x0, x1, 0, 0.2);
  const niGate = (g) => g.replace('M-2 0L-0.9 0', 'M-2 0L-1.85 0' + R_G(-1.85, -1.15) + 'M-1.15 0L-0.9 0');
  def('MOSNI', { name: 'MOSFET (non-ideal)', zh: 'N 通道 MOSFET（非理想）', pre: 'Q', pins: [PP(0, -2, 'D'), PP(0, 2, 'S'), GT(-2, 0, 'G')], bb: [-2, -2, 1.2, 2], show: ['ron'],
    sym: () => ({ g: niGate(MOS_SYM(false)) }),
    params: [prm('ron', 'Rds(on) 導通電阻', '50m', 'Ω', '閘極驅動為高準位時的通道電阻（小 Vds）。'), prm('vth', 'Vgs(th) 臨界電壓', 3, 'V', ''),
      prm('ciss', 'Ciss 輸入電容', '1.5n', 'F', 'Cgs + Cgd（資料手冊值，Vds = Vcap）。'), prm('crss', 'Crss 回授電容', '30p', 'F', 'Cgd 在 Vds = Vcap 的值（高壓端）。'), prm('coss', 'Coss 輸出電容', '150p', 'F', 'Cds + Cgd 在 Vds = Vcap 的值。')]
      .concat(NI_GATE(false), NI_CAPQ('15n', 400), NI_BODY(0.85, 5, '50n'), [prm('lam', 'λ 通道長度調變', 0, '1/V', '飽和區輸出電導，0 表示忽略。', { adv: 1 })]),
    desc: '非理想 N 通道 MOSFET：閘極接控制訊號（0/1），內含閘極驅動器（高/低準位電壓經 Rg 輸出），通道為 SPICE level-1 平方律模型（Vth、由 Rds(on) 推算的 KP），含 Cgs、Cgd（米勒平台）、Cds 與本體二極體（指數 I-V、反向恢復）。可看到有限的切換時間、米勒平台、切換損失、電壓/電流重疊與二極體反向恢復。電容為定值（實際元件的 Crss、Coss 隨電壓變化很大，以資料手冊在工作電壓附近的值估算）。時間步長需遠小於 Rg·Ciss 才能解析切換暫態。' });
  def('PMOSNI', { name: 'p-MOSFET (non-ideal)', zh: 'P 通道 MOSFET（非理想）', pre: 'Q', pins: [PP(0, -2, 'D'), PP(0, 2, 'S'), GT(-2, 0, 'G')], bb: [-2, -2, 1.2, 2], show: ['ron'],
    sym: () => ({ g: niGate(MOS_SYM(true)) }),
    params: [prm('ron', 'Rds(on) 導通電阻', '80m', 'Ω', ''), prm('vth', '|Vgs(th)| 臨界電壓', 2.5, 'V', '以正值輸入。'),
      prm('ciss', 'Ciss 輸入電容', '1.5n', 'F', ''), prm('crss', 'Crss 回授電容', '40p', 'F', ''), prm('coss', 'Coss 輸出電容', '200p', 'F', '')]
      .concat(NI_GATE(true), NI_CAPQ('15n', 100), NI_BODY(0.85, 5, '50n'), [prm('lam', 'λ 通道長度調變', 0, '1/V', '', { adv: 1 })]),
    desc: '非理想 P 通道 MOSFET：與 P 通道 MOSFET 相同，閘極訊號為低時導通（驅動器輸出 Vgs = −|驅動電壓|）。模型同非理想 N 通道 MOSFET（極性相反），本體二極體由 D 指向 S。' });
  def('IGBTNI', { name: 'IGBT (non-ideal)', zh: 'IGBT（非理想）', pre: 'Q', pins: [PP(0, -2, 'C'), PP(0, 2, 'E'), GT(-2, 0, 'G')], bb: [-2, -2, 1.3, 2], show: ['vce'],
    sym: () => ({ g: niGate(IGBT_SYM()) }),
    params: [prm('vth', 'Vge(th) 臨界電壓', 5.5, 'V', ''), prm('vce', 'Vce(sat) 飽和壓降', 1.8, 'V', '在下方測試電流、閘極驅動為高準位時的 Vce。'), prm('icn', 'Vce(sat) 測試電流', 20, 'A', ''),
      prm('beta', 'PNP gain 內部 PNP 電流增益', 0.5, '', '內部寬基極 PNP 的 β，決定集極電流中雙極成分的比例（一般 0.3~1）。', { adv: 1 }),
      prm('ttail', 'Tail time constant 拖尾電流時間常數', '0.4u', 's', '關斷後拖尾電流的衰減時間常數（少數載子復合）。'),
      prm('cies', 'Cies 輸入電容', '2n', 'F', ''), prm('cres', 'Cres 回授電容', '60p', 'F', ''), prm('coes', 'Coes 輸出電容', '150p', 'F', ''),
      prm('von', 'Gate drive high 驅動高準位 Vge', 15, 'V', ''), prm('voff', 'Gate drive low 驅動低準位 Vge', 0, 'V', '可設負壓，例如 −8。'), prm('rg', 'Rg 閘極迴路電阻', 10, 'Ω', '')]
      .concat(NI_CAPQ('40n', 600), NI_BODY(1.5, 20, '100n')),
    desc: '非理想 IGBT（含反並聯二極體）：以 MOSFET 驅動寬基極 PNP 的巨集模型（IGBT 的物理結構），閘極接控制訊號，內含閘極驅動器與 Rg。可模擬 Vce(sat)（PNP 的 Veb + MOS 通道壓降）、米勒平台、有限切換時間、關斷拖尾電流與二極體反向恢復。參數由 Vce(sat)、Vth 等資料手冊值推算，溫度固定 27 °C；此巨集模型不含閂鎖效應與動態飽和，關斷損失主要來自拖尾，精度約為資料手冊等級。' });

  /* ===================================================================
   * POWER — Transformers
   * =================================================================== */
  cat('Elements/Power/Transformers');
  const xfSym = (dotsBig) => pth(ln(-3, -1, -1.4, -1) + ln(-3, 1, -1.4, 1) + ln(1.4, -1, 3, -1) + ln(1.4, 1, 3, 1)) +
    `<path d="M-1.4 -1 a0.25 0.25 0 0 1 0 0.5 a0.25 0.25 0 0 1 0 0.5 a0.25 0.25 0 0 1 0 0.5 a0.25 0.25 0 0 1 0 0.5"/>` +
    `<path d="M1.4 -1 a0.25 0.25 0 0 0 0 0.5 a0.25 0.25 0 0 0 0 0.5 a0.25 0.25 0 0 0 0 0.5 a0.25 0.25 0 0 0 0 0.5"/>` + pth(ln(-0.15, -1.1, -0.15, 1.1) + ln(0.15, -1.1, 0.15, 1.1), 'thin') + cir(-1.0, -1.2, dotsBig ? 0.22 : 0.16, 'f') + cir(1.0, -1.2, 0.14, 'f');
  def('TFI', { name: 'Ideal Transformer', zh: '理想變壓器', pre: 'TF', pins: [PP(-3, -1, 'p+'), PP(-3, 1, 'p-'), PP(3, -1, 's+'), PP(3, 1, 's-')], bb: [-3, -1.5, 3, 1.2], show: ['np', 'ns'],
    sym: () => ({ g: xfSym(true) }),
    params: [prm('np', 'Np (primary) 一次側匝數', 1, '', '一次側匝數（可用額定電壓代替）。'), prm('ns', 'Ns (secondary) 二次側匝數', 1, '', '二次側匝數。')],
    desc: '理想變壓器：無損耗、無漏磁、無激磁電流，vs/vp = Ns/Np、Np·ip + Ns·is = 0。大圓點為一次側極性端。' });
  const XF_P = [prm('rp', 'Rp (primary) 一次側電阻', 0, 'Ω', '一次側繞組電阻。'), prm('rs', 'Rs (secondary) 二次側電阻', 0, 'Ω', '二次側繞組電阻（歸算至一次側）。'),
    prm('lp', 'Lp (pri. leakage) 一次側漏感', 0, 'H', '一次側漏感。'), prm('ls', 'Ls (sec. leakage) 二次側漏感', 0, 'H', '二次側漏感（歸算至一次側）。'),
    prm('lm', 'Lm (magnetizing) 激磁電感', '1', 'H', '激磁電感（由一次側看入）；0 表示不考慮激磁。'), prm('np', 'Np (primary) 一次側匝數', 1, '', ''), prm('ns', 'Ns (secondary) 二次側匝數', 1, '', '')];
  def('TF1', { name: '1-ph Transformer', zh: '單相變壓器', pre: 'TF', pins: [PP(-3, -1, 'p+'), PP(-3, 1, 'p-'), PP(3, -1, 's+'), PP(3, 1, 's-')], bb: [-3, -1.5, 3, 1.2], show: ['np', 'ns'],
    sym: () => ({ g: xfSym(true) }), params: XF_P,
    desc: '單相雙繞組變壓器，模型為：一次側 Rp、Lp 串聯 → 激磁電感 Lm 並聯 → 理想變壓器 Np:Ns → 二次側 Rs、Ls。所有電阻、電感值皆歸算至一次側。' });
  def('TF3W', { name: '1-ph 3-winding Transformer', zh: '單相三繞組變壓器', pre: 'TF', pins: [PP(-3, -1, 'p+'), PP(-3, 1, 'p-'), PP(3, -2, 's+'), PP(3, -1, 's-'), PP(3, 1, 't+'), PP(3, 2, 't-')], bb: [-3, -2.5, 3, 2.3],
    sym: () => ({ g: pth(ln(-3, -1, -1.4, -1) + ln(-3, 1, -1.4, 1) + ln(1.4, -2, 3, -2) + ln(1.4, -1, 3, -1) + ln(1.4, 1, 3, 1) + ln(1.4, 2, 3, 2)) + `<path d="${coilV(-1.4, -1, 1, false)}"/><path d="${coilV(1.4, -2, -1, true)}"/><path d="${coilV(1.4, 1, 2, true)}"/>` + pth(ln(-0.15, -2.1, -0.15, 2.1) + ln(0.15, -2.1, 0.15, 2.1), 'thin') + cir(-1, -1.2, 0.22, 'f') + cir(1, -2.2, 0.14, 'f') + cir(1, 0.8, 0.14, 'f') }),
    params: [prm('rp', 'Rp (primary)', 0, 'Ω', ''), prm('rs', 'Rs (secondary)', 0, 'Ω', ''), prm('rt', 'Rt (tertiary)', 0, 'Ω', ''), prm('lp', 'Lp (pri. leakage)', 0, 'H', ''), prm('ls', 'Ls (sec. leakage)', 0, 'H', ''), prm('lt', 'Lt (ter. leakage)', 0, 'H', ''), prm('lm', 'Lm (magnetizing)', '1', 'H', ''), prm('np', 'Np', 1, '', ''), prm('ns', 'Ns', 1, '', ''), prm('nt', 'Nt', 1, '', '')],
    desc: '單相三繞組變壓器（1 個一次側、2 個二次側：s 與 t）。所有阻抗歸算至一次側。' });
  function coilV(x, y0, y1, right) { const n = 4, h = (y1 - y0) / n, r = h / 2; let d = `M${f(x)} ${f(y0)}`; for (let i = 0; i < n; i++) d += `a${f(r)} ${f(r)} 0 0 ${right ? 0 : 1} 0 ${f(h)}`; return d; }
  const xf3Sym = (delta) => {
    let d = ''; [-2, -1, 0, 1].forEach(y => { d += ln(-4, y, -2.4, y); }); (delta ? [-2, -1, 0] : [-2, -1, 0, 1]).forEach(y => { d += ln(2.4, y, 4, y); });
    return pth(d) + rect(-2.4, -2.6, 2.4, 1.6) + cir(-0.9, -0.5, 0.75) + (delta ? pth('M0.4 -0.05L1.6 -0.05L1.0 -1.1Z') : cir(0.9, -0.5, 0.75));
  };
  def('TF3YY', { name: '3-ph Y/Y Transformer', zh: '三相 Y/Y 變壓器', pre: 'TF', pins: [PP(-4, -2, 'A'), PP(-4, -1, 'B'), PP(-4, 0, 'C'), PP(-4, 1, 'N'), PP(4, -2, 'a'), PP(4, -1, 'b'), PP(4, 0, 'c'), PP(4, 1, 'n')], bb: [-4, -2.6, 4, 1.6],
    sym: () => ({ g: xf3Sym(false), t: [T(-3, -2.25, 'A', 0.5), T(-3, -1.25, 'B', 0.5), T(-3, -0.25, 'C', 0.5), T(-3, 0.75, 'N', 0.5), T(3, -2.25, 'a', 0.5), T(3, -1.25, 'b', 0.5), T(3, -0.25, 'c', 0.5), T(3, 0.75, 'n', 0.5)] }), params: XF_P,
    desc: '三相 Y/Y 變壓器，每相以單相變壓器模型表示（不含三柱鐵心的相間耦合）。N、n 為中性點。' });
  def('TF3YD', { name: '3-ph Y/Δ Transformer', zh: '三相 Y/Δ 變壓器', pre: 'TF', pins: [PP(-4, -2, 'A'), PP(-4, -1, 'B'), PP(-4, 0, 'C'), PP(-4, 1, 'N'), PP(4, -2, 'a'), PP(4, -1, 'b'), PP(4, 0, 'c')], bb: [-4, -2.6, 4, 1.6],
    sym: () => ({ g: xf3Sym(true), t: [T(-3, -2.25, 'A', 0.5), T(-3, -1.25, 'B', 0.5), T(-3, -0.25, 'C', 0.5), T(-3, 0.75, 'N', 0.5), T(3, -2.25, 'a', 0.5), T(3, -1.25, 'b', 0.5), T(3, -0.25, 'c', 0.5)] }), params: XF_P,
    desc: '三相 Y/Δ 變壓器，每相以單相變壓器模型表示。Δ 側 A 相繞組接在 a–b、B 相在 b–c、C 相在 c–a。' });

  /* ===================================================================
   * POWER — Other
   * =================================================================== */
  cat('Elements/Power/Other');
  def('MUT2', { name: 'Coupled Inductors (2)', zh: '耦合電感（2 繞組）', pre: 'MUT', pins: [PP(-2, -1, '1+'), PP(2, -1, '1-'), PP(-2, 1, '2+'), PP(2, 1, '2-')], bb: [-2, -1.8, 2, 1.4],
    sym: () => ({ g: pth(ln(-2, -1, -1.2, -1) + coil(-1.2, 1.2, -1) + ln(1.2, -1, 2, -1) + ln(-2, 1, -1.2, 1) + coil(-1.2, 1.2, 1) + ln(1.2, 1, 2, 1) + ln(-1.2, -0.1, 1.2, -0.1), 'thin') + pth(ln(-2, -1, -1.2, -1) + coil(-1.2, 1.2, -1) + ln(1.2, -1, 2, -1) + ln(-2, 1, -1.2, 1) + coil(-1.2, 1.2, 1) + ln(1.2, 1, 2, 1)) + dot(-1.6, -1.45) + dot(-1.6, 0.55) }),
    params: [prm('l11', 'L11 (self) 自感', '1m', 'H', '繞組 1 自感。'), prm('l12', 'L12 (mutual) 互感', '0.9m', 'H', '互感（L12 = L21）。'), prm('l22', 'L22 (self) 自感', '1m', 'H', '繞組 2 自感。'), prm('i1', 'i1 initial', 0, 'A', ''), prm('i2', 'i2 initial', 0, 'A', '')],
    desc: '兩繞組耦合電感：[v1; v2] = [L11 L12; L12 L22]·d[i1; i2]/dt。以支路電流為未知數，可處理耦合係數 k = 1 的情況。圓點為各繞組極性端。' });
  def('MUT3', { name: 'Coupled Inductors (3)', zh: '耦合電感（3 繞組）', pre: 'MUT', pins: [PP(-2, -2, '1+'), PP(2, -2, '1-'), PP(-2, 0, '2+'), PP(2, 0, '2-'), PP(-2, 2, '3+'), PP(2, 2, '3-')], bb: [-2, -2.8, 2, 2.4],
    sym: () => ({ g: pth([-2, 0, 2].map(y => ln(-2, y, -1.2, y) + coil(-1.2, 1.2, y) + ln(1.2, y, 2, y)).join('')) + dot(-1.6, -2.45) + dot(-1.6, -0.45) + dot(-1.6, 1.55) }),
    params: [prm('l11', 'L11', '1m', 'H', ''), prm('l12', 'L12', '0.9m', 'H', ''), prm('l13', 'L13', '0.9m', 'H', ''), prm('l22', 'L22', '1m', 'H', ''), prm('l23', 'L23', '0.9m', 'H', ''), prm('l33', 'L33', '1m', 'H', ''), prm('i1', 'i1 initial', 0, 'A', ''), prm('i2', 'i2 initial', 0, 'A', ''), prm('i3', 'i3 initial', 0, 'A', '')],
    desc: '三繞組耦合電感，電感矩陣對稱（Lij = Lji）。' });
  const OPAMP_P = [prm('vsp', 'Voltage Vs+ 正電源', 15, 'V', '輸出上限。'), prm('vsn', 'Voltage Vs- 負電源', -15, 'V', '輸出下限。'), prm('A', 'Open-loop Gain 開迴路增益', '1e5', '', 'PSIM 預設 100,000。', { adv: 1 }), prm('ro', 'Output Resistance 輸出電阻', 80, 'Ω', 'PSIM 預設 80 Ω。', { adv: 1 })];
  def('OPAMP', { name: 'Op. Amp.', zh: '運算放大器', pre: 'U', pins: [PP(-3, -1, 'V-'), PP(-3, 1, 'V+'), PP(3, 0, 'Vo')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: pth(ln(-3, -1, -1.8, -1) + ln(-3, 1, -1.8, 1) + ln(1.8, 0, 3, 0)) + pth('M-1.8 -2L1.8 0L-1.8 2Z'), t: [T(-1.25, -0.75, '−', 0.8), T(-1.25, 1.3, '+', 0.8)] }), params: OPAMP_P,
    desc: '理想運算放大器（功率電路模型）：Vo = A·(V+ − V−) 經 Ro 輸出，並箝位於 Vs− ~ Vs+。參考地為功率地。注意：運算放大器的反相輸入在上方，比較器則相反。' });
  def('OPAMP1', { name: 'Op. Amp. (with ground)', zh: '運算放大器（含地端）', pre: 'U', pins: [PP(-3, -1, 'V-'), PP(-3, 1, 'V+'), PP(3, 0, 'Vo'), PP(0, 2, 'gnd')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: pth(ln(-3, -1, -1.8, -1) + ln(-3, 1, -1.8, 1) + ln(1.8, 0, 3, 0) + ln(0, 1, 0, 2)) + pth('M-1.8 -2L1.8 0L-1.8 2Z'), t: [T(-1.25, -0.75, '−', 0.8), T(-1.25, 1.3, '+', 0.8)] }), params: OPAMP_P,
    desc: '與 Op. Amp. 相同，但模型的參考地接腳可接到任意節點（可浮接）。' });
  def('COMPOC', { name: 'Comparator (open collector)', zh: '比較器（開集極，LM393 類）', pre: 'U', lblRight: true, pins: [PP(-3, -1, 'IN+'), PP(-3, 1, 'IN-'), PP(3, 0, 'OUT'), PP(0, -2, 'V+'), PP(0, 2, 'GND')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: pth(ln(-3, -1, -1.8, -1) + ln(-3, 1, -1.8, 1) + ln(1.8, 0, 3, 0) + ln(0, -1, 0, -2) + ln(0, 1, 0, 2)) + pth('M-1.8 -2L1.8 0L-1.8 2Z') + pth('M-0.3 0.25L-0.05 0.5L-0.3 0.75L-0.55 0.5ZM-0.58 0.92L-0.02 0.92', 'thin'),
      t: [T(-1.25, -0.7, '+', 0.8), T(-1.25, 1.35, '−', 0.8)] }),
    params: [prm('tpd', 'Response time 響應時間', '1.3u', 's', '內部翻轉延遲（輸入跨越到輸出 50 %），LM393/AS393 資料手冊典型 1.3 µs。'),
      prm('isc', 'Output sink current 輸出吸入電流', '16m', 'A', '輸出電晶體最大吸入電流（短路電流），LM393 典型 16 mA。'),
      prm('vol', 'Vol 低準位輸出電壓', '0.15', 'V', '在下方測試電流時的輸出飽和電壓。'),
      prm('iol', 'Vol 測試電流', '4m', 'A', 'Vol 對應的吸入電流（LM393：4 mA 時 0.15 V 典型）。'),
      prm('vos', 'Input offset Vos 輸入偏移', '0', 'V', '加在 IN+ 端的偏移電壓，可用於誤差分析（LM393 最大 ±5 mV）。', { adv: 1 }),
      prm('ib', 'Input bias current 輸入偏壓電流', '25n', 'A', '由輸入腳流出的偏壓電流（PNP 輸入級）。', { adv: 1 }),
      prm('vcmr', 'Common-mode headroom 共模上限餘裕', '1.5', 'V', '輸入共模範圍上限 = V+ − 此值。任一輸入超出時，模擬結束後會列出警告與累計時間。', { adv: 1 }),
      prm('vsmin', 'Min supply 最低工作電壓', '2', 'V', '電源低於此值時輸出電晶體無法導通（輸出被外部上拉）。', { adv: 1 }),
      prm('iq', 'Supply current 靜態電流', '0.2m', 'A', '每個比較器由 V+ 流到 GND 的靜態電流（LM393 兩組合計 0.4 mA）。', { adv: 1 })],
    desc: '開集極比較器（LM393 / AS393 / LM339 / LM2903 這類）。IN+ > IN− 時輸出電晶體關閉（輸出由外部上拉電阻拉高）；IN+ < IN− 時輸出拉到 GND。模型已內含開集極輸出級，直接把上拉電阻接到 OUT 即可，不需要另接感測器或開關。V+、GND 為電源腳（例如 LM393 第 8、4 腳），每顆雙比較器的兩個通道各放一個元件。模型包含：輸入跨越時刻內插、響應時間、輸出飽和特性（Vol、吸入電流上限）、輸入偏壓電流、輸入對 GND 的寄生箝位二極體、低電源時輸出失效，以及輸入共模範圍監測。不支援線性頻域 AC 分析（請用小訊號注入法）。' });
  def('TL431', { name: 'TL431 Shunt Regulator', zh: 'TL431 可調分流穩壓器', pre: 'U', pins: [PP(0, -2, 'K'), PP(0, 2, 'A'), PP(-2, 0, 'REF')], bb: [-2, -2, 1, 2],
    sym: () => ({ g: pth(ln(0, -2, 0, -0.6) + ln(0, 2, 0, 0.6) + ln(-2, 0, -0.3, 0)) + pth('M-0.6 0.6L0.6 0.6L0 -0.6Z', 'f') + pth('M-0.85 -0.38L-0.6 -0.6L0.6 -0.6') }),
    params: [prm('vref', 'Vref 參考電壓', 2.495, 'V', 'REF = K、Ika = 10 mA 時的參考電壓（TL431 典型 2.495 V；TL431A ±1 %、B ±0.5 % 只影響容差）。'),
      prm('iref', 'Iref 參考端輸入電流', '2u', 'A', '流入 REF 腳的偏壓電流（TL431 典型 2 µA、最大 4 µA）。'),
      prm('imin', 'Imin 最小工作電流', '0.4m', 'A', '低於此電流時內部偏壓不足、轉導很低（約 0.03 A/V），Vref 會明顯下降（TL431 典型 0.4 mA、最大 1 mA）。'),
      prm('zka', '|Zka| 動態阻抗', 0.2, 'Ω', 'REF = K 時的動態阻抗，決定大電流區的轉導 gm = 1/Zka。'),
      prm('kvk', 'ΔVref/ΔVka 線性調整率', '-1.4m', 'V/V', '陰極電壓升高時 Vref 的變化（TL431：Vka 2.5~10 V 時 −1.4 mV/V）。'),
      prm('fp', 'Transconductance pole 轉導極點', '4.5k', 'Hz', '內部補償形成的主極點。預設值使資料手冊小訊號增益測試電路（Ika = 10 mA、負載 230 Ω）的 DC 增益約 53 dB、單位增益頻寬約 2 MHz。', { adv: 1 }),
      prm('zhf', 'High-freq. cathode impedance 高頻陰極阻抗', 10, 'Ω', '極點以上頻率的陰極阻抗上限（內部補償的前饋路徑）。', { adv: 1 })],
    desc: 'TL431 可調分流穩壓器（K 陰極、A 陽極、REF 參考端）。行為模型依資料手冊典型值建立：Vref、Iref、Imin（低電流區低轉導）、Zka（大電流區轉導）、ΔVref/ΔVka、單極點頻率響應（約 2 MHz 單位增益頻寬）、陰極電壓低於約 2 V 時輸出飽和、A→K 的寄生二極體。溫度固定 27 °C；不模擬雜訊、溫度漂移與 TL431 在某些電容負載下的振盪區域，需要時請以實際元件確認。常見接法：REF 接分壓電阻中點，K 經電阻（或光耦合器 LED）接到被調節的電壓。' });
  const ledV = (x, y0, y1) => pth(ln(x, y0, x, -0.45 + (y0 + y1) / 2) + ln(x, 0.45 + (y0 + y1) / 2, x, y1)) + diodeV(x, (y0 + y1) / 2, 0.45, false);
  const photoQ = (yc) => pth(ln(0.6, yc - 0.6, 0.6, yc + 0.6) + ln(0.6, yc - 0.25, 1.4, yc - 0.9) + ln(1.4, yc - 0.9, 1.4, yc - 1) + ln(1.4, yc - 1, 3, yc - 1) + ln(0.6, yc + 0.25, 1.4, yc + 0.9) + ln(1.4, yc + 0.9, 1.4, yc + 1) + ln(1.4, yc + 1, 3, yc + 1)) + pth(`M1.4 ${f(yc + 0.9)}L1 ${f(yc + 0.85)}L1.2 ${f(yc + 0.6)}Z`, 'f');
  const lightArr = (y0, y1) => pth(`M-0.55 ${f(y0)}L0.2 ${f(y1)}M-0.55 ${f(y0 + 0.4)}L0.2 ${f(y1 + 0.4)}`, 'thin') + pth(`M0.2 ${f(y1)}l-0.3 0.02l0.12 0.25zM0.2 ${f(y1 + 0.4)}l-0.3 0.02l0.12 0.25z`, 'f thin');
  const OPTO_LED = [prm('vf', 'LED Vf 順向壓降', 1.2, 'V', '輸入 LED 在下方測試電流時的順向壓降（PC817 類：1.2 V @ 20 mA）。'), prm('ifv', 'Vf 測試電流', '20m', 'A', '')];
  const OPTO_Q = (n) => [prm('bf', 'Phototransistor β 光電晶體增益', 400, '', '輸出光電晶體的電流增益；CTR 固定時影響基極等效阻抗與頻寬。', { adv: 1 }),
    prm('ccb', 'Ccb 集-基極電容', '5p', 'F', '光電晶體的集-基極接面電容（大面積光二極體），經米勒效應決定光耦合器的頻寬：負載電阻越大、極點頻率越低。', { adv: 1 }),
    prm('tf', 'TF 渡越時間', '5.5n', 's', '光電晶體的順向渡越時間，影響小負載電阻時的頻寬。', { adv: 1 }),
    prm('ifn', 'CTR 測試電流 IF', '5m', 'A', 'CTR 定義點的 LED 電流（PC817：IF = 5 mA、VCE = 5 V）。')];
  def('OPTO1', { name: 'Optocoupler (1 output)', zh: '光耦合器（一對一）', pre: 'PC', pins: [PP(-3, -1, 'A'), PP(-3, 1, 'K'), PP(3, -1, 'C'), PP(3, 1, 'E')], bb: [-3, -2, 3, 2], show: ['ctr'],
    sym: () => ({ g: pth(ln(-3, -1, -1.4, -1) + ln(-3, 1, -1.4, 1)) + ledV(-1.4, -1, 1) + lightArr(-0.45, -0.05) + photoQ(0) + rect(-2.4, -1.8, 2.4, 1.8, 'dash') }),
    params: [prm('ctr', 'CTR 電流轉換比', 100, '%', 'Ic / IF（在測試電流 IF、VCE = 5 V 時）。PC817 依等級 50~600 %。')].concat(OPTO_LED, OPTO_Q(1)),
    desc: '光耦合器（LED 輸入 + 光電晶體輸出，四腳型）。輸入 LED 為 SPICE 指數模型（Vf 依測試點推算）；輸出為 Gummel-Poon NPN 光電晶體，其基極電流 = 光電流 ∝ LED 電流，增益依 CTR 校正。飽和壓降、頻寬隨負載電阻改變（米勒效應）等特性自然呈現：預設參數在 RL = 100 Ω 時頻寬約數十 kHz，在 TL431 回授常見的 10~20 kΩ 上拉時極點約數 kHz。CTR 隨 LED 電流與溫度的變化未建模（固定在測試點）。' });
  def('OPTO2', { name: 'Optocoupler (2 outputs)', zh: '光耦合器（一對二）', pre: 'PC', pins: [PP(-3, -1, 'A'), PP(-3, 1, 'K'), PP(3, -3, 'C1'), PP(3, -1, 'E1'), PP(3, 1, 'C2'), PP(3, 3, 'E2')], bb: [-3, -3.6, 3, 3.6], show: ['ctr1', 'ctr2'],
    sym: () => ({ g: pth(ln(-3, -1, -1.4, -1) + ln(-3, 1, -1.4, 1)) + ledV(-1.4, -1, 1) + lightArr(-1.4, -1.9) + lightArr(0.6, 1.0) + photoQ(-2) + photoQ(2) + rect(-2.4, -3.4, 2.4, 3.4, 'dash') }),
    params: [prm('ctr1', 'CTR1 輸出 1 電流轉換比', 100, '%', ''), prm('ctr2', 'CTR2 輸出 2 電流轉換比', 100, '%', '')].concat(OPTO_LED, OPTO_Q(2)),
    desc: '一個 LED 同時照射兩個光電晶體的光耦合器（兩組輸出可分別接到不同電路，例如一次側回授與保護）。兩組輸出各自的 CTR 可分別設定，其餘模型同一對一光耦合器。' });
  def('DVDT', { name: 'dv/dt Block', zh: '微分方塊（功率）', pre: 'DV', pins: [PP(-2, 0, 'in'), PP(2, 0, 'out')], bb: [-2, -1, 2, 1],
    sym: () => ({ g: pth(ln(-2, 0, -1.2, 0) + ln(1.2, 0, 2, 0)) + rect(-1.2, -0.9, 1.2, 0.9), t: [T(0, 0.3, 'dv/dt', 0.6)] }),
    desc: '功率電路中的微分方塊，輸出為對地電壓 Vo = [Vin(t) − Vin(t−Δt)]/Δt，Δt 為該步長。' });

  /* ===================================================================
   * CONTROL
   * =================================================================== */
  // generic block: inputs on the left, outputs on the right
  function blk(label, opt) {
    opt = opt || {}; const w = opt.w || 1.3, h = opt.h || 0.9;
    let d = '';
    (opt.ins || [[-2, 0]]).forEach(([x, y]) => { if (x < -w) d += ln(x, y, -w, y); else if (y > h) d += ln(x, y, x, h); else if (y < -h) d += ln(x, y, x, -h); });
    (opt.outs || [[2, 0]]).forEach(([x, y]) => { if (x > w) d += ln(w, y, x, y); else if (y > h) d += ln(x, h, x, y); });
    const t = Array.isArray(label) ? label : [T(0, 0.3, label, opt.sz || 0.62)];
    return { g: pth(d) + rect(-w, -h, w, h) + (opt.extra || ''), t: t.concat(opt.t || []) };
  }
  const C1 = [CI(-2, 0, 'in'), CO(2, 0, 'out')];
  cat('Elements/Control');
  def('CONST', { name: 'Constant', zh: '常數', pre: 'K', pins: [CO(2, 0, 'out')], bb: [-1.4, -1, 2, 1],
    sym: (el) => { let v = String(el && el.params ? el.params.v : '1'); if (v.length > 6) v = v.slice(0, 5) + '…'; return blk(v, { ins: [], sz: v.length > 3 ? 0.5 : 0.62, extra: oArr(2, 0, 1, 0) }); },
    params: [prm('v', 'Value 數值', 1, '', '輸出的常數值，可使用參數檔中的名稱或運算式（例如 Vo_ref*0.5）。')],
    desc: '常數：輸出固定數值的控制訊號，作為控制迴路的參考值、增益或偏移量（例如電壓命令、PWM 比較的直流準位）。AC 小訊號分析時視為 0。' });
  def('P', { name: 'Proportional', zh: '比例控制器', pre: 'P', pins: C1, bb: [-2, -1, 2, 1], show: ['k'],
    sym: () => blk('K'), params: [prm('k', 'Gain 增益', 1, '', '比例增益 k。')],
    desc: '比例控制器：輸出 = k·輸入。' });
  def('I', { name: 'Integrator', zh: '積分器', pre: 'INT', pins: C1, bb: [-2, -1, 2, 1], show: ['T'],
    sym: () => blk('1/sT'), params: [prm('T', 'Time Constant 時間常數', '1m', 's', '積分時間常數 T，G(s) = 1/(sT)。'), prm('y0', 'Initial Output Value 初始輸出', 0, '', '')],
    desc: '積分器 G(s) = 1/(sT)，以梯形法離散：y(n) = y(n−1) + h/(2T)·[u(n) + u(n−1)]。建議在輸出加限制器避免過度飽和。' });
  def('RESETI', { name: 'Resettable Integrator', zh: '可重置積分器', pre: 'INT', pins: [CI(-2, 0, 'in'), CO(2, 0, 'out'), CI(0, 2, 'reset')], bb: [-2, -1, 2, 2],
    sym: () => blk('1/sT', { ins: [[-2, 0], [0, 2]] }),
    params: [prm('T', 'Time Constant 時間常數', '1m', 's', ''), prm('y0', 'Initial Output Value 初始輸出', 0, '', ''), choice('flag', 'Reset Flag 重置方式', 0, [['0', '0：邊緣重置（上升緣）'], ['1', '1：準位重置（高電位期間）']], '')],
    desc: '可由下方控制訊號重置的積分器。邊緣重置：控制訊號上升緣時輸出歸零；準位重置：控制訊號為高時輸出保持為零。' });
  def('DIFF', { name: 'Differentiator', zh: '微分器', pre: 'DIF', pins: C1, bb: [-2, -1, 2, 1], show: ['T'],
    sym: () => blk('sT'), params: [prm('T', 'Time Constant 時間常數', '1m', 's', 'G(s) = sT。')],
    desc: '微分器 G(s) = sT，計算為 y = T·[u(t) − u(t−Δt)]/Δt。輸入突變會產生尖峰，建議先經過低通濾波。' });
  def('PI', { name: 'PI Controller', zh: 'PI 控制器', pre: 'PI', pins: C1, bb: [-2, -1, 2, 1], show: ['k', 'T'],
    sym: () => blk('PI'), params: [prm('k', 'Gain 增益', 1, '', ''), prm('T', 'Time Constant 時間常數', '1m', 's', '')],
    desc: 'PI 控制器 G(s) = k·(1 + sT)/(sT)。建議在輸出加限制器避免積分飽和。' });
  const sumSym = (signs, ins) => {
    let d = ''; ins.forEach(([x, y]) => { if (x < -0.8) d += ln(x, y, -0.8, y); else if (y > 0.8) d += ln(x, y, x, 0.8); else if (y < -0.8) d += ln(x, y, x, -0.8); });
    d += ln(0.8, 0, 2, 0);
    const t = [T(0, 0.3, 'Σ', 0.8)]; signs.forEach(([x, y, s]) => t.push(T(x, y, s, 0.55)));
    return { g: pth(d) + cir(0, 0, 0.8), t };
  };
  def('SUM2', { name: 'Summer (2-input)', zh: '加法器（雙輸入）', pre: 'SUM', pins: [CI(-2, 0, 'in1'), CI(0, 2, 'in2'), CO(2, 0, 'out')], bb: [-2, -1, 2, 2],
    sym: () => sumSym([[-1.3, -0.25, '+'], [0.35, 1.5, '+']], [[-2, 0], [0, 2]]), params: [prm('k1', 'Gain_1 增益 1', 1, '', ''), prm('k2', 'Gain_2 增益 2', 1, '', '')],
    desc: '加法器：輸出 = k1·in1 + k2·in2。左側為輸入 1，下方為輸入 2。' });
  def('SUB2', { name: 'Summer (+/-)', zh: '減法器', pre: 'SUM', pins: [CI(-2, 0, 'in+'), CI(0, 2, 'in-'), CO(2, 0, 'out')], bb: [-2, -1, 2, 2],
    sym: () => sumSym([[-1.3, -0.25, '+'], [0.35, 1.5, '−']], [[-2, 0], [0, 2]]),
    desc: '減法器：輸出 = in+ − in−，常用於誤差計算（參考值 − 回授值）。' });
  def('SUM3', { name: 'Summer (3-input)', zh: '加法器（三輸入）', pre: 'SUM', pins: [CI(0, -2, 'in1'), CI(-2, 0, 'in2'), CI(0, 2, 'in3'), CO(2, 0, 'out')], bb: [-2, -2, 2, 2],
    sym: () => { const s = sumSym([[0.4, -1.2, '+'], [-1.3, -0.25, '+'], [0.4, 1.6, '+']], [[0, -2], [-2, 0], [0, 2]]); s.g += dot(-0.3, -1.3); return s; },
    params: [prm('k1', 'Gain_1', 1, '', ''), prm('k2', 'Gain_2', 1, '', ''), prm('k3', 'Gain_3', 1, '', '')],
    desc: '三輸入加法器：輸出 = k1·in1 + k2·in2 + k3·in3，有圓點者（上方）為輸入 1。' });
  def('COMP', { name: 'Comparator (logic)', zh: '邏輯比較器（控制電路）', pre: 'CMP', pins: [CI(-3, -1, 'in+'), CI(-3, 1, 'in-'), CO(3, 0, 'out')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: pth(ln(-3, -1, -1.8, -1) + ln(-3, 1, -1.8, 1) + ln(1.8, 0, 3, 0)) + pth('M-1.8 -2L1.8 0L-1.8 2Z'), t: [T(-1.25, -0.7, '+', 0.8), T(-1.25, 1.35, '−', 0.8)] }),
    desc: '控制電路用的理想邏輯比較器（PSIM Comparator）：正輸入大於負輸入時輸出 1，小於時輸出 0，相等時保持前值。實際的比較器 IC（LM393 等）請使用「比較器（開集極）」。輸出變化的瞬間會由兩輸入線性內插求出，搭配開關的切換瞬間內插可得到精確的 PWM 占空比。注意：比較器的正輸入在上方（與運算放大器相反）。' });
  def('LIM', { name: 'Limiter', zh: '限制器', pre: 'LIM', pins: C1, bb: [-2, -1, 2, 1], show: ['lo', 'hi'],
    sym: () => blk('', { extra: pth('M-0.9 0.55L-0.4 0.55L0.4 -0.55L0.9 -0.55', 'thin') }),
    params: [prm('lo', 'Lower Limit 下限', -1, '', ''), prm('hi', 'Upper Limit 上限', 1, '', '')],
    desc: '限制器：輸入超出上下限時輸出被箝位在上限或下限，否則輸出等於輸入。' });

  cat('Elements/Control/Filters');
  const filtSym = (path) => blk('', { extra: pth(path, 'thin') });
  def('LP1', { name: 'Low-pass Filter (1st order)', zh: '一階低通濾波器', pre: 'LPF', pins: C1, bb: [-2, -1, 2, 1], show: ['fc'], sym: () => filtSym('M-0.9 -0.3L0 -0.3Q0.5 -0.3 0.9 0.5'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('fc', 'Cut-off Frequency 截止頻率', '1k', 'Hz', '')], desc: '一階低通濾波器 G(s) = k·ωc/(s + ωc)，ωc = 2π·fc。' });
  def('HP1', { name: 'High-pass Filter (1st order)', zh: '一階高通濾波器', pre: 'HPF', pins: C1, bb: [-2, -1, 2, 1], show: ['fc'], sym: () => filtSym('M-0.9 0.5Q-0.5 -0.3 0 -0.3L0.9 -0.3'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('fc', 'Cut-off Frequency 截止頻率', '1k', 'Hz', '')], desc: '一階高通濾波器 G(s) = k·s/(s + ωc)。' });
  def('LP2', { name: 'Low-pass Filter (2nd order)', zh: '二階低通濾波器', pre: 'LPF', pins: C1, bb: [-2, -1, 2, 1], show: ['fc'], sym: () => filtSym('M-0.9 -0.3L-0.2 -0.3Q0.2 -0.5 0.3 0L0.6 0.5'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('z', 'Damping Ratio 阻尼比', 0.7071, '', 'ξ'), prm('fc', 'Cut-off Frequency 截止頻率', '1k', 'Hz', '')], desc: '二階低通濾波器 G(s) = k·ωc²/(s² + 2ξωc·s + ωc²)。' });
  def('HP2', { name: 'High-pass Filter (2nd order)', zh: '二階高通濾波器', pre: 'HPF', pins: C1, bb: [-2, -1, 2, 1], show: ['fc'], sym: () => filtSym('M-0.6 0.5L-0.3 0Q-0.2 -0.5 0.2 -0.3L0.9 -0.3'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('z', 'Damping Ratio 阻尼比', 0.7071, '', ''), prm('fc', 'Cut-off Frequency 截止頻率', '1k', 'Hz', '')], desc: '二階高通濾波器 G(s) = k·s²/(s² + 2ξωc·s + ωc²)。' });
  def('BP2', { name: 'Band-pass Filter', zh: '帶通濾波器', pre: 'BPF', pins: C1, bb: [-2, -1, 2, 1], show: ['fo'], sym: () => filtSym('M-0.9 0.5Q-0.3 0.4 -0.2 -0.4L0.2 -0.4Q0.3 0.4 0.9 0.5'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('fo', 'Center Frequency 中心頻率', '1k', 'Hz', ''), prm('fb', 'Passing Band 通帶寬度', 100, 'Hz', '')], desc: '二階帶通濾波器 G(s) = k·B·s/(s² + B·s + ωo²)，B = 2π·fb。' });
  def('BS2', { name: 'Band-stop Filter', zh: '帶拒濾波器', pre: 'BSF', pins: C1, bb: [-2, -1, 2, 1], show: ['fo'], sym: () => filtSym('M-0.9 -0.4Q-0.3 -0.4 -0.2 0.4L0.2 0.4Q0.3 -0.4 0.9 -0.4'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('fo', 'Center Frequency 中心頻率', '1k', 'Hz', ''), prm('fb', 'Stopping Band 拒帶寬度', 100, 'Hz', '')], desc: '二階帶拒濾波器 G(s) = k·(s² + ωo²)/(s² + B·s + ωo²)。' });

  cat('Elements/Control/Computational Blocks');
  const twoIn = [CI(-2, -1, 'in1'), CI(-2, 1, 'in2'), CO(2, 0, 'out')];
  def('MULT', { name: 'Multiplier', zh: '乘法器', pre: 'MUL', pins: [CI(-2, 0, 'in1'), CI(0, 2, 'in2'), CO(2, 0, 'out')], bb: [-2, -1, 2, 2],
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0, 2, 0, 0.8) + ln(0.8, 0, 2, 0) + ln(-0.45, -0.45, 0.45, 0.45) + ln(-0.45, 0.45, 0.45, -0.45)) + cir(0, 0, 0.8) }),
    desc: '乘法器：輸出 = in1 × in2。' });
  def('DIVD', { name: 'Divider', zh: '除法器', pre: 'DIV', pins: [CI(-2, -1, 'num'), CI(-2, 1, 'den'), CO(2, 0, 'out')], bb: [-2, -1.6, 2, 1.6],
    sym: () => { const b = blk('÷', { h: 1.4, ins: [[-2, -1], [-2, 1]], sz: 1 }); b.g += dot(-1.6, -1.35); return b; },
    desc: '除法器：輸出 = 被除數 / 除數。有圓點的上方輸入為被除數（分子）。除數為 0 時輸出以極大值表示。' });
  def('SQRT', { name: 'Square-root', zh: '平方根', pre: 'SQ', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('√x', { sz: 0.75 }), desc: '平方根：輸出 = √輸入（輸入小於 0 時以 0 計算）。' });
  const trig = (t, nm, zh, lab, d) => def(t, { name: nm, zh, pre: 'F', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk(lab), desc: d });
  trig('SIN', 'Sine', '正弦（度）', 'sin', '正弦函數，輸入單位為度：輸出 = sin(輸入·π/180)。');
  trig('SINR', 'Sine (in rad.)', '正弦（弧度）', 'sin r', '正弦函數，輸入單位為弧度。');
  trig('ASIN', 'Arcsine', '反正弦', 'sin⁻¹', '反正弦函數，輸出單位為度。');
  trig('COS', 'Cosine', '餘弦（度）', 'cos', '餘弦函數，輸入單位為度。');
  trig('COSR', 'Cosine (in rad.)', '餘弦（弧度）', 'cos r', '餘弦函數，輸入單位為弧度。');
  trig('ACOS', 'Arccosine', '反餘弦', 'cos⁻¹', '反餘弦函數，輸出單位為度。');
  trig('TAN', 'Tangent', '正切', 'tan', '正切函數，輸入單位為度。');
  def('ATAN', { name: 'Arctangent', zh: '反正切', pre: 'F', pins: [CI(-2, -1, 'real'), CI(-2, 1, 'imag'), CO(2, 0, 'out')], bb: [-2, -1.6, 2, 1.6],
    sym: () => { const b = blk('tg⁻¹', { h: 1.4, ins: [[-2, -1], [-2, 1]] }); b.g += dot(-1.6, -1.35); return b; },
    desc: '反正切：輸出 = tan⁻¹(虛部/實部)，單位為度。有圓點的上方輸入為實部、下方為虛部。' });
  def('ATAN2', { name: 'Arctangent 2', zh: '四象限反正切', pre: 'F', pins: [CI(-2, -1, 'y'), CI(-2, 1, 'x'), CO(2, 0, 'out')], bb: [-2, -1.6, 2, 1.6],
    sym: () => blk([T(0, 0.3, 'atan2', 0.55), T(-1.1, -0.75, 'y', 0.45, 'start'), T(-1.1, 1.25, 'x', 0.45, 'start')], { h: 1.4, ins: [[-2, -1], [-2, 1]] }),
    desc: '四象限反正切：輸出 = atan2(y, x)，單位為度（−180° ~ 180°）。上方為 y、下方為 x。' });
  def('EXP', { name: 'Exponential (a^x)', zh: '指數函數', pre: 'F', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('k₂ˣ'),
    params: [prm('k1', 'Coefficient k1 係數', 1, '', ''), prm('k2', 'Coefficient k2 底數', '2.718281828', '', '')], desc: '指數函數：輸出 = k1·k2^輸入。k2 = e 時即自然指數。' });
  def('POW', { name: 'Power (x^a)', zh: '冪次函數', pre: 'F', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('xᵏ²'),
    params: [prm('k1', 'Coefficient k1 係數', 1, '', ''), prm('k2', 'Coefficient k2 次方', 2, '', '')], desc: '冪次函數：輸出 = k1·輸入^k2。' });
  trig('LOG', 'LOG (base e)', '自然對數', 'ln', '自然對數（以 e 為底）。');
  trig('LOG10', 'LOG10 (base 10)', '常用對數', 'log₁₀', '常用對數（以 10 為底）。');
  def('RMS', { name: 'RMS', zh: '均方根', pre: 'RMS', pins: C1, bb: [-2, -1, 2, 1], show: ['fb'], sym: () => blk('rms'),
    params: [prm('fb', 'Base Frequency 基本頻率', 60, 'Hz', '計算區間 T = 1/fb。')], desc: '均方根值：Vrms = √[(1/T)∫v²dt]，T = 1/fb。輸出只在每個週期開始時更新（週期邊界會設為模擬斷點）。' });
  trig('ABS', 'Absolute Value', '絕對值', '|x|', '絕對值：輸出 = |輸入|。');
  trig('SIGN', 'Sign Block', '符號函數', 'sign', '符號函數：輸入為正輸出 1、負輸出 −1、0 時輸出 0。');
  def('MAXMIN', { name: 'Maximum/Minimum Block', zh: '最大/最小值', pre: 'F', pins: twoIn, bb: [-2, -1.6, 2, 1.6], sym: () => blk('max', { h: 1.4, ins: [[-2, -1], [-2, 1]] }),
    params: [choice('flag', 'Flag 功能', 0, [['0', '0：最大值'], ['1', '1：最小值']], '')], desc: '兩輸入取最大值（Flag=0）或最小值（Flag=1）。' });

  cat('Elements/Control/Other Function Blocks');
  def('SAMP', { name: 'Sample-and-hold', zh: '取樣保持', pre: 'SH', pins: [CI(-2, 0, 'in'), CI(0, 2, 'ctrl'), CO(2, 0, 'out')], bb: [-2, -1, 2, 2],
    sym: () => blk('', { ins: [[-2, 0], [0, 2]], extra: pth('M-0.9 0.4L-0.9 -0.2L-0.3 -0.2L-0.3 -0.6L0.3 -0.6L0.3 0L0.9 0', 'thin') }),
    desc: '取樣保持：控制訊號（下方）由低轉高時取樣輸入，並保持到下一次取樣。取樣瞬間由控制訊號交越 0.5 的時間內插求得，取樣值為該瞬間的內插輸入值。' });
  def('LKUP', { name: 'Lookup Table', zh: '查表', pre: 'LKUP', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('LKUP', { sz: 0.55 }),
    params: [prm('tab', 'Table (x1 y1 x2 y2 ...) 表格', '0 0 1 10 2 15 3 17', '', '輸入與輸出成對列出，輸入值必須遞增（取代 PSIM 的表格檔案）。', { type: 'list' })],
    desc: '一維查表：兩點之間線性內插，輸入小於第一點或大於最後一點時輸出箝位於端點值。' });
  def('LKUPTZ', { name: 'Lookup Table (trapezoid)', zh: '梯形波查表', pre: 'LKUP', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M-1 0L-0.6 -0.5L-0.2 -0.5L0.2 0L0.6 0.5L1 0.5', 'thin') }),
    params: [prm('th', 'Rising Angle theta 上升角', 30, 'deg', ''), prm('vpk', 'Peak Value 峰值', 1, '', '')],
    desc: '梯形波查表：輸入為角度（−360°~360°），輸出為具半波及四分之一波對稱的梯形波，上升角 θ、峰值 Vpk。' });
  def('LKUPSQ', { name: 'Lookup Table (square)', zh: '方波查表', pre: 'LKUP', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M-1 0L-0.6 0L-0.6 -0.5L-0.1 -0.5L-0.1 0.5L0.5 0.5L0.5 0L1 0', 'thin') }),
    params: [prm('th', 'Pulse Width (deg.) 脈寬', 120, 'deg', '半週期內的脈寬。')],
    desc: '方波查表：輸入為角度，輸出為 ±1 的方波，在每半週期中央寬 θ 度，具半波及四分之一波對稱。' });
  def('FFT', { name: 'FFT', zh: '快速傅立葉轉換', pre: 'FFT', pins: [CI(-2, 0, 'in'), CO(2, -1, 'amp'), CO(2, 1, 'phase')], bb: [-2, -1.6, 2, 1.6],
    sym: () => { const b = blk('FFT', { h: 1.4, outs: [[2, -1], [2, 1]] }); b.g += dot(1.6, -1.35); return b; },
    params: [prm('N', 'No. of Sampling Points 取樣點數', 64, '', '每個基本週期的取樣點數，須為 2 的次方（≤ 1024）。'), prm('fb', 'Fundamental Frequency 基本頻率', 60, 'Hz', '')],
    desc: '計算輸入的基本波成分：上方（圓點）輸出為峰值振幅，下方輸出為相位角（度），Vm·sin(ωt) 的相位為 0。取樣時刻精確對齊 k/(N·fb)。' });
  def('TFS', { name: 's-domain Transfer Function', zh: 's 域轉移函數', pre: 'TF', pins: C1, bb: [-2, -1, 2, 1],
    sym: () => blk('H(s)'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('num', 'Coeff. Bn...B0 分子係數', '0 1', '', '由最高次方排到常數項。', { type: 'list' }), prm('den', 'Coeff. An...A0 分母係數', '1e-3 1', '', '由最高次方排到常數項。', { type: 'list' })],
    desc: 'G(s) = k·(Bn·sⁿ + … + B0)/(An·sⁿ + … + A0)。轉成可觀測標準型狀態方程式並以梯形法離散，與功率電路的積分法一致。例：1.5·400e3/(s² + 1200s + 400e3) 輸入 k=1.5、分子 "0 0 400e3"、分母 "1 1200 400e3"。' });
  def('TFS1', { name: 's-domain Transfer Function (initial value)', zh: 's 域轉移函數（含初值）', pre: 'TF', pins: C1, bb: [-2, -1, 2, 1],
    sym: () => blk('H(s)'),
    params: [prm('k', 'Gain 增益', 1, '', ''), prm('num', 'Coeff. Bn...B0 分子係數', '0 1', '', '', { type: 'list' }), prm('den', 'Coeff. An...A0 分母係數', '1e-3 1', '', '', { type: 'list' }), prm('x0', 'Initial Values xn..x1 初始狀態', '0', '', '狀態變數初始值，由 xn 排到 x1。', { type: 'list' })],
    desc: '與 s 域轉移函數相同，但可設定狀態變數初始值（PSIM TFCTN1）。' });
  def('TDELAY', { name: 'Time Delay', zh: '時間延遲', pre: 'TD', pins: C1, bb: [-2, -1, 2, 1], show: ['Td'], sym: () => blk('', { extra: pth('M-0.9 0.4L-0.5 0.4L-0.5 -0.4L0 -0.4L0 0.4L0.4 0.4L0.4 -0.4L0.9 -0.4', 'thin') }),
    params: [prm('Td', 'Time Delay 延遲時間', '1m', 's', '')],
    desc: '連續時間延遲：將輸入波形延遲指定時間。輸入中的跳變（例如數位訊號邊緣）會被精確記錄並在延遲後的時刻準確重現（設為模擬斷點）。' });
  def('UTD', { name: 'Unit Time Delay', zh: '單位時間延遲', pre: 'TD', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('Δt'),
    desc: '延遲一個模擬時間步：輸出 = 上一個計算點的輸入。可用來打斷控制電路中的代數迴路。' });
  def('THD', { name: 'THD', zh: '總諧波失真', pre: 'THD', pins: [CI(-2, 0, 'in'), CO(2, -1, 'THD'), CO(2, 1, 'v1')], bb: [-2, -1.6, 2, 1.6],
    sym: () => { const b = blk('THD', { h: 1.4, outs: [[2, -1], [2, 1]] }); b.g += dot(1.6, -1.35); return b; },
    params: [prm('f', 'Fundamental Frequency 基本頻率', 60, 'Hz', ''), prm('fb', 'Passing Band 通帶寬度', 20, 'Hz', '擷取基本波用的帶通濾波器頻寬。收斂時間 ≈ 3/fb 秒；fb ≤ f/5 時穩態誤差 < 1%。')],
    desc: '總諧波失真 THD = √(Vrms² − V1²)/V1。以二階帶通濾波器擷取基本波 v1(t)（下方輸出），上方輸出為 THD。【啟動不準】帶通濾波器與一週期移動均方根需要時間收斂：輸出在訊號出現後約 3/fb 秒才可信（誤差 < 1%；fb = 20 Hz → 0.15 s，fb = 10 Hz → 0.33 s，fb = 5 Hz → 0.66 s），約 2.8/fb 秒內誤差 < 5%；在此之前的讀值沒有意義（已限幅於 1000%）。訊號幅度改變（負載步階、市電驟降）後同樣要再等 3/fb 秒。fb 越小越準但越慢：fb ≤ f/5 穩態誤差 < 1%，fb = f/2.5 約 −1%，fb ≈ f 約 −3%（諧波漏進基頻濾波器）。含開關漣波的電流請先經低通（例如 2 kHz 二階）再量，否則漣波全算進 THD。' });
  def('ROUND', { name: 'Roundoff', zh: '四捨五入', pre: 'RND', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('INT', { sz: 0.55 }),
    params: [prm('N', 'No. of Digits 小數位數', 0, '', '小數點後位數，可為負值。'), choice('flag', 'Truncation Flag 截斷', 0, [['0', '0：四捨五入'], ['1', '1：無條件捨去']], '')],
    desc: '捨入方塊：輸入先乘以 10^N，再四捨五入（或截斷），最後除以 10^N。例：34.5678、N=1、截斷 → 34.5。' });
  def('LIMDVDT', { name: 'dv/dt Limiter', zh: '變化率限制器', pre: 'LIM', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M-0.9 0.5L0.9 -0.5M-0.4 -0.3L0.4 0.3', 'thin') }),
    params: [prm('L', 'dv/dt Limit 變化率上限', '1000', '1/s', '')], desc: '限制輸入的變化率，若變化率在範圍內則輸出等於輸入。' });
  const muxSym = (n, k) => { let d = ''; const h = n / 2 + 0.3; for (let i = 0; i < n; i++) d += ln(-2, i - n / 2 + 0.5 - 0.5, -1, i - n / 2); for (let i = 0; i < k; i++) d += ln(-k / 2 + i + 0.5 - 0.5, h + 0.6, -k / 2 + i, h - 0.2); return pth(d) + pth(`M-1 ${-h}L1 ${-h + 0.6}L1 ${h - 0.6}L-1 ${h}Z`) + pth(ln(1, 0, 2, 0)); };
  function muxDef(t, n, k) {
    const pins = []; for (let i = 0; i < n; i++) pins.push(CI(-2, i - Math.floor(n / 2), 'd' + i));
    const h = Math.ceil(n / 2) + 1; for (let i = 0; i < k; i++) pins.push(CI(-(k - 1) + 2 * i - (k === 1 ? 0 : 0), h, 's' + (k - 1 - i)));
    pins.push(CO(2, 0, 'Y'));
    // s pins order: s0..s(k-1) for engine — re-order: engine expects s0 first
    const sp = pins.slice(n, n + k).reverse(); for (let i = 0; i < k; i++) pins[n + i] = sp[i];
    const top = -Math.floor(n / 2) - 0.6, bot = h - 0.8;
    def(t, { name: 'Multiplexer (' + n + '-input)', zh: n + ' 輸入多工器', pre: 'MUX', pins, bb: [-2, top - 0.2, 2, h],
      sym: () => {
        let d = ''; for (let i = 0; i < n; i++) { const y = i - Math.floor(n / 2); d += ln(-2, y, -1, y); }
        pins.slice(n, n + k).forEach(p => { d += ln(p.x, p.y, p.x, bot); });
        return { g: pth(d + ln(1, 0, 2, 0)) + pth(`M-1 ${f(top)}L1 ${f(top + 0.7)}L1 ${f(bot - 0.7)}L-1 ${f(bot)}Z`), t: [T(0, 0.3, 'MUX', 0.5)].concat(pins.slice(n, n + k).map(p => T(p.x + 0.35, bot - 0.2, p.n, 0.4, 'start'))) };
      },
      desc: n + ' 輸入多工器：依選擇訊號 ' + pins.slice(n, n + k).map(p => p.n).reverse().join('、') + '（> 0.5 為 1）輸出 d0~d' + (n - 1) + ' 其中之一，資料可為類比或數位訊號。' });
  }
  muxDef('MUX2', 2, 1); muxDef('MUX4', 4, 2); muxDef('MUX8', 8, 3);
  const dqPins = (ins, outs) => ins.map((n, i) => CI(-3, i - 1.5 < 0 ? i - 2 : i - 1, n)).concat(outs.map((n, i) => CO(3, i - 1, n)));
  def('ABC2DQO', { name: 'abc-dqo Transformation', zh: 'abc→dqo 轉換', pre: 'DQ', pins: [CI(-3, -2, 'a'), CI(-3, -1, 'b'), CI(-3, 0, 'c'), CI(-3, 1, 'θ'), CO(3, -1, 'd'), CO(3, 0, 'q'), CO(3, 1, 'o')], bb: [-3, -2.6, 3, 1.6],
    sym: () => ({ g: pth(ln(-3, -2, -1.8, -2) + ln(-3, -1, -1.8, -1) + ln(-3, 0, -1.8, 0) + ln(-3, 1, -1.8, 1) + ln(1.8, -1, 3, -1) + ln(1.8, 0, 3, 0) + ln(1.8, 1, 3, 1)) + rect(-1.8, -2.5, 1.8, 1.5), t: [T(0, -0.5, 'abc', 0.55), T(0, 0.5, '→dqo', 0.55), T(-1.6, -1.75, 'a', 0.4, 'start'), T(-1.6, -0.75, 'b', 0.4, 'start'), T(-1.6, 0.25, 'c', 0.4, 'start'), T(-1.6, 1.25, 'θ', 0.4, 'start')] }),
    desc: 'abc → dqo 轉換：vd = 2/3·[va·cosθ + vb·cos(θ−2π/3) + vc·cos(θ+2π/3)]，vq 以 sin 計算，vo = (va+vb+vc)/3。θ 單位為弧度。' });
  def('DQO2ABC', { name: 'dqo-abc Transformation', zh: 'dqo→abc 轉換', pre: 'DQ', pins: [CI(-3, -2, 'd'), CI(-3, -1, 'q'), CI(-3, 0, 'o'), CI(-3, 1, 'θ'), CO(3, -1, 'a'), CO(3, 0, 'b'), CO(3, 1, 'c')], bb: [-3, -2.6, 3, 1.6],
    sym: () => ({ g: pth(ln(-3, -2, -1.8, -2) + ln(-3, -1, -1.8, -1) + ln(-3, 0, -1.8, 0) + ln(-3, 1, -1.8, 1) + ln(1.8, -1, 3, -1) + ln(1.8, 0, 3, 0) + ln(1.8, 1, 3, 1)) + rect(-1.8, -2.5, 1.8, 1.5), t: [T(0, -0.5, 'dqo', 0.55), T(0, 0.5, '→abc', 0.55), T(-1.6, 1.25, 'θ', 0.4, 'start')] }),
    desc: 'dqo → abc 反轉換：va = vd·cosθ + vq·sinθ + vo，vb、vc 相角分別減、加 2π/3。θ 單位為弧度。' });

  cat('Elements/Control/Logic Elements');
  function gateSym(kind, n) {
    const ys = n === 3 ? [-1, 0, 1] : (n === 1 ? [0] : [-1, 1]); let d = ''; let body = '';
    const neg = kind === 'NAND' || kind === 'NOR' || kind === 'NOT';
    if (kind === 'NOT') { return { g: pth(ln(-2, 0, -0.8, 0) + ln(1.0, 0, 2, 0)) + pth('M-0.8 -0.7L0.7 0L-0.8 0.7Z') + cir(0.85, 0, 0.15) }; }
    if (kind === 'AND' || kind === 'NAND') { body = pth('M-1 -1.2L0 -1.2A1.2 1.2 0 0 1 0 1.2L-1 1.2Z'); ys.forEach(y => { d += ln(-3, y, -1, y); }); }
    else { body = pth('M-1.2 -1.2Q0.2 -1.2 1.2 0Q0.2 1.2 -1.2 1.2Q-0.6 0 -1.2 -1.2Z'); ys.forEach(y => { d += ln(-3, y, -0.95, y); }); if (kind === 'XOR') body += pth('M-1.55 -1.2Q-0.95 0 -1.55 1.2'); }
    d += ln(neg ? 1.5 : 1.2, 0, 3, 0);
    return { g: pth(d) + body + (neg ? cir(1.35, 0, 0.15) : '') };
  }
  [['AND', 'AND Gate', '及閘', 2, a => 'AND'], ['AND3', 'AND Gate (3-input)', '三輸入及閘', 3], ['OR', 'OR Gate', '或閘', 2], ['OR3', 'OR Gate (3-input)', '三輸入或閘', 3], ['XOR', 'XOR Gate', '互斥或閘', 2], ['NOT', 'NOT Gate', '反閘', 1], ['NAND', 'NAND Gate', '反及閘', 2], ['NOR', 'NOR Gate', '反或閘', 2]].forEach(([t, nm, zh, n]) => {
    const kind = t.replace('3', '');
    const ys = n === 3 ? [-1, 0, 1] : (n === 1 ? [0] : [-1, 1]);
    const pins = ys.map((y, i) => CI(n === 1 ? -2 : -3, y, 'in' + (i + 1))).concat([CO(n === 1 ? 2 : 3, 0, 'out')]);
    def(t, { name: nm, zh, pre: 'G', pins, bb: n === 1 ? [-2, -0.8, 2, 0.8] : [-3, -1.3, 3, 1.3], sym: () => gateSym(kind, n),
      desc: zh + '：輸入大於 0.5 視為邏輯 1，輸出為 0 或 1。輸出變化時會沿用造成變化之輸入的精確事件時間（例如比較器的交越時刻）。' });
  });
  function ffSym(ins, outs, title) {
    let d = ''; ins.forEach(([x, y]) => { d += x < -1.5 ? ln(x, y, -1.5, y) : ln(x, y, x, y < 0 ? -1.9 : 1.9); }); outs.forEach(([x, y]) => { d += ln(1.5, y, x, y); });
    return pth(d) + rect(-1.5, -1.9, 1.5, 1.9);
  }
  const lbl = (arr) => arr.map(([x, y, s]) => T(x, y, s, 0.5, x < 0 ? 'start' : (x > 0 ? 'end' : 'middle')));
  def('SRFF', { name: 'Set-Reset Flip-Flop', zh: 'SR 正反器', pre: 'FF', pins: [CI(-3, -1, 'S'), CI(-3, 1, 'R'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [-3, 1]], [[3, -1], [3, 1]]), t: lbl([[-1.3, -0.8, 'S'], [-1.3, 1.2, 'R'], [1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄']]) }),
    params: [choice('flag', 'Trigger Flag 觸發方式', 0, [['0', '0：邊緣觸發'], ['1', '1：準位觸發']], '')],
    desc: 'SR 正反器。邊緣觸發：S 上升緣輸出 Q=1，R 上升緣輸出 Q=0；準位觸發：S=1、R=0 時 Q=1，S=0、R=1 時 Q=0，皆為 0 時保持。S、R 同時有效為禁止狀態（保持）。' });
  def('JKFF', { name: 'J-K Flip-Flop', zh: 'JK 正反器', pre: 'FF', pins: [CI(-3, -1, 'J'), CI(-3, 0, 'CLK'), CI(-3, 1, 'K'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [-3, 0], [-3, 1]], [[3, -1], [3, 1]]) + pth('M-1.5 -0.3L-1.1 0L-1.5 0.3', 'thin'), t: lbl([[-1.3, -0.8, 'J'], [-1.3, 1.2, 'K'], [1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄']]) }),
    desc: 'JK 正反器，時脈上升緣觸發：J=K=0 保持、J=0/K=1 清除、J=1/K=0 設定、J=K=1 反轉。J、K 取時脈邊緣前一刻的值。' });
  def('JKFFSR', { name: 'J-K Flip-Flop with Set-Reset', zh: 'JK 正反器（含設定/重置）', pre: 'FF', pins: [CI(0, -3, 'S'), CI(-3, -1, 'J'), CI(-3, 0, 'CLK'), CI(-3, 1, 'K'), CI(0, 3, 'R'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -3, 3, 3],
    sym: () => ({ g: ffSym([[0, -3], [-3, -1], [-3, 0], [-3, 1], [0, 3]], [[3, -1], [3, 1]]) + pth('M-1.5 -0.3L-1.1 0L-1.5 0.3', 'thin'), t: lbl([[-1.3, -0.8, 'J'], [-1.3, 1.2, 'K'], [1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄'], [0, -1.3, 'S'], [0, 1.6, 'R']]) }),
    desc: '含非同步設定 (S) 與重置 (R) 的 JK 正反器：S=1 立即令 Q=1，R=1 立即令 Q=0（高電位有效），否則依時脈上升緣運作。' });
  def('DFF', { name: 'D Flip-Flop', zh: 'D 正反器', pre: 'FF', pins: [CI(-3, -1, 'D'), CI(-3, 1, 'CLK'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [-3, 1]], [[3, -1], [3, 1]]) + pth('M-1.5 0.7L-1.1 1L-1.5 1.3', 'thin'), t: lbl([[-1.3, -0.8, 'D'], [1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄']]) }),
    desc: 'D 正反器，時脈上升緣將 D 的值鎖入 Q。' });
  def('DFFSR', { name: 'D Flip-Flop with Set-Reset', zh: 'D 正反器（含設定/重置）', pre: 'FF', pins: [CI(0, -3, 'S'), CI(-3, -1, 'D'), CI(-3, 1, 'CLK'), CI(0, 3, 'R'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -3, 3, 3],
    sym: () => ({ g: ffSym([[0, -3], [-3, -1], [-3, 1], [0, 3]], [[3, -1], [3, 1]]) + pth('M-1.5 0.7L-1.1 1L-1.5 1.3', 'thin'), t: lbl([[-1.3, -0.8, 'D'], [1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄'], [0, -1.3, 'S'], [0, 1.6, 'R']]) }),
    desc: '含非同步設定/重置的 D 正反器。' });
  def('MONO', { name: 'Monostable', zh: '單擊多諧振盪器', pre: 'MONO', pins: [CI(-3, 0, 'in'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -2, 3, 2], show: ['W'],
    sym: () => ({ g: ffSym([[-3, 0]], [[3, -1], [3, 1]]) + pth('M-1.2 0.6L-0.8 0.6L-0.8 -0.4L0.2 -0.4L0.2 0.6L0.6 0.6', 'thin'), t: lbl([[1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄']]) }),
    params: [prm('W', 'Pulse Width 脈寬', '1m', 's', '輸出高電位的時間。')],
    desc: '單擊電路：輸入上升緣觸發，輸出產生指定寬度的脈波（脈波期間不再重新觸發）。脈波結束時刻精確設為模擬斷點。' });
  def('MONOC', { name: 'Controlled Monostable', zh: '可控單擊電路', pre: 'MONO', pins: [CI(-3, 0, 'in'), CI(0, 3, 'W'), CO(3, -1, 'Q'), CO(3, 1, 'Qn')], bb: [-3, -2, 3, 3],
    sym: () => ({ g: ffSym([[-3, 0], [0, 3]], [[3, -1], [3, 1]]) + pth('M-1.2 0.6L-0.8 0.6L-0.8 -0.4L0.2 -0.4L0.2 0.6L0.6 0.6', 'thin'), t: lbl([[1.3, -0.8, 'Q'], [1.3, 1.2, 'Q̄']]) }),
    desc: '可控單擊電路：脈寬（秒）由下方輸入在觸發瞬間決定。' });
  def('PWCT', { name: 'Pulse Width Counter', zh: '脈寬計數器', pre: 'PWC', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M-0.9 0.4L-0.5 0.4L-0.5 -0.4L0.3 -0.4L0.3 0.4L0.9 0.4M-0.5 -0.6L0.3 -0.6', 'thin') }),
    desc: '脈寬計數器：輸入上升緣開始計時，下降緣時輸出該脈波寬度（秒），並保持到下一個下降緣。邊緣時刻以內插精確求得。' });
  def('UPDN', { name: 'Up/Down Counter', zh: '上/下數計數器', pre: 'CNT', pins: [CI(-3, -1, 'U'), CI(-3, 0, 'D'), CI(-3, 1, 'R'), CO(3, 0, 'out')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [-3, 0], [-3, 1]], [[3, 0]]) + oArr(3, 0, 1, 0), t: lbl([[-1.3, -0.8, 'UP'], [-1.3, 0.2, 'DN'], [-1.3, 1.2, 'R']]).concat([T(0.4, -1.2, 'CNT', 0.45)]) }),
    params: [prm('bits', 'No. of Bits 位元數', 8, '', '計數範圍 0 ~ 2^N − 1，超出時環繞。')],
    desc: '上/下數計數器：UP 上升緣加 1、DN 上升緣減 1，R 為高時清除為 0。輸出為整數計數值。' });
  def('ADC', { name: 'A/D Converter', zh: 'A/D 轉換器', pre: 'ADC', pins: [CI(-3, -1, 'Vin'), CI(0, -3, 'Vref'), CI(-3, 1, 'CLK'), CO(3, 0, 'out')], bb: [-3, -3, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [0, -3], [-3, 1]], [[3, 0]]), t: lbl([[-1.3, -0.8, 'Vin'], [-1.3, 1.2, 'clk'], [0, -1.3, 'Vref']]).concat([T(0.5, 0.25, 'A/D', 0.5)]) }),
    params: [prm('bits', 'No. of Bits 位元數', 8, '', '8 或 10 位元（任意整數皆可）。')],
    desc: 'A/D 轉換器：時脈上升緣取樣，輸出 = floor(2^N/Vref·Vin)，限制在 0 ~ 2^N−1。例：Vref=5、Vin=3.2、N=8 → 163。' });
  def('DAC', { name: 'D/A Converter', zh: 'D/A 轉換器', pre: 'DAC', pins: [CI(-3, 0, 'Vin'), CI(0, -3, 'Vref'), CO(3, 0, 'out')], bb: [-3, -3, 3, 2],
    sym: () => ({ g: ffSym([[-3, 0], [0, -3]], [[3, 0]]), t: lbl([[-1.3, 0.2, 'Din'], [0, -1.3, 'Vref']]).concat([T(0.5, 0.25, 'D/A', 0.5)]) }),
    params: [prm('bits', 'No. of Bits 位元數', 8, '', '')],
    desc: 'D/A 轉換器：輸出 = Vref/2^N·Vin。例：Vref=5、Vin=163、N=8 → 3.1836。' });

  cat('Elements/Control/Digital Control Module');
  const FS = prm('fs', 'Sampling Frequency 取樣頻率', '10k', 'Hz', '取樣時刻為 k/fs，與模擬起點同步；取樣時刻會被設為模擬斷點以精確對齊。');
  def('ZOH', { name: 'Zero-Order Hold', zh: '零階保持', pre: 'ZOH', pins: C1, bb: [-2, -1, 2, 1], show: ['fs'], sym: () => blk('ZOH', { sz: 0.6 }), params: [FS],
    desc: '零階保持：在取樣時刻取樣輸入，並保持到下一個取樣時刻。' });
  def('UDELAY', { name: 'Unit Delay', zh: '單位延遲 1/z', pre: 'UD', pins: C1, bb: [-2, -1, 2, 1], show: ['fs'], sym: () => blk([T(0, -0.1, '1', 0.55), T(0, 0.65, 'z', 0.55)], { extra: pth(ln(-0.4, 0.05, 0.4, 0.05), 'thin') }), params: [FS],
    desc: '單位延遲：輸出 = 前一個取樣時刻的輸入（延遲一個取樣週期）。' });
  const ALG = choice('alg', 'Algorithm Flag 積分法', 0, [['0', '0：梯形法'], ['1', '1：後向尤拉'], ['2', '2：前向尤拉']], '');
  def('ID', { name: 'Integrator', zh: '離散積分器', pre: 'IZ', pins: C1, bb: [-2, -1, 2, 1], show: ['fs'], sym: () => blk('∫z', { sz: 0.7 }), params: [ALG, prm('y0', 'Initial Output Value 初始輸出', 0, '', ''), FS],
    desc: '離散積分器（T = 1/fs）：梯形法 y(n) = y(n−1) + T/2·[u(n)+u(n−1)]；後向尤拉 y(n) = y(n−1) + T·u(n)；前向尤拉 y(n) = y(n−1) + T·u(n−1)。' });
  def('IRSTD', { name: 'External Resettable Integrator', zh: '外部重置積分器', pre: 'IZ', pins: [CI(-2, 0, 'in'), CI(0, 2, 'reset'), CO(2, 0, 'out')], bb: [-2, -1, 2, 2], sym: () => blk('∫z', { ins: [[-2, 0], [0, 2]], sz: 0.7 }),
    params: [ALG, prm('y0', 'Initial Output Value 初始輸出', 0, '', ''), choice('flag', 'Reset Flag 重置方式', 0, [['0', '0：邊緣重置'], ['1', '1：準位重置']], ''), FS],
    desc: '可由外部訊號（下方）重置的離散積分器；邊緣重置於取樣時偵測到上升緣即歸零，準位重置在訊號為高時歸零。' });
  def('IRSTI', { name: 'Internal Resettable Integrator', zh: '內部重置積分器', pre: 'IZ', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk([T(0, 0.15, '∫z', 0.6), T(0.75, 0.75, 'M', 0.35)]),
    params: [ALG, prm('y0', 'Initial Output Value 初始輸出', 0, '', ''), prm('lo', 'Lower Limit 下限', 0, '', ''), prm('hi', 'Upper Limit 上限', 360, '', ''), FS],
    desc: '內部重置積分器：輸出超過上限時減去 (上限 − 下限)，低於下限時加上 (上限 − 下限)，形成環繞，常用於產生 0~360° 或 0~2π 的角度。' });
  def('DD', { name: 'Differentiator', zh: '離散微分器', pre: 'DZ', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('D(z)', { sz: 0.6 }), params: [FS],
    desc: '離散微分器 H(z) = (1/T)·(z − 1)/z，y(n) = [u(n) − u(n−1)]/T。' });
  const BCOEF = prm('b', 'Coeff. b0...bN 分子係數', '0.0201 0.0402 0.0201', '', '', { type: 'list' });
  const ACOEF = prm('a', 'Coeff. a0...aN 分母係數', '1 -1.561 0.6414', '', '', { type: 'list' });
  def('FIR', { name: 'FIR Filter', zh: 'FIR 濾波器', pre: 'FIR', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('FIR', { sz: 0.6 }), params: [prm('b', 'Coeff. b0...bN 係數', '0.25 0.25 0.25 0.25', '', '', { type: 'list' }), FS],
    desc: '有限脈衝響應濾波器 H(z) = b0 + b1·z⁻¹ + … + bN·z⁻ᴺ。' });
  def('FILTD', { name: 'Digital Filter', zh: '數位濾波器 (IIR)', pre: 'DF', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('H(z)'), params: [BCOEF, ACOEF, FS],
    desc: '一般數位濾波器 H(z) = (b0 + b1·z⁻¹ + … + bN·z⁻ᴺ)/(a0 + a1·z⁻¹ + … + aN·z⁻ᴺ)。例：fc=1kHz、fs=10kHz 二階 Butterworth：b = 0.0201 0.0402 0.0201、a = 1 −1.561 0.6414。' });
  def('TFZ', { name: 'z-domain Transfer Function', zh: 'z 域轉移函數', pre: 'TFZ', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('H(z)'), params: [BCOEF, ACOEF, FS],
    desc: 'z 域轉移函數 H(z) = (b0·zᴺ + … + bN)/(a0·zᴺ + … + aN)，等效差分方程式 y(n) = [Σbi·u(n−i) − Σai·y(n−i)]/a0。' });
  const QP = [prm('N', 'No. of Bits 位元數', 8, '', ''), prm('vimin', 'Vin_min', 0, '', ''), prm('vimax', 'Vin_max', 5, '', ''), prm('vomin', 'Vo_min', 0, '', ''), prm('vomax', 'Vo_max', 255, '', ''), FS];
  def('DIGIT', { name: 'Quantization Block', zh: '量化方塊', pre: 'Q', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M-0.9 0.5L-0.5 0.5L-0.5 0.15L-0.1 0.15L-0.1 -0.2L0.3 -0.2L0.3 -0.55L0.9 -0.55', 'thin') }), params: QP,
    desc: '量化方塊（模擬 A/D 量化誤差）：先將輸入由 [Vin_min, Vin_max] 線性映射到 [Vo_min, Vo_max]，再依解析度 ΔV = (Vo_max − Vo_min)/(2^N − 1) 向下截斷。' });
  def('DIGIT2', { name: 'Quantization Block (with offset)', zh: '量化方塊（含偏移）', pre: 'Q', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M-0.9 0.5L-0.5 0.5L-0.5 0.15L-0.1 0.15L-0.1 -0.2L0.3 -0.2L0.3 -0.55L0.9 -0.55M-0.9 0.25L0.9 -0.7', 'thin') }), params: QP,
    desc: '量化方塊（含 ½ LSB 偏移）：與量化方塊相同，但量化採四捨五入至最接近的階層，而非向下截斷。' });
  def('CBUF', { name: 'Circular Buffer', zh: '環形緩衝區', pre: 'CB', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('', { extra: pth('M0.5 -0.1A0.5 0.5 0 1 1 0 -0.5', 'thin') + pth('M0 -0.75L0.3 -0.5L0 -0.25Z', 'f thin') }), params: [prm('L', 'Buffer Length 長度', 4, '', ''), FS],
    desc: '環形緩衝區：每個取樣時刻依序寫入，到達尾端後從頭覆寫。輸出為向量，需搭配 Memory Read 讀取個別位置。' });
  def('ARRAY', { name: 'Array', zh: '資料陣列', pre: 'ARR', pins: [CO(2, 0, 'out')], bb: [-1.4, -1, 2, 1], sym: () => blk('[..]', { ins: [] }),
    params: [prm('L', 'Array Length 長度', 4, '', ''), prm('vals', 'Values 數值', '2 4 6 8', '', '', { type: 'list' })], desc: '一維資料陣列（向量常數）。' });
  def('CONV', { name: 'Convolution', zh: '摺積', pre: 'CONV', pins: [CI(-2, 0, 'A'), CI(0, 2, 'B'), CO(2, 0, 'out')], bb: [-2, -1, 2, 2],
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0, 2, 0, 0.8) + ln(0.8, 0, 2, 0) + ln(-0.45, -0.45, 0.45, 0.45) + ln(-0.45, 0.45, 0.45, -0.45)) + cir(0, 0, 0.8) + cir(0, 0, 0.55) }),
    desc: '兩向量的摺積，輸出向量長度 m+n−1。例：[1 2 3] ⊗ [4 5] = [4 13 22 15]。' });
  def('MEMREAD', { name: 'Memory Read', zh: '記憶體讀取', pre: 'MR', pins: C1, bb: [-2, -1, 2, 1], sym: () => blk('[ ]'), params: [prm('off', 'Memory Index Offset 位址偏移', 0, '', '由起始位置算起的偏移量。')],
    desc: '讀取向量（陣列、環形緩衝區、摺積輸出）中指定位置的值。例：A = [2 4 6 8]，偏移 2 → 輸出 6。' });
  def('STACK', { name: 'Stack', zh: '堆疊', pre: 'STK', pins: [CI(-3, -1, 'Vin'), CI(-3, 0, 'push'), CI(-3, 1, 'pop'), CO(3, 0, 'out')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [-3, 0], [-3, 1]], [[3, 0]]) + oArr(3, 0, 1, 0), t: lbl([[-1.3, -0.8, 'Vin'], [-1.3, 0.2, 'push'], [-1.3, 1.2, 'pop']]) }), params: [prm('depth', 'Stack Depth 深度', 8, '', '')],
    desc: '先進後出暫存器：push 上升緣存入、pop 上升緣取出；堆疊滿時最底層資料被擠出，空堆疊 pop 時輸出不變。' });

  /* ===================================================================
   * OTHER
   * =================================================================== */
  cat('Elements/Other');
  def('SIMCTRL', { name: 'Simulation Control', zh: '模擬控制', pre: 'SIM', pins: [], bb: [-1.2, -1.2, 1.2, 1.2], single: true, nosim: true, noname: true, show: ['dt', 'tt'],
    sym: () => ({ g: cir(0, 0, 1.1) + pth('M0 0L0 -0.8M0 0L0.55 0.3') + pth(ln(-1.2, 1.2, 1.2, 1.2) + ln(-1.2, -1.2, -1.2, 1.2) + ln(1.2, -1.2, 1.2, 1.2) + ln(-1.2, -1.2, 1.2, -1.2), 'thin') }),
    params: [prm('dt', 'Time step 時間步長', '1E-005', 's', '固定模擬步長（最小可到 1 ps）。'), prm('tt', 'Total time 總時間', '0.1', 's', ''), prm('tp', 'Print time 輸出起始時間', '0', 's', '此時間之前不儲存結果。'), prm('ps', 'Print step 輸出間隔', '1', '', '每 N 個時間步儲存一點。'),
      choice('lte', 'Auto step refinement 自動細分步長', 1, [['1', '1：開啟（估計每步誤差，必要時細分至 1/16）'], ['0', '0：關閉（固定步長）']], '以最近四點估計梯形法的局部截斷誤差；超過容許值時把該步重算成兩個半步（遞迴，最多 16 倍）。切換瞬間附近與訊號源轉折點不估計。'),
      prm('ltetol', 'Refinement tolerance 細分容許誤差', '1e-3', '', '相對於各波形最大幅度的誤差容許值；越小越準、越慢。', { adv: 1 }),
      choice('interp', 'Switching interpolation 切換瞬間內插', 1, [['1', '1：開啟（建議）'], ['0', '0：關閉（PSIM 傳統固定步長）']], '', { adv: 1 }), choice('damp', 'Damping after switching 切換後阻尼', 1, [['1', '1：開啟（切換後以兩個 1/8 短步後向尤拉阻尼，其餘梯形法）'], ['0', '0：關閉（純梯形法）']], '')],
    desc: '模擬控制：設定暫態分析的時間步長、總時間、輸出起始時間與輸出間隔。每個電路只能有一個。積分法為梯形法；開關切換瞬間以割線法精確定位（誤差 < 步長的 1e-4），切換後用兩個 1/8 步長的後向尤拉短步抑制數值振盪，再視每步誤差估計自動細分步長。' });
  def('ACSWEEP', { name: 'AC Sweep', zh: 'AC 掃頻（波德圖）', pre: 'AC', pins: [], bb: [-2, -1.2, 2, 1.2], single: true, nosim: true, noname: true, show: ['f0', 'f1', 'src'],
    sym: () => ({ g: rect(-2, -1.2, 2, 1.2) + pth('M-1.6 0.6L-0.8 0.6Q-0.3 0.6 0 0Q0.4 -0.7 1.6 -0.7', 'thin'), t: [T(0, -0.3, 'AC SWEEP', 0.5)] }),
    params: [prm('f0', 'Start Frequency 起始頻率', '10', 'Hz', ''), prm('f1', 'End Frequency 結束頻率', '100k', 'Hz', '最高 1 MHz。'), prm('n', 'No. of Points 點數', '40', '', ''),
      choice('flag', 'Flag for Points 點分佈', 0, [['0', '0：對數均勻'], ['1', '1：線性均勻']], ''), prm('src', 'Source Name 激勵源名稱', 'Vac', '', '作為激勵的正弦波電壓/電流源名稱（Sine 源）。', { type: 'text' }),
      prm('a0', 'Start Amplitude 起始振幅', '0.1', '', '起始頻率時的激勵振幅。'), prm('a1', 'End Amplitude 結束振幅', '0.1', '', '結束頻率時的激勵振幅。'), prm('extra', 'Freq. for extra Points 額外頻率點', '', 'Hz', '額外加入的頻率點（以空白分隔）。', { type: 'list' }),
      choice('method', 'Method 分析方法', 0, [['0', '0：小訊號注入法（時域，可含開關）'], ['1', '1：線性頻域法（僅線性電路，快速）']], ''),
      prm('settle', 'Settling cycles 穩定週期數', 5, '', '注入後先經過的週期數才開始量測。'), prm('mcyc', 'Cycles per window 量測週期數', 4, '', '每個 DFT 量測窗的週期數。'), prm('maxc', 'Max. cycles 最大週期數', 60, '', '若在此週期數內未達穩態即提出警告。'), prm('tol', 'Steady-state tolerance 穩態容許誤差', '0.1', '%', '連續兩個量測窗的差異小於此值視為穩態。'),
      prm('tpre', 'Pre-run time 預跑時間', '-1', 's', '注入前先模擬到穩態的時間；−1 表示使用 Simulation Control 的總時間。'), prm('tset', 'Min. settling time 最小穩定時間', '0', 's', '注入後至少等待的時間。'), choice('base', 'Ripple cancellation 漣波抵銷', 1, [['1', '1：開啟（同步跑一份無激勵的基準模擬並相減）'], ['0', '0：關閉（速度加倍）']], '')],
    desc: 'AC 掃頻：設定好後按「執行模擬」即進行頻率響應分析並繪製波德圖。小訊號注入法會先將電路模擬到穩態，在每個頻率注入小振幅正弦波，以 DFT 擷取同頻率成分（含線性趨勢去除與漣波抵銷），可直接分析開關電路；線性頻域法直接解複數 MNA，只適用線性元件。需搭配 AC Sweep Probe 量測輸出。' });
  def('PARAMS', { name: 'Parameter File', zh: '參數檔', pre: 'PAR', pins: [], bb: [-2, -1, 2, 1], nosim: true, noname: true,
    sym: () => ({ g: rect(-2, -1, 2, 1) + pth(ln(-1.6, -0.3, 1.6, -0.3) + ln(-1.6, 0.3, 1.0, 0.3), 'thin'), t: [T(0, -0.45, '.PARAM', 0.45)] }),
    params: [prm('text', 'Parameters 參數定義', 'Vin = 100\nfsw = 50k\nL1 = 100u', '', '每行一個「名稱 = 數值或運算式」，% 之後為註解。其他元件的參數可直接引用這些名稱。', { type: 'code' })],
    desc: '參數定義：在此定義的名稱可用於任何元件的參數欄位（例如電阻值輸入 R1*0.5+(Vo+0.7)/Io）。支援 k、M、m、u、n、p 等字尾與數學函數。' });

  cat('Elements/Other/Switch Controllers');
  def('ONCTRL', { name: 'On-Off Controller', zh: '開關控制器', pre: 'ON', pins: [CI(-2, 0, 'in'), CO(2, 0, 'gate')], bb: [-2, -1, 2, 1],
    sym: () => blk('', { extra: pth('M-0.9 0.4L-0.4 0.4L-0.4 -0.4L0.4 -0.4L0.4 0.4L0.9 0.4', 'thin') + oArr(2, 0, 1, 0) }),
    desc: '開關控制器：將控制電路的邏輯訊號（0/1）傳遞給功率開關的閘極，是控制電路與功率開關之間的介面。比較器等產生的精確切換時刻會一併傳遞。' });
  def('ACTRL', { name: 'Alpha Controller', zh: '觸發角控制器', pre: 'ALPHA', pins: [CI(-3, -1, 'alpha'), CI(-3, 0, 'sync'), CI(-3, 1, 'enable'), CO(3, 0, 'gate')], bb: [-3, -2, 3, 2],
    sym: () => ({ g: ffSym([[-3, -1], [-3, 0], [-3, 1]], [[3, 0]]) + oArr(3, 0, 1, 0), t: lbl([[-1.3, -0.8, 'α'], [-1.3, 0.2, 'sync'], [-1.3, 1.2, 'en']]).concat([T(0.6, 0.25, 'α', 0.8)]) }),
    params: [prm('f', 'Frequency 頻率', 60, 'Hz', '被控開關的工作頻率。'), prm('pw', 'Pulse Width 脈寬', 90, 'deg', '觸發脈波寬度（度）。')],
    desc: '閘流體觸發角控制器：同步訊號由低轉高的瞬間代表 α = 0，延遲 α 度後輸出指定寬度的觸發脈波；enable 為低時停止輸出。α 輸入單位為度，可即時變化。脈波起訖時刻精確設為模擬斷點。' });
  def('GATING', { name: 'Gating Block', zh: '閘極脈波產生器', pre: 'G', pins: [CO(2, 0, 'gate')], bb: [-1.4, -1, 2, 1],
    sym: () => blk('', { ins: [], extra: pth('M-1 0.5L-0.6 0.5L-0.6 -0.5L-0.1 -0.5L-0.1 0.5L0.3 0.5L0.3 -0.5L0.6 -0.5L0.6 0.5L1 0.5', 'thin') + oArr(2, 0, 1, 0) }),
    params: [prm('f', 'Frequency 頻率', 2000, 'Hz', '頻率為 0 時切換點以秒為單位（不重複）。'), prm('n', 'No. of Points 切換點數', 2, '', '一個週期內的切換次數（導通與關斷各算一次）。'), prm('pts', 'Switching Points 切換點', '0 180', 'deg', '切換點（度），由低電位開始，每遇一點反轉一次。', { type: 'list' })],
    desc: '開關閘極脈波產生器：在一個週期內依指定角度切換輸出（初始為低）。例：f=2000、6 點、35 92 175 187 345 357。切換時刻精確設為模擬斷點。' });

  cat('Elements/Other/Sensors');
  def('VSEN', { name: 'Voltage Sensor', zh: '電壓感測器', pre: 'VSEN', pins: [PP(-2, -1, '+'), PP(-2, 1, '-'), CO(2, 0, 'out')], bb: [-2, -1.4, 2, 1.4], show: ['k'],
    sym: () => ({ g: pth(ln(-2, -1, -0.8, -1) + ln(-2, 1, -0.8, 1) + ln(0.9, 0, 2, 0)) + cir(0, 0, 0.9), t: [T(0, 0.3, 'V', 0.8), T(-1.35, -1.25, '+', 0.5), T(-1.35, 0.85, '−', 0.5)] }),
    params: [prm('k', 'Gain 增益', 1, '', '')], desc: '電壓感測器：量測功率電路兩點電壓並乘以增益送到控制電路（理想、無負載效應）。' });
  def('ISEN', { name: 'Current Sensor', zh: '電流感測器', pre: 'ISEN', pins: [PP(-2, 0, 'in'), PP(2, 0, 'out'), CO(0, -2, 'o')], bb: [-2, -2, 2, 0.9], show: ['k'],
    sym: () => ({ g: pth(ln(-2, 0, -0.9, 0) + ln(0.9, 0, 2, 0) + ln(0, -0.9, 0, -2)) + cir(0, 0, 0.9) + pth('M-0.5 0L0.4 0M0.15 -0.25L0.45 0L0.15 0.25', 'thin') }),
    params: [prm('k', 'Gain 增益', 1, '', '')], desc: '電流感測器：串接於功率支路中，量測由 in 流向 out 的電流並乘以增益送到控制電路。內部以 0 V 理想電流計實現（不引入電阻）。' });

  cat('Elements/Other/Probes');
  def('VP', { name: 'Voltage Probe', zh: '電壓探棒（對地）', pre: 'V', pins: [AN(0, 2, 'n')], bb: [-0.9, -0.8, 0.9, 2], probe: true,
    sym: () => ({ g: pth(ln(0, 2, 0, 0.8)) + pth('M-0.8 -0.7L0.8 -0.7L0.8 0.5L0 0.8L-0.8 0.5Z'), t: [T(0, 0.15, 'V', 0.65)] }),
    desc: '電壓探棒：量測節點對地電壓，可接在功率電路或控制電路。探棒名稱即為波形名稱。' });
  def('VP2', { name: 'Voltage Probe (2-terminal)', zh: '電壓探棒（兩點）', pre: 'V', pins: [AN(0, -2, '+'), AN(0, 2, '-')], bb: [-0.9, -2, 0.9, 2], probe: true,
    sym: () => ({ g: pth(ln(0, -2, 0, -0.8) + ln(0, 2, 0, 0.8)) + cir(0, 0, 0.8), t: [T(0, 0.28, 'V', 0.7), T(0.55, -1.05, '+', 0.5)] }),
    desc: '兩點電壓探棒：量測 + 端相對 − 端的電壓。' });
  def('IP', { name: 'Current Probe', zh: '電流探棒', pre: 'I', pins: [PP(-2, 0, 'in'), PP(2, 0, 'out')], bb: [-2, -0.9, 2, 0.9], probe: true,
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0.8, 0, 2, 0)) + cir(0, 0, 0.8) + pth('M-0.45 0L0.4 0M0.15 -0.25L0.45 0L0.15 0.25', 'thin') }),
    desc: '電流探棒：串接於功率支路中，量測由 in 流向 out 的電流。內部為 0 V 理想電流計，不影響電路。' });
  const meter = (t, nm, zh, lab, two, params, d) => def(t, { name: nm, zh, pre: lab[0] === 'V' ? 'V' : 'I', probe: true,
    pins: two ? [PP(0, -2, '+'), PP(0, 2, '-')] : [PP(-2, 0, 'in'), PP(2, 0, 'out')], bb: two ? [-0.9, -2, 0.9, 2] : [-2, -0.9, 2, 0.9],
    sym: () => ({ g: pth(two ? ln(0, -2, 0, -0.8) + ln(0, 2, 0, 0.8) : ln(-2, 0, -0.8, 0) + ln(0.8, 0, 2, 0)) + cir(0, 0, 0.8), t: [T(0, 0.25, lab, 0.55)] }), params, desc: d });
  meter('VDCM', 'DC Voltmeter', '直流電壓表', 'V=', true, [prm('fc', 'Cut-off Frequency 截止頻率', 10, 'Hz', '低通濾波器截止頻率，決定讀值的反應速度。')], '直流電壓表：以一階低通濾波器取電壓平均值；讀值需達穩態才有意義。');
  meter('VACM', 'AC Voltmeter', '交流電壓表', 'V~', true, [prm('f', 'Operating Frequency 工作頻率', 60, 'Hz', ''), prm('fc', 'Cut-off Frequency 截止頻率', 10, 'Hz', '濾除直流的高通濾波器截止頻率。')], '交流電壓表：先以高通濾波器去除直流成分，再計算一個工作週期的移動均方根值。');
  meter('ADCM', 'DC Ammeter', '直流電流表', 'A=', false, [prm('fc', 'Cut-off Frequency 截止頻率', 10, 'Hz', '')], '直流電流表：串接於支路中，以低通濾波器取電流平均值。');
  meter('AACM', 'AC Ammeter', '交流電流表', 'A~', false, [prm('f', 'Operating Frequency 工作頻率', 60, 'Hz', ''), prm('fc', 'Cut-off Frequency 截止頻率', 10, 'Hz', '')], '交流電流表：高通濾波去除直流後計算移動均方根值。');
  def('WATT', { name: 'Wattmeter', zh: '瓦特計', pre: 'P', probe: true, pins: [PP(-2, 0, 'I+'), PP(2, 0, 'I-'), PP(0, 2, 'V-')], bb: [-2, -0.9, 2, 2],
    sym: () => ({ g: pth(ln(-2, 0, -0.8, 0) + ln(0.8, 0, 2, 0) + ln(0, 0.8, 0, 2)) + cir(0, 0, 0.8), t: [T(0, 0.28, 'W', 0.7)] }),
    params: [prm('fc', 'Cut-off Frequency 截止頻率', 10, 'Hz', '')],
    desc: '瓦特計：電流由 I+ 流向 I−，電壓取 I+ 相對 V− 的電壓，讀值為 v·i 經低通濾波後的平均功率。' });
  def('DEVP', { name: 'Device Probe', zh: '元件內部探棒', pre: 'Q', probe: true, pins: [], bb: [-1.3, -0.9, 1.3, 0.9], show: ['el', 'q'],
    sym: () => ({ g: rect(-1.3, -0.9, 1.3, 0.9) + pth('M-0.9 0.5L-0.3 0.5L-0.3 -0.4L0.3 -0.4L0.3 0.5L0.9 0.5', 'thin'), t: [T(0, -0.45, 'DEV', 0.45)] }),
    params: [prm('el', 'Element 元件名稱', 'Q1', '', '要觀察的非理想開關名稱（MOSNI / PMOSNI / IGBTNI）。', { type: 'text' }),
      choice('q', 'Quantity 量', 'p', [['vg', 'vg：閘極電壓 Vgs / Vge（內部閘極節點，含 Rg 壓降）'], ['vd', 'vd：端電壓 Vds / Vce'], ['id', 'id：端電流 Id / Ic（流入汲極/集極，含本體二極體與 Coss 電流）'], ['ig', 'ig：驅動器輸出電流'], ['p', 'p：瞬時功率 v·i（導通＋切換損失）'], ['e', 'e：累積能量 ∫v·i dt（Eon / Eoff 用游標量差值）']], '')],
    desc: '元件內部探棒：不需接線，直接讀取指定非理想開關（MOSNI / PMOSNI / IGBTNI）的內部閘極電壓、端電壓、端電流、瞬時功率或累積能量。能量以梯形法在每個模擬步累積；在波形視窗用游標 C1、C2 夾住一次切換，讀 Δ 即為該次 Eon 或 Eoff。探棒名稱即為波形名稱。' });
  def('ACP', { name: 'AC Sweep Probe', zh: 'AC 掃頻探棒', pre: 'Vout', pins: [AN(0, 2, 'n')], bb: [-1, -0.8, 1, 2], acprobe: true,
    sym: () => ({ g: pth(ln(0, 2, 0, 0.8)) + pth('M-0.95 -0.7L0.95 -0.7L0.95 0.5L0 0.8L-0.95 0.5Z'), t: [T(0, 0.15, 'AC', 0.55)] }),
    desc: 'AC 掃頻輸出探棒：量測此節點（對地）相對於激勵源的頻率響應 H = V/激勵，結果顯示於波德圖。可接功率或控制節點。' });
  def('ACP2', { name: 'AC Sweep Probe (loop)', zh: 'AC 掃頻探棒（迴路）', pre: 'T', pins: [AN(-2, 0, 'dot'), AN(2, 0, 'x')], bb: [-2, -1, 2, 1], acprobe: true,
    sym: () => ({ g: pth(ln(-2, 0, -1.2, 0) + ln(1.2, 0, 2, 0)) + rect(-1.2, -0.75, 1.2, 0.75) + dot(-1.6, -0.4), t: [T(0, 0.22, 'loop', 0.5)] }),
    params: [choice('neg', 'Polarity 極性', 1, [['1', '1：T = −V(x)/V(•)（迴路增益，相位裕度 = 180° + ∠T）'], ['0', '0：H = V(x)/V(•)']], '')],
    desc: '迴路增益探棒：圓點端接在激勵源注入「之後」的節點，另一端接注入之前的節點，直接量測閉迴路系統的迴路增益，波德圖會標示穿越頻率與相位裕度。' });

  cat('Elements/Other/Function Blocks');
  def('CTOP', { name: 'Control-Power Interface', zh: '控制-功率介面', pre: 'CTOP', pins: [CI(-2, 0, 'in'), PP(2, 0, 'out')], bb: [-2, -1, 2, 1],
    sym: () => blk([T(0, 0.25, 'C→P', 0.5)]),
    desc: '將控制電路的數值傳到功率電路，輸出端視為對地的電壓源（求解功率電路時使用前一步的控制值）。' });
  function mathDef(t, n) {
    const pins = []; const top = -Math.floor((n - 1) / 2);
    for (let i = 0; i < n; i++) pins.push(CI(-2, top + i, 'x' + (i + 1)));
    pins.push(CO(2, 0, 'out'));
    const h = Math.max(1, (n - 1) / 2 + 0.6);
    def(t, { name: 'Math Function (' + n + '-input)', zh: n + ' 輸入數學函數', pre: 'FCN', pins, bb: [-2, -h, 2, h],
      sym: () => blk('f(x)', { h, ins: pins.slice(0, n).map(p => [p.x, p.y]), sz: 0.6 }),
      params: [prm('expr', 'Expression f(x1..xn) 運算式', n === 1 ? 'x*x' : (n === 2 ? 'x1*x2' : 'x1+x2+x3'), '', '可用變數：t（時間）、' + (n === 1 ? 'x、x1' : 'x1~x' + n) + '。支援 + − * / ^、sqrt sin cos tan asin acos atan atan2 exp log(ln) log10 abs sign min max pow、比較運算 > < >= <= == != 與 && ||（成立為 1）。', { type: 'expr' })],
      desc: n + ' 輸入數學函數方塊：輸出為輸入的任意數學運算式，可處理複雜的非線性關係。' });
  }
  [1, 2, 3, 5, 10].forEach(n => mathDef('MATH' + (n === 1 ? '1' : n), n));
  def('CBLOCK', { name: 'Simplified C Block', zh: '簡化 C 程式方塊', pre: 'CB', pins: null, bb: [-2, -1.6, 2, 1.6], dynPins: 1,
    sym: (el) => { const n = +((el && el.params && el.params.nin) || 1), m = +((el && el.params && el.params.nout) || 1); const h = Math.max(n, m) / 2 + 0.4; return blk('C', { h, ins: cbPins(n, m).filter(p => p.k === 'ci').map(p => [p.x, p.y]), outs: cbPins(n, m).filter(p => p.k === 'co').map(p => [p.x, p.y]), sz: 0.9 }); },
    params: [prm('nin', 'No. of Inputs 輸入數', 1, '', '0~10'), prm('nout', 'No. of Outputs 輸出數', 1, '', '1~10'),
      prm('glob', 'Variable definitions 全域變數', 'static double acc = 0;', '', '程式開始時執行一次；在此宣告的變數會在每一步之間保留（相當於 static）。', { type: 'code' }),
      prm('code', 'Step code 每步執行的程式', 'acc = acc + x1*delt;\ny1 = 2*x1 + 100*acc;', '', '每個計算點執行。輸入為 x1..xn、輸出為 y1..ym，t 為時間、delt 為步長。支援 C 的運算子、if/for/while、math.h 函數與一維陣列。', { type: 'code' })],
    desc: '簡化 C 程式方塊：以 C 語法撰寫控制演算法（會轉譯為 JavaScript 執行）。注意：整數除法依浮點數計算；需要取整請用 (int) 轉型或 floor()。程式在每個計算點執行，若需固定取樣率請自行以 t 判斷。' });
  function cbPins(n, m) { const out = []; const tn = -Math.floor((n - 1) / 2), tm = -Math.floor((m - 1) / 2); for (let i = 0; i < n; i++) out.push(CI(-2, tn + i, 'x' + (i + 1))); for (let i = 0; i < m; i++) out.push(CO(2, tm + i, 'y' + (i + 1))); return out; }
  D.CBLOCK.pinsFor = el => cbPins(+((el.params && el.params.nin) || 1), +((el.params && el.params.nout) || 1));

  /* ===================================================================
   * SOURCES
   * =================================================================== */
  cat('Elements/Sources/Voltage');
  const vsrc = (inner, isI) => ({ g: pth(ln(0, -2, 0, -0.9) + ln(0, 2, 0, 0.9)) + cir(0, 0, 0.9) + inner + (isI ? '' : '') });
  const SP = [AN(0, -2, '+'), AN(0, 2, '-')];
  const SPI = [PP(0, -2, '+'), PP(0, 2, '-')];
  const plusMinus = pth('M-0.25 -0.45L0.25 -0.45M0 -0.7L0 -0.2M-0.25 0.45L0.25 0.45', 'thin');
  const arrowUp = pth('M0 0.55L0 -0.5M-0.25 -0.2L0 -0.55L0.25 -0.2', 'thin');
  const W_ = {
    DC: '', SIN: pth('M-0.6 0Q-0.3 -0.6 0 0Q0.3 0.6 0.6 0', 'thin'), TRI: pth('M-0.6 0.3L-0.3 -0.3L0.15 0.3L0.6 -0.3', 'thin'), SAW: pth('M-0.6 0.3L0 -0.3L0 0.3L0.6 -0.3', 'thin'),
    SQU: pth('M-0.6 0.3L-0.6 -0.3L0 -0.3L0 0.3L0.6 0.3L0.6 -0.3', 'thin'), STEP: pth('M-0.6 0.3L0 0.3L0 -0.3L0.6 -0.3', 'thin'), STEP2: pth('M-0.6 0.3L-0.2 0.3L0.2 -0.3L0.6 -0.3', 'thin'),
    PWL: pth('M-0.6 0.3L-0.3 -0.2L0.1 0L0.6 -0.35', 'thin'), RAND: pth('M-0.6 0L-0.4 -0.3L-0.2 0.2L0 -0.4L0.2 0.3L0.4 -0.1L0.6 0.1', 'thin'), MATH: ''
  };
  const WP = {
    DC: [prm('V', 'Amplitude 振幅', 10, 'V', '')],
    SIN: [prm('Vm', 'Peak Amplitude 峰值', 10, 'V', ''), prm('f', 'Frequency 頻率', 60, 'Hz', ''), prm('ph', 'Phase Angle 相位', 0, 'deg', ''), prm('off', 'DC Offset 直流偏移', 0, 'V', ''), prm('ts', 'Tstart 起始時間', 0, 's', '此時間之前輸出為 0。')],
    TRI: [prm('Vpp', 'Vpeak-peak 峰對峰值', 1, 'V', ''), prm('f', 'Frequency 頻率', '10k', 'Hz', ''), prm('D', 'Duty Cycle 上升時間比', 0.5, '', '上升斜率區間占週期的比例。'), prm('off', 'DC Offset 直流偏移', 0, 'V', '波形最低點。'), prm('ph', 'Phase Delay 相位延遲', 0, 'deg', '正值表示往右平移。')],
    SAW: [prm('Vpk', 'V peak 峰值', 1, 'V', ''), prm('f', 'Frequency 頻率', '10k', 'Hz', ''), prm('off', 'DC Offset 直流偏移', 0, 'V', ''), prm('ph', 'Phase Delay 相位延遲', 0, 'deg', '')],
    SQU: [prm('Vpp', 'Vpeak-peak 峰對峰值', 1, 'V', ''), prm('f', 'Frequency 頻率', '10k', 'Hz', ''), prm('D', 'Duty Cycle 占空比', 0.5, '', '高電位區間占週期的比例。'), prm('off', 'DC Offset 直流偏移', 0, 'V', '低電位值。'), prm('ph', 'Phase Delay 相位延遲', 0, 'deg', '')],
    STEP: [prm('V', 'Vstep 步階值', 1, 'V', '步階後的值（之前為 0）。'), prm('ts', 'Tstep 步階時間', '1m', 's', '')],
    STEP2: [prm('V1', 'Vstep1 步階前', 0, 'V', ''), prm('V2', 'Vstep2 步階後', 1, 'V', ''), prm('ts', 'Tstep 步階時間', '1m', 's', ''), prm('tr', 'T_transition 轉換時間', 0, 's', '由 V1 線性變化到 V2 的時間。')],
    PWL: [prm('f', 'Frequency 頻率', 0, 'Hz', '0 表示不重複。'), prm('vals', 'Values V1...Vn 數值', '0 1 1 0', 'V', '', { type: 'list' }), prm('times', 'Times T1...Tn 時間', '0 1m 2m 3m', 's', '必須遞增；兩點時間相同表示跳變。', { type: 'list' })],
    PWL2: [prm('f', 'Frequency 頻率', 0, 'Hz', '0 表示不重複。'), prm('pairs', 'Times, Values (t1,v1)...', '(0,0) (1m,1) (2m,1) (3m,0)', '', '每組以括號包住 (時間, 數值)。', { type: 'text' })],
    RAND: [prm('Vpp', 'Peak-Peak Amplitude 峰對峰值', 1, 'V', ''), prm('off', 'DC Offset 直流偏移', 0, 'V', ''), prm('seed', 'Seed 亂數種子', 1, '', '相同種子可重現相同結果。')],
    MATH: [prm('expr', 'Expression 運算式', 'sin(2*pi*60*t)', '', '以 t 表示時間。', { type: 'expr' }), prm('ts', 'Tstart 起始時間', 0, 's', '')]
  };
  const WD = {
    DC: '直流源：固定振幅。', SIN: '正弦波源：v = Vm·sin(2π·f·t + θ) + Voffset，t < Tstart 時為 0。也可當作 AC 掃頻的激勵源（以元件名稱指定）。',
    TRI: '三角波源：由 Voffset 開始以上升時間比 D 上升到 Voffset + Vpp 再下降，常作為 PWM 載波。轉折點會被設為模擬斷點，與比較器交越內插搭配可得到精確 PWM。', SAW: '鋸齒波源：在一個週期內由 Voffset 線性上升到 Voffset + Vpeak 後瞬間歸零。',
    SQU: '方波源：週期開始時為高電位 Voffset + Vpp，持續 D·T 後變為低電位 Voffset。邊緣時刻精確設為模擬斷點。', STEP: '步階源：t < Tstep 時為 0，之後為 Vstep。', STEP2: '雙準位步階源：由 Vstep1 在 Tstep 時以 T_transition 線性轉換到 Vstep2。',
    PWL: '分段線性源：由 (Ti, Vi) 點連成的折線，可設定頻率使其週期性重複。', PWL2: '分段線性源（成對輸入）：以 (t, v) 成對的格式輸入。', RAND: '亂數源：每個時間步產生 Vpp·n + Voffset，n 為 0~1 的亂數。',
    MATH: '數學函數源：以 t 的運算式定義輸出，例如 sin(2*pi*60*t+2.09)。'
  };
  function srcDefs(isI) {
    const P_ = isI ? SPI : SP; const unit = isI ? 'A' : 'V';
    const fix = ps => ps.map(p => Object.assign({}, p, { u: p.u === 'V' ? unit : p.u, l: isI ? p.l.replace('Vpeak-peak', 'Ipeak-peak').replace('Vstep', 'Istep').replace('V peak', 'I peak') : p.l }));
    const pre = isI ? 'I' : 'V';
    const list = [['DC', 'DC', '直流'], ['SIN', 'Sine', '正弦'], ['TRI', 'Triangular', '三角波'], ['SQU', 'Square', '方波'], ['STEP', 'Step', '步階'], ['STEP2', 'Step (2-level)', '雙準位步階'], ['PWL', 'Piecewise Linear', '分段線性'], ['PWL2', 'Piecewise Linear (in pair)', '分段線性（成對）']];
    if (!isI) list.splice(3, 0, ['SAW', 'Sawtooth', '鋸齒波']);
    list.forEach(([w, nm, zh]) => {
      def(pre + w, { name: nm, zh: zh + (isI ? '電流源' : '電壓源'), pre, pins: P_, bb: [-1, -2, 1, 2], show: w === 'DC' ? ['V'] : (w === 'SIN' ? ['Vm', 'f'] : []), src: true,
        sym: () => vsrc((isI ? arrowUp : (w === 'DC' ? plusMinus : '')) + (isI && w === 'DC' ? '' : (isI ? `<g transform="translate(0.55 0) scale(0.45)">${W_[w]}</g>` : W_[w]))),
        params: fix(WP[w]),
        desc: WD[w] + (isI ? ' 電流方向：電流由 + 端流出到外部電路，再由 − 端流回。電流源只能用於功率電路。' : ' 可用於功率電路或控制電路；用於控制電路時負端必須接地。') });
    });
  }
  srcDefs(false);
  def('VBAT', { name: 'DC (battery)', zh: '直流電池', pre: 'V', pins: SP, bb: [-1, -2, 1, 2], show: ['V'], src: true,
    sym: () => ({ g: pth(ln(0, -2, 0, -0.5) + ln(0, 2, 0, 0.5) + ln(-0.8, -0.5, 0.8, -0.5) + ln(-0.45, -0.15, 0.45, -0.15) + ln(-0.8, 0.2, 0.8, 0.2) + ln(-0.45, 0.5, 0.45, 0.5)) }),
    params: WP.DC, desc: '直流電池（理想直流電壓源，符號不同）。' });
  def('VSIN3', { name: '3-ph Sine', zh: '三相正弦電壓源', pre: 'V', pins: [PP(-2, 0, 'n'), PP(2, -1, 'a'), PP(2, 0, 'b'), PP(2, 1, 'c')], bb: [-2, -1.6, 2, 1.6], show: ['V', 'f'], src: true,
    sym: () => ({ g: pth(ln(-2, 0, -1.2, 0) + ln(1.2, -1, 2, -1) + ln(1.2, 0, 2, 0) + ln(1.2, 1, 2, 1)) + rect(-1.2, -1.5, 1.2, 1.5) + W_.SIN + dot(1.6, -1.35), t: [T(0.75, -0.75, 'a', 0.4), T(0.75, 0.25, 'b', 0.4), T(0.75, 1.25, 'c', 0.4)] }),
    params: [prm('V', 'V (line-line-rms) 線電壓有效值', 380, 'V', ''), prm('f', 'Frequency 頻率', 60, 'Hz', ''), prm('ph', 'Init. Angle (phase A) A 相初始角', 0, 'deg', '')],
    desc: '對稱三相 Y 接正弦電壓源，n 為中性點，有圓點者為 A 相。相電壓峰值 = V·√2/√3，B、C 相分別落後、超前 120°。' });
  // controlled sources
  const diam = (inner, isI) => pth('M0 -0.95L0.95 0L0 0.95L-0.95 0Z') + (isI ? arrowUp : plusMinus) + (inner || '');
  const ctlSym = (isI, ins, extra) => {
    let d = ln(0, -2, 0, -0.95) + ln(0, 2, 0, 0.95);
    ins.forEach(([x, y]) => { d += ln(x, y, -0.95, y) ; });
    return { g: pth(d) + diam('', isI) + (extra || '') };
  };
  const CT = (isI) => [AN(0, -2, '+'), AN(0, 2, '-')];
  function ctlDefs(isI) {
    const pre = isI ? 'I' : 'V'; const o = isI ? '電流' : '電壓'; const P0 = isI ? [PP(0, -2, '+'), PP(0, 2, '-')] : [PP(0, -2, '+'), PP(0, 2, '-')];
    const add = (t, nm, zh, pins, params, d, ins, extra) => def(t, { name: nm, zh, pre, pins: P0.concat(pins), bb: [-2, -2, 1, 2], show: params.length ? ['k'] : [], src: true, sym: () => ctlSym(isI, ins, extra), params, desc: d });
    const K = [prm('k', 'Gain 增益', 1, '', '')];
    add(isI ? 'ICCCS' : 'VCCVS', 'Current-controlled', '電流控制' + o + '源',
      [PP(-2, -1, 'c+'), PP(-2, 1, 'c-')], K, '電流控制' + o + '源：輸出 = k × 控制電流。兩個控制端必須跨接在一個 R/L/C 支路的兩端（相同節點），控制電流即為該支路由 c+ 流向 c− 的電流。', [[-2, -1], [-2, 1]], pth('M-1.6 -0.6L-1.6 0.6M-1.8 0.35L-1.6 0.6L-1.4 0.35', 'thin'));
    add(isI ? 'ICCCS1' : 'VCCVS1', 'Current-controlled (flowing through)', '電流控制' + o + '源（串接）',
      [PP(-2, -1, 'in'), PP(-2, 1, 'out')], K, '電流控制' + o + '源（串接式）：控制電流由 in 流入、out 流出（內部以 0 V 理想電流計量測），輸出 = k × 控制電流。', [[-2, -1], [-2, 1]], pth('M-1.6 -0.8L-1.6 0.8', 'thin'));
    add(isI ? 'IVCCS' : 'VVCVS', 'Voltage-controlled', '電壓控制' + o + '源',
      [AN(-2, -1, 'in+'), AN(-2, 1, 'in-')], K, '電壓控制' + o + '源：輸出 = k × (v(in+) − v(in−))。控制端可接功率電路或控制電路（控制電路時使用前一步的控制值）。', [[-2, -1], [-2, 1]]);
    add(isI ? 'IVCCSV' : 'VVCVSV', 'Variable-gain voltage-controlled', '可變增益電壓控制' + o + '源',
      [AN(-2, -1, 'vin1'), AN(-2, 1, 'vin2')], K, '可變增益電壓控制' + o + '源：輸出 = (k·vin2)·vin1，vin2 取前一步的值（假設 vin2 變化緩慢），收斂性比非線性源好。輸入為對地電壓。', [[-2, -1], [-2, 1]]);
    add(isI ? 'INONM' : 'VNONM', 'Nonlinear (multiplication)', '非線性' + o + '源（乘法）',
      [AN(-2, -1, 'vin1'), AN(-2, 1, 'vin2')], K, '非線性' + o + '源：輸出 = k·vin1·vin2，若輸入為功率節點則以牛頓法於當步求解。', [[-2, -1], [-2, 1]]);
    add(isI ? 'INOND' : 'VNOND', 'Nonlinear (division)', '非線性' + o + '源（除法）',
      [AN(-2, -1, 'vin1'), AN(-2, 1, 'vin2')], K, '非線性' + o + '源：輸出 = k·vin1/vin2（上方 vin1 為被除數）。', [[-2, -1], [-2, 1]]);
    add(isI ? 'INONSQ' : 'VNONSQ', 'Nonlinear (square-root)', '非線性' + o + '源（平方根）',
      [AN(-2, 0, 'vin1')], K, '非線性' + o + '源：輸出 = k·√vin1。', [[-2, 0]]);
  }
  ctlDefs(false);
  def('VPOWERS', { name: 'Power', zh: '冪次電壓源', pre: 'V', pins: [PP(0, -2, '+'), PP(0, 2, '-'), AN(-2, 0, 'vin')], bb: [-2, -2, 1, 2], src: true,
    sym: () => ctlSym(false, [[-2, 0]]), params: [prm('k', 'Gain 增益', 1, '', ''), prm('k1', 'Coefficient k1', 1, '', ''), prm('k2', 'Coefficient k2', 2, '', '')],
    desc: '冪次電壓源：vo = sign(vin)·k·(k1·|vin|)^k2。' });
  def('VGDC', { name: 'Grounded DC (circle)', zh: '接地直流源（圓形）', pre: 'V', pins: [AN(0, 1, 'n')], bb: [-0.6, -1, 0.6, 1], show: ['V'], src: true,
    sym: () => ({ g: pth(ln(0, 1, 0, -0.3)) + cir(0, -0.6, 0.3) }), params: WP.DC, desc: '單端直流電壓源，參考點為地；可用於功率電路或控制電路（例如控制參考值）。' });
  def('VGDCT', { name: 'Grounded DC (T)', zh: '接地直流源（T 形）', pre: 'V', pins: [AN(0, 1, 'n')], bb: [-0.7, -0.8, 0.7, 1], show: ['V'], src: true,
    sym: () => ({ g: pth(ln(0, 1, 0, -0.6) + ln(-0.6, -0.6, 0.6, -0.6)) }), params: WP.DC, desc: '單端直流電壓源（T 形符號），參考點為地。' });
  def('VRAND', { name: 'Random', zh: '亂數電壓源', pre: 'V', pins: SP, bb: [-1, -2, 1, 2], src: true, sym: () => vsrc(W_.RAND), params: WP.RAND, desc: WD.RAND + ' 可用於功率或控制電路。' });
  def('VMATH', { name: 'Math Function', zh: '數學函數電壓源', pre: 'V', pins: SP, bb: [-1, -2, 1, 2], src: true, sym: () => ({ g: vsrc('').g, t: [T(0, 0.25, 'f(t)', 0.45)] }), params: WP.MATH, desc: WD.MATH + ' 可用於功率或控制電路。' });

  cat('Elements/Sources/Current');
  srcDefs(true);
  ctlDefs(true);
  def('IRAND', { name: 'Random', zh: '亂數電流源', pre: 'I', pins: SPI, bb: [-1, -2, 1, 2], src: true, sym: () => vsrc(arrowUp), params: WP.RAND.map(p => Object.assign({}, p, { u: p.u === 'V' ? 'A' : p.u })), desc: WD.RAND + ' 電流由 + 端流出。' });
  def('IMATH', { name: 'Math Function', zh: '數學函數電流源', pre: 'I', pins: SPI, bb: [-1, -2, 1, 2], src: true, sym: () => ({ g: vsrc(arrowUp).g, t: [T(0.5, 0.3, 'f', 0.45)] }), params: WP.MATH, desc: WD.MATH + ' 電流由 + 端流出。' });

  /* ===================================================================
   * SYMBOLS
   * =================================================================== */
  // parameters only the ideal / legacy model uses (greyed out in the dialog when a SPICE model is selected)
  Object.entries({ DIODE: ['vf', 'init', 'ron', 'roff'], LED: ['vf', 'ron', 'roff'], ZENER: ['vf', 'ron', 'roff'], NPN3: ['beta', 'vr', 'vsat', 'roff'], PNP3: ['beta', 'vr', 'vsat', 'roff'], MOS3: ['vth', 'gm', 'ron', 'vd'], PMOS3: ['vth', 'gm', 'ron', 'vd'] })
    .forEach(([t, ks]) => { D[t].params = D[t].params.map(p => ks.includes(p.k) ? Object.assign({}, p, { idealOnly: true }) : p); });
  cat('Elements/Symbols');
  def('GND', { name: 'Ground', zh: '接地', pre: 'GND', pins: [{ x: 0, y: 0, k: 'gnd', n: 'gnd' }], bb: [-0.8, 0, 0.8, 1.2], noname: true,
    sym: () => ({ g: pth(ln(0, 0, 0, 0.5) + ln(-0.8, 0.5, 0.8, 0.5) + ln(-0.5, 0.8, 0.5, 0.8) + ln(-0.2, 1.1, 0.2, 1.1)) }),
    desc: '接地：功率電路與控制電路的共同參考點（0 V）。' });
  def('LABEL', { name: 'Label', zh: '標籤', pre: 'LBL', pins: [{ x: 0, y: 0, k: 'lbl', n: 'lbl' }], bb: [0, -0.6, 3, 0.6], noname: true, label: true,
    sym: () => ({ g: '' }), params: [prm('text', 'Label 標籤名稱', 'Vo', '', '名稱相同的標籤彼此相連，等同以導線連接。', { type: 'text' })],
    desc: '節點標籤：名稱相同的標籤視為同一節點，可減少交錯走線。' });
  def('TEXT', { name: 'Text', zh: '文字註解', pre: 'TXT', pins: [], bb: [0, -0.8, 4, 0.4], noname: true, text: true, nosim: true,
    sym: () => ({ g: '' }), params: [prm('text', 'Text 文字', '註解文字', '', '', { type: 'text' })], desc: '在電路圖上放置文字註解，不參與模擬。' });

  // ---------- tree ----------
  function tree() {
    const root = { name: 'Elements', path: 'Elements', kids: [], items: [] };
    const find = (path) => {
      const parts = path.split('/'); let node = root;
      for (let i = 1; i < parts.length; i++) {
        const p = parts.slice(0, i + 1).join('/');
        let k = node.kids.find(c => c.path === p);
        if (!k) { k = { name: parts[i], path: p, kids: [], items: [] }; node.kids.push(k); }
        node = k;
      }
      return node;
    };
    ORDER.forEach(t => find(D[t].cat).items.push(t));
    return root;
  }
  ORDER.forEach(t => { if (!D[t].pinsFor) { const p = D[t].pins; D[t].pinsFor = () => p; } });
  return { D, ORDER, tree };
})();
if (typeof module !== 'undefined') module.exports = LIB;

