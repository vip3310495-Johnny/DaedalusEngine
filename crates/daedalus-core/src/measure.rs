//! Oscilloscope-style automatic measurements of one waveform over a time range (waveform viewer, reports).
//!
//! The waveform is the polyline through the stored samples; the range ends are interpolated, so a measurement
//! covers exactly [a, b]. Integrals use the trapezoid rule (as the measurement table always did).
//! Levels follow the usual oscilloscope definitions: Top / Base are the most frequent high / low levels
//! (histogram), else Max / Min; edges are crossings of the 50 % level with ±10 % hysteresis; rise and fall
//! times run from 10 % to 90 % of the amplitude.

use crate::runner::TranResult;
use serde::{Deserialize, Serialize};
use std::cell::OnceCell;

#[derive(Clone, Copy, PartialEq, Eq, Debug, Default, Serialize, Deserialize)]
pub enum Meas {
    #[default]
    None,
    Max,
    Min,
    Pp,
    Top,
    Base,
    Amp,
    Mean,
    Rms,
    AcRms,
    CycMean,
    CycRms,
    Crest,
    OverP,
    OverN,
    Area,
    Freq,
    Period,
    WidthP,
    WidthN,
    DutyP,
    DutyN,
    Rise,
    Fall,
}

#[derive(Clone, Copy, PartialEq, Eq, Debug)]
pub enum Unit {
    /// unit of the signal (unknown: shown without a unit)
    Sig,
    Time,
    Hz,
    Pct,
    /// plain number
    Ratio,
    /// signal × seconds
    SigTime,
}

impl Meas {
    /// amplitude measurements, then time measurements (menu order)
    pub const LEVEL: [Meas; 15] = [Meas::Max, Meas::Min, Meas::Pp, Meas::Top, Meas::Base, Meas::Amp, Meas::Mean, Meas::Rms, Meas::AcRms, Meas::CycMean, Meas::CycRms, Meas::Crest, Meas::OverP, Meas::OverN, Meas::Area];
    pub const TIME: [Meas; 8] = [Meas::Freq, Meas::Period, Meas::WidthP, Meas::WidthN, Meas::DutyP, Meas::DutyN, Meas::Rise, Meas::Fall];

    /// column heading
    pub fn head(self) -> &'static str {
        match self {
            Meas::None => "",
            Meas::Max => "Max",
            Meas::Min => "Min",
            Meas::Pp => "P-P",
            Meas::Top => "Top",
            Meas::Base => "Base",
            Meas::Amp => "振幅",
            Meas::Mean => "平均",
            Meas::Rms => "RMS",
            Meas::AcRms => "AC RMS",
            Meas::CycMean => "週期平均",
            Meas::CycRms => "週期 RMS",
            Meas::Crest => "峰值因數",
            Meas::OverP => "+過衝",
            Meas::OverN => "−過衝",
            Meas::Area => "∫dt",
            Meas::Freq => "頻率",
            Meas::Period => "週期",
            Meas::WidthP => "+脈寬",
            Meas::WidthN => "−脈寬",
            Meas::DutyP => "+工作週期",
            Meas::DutyN => "−工作週期",
            Meas::Rise => "上升時間",
            Meas::Fall => "下降時間",
        }
    }
    /// name in the settings list
    pub fn name(self) -> &'static str {
        match self {
            Meas::None => "（不顯示）",
            Meas::Max => "Max 最大值",
            Meas::Min => "Min 最小值",
            Meas::Pp => "P-P 峰對峰值",
            Meas::Top => "Top 高準位",
            Meas::Base => "Base 低準位",
            Meas::Amp => "Amplitude 振幅",
            Meas::Mean => "Mean 平均值",
            Meas::Rms => "RMS 均方根值",
            Meas::AcRms => "AC RMS 交流均方根值",
            Meas::CycMean => "Cycle Mean 週期平均",
            Meas::CycRms => "Cycle RMS 週期均方根",
            Meas::Crest => "Crest Factor 峰值因數",
            Meas::OverP => "+Overshoot 正過衝",
            Meas::OverN => "−Overshoot 負過衝",
            Meas::Area => "Area 面積（∫dt）",
            Meas::Freq => "Frequency 頻率",
            Meas::Period => "Period 週期",
            Meas::WidthP => "+Width 正脈寬",
            Meas::WidthN => "−Width 負脈寬",
            Meas::DutyP => "+Duty 正工作週期",
            Meas::DutyN => "−Duty 負工作週期",
            Meas::Rise => "Rise Time 上升時間",
            Meas::Fall => "Fall Time 下降時間",
        }
    }
    /// definition (hover text, manual)
    pub fn help(self) -> &'static str {
        match self {
            Meas::None => "不顯示這一欄。",
            Meas::Max => "量測範圍內的最大值。",
            Meas::Min => "量測範圍內的最小值。",
            Meas::Pp => "Max − Min。",
            Meas::Top => "高準位：數值分佈（直方圖）上半部出現最多的準位，例如方波的高電位；沒有明顯平頂（正弦、三角波）時等於 Max。",
            Meas::Base => "低準位：數值分佈下半部出現最多的準位；沒有明顯平底時等於 Min。",
            Meas::Amp => "Top − Base。",
            Meas::Mean => "時間平均：∫v dt ÷ 量測時間。",
            Meas::Rms => "均方根值：√(∫v² dt ÷ 量測時間)，含直流成分。",
            Meas::AcRms => "去除直流後的均方根值：√(RMS² − Mean²)。",
            Meas::CycMean => "整數個週期（第一個到最後一個上升緣之間）的平均值，不受範圍邊緣半個週期影響。",
            Meas::CycRms => "整數個週期（第一個到最後一個上升緣之間）的均方根值。",
            Meas::Crest => "峰值因數：max(|Max|, |Min|) ÷ RMS（正弦波為 1.414）。",
            Meas::OverP => "(Max − Top) ÷ 振幅 × 100 %。",
            Meas::OverN => "(Base − Min) ÷ 振幅 × 100 %。",
            Meas::Area => "∫v dt（訊號單位 × 秒）。",
            Meas::Freq => "1 ÷ 週期。",
            Meas::Period => "相鄰上升緣（通過 50 % 準位，±10 % 遲滯）的平均間隔；上升緣不足兩個時改用下降緣。",
            Meas::WidthP => "上升緣到下一個下降緣的平均時間（50 % 準位）。",
            Meas::WidthN => "下降緣到下一個上升緣的平均時間（50 % 準位）。",
            Meas::DutyP => "+脈寬 ÷ 週期 × 100 %。",
            Meas::DutyN => "−脈寬 ÷ 週期 × 100 %。",
            Meas::Rise => "第一個完整上升緣由振幅 10 % 升到 90 % 的時間。",
            Meas::Fall => "第一個完整下降緣由振幅 90 % 降到 10 % 的時間。",
        }
    }
    pub fn unit(self) -> Unit {
        match self {
            Meas::Freq => Unit::Hz,
            Meas::Period | Meas::WidthP | Meas::WidthN | Meas::Rise | Meas::Fall => Unit::Time,
            Meas::DutyP | Meas::DutyN | Meas::OverP | Meas::OverN => Unit::Pct,
            Meas::Crest => Unit::Ratio,
            Meas::Area => Unit::SigTime,
            _ => Unit::Sig,
        }
    }
    /// value as shown in the viewer ("—" when it cannot be measured)
    pub fn fmt(self, v: f64) -> String {
        if !v.is_finite() {
            return "—".into();
        }
        match self.unit() {
            Unit::Sig | Unit::SigTime => crate::util::si(v, "", 4),
            Unit::Time => crate::util::si(v, "s", 4),
            Unit::Hz => crate::util::si(v, "Hz", 4),
            Unit::Pct => format!("{:.2} %", v),
            Unit::Ratio => format!("{:.4}", v),
        }
    }
    /// heading with the unit, for spreadsheets ("頻率 (Hz)")
    pub fn head_unit(self) -> String {
        match self.unit() {
            Unit::Time => format!("{} (s)", self.head()),
            Unit::Hz => format!("{} (Hz)", self.head()),
            Unit::Pct => format!("{} (%)", self.head()),
            _ => self.head().to_string(),
        }
    }
}

/// one crossing of the 50 % level
#[derive(Clone, Copy, Debug)]
struct Edge {
    t: f64,
    rising: bool,
}

/// measurements of one signal over [a, b]
pub struct Meter<'a> {
    d: &'a [f64],
    t0: f64,
    dt: f64,
    a: f64,
    b: f64,
    pub max: f64,
    pub min: f64,
    pub mean: f64,
    pub rms: f64,
    pub area: f64,
    top_base: OnceCell<(f64, f64)>,
    edges: OnceCell<Vec<Edge>>,
    res: &'a TranResult,
    ci: usize,
}

impl<'a> Meter<'a> {
    /// None for an empty signal
    pub fn new(r: &'a TranResult, ci: usize, a: f64, b: f64) -> Option<Self> {
        let d = r.data.get(ci)?;
        let n = d.len().min(r.n);
        if n == 0 || !(r.dt > 0.0) || !a.is_finite() || !b.is_finite() {
            return None;
        }
        let d = &d[..n];
        let tend = r.t0 + (n - 1) as f64 * r.dt;
        let (lo, hi) = (a.min(b), a.max(b));
        let a = lo.max(r.t0).min(tend);
        let b = hi.min(tend).max(a);
        let mut m = Meter { d, t0: r.t0, dt: r.dt, a, b, max: f64::NAN, min: f64::NAN, mean: f64::NAN, rms: f64::NAN, area: 0.0, top_base: OnceCell::new(), edges: OnceCell::new(), res: r, ci };
        let (mut mx, mut mn) = (f64::NEG_INFINITY, f64::INFINITY);
        let (mut s1, mut s2, mut tt) = (0.0, 0.0, 0.0);
        let mut prev: Option<(f64, f64)> = None;
        for (t, v) in m.pts() {
            if v.is_finite() {
                mx = mx.max(v);
                mn = mn.min(v);
            }
            if let Some((tp, vp)) = prev {
                let h = t - tp;
                if vp.is_finite() && v.is_finite() && h > 0.0 {
                    s1 += (vp + v) / 2.0 * h;
                    s2 += (vp * vp + v * v) / 2.0 * h;
                    tt += h;
                }
            }
            prev = Some((t, v));
        }
        if mx >= mn {
            m.max = mx;
            m.min = mn;
        }
        m.area = s1;
        if tt > 0.0 {
            m.mean = s1 / tt;
            m.rms = (s2 / tt).sqrt();
        } else if let Some((_, v)) = prev {
            // a single instant
            m.mean = v;
            m.rms = v.abs();
        }
        Some(m)
    }

    fn at(&self, t: f64) -> f64 {
        let n = self.d.len();
        let f = ((t - self.t0) / self.dt).clamp(0.0, (n - 1) as f64);
        let i = (f.floor() as usize).min(n - 1);
        let j = (i + 1).min(n - 1);
        self.d[i] + (self.d[j] - self.d[i]) * (f - i as f64)
    }

    /// the polyline over [a, b]: interpolated ends and the samples strictly between them
    fn pts(&self) -> impl Iterator<Item = (f64, f64)> + '_ {
        let n = self.d.len();
        let eps = self.dt * 1e-9;
        let first = ((self.a + eps - self.t0) / self.dt).floor() as i64 + 1;
        let last = ((self.b - eps - self.t0) / self.dt).ceil() as i64 - 1;
        let first = first.max(0) as usize;
        let last = last.min(n as i64 - 1);
        let mid: Box<dyn Iterator<Item = usize>> = if last >= first as i64 { Box::new(first..=last as usize) } else { Box::new(std::iter::empty()) };
        let end = if self.b > self.a { Some((self.b, self.at(self.b))) } else { None };
        std::iter::once((self.a, self.at(self.a))).chain(mid.map(move |i| (self.t0 + i as f64 * self.dt, self.d[i]))).chain(end)
    }

    /// (Top, Base): the most frequent level of the upper / lower half of the histogram, else Max / Min
    pub fn top_base(&self) -> (f64, f64) {
        *self.top_base.get_or_init(|| {
            let (mx, mn) = (self.max, self.min);
            if !(mx > mn) {
                return (mx, mn);
            }
            const NB: usize = 256;
            let mut cnt = [0u32; NB];
            let mut sum = [0f64; NB];
            let w = (mx - mn) / NB as f64;
            for (_, v) in self.pts() {
                if v.is_finite() {
                    let k = (((v - mn) / w) as usize).min(NB - 1);
                    cnt[k] += 1;
                    sum[k] += v;
                }
            }
            // a clear mode: at least twice the average count of that half
            let pick = |r: std::ops::Range<usize>, fallback: f64| {
                let len = r.len() as f64;
                let tot: u32 = cnt[r.clone()].iter().sum();
                let (k, c) = r.map(|k| (k, cnt[k])).fold((0, 0), |b, x| if x.1 > b.1 { x } else { b });
                if c > 0 && c as f64 >= 2.0 * (tot as f64 / len).max(1.0) {
                    sum[k] / c as f64
                } else {
                    fallback
                }
            };
            (pick(NB / 2..NB, mx), pick(0..NB / 2, mn))
        })
    }

    pub fn amp(&self) -> f64 {
        let (t, b) = self.top_base();
        t - b
    }

    /// crossings of the 50 % level with ±10 % hysteresis
    fn edges(&self) -> &Vec<Edge> {
        self.edges.get_or_init(|| {
            let (top, base) = self.top_base();
            let amp = top - base;
            let mut out = vec![];
            if !(amp > 0.0) {
                return out;
            }
            let mid = (top + base) / 2.0;
            let (hi, lo) = (mid + 0.1 * amp, mid - 0.1 * amp);
            let mut state = 0i8;
            let (mut up, mut dn): (Option<f64>, Option<f64>) = (None, None);
            let mut prev: Option<(f64, f64)> = None;
            for (t, v) in self.pts() {
                if !v.is_finite() {
                    prev = None;
                    continue;
                }
                if let Some((tp, vp)) = prev {
                    if vp <= mid && v > mid {
                        up = Some(tp + (t - tp) * (mid - vp) / (v - vp));
                    }
                    if vp >= mid && v < mid {
                        dn = Some(tp + (t - tp) * (vp - mid) / (vp - v));
                    }
                }
                if v > hi {
                    if state == -1 {
                        if let Some(tc) = up {
                            out.push(Edge { t: tc, rising: true });
                        }
                    }
                    state = 1;
                } else if v < lo {
                    if state == 1 {
                        if let Some(tc) = dn {
                            out.push(Edge { t: tc, rising: false });
                        }
                    }
                    state = -1;
                }
                prev = Some((t, v));
            }
            out
        })
    }

    pub fn period(&self) -> f64 {
        let e = self.edges();
        let pick = |rising: bool| {
            let t: Vec<f64> = e.iter().filter(|x| x.rising == rising).map(|x| x.t).collect();
            if t.len() >= 2 {
                Some((t[t.len() - 1] - t[0]) / (t.len() - 1) as f64)
            } else {
                None
            }
        };
        pick(true).or_else(|| pick(false)).unwrap_or(f64::NAN)
    }

    /// mean time from an edge of one direction to the next edge (of the other direction)
    fn width(&self, from_rising: bool) -> f64 {
        let e = self.edges();
        let w: Vec<f64> = e.windows(2).filter(|p| p[0].rising == from_rising && p[1].rising != from_rising).map(|p| p[1].t - p[0].t).collect();
        if w.is_empty() {
            f64::NAN
        } else {
            w.iter().sum::<f64>() / w.len() as f64
        }
    }

    /// 10 % → 90 % (rise) or 90 % → 10 % (fall) on the first complete edge
    fn transition(&self, rise: bool) -> f64 {
        let (top, base) = self.top_base();
        let amp = top - base;
        if !(amp > 0.0) {
            return f64::NAN;
        }
        // work on a rising edge: for a fall, mirror the signal
        let s = if rise { 1.0 } else { -1.0 };
        let (l1, l2) = if rise { (base + 0.1 * amp, base + 0.9 * amp) } else { (-(top - 0.1 * amp), -(base + 0.1 * amp)) };
        let cross = |tp: f64, vp: f64, t: f64, v: f64, l: f64| tp + (t - tp) * (l - vp) / (v - vp);
        let mut below = false;
        let mut t1: Option<f64> = None;
        let mut prev: Option<(f64, f64)> = None;
        for (t, v) in self.pts() {
            let v = v * s;
            if !v.is_finite() {
                prev = None;
                continue;
            }
            if let Some((tp, vp)) = prev {
                if vp < l1 {
                    below = true;
                }
                if below && vp < l1 && v >= l1 {
                    t1 = Some(cross(tp, vp, t, v, l1));
                }
                if let Some(a) = t1 {
                    if vp < l2 && v >= l2 {
                        return cross(tp, vp, t, v, l2) - a;
                    }
                }
            } else if v < l1 {
                below = true;
            }
            if v < l1 {
                t1 = None;
                below = true;
            }
            prev = Some((t, v));
        }
        f64::NAN
    }

    /// mean / RMS over whole periods (first to last rising edge)
    fn cycle(&self) -> Option<Meter<'a>> {
        let r: Vec<f64> = self.edges().iter().filter(|e| e.rising).map(|e| e.t).collect();
        if r.len() < 2 || !(r[r.len() - 1] > r[0]) {
            return None;
        }
        Meter::new(self.res, self.ci, r[0], r[r.len() - 1])
    }

    pub fn value(&self, m: Meas) -> f64 {
        match m {
            Meas::None => f64::NAN,
            Meas::Max => self.max,
            Meas::Min => self.min,
            Meas::Pp => self.max - self.min,
            Meas::Top => self.top_base().0,
            Meas::Base => self.top_base().1,
            Meas::Amp => self.amp(),
            Meas::Mean => self.mean,
            Meas::Rms => self.rms,
            Meas::AcRms => (self.rms * self.rms - self.mean * self.mean).max(0.0).sqrt(),
            Meas::CycMean => self.cycle().map(|c| c.mean).unwrap_or(f64::NAN),
            Meas::CycRms => self.cycle().map(|c| c.rms).unwrap_or(f64::NAN),
            Meas::Crest => {
                let pk = self.max.abs().max(self.min.abs());
                if self.rms > 0.0 {
                    pk / self.rms
                } else {
                    f64::NAN
                }
            }
            Meas::OverP | Meas::OverN => {
                let (top, base) = self.top_base();
                let amp = top - base;
                if !(amp > 0.0) {
                    return f64::NAN;
                }
                if m == Meas::OverP {
                    (self.max - top) / amp * 100.0
                } else {
                    (base - self.min) / amp * 100.0
                }
            }
            Meas::Area => self.area,
            Meas::Period => self.period(),
            Meas::Freq => 1.0 / self.period(),
            Meas::WidthP => self.width(true),
            Meas::WidthN => self.width(false),
            Meas::DutyP => self.width(true) / self.period() * 100.0,
            Meas::DutyN => self.width(false) / self.period() * 100.0,
            Meas::Rise => self.transition(true),
            Meas::Fall => self.transition(false),
        }
    }
}

/// the chosen measurements of one signal (NaN where not measurable)
pub fn measure(r: &TranResult, ci: usize, a: f64, b: f64, ms: &[Meas]) -> Vec<f64> {
    match Meter::new(r, ci, a, b) {
        Some(m) => ms.iter().map(|&k| m.value(k)).collect(),
        None => vec![f64::NAN; ms.len()],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn res(dt: f64, n: usize, f: impl Fn(f64) -> f64) -> TranResult {
        let mut r = TranResult::default();
        r.t0 = 0.0;
        r.dt = dt;
        r.n = n;
        r.names = vec!["v".into()];
        r.data = vec![(0..n).map(|i| f(i as f64 * dt)).collect()];
        r
    }
    fn near(a: f64, b: f64, tol: f64) -> bool {
        (a - b).abs() <= tol * b.abs().max(1e-300)
    }

    #[test]
    fn sine() {
        // 2 V peak, 1 V offset, 1 kHz, 10 periods, 1000 samples per period
        let r = res(1e-6, 10001, |t| 1.0 + 2.0 * (2.0 * std::f64::consts::PI * 1e3 * t).sin());
        let m = Meter::new(&r, 0, 0.0, 0.01).unwrap();
        assert!(near(m.value(Meas::Mean), 1.0, 1e-6), "mean {}", m.value(Meas::Mean));
        assert!(near(m.value(Meas::AcRms), 2.0 / 2f64.sqrt(), 1e-5), "acrms {}", m.value(Meas::AcRms));
        assert!(near(m.value(Meas::Rms), (1.0f64 + 2.0).sqrt(), 1e-5), "rms {}", m.value(Meas::Rms));
        assert!(near(m.value(Meas::Freq), 1e3, 1e-6), "freq {}", m.value(Meas::Freq));
        assert!(near(m.value(Meas::Pp), 4.0, 1e-6));
        // no flat top: Top = Max within the top histogram bin
        assert!(near(m.value(Meas::Top), 3.0, 0.01), "top {}", m.value(Meas::Top));
        assert!(near(m.value(Meas::DutyP), 50.0, 1e-4), "duty {}", m.value(Meas::DutyP));
        // a range that is not a whole number of periods: cycle mean still exact
        let m2 = Meter::new(&r, 0, 0.00013, 0.00977).unwrap();
        assert!(near(m2.value(Meas::CycMean), 1.0, 1e-5), "cycle mean {}", m2.value(Meas::CycMean));
        assert!(!near(m2.value(Meas::Mean), 1.0, 1e-3), "plain mean should differ");
        assert!(near(m.value(Meas::Crest), 3.0 / 3f64.sqrt(), 1e-5));
    }

    #[test]
    fn pulse() {
        // 0 / 5 V pulse train, 100 µs period, 30 % duty, 2 µs linear edges, 0.1 µs samples
        let (per, ton, tr) = (100e-6, 30e-6, 2e-6);
        let f = move |t: f64| {
            let u = t % per;
            if u < tr {
                5.0 * u / tr
            } else if u < ton {
                5.0
            } else if u < ton + tr {
                5.0 * (1.0 - (u - ton) / tr)
            } else {
                0.0
            }
        };
        let r = res(1e-7, 10001, f);
        let m = Meter::new(&r, 0, 0.0, 1e-3).unwrap();
        assert!(near(m.value(Meas::Top), 5.0, 1e-9) && m.value(Meas::Base).abs() < 1e-9, "levels {:?}", m.top_base());
        assert!(near(m.value(Meas::Period), per, 1e-6), "period {}", m.value(Meas::Period));
        // 50 % crossings at tr/2 and ton + tr/2: width = ton
        assert!(near(m.value(Meas::WidthP), ton, 1e-6), "width {}", m.value(Meas::WidthP));
        assert!(near(m.value(Meas::DutyP), 30.0, 1e-5), "duty {}", m.value(Meas::DutyP));
        assert!(near(m.value(Meas::DutyN), 70.0, 1e-5));
        assert!(near(m.value(Meas::Rise), 0.8 * tr, 1e-6), "rise {}", m.value(Meas::Rise));
        assert!(near(m.value(Meas::Fall), 0.8 * tr, 1e-6), "fall {}", m.value(Meas::Fall));
        assert!(m.value(Meas::OverP).abs() < 1e-9);
        // mean of the trapezoidal pulse: 5·(ton)/per (edges are symmetric)
        assert!(near(m.value(Meas::Mean), 5.0 * ton / per, 1e-6), "mean {}", m.value(Meas::Mean));
    }

    #[test]
    fn exact_range_and_degenerate() {
        // ramp v = t: mean over [a, b] is (a + b) / 2 even between samples
        let r = res(1.0, 11, |t| t);
        let m = Meter::new(&r, 0, 2.25, 7.5).unwrap();
        assert!(near(m.mean, (2.25 + 7.5) / 2.0, 1e-12), "mean {}", m.mean);
        assert!(near(m.max, 7.5, 1e-12) && near(m.min, 2.25, 1e-12));
        // a constant: no edges, no frequency, levels = the value
        let c = res(1.0, 11, |_| 3.0);
        let mc = Meter::new(&c, 0, 0.0, 10.0).unwrap();
        assert!(mc.value(Meas::Freq).is_nan() && mc.value(Meas::Rise).is_nan());
        assert_eq!(mc.value(Meas::Top), 3.0);
        // a single instant
        let m1 = Meter::new(&r, 0, 4.0, 4.0).unwrap();
        assert_eq!(m1.mean, 4.0);
        // a triangle has no flat top: Top = Max
        let tri = res(1e-3, 4001, |t| { let u = t % 1.0; if u < 0.5 { 2.0 * u } else { 2.0 - 2.0 * u } });
        let mt = Meter::new(&tri, 0, 0.0, 4.0).unwrap();
        assert!(near(mt.value(Meas::Top), 1.0, 1e-9), "triangle top {}", mt.value(Meas::Top));
        assert!(near(mt.value(Meas::Freq), 1.0, 1e-6));
    }
}
