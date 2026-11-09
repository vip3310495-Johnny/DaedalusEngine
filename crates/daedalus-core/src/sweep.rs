//! Parameter sweep: the same transient analysis for every value of one parameter-file variable,
//! run in parallel on all CPU cores. Every run uses the identical time grid, so the results are
//! merged into one `TranResult` with one column per (probe, value).

use crate::doc::Doc;
use crate::expr::{self, Syms};
use crate::netlist::{Netlist, NlError};
use crate::runner::{SimCtrl, Stats, SweepInfo, TranResult, TransientRunner};
use crate::sim::{SimError, R};
use std::sync::atomic::{AtomicBool, AtomicU64, AtomicUsize, Ordering};
use std::sync::Mutex;
use std::time::{Duration, Instant};

/// what one run changes: a parameter-file variable, or one parameter of one element
#[derive(Clone, Debug, PartialEq)]
pub enum Target {
    Var(String),
    El { id: i64, key: String },
}
impl Target {
    /// the same thing (variable names are case-insensitive)
    pub fn same(&self, o: &Target) -> bool {
        match (self, o) {
            (Target::Var(a), Target::Var(b)) => a.eq_ignore_ascii_case(b),
            _ => self == o,
        }
    }
}

/// a netlist with one override applied
pub fn apply_override(n: &mut Netlist, t: &Target, v: f64) {
    match t {
        Target::Var(var) => n.param_text = override_param(&n.param_text, var, v),
        Target::El { id, key } => {
            if let Some(e) = n.elements.iter_mut().find(|e| e.id == *id) {
                e.params.insert(key.clone(), format!("{:e}", v));
            }
        }
    }
}

#[derive(Clone, Debug)]
pub struct SweepSpec {
    pub var: String,
    pub values: Vec<f64>,
    pub el: i64,
}

fn is_ident(s: &str) -> bool {
    let mut c = s.chars();
    matches!(c.next(), Some(x) if x.is_ascii_alphabetic() || x == '_') && c.all(|x| x.is_ascii_alphanumeric() || x == '_')
}

/// the enabled Parameter Sweep element of a document, evaluated
pub fn eval_sweep(doc: &Doc, syms: &Syms) -> Result<Option<SweepSpec>, NlError> {
    let el = match doc.elements.iter().find(|e| e.typ == "PSWEEP" && !e.dis) {
        Some(e) => e,
        None => return Ok(None),
    };
    let err = |m: String| NlError { msg: format!("Parameter Sweep：{}", m), els: vec![el.id] };
    let p = |k: &str| el.params.get(k).cloned().unwrap_or_default();
    let var = p("var").trim().to_string();
    if !is_ident(&var) {
        return Err(err(format!("掃描變數「{}」不是合法名稱（英文字母開頭，可含數字與底線）", var)));
    }
    let num = |k: &str| expr::eval_const(&p(k), syms).map_err(|e| err(format!("{} {}", k, e.0)));
    let mode = p("mode").trim().parse::<i64>().unwrap_or(0);
    let values = match mode {
        0 => expr::eval_list(&p("vals"), syms).map_err(|e| err(format!("數值清單 {}", e.0)))?,
        _ => {
            let (a, b) = (num("v0")?, num("v1")?);
            let n = num("n")?.round();
            if !(n >= 1.0) || n > 1000.0 {
                return Err(err("點數必須為 1 ~ 1000".into()));
            }
            let n = n as usize;
            if mode == 2 && !(a * b > 0.0) {
                return Err(err("對數分佈的起始與結束值必須同號且不為 0".into()));
            }
            (0..n)
                .map(|i| {
                    let f = if n == 1 { 0.0 } else { i as f64 / (n - 1) as f64 };
                    if mode == 2 {
                        a.signum() * (a.abs().ln() + f * (b.abs().ln() - a.abs().ln())).exp()
                    } else {
                        a + f * (b - a)
                    }
                })
                .collect()
        }
    };
    if values.is_empty() {
        return Err(err("沒有任何掃描數值".into()));
    }
    if values.len() > 1000 {
        return Err(err("掃描數值最多 1000 個".into()));
    }
    if let Some(v) = values.iter().find(|v| !v.is_finite()) {
        return Err(err(format!("掃描數值 {} 不是有限數", v)));
    }
    Ok(Some(SweepSpec { var, values, el: el.id }))
}

/// parameter text with `var` redefined as `v` (in place, so later definitions that use it see the new value)
pub fn override_param(text: &str, var: &str, v: f64) -> String {
    let def = format!("{} = {:e}", var, v);
    let mut found = false;
    let mut out: Vec<String> = text
        .split('\n')
        .map(|line| {
            // names are case-insensitive
            let t = line.trim_start();
            let head: String = t.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            let defines = head.eq_ignore_ascii_case(var) && t[head.len()..].chars().next().map(|c| c == '=' || c.is_whitespace()).unwrap_or(false);
            if defines && !found {
                found = true;
                def.clone()
            } else if defines {
                String::new() // a later redefinition would undo the sweep
            } else {
                line.to_string()
            }
        })
        .collect();
    if !found {
        out.insert(0, def);
    }
    out.join("\n")
}

/// engineering notation for labels: 4.7e-5 → "47µ"
pub fn eng(v: f64) -> String {
    if v == 0.0 || !v.is_finite() {
        return format!("{}", v);
    }
    const P: [(f64, &str); 9] = [(1e9, "G"), (1e6, "Meg"), (1e3, "k"), (1.0, ""), (1e-3, "m"), (1e-6, "µ"), (1e-9, "n"), (1e-12, "p"), (1e-15, "f")];
    for (m, s) in P {
        if v.abs() >= m * 0.999_999_999 {
            let x = v / m;
            let t = format!("{:.6}", x);
            let t = t.trim_end_matches('0').trim_end_matches('.');
            return format!("{}{}", t, s);
        }
    }
    format!("{:e}", v)
}

/// run the sweep; `progress(fraction, runs_done)` is called from worker threads
pub fn run_sweep(net: &Netlist, sc: &SimCtrl, sw: &SweepSpec, threads: usize, progress: &(dyn Fn(f64, f64) + Sync), cancel: &AtomicBool) -> R<TranResult> {
    let cases: Vec<(String, Vec<(Target, f64)>)> = sw.values.iter().map(|&v| (format!("{}={}", sw.var, eng(v)), vec![(Target::Var(sw.var.clone()), v)])).collect();
    run_cases(net, sc, &cases, &sw.var, threads, progress, cancel)
}

/// One transient per case (label, parameter overrides), in parallel on `threads` workers, merged into one
/// result with probe-major columns "Vo [label]". Used by the parameter sweep and the Monte Carlo analysis.
pub fn run_cases(net: &Netlist, sc: &SimCtrl, cases: &[(String, Vec<(Target, f64)>)], var_desc: &str, threads: usize, progress: &(dyn Fn(f64, f64) + Sync), cancel: &AtomicBool) -> R<TranResult> {
    let nv = cases.len();
    if nv == 0 {
        return Err(SimError::new("沒有任何模擬案例"));
    }
    let labels: Vec<String> = cases.iter().map(|c| c.0.clone()).collect();
    // build every run first so that netlist / parameter errors are reported before any work starts
    let mut runners: Vec<Mutex<Option<TransientRunner>>> = Vec::with_capacity(nv);
    let mut nets: Vec<Netlist> = Vec::with_capacity(nv);
    for (i, (_, ov)) in cases.iter().enumerate() {
        let mut n = net.clone();
        for (t, v) in ov {
            apply_override(&mut n, t, *v);
        }
        nets.push(n.clone());
        let r = TransientRunner::new(&n, sc).map_err(|e| SimError { msg: format!("[{}] {}", labels[i], e.msg), el: e.el })?;
        if i == 0 {
            let bytes = r.n_rec() as f64 * r.names().len() as f64 * 8.0 * nv as f64;
            if bytes > 2.4e9 {
                return Err(SimError::new(format!("多次模擬的輸出資料量過大（約 {:.0} MB）。請減少次數、加大 Print step 或延後 Print time。", bytes / 1e6)));
            }
        }
        runners.push(Mutex::new(Some(r)));
    }
    let threads = threads.clamp(1, nv);
    let next = AtomicUsize::new(0);
    let done_steps: Vec<AtomicU64> = (0..nv).map(|_| AtomicU64::new(0)).collect();
    let total_steps: u64 = runners.iter().map(|r| r.lock().unwrap().as_ref().map(|x| x.n_steps()).unwrap_or(1)).sum::<u64>().max(1);
    let results: Vec<Mutex<Option<R<TranResult>>>> = (0..nv).map(|_| Mutex::new(None)).collect();
    let last_report = Mutex::new(Instant::now());
    let finished = AtomicUsize::new(0);
    let failed = AtomicBool::new(false);
    std::thread::scope(|s| {
        for w in 0..threads {
            let (runners, next, done_steps, results, last_report, finished, failed, nets) = (&runners, &next, &done_steps, &results, &last_report, &finished, &failed, &nets);
            std::thread::Builder::new()
                .name(format!("sweep-{}", w))
                .stack_size(64 << 20)
                .spawn_scoped(s, move || loop {
                    let i = next.fetch_add(1, Ordering::SeqCst);
                    if i >= nv || cancel.load(Ordering::Relaxed) || failed.load(Ordering::Relaxed) {
                        break;
                    }
                    let mut r = runners[i].lock().unwrap().take().unwrap();
                    if sc.acc {
                        // high-accuracy mode: each value refines its own step (the sweep is already parallel)
                        drop(r);
                        let out = crate::accuracy::run_accurate(&nets[i], sc, 1, &|_, _| {}, cancel);
                        done_steps[i].store(1, Ordering::Relaxed);
                        if out.is_err() {
                            failed.store(true, Ordering::Relaxed);
                        }
                        *results[i].lock().unwrap() = Some(out);
                        let f = finished.fetch_add(1, Ordering::Relaxed) + 1;
                        progress(f as f64 / nv as f64, f as f64);
                        continue;
                    }
                    let out = (|| -> R<TranResult> {
                        loop {
                            if r.run_steps(256)? {
                                break;
                            }
                            if cancel.load(Ordering::Relaxed) || failed.load(Ordering::Relaxed) {
                                return Err(SimError::new("已中止"));
                            }
                            done_steps[i].store((r.progress() * r.n_steps() as f64) as u64, Ordering::Relaxed);
                            let mut lr = last_report.lock().unwrap();
                            if lr.elapsed() > Duration::from_millis(80) {
                                *lr = Instant::now();
                                drop(lr);
                                let d: u64 = done_steps.iter().map(|x| x.load(Ordering::Relaxed)).sum();
                                progress(d as f64 / total_steps as f64, finished.load(Ordering::Relaxed) as f64);
                            }
                        }
                        done_steps[i].store(r.n_steps(), Ordering::Relaxed);
                        Ok(r.result())
                    })();
                    if out.is_err() {
                        failed.store(true, Ordering::Relaxed);
                    }
                    *results[i].lock().unwrap() = Some(out);
                    finished.fetch_add(1, Ordering::Relaxed);
                })
                .expect("spawn sweep worker");
        }
    });
    if cancel.load(Ordering::Relaxed) {
        return Err(SimError::new("已中止"));
    }
    let mut runs: Vec<TranResult> = Vec::with_capacity(nv);
    for (i, r) in results.into_iter().enumerate() {
        match r.into_inner().unwrap() {
            Some(Ok(x)) => runs.push(x),
            Some(Err(e)) if e.msg != "已中止" => return Err(SimError { msg: format!("[{}] {}", labels[i], e.msg), el: e.el }),
            _ => {}
        }
    }
    if runs.len() != nv {
        // another run failed and stopped this one; report the real error
        return Err(SimError::new("參數掃描中止"));
    }
    // merge: probe-major columns, "Vo [L=47µ]"
    let (t0, dt, n) = (runs[0].t0, runs[0].dt, runs.iter().map(|r| r.n).min().unwrap_or(0));
    let probes = runs[0].names.clone();
    let lte = runs[0].stats.lte;
    let mut names = Vec::with_capacity(probes.len() * nv);
    let mut cols: Vec<Vec<Vec<f64>>> = runs.iter_mut().map(|r| std::mem::take(&mut r.data)).collect();
    let mut data = Vec::with_capacity(probes.len() * nv);
    for (j, p) in probes.iter().enumerate() {
        for i in 0..nv {
            names.push(format!("{} [{}]", p, labels[i]));
            let mut c = std::mem::take(&mut cols[i][j]);
            c.truncate(n);
            data.push(c);
        }
    }
    let mut warnings = Vec::new();
    let mut stats = Stats { lte, ..Default::default() };
    let mut events = 0;
    for (i, r) in runs.iter().enumerate() {
        for w in &r.warnings {
            warnings.push(format!("[{}] {}", labels[i], w));
        }
        events += r.events;
        stats.steps += r.stats.steps;
        stats.refined += r.stats.refined;
        stats.refine_max += r.stats.refine_max;
        stats.evt_refined += r.stats.evt_refined;
        stats.non_conv += r.stats.non_conv;
    }
    Ok(TranResult { t0, dt, n, names, data, warnings, events, stats, sweep: Some(SweepInfo { var: var_desc.to_string(), labels, probes: probes.len(), threads }), accuracy: None, mc: None })
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn override_in_place() {
        let t = "Vin = 100\nL1 = 100u % inductor\nX = L1*2\n";
        let o = override_param(t, "L1", 47e-6);
        let (s, e) = expr::parse_param_text(&o);
        assert!(e.is_empty());
        assert_eq!(s["l1"], 47e-6);
        assert_eq!(s["x"], 94e-6);
        let o2 = override_param(t, "Lnew", 1.5);
        assert_eq!(expr::parse_param_text(&o2).0["lnew"], 1.5);
        let o4 = override_param("lf = 1
Y = LF + 1", "Lf", 2.0);
        assert_eq!(expr::parse_param_text(&o4).0["y"], 3.0);
        // a name that only starts like the variable is left alone
        let o3 = override_param("L10 = 5\nL1 = 3", "L1", 7.0);
        let s3 = expr::parse_param_text(&o3).0;
        assert_eq!((s3["l10"], s3["l1"]), (5.0, 7.0));
    }
    #[test]
    fn eng_labels() {
        assert_eq!(eng(47e-6), "47µ");
        assert_eq!(eng(2.2e3), "2.2k");
        assert_eq!(eng(1.0), "1");
        assert_eq!(eng(1e6), "1Meg");
    }
}
