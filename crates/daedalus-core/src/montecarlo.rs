//! Monte Carlo analysis: element parameters ("Ra.R", or just "Ra" for its main value) and
//! parameter-file variables get random tolerances (uniform, or Gaussian truncated at the tolerance),
//! the transient runs once per sample in parallel, and per-probe statistics are collected across the
//! runs. Run #0 is always the nominal circuit. All samples are drawn up front from the seed, so
//! results are reproducible and independent of the number of threads.

use crate::doc::{Doc, Element};
use crate::expr::{self, Syms};
use crate::library::{lib, ElDef, ParamDef};
use crate::netlist::{Netlist, NlError};
use crate::runner::{SimCtrl, TranResult};
use crate::sim::R;
use crate::sweep::Target;
use std::collections::HashSet;
use std::sync::atomic::AtomicBool;

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum Dist {
    /// uniform in nominal·(1 ± tol)
    Uniform,
    /// Gaussian with σ = tol/3 (tol is the 3σ bound), truncated at ± tol
    Gauss,
}
impl Dist {
    /// the keyword written in the tolerance list
    pub fn key(self) -> &'static str {
        match self {
            Dist::Uniform => "uniform",
            Dist::Gauss => "gauss",
        }
    }
}

#[derive(Clone, Debug)]
pub struct McVar {
    /// what the statistics and the CSV call it: "Ra.R" or the parameter-file name
    pub name: String,
    pub target: Target,
    pub nominal: f64,
    /// unit of an element parameter (empty for a variable)
    pub unit: String,
    pub tol: f64,
    pub dist: Dist,
}

#[derive(Clone, Debug)]
pub struct McSpec {
    pub runs: usize,
    pub seed: u64,
    pub vars: Vec<McVar>,
    pub el: i64,
    /// notes for the result (e.g. a parameter-file variable that nothing uses)
    pub warnings: Vec<String>,
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
    pub units: Vec<String>,
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

/// a tolerance as a percentage without float noise: 0.07 → "7"
pub fn fmt_pct(tol: f64) -> String {
    let p = (tol * 100.0 * 1e9).round() / 1e9;
    format!("{}", p)
}

/// "5%" → 0.05, "0.05" → 0.05
pub fn parse_tol(s: &str) -> Option<f64> {
    let s = s.trim();
    let v = if let Some(p) = s.strip_suffix('%') { p.trim().parse::<f64>().ok().map(|v| v / 100.0) } else { s.parse::<f64>().ok() };
    v.filter(|v| v.is_finite())
}

/// the distribution keyword (English or Chinese)
pub fn parse_dist(s: &str) -> Option<Dist> {
    match s.to_lowercase().as_str() {
        "g" | "gauss" | "gaussian" | "normal" | "常態" | "常態分佈" | "常態分布" | "高斯" => Some(Dist::Gauss),
        "u" | "uniform" | "flat" | "均勻" | "均勻分佈" | "均勻分布" => Some(Dist::Uniform),
        _ => None,
    }
}

/// the tokens of one tolerance line (comments removed; "5 %", "±5%", full-width ％ and ， accepted);
/// empty for a blank or comment-only line
pub fn line_tokens(line: &str) -> Vec<String> {
    // comments: "#" or "//" ("%" is the tolerance unit here)
    let line = line.split('#').next().unwrap_or("");
    let line = match line.find("//") {
        Some(i) => &line[..i],
        None => line,
    };
    let mut s = line.replace('％', "%").replace('，', ",").replace('±', " ").replace("+-", " ").replace('\t', " ");
    while s.contains(" %") {
        s = s.replace(" %", "%");
    }
    s.split([' ', ',']).filter(|t| !t.is_empty()).map(|t| t.to_string()).collect()
}

/// one tolerance line: (target, tolerance, distribution); None for a blank or comment-only line
pub fn parse_line(line: &str) -> Option<Result<(String, f64, Dist), String>> {
    let toks = line_tokens(line);
    let name = toks.first()?.clone();
    let tol = match toks.get(1) {
        None => return Some(Err(format!("「{}」後面缺少容差（例如「{} 5%」）", name, name))),
        Some(t) if t == "%" => return Some(Err(format!("{} 的容差是空白的", name))),
        Some(t) => match parse_tol(t) {
            None => return Some(Err(format!("容差「{}」格式錯誤（寫成 5% 或 0.05）", t))),
            Some(v) if !(0.0..1.0).contains(&v) => return Some(Err(format!("容差「{}」超出範圍：必須小於 100 %（5 % 請寫成 5% 或 0.05）", t))),
            Some(v) => v,
        },
    };
    let dist = match toks.get(2) {
        None => Dist::Gauss,
        Some(d) => match parse_dist(d) {
            Some(d) => d,
            None => return Some(Err(format!("不認得的分佈「{}」（gauss 常態 或 uniform 均勻）", d))),
        },
    };
    if let Some(x) = toks.get(3) {
        return Some(Err(format!("多出的內容「{}」（每行：對象 容差 分佈）", x)));
    }
    Some(Ok((name, tol, dist)))
}

/// numeric parameters of an element type that a tolerance can vary
fn num_params(d: &ElDef) -> impl Iterator<Item = &ParamDef> {
    d.params.iter().filter(|p| p.typ.as_deref().unwrap_or("num") == "num")
}
/// the parameter "Ra" alone refers to: the first numeric one shown on the schematic, else the first one
pub fn main_param(d: &ElDef) -> Option<&ParamDef> {
    d.show.iter().filter_map(|k| d.param(k)).find(|p| p.typ.as_deref().unwrap_or("num") == "num").or_else(|| num_params(d).find(|p| !p.adv))
}
/// elements whose parameters a Monte Carlo run can vary (named, simulated)
fn variable_el(e: &Element) -> Option<&'static ElDef> {
    lib().get(&e.typ).filter(|d| !d.noname() && !d.nosim() && !e.name.is_empty())
}
fn find_el<'a>(doc: &'a Doc, name: &str) -> Option<&'a Element> {
    let named = || doc.elements.iter().filter(|e| variable_el(e).is_some());
    named().find(|e| e.name == name).or_else(|| {
        let mut m = named().filter(|e| e.name.eq_ignore_ascii_case(name));
        match (m.next(), m.next()) {
            (Some(e), None) => Some(e),
            _ => None,
        }
    })
}

/// identifiers (lower case) an expression refers to; number literals such as 4.7u or 1e3 are skipped
pub fn idents(s: &str) -> Vec<String> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_ascii_alphabetic() || c[i] == '_' {
            let j0 = i;
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '_') {
                i += 1;
            }
            out.push(c[j0..i].iter().collect::<String>().to_lowercase());
        } else if c[i].is_ascii_digit() || c[i] == '.' {
            while i < c.len() && (c[i].is_ascii_alphanumeric() || c[i] == '.' || c[i] == '_') {
                i += 1;
            }
        } else {
            i += 1;
        }
    }
    out
}

/// parameter-file definitions (name lower case, right-hand side) of the enabled PARAMS elements
fn param_defs(doc: &Doc) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    for e in doc.elements.iter().filter(|e| e.typ == "PARAMS" && !e.dis) {
        for line in e.params.get("text").map(|s| s.as_str()).unwrap_or("").split('\n') {
            let mut l = line.trim_end_matches('\r').to_string();
            if let Some(p) = l.find(['%', '#']) {
                l.truncate(p);
            }
            if let Some(p) = l.find("//") {
                l.truncate(p);
            }
            let l = l.trim();
            let head: String = l.chars().take_while(|c| c.is_ascii_alphanumeric() || *c == '_').collect();
            if head.is_empty() || head.starts_with(|c: char| c.is_ascii_digit()) || head.eq_ignore_ascii_case("limit") {
                continue;
            }
            let rest = l[head.len()..].trim_start().trim_start_matches('=');
            out.push((head.to_lowercase(), head, rest.to_string()));
        }
    }
    out
}

/// parameter-file variables (lower case) that some element depends on, directly or through other variables
fn used_vars(doc: &Doc) -> HashSet<String> {
    let mut used: HashSet<String> = HashSet::new();
    for e in &doc.elements {
        if e.dis || matches!(e.typ.as_str(), "PARAMS" | "MONTECARLO" | "PSWEEP" | "SIMCTRL" | "TEXT") {
            continue;
        }
        for v in e.params.values() {
            used.extend(idents(v));
        }
    }
    let defs = param_defs(doc);
    loop {
        let n = used.len();
        for (k, _, rhs) in &defs {
            if used.contains(k) {
                used.extend(idents(rhs));
            }
        }
        if used.len() == n {
            return used;
        }
    }
}

/// variables (lower case) an expression depends on through the parameter file
fn expr_deps(defs: &[(String, String, String)], ex: &str) -> HashSet<String> {
    let mut d: HashSet<String> = idents(ex).into_iter().collect();
    loop {
        let n = d.len();
        for (k, _, rhs) in defs {
            if d.contains(k) {
                d.extend(idents(rhs));
            }
        }
        if d.len() == n {
            return d;
        }
    }
}

/// what a tolerance line's target refers to
#[derive(Clone, Debug, PartialEq)]
pub struct Resolved {
    pub target: Target,
    /// "Ra.R" or the parameter-file name as written there
    pub label: String,
    pub nominal: f64,
    /// unit of the element parameter (empty for a variable)
    pub unit: String,
    /// the value text it replaces ("1k", "R1", the variable's definition)
    pub expr: String,
}

/// resolve "Ra.R", "Ra" (its main parameter) or a parameter-file variable
pub fn resolve(doc: &Doc, syms: &Syms, name: &str) -> Result<Resolved, String> {
    let (el_name, key) = match name.split_once('.') {
        Some((a, b)) => (a, Some(b)),
        None => (name, None),
    };
    if key.is_none() {
        if let Some(v) = syms.get(&name.to_lowercase()) {
            let defs = param_defs(doc);
            let def = defs.iter().rev().find(|d| d.0 == name.to_lowercase());
            return Ok(Resolved {
                target: Target::Var(name.to_string()),
                label: def.map(|d| d.1.clone()).unwrap_or_else(|| name.to_string()),
                nominal: *v,
                unit: String::new(),
                expr: def.map(|d| d.2.trim().to_string()).unwrap_or_default(),
            });
        }
    }
    let el = match find_el(doc, el_name) {
        Some(e) => e,
        None if key.is_some() => return Err(format!("找不到名稱為「{}」的元件", el_name)),
        None => return Err(format!("找不到「{}」：不是元件名稱，也不是參數檔中的變數", name)),
    };
    if el.dis {
        return Err(format!("元件 {} 已停用", el.name));
    }
    let d = variable_el(el).ok_or_else(|| format!("元件 {} 不能設定容差", el.name))?;
    let list = || num_params(d).map(|p| p.k.as_str()).collect::<Vec<_>>().join("、");
    let p = match key {
        Some(k) => match d.params.iter().find(|p| p.k.eq_ignore_ascii_case(k)) {
            Some(p) if p.typ.as_deref().unwrap_or("num") == "num" => p,
            Some(p) => return Err(format!("{}.{} 不是數值參數（可用：{}）", el.name, p.k, list())),
            None => return Err(format!("元件 {} 沒有參數「{}」（可用：{}）", el.name, k, list())),
        },
        None => main_param(d).ok_or_else(|| format!("元件 {} 沒有可設定容差的數值參數", el.name))?,
    };
    let v = el.params.get(&p.k).cloned().unwrap_or_else(|| p.d.clone());
    let nominal = expr::eval_const(&v, syms).map_err(|e| format!("{}.{} 的值「{}」無法計算：{}", el.name, p.k, v, e.0))?;
    Ok(Resolved { target: Target::El { id: el.id, key: p.k.clone() }, label: format!("{}.{}", el.name, p.k), nominal, unit: p.u.clone(), expr: v.trim().to_string() })
}

/// one non-blank line of the tolerance list, checked
#[derive(Clone, Debug)]
pub struct LineCheck {
    /// 1-based line number in the text
    pub line: usize,
    pub var: Result<McVar, String>,
    pub unit: String,
    pub warn: Option<String>,
}

/// check every line of a tolerance list (also used by the properties dialog while typing)
pub fn check_lines(doc: &Doc, syms: &Syms, text: &str) -> Vec<LineCheck> {
    let defs = param_defs(doc);
    let used = used_vars(doc);
    let mut out: Vec<LineCheck> = Vec::new();
    for (li, line) in text.split('\n').enumerate() {
        let Some(p) = parse_line(line) else { continue };
        let mut unit = String::new();
        let mut warn = None;
        let var = p.and_then(|(name, tol, dist)| {
            let r = resolve(doc, syms, &name)?;
            unit = r.unit.clone();
            if !r.nominal.is_finite() {
                return Err(format!("{} 的標稱值不是有限數", r.label));
            }
            if r.nominal == 0.0 && tol > 0.0 {
                return Err(format!("{} 的標稱值為 0，相對容差不會改變它", r.label));
            }
            if let Target::Var(v) = &r.target {
                if !used.contains(&v.to_lowercase()) {
                    warn = Some(format!("參數檔變數 {} 沒有被任何元件使用，它的變化不會影響模擬（元件參數欄位要填 {}，或改選元件參數）", r.label, r.label));
                }
            }
            Ok(McVar { name: r.label, target: r.target, nominal: r.nominal, unit: r.unit, tol, dist })
        });
        out.push(LineCheck { line: li + 1, var, unit, warn });
    }
    // the same target twice, or an element parameter whose value comes from a listed variable
    for i in 0..out.len() {
        let Ok(vi) = &out[i].var else { continue };
        let mut e = None;
        for j in 0..out.len() {
            let Ok(vj) = &out[j].var else { continue };
            if j < i && vj.target.same(&vi.target) {
                e = Some(format!("{} 重複（前面已經設定過）", vi.name));
                break;
            }
            if let (Target::El { id, key }, Target::Var(var)) = (&vi.target, &vj.target) {
                let ex = doc.elements.iter().find(|x| x.id == *id).and_then(|x| x.params.get(key)).cloned().unwrap_or_default();
                if expr_deps(&defs, &ex).contains(&var.to_lowercase()) {
                    e = Some(format!("{} 的值「{}」來自也在清單中的變數 {}：兩者只能留一個", vi.name, ex.trim(), vj.name));
                    break;
                }
            }
        }
        if let Some(e) = e {
            out[i].var = Err(e);
        }
    }
    out
}

/// a line of the tolerance text, trimmed (for error messages)
fn line_text(text: &str, line: usize) -> String {
    text.split('\n').nth(line.saturating_sub(1)).unwrap_or("").trim().to_string()
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
    let mut warnings = Vec::new();
    for (k, c) in check_lines(doc, syms, &p("vars")).into_iter().enumerate() {
        match c.var {
            Ok(v) => vars.push(v),
            Err(m) => return Err(err(format!("容差設定第 {} 項（{}）：{}", k + 1, line_text(&p("vars"), c.line), m))),
        }
        warnings.extend(c.warn.map(|w| format!("蒙地卡羅：{}", w)));
    }
    if vars.is_empty() {
        return Err(err("還沒有設定任何容差：在蒙地卡羅元件的屬性視窗按「加入」選擇要隨機變化的元件參數（例如 R1.R），填入容差與分佈".into()));
    }
    Ok(Some(McSpec { runs: runs as usize, seed, vars, el: el.id, warnings }))
}

/// candidate targets for the properties dialog: (target text, group, description)
pub fn candidates(doc: &Doc, syms: &Syms) -> Vec<(String, String, String)> {
    let mut out = Vec::new();
    let mut seen = HashSet::new();
    for (k, name, rhs) in param_defs(doc).into_iter().rev() {
        if syms.contains_key(&k) && seen.insert(k) {
            out.push((name, "參數檔變數".to_string(), format!("= {}", rhs.trim())));
        }
    }
    out.reverse();
    let mut els: Vec<&Element> = doc.elements.iter().filter(|e| !e.dis && variable_el(e).is_some()).collect();
    els.sort_by(|a, b| natural_key(&a.name).cmp(&natural_key(&b.name)));
    for e in els {
        let d = variable_el(e).unwrap();
        for p in num_params(d).filter(|p| !p.adv) {
            let v = e.params.get(&p.k).cloned().unwrap_or_else(|| p.d.clone());
            out.push((format!("{}.{}", e.name, p.k), e.name.clone(), format!("{} = {}{}", p.l, v.trim(), if p.u.is_empty() { String::new() } else { format!(" {}", p.u) })));
        }
    }
    out
}
/// "R10" after "R9"
fn natural_key(s: &str) -> (String, u64, String) {
    let head: String = s.chars().take_while(|c| !c.is_ascii_digit()).collect();
    let rest = &s[head.len()..];
    let num: String = rest.chars().take_while(|c| c.is_ascii_digit()).collect();
    (head.to_lowercase(), num.parse().unwrap_or(0), rest[num.len()..].to_string())
}

/// one sample of a relative deviation
fn deviation(rng: &mut Rng, v: &McVar) -> f64 {
    match v.dist {
        Dist::Uniform => v.tol * (2.0 * rng.uniform() - 1.0),
        // truncated at 3σ = tol: a 5 % part never lands outside ± 5 %
        Dist::Gauss => loop {
            let z = rng.normal();
            if z.abs() <= 3.0 {
                break v.tol / 3.0 * z;
            }
        },
    }
}

/// samples[run][var]; run 0 is the nominal circuit
pub fn draw_samples(spec: &McSpec) -> Vec<Vec<f64>> {
    let mut rng = Rng::new(spec.seed);
    let mut out = vec![spec.vars.iter().map(|v| v.nominal).collect::<Vec<_>>()];
    for _ in 1..spec.runs {
        out.push(spec.vars.iter().map(|v| v.nominal * (1.0 + deviation(&mut rng, v))).collect());
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
    let cases: Vec<(String, Vec<(Target, f64)>)> = samples
        .iter()
        .enumerate()
        .map(|(k, s)| (if k == 0 { "#0 標稱".to_string() } else { format!("#{}", k) }, spec.vars.iter().zip(s).map(|(v, x)| (v.target.clone(), *x)).collect()))
        .collect();
    let desc = spec.vars.iter().map(|v| v.name.as_str()).collect::<Vec<_>>().join(", ");
    let mut res = crate::sweep::run_cases(net, sc, &cases, &desc, threads, progress, cancel)?;
    res.warnings = spec.warnings.iter().cloned().chain(std::mem::take(&mut res.warnings)).collect();
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
        units: spec.vars.iter().map(|v| v.unit.clone()).collect(),
        dists: spec.vars.iter().map(|v| if v.dist == Dist::Gauss { "gauss（σ = 容差/3，截斷於 ±容差）".to_string() } else { "uniform".to_string() }).collect(),
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
    fn var(dist: Dist) -> McVar {
        McVar { name: "x".into(), target: Target::Var("x".into()), nominal: 10.0, unit: String::new(), tol: 0.06, dist }
    }
    #[test]
    fn distributions() {
        let mk = |dist| McSpec { runs: 40001, seed: 7, vars: vec![var(dist)], el: 0, warnings: vec![] };
        // uniform ±6 %: σ = 0.06/√3 ; Gaussian truncated at 3σ: σ = 0.02·0.98659
        for (dist, sig) in [(Dist::Uniform, 0.06 / 3f64.sqrt()), (Dist::Gauss, 0.02 * 0.986_58)] {
            let s = draw_samples(&mk(dist));
            let v: Vec<f64> = s[1..].iter().map(|r| r[0] / 10.0 - 1.0).collect();
            let m = v.iter().sum::<f64>() / v.len() as f64;
            let sd = (v.iter().map(|x| (x - m) * (x - m)).sum::<f64>() / (v.len() - 1) as f64).sqrt();
            assert!(m.abs() < 3.0 * sig / (v.len() as f64).sqrt() + 1e-12, "{:?} mean {}", dist, m);
            assert!((sd / sig - 1.0).abs() < 0.015, "{:?} std {} vs {}", dist, sd, sig);
            // both stay inside the tolerance
            assert!(v.iter().all(|x| x.abs() <= 0.06 + 1e-15), "{:?} outside ±6 %", dist);
        }
        // reproducible
        assert_eq!(draw_samples(&mk(Dist::Gauss))[5], draw_samples(&mk(Dist::Gauss))[5]);
        assert_eq!(draw_samples(&mk(Dist::Gauss))[0][0], 10.0);
    }
    #[test]
    fn line_syntax() {
        let ok = |s: &str| parse_line(s).unwrap().unwrap();
        assert_eq!(ok("R1 5% gauss"), ("R1".into(), 0.05, Dist::Gauss));
        assert_eq!(ok("Ra.R 5 % uniform"), ("Ra.R".into(), 0.05, Dist::Uniform));
        assert_eq!(ok("Ra.R ±5％ 均勻"), ("Ra.R".into(), 0.05, Dist::Uniform));
        assert_eq!(ok("Ra.R, 0.1, 常態 # note"), ("Ra.R".into(), 0.1, Dist::Gauss));
        assert_eq!(ok("C1 10%"), ("C1".into(), 0.1, Dist::Gauss));
        assert!(parse_line("  # only a comment").is_none());
        assert!(parse_line("R1").unwrap().is_err());
        assert!(parse_line("R1 5").unwrap().unwrap_err().contains("100 %"));
        assert!(parse_line("R1 5% weird").unwrap().is_err());
        assert_eq!(fmt_pct(0.07), "7");
        assert_eq!(fmt_pct(0.125), "12.5");
        assert_eq!(idents("2*Rb + 4.7u/x_1 - 1e3"), vec!["rb", "x_1"]);
    }
    #[test]
    fn targets() {
        let doc: Doc = serde_json::from_str(
            r#"{"elements":[
            {"id":1,"type":"R","name":"Ra","params":{"R":"1k"}},
            {"id":2,"type":"R","name":"Rb","params":{"R":"Rx"}},
            {"id":3,"type":"C","name":"C1","params":{"C":"2*Cx","v0":"0"}},
            {"id":4,"type":"PARAMS","params":{"text":"Rx = 2k\nCx = 0.5u\nUnused = 3"}},
            {"id":5,"type":"R","name":"Rd","dis":true,"params":{"R":"1"}}],"wires":[],"nid":6}"#,
        )
        .unwrap();
        let syms = crate::netlist::doc_syms(&doc);
        let r = resolve(&doc, &syms, "Ra.R").unwrap();
        assert_eq!((r.target.clone(), r.nominal, r.label.as_str()), (Target::El { id: 1, key: "R".into() }, 1000.0, "Ra.R"));
        // the element name alone is its main value; "c1" finds C1 when no variable is called that
        assert_eq!(resolve(&doc, &syms, "c1").unwrap().nominal, 1e-6);
        assert_eq!(resolve(&doc, &syms, "Rb").unwrap().nominal, 2000.0);
        assert_eq!(resolve(&doc, &syms, "rx").unwrap().target, Target::Var("rx".into()));
        assert!(resolve(&doc, &syms, "Ra.L").unwrap_err().contains("沒有參數"));
        assert!(resolve(&doc, &syms, "Zz").is_err());
        assert!(resolve(&doc, &syms, "Rd.R").unwrap_err().contains("停用"));
        let c = check_lines(&doc, &syms, "Ra.R 5%\nRb 1% uniform\n\nCx 10%\nUnused 1%\nra.r 2%");
        assert_eq!(c.len(), 5);
        assert_eq!(c[3].line, 5);
        assert!(c[0].var.is_ok() && c[1].var.is_ok() && c[2].var.is_ok() && c[2].warn.is_none());
        assert!(c[3].warn.as_deref().unwrap_or("").contains("沒有被任何元件使用"));
        assert!(c[4].var.as_ref().unwrap_err().contains("重複"));
        // Rb.R = Rx and Rx is listed too
        let c = check_lines(&doc, &syms, "Rx 3%\nRb.R 1%");
        assert!(c[1].var.as_ref().unwrap_err().contains("只能留一個"));
        let names: Vec<String> = candidates(&doc, &syms).into_iter().map(|c| c.0).collect();
        assert_eq!(names, vec!["Rx", "Cx", "Unused", "C1.C", "C1.v0", "Ra.R", "Rb.R"]);
    }
}
