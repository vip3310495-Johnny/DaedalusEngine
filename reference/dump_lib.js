const LIB = require('./lib.js');
const out = { order: LIB.ORDER, defs: {} };
for (const t of LIB.ORDER) {
  const d = LIB.D[t];
  const o = {};
  for (const k of Object.keys(d)) { if (typeof d[k] !== 'function') o[k] = d[k]; }
  try { const el = { params: {} }; d.params.forEach(p => el.params[p.k] = p.d); o.symbol = d.sym ? d.sym(el) : null; } catch (e) { o.symbol = { err: e.message }; }
  try { const el = { params: {} }; d.params.forEach(p => el.params[p.k] = p.d); o.pinsDefault = d.pinsFor(el); } catch (e) {}
  out.defs[t] = o;
}
require('fs').writeFileSync('lib_dump.json', JSON.stringify(out, null, 1));
const cats = {}; for (const t of LIB.ORDER) { const c = LIB.D[t].cat; (cats[c] = cats[c] || []).push(t); }
for (const c in cats) console.log(c + ': ' + cats[c].join(' '));
