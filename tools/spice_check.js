// Cross-check of the SPICE device models against ngspice: identical circuits and stimuli in both
// simulators, waveforms compared point by point (ngspice run with tight tolerances as the reference).
// usage: node tools/spice_check.js [ngspice_con.exe] [daedalus-cli.exe] [workdir] [-v]
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const NG = process.argv[2] || 'tools/ngspice/Spice64/bin/ngspice_con.exe';
const CLI = process.argv[3] || 'target/release/daedalus-cli.exe';
const WD = process.argv[4] || 'tests/spice';
const VERBOSE = process.argv.includes('-v');
fs.mkdirSync(WD, { recursive: true });

// SPICE model cards exactly as the DaedalusEngine presets (crates/daedalus-core/src/models/devices.rs)
const CARDS = {
  '1N4148': 'D IS=2.52n RS=0.568 N=1.752 CJO=4p M=0.4 TT=20n BV=100 IBV=100u',
  '1N4007': 'D IS=7.02767n RS=0.0341512 N=1.80803 BV=1100 IBV=0.1 CJO=39p VJ=0.7 M=0.5 FC=0.5 TT=100n',
  '1N5819': 'D IS=31.7u RS=0.051 N=1.373 CJO=259p M=0.35 VJ=0.38 BV=40 IBV=1m',
  ZENER: 'D IS=5n N=1.2 RS=2 CJO=100p VJ=0.75 M=0.33 IBV=5m',
  LEDRED: 'D IS=93.2p RS=42m N=3.73 BV=4 IBV=10u CJO=2.97p VJ=0.75 M=0.333 TT=4.32u',
  '2N3904': 'NPN IS=6.734f VAF=74.03 BF=416.4 NE=1.259 ISE=6.734f IKF=66.78m BR=.7371 NC=2 ISC=0 IKR=0 RC=1 CJC=3.638p MJC=.3085 VJC=.75 FC=.5 CJE=4.493p MJE=.2593 VJE=.75 TR=239.5n TF=301.2p RB=10',
  '2N4401': 'NPN IS=26.03f VAF=90.7 BF=4.292K NE=1.244 ISE=26.03f IKF=.2061 BR=1.01 NC=2 ISC=0 IKR=0 RC=.5 CJC=11.01p MJC=.3763 VJC=.75 FC=.5 CJE=24.07p MJE=.3641 VJE=.75 TR=233.7n TF=466.5p RB=4.7',
  '2N2222A': 'NPN IS=14.34f VAF=74.03 BF=255.9 NE=1.307 ISE=14.34f IKF=.2847 BR=6.092 NC=2 ISC=0 IKR=0 RC=1 CJC=7.306p MJC=.3416 VJC=.75 FC=.5 CJE=22.01p MJE=.377 VJE=.75 TR=46.91n TF=411.1p RB=10',
  '2N3906': 'PNP IS=1.41f VAF=18.7 BF=180.7 NE=1.5 ISE=0 IKF=80m BR=4.977 NC=2 ISC=0 IKR=0 RC=2.5 CJC=9.728p MJC=.5776 VJC=.75 FC=.5 CJE=8.063p MJE=.3677 VJE=.75 TR=33.42n TF=179.3p RB=10',
  GENERIC_N: 'NMOS LEVEL=1 VTO=2.1 KP=0.105 LAMBDA=0.01',
  GENERIC_P: 'PMOS LEVEL=1 VTO=-2 KP=0.05 LAMBDA=0.01',
};
// MOSFET presets carry lumped CGS/CGD/CDS: in ngspice they are added as external capacitors
const MOSCAP = { GENERIC_N: [25e-12, 5e-12, 10e-12], GENERIC_P: [30e-12, 6e-12, 12e-12] };

const pwl = pts => pts.map(p => p.join(' ')).join(' ');
const dpwl = pts => ({ vals: pts.map(p => p[1]).join(' '), times: pts.map(p => p[0]).join(' ') });

// circuit description shared by both simulators. nodes: 0 = ground, strings otherwise.
// parts: [kind, name, nodes[], value/model]  kinds: V (PWL pts), R, C, D, Q, M
function ngNetlist(c) {
  const L = [c.title];
  const models = new Set();
  for (const [k, n, nd, v] of c.parts) {
    if (k === 'V') L.push(`V${n} ${nd[0]} ${nd[1]} PWL(${pwl(v)})`);
    if (k === 'R') L.push(`R${n} ${nd[0]} ${nd[1]} ${v}`);
    if (k === 'C') L.push(`C${n} ${nd[0]} ${nd[1]} ${v}`);
    if (k === 'D') { L.push(`D${n} ${nd[0]} ${nd[1]} M_${v}${c.bv ? '_BV' : ''}`); models.add(v); }
    if (k === 'Q') { L.push(`Q${n} ${nd[0]} ${nd[2]} ${nd[1]} M_${v}`); models.add(v); } // nodes given C,E,B
    if (k === 'M') {
      L.push(`M${n} ${nd[0]} ${nd[2]} ${nd[1]} ${nd[1]} M_${v} W=100u L=100u`); // D,S,G → D G S B
      models.add(v);
      const [cgs, cgd, cds] = MOSCAP[v];
      L.push(`CGS${n} ${nd[2]} ${nd[1]} ${cgs}`, `CGD${n} ${nd[2]} ${nd[0]} ${cgd}`, `CDS${n} ${nd[0]} ${nd[1]} ${cds}`);
    }
  }
  for (const m of models) L.push(`.model M_${m}${c.bv ? '_BV' : ''} ${CARDS[m]}${c.bv ? ' BV=' + c.bv : ''}`);
  // measurement: probe voltages and currents through 0 V sources
  for (const [n, a, b] of c.ammeters || []) L.push(`VAM_${n} ${a} ${b} 0`);
  L.push('.options reltol=1e-7 abstol=1e-15 vntol=1e-10 chgtol=1e-18 method=trap maxord=2 itl4=200');
  L.push(`.tran ${c.dt / 4} ${c.tt} 0 ${c.dt / 4}`);
  const vecs = c.signals.map(s => (s.v ? `v(${s.v})` : `i(VAM_${s.i})`));
  L.push('.control', 'set wr_singlescale', 'set wr_vecnames', 'option numdgt=12', 'run', `wrdata ${c.name}.ng.txt ${vecs.join(' ')}`, '.endc', '.end');
  return L.join('\n');
}
function deNetlist(c) {
  const nodes = new Map([['0', 0]]);
  const kinds = ['G'];
  const id = x => { x = String(x); if (!nodes.has(x)) { nodes.set(x, kinds.length); kinds.push('P'); } return nodes.get(x); };
  const els = [];
  let k = 1;
  const add = (type, name, params, pins) => els.push({ id: k++, type, name, params, pins });
  // ammeters split a node: the part attached on the far side uses node "<b>"
  for (const [kind, n, nd, v] of c.parts) {
    if (kind === 'V') add('VPWL', 'V' + n, Object.assign({ f: '0' }, dpwl(v)), [id(nd[0]), id(nd[1])]);
    if (kind === 'R') add('R', 'R' + n, { R: String(v) }, [id(nd[0]), id(nd[1])]);
    if (kind === 'C') add('C', 'C' + n, { C: String(v), v0: '0' }, [id(nd[0]), id(nd[1])]);
    if (kind === 'D') add(c.bv ? 'ZENER' : 'DIODE', 'D' + n, c.bv ? { model: v, vb: String(c.bv) } : { model: v }, [id(nd[0]), id(nd[1])]);
    if (kind === 'Q') add(CARDS[v].startsWith('PNP') ? 'PNP3' : 'NPN3', 'Q' + n, { model: v }, [id(nd[0]), id(nd[1]), id(nd[2])]);
    if (kind === 'M') add(CARDS[v].startsWith('PMOS') ? 'PMOS3' : 'MOS3', 'M' + n, { model: v }, [id(nd[0]), id(nd[1]), id(nd[2])]);
  }
  for (const [n, a, b] of c.ammeters || []) add('IP', n, {}, [id(a), id(b)]);
  for (const s of c.signals) if (s.v) add('VP', 'v_' + s.v, {}, [id(s.v)]);
  return { nets: kinds.map(x => ({ kind: x })), elements: els, paramText: '' };
}

function run(c) {
  const ng = path.join(WD, c.name + '.cir');
  fs.writeFileSync(ng, ngNetlist(c));
  execFileSync(path.resolve(NG), ['-b', path.basename(ng)], { cwd: WD, stdio: ['ignore', 'pipe', 'pipe'], maxBuffer: 1 << 28 });
  const ngl = fs.readFileSync(path.join(WD, c.name + '.ng.txt'), 'utf8').trim().split(/\r?\n/);
  const ngT = [], ngV = c.signals.map(() => []);
  for (const l of ngl.slice(1)) { const v = l.trim().split(/\s+/).map(Number); ngT.push(v[0]); c.signals.forEach((_, j) => ngV[j].push(v[j + 1])); }
  const cj = path.join(WD, c.name + '.json'), cc = path.join(WD, c.name + '.de.csv');
  fs.writeFileSync(cj, JSON.stringify({ net: deNetlist(c), sc: { dt: c.dt, tTotal: c.tt, tPrint: 0, printStep: 1, interp: true, damp: true, lte: true, lteTol: 1e-3 }, ac: null }));
  execFileSync(path.resolve(CLI), [cj, cc]);
  const dl = fs.readFileSync(cc, 'utf8').trim().split(/\r?\n/);
  const head = dl[0].split(',');
  const dT = [], dV = c.signals.map(() => []);
  const colOf = s => head.indexOf(s.v ? 'v_' + s.v : s.i);
  const cols = c.signals.map(colOf);
  for (const l of dl.slice(1)) { const v = l.split(',').map(Number); dT.push(v[0]); cols.forEach((ci, j) => dV[j].push(v[ci])); }
  // compare at DaedalusEngine output points (ngspice interpolated linearly on its 4x finer grid)
  const res = c.signals.map((s, j) => {
    let p = 0, emax = 0, at = 0, scale = 0;
    for (let i = 0; i < dT.length; i++) {
      const t = dT[i];
      if (c.skip && t < c.skip) continue;
      while (p < ngT.length - 2 && ngT[p + 1] < t) p++;
      const a = ngT[p], b = ngT[p + 1];
      const r = b > a ? ngV[j][p] + (ngV[j][p + 1] - ngV[j][p]) * (t - a) / (b - a) : ngV[j][p];
      const sgn = s.neg ? -1 : 1;
      scale = Math.max(scale, Math.abs(r));
      const e = Math.abs(sgn * dV[j][i] - r);
      if (e > emax) { emax = e; at = t; }
    }
    return { sig: s.label, rel: emax / (scale || 1), emax, scale, at };
  });
  return res;
}

// ---------------- test circuits ----------------
const T = [];
// diode static I-V: slow ramp through a series resistor (capacitive current negligible)
for (const [m, vmax, r] of [['1N4148', 3, 10], ['1N4007', 5, 1], ['1N5819', 5, 0.5], ['LEDRED', 6, 50]]) {
  T.push({ name: 'iv_' + m, title: `${m} forward I-V`, dt: 1e-4, tt: 1, kind: '靜態 I-V（正向）',
    parts: [['V', 1, ['in', 0], [[0, -5], [1, vmax]]], ['R', 1, ['in', 'a'], r], ['D', 1, ['k', 0], m]],
    ammeters: [['ID', 'a', 'k']], signals: [{ i: 'ID', label: 'I(D)' }, { v: 'a', label: 'V(D)' }] });
}
// zener: reverse breakdown with the ZENER element (BV from vb)
T.push({ name: 'iv_zener', title: 'zener 5.1V I-V', bv: 5.1, dt: 1e-4, tt: 1, kind: '靜態 I-V（含崩潰區）',
  parts: [['V', 1, ['in', 0], [[0, -12], [1, 2]]], ['R', 1, ['in', 'a'], 100], ['D', 1, ['k', 0], 'ZENER']],
  ammeters: [['ID', 'a', 'k']], signals: [{ i: 'ID', label: 'I(D)' }, { v: 'a', label: 'V(D)' }] });
// diode reverse recovery: +5 V → −5 V through 50 Ω (TT charge storage, junction capacitance)
for (const [m, dt, t1] of [['1N4148', 0.05e-9, 100e-9], ['1N4007', 0.5e-9, 1e-6]]) {
  T.push({ name: 'rr_' + m, title: `${m} reverse recovery`, dt, tt: t1 * 3, kind: '反向恢復（TT、CJO）',
    parts: [['V', 1, ['in', 0], [[0, 5], [t1, 5], [t1 + 2e-9, -5], [10, -5]]], ['R', 1, ['in', 'a'], 50], ['D', 1, ['k', 0], m]],
    ammeters: [['ID', 'a', 'k']], signals: [{ i: 'ID', label: 'I(D)' }, { v: 'a', label: 'V(D)' }] });
}
// BJT output characteristic: base fed from 5 V through Rb, Vce ramp 0 → 10 V
for (const [m, rb, vc] of [['2N3904', 43e3, 10], ['2N4401', 4.3e3, 10], ['2N2222A', 4.3e3, 10]]) {
  T.push({ name: 'out_' + m, title: `${m} output curve`, dt: 1e-4, tt: 1, kind: '輸出特性 Ic–Vce（Early、飽和）',
    parts: [['V', 1, ['vb', 0], [[0, 5], [1, 5]]], ['R', 1, ['vb', 'b'], rb], ['V', 2, ['vc', 0], [[0, 0], [1, vc]]], ['Q', 1, ['c', 0, 'b'], m]],
    ammeters: [['IC', 'vc', 'c']], signals: [{ i: 'IC', label: 'Ic' }, { v: 'b', label: 'Vbe' }] });
}
// BJT Gummel: Vbe ramp 0.4 → 0.95 V at Vce = 5 V (IS, NE/ISE, IKF high injection)
for (const m of ['2N3904', '2N4401']) {
  T.push({ name: 'gum_' + m, title: `${m} gummel`, dt: 1e-4, tt: 1, kind: 'Gummel 曲線（Ib、Ic 對 Vbe）',
    parts: [['V', 1, ['vb', 0], [[0, 0.4], [1, 0.95]]], ['V', 2, ['vc', 0], [[0, 5], [1, 5]]], ['Q', 1, ['c', 0, 'b'], m]],
    ammeters: [['IC', 'vc', 'c'], ['IB', 'vb', 'b']], signals: [{ i: 'IC', label: 'Ic' }, { i: 'IB', label: 'Ib' }] });
}
// PNP output curve (signs reversed)
T.push({ name: 'out_2N3906', title: '2N3906 output', dt: 1e-4, tt: 1, kind: '輸出特性（PNP）',
  parts: [['V', 1, ['vb', 0], [[0, -5], [1, -5]]], ['R', 1, ['vb', 'b'], 43e3], ['V', 2, ['vc', 0], [[0, 0], [1, -10]]], ['Q', 1, ['c', 0, 'b'], '2N3906']],
  ammeters: [['IC', 'vc', 'c']], signals: [{ i: 'IC', label: 'Ic' }, { v: 'b', label: 'Vbe' }] });
// BJT switching: 1 kΩ load at 10 V, base pulse 0 → 5 V through 1 kΩ (TF, TR, CJE, CJC, RB)
for (const m of ['2N4401', '2N2222A']) {
  T.push({ name: 'sw_' + m, title: `${m} switching`, dt: 0.1e-9, tt: 2e-6, kind: '開關暫態（儲存時間、Cjc/Cje）',
    parts: [['V', 1, ['vb', 0], [[0, 0], [100e-9, 0], [105e-9, 5], [1e-6, 5], [1.005e-6, 0], [10, 0]]], ['R', 1, ['vb', 'b'], 1e3], ['V', 2, ['vcc', 0], [[0, 10], [1, 10]]], ['R', 2, ['vcc', 'c'], 1e3], ['Q', 1, ['c', 0, 'b'], m]],
    signals: [{ v: 'c', label: 'Vce' }, { v: 'b', label: 'Vbe' }] });
}
// MOSFET level 1: output curve at Vgs = 5 V and transfer curve at Vds = 10 V; PMOS output
T.push({ name: 'mos_out', title: 'NMOS output', dt: 1e-4, tt: 1, kind: 'Level-1 輸出特性',
  parts: [['V', 1, ['g', 0], [[0, 5], [1, 5]]], ['V', 2, ['vd', 0], [[0, 0], [1, 10]]], ['M', 1, ['d', 0, 'g'], 'GENERIC_N']],
  ammeters: [['ID', 'vd', 'd']], signals: [{ i: 'ID', label: 'Id' }] });
T.push({ name: 'mos_tr', title: 'NMOS transfer', dt: 1e-4, tt: 1, kind: 'Level-1 轉移特性',
  parts: [['V', 1, ['g', 0], [[0, 0], [1, 8]]], ['V', 2, ['vd', 0], [[0, 10], [1, 10]]], ['M', 1, ['d', 0, 'g'], 'GENERIC_N']],
  ammeters: [['ID', 'vd', 'd']], signals: [{ i: 'ID', label: 'Id' }] });
T.push({ name: 'pmos_out', title: 'PMOS output', dt: 1e-4, tt: 1, kind: 'Level-1 輸出特性（PMOS）',
  parts: [['V', 1, ['g', 0], [[0, -5], [1, -5]]], ['V', 2, ['vd', 0], [[0, 0], [1, -10]]], ['M', 1, ['d', 0, 'g'], 'GENERIC_P']],
  ammeters: [['ID', 'vd', 'd']], signals: [{ i: 'ID', label: 'Id' }] });
// MOSFET switching with lumped capacitances (Miller effect)
T.push({ name: 'mos_sw', title: 'NMOS switching', dt: 0.1e-9, tt: 400e-9, kind: '開關暫態（Cgs、Cgd、Cds）',
  parts: [['V', 1, ['vg', 0], [[0, 0], [20e-9, 0], [22e-9, 10], [200e-9, 10], [202e-9, 0], [1, 0]]], ['R', 1, ['vg', 'g'], 50], ['V', 2, ['vdd', 0], [[0, 20], [1, 20]]], ['R', 2, ['vdd', 'd'], 20], ['M', 1, ['d', 0, 'g'], 'GENERIC_N']],
  signals: [{ v: 'd', label: 'Vds' }, { v: 'g', label: 'Vgs' }] });

let worst = 0;
const rows = [];
for (const c of T) {
  let r;
  try { r = run(c); } catch (e) { console.log(`${c.name}: ERROR ${String(e.message).split('\n')[0]}`); worst = Infinity; continue; }
  for (const x of r) {
    worst = Math.max(worst, x.rel);
    rows.push(x);
    console.log(`${c.name.padEnd(13)} ${c.kind.padEnd(18)} ${x.sig.padEnd(6)} 相對誤差 ${x.rel.toExponential(2)}（最大差 ${x.emax.toExponential(2)}，滿刻度 ${x.scale.toExponential(2)}，@${x.at.toExponential(3)} s）`);
  }
}
console.log(`WORST ${worst.toExponential(2)}`);
