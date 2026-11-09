// Device presets against manufacturer datasheet limits (25 °C).
// SPICE presets: DC points with ngspice (identical to DaedalusEngine to ~1e-7, see spice_check.js);
// dynamic and behavioural models: DaedalusEngine itself.
// usage: node tools/datasheet_check.js [ngspice_con.exe] [daedalus-cli.exe] [workdir]
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const NG = path.resolve(process.argv[2] || 'tools/ngspice/Spice64/bin/ngspice_con.exe');
const CLI = path.resolve(process.argv[3] || 'target/release/daedalus-cli.exe');
const WD = process.argv[4] || 'tests/datasheet';
fs.mkdirSync(WD, { recursive: true });
const CARDS = {
  '1N4148': 'D IS=2.52n RS=0.568 N=1.752 CJO=4p M=0.4 TT=20n BV=100 IBV=100u',
  '1N4007': 'D IS=7.02767n RS=0.0341512 N=1.80803 BV=1100 IBV=0.1 CJO=39p VJ=0.7 M=0.5 FC=0.5 TT=100n',
  '1N5819': 'D IS=31.7u RS=0.051 N=1.373 CJO=259p M=0.35 VJ=0.38 BV=40 IBV=1m',
  '2N3904': 'NPN IS=6.734f VAF=74.03 BF=416.4 NE=1.259 ISE=6.734f IKF=66.78m BR=.7371 NC=2 ISC=0 IKR=0 RC=1 CJC=3.638p MJC=.3085 VJC=.75 FC=.5 CJE=4.493p MJE=.2593 VJE=.75 TR=239.5n TF=301.2p RB=10',
  '2N4401': 'NPN IS=26.03f VAF=90.7 BF=4.292K NE=1.244 ISE=26.03f IKF=.2061 BR=1.01 NC=2 ISC=0 IKR=0 RC=.5 CJC=11.01p MJC=.3763 VJC=.75 FC=.5 CJE=24.07p MJE=.3641 VJE=.75 TR=233.7n TF=466.5p RB=4.7',
  '2N3906': 'PNP IS=1.41f VAF=18.7 BF=180.7 NE=1.5 ISE=0 IKF=80m BR=4.977 NC=2 ISC=0 IKR=0 RC=2.5 CJC=9.728p MJC=.5776 VJC=.75 FC=.5 CJE=8.063p MJE=.3677 VJE=.75 TR=33.42n TF=179.3p RB=10',
};
let seq = 0;
function ng(lines, vecs) {
  const name = 'ds' + seq++;
  fs.writeFileSync(path.join(WD, name + '.cir'), [name, ...lines, '.control', 'set wr_singlescale', 'option numdgt=10', 'run', `wrdata ${name}.txt ${vecs.join(' ')}`, '.endc', '.end'].join('\n'));
  execFileSync(NG, ['-b', name + '.cir'], { cwd: WD, stdio: ['ignore', 'pipe', 'pipe'] });
  return fs.readFileSync(path.join(WD, name + '.txt'), 'utf8').trim().split(/\r?\n/).map(l => l.trim().split(/\s+/).map(Number));
}
const rows = [];
const row = (part, item, cond, lim, sim, ok, src) => rows.push({ part, item, cond, lim, sim, ok, src });
const f3 = x => (Math.abs(x) >= 1e4 || (Math.abs(x) < 1e-3 && x !== 0) ? x.toExponential(3) : (+x.toPrecision(4)).toString());
const VISHAY_4148 = 'Vishay 1N4148 (doc 81857)', VISHAY_5819 = 'Vishay 1N5817-19 (doc 88525)', VISHAY_4007 = 'Vishay 1N4001-07 (doc 88503)';
const ON = p => `onsemi ${p} datasheet`;

// ---------- diodes: VF at forced current ----------
function vf(model, i) {
  const r = ng([`I1 0 a DC ${i}`, `D1 a 0 M`, `.model M ${CARDS[model]}`, '.op'], ['v(a)']);
  return r[0][1];
}
const dv = vf('1N4148', 10e-3);
row('1N4148', 'VF', 'IF = 10 mA', '≤ 1.0 V', dv, dv <= 1.0, VISHAY_4148);
const cd0 = 4e-12;
row('1N4148', 'CD', 'VR = 0 V', '≤ 4 pF', cd0, cd0 <= 4e-12, VISHAY_4148);
const ir = -ng([`V1 a 0 DC -20`, `D1 a 0 M`, `.model M ${CARDS['1N4148']}`, '.op'], ['i(V1)'])[0][1];
row('1N4148', 'IR', 'VR = 20 V', '≤ 25 nA', Math.abs(ir), Math.abs(ir) <= 25e-9, VISHAY_4148);
const bvI = ng([`I1 0 a DC 100u`, `D1 0 a M`, `.model M ${CARDS['1N4148']}`, '.op'], ['v(a)'])[0][1];
row('1N4148', 'V(BR)', 'IR = 100 µA', '≥ 100 V', Math.abs(bvI), Math.abs(bvI) >= 99.9, VISHAY_4148);
for (const [i, lim] of [[1, 1.1]]) { const v = vf('1N4007', i); row('1N4007', 'VF', `IF = ${i} A`, `≤ ${lim} V`, v, v <= lim, VISHAY_4007); }
const ir7 = Math.abs(ng([`V1 a 0 DC -1000`, `D1 a 0 M`, `.model M ${CARDS['1N4007']}`, '.op'], ['i(V1)'])[0][1]);
row('1N4007', 'IR', 'VR = 1000 V', '≤ 5 µA', ir7, ir7 <= 5e-6, VISHAY_4007);
const cj = (c0, vj, m, v) => c0 / Math.pow(1 + v / vj, m);
const cj7 = cj(39e-12, 0.7, 0.5, 4);
row('1N4007', 'CJ', 'VR = 4 V', '典型 15 pF', cj7, cj7 > 7.5e-12 && cj7 < 30e-12, VISHAY_4007);
for (const [i, lim] of [[1, 0.6], [3.1, 0.9]]) { const v = vf('1N5819', i); row('1N5819', 'VF', `IF = ${i} A`, `≤ ${lim} V`, v, v <= lim, VISHAY_5819); }
const cj9 = cj(259e-12, 0.38, 0.35, 4);
row('1N5819', 'CJ', 'VR = 4 V', '典型 110 pF', cj9, cj9 > 55e-12 && cj9 < 220e-12, VISHAY_5819);

// 1N4148 trr: IF = 10 mA, then VR = 6 V through RL = 100 Ω, measured to iR = 0.1·IR (DaedalusEngine transient)
{
  const vfw = 10e-3 * 100 + dv;
  const net = { nets: [{ kind: 'G' }, { kind: 'P' }, { kind: 'P' }, { kind: 'P' }], elements: [
    { id: 1, type: 'VPWL', name: 'V1', params: { f: '0', vals: `${vfw} ${vfw} -6 -6`, times: '0 50n 50.1n 1' }, pins: [1, 0] },
    { id: 2, type: 'R', name: 'RL', params: { R: '100' }, pins: [1, 2] },
    { id: 3, type: 'IP', name: 'ID', params: {}, pins: [2, 3] },
    { id: 4, type: 'DIODE', name: 'D1', params: { model: '1N4148' }, pins: [3, 0] },
  ], paramText: '' };
  fs.writeFileSync(path.join(WD, 'trr.json'), JSON.stringify({ net, sc: { dt: 5e-12, tTotal: 100e-9, tPrint: 0, printStep: 1, interp: true, damp: true, lte: true, lteTol: 1e-3 }, ac: null }));
  execFileSync(CLI, [path.join(WD, 'trr.json'), path.join(WD, 'trr.csv')]);
  const L = fs.readFileSync(path.join(WD, 'trr.csv'), 'utf8').trim().split(/\r?\n/).slice(1).map(l => l.split(',').map(Number));
  const t0 = L.find(r => r[0] > 50e-9 && r[1] < 0)[0];
  const irPk = -Math.min(...L.map(r => r[1]));
  const t1 = L.find(r => r[0] > t0 && r[1] < -0.5 * irPk && false) || null;
  let tEnd = NaN;
  let seenPeak = false;
  for (const r of L) { if (r[0] <= t0) continue; if (-r[1] >= 0.99 * irPk) seenPeak = true; if (seenPeak && -r[1] <= 0.1 * irPk) { tEnd = r[0]; break; } }
  const trr = tEnd - t0;
  row('1N4148', 'trr', 'IF=10mA→VR=6V, RL=100Ω, 至 0.1·IR', '≤ 4 ns', trr, trr <= 4e-9, VISHAY_4148);
}

// ---------- BJTs: hFE at VCE = 1 V (Vbe sweep), saturation at forced currents ----------
function hfe(model, pnp) {
  const s = pnp ? -1 : 1;
  const r = ng([`VB b 0 DC 0`, `VC c 0 DC ${s * 1}`, `Q1 c b 0 M`, `.model M ${CARDS[model]}`, `.dc VB ${s * 0.3} ${s * 1.05} ${s * 0.0002}`], ['i(VC)', 'i(VB)']);
  return ic => {
    for (let k = 1; k < r.length; k++) {
      const a = -s * r[k - 1][1], b = -s * r[k][1];
      if (a < ic && b >= ic) { const f = (ic - a) / (b - a); const ib = -s * (r[k - 1][2] + f * (r[k][2] - r[k - 1][2])); return ic / ib; }
    }
    return NaN;
  };
}
function sat(model, ic, ib, pnp) {
  const s = pnp ? -1 : 1;
  const r = ng([`IC 0 c DC ${s * ic}`, `IB 0 b DC ${s * ib}`, `Q1 c b 0 M`, `.model M ${CARDS[model]}`, '.op'], ['v(c)', 'v(b)']);
  return [s * r[0][1], s * r[0][2]];
}
const HFE = {
  '2N3904': [[0.1e-3, 40], [1e-3, 70], [10e-3, 100, 300], [50e-3, 60], [100e-3, 30]],
  '2N4401': [[0.1e-3, 20], [1e-3, 40], [10e-3, 80], [150e-3, 100, 300]],
  '2N3906': [[0.1e-3, 60], [1e-3, 80], [10e-3, 100, 300], [50e-3, 60], [100e-3, 30]],
};
for (const [m, pts] of Object.entries(HFE)) {
  const pnp = m === '2N3906';
  const h = hfe(m, pnp);
  for (const [ic, mn, mx] of pts) {
    const v = h(ic);
    row(m, 'hFE', `IC = ${f3(ic * 1e3)} mA, VCE = 1 V`, mx ? `${mn} ~ ${mx}` : `≥ ${mn}`, v, v >= mn && (!mx || v <= mx), ON(m));
  }
}
const SAT = {
  '2N3904': [[10e-3, 1e-3, 0.2, 0.65, 0.85], [50e-3, 5e-3, 0.3, 0, 0.95]],
  '2N4401': [[150e-3, 15e-3, 0.4, 0.75, 0.95], [500e-3, 50e-3, 0.75, 0, 1.2]],
  '2N3906': [[10e-3, 1e-3, 0.25, 0.65, 0.85], [50e-3, 5e-3, 0.4, 0, 0.95]],
};
for (const [m, pts] of Object.entries(SAT)) {
  for (const [ic, ib, vcemax, vbemin, vbemax] of pts) {
    const [vce, vbe] = sat(m, ic, ib, m === '2N3906');
    row(m, 'VCE(sat)', `IC/IB = ${ic * 1e3}/${ib * 1e3} mA`, `≤ ${vcemax} V`, vce, vce <= vcemax, ON(m));
    row(m, 'VBE(sat)', `IC/IB = ${ic * 1e3}/${ib * 1e3} mA`, vbemin ? `${vbemin} ~ ${vbemax} V` : `≤ ${vbemax} V`, vbe, vbe >= vbemin && vbe <= vbemax, ON(m));
  }
}

// ---------- report ----------
let bad = 0;
for (const r of rows) {
  if (!r.ok) bad++;
  console.log(`${r.ok ? ' OK ' : 'FAIL'} | ${r.part.padEnd(7)} | ${r.item.padEnd(8)} | ${r.cond.padEnd(30)} | 規格 ${String(r.lim).padEnd(12)} | 模型 ${f3(r.sim)} | ${r.src}`);
}
console.log(`${rows.length - bad}/${rows.length} within datasheet limits`);
