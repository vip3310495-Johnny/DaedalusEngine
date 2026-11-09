// Reference harness: runs the original JS engine headless (for cross-validation of the Rust port).
// usage: node harness.js dump-examples <outdir>
//        node harness.js run <file.psimx> <out.csv> [ac]
//        node harness.js netlist <file.psimx> <out.json>
'use strict';
const fs = require('fs'); const path = require('path');
global.SIM = require('./engine.js');
global.LIB = require('./lib.js');
const ui = fs.readFileSync(path.join(__dirname, 'ui.js'), 'utf8').split('\n');
const seg = (a, b) => ui.slice(a - 1, b).join('\n');
const store = { get() { return null; }, set() {} };
let src = 'const store = { get(){return null;}, set(){} };\n' +
  seg(33, 54) + '\n' + seg(59, 66) + '\n' + seg(76, 76) + '\n' + 'const elById = id => ED.doc.elements.find(e => e.id === id);\n' +
  seg(93, 136) + '\n' + seg(990, 1045) + '\n' + seg(1214, 1233) + '\n' + seg(1875, 2017) + '\n' +
  'module.exports = { ED, buildNetlist, evalSimCtrl, evalAC, EXAMPLES, makeDoc, defaultParams };';
const Module = module.constructor; const m = new Module(); m.paths = module.paths; m._compile(src, 'uiharness.js');
const U = m.exports;
function docFromJson(src) {
  const doc = JSON.parse(JSON.stringify(src));
  doc.elements = doc.elements.filter(e => LIB.D[e.type]);
  doc.elements.forEach(e => { e.params = Object.assign(U.defaultParams(e.type), e.params || {}); e.rot = e.rot || 0; });
  return doc;
}
const cmd = process.argv[2];
if (cmd === 'dump-examples') {
  const out = process.argv[3]; fs.mkdirSync(out, { recursive: true });
  const list = [];
  for (const k of Object.keys(U.EXAMPLES)) {
    const ex = U.EXAMPLES[k]; const doc = ex.doc ? docFromJson(ex.doc) : U.makeDoc(ex.build);
    fs.writeFileSync(path.join(out, k + '.psimx'), JSON.stringify({ app: 'psimweb', version: 'V3.2.1', doc, screens: ex.screens }, null, 1));
    list.push({ key: k, name: ex.name });
  }
  fs.writeFileSync(path.join(out, 'index.json'), JSON.stringify(list, null, 1));
  console.log(list.map(e => e.key + ': ' + e.name).join('\n'));
} else if (cmd === 'run' || cmd === 'netlist') {
  const o = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
  U.ED.doc = docFromJson(o.doc || o);
  const nl = U.buildNetlist();
  if (nl.errors.length) { console.error('netlist errors:', JSON.stringify(nl.errors)); process.exit(2); }
  if (cmd === 'netlist') { fs.writeFileSync(process.argv[4], JSON.stringify(nl.net, null, 1)); process.exit(0); }
  const sc = U.evalSimCtrl(nl.syms); const ac = process.argv[5] === 'ac' ? U.evalAC(nl.syms) : null;
  const t0 = Date.now();
  let R;
  if (ac) R = ac.method === 1 ? new SIM.ACLinearRunner(nl.net, sc, ac) : new SIM.ACInjectRunner(nl.net, sc, ac);
  else R = new SIM.TransientRunner(nl.net, sc);
  while (!R.run(1e9)) {}
  const res = R.result(); const secs = (Date.now() - t0) / 1000;
  let csv = '';
  if (res.kind === 'tran') {
    csv = 'Time,' + res.names.join(',') + '\n';
    for (let i = 0; i < res.n; i++) { let l = String(res.t0 + i * res.dt); for (const d of res.data) l += ',' + d[i]; csv += l + '\n'; }
  } else {
    csv = 'Freq'; res.curves.forEach(c => { csv += ',' + c.name + '_re,' + c.name + '_im'; }); csv += '\n';
    res.freqs.forEach((f, i) => { let l = String(f); res.curves.forEach(c => { l += ',' + c.re[i] + ',' + c.im[i]; }); csv += l + '\n'; });
  }
  fs.writeFileSync(process.argv[4], csv);
  console.log(JSON.stringify({ secs, stats: res.stats, events: res.events, warnings: res.warnings }));
}
if (cmd === 'runnet') {
  const o = JSON.parse(fs.readFileSync(process.argv[3], 'utf8'));
  const t0 = Date.now(); let R;
  try {
    if (o.ac) R = o.ac.method === 1 ? new SIM.ACLinearRunner(o.net, o.sc, o.ac) : new SIM.ACInjectRunner(o.net, o.sc, o.ac);
    else R = new SIM.TransientRunner(o.net, o.sc);
    while (!R.run(1e9)) {}
  } catch (e) { console.log(JSON.stringify({ error: e.message })); process.exit(0); }
  const res = R.result(); const secs = (Date.now() - t0) / 1000;
  let csv = '';
  if (res.kind === 'tran') {
    csv = 'Time,' + res.names.join(',') + '\n';
    for (let i = 0; i < res.n; i++) { let l = String(res.t0 + i * res.dt); for (const d of res.data) l += ',' + d[i]; csv += l + '\n'; }
  } else {
    csv = 'Freq'; res.curves.forEach(c => { csv += ',' + c.name + '_re,' + c.name + '_im'; }); csv += '\n';
    res.freqs.forEach((f, i) => { let l = String(f); res.curves.forEach(c => { l += ',' + c.re[i] + ',' + c.im[i]; }); csv += l + '\n'; });
  }
  fs.writeFileSync(process.argv[4], csv);
  console.log(JSON.stringify({ secs, stats: res.stats, events: res.events, warnings: res.warnings }));
}
