// End-to-end test of the MCP server (daedalus-cli mcp): talks JSON-RPC over stdio as an MCP client does and
// exercises every tool. Pictures and the full responses are written to the output folder for inspection.
// usage: node tools/mcp/test.js <daedalus-cli.exe> [out-dir]
'use strict';
const { spawn } = require('child_process');
const fs = require('fs');
const path = require('path');

const CLI = path.resolve(process.argv[2] || 'target/release/daedalus-cli.exe');
const OUT = path.resolve(process.argv[3] || 'out/mcp');
fs.rmSync(OUT, { recursive: true, force: true });
fs.mkdirSync(OUT, { recursive: true });
const log = fs.createWriteStream(path.join(OUT, 'log.txt'));

const srv = spawn(CLI, ['mcp'], { stdio: ['pipe', 'pipe', 'pipe'] });
let buf = '';
const waiting = new Map();
const stray = [];
srv.stdout.setEncoding('utf8');
srv.stdout.on('data', d => {
  buf += d;
  let i;
  while ((i = buf.indexOf('\n')) >= 0) {
    const line = buf.slice(0, i).trim();
    buf = buf.slice(i + 1);
    if (!line) continue;
    let m;
    try { m = JSON.parse(line); } catch (e) { stray.push(line); continue; }
    const w = waiting.get(m.id);
    if (w) { waiting.delete(m.id); w(m); } else stray.push(line);
  }
});
let stderr = '';
srv.stderr.on('data', d => { stderr += d; });

let seq = 0;
function rpc(method, params, timeout = 180000) {
  const id = ++seq;
  return new Promise((resolve, reject) => {
    const t = setTimeout(() => { waiting.delete(id); reject(new Error(`${method} timed out`)); }, timeout);
    waiting.set(id, m => { clearTimeout(t); resolve(m); });
    srv.stdin.write(JSON.stringify({ jsonrpc: '2.0', id, method, params }) + '\n');
  });
}
function notify(method, params) {
  srv.stdin.write(JSON.stringify({ jsonrpc: '2.0', method, params }) + '\n');
}
async function call(name, args, label) {
  const r = await rpc('tools/call', { name, arguments: args });
  if (r.error) throw new Error(`${name}: JSON-RPC error ${JSON.stringify(r.error)}`);
  const res = r.result;
  const text = res.content.filter(c => c.type === 'text').map(c => c.text).join('\n');
  const images = res.content.filter(c => c.type === 'image');
  log.write(`\n===== ${label || name} ${JSON.stringify(args).slice(0, 300)}\n${res.isError ? '[isError] ' : ''}${text}\n`);
  images.forEach((im, k) => {
    const f = path.join(OUT, `${(label || name).replace(/[^\w-]/g, '_')}${k ? '_' + k : ''}.png`);
    fs.writeFileSync(f, Buffer.from(im.data, 'base64'));
  });
  return { text, images, isError: res.isError };
}

let pass = 0, fail = 0;
function check(ok, what, detail) {
  if (ok) pass++; else fail++;
  console.log(`${ok ? 'ok  ' : 'FAIL'} ${what}${ok || !detail ? '' : ' — ' + String(detail).slice(0, 400)}`);
}
const isPng = im => im && Buffer.from(im.data, 'base64').slice(0, 4).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47]));
const SI = { f: 1e-15, p: 1e-12, n: 1e-9, 'µ': 1e-6, u: 1e-6, m: 1e-3, '': 1, k: 1e3, M: 1e6, G: 1e9 };
// mean column of a probe in the statistics table: | Vo | 12.00 V | ...
function mean(text, probe) {
  const m = new RegExp(`\\| ${probe} \\| (-?[\\d.]+(?:e-?\\d+)?) ?([fpnµumkMG]?)`).exec(text);
  return m ? parseFloat(m[1]) * SI[m[2]] : NaN;
}

const buck = {
  title: '24 V → 12 V Buck（MCP 測試）',
  sim: { dt: '0.2u', tt: '10m' },
  params: 'Vin = 24\nfsw = 50k',
  elements: [
    { type: 'VDC', name: 'Vin', params: { V: 'Vin' }, pins: { '+': 'VIN', '-': '0' } },
    { type: 'MOSFET', name: 'Q1', pins: { D: 'VIN', S: 'SW', G: 'GATE' } },
    { type: 'DIODE', name: 'Dfw', pins: { A: '0', K: 'SW' } },
    { type: 'L', name: 'L1', params: { L: '100u' }, pins: { 1: 'SW', 2: 'LO' } },
    { type: 'IP', name: 'IL', pins: { in: 'LO', out: 'VO' } },
    { type: 'RC', name: 'Co', params: { R: '10m', C: '100u' }, pins: { 1: 'VO', 2: '0' } },
    { type: 'R', name: 'RL', params: { R: 6 }, pins: { 1: 'VO', 2: '0' } },
    { type: 'VP', name: 'Vo', pins: { n: 'VO' } },
    { type: 'CONST', name: 'Duty', params: { v: 0.5 }, pins: { out: 'D' } },
    { type: 'VTRI', name: 'Vcar', params: { Vpp: 1, f: 'fsw' }, pins: { '+': 'CAR', '-': '0' } },
    { type: 'COMP', name: 'PWM', pins: { 'in+': 'D', 'in-': 'CAR', out: 'GATE' } },
  ],
  screens: [['Vo'], ['IL']],
};

(async () => {
  const init = await rpc('initialize', { protocolVersion: '2025-06-18', capabilities: {}, clientInfo: { name: 'daedalus-test', version: '1' } });
  check(init.result && init.result.serverInfo.name === 'daedalus' && /daedalus_new/.test(init.result.instructions), 'initialize', JSON.stringify(init));
  notify('notifications/initialized', {});
  const list = await rpc('tools/list', {});
  const names = (list.result.tools || []).map(t => t.name);
  check(names.length === 10 && names.includes('daedalus_simulate'), `tools/list (${names.length} tools)`, names.join(','));
  check((await rpc('ping', {})).result !== undefined, 'ping');

  let r = await call('daedalus_guide', { topic: 'netlist' });
  check(!r.isError && /pins/.test(r.text) && /"0"/.test(r.text), 'guide netlist', r.text);
  r = await call('daedalus_elements', { query: '比較器' });
  check(!r.isError && /COMP/.test(r.text), 'elements search', r.text);
  r = await call('daedalus_elements', { type: 'mosfet' });
  check(!r.isError && /G ［閘極控制］/.test(r.text) && /ron/.test(r.text), 'elements detail', r.text);
  r = await call('daedalus_elements', {});
  check(!r.isError && /全部 223 種/.test(r.text), 'elements index', r.text.slice(0, 200));

  const P = f => path.join(OUT, f);
  r = await call('daedalus_new', { path: P('buck.psimx'), netlist: buck, render: true }, 'new');
  check(!r.isError && /問題：無/.test(r.text) && isPng(r.images[0]), 'new: built, no problems, picture', r.text);
  const nb = JSON.parse(fs.readFileSync(P('buck.psimx'), 'utf8'));
  const nlab = nb.doc.elements.filter(e => e.type === 'LABEL').length;
  check(nlab <= 2 && nb.nets && nb.nets.SW, `new: drawn with wires (${nlab} labels), net names kept in the file`, JSON.stringify(nb.nets));
  r = await call('daedalus_new', { path: P('buck.psimx'), netlist: buck }, 'new_again');
  check(r.isError && /overwrite/.test(r.text), 'new refuses to overwrite', r.text);
  r = await call('daedalus_inspect', { path: P('buck.psimx') });
  check(!r.isError && /SW：/.test(r.text) && /Q1\.S/.test(r.text) && /GATE［控制］/.test(r.text), 'inspect: nets', r.text);
  r = await call('daedalus_inspect', { path: P('buck.psimx'), element: 'Q1' }, 'inspect_q1');
  check(!r.isError && /G ［閘極控制］/.test(r.text) && /GATE/.test(r.text), 'inspect element', r.text);

  r = await call('daedalus_simulate', { path: P('buck.psimx'), measures: ['CycMean', 'Pp'], image: true }, 'simulate');
  const vo = mean(r.text, 'Vo'), il = mean(r.text, 'IL');
  check(!r.isError && Math.abs(vo - 12) < 0.5 && Math.abs(il - 2) < 0.1 && isPng(r.images[0]), `simulate: Vo ${vo} V, IL ${il} A`, r.text);
  r = await call('daedalus_simulate', { path: P('buck.psimx'), probes: ['Vo'], from: '9m', to: '10m', measures: ['Freq'] }, 'simulate_window');
  check(!r.isError && /9 ms ～ 10 ms/.test(r.text) && !/\| IL \|/.test(r.text), 'simulate: window and probe filter', r.text);
  r = await call('daedalus_waveform', { path: P('buck.psimx'), table: true, points: 20 }, 'waveform');
  check(!r.isError && isPng(r.images[0]) && /\| t \|/.test(r.text), 'waveform: picture and table', r.text.slice(0, 300));
  r = await call('daedalus_waveform', { path: P('buck.psimx'), probes: ['IL'], from: '9.9m' }, 'waveform_zoom');
  check(!r.isError && isPng(r.images[0]), 'waveform: zoom', r.text);

  r = await call('daedalus_edit', { path: P('buck.psimx'), ops: [{ op: 'add', type: 'VP', name: 'Vsw', pins: { n: 'SW' } }, { op: 'set', name: 'RL', params: { R: '12' } }, { op: 'screens', screens: [['Vo'], ['IL'], ['Vsw']] }], render: true }, 'edit');
  check(!r.isError && /Vsw\.n：新增，SW/.test(r.text) && /問題：無/.test(r.text), 'edit: add probe, change load', r.text);
  r = await call('daedalus_simulate', { path: P('buck.psimx') }, 'simulate_after_edit');
  const il2 = mean(r.text, 'IL');
  check(!r.isError && Math.abs(il2 - 1) < 0.08 && /\| Vsw \|/.test(r.text), `simulate after edit: IL ${il2} A`, r.text);
  r = await call('daedalus_edit', { path: P('buck.psimx'), ops: [{ op: 'connect', pin: 'Q1.X', net: 'A' }] }, 'edit_bad');
  check(r.isError && /沒有接腳/.test(r.text), 'edit: bad pin is explained', r.text);
  r = await call('daedalus_edit', { path: P('buck.psimx'), ops: [{ op: 'remove', name: 'Vsw' }], dry_run: true }, 'edit_dry');
  check(!r.isError && /dry_run/.test(r.text), 'edit: dry run', r.text);
  r = await call('daedalus_edit', { path: P('buck.psimx'), ops: [{ op: 'connect', pin: 'Vsw.n', net: 'GATE' }], save_as: P('buck_bad.psimx') }, 'edit_mix');
  check(!r.isError && /功率電路與控制電路直接相連/.test(r.text) && fs.existsSync(P('buck_bad.psimx')), 'edit: power/control mix reported', r.text);

  r = await call('daedalus_render', { path: P('buck.psimx'), highlight: 'SW', pins: true, save: P('buck.svg') }, 'render');
  check(!r.isError && isPng(r.images[0]) && fs.readFileSync(P('buck.svg'), 'utf8').startsWith('<svg'), 'render: picture and svg', r.text);

  // subcircuits: a divider definition (with a probe inside) used by two blocks, one with its own value
  const sub = {
    title: '子電路（MCP 測試）', sim: { dt: '1u', tt: '20u' },
    subckts: {
      DIV: {
        params: 'Rt = 10\nRb = 10',
        ports: [{ name: 'in', net: 'A' }, { name: 'out', net: 'B' }],
        elements: [
          { type: 'R', name: 'R1', params: { R: 'Rt' }, pins: { 1: 'A', 2: 'B' } },
          { type: 'R', name: 'R2', params: { R: 'Rb' }, pins: { 1: 'B', 2: '0' } },
          { type: 'VP', name: 'Vm', pins: { n: 'B' } },
        ],
      },
    },
    elements: [
      { type: 'VDC', name: 'V1', params: { V: '12' }, pins: { '+': 'VIN', '-': '0' } },
      { type: 'SUBCKT', name: 'X1', params: { def: 'DIV' }, pins: { in: 'VIN', out: 'O1' } },
      { type: 'SUBCKT', name: 'X2', params: { def: 'DIV', vars: 'Rb = 30' }, pins: { in: 'VIN', out: 'O2' } },
      { type: 'VP', name: 'Vo1', pins: { n: 'O1' } },
      { type: 'VP', name: 'Vo2', pins: { n: 'O2' } },
    ],
  };
  r = await call('daedalus_new', { path: P('sub.psimx'), netlist: sub, render: true }, 'sub_new');
  check(!r.isError && /問題：無/.test(r.text) && /DIV：in［L］ out［R］/.test(r.text) && isPng(r.images[0]), 'subcircuit: built from a netlist, definition listed', r.text);
  r = await call('daedalus_simulate', { path: P('sub.psimx') }, 'sub_simulate');
  const s1 = mean(r.text, 'Vo1'), s2 = mean(r.text, 'Vo2'), s3 = mean(r.text, 'X2\\.Vm');
  check(!r.isError && Math.abs(s1 - 6) < 1e-6 && Math.abs(s2 - 9) < 1e-6 && Math.abs(s3 - 9) < 1e-6, `subcircuit: Vo1 ${s1} V, Vo2 ${s2} V (own value), probe inside X2.Vm ${s3} V`, r.text);
  r = await call('daedalus_edit', { path: P('sub.psimx'), ops: [{ op: 'set', name: 'X1', params: { vars: 'Rb = 20' } }] }, 'sub_edit');
  r = await call('daedalus_simulate', { path: P('sub.psimx') }, 'sub_simulate2');
  const s4 = mean(r.text, 'Vo1');
  check(!r.isError && Math.abs(s4 - 8) < 1e-6, `subcircuit: block value changed by edit, Vo1 ${s4} V`, r.text);
  r = await call('daedalus_inspect', { path: P('sub.psimx'), subckt: 'DIV' }, 'sub_inspect_def');
  check(!r.isError && /子電路 DIV：埠 in［L，功率］ out［R，功率］/.test(r.text) && /out（子電路埠 out）/.test(r.text) && /問題：無/.test(r.text), 'subcircuit: inspect a definition (ports, nets)', r.text);
  // inside the definition: change the top resistor of every block, add a port, then a new definition with a block
  r = await call('daedalus_edit', { path: P('sub.psimx'), ops: [
    { op: 'params', subckt: 'DIV', text: 'Rt = 20\nRb = 10' },
    { op: 'add', subckt: 'DIV', type: 'SUBPORT', params: { text: 'mid', side: 'B' }, pins: { port: 'out' } },
    { op: 'subckt_new', name: 'SER', ports: [{ name: 'a', net: 'A' }, { name: 'b', net: 'B' }], elements: [{ type: 'R', name: 'R1', params: { R: '5' }, pins: { 1: 'A', 2: 'B' } }] },
    { op: 'add', type: 'SUBCKT', name: 'X3', params: { def: 'SER' }, pins: { a: 'O1', b: 'O3' } },
    { op: 'add', type: 'R', name: 'R3', params: { R: '5' }, pins: { 1: 'O3', 2: '0' } },
    { op: 'add', type: 'VP', name: 'Vo3', pins: { n: 'O3' } },
  ], render: true }, 'sub_edit_def');
  check(!r.isError && /［子電路 DIV］新增 port:mid/.test(r.text) && /子電路 DIV：埠 in［L，功率］ out［R，功率］ mid［B，功率］/.test(r.text) && /新增子電路 SER/.test(r.text) && isPng(r.images[0]), 'subcircuit: edit inside a definition, add a port, new definition', r.text);
  r = await call('daedalus_simulate', { path: P('sub.psimx') }, 'sub_simulate3');
  // DIV: Rt = 20; X1 Rb = 20 → 6 V loaded by X3+R3 (10 Ω): out = 12·(20∥10)/(20 + 20∥10) = 3.0 V; X2 Rb = 30 → 12·30/50 = 7.2 V
  const t1 = mean(r.text, 'Vo1'), t2 = mean(r.text, 'Vo2'), t3 = mean(r.text, 'Vo3');
  check(!r.isError && Math.abs(t1 - 3) < 1e-6 && Math.abs(t2 - 7.2) < 1e-6 && Math.abs(t3 - 1.5) < 1e-6, `subcircuit: definition change reaches every block: Vo1 ${t1}, Vo2 ${t2}, Vo3 ${t3} V`, r.text);
  r = await call('daedalus_edit', { path: P('sub.psimx'), ops: [{ op: 'add', subckt: 'SER', type: 'SUBCKT', name: 'XL', params: { def: 'SER' }, pins: { a: 'A', b: null } }], dry_run: true }, 'sub_edit_loop');
  check(r.isError && /包含它自己/.test(r.text), 'subcircuit: a definition cannot contain itself', r.text);
  r = await call('daedalus_render', { path: P('sub.psimx'), subckt: 'DIV' }, 'sub_render_def');
  check(!r.isError && isPng(r.images[0]) && /子電路 DIV/.test(r.text), 'subcircuit: render a definition', r.text);

  r = await call('daedalus_examples', {});
  check(!r.isError && /buckac/.test(r.text), 'examples: list', r.text.slice(0, 200));
  r = await call('daedalus_examples', { key: 'buck', copy_to: P('example_buck.psimx') }, 'example_buck');
  check(!r.isError && /問題：無/.test(r.text), 'examples: copy (hand-drawn circuit)', r.text);
  r = await call('daedalus_edit', { path: P('example_buck.psimx'), ops: [{ op: 'remove', name: 'Vo' }, { op: 'add', type: 'VP', name: 'Vsw', pins: { n: 'D1.K' } }] }, 'edit_example');
  check(!r.isError && /問題：無/.test(r.text) && /Vsw\.n：新增/.test(r.text), 'edit hand-drawn: remove probe on a rail, probe by pin reference', r.text);
  r = await call('daedalus_render', { path: P('example_buck.psimx') }, 'render_example');
  check(!r.isError && isPng(r.images[0]), 'render hand-drawn', r.text);
  r = await call('daedalus_examples', { key: 'buckac', copy_to: P('buckac.psimx') }, 'example_buckac');
  r = await call('daedalus_simulate', { path: P('buckac.psimx') }, 'simulate_ac');
  check(!r.isError && /AC 掃頻完成/.test(r.text) && /dB/.test(r.text), 'simulate: AC sweep', r.text);
  r = await call('daedalus_waveform', { path: P('buckac.psimx') }, 'bode');
  check(!r.isError && isPng(r.images[0]), 'waveform: Bode plot', r.text);

  r = await call('daedalus_inspect', { path: P('no_such.psimx') }, 'missing');
  check(r.isError && /無法讀取/.test(r.text), 'missing file is an error result', r.text);
  const bad = await rpc('tools/call', { name: 'no_such_tool', arguments: {} });
  check(bad.error && bad.error.code === -32602, 'unknown tool', JSON.stringify(bad));
  const nf = await rpc('no/such', {});
  check(nf.error && nf.error.code === -32601, 'unknown method', JSON.stringify(nf));
  check(stray.length === 0, 'stdout carries only JSON-RPC responses', stray.join('\n'));

  // the same tools on the command line (for the skill)
  const { spawnSync } = require('child_process');
  fs.writeFileSync(P('nl.json'), JSON.stringify(buck));
  let c = spawnSync(CLI, ['new', P('nl.json'), P('cli_buck.psimx')], { encoding: 'utf8' });
  check(c.status === 0 && /問題：無/.test(c.stdout), 'cli: new from a netlist file', c.stdout + c.stderr);
  c = spawnSync(CLI, ['render', P('cli_buck.psimx'), P('cli_buck.png'), '--pins'], { encoding: 'utf8' });
  check(c.status === 0 && fs.existsSync(P('cli_buck.png')) && fs.readFileSync(P('cli_buck.png')).slice(0, 4).equals(Buffer.from([0x89, 0x50, 0x4e, 0x47])), 'cli: render to png', c.stdout + c.stderr);
  c = spawnSync(CLI, ['inspect', P('cli_buck.psimx')], { encoding: 'utf8' });
  check(c.status === 0 && /SW：/.test(c.stdout), 'cli: inspect', c.stdout + c.stderr);

  srv.stdin.end();
  const code = await new Promise(res => srv.on('exit', res));
  check(code === 0, `server exits when stdin closes (code ${code})`, stderr);
  log.end();
  console.log(`${pass}/${pass + fail} MCP checks passed`);
  process.exit(fail ? 1 : 0);
})().catch(e => {
  console.log('FAIL', e.stack || e);
  console.log(stderr);
  srv.kill();
  process.exit(1);
});
