// Examples 7–12: thyristor rectifiers, switch modules, mechanical switches, SPICE transistors (ngspice
// reference), non-ideal IGBT / P-MOSFET, op-amp circuits with a parameter sweep.
module.exports = (EX, H) => {
  const { PI, row, wave, phasor } = H;

  // =================================================================== 7
  {
    const Vll = 400, Vm = Vll * Math.SQRT2 / Math.sqrt(3), d2r = PI / 180;
    const ctl = (k, alpha, ph, gate) => [
      ['ACTRL', 'Act' + k, { f: 50, pw: 120 }, { alpha: 'al' + k, sync: 'sy' + k, enable: 'en', gate }],
      ['CONST', 'Alpha' + k, { v: alpha }, { out: 'al' + k }],
      ['VSQU', 'Sync' + k, { Vpp: 1, f: 50, D: 0.5, ph }, { '+': 'sy' + k, '-': '0' }],
    ];
    EX.push({
      key: 'thyristor3', cat: '電力轉換', name: '三相閘流體整流：六脈波全橋、三脈波半波、六相半波',
      text: '400 V／50 Hz 閘流體整流（理想元件、電流連續）：\n六脈波全橋 α = 30°、RL 負載：Vdc = (3√2/π)·VLL·cos α = 467.8 V；\n三脈波半波 α = 15°、電阻負載：Vdc = (3√3/2π)·Vm·cos α = 260.9 V；\n六相半波（兩組相差 180° 的三相電源）α = 10°：Vdc = (6/π)·sin(π/6)·Vm·cos α = 307.1 V。\n觸發角控制器的同步訊號落在自然換相點（全橋與三脈波 30°、六相 60°）。',
      sc: { dt: '5u', tt: '0.2' },
      parts: [
        ['VSIN3', 'V1', { V: Vll, f: 50 }, { n: '0', a: 'a', b: 'b', c: 'c' }],
        ['BTHY3', 'B6', {}, { A: 'a', B: 'b', C: 'c', 'DC+': 'p', 'DC-': 'q', Ct: 'g1' }],
        ['RL', 'Load6', { R: 10, L: '100m' }, { 1: 'p', 2: 'q' }], ['VP2', 'Vdc6', {}, { '+': 'p', '-': 'q' }],
        ...ctl(1, 30, 30, 'g1'),
        ['CONST', 'Enable', { v: 1 }, { out: 'en' }],
        ['BTHY3H', 'B3', {}, { A: 'a', B: 'b', C: 'c', N: 'p3', Ct: 'g2' }],
        ['R', 'Load3', { R: 20 }, { 1: 'p3', 2: '0' }], ['VP', 'Vdc3', {}, { n: 'p3' }],
        ...ctl(2, 15, 30, 'g2'),
        ['VSIN3', 'V6b', { V: Vll, f: 50, ph: 180 }, { n: '0', a: 'a2', b: 'b2', c: 'c2' }],
        ['BTHY6H', 'B6h', {}, { A1: 'a', A2: 'c2', A3: 'b', A4: 'a2', A5: 'c', A6: 'b2', N: 'p6', Ct: 'g3' }],
        ['R', 'Load6h', { R: 20 }, { 1: 'p6', 2: '0' }], ['VP', 'Vdc6h', {}, { n: 'p6' }],
        ...ctl(3, 10, 60, 'g3'),
      ],
      screens: [['Vdc6'], ['Vdc3'], ['Vdc6h']],
      checks: h => {
        const v6 = 3 * Math.SQRT2 / PI * Vll * Math.cos(30 * d2r), v3 = 3 * Math.sqrt(3) / (2 * PI) * Vm * Math.cos(15 * d2r), v6h = 6 / PI * Math.sin(PI / 6) * Vm * Math.cos(10 * d2r);
        const dips = h.cross('Vdc6', v6, 1).filter(t => t > 0.18).length;
        return [
          row('六脈波全橋 Vdc', v6, h.avg('Vdc6', 0.18, 0.2), h.near(h.avg('Vdc6', 0.18, 0.2), v6, 1e-3)),
          row('三脈波半波 Vdc', v3, h.avg('Vdc3', 0.18, 0.2), h.near(h.avg('Vdc3', 0.18, 0.2), v3, 1e-3)),
          row('六相半波 Vdc', v6h, h.avg('Vdc6h', 0.18, 0.2), h.near(h.avg('Vdc6h', 0.18, 0.2), v6h, 1e-3)),
          row('六脈波全橋每週期脈波數', 6, dips, dips === 6),
        ];
      },
    });
  }

  // =================================================================== 8
  EX.push({
    key: 'switches', cat: '功率元件', name: '開關元件測試台（NPN、PNP、IGBT、PMOS、GTO、雙向開關）',
    text: '開關模組的導通條件：閘極訊號在 1 ~ 3 ms 為高。\nNPN、IGBT、GTO 在閘極為高時導通（10 V／10 Ω → 1 A）；PNP、PMOS 在閘極為低時導通；\n雙向開關在閘極為高時讓 1 kHz 正弦電流雙向通過。關斷時只剩 1 MΩ 漏電流。',
    sc: { dt: '1u', tt: '5m' },
    parts: [
      ['GATING', 'G1', { f: 0, n: 2, pts: '1m 3m' }, { gate: 'g' }],
      ['VDC', 'Vcc', { V: 10 }, { '+': 'vcc', '-': '0' }],
      ['R', 'R1', { R: 10 }, { 1: 'vcc', 2: 'c1' }], ['IP', 'I_npn', {}, { in: 'c1', out: 'c1b' }], ['NPN', 'Q1', {}, { C: 'c1b', E: '0', B: 'g' }],
      ['IP', 'I_pnp', {}, { in: 'vcc', out: 'e2' }], ['PNP', 'Q2', {}, { C: 'c2', E: 'e2', B: 'g' }], ['R', 'R2', { R: 10 }, { 1: 'c2', 2: '0' }],
      ['R', 'R3', { R: 10 }, { 1: 'vcc', 2: 'c3' }], ['IP', 'I_igbt', {}, { in: 'c3', out: 'c3b' }], ['IGBT', 'Q3', {}, { C: 'c3b', E: '0', G: 'g' }],
      ['IP', 'I_pmos', {}, { in: 'vcc', out: 's4' }], ['PMOS', 'Q4', {}, { D: 'd4', S: 's4', G: 'g' }], ['R', 'R4', { R: 10 }, { 1: 'd4', 2: '0' }],
      ['R', 'R5', { R: 10 }, { 1: 'vcc', 2: 'a5' }], ['IP', 'I_gto', {}, { in: 'a5', out: 'a5b' }], ['GTO', 'Q5', {}, { A: 'a5b', K: '0', Gate: 'g' }],
      ['VSIN', 'Vac', { Vm: 10, f: '1k' }, { '+': 'ac', '-': '0' }], ['R', 'R6', { R: 10 }, { 1: 'ac', 2: 'a6' }], ['IP', 'I_ssw', {}, { in: 'a6', out: 'a6b' }], ['SSWI', 'S6', {}, { 1: 'a6b', 2: '0', Gate: 'g' }],
    ],
    screens: [['I_npn', 'I_igbt', 'I_gto'], ['I_pnp', 'I_pmos'], ['I_ssw']],
    checks: h => {
      const r = [];
      for (const n of ['I_npn', 'I_igbt', 'I_gto']) {
        r.push(row(`${n} 導通（2 ms）`, 1, h.at(n, 2e-3), h.near(h.at(n, 2e-3), 1, 1e-5)));
        r.push(row(`${n} 關斷（4 ms，僅漏電）`, 1e-5, h.at(n, 4e-3), Math.abs(h.at(n, 4e-3)) < 2e-5));
        const up = h.cross(n, 0.5, 1)[0];
        r.push(row(`${n} 導通時刻`, 1e-3, up, Math.abs(up - 1e-3) <= 1e-6));
      }
      for (const n of ['I_pnp', 'I_pmos']) {
        r.push(row(`${n} 閘極低時導通（0.5 ms）`, 1, h.at(n, 0.5e-3), h.near(h.at(n, 0.5e-3), 1, 1e-5)));
        r.push(row(`${n} 閘極高時關斷（2 ms）`, 1e-5, h.at(n, 2e-3), Math.abs(h.at(n, 2e-3)) < 2e-5));
      }
      r.push(wave(h, 'I_ssw', t => Math.sin(2 * PI * 1e3 * t), 1.01e-3, 2.99e-3, [], 1e-5, '雙向開關導通期間 i = v/R（含負半週）'));
      r.push(row('雙向開關關斷時電流', 0, h.max('I_ssw', 3.01e-3, 5e-3), h.max('I_ssw', 3.01e-3, 5e-3) < 2e-5));
      return r;
    },
  });

  // =================================================================== 9
  EX.push({
    key: 'mech_switches', cat: '功率元件', name: '機械開關：按鈕、單刀雙擲、雙刀單擲、雙刀雙擲極性反轉、三相開關',
    text: '按鈕開關在 1 ms 按下、3 ms 放開：10 V 經 1 kΩ 對 1 µF 充電（τ = 1 ms）。\nSPDT 在閘極為高時接 10 V、為低時接地（充電後放電）；DPST 同時切換兩組；\nDPDT 把 10 V 以相反極性接到負載（−10 → +10 → −10 V）；三相開關在 5 ~ 15 ms 接通 400 V 三相電阻負載。',
    sc: { dt: '1u', tt: '20m' },
    parts: [
      ['GATING', 'G1', { f: 0, n: 2, pts: '1m 3m' }, { gate: 'g' }],
      ['VDC', 'V10', { V: 10 }, { '+': 'v10', '-': '0' }], ['VDC', 'V5', { V: 5 }, { '+': 'v5', '-': '0' }],
      ['PBSW', 'Pb', { init: 0, times: '1m 3m' }, { 1: 'v10', 2: 'p1' }], ['R', 'Rp', { R: '1k' }, { 1: 'p1', 2: 'cp' }], ['C', 'Cp', { C: '1u' }, { 1: 'cp', 2: '0' }], ['VP', 'V_pb', {}, { n: 'cp' }],
      ['SPDT', 'Sw', {}, { COM: 'm', 1: 'v10', 2: '0', Gate: 'g' }], ['R', 'Rs', { R: '1k' }, { 1: 'm', 2: 'cs' }], ['C', 'Cs', { C: '1u' }, { 1: 'cs', 2: '0' }], ['VP', 'V_spdt', {}, { n: 'cs' }],
      ['DPST', 'Dp', {}, { '1a': 'v10', '2a': 'da', '1b': 'v5', '2b': 'db', Gate: 'g' }],
      ['R', 'Rda', { R: '1k' }, { 1: 'da', 2: 'cda' }], ['C', 'Cda', { C: '1u' }, { 1: 'cda', 2: '0' }], ['VP', 'V_dpst_a', {}, { n: 'cda' }],
      ['R', 'Rdb', { R: '1k' }, { 1: 'db', 2: 'cdb' }], ['C', 'Cdb', { C: '1u' }, { 1: 'cdb', 2: '0' }], ['VP', 'V_dpst_b', {}, { n: 'cdb' }],
      ['DPDT', 'Dd', {}, { COMa: 'v10', a1: 'x', a2: 'y', COMb: '0', b1: 'y', b2: 'x', Gate: 'g' }], ['R', 'Rl', { R: 100 }, { 1: 'x', 2: 'y' }], ['VP2', 'V_dpdt', {}, { '+': 'x', '-': 'y' }],
      ['VSIN3', 'V3', { V: 400, f: 50 }, { n: '0', a: 'a', b: 'b', c: 'c' }], ['GATING', 'G3', { f: 0, n: 2, pts: '5m 15m' }, { gate: 'g3' }],
      ['SSWI3', 'S3', {}, { A: 'a', B: 'b', C: 'c', a: 'la', b: 'lb', c: 'lc', Gate: 'g3' }], ['IP', 'Ia', {}, { in: 'la', out: 'la2' }],
      ['R3', 'Rl3', { R: 10 }, { A: 'la2', B: 'lb', C: 'lc', a: '0', b: '0', c: '0' }],
    ],
    screens: [['V_pb', 'V_spdt'], ['V_dpst_a', 'V_dpst_b'], ['V_dpdt'], ['Ia']],
    checks: h => {
      // open contacts leak through 1 MΩ: before closing and after opening the RC keeps charging with τ = (1k + 1M)·1µ
      const toff = (1e3 + 1e6) * 1e-6;
      const leakRC = V => t => {
        const v1 = V * (1 - Math.exp(-1e-3 / toff));
        if (t < 1e-3) return V * (1 - Math.exp(-t / toff));
        const von = V - (V - v1) * Math.exp(-(t - 1e-3) / ((1e3 + 1e-5) * 1e-6));
        if (t < 3e-3) return von;
        const v3 = V - (V - v1) * Math.exp(-2e-3 / ((1e3 + 1e-5) * 1e-6));
        return V - (V - v3) * Math.exp(-(t - 3e-3) / toff);
      };
      const ch = (V, t) => V * (1 - Math.exp(-(t - 1e-3) / 1e-3));
      const v3 = ch(10, 3e-3), Vm = 400 * Math.SQRT2 / Math.sqrt(3), w = 2 * PI * 50;
      return [
        wave(h, 'V_pb', leakRC(10), 0, 20e-3, [1e-3, 3e-3], 1e-5, '按鈕開關 RC 充電後保持（含 1 MΩ 漏電）'),
        wave(h, 'V_spdt', t => (t < 1e-3 ? 0 : t < 3e-3 ? ch(10, t) : v3 * Math.exp(-(t - 3e-3) / 1e-3)), 0, 20e-3, [1e-3, 3e-3], 1e-5, 'SPDT 充電後放電'),
        wave(h, 'V_dpst_b', leakRC(5), 0, 20e-3, [1e-3, 3e-3], 1e-5, 'DPST 第二組（5 V）充電後保持'),
        wave(h, 'V_dpst_a', leakRC(10), 0, 20e-3, [1e-3, 3e-3], 1e-5, 'DPST 第一組（10 V）同時切換'),
        row('DPDT 極性：閘極低', -10, h.at('V_dpdt', 0.5e-3), h.near(h.at('V_dpdt', 0.5e-3), -10, 1e-5)),
        row('DPDT 極性：閘極高', 10, h.at('V_dpdt', 2e-3), h.near(h.at('V_dpdt', 2e-3), 10, 1e-5)),
        wave(h, 'Ia', t => (t > 5e-3 && t < 15e-3 ? Vm * Math.sin(w * t) / 10 : 0), 0, 20e-3, [5e-3, 15e-3], 1e-5, '三相開關 A 相電流 = va/R（5 ~ 15 ms）'),
      ];
    },
  });

  // =================================================================== 10
  {
    const CARDS = {
      Q2N3904: 'NPN IS=6.734f VAF=74.03 BF=416.4 NE=1.259 ISE=6.734f IKF=66.78m BR=.7371 NC=2 ISC=0 IKR=0 RC=1 CJC=3.638p MJC=.3085 VJC=.75 FC=.5 CJE=4.493p MJE=.2593 VJE=.75 TR=239.5n TF=301.2p RB=10',
      Q2N3906: 'PNP IS=1.41f VAF=18.7 BF=180.7 NE=1.5 ISE=0 IKF=80m BR=4.977 NC=2 ISC=0 IKR=0 RC=2.5 CJC=9.728p MJC=.5776 VJC=.75 FC=.5 CJE=8.063p MJE=.3677 VJE=.75 TR=33.42n TF=179.3p RB=10',
      MN: 'NMOS LEVEL=1 VTO=2.1 KP=0.105 LAMBDA=0.01',
      MP: 'PMOS LEVEL=1 VTO=-2 KP=0.05 LAMBDA=0.01',
      DZ: 'D IS=5n N=1.2 RS=2 CJO=100p VJ=0.75 M=0.33 IBV=5m BV=5.1',
    };
    const g4 = '5 5 0 0 5 5 0 0 5 5 0 0', g4t = '0 0.5m 0.501m 1.501m 1.502m 2.501m 2.502m 3.502m 3.503m 4.502m 4.503m 5.503m';
    const pwl = g4t.split(' ').map((t, i) => `${t} ${g4.split(' ')[i]}`).join(' ');
    const ngNet = [
      'spice transistors',
      'Vcc vcc 0 DC 12',
      'Vin in 0 SIN(0 0.02 1k)', 'Cin in b1 10u', 'Rb1 vcc b1 47k', 'Rb2 b1 0 10k', 'Rc vcc c1 2.2k', 'Re e1 0 470', 'Q1 c1 b1 e1 Q2N3904',
      'Vb2 b2 0 DC 10.7', 'Re2 vcc e2 1k', 'Q2 c2 b2 e2 Q2N3906', 'Rc2 c2 0 2.2k',
      'Vg3 g3 0 SIN(3 0.1 1k)', 'Rd3 vcc d3 1k', 'M3 d3 g3 0 0 MN W=100u L=100u', 'Cgs3 g3 0 25p', 'Cgd3 g3 d3 5p', 'Cds3 d3 0 10p',
      'V5 v5 0 DC 5', `Vg4 g4 0 PWL(${pwl})`, 'M4 d4 g4 v5 v5 MP W=100u L=100u', 'Cgs4 g4 v5 30p', 'Cgd4 g4 d4 6p', 'Cds4 d4 v5 12p', 'Rl4 d4 0 100',
      'Rz vcc z 470', 'Dz 0 z DZ', 'Rlz z 0 1k',
      ...Object.entries(CARDS).map(([k, v]) => `.model ${k} ${v}`),
      '.ic v(b1)=0', // DaedalusEngine starts the coupling capacitor at 0 V (C v0 = 0)
      '.options reltol=1e-7 abstol=1e-15 vntol=1e-10 method=trap',
      '.tran 0.25u 5m 0 0.25u',
      '.control', 'set wr_singlescale', 'set wr_vecnames', 'option numdgt=12', 'run', 'wrdata spice_ref.txt v(c1) v(c2) v(d3) v(d4) v(z)', '.endc', '.end',
    ].join('\n');
    EX.push({
      key: 'spice_transistors', cat: '類比與控制 IC', name: 'SPICE 電晶體電路（BJT 放大、PNP 電流源、MOSFET 放大與負載開關、稽納穩壓）',
      text: 'SPICE 物理模型電路：2N3904 共射放大（分壓偏壓、Re = 470 Ω，增益約 −Rc/Re）、\n2N3906 定電流源、N-MOSFET 共源放大、P-MOSFET 負載開關（1 kHz 方波驅動）、稽納 5.1 V 穩壓。\n驗證：同一電路以 ngspice 47 模擬比較整段波形（tools/ngspice）。',
      sc: { dt: '1u', tt: '5m' },
      parts: [
        ['VDC', 'Vcc', { V: 12 }, { '+': 'vcc', '-': '0' }],
        ['VSIN', 'Vin', { Vm: 0.02, f: '1k' }, { '+': 'in', '-': '0' }], ['C', 'Cin', { C: '10u' }, { 1: 'in', 2: 'b1' }], ['R', 'Rb1', { R: '47k' }, { 1: 'vcc', 2: 'b1' }], ['R', 'Rb2', { R: '10k' }, { 1: 'b1', 2: '0' }],
        ['R', 'Rc', { R: '2.2k' }, { 1: 'vcc', 2: 'c1' }], ['R', 'Re', { R: 470 }, { 1: 'e1', 2: '0' }], ['NPN3', 'Q1', { model: '2N3904' }, { C: 'c1', E: 'e1', B: 'b1' }], ['VP', 'Vc_ce', {}, { n: 'c1' }],
        ['VDC', 'Vb2', { V: 10.7 }, { '+': 'b2', '-': '0' }], ['R', 'Re2', { R: '1k' }, { 1: 'vcc', 2: 'e2' }], ['PNP3', 'Q2', { model: '2N3906' }, { C: 'c2', E: 'e2', B: 'b2' }], ['R', 'Rc2', { R: '2.2k' }, { 1: 'c2', 2: '0' }], ['VP', 'Vc_pnp', {}, { n: 'c2' }],
        ['VSIN', 'Vg3', { Vm: 0.1, f: '1k', off: 3 }, { '+': 'g3', '-': '0' }], ['R', 'Rd3', { R: '1k' }, { 1: 'vcc', 2: 'd3' }], ['MOS3', 'M3', { model: 'GENERIC_N' }, { D: 'd3', S: '0', G: 'g3' }], ['VP', 'Vd_cs', {}, { n: 'd3' }],
        ['VDC', 'V5', { V: 5 }, { '+': 'v5', '-': '0' }], ['VPWL', 'Vg4', { vals: g4, times: g4t }, { '+': 'g4', '-': '0' }],
        ['PMOS3', 'M4', { model: 'GENERIC_P' }, { D: 'd4', S: 'v5', G: 'g4' }], ['R', 'Rl4', { R: 100 }, { 1: 'd4', 2: '0' }], ['VP', 'Vd_psw', {}, { n: 'd4' }],
        ['R', 'Rz', { R: 470 }, { 1: 'vcc', 2: 'z' }], ['ZENER', 'Dz', { model: 'ZENER', vb: 5.1 }, { A: '0', K: 'z' }], ['R', 'Rlz', { R: '1k' }, { 1: 'z', 2: '0' }], ['VP', 'Vz', {}, { n: 'z' }],
      ],
      screens: [['Vc_ce'], ['Vc_pnp', 'Vz'], ['Vd_cs'], ['Vd_psw']],
      checks: h => {
        const fs = require('fs'), path = require('path'), { execFileSync } = require('child_process');
        const root = path.resolve(__dirname, '..', '..');
        const ng = path.join(root, 'tools/ngspice/Spice64/bin/ngspice_con.exe');
        if (!fs.existsSync(ng)) return [row('ngspice 參考', '需要 tools/ngspice', '略過', true, '未安裝 ngspice')];
        const wd = path.join(root, 'tests/examples');
        fs.writeFileSync(path.join(wd, 'spice_ref.cir'), ngNet);
        execFileSync(ng, ['-b', 'spice_ref.cir'], { cwd: wd, stdio: 'ignore' });
        const L = fs.readFileSync(path.join(wd, 'spice_ref.txt'), 'utf8').trim().split(/\r?\n/).slice(1).map(l => l.trim().split(/\s+/).map(Number));
        const ref = j => t => { let lo = 0, hi = L.length - 1; while (hi - lo > 1) { const m = (lo + hi) >> 1; if (L[m][0] <= t) lo = m; else hi = m; } const a = L[lo], b = L[hi]; return a[j] + (b[j] - a[j]) * (t - a[0]) / ((b[0] - a[0]) || 1); };
        return [
          wave(h, 'Vc_ce', ref(1), 0, 5e-3, [], 1e-4, '2N3904 共射放大 Vc vs ngspice'),
          wave(h, 'Vc_pnp', ref(2), 0, 5e-3, [], 1e-4, '2N3906 電流源 Vc vs ngspice'),
          wave(h, 'Vd_cs', ref(3), 0, 5e-3, [], 1e-4, 'N-MOSFET 共源 Vd vs ngspice'),
          wave(h, 'Vd_psw', ref(4), 0, 5e-3, [0.5e-3, 1.501e-3, 2.501e-3, 3.502e-3, 4.502e-3], 2e-3, 'P-MOSFET 負載開關 Vd vs ngspice'),
          wave(h, 'Vz', ref(5), 0, 5e-3, [], 1e-4, '稽納穩壓 Vz vs ngspice'),
        ];
      },
    });
  }

  // =================================================================== 11
  EX.push({
    key: 'nonideal_igbt_pmos', cat: '功率元件', name: '非理想 IGBT 與 P-MOSFET（導通壓降、關斷）',
    text: '非理想 IGBT：400 V／20 Ω，導通電流約 20 A 時 Vce 應接近資料手冊 Vce(sat) = 1.8 V @ 20 A；\n20 µs 關斷（含米勒平台與 0.4 µs 拖尾電流）。\n非理想 P-MOSFET 高側開關：48 V／4.8 Ω，導通壓降 = I·Rds(on)（80 mΩ）。',
    sc: { dt: '2n', tt: '40u' },
    parts: [
      ['VDC', 'Vh', { V: 400 }, { '+': 'vh', '-': '0' }], ['R', 'Rl', { R: 20 }, { 1: 'vh', 2: 'c' }], ['IP', 'Ic', {}, { in: 'c', out: 'c2' }],
      ['IGBTNI', 'Q1', {}, { C: 'c2', E: '0', G: 'g1' }], ['VP', 'Vce', {}, { n: 'c2' }], ['GATING', 'G1', { f: 0, n: 2, pts: '1u 20u' }, { gate: 'g1' }],
      ['VDC', 'V48', { V: 48 }, { '+': 'v48', '-': '0' }], ['PMOSNI', 'Q2', {}, { D: 'd', S: 'v48', G: 'g2' }], ['IP', 'Id', {}, { in: 'd', out: 'd2' }],
      ['R', 'Rl2', { R: 4.8 }, { 1: 'd2', 2: '0' }], ['VP', 'Vd', {}, { n: 'd' }], ['GATING', 'G2', { f: 0, n: 2, pts: '25u 35u' }, { gate: 'g2' }],
    ],
    screens: [['Ic'], ['Vce'], ['Id']],
    checks: h => {
      const ic = h.at('Ic', 15e-6), vce = h.at('Vce', 15e-6);
      const i10 = h.cross('Ic', 0.1 * ic, -1).filter(t => t > 20e-6);
      const id = h.at('Id', 10e-6);
      // level-1 channel: Rds(on) = 80 mΩ at small Vds, Vov = 10 − 2.5 V: I = K·(Vov·Vsd − Vsd²/2), K = 1/(Rds·Vov)
      const Vov = 7.5, K = 1 / (0.08 * Vov);
      let vsd = 0.8;
      for (let i = 0; i < 60; i++) { const f = K * (Vov * vsd - vsd * vsd / 2) - (48 - vsd) / 4.8, df = K * (Vov - vsd) + 1 / 4.8; vsd -= f / df; }
      return [
        row('IGBT 導通 Vce（Ic ≈ 19.9 A）', 1.8, vce, Math.abs(vce - 1.8) < 0.05, `Ic = ${ic.toFixed(3)} A`),
        row('IGBT 導通電流 = (400 − Vce)/20', (400 - vce) / 20, ic, h.near(ic, (400 - vce) / 20, 1e-6)),
        row('IGBT 關斷後電流降到 10 % 的時間', '< 2 µs', i10[0] - 20e-6, i10.length > 0 && i10[0] - 20e-6 < 2e-6, '含米勒平台與拖尾'),
        row('P-MOSFET 閘極為高（關斷）時電流', 0, h.at('Id', 30e-6), Math.abs(h.at('Id', 30e-6)) < 1e-3),
        row('P-MOSFET 導通電流（平方律通道）', (48 - vsd) / 4.8, id, h.near(id, (48 - vsd) / 4.8, 1e-4)),
        row('P-MOSFET 導通壓降（Rds(on) 於小 Vds 為 80 mΩ）', vsd, 48 - h.at('Vd', 10e-6), h.near(48 - h.at('Vd', 10e-6), vsd, 2e-3), '大電流時 Vds 使通道電阻上升約 5.6 %'),
      ];
    },
  });

  // =================================================================== 12
  {
    const opG = Rf => {
      // V− node and output node with gain A, output resistance ro, no load: solve for vo/vin
      const A = 1e5, ro = 80, R1 = 1e3;
      // (vin − vm)/R1 + (vo − vm)/Rf = 0 ;  (−A·vm − vo)/ro = (vo − vm)/Rf
      const a11 = -(1 / R1 + 1 / Rf), a12 = 1 / Rf, b1 = -1 / R1;
      const a21 = -A / ro + 1 / Rf, a22 = -(1 / ro + 1 / Rf), b2 = 0;
      const det = a11 * a22 - a12 * a21;
      return (a11 * b2 - a21 * b1) / det;
    };
    EX.push({
      key: 'opamp', cat: '類比與控制 IC', name: '運算放大器：反相放大（參數掃描 Rf）、積分器、一對二光耦',
      text: '反相放大器（A = 10⁵、Ro = 80 Ω）以參數掃描 Rf = 10k、20k、47k：增益 ≈ −Rf/1k（含有限增益修正）。\n運算放大器（含地端）做積分器：V+ 接 2.5 V 參考、輸入 3.5 V、10 kΩ／10 nF → 輸出以 −10⁴ V/s 下降。\n一對二光耦合器：同一顆 LED（約 5 mA）照射兩組光電晶體，CTR 100 % 與 50 %。',
      sc: { dt: '1u', tt: '2m' },
      params: 'Rf = 10k',
      extra: [['PSWEEP', { var: 'Rf', mode: 0, vals: '10k 20k 47k' }]],
      parts: [
        ['VSIN', 'Vin', { Vm: 0.1, f: '1k' }, { '+': 'in', '-': '0' }], ['R', 'R1', { R: '1k' }, { 1: 'in', 2: 'm' }], ['R', 'Rfb', { R: 'Rf' }, { 1: 'm', 2: 'out' }],
        ['OPAMP', 'U1', {}, { 'V-': 'm', 'V+': '0', Vo: 'out' }], ['VP', 'Vo', {}, { n: 'out' }],
        ['VGDCT', 'Vref', { V: 2.5 }, { n: 'ref' }], ['VGDC', 'Vi', { V: 3.5 }, { n: 'vi' }], ['R', 'Ri', { R: '10k' }, { 1: 'vi', 2: 'mi' }], ['C', 'Ci', { C: '10n' }, { 1: 'mi', 2: 'oi' }],
        ['OPAMP1', 'U2', {}, { 'V-': 'mi', 'V+': 'ref', Vo: 'oi', gnd: 'ref' }], ['VP', 'Vint', {}, { n: 'oi' }],
        ['VDC', 'V5', { V: 5 }, { '+': 'v5', '-': '0' }], ['R', 'Rled', { R: 780 }, { 1: 'v5', 2: 'la' }], ['IP', 'If', {}, { in: 'la', out: 'la2' }],
        ['OPTO2', 'U3', { ctr1: 100, ctr2: 50 }, { A: 'la2', K: '0', C1: 'k1', E1: '0', C2: 'k2', E2: '0' }],
        ['IP', 'Ic1', {}, { in: 'v5', out: 'p1' }], ['R', 'Rp1', { R: 100 }, { 1: 'p1', 2: 'k1' }],
        ['IP', 'Ic2', {}, { in: 'v5', out: 'p2' }], ['R', 'Rp2', { R: 100 }, { 1: 'p2', 2: 'k2' }],
      ],
      checks: h => {
        const r = [];
        for (const [lab, Rf] of [['10k', 1e4], ['20k', 2e4], ['47k', 4.7e4]]) {
          const g = opG(Rf);
          r.push(...phasor(h, `Vo [Rf=${lab}]`, 1e3, 1e-3, 2e-3, 0.1 * Math.abs(g), 180, 1e-4, 0.01, `反相放大 Rf=${lab}`));
        }
        const s = (h.at('Vint [Rf=10k]', 1.5e-3) - h.at('Vint [Rf=10k]', 0.5e-3)) / 1e-3;
        r.push(row('積分器斜率（V/s）', -1e4, s, h.near(s, -1e4, 1e-3)));
        const iF = h.last('If [Rf=10k]'), i1 = h.last('Ic1 [Rf=10k]'), i2 = h.last('Ic2 [Rf=10k]');
        r.push(row('光耦輸出 1 CTR（Ic1/IF）', 1, i1 / iF, h.near(i1 / iF, 1, 0.05), `IF = ${(iF * 1e3).toFixed(3)} mA`));
        r.push(row('光耦兩輸出電流比 Ic2/Ic1（CTR2/CTR1）', 0.5, i2 / i1, h.near(i2 / i1, 0.5, 0.01)));
        return r;
      },
    });
  }
};
