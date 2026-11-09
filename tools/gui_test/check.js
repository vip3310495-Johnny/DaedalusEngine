// Checks the state dumps of the scripted GUI test (tools/gui_test/gui.script).
// usage: node tools/gui_test/check.js <prefix>
const fs = require('fs');
const pre = process.argv[2];
let bad = 0, n = 0;
const load = k => { try { return JSON.parse(fs.readFileSync(`${pre}_${k}.json`, 'utf8')); } catch (e) { return null; } };
const ok = (cond, what, info) => { n++; if (!cond) bad++; console.log(`${cond ? ' OK ' : 'FAIL'} | ${what}${info !== undefined ? ' | ' + info : ''}`); };
const el = (d, id) => d && d.elements.find(e => e.id === id);
const labels = d => d ? d.elements.filter(e => e.typ === 'LABEL').length : -1;

const s1 = load('01_start');
ok(!!s1, '程式啟動並載入測試電路', s1 && `${s1.elements.length} 個元件`);
const s2 = load('02_paste_ghost');
ok(s2 && s2.mode === 'paste', '複製標籤後 Ctrl+V 進入貼上模式（灰色預覽跟著滑鼠）', s2 && s2.mode);
const s3 = load('03_pasted');
ok(labels(s3) === 2, '點擊後才貼上標籤', `標籤數 ${labels(s3)}`);
const s4 = load('04_label_space'), s5 = load('05_label_ctrlR');
ok(el(s4, 4) && el(s4, 4).rot === 1, '標籤：空白鍵旋轉', el(s4, 4) && `rot = ${el(s4, 4).rot}`);
ok(el(s5, 4) && el(s5, 4).rot === 2, '標籤：Ctrl+R 旋轉', el(s5, 4) && `rot = ${el(s5, 4).rot}`);
const s6 = load('06_ctxmenu'), s7 = load('07_ctx_rotated'), s8 = load('08_ctx_flipped');
ok(s6 && s6.ctx_menu === true, '右鍵選單開啟');
ok(el(s7, 3) && el(s7, 3).rot === 1 && s7.ctx_menu === false, '右鍵選單「旋轉」有作用並關閉選單', el(s7, 3) && `R1 rot = ${el(s7, 3).rot}`);
ok(el(s8, 3) && el(s8, 3).mir === true && el(s8, 3).rot === 3, '右鍵選單「上下鏡像」有作用', el(s8, 3) && `R1 rot = ${el(s8, 3).rot}, mir = ${el(s8, 3).mir}`);
const s8b = load('08b_undo');
ok(el(s8b, 3) && el(s8b, 3).rot === 0 && el(s8b, 3).mir === false, 'Ctrl+Z 兩次復原 R1 的旋轉與鏡像', el(s8b, 3) && `R1 rot = ${el(s8b, 3).rot}, mir = ${el(s8b, 3).mir}`);
const s9 = load('09_hand'), s10 = load('10_hand_dragged');
ok(s9 && s9.mode === 'hand', 'H 鍵進入手形工具', s9 && s9.mode);
const moved = s9 && s10 ? Math.hypot(s10.origin[0] - s9.origin[0], s10.origin[1] - s9.origin[1]) : 0;
const expect = s9 ? Math.hypot(4, 4) * s9.scale : 0;
ok(moved > 0.8 * expect && moved < 1.2 * expect && s10.elements.every((e, i) => e.x === s9.elements[i].x && e.y === s9.elements[i].y), '手形工具拖曳只移動視圖、不移動元件', `移動 ${moved.toFixed(1)} px（預期 ${expect.toFixed(1)}）`);
for (const [k, key, typ] of [['11_key_R', 'R', 'R'], ['12_key_C', 'C', 'C'], ['13_key_L', 'L', 'L'], ['14_key_D', 'D', 'DIODE'], ['15_key_P', 'P', 'VP'], ['16_key_N', 'N', 'LABEL'], ['17_key_G', 'G', 'GND']]) {
  const d = load(k);
  ok(d && d.mode === `place:${typ}`, `快捷鍵 ${key} 放置 ${typ}`, d && d.mode);
}
const w0 = load('20_wave_default');
ok(w0 && w0.wave.open && w0.wave.screens.length === 1 && w0.wave.screens[0].length === 0, '波形檢視器預設為一個空白畫面', w0 && JSON.stringify(w0.wave.screens));
const w1 = load('21_wave_math');
ok(w1 && w1.wave.math.length === 1 && w1.wave.math[0] === 'P2=Vout*2', '波形運算：加入 P2 = Vout*2；錯誤運算式被拒絕', w1 && JSON.stringify(w1.wave.math));
ok(w1 && JSON.stringify(w1.wave.screens) === JSON.stringify([['Vout', 'P2']]), '運算訊號自動加入目前畫面', w1 && JSON.stringify(w1.wave.screens));
const w2 = load('22_fft');
ok(w2 && w2.wave.fft === true, 'FFT 視窗開啟');
console.log(`\n${n - bad}/${n} GUI checks passed`);
process.exit(bad ? 1 : 0);
