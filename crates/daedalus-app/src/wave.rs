//! Waveform viewer (SIMVIEW-style): stacked screens sharing one time axis, cursors, measurements.

use crate::plot::{curve_points, si, Axes};
use crate::theme::Pal;
use daedalus_core::runner::TranResult;
use egui::{pos2, vec2, Align2, Color32, FontId, Pos2, Rect, Sense, Shape, Stroke, Ui};
use std::sync::Arc;

#[derive(Clone, Copy, PartialEq)]
pub enum Tool {
    Zoom,
    Pan,
    Box,
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
}

impl WaveView {
    pub fn new() -> Self {
        WaveView { res: None, screens: vec![], act: 0, x: (0.0, 1.0), cursors: false, c1: None, c2: None, tool: Tool::Zoom, open: false, drag: None, hover_t: None, title: String::new() }
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
            let n = names.len();
            if n <= 3 {
                screens = vec![Screen { curves: (0..n).collect(), y: None }];
            } else {
                let per = n.div_ceil(4).max(2);
                screens = (0..n).collect::<Vec<_>>().chunks(per).map(|c| Screen { curves: c.to_vec(), y: None }).collect();
            }
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
        let res = match &self.res {
            Some(r) => r.clone(),
            None => {
                ui.label("尚無模擬結果，請先按 F5 執行模擬。");
                return;
            }
        };
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
                ui.separator();
                if button(ui, Icon::Cursor, "游標", "左鍵設定 C1、右鍵設定 C2，拖曳游標線可移動", true, self.cursors, None).clicked() {
                    self.cursors = !self.cursors;
                }
                ui.separator();
                if button(ui, Icon::Export, "匯出 CSV", "匯出目前顯示時間範圍內的所有訊號", true, false, None).clicked() {
                    if let Some(csv) = self.csv() {
                        if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}.csv", self.title)).save_file() {
                            let _ = std::fs::write(p, csv);
                        }
                    }
                }
            });
        });
        // side panel: variables, readouts, measurements
        egui::Panel::left("wv_side").default_size(280.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.strong("探棒訊號（勾選加入目前畫面）");
                for (ci, name) in res.names.iter().enumerate() {
                    let mut on = self.screens.get(self.act).map(|s| s.curves.contains(&ci)).unwrap_or(false);
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, Self::color(pal, ci));
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
                    });
                }
                ui.separator();
                ui.strong("游標讀值");
                if let Some(t) = self.hover_t {
                    ui.label(format!("t = {}", si(t, "s", 6)));
                    if let Some(s) = self.screens.get(self.act) {
                        for &ci in &s.curves {
                            ui.colored_label(Self::color(pal, ci), format!("{} = {}", res.names[ci], si(Self::val_at(&res, ci, t), "", 5)));
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
                            ui.colored_label(Self::color(pal, ci), &res.names[ci]);
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
                                ui.colored_label(Self::color(pal, ci), &res.names[ci]);
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
                let plot = Rect::from_min_max(pos2(srect.min.x + 64.0, srect.min.y + 6.0), pos2(srect.max.x - 10.0, srect.max.y - if last { 22.0 } else { 8.0 }));
                if plot.width() < 20.0 || plot.height() < 20.0 {
                    continue;
                }
                let y = self.screens[si_].y.unwrap_or_else(|| Self::auto_y(&res, &self.screens[si_], self.x));
                let ax = Axes { plot, x: self.x, y, xlog: false };
                let painter = ui.painter_at(srect);
                ax.draw(&painter, pal, "s", last);
                if si_ == self.act && self.screens.len() > 1 {
                    painter.rect_stroke(plot.expand(1.0), 0.0, Stroke::new(2.0, pal.accent), egui::StrokeKind::Outside);
                }
                let pclip = ui.painter_at(plot);
                for &ci in &self.screens[si_].curves {
                    let pts = curve_points(&ax, res.t0, res.dt, &res.data[ci]);
                    if pts.len() >= 2 {
                        pclip.add(Shape::line(pts, Stroke::new(1.4, Self::color(pal, ci))));
                    }
                }
                // legend: every curve while it fits; many curves (sweeps, Monte Carlo) get a one-line summary
                // (the probe list on the left still names and colours each one)
                let cv = &self.screens[si_].curves;
                let mut ly = plot.min.y + 4.0;
                let fit = ((plot.height() - 8.0) / 16.0).floor().max(1.0) as usize;
                if cv.len() <= fit.min(12) {
                    for &ci in cv {
                        let r = pclip.text(pos2(plot.min.x + 8.0, ly), Align2::LEFT_TOP, &res.names[ci], FontId::proportional(13.0), Self::color(pal, ci));
                        ly = r.max.y + 1.0;
                    }
                } else {
                    let base = |n: &str| n.split(" [").next().unwrap_or(n).to_string();
                    let mut bases: Vec<String> = Vec::new();
                    for &ci in cv {
                        let b = base(&res.names[ci]);
                        if !bases.contains(&b) {
                            bases.push(b);
                        }
                    }
                    let txt = format!("{}：{} 條曲線", bases.join("、"), cv.len());
                    pclip.text(pos2(plot.min.x + 8.0, ly), Align2::LEFT_TOP, txt, FontId::proportional(13.0), pal.text);
                }
                // cursors
                if self.cursors {
                    for (k, c) in [(1u8, self.c1), (2u8, self.c2)] {
                        if let Some(t) = c {
                            let x = ax.xs(t);
                            if x >= plot.min.x && x <= plot.max.x {
                                let col = if k == 1 { pal.accent } else { pal.ctl };
                                pclip.extend(Shape::dashed_line(&[pos2(x, plot.min.y), pos2(x, plot.max.y)], Stroke::new(1.2, col), 5.0, 3.0));
                                pclip.text(pos2(x + 3.0, plot.min.y + 2.0), Align2::LEFT_TOP, format!("C{}", k), FontId::proportional(11.0), col);
                            }
                        }
                    }
                }
                // interaction
                let id = ui.scope_id().with(("wplot", si_));
                let resp = ui.interact(plot, id, Sense::click_and_drag());
                let ptr = ui.input(|i| i.pointer.hover_pos());
                if let Some(p) = ptr.filter(|p| plot.contains(*p)) {
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
    }
}
