// make a CLI case JSON from a harness netlist: override sim-control fields and add voltage probes
// usage: node tools/variant.js <net.json> <out.json> '<sc overrides json>' [probeName=net ...]
const fs = require('fs');
const [src, out, sco, ...probes] = process.argv.slice(2);
const net = JSON.parse(fs.readFileSync(src, 'utf8'));
const sc = Object.assign({ dt: 1e-6, tTotal: 1e-3, tPrint: 0, printStep: 1, interp: true, damp: true, lte: true, lteTol: 1e-3 }, JSON.parse(sco || '{}'));
let id = Math.max(...net.elements.map(e => e.id)) + 1;
for (const p of probes) {
  const [name, n] = p.split('=');
  net.elements.push({ id: id++, type: 'VP', name, params: {}, pins: [parseInt(n)] });
}
fs.writeFileSync(out, JSON.stringify({ net, sc, ac: null }));
