// Regression circuits covering every element type, as netlists {net, sc, ac} for both engines.
const fs = require('fs'); const path = require('path');
const LIB = require('../crates/daedalus-core/assets/library.json');
const DEF = {}; LIB.forEach(d => DEF[d.type] = d);
class TB {
  constructor(name, sc) { this.name = name; this.sc = Object.assign({ dt: 1e-6, tTotal: 5e-3, tPrint: 0, printStep: 1, interp: true, damp: true, lte: true, lteTol: 1e-3 }, sc || {}); this.nets = [{ kind: 'G' }]; this.els = []; this.id = 1; this.names = {}; }
  p() { this.nets.push({ kind: 'P' }); return this.nets.length - 1; }
  c() { this.nets.push({ kind: 'C' }); return this.nets.length - 1; }
  el(type, pins, params, name) {
    const d = DEF[type]; if (!d) throw new Error('unknown ' + type);
    const prm = {}; d.params.forEach(p => prm[p.k] = p.d); Object.assign(prm, params || {});
    if (!name) { let i = 1; while (this.names[d.pre + i]) i++; name = d.pre + i; }
    this.names[name] = 1;
    this.els.push({ id: this.id++, type, name, params: prm, pins }); return name;
  }
  vp(n, name) { return this.el('VP', [n], {}, name); }
  // load resistor + current probe in series from node o to ground
  save(dir) { fs.writeFileSync(path.join(dir, this.name + '.json'), JSON.stringify({ net: { nets: this.nets, elements: this.els, paramText: this.paramText || '' }, sc: this.sc, ac: this.ac || null }, null, 1)); }
}
const out = process.argv[2]; const all = [];
const G = 0;

// ---------------------------------------------------------------- RLC, transformers, coupled L, nonlinear, op-amps
{
  const t = new TB('rlc_zoo', { dt: 2e-6, tTotal: 4e-3 });
  const s = t.p(); t.el('VSIN', [s, G], { Vm: '10', f: '1k' }, 'Vs');
  const a = t.p(); t.el('R', [s, a], { R: '10' }); t.el('C', [a, G], { C: '1u', v0: '1' }); t.vp(a, 'Vc');
  const c = t.p(); t.el('IP', [s, c], {}, 'IL'); t.el('L', [c, G], { L: '10m', i0: '0.1' });
  for (const [ty, nm] of [['RL', 'Irl'], ['RC', 'Irc'], ['LC', 'Ilc'], ['RLC', 'Irlc']]) { const m = t.p(); t.el('IP', [s, m], {}, nm); t.el(ty, [m, G], { R: '5', L: '2m', C: '2u' }); }
  const r1 = t.p(); t.el('RHEO', [s, G, r1], { R: '100', tap: '0.3' }); t.vp(r1, 'Vtap');
  const ls = t.p(), ls2 = t.p(); t.el('IP', [s, ls], {}, 'Isat'); t.el('LSAT', [ls, ls2], { tab: '0.1 20m 0.3 10m 1 2m' }); t.el('R', [ls2, G], { R: '20' });
  const n1 = t.p(), n2 = t.p(); t.el('VDC', [n1, G], { V: '0.8' }); t.el('R', [n1, n2], { R: '1' }); t.el('NONI', [n2, G], { hi: '1' }); t.vp(n2, 'Vnoni');
  const v1 = t.p(); t.el('IP', [s, v1], {}, 'Inonv'); t.el('NONV', [v1, G], {});
  const ts = t.p(); t.el('TFI', [s, G, ts, G], { np: '1', ns: '2' }); t.el('R', [ts, G], { R: '100' }); t.vp(ts, 'Vtfi');
  const t1 = t.p(); t.el('TF1', [s, G, t1, G], { rp: '0.1', rs: '0.2', lp: '10u', ls: '10u', lm: '5m', np: '2', ns: '1' }); t.el('R', [t1, G], { R: '10' }); t.vp(t1, 'Vtf1');
  const w2 = t.p(), w3 = t.p(); t.el('TF3W', [s, G, w2, G, w3, G], { lm: '5m', np: '1', ns: '1', nt: '0.5', rs: '0.1' }); t.el('R', [w2, G], { R: '50' }); t.el('R', [w3, G], { R: '20' }); t.vp(w2, 'Vw2'); t.vp(w3, 'Vw3');
  const m1 = t.p(), m2 = t.p(); t.el('IP', [s, m1], {}, 'Imut'); t.el('MUT2', [m1, G, m2, G], {}); t.el('R', [m2, G], { R: '5' }); t.vp(m2, 'Vmut2');
  const k1 = t.p(), k2 = t.p(), k3 = t.p(); t.el('IP', [s, k1], {}, 'Imut3'); t.el('MUT3', [k1, G, k2, G, k3, G], {}); t.el('R', [k2, G], { R: '5' }); t.el('R', [k3, G], { R: '8' }); t.vp(k3, 'Vmut3');
  const dv = t.p(); t.el('DVDT', [a, dv], {}); t.el('R', [dv, G], { R: '1k' }); t.vp(dv, 'Vdvdt');
  const om = t.p(), oo = t.p(); t.el('R', [s, om], { R: '1k' }); t.el('R', [om, oo], { R: '4k' }); t.el('OPAMP', [om, G, oo], {}); t.vp(oo, 'Vopa');
  const om2 = t.p(), oo2 = t.p(), og = t.p(); t.el('R', [s, om2], { R: '1k' }); t.el('R', [om2, oo2], { R: '3k' }); t.el('R', [og, G], { R: '1' }); t.el('OPAMP1', [om2, G, oo2, og], { vsp: '12', vsn: '-12' }); t.vp(oo2, 'Vopa1');
  all.push(t);
}
{
  const t = new TB('xfmr3', { dt: 5e-6, tTotal: 40e-3 });
  const n = t.p(), A = t.p(), B = t.p(), C = t.p(); t.el('VSIN3', [n, A, B, C], { V: '380', f: '50' }); t.el('R', [n, G], { R: '0' });
  const a = t.p(), b = t.p(), c = t.p(), nn = t.p(); t.el('TF3YY', [A, B, C, n, a, b, c, nn], { np: '2', ns: '1', lm: '0.5', rs: '0.1', ls: '1m' }); t.el('R3', [a, b, c, nn, nn, nn], { R: '10' }); t.el('R', [nn, G], { R: '1e6' });
  const d1 = t.p(), d2 = t.p(), d3 = t.p(); t.el('TF3YD', [A, B, C, n, d1, d2, d3], { np: '1', ns: '1', lm: '0.5', rs: '0.05', ls: '0.2m' }); t.el('RL3', [d1, d2, d3, G, G, G], { R: '20', L: '5m' });
  const e1 = t.p(), e2 = t.p(), e3 = t.p(); t.el('RC3', [A, B, C, e1, e2, e3], { R: '50', C: '10u' }); t.el('RLC3', [e1, e2, e3, G, G, G], { R: '10', L: '1m', C: '100u' });
  t.vp(a, 'Va'); t.vp(b, 'Vb'); t.vp(d1, 'Vd1'); t.vp(e2, 'Ve2'); t.el('VP2', [d1, d2], {}, 'Vd12');
  all.push(t);
}
// ---------------------------------------------------------------- controlled sources, sensors, meters
{
  const t = new TB('ctl_sources', { dt: 2e-6, tTotal: 10e-3 });
  const s = t.p(); t.el('VSIN', [s, G], { Vm: '5', f: '500', off: '1' }, 'Vs'); const q = t.p(); t.el('VDC', [q, G], { V: '2' }, 'Vq');
  const rs = t.p(); t.el('R', [s, rs], { R: '10' }, 'Rsense'); t.el('R', [rs, G], { R: '5' });
  const mk = (ty, pins, nm, prm) => { const o = t.p(); t.el(ty, [o, G].concat(pins), prm || { k: '2' }); t.el('R', [o, G], { R: '50' }); t.vp(o, nm); };
  mk('VVCVS', [s, q], 'Vvcvs'); mk('IVCCS', [s, q], 'Vivccs', { k: '0.1' }); mk('VCCVS', [s, rs], 'Vccvs'); mk('ICCCS', [s, rs], 'Viccc');
  const c1 = t.p(), o1 = t.p(); t.el('VCCVS1', [o1, G, s, c1], { k: '3' }); t.el('R', [c1, G], { R: '20' }); t.el('R', [o1, G], { R: '30' }); t.vp(o1, 'Vccvs1');
  const c2 = t.p(), o2 = t.p(); t.el('ICCCS1', [o2, G, s, c2], { k: '0.5' }); t.el('R', [c2, G], { R: '20' }); t.el('R', [o2, G], { R: '30' }); t.vp(o2, 'Viccs1');
  mk('VNONM', [s, q], 'Vnonm', { k: '0.5' }); mk('VNOND', [s, q], 'Vnond'); mk('VNONSQ', [q], 'Vnonsq'); mk('VPOWERS', [s], 'Vpowers', { k: '1', k1: '1', k2: '1.5' }); mk('VVCVSV', [s, q], 'Vvcvsv');
  mk('INONM', [s, q], 'Vinonm', { k: '0.05' }); mk('INOND', [s, q], 'Vinond', { k: '0.05' }); mk('INONSQ', [q], 'Vinonsq', { k: '0.05' }); mk('IVCCSV', [s, q], 'Vivccsv', { k: '0.05' });
  const cs = t.c(); t.el('VSIN', [cs, G], { Vm: '1', f: '300' }, 'Vcs'); const cq = t.c(); t.el('CONST', [cq], { v: '0.5' });
  mk('VVCVS', [cs, G], 'Vvcvs_c', { k: '3' }); mk('VNONM', [cs, cq], 'Vnonm_c', { k: '4' }); mk('VVCVSV', [cs, cq], 'Vvcvsv_c', { k: '4' }); mk('IVCCS', [cs, G], 'Vivccs_c', { k: '0.02' });
  const ct = t.p(); t.el('CTOP', [cs, ct], {}); t.el('R', [ct, G], { R: '1k' }); t.vp(ct, 'Vctop');
  const vs = t.c(); t.el('VSEN', [s, G, vs], { k: '0.1' }); t.vp(vs, 'Vsen');
  const i1 = t.p(), is = t.c(); t.el('ISEN', [s, i1, is], {}, 'Isen1'); t.el('R', [i1, G], { R: '25' }); t.vp(is, 'Isen');
  t.el('VDCM', [s, G], { fc: '50' }, 'Mvdc'); t.el('VACM', [s, G], { f: '500', fc: '20' }, 'Mvac');
  const am = t.p(); t.el('ADCM', [s, am], { fc: '50' }, 'Madc'); t.el('R', [am, G], { R: '10' });
  const aa = t.p(); t.el('AACM', [s, aa], { f: '500', fc: '20' }, 'Maac'); t.el('R', [aa, G], { R: '10' });
  const wm = t.p(); t.el('WATT', [s, wm, G], { fc: '50' }, 'Mwatt'); t.el('R', [wm, G], { R: '10' });
  all.push(t);
}
// ---------------------------------------------------------------- sources
{
  const t = new TB('sources', { dt: 1e-6, tTotal: 4e-3 });
  const mkv = (ty, prm, nm) => { const o = t.p(); t.el(ty, [o, G], prm, nm); t.el('R', [o, G], { R: '10' }); t.vp(o, 'V_' + nm); };
  mkv('VDC', { V: '3' }, 'dc'); mkv('VSIN', { Vm: '2', f: '1k', ph: '30', off: '0.5', ts: '0.5m' }, 'sin'); mkv('VTRI', { Vpp: '2', f: '2k', D: '0.3', off: '-1', ph: '45' }, 'tri');
  mkv('VSAW', { Vpk: '1', f: '1.5k', off: '0', ph: '10' }, 'saw'); mkv('VSQU', { Vpp: '5', f: '1.7k', D: '0.4', off: '-1', ph: '20' }, 'squ'); mkv('VSTEP', { V: '2', ts: '1.3m' }, 'step');
  mkv('VSTEP2', { V1: '1', V2: '-2', ts: '1m', tr: '0.5m' }, 'step2'); mkv('VPWL', { f: '500', vals: '0 1 1 -1 0', times: '0 0.3m 0.6m 0.6m 2m' }, 'pwl'); mkv('VPWL2', { f: '0', pairs: '(0,0) (1m,2) (2m,2) (2m,0) (3m,1)' }, 'pwl2');
  mkv('VBAT', { V: '12' }, 'bat'); mkv('VRAND', { Vpp: '1', off: '0.2', seed: '7' }, 'rand'); mkv('VMATH', { expr: 'sin(2*pi*1k*t)^2 + (t>1m ? 0.5 : 0)', ts: '0.2m' }, 'math');
  const g1 = t.p(); t.el('VGDC', [g1], { V: '4' }); t.el('R', [g1, G], { R: '5' }); t.vp(g1, 'V_gdc'); const g2 = t.p(); t.el('VGDCT', [g2], { V: '-3' }); t.el('R', [g2, G], { R: '5' }); t.vp(g2, 'V_gdct');
  const mki = (ty, prm, nm) => { const o = t.p(); t.el(ty, [o, G], prm, nm); t.el('R', [o, G], { R: '10' }); t.vp(o, 'I_' + nm); };
  mki('IDC', { V: '0.3' }, 'idc'); mki('ISIN', { Vm: '0.2', f: '1k' }, 'isin'); mki('ITRI', { Vpp: '0.2', f: '2k' }, 'itri'); mki('ISQU', { Vpp: '0.5', f: '1.7k', D: '0.3' }, 'isqu');
  mki('ISTEP', { V: '0.2', ts: '1.1m' }, 'istep'); mki('ISTEP2', { V1: '0.1', V2: '-0.2', ts: '1m', tr: '0.2m' }, 'istep2'); mki('IPWL', { f: '0', vals: '0 0.1 0.1 0', times: '0 1m 2m 3m' }, 'ipwl');
  mki('IPWL2', { f: '600', pairs: '(0,0) (0.5m,0.2) (1m,0)' }, 'ipwl2'); mki('IRAND', { Vpp: '0.1', seed: '3' }, 'irand'); mki('IMATH', { expr: '0.1*cos(2*pi*700*t)' }, 'imath');
  const c1 = t.c(); t.el('VSIN', [c1, G], { Vm: '1', f: '1k' }, 'Vcs1'); t.vp(c1, 'C_sin');
  const c2 = t.c(); t.el('VSQU', [G, c2], { Vpp: '1', f: '1k' }, 'Vcs2'); t.vp(c2, 'C_squneg');
  const c3 = t.c(); t.el('VGDC', [c3], { V: '2.5' }); t.vp(c3, 'C_gdc');
  all.push(t);
}
// ---------------------------------------------------------------- switches
{
  const t = new TB('switches', { dt: 2e-6, tTotal: 30e-3 });
  const s = t.p(); t.el('VSIN', [s, G], { Vm: '50', f: '100' }, 'Vs');
  const gsq = t.c(); t.el('VSQU', [gsq, G], { Vpp: '1', f: '300', D: '0.5' }, 'Vg');
  const gt = t.c(); t.el('GATING', [gt], { f: '100', n: '2', pts: '30 60' }, 'Gth');
  const ld = (ty, pins, prm, nm, rev) => { const o = t.p(); t.el(ty, rev ? [o, s].concat(pins) : [s, o].concat(pins), prm, nm); const m = t.p(); t.el('IP', [o, m], {}, 'I' + nm); t.el('R', [m, G], { R: '20' }); t.vp(o, 'V' + nm); };
  ld('DIODE', [], {}, 'd_ideal'); ld('DIODE', [], { vf: '0.7', ron: '0.1' }, 'd_vf'); ld('DIODE', [], { model: 'ds', vfd: '1', ifd: '2', rd: '50m', trr: '1u', cjd: '100p', vcj: '25', bvd: '100' }, 'd_ds');
  ld('DIODE', [], { model: '1N4148' }, 'd_4148'); ld('DIODE', [], { model: 'custom', spice: '.model DX D(IS=1n N=1.5 RS=0.1 CJO=20p TT=50n BV=60 IBV=1m)' }, 'd_cust');
  ld('LED', [], {}, 'led_i'); ld('LED', [], { model: 'LEDRED' }, 'led_p'); ld('ZENER', [], { vb: '12' }, 'z_i'); ld('ZENER', [], { vb: '12', model: 'ZENER' }, 'z_p');
  ld('DIAC', [], { vbo: '30', vbb: '5' }, 'diac'); ld('THY', [gt], { vf: '1', ih: '0.05', il: '0.1' }, 'thy'); ld('TRIAC', [gt], {}, 'triac'); ld('GTO', [gsq], {}, 'gto');
  ld('NPN', [gsq], {}, 'npn'); ld('PNP', [gsq], {}, 'pnp', true); ld('IGBT', [gsq], { vf: '1', vd: '0.7' }, 'igbt'); ld('MOSFET', [gsq], { ron: '0.05', vd: '0.6' }, 'mos'); ld('PMOS', [gsq], { ron: '0.05' }, 'pmos', true);
  ld('SSWI', [gsq], {}, 'ssw'); ld('PBSW', [], { init: '0', times: '5m 12m 20m' }, 'pbsw');
  const a1 = t.p(), a2 = t.p(), a3 = t.p(); t.el('SSWI3', [s, s, s, a1, a2, a3, gsq], {}); [a1, a2, a3].forEach((n, i) => t.el('R', [n, G], { R: String(10 + 5 * i) })); t.vp(a2, 'Vsw3');
  const p1 = t.p(), p2 = t.p(); t.el('SPDT', [s, p1, p2, gsq], {}); t.el('R', [p1, G], { R: '10' }); t.el('R', [p2, G], { R: '15' }); t.vp(p1, 'Vspdt1'); t.vp(p2, 'Vspdt2');
  const q1 = t.p(), q2 = t.p(); t.el('DPST', [s, q1, s, q2, gsq], { init: '1' }); t.el('R', [q1, G], { R: '10' }); t.el('R', [q2, G], { R: '15' }); t.vp(q2, 'Vdpst');
  const r1 = t.p(), r2 = t.p(), r3 = t.p(), r4 = t.p(); t.el('DPDT', [s, r1, r2, s, r3, r4, gsq], {}); [r1, r2, r3, r4].forEach(n => t.el('R', [n, G], { R: '12' })); t.vp(r2, 'Vdpdt2'); t.vp(r3, 'Vdpdt3');
  all.push(t);
}
{
  const t = new TB('three_state', { dt: 1e-6, tTotal: 4e-3 });
  const vcc = t.p(); t.el('VDC', [vcc, G], { V: '12' }, 'Vcc'); const vin = t.p(); t.el('VSIN', [vin, G], { Vm: '3', f: '1k', off: '3' }, 'Vin');
  const amp = (ty, prm, nm, pnp) => { const c = t.p(), b = t.p(), e = t.p(); t.el('R', [pnp ? G : vcc, c], { R: '1k' }); t.el('R', [vin, b], { R: '10k' }); t.el('R', [e, pnp ? vcc : G], { R: '100' }); t.el(ty, [c, e, b], prm, nm); t.vp(c, 'Vc_' + nm); t.vp(e, 'Ve_' + nm); };
  amp('NPN3', { model: 'ideal', beta: '100' }, 'q3i'); amp('NPN3', { model: '2N3904' }, 'q3p'); amp('NPN3', { model: '2N2222A' }, 'q3p2'); amp('PNP3', { model: 'ideal', beta: '80' }, 'qp3i', true); amp('PNP3', { model: '2N3906' }, 'qp3p', true);
  const mos = (ty, prm, nm, p) => { const d = t.p(), g = t.p(); t.el('R', [p ? G : vcc, d], { R: '100' }); t.el('R', [vin, g], { R: '1k' }); t.el(ty, p ? [d, vcc, g] : [d, G, g], prm, nm); t.vp(d, 'Vd_' + nm); };
  mos('MOS3', { model: '3state', vth: '2', gm: '0.5', ron: '1' }, 'm3i'); mos('MOS3', { model: 'GENERIC_N' }, 'm3p'); mos('PMOS3', { model: '3state', vth: '2', gm: '0.5', ron: '1' }, 'pm3i', true); mos('PMOS3', { model: 'GENERIC_P' }, 'pm3p', true);
  all.push(t);
}
{
  const t = new TB('bridges', { dt: 5e-6, tTotal: 50e-3 });
  const s = t.p(), sn = t.p(); t.el('VSIN', [s, sn], { Vm: '170', f: '60' }, 'Vs'); t.el('R', [sn, G], { R: '0' });
  const dp = t.p(), dn = t.p(); t.el('BDIODE1', [s, sn, dp, dn], { vf: '0.7' }); t.el('C', [dp, dn], { C: '470u' }); t.el('R', [dp, dn], { R: '50' }); t.el('VP2', [dp, dn], {}, 'Vbd1');
  const n = t.p(), A = t.p(), B = t.p(), C = t.p(); t.el('VSIN3', [n, A, B, C], { V: '208', f: '60' }); t.el('R', [n, G], { R: '1e6' });
  const e1 = t.p(), e2 = t.p(); t.el('BDIODE3', [A, B, C, e1, e2], {}); t.el('RL', [e1, e2], { R: '20', L: '20m' }); t.el('VP2', [e1, e2], {}, 'Vbd3');
  const g1 = t.c(); t.el('GATING', [g1], { f: '60', n: '2', pts: '45 165' }, 'G1');
  const f1 = t.p(), f2 = t.p(); t.el('BTHY1', [s, sn, f1, f2, g1], { vf: '1' }); t.el('RL', [f1, f2], { R: '30', L: '50m' }); t.el('VP2', [f1, f2], {}, 'Vbt1');
  const g3 = t.c(); t.el('GATING', [g3], { f: '60', n: '2', pts: '60 180' }, 'G3');
  const h1 = t.p(), h2 = t.p(); t.el('BTHY3', [A, B, C, h1, h2, g3], {}); t.el('RL', [h1, h2], { R: '20', L: '30m' }); t.el('VP2', [h1, h2], {}, 'Vbt3');
  const g4 = t.c(); t.el('GATING', [g4], { f: '60', n: '2', pts: '60 200' }, 'G4');
  const k = t.p(); t.el('BTHY3H', [A, B, C, k, g4], {}); t.el('RL', [k, n], { R: '25', L: '10m' }); t.el('VP2', [k, n], {}, 'Vbt3h');
  const n2 = t.p(), a2 = t.p(), b2 = t.p(), c2 = t.p(); t.el('VSIN3', [n2, a2, b2, c2], { V: '208', f: '60', ph: '180' }); t.el('R', [n2, n], { R: '0' });
  const g6 = t.c(); t.el('GATING', [g6], { f: '60', n: '2', pts: '40 100' }, 'G6');
  const m = t.p(); t.el('BTHY6H', [A, c2, B, a2, C, b2, m, g6], {}); t.el('RL', [m, n], { R: '25', L: '10m' }); t.el('VP2', [m, n], {}, 'Vbt6h');
  const vs = t.c(), z = t.c(), syn = t.c(), al = t.c(), en = t.c(), ga = t.c();
  t.el('VSEN', [s, sn, vs], {}); t.el('CONST', [z], { v: '0' }); t.el('COMP', [vs, z, syn], {}); t.el('CONST', [al], { v: '50' }); t.el('CONST', [en], { v: '1' }); t.el('ACTRL', [al, syn, en, ga], { f: '60', pw: '120' }, 'AC1');
  const th = t.p(); t.el('THY', [s, th, ga], {}); t.el('R', [th, G], { R: '20' }); t.vp(th, 'Vthy_alpha'); t.vp(ga, 'Galpha');
  all.push(t);
}
// ---------------------------------------------------------------- control blocks
{
  const t = new TB('control', { dt: 1e-5, tTotal: 50e-3 });
  const u = t.c(); t.el('VSIN', [u, G], { Vm: '1', f: '60', off: '0.2' }, 'Vu'); const w = t.c(); t.el('VTRI', [w, G], { Vpp: '2', f: '200', off: '-1' }, 'Vw');
  const sq = t.c(); t.el('VSQU', [sq, G], { Vpp: '1', f: '150' }, 'Vsq'); const k = t.c(); t.el('CONST', [k], { v: '0.7' }, 'K1');
  const one = (ty, prm, nm) => { const o = t.c(); t.el(ty, [u, o], prm || {}); t.vp(o, nm || ty); return o; };
  const two = (ty, a, b, prm, nm) => { const o = t.c(); t.el(ty, [a, b, o], prm || {}); t.vp(o, nm || ty); return o; };
  one('P', { k: '3' }); one('I', { T: '1m', y0: '0.1' }); one('DIFF', { T: '1m' }); one('PI', { k: '2', T: '5m' }); one('LIM', { lo: '-0.5', hi: '0.9' });
  one('LP1', { fc: '100' }); one('HP1', { fc: '100' }); one('LP2', { fc: '80', z: '0.5' }); one('HP2', { fc: '80' }); one('BP2', { fo: '60', fb: '20' }); one('BS2', { fo: '60', fb: '20' });
  one('SQRT'); one('SIN'); one('SINR'); one('ASIN'); one('COS'); one('COSR'); one('ACOS'); one('TAN'); one('EXP', { k1: '2', k2: '3' }); one('POW', { k1: '1.5', k2: '3' }); one('LOG'); one('LOG10'); one('ABS'); one('SIGN');
  one('RMS', { fb: '60' }); one('LKUP', { tab: '-1 -2 0 0 0.5 3 1.5 4' }); one('LKUPTZ', { th: '30', vpk: '2' }); one('LKUPSQ', { th: '120' }); one('TFS', { num: '1 2', den: '1e-3 1 50' }); one('TFS1', { num: '0 0 100', den: '1 20 100', x0: '0.1 0' });
  one('TDELAY', { Td: '1.234m' }); one('UTD'); one('ROUND', { N: '1' }); one('ROUND', { N: '0', flag: '1' }, 'ROUNDT'); one('LIMDVDT', { L: '50' }); one('MATH1', { expr: 'x^3 - t' });
  two('SUM2', u, w, { k1: '2', k2: '-1' }); two('SUB2', u, w); two('COMP', u, w); two('MULT', u, w); two('DIVD', u, k); two('ATAN', u, w); two('ATAN2', u, w); two('MAXMIN', u, w, {}, 'MAX'); two('MAXMIN', u, w, { flag: '1' }, 'MIN'); two('SAMP', u, sq); two('MATH2', u, w, { expr: 'x1*x2 + sin(t*100)' });
  const s3 = t.c(); t.el('SUM3', [u, w, k, s3], { k3: '-2' }); t.vp(s3, 'SUM3');
  const m3 = t.c(); t.el('MATH3', [u, w, k, m3], { expr: 'max(x1,x2)*x3' }); t.vp(m3, 'MATH3');
  const m5 = t.c(); t.el('MATH5', [u, w, k, sq, u, m5], { expr: 'x1+x2*x3-x4+x5' }); t.vp(m5, 'MATH5');
  const ri = t.c(); t.el('RESETI', [u, ri, sq], { T: '2m', flag: '0' }); t.vp(ri, 'RESETI_e'); const ri2 = t.c(); t.el('RESETI', [u, ri2, sq], { T: '2m', flag: '1' }); t.vp(ri2, 'RESETI_l');
  const fa = t.c(), fp = t.c(); t.el('FFT', [u, fa, fp], { N: '32', fb: '60' }); t.vp(fa, 'FFTamp'); t.vp(fp, 'FFTph');
  const th1 = t.c(), th2 = t.c(); t.el('THD', [u, th1, th2], { f: '60', fb: '20' }); t.vp(th1, 'THD'); t.vp(th2, 'THDv1');
  const mx = t.c(); t.el('MUX2', [u, w, sq, mx], {}); t.vp(mx, 'MUX2');
  const mx4 = t.c(); t.el('MUX4', [u, w, k, sq, sq, sq, mx4], {}); t.vp(mx4, 'MUX4');
  const mx8 = t.c(); t.el('MUX8', [u, w, k, sq, u, w, k, sq, sq, G, sq, mx8], {}); t.vp(mx8, 'MUX8');
  const m10 = t.c(); t.el('MATH10', [u, w, k, sq, u, w, k, sq, u, w, m10], { expr: 'x1+2*x2-x3*x4+x5/2+x6+x7+x8-x9+x10*t' }); t.vp(m10, 'MATH10');
  const th = t.c(); t.el('VSAW', [th, G], { Vpk: '6.283185307', f: '60' }, 'Vth');
  const d = t.c(), q = t.c(), o = t.c(); t.el('ABC2DQO', [u, w, k, th, d, q, o], {}); t.vp(d, 'Dd'); t.vp(q, 'Dq'); t.vp(o, 'Do');
  const A = t.c(), B = t.c(), C = t.c(); t.el('DQO2ABC', [d, q, o, th, A, B, C], {}); t.vp(A, 'Aa'); t.vp(C, 'Ac');
  const y = t.c(), fb = t.c(); t.el('SUM2', [u, fb, y], { k1: '1', k2: '-1' }); t.el('P', [y, fb], { k: '0.5' }); t.vp(y, 'LoopY');
  all.push(t);
}
{
  const t = new TB('logic_digital', { dt: 1e-5, tTotal: 20e-3 });
  const a = t.c(); t.el('VSQU', [a, G], { Vpp: '1', f: '500' }, 'Va'); const b = t.c(); t.el('VSQU', [b, G], { Vpp: '1', f: '330', D: '0.3' }, 'Vb'); const c = t.c(); t.el('VSQU', [c, G], { Vpp: '1', f: '170', D: '0.6' }, 'Vc');
  const u = t.c(); t.el('VSIN', [u, G], { Vm: '2', f: '100', off: '2.5' }, 'Vu'); const ref = t.c(); t.el('CONST', [ref], { v: '5' });
  const g2 = (ty, nm) => { const o = t.c(); t.el(ty, [a, b, o], {}); t.vp(o, nm || ty); };
  g2('AND'); g2('OR'); g2('XOR'); g2('NAND'); g2('NOR'); const n1 = t.c(); t.el('NOT', [a, n1], {}); t.vp(n1, 'NOT');
  const a3 = t.c(); t.el('AND3', [a, b, c, a3], {}); t.vp(a3, 'AND3'); const o3 = t.c(); t.el('OR3', [a, b, c, o3], {}); t.vp(o3, 'OR3');
  const q1 = t.c(), q1n = t.c(); t.el('SRFF', [a, b, q1, q1n], {}); t.vp(q1, 'SRFF'); t.vp(q1n, 'SRFFn'); const q2 = t.c(); t.el('SRFF', [a, b, q2, -1], { flag: '1' }); t.vp(q2, 'SRFFl');
  const q3 = t.c(); t.el('JKFF', [b, a, c, q3, -1], {}); t.vp(q3, 'JKFF'); const q4 = t.c(); t.el('JKFFSR', [G, b, a, c, G, q4, -1], {}); t.vp(q4, 'JKFFSR');
  const q5 = t.c(); t.el('DFF', [b, a, q5, -1], {}); t.vp(q5, 'DFF'); const q6 = t.c(); t.el('DFFSR', [G, b, a, c, q6, -1], {}); t.vp(q6, 'DFFSR');
  const mo = t.c(), mon = t.c(); t.el('MONO', [b, mo, mon], { W: '0.4m' }); t.vp(mo, 'MONO'); t.vp(mon, 'MONOn');
  const wd = t.c(); t.el('CONST', [wd], { v: '0.7m' }); const mc = t.c(); t.el('MONOC', [b, wd, mc, -1], {}); t.vp(mc, 'MONOC');
  const pw = t.c(); t.el('PWCT', [b, pw], {}); t.vp(pw, 'PWCT'); const ud = t.c(); t.el('UPDN', [a, b, c, ud], { bits: '4' }); t.vp(ud, 'UPDN');
  const ad = t.c(); t.el('ADC', [u, ref, a, ad], { bits: '8' }); t.vp(ad, 'ADC'); const da = t.c(); t.el('DAC', [ad, ref, da], { bits: '8' }); t.vp(da, 'DAC');
  const one = (ty, prm, nm) => { const o = t.c(); t.el(ty, [u, o], prm || {}); t.vp(o, nm || ty); return o; };
  one('ZOH', { fs: '2k' }); one('UDELAY', { fs: '2k' }); one('ID', { fs: '5k', alg: '0' }); one('ID', { fs: '5k', alg: '1' }, 'ID_be'); one('ID', { fs: '5k', alg: '2' }, 'ID_fe');
  const ir = t.c(); t.el('IRSTD', [u, a, ir], { fs: '5k' }); t.vp(ir, 'IRSTD'); one('IRSTI', { fs: '5k', lo: '0', hi: '1' }); one('DD', { fs: '2k' });
  one('FIR', { fs: '4k' }); one('FILTD', { fs: '4k' }); one('TFZ', { fs: '4k', b: '0.1 0.2', a: '1 -0.7' }); one('DIGIT', { fs: '3k', N: '4' }); one('DIGIT2', { fs: '3k', N: '4' });
  const cb = t.c(); t.el('CBUF', [u, cb], { L: '5', fs: '1k' }); const mr = t.c(); t.el('MEMREAD', [cb, mr], { off: '2' }); t.vp(mr, 'CBUF_mem2');
  const ar = t.c(); t.el('ARRAY', [ar], { L: '3', vals: '1 -2 3' }); const cv = t.c(); t.el('CONV', [ar, cb, cv], {}); const mr2 = t.c(); t.el('MEMREAD', [cv, mr2], { off: '3' }); t.vp(mr2, 'CONV_mem3');
  const st = t.c(); t.el('STACK', [u, a, b, st], { depth: '3' }); t.vp(st, 'STACK');
  const on = t.c(); t.el('ONCTRL', [u, on], {}); t.vp(on, 'ONCTRL');
  const cbo = t.c(), cbo2 = t.c(); t.el('CBLOCK', [u, a, cbo, cbo2], { nin: '2', nout: '2', glob: 'static double acc = 0;\nstatic int cnt = 0;\ndouble buf[4] = {1, 2, 3, 4};', code: 'int i;\ndouble s = 0;\nacc = acc + x1*delt;\nif (x2 > 0.5) { cnt++; } else { cnt = cnt % 7; }\nfor (i = 0; i < 4; i++) s += buf[i] * x1;\nswitch (cnt % 3) { case 0: y2 = s; break; case 1: y2 = -s; break; default: y2 = 0; }\ny1 = 2*x1 + 100*acc + (cnt > 3 ? 1 : 0);' }, 'CB1');
  t.vp(cbo, 'CB_y1'); t.vp(cbo2, 'CB_y2');
  all.push(t);
}
// ---------------------------------------------------------------- devices
{
  const t = new TB('devices', { dt: 5e-9, tTotal: 40e-6 });
  const vin = t.p(); t.el('VDC', [vin, G], { V: '200' }, 'Vin'); const g = t.c(); t.el('VSQU', [g, G], { Vpp: '1', f: '100k', D: '0.4' }, 'Vg');
  const sw = t.p(), o = t.p(); t.el('MOSNI', [vin, sw, g], {}, 'Q1'); t.el('DIODE', [G, sw], { model: 'ds', vfd: '1.2', ifd: '10', trr: '30n' }, 'D1'); t.el('L', [sw, o], { L: '100u' }); t.el('C', [o, G], { C: '10u' }); t.el('R', [o, G], { R: '20' });
  t.vp(sw, 'Vsw'); t.vp(o, 'Vo'); t.el('DEVP', [], { el: 'Q1', q: 'vg' }, 'Q1vg'); t.el('DEVP', [], { el: 'Q1', q: 'id' }, 'Q1id'); t.el('DEVP', [], { el: 'Q1', q: 'e' }, 'Q1e'); t.el('DEVP', [], { el: 'Q1', q: 'ig' }, 'Q1ig');
  const v2 = t.p(); t.el('VDC', [v2, G], { V: '-100' }, 'Vn'); const pd = t.p(); t.el('R', [pd, G], { R: '50' }); t.el('PMOSNI', [pd, v2, g], { von: '10' }, 'Q2'); t.vp(pd, 'Vpd');
  const ic = t.p(); t.el('R', [vin, ic], { R: '20' }); t.el('IGBTNI', [ic, G, g], {}, 'Q3'); t.vp(ic, 'Vce'); t.el('DEVP', [], { el: 'Q3', q: 'p' }, 'Q3p');
  all.push(t);
}
{
  const t = new TB('analog_ics', { dt: 2e-7, tTotal: 2e-3 });
  const vcc = t.p(); t.el('VDC', [vcc, G], { V: '5' }, 'Vcc'); const vin = t.p(); t.el('VSIN', [vin, G], { Vm: '2', f: '2k', off: '2.5' }, 'Vin'); const rf = t.p(); t.el('VDC', [rf, G], { V: '2.5' }, 'Vref');
  const out = t.p(); t.el('COMPOC', [vin, rf, out, vcc, G], {}, 'U1'); t.el('R', [vcc, out], { R: '4.7k' }); t.vp(out, 'Vcmp');
  const k = t.p(), rr = t.p(); t.el('R', [vcc, k], { R: '330' }); t.el('TL431', [k, G, rr], {}, 'U2'); t.el('R', [k, rr], { R: '10k' }); t.el('R', [rr, G], { R: '10k' }); t.vp(k, 'Vk');
  const la = t.p(); t.el('R', [vin, la], { R: '200' }); const cc = t.p(), c2 = t.p(); t.el('OPTO2', [la, G, cc, G, c2, G], {}, 'U3'); t.el('R', [vcc, cc], { R: '2k' }); t.el('R', [vcc, c2], { R: '1k' }); t.vp(cc, 'Vopto1'); t.vp(c2, 'Vopto2');
  const la1 = t.p(), c1 = t.p(); t.el('R', [vin, la1], { R: '330' }); t.el('OPTO1', [la1, G, c1, G], { ctr: '50' }, 'U4'); t.el('R', [vcc, c1], { R: '3k' }); t.vp(c1, 'Vopto3');
  all.push(t);
}
// ---------------------------------------------------------------- integration options
for (const [nm, o] of [['opt_nointerp', { interp: false }], ['opt_nodamp', { damp: false }], ['opt_nolte', { lte: false }], ['opt_tightlte', { lteTol: 1e-5 }], ['opt_print', { tPrint: 2e-3, printStep: 3 }]]) {
  const t = new TB(nm, Object.assign({ dt: 4e-6, tTotal: 6e-3 }, o));
  const vin = t.p(); t.el('VDC', [vin, G], { V: '48' }, 'Vin'); const g = t.c(); const ref = t.c(), tri = t.c(); t.el('CONST', [ref], { v: '0.42' }); t.el('VTRI', [tri, G], { Vpp: '1', f: '20k' }); t.el('COMP', [ref, tri, g], {});
  const sw = t.p(), o2 = t.p(), mm = t.p(); t.el('MOSFET', [vin, sw, g], {}, 'Q1'); t.el('DIODE', [G, sw], {}, 'D1'); t.el('IP', [sw, mm], {}, 'Isw'); t.el('L', [mm, o2], { L: '50u' }); t.el('C', [o2, G], { C: '22u' }); t.el('R', [o2, G], { R: '3' });
  t.vp(o2, 'Vo'); t.vp(sw, 'Vsw');
  all.push(t);
}
// ---------------------------------------------------------------- AC linear analysis
{
  const t = new TB('ac_linear', { dt: 1e-6, tTotal: 1e-3 }); t.ac = { fStart: 10, fEnd: 100e3, nPts: 60, flag: 0, src: 'Vac', aStart: 1, aEnd: 1, extra: [1234], method: 1, settle: 5, mcyc: 4, maxCyc: 60, tol: 1e-3, tPre: -1, tSettle: 0, baseline: 1 };
  const s = t.p(); t.el('VSIN', [s, G], { Vm: '1', f: '1k' }, 'Vac'); const a = t.p(), b = t.p(); t.el('RL', [s, a], { R: '1', L: '1m' }); t.el('C', [a, G], { C: '10u' }); t.el('RC', [a, b], { R: '10', C: '1u' }); t.el('R', [b, G], { R: '100' });
  t.el('ACP', [a], {}, 'Va'); t.el('ACP2', [s, b], { neg: '0' }, 'Hab');
  const vs = t.c(); t.el('VSEN', [a, G, vs], {}); const f1 = t.c(); t.el('PI', [vs, f1], { k: '2', T: '1m' }); const f2 = t.c(); t.el('LP2', [f1, f2], { fc: '5k' }); const f3 = t.c(); t.el('SUM2', [f2, vs, f3], { k2: '-1' });
  const f4 = t.c(); t.el('TDELAY', [f3, f4], { Td: '10u' }); const f5 = t.c(); t.el('I', [f4, f5], { T: '1m' }); t.el('ACP', [f5], {}, 'Ctl');
  const ct = t.p(); t.el('CTOP', [f2, ct], {}); t.el('R', [ct, G], { R: '1k' }); t.el('ACP', [ct], {}, 'Vct');
  const op = t.p(), oo = t.p(); t.el('R', [a, op], { R: '1k' }); t.el('C', [op, oo], { C: '10n' }); t.el('R', [op, oo], { R: '10k' }); t.el('OPAMP', [op, G, oo], {}); t.el('ACP', [oo], {}, 'Vop');
  const is = t.p(), ic = t.c(); t.el('ISEN', [s, is, ic], {}); t.el('R', [is, G], { R: '50' }); t.el('ACP', [ic], {}, 'Isen');
  all.push(t);
}

fs.mkdirSync(out, { recursive: true });
all.forEach(t => t.save(out));
console.log(all.map(t => t.name).join(' '));
// AC sweep by injection on a closed-loop buck (voltage-mode PI)
{
  const t = new TB('ac_inject', { dt: 0.5e-6, tTotal: 4e-3 }); t.ac = { fStart: 200, fEnd: 20e3, nPts: 8, flag: 0, src: 'Vinj', aStart: 0.02, aEnd: 0.02, extra: [], method: 0, settle: 5, mcyc: 4, maxCyc: 60, tol: 1e-3, tPre: 3e-3, tSettle: 0, baseline: 1 };
  const vin = t.p(); t.el('VDC', [vin, G], { V: '24' }, 'Vin');
  const sw = t.p(), o = t.p(); const g = t.c(); t.el('MOSFET', [vin, sw, g], {}, 'Q1'); t.el('DIODE', [G, sw], {}, 'D1'); t.el('L', [sw, o], { L: '47u' }); t.el('C', [o, G], { C: '100u' }); t.el('R', [o, G], { R: '2' });
  const vs = t.c(), ref = t.c(), err = t.c(), inj = t.c(), cmd = t.c(), tri = t.c(), lim = t.c();
  t.el('VSEN', [o, G, vs], { k: '0.1' }); t.el('CONST', [ref], { v: '1.2' }); t.el('VSIN', [inj, G], { Vm: '0', f: '1k' }, 'Vinj');
  const fbk = t.c(); t.el('SUM2', [vs, inj, fbk], {}); t.el('SUB2', [ref, fbk, err], {}); t.el('PI', [err, cmd], { k: '0.5', T: '0.2m' }); t.el('LIM', [cmd, lim], { lo: '0.05', hi: '0.95' });
  t.el('VTRI', [tri, G], { Vpp: '1', f: '100k' }); t.el('COMP', [lim, tri, g], {});
  t.el('ACP2', [fbk, vs], { neg: '1' }, 'Tloop'); t.el('ACP', [o], {}, 'Vo');
  all.push(t);
  fs.mkdirSync(out, { recursive: true }); t.save(out);
}
