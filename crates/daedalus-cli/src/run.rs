//! Simulating a document for the MCP tools: transient (also high accuracy, parameter sweep, Monte Carlo) or the
//! AC sweep, with a time limit. The same core calls as the command-line runner in main.rs.
use daedalus_core::doc::Doc;
use daedalus_core::netlist::build_netlist;
use daedalus_core::runner::*;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum Outcome {
    Tran(TranResult),
    Ac(AcResult),
}

pub struct Run {
    pub out: Outcome,
    pub secs: f64,
    /// "暫態"、"高精度暫態"、"參數掃描"、"蒙地卡羅"、"AC 掃頻"
    pub kind: &'static str,
    pub sc: SimCtrl,
}

pub fn threads() -> usize {
    std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4)
}

/// `ac`: the AC sweep of the ACSWEEP element instead of the transient; `tt`: total time override (expression);
/// `limit`: seconds before the run is cancelled
pub fn run_doc(doc: &Doc, ac: bool, tt: Option<&str>, limit: f64) -> Result<Run, String> {
    let br = build_netlist(doc);
    if !br.errors.is_empty() {
        return Err(format!("電路有錯誤，無法模擬：\n{}", br.errors.iter().map(|e| format!("- {}", e.msg)).collect::<Vec<_>>().join("\n")));
    }
    let mut sc = eval_sim_ctrl(doc, &br.syms).map_err(|e| format!("模擬控制：{}", e.msg))?;
    if let Some(t) = tt {
        let v = daedalus_core::expr::eval_const(t, &br.syms).map_err(|e| format!("總時間 {}：{}", t, e.0))?;
        if !(v > 0.0) {
            return Err(format!("總時間必須大於 0（{}）", t));
        }
        sc.t_total = v;
    }
    let cancel = Arc::new(AtomicBool::new(false));
    let done = Arc::new(AtomicBool::new(false));
    {
        // watchdog: cancels the run after `limit` seconds
        let (cancel, done) = (cancel.clone(), done.clone());
        let lim = Duration::from_secs_f64(limit.max(1.0));
        std::thread::spawn(move || {
            let t0 = Instant::now();
            while !done.load(Ordering::Relaxed) {
                if t0.elapsed() >= lim {
                    cancel.store(true, Ordering::Relaxed);
                    return;
                }
                std::thread::sleep(Duration::from_millis(50));
            }
        });
    }
    let t0 = Instant::now();
    let prog = |_p: f64, _f: f64| {};
    let net = &br.net;
    let n = threads();
    let r = (|| -> Result<(Outcome, &'static str), String> {
        if ac {
            let spec = eval_ac(doc, &br.syms).map_err(|e| e.msg)?.ok_or_else(|| "電路中沒有 AC 掃頻元件（ACSWEEP）".to_string())?;
            let res = if spec.method == 1 { run_ac_linear(net, &sc, &spec, &prog, &cancel) } else { run_ac_inject(net, &sc, &spec, n, &prog, &cancel) };
            return res.map(|r| (Outcome::Ac(r), "AC 掃頻")).map_err(|e| e.msg);
        }
        if let Some(mc) = daedalus_core::montecarlo::eval_mc(doc, &br.syms).map_err(|e| e.msg)? {
            return daedalus_core::montecarlo::run_mc(net, &sc, &mc, n, &prog, &cancel).map(|r| (Outcome::Tran(r), "蒙地卡羅")).map_err(|e| e.msg);
        }
        if let Some(sw) = daedalus_core::sweep::eval_sweep(doc, &br.syms).map_err(|e| e.msg)? {
            return daedalus_core::sweep::run_sweep(net, &sc, &sw, n, &prog, &cancel).map(|r| (Outcome::Tran(r), "參數掃描")).map_err(|e| e.msg);
        }
        if sc.acc {
            return daedalus_core::accuracy::run_accurate(net, &sc, n, &prog, &cancel).map(|r| (Outcome::Tran(r), "高精度暫態")).map_err(|e| e.msg);
        }
        let mut tr = TransientRunner::new(net, &sc).map_err(|e| e.msg)?;
        loop {
            if cancel.load(Ordering::Relaxed) {
                return Err("canceled".into());
            }
            match tr.run_steps(1 << 15) {
                Ok(true) => break,
                Ok(false) => {}
                Err(e) => return Err(e.msg),
            }
        }
        Ok((Outcome::Tran(tr.result()), "暫態"))
    })();
    done.store(true, Ordering::Relaxed);
    let secs = t0.elapsed().as_secs_f64();
    match r {
        Ok((out, kind)) => Ok(Run { out, secs, kind, sc }),
        Err(_) if cancel.load(Ordering::Relaxed) => Err(format!(
            "模擬超過時間上限 {:.0} 秒而中止。可縮短總時間（tt）、放大步長（dt），或以 timeout 參數放寬上限。",
            limit
        )),
        Err(e) => Err(format!("模擬失敗：{}", e)),
    }
}
