//! Waveform viewer tools: math channels (arithmetic / functions of probe signals) and FFT spectrum analysis.

use crate::plot::{si, Axes};
use crate::theme::Pal;
use daedalus_core::expr::{self, Syms};
use daedalus_core::runner::TranResult;
use egui::{pos2, vec2, Align2, FontId, Sense, Shape, Stroke};

/* ------------------------------------------------------------------ math channels */

/// a computed signal: `name = expr(probes, t)`
#[derive(Clone, Debug)]
pub struct MathDef {
    pub name: String,
    pub expr: String,
}

/// probe name → identifier usable in an expression ("Vo [Rf=10k]" → "Vo_Rf_10k")
pub fn ident(name: &str) -> String {
    let mut s = String::new();
    for ch in name.chars() {
        if ch.is_ascii_alphanumeric() || ch == '_' {
            s.push(ch);
        } else if !s.ends_with('_') {
            s.push('_');
        }
    }
    let s = s.trim_matches('_').to_string();
    if s.is_empty() || s.chars().next().map(|c| c.is_ascii_digit()).unwrap_or(false) {
        format!("s_{}", s)
    } else {
        s
    }
}

/// identifiers for all signal names (made unique, case-insensitively, as the expression engine ignores case)
pub fn idents(names: &[String]) -> Vec<String> {
    let mut out: Vec<String> = Vec::with_capacity(names.len());
    for n in names {
        let base = ident(n);
        let mut id = base.clone();
        let mut k = 2;
        while id.eq_ignore_ascii_case("t") || out.iter().any(|o| o.eq_ignore_ascii_case(&id)) {
            id = format!("{}_{}", base, k);
            k += 1;
        }
        out.push(id);
    }
    out
}

/// evaluate `expr` over the first `upto` signals of a result (variables: their identifiers and t)
pub fn eval_math(r: &TranResult, upto: usize, src: &str) -> Result<Vec<f64>, String> {
    let upto = upto.min(r.names.len()).min(r.data.len());
    let ids = idents(&r.names[..upto]);
    let mut args: Vec<&str> = vec!["t"];
    args.extend(ids.iter().map(|s| s.as_str()));
    let f = expr::compile_fn(src, &args, &Syms::new()).map_err(|e| e.0)?;
    let mut a = vec![0.0; upto + 1];
    let mut out = Vec::with_capacity(r.n);
    for i in 0..r.n {
        a[0] = r.t0 + i as f64 * r.dt;
        for j in 0..upto {
            a[j + 1] = r.data[j].get(i).copied().unwrap_or(f64::NAN);
        }
        out.push(f.call(&a));
    }
    Ok(out)
}

pub enum MathAct {
    None,
    Add(String, String),
    Close,
}

pub struct MathDlg {
    pub name: String,
    pub expr: String,
    pub err: String,
}

impl MathDlg {
    pub fn new(n: usize) -> Self {
        MathDlg { name: format!("M{}", n + 1), expr: String::new(), err: String::new() }
    }
    pub fn ui(&mut self, ctx: &egui::Context, names: &[String]) -> MathAct {
        let mut act = MathAct::None;
        let mut open = true;
        egui::Window::new("波形運算").open(&mut open).default_width(560.0).resizable(true).show(ctx, |ui| {
            ui.label("以探棒訊號組成運算式，結果會成為新的訊號，可加入畫面、量測、游標讀值與 FFT。");
            egui::Grid::new("math_in").num_columns(2).spacing([8.0, 6.0]).show(ui, |ui| {
                ui.label("名稱");
                ui.add(egui::TextEdit::singleline(&mut self.name).desired_width(160.0));
                ui.end_row();
                ui.label("運算式");
                ui.add(egui::TextEdit::singleline(&mut self.expr).desired_width(400.0).hint_text("例如 Vo*Io、sqrt(Va^2+Vb^2)、(V1-V2)/10"));
                ui.end_row();
            });
            ui.add_space(4.0);
            ui.strong("訊號（點選插入）");
            let ids = idents(names);
            ui.horizontal_wrapped(|ui| {
                for (n, id) in names.iter().zip(ids.iter()) {
                    let b = ui.small_button(id.as_str());
                    if b.clone().on_hover_text(n.as_str()).clicked() {
                        if !self.expr.is_empty() && !self.expr.ends_with([' ', '(', '+', '-', '*', '/', '^', ',']) {
                            self.expr.push(' ');
                        }
                        self.expr.push_str(id);
                    }
                }
                if ui.small_button("t").on_hover_text("時間（秒）").clicked() {
                    self.expr.push('t');
                }
            });
            ui.weak("運算：+ − * / ^ 與括號；函數：sqrt abs sin cos tan asin acos atan atan2 exp log（自然對數）log10 min max pow sign；常數 pi；單位字尾 k m u n 等。");
            if !self.err.is_empty() {
                ui.colored_label(egui::Color32::from_rgb(0xd9, 0x36, 0x3e), &self.err);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("加入").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter)) {
                    act = MathAct::Add(self.name.trim().to_string(), self.expr.trim().to_string());
                }
                if ui.button("關閉").clicked() {
                    act = MathAct::Close;
                }
            });
        });
        if !open {
            act = MathAct::Close;
        }
        act
    }
}

/* ------------------------------------------------------------------ FFT */

/// in-place radix-2 FFT (length must be a power of two)
pub fn fft(re: &mut [f64], im: &mut [f64]) {
    let n = re.len();
    let mut j = 0usize;
    for i in 1..n {
        let mut bit = n >> 1;
        while j & bit != 0 {
            j ^= bit;
            bit >>= 1;
        }
        j |= bit;
        if i < j {
            re.swap(i, j);
            im.swap(i, j);
        }
    }
    let mut len = 2;
    while len <= n {
        let ang = -2.0 * std::f64::consts::PI / len as f64;
        let (wr, wi) = (ang.cos(), ang.sin());
        for start in (0..n).step_by(len) {
            let (mut cr, mut ci) = (1.0f64, 0.0f64);
            for k in 0..len / 2 {
                let (a, b) = (start + k, start + k + len / 2);
                let (xr, xi) = (re[b] * cr - im[b] * ci, re[b] * ci + im[b] * cr);
                re[b] = re[a] - xr;
                im[b] = im[a] - xi;
                re[a] += xr;
                im[a] += xi;
                let t = cr * wr - ci * wi;
                ci = cr * wi + ci * wr;
                cr = t;
            }
        }
        len <<= 1;
    }
}

/// amplitude spectrum of one signal over [a, b]
#[derive(Clone)]
pub struct Spectrum {
    /// frequency resolution 1/(b − a)
    pub df: f64,
    /// peak amplitude of bin k (k = 0: DC value)
    pub amp: Vec<f64>,
    /// phase of bin k in degrees, sine reference (A·sin(ωt + φ) relative to t = a)
    pub ph: Vec<f64>,
    /// FFT length and the number of original samples in the range
    pub m: usize,
    pub n_src: usize,
}

fn interp(d: &[f64], t0: f64, dt: f64, t: f64) -> f64 {
    let f = (t - t0) / dt;
    let i = (f.floor().max(0.0) as usize).min(d.len() - 1);
    let j = (i + 1).min(d.len() - 1);
    let a = (f - i as f64).clamp(0.0, 1.0);
    d[i] + (d[j] - d[i]) * a
}

/// The range is resampled to a power-of-two length over exactly [a, b), so a signal that is periodic with an
/// integer number of periods in the range has its harmonics exactly on the bins (no leakage with the rectangular
/// window). Hann windowing reduces leakage otherwise; amplitudes are corrected for the window gain.
pub fn spectrum(d: &[f64], t0: f64, dt: f64, a: f64, b: f64, hann: bool) -> Option<Spectrum> {
    if d.len() < 2 || !(b > a) || !(dt > 0.0) {
        return None;
    }
    let i0 = (((a - t0) / dt).floor().max(0.0) as usize).min(d.len() - 1);
    let i1 = (((b - t0) / dt).ceil().max(0.0) as usize).min(d.len() - 1);
    let n_src = i1.saturating_sub(i0) + 1;
    let m = n_src.next_power_of_two().clamp(16, 1 << 20);
    let span = b - a;
    let (mut re, mut im) = (vec![0.0; m], vec![0.0; m]);
    let mut sw = 0.0;
    for k in 0..m {
        let t = a + k as f64 * span / m as f64;
        let w = if hann { 0.5 * (1.0 - (2.0 * std::f64::consts::PI * k as f64 / m as f64).cos()) } else { 1.0 };
        let v = interp(d, t0, dt, t);
        re[k] = if v.is_finite() { v * w } else { 0.0 };
        sw += w;
    }
    fft(&mut re, &mut im);
    let half = m / 2;
    let mut amp = Vec::with_capacity(half + 1);
    let mut ph = Vec::with_capacity(half + 1);
    for k in 0..=half {
        let mag = re[k].hypot(im[k]) / sw;
        amp.push(if k == 0 || k == half { mag } else { 2.0 * mag });
        let mut p = im[k].atan2(re[k]).to_degrees() + 90.0;
        if p > 180.0 {
            p -= 360.0;
        }
        ph.push(p);
    }
    Some(Spectrum { df: 1.0 / span, amp, ph, m, n_src })
}

/// fundamental bin: the given frequency (rounded to a bin) or the largest non-DC bin; and THD over its harmonics
pub fn thd(s: &Spectrum, f0: Option<f64>) -> Option<(usize, f64)> {
    let k1 = match f0 {
        Some(f) if f > 0.0 => (f / s.df).round() as usize,
        _ => (1..s.amp.len()).max_by(|&a, &b| s.amp[a].partial_cmp(&s.amp[b]).unwrap_or(std::cmp::Ordering::Equal))?,
    };
    if k1 == 0 || k1 >= s.amp.len() || !(s.amp[k1] > 0.0) {
        return None;
    }
    let mut h2 = 0.0;
    let mut h = 2;
    while h * k1 < s.amp.len() - 1 {
        h2 += s.amp[h * k1].powi(2);
        h += 1;
    }
    Some((k1, h2.sqrt() / s.amp[k1]))
}

pub struct FftView {
    pub ci: usize,
    pub hann: bool,
    pub db: bool,
    pub f0: String,
    pub fmax: String,
    cache: Option<((usize, u64, u64, bool, usize), Spectrum)>,
    hover: Option<usize>,
}

impl FftView {
    pub fn new(ci: usize) -> Self {
        FftView { ci, hann: false, db: false, f0: String::new(), fmax: String::new(), cache: None, hover: None }
    }
    /// returns false when the window was closed
    pub fn ui(&mut self, ctx: &egui::Context, r: &TranResult, range: (f64, f64), range_desc: &str, pal: &Pal, cols: &[egui::Color32]) -> bool {
        let mut open = true;
        egui::Window::new("FFT 頻譜分析").open(&mut open).default_size([760.0, 600.0]).resizable(true).show(ctx, |ui| {
            if r.names.is_empty() {
                ui.label("沒有訊號");
                return;
            }
            self.ci = self.ci.min(r.names.len() - 1);
            ui.horizontal_wrapped(|ui| {
                ui.label("訊號");
                egui::ComboBox::from_id_salt("fft_sig").selected_text(r.names[self.ci].as_str()).width(200.0).show_ui(ui, |ui| {
                    for (i, n) in r.names.iter().enumerate() {
                        ui.selectable_value(&mut self.ci, i, n.as_str());
                    }
                });
                ui.separator();
                ui.label("窗函數");
                ui.radio_value(&mut self.hann, false, "矩形");
                ui.radio_value(&mut self.hann, true, "Hann");
                ui.separator();
                ui.label("基本頻率");
                ui.add(egui::TextEdit::singleline(&mut self.f0).desired_width(70.0).hint_text("自動"));
                ui.label("Hz　上限");
                ui.add(egui::TextEdit::singleline(&mut self.fmax).desired_width(70.0).hint_text("自動"));
                ui.label("Hz");
                ui.checkbox(&mut self.db, "dB");
            });
            let key = (self.ci, range.0.to_bits(), range.1.to_bits(), self.hann, r.n);
            if self.cache.as_ref().map(|c| c.0 != key).unwrap_or(true) {
                self.cache = spectrum(&r.data[self.ci], r.t0, r.dt, range.0, range.1, self.hann).map(|s| (key, s));
            }
            let s = match &self.cache {
                Some((_, s)) => s.clone(),
                None => {
                    ui.label("分析範圍內的資料不足。");
                    return;
                }
            };
            let span = range.1 - range.0;
            ui.weak(format!(
                "分析範圍：{}，{} ~ {}（T = {}，解析度 Δf = {}）；{} 個取樣內插為 {} 點 FFT。",
                range_desc,
                si(range.0, "s", 5),
                si(range.1, "s", 5),
                si(span, "s", 5),
                si(s.df, "Hz", 4),
                s.n_src,
                s.m
            ));
            let syms = Syms::new();
            let f0 = expr::eval_const(self.f0.trim(), &syms).ok().filter(|v| *v > 0.0);
            let th = thd(&s, f0);
            if let Some(f) = f0 {
                let p = f * span;
                if (p - p.round()).abs() > 0.02 {
                    ui.colored_label(pal.warn, format!("分析範圍是基本波週期的 {:.3} 倍，不是整數倍：頻譜會洩漏。請用游標框選整數個週期，或改用 Hann 窗。", p));
                }
            }
            // spectrum plot
            let nyq = s.df * (s.amp.len() - 1) as f64;
            let auto_max = th.map(|(k1, _)| (k1 as f64 * s.df * 25.0).min(nyq)).unwrap_or(nyq);
            let fmax = expr::eval_const(self.fmax.trim(), &syms).ok().filter(|v| *v > 0.0).unwrap_or(auto_max).min(nyq).max(s.df * 4.0);
            let kmax = ((fmax / s.df).ceil() as usize).min(s.amp.len() - 1);
            let val = |a: f64| if self.db { 20.0 * a.max(1e-300).log10() } else { a };
            let peak = s.amp[..=kmax].iter().cloned().fold(0.0f64, f64::max).max(1e-300);
            let y = if self.db { (val(peak) - 120.0, val(peak) + 6.0) } else { (0.0, peak * 1.08) };
            let w = ui.available_width();
            let (rect, resp) = ui.allocate_exact_size(vec2(w, 280.0), Sense::hover());
            let plot = egui::Rect::from_min_max(pos2(rect.min.x + 64.0, rect.min.y + 6.0), pos2(rect.max.x - 10.0, rect.max.y - 22.0));
            let ax = Axes { plot, x: (0.0, kmax as f64 * s.df), y, xlog: false };
            let painter = ui.painter_at(rect);
            ax.draw(&painter, pal, "Hz", true);
            let pc = ui.painter_at(plot);
            let col = cols.get(self.ci).copied().unwrap_or(pal.curves[self.ci % 8]);
            let bottom = y.0;
            if (kmax as f32) < plot.width() / 3.0 {
                for k in 0..=kmax {
                    let x = ax.xs(k as f64 * s.df);
                    let yv = val(s.amp[k]).max(bottom);
                    pc.line_segment([pos2(x, ax.ys(bottom)), pos2(x, ax.ys(yv))], Stroke::new(2.0, col));
                }
            } else {
                // many bins: the largest value per pixel column
                let mut pts = Vec::new();
                let px = plot.width().max(1.0) as usize;
                let mut k = 0usize;
                for c in 0..=px {
                    let k_end = (((c + 1) as f64 / px as f64) * kmax as f64).ceil() as usize;
                    let mut m = f64::NEG_INFINITY;
                    while k <= kmax && k <= k_end {
                        m = m.max(val(s.amp[k]));
                        k += 1;
                    }
                    if m.is_finite() {
                        let x = plot.min.x + c as f32;
                        pts.push(pos2(x, ax.ys(bottom)));
                        pts.push(pos2(x, ax.ys(m.max(bottom))));
                        pts.push(pos2(x, ax.ys(bottom)));
                    }
                }
                if pts.len() >= 2 {
                    pc.add(Shape::line(pts, Stroke::new(1.0, col)));
                }
            }
            self.hover = None;
            if let Some(p) = resp.hover_pos().filter(|p| plot.contains(*p)) {
                let k = ((ax.xv(p.x) / s.df).round().max(0.0) as usize).min(kmax);
                self.hover = Some(k);
                let x = ax.xs(k as f64 * s.df);
                pc.line_segment([pos2(x, plot.min.y), pos2(x, plot.max.y)], Stroke::new(1.0, pal.faint));
                let txt = format!("{}：{}{}", si(k as f64 * s.df, "Hz", 5), si(s.amp[k], "", 5), if self.db { format!("（{:.1} dB）", val(s.amp[k])) } else { String::new() });
                pc.text(pos2(plot.min.x + 6.0, plot.min.y + 4.0), Align2::LEFT_TOP, txt, FontId::proportional(13.0), pal.text);
            }
            // numbers
            ui.horizontal_wrapped(|ui| {
                ui.label(format!("直流成分 {}", si(s.amp[0], "", 5)));
                if let Some((k1, t)) = th {
                    ui.separator();
                    ui.strong(format!("基本波 {}：振幅 {}、相位 {:.2}°", si(k1 as f64 * s.df, "Hz", 5), si(s.amp[k1], "", 5), s.ph[k1]));
                    ui.separator();
                    ui.strong(format!("THD = {:.3} %", t * 100.0));
                    ui.separator();
                    ui.label(format!("有效值（基本波）{}", si(s.amp[k1] / std::f64::consts::SQRT_2, "", 5)));
                }
            });
            if let Some((k1, _)) = th {
                egui::ScrollArea::vertical().max_height(170.0).show(ui, |ui| {
                    egui::Grid::new("fft_h").striped(true).num_columns(5).show(ui, |ui| {
                        for h in ["次數", "頻率", "振幅", "相對基本波", "相位"] {
                            ui.strong(h);
                        }
                        ui.end_row();
                        let mut h = 1;
                        while h * k1 < s.amp.len() && h <= 40 {
                            let k = h * k1;
                            ui.label(format!("{}", h));
                            ui.monospace(si(k as f64 * s.df, "Hz", 5));
                            ui.monospace(si(s.amp[k], "", 5));
                            ui.monospace(format!("{:.3} %", s.amp[k] / s.amp[k1] * 100.0));
                            ui.monospace(format!("{:.2}°", s.ph[k]));
                            ui.end_row();
                            h += 1;
                        }
                    });
                });
            }
            ui.horizontal(|ui| {
                if ui.button("匯出頻譜 CSV").clicked() {
                    let mut csv = String::from("Frequency (Hz),Amplitude,Phase (deg)\n");
                    for k in 0..s.amp.len() {
                        csv.push_str(&format!("{},{},{}\n", k as f64 * s.df, s.amp[k], s.ph[k]));
                    }
                    if let Some(p) = rfd::FileDialog::new().add_filter("CSV", &["csv"]).set_file_name(format!("FFT_{}.csv", ident(&r.names[self.ci]))).save_file() {
                        let _ = std::fs::write(p, csv);
                    }
                }
                ui.weak("振幅為峰值；相位以 sin 為 0°、相對分析範圍起點。");
            });
        });
        open
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    fn res(names: &[&str], data: Vec<Vec<f64>>, dt: f64) -> TranResult {
        let n = data[0].len();
        TranResult { t0: 0.0, dt, n, names: names.iter().map(|s| s.to_string()).collect(), data, ..Default::default() }
    }
    #[test]
    fn idents_are_unique_identifiers() {
        let n: Vec<String> = ["Vo", "Vo [Rf=10k]", "vo", "1x", "t"].iter().map(|s| s.to_string()).collect();
        assert_eq!(idents(&n), vec!["Vo", "Vo_Rf_10k", "vo_2", "s_1x", "t_2"]);
    }
    #[test]
    fn math_channels() {
        let dt = 1e-3;
        let a: Vec<f64> = (0..100).map(|i| i as f64).collect();
        let b: Vec<f64> = (0..100).map(|i| 2.0 * i as f64 + 1.0).collect();
        let r = res(&["Va", "I b"], vec![a, b], dt);
        let p = eval_math(&r, 2, "Va*I_b").unwrap();
        assert_eq!(p[3], 3.0 * 7.0);
        let q = eval_math(&r, 2, "sqrt(Va) + t*1000").unwrap();
        assert!((q[16] - (4.0 + 16.0)).abs() < 1e-12);
        assert!(eval_math(&r, 2, "Va +* 2").is_err());
        assert!(eval_math(&r, 2, "nosuch * 2").is_err());
    }
    #[test]
    fn fft_amplitude_phase_thd() {
        // 50 Hz, amplitude 3, phase 30°, plus 20 % 5th harmonic and 0.5 DC; 4 periods, 10 µs steps
        let dt = 1e-5;
        let n = 8001;
        let w = 2.0 * std::f64::consts::PI * 50.0;
        let d: Vec<f64> = (0..n).map(|i| {
            let t = i as f64 * dt;
            0.5 + 3.0 * (w * t + 30f64.to_radians()).sin() + 0.6 * (5.0 * w * t).sin()
        }).collect();
        let s = spectrum(&d, 0.0, dt, 0.0, 0.08, false).unwrap();
        assert!((s.df - 12.5).abs() < 1e-9);
        assert!((s.amp[0] - 0.5).abs() < 1e-4, "dc {}", s.amp[0]);
        assert!((s.amp[4] - 3.0).abs() < 1e-3, "fund {}", s.amp[4]);
        assert!((s.ph[4] - 30.0).abs() < 0.05, "phase {}", s.ph[4]);
        assert!((s.amp[20] - 0.6).abs() < 1e-3, "5th {}", s.amp[20]);
        let (k1, t) = thd(&s, None).unwrap();
        assert_eq!(k1, 4);
        assert!((t - 0.2).abs() < 1e-3, "thd {}", t);
        let (k1b, _) = thd(&s, Some(50.0)).unwrap();
        assert_eq!(k1b, 4);
        // Hann window keeps the amplitude of an on-bin sine
        let sh = spectrum(&d, 0.0, dt, 0.0, 0.08, true).unwrap();
        assert!((sh.amp[4] - 3.0).abs() < 2e-3, "hann {}", sh.amp[4]);
    }
}
