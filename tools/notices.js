// THIRD-PARTY-NOTICES.txt: copyright notices and license texts of every Rust crate linked into the shipped
// programs (DaedalusEngine.exe, daedalus-cli.exe, the installer) on Windows, from the crate sources in the cargo
// registry. Identical license texts are printed once. Where a crate offers a choice ("MIT OR Apache-2.0") the
// MIT terms are used.
// usage: node tools/notices.js [out.txt] [--offline] [--check]
//   --check: exit 1 if the file on disk differs from what would be written (nothing is written)
const fs = require('fs');
const path = require('path');
const crypto = require('crypto');
const { execFileSync } = require('child_process');

const ROOT = path.resolve(__dirname, '..');
const argv = process.argv.slice(2);
const OUT = path.resolve(ROOT, argv.find(a => !a.startsWith('--')) || 'THIRD-PARTY-NOTICES.txt');
const VERSION = 'V' + /version = "([^"]+)"/.exec(fs.readFileSync(path.join(ROOT, 'Cargo.toml'), 'utf8'))[1];
const SHIPPED = ['daedalus-app', 'daedalus-cli', 'daedalus-core', 'daedalus-installer'];

const meta = JSON.parse(execFileSync('cargo', ['metadata', '--format-version', '1', '--filter-platform', 'x86_64-pc-windows-msvc', '--locked']
  .concat(argv.includes('--offline') ? ['--offline'] : []), { cwd: ROOT, maxBuffer: 1 << 28 }).toString());
const ws = new Set(meta.workspace_members);
const pkgs = new Map(meta.packages.map(p => [p.id, p]));
const nodes = new Map(meta.resolve.nodes.map(n => [n.id, n]));

// crates reachable from the shipped programs through normal (not build or dev) dependencies
const seen = new Set();
const stack = meta.packages.filter(p => ws.has(p.id) && SHIPPED.includes(p.name)).map(p => p.id);
while (stack.length) {
  const id = stack.pop();
  if (seen.has(id)) continue;
  seen.add(id);
  for (const d of nodes.get(id).deps) if (d.dep_kinds.some(k => k.kind === null)) stack.push(d.pkg);
}
const crates = [...seen].filter(id => !ws.has(id)).map(id => pkgs.get(id))
  .sort((a, b) => a.name.localeCompare(b.name) || a.version.localeCompare(b.version, undefined, { numeric: true }));

const LICENSE_FILE = /^(licen[cs]e|copying|notice|copyright|unlicense|ofl|ufl)|licen[cs]e|^hack-regular\.txt$|^egui-icons\.txt$/i;
const SKIP_DIR = /^(tests?|benches|examples|target|src|\.git.*)$/i;
function licenseFiles(dir, depth = 0, rel = '') {
  let out = [];
  for (const e of fs.readdirSync(dir, { withFileTypes: true }).sort((a, b) => a.name.localeCompare(b.name))) {
    const r = rel ? rel + '/' + e.name : e.name;
    if (e.isDirectory()) {
      if (depth < 2 && !SKIP_DIR.test(e.name)) out = out.concat(licenseFiles(path.join(dir, e.name), depth + 1, r));
    } else if (LICENSE_FILE.test(e.name) && !/\.(rs|toml|ttf|otf|png|svg|json|ya?ml|sh|py)$/i.test(e.name)) {
      out.push({ rel: r, text: fs.readFileSync(path.join(dir, e.name), 'utf8').replace(/^﻿/, '').replace(/\r\n?/g, '\n').replace(/[ \t]+$/gm, '').trim() });
    }
  }
  return out;
}

const MIT_BODY = `Permission is hereby granted, free of charge, to any person obtaining a copy of this software and associated
documentation files (the "Software"), to deal in the Software without restriction, including without limitation the
rights to use, copy, modify, merge, publish, distribute, sublicense, and/or sell copies of the Software, and to permit
persons to whom the Software is furnished to do so, subject to the following conditions:

The above copyright notice and this permission notice shall be included in all copies or substantial portions of the
Software.

THE SOFTWARE IS PROVIDED "AS IS", WITHOUT WARRANTY OF ANY KIND, EXPRESS OR IMPLIED, INCLUDING BUT NOT LIMITED TO THE
WARRANTIES OF MERCHANTABILITY, FITNESS FOR A PARTICULAR PURPOSE AND NONINFRINGEMENT. IN NO EVENT SHALL THE AUTHORS OR
COPYRIGHT HOLDERS BE LIABLE FOR ANY CLAIM, DAMAGES OR OTHER LIABILITY, WHETHER IN AN ACTION OF CONTRACT, TORT OR
OTHERWISE, ARISING FROM, OUT OF OR IN CONNECTION WITH THE SOFTWARE OR THE USE OR OTHER DEALINGS IN THE SOFTWARE.`;

const isMit = t => /Permission is hereby granted, free of charge/i.test(t) && !/Apache License/i.test(t);
const isApache = t => /Apache License\s*\n?\s*Version 2\.0/i.test(t) && !/Permission is hereby granted, free of charge/i.test(t);
const authors = p => (p.authors || []).map(a => a.replace(/\s*<[^>]*>/, '')).join(', ');

const texts = new Map(); // hash -> crate label that printed it first
const sections = [];
let generated = 0;
for (const p of crates) {
  const dir = path.dirname(p.manifest_path);
  const label = `${p.name} ${p.version}`;
  const lic = p.license || (p.license_file ? 'see ' + p.license_file : 'unspecified');
  const files = licenseFiles(dir);
  // a choice that includes MIT ("MIT OR Apache-2.0", "Apache-2.0 OR MIT", "MIT/Apache-2.0"): the MIT terms
  const choice = !/\bAND\b/.test(lic) && /\bMIT\b/.test(lic) && /\bOR\b|\//.test(lic);
  let use = files;
  let note = '';
  if (choice) {
    use = files.filter(f => f.rel.includes('/') || isMit(f.text) || (!isApache(f.text) && !/licen[cs]e/i.test(f.rel)));
    note = '（選用 MIT 授權 / MIT terms elected）';
  }
  // MIT terms without a license file in the package: the standard text with the package authors
  // (a MIT text in a subfolder belongs to a bundled part, e.g. a font, not to the crate itself)
  if (/\bMIT\b/.test(lic) && !use.some(f => isMit(f.text) && !f.rel.includes('/'))) {
    use.unshift({ rel: 'MIT', text: `Copyright (c) ${authors(p) || 'the ' + p.name + ' authors'}\n\n${MIT_BODY}`, gen: true });
    generated++;
  }
  const lines = [`${label}`, `  License: ${lic}${note}`];
  if (p.repository) lines.push(`  Source:  ${p.repository}`);
  if (authors(p)) lines.push(`  Authors: ${authors(p)}`);
  for (const f of use) {
    const h = crypto.createHash('sha256').update(f.text).digest('hex');
    const name = f.gen ? 'MIT license (no license file in the package; copyright from its authors)' : f.rel;
    if (texts.has(h)) {
      lines.push('', `  [${name}] same text as ${texts.get(h)} above`);
    } else {
      texts.set(h, label);
      lines.push('', `  ---- ${name} ----`, '', f.text);
    }
  }
  if (!use.length) lines.push('', `  (no license file in the package; license: ${lic}, see https://spdx.org/licenses/)`);
  sections.push(lines.join('\n'));
}

const head = `DaedalusEngine ${VERSION} — 第三方元件授權聲明 / Third-party notices
${'='.repeat(78)}

DaedalusEngine 的程式（DaedalusEngine.exe、daedalus-cli.exe 與安裝程式）包含下列 ${crates.length} 個開源 Rust 套件。
各套件的著作權屬其作者所有，並依其各自的授權條款散布；以下列出每個套件的名稱、版本、授權、來源與授權全文
（相同的全文只列一次）。套件提供選擇性授權（例如「MIT OR Apache-2.0」）時，本程式選用 MIT 授權。
DaedalusEngine 本身的著作權與授權見 NOTICE.txt 與 LICENSE。

The DaedalusEngine programs include the ${crates.length} open-source Rust crates listed below. Each crate is copyright of
its authors and is distributed under its own license; for each crate the name, version, license, source and license
text are given (identical texts are printed once). Where a crate offers a choice of licenses, the MIT license is used.
This file is generated by tools/notices.js from the crate sources.
`;
const body = head + '\n' + sections.map(s => '-'.repeat(78) + '\n' + s).join('\n\n') + '\n';

if (argv.includes('--check')) {
  const cur = fs.existsSync(OUT) ? fs.readFileSync(OUT, 'utf8').replace(/\r\n/g, '\n') : '';
  if (cur !== body) {
    console.log(`${path.relative(ROOT, OUT)} is out of date: run node tools/notices.js`);
    process.exit(1);
  }
  console.log(`${path.relative(ROOT, OUT)} up to date (${crates.length} crates)`);
} else {
  fs.writeFileSync(OUT, body);
  console.log(`${path.relative(ROOT, OUT)}: ${crates.length} crates, ${texts.size} license texts, ${generated} MIT texts from package authors, ${(body.length / 1024).toFixed(0)} KB`);
}
