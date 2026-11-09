//! Waveform viewer (SIMVIEW-style): stacked screens sharing one time axis, cursors, measurements.

use crate::pdf_export::{Capture, Page, Part, A4};
use crate::plot::{curve_points, si, Axes};
use crate::settings::WaveStyle;
use crate::theme::Pal;
use crate::wave_report;
use crate::wave_tools::{eval_math, FftView, MathAct, MathDef, MathDlg};
use daedalus_core::runner::TranResult;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Zoom,
    Pan,
    Box,
    /// drag left / right stretches the time axis, up / down the Y axis (anchored where the drag started)
    Stretch,
}
#[derive(Clone, Default)]
pub struct Screen {
    pub curves: Vec<usize>,
    pub y: Option<(f64, f64)>,
}
enum WDrag {
    XZoom(f32),
    Box(Pos2),
    Pan,
    Cursor(u8),
    Stretch { p0: Pos2, x0: (f64, f64), y0: (f64, f64) },
}

pub struct WaveView {
    pub res: Option<Arc<TranResult>>,
    pub screens: Vec<Screen>,
    pub act: usize,
    pub x: (f64, f64),
    pub cursors: bool,
    pub c1: Option<f64>,
    pub c2: Option<f64>,
    pub tool: Tool,
    pub open: bool,
    drag: Option<(usize, WDrag)>,
    hover_t: Option<f64>,
    pub title: String,
    /// math channels (re-evaluated for every new result) and the number of simulated signals before them
    pub math: Vec<MathDef>,
    /// user-chosen curve colours by signal name (kept across runs and files, saved with the app settings)
    pub colors: HashMap<String, Color32>,
    nbase: usize,
    math_dlg: Option<MathDlg>,
    math_err: String,
    fft: Option<FftView>,
    pending: Vec<Pending>,
    /// fonts and line width (saved with the app settings, also used for the report)
    pub style: WaveStyle,
    style_open: bool,
    /// report to write in this frame (from the save dialog or a GUI-test script) and the last outcome
    pub report_req: Option<PathBuf>,
    pub report_msg: String,
    /// height of the signal list (GUI test)
    sig_h: f32,
}

/// pages of the PDF report, painted with egui on off-screen layers
struct Sheet {
    ctx: egui::Context,
    cap: Option<Capture>,
    pages: Vec<Page>,
    y: f32,
    m: f32,
}

impl Sheet {
    fn page_rect() -> Rect {
        Rect::from_min_size(Pos2::ZERO, vec2(A4.0, A4.1))
    }
    fn new(ctx: &egui::Context, m: f32) -> Self {
        Sheet { ctx: ctx.clone(), cap: Some(Capture::new(ctx, ("report", 0usize), Self::page_rect())), pages: vec![], y: m, m }
    }
    fn p(&self) -> &egui::Painter {
        &self.cap.as_ref().expect("page").painter
    }
    /// room for `h` points on this page, else a new page
    fn need(&mut self, h: f32) {
        if self.y + h > A4.1 - self.m {
            self.flush();
            let n = self.pages.len();
            self.cap = Some(Capture::new(&self.ctx, ("report", n), Self::page_rect()));
            self.y = self.m;
        }
    }
    fn flush(&mut self) {
        if let Some(c) = self.cap.take() {
            self.pages.push(Page { size: A4, parts: vec![Part { shapes: c.finish(), scale: 1.0, offset: egui::Vec2::ZERO }] });
        }
    }
    fn heading(&mut self, s: &str, fs: f32, pal: &Pal) {
        self.need(fs * 4.0);
        let r = self.p().text(pos2(self.m, self.y), Align2::LEFT_TOP, s, FontId::proportional(fs + 1.0), pal.text);
        self.y = r.max.y + 6.0;
    }
    fn line(&mut self, s: &str, fs: f32, col: Color32) {
        self.need(fs * 1.6);
        let r = self.p().text(pos2(self.m, self.y), Align2::LEFT_TOP, s, FontId::proportional(fs), col);
        self.y = r.max.y + 4.0;
    }
    /// header row and rows of (text, colour): the first column left-aligned, the others right-aligned
    fn table(&mut self, w: f32, head: &[&str], rows: &[Vec<(String, Color32)>], fs: f32, pal: &Pal) {
        let x = self.m;
        let font = FontId::proportional(fs);
        let rh = (fs * 1.65).round();
        let measure = |p: &egui::Painter, s: &str| p.layout_no_wrap(s.to_string(), font.clone(), Color32::BLACK).size().x;
        let mut c0 = measure(self.p(), head[0]);
        for r in rows {
            c0 = c0.max(measure(self.p(), &r[0].0));
        }
        let c0 = (c0 + 16.0).min(w * 0.4);
        let nc = (head.len() - 1).max(1);
        let cw = (w - c0) / nc as f32;
        let right = |k: usize| x + c0 + cw * k as f32 - 6.0;
        self.need(rh * 2.0);
        let y = self.y;
        self.p().rect_filled(Rect::from_min_size(pos2(x, y), vec2(w, rh)), 0.0, pal.panel2);
        for (k, h) in head.iter().enumerate() {
            let (pos, al) = if k == 0 { (pos2(x + 6.0, y + rh / 2.0), Align2::LEFT_CENTER) } else { (pos2(right(k), y + rh / 2.0), Align2::RIGHT_CENTER) };
            self.p().text(pos, al, *h, font.clone(), pal.muted);
        }
        self.p().line_segment([pos2(x, y + rh), pos2(x + w, y + rh)], Stroke::new(0.8, pal.faint));
        self.y += rh;
        for row in rows {
            self.need(rh);
            let y = self.y;
            for (k, (t, c)) in row.iter().enumerate() {
                let (pos, al) = if k == 0 { (pos2(x + 6.0, y + rh / 2.0), Align2::LEFT_CENTER) } else { (pos2(right(k), y + rh / 2.0), Align2::RIGHT_CENTER) };
                self.p().text(pos, al, t, font.clone(), *c);
            }
            self.p().line_segment([pos2(x, y + rh), pos2(x + w, y + rh)], Stroke::new(0.5, pal.line));
            self.y += rh;
        }
    }
}

/// edits of the signal list, applied at the start of the next frame (when no other reference to the result is
/// alive, so the math channels are appended in place)
enum Pending {
    AddMath(String, String),
    RemoveMath(usize),
}

impl WaveView {
    pub fn new() -> Self {
        WaveView { res: None, screens: vec![], act: 0, x: (0.0, 1.0), cursors: false, c1: None, c2: None, tool: Tool::Zoom, open: false, drag: None, hover_t: None, title: String::new(), math: vec![], colors: HashMap::new(), nbase: 0, math_dlg: None, math_err: String::new(), fft: None, pending: vec![], style: WaveStyle::default(), style_open: false, report_req: None, report_msg: String::new(), sig_h: 0.0 }
    }
    fn full_x(r: &TranResult) -> (f64, f64) {
        (r.t0, r.t0 + (r.n.max(2) - 1) as f64 * r.dt)
    }
    /// screen layout as probe names (saved with the circuit file)
    pub fn layout_names(&self) -> Option<Vec<Vec<String>>> {
        let r = self.res.as_ref()?;
        Some(self.screens.iter().map(|s| s.curves.iter().filter_map(|&i| r.names.get(i).cloned()).collect()).collect())
    }
    pub fn set_result(&mut self, res: Arc<TranResult>, layout: Option<Vec<Vec<String>>>) {
        let prev_names: Option<Vec<Vec<String>>> = self.layout_names();
        let mut res = res;
        self.nbase = res.names.len();
        self.math_err.clear();
        if !self.math.is_empty() {
            let defs = std::mem::take(&mut self.math);
            let r = Arc::make_mut(&mut res);
            for m in defs {
                match eval_math(r, r.names.len(), &m.expr) {
                    Ok(v) if !r.names.contains(&m.name) => {
                        r.names.push(m.name.clone());
                        r.data.push(v);
                        self.math.push(m);
                    }
                    Ok(_) => self.math_err = format!("運算訊號「{}」與探棒同名，已移除。", m.name),
                    Err(e) => self.math_err = format!("運算訊號「{}」無法計算（{}），已移除。", m.name, e),
                }
            }
        }
        let names = &res.names;
        let from_names = |l: &Vec<Vec<String>>| -> Vec<Screen> {
            l.iter()
                .map(|s| Screen { curves: s.iter().filter_map(|n| names.iter().position(|m| m == n)).collect(), y: None })
                .filter(|s| !s.curves.is_empty())
                .collect()
        };
        let mut screens = prev_names.as_ref().map(from_names).unwrap_or_default();
        if screens.is_empty() {
            if let Some(l) = &layout {
                screens = from_names(l);
            }
        }
        if screens.is_empty() {
            if let Some(sw) = &res.sweep {
                // parameter sweep: every run of the same probe overlaid on one screen
                let nv = sw.labels.len().max(1);
                screens = (0..sw.probes).map(|j| Screen { curves: (j * nv..(j + 1) * nv).collect(), y: None }).collect();
            }
        }
        if screens.is_empty() {
            // default: one blank screen; signals are added from the list on the left
            screens = vec![Screen::default()];
        }
        // keep the zoom only when the time span is the same
        let fx = Self::full_x(&res);
        let same_span = self.res.as_ref().map(|r| Self::full_x(r) == fx).unwrap_or(false);
        if !same_span {
            self.x = fx;
            self.c1 = None;
            self.c2 = None;
        }
        for s in screens.iter_mut() {
            s.y = None;
        }
        self.screens = screens;
        self.act = self.act.min(self.screens.len().saturating_sub(1));
        self.res = Some(res);
        self.open = true;
    }
    pub fn fit(&mut self) {
        if let Some(r) = &self.res {
            self.x = Self::full_x(r);
        }
        for s in self.screens.iter_mut() {
            s.y = None;
        }
    }
    fn clamp_x(&mut self) {
        if let Some(r) = &self.res {
            let (a, b) = Self::full_x(r);
            let w = (self.x.1 - self.x.0).max(r.dt * 4.0).min(b - a);
            let mut x0 = self.x.0.max(a);
            if x0 + w > b {
                x0 = b - w;
            }
            self.x = (x0, x0 + w);
        }
    }
    fn val_at(r: &TranResult, ci: usize, t: f64) -> f64 {
        let d = &r.data[ci];
        if d.is_empty() {
            return f64::NAN;
        }
        let f = (t - r.t0) / r.dt;
        let i = (f.floor().max(0.0) as usize).min(r.n - 1);
        let j = (i + 1).min(r.n - 1);
        let a = (f - i as f64).clamp(0.0, 1.0);
        d[i] + (d[j] - d[i]) * a
    }
    fn auto_y(r: &TranResult, s: &Screen, x: (f64, f64)) -> (f64, f64) {
        let (mut mn, mut mx) = (f64::INFINITY, f64::NEG_INFINITY);
        for &ci in &s.curves {
            let d = &r.data[ci];
            if d.is_empty() {
                continue;
            }
            let i0 = (((x.0 - r.t0) / r.dt).floor().max(0.0) as usize).min(r.n - 1);
            let i1 = (((x.1 - r.t0) / r.dt).ceil().max(0.0) as usize).min(r.n - 1);
            for &v in &d[i0..=i1] {
                if v.is_finite() {
                    mn = mn.min(v);
                    mx = mx.max(v);
                }
            }
        }
        if !mn.is_finite() {
            return (-1.0, 1.0);
        }
        if mx - mn < 1e-12 * mx.abs().max(1e-30) || mx == mn {
            let d = if mx.abs() > 0.0 { mx.abs() * 0.1 } else { 1.0 };
            return (mn - d, mx + d);
        }
        let m = (mx - mn) * 0.06;
        (mn - m, mx + m)
    }
    /// (max, min, avg, rms, p-p, integral) over [a, b]
    pub fn stats(r: &TranResult, ci: usize, a: f64, b: f64) -> [f64; 6] {
        let d = &r.data[ci];
        let i0 = (((a - r.t0) / r.dt).floor().max(0.0) as usize).min(r.n.saturating_sub(1));
        let i1 = (((b - r.t0) / r.dt).ceil().max(0.0) as usize).min(r.n.saturating_sub(1));
        let (mut mx, mut mn, mut s1, mut s2, mut tt) = (f64::NEG_INFINITY, f64::INFINITY, 0.0, 0.0, 0.0);
        for i in i0..=i1 {
            let v = d[i];
            if v > mx {
                mx = v;
            }
            if v < mn {
                mn = v;
            }
            if i > i0 {
                let u = d[i - 1];
                s1 += (u + v) / 2.0 * r.dt;
                s2 += (u * u + v * v) / 2.0 * r.dt;
                tt += r.dt;
            }
        }
        let avg = if tt > 0.0 { s1 / tt } else { d[i0] };
        let rms = if tt > 0.0 { (s2 / tt).sqrt() } else { d[i0].abs() };
        [mx, mn, avg, rms, mx - mn, s1]
    }
    pub fn color(pal: &Pal, ci: usize) -> Color32 {
        pal.curves[ci % 8]
    }
    pub fn csv(&self) -> Option<String> {
        let r = self.res.as_ref()?;
        let i0 = (((self.x.0 - r.t0) / r.dt).floor().max(0.0) as usize).min(r.n - 1);
        let i1 = (((self.x.1 - r.t0) / r.dt).ceil().max(0.0) as usize).min(r.n - 1);
        let mut s = String::from("Time");
        for n in &r.names {
            s.push(',');
            s.push_str(n);
        }
        s.push('\n');
        for i in i0..=i1 {
            s.push_str(&format!("{}", r.t0 + i as f64 * r.dt));
            for d in &r.data {
                s.push_str(&format!(",{}", d[i]));
            }
            s.push('\n');
        }
        Some(s)
    }

    /* ---------------------------------------------------------------- UI */
    pub fn ui(&mut self, ui: &mut Ui, pal: &Pal) {
        for p in std::mem::take(&mut self.pending) {
            self.apply(p);
        }
        let res = match &self.res {
            Some(r) => r.clone(),
            None => {
                ui.label("尚無模擬結果，請先按 F5 執行模擬。");
                return;
            }
        };
        // curve colours: the user's choice for the signal name, else the palette
        let cols: Vec<Color32> = res.names.iter().enumerate().map(|(ci, n)| self.colors.get(n).copied().unwrap_or(pal.curves[ci % 8])).collect();
        // toolbar
        egui::Panel::top("wv_tools").show(ui, |ui| {
            use crate::icons::{button, Icon};
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if button(ui, Icon::Plus, "新增畫面", "新增一個波形畫面", true, false, None).clicked() {
                    self.screens.push(Screen::default());
                    self.act = self.screens.len() - 1;
                }
                if button(ui, Icon::Minus, "刪除畫面", "刪除目前（橘框）畫面", self.screens.len() > 1, false, None).clicked() {
                    self.screens.remove(self.act);
                    self.act = self.act.min(self.screens.len() - 1);
                }
                ui.separator();
                if button(ui, Icon::Fit, "全部顯示", "自動縮放（雙擊波形亦可）", true, false, None).clicked() {
                    self.fit();
                }
                if button(ui, Icon::ZoomIn, "", "時間軸放大", true, false, None).clicked() {
                    let m = (self.x.0 + self.x.1) / 2.0;
                    let w = (self.x.1 - self.x.0) / 2.0;
                    self.x = (m - w / 2.0, m + w / 2.0);
                    self.clamp_x();
                }
                if button(ui, Icon::ZoomOut, "", "時間軸縮小", true, false, None).clicked() {
                    let m = (self.x.0 + self.x.1) / 2.0;
                    let w = (self.x.1 - self.x.0) * 2.0;
                    self.x = (m - w / 2.0, m + w / 2.0);
                    self.clamp_x();
                }
                ui.separator();
                if button(ui, Icon::XZoom, "框選時間", "拖曳選取時間範圍放大", true, self.tool == Tool::Zoom, None).clicked() {
                    self.tool = Tool::Zoom;
                }
                if button(ui, Icon::BoxZoom, "區域放大", "拖曳框選同時放大 X 與 Y", true, self.tool == Tool::Box, None).clicked() {
                    self.tool = Tool::Box;
                }
                if button(ui, Icon::Pan, "平移", "拖曳平移（中鍵拖曳在任何工具下皆可平移）", true, self.tool == Tool::Pan, None).clicked() {
                    self.tool = Tool::Pan;
                }
                if button(ui, Icon::Stretch, "XY 縮放", "按住左鍵左右拖曳縮放時間軸（向右放大）、上下拖曳縮放 Y 軸（向上放大），以按下的位置為中心", true, self.tool == Tool::Stretch, None).clicked() {
                    self.tool = Tool::Stretch;
                }
                ui.separator();
                if button(ui, Icon::Cursor, "游標", "左鍵設定 C1、右鍵設定 C2，拖曳游標線可移動", true, self.cursors, None).clicked() {
                    self.cursors = !self.cursors;
                }
                ui.separator();
                if button(ui, Icon::Plus, "波形運算", "以探棒訊號做四則運算、開根號等，建立新的訊號", true, self.math_dlg.is_some(), None).clicked() {
                    self.math_dlg = if self.math_dlg.is_some() { None } else { Some(MathDlg::new(self.math.len())) };
                }
                if button(ui, Icon::Wave, "FFT", "頻譜分析：游標 C1–C2 之間（未設游標時為目前顯示範圍）", true, self.fft.is_some(), None).clicked() {
                    let ci = self.screens.get(self.act).and_then(|s| s.curves.first().copied()).unwrap_or(0);
                    self.fft = if self.fft.is_some() { None } else { Some(FftView::new(ci)) };
                }
                ui.separator();
                if button(ui, Icon::Export, "匯出 CSV", "匯出目前顯示時間範圍內的所有訊號", true, false, None).clicked() {
                    if let Some(csv) = self.csv() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}.csv", self.title)).save_file() {
                            let _ = std::fs::write(p, csv);
                        }
                    }
                }
                if button(ui, Icon::Report, "匯出報告", "第一個畫面＋量測值＋C1/C2 游標讀值；存檔類型可選 PDF 或 Excel", true, false, None).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).add_filter("Excel", &["xlsx"]).set_file_name(format!("{} 波形報告", self.title)).save_file() {
                        self.report_req = Some(p);
                    }
                }
                ui.separator();
                if button(ui, Icon::Gear, "設定", "欄位、波形畫面、X/Y 軸字體大小與波形線寬（也套用到匯出的報告）", true, self.style_open, None).clicked() {
                    self.style_open = !self.style_open;
                }
            });
        });
        // side panel: variables, readouts, measurements
        egui::Panel::left("wv_side").default_size(280.0).show(ui, |ui| {
            ui.style_mut().text_styles = crate::settings::text_styles(self.style.side_font);
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.strong("探棒訊號（勾選加入目前畫面，點色塊改顏色）");
                // fixed height (about eight rows): many signals scroll inside instead of pushing the rest down
                let row_h = ui.spacing().interact_size.y.max(self.style.side_font * 1.4) + ui.spacing().item_spacing.y;
                let list_h = (row_h * 8.0).round();
                let list = egui::ScrollArea::vertical().id_salt("wv_signals").max_height(list_h).min_scrolled_height(list_h).auto_shrink([false, false]).show(ui, |ui| {
                for (ci, name) in res.names.iter().enumerate() {
                    let mut on = self.screens.get(self.act).map(|s| s.curves.contains(&ci)).unwrap_or(false);
                    ui.horizontal(|ui| {
                        let mut c = cols[ci];
                        if egui::color_picker::color_edit_button_srgba(ui, &mut c, egui::color_picker::Alpha::Opaque).on_hover_text("點選以更改此訊號的顏色").changed() {
                            self.colors.insert(name.clone(), c);
                        }
                        if ui.checkbox(&mut on, name).changed() {
                            if let Some(s) = self.screens.get_mut(self.act) {
                                if on {
                                    s.curves.push(ci);
                                } else {
                                    s.curves.retain(|&c| c != ci);
                                }
                                s.y = None;
                            }
                        }
                        if self.colors.contains_key(name) && ui.small_button("↺").on_hover_text("還原預設顏色").clicked() {
                            self.colors.remove(name);
                        }
                    });
                }
                });
                self.sig_h = list.inner_rect.height();
                if !self.math.is_empty() || !self.math_err.is_empty() {
                    ui.separator();
                    ui.strong("運算訊號");
                    let mut rm = None;
                    for (k, m) in self.math.iter().enumerate() {
                        ui.horizontal(|ui| {
                            if ui.small_button("×").on_hover_text("刪除此運算訊號").clicked() {
                                rm = Some(k);
                            }
                            ui.label(format!("{} = {}", m.name, m.expr));
                        });
                    }
                    if let Some(k) = rm {
                        self.pending.push(Pending::RemoveMath(k));
                        ui.ctx().request_repaint();
                    }
                    if !self.math_err.is_empty() {
                        ui.colored_label(pal.err, &self.math_err);
                    }
                }
                ui.separator();
                ui.strong("游標讀值");
                if let Some(t) = self.hover_t {
                    ui.label(format!("t = {}", si(t, "s", 6)));
                    if let Some(s) = self.screens.get(self.act) {
                        for &ci in &s.curves {
                            ui.colored_label(cols[ci], format!("{} = {}", res.names[ci], si(Self::val_at(&res, ci, t), "", 5)));
                        }
                    }
                } else {
                    ui.weak("將游標移到波形上");
                }
                ui.separator();
                ui.strong("量測");
                let use_c = self.cursors && self.c1.is_some() && self.c2.is_some();
                let (a, b) = if use_c { (self.c1.unwrap().min(self.c2.unwrap()), self.c1.unwrap().max(self.c2.unwrap())) } else { self.x };
                ui.weak(format!("{}（{}）", if use_c { "游標 C1–C2 之間" } else { "目前顯示範圍" }, si(b - a, "s", 4)));
                if let Some(s) = self.screens.get(self.act) {
                    egui::Grid::new("meas").striped(true).num_columns(7).show(ui, |ui| {
                        ui.label("");
                        for h in ["Max", "Min", "Avg", "RMS", "P-P", "∫dt"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        for &ci in &s.curves {
                            let st = Self::stats(&res, ci, a, b);
                            ui.colored_label(cols[ci], &res.names[ci]);
                            for v in st {
                                ui.monospace(si(v, "", 4));
                            }
                            ui.end_row();
                        }
                    });
                }
                if self.cursors {
                    ui.separator();
                    ui.label(format!("C1 = {}", self.c1.map(|c| si(c, "s", 6)).unwrap_or_else(|| "—（左鍵設定）".into())));
                    ui.label(format!("C2 = {}", self.c2.map(|c| si(c, "s", 6)).unwrap_or_else(|| "—（右鍵設定）".into())));
                    if let (Some(c1), Some(c2)) = (self.c1, self.c2) {
                        ui.label(format!("Δt = {}，1/Δt = {}", si(c2 - c1, "s", 6), si(1.0 / (c2 - c1).abs(), "Hz", 5)));
                    }
                    if let (Some(c1), Some(s)) = (self.c1, self.screens.get(self.act)) {
                        egui::Grid::new("curs").striped(true).show(ui, |ui| {
                            ui.label("");
                            ui.strong("@C1");
                            ui.strong("@C2");
                            ui.strong("Δ");
                            ui.end_row();
                            for &ci in &s.curves {
                                let v1 = Self::val_at(&res, ci, c1);
                                ui.colored_label(cols[ci], &res.names[ci]);
                                ui.monospace(si(v1, "", 5));
                                match self.c2 {
                                    Some(c2) => {
                                        let v2 = Self::val_at(&res, ci, c2);
                                        ui.monospace(si(v2, "", 5));
                                        ui.monospace(si(v2 - v1, "", 5));
                                    }
                                    None => {
                                        ui.label("—");
                                        ui.label("—");
                                    }
                                }
                                ui.end_row();
                            }
                        });
                    }
                }
                ui.separator();
                ui.weak(format!("{} 點，Δt = {}", res.n, si(res.dt, "s", 4)));
                if !self.report_msg.is_empty() {
                    ui.separator();
                    let err = !self.report_msg.starts_with("已匯出");
                    ui.colored_label(if err { pal.err } else { pal.ok }, &self.report_msg);
                }
            });
        });
        // plots
        egui::CentralPanel::default().show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let ns = self.screens.len().max(1);
            let h = area.height() / ns as f32;
            self.hover_t = None;
            let mut new_x: Option<(f64, f64)> = None;
            for si_ in 0..self.screens.len() {
                let srect = Rect::from_min_size(pos2(area.min.x, area.min.y + h * si_ as f32), vec2(area.width(), h));
                let last = si_ + 1 == self.screens.len();
                let plot = Self::plot_rect(srect, &self.style, last);
                if plot.width() < 20.0 || plot.height() < 20.0 {
                    continue;
                }
                let y = self.screens[si_].y.unwrap_or_else(|| Self::auto_y(&res, &self.screens[si_], self.x));
                let painter = ui.painter_at(srect);
                let ax = self.paint_plot(&painter, plot, &self.screens[si_], y, &res, &cols, pal, last);
                if si_ == self.act && self.screens.len() > 1 {
                    painter.rect_stroke(plot.expand(1.0), 0.0, Stroke::new(2.0, pal.accent), egui::StrokeKind::Outside);
                }
                // interaction
                let id = ui.scope_id().with(("wplot", si_));
                let resp = ui.interact(plot, id, Sense::click_and_drag());
                if self.tool == Tool::Stretch && resp.hovered() {
                    ui.ctx().set_cursor_icon(egui::CursorIcon::Move);
                }
                let ptr = ui.input(|i| i.pointer.hover_pos());
                // only when this plot is really under the pointer: a tool window (FFT, math) on top takes the wheel
                if let Some(p) = ptr.filter(|p| plot.contains(*p) && resp.contains_pointer()) {
                    let t = ax.xv(p.x);
                    self.hover_t = Some(t);
                    painter.line_segment([pos2(p.x, plot.min.y), pos2(p.x, plot.max.y)], Stroke::new(1.0, pal.faint.gamma_multiply(0.6)));
                    let (scroll, shift, zoom) = ui.input(|i| (i.smooth_scroll_delta.y, i.modifiers.shift, i.zoom_delta()));
                    let f = if zoom != 1.0 { 1.0 / zoom as f64 } else if scroll != 0.0 { (-scroll as f64 / 300.0).exp() } else { 1.0 };
                    if f != 1.0 {
                        if shift {
                            let yv = ax.yv(p.y);
                            self.screens[si_].y = Some((yv - (yv - y.0) * f, yv + (y.1 - yv) * f));
                        } else {
                            new_x = Some((t - (t - self.x.0) * f, t + (self.x.1 - t) * f));
                        }
                    }
                }
                if resp.clicked() || resp.secondary_clicked() || resp.drag_started() {
                    self.act = si_;
                }
                if resp.double_clicked() {
                    self.fit();
                }
                if self.cursors {
                    if let Some(p) = resp.interact_pointer_pos() {
                        if resp.clicked() {
                            self.c1 = Some(ax.xv(p.x));
                        }
                        if resp.secondary_clicked() {
                            self.c2 = Some(ax.xv(p.x));
                        }
                    }
                }
                if resp.dragged_by(egui::PointerButton::Middle) {
                    let dx = resp.drag_delta().x as f64 / plot.width() as f64 * (self.x.1 - self.x.0);
                    new_x = Some((self.x.0 - dx, self.x.1 - dx));
                }
                if resp.drag_started_by(egui::PointerButton::Primary) {
                    if let Some(p) = ui.input(|i| i.pointer.press_origin()) {
                        let near = |c: Option<f64>| c.map(|t| (ax.xs(t) - p.x).abs() < 6.0).unwrap_or(false);
                        let d = if self.cursors && near(self.c1) {
                            WDrag::Cursor(1)
                        } else if self.cursors && near(self.c2) {
                            WDrag::Cursor(2)
                        } else {
                            match self.tool {
                                Tool::Zoom => WDrag::XZoom(p.x),
                                Tool::Box => WDrag::Box(p),
                                Tool::Pan => WDrag::Pan,
                                Tool::Stretch => WDrag::Stretch { p0: p, x0: self.x, y0: y },
                            }
                        };
                        self.drag = Some((si_, d));
                    }
                }
                if let Some((ds, d)) = &self.drag {
                    if *ds == si_ {
                        let cur = ptr.unwrap_or(plot.center());
                        match d {
                            WDrag::XZoom(x0) => {
                                let r = Rect::from_x_y_ranges(x0.min(cur.x)..=x0.max(cur.x), plot.y_range());
                                painter.rect_filled(r, 0.0, pal.accent_soft);
                                if resp.drag_stopped() {
                                    let (a, b) = (ax.xv(r.min.x), ax.xv(r.max.x));
                                    if r.width() > 4.0 {
                                        new_x = Some((a, b));
                                    }
                                }
                            }
                            WDrag::Box(p0) => {
                                let r = Rect::from_two_pos(*p0, cur).intersect(plot);
                                painter.rect(r, 0.0, pal.accent_soft, Stroke::new(1.0, pal.accent), egui::StrokeKind::Inside);
                                if resp.drag_stopped() && r.width() > 4.0 && r.height() > 4.0 {
                                    new_x = Some((ax.xv(r.min.x), ax.xv(r.max.x)));
                                    self.screens[si_].y = Some((ax.yv(r.max.y), ax.yv(r.min.y)));
                                }
                            }
                            WDrag::Pan => {
                                let dd = resp.drag_delta();
                                let dx = dd.x as f64 / plot.width() as f64 * (self.x.1 - self.x.0);
                                new_x = Some((self.x.0 - dx, self.x.1 - dx));
                                let dy = dd.y as f64 / plot.height() as f64 * (y.1 - y.0);
                                if dy != 0.0 {
                                    self.screens[si_].y = Some((y.0 + dy, y.1 + dy));
                                }
                            }
                            WDrag::Stretch { p0, x0, y0 } => {
                                // independent X and Y stretch from the drag start: right / up zoom in
                                let d = cur - *p0;
                                let fx = (-(d.x as f64) / 150.0).exp();
                                let fy = ((d.y as f64) / 150.0).exp();
                                let a0 = Axes { plot, x: *x0, y: *y0, xlog: false };
                                let (t, v) = (a0.xv(p0.x), a0.yv(p0.y));
                                new_x = Some((t - (t - x0.0) * fx, t + (x0.1 - t) * fx));
                                self.screens[si_].y = Some((v - (v - y0.0) * fy, v + (y0.1 - v) * fy));
                                painter.line_segment([pos2(p0.x - 9.0, p0.y), pos2(p0.x + 9.0, p0.y)], Stroke::new(1.2, pal.accent));
                                painter.line_segment([pos2(p0.x, p0.y - 9.0), pos2(p0.x, p0.y + 9.0)], Stroke::new(1.2, pal.accent));
                                painter.text(cur + vec2(14.0, 10.0), Align2::LEFT_TOP, format!("X ×{:.2}　Y ×{:.2}", 1.0 / fx, 1.0 / fy), FontId::proportional(12.0), pal.accent);
                            }
                            WDrag::Cursor(k) => {
                                let t = ax.xv(cur.x.clamp(plot.min.x, plot.max.x));
                                if *k == 1 {
                                    self.c1 = Some(t);
                                } else {
                                    self.c2 = Some(t);
                                }
                            }
                        }
                        if resp.drag_stopped() {
                            self.drag = None;
                        }
                    }
                }
            }
            if let Some(x) = new_x {
                if x.1 > x.0 {
                    self.x = x;
                    self.clamp_x();
                }
            }
        });
        // tool windows
        let mut act = MathAct::None;
        if let Some(d) = &mut self.math_dlg {
            act = d.ui(ui.ctx(), &res.names);
        }
        match act {
            MathAct::Add(n, e) => {
                self.pending.push(Pending::AddMath(n, e));
                ui.ctx().request_repaint();
            }
            MathAct::Close => self.math_dlg = None,
            MathAct::None => {}
        }
        let (range, desc) = self.analysis_range();
        let mut keep = true;
        if let Some(f) = &mut self.fft {
            keep = f.ui(ui.ctx(), &res, range, desc, pal, &cols);
        }
        if !keep {
            self.fft = None;
        }
        if self.style_open && !self.style.ui(ui.ctx()) {
            self.style_open = false;
        }
        if let Some(p) = self.report_req.take() {
            self.report_msg = match self.export_report(ui.ctx(), &res, &p) {
                Ok(p) => format!("已匯出報告：{}", p.display()),
                Err(e) => format!("報告匯出失敗：{}", e),
            };
        }
    }

    /// plot area of a screen (room on the left and below for the tick labels)
    fn plot_rect(srect: Rect, st: &WaveStyle, x_labels: bool) -> Rect {
        let left = (64.0 * st.axis_font / 11.0).max(48.0);
        let bottom = if x_labels { st.axis_font + 11.0 } else { 8.0 };
        Rect::from_min_max(pos2(srect.min.x + left, srect.min.y + 6.0), pos2(srect.max.x - 10.0, srect.max.y - bottom))
    }

    /// One waveform screen: axes, curves, legend and cursor lines; on screen and in the PDF report.
    #[allow(clippy::too_many_arguments)]
    fn paint_plot(&self, painter: &egui::Painter, plot: Rect, s: &Screen, y: (f64, f64), res: &TranResult, cols: &[Color32], pal: &Pal, x_labels: bool) -> Axes {
        let st = self.style;
        let ax = Axes { plot, x: self.x, y, xlog: false };
        ax.draw_sized(painter, pal, "s", x_labels, st.axis_font);
        let pclip = painter.with_clip_rect(plot.intersect(painter.clip_rect()));
        for &ci in &s.curves {
            if let (Some(d), Some(&c)) = (res.data.get(ci), cols.get(ci)) {
                let pts = curve_points(&ax, res.t0, res.dt, d);
                if pts.len() >= 2 {
                    pclip.add(Shape::line(pts, Stroke::new(st.line_w, c)));
                }
            }
        }
        // legend: every curve while it fits; many curves (sweeps, Monte Carlo) get a one-line summary
        // (the probe list on the left still names and colours each one)
        let lf = FontId::proportional(st.plot_font);
        let cv = &s.curves;
        let mut ly = plot.min.y + 4.0;
        let fit = ((plot.height() - 8.0) / (st.plot_font * 1.23)).floor().max(1.0) as usize;
        if cv.len() <= fit.min(12) {
            for &ci in cv {
                if let (Some(n), Some(&c)) = (res.names.get(ci), cols.get(ci)) {
                    let r = pclip.text(pos2(plot.min.x + 8.0, ly), Align2::LEFT_TOP, n, lf.clone(), c);
                    ly = r.max.y + 1.0;
                }
            }
        } else {
            let base = |n: &str| n.split(" [").next().unwrap_or(n).to_string();
            let mut bases: Vec<String> = Vec::new();
            for &ci in cv {
                if let Some(n) = res.names.get(ci) {
                    let b = base(n);
                    if !bases.contains(&b) {
                        bases.push(b);
                    }
                }
            }
            let txt = format!("{}：{} 條曲線", bases.join("、"), cv.len());
            pclip.text(pos2(plot.min.x + 8.0, ly), Align2::LEFT_TOP, txt, lf.clone(), pal.text);
        }
        // cursors
        if self.cursors {
            for (k, c) in [(1u8, self.c1), (2u8, self.c2)] {
                if let Some(t) = c {
                    let x = ax.xs(t);
                    if x >= plot.min.x && x <= plot.max.x {
                        let col = if k == 1 { pal.accent } else { pal.ctl };
                        pclip.extend(Shape::dashed_line(&[pos2(x, plot.min.y), pos2(x, plot.max.y)], Stroke::new(1.2, col), 5.0, 3.0));
                        pclip.text(pos2(x + 3.0, plot.min.y + 2.0), Align2::LEFT_TOP, format!("C{}", k), FontId::proportional(st.plot_font * 0.85), col);
                    }
                }
            }
        }
        ax
    }

    /* ---------------------------------------------------------------- report */
    /// curve colours for printing: the user's choice, else the light palette
    fn print_cols(&self, res: &TranResult) -> Vec<Color32> {
        let pal = &crate::theme::LIGHT;
        res.names.iter().enumerate().map(|(ci, n)| self.colors.get(n).copied().unwrap_or(pal.curves[ci % 8])).collect()
    }

    /// first screen, its measurements and the cursor readings
    fn report_data(&self, res: &TranResult) -> wave_report::Report {
        let pal = &crate::theme::LIGHT;
        let cols = self.print_cols(res);
        let s0 = self.screens.first().cloned().unwrap_or_default();
        let y = s0.y.unwrap_or_else(|| Self::auto_y(res, &s0, self.x));
        let use_c = self.cursors && self.c1.is_some() && self.c2.is_some();
        let (a, b) = match (use_c, self.c1, self.c2) {
            (true, Some(c1), Some(c2)) => (c1.min(c2), c1.max(c2)),
            _ => self.x,
        };
        let meas_desc = format!("{}（{}）", if use_c { "游標 C1–C2 之間" } else { "目前顯示範圍" }, si(b - a, "s", 4));
        let at = |c: Option<f64>, ci: usize| if self.cursors { c.map(|t| Self::val_at(res, ci, t)) } else { None };
        let curves = s0
            .curves
            .iter()
            .filter(|&&ci| ci < res.data.len() && ci < res.names.len())
            .map(|&ci| wave_report::Curve {
                name: res.names[ci].clone(),
                color: cols[ci],
                pts: wave_report::decimate(res, ci, self.x, 2000),
                stats: Self::stats(res, ci, a, b),
                at_c1: at(self.c1, ci),
                at_c2: at(self.c2, ci),
            })
            .collect();
        wave_report::Report {
            title: self.title.clone(),
            x: self.x,
            y,
            curves,
            meas_desc,
            cursors: if self.cursors { Some((self.c1, self.c2)) } else { None },
            cursor_cols: (pal.accent, pal.ctl),
            style: self.style,
        }
    }

    /// the PDF report: painted like the viewer (same plot code, fonts and line width) on A4 pages
    fn report_pages(&self, ctx: &egui::Context, res: &TranResult, rep: &wave_report::Report) -> Vec<Page> {
        let pal = &crate::theme::LIGHT;
        let st = self.style;
        let m = 40.0;
        let w = A4.0 - 2.0 * m;
        let fs = st.side_font;
        let mut sh = Sheet::new(ctx, m);
        let r = sh.p().text(pos2(m, sh.y), Align2::LEFT_TOP, format!("{} 波形報告", rep.title), FontId::proportional(fs + 6.0), pal.text);
        sh.y = r.max.y + 4.0;
        let sub = format!("DaedalusEngine {}　·　時間範圍 {} – {}", daedalus_core::VERSION, si(rep.x.0, "s", 5), si(rep.x.1, "s", 5));
        sh.line(&sub, fs * 0.85, pal.muted);
        sh.y += 6.0;
        // the first waveform screen
        let srect = Rect::from_min_size(pos2(m, sh.y), vec2(w, 330.0));
        let s0 = self.screens.first().cloned().unwrap_or_default();
        let cols = self.print_cols(res);
        let plot = Self::plot_rect(srect, &st, true);
        if plot.width() > 20.0 && plot.height() > 20.0 {
            let p = sh.p().clone();
            self.paint_plot(&p, plot, &s0, rep.y, res, &cols, pal, true);
        }
        sh.y = srect.max.y + 16.0;
        // measurements
        sh.heading(&format!("量測：{}", rep.meas_desc), fs, pal);
        let mut head = vec!["訊號"];
        head.extend(wave_report::MEAS_HEAD);
        let rows: Vec<Vec<(String, Color32)>> = rep
            .curves
            .iter()
            .map(|c| {
                let mut v = vec![(c.name.clone(), c.color)];
                v.extend(c.stats.iter().map(|x| (si(*x, "", 4), pal.text)));
                v
            })
            .collect();
        if rows.is_empty() {
            sh.line("（第一個畫面沒有訊號）", fs, pal.muted);
        } else {
            sh.table(w, &head, &rows, fs, pal);
        }
        sh.y += fs;
        // cursor readings
        sh.heading("游標讀值", fs, pal);
        match rep.cursors {
            None => sh.line("未開啟游標", fs, pal.muted),
            Some((c1, c2)) => {
                let mut parts = vec![];
                if let Some(a) = c1 {
                    parts.push(format!("C1 = {}", si(a, "s", 6)));
                }
                if let Some(b) = c2 {
                    parts.push(format!("C2 = {}", si(b, "s", 6)));
                }
                if let (Some(a), Some(b)) = (c1, c2) {
                    parts.push(format!("Δt = {}", si(b - a, "s", 6)));
                    if a != b {
                        parts.push(format!("1/Δt = {}", si(1.0 / (b - a).abs(), "Hz", 5)));
                    }
                }
                if parts.is_empty() {
                    sh.line("尚未設定 C1、C2（左鍵設定 C1、右鍵設定 C2）", fs, pal.muted);
                } else {
                    sh.line(&parts.join("　　"), fs, pal.text);
                    sh.y += 4.0;
                    let v = |x: Option<f64>| x.map(|x| si(x, "", 5)).unwrap_or_else(|| "—".into());
                    let rows: Vec<Vec<(String, Color32)>> = rep
                        .curves
                        .iter()
                        .map(|c| {
                            let d = match (c.at_c1, c.at_c2) {
                                (Some(a), Some(b)) => Some(b - a),
                                _ => None,
                            };
                            vec![(c.name.clone(), c.color), (v(c.at_c1), pal.text), (v(c.at_c2), pal.text), (v(d), pal.text)]
                        })
                        .collect();
                    if !rows.is_empty() {
                        sh.table(w, &["訊號", "@C1", "@C2", "Δ"], &rows, fs, pal);
                    }
                }
            }
        }
        sh.flush();
        sh.pages
    }

    /// writes the report; PDF or Excel by the file extension (no extension: PDF)
    fn export_report(&self, ctx: &egui::Context, res: &TranResult, path: &Path) -> Result<PathBuf, String> {
        let ext = path.extension().and_then(|e| e.to_str()).map(|e| e.to_ascii_lowercase()).unwrap_or_default();
        let (excel, path) = match ext.as_str() {
            "xlsx" | "xls" => (true, path.with_extension("xlsx")),
            "pdf" => (false, path.to_path_buf()),
            _ => {
                let mut s = path.as_os_str().to_owned();
                s.push(".pdf");
                (false, PathBuf::from(s))
            }
        };
        let rep = self.report_data(res);
        let bytes = if excel { wave_report::xlsx(&rep)? } else { crate::pdf_export::write_pdf(&self.report_pages(ctx, res, &rep))? };
        std::fs::write(&path, bytes).map_err(|e| format!("無法寫入 {}（{}）", path.display(), e))?;
        Ok(path)
    }

    /* ---------------------------------------------------------------- scripted GUI tests */
    pub fn script_add(&mut self, name: &str) {
        let ci = self.res.as_ref().and_then(|r| r.names.iter().position(|n| n == name));
        if let (Some(ci), Some(s)) = (ci, self.screens.get_mut(self.act)) {
            s.curves.push(ci);
            s.y = None;
        }
    }
    pub fn script_math(&mut self, name: String, expr: String) {
        self.pending.push(Pending::AddMath(name, expr));
    }
    pub fn script_fft(&mut self, name: &str) {
        let ci = self.res.as_ref().and_then(|r| r.names.iter().position(|n| n == name)).unwrap_or(0);
        self.fft = Some(FftView::new(ci));
    }
    pub fn script_color(&mut self, name: &str, hex: &str) {
        let h = hex.trim_start_matches('#');
        if let (Ok(r), Ok(g), Ok(b)) = (u8::from_str_radix(h.get(0..2).unwrap_or(""), 16), u8::from_str_radix(h.get(2..4).unwrap_or(""), 16), u8::from_str_radix(h.get(4..6).unwrap_or(""), 16)) {
            self.colors.insert(name.to_string(), Color32::from_rgb(r, g, b));
        }
    }
    pub fn script_style(&mut self, v: [f32; 4]) {
        self.style = WaveStyle { side_font: v[0], plot_font: v[1], axis_font: v[2], line_w: v[3] }.clamped();
    }
    pub fn script_tool(&mut self, name: &str) {
        self.tool = match name {
            "stretch" => Tool::Stretch,
            "pan" => Tool::Pan,
            "box" => Tool::Box,
            _ => Tool::Zoom,
        };
    }
    pub fn script_close_tools(&mut self) {
        self.fft = None;
        self.math_dlg = None;
        self.style_open = false;
    }
    pub fn script_cursors_off(&mut self) {
        self.cursors = false;
    }
    pub fn script_settings(&mut self, open: bool) {
        self.style_open = open;
    }
    pub fn script_cursors(&mut self, a: f64, b: f64) {
        self.cursors = true;
        self.c1 = Some(a);
        self.c2 = Some(b);
    }
    pub fn script_dump(&self) -> serde_json::Value {
        let names = self.res.as_ref().map(|r| r.names.clone()).unwrap_or_default();
        let screens: Vec<Vec<String>> = self.layout_names().unwrap_or_default();
        let math: Vec<String> = self.math.iter().map(|m| format!("{}={}", m.name, m.expr)).collect();
        let colors: HashMap<String, String> = self.colors.iter().map(|(k, c)| (k.clone(), format!("#{:02x}{:02x}{:02x}", c.r(), c.g(), c.b()))).collect();
        let ys: Vec<Option<[f64; 2]>> = self.screens.iter().map(|s| s.y.map(|y| [y.0, y.1])).collect();
        let tool = match self.tool {
            Tool::Zoom => "zoom",
            Tool::Pan => "pan",
            Tool::Box => "box",
            Tool::Stretch => "stretch",
        };
        let st = self.style;
        serde_json::json!({ "open": self.open, "x": [self.x.0, self.x.1], "ys": ys, "names": names, "screens": screens, "math": math, "math_err": self.math_err, "fft": self.fft.is_some(), "colors": colors, "tool": tool, "style": [st.side_font, st.plot_font, st.axis_font, st.line_w], "style_open": self.style_open, "sig_h": self.sig_h, "report_msg": self.report_msg })
    }

    /// cursors C1–C2 when both are set, else the visible time range
    fn analysis_range(&self) -> ((f64, f64), &'static str) {
        match (self.cursors, self.c1, self.c2) {
            (true, Some(a), Some(b)) if a != b => ((a.min(b), a.max(b)), "游標 C1–C2 之間"),
            _ => (self.x, "目前顯示範圍"),
        }
    }

    fn apply(&mut self, p: Pending) {
        if self.res.is_none() {
            return;
        }
        match p {
            Pending::AddMath(name, expr) => {
                let checked = {
                    let r = self.res.as_ref().unwrap();
                    if name.is_empty() || expr.is_empty() {
                        Err("請輸入名稱與運算式".to_string())
                    } else if r.names.iter().any(|n| n.eq_ignore_ascii_case(&name)) {
                        Err(format!("名稱「{}」已被使用", name))
                    } else {
                        eval_math(r, r.names.len(), &expr).map_err(|e| format!("運算式錯誤：{}", e))
                    }
                };
                match checked {
                    Err(m) => {
                        if let Some(d) = self.math_dlg.as_mut() {
                            d.err = m;
                        }
                    }
                    Ok(v) => {
                        let r = Arc::make_mut(self.res.as_mut().unwrap());
                        r.names.push(name.clone());
                        r.data.push(v);
                        let ci = r.names.len() - 1;
                        if let Some(s) = self.screens.get_mut(self.act) {
                            s.curves.push(ci);
                            s.y = None;
                        }
                        self.math.push(MathDef { name, expr });
                        let n = self.math.len();
                        if let Some(d) = self.math_dlg.as_mut() {
                            *d = MathDlg::new(n);
                        }
                    }
                }
            }
            Pending::RemoveMath(k) => {
                if k >= self.math.len() {
                    return;
                }
                // rebuild the math channels after the simulated signals, keeping the screens by name
                let layout = self.layout_names().unwrap_or_default();
                self.math.remove(k);
                let defs = std::mem::take(&mut self.math);
                let nbase = self.nbase;
                let r = Arc::make_mut(self.res.as_mut().unwrap());
                r.names.truncate(nbase);
                r.data.truncate(nbase);
                for m in defs {
                    if let Ok(v) = eval_math(r, r.names.len(), &m.expr) {
                        r.names.push(m.name.clone());
                        r.data.push(v);
                        self.math.push(m);
                    }
                }
                let names = r.names.clone();
                for (s, l) in self.screens.iter_mut().zip(layout.iter()) {
                    s.curves = l.iter().filter_map(|n| names.iter().position(|m| m == n)).collect();
                    s.y = None;
                }
            }
        }
    }
}
