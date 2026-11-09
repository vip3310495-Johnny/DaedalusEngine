// Behavioural device models measured with datasheet test circuits in DaedalusEngine:
// each model must reproduce the datasheet values it was parameterised with.
// usage: node tools/behav_check.js [daedalus-cli.exe] [workdir]
const fs = require('fs');
const path = require('path');
const { execFileSync } = require('child_process');
const CLI = path.resolve(process.argv[2] || 'target/release/daedalus-cli.exe');
const WD = process.argv[3] || 'tests/behav';
fs.mkdirSync(WD, { recursive: true });

// tiny netlist builder: nodes by name ('0' = ground); control nets marked with a leading '~'
function run(name, parts, sc) {
  const ids = new Map([['0', 0]]), kinds = ['G'];
  const id = n => { if (!ids.has(n)) { ids.set(n, kinds.length); kinds.push(n.startsWith('~') ? 'C' : 'P'); } return ids.get(n); };
  const elements = parts.map(([type, nm, params, pins], i) => ({ id: i + 1, type, name: nm, params, pins: pins.map(id) }));
  fs.writeFileSync(path.join(WD, name + '.json'), JSON.stringify({ net: { nets: kinds.map(k => ({ kind: k })), elements, paramText: '' }, sc: Object.assign({ tPrint: 0, printStep: 1, interp: true, damp: true, lte: true, lteTol: 1e-3 }, sc), ac: null }));
  execFileSync(CLI, [path.join(WD, name + '.json'), path.join(WD, name + '.csv')]);
  const L = fs.readFileSync(path.join(WD, name + '.csv'), 'utf8').trim().split(/\r?\n/);
  const h = L[0].split(',');
  const c = {};
  h.forEach(k => (c[k] = []));
  L.slice(1).forEach(l => l.split(',').forEach((v, i) => c[h[i]].push(+v)));
  return c;
}
const at = (c, k, t) => { const i = c.Time.findIndex(x => x >= t); return c[k][i]; };
const whereFirst = (c, k, pred) => { const i = c[k].findIndex(pred); return i < 0 ? NaN : c.Time[i]; };
const rows = [];
const row = (part, item, cond, spec, sim, ok, note = '') => rows.push({ part, item, cond, spec, sim, ok, note });
const f4 = x => (Math.abs(x) >= 1e4 || (Math.abs(x) < 1e-3 && x !== 0) ? x.toExponential(3) : (+x.toPrecision(4)).toString());
const near = (a, b, r) => Math.abs(a - b) <= r * Math.abs(b);

// ---------------- TL431 (REF = K), cathode current ramp 0 → 20 mA ----------------
{
  const c = run('tl431', [
    ['IPWL', 'IK', { f: '0', vals: '0 20m', times: '0 1' }, ['k', '0']],
    ['TL431', 'U1', { vref: '2.495', iref: '2u', imin: '0.4m', zka: '0.2', kvk: '-1.4m', fp: '4.5k', zhf: '10' }, ['k', '0', 'k']],
    ['VP', 'Vk', {}, ['k']],
  ], { dt: 1e-4, tTotal: 1 });
  const v = i => at(c, 'Vk', i / 20e-3);
  const vr = v(10e-3);
  row('TL431', 'Vref', 'REF = K, IKA = 10 mA', '2.495 V（TI 典型）', vr, near(vr, 2.495, 1e-3));
  const zka = (v(15e-3) - v(5e-3)) / 10e-3;
  row('TL431', '|Zka|', 'IKA 5 → 15 mA', '0.2 Ω（典型）', zka, near(zka, 0.2, 0.15));
  const d1 = vr - v(1e-3), d04 = vr - v(0.4e-3);
  row('TL431', 'Vref 下降', 'IKA = 1 mA（Imin 上限）', '< 10 mV', d1, d1 < 10e-3, `0.4 mA 時下降 ${f4(d04 * 1e3)} mV`);
}

// ---------------- LM393 (COMPOC), V+ = 5 V ----------------
const cmp = { tpd: '1.3u', isc: '16m', vol: '0.15', iol: '4m', vos: '0', ib: '25n', vcmr: '1.5', vsmin: '2', iq: '0.2m' };
{
  // output low: IN+ = 1 V < IN− = 2 V; pull-up sized for 4 mA
  const c = run('cmp_vol', [
    ['VDC', 'Vcc', { V: '5' }, ['vcc', '0']], ['VDC', 'Vp', { V: '1' }, ['ip', '0']], ['VDC', 'Vn', { V: '2' }, ['in', '0']],
    ['COMPOC', 'U1', cmp, ['ip', 'in', 'o', 'vcc', '0']],
    ['R', 'RL', { R: String((5 - 0.15) / 4e-3) }, ['vcc', 'o']], ['VP', 'Vo', {}, ['o']],
  ], { dt: 1e-6, tTotal: 50e-6 });
  const vo = c.Vo[c.Vo.length - 1], io = (5 - vo) / ((5 - 0.15) / 4e-3);
  row('LM393', 'VOL', `ISINK = ${f4(io * 1e3)} mA`, '0.15 V @ 4 mA（典型）', vo, near(vo, 0.15, 0.03));
  const s = run('cmp_isc', [
    ['VDC', 'Vcc', { V: '5' }, ['vcc', '0']], ['VDC', 'Vp', { V: '1' }, ['ip', '0']], ['VDC', 'Vn', { V: '2' }, ['in', '0']],
    ['COMPOC', 'U1', cmp, ['ip', 'in', 'o', 'vcc', '0']],
    ['IP', 'Io', {}, ['vcc', 'o']],
  ], { dt: 1e-6, tTotal: 50e-6 });
  const isc = s.Io[s.Io.length - 1];
  row('LM393', 'ISINK', 'VOUT = 5 V（短路）', '16 mA（典型）', isc, near(isc, 16e-3, 0.05));
  // response time: IN+ steps 1.1 → 0.9 V against IN− = 1 V; RL = 5.1 kΩ to 5 V; delay to 50 %
  const t = run('cmp_tpd', [
    ['VDC', 'Vcc', { V: '5' }, ['vcc', '0']], ['VPWL', 'Vp', { f: '0', vals: '1.1 1.1 0.9 0.9', times: '0 10u 10.001u 1' }, ['ip', '0']], ['VDC', 'Vn', { V: '1' }, ['in', '0']],
    ['COMPOC', 'U1', cmp, ['ip', 'in', 'o', 'vcc', '0']],
    ['R', 'RL', { R: '5.1k' }, ['vcc', 'o']], ['VP', 'Vo', {}, ['o']],
  ], { dt: 1e-9, tTotal: 20e-6 });
  const td = whereFirst(t, 'Vo', (v, i) => t.Time[i] > 10e-6 && v < 2.5) - 10.0005e-6;
  row('LM393', 'tpd', '100 mV 步階，RL = 5.1 kΩ，至 50 %', '1.3 µs（典型）', td, near(td, 1.3e-6, 0.1));
}

// ---------------- optocoupler (PC817 class) ----------------
{
  const opto = { ctr: '100', vf: '1.2', ifv: '20m', bf: '400', ccb: '5p', tf: '5.5n', ifn: '5m' };
  for (const [ifw, vce] of [[5e-3, 5], [20e-3, 5]]) {
    const c = run('opto_' + ifw, [
      ['IPWL', 'IF', { f: '0', vals: `${ifw} ${ifw}`, times: '0 1' }, ['a', '0']],
      ['OPTO1', 'U1', opto, ['a', '0', 'c', '0']],
      ['VDC', 'Vc', { V: String(vce) }, ['vc', '0']], ['IP', 'Ic', {}, ['vc', 'c']], ['VP', 'Va', {}, ['a']],
    ], { dt: 1e-6, tTotal: 200e-6 });
    const ic = c.Ic[c.Ic.length - 1], va = c.Va[c.Va.length - 1];
    if (ifw === 5e-3) row('PC817 類', 'CTR', 'IF = 5 mA, VCE = 5 V', '100 %（參數）', ic / ifw * 100, near(ic / ifw * 100, 100, 0.03));
    else row('PC817 類', 'VF', 'IF = 20 mA', '1.2 V（參數）', va, near(va, 1.2, 0.01));
  }
}

// ---------------- non-ideal MOSFET (datasheet parameters) ----------------
const mosni = { ron: '50m', vth: '3', ciss: '1.5n', crss: '30p', coss: '150p', von: '12', voff: '0', rg: '5', qgd: '15n', vq: '400', vcap: '25', vfd: '0.85', ifd: '5', ttd: '50n', lam: '0' };
{
  // Rds(on): gate driven high, Vds = 0.1 V
  const c = run('mos_ron', [
    ['GATING', 'G1', { f: '0', n: '1', pts: '0' }, ['~g']],
    ['MOSNI', 'Q1', mosni, ['d', '0', '~g']],
    ['VDC', 'Vd', { V: '0.1' }, ['vd', '0']], ['IP', 'Id', {}, ['vd', 'd']],
  ], { dt: 1e-7, tTotal: 20e-6 });
  const r = 0.1 / c.Id[c.Id.length - 1];
  row('MOSFET（非理想）', 'Rds(on)', 'Vgs = 12 V, Vds = 0.1 V', '50 mΩ（參數）', r, near(r, 0.05, 0.03));
  // Coss at Vcap: gate low, Vds ramp 0 → 50 V in 1 ms; C = I / (dV/dt)
  const k = run('mos_coss', [
    ['GATING', 'G1', { f: '0', n: '1', pts: '1' }, ['~g']],
    ['MOSNI', 'Q1', mosni, ['d', '0', '~g']],
    ['VPWL', 'Vd', { f: '0', vals: '0 50', times: '0 1m' }, ['vd', '0']], ['IP', 'Id', {}, ['vd', 'd']],
  ], { dt: 1e-7, tTotal: 1e-3 });
  const cos = at(k, 'Id', 0.5e-3) / 5e4;
  row('MOSFET（非理想）', 'Coss', 'Vds = 25 V（Vcap）', '150 pF（參數）', cos, near(cos, 150e-12, 0.05));
  // body diode: 5 A source → drain with the gate low
  const b = run('mos_body', [
    ['GATING', 'G1', { f: '0', n: '1', pts: '1' }, ['~g']],
    ['MOSNI', 'Q1', mosni, ['d', '0', '~g']],
    ['IPWL', 'If', { f: '0', vals: '5 5', times: '0 1' }, ['0', 'd']], ['VP', 'Vd', {}, ['d']],
  ], { dt: 1e-7, tTotal: 20e-6 });
  const vsd = -b.Vd[b.Vd.length - 1];
  row('MOSFET（非理想）', '本體二極體 VSD', 'IS = 5 A', '0.85 V（參數）', vsd, near(vsd, 0.85, 0.02));
}

// ---------------- datasheet-parameter diode (model = ds) ----------------
{
  const dsd = { model: 'ds', vfd: '1.5', ifd: '20', rd: '15m', trr: '0', qrr: '400n', irr: '20', cjd: '60p', vcj: '100', bvd: '650' };
  const c = run('dsd_vf', [['IPWL', 'If', { f: '0', vals: '20 20', times: '0 1' }, ['a', '0']], ['DIODE', 'D1', dsd, ['a', '0']], ['VP', 'Va', {}, ['a']]], { dt: 1e-7, tTotal: 10e-6 });
  const v = c.Va[c.Va.length - 1];
  row('二極體（資料手冊參數）', 'VF', 'IF = 20 A', '1.5 V（參數）', v, near(v, 1.5, 0.01));
  const k = run('dsd_cj', [['VPWL', 'Vr', { f: '0', vals: '0 -200', times: '0 1m' }, ['a', '0']], ['IP', 'Ir', {}, ['a', 'x']], ['DIODE', 'D1', dsd, ['x', '0']]], { dt: 1e-7, tTotal: 1e-3 });
  const cj = -at(k, 'Ir', 0.5e-3) / 2e5;
  row('二極體（資料手冊參數）', 'Cj', 'VR = 100 V', '60 pF（參數）', cj, near(cj, 60e-12, 0.05));
  // charge control (SPICE TT = Qrr/IF): abrupt reversal to −IR, storage phase lasts
  // ts = TT·ln(1 + Q0/(IR·TT)) with Q0 = TT·IF, so the charge extracted during it is IR·ts
  const TT = 400e-9 / 20, IR = 20;
  const q = run('dsd_qrr', [
    ['IPWL', 'If', { f: '0', vals: '20 20 -20 -20', times: '0 5u 5.0005u 1' }, ['a', '0']],
    ['IP', 'Id', {}, ['a', 'x']], ['DIODE', 'D1', dsd, ['x', '0']], ['VP', 'Va', {}, ['a']],
  ], { dt: 0.05e-9, tTotal: 5.1e-6 });
  // end of the storage phase: the junction leaves forward bias (voltage falls through 0)
  const tsEnd = whereFirst(q, 'Va', (v, i) => q.Time[i] > 5e-6 && v < 0);
  const ts = tsEnd - 5.00025e-6;
  const tsTh = TT * Math.log(1 + (TT * 20) / (IR * TT));
  row('二極體（資料手冊參數）', '儲存時間 ts', 'IF = 20 A → IR = 20 A（電荷控制）', `TT·ln2 = ${f4(tsTh * 1e9)} ns`, ts, near(ts, tsTh, 0.05), '模型把 Qrr 當作儲存電荷 TT·IF；有限 di/dt 下實際抽出電荷較少（見報告）');
}

// Qrr test at the datasheet di/dt: IF = 20 A, ramp down at 200 A/µs, reverse voltage clamped at −50 V.
// With di/dt given, TT is calibrated so that the extracted charge equals Qrr.
for (const didt of ['200', '0']) {
  const dsd = { model: 'ds', vfd: '1.5', ifd: '20', rd: '15m', trr: '0', qrr: '400n', irr: '20', didt, cjd: '60p', vcj: '100', bvd: '650' };
  const q = run('dsd_qrr_' + didt, [
    ['IPWL', 'If', { f: '0', vals: '20 20 -60 -60', times: '0 5u 5.4u 1' }, ['a', '0']],
    ['IP', 'Id', {}, ['a', 'x']], ['DIODE', 'D1', dsd, ['x', '0']],
    ['VDC', 'Vcl', { V: '-50' }, ['cl', '0']], ['DIODE', 'Dc', { model: 'ideal', vf: '0' }, ['cl', 'a']],
  ], { dt: 0.1e-9, tTotal: 5.6e-6 });
  let qr = 0;
  for (let i = 1; i < q.Time.length; i++) if (q.Time[i] > 5e-6 && q.Id[i] < 0) qr += -q.Id[i] * (q.Time[i] - q.Time[i - 1]);
  if (didt !== '0') row('二極體（資料手冊參數）', 'Qrr', 'IF = 20 A，di/dt = 200 A/µs', '400 nC（參數）', qr, near(qr, 400e-9, 0.08), '含接面電容充電約數 nC');
  else row('二極體（資料手冊參數）', 'Qrr（未填 di/dt）', 'IF = 20 A，di/dt = 200 A/µs', '舊算法（參考）', qr, true, '未校正時抽出電荷遠小於 Qrr');
}

let bad = 0;
for (const r of rows) {
  if (!r.ok) bad++;
  console.log(`${r.ok ? ' OK ' : 'FAIL'} | ${r.part.padEnd(10)} | ${r.item.padEnd(10)} | ${r.cond.padEnd(30)} | 規格 ${r.spec.padEnd(18)} | 模型 ${f4(r.sim)}${r.note ? ' | ' + r.note : ''}`);
}
console.log(`${rows.length - bad}/${rows.length} reproduce their datasheet values`);
