// Compare two CSV result files column by column: max |a-b| relative to the column's peak magnitude.
const fs = require('fs');
function load(f) { const L = fs.readFileSync(f, 'utf8').trim().split('\n'); const h = L[0].split(','); return { h, rows: L.slice(1).map(l => l.split(',').map(Number)) }; }
const a = load(process.argv[2]), b = load(process.argv[3]);
if (a.rows.length !== b.rows.length) console.log('ROW COUNT DIFFERS', a.rows.length, b.rows.length);
const n = Math.min(a.rows.length, b.rows.length);
let worst = 0;
for (let c = 1; c < a.h.length; c++) {
  let pk = 0, md = 0, at = 0;
  for (let i = 0; i < n; i++) { pk = Math.max(pk, Math.abs(a.rows[i][c])); const x = a.rows[i][c], y = b.rows[i][c]; const d = (x !== x && y !== y) ? 0 : Math.abs(x - y); if (d > md || (d !== d)) { md = d; at = a.rows[i][0]; } }
  const rel = pk > 0 ? md / pk : md;
  worst = Math.max(worst, rel);
  console.log(`  ${a.h[c].padEnd(14)} peak=${pk.toExponential(3)} maxdiff=${md.toExponential(3)} rel=${rel.toExponential(2)} @ ${at.toExponential(4)}`);
}
console.log('  WORST REL', worst.toExponential(2));
