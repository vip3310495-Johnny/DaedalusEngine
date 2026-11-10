// Element tables of the circuit-builder skill (skills/daedalus-circuit-builder), generated from the library so
// they always match the program:
//   scripts/elements.json      pins (name, x, y, kind at rot 0) and parameter defaults of every element type
//   references/elements.md     the same as a readable reference, grouped by category
// usage: node tools/skill/gen.js          write both files
//        node tools/skill/gen.js --check  fail when the committed files differ from the library (CI)
'use strict';
const fs = require('fs');
const path = require('path');
const ROOT = path.resolve(__dirname, '..', '..');
const LIB = JSON.parse(fs.readFileSync(path.join(ROOT, 'crates/daedalus-core/assets/library.json'), 'utf8'));
const DIR = path.join(ROOT, 'skills/daedalus-circuit-builder');
const KIND = { p: 'P', ci: 'Ci', co: 'Co', g: 'G', a: 'A', gnd: 'gnd', lbl: 'lbl' };
const CAT = {
  'Elements/Power/RLC Branches': '功率元件 › RLC 支路', 'Elements/Power/Switches': '功率元件 › 開關', 'Elements/Power/Non-ideal Switches': '功率元件 › 非理想開關',
  'Elements/Power/Transformers': '功率元件 › 變壓器', 'Elements/Power/Other': '功率元件 › 其他', 'Elements/Control': '控制元件', 'Elements/Control/Filters': '控制元件 › 濾波器',
  'Elements/Control/Computational Blocks': '控制元件 › 運算方塊', 'Elements/Control/Other Function Blocks': '控制元件 › 其他功能方塊', 'Elements/Control/Logic Elements': '控制元件 › 邏輯元件',
  'Elements/Control/Digital Control Module': '控制元件 › 數位控制模組', 'Elements/Other': '其他 › 分析與模擬設定', 'Elements/Other/Switch Controllers': '其他 › 開關控制器', 'Elements/Other/Sensors': '其他 › 感測器',
  'Elements/Other/Probes': '其他 › 探棒與量表', 'Elements/Other/Function Blocks': '其他 › 函數方塊', 'Elements/Sources/Voltage': '電源 › 電壓源', 'Elements/Sources/Current': '電源 › 電流源', 'Elements/Symbols': '符號',
};

function tables() {
  const els = {};
  for (const d of LIB) {
    els[d.type] = {
      zh: d.zh,
      name: d.name,
      pins: d.pins.map(p => [p.n, p.x, p.y, KIND[p.k] || p.k]),
      params: Object.fromEntries((d.params || []).map(p => [p.k, String(p.d)])),
      flags: d.flags || [],
    };
  }
  const json = JSON.stringify({ generated: 'tools/skill/gen.js from crates/daedalus-core/assets/library.json', elements: els }, null, 0).replace(/\},"/g, '},\n"') + '\n';
  const fmtParam = p => {
    const v = String(p.d).replace(/\n/g, '⏎');
    return `${p.k}=${v.length > 40 ? v.slice(0, 40) + '…' : v}${p.u && !/^[⏎]/.test(p.u) ? p.u : ''}${p.adv ? '（進階）' : ''}`;
  };
  let md = '# 元件參考（由元件庫自動產生，請勿手動修改）\n\n每列：`型別代碼` 中文名 — 接腳 名稱(x,y)類型（rot=0、未鏡像）— 參數=預設值。類型：P 功率、Ci 控制輸入、Co 控制輸出、G 閘極（控制訊號）、A 功率或控制。CBLOCK 的接腳依 nin／nout 變化（見 SKILL.md）。\n';
  const cats = [];
  for (const d of LIB) if (!cats.includes(d.cat)) cats.push(d.cat);
  for (const c of cats) {
    md += `\n## ${CAT[c] || c}\n\n`;
    for (const d of LIB.filter(x => x.cat === c)) {
      const pins = d.pins.length ? d.pins.map(p => `${p.n}(${p.x},${p.y})${KIND[p.k] || p.k}`).join(' ') : '—';
      const ps = (d.params || []).map(fmtParam).join(', ');
      const desc = d.desc.replace(/\s+/g, ' ').slice(0, 160) + (d.desc.length > 160 ? '…' : '');
      md += `- \`${d.type}\` ${d.zh} — pins: ${pins}${ps ? ' — ' + ps : ''}\n  ${desc}\n`;
    }
  }
  return { json, md };
}

const { json, md } = tables();
const files = [[path.join(DIR, 'scripts/elements.json'), json], [path.join(DIR, 'references/elements.md'), md]];
if (process.argv.includes('--check')) {
  const stale = files.filter(([f, t]) => !fs.existsSync(f) || fs.readFileSync(f, 'utf8') !== t).map(([f]) => path.relative(ROOT, f));
  if (stale.length) {
    console.log('skill tables out of date (run node tools/skill/gen.js): ' + stale.join(', '));
    process.exit(1);
  }
  console.log(`skill tables up to date (${LIB.length} element types)`);
} else {
  for (const [f, t] of files) {
    fs.mkdirSync(path.dirname(f), { recursive: true });
    fs.writeFileSync(f, t);
  }
  console.log(`wrote skill tables: ${LIB.length} element types`);
}
