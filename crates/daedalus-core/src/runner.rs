//! Analysis runners: transient, AC sweep by small-signal injection (time domain), linear AC.

use crate::doc::Doc;
use crate::expr::{self, Syms};
use crate::netlist::{NlError, Netlist};
use crate::sim::*;
use crate::sparse::Clu;
use crate::util::js_to_precision;
use std::sync::atomic::{AtomicBool, AtomicUsize, Ordering};
use std::sync::Mutex;

/* ------------------------------------------------------------------ simulation control */
#[derive(Clone, Debug)]
pub struct SimCtrl {
    pub dt: f64,
    pub t_total: f64,
    pub t_print: f64,
    pub print_step: u64,
    pub interp: bool,
    pub damp: bool,
    pub lte: bool,
    pub lte_tol: f64,
    /// high-accuracy mode: automatic step from a global error target (see accuracy.rs)
    pub acc: bool,
    /// target relative error of the high-accuracy mode
    pub etol: f64,
    /// maximum number of step halvings in the high-accuracy mode
    pub max_lv: u32,
}
impl SimCtrl {
    pub fn opts(&self) -> SimOpts {
        let mut o = SimOpts::new(self.dt);
        o.interp = self.interp;
        o.damp = self.damp;
        o.lte = self.lte;
        o.lte_tol = if self.lte_tol > 0.0 { self.lte_tol } else { 1e-3 };
        o.opt_t_end = self.t_total;
        o
    }
}

pub fn eval_sim_ctrl(doc: &Doc, syms: &Syms) -> Result<SimCtrl, NlError> {
    let el = doc.elements.iter().find(|e| e.typ == "SIMCTRL" && !e.dis).ok_or_else(|| NlError { msg: "電路中沒有 Simulation Control 元件，請由「模擬 → 模擬控制」(F7) 設定時間步長與總時間。".into(), els: vec![] })?;
    let g = |k: &str| -> Result<f64, NlError> {
        let v = el.params.get(k).cloned().unwrap_or_default();
        expr::eval_const(&v, syms).map_err(|e| NlError { msg: format!("Simulation Control：{} {}", k, e.0), els: vec![el.id] })
    };
    let p = |k: &str| el.params.get(k).cloned();
    let sc = SimCtrl {
        dt: g("dt")?,
        t_total: g("tt")?,
        t_print: g("tp")?,
        print_step: expr::js_round(g("ps")?).max(1.0) as u64,
        interp: p("interp").as_deref() != Some("0"),
        damp: p("damp").as_deref() != Some("0"),
        lte: p("lte").filter(|s| !s.is_empty()).as_deref() != Some("0"),
        lte_tol: if p("ltetol").map(|s| !s.is_empty()).unwrap_or(false) { g("ltetol")? } else { 1e-3 },
        acc: p("acc").as_deref().map(|s| s.trim() == "1").unwrap_or(false),
        etol: if p("etol").map(|s| !s.is_empty()).unwrap_or(false) { g("etol")? } else { 1e-6 },
        max_lv: if p("maxlv").map(|s| !s.is_empty()).unwrap_or(false) { expr::js_round(g("maxlv")?).clamp(1.0, 20.0) as u32 } else { 8 },
    };
    if sc.acc && !(sc.etol > 0.0 && sc.etol < 1.0) {
        return Err(NlError { msg: "Simulation Control：高精度模式的目標誤差必須介於 0 與 1 之間（例如 1e-6）".into(), els: vec![el.id] });
    }
    if !(sc.dt > 0.0) || !(sc.t_total > 0.0) {
        return Err(NlError { msg: "Simulation Control：時間步長與總時間必須大於 0".into(), els: vec![el.id] });
    }
    if sc.dt < 1e-15 * 0.9999 {
        return Err(NlError { msg: "Simulation Control：時間步長最小為 1 fs".into(), els: vec![el.id] });
    }
    Ok(sc)
}

#[derive(Clone, Debug)]
pub struct AcSpec {
    pub f_start: f64,
    pub f_end: f64,
    pub n_pts: f64,
    pub flag: i64,
    pub src: String,
    pub a_start: f64,
    pub a_end: f64,
    pub extra: Vec<f64>,
    pub method: i64,
    pub settle: f64,
    pub mcyc: f64,
    pub max_cyc: f64,
    pub tol: f64,
    pub t_pre: f64,
    pub t_settle: f64,
    pub baseline: bool,
}
fn js_plus_int(s: Option<&String>) -> i64 {
    s.and_then(|v| v.trim().parse::<f64>().ok()).filter(|v| !v.is_nan()).map(|v| v as i64).unwrap_or(0)
}
pub fn eval_ac(doc: &Doc, syms: &Syms) -> Result<Option<AcSpec>, NlError> {
    let el = match doc.elements.iter().find(|e| e.typ == "ACSWEEP" && !e.dis) {
        Some(e) => e,
        None => return Ok(None),
    };
    let g = |k: &str| -> Result<f64, NlError> {
        let v = el.params.get(k).cloned().unwrap_or_default();
        expr::eval_const(&v, syms).map_err(|e| NlError { msg: format!("AC Sweep：{} {}", k, e.0), els: vec![el.id] })
    };
    let extra = expr::eval_list(el.params.get("extra").map(|s| s.as_str()).unwrap_or(""), syms).map_err(|_| NlError { msg: "AC Sweep：額外頻率點格式錯誤".into(), els: vec![el.id] })?;
    let ac = AcSpec {
        f_start: g("f0")?,
        f_end: g("f1")?,
        n_pts: expr::js_round(g("n")?),
        flag: js_plus_int(el.params.get("flag")),
        src: el.params.get("src").cloned().unwrap_or_default().trim().to_string(),
        a_start: g("a0")?,
        a_end: g("a1")?,
        extra,
        method: js_plus_int(el.params.get("method")),
        settle: g("settle")?.max(1.0),
        mcyc: expr::js_round(g("mcyc")?).max(1.0),
        max_cyc: g("maxc")?.max(4.0),
        tol: (g("tol")? / 100.0).max(1e-7),
        t_pre: g("tpre")?,
        t_settle: g("tset")?,
        baseline: el.params.get("base").map(|s| s.trim().parse::<f64>().ok() != Some(0.0)).unwrap_or(true),
    };
    if ac.f_end > 1e6 * 1.000001 {
        return Err(NlError { msg: "AC Sweep：結束頻率上限為 1 MHz".into(), els: vec![el.id] });
    }
    if !(ac.f_start > 0.0) || !(ac.f_end > ac.f_start) {
        return Err(NlError { msg: "AC Sweep：頻率範圍設定錯誤".into(), els: vec![el.id] });
    }
    if ac.src.is_empty() {
        return Err(NlError { msg: "AC Sweep：請輸入激勵源名稱".into(), els: vec![el.id] });
    }
    Ok(Some(ac))
}

/* ------------------------------------------------------------------ transient */
#[derive(Clone, Debug, Default)]
pub struct Stats {
    pub steps: u64,
    pub refined: u64,
    pub refine_max: u64,
    pub evt_refined: u64,
    pub non_conv: u64,
    pub lte: bool,
}
#[derive(Clone, Debug, Default)]
pub struct TranResult {
    pub t0: f64,
    pub dt: f64,
    pub n: usize,
    pub names: Vec<String>,
    pub data: Vec<Vec<f64>>,
    pub warnings: Vec<String>,
    pub events: u64,
    pub stats: Stats,
    /// set for a parameter sweep: columns are probe-major, `labels.len()` runs per probe
    pub sweep: Option<SweepInfo>,
    /// set by the high-accuracy mode
    pub accuracy: Option<crate::accuracy::AccInfo>,
    /// set by the Monte Carlo analysis
    pub mc: Option<crate::montecarlo::McInfo>,
}
#[derive(Clone, Debug, Default)]
pub struct SweepInfo {
    pub var: String,
    pub labels: Vec<String>,
    pub probes: usize,
    pub threads: usize,
}

pub struct TransientRunner {
    pub sim: Sim,
    n_steps: u64,
    ps: u64,
    n_print0: u64,
    n_rec: usize,
    data: Vec<Vec<f64>>,
    names: Vec<String>,
    rec: usize,
    n: u64,
    started: bool,
    finished: bool,
}
impl TransientRunner {
    pub fn new(net: &Netlist, sc: &SimCtrl) -> R<Self> {
        let (dt, tt) = (sc.dt, sc.t_total);
        if !(dt > 0.0) {
            return Err(SimError::new("時間步長必須大於 0"));
        }
        if !(tt > 0.0) {
            return Err(SimError::new("總時間必須大於 0"));
        }
        let sim = Sim::new(net, sc.opts())?;
        let mut n_steps = (tt / dt).round().max(1.0) as u64;
        if (n_steps as f64 * dt - tt).abs() > dt * 1e-6 {
            n_steps = (tt / dt - 1e-9).ceil() as u64;
        }
        let ps = sc.print_step.max(1);
        let n_print0 = ((sc.t_print / dt - 1e-9).ceil()).max(0.0) as u64;
        let n_rec = if n_steps >= n_print0 { ((n_steps - n_print0) / ps + 1) as usize } else { 0 };
        if sim.probes.is_empty() {
            return Err(SimError::new("電路中沒有任何量測探棒（Probe），請放置電壓/電流探棒以觀察波形。"));
        }
        let bytes = n_rec as f64 * sim.probes.len() as f64 * 8.0;
        if bytes > 1.2e9 {
            return Err(SimError::new(format!("輸出資料量過大（約 {:.0} MB）。請加大 Print step 或延後 Print time。", bytes / 1e6)));
        }
        let names = sim.probes.iter().map(|&i| sim.insts[i].pname()).collect();
        let data = sim.probes.iter().map(|_| Vec::with_capacity(n_rec)).collect();
        Ok(TransientRunner { sim, n_steps, ps, n_print0, n_rec, data, names, rec: 0, n: 0, started: false, finished: false })
    }
    fn record(&mut self) {
        if self.n >= self.n_print0 && (self.n - self.n_print0) % self.ps == 0 && self.rec < self.n_rec {
            for (j, &i) in self.sim.probes.iter().enumerate() {
                let v = self.sim.probe_value(i);
                self.data[j].push(v);
            }
            self.rec += 1;
        }
    }
    /// advance up to `max_steps` time steps; returns true when done
    pub fn run_steps(&mut self, max_steps: u64) -> R<bool> {
        if !self.started {
            self.sim.init()?;
            self.started = true;
            self.record();
        }
        let dt = self.sim.o.dt;
        let mut k = 0;
        while self.n < self.n_steps {
            let n = self.n + 1;
            self.sim.advance_to(n as f64 * dt)?;
            self.n = n;
            self.record();
            k += 1;
            if k >= max_steps {
                break;
            }
        }
        if self.n >= self.n_steps && !self.finished {
            self.finished = true;
            self.sim.finish();
        }
        Ok(self.finished)
    }
    pub fn progress(&self) -> f64 {
        self.n as f64 / self.n_steps as f64
    }
    pub fn t(&self) -> f64 {
        self.sim.t
    }
    pub fn n_steps(&self) -> u64 {
        self.n_steps
    }
    pub fn names(&self) -> &[String] {
        &self.names
    }
    /// number of output samples the run will record
    pub fn n_rec(&self) -> usize {
        self.n_rec
    }
    pub fn recorded(&self) -> usize {
        self.rec
    }
    /// recorded samples of probe j so far
    pub fn data(&self, j: usize) -> &[f64] {
        &self.data[j]
    }
    pub fn t0(&self) -> f64 {
        self.n_print0 as f64 * self.sim.o.dt
    }
    pub fn dt_out(&self) -> f64 {
        self.sim.o.dt * self.ps as f64
    }
    pub fn result(self) -> TranResult {
        let s = &self.sim;
        TranResult {
            t0: self.n_print0 as f64 * s.o.dt,
            dt: s.o.dt * self.ps as f64,
            n: self.rec,
            names: self.names,
            data: self.data,
            warnings: s.e.warnings.clone(),
            events: s.events,
            stats: Stats { steps: s.steps, refined: s.refined, refine_max: s.refine_max, evt_refined: s.evt_refined, non_conv: s.non_conv, lte: s.o.lte },
            sweep: None,
            accuracy: None,
            mc: None,
        }
    }
}

/* ------------------------------------------------------------------ AC sweep */
#[derive(Clone, Debug, Default)]
pub struct AcCurve {
    pub name: String,
    pub mag: Vec<f64>,
    pub ph: Vec<f64>,
    pub re: Vec<f64>,
    pub im: Vec<f64>,
    /// loop-gain curve (ACP2 with polarity 1): the Bode window reports crossover, phase and gain margin
    pub lp: bool,
}
#[derive(Clone, Debug, Default)]
pub struct AcResult {
    pub freqs: Vec<f64>,
    pub curves: Vec<AcCurve>,
    pub warnings: Vec<String>,
}

pub fn sweep_freqs(ac: &AcSpec) -> R<Vec<f64>> {
    let (f0, f1) = (ac.f_start, ac.f_end);
    let n = ac.n_pts.max(1.0) as usize;
    if !(f0 > 0.0) || !(f1 > f0) {
        return Err(SimError::new("AC 掃頻：結束頻率必須大於起始頻率，且起始頻率需大於 0"));
    }
    let mut fs = Vec::new();
    for i in 0..n {
        let r = if n == 1 { 0.0 } else { i as f64 / (n - 1) as f64 };
        fs.push(if ac.flag == 1 { f0 + (f1 - f0) * r } else { f0 * (f1 / f0).powf(r) });
    }
    for &f in &ac.extra {
        if f > 0.0 {
            fs.push(f);
        }
    }
    fs.sort_by(|a, b| a.partial_cmp(b).unwrap_or(std::cmp::Ordering::Equal));
    let mut out: Vec<f64> = Vec::new();
    for (i, &f) in fs.iter().enumerate() {
        if i == 0 || (f - fs[i - 1]).abs() > 1e-9 * f {
            out.push(f);
        }
    }
    Ok(out)
}
fn amp_at(ac: &AcSpec, f: f64) -> f64 {
    let (a0, a1) = (ac.a_start, ac.a_end);
    if ac.f_end <= ac.f_start || a0 == a1 {
        return a0;
    }
    let r = if ac.flag == 1 { (f - ac.f_start) / (ac.f_end - ac.f_start) } else { (f / ac.f_start).ln() / (ac.f_end / ac.f_start).ln() };
    let r = r.clamp(0.0, 1.0);
    if ac.flag != 1 && a0 > 0.0 && a1 > 0.0 {
        return a0 * (a1 / a0).powf(r);
    }
    a0 + (a1 - a0) * r
}
pub fn fmt_f(f: f64) -> String {
    if f >= 1e6 {
        format!("{} MHz", js_to_precision(f / 1e6, 4))
    } else if f >= 1e3 {
        format!("{} kHz", js_to_precision(f / 1e3, 4))
    } else {
        format!("{} Hz", js_to_precision(f, 4))
    }
}
fn make_bode(freqs: Vec<f64>, res: Vec<(String, Vec<f64>, Vec<f64>, bool)>, warnings: Vec<String>) -> AcResult {
    let curves = res
        .into_iter()
        .map(|(name, re, im, lp)| {
            let mut mag = Vec::new();
            let mut ph = Vec::new();
            let mut prev: Option<f64> = None;
            for i in 0..re.len() {
                mag.push(20.0 * re[i].hypot(im[i]).log10());
                let mut p = im[i].atan2(re[i]) * 180.0 / std::f64::consts::PI;
                if let Some(pv) = prev {
                    while p - pv > 180.0 {
                        p -= 360.0;
                    }
                    while p - pv < -180.0 {
                        p += 360.0;
                    }
                }
                ph.push(p);
                prev = Some(p);
            }
            // keep the phase near the principal range at the first point
            if !ph.is_empty() {
                let k = expr::js_round(ph[0] / 360.0);
                if k != 0.0 {
                    for p in ph.iter_mut() {
                        *p -= 360.0 * k;
                    }
                }
            }
            AcCurve { name, mag, ph, re, im, lp }
        })
        .collect();
    AcResult { freqs, curves, warnings }
}

struct Win {
    ts: f64,
    te: f64,
    tm: f64,
    sr: Vec<f64>,
    si: Vec<f64>,
    s1: Vec<f64>,
    gr: f64,
    gi: f64,
}
fn probe_vals(a: &Sim, b: Option<&Sim>) -> Vec<f64> {
    let mut out = Vec::with_capacity(a.ac_probes.len() * 2);
    for (k, &i) in a.ac_probes.iter().enumerate() {
        let (mut x, mut y) = a.ac_vals(i);
        if let Some(b) = b {
            let (bx, by) = b.ac_vals(b.ac_probes[k]);
            x -= bx;
            y -= by;
        }
        out.push(x);
        out.push(y);
    }
    out
}

/// One frequency of the injection sweep starting from the steady-state snapshot.
fn ac_one_freq(snap: &Sim, src: usize, ac: &AcSpec, sc: &SimCtrl, f: f64, cancel: &AtomicBool) -> R<(Vec<(f64, f64)>, Vec<String>, Vec<String>)> {
    let dt = sc.dt;
    let mut warn = Vec::new();
    if f * dt > 0.05 {
        warn.push(format!("頻率 {} 的每週期取樣點不足 20 點，結果可能不準確，請縮小時間步長。", fmt_f(f)));
    }
    let mut a = snap.clone();
    a.after_restore();
    let mut b = if ac.baseline {
        let mut b = snap.clone();
        b.after_restore();
        Some(b)
    } else {
        None
    };
    let amp = amp_at(ac, f);
    let t0 = a.t;
    let tp = 1.0 / f;
    let settle = (ac.settle * tp).max(if ac.t_settle > 0.0 { ac.t_settle } else { 0.0 });
    let tr = (2.0 * tp).min(0.5 * settle);
    a.insts[src].set_inj(Inj { a: amp, f, t0, tr });
    if let Some(b) = &mut b {
        b.insts[src].set_inj(Inj { a: 0.0, f, t0, tr });
    }
    let w = 2.0 * std::f64::consts::PI * f;
    let nm = ac.mcyc.max(1.0);
    let mut max_cyc = ac.max_cyc.max(ac.settle + 2.0 * ac.mcyc);
    max_cyc = max_cyc.max((settle / tp).ceil() + 2.0 * nm);
    let np = a.ac_probes.len();
    let new_win = |ts: f64| -> Win {
        let te = ts + nm * tp;
        Win { ts, te, tm: 0.5 * (ts + te), sr: vec![0.0; np * 2], si: vec![0.0; np * 2], s1: vec![0.0; np * 2], gr: 0.0, gi: 0.0 }
    };
    let mut win = new_win(t0 + settle);
    let mut prev_y = probe_vals(&a, b.as_ref());
    let mut prev_t = t0;
    let mut last: Option<Vec<(f64, f64)>> = None;
    let loops: Vec<bool> = a.ac_probes.iter().map(|&i| a.insts[i].ac_loop()).collect();
    let negs: Vec<bool> = a.ac_probes.iter().map(|&i| a.insts[i].ac_neg()).collect();
    let mut steps = 0u64;
    loop {
        steps += 1;
        if steps % 256 == 0 && cancel.load(Ordering::Relaxed) {
            return Err(SimError::new("已中止"));
        }
        let tn = a.t + dt;
        let tg = (tn / dt).round() * dt;
        a.advance_to(tg)?;
        if let Some(b) = &mut b {
            b.advance_to(tg)?;
        }
        let y = probe_vals(&a, b.as_ref());
        // integrate segment [prev_t, tg] (linear interpolation of y) of y*exp(-jw(t-t0)) into the window
        {
            let (ta, tb) = (prev_t, tg);
            let lo = ta.max(win.ts);
            let hi = tb.min(win.te);
            if hi > lo {
                let l = tb - ta;
                let pa = w * (lo - t0);
                let pb = w * (hi - t0);
                let (ca, sa, cb, sb) = (pa.cos(), -pa.sin(), pb.cos(), -pb.sin());
                let fa = (lo - ta) / l;
                let fb = (hi - ta) / l;
                let dh = (hi - lo) / 2.0;
                let ga = lo - win.tm;
                let gb = hi - win.tm;
                win.gr += dh * (ga * ca + gb * cb);
                win.gi += dh * (ga * sa + gb * sb);
                for i in 0..y.len() {
                    let y1 = prev_y[i] + (y[i] - prev_y[i]) * fa;
                    let y2 = prev_y[i] + (y[i] - prev_y[i]) * fb;
                    win.sr[i] += dh * (y1 * ca + y2 * cb);
                    win.si[i] += dh * (y1 * sa + y2 * sb);
                    win.s1[i] += dh * (y1 * ga + y2 * gb);
                }
            }
        }
        prev_t = tg;
        prev_y = y;
        let cyc = (tg - t0) / tp;
        if tg >= win.te - 1e-12 * tp {
            // finish window: remove a linear trend, then the Fourier coefficient
            let tw = win.te - win.ts;
            let scale = 2.0 / tw;
            let q = tw * tw * tw / 12.0;
            let den = q - (2.0 / tw) * (win.gr * win.gr + win.gi * win.gi);
            for i in 0..win.sr.len() {
                let bb = if den > 0.0 { (win.s1[i] - (2.0 / tw) * (win.gr * win.sr[i] + win.gi * win.si[i])) / den } else { 0.0 };
                win.sr[i] -= bb * win.gr;
                win.si[i] -= bb * win.gi;
            }
            let mut vals = Vec::with_capacity(np);
            for i in 0..np {
                let (ar, ai_, br, bi) = (win.sr[2 * i] * scale, win.si[2 * i] * scale, win.sr[2 * i + 1] * scale, win.si[2 * i + 1] * scale);
                let (mut hr, mut hi);
                if loops[i] {
                    // H = V(other)/V(dot)
                    let d = ar * ar + ai_ * ai_;
                    if d == 0.0 {
                        hr = f64::NAN;
                        hi = f64::NAN;
                    } else {
                        hr = (br * ar + bi * ai_) / d;
                        hi = (bi * ar - br * ai_) / d;
                    }
                    if negs[i] {
                        hr = -hr;
                        hi = -hi;
                    }
                } else {
                    // injection phasor: amp*sin -> -j*amp ; Y / (-j amp) = j Y / amp
                    hr = -ai_ / amp;
                    hi = ar / amp;
                }
                vals.push((hr, hi));
            }
            let tol = if ac.tol > 0.0 { ac.tol } else { 1e-3 };
            let conv = match &last {
                Some(lv) => vals.iter().zip(lv.iter()).all(|(v, a)| {
                    let ma = a.0.hypot(a.1);
                    let mb = v.0.hypot(v.1);
                    if !(ma > 0.0) || !(mb > 0.0) {
                        return ma == mb;
                    }
                    let dm = (mb / ma - 1.0).abs();
                    let dp = (a.0 * v.1 - a.1 * v.0).atan2(a.0 * v.0 + a.1 * v.1).abs();
                    dm < tol && dp < tol
                }),
                None => false,
            };
            last = Some(vals.clone());
            let over = cyc + nm > max_cyc;
            if conv || over {
                if !conv {
                    warn.push(format!("頻率 {} 在 {} 個週期內未達穩態，結果可能有誤差。", fmt_f(f), expr::js_round(cyc)));
                }
                return Ok((vals, warn, a.e.warnings.clone()));
            }
            win = new_win(win.te);
        }
    }
}

/// AC sweep by small-signal injection. Frequencies run in parallel from the same steady-state snapshot.
pub fn run_ac_inject(net: &Netlist, sc: &SimCtrl, ac: &AcSpec, threads: usize, progress: &(dyn Fn(f64, f64) + Sync), cancel: &AtomicBool) -> R<AcResult> {
    let mut o = SimOpts::new(sc.dt);
    o.interp = sc.interp;
    o.damp = sc.damp;
    let mut a = Sim::new(net, o)?;
    if a.ac_probes.is_empty() {
        return Err(SimError::new("AC 掃頻：電路中沒有 AC Sweep 探棒（AC Sweep Probe）。"));
    }
    let src = match a.find_source(&ac.src) {
        Some(i) if a.insts[i].can_inject() => i,
        _ => return Err(SimError::new(format!("AC 掃頻：找不到名稱為「{}」的正弦波激勵源（需為 Sine 電壓/電流源）。", ac.src))),
    };
    let freqs = sweep_freqs(ac)?;
    let pre_n = ((if ac.t_pre > 0.0 { ac.t_pre } else { sc.t_total }) / sc.dt).round().max(1.0) as u64;
    a.insts[src].set_inj(Inj { a: 0.0, f: 1.0, t0: 0.0, tr: 0.0 });
    a.init()?;
    for n in 1..=pre_n {
        a.advance_to(n as f64 * sc.dt)?;
        if n % 256 == 0 {
            if cancel.load(Ordering::Relaxed) {
                return Err(SimError::new("已中止"));
            }
            progress(0.15 * n as f64 / pre_n as f64, a.t);
        }
    }
    let snap = a;
    let nf = freqs.len();
    let results: Mutex<Vec<Option<(Vec<(f64, f64)>, Vec<String>, Vec<String>)>>> = Mutex::new(vec![None; nf]);
    let next = AtomicUsize::new(0);
    let done = AtomicUsize::new(0);
    let first_err: Mutex<Option<SimError>> = Mutex::new(None);
    let stop = AtomicBool::new(false);
    let nthreads = threads.max(1).min(nf.max(1));
    std::thread::scope(|s| {
        for _ in 0..nthreads {
            s.spawn(|| loop {
                if stop.load(Ordering::Relaxed) || cancel.load(Ordering::Relaxed) {
                    break;
                }
                let i = next.fetch_add(1, Ordering::Relaxed);
                if i >= nf {
                    break;
                }
                match ac_one_freq(&snap, src, ac, sc, freqs[i], cancel) {
                    Ok(r) => {
                        results.lock().unwrap()[i] = Some(r);
                        let d = done.fetch_add(1, Ordering::Relaxed) + 1;
                        progress(0.15 + 0.85 * d as f64 / nf as f64, freqs[i]);
                    }
                    Err(e) => {
                        let mut fe = first_err.lock().unwrap();
                        if fe.is_none() {
                            *fe = Some(e);
                        }
                        stop.store(true, Ordering::Relaxed);
                        break;
                    }
                }
            });
        }
    });
    if let Some(e) = first_err.into_inner().unwrap() {
        return Err(e);
    }
    if cancel.load(Ordering::Relaxed) {
        return Err(SimError::new("已中止"));
    }
    let results = results.into_inner().unwrap();
    let names: Vec<(String, bool)> = snap.ac_probes.iter().map(|&i| (snap.insts[i].pname(), snap.insts[i].ac_loop() && snap.insts[i].ac_neg())).collect();
    let mut res: Vec<(String, Vec<f64>, Vec<f64>, bool)> = names.iter().map(|(n, l)| (n.clone(), vec![], vec![], *l)).collect();
    let mut warnings: Vec<String> = Vec::new();
    let mut sim_warn: Vec<String> = snap.e.warnings.clone();
    let mut fdone = Vec::new();
    for (i, r) in results.into_iter().enumerate() {
        if let Some((vals, w, sw)) = r {
            for (k, v) in vals.iter().enumerate() {
                res[k].1.push(v.0);
                res[k].2.push(v.1);
            }
            warnings.extend(w);
            for m in sw {
                if !sim_warn.contains(&m) && sim_warn.len() < 200 {
                    sim_warn.push(m);
                }
            }
            fdone.push(freqs[i]);
        }
    }
    warnings.extend(sim_warn);
    Ok(make_bode(fdone, res, warnings))
}

/// Linear frequency-domain AC analysis
pub fn run_ac_linear(net: &Netlist, sc: &SimCtrl, ac: &AcSpec, progress: &(dyn Fn(f64, f64) + Sync), cancel: &AtomicBool) -> R<AcResult> {
    let mut sim = Sim::new(net, SimOpts::new(sc.dt))?;
    if sim.ac_probes.is_empty() {
        return Err(SimError::new("AC 掃頻：電路中沒有 AC Sweep 探棒。"));
    }
    let src = match sim.find_source(&ac.src) {
        Some(i) if sim.insts[i].can_inject() => i,
        _ => return Err(SimError::new(format!("AC 掃頻：找不到名稱為「{}」的正弦波激勵源。", ac.src))),
    };
    sim.n_ac_check()?;
    let freqs = sweep_freqs(ac)?;
    let n = sim.n;
    let nn = n + sim.n_c - 1;
    sim.init_elements()?;
    let mut res: Vec<(String, Vec<f64>, Vec<f64>, bool)> = sim.ac_probes.iter().map(|&i| (sim.insts[i].pname(), vec![], vec![], sim.insts[i].ac_loop() && sim.insts[i].ac_neg())).collect();
    let mut xr = vec![0.0; nn];
    let mut xi = vec![0.0; nn];
    for (fi, &f) in freqs.iter().enumerate() {
        if cancel.load(Ordering::Relaxed) {
            return Err(SimError::new("已中止"));
        }
        let w = 2.0 * std::f64::consts::PI * f;
        let mut c = AcCtx { nn, n, w, ar: vec![0.0; nn * nn], ai: vec![0.0; nn * nn], br: vec![0.0; nn], bi: vec![0.0; nn] };
        sim.stamp_ac_all(&mut c, src)?;
        let mut lu = Clu::new(nn);
        if !lu.factor(&c.ar, &c.ai) {
            return Err(SimError::new(format!("AC 分析矩陣奇異（f={}）。", fmt_f(f))));
        }
        lu.solve(&c.br, &c.bi, &mut xr, &mut xi);
        for (k, &pi) in sim.ac_probes.iter().enumerate() {
            let v = sim.insts[pi].ac_phasor(&xr, &xi, n);
            let (mut hr, mut hi);
            if sim.insts[pi].ac_loop() {
                let d = v[0] * v[0] + v[1] * v[1];
                hr = (v[2] * v[0] + v[3] * v[1]) / d;
                hi = (v[3] * v[0] - v[2] * v[1]) / d;
                if sim.insts[pi].ac_neg() {
                    hr = -hr;
                    hi = -hi;
                }
            } else {
                // excitation phasor -j  => H = Y / (-j) = j Y
                hr = -v[1];
                hi = v[0];
            }
            res[k].1.push(hr);
            res[k].2.push(hi);
        }
        progress((fi + 1) as f64 / freqs.len() as f64, f);
    }
    let warnings: Vec<String> = sim.e.warnings.iter().filter(|w| !w.contains("代數迴路")).cloned().collect();
    Ok(make_bode(freqs, res, warnings))
}
