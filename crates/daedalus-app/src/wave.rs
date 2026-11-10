//! Waveform viewer (SIMVIEW-style): stacked screens sharing one time axis, cursors, measurements.

use crate::measure::{measure, Meas};
use crate::pdf_export::{Capture, Page, Part, A4};
use crate::plot::{curve_points, si, Axes};
use crate::settings::WaveStyle;
use crate::theme::Pal;
use crate::wave_report;
use crate::wave_tools::{eval_math, FftView, MathAct, MathDef, MathDlg};
use daedalus_core::runner::{SweepInfo, TranResult};
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, RichText, Sense, Shape, Stroke, Ui};
use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;

/// waveform file: the simulated signals and the viewer layout, readable again later
pub const WAVE_EXT: &str = "dwave";
const WAVE_MAGIC: &[u8; 8] = b"DAEDWAVE";
const WAVE_FORMAT: u32 = 1;

/// screens shown at once; more screens keep this height and scroll
const MAX_VISIBLE: usize = 4;

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

/// header of a waveform file (JSON, followed by the samples as little-endian f64, signal by signal)
#[derive(serde::Serialize, serde::Deserialize, Default)]
#[serde(default)]
struct WaveHead {
    app: String,
    version: String,
    title: String,
    t0: f64,
    dt: f64,
    n: usize,
    names: Vec<String>,
    math: Vec<(String, String)>,
    screens: Vec<Vec<String>>,
    x: Option<(f64, f64)>,
    cursors: bool,
    c1: Option<f64>,
    c2: Option<f64>,
    colors: HashMap<String, [u8; 3]>,
    sweep: Option<(String, Vec<String>, usize)>,
    /// fonts, line width, measurement columns, every / selected screen switches
    style: Option<WaveStyle>,
    /// Y range of each screen (None = automatic) and the selected screen
    ys: Vec<Option<(f64, f64)>>,
    act: usize,
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
    /// fonts, line width, measurement columns (saved with the app settings, also used for the report)
    pub style: WaveStyle,
    style_open: bool,
    /// report to write in this frame (path, all screens) from the save dialog or a GUI-test script
    pub report_req: Option<(PathBuf, bool)>,
    /// last outcome of a report, CSV or waveform-file operation
    pub report_msg: String,
    /// height of the signal list (GUI test)
    sig_h: f32,
    /// measurements by signal: (result, signal, range) key, columns, values
    mcache: HashMap<usize, ((usize, u64, u64), Vec<Meas>, Vec<f64>)>,
    /// bring this screen into view (screens beyond the fourth scroll)
    scroll_to: Option<usize>,
    /// height of one screen and of all screens (GUI test)
    screen_h: (f32, f32),
    /// the circuit whose results this viewer shows (0: a waveform file) and the window id
    pub doc: u64,
    pub uid: u64,
    /// colours or settings changed by the user (they become the defaults of new viewers)
    pub prefs_changed: bool,
    /// a waveform file to open in a new viewer window
    pub open_req: Option<PathBuf>,
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

/// "全部畫面 / 目前畫面" switch beside a section title
fn section_head(ui: &mut Ui, title: &str, all: &mut bool) {
    ui.horizontal(|ui| {
        ui.strong(title);
        ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), |ui| {
            ui.selectable_value(all, false, "目前畫面").on_hover_text("只顯示目前（橘框）畫面的訊號");
            ui.selectable_value(all, true, "全部畫面").on_hover_text("顯示所有畫面的訊號");
        });
    });
}

impl WaveView {
    pub fn new() -> Self {
        WaveView {
            res: None,
            screens: vec![],
            act: 0,
            x: (0.0, 1.0),
            cursors: false,
            c1: None,
            c2: None,
            tool: Tool::Zoom,
            open: false,
            drag: None,
            hover_t: None,
            title: String::new(),
            math: vec![],
            colors: HashMap::new(),
            nbase: 0,
            math_dlg: None,
            math_err: String::new(),
            fft: None,
            pending: vec![],
            style: WaveStyle::default(),
            style_open: false,
            report_req: None,
            report_msg: String::new(),
            sig_h: 0.0,
            mcache: HashMap::new(),
            scroll_to: None,
            screen_h: (0.0, 0.0),
            doc: 0,
            uid: 0,
            prefs_changed: false,
            open_req: None,
        }
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
        self.mcache.clear();
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

    /// measurement columns chosen in the settings
    fn meas_cols(&self) -> Vec<Meas> {
        self.style.meas.iter().copied().filter(|m| *m != Meas::None).collect()
    }
    /// the measuring range: cursors C1–C2 when both are set, else the visible time range
    fn meas_range(&self) -> ((f64, f64), String) {
        let use_c = self.cursors && self.c1.is_some() && self.c2.is_some();
        let (a, b) = match (use_c, self.c1, self.c2) {
            (true, Some(c1), Some(c2)) => (c1.min(c2), c1.max(c2)),
            _ => self.x,
        };
        ((a, b), format!("{}（{}）", if use_c { "游標 C1–C2 之間" } else { "目前顯示範圍" }, si(b - a, "s", 4)))
    }
    fn meas_cached(&mut self, res: &Arc<TranResult>, ci: usize, a: f64, b: f64, ms: &[Meas]) -> Vec<f64> {
        let key = (Arc::as_ptr(res) as usize, a.to_bits(), b.to_bits());
        if let Some((k, m, v)) = self.mcache.get(&ci) {
            if *k == key && m.as_slice() == ms {
                return v.clone();
            }
        }
        let v = measure(res, ci, a, b, ms);
        self.mcache.insert(ci, (key, ms.to_vec(), v.clone()));
        v
    }
    /// signals listed in the left column: every screen (each signal once, under its first screen) or only the
    /// selected one
    fn listed(&self, all: bool) -> Vec<(Option<usize>, Vec<usize>)> {
        if !all || self.screens.len() <= 1 {
            return vec![(None, self.screens.get(self.act).map(|s| s.curves.clone()).unwrap_or_default())];
        }
        let mut seen: Vec<usize> = vec![];
        let mut out = vec![];
        for (k, s) in self.screens.iter().enumerate() {
            let v: Vec<usize> = s
                .curves
                .iter()
                .copied()
                .filter(|c| {
                    if seen.contains(c) {
                        false
                    } else {
                        seen.push(*c);
                        true
                    }
                })
                .collect();
            if !v.is_empty() {
                out.push((Some(k), v));
            }
        }
        out
    }

    /* ---------------------------------------------------------------- waveform file */
    /// writes the simulated signals (math channels as their expressions) and the layout
    pub fn save_wave(&self, path: &Path) -> Result<PathBuf, String> {
        let r = self.res.as_ref().ok_or("沒有波形可儲存")?;
        let path = if path.extension().is_some() { path.to_path_buf() } else { path.with_extension(WAVE_EXT) };
        let nb = self.nbase.min(r.names.len()).min(r.data.len());
        let colors = r.names.iter().filter_map(|n| self.colors.get(n).map(|c| (n.clone(), [c.r(), c.g(), c.b()]))).collect();
        let head = WaveHead {
            app: "DaedalusEngine".into(),
            version: daedalus_core::VERSION.into(),
            title: self.title.clone(),
            t0: r.t0,
            dt: r.dt,
            n: r.n,
            names: r.names[..nb].to_vec(),
            math: self.math.iter().map(|m| (m.name.clone(), m.expr.clone())).collect(),
            screens: self.layout_names().unwrap_or_default(),
            x: Some(self.x),
            cursors: self.cursors,
            c1: self.c1,
            c2: self.c2,
            colors,
            sweep: r.sweep.as_ref().map(|s| (s.var.clone(), s.labels.clone(), s.probes)),
            style: Some(self.style),
            ys: self.screens.iter().map(|s| s.y).collect(),
            act: self.act,
        };
        let hj = serde_json::to_vec(&head).map_err(|e| e.to_string())?;
        let mut raw = Vec::with_capacity(4 + hj.len() + nb * r.n * 8);
        raw.extend_from_slice(&(hj.len() as u32).to_le_bytes());
        raw.extend_from_slice(&hj);
        for d in &r.data[..nb] {
            for i in 0..r.n {
                raw.extend_from_slice(&d.get(i).copied().unwrap_or(f64::NAN).to_le_bytes());
            }
        }
        let mut out = Vec::with_capacity(raw.len() / 2 + 16);
        out.extend_from_slice(WAVE_MAGIC);
        out.extend_from_slice(&WAVE_FORMAT.to_le_bytes());
        out.extend_from_slice(&miniz_oxide::deflate::compress_to_vec_zlib(&raw, 6));
        std::fs::write(&path, out).map_err(|e| format!("無法寫入 {}（{}）", path.display(), e))?;
        Ok(path)
    }

    /// reads a waveform file into the viewer (layout, zoom, cursors and math channels as saved)
    pub fn load_wave(&mut self, path: &Path) -> Result<(), String> {
        let bytes = std::fs::read(path).map_err(|e| format!("無法讀取 {}（{}）", path.display(), e))?;
        let (res, head) = parse_wave(&bytes)?;
        self.res = None;
        self.screens.clear();
        self.act = 0;
        self.fft = None;
        self.math = head.math.iter().map(|(n, e)| MathDef { name: n.clone(), expr: e.clone() }).collect();
        for (k, c) in &head.colors {
            self.colors.insert(k.clone(), Color32::from_rgb(c[0], c[1], c[2]));
        }
        self.title = if head.title.is_empty() { path.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_default() } else { head.title.clone() };
        self.set_result(Arc::new(res), Some(head.screens.clone()));
        let full = self.res.as_ref().map(|r| Self::full_x(r)).unwrap_or((0.0, 1.0));
        if let Some(x) = head.x.filter(|x| x.1 > x.0 && x.0 >= full.0 - 1e-12 * full.1.abs().max(1.0) && x.1 <= full.1 + 1e-12 * full.1.abs().max(1.0)) {
            self.x = x;
            self.clamp_x();
        }
        self.cursors = head.cursors;
        self.c1 = head.c1;
        self.c2 = head.c2;
        // the screens exactly as saved (blank ones too), their Y zoom, the selected screen and the settings
        if let Some(r) = self.res.as_ref() {
            if !head.screens.is_empty() {
                let names = &r.names;
                self.screens = head.screens.iter().map(|l| Screen { curves: l.iter().filter_map(|n| names.iter().position(|m| m == n)).collect(), y: None }).collect();
            }
        }
        for (s, y) in self.screens.iter_mut().zip(&head.ys) {
            s.y = y.filter(|y| y.0.is_finite() && y.1.is_finite() && y.1 > y.0);
        }
        self.act = head.act.min(self.screens.len().saturating_sub(1));
        if let Some(st) = head.style {
            self.style = st.clamped();
        }
        Ok(())
    }

    /* ---------------------------------------------------------------- UI */
    pub fn ui(&mut self, ui: &mut Ui, pal: &Pal) {
        for p in std::mem::take(&mut self.pending) {
            self.apply(p);
        }
        let Some(res) = self.res.clone() else {
            ui.horizontal(|ui| {
                ui.label("尚無模擬結果，請先按 F5 執行模擬，或");
                if ui.button("開啟波形檔…").clicked() {
                    self.open_dialog();
                }
            });
            if !self.report_msg.is_empty() {
                ui.colored_label(pal.err, &self.report_msg);
            }
            return;
        };
        // curve colours: the user's choice for the signal name, else the palette
        let cols: Vec<Color32> = res.names.iter().enumerate().map(|(ci, n)| self.colors.get(n).copied().unwrap_or(pal.curves[ci % 8])).collect();
        self.toolbar(ui);
        self.side_panel(ui, pal, &res, &cols);
        // plots: at most four screens share the height; more screens keep that height and scroll
        egui::CentralPanel::default().show(ui, |ui| {
            let ns = self.screens.len().max(1);
            self.hover_t = None;
            if ns <= MAX_VISIBLE {
                let area = ui.available_rect_before_wrap();
                let h = area.height() / ns as f32;
                self.screen_h = (h, area.height());
                self.plots(ui, area, h, false, &res, &cols, pal);
            } else {
                let full = ui.available_rect_before_wrap();
                let h = (full.height() / MAX_VISIBLE as f32).floor().max(60.0);
                egui::ScrollArea::vertical()
                    .id_salt("wv_screens")
                    .auto_shrink([false, false])
                    .scroll_bar_visibility(egui::scroll_area::ScrollBarVisibility::AlwaysVisible)
                    .show(ui, |ui| {
                        let w = ui.available_width();
                        let (rect, _) = ui.allocate_exact_size(vec2(w, h * ns as f32), Sense::hover());
                        if let Some(k) = self.scroll_to.take() {
                            ui.scroll_to_rect(Rect::from_min_size(pos2(rect.min.x, rect.min.y + h * k as f32), vec2(w, h)), None);
                        }
                        self.screen_h = (h, rect.height());
                        self.plots(ui, rect, h, true, &res, &cols, pal);
                    });
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
        let before = self.style;
        if self.style_open && !self.style.ui(ui.ctx()) {
            self.style_open = false;
        }
        if self.style != before {
            self.prefs_changed = true;
        }
        if let Some((p, all)) = self.report_req.take() {
            self.report_msg = match self.export_report(ui.ctx(), &res, &p, all) {
                Ok(p) => format!("已匯出報告：{}", p.display()),
                Err(e) => format!("報告匯出失敗：{}", e),
            };
        }
    }

    /// a waveform file opens in a viewer window of its own (this one keeps its results)
    fn open_dialog(&mut self) {
        if let Some(p) = rfd::FileDialog::new().add_filter("DaedalusEngine 波形檔", &[WAVE_EXT]).pick_file() {
            self.open_req = Some(p);
        }
    }

    fn toolbar(&mut self, ui: &mut Ui) {
        egui::Panel::top("wv_tools").show(ui, |ui| {
            use crate::icons::{button, Icon};
            ui.horizontal_wrapped(|ui| {
                ui.spacing_mut().item_spacing.x = 2.0;
                if button(ui, Icon::Open, "", "開啟波形檔（.dwave），在新的視窗顯示", true, false, None).clicked() {
                    self.open_dialog();
                }
                if button(ui, Icon::Save, "", "儲存波形檔（.dwave）：訊號、畫面配置、運算訊號與游標，日後可再開啟", true, false, None).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("DaedalusEngine 波形檔", &[WAVE_EXT]).set_file_name(format!("{}.{}", self.title, WAVE_EXT)).save_file() {
                        self.report_msg = match self.save_wave(&p) {
                            Ok(p) => format!("已儲存波形檔：{}", p.display()),
                            Err(e) => format!("波形檔儲存失敗：{}", e),
                        };
                    }
                }
                ui.separator();
                if button(ui, Icon::Plus, "新增畫面", "新增一個波形畫面（超過 4 個時可上下捲動）", true, false, None).clicked() {
                    self.screens.push(Screen::default());
                    self.act = self.screens.len() - 1;
                    self.scroll_to = Some(self.act);
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
                if button(ui, Icon::Gear, "設定", "字體大小、波形線寬與左側的量測項目（也套用到匯出的報告）", true, self.style_open, None).clicked() {
                    self.style_open = !self.style_open;
                }
                let er = button(ui, Icon::Export, "匯出", "匯出 CSV 或波形報告（PDF／Excel）", true, false, None);
                egui::Popup::menu(&er).show(|ui| {
                    if ui.button("匯出 CSV…").on_hover_text("目前顯示時間範圍內的所有訊號").clicked() {
                        if let Some(csv) = self.csv() {
                            if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}.csv", self.title)).save_file() {
                                self.report_msg = match std::fs::write(&p, csv) {
                                    Ok(()) => format!("已匯出 CSV：{}", p.display()),
                                    Err(e) => format!("CSV 匯出失敗：{}", e),
                                };
                            }
                        }
                    }
                    if ui.button("匯出所選波形報告…").on_hover_text("目前（橘框）畫面＋量測值＋游標讀值；存檔類型可選 PDF 或 Excel").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).add_filter("Excel", &["xlsx"]).set_file_name(format!("{} 波形報告", self.title)).save_file() {
                            self.report_req = Some((p, false));
                        }
                    }
                    if ui.button("匯出所有波形報告…").on_hover_text("所有畫面，各附量測值，最後是游標讀值；存檔類型可選 PDF 或 Excel").clicked() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("PDF", &["pdf"]).add_filter("Excel", &["xlsx"]).set_file_name(format!("{} 波形報告（全部畫面）", self.title)).save_file() {
                            self.report_req = Some((p, true));
                        }
                    }
                });
            });
        });
    }

    /// left column: signals, cursor readings, measurements
    fn side_panel(&mut self, ui: &mut Ui, pal: &Pal, res: &Arc<TranResult>, cols: &[Color32]) {
        let before = (self.style.meas_all, self.style.cur_all);
        egui::Panel::left("wv_side").default_size(300.0).show(ui, |ui| {
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
                            if crate::colorpick::color_button(ui, &mut c).on_hover_text("點選以更改此訊號的顏色").changed() {
                                self.colors.insert(name.clone(), c);
                                self.prefs_changed = true;
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
                                self.prefs_changed = true;
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
                let screen_head = |ui: &mut Ui, k: Option<usize>| {
                    if let Some(k) = k {
                        ui.label(RichText::new(format!("畫面 {}", k + 1)).weak());
                        ui.end_row();
                    }
                };
                // cursors: the value under the mouse pointer, C1 / C2
                ui.separator();
                section_head(ui, "游標", &mut self.style.cur_all);
                let groups = self.listed(self.style.cur_all);
                if let Some(t) = self.hover_t {
                    ui.label(format!("t = {}", si(t, "s", 6)));
                    egui::Grid::new("hover").num_columns(2).show(ui, |ui| {
                        for (k, cv) in &groups {
                            screen_head(ui, *k);
                            for &ci in cv {
                                ui.colored_label(cols[ci], &res.names[ci]);
                                ui.monospace(si(Self::val_at(res, ci, t), "", 5));
                                ui.end_row();
                            }
                        }
                    });
                } else {
                    ui.weak("將滑鼠移到波形上讀值");
                }
                if self.cursors {
                    ui.add_space(4.0);
                    ui.label(format!("C1 = {}", self.c1.map(|c| si(c, "s", 6)).unwrap_or_else(|| "—（左鍵設定）".into())));
                    ui.label(format!("C2 = {}", self.c2.map(|c| si(c, "s", 6)).unwrap_or_else(|| "—（右鍵設定）".into())));
                    if let (Some(c1), Some(c2)) = (self.c1, self.c2) {
                        ui.label(format!("Δt = {}，1/Δt = {}", si(c2 - c1, "s", 6), si(1.0 / (c2 - c1).abs(), "Hz", 5)));
                    }
                    if let Some(c1) = self.c1 {
                        egui::Grid::new("curs").striped(true).show(ui, |ui| {
                            ui.label("");
                            ui.strong("@C1");
                            ui.strong("@C2");
                            ui.strong("Δ");
                            ui.end_row();
                            for (k, cv) in &groups {
                                screen_head(ui, *k);
                                for &ci in cv {
                                    let v1 = Self::val_at(res, ci, c1);
                                    ui.colored_label(cols[ci], &res.names[ci]);
                                    ui.monospace(si(v1, "", 5));
                                    match self.c2 {
                                        Some(c2) => {
                                            let v2 = Self::val_at(res, ci, c2);
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
                            }
                        });
                    }
                }
                // measurements
                ui.separator();
                section_head(ui, "量測", &mut self.style.meas_all);
                let ((a, b), desc) = self.meas_range();
                ui.weak(desc);
                let ms = self.meas_cols();
                let groups = self.listed(self.style.meas_all);
                if ms.is_empty() {
                    ui.weak("未選擇量測項目（在「設定」選擇）");
                } else {
                    let mut rows: Vec<(Option<usize>, Vec<(usize, Vec<f64>)>)> = vec![];
                    for (k, cv) in &groups {
                        let r: Vec<(usize, Vec<f64>)> = cv.iter().map(|&ci| (ci, self.meas_cached(res, ci, a, b, &ms))).collect();
                        rows.push((*k, r));
                    }
                    egui::Grid::new("meas").striped(true).num_columns(1 + ms.len()).show(ui, |ui| {
                        ui.label("");
                        for m in &ms {
                            ui.strong(m.head()).on_hover_text(format!("{}：{}", m.name(), m.help()));
                        }
                        ui.end_row();
                        for (k, r) in &rows {
                            screen_head(ui, *k);
                            for (ci, vals) in r {
                                ui.colored_label(cols[*ci], &res.names[*ci]);
                                for (m, v) in ms.iter().zip(vals) {
                                    ui.monospace(m.fmt(*v));
                                }
                                ui.end_row();
                            }
                        }
                    });
                    ui.weak("量測項目可在「設定」更改");
                }
                ui.separator();
                ui.weak(format!("{} 點，Δt = {}", res.n, si(res.dt, "s", 4)));
                if !self.report_msg.is_empty() {
                    ui.separator();
                    let err = !self.report_msg.starts_with("已");
                    ui.colored_label(if err { pal.err } else { pal.ok }, &self.report_msg);
                }
            });
        });
        if (self.style.meas_all, self.style.cur_all) != before {
            self.prefs_changed = true;
        }
    }

    /// the waveform screens in `area`, `h` points each
    #[allow(clippy::too_many_arguments)]
    fn plots(&mut self, ui: &mut Ui, area: Rect, h: f32, all_labels: bool, res: &Arc<TranResult>, cols: &[Color32], pal: &Pal) {
        let mut new_x: Option<(f64, f64)> = None;
        for si_ in 0..self.screens.len() {
            let srect = Rect::from_min_size(pos2(area.min.x, area.min.y + h * si_ as f32), vec2(area.width(), h));
            let last = si_ + 1 == self.screens.len();
            let labels = last || all_labels;
            let plot = Self::plot_rect(srect, &self.style, labels);
            if plot.width() < 20.0 || plot.height() < 20.0 {
                continue;
            }
            let y = self.screens[si_].y.unwrap_or_else(|| Self::auto_y(res, &self.screens[si_], self.x));
            let painter = ui.painter_at(srect);
            if !painter.clip_rect().is_positive() {
                continue;
            }
            let ax = self.paint_plot(&painter, plot, &self.screens[si_], y, res, cols, pal, labels);
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
            if let Some(p) = ptr.filter(|p| plot.contains(*p) && resp.contains_pointer() && painter.clip_rect().contains(*p)) {
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
                    // the wheel zoomed this plot: the scrolling screen list must not move as well
                    ui.input_mut(|i| i.smooth_scroll_delta = egui::Vec2::ZERO);
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

    /// the selected screen (or every screen), its measurements and the cursor readings
    fn report_data(&self, res: &TranResult, all: bool) -> wave_report::Report {
        let pal = &crate::theme::LIGHT;
        let cols = self.print_cols(res);
        let ((a, b), meas_desc) = self.meas_range();
        let ms = self.meas_cols();
        let at = |c: Option<f64>, ci: usize| if self.cursors { c.map(|t| Self::val_at(res, ci, t)) } else { None };
        let idx: Vec<usize> = if all { (0..self.screens.len()).collect() } else { vec![self.act.min(self.screens.len().saturating_sub(1))] };
        let screens = idx
            .into_iter()
            .filter_map(|k| self.screens.get(k).map(|s| (k, s)))
            .map(|(k, s)| {
                let y = s.y.unwrap_or_else(|| Self::auto_y(res, s, self.x));
                let curves = s
                    .curves
                    .iter()
                    .filter(|&&ci| ci < res.data.len() && ci < res.names.len())
                    .map(|&ci| wave_report::Curve {
                        name: res.names[ci].clone(),
                        color: cols[ci],
                        pts: wave_report::decimate(res, ci, self.x, 2000),
                        vals: measure(res, ci, a, b, &ms),
                        at_c1: at(self.c1, ci),
                        at_c2: at(self.c2, ci),
                    })
                    .collect();
                wave_report::Screen { label: format!("畫面 {}", k + 1), index: k, y, curves }
            })
            .collect();
        wave_report::Report {
            title: self.title.clone(),
            x: self.x,
            screens,
            meas: ms,
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
        let cols = self.print_cols(res);
        let many = rep.screens.len() > 1;
        let plot_h = if many { 270.0 } else { 330.0 };
        let mut head = vec!["訊號"];
        head.extend(rep.meas.iter().map(|m| m.head()));
        for scr in &rep.screens {
            // keep a screen's title, plot and table header together
            sh.need(plot_h + fs * 6.0);
            if many {
                sh.heading(&scr.label, fs, pal);
            }
            let srect = Rect::from_min_size(pos2(m, sh.y), vec2(w, plot_h));
            let s = self.screens.get(scr.index).cloned().unwrap_or_default();
            let plot = Self::plot_rect(srect, &st, true);
            if plot.width() > 20.0 && plot.height() > 20.0 {
                let p = sh.p().clone();
                self.paint_plot(&p, plot, &s, scr.y, res, &cols, pal, true);
            }
            sh.y = srect.max.y + 12.0;
            sh.heading(&format!("量測：{}", rep.meas_desc), fs, pal);
            let rows: Vec<Vec<(String, Color32)>> = scr
                .curves
                .iter()
                .map(|c| {
                    let mut v = vec![(c.name.clone(), c.color)];
                    v.extend(rep.meas.iter().zip(&c.vals).map(|(m, x)| (m.fmt(*x), pal.text)));
                    v
                })
                .collect();
            if rows.is_empty() {
                sh.line("（此畫面沒有訊號）", fs, pal.muted);
            } else if rep.meas.is_empty() {
                sh.line("（未選擇量測項目）", fs, pal.muted);
            } else {
                sh.table(w, &head, &rows, fs, pal);
            }
            sh.y += fs * 1.5;
        }
        // cursor readings (each signal once)
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
                        .unique_curves()
                        .into_iter()
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
    fn export_report(&self, ctx: &egui::Context, res: &TranResult, path: &Path, all: bool) -> Result<PathBuf, String> {
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
        let rep = self.report_data(res, all);
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
        self.style = WaveStyle { side_font: v[0], plot_font: v[1], axis_font: v[2], line_w: v[3], ..self.style }.clamped();
    }
    /// measurement column k (1–4) by its name in the source (Max, Freq, DutyP, …; None hides it)
    pub fn script_meas(&mut self, k: usize, name: &str) {
        let m = Meas::LEVEL.iter().chain(Meas::TIME.iter()).copied().find(|m| format!("{:?}", m) == name).unwrap_or(Meas::None);
        if (1..=4).contains(&k) {
            self.style.meas[k - 1] = m;
        }
    }
    pub fn script_all(&mut self, what: &str, on: bool) {
        match what {
            "meas" => self.style.meas_all = on,
            _ => self.style.cur_all = on,
        }
    }
    /// adds blank screens until there are `n`
    pub fn script_screens(&mut self, n: usize) {
        while self.screens.len() < n {
            self.screens.push(Screen::default());
        }
        self.screens.truncate(n.max(1));
        self.act = self.act.min(self.screens.len() - 1);
    }
    pub fn script_select(&mut self, k: usize) {
        if k < self.screens.len() {
            self.act = k;
            self.scroll_to = Some(k);
        }
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
    pub fn script_save(&mut self, path: &Path) {
        self.report_msg = match self.save_wave(path) {
            Ok(p) => format!("已儲存波形檔：{}", p.display()),
            Err(e) => format!("波形檔儲存失敗：{}", e),
        };
    }
    pub fn script_load(&mut self, path: &Path) {
        self.report_msg = match self.load_wave(path) {
            Ok(()) => format!("已開啟波形檔：{}", path.display()),
            Err(e) => format!("無法開啟波形檔：{}", e),
        };
        self.open = true;
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
        // the measurement table as shown: [signal, values…] per listed signal
        let ms = self.meas_cols();
        let ((a, b), _) = self.meas_range();
        let meas: Vec<serde_json::Value> = match &self.res {
            Some(r) => self.listed(st.meas_all).iter().flat_map(|(_, cv)| cv.iter().map(|&ci| serde_json::json!({ "name": r.names[ci], "vals": measure(r, ci, a, b, &ms) })).collect::<Vec<_>>()).collect(),
            None => vec![],
        };
        let meas_heads: Vec<String> = ms.iter().map(|m| format!("{:?}", m)).collect();
        let cur_listed: Vec<String> = match &self.res {
            Some(r) => self.listed(st.cur_all).iter().flat_map(|(_, cv)| cv.iter().map(|&ci| r.names[ci].clone()).collect::<Vec<_>>()).collect(),
            None => vec![],
        };
        let mut v = serde_json::json!({ "open": self.open, "x": [self.x.0, self.x.1], "ys": ys, "names": names, "screens": screens, "math": math, "math_err": self.math_err, "fft": self.fft.is_some(), "colors": colors, "tool": tool, "style": [st.side_font, st.plot_font, st.axis_font, st.line_w], "style_open": self.style_open, "sig_h": self.sig_h, "report_msg": self.report_msg });
        v["meas"] = serde_json::json!(meas);
        v["meas_heads"] = serde_json::json!(meas_heads);
        v["meas_all"] = serde_json::json!(st.meas_all);
        v["cur_all"] = serde_json::json!(st.cur_all);
        v["cur_listed"] = serde_json::json!(cur_listed);
        v["act"] = serde_json::json!(self.act);
        v["cursors"] = serde_json::json!([self.cursors, self.c1, self.c2]);
        v["screen_h"] = serde_json::json!([self.screen_h.0, self.screen_h.1]);
        v["title"] = serde_json::json!(self.title);
        v
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
        self.mcache.clear();
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

/// a waveform file → result and header; checks every size before using it
fn parse_wave(bytes: &[u8]) -> Result<(TranResult, WaveHead), String> {
    let bad = || "不是 DaedalusEngine 波形檔或檔案已損毀".to_string();
    if bytes.len() < 12 || &bytes[..8] != WAVE_MAGIC {
        return Err(bad());
    }
    let fmt = u32::from_le_bytes([bytes[8], bytes[9], bytes[10], bytes[11]]);
    if fmt != WAVE_FORMAT {
        return Err(format!("波形檔格式 {} 不支援（請用較新版本開啟）", fmt));
    }
    // at most 4 GiB of samples
    let raw = miniz_oxide::inflate::decompress_to_vec_zlib_with_limit(&bytes[12..], 1usize << 32).map_err(|_| bad())?;
    if raw.len() < 4 {
        return Err(bad());
    }
    let hl = u32::from_le_bytes([raw[0], raw[1], raw[2], raw[3]]) as usize;
    let hend = 4usize.checked_add(hl).filter(|&e| e <= raw.len()).ok_or_else(bad)?;
    let head: WaveHead = serde_json::from_slice(&raw[4..hend]).map_err(|_| bad())?;
    let ns = head.names.len();
    let need = ns.checked_mul(head.n).and_then(|x| x.checked_mul(8)).ok_or_else(bad)?;
    if raw.len() - hend != need || head.n == 0 || !(head.dt > 0.0) || !head.t0.is_finite() || !head.dt.is_finite() {
        return Err(bad());
    }
    let mut data = Vec::with_capacity(ns);
    for k in 0..ns {
        let s = &raw[hend + k * head.n * 8..hend + (k + 1) * head.n * 8];
        data.push(s.chunks_exact(8).map(|c| f64::from_le_bytes([c[0], c[1], c[2], c[3], c[4], c[5], c[6], c[7]])).collect());
    }
    let r = TranResult {
        t0: head.t0,
        dt: head.dt,
        n: head.n,
        names: head.names.clone(),
        data,
        sweep: head.sweep.as_ref().map(|(var, labels, probes)| SweepInfo { var: var.clone(), labels: labels.clone(), probes: *probes, threads: 0 }),
        ..Default::default()
    };
    Ok((r, head))
}

#[cfg(test)]
mod tests {
    use super::*;

    fn view() -> WaveView {
        let mut r = TranResult::default();
        r.t0 = 0.0;
        r.dt = 1e-6;
        r.n = 1001;
        r.names = vec!["Vout".into(), "I(L1)".into()];
        r.data = vec![(0..1001).map(|i| (i as f64 * 1e-3).sin()).collect(), (0..1001).map(|i| i as f64 * 0.5 - 3.0).collect()];
        let mut w = WaveView::new();
        w.title = "測試".into();
        w.set_result(Arc::new(r), Some(vec![vec!["Vout".into()], vec!["I(L1)".into(), "Vout".into()]]));
        w.pending.push(Pending::AddMath("P".into(), "Vout*I_L1".into()));
        let p = std::mem::take(&mut w.pending);
        for x in p {
            w.apply(x);
        }
        w.x = (1e-4, 6e-4);
        w.cursors = true;
        w.c1 = Some(2e-4);
        w.c2 = Some(5e-4);
        w.colors.insert("Vout".into(), Color32::from_rgb(1, 2, 3));
        w.style.meas = [Meas::Freq, Meas::None, Meas::Rise, Meas::AcRms];
        w.style.line_w = 3.0;
        w.style.meas_all = false;
        w.screens.push(Screen::default());
        w.screens[1].y = Some((-2.0, 7.5));
        w.act = 2;
        w
    }

    #[test]
    fn wave_file_round_trip() {
        let w = view();
        let dir = std::env::temp_dir().join(format!("daedalus_wave_{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let p = w.save_wave(&dir.join("a")).expect("save");
        assert_eq!(p.extension().and_then(|e| e.to_str()), Some(WAVE_EXT));
        let mut v = WaveView::new();
        v.load_wave(&p).expect("load");
        let (a, b) = (w.res.as_ref().unwrap(), v.res.as_ref().unwrap());
        assert_eq!(a.names, b.names, "names incl. the math channel");
        assert_eq!(a.data, b.data, "samples bit-exact (math re-evaluated)");
        assert_eq!(v.layout_names(), w.layout_names());
        assert_eq!((v.x, v.cursors, v.c1, v.c2), (w.x, w.cursors, w.c1, w.c2));
        assert_eq!(v.colors.get("Vout"), Some(&Color32::from_rgb(1, 2, 3)));
        assert_eq!(v.title, "測試");
        // viewer settings, Y zoom, the blank third screen and the selected screen come back too
        assert_eq!(v.style, w.style);
        assert_eq!(v.screens.len(), 3);
        assert_eq!(v.screens[1].y, Some((-2.0, 7.5)));
        assert_eq!(v.act, 2);
        // damaged files are refused, not trusted
        let mut bytes = std::fs::read(&p).unwrap();
        let n = bytes.len();
        bytes.truncate(n - 9);
        assert!(parse_wave(&bytes).is_err());
        assert!(parse_wave(b"DAEDWAVE\x02\0\0\0").is_err());
        assert!(parse_wave(b"hello").is_err());
        let _ = std::fs::remove_dir_all(&dir);
    }

    #[test]
    fn listed_signals_once_per_screen() {
        let w = view();
        // screen 1: Vout and the math channel P (added to the selected screen); screen 2: I(L1), Vout (listed above)
        assert_eq!(w.listed(true), vec![(Some(0), vec![0, 2]), (Some(1), vec![1])]);
        let mut w2 = w;
        w2.act = 1;
        assert_eq!(w2.listed(false), vec![(None, vec![1, 0])]);
        assert_eq!(w2.listed(true).len(), 2);
    }
}
