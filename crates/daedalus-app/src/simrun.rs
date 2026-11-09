//! Simulation in a worker thread with progress reporting and cancellation.

use daedalus_core::netlist::Netlist;
use daedalus_core::runner::*;
use daedalus_core::montecarlo::{run_mc, McSpec};
use daedalus_core::sweep::{eng, run_sweep, SweepSpec};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver};
use std::sync::Arc;
use std::time::{Duration, Instant};

pub enum Msg {
    Prog(f64, f64),
    Tran(Result<TranResult, (String, Option<i64>)>),
    Ac(Result<AcResult, (String, Option<i64>)>),
}

pub struct Running {
    rx: Receiver<Msg>,
    pub cancel: Arc<AtomicBool>,
    pub start: Instant,
    pub prog: f64,
    pub at: f64,
    pub is_ac: bool,
    /// number of runs of a parameter sweep (0 = not a sweep)
    pub sweep_n: usize,
    /// high-accuracy mode (progress value is the refinement level)
    pub acc: bool,
    pub title: String,
    pub info: String,
}

pub enum Done {
    Tran(TranResult, f64),
    Ac(AcResult, f64),
    Err(String, Option<i64>),
}

impl Running {
    /// `threads`: worker threads for the AC sweep and the parameter sweep
    pub fn start(net: Netlist, sc: SimCtrl, ac: Option<AcSpec>, sweep: Option<SweepSpec>, mc: Option<McSpec>, threads: usize, ctx: egui::Context) -> Running {
        let (tx, rx) = channel();
        let cancel = Arc::new(AtomicBool::new(false));
        let c2 = cancel.clone();
        let is_ac = ac.is_some();
        let sweep_n = sweep.as_ref().map(|s| s.values.len()).or(mc.as_ref().map(|m| m.runs)).unwrap_or(0);
        let acc_on = sc.acc;
        let title = match &ac {
            Some(a) if a.method == 1 => "AC 掃頻（線性頻域法）".to_string(),
            Some(_) => "AC 掃頻（小訊號注入法）".to_string(),
            None if mc.is_some() => format!("蒙地卡羅分析（{} 次暫態，{} 核心平行）", sweep_n, threads.min(sweep_n)),
            None if sweep_n > 0 => format!("參數掃描（{} 次暫態，{} 核心平行）", sweep_n, threads.min(sweep_n)),
            None if sc.acc => format!("高精度暫態模擬（目標誤差 {:.0e}）", sc.etol),
            None => "暫態模擬".to_string(),
        };
        let info = match &ac {
            Some(a) => format!("頻率 {} ~ {}，{} 點", crate::plot::si(a.f_start, "Hz", 3), crate::plot::si(a.f_end, "Hz", 3), a.n_pts),
            None => {
                let base = format!("步長 {}，總時間 {}，共 {} 步", crate::plot::si(sc.dt, "s", 4), crate::plot::si(sc.t_total, "s", 4), (sc.t_total / sc.dt).round());
                match &sweep {
                    None if mc.is_some() => {
                        let m = mc.as_ref().unwrap();
                        format!("{}；種子 {}；每次{}", m.vars.iter().map(|v| format!("{} ±{}%", v.name, v.tol * 100.0)).collect::<Vec<_>>().join("、"), m.seed, base)
                    }
                    Some(s) => {
                        let lo = s.values.iter().cloned().fold(f64::INFINITY, f64::min);
                        let hi = s.values.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
                        format!("{} = {} ~ {}；每次{}", s.var, eng(lo), eng(hi), base)
                    }
                    None => base,
                }
            }
        };
        std::thread::Builder::new()
            .name("simulation".into())
            .stack_size(64 << 20)
            .spawn(move || {
                let cancel = c2;
                match ac {
                    None if mc.is_some() => {
                        let m = mc.unwrap();
                        let ctx2 = ctx.clone();
                        let txp = std::sync::Mutex::new(tx.clone());
                        let prog = move |p: f64, done: f64| {
                            if let Ok(t) = txp.lock() {
                                let _ = t.send(Msg::Prog(p, done));
                            }
                            ctx2.request_repaint();
                        };
                        let r = run_mc(&net, &sc, &m, threads, &prog, &cancel);
                        let _ = tx.send(Msg::Tran(r.map_err(|e| (e.msg, e.el))));
                    }
                    None if sweep.is_some() => {
                        let sw = sweep.unwrap();
                        let ctx2 = ctx.clone();
                        let txp = std::sync::Mutex::new(tx.clone());
                        let prog = move |p: f64, done: f64| {
                            if let Ok(t) = txp.lock() {
                                let _ = t.send(Msg::Prog(p, done));
                            }
                            ctx2.request_repaint();
                        };
                        let r = run_sweep(&net, &sc, &sw, threads, &prog, &cancel);
                        let _ = tx.send(Msg::Tran(r.map_err(|e| (e.msg, e.el))));
                    }
                    None if sc.acc => {
                        let ctx2 = ctx.clone();
                        let txp = std::sync::Mutex::new(tx.clone());
                        let prog = move |p: f64, lv: f64| {
                            if let Ok(t) = txp.lock() {
                                let _ = t.send(Msg::Prog(p, lv));
                            }
                            ctx2.request_repaint();
                        };
                        let r = daedalus_core::accuracy::run_accurate(&net, &sc, threads, &prog, &cancel);
                        let _ = tx.send(Msg::Tran(r.map_err(|e| (e.msg, e.el))));
                    }
                    None => {
                        let r = (|| -> Result<TranResult, (String, Option<i64>)> {
                            let mut r = TransientRunner::new(&net, &sc).map_err(|e| (e.msg, e.el))?;
                            let mut last = Instant::now();
                            loop {
                                let done = r.run_steps(256).map_err(|e| (e.msg, e.el))?;
                                if done {
                                    break;
                                }
                                if cancel.load(Ordering::Relaxed) {
                                    return Err(("已中止".into(), None));
                                }
                                if last.elapsed() > Duration::from_millis(80) {
                                    last = Instant::now();
                                    let _ = tx.send(Msg::Prog(r.progress(), r.t()));
                                    ctx.request_repaint();
                                }
                            }
                            Ok(r.result())
                        })();
                        let _ = tx.send(Msg::Tran(r));
                    }
                    Some(ac) => {
                        let ctx2 = ctx.clone();
                        let txp = std::sync::Mutex::new(tx.clone());
                        let prog = move |p: f64, f: f64| {
                            if let Ok(t) = txp.lock() {
                                let _ = t.send(Msg::Prog(p, f));
                            }
                            ctx2.request_repaint();
                        };
                        let r = if ac.method == 1 { run_ac_linear(&net, &sc, &ac, &prog, &cancel) } else { run_ac_inject(&net, &sc, &ac, threads, &prog, &cancel) };
                        let _ = tx.send(Msg::Ac(r.map_err(|e| (e.msg, e.el))));
                    }
                }
                ctx.request_repaint();
            })
            .expect("spawn simulation thread");
        let acc = acc_on && !is_ac && sweep_n == 0;
        Running { rx, cancel, start: Instant::now(), prog: 0.0, at: 0.0, is_ac, sweep_n, acc, title, info }
    }
    /// poll messages; returns Some when finished
    pub fn poll(&mut self) -> Option<Done> {
        while let Ok(m) = self.rx.try_recv() {
            match m {
                Msg::Prog(p, a) => {
                    self.prog = p;
                    self.at = a;
                }
                Msg::Tran(r) => {
                    let s = self.start.elapsed().as_secs_f64();
                    return Some(match r {
                        Ok(r) => Done::Tran(r, s),
                        Err((m, e)) => Done::Err(m, e),
                    });
                }
                Msg::Ac(r) => {
                    let s = self.start.elapsed().as_secs_f64();
                    return Some(match r {
                        Ok(r) => Done::Ac(r, s),
                        Err((m, e)) => Done::Err(m, e),
                    });
                }
            }
        }
        None
    }
}

pub fn fmt_dur(s: f64) -> String {
    if s < 1.0 {
        format!("{:.0} ms", s * 1000.0)
    } else if s < 60.0 {
        format!("{:.1} s", s)
    } else {
        let m = (s / 60.0).floor();
        format!("{} 分 {} 秒", m, (s - 60.0 * m).round())
    }
}
