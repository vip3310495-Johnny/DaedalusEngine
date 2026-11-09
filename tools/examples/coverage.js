// Element-type coverage of the bundled examples against the library (assets/library.json).
const fs = require('fs'), path = require('path');
const ROOT = path.join(__dirname, '../..');
const lib = JSON.parse(fs.readFileSync(path.join(ROOT, 'crates/daedalus-core/assets/library.json'), 'utf8'));
const defs = lib.defs || lib.elements || lib;
const all = (Array.isArray(defs) ? defs.map(d => d.type) : Object.keys(defs)).filter(Boolean);
const dir = path.join(ROOT, 'crates/daedalus-app/assets/examples');
const used = new Map();
for (const f of fs.readdirSync(dir).filter(f => f.endsWith('.psimx'))) {
  const j = JSON.parse(fs.readFileSync(path.join(dir, f), 'utf8'));
  const els = (j.doc && j.doc.elements) || j.elements || [];
  for (const e of els) { if (!used.has(e.type)) used.set(e.type, []); used.get(e.type).push(f.replace('.psimx', '')); }
}
const cov = all.filter(t => used.has(t)), miss = all.filter(t => !used.has(t));
console.log(`library types: ${all.length}  used: ${cov.length}  coverage: ${(100 * cov.length / all.length).toFixed(1)}%`);
if (process.argv.includes('-m')) console.log('missing: ' + miss.join(' '));
if (process.argv.includes('-j')) fs.writeFileSync(path.join(ROOT, 'tests/examples/coverage.json'), JSON.stringify({ total: all.length, used: cov.length, missing: miss, by: Object.fromEntries(used) }, null, 1));
