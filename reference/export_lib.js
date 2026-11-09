// Export the element library (pins, parameters, descriptions, symbols) for the Rust port.
const LIB = require('./lib.js');
const out = [];
for (const t of LIB.ORDER) {
  const d = LIB.D[t];
  const el = { params: {} }; d.params.forEach(p => el.params[p.k] = p.d);
  const sym = d.sym ? d.sym(el) : { g: '' };
  const flags = ['single', 'nosim', 'noname', 'probe', 'acprobe', 'dynPins', 'src', 'label', 'text', 'lblRight'].filter(k => d[k]);
  out.push({
    type: t, name: d.name, zh: d.zh, pre: d.pre, cat: d.cat,
    pins: (d.pins || []).map(p => ({ x: p.x, y: p.y, k: p.k, n: p.n })),
    bb: d.bb, show: d.show || [], flags,
    params: d.params.map(p => {
      const o = { k: p.k, l: p.l, d: p.d, u: p.u || '', desc: p.desc || '' };
      if (p.type) o.type = p.type; if (p.opts) o.opts = p.opts; if (p.adv) o.adv = true; if (p.idealOnly) o.idealOnly = true; if (p.presetKind) o.presetKind = p.presetKind;
      return o;
    }),
    desc: d.desc || '',
    sym: { g: sym.g || '', t: (sym.t || []).map(x => ({ x: x.x, y: x.y, s: x.s, sz: x.sz, a: x.a })) }
  });
}
require('fs').writeFileSync(process.argv[2], JSON.stringify(out));
console.log(out.length + ' elements');
