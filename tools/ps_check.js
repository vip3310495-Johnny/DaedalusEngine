// Picosecond time-precision tests: builds netlists, runs daedalus-cli, compares with closed-form answers.
// usage: node tools/ps_check.js [cli.exe] [workdir]
const fs = require('fs');
const { execFileSync } = require('child_process');
const cli = process.argv[2] || 'target/release/daedalus-cli.exe';
const wd = process.argv[3] || 'tests/ps';
fs.mkdirSync(wd, { recursive: true });

function run(name, kinds, els, sc) {
  const net = { nets: kinds.map(k => ({ kind: k })), elements: els.map((e, i) => ({ id: i + 1, name: e[1] || '', type: e[0], params: e[2] || {}, pins: e[3] })), paramText: '' };
  const f = `${wd}/${name}.json`, o = `${wd}/${name}.csv`;
  fs.writeFileSync(f, JSON.stringify({ net, sc: Object.assign({ tPrint: 0, printStep: 1, interp: true, damp: true, lte: true, lteTol: 1e-3 }, sc), ac: null }));
  const t0 = Date.now();
  const st = JSON.parse(execFileSync(cli, [f, o]).toString().trim().split('\n').pop());
  const lines = fs.readFileSync(o, 'utf8').trim().split(/\r?\n/);
  const head = lines[0].split(',');
  const cols = {};
  head.forEach(h => (cols[h] = []));
  for (let i = 1; i < lines.length; i++) lines[i].split(',').forEach((v, k) => cols[head[k]].push(parseFloat(v)));
  cols._stats = st;
  cols._ms = Date.now() - t0;
  return cols;
}
const ps = x => (x * 1e12).toFixed(4) + ' ps';
const out = [];

// ---- A: RC, tau = 10 ps, step at 50 ps, dt = 1 ps (and 0.1 ps) --------------------------------
for (const dt of [1e-12, 0.1e-12]) {
  // nets: 0 gnd, 1 src, 2 cap
  const d = run('rc10ps_' + dt, ['G', 'P', 'P'], [
    ['VSTEP', 'Vs', { V: '1', ts: '50p' }, [1, 0]],
    ['R', 'R1', { R: '1' }, [1, 2]],
    ['C', 'C1', { C: '10p', v0: '0' }, [2, 0]],
    ['VP', 'Vc', {}, [2]],
  ], { dt, tTotal: 300e-12 });
  const tau = 10e-12;
  let emax = 0, tmax = 0;
  d.Time.forEach((t, i) => {
    const th = t <= 50e-12 ? 0 : 1 - Math.exp(-(t - 50e-12) / tau);
    const e = Math.abs(d.Vc[i] - th);
    emax = Math.max(emax, e);
    // equivalent timing error: voltage error divided by the slope
    if (t > 51e-12 && th < 0.99) tmax = Math.max(tmax, e / (Math.exp(-(t - 50e-12) / tau) / tau));
  });
  out.push(`A  RC τ=10 ps，dt=${dt * 1e12} ps：最大電壓誤差 ${emax.toExponential(3)} V（滿刻度 1 V），等效時間誤差 ${ps(tmax)}`);
}

// ---- B: series LC at 10 GHz (period 100 ps), 20 cycles ------------------------------------------
for (const dt of [1e-12, 0.25e-12]) {
  const L = 0.1e-9, f0 = 10e9, w = 2 * Math.PI * f0, C = 1 / (w * w * L);
  const d = run('lc10g_' + dt, ['G', 'P', 'P', 'P'], [
    ['VSTEP', 'Vs', { V: '1', ts: '10p' }, [1, 0]],
    ['L', 'L1', { L: String(L), i0: '0' }, [1, 2]],
    ['R', 'R1', { R: '1u' }, [2, 3]],
    ['C', 'C1', { C: String(C), v0: '0' }, [3, 0]],
    ['VP', 'Vc', {}, [3]],
  ], { dt, tTotal: 2.01e-9 });
  // zero crossings of (Vc − 1): exact at 10 ps + T/4 + k·T/2
  const zc = [];
  for (let i = 1; i < d.Time.length; i++) {
    const a = d.Vc[i - 1] - 1, b = d.Vc[i] - 1;
    if (a * b < 0) zc.push(d.Time[i - 1] + (d.Time[i] - d.Time[i - 1]) * a / (a - b));
  }
  let emax = 0;
  zc.forEach((t, k) => { emax = Math.max(emax, Math.abs(t - (10e-12 + 25e-12 + k * 50e-12))); });
  const last = zc[zc.length - 1], k = zc.length - 1;
  out.push(`B  LC 10 GHz，dt=${dt * 1e12} ps，${(zc.length / 2).toFixed(0)} 週期：零交越最大時間誤差 ${ps(emax)}（最後一次 ${ps(last - (35e-12 + k * 50e-12))}；梯形法頻率翹曲 (ωh)²/12 = ${((w * dt) ** 2 / 12).toExponential(2)}）`);
}

// ---- C: comparator crossing at t ≈ 1 ms, located by interpolation; dt = 1 ns and 10 ns -------------
for (const dt of [1e-9, 10e-9]) {
  // power: 0 gnd, 1 Vdc, 2 sw out, 3 freewheel, 4 inductor ; control: 5 sine, 6 ref, 7 cmp out, 8 gate
  // the switch puts 1 V across L = 1 µH: i = (1/ron)(1 − e^(−ron(t−te)/L)) ≈ (t − te)/L, so each sample of i
  // gives the closing instant te; when it opens the current freewheels through D + 100 Ω and dies out (τ = 10 ns)
  const d = run('cross1ms_' + dt, ['G', 'P', 'P', 'P', 'P', 'C', 'C', 'C', 'C'], [
    ['VDC', 'V1', { V: '1' }, [1, 0]],
    ['MOSFET', 'Q1', { ron: '10u', vd: '0', init: '0', roff: '1G' }, [1, 2, 8]],
    ['IP', 'IL', {}, [2, 4]],
    ['L', 'L1', { L: '1u', i0: '0' }, [4, 0]],
    ['DIODE', 'D1', { vf: '0', init: '0', ron: '10u', roff: '1G', model: 'ideal' }, [3, 2]],
    ['R', 'Rf', { R: '100' }, [0, 3]],
    ['VSIN', 'Vsin', { Vm: '1', f: '1Meg', ph: '0', off: '0', ts: '0' }, [5, 0]],
    ['VDC', 'Vref', { V: '0.5' }, [6, 0]],
    ['COMP', 'CMP', {}, [5, 6, 7]],
    ['ONCTRL', 'ON', {}, [7, 8]],
  ], { dt, tTotal: 1e-3, tPrint: 1e-3 - 2e-6 });
  // switch closes when sin(2π·1MHz·t) rises through 0.5: t = k·1µs + 1/12 µs
  const te = 999e-6 + (1 / 12) * 1e-6;
  const ron = 10e-6, L = 1e-6;
  const est = [];
  d.Time.forEach((t, i) => { if (t > te + 1e-9 && t < te + 0.3e-6) est.push(t + (L / ron) * Math.log(1 - ron * d.IL[i])); });
  const err = est.length ? Math.max(...est.map(x => Math.abs(x - te))) : NaN;
  out.push(`C  比較器跨越於 t≈999.083 µs 定位，dt=${dt * 1e9} ns：開關時刻誤差 ${ps(err)}（${est.length} 個取樣反推）`);
}

console.log(out.join('\n'));
