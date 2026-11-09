//! Plot helpers: tick generation, SI formatting, axis drawing.

use crate::theme::Pal;
use egui::{pos2, Align2, FontId, Painter, Rect, Stroke};

/// value with SI prefix, `dig` significant digits
pub fn si(v: f64, unit: &str, dig: usize) -> String {
    if v == 0.0 {
        return if unit.is_empty() { "0".into() } else { format!("0 {}", unit) };
    }
    if !v.is_finite() {
        return format!("{}", v);
    }
    let a = v.abs();
    const P: [(f64, &str); 9] = [(1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p"), (1e-15, "f")];
    for (m, s) in P {
        if a >= m * 0.9999999 {
            let x = v / m;
            let t = daedalus_core::util::js_to_precision(x, dig);
            let t: f64 = t.parse().unwrap_or(x);
            let mut t = format!("{}", t);
            if t.contains('e') {
                t = format!("{:.3}", x);
            }
            return format!("{} {}{}", t, s, unit).trim_end().to_string();
        }
    }
    format!("{:.3e} {}", v, unit).trim_end().to_string()
}

pub fn nice_ticks(mut a: f64, mut b: f64, n: usize) -> (Vec<f64>, f64) {
    if !(b > a) {
        let d = if a.abs() > 0.0 { a.abs() } else { 1.0 };
        a -= d * 0.5;
        b += d * 0.5;
    }
    let span = b - a;
    let raw = span / n.max(1) as f64;
    let p = 10f64.powf(raw.log10().floor());
    let m = raw / p;
    let step = (if m < 1.5 {
        1.0
    } else if m < 3.0 {
        2.0
    } else if m < 7.0 {
        5.0
    } else {
        10.0
    }) * p;
    let mut out = Vec::new();
    let mut v = (a / step).ceil() * step;
    let mut guard = 0;
    while v <= b + step * 1e-9 && guard < 200 {
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
        guard += 1;
    }
    (out, step)
}

pub fn fmt_tick(v: f64, step: f64) -> String {
    if v == 0.0 {
        return "0".into();
    }
    let a = v.abs().max(step.abs());
    const P: [(f64, &str); 8] = [(1e9, "G"), (1e6, "M"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p")];
    for (m, s) in P {
        if a >= m * 0.999 {
            let dec = (-(step / m).log10() - 1e-9).ceil().clamp(0.0, 6.0) as usize;
            return format!("{:.*}{}", dec, v / m, s);
        }
    }
    format!("{:.1e}", v)
}

pub struct Axes {
    pub plot: Rect,
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub xlog: bool,
}
impl Axes {
    pub fn xs(&self, x: f64) -> f32 {
        let (a, b) = if self.xlog { (self.x.0.log10(), self.x.1.log10()) } else { self.x };
        let xv = if self.xlog { x.log10() } else { x };
        self.plot.min.x + ((xv - a) / (b - a)) as f32 * self.plot.width()
    }
    pub fn ys(&self, y: f64) -> f32 {
        self.plot.max.y - ((y - self.y.0) / (self.y.1 - self.y.0)) as f32 * self.plot.height()
    }
    pub fn xv(&self, px: f32) -> f64 {
        let f = ((px - self.plot.min.x) / self.plot.width()) as f64;
        if self.xlog {
            let (a, b) = (self.x.0.log10(), self.x.1.log10());
            10f64.powf(a + f * (b - a))
        } else {
            self.x.0 + f * (self.x.1 - self.x.0)
        }
    }
    pub fn yv(&self, py: f32) -> f64 {
        let f = ((self.plot.max.y - py) / self.plot.height()) as f64;
        self.y.0 + f * (self.y.1 - self.y.0)
    }
    /// grid, frame and tick labels
    pub fn draw(&self, p: &Painter, pal: &Pal, xunit: &str, show_x_labels: bool) {
        self.draw_sized(p, pal, xunit, show_x_labels, 11.0);
    }
    /// the same with tick labels of `fs` points (the tick spacing grows with the labels)
    pub fn draw_sized(&self, p: &Painter, pal: &Pal, xunit: &str, show_x_labels: bool, fs: f32) {
        let r = self.plot;
        p.rect_filled(r, 0.0, pal.canvas);
        let font = FontId::monospace(fs);
        let (yt, ys) = nice_ticks(self.y.0, self.y.1, ((r.height() / (fs * 3.6)) as usize).max(2));
        for v in yt {
            let y = self.ys(v);
            if y < r.min.y - 0.5 || y > r.max.y + 0.5 {
                continue;
            }
            p.line_segment([pos2(r.min.x, y), pos2(r.max.x, y)], Stroke::new(1.0, pal.grid2));
            p.text(pos2(r.min.x - 4.0, y), Align2::RIGHT_CENTER, fmt_tick(v, ys), font.clone(), pal.muted);
        }
        if self.xlog {
            let (a, b) = (self.x.0.log10().floor() as i32, self.x.1.log10().ceil() as i32);
            for dcd in a..=b {
                for m in 1..10 {
                    let f = m as f64 * 10f64.powi(dcd);
                    if f < self.x.0 || f > self.x.1 {
                        continue;
                    }
                    let x = self.xs(f);
                    p.line_segment([pos2(x, r.min.y), pos2(x, r.max.y)], Stroke::new(if m == 1 { 1.0 } else { 0.5 }, if m == 1 { pal.grid } else { pal.grid2 }));
                    if m == 1 && show_x_labels {
                        p.text(pos2(x, r.max.y + 3.0), Align2::CENTER_TOP, si(f, xunit, 3), font.clone(), pal.muted);
                    }
                }
            }
        } else {
            let (xt, xs) = nice_ticks(self.x.0, self.x.1, ((r.width() / (fs * 8.2)) as usize).max(2));
            for v in xt {
                let x = self.xs(v);
                if x < r.min.x - 0.5 || x > r.max.x + 0.5 {
                    continue;
                }
                p.line_segment([pos2(x, r.min.y), pos2(x, r.max.y)], Stroke::new(1.0, pal.grid2));
                if show_x_labels {
                    p.text(pos2(x, r.max.y + 3.0), Align2::CENTER_TOP, format!("{}{}", fmt_tick(v, xs), xunit), font.clone(), pal.muted);
                }
            }
        }
        p.rect_stroke(r, 0.0, Stroke::new(1.0, pal.line), egui::StrokeKind::Inside);
    }
}

/// polyline of y(t) for t on a uniform grid, decimated to min/max per pixel column
pub fn curve_points(ax: &Axes, t0: f64, dt: f64, data: &[f64]) -> Vec<egui::Pos2> {
    let n = data.len();
    if n == 0 {
        return vec![];
    }
    let i0 = (((ax.x.0 - t0) / dt).floor().max(0.0) as usize).min(n - 1);
    let i1 = (((ax.x.1 - t0) / dt).ceil().max(0.0) as usize).min(n - 1);
    let cnt = i1.saturating_sub(i0) + 1;
    let w = ax.plot.width().max(1.0) as usize;
    let clampy = |y: f32| y.clamp(ax.plot.min.y - 2000.0, ax.plot.max.y + 2000.0);
    let mut out = Vec::new();
    if cnt <= 3 * w {
        for i in i0..=i1 {
            let v = data[i];
            if !v.is_finite() {
                continue;
            }
            out.push(pos2(ax.xs(t0 + i as f64 * dt), clampy(ax.ys(v))));
        }
        return out;
    }
    // min/max per column
    let per = cnt as f64 / w as f64;
    for c in 0..w {
        let a = i0 + (c as f64 * per) as usize;
        let b = (i0 + ((c + 1) as f64 * per) as usize).min(i1 + 1);
        if a >= b {
            continue;
        }
        let (mut mn, mut mx, mut imn, mut imx) = (f64::INFINITY, f64::NEG_INFINITY, a, a);
        for (i, &v) in data[a..b].iter().enumerate() {
            if v < mn {
                mn = v;
                imn = a + i;
            }
            if v > mx {
                mx = v;
                imx = a + i;
            }
        }
        if !mn.is_finite() || !mx.is_finite() {
            continue;
        }
        let x = ax.plot.min.x + c as f32 + 0.5;
        if imn <= imx {
            out.push(pos2(x, clampy(ax.ys(mn))));
            out.push(pos2(x, clampy(ax.ys(mx))));
        } else {
            out.push(pos2(x, clampy(ax.ys(mx))));
            out.push(pos2(x, clampy(ax.ys(mn))));
        }
    }
    out
}
