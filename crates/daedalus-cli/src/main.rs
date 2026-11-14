//! Headless runner.
//!   daedalus-cli <file.psimx> <out.csv> [--ac] [--threads N]
//!   daedalus-cli <case.json> <out.csv>          (regression case: {net, sc, ac})
//!   daedalus-cli mcp                            (Model Context Protocol server on stdio, for AI agents)
//!   daedalus-cli skill install [folder]         (circuit-builder skill for Claude Code)
//!   daedalus-cli new <netlist.json> <out.psimx> | inspect <file> | render <file> <out.png> | tool <name> <args>
mod circuit;
mod layout;
mod mcp;
mod render;
mod run;
mod skill;
mod tools;

use daedalus_core::doc::Doc;
use daedalus_core::netlist::{build_netlist, JsNetlist, Netlist};
use daedalus_core::runner::*;
use std::io::Write;
use std::sync::atomic::AtomicBool;
use std::time::Instant;

fn die(msg: &str, code: i32) -> ! {
    eprintln!("{}", msg);
    std::process::exit(code)
}

fn sc_from(v: &serde_json::Value) -> SimCtrl {
    let f = |k: &str, d: f64| v.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
    let b = |k: &str| v.get(k).map(|x| x.as_bool().unwrap_or(true)).unwrap_or(true);
    SimCtrl { dt: f("dt", 1e-6), t_total: f("tTotal", 1e-3), t_print: f("tPrint", 0.0), print_step: f("printStep", 1.0).max(1.0) as u64, interp: b("interp"), damp: b("damp"), lte: b("lte"), lte_tol: f("lteTol", 1e-3), acc: v.get("acc").and_then(|x| x.as_bool()).unwrap_or(false), etol: f("etol", 1e-6), max_lv: f("maxLv", 8.0) as u32 }
}
fn ac_from(v: &serde_json::Value) -> AcSpec {
    let f = |k: &str, d: f64| v.get(k).and_then(|x| x.as_f64()).unwrap_or(d);
    AcSpec {
        f_start: f("fStart", 10.0),
        f_end: f("fEnd", 1e5),
        n_pts: f("nPts", 40.0),
        flag: f("flag", 0.0) as i64,
        src: v.get("src").and_then(|x| x.as_str()).unwrap_or("").to_string(),
        a_start: f("aStart", 0.1),
        a_end: f("aEnd", 0.1),
        extra: v.get("extra").and_then(|x| x.as_array()).map(|a| a.iter().filter_map(|y| y.as_f64()).collect()).unwrap_or_default(),
        method: f("method", 0.0) as i64,
        settle: f("settle", 5.0),
        mcyc: f("mcyc", 4.0),
        max_cyc: f("maxCyc", 60.0),
        tol: f("tol", 1e-3),
        t_pre: f("tPre", -1.0),
        t_settle: f("tSettle", 0.0),
        baseline: f("baseline", 1.0) != 0.0,
    }
}

fn write_ac(out: &mut impl Write, res: &AcResult) {
    write!(out, "Freq").unwrap();
    for c in &res.curves {
        write!(out, ",{}_re,{}_im", c.name, c.name).unwrap();
    }
    writeln!(out).unwrap();
    for (i, f) in res.freqs.iter().enumerate() {
        write!(out, "{}", f).unwrap();
        for c in &res.curves {
            write!(out, ",{},{}", c.re[i], c.im[i]).unwrap();
        }
        writeln!(out).unwrap();
    }
}

fn run_tran(net: &Netlist, sc: &SimCtrl, out: &mut impl Write) -> Result<String, String> {
    let t0 = Instant::now();
    if sc.acc {
        let threads = std::thread::available_parallelism().map(|n| n.get()).unwrap_or(2);
        let res = daedalus_core::accuracy::run_accurate(net, sc, threads, &|_, _| {}, &AtomicBool::new(false)).map_err(|e| e.msg)?;
        return Ok(write_tran(out, &res, t0.elapsed().as_secs_f64()));
    }
    let mut r = TransientRunner::new(net, sc).map_err(|e| e.msg)?;
    loop {
        match r.run_steps(1 << 20) {
            Ok(true) => break,
            Ok(false) => {}
            Err(e) => return Err(e.msg),
        }
    }
    let res = r.result();
    Ok(write_tran(out, &res, t0.elapsed().as_secs_f64()))
}

fn write_tran(out: &mut impl Write, res: &TranResult, secs: f64) -> String {
    write!(out, "Time").unwrap();
    for n in &res.names {
        write!(out, ",{}", n).unwrap();
    }
    writeln!(out).unwrap();
    for i in 0..res.n {
        write!(out, "{}", res.t0 + i as f64 * res.dt).unwrap();
        for d in &res.data {
            write!(out, ",{}", d[i]).unwrap();
        }
        writeln!(out).unwrap();
    }
    let acc = res.accuracy.as_ref().map(|a| format!(",\"accuracy\":{{\"target\":{},\"est\":{},\"met\":{},\"dt\":{},\"level\":{},\"runs\":{},\"worst\":{:?}}}", a.target, a.est, a.met, a.dt, a.level, a.runs, a.worst)).unwrap_or_default();
    let mcj = res.mc.as_ref().map(|m| {
        let st: Vec<String> = m.stats.iter().map(|s| format!("{{\"probe\":{:?},\"metric\":{:?},\"mean\":{},\"std\":{},\"min\":{},\"max\":{}}}", s.probe, s.metric, s.mean, s.std, s.min, s.max)).collect();
        let sm: Vec<String> = m.samples.iter().map(|r| format!("[{}]", r.iter().map(|x| x.to_string()).collect::<Vec<_>>().join(","))).collect();
        format!(",\"mc\":{{\"vars\":{:?},\"seed\":{},\"stats\":[{}],\"samples\":[{}]}}", m.vars, m.seed, st.join(","), sm.join(","))
    }).unwrap_or_default();
    let sweep = res.sweep.as_ref().map(|s| format!(",\"sweep\":{{\"var\":{:?},\"runs\":{},\"threads\":{}}}", s.var, s.labels.len(), s.threads)).unwrap_or_default();
    format!(
        "{{\"secs\":{},\"steps\":{},\"refined\":{},\"refineMax\":{},\"evtRefined\":{},\"nonConv\":{},\"events\":{}{},\"warnings\":{:?}}}",
        secs, res.stats.steps, res.stats.refined, res.stats.refine_max, res.stats.evt_refined, res.stats.non_conv, res.events, sweep + &acc + &mcj, res.warnings
    )
}

fn main() {
    let args: Vec<String> = std::env::args().collect();
    match args.get(1).map(String::as_str) {
        Some("mcp") => std::process::exit(mcp::serve()),
        Some("skill") => std::process::exit(skill::cmd(&args[2..])),
        Some("tool") | Some("new") | Some("inspect") | Some("render") => std::process::exit(tools::cli(&args[1..])),
        Some("--version") | Some("-V") => {
            println!("daedalus-cli {}", daedalus_core::VERSION);
            return;
        }
        _ => {}
    }
    if args.len() < 3 {
        die("usage: daedalus-cli <file.psimx|case.json> <out.csv> [--ac] [--threads N]
       daedalus-cli mcp
       daedalus-cli skill install [folder]", 1);
    }
    let txt = std::fs::read_to_string(&args[1]).unwrap_or_else(|e| die(&format!("read: {}", e), 1));
    let threads = args.iter().position(|a| a == "--threads").and_then(|i| args.get(i + 1)).and_then(|s| s.parse().ok()).unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get()).unwrap_or(4));
    let cancel = AtomicBool::new(false);
    let prog = |_p: f64, _f: f64| {};
    let mut out = std::io::BufWriter::new(std::fs::File::create(&args[2]).unwrap());
    let v: serde_json::Value = serde_json::from_str(&txt).unwrap_or_else(|e| die(&format!("json: {}", e), 1));
    if v.get("net").is_some() && v.get("sc").is_some() {
        // regression case
        let jn: JsNetlist = serde_json::from_value(v["net"].clone()).unwrap_or_else(|e| die(&format!("net: {}", e), 1));
        let net = jn.to_netlist();
        let sc = sc_from(&v["sc"]);
        if v.get("ac").map(|a| !a.is_null()).unwrap_or(false) {
            let ac = ac_from(&v["ac"]);
            let t0 = Instant::now();
            let res = if ac.method == 1 { run_ac_linear(&net, &sc, &ac, &prog, &cancel) } else { run_ac_inject(&net, &sc, &ac, threads, &prog, &cancel) };
            match res {
                Ok(r) => {
                    write_ac(&mut out, &r);
                    println!("{{\"secs\":{},\"warnings\":{:?}}}", t0.elapsed().as_secs_f64(), r.warnings);
                }
                Err(e) => println!("{{\"error\":{:?}}}", e.msg),
            }
        } else {
            match run_tran(&net, &sc, &mut out) {
                Ok(s) => println!("{}", s),
                Err(e) => println!("{{\"error\":{:?}}}", e),
            }
        }
        return;
    }
    let (doc, _) = Doc::from_json(&txt).unwrap_or_else(|e| die(&e, 2));
    let br = build_netlist(&doc);
    if !br.errors.is_empty() {
        for e in &br.errors {
            eprintln!("netlist error: {} {:?}", e.msg, e.els);
        }
        std::process::exit(2);
    }
    let sc = eval_sim_ctrl(&doc, &br.syms).unwrap_or_else(|e| die(&e.msg, 2));
    if args.iter().any(|a| a == "--ac") {
        let ac = eval_ac(&doc, &br.syms).unwrap_or_else(|e| die(&e.msg, 2)).unwrap_or_else(|| die("no ACSWEEP element", 2));
        let t0 = Instant::now();
        let res = if ac.method == 1 { run_ac_linear(&br.net, &sc, &ac, &prog, &cancel) } else { run_ac_inject(&br.net, &sc, &ac, threads, &prog, &cancel) };
        let res = res.unwrap_or_else(|e| die(&format!("sim error: {}", e.msg), 3));
        write_ac(&mut out, &res);
        println!("{{\"secs\":{},\"warnings\":{:?}}}", t0.elapsed().as_secs_f64(), res.warnings);
    } else if let Some(mc) = daedalus_core::montecarlo::eval_mc(&doc, &br.syms).unwrap_or_else(|e| die(&e.msg, 2)) {
        let t0 = Instant::now();
        let res = daedalus_core::montecarlo::run_mc(&br.net, &sc, &mc, threads, &prog, &cancel).unwrap_or_else(|e| die(&format!("sim error: {}", e.msg), 3));
        println!("{}", write_tran(&mut out, &res, t0.elapsed().as_secs_f64()));
    } else if let Some(sw) = daedalus_core::sweep::eval_sweep(&doc, &br.syms).unwrap_or_else(|e| die(&e.msg, 2)) {
        // parameter sweep: all values in parallel (--threads N, default all cores)
        let t0 = Instant::now();
        let res = daedalus_core::sweep::run_sweep(&br.net, &sc, &sw, threads, &prog, &cancel).unwrap_or_else(|e| die(&format!("sim error: {}", e.msg), 3));
        println!("{}", write_tran(&mut out, &res, t0.elapsed().as_secs_f64()));
    } else {
        match run_tran(&br.net, &sc, &mut out) {
            Ok(s) => println!("{}", s),
            Err(e) => die(&format!("sim error: {}", e), 3),
        }
    }
}
