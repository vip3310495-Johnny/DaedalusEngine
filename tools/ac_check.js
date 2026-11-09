// compare AC-sweep results of the examples with closed-form transfer functions
const fs = require('fs');
const rd = f => fs.readFileSync(f, 'utf8').trim().split(/\r?\n/).slice(1).map(l => l.split(',').map(Number));
const c = (re, im) => ({ re, im }), add = (a, b) => c(a.re + b.re, a.im + b.im), mul = (a, b) => c(a.re * b.re - a.im * b.im, a.re * b.im + a.im * b.re);
const div = (a, b) => { const d = b.re * b.re + b.im * b.im; return c((a.re * b.re + a.im * b.im) / d, (a.im * b.re - a.re * b.im) / d); };
const par = (a, b) => div(mul(a, b), add(a, b));
const db = z => 20 * Math.log10(Math.hypot(z.re, z.im)), ph = z => Math.atan2(z.im, z.re) * 180 / Math.PI;
function cmp(name, rows, H) {
  let wm = 0, wp = 0, wf = 0;
  for (const [f, re, im] of rows) {
    const h = H(2 * Math.PI * f), z = c(re, im);
    const em = Math.abs(db(z) - db(h)), ep = Math.abs(((ph(z) - ph(h) + 540) % 360) - 180);
    if (em > wm) { wm = em; wf = f; }
    wp = Math.max(wp, ep);
    if (process.argv[2] === '-v') console.log(name, f.toFixed(1), db(z).toFixed(3), db(h).toFixed(3), ph(z).toFixed(2), ph(h).toFixed(2));
  }
  console.log(`${name}: 最大增益誤差 ${wm.toExponential(3)} dB (@${wf.toFixed(0)} Hz)，最大相位誤差 ${wp.toExponential(3)}°`);
}
// RLC: Vac → R1 2Ω → L 1mH → (C 10µF ∥ RL 20Ω)
cmp('rlc   ', rd('tests/phys/rlc_ac.csv'), w => { const z = par(c(20, 0), c(0, -1 / (w * 10e-6))); return div(z, add(z, c(2, w * 1e-3))); });
// buck control-to-output: Vin/Vpp · (RL∥C)/(sL + ron + RL∥C), averaged model (valid well below fsw/2)
const bk = rd('tests/phys/buckac_ac.csv');
cmp('buckac', bk, w => { const z = par(c(5, 0), c(0, -1 / (w * 100e-6))); return mul(c(100, 0), div(z, add(z, c(10e-6, w * 100e-6)))); });
cmp('buckac (f ≤ 5 kHz)', bk.filter(r => r[0] <= 5e3), w => { const z = par(c(5, 0), c(0, -1 / (w * 100e-6))); return mul(c(100, 0), div(z, add(z, c(10e-6, w * 100e-6)))); });
