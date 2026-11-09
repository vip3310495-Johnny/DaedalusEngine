// print element -> net connections from a netlist JSON produced by reference/harness.js netlist
const fs = require('fs');
const lib = JSON.parse(fs.readFileSync(__dirname + '/../reference/lib_dump.json', 'utf8'));
const defs = lib.defs || lib;
const pinNames = t => { const d = Array.isArray(defs) ? defs.find(x => x.type === t) : defs[t]; return d && d.pins ? d.pins.map(p => p.n || p.name) : []; };
for (const f of process.argv.slice(2)) {
  const n = JSON.parse(fs.readFileSync(f, 'utf8'));
  console.log('== ' + f);
  for (const el of n.elements) {
    const pn = pinNames(el.type);
    const pins = (el.pins || []).map((x, i) => (pn[i] || i) + '=' + (n.nets[x] && n.nets[x].kind === 'G' ? 'gnd' : 'n' + x)).join(' ');
    if (el.type !== 'GND') console.log('  ' + (el.type + ' ' + (el.name || '')).padEnd(18) + pins);
  }
}
