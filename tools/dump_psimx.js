// print the elements and parameters of .psimx files compactly
const fs = require('fs');
for (const f of process.argv.slice(2)) {
  const j = JSON.parse(fs.readFileSync(f, 'utf8'));
  const els = j.elements || (j.doc && j.doc.elements) || [];
  console.log('=== ' + f + '  (' + els.length + ' elements)');
  for (const e of els) {
    const p = Object.entries(e.params || {}).filter(([k, v]) => v !== '' && v != null).map(([k, v]) => k + '=' + v).join(' ');
    console.log('  ' + e.type + ' ' + (e.name || '') + (e.disabled ? ' [DIS]' : '') + '  ' + p);
  }
}
