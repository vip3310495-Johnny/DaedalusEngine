//! Monte Carlo analysis: parameter-file variables get random tolerances (uniform or Gaussian), the
//! transient runs once per sample in parallel, and per-probe statistics are collected across the runs.
//! Run #0 is always the nominal circuit. All samples are drawn up front from the seed, so results are
//! reproducible and independent of the number of threads.

use crate::doc::Doc;
use crate::expr::{self, Syms};
use crate::netlist::{Netlist, NlError};
use crate::runner::{SimCtrl, TranResult};
use crate::sim::R;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dist {
    /// uniform in nominal·(1 ± tol)
    Uniform,
    /// Gaussian with σ = tol/3 (tol is the 3σ bound), not truncated
    Gauss,
}

#[derive(Clone, Debug)]
pub struct McVar {
    pub name: String,
    pub nominal: f64,
    pub tol: f64,
    pub dist: Dist,
}

#[derive(Clone, Debug)]
pub struct McSpec {
    pub runs: usize,
    pub seed: u64,
    pub vars: Vec<McVar>,
    pub el: i64,
}

#[derive(Clone, Debug, Default)]
pub struct McStat {
    pub probe: String,
    /// "末值" | "平均" | "最大" | "最小"
    pub metric: String,
    pub mean: f64,
    pub std: f64,
    pub min: f64,
    pub max: f64,
    /// the metric of every run (run 0 = nominal)
    pub values: Vec<f64>,
}

#[derive(Clone, Debug, Default)]
pub struct McInfo {
    pub vars: Vec<String>,
    pub nominal: Vec<f64>,
    pub dists: Vec<String>,
    pub tols: Vec<f64>,
    /// samples[run][var]
    pub samples: Vec<Vec<f64>>,
    pub seed: u64,
    pub stats: Vec<McStat>,
}

/// splitmix64 → uniform [0, 1)
pub struct Rng(u64);
impl Rng {
    pub fn new(seed: u64) -> Self {
        Rng(seed ^ 0x9E37_79B9_7F4A_7C15)
    }
    pub fn next_u64(&mut self) -> u64 {
        self.0 = self.0.wrapping_add(0x9E37_79B9_7F4A_7C15);
        let mut z = self.0;
        z = (z ^ (z >> 30)).wrapping_mul(0xBF58_476D_1CE4_E5B9);
        z = (z ^ (z >> 27)).wrapping_mul(0x94D0_49BB_1331_11EB);
        z ^ (z >> 31)
    }
    pub fn uniform(&mut self) -> f64 {
        (self.next_u64() >> 11) as f64 / (1u64 << 53) as f64
    }
    /// standard normal (Box–Muller)
    pub fn normal(&mut self) -> f64 {
        let u1 = 1.0 - self.uniform(); // (0, 1]
        let u2 = self.uniform();
        (-2.0 * u1.ln()).sqrt() * (2.0 * std::f64::consts::PI * u2).cos()
    }
}

fn parse_tol(s: &str) -> Option<f64> {
    let s = s.trim();
    if let Some(p) = s.strip_suffix('%') {
        p.trim().parse::<f64>().ok().map(|v| v / 100.0)
    } else {
        s.parse::<f64>().ok()
    }
}

/// the enabled Monte Carlo element of a document, evaluated against the parameter file
pub fn eval_mc(doc: &Doc, syms: &Syms) -> Result<Option<McSpec>, NlError> {
    let el = match doc.elements.iter().find(|e| e.typ == "MONTECARLO" && !e.dis) {
        Some(e) => e,
        None => return Ok(None),
    };
    let err = |m: String| NlError { msg: format!("Monte Carlo：{}", m), els: vec![el.id] };
    let p = |k: &str| el.params.get(k).cloned().unwrap_or_default();
    let runs = expr::eval_const(&p("n"), syms).map_err(|e| err(format!("次數 {}", e.0)))?.round();
    if !(runs >= 1.0) || runs > 10000.0 {
        return Err(err("次數必須為 1 ~ 10000".into()));
    }
    let seed = expr::eval_const(&p("seed"), syms).map_err(|e| err(format!("種子 {}", e.0)))?.round().abs() as u64;
    let mut vars = Vec::new();
    for (li, line) in p("vars").split('\n').enumerate() {
        // comments: "#" or "//" ("%" is the tolerance unit here)
        let line = line.split('#').next().unwrap_or("");
        let line = match line.find("//") {
            Some(i) => &line[..i],
            None => line,
        };
        let toks: Vec<&str> = line.split([' ', '\t', ',']).filter(|t| !t.is_empty()).collect();
        if toks.is_empty() {
            continue;
        }
        let name = toks[0];
        let key = name.to_lowercase();
        let nominal = *syms.get(&key).ok_or_else(|| err(format!("第 {} 行：參數檔中沒有變數「{}」（蒙地卡羅只改變參數檔中的變數，元件參數欄位請改填此名稱）", li + 1, name)))?;
        let tol = toks.get(1).and_then(|t| parse_tol(t)).ok_or_else(|| err(format!("第 {} 行：容差格式錯誤（例如 5% 或 0.05）", li + 1)))?;
        if !(tol >= 0.0 && tol < 1.0) {
            return Err(err(format!("第 {} 行：容差必須介於 0 與 100% 之間", li + 1)));
        }
        let dist = match toks.get(2).map(|d| d.to_lowercase()).as_deref() {
            None | Some("g") | Some("gauss") | Some("gaussian") | Some("normal") | Some("常態") => Dist::Gauss,
            Some("u") | Some("uniform") | Some("均勻") => Dist::Uniform,
            Some(o) => return Err(err(format!("第 {} 行：不認得的分佈「{}」（gauss 或 uniform）", li + 1, o))),
        };
        vars.push(McVar { name: name.to_string(), nominal, tol, dist });
    }
    if vars.is_empty() {
        return Err(err("沒有設定任何變數（每行：名稱 容差 分佈，例如「R1 5% gauss」）".into()));
    }
    Ok(Some(McSpec { runs: runs as usize, seed, vars, el: el.id }))
}

/// samples[run][var]; run 0 is the nominal circuit
pub fn draw_samples(spec: &McSpec) -> Vec<Vec<f64>> {
    let mut rng = Rng::new(spec.seed);
    let mut out = vec![spec.vars.iter().map(|v| v.nominal).collect::<Vec<_>>()];
    for _ in 1..spec.runs {
        out.push(
            spec.vars
                .iter()
                .map(|v| {
                    let d = match v.dist {
                        Dist::Uniform => v.tol * (2.0 * rng.uniform() - 1.0),
                        Dist::Gauss => v.tol / 3.0 * rng.normal(),
                    };
                    v.nominal * (1.0 + d)
                })
                .collect(),
        );
    }
    out
}

fn summary(probe: &str, metric: &str, v: Vec<f64>) -> McStat {
    let n = v.len() as f64;
    let mean = v.iter().sum::<f64>() / n;
    let std = if v.len() > 1 { (v.iter().map(|x| (x - mean) * (x - mean)).sum::<f64>() / (n - 1.0)).sqrt() } else { 0.0 };
    McStat {
        probe: probe.to_string(),
        metric: metric.to_string(),
        mean,
        std,
        min: v.iter().cloned().fold(f64::INFINITY, f64::min),
        max: v.iter().cloned().fold(f64::NEG_INFINITY, f64::max),
        values: v,
    }
}

/// run the analysis (`progress(fraction, runs_done)`)
pub fn run_mc(net: &Netlist, sc: &SimCtrl, spec: &McSpec, threads: usize, progress: &(dyn Fn(f64, f64) + Sync), cancel: &AtomicBool) -> R<TranResult> {
    let samples = draw_samples(spec);
    let cases: Vec<(String, Vec<(String, f64)>)> = samples
        .iter()
        .enumerate()
        .map(|(k, s)| (if k == 0 { "#0 標稱".to_string() } else { format!("#{}", k) }, spec.vars.iter().zip(s).map(|(v, x)| (v.name.clone(), *x)).collect()))
        .collect();
    let desc = spec.vars.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", ");
    let mut res = crate::sweep::run_cases(net, sc, &cases, &desc, threads, progress, cancel)?;
    // statistics per probe across the runs (columns are probe-major: runs consecutive)
    let nv = cases.len();
    let probes = res.sweep.as_ref().map(|s| s.probes).unwrap_or(0);
    let mut stats = Vec::new();
    for j in 0..probes {
        let name = res.names[j * nv].rsplit_once(" [").map(|x| x.0.to_string()).unwrap_or_default();
        let cols = &res.data[j * nv..(j + 1) * nv];
        let fin: Vec<f64> = cols.iter().map(|c| c.last().copied().unwrap_or(f64::NAN)).collect();
        let avg: Vec<f64> = cols.iter().map(|c| if c.is_empty() { f64::NAN } else { c.iter().sum::<f64>() / c.len() as f64 }).collect();
        let mx: Vec<f64> = cols.iter().map(|c| c.iter().cloned().fold(f64::NEG_INFINITY, f64::max)).collect();
        let mn: Vec<f64> = cols.iter().map(|c| c.iter().cloned().fold(f64::INFINITY, f64::min)).collect();
        stats.push(summary(&name, "末值", fin));
        stats.push(summary(&name, "平均", avg));
        stats.push(summary(&name, "最大", mx));
        stats.push(summary(&name, "最小", mn));
    }
    res.mc = Some(McInfo {
        vars: spec.vars.iter().map(|v| v.name.clone()).collect(),
        nominal: spec.vars.iter().map(|v| v.nominal).collect(),
        dists: spec.vars.iter().map(|v| if v.dist == Dist::Gauss { "gauss（σ = 容差/3）".to_string() } else { "uniform".to_string() }).collect(),
        tols: spec.vars.iter().map(|v| v.tol).collect(),
        samples,
        seed: spec.seed,
        stats,
    });
    Ok(res)
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn distributions() {
        let mk = |dist| McSpec { runs: 40001, seed: 7, vars: vec![McVar { name: "x".into(), nominal: 10.0, tol: 0.06, dist }], el: 0 };
        // uniform ±6 %: σ = 0.06/√3 ; Gaussian: σ = 0.06/3
        for (dist, sig) in [(Dist::Uniform, 0.06 / 3f64.sqrt()), (Dist::Gauss, 0.02)] {
            let s = draw_samples(&mk(dist));
            let v: Vec<f64> = s[1..].iter().map(|r| r[0] / 10.0 - 1.0).collect();
            let m = v.iter().sum::<f64>() / v.len() as f64;
            let sd = (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64).sqrt();
            assert!(m.abs() < 3.0 * sig / (v.len() as f64).sqrt() + 1e-12, "{:?} mean {}", dist, m);
            assert!((sd / sig - 1.0).abs() < 0.02, "{:?} std {} vs {}", dist, sd, sig);
            if dist == Dist::Uniform {
                assert!(v.iter().all(|x| x.abs() <= 0.06));
            }
        }
        // reproducible
        assert_eq!(draw_samples(&mk(Dist::Gauss))[5], draw_samples(&mk(Dist::Gauss))[5]);
        assert_eq!(draw_samples(&mk(Dist::Gauss))[0][0], 10.0);
    }
}
