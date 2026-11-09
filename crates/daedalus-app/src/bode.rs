//! Bode plot viewer for AC sweep results.

use crate::plot::{si, Axes};
use crate::theme::Pal;
use daedalus_core::runner::{AcCurve, AcResult};
use egui::{pos2, vec2, Align2, FontId, Rect, Sense, Shape, Stroke, Ui};
use std::sync::Arc;

pub struct BodeView {
    pub res: Option<Arc<AcResult>>,
    pub visible: Vec<bool>,
    pub open: bool,
    hover_f: Option<f64>,
    pub title: String,
}

fn interp_f(f0: f64, f1: f64, r: f64) -> f64 {
    f0 * (f1 / f0).powf(r)
}

/// crossover / margins / bandwidth summary (same analysis as the original viewer)
pub fn summary(c: &AcCurve, fs: &[f64]) -> Vec<String> {
    let mut out = vec![];
    if fs.is_empty() {
        return out;
    }
    for k in 1..fs.len() {
        let (a, b) = (c.mag[k - 1], c.mag[k]);
        if (a >= 0.0 && b < 0.0) || (a < 0.0 && b >= 0.0) {
            let r = a / (a - b);
            let f = interp_f(fs[k - 1], fs[k], r);
            let ph = c.ph[k - 1] + (c.ph[k] - c.ph[k - 1]) * r;
            let mut pm = ph + 180.0;
            while pm > 180.0 {
                pm -= 360.0;
            }
            while pm < -180.0 {
                pm += 360.0;
            }
            if c.lp {
                out.push(format!("穿越頻率 fc = {}", si(f, "Hz", 4)));
                out.push(format!("相位 = {:.2}°，相位裕度 PM = {:.2}°", ph, pm));
            } else {
                out.push(format!("0 dB 穿越 @ {}（相位 {:.2}°）", si(f, "Hz", 4), ph));
            }
            break;
        }
    }
    if c.lp {
        let m = |x: f64| ((x % 360.0) + 540.0) % 360.0 - 180.0;
        for k in 1..fs.len() {
            let (pa, pb) = (m(c.ph[k - 1] + 180.0), m(c.ph[k] + 180.0));
            if (pa - pb).abs() < 90.0 && ((pa >= 0.0 && pb < 0.0) || (pa < 0.0 && pb >= 0.0)) {
                let r = pa / (pa - pb);
                let f = interp_f(fs[k - 1], fs[k], r);
                let g = c.mag[k - 1] + (c.mag[k] - c.mag[k - 1]) * r;
                out.push(format!("相位 −180° 於 {}，增益裕度 GM = {:.2} dB", si(f, "Hz", 4), -g));
                break;
            }
        }
    }
    let mut pk = 0;
    for k in 0..c.mag.len() {
        if c.mag[k] > c.mag[pk] {
            pk = k;
        }
    }
    out.push(format!("低頻增益 {:.2} dB @ {}", c.mag[0], si(fs[0], "Hz", 4)));
    out.push(format!("峰值 {:.2} dB @ {}", c.mag[pk], si(fs[pk], "Hz", 4)));
    if !c.lp {
        let rf = c.mag[0] - 3.0;
        for k in 1..fs.len() {
            if c.mag[k - 1] >= rf && c.mag[k] < rf {
                let r = (c.mag[k - 1] - rf) / (c.mag[k - 1] - c.mag[k]);
                out.push(format!("−3 dB 頻寬 {}", si(interp_f(fs[k - 1], fs[k], r), "Hz", 4)));
                break;
            }
        }
    }
    out
}

impl BodeView {
    pub fn new() -> Self {
        BodeView { res: None, visible: vec![], open: false, hover_f: None, title: String::new() }
    }
    pub fn set_result(&mut self, r: Arc<AcResult>) {
        self.visible = vec![true; r.curves.len()];
        self.res = Some(r);
        self.open = true;
    }
    fn csv(r: &AcResult) -> String {
        let mut s = String::from("Freq");
        for c in &r.curves {
            s.push_str(&format!(",{0}_mag_dB,{0}_phase_deg,{0}_re,{0}_im", c.name));
        }
        s.push('\n');
        for (i, f) in r.freqs.iter().enumerate() {
            s.push_str(&format!("{}", f));
            for c in &r.curves {
                s.push_str(&format!(",{},{},{},{}", c.mag[i], c.ph[i], c.re[i], c.im[i]));
            }
            s.push('\n');
        }
        s
    }
    fn val(c: &AcCurve, fs: &[f64], f: f64) -> (f64, f64) {
        if fs.is_empty() {
            return (f64::NAN, f64::NAN);
        }
        if f <= fs[0] {
            return (c.mag[0], c.ph[0]);
        }
        for k in 1..fs.len() {
            if f <= fs[k] {
                let r = (f / fs[k - 1]).ln() / (fs[k] / fs[k - 1]).ln();
                return (c.mag[k - 1] + (c.mag[k] - c.mag[k - 1]) * r, c.ph[k - 1] + (c.ph[k] - c.ph[k - 1]) * r);
            }
        }
        (*c.mag.last().unwrap(), *c.ph.last().unwrap())
    }
    pub fn ui(&mut self, ui: &mut Ui, pal: &Pal) {
        let res = match &self.res {
            Some(r) => r.clone(),
            None => {
                ui.label("尚無 AC 掃頻結果。");
                return;
            }
        };
        egui::Panel::top("bv_tools").show(ui, |ui| {
            ui.horizontal(|ui| {
                if crate::icons::button(ui, crate::icons::Icon::Export, "匯出 CSV", "匯出所有曲線的增益、相位與實虛部", true, false, None).clicked() {
                    if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("{}_bode.csv", self.title)).save_file() {
                        let _ = std::fs::write(p, Self::csv(&res));
                    }
                }
                ui.weak(format!("{} 個頻率點", res.freqs.len()));
            });
        });
        egui::Panel::left("bv_side").default_size(300.0).show(ui, |ui| {
            egui::ScrollArea::vertical().show(ui, |ui| {
                ui.strong("曲線");
                for (i, c) in res.curves.iter().enumerate() {
                    ui.horizontal(|ui| {
                        let (r, _) = ui.allocate_exact_size(vec2(12.0, 12.0), Sense::hover());
                        ui.painter().rect_filled(r, 2.0, pal.curves[i % 8]);
                        ui.checkbox(&mut self.visible[i], format!("{}{}", c.name, if c.lp { "（迴路增益）" } else { "" }));
                    });
                }
                ui.separator();
                ui.strong("游標讀值");
                match self.hover_f {
                    Some(f) => {
                        ui.label(format!("f = {}", si(f, "Hz", 5)));
                        for (i, c) in res.curves.iter().enumerate() {
                            if self.visible[i] {
                                let (m, p) = Self::val(c, &res.freqs, f);
                                ui.colored_label(pal.curves[i % 8], format!("{}：{:.3} dB，{:.2}°", c.name, m, p));
                            }
                        }
                    }
                    None => {
                        ui.weak("將游標移到圖上");
                    }
                }
                ui.separator();
                ui.strong("分析");
                for (i, c) in res.curves.iter().enumerate() {
                    ui.colored_label(pal.curves[i % 8], egui::RichText::new(&c.name).strong());
                    for l in summary(c, &res.freqs) {
                        ui.label(l);
                    }
                    ui.add_space(4.0);
                }
            });
        });
        egui::CentralPanel::default().show(ui, |ui| {
            let area = ui.available_rect_before_wrap();
            let f0 = res.freqs.first().copied().unwrap_or(1.0);
            let f1 = res.freqs.last().copied().unwrap_or(10.0).max(f0 * 1.0001);
            let h = area.height() / 2.0;
            self.hover_f = None;
            for k in 0..2 {
                let srect = Rect::from_min_size(pos2(area.min.x, area.min.y + h * k as f32), vec2(area.width(), h));
                let plot = Rect::from_min_max(pos2(srect.min.x + 64.0, srect.min.y + 8.0), pos2(srect.max.x - 10.0, srect.max.y - 22.0));
                if plot.height() < 20.0 {
                    continue;
                }
                let (mut mn, mut mx) = (f64::INFINITY, f64::NEG_INFINITY);
                for (i, c) in res.curves.iter().enumerate() {
                    if !self.visible[i] {
                        continue;
                    }
                    for &v in if k == 0 { &c.mag } else { &c.ph } {
                        if v.is_finite() {
                            mn = mn.min(v);
                            mx = mx.max(v);
                        }
                    }
                }
                if !mn.is_finite() {
                    mn = -1.0;
                    mx = 1.0;
                }
                let pad = ((mx - mn) * 0.08).max(if k == 0 { 1.0 } else { 5.0 });
                let ax = Axes { plot, x: (f0, f1), y: (mn - pad, mx + pad), xlog: true };
                let painter = ui.painter_at(srect);
                ax.draw(&painter, pal, "Hz", true);
                painter.text(pos2(plot.max.x - 4.0, plot.min.y + 4.0), Align2::RIGHT_TOP, if k == 0 { "增益 (dB)" } else { "相位 (°)" }, FontId::proportional(12.0), pal.muted);
                let pc = ui.painter_at(plot);
                if k == 0 && ax.y.0 < 0.0 && ax.y.1 > 0.0 {
                    pc.line_segment([pos2(plot.min.x, ax.ys(0.0)), pos2(plot.max.x, ax.ys(0.0))], Stroke::new(1.0, pal.faint));
                }
                for (i, c) in res.curves.iter().enumerate() {
                    if !self.visible[i] {
                        continue;
                    }
                    let ys = if k == 0 { &c.mag } else { &c.ph };
                    let pts: Vec<egui::Pos2> = res.freqs.iter().zip(ys.iter()).filter(|(_, v)| v.is_finite()).map(|(&f, &v)| pos2(ax.xs(f), ax.ys(v))).collect();
                    if pts.len() >= 2 {
                        pc.add(Shape::line(pts.clone(), Stroke::new(1.6, pal.curves[i % 8])));
                    }
                    for p in pts {
                        pc.circle_filled(p, 2.0, pal.curves[i % 8]);
                    }
                }
                if let Some(p) = ui.input(|i| i.pointer.hover_pos()).filter(|p| plot.contains(*p)) {
                    self.hover_f = Some(ax.xv(p.x));
                }
                if let Some(f) = self.hover_f {
                    let x = ax.xs(f);
                    pc.line_segment([pos2(x, plot.min.y), pos2(x, plot.max.y)], Stroke::new(1.0, pal.faint));
                }
            }
        });
    }
}
