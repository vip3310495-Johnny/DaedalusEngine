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
const vps = d => d ? d.elements.filter(e => e.typ === 'VP').length : -1;
const p18 = load('18_probe_paste');
ok(p18 && p18.mode === 'paste', '探棒（點符號選取）Ctrl+C／Ctrl+V 進入貼上模式（灰色預覽）', p18 && p18.mode);
const p18b = load('18b_probe_placed');
ok(vps(p18b) === 2, '點擊後貼上探棒', `探棒數 ${vps(p18b)}`);
const p19 = load('19_probe_text');
ok(p19 && p19.sel.length === 1 && p19.sel[0] === 5 && p19.inline_open === false, '點探棒名稱文字：第一次只選取、不進入改名', p19 && `sel = ${JSON.stringify(p19.sel)}, inline = ${p19.inline_open}`);
const p19b = load('19b_probe_text_paste');
ok(p19b && p19b.mode === 'paste', '以名稱選取探棒後 Ctrl+C／Ctrl+V 進入貼上模式', p19b && p19b.mode);
const p19c = load('19c_text_edit');
ok(p19c && p19c.inline_open === true, '已選取時再點一次名稱文字：就地改名', p19c && `inline = ${p19c.inline_open}`);
ok(p19c && p19c.inline_field_w >= 380, '就地修改輸入框寬度 ≈ 400（原 160 的 2.5 倍）', p19c && `${p19c.inline_field_w}`);
const p19d = load('19d_undo_paste');
ok(vps(p19d) === 1, '復原兩次貼上的探棒', `探棒數 ${vps(p19d)}`);
const p19e = load('19e_rdrag');
const rmv = p19d && p19e ? Math.hypot(p19e.origin[0] - p19d.origin[0], p19e.origin[1] - p19d.origin[1]) : 0;
const rexp = p19d ? Math.hypot(3, 3) * p19d.scale : 0;
ok(p19e && rmv > 0.8 * rexp && rmv < 1.2 * rexp && p19e.ctx_menu === false, '右鍵拖曳平移視圖、不開啟選單', `移動 ${rmv.toFixed(1)} px（預期 ${rexp.toFixed(1)}）`);
const p19f = load('19f_props');
ok(p19f && p19f.props_open === true && p19f.props_field_w >= 95, '屬性視窗數值輸入框寬度 ≈ 100（原 40 的 2.5 倍）', p19f && `${p19f.props_field_w}`);
const w0 = load('20_wave_default');
ok(w0 && w0.wave.open && w0.wave.screens.length === 1 && w0.wave.screens[0].length === 0, '波形檢視器預設為一個空白畫面', w0 && JSON.stringify(w0.wave.screens));
const w1 = load('21_wave_math');
ok(w1 && w1.wave.math.length === 1 && w1.wave.math[0] === 'P2=Vout*2', '波形運算：加入 P2 = Vout*2；錯誤運算式被拒絕', w1 && JSON.stringify(w1.wave.math));
ok(w1 && JSON.stringify(w1.wave.screens) === JSON.stringify([['Vout', 'P2']]), '運算訊號自動加入目前畫面', w1 && JSON.stringify(w1.wave.screens));
ok(w1 && w1.wave.colors && w1.wave.colors.Vout === '#e01060', '波形顏色可自訂', w1 && JSON.stringify(w1.wave.colors));
const w2 = load('22_fft');
ok(w2 && w2.wave.fft === true, 'FFT 視窗開啟');
const w3 = load('23_fft_scroll'), w4 = load('24_plot_scroll');
ok(w2 && w3 && JSON.stringify(w3.wave.x) === JSON.stringify(w2.wave.x), 'FFT 視窗上捲動滾輪不會縮放下方的波形', w3 && JSON.stringify(w3.wave.x));
ok(w3 && w4 && JSON.stringify(w4.wave.x) !== JSON.stringify(w3.wave.x), '（對照）波形圖未被遮住處捲動滾輪會縮放', w4 && JSON.stringify(w4.wave.x));
// File > Settings, Export view
const d25 = load('25_dbl_off'), d26 = load('26_dbl_on'), d27 = load('27_settings'), d28 = load('28_export');
ok(d25 && d25.quick_open === false, '預設關閉：空白處雙擊不會開啟快速加入元件', d25 && `quick = ${d25.quick_open}`);
ok(d26 && d26.quick_open === true && d26.settings.dbl_quick === true, '設定開啟後空白處雙擊會開啟快速加入元件', d26 && `quick = ${d26.quick_open}`);
ok(d26 && d26.settings.ui_font === 15 && d26.settings.applied === 15 && Math.abs(d26.label_k - 1.5) < 1e-6, '設定：介面文字 15 pt、元件文字 150 % 已套用', d26 && JSON.stringify(d26.settings) + ` label_k = ${d26.label_k}`);
ok(d27 && d27.settings.open === true, '「檔案 → 設定」視窗開啟（見截圖）');
const pdfOk = (file, min) => { try { const b = fs.readFileSync(file); return b.length > min && b.slice(0, 5).toString() === '%PDF-' ? b.length : 0; } catch (e) { return 0; } };
const sp = pdfOk(`${pre}_schematic.pdf`, 2000);
ok(d28 && d28.export_msg === 'ok' && sp > 0, '「檔案 → 匯出畫面」輸出電路圖 PDF', d28 && `${d28.export_msg}，${sp} bytes`);
// waveform viewer: fixed signal list, settings, report, XY stretch
const w29 = load('29_sig_before'), w30 = load('30_sig_after');
ok(w29 && w30 && w29.wave.sig_h > 50 && w29.wave.sig_h === w30.wave.sig_h && w30.wave.names.length === w29.wave.names.length + 1, '探棒訊號清單固定高度（加入訊號後高度不變）', w29 && w30 && `${w29.wave.sig_h} → ${w30.wave.sig_h}`);
const w31 = load('31_wave_settings');
ok(w31 && JSON.stringify(w31.wave.style) === JSON.stringify([15, 16, 13, 2.5]) && w31.wave.style_open === true, '波形檢視器設定：欄位 / 波形畫面 / 軸字體與線寬', w31 && JSON.stringify(w31.wave.style));
const w32 = load('32_report');
const rp = pdfOk(`${pre}_report.pdf`, 5000);
ok(rp > 0, '匯出報告：PDF', `${rp} bytes`);
let xl = null; try { xl = fs.readFileSync(`${pre}_report.xlsx`); } catch (e) {}
const xlOk = xl && xl.slice(0, 2).toString() === 'PK' && xl.includes('xl/charts/chart1.xml') && xl.includes('xl/worksheets/sheet2.xml');
ok(xlOk && w32 && /已匯出報告.*\.xlsx$/.test(w32.wave.report_msg), '匯出報告：Excel（圖表＋資料工作表）', w32 && `${w32.wave.report_msg}，${xl ? xl.length : 0} bytes`);
const w33 = load('33_stretch_x'), w34 = load('34_stretch_y');
const span = a => a ? a[1] - a[0] : NaN;
const rx = w32 && w33 ? span(w33.wave.x) / span(w32.wave.x) : NaN;
ok(w33 && w33.wave.tool === 'stretch' && rx > 0.45 && rx < 0.58, 'XY 縮放：向右拖曳 100 px 放大時間軸（約 1.95 倍）', `比例 ${rx.toFixed(3)}（預期 0.513）`);
const ry = w33 && w34 && w33.wave.ys[0] && w34.wave.ys[0] ? span(w34.wave.ys[0]) / span(w33.wave.ys[0]) : NaN;
ok(w34 && JSON.stringify(w34.wave.x) === JSON.stringify(w33.wave.x) && ry > 0.45 && ry < 0.58, 'XY 縮放：向上拖曳 100 px 只放大 Y 軸', `Y 比例 ${ry.toFixed(3)}（預期 0.513），X ${w34 && JSON.stringify(w34.wave.x) === JSON.stringify(w33.wave.x) ? '不變' : '改變'}`);
// waveform viewer: measurement columns, every / selected screen, scrolling screens, waveform file, all-screen report
const g39 = load('39_meas_all'), g40 = load('40_meas_one'), g41 = load('41_six_screens'), g42 = load('42_report_all'), g43 = load('43_saved'), g44 = load('44_reopened');
const mrow = (d, n) => d && d.wave.meas.find(r => r.name === n);
ok(g39 && JSON.stringify(g39.wave.meas_heads) === JSON.stringify(['Freq', 'DutyP', 'Rms']), '設定中選擇的量測欄位（Measure 1–3，第 4 欄不顯示）', g39 && JSON.stringify(g39.wave.meas_heads));
const vo = mrow(g39, 'Vout'), p3 = mrow(g39, 'P3');
ok(vo && Math.abs(vo.vals[0] / 1000 - 1) < 0.03 && Math.abs(vo.vals[1] - 50) < 5, '示波器量測：Vout 頻率 ≈ 1 kHz、正工作週期 ≈ 50 %', vo && `f = ${vo.vals[0].toFixed(2)} Hz, duty = ${vo.vals[1].toFixed(2)} %`);
ok(vo && p3 && Math.abs(p3.vals[2] / vo.vals[2] - 3) < 1e-9, '量測：P3 = 3·Vout 的 RMS 為 3 倍', vo && p3 && `${p3.vals[2]} / ${vo.vals[2]}`);
ok(g39 && g39.wave.meas_all && g39.wave.meas.map(r => r.name).join(',') === 'Vout,P2,P3' && g39.wave.cur_listed.join(',') === 'Vout,P2,P3', '量測、游標預設顯示所有畫面（同一訊號只列一次）', g39 && g39.wave.meas.map(r => r.name).join(','));
ok(g40 && !g40.wave.meas_all && !g40.wave.cur_all && g40.wave.meas.map(r => r.name).join(',') === 'P3' && g40.wave.cur_listed.join(',') === 'P3', '改為只顯示目前畫面（畫面 2：P3）', g40 && `${g40.wave.meas.map(r => r.name).join(',')} / ${g40.wave.cur_listed.join(',')}`);
const sh = g41 && g41.wave.screen_h;
ok(g41 && g41.wave.screens.length === 6 && sh && sh[0] > 60 && Math.abs(sh[1] - 6 * sh[0]) < 1 && g41.wave.act === 5, '6 個畫面：每個保持 4 個畫面時的高度，可上下捲動', sh && `每個 ${sh[0].toFixed(1)} px，全部 ${sh[1].toFixed(1)} px`);
const ra = pdfOk(`${pre}_report_all.pdf`, 8000);
let xa = null; try { xa = fs.readFileSync(`${pre}_report_all.xlsx`); } catch (e) {}
// 6 screens, 2 with signals: a chart for each of those, the 4 blank ones are only listed
const xaOk = xa && xa.includes('xl/charts/chart2.xml') && !xa.includes('xl/charts/chart3.xml');
ok(ra > 0 && xaOk && g42 && /已匯出報告.*report_all\.xlsx$/.test(g42.wave.report_msg), '匯出所有波形報告：PDF 與 Excel（有訊號的 2 個畫面各一張圖，空白畫面只列出）', `PDF ${ra} bytes，Excel ${xa ? xa.length : 0} bytes`);
let wf = null; try { wf = fs.readFileSync(`${pre}_wave.dwave`); } catch (e) {}
ok(wf && wf.slice(0, 8).toString() === 'DAEDWAVE' && g43 && /已儲存波形檔/.test(g43.wave.report_msg), '儲存波形檔（.dwave）', wf ? `${wf.length} bytes` : g43 && g43.wave.report_msg);
ok(g44 && g43 && g44.wave.open && JSON.stringify(g44.wave.names) === JSON.stringify(g43.wave.names) && JSON.stringify(g44.wave.screens) === JSON.stringify(g43.wave.screens) && g44.wave.act === g43.wave.act && JSON.stringify(g44.wave.style) === JSON.stringify(g43.wave.style) && JSON.stringify(g44.wave.meas_heads) === JSON.stringify(g43.wave.meas_heads) && JSON.stringify(g44.wave.x) === JSON.stringify(g43.wave.x) && JSON.stringify(g44.wave.math) === JSON.stringify(g43.wave.math) && JSON.stringify(g44.wave.meas) === JSON.stringify(g43.wave.meas), '重新開啟波形檔：訊號、畫面（含空白畫面）、選取的畫面、時間範圍、運算訊號、字體與量測設定都相同', g44 && `${g44.wave.names.join(',')} | ${JSON.stringify(g44.wave.screens)}`);
const g45 = load('45_lib_hidden'), g46 = load('46_lib_fly'), g47 = load('47_lib_closed'), g48 = load('48_lib_pinned');
const rect = (d, n) => d && d.lib.rects.find(r => r[0] === n);
ok(g45 && !g45.lib.pinned && !g45.lib.fly && rect(g45, 'lib_tab'), '元件庫取消釘選：隱藏成左緣的「元件庫」標籤', g45 && JSON.stringify(g45.lib));
const fr = rect(g46, 'lib_fly');
ok(g46 && g46.lib.fly && fr && fr[3] - fr[1] >= 280, '滑鼠移到標籤：元件庫展開在電路圖上', fr && `寬 ${(fr[3] - fr[1]).toFixed(0)} px`);
ok(g47 && !g47.lib.fly, '滑鼠離開後元件庫自動隱藏', g47 && JSON.stringify(g47.lib.fly));
ok(g48 && g48.lib.pinned && !rect(g48, 'lib_tab'), '重新釘選：元件庫固定在左側', g48 && JSON.stringify(g48.lib));
let man = ''; try { man = fs.readFileSync(`${pre}_manual.html`, 'utf8'); } catch (e) {}
const nEl = (man.match(/<article class="el"/g) || []).length, nFig = (man.match(/<figure>/g) || []).length;
const nLib = JSON.parse(fs.readFileSync(require('path').join(__dirname, '../../crates/daedalus-core/assets/library.json'), 'utf8')).length;
ok(/DaedalusEngine 使用說明書/.test(man) && nEl === nLib && nFig >= nLib - 4, '說明 › 使用說明書：程式內建 HTML 說明書（每個元件一張卡片與示範圖）', `${nEl} / ${nLib} 個元件、${nFig} 張圖、${(man.length / 1024).toFixed(0)} KB`);
const g49 = load('49_two_viewers');
const ws = g49 ? g49.waves.filter(w => w.open) : [];
ok(ws.length >= 2 && ws.some(w => w.names.includes('I_rc')) && ws.some(w => w.names.some(n => n.startsWith('Vout'))) && new Set(ws.map(w => w.doc)).size === ws.length, '模擬另一個電路時開新的波形檢視器，原本電路的結果保留在原視窗', g49 && JSON.stringify(ws.map(w => `${w.title}: ${w.names.slice(0, 3).join(',')}`)));
// Monte Carlo: tolerance table in the properties dialog, run with element parameters
const m35 = load('35_mc_empty'), m36 = load('36_mc_rows'), m37 = load('37_mc_applied'), m38 = load('38_mc_run');
ok(m35 && m35.props_mc && m35.props_mc.rows.length === 0, '蒙地卡羅屬性視窗：新元件沒有容差列（顯示加入說明）', m35 && JSON.stringify(m35.props_mc));
const pm = m36 && m36.props_mc;
ok(pm && JSON.stringify(pm.rows) === JSON.stringify(['R1.R', 'C1.C', 'Vs.Vm', 'R1']), '「加入所有 R、L、C」與「加入」產生容差列', pm && JSON.stringify(pm.rows));
ok(pm && pm.status.slice(0, 3).every(s => s[0]) && /950 Ω ~ 1.05 kΩ/.test(pm.status[0][1]), '每一列顯示抽樣範圍', pm && pm.status.slice(0, 3).map(s => s[1]).join(' / '));
ok(pm && pm.status[3] && !pm.status[3][0] && /重複/.test(pm.status[3][1]), '同一元件參數重複時標示錯誤（R1 即 R1.R）', pm && pm.status[3] && pm.status[3][1]);
const mcEl = m37 && m37.elements.find(e => e.typ === 'MONTECARLO');
ok(m37 && !m37.props_open && mcEl && mcEl.params.vars === 'R1.R 5% gauss\nC1.C 5% gauss\nVs.Vm 5% gauss' && mcEl.params.n === '4', '確定後容差表格寫回元件', mcEl && JSON.stringify(mcEl.params));
const mr = m38 && m38.mc;
const near = (a, b) => Math.abs(a / b - 1) < 1e-12;
const nomOk = mr && mr.samples.length === 4 && near(mr.samples[0][0], 1000) && near(mr.samples[0][1], 1e-6) && near(mr.samples[0][2], 1);
const inTol = mr && mr.samples.slice(1).every(s => Math.abs(s[0] / 1000 - 1) <= 0.05 + 1e-12 && Math.abs(s[1] / 1e-6 - 1) <= 0.05 + 1e-12 && Math.abs(s[2] - 1) <= 0.05 + 1e-12 && s[0] !== 1000);
ok(mr && JSON.stringify(mr.vars) === JSON.stringify(['R1.R', 'C1.C', 'Vs.Vm']) && nomOk && inTol && mr.open, '蒙地卡羅執行 4 次：#0 標稱，其餘在 ±5 % 內', mr ? JSON.stringify(mr.samples.map(s => s.map(x => +x.toPrecision(4)))) : m38 && JSON.stringify(m38.msgs));
console.log(`\n${n - bad}/${n} GUI checks passed`);
process.exit(bad ? 1 : 0);
