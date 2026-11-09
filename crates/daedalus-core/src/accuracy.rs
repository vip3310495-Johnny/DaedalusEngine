//! High-accuracy transient mode: automatic time-step selection from a global error target.
//!
//! The transient is run at dt·2^-k and dt·2^-(k+1) (in parallel) on the user's output grid. The global
//! error of the finer run is estimated by step doubling, max|x(k+1) − x(k)| / 3 relative to each probe's
//! full scale (trapezoidal rule: error ∝ dt²). When the estimate meets the target, the result is the
//! Richardson extrapolation x(k+1) + (x(k+1) − x(k))/3; otherwise the step is refined by the factor the dt²
//! law predicts and the check repeats, up to `max_lv` halvings.

use crate::netlist::Netlist;
use crate::runner::{SimCtrl, TranResult, TransientRunner};
use crate::sim::{SimError, R};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

#[derive(Clone, Debug, Default)]
pub struct AccInfo {
    /// target relative error
    pub target: f64,
    /// estimated relative error of the reported result (upper estimate: error of the finer run)
    pub est: f64,
    pub met: bool,
    /// time step of the finest run and number of halvings of the user's step
    pub dt: f64,
    pub level: u32,
    /// probe and time where the estimate is largest
    pub worst: String,
    pub worst_t: f64,
    /// Richardson extrapolation applied to the output
    pub extrapolated: bool,
    /// transient runs performed
    pub runs: u32,
}

/// the run at level k: same stop time, print start and output grid as level 0
fn level_ctrl(sc: &SimCtrl, k: u32, t_end: f64, t_print: f64) -> SimCtrl {
    let f = 1u64 << k;
    SimCtrl { dt: sc.dt / f as f64, t_total: t_end, t_print, print_step: sc.print_step.max(1) * f, acc: false, ..sc.clone() }
}

struct Prog<'a> {
    done: Vec<AtomicU64>,
    total: AtomicU64,
    base: AtomicU64,
    last: Mutex<Instant>,
    f: &'a (dyn Fn(f64, f64) + Sync),
    level: AtomicU64,
}

fn run_level(net: &Netlist, sc: &SimCtrl, slot: usize, p: &Prog, cancel: &AtomicBool) -> R<TranResult> {
    let mut r = TransientRunner::new(net, sc)?;
    loop {
        if r.run_steps(256)? {
            break;
        }
        if cancel.load(Ordering::Relaxed) {
            return Err(SimError::new("已中止"));
        }
        p.done[slot].store((r.progress() * r.n_steps() as f64) as u64, Ordering::Relaxed);
        let mut l = p.last.lock().unwrap();
        if l.elapsed() > Duration::from_millis(80) {
            *l = Instant::now();
            drop(l);
            let d = p.base.load(Ordering::Relaxed) + p.done.iter().map(|x| x.load(Ordering::Relaxed)).sum::<u64>();
            (p.f)((d as f64 / p.total.load(Ordering::Relaxed).max(1) as f64).min(0.999), p.level.load(Ordering::Relaxed) as f64);
        }
    }
    p.done[slot].store(r.n_steps(), Ordering::Relaxed);
    Ok(r.result())
}

/// step-doubling estimate of the finer run's error: (relative error, probe index, time)
fn estimate(coarse: &TranResult, fine: &TranResult) -> (f64, usize, f64) {
    let n = coarse.n.min(fine.n);
    let mut worst = (0.0, 0, 0.0);
    for (j, (a, b)) in coarse.data.iter().zip(fine.data.iter()).enumerate() {
        let scale = b[..n].iter().fold(0.0f64, |m, v| m.max(v.abs()));
        if !(scale > 0.0) || !scale.is_finite() {
            continue;
        }
        for i in 0..n {
            let e = (b[i] - a[i]).abs() / (3.0 * scale);
            if e > worst.0 || !e.is_finite() {
                worst = (if e.is_finite() { e } else { f64::INFINITY }, j, fine.t0 + i as f64 * fine.dt);
            }
        }
    }
    worst
}

/// run the high-accuracy transient. `progress(fraction, level)`; `threads` ≥ 2 runs each pair in parallel.
pub fn run_accurate(net: &Netlist, sc: &SimCtrl, threads: usize, progress: &(dyn Fn(f64, f64) + Sync), cancel: &AtomicBool) -> R<TranResult> {
    let target = if sc.etol > 0.0 { sc.etol } else { 1e-6 };
    let max_lv = sc.max_lv.clamp(1, 20);
    // level 0 fixes the time grid (stop time and print start rounded to whole steps exactly as a normal run)
    let probe = TransientRunner::new(net, &SimCtrl { acc: false, ..sc.clone() })?;
    let t_end = probe.n_steps() as f64 * sc.dt;
    let t_print = probe.t0();
    let steps0 = probe.n_steps();
    drop(probe);
    let p = Prog { done: vec![AtomicU64::new(0), AtomicU64::new(0)], total: AtomicU64::new(0), base: AtomicU64::new(0), last: Mutex::new(Instant::now()), f: progress, level: AtomicU64::new(0) };
    let mut runs = 0u32;
    // run levels (a, b) — b = a + 1 — reusing an existing run for a when given
    let mut pair = |a: u32, have_a: Option<TranResult>| -> R<(TranResult, TranResult)> {
        let ca = level_ctrl(sc, a, t_end, t_print);
        let cb = level_ctrl(sc, a + 1, t_end, t_print);
        let sa = if have_a.is_some() { 0 } else { steps0 << a };
        let sb = steps0 << (a + 1);
        p.base.fetch_add(p.done.iter().map(|x| x.swap(0, Ordering::Relaxed)).sum::<u64>(), Ordering::Relaxed);
        p.total.fetch_add(sa + sb, Ordering::Relaxed);
        p.level.store(a as u64 + 1, Ordering::Relaxed);
        match have_a {
            Some(ra) => {
                runs += 1;
                Ok((ra, run_level(net, &cb, 1, &p, cancel)?))
            }
            None if threads >= 2 => {
                runs += 2;
                let (ra, rb) = std::thread::scope(|s| {
                    let ha = std::thread::Builder::new().stack_size(64 << 20).spawn_scoped(s, || run_level(net, &ca, 0, &p, cancel)).expect("spawn");
                    let hb = std::thread::Builder::new().stack_size(64 << 20).spawn_scoped(s, || run_level(net, &cb, 1, &p, cancel)).expect("spawn");
                    (ha.join().unwrap(), hb.join().unwrap())
                });
                Ok((ra?, rb?))
            }
            None => {
                runs += 2;
                Ok((run_level(net, &ca, 0, &p, cancel)?, run_level(net, &cb, 1, &p, cancel)?))
            }
        }
    };
    let (mut lo, mut hi) = pair(0, None)?;
    let mut k = 0u32; // hi is level k + 1
    let mut notes: Vec<String> = Vec::new();
    // best pair so far (smallest estimate): returned when refining stops helping
    let mut best: Option<(f64, usize, f64, u32, TranResult, TranResult)> = None;
    let mut floor = false;
    loop {
        let (est, wj, wt) = estimate(&lo, &hi);
        let improved = best.as_ref().map(|b| est < 0.8 * b.0).unwrap_or(true);
        // Newton failures that the coarser run did not have mean the step is below what double precision
        // resolves for this circuit (capacitor companions ∝ 1/h against inductor ones ∝ h)
        let newton_floor = hi.stats.non_conv > lo.stats.non_conv && hi.stats.non_conv > 0;
        if (improved && !newton_floor) || best.is_none() {
            best = Some((est, wj, wt, k, lo, hi));
        } else {
            floor = true;
            if newton_floor {
                notes.push(format!("高精度模式：時間步長 {} 時非線性元件開始無法收斂（矩陣條件數隨步長平方惡化，已達雙精度浮點的數值極限），採用較粗一層的結果。", fmt_s(sc.dt / (1u64 << (k + 1)) as f64)));
            } else {
                notes.push(format!("高精度模式：細分到 {} 後估計誤差未再下降（{:.2e}），電路可能含極快的切換邊緣；採用目前最佳結果。", fmt_s(sc.dt / (1u64 << (k + 1)) as f64), est));
            }
            break;
        }
        if est <= target || k + 1 >= max_lv {
            break;
        }
        // dt² law: halvings needed so that est·4^-j ≤ target (one or two per round, so convergence is observed)
        let j = (0.5 * (est / target).log2()).ceil().clamp(1.0, 2.0) as u32;
        let next = (k + j).min(max_lv - 1);
        let reuse = if next == k + 1 { best.as_ref().map(|b| b.5.clone()) } else { None };
        let (a, b) = pair(next, reuse)?;
        lo = a;
        hi = b;
        k = next;
    }
    let (est, wj, wt, kb, lo, hi) = best.expect("first pair always kept");
    let k = kb;
    let _ = floor;
    let level = k + 1;
    let met = est <= target;
    // Richardson extrapolation of the two finest runs
    let n = lo.n.min(hi.n);
    let mut out = hi;
    for (b, a) in out.data.iter_mut().zip(lo.data.iter()) {
        b.truncate(n);
        for i in 0..n {
            b[i] += (b[i] - a[i]) / 3.0;
        }
    }
    out.n = n;
    let worst = out.names.get(wj).cloned().unwrap_or_default();
    out.warnings.extend(notes);
    if !met {
        out.warnings.push(format!(
            "高精度模式：已細分到 dt/{}（{}），估計誤差 {:.2e} 仍高於目標 {:.0e}；誤差最大處在「{}」t = {}。可提高最大細分層數或放寬目標。",
            1u64 << level,
            fmt_s(sc.dt / (1u64 << level) as f64),
            est,
            target,
            worst,
            fmt_s(wt)
        ));
    }
    out.accuracy = Some(AccInfo { target, est, met, dt: sc.dt / (1u64 << level) as f64, level, worst, worst_t: wt, extrapolated: true, runs });
    progress(1.0, level as f64);
    Ok(out)
}

fn fmt_s(t: f64) -> String {
    crate::sweep::eng(t) + "s"
}
