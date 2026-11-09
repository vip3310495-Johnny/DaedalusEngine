//! The simulator: MNA transient solver (trapezoidal + backward-Euler damping),
//! PWL switch iteration with switching-instant interpolation, breakpoint-aligned
//! sub-stepping, local-truncation-error step refinement, control-block scheduler.

use crate::expr::{self, Syms};
use crate::netlist::Netlist;
use crate::sparse::{Lu, Pattern, Sym};
use crate::util::{js_to_exponential, js_to_precision};
use std::collections::{HashMap, HashSet};
use std::sync::Arc;

pub const NAN: f64 = f64::NAN;

/* ------------------------------------------------------------------ errors */
#[derive(Debug, Clone)]
pub struct SimError {
    pub msg: String,
    pub el: Option<i64>,
}
impl SimError {
    pub fn new(msg: impl Into<String>) -> Self {
        SimError { msg: msg.into(), el: None }
    }
    pub fn el(msg: impl Into<String>, id: i64) -> Self {
        SimError { msg: msg.into(), el: Some(id) }
    }
}
impl std::fmt::Display for SimError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(&self.msg)
    }
}
impl std::error::Error for SimError {}
pub type R<T> = Result<T, SimError>;

/* ------------------------------------------------------------------ matrix */
#[derive(Clone)]
pub struct Mat {
    pub n: usize,
    pub a: Vec<f64>,
    pub pat: Pattern,
}
impl Mat {
    pub fn new(n: usize) -> Self {
        Mat { n, a: vec![0.0; n * n], pat: Pattern::new(n) }
    }
    #[inline]
    pub fn a(&mut self, i: i32, j: i32, v: f64) {
        if i >= 0 && j >= 0 {
            let k = i as usize * self.n + j as usize;
            self.a[k] += v;
            if self.pat.mask[k] == 0 {
                self.pat.add(k);
            }
        }
    }
    #[inline]
    pub fn g(&mut self, a: i32, b: i32, g: f64) {
        self.a(a, a, g);
        self.a(b, b, g);
        self.a(a, b, -g);
        self.a(b, a, -g);
    }
    /// branch variable k with current flowing a -> b through the branch
    #[inline]
    pub fn br(&mut self, a: i32, b: i32, k: i32) {
        self.a(a, k, 1.0);
        self.a(b, k, -1.0);
    }
    /// branch equation c*(v_a - v_b) in row k
    #[inline]
    pub fn v(&mut self, k: i32, a: i32, b: i32, c: f64) {
        self.a(k, a, c);
        self.a(k, b, -c);
    }
}
#[inline]
pub fn ab(b: &mut [f64], i: i32, v: f64) {
    if i >= 0 {
        b[i as usize] += v;
    }
}
/// current source of value I flowing from node a through the element to node k
#[inline]
pub fn ai(b: &mut [f64], a: i32, k: i32, i: f64) {
    if a >= 0 {
        b[a as usize] -= i;
    }
    if k >= 0 {
        b[k as usize] += i;
    }
}
#[inline]
pub fn vx(x: &[f64], i: i32) -> f64 {
    if i >= 0 {
        x[i as usize]
    } else {
        0.0
    }
}

/* ------------------------------------------------------------------ AC context */
pub struct AcCtx {
    pub nn: usize,
    pub n: usize,
    pub w: f64,
    pub ar: Vec<f64>,
    pub ai: Vec<f64>,
    pub br: Vec<f64>,
    pub bi: Vec<f64>,
}
impl AcCtx {
    #[inline]
    pub fn a(&mut self, i: i32, j: i32, re: f64, im: f64) {
        if i >= 0 && j >= 0 {
            let k = i as usize * self.nn + j as usize;
            self.ar[k] += re;
            self.ai[k] += im;
        }
    }
    #[inline]
    pub fn b(&mut self, i: i32, re: f64, im: f64) {
        if i >= 0 {
            self.br[i as usize] += re;
            self.bi[i as usize] += im;
        }
    }
    pub fn g(&mut self, a: i32, b: i32, re: f64, im: f64) {
        self.a(a, a, re, im);
        self.a(b, b, re, im);
        self.a(a, b, -re, -im);
        self.a(b, a, -re, -im);
    }
    /// control node index in the complex system
    #[inline]
    pub fn ci(&self, c: i32) -> i32 {
        if c > 0 {
            self.n as i32 + c - 1
        } else {
            -1
        }
    }
}

/* ------------------------------------------------------------------ shared engine state */
#[derive(Clone)]
pub struct E {
    pub ps: Vec<f64>,
    pub cs: Vec<f64>,
    pub scratch: Vec<f64>,
    pub x_p: Vec<f64>,
    pub cv: Vec<f64>,
    pub cv_p: Vec<f64>,
    pub cev: Vec<f64>,
    pub cvec: Vec<Option<Vec<f64>>>,
    pub dt: f64,
    pub eps_t: f64,
    pub warnings: Vec<String>,
    pub init_currents: Option<Vec<f64>>,
    pub gate_freq: Vec<f64>,
    pub n_node: usize,
    /// set by an element whose algebraic relation changed slope at the accepted step: next step uses BE
    pub damp_req: bool,
}
impl E {
    pub fn warn(&mut self, msg: String) {
        if self.warnings.len() < 200 && !self.warnings.contains(&msg) {
            self.warnings.push(msg);
        }
    }
}

/// Injection parameters for a sine source used as AC-sweep excitation.
#[derive(Clone, Copy, Debug)]
pub struct Inj {
    pub a: f64,
    pub f: f64,
    pub t0: f64,
    pub tr: f64,
}

#[derive(Clone, Copy, Default, Debug)]
pub struct Caps {
    pub p: bool,
    pub c: bool,
    pub sw: bool,
    pub nl: bool,
    pub nl_init: bool,
    pub dyn_: bool,
    pub timed: bool,
    pub probe: bool,
    pub ac_probe: bool,
    pub nb: bool,
    pub jump: bool,
    pub ac: bool,
    pub qq: bool,
    pub init_g: bool,
    pub init_b: bool,
}

/// Information a device probe needs about a non-ideal switch.
#[derive(Clone, Debug)]
pub struct DevInfo {
    pub g: i32,
    pub d: i32,
    pub s: i32,
    pub amm_k: i32, // var ordinal (fixed later)
    pub drv_so: usize,
    pub drv_gc: f64,
    pub drv_von: f64,
    pub drv_voff: f64,
}

/// A model instance. Every method has a no-op default; `caps()` says which roles it plays.
#[allow(unused_variables)]
pub trait Inst: Send + Sync {
    fn clone_box(&self) -> Box<dyn Inst>;
    fn caps(&self) -> Caps;
    // ---- construction / linking
    fn setup(&mut self, b: &mut Builder) {}
    fn fix_vars(&mut self, n_node: i32) {}
    fn lte_skip(&self) -> Vec<i32> {
        vec![]
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![]
    }
    fn ins(&self) -> Vec<i32> {
        vec![]
    }
    fn outs(&self) -> Vec<i32> {
        vec![]
    }
    fn ft(&self) -> bool {
        true
    }
    fn cins(&self) -> Vec<i32> {
        vec![]
    }
    fn gate_freq(&self) -> f64 {
        0.0
    }
    fn is_rlc(&self) -> bool {
        false
    }
    fn set_want_i(&mut self) {}
    fn branch_k(&self) -> i32 {
        -1
    }
    /// (net of pin 2, net of pin 3) for current-controlled sources referencing a branch
    fn link_rlc(&self) -> Option<(i64, i64)> {
        None
    }
    fn set_link(&mut self, target: usize, sg: f64) {}
    fn link_target(&self) -> Option<usize> {
        None
    }
    fn set_ref_k(&mut self, k: i32) {}
    fn dev_target(&self) -> Option<String> {
        None
    }
    fn set_dev(&mut self, d: DevInfo) {}
    // ---- power element
    fn init(&mut self, e: &mut E) -> R<()> {
        Ok(())
    }
    fn stamp_g(&self, m: &mut Mat, h: f64, be: bool, e: &E) {}
    fn stamp_b(&self, b: &mut [f64], t: f64, h: f64, be: bool, e: &E) {}
    fn stamp_init_g(&self, m: &mut Mat, relax: bool, e: &E) {}
    fn stamp_init_b(&self, b: &mut [f64], relax: bool, e: &E) {}
    fn init_var(&self) -> bool {
        false
    }
    fn set_iv(&mut self, iv: i32) {}
    /// inductor treated as a DC short in the relaxed t = 0 pass: its branch variable
    fn relaxed_short(&self) -> Option<i32> {
        None
    }
    fn accept(&mut self, x: &[f64], t: f64, h: f64, be: bool, e: &mut E) {}
    fn accept_init(&mut self, x: &[f64], e: &mut E) {}
    fn finish(&mut self, t: f64, e: &mut E) {}
    // ---- switches
    fn state(&self, e: &E) -> i32 {
        0
    }
    fn set_state(&mut self, v: i32, e: &mut E) {}
    fn skey(&self, e: &E) -> u32 {
        0
    }
    fn desired(&self, x: &[f64], t: f64, e: &E) -> i32 {
        0
    }
    fn tau(&self, xa: &[f64], xb: &[f64], ns: i32, e: &E) -> f64 {
        NAN
    }
    fn qq(&self, xa: &[f64], xb: &[f64], ns: i32, e: &E) -> Option<(f64, f64)> {
        None
    }
    // ---- gated (switch or gate driver driven by a control node)
    fn gate_node(&self) -> i32 {
        -1
    }
    fn so(&self) -> usize {
        0
    }
    fn on_gate(&mut self, g: i32, init: bool, e: &mut E) -> bool {
        false
    }
    // ---- time-scheduled switch
    fn state_at(&self, t: f64) -> i32 {
        0
    }
    // ---- nonlinear (Newton)
    fn nl_guess(&mut self, x: &[f64], xr: &[f64], e: &mut E) {}
    #[allow(clippy::too_many_arguments)]
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], t: f64, h: f64, be: bool, init: bool, e: &mut E) {}
    fn nl_ok(&self) -> bool {
        true
    }
    // ---- breakpoints
    fn next_break(&self, t: f64, e: &E) -> f64 {
        f64::INFINITY
    }
    fn jump_at(&self, t: f64, e: &E) -> bool {
        false
    }
    // ---- control block
    fn eval(&mut self, t: f64, h: f64, e: &mut E, x: &[f64]) -> R<()> {
        Ok(())
    }
    fn commit(&mut self, t: f64, h: f64, e: &mut E) {}
    // ---- probes
    fn pname(&self) -> String {
        String::new()
    }
    fn value(&self, e: &E, x: &[f64]) -> f64 {
        0.0
    }
    fn ac_vals(&self, e: &E, x: &[f64]) -> (f64, f64) {
        (0.0, 0.0)
    }
    fn ac_loop(&self) -> bool {
        false
    }
    fn ac_neg(&self) -> bool {
        false
    }
    fn ac_phasor(&self, xr: &[f64], xi: &[f64], n: usize) -> [f64; 4] {
        [0.0; 4]
    }
    // ---- sources
    fn src_name(&self) -> Option<String> {
        None
    }
    fn can_inject(&self) -> bool {
        false
    }
    fn set_inj(&mut self, inj: Inj) {}
    // ---- linear AC
    fn stamp_ac(&self, c: &mut AcCtx, is_src: bool, e: &E) -> R<()> {
        Ok(())
    }
}
impl Clone for Box<dyn Inst> {
    fn clone(&self) -> Self {
        self.clone_box()
    }
}

/// Implements `clone_box` for a `Clone` type.
#[macro_export]
macro_rules! clone_box {
    () => {
        fn clone_box(&self) -> Box<dyn $crate::sim::Inst> {
            Box::new(self.clone())
        }
    };
}

/* ------------------------------------------------------------------ builder */
pub struct Builder {
    pub n_node: i32,
    pub n_c: i32,
    pub n_var: i32,
    pub ps_len: usize,
    pub cs_len: usize,
    pub scratch_len: usize,
    pub dt: f64,
    pub dev_want: HashSet<String>,
    pub devs: HashMap<String, DevInfo>,
    pub warnings: Vec<String>,
}
impl Builder {
    pub fn node(&mut self) -> i32 {
        let n = self.n_node;
        self.n_node += 1;
        n
    }
    pub fn cnode(&mut self) -> i32 {
        let n = self.n_c;
        self.n_c += 1;
        n
    }
    /// branch variable request; the returned ordinal becomes n_node + ordinal in fix_vars
    pub fn var(&mut self) -> i32 {
        let r = self.n_var;
        self.n_var += 1;
        r
    }
    pub fn req_p(&mut self, len: usize) -> usize {
        let o = self.ps_len;
        self.ps_len += len;
        o
    }
    pub fn req_c(&mut self, len: usize) -> usize {
        let o = self.cs_len;
        self.cs_len += len;
        o
    }
    pub fn req_scratch(&mut self) -> usize {
        let o = self.scratch_len;
        self.scratch_len += 1;
        o
    }
    pub fn warn(&mut self, msg: String) {
        if self.warnings.len() < 200 && !self.warnings.contains(&msg) {
            self.warnings.push(msg);
        }
    }
}

/* ------------------------------------------------------------------ element context */
#[derive(Clone, Copy, Debug)]
pub struct PinInfo {
    pub net: i64,
    pub kind: char, // 'P', 'C', 'G', 'N'
    pub p: i32,
    pub c: i32,
}

/// Input descriptor: power node, control node or ground.
#[derive(Clone, Copy, Debug)]
pub enum InD {
    P(i32),
    C(i32),
    G,
}
impl InD {
    #[inline]
    pub fn val(&self, e: &E, x: &[f64]) -> f64 {
        match *self {
            InD::C(c) => e.cv[c as usize],
            InD::P(p) => vx(x, p),
            InD::G => 0.0,
        }
    }
    pub fn c(&self) -> Option<i32> {
        if let InD::C(c) = *self {
            Some(c)
        } else {
            None
        }
    }
    pub fn p(&self) -> Option<i32> {
        if let InD::P(p) = *self {
            Some(p)
        } else {
            None
        }
    }
}

pub struct ElCtx<'a> {
    pub b: &'a mut Builder,
    pub id: i64,
    pub name: String,
    pub typ: String,
    pub params: &'a HashMap<String, String>,
    pub pins: Vec<PinInfo>,
    pub net_pins: Vec<i64>,
    pub syms: &'a Syms,
}

impl<'a> ElCtx<'a> {
    pub fn err(&self, msg: impl AsRef<str>) -> SimError {
        SimError::el(format!("{}：{}", self.name, msg.as_ref()), self.id)
    }
    fn pin(&self, i: usize) -> Option<&PinInfo> {
        self.pins.get(i)
    }
    /// power node of pin i (must be power or ground)
    pub fn p(&self, i: usize) -> R<i32> {
        match self.pin(i) {
            None => Err(self.err(format!("第 {} 個接腳未連接", i + 1))),
            Some(pn) if pn.kind == 'N' => Err(self.err(format!("第 {} 個接腳未連接", i + 1))),
            Some(pn) if pn.kind == 'C' => Err(self.err("此接腳必須接到功率電路，卻接到控制電路")),
            Some(pn) => Ok(pn.p),
        }
    }
    /// control node of pin i (control or ground)
    pub fn c(&self, i: usize) -> R<i32> {
        match self.pin(i) {
            None => Err(self.err(format!("第 {} 個接腳未連接", i + 1))),
            Some(pn) if pn.kind == 'N' => Err(self.err(format!("第 {} 個接腳未連接", i + 1))),
            Some(pn) if pn.kind == 'P' => Err(self.err("此接腳必須接到控制電路，卻接到功率電路")),
            Some(pn) => Ok(pn.c),
        }
    }
    /// optional control pin: -1 when unconnected
    pub fn c_opt(&self, i: usize) -> R<i32> {
        match self.pin(i) {
            None => Ok(-1),
            Some(pn) if pn.kind == 'N' => Ok(-1),
            _ => self.c(i),
        }
    }
    pub fn kind(&self, i: usize) -> char {
        self.pin(i).map(|p| p.kind).unwrap_or('N')
    }
    pub fn is_ctl(&self, i: usize) -> bool {
        self.kind(i) == 'C'
    }
    pub fn in_desc(&self, i: usize) -> R<InD> {
        match self.kind(i) {
            'N' => Err(self.err("控制輸入端未連接")),
            'C' => Ok(InD::C(self.c(i)?)),
            'G' => Ok(InD::G),
            _ => Ok(InD::P(self.p(i)?)),
        }
    }
    // ---- parameters
    fn get(&self, k: &str) -> Option<&String> {
        self.params.get(k)
    }
    pub fn raw(&self, k: &str, d: &str) -> String {
        match self.get(k) {
            Some(v) if !v.is_empty() => v.clone(),
            _ => d.to_string(),
        }
    }
    pub fn raw_opt(&self, k: &str) -> Option<String> {
        self.get(k).cloned()
    }
    pub fn num_opt(&self, k: &str, d: Option<f64>) -> R<f64> {
        let v = self.get(k);
        match v {
            Some(s) if !s.trim().is_empty() => expr::eval_const(s, self.syms).map_err(|e| SimError::el(format!("{}：參數 {} = \"{}\" {}", self.name, k, s, e.0), self.id)),
            _ => match d {
                Some(d) => Ok(d),
                None => Err(SimError::el(format!("{}：參數 {} 未設定", self.name, k), self.id)),
            },
        }
    }
    pub fn num(&self, k: &str) -> R<f64> {
        self.num_opt(k, None)
    }
    pub fn numd(&self, k: &str, d: f64) -> R<f64> {
        self.num_opt(k, Some(d))
    }
    pub fn int(&self, k: &str, d: f64) -> R<i64> {
        Ok(expr::js_round(self.numd(k, d)?) as i64)
    }
    pub fn int_req(&self, k: &str) -> R<i64> {
        Ok(expr::js_round(self.num(k)?) as i64)
    }
    fn pos_check(&self, k: &str, v: f64) -> R<f64> {
        if !(v > 0.0) {
            return Err(SimError::el(format!("{}：參數 {} 必須大於 0", self.name, k), self.id));
        }
        Ok(v)
    }
    pub fn pos(&self, k: &str) -> R<f64> {
        let v = self.num(k)?;
        self.pos_check(k, v)
    }
    pub fn posd(&self, k: &str, d: f64) -> R<f64> {
        let v = self.numd(k, d)?;
        self.pos_check(k, v)
    }
    pub fn list_opt(&self, k: &str, d: Option<Vec<f64>>) -> R<Vec<f64>> {
        match self.get(k) {
            Some(s) if !s.trim().is_empty() => expr::eval_list(s, self.syms).map_err(|e| SimError::el(format!("{}：參數 {} {}", self.name, k, e.0), self.id)),
            _ => match d {
                Some(d) => Ok(d),
                None => Err(SimError::el(format!("{}：參數 {} 未設定", self.name, k), self.id)),
            },
        }
    }
    pub fn list(&self, k: &str) -> R<Vec<f64>> {
        self.list_opt(k, None)
    }
    pub fn pairs(&self, k: &str) -> R<Vec<(f64, f64)>> {
        let s = self.get(k).cloned().unwrap_or_default();
        expr::eval_pairs(&s, self.syms).map_err(|e| SimError::el(format!("{}：參數 {} {}", self.name, k, e.0), self.id))
    }
    pub fn func(&self, k: &str, args: &[&str]) -> R<expr::CompiledFn> {
        match self.get(k) {
            Some(s) if !s.trim().is_empty() => expr::compile_fn(s, args, self.syms).map_err(|e| SimError::el(format!("{}：運算式 \"{}\" {}", self.name, s, e.0), self.id)),
            _ => Err(SimError::el(format!("{}：運算式 {} 未設定", self.name, k), self.id)),
        }
    }
}

/* ------------------------------------------------------------------ options */
#[derive(Clone, Debug)]
pub struct SimOpts {
    pub dt: f64,
    pub interp: bool,
    pub damp: bool,
    pub refine: bool,
    pub lte: bool,
    pub lte_tol: f64,
    pub lte_abs_v: f64,
    pub lte_abs_i: f64,
    pub lte_max_depth: u32,
    pub be_split: bool,
    pub refine_evt: bool,
    /// stop time of the run (0 = unknown); keeps the event-time tolerance above double-precision resolution
    pub opt_t_end: f64,
}
impl SimOpts {
    pub fn new(dt: f64) -> Self {
        SimOpts { dt, interp: true, damp: true, refine: true, lte: true, lte_tol: 1e-3, lte_abs_v: 1e-6, lte_abs_i: 1e-9, lte_max_depth: 4, be_split: true, refine_evt: true, opt_t_end: 0.0 }
    }
    /// time tolerance for coincident events: 1e-6 of the step, but never below a few ulps of the largest time
    pub fn eps_t(&self) -> f64 {
        (self.dt * 1e-6).max(self.opt_t_end * 8.0 * f64::EPSILON)
    }
}

#[derive(Clone)]
struct CGroup {
    blks: Vec<usize>,
    lp: bool,
    outs: Vec<i32>,
}

#[derive(Clone, Default)]
struct Snap {
    ps: Vec<f64>,
    x: Vec<f64>,
    xp: Vec<f64>,
    xpp: Vec<f64>,
    xppp: Vec<f64>,
    x4: Vec<f64>,
    cv: Vec<f64>,
    be: bool,
    fs: bool,
    topo: bool,
    hp: f64,
    hpp: f64,
    hppp: f64,
    hist_n: u32,
    last_be: bool,
    pred_ok: bool,
}

#[derive(Clone)]
struct Change {
    m: usize,
    ns: i32,
    tau: f64,
}

type Key = Vec<u64>;

#[derive(Clone)]
pub struct Sim {
    pub o: SimOpts,
    pub insts: Vec<Box<dyn Inst>>,
    pub inst_el: Vec<usize>,
    pub el_names: Vec<String>,
    pub el_ids: Vec<i64>,
    pub e: E,
    pub p_els: Vec<usize>,
    pub c_blks: Vec<usize>,
    sw_els: Vec<usize>,
    gated: Vec<usize>,
    nl_els: Vec<usize>,
    timed_sw: Vec<usize>,
    dyn_els: Vec<usize>,
    bp_els: Vec<usize>,
    pub probes: Vec<usize>,
    pub ac_probes: Vec<usize>,
    pub sources: HashMap<String, usize>,
    caps: Vec<Caps>,
    pub n_node: usize,
    pub n_ext_node: usize,
    pub n: usize,
    pub n_c: usize,
    ties: Vec<i32>,
    m: Mat,
    b: Vec<f64>,
    b0: Vec<f64>,
    pub x: Vec<f64>,
    x_pp: Vec<f64>,
    x_ppp: Vec<f64>,
    x4: Vec<f64>,
    x_t: Vec<f64>,
    r: Vec<f64>,
    d: Vec<f64>,
    x_scale: Vec<f64>,
    lte_w: Vec<f64>,
    sc_v: f64,
    sc_i: f64,
    h_p: f64,
    h_pp: f64,
    h_ppp: f64,
    hist_n: u32,
    last_be: bool,
    pred_ok: bool,
    first_step: bool,
    topo: bool,
    be: bool,
    has_nl: bool,
    has_dyn: bool,
    lin_cache: HashMap<Key, Vec<f64>>,
    cache: HashMap<Key, (Lu, u64)>,
    cache_tick: u64,
    sym: Option<Arc<Sym>>,
    nlu: Option<Lu>,
    sw_ver: u64,
    kkey: Option<(u64, bool, u64, Key)>,
    pub t: f64,
    pub events: u64,
    pub non_conv: u64,
    pub evt_refined: u64,
    pub refined: u64,
    pub refine_max: u64,
    pub steps: u64,
    lte_depth: u32,
    pub drv: Vec<i64>,
    c_groups: Vec<CGroup>,
    snaps: Vec<Snap>,
    nan_seen: bool,
}

fn singular_msg(row: i64) -> String {
    let mut s = "電路矩陣奇異（無法求解）。常見原因：電壓源或電感形成迴路、電流源串聯、節點浮接或元件短路。".to_string();
    if row >= 0 {
        s += &format!("（第 {} 列）", row);
    }
    s
}

fn solve_lin(lu: &mut Lu, b: &[f64], x: &mut [f64], r: &mut [f64], d: &mut [f64], refine: bool) {
    lu.solve(b, x);
    if refine {
        // one step of iterative refinement (sparse residual on the kept matrix values)
        if lu.residual(b, x, r) {
            lu.solve(r, d);
            for i in 0..x.len().min(d.len()) {
                x[i] += d[i];
            }
        }
    }
}

impl Sim {
    /* ---------- construction ---------- */
    pub fn new(net: &Netlist, o: SimOpts) -> R<Sim> {
        let (syms, _) = expr::parse_param_text(&net.param_text);
        let nn = net.nets.len();
        let mut pnode = vec![-1i32; nn];
        let mut cnode = vec![-1i32; nn];
        let mut b = Builder { n_node: 0, n_c: 1, n_var: 0, ps_len: 0, cs_len: 0, scratch_len: 0, dt: o.dt, dev_want: HashSet::new(), devs: HashMap::new(), warnings: Vec::new() };
        for (i, k) in net.nets.iter().enumerate() {
            match k {
                'P' => pnode[i] = b.node(),
                'C' => cnode[i] = b.cnode(),
                'G' => {
                    pnode[i] = -1;
                    cnode[i] = 0;
                }
                _ => {}
            }
        }
        let n_ext_node = b.n_node as usize;
        for el in &net.elements {
            if el.typ == "DEVP" {
                b.dev_want.insert(el.params.get("el").cloned().unwrap_or_default().trim().to_string());
            }
        }
        let mut insts: Vec<Box<dyn Inst>> = Vec::new();
        let mut inst_el = Vec::new();
        for (ei, el) in net.elements.iter().enumerate() {
            let pins: Vec<PinInfo> = el
                .pins
                .iter()
                .map(|&ni| {
                    if ni < 0 {
                        PinInfo { net: ni, kind: 'N', p: -1, c: -1 }
                    } else {
                        let u = ni as usize;
                        PinInfo { net: ni, kind: net.nets[u], p: pnode[u], c: cnode[u] }
                    }
                })
                .collect();
            let mut ctx = ElCtx { b: &mut b, id: el.id, name: el.name.clone(), typ: el.typ.clone(), params: &el.params, pins, net_pins: el.pins.clone(), syms: &syms };
            if let Some(list) = crate::models::make(&mut ctx)? {
                for m in list {
                    insts.push(m);
                    inst_el.push(ei);
                }
            }
        }
        let el_names: Vec<String> = net.elements.iter().map(|e| e.name.clone()).collect();
        let el_ids: Vec<i64> = net.elements.iter().map(|e| e.id).collect();
        // ---- links (current-controlled sources referencing an R/L/C branch; device probes)
        for i in 0..insts.len() {
            if let Some((n2, n3)) = insts[i].link_rlc() {
                let my_el = inst_el[i];
                let mut found = None;
                for j in 0..insts.len() {
                    if j == i || inst_el[j] == my_el || !insts[j].is_rlc() {
                        continue;
                    }
                    let el = &net.elements[inst_el[j]];
                    if !["R", "L", "C", "RL", "RC", "LC", "RLC"].contains(&el.typ.as_str()) {
                        continue;
                    }
                    let pn = &el.pins;
                    // every ground symbol is its own net index, but all of them are the same node
                    let gnd = |k: i64| k >= 0 && net.nets.get(k as usize).map(|c| *c == 'G').unwrap_or(false);
                    let same = |a: i64, b: i64| a == b || (gnd(a) && gnd(b));
                    if pn.len() >= 2 && same(pn[0], n2) && same(pn[1], n3) {
                        found = Some((j, 1.0));
                        break;
                    }
                    if pn.len() >= 2 && same(pn[0], n3) && same(pn[1], n2) {
                        found = Some((j, -1.0));
                        break;
                    }
                }
                let el = &net.elements[my_el];
                match found {
                    None => return Err(SimError::el(format!("{}：控制端必須跨接在一個 R/L/C 支路兩端（以量測該支路電流）", el.name), el.id)),
                    Some((j, sg)) => {
                        insts[j].set_want_i();
                        insts[i].set_link(j, sg);
                    }
                }
            }
            if let Some(tg) = insts[i].dev_target() {
                let el = &net.elements[inst_el[i]];
                match b.devs.get(&tg) {
                    None => return Err(SimError::el(format!("{}：找不到名為「{}」的非理想開關（MOSNI / PMOSNI / IGBTNI）", el.name, tg), el.id)),
                    Some(d) => {
                        let d = d.clone();
                        insts[i].set_dev(d);
                    }
                }
            }
        }
        for m in insts.iter_mut() {
            m.setup(&mut b);
        }
        let caps: Vec<Caps> = insts.iter().map(|m| m.caps()).collect();
        let mut s = Sim {
            o: o.clone(),
            insts,
            inst_el,
            el_names,
            el_ids,
            e: E { ps: vec![], cs: vec![], scratch: vec![], x_p: vec![], cv: vec![], cv_p: vec![], cev: vec![], cvec: vec![], dt: o.dt, eps_t: o.eps_t(), warnings: vec![], init_currents: None, gate_freq: vec![], n_node: 0, damp_req: false },
            p_els: vec![],
            c_blks: vec![],
            sw_els: vec![],
            gated: vec![],
            nl_els: vec![],
            timed_sw: vec![],
            dyn_els: vec![],
            bp_els: vec![],
            probes: vec![],
            ac_probes: vec![],
            sources: HashMap::new(),
            caps: caps.clone(),
            n_node: 0,
            n_ext_node,
            n: 0,
            n_c: 0,
            ties: vec![],
            m: Mat::new(0),
            b: vec![],
            b0: vec![],
            x: vec![],
            x_pp: vec![],
            x_ppp: vec![],
            x4: vec![],
            x_t: vec![],
            r: vec![],
            d: vec![],
            x_scale: vec![],
            lte_w: vec![],
            sc_v: 0.0,
            sc_i: 0.0,
            h_p: 0.0,
            h_pp: 0.0,
            h_ppp: 0.0,
            hist_n: 0,
            last_be: true,
            pred_ok: false,
            first_step: false,
            topo: false,
            be: true,
            has_nl: false,
            has_dyn: false,
            lin_cache: HashMap::new(),
            cache: HashMap::new(),
            cache_tick: 0,
            sym: None,
            nlu: None,
            sw_ver: 0,
            kkey: None,
            t: 0.0,
            events: 0,
            non_conv: 0,
            evt_refined: 0,
            refined: 0,
            refine_max: 0,
            steps: 0,
            lte_depth: 0,
            drv: vec![],
            c_groups: vec![],
            snaps: vec![],
            nan_seen: false,
        };
        for (i, c) in caps.iter().enumerate() {
            if c.p {
                s.p_els.push(i);
            }
            if c.c {
                s.c_blks.push(i);
            }
            if c.sw {
                s.sw_els.push(i);
            }
            if s.insts[i].gate_node() >= 0 {
                s.gated.push(i);
            }
            if c.nl {
                s.nl_els.push(i);
            }
            if c.timed {
                s.timed_sw.push(i);
            }
            if c.dyn_ {
                s.dyn_els.push(i);
            }
            if c.nb {
                s.bp_els.push(i);
            }
            if c.probe {
                s.probes.push(i);
            }
            if c.ac_probe {
                s.ac_probes.push(i);
            }
            if let Some(nm) = s.insts[i].src_name() {
                s.sources.insert(nm.to_lowercase(), i);
            }
        }
        // ---- variables: resolved after all nodes are known
        let n_node = b.n_node;
        for m in s.insts.iter_mut() {
            m.fix_vars(n_node);
        }
        for i in 0..s.insts.len() {
            if let Some(j) = s.insts[i].link_target() {
                let k = s.insts[j].branch_k();
                s.insts[i].set_ref_k(k);
            }
        }
        let mut lte_skip = Vec::new();
        for m in &s.insts {
            lte_skip.extend(m.lte_skip());
        }
        // ---- ground islands: union-find over power nodes
        let nnode = n_node as usize;
        let mut par: Vec<usize> = (0..=nnode).collect();
        fn find(par: &mut [usize], mut i: usize) -> usize {
            while par[i] != i {
                par[i] = par[par[i]];
                i = par[i];
            }
            i
        }
        for &i in &s.p_els {
            for g in s.insts[i].conn() {
                for k in 1..g.len() {
                    let a = if g[0] < 0 { nnode } else { g[0] as usize };
                    let bb = if g[k] < 0 { nnode } else { g[k] as usize };
                    let ra = find(&mut par, a);
                    let rb = find(&mut par, bb);
                    if ra != rb {
                        par[ra] = rb;
                    }
                }
            }
        }
        let mut seen = HashSet::new();
        let rg = find(&mut par, nnode);
        for i in 0..nnode {
            let r = find(&mut par, i);
            if r == rg || seen.contains(&r) {
                continue;
            }
            seen.insert(r);
            s.ties.push(i as i32);
        }
        s.n_node = nnode;
        s.n = nnode + b.n_var as usize;
        s.n_c = b.n_c as usize;
        let n = s.n;
        s.e.ps = vec![0.0; b.ps_len.max(1)];
        s.e.cs = vec![0.0; b.cs_len.max(1)];
        s.e.scratch = vec![0.0; b.scratch_len.max(1)];
        s.e.n_node = nnode;
        s.m = Mat::new(n);
        s.b = vec![0.0; n];
        s.b0 = vec![0.0; n];
        s.x = vec![0.0; n];
        s.e.x_p = vec![0.0; n];
        s.x_pp = vec![0.0; n];
        s.x_t = vec![0.0; n];
        s.r = vec![0.0; n];
        s.d = vec![0.0; n];
        s.x_ppp = vec![0.0; n];
        s.x4 = vec![0.0; n];
        s.x_scale = vec![0.0; n];
        s.lte_w = vec![1.0; n];
        for k in lte_skip {
            if k >= 0 && (k as usize) < n {
                s.lte_w[k as usize] = 0.0;
            }
        }
        s.e.cv = vec![0.0; s.n_c];
        s.e.cv_p = vec![0.0; s.n_c];
        s.e.cev = vec![NAN; s.n_c];
        s.e.cvec = vec![None; s.n_c];
        s.e.warnings = std::mem::take(&mut b.warnings);
        s.order_control()?;
        s.has_nl = !s.nl_els.is_empty();
        s.has_dyn = !s.dyn_els.is_empty();
        Ok(s)
    }

    pub fn warn(&mut self, msg: String) {
        self.e.warn(msg);
    }
    fn el_name(&self, i: usize) -> &str {
        &self.el_names[self.inst_el[i]]
    }
    fn el_id(&self, i: usize) -> i64 {
        self.el_ids[self.inst_el[i]]
    }

    fn order_control(&mut self) -> R<()> {
        let mut drv = vec![-1i64; self.n_c];
        for &bi in &self.c_blks {
            for o in self.insts[bi].outs() {
                if o == 0 {
                    return Err(SimError::el(format!("{}：輸出端不可直接接地", self.el_name(bi)), self.el_id(bi)));
                }
                if o < 0 {
                    continue;
                }
                if drv[o as usize] >= 0 {
                    let d = drv[o as usize] as usize;
                    return Err(SimError::el(format!("控制節點有多個輸出元件驅動：{} 與 {}", self.el_name(d), self.el_name(bi)), self.el_id(bi)));
                }
                drv[o as usize] = bi as i64;
            }
        }
        for &bi in &self.c_blks {
            for i in self.insts[bi].ins() {
                if i > 0 && drv[i as usize] < 0 {
                    return Err(SimError::el(format!("{}：輸入端沒有連接到任何訊號來源", self.el_name(bi)), self.el_id(bi)));
                }
            }
        }
        for &pi in &self.p_els {
            for i in self.insts[pi].cins() {
                if i > 0 && drv[i as usize] < 0 {
                    return Err(SimError::el(format!("{}：控制/閘極輸入端沒有訊號來源", self.el_name(pi)), self.el_id(pi)));
                }
            }
        }
        for &pi in &self.probes {
            for i in self.insts[pi].cins() {
                if i > 0 && drv[i as usize] < 0 {
                    return Err(SimError::el(format!("{}：量測的控制節點沒有訊號來源", self.el_name(pi)), self.el_id(pi)));
                }
            }
        }
        // gate frequency of each control node's driver (thyristor-bridge phase shifters)
        self.e.gate_freq = (0..self.n_c).map(|c| if drv[c] >= 0 { self.insts[drv[c] as usize].gate_freq() } else { 0.0 }).collect();
        // graph: edge drv(in) -> b, for feed-through blocks
        let blks = self.c_blks.clone();
        let nb = blks.len();
        let mut idx = HashMap::new();
        for (i, &b) in blks.iter().enumerate() {
            idx.insert(b, i);
        }
        let mut adj: Vec<Vec<usize>> = vec![vec![]; nb];
        for (i, &b) in blks.iter().enumerate() {
            if !self.insts[b].ft() {
                continue;
            }
            for inp in self.insts[b].ins() {
                if inp > 0 && drv[inp as usize] >= 0 {
                    let d = drv[inp as usize] as usize;
                    adj[idx[&d]].push(i);
                }
            }
        }
        // Tarjan (iterative)
        let mut index = 0i64;
        let mut ix = vec![-1i64; nb];
        let mut low = vec![0i64; nb];
        let mut on = vec![false; nb];
        let mut st: Vec<usize> = Vec::new();
        let mut sccs: Vec<Vec<usize>> = Vec::new();
        for v0 in 0..nb {
            if ix[v0] >= 0 {
                continue;
            }
            let mut stack: Vec<(usize, usize)> = vec![(v0, 0)];
            ix[v0] = index;
            low[v0] = index;
            index += 1;
            st.push(v0);
            on[v0] = true;
            while let Some(&(v, ei)) = stack.last() {
                if ei < adj[v].len() {
                    stack.last_mut().unwrap().1 += 1;
                    let w = adj[v][ei];
                    if ix[w] < 0 {
                        ix[w] = index;
                        low[w] = index;
                        index += 1;
                        st.push(w);
                        on[w] = true;
                        stack.push((w, 0));
                    } else if on[w] {
                        low[v] = low[v].min(ix[w]);
                    }
                } else {
                    stack.pop();
                    if let Some(&(u2, _)) = stack.last() {
                        low[u2] = low[u2].min(low[v]);
                    }
                    if low[v] == ix[v] {
                        let mut comp = Vec::new();
                        loop {
                            let w = st.pop().unwrap();
                            on[w] = false;
                            comp.push(w);
                            if w == v {
                                break;
                            }
                        }
                        sccs.push(comp);
                    }
                }
            }
        }
        sccs.reverse(); // Tarjan yields reverse topological order
        let mut any_loop = false;
        self.c_groups = sccs
            .into_iter()
            .map(|c| {
                let lp = c.len() > 1 || adj[c[0]].contains(&c[0]);
                any_loop |= lp;
                let bl: Vec<usize> = c.iter().map(|&i| blks[i]).collect();
                let mut outs = Vec::new();
                for &b in &bl {
                    for o in self.insts[b].outs() {
                        if o > 0 {
                            outs.push(o);
                        }
                    }
                }
                CGroup { blks: bl, lp, outs }
            })
            .collect();
        if any_loop {
            self.warn("控制電路含有代數迴路，已以逐步疊代求解。".into());
        }
        self.drv = drv;
        Ok(())
    }

    /* ---------- matrix assembly and solve ---------- */
    fn key(&mut self, h: f64, be: bool) -> Key {
        if let Some((kh, kbe, kv, k)) = &self.kkey {
            if *kh == h.to_bits() && *kbe == be && *kv == self.sw_ver {
                return k.clone();
            }
        }
        let mut k: Key = Vec::with_capacity(self.sw_els.len() + 2);
        k.push(h.to_bits());
        k.push(be as u64);
        for &i in &self.sw_els {
            k.push(self.insts[i].skey(&self.e) as u64);
        }
        self.kkey = Some((h.to_bits(), be, self.sw_ver, k.clone()));
        k
    }

    fn assemble(&mut self, h: f64, be: bool) {
        for &k in &self.m.pat.list {
            self.m.a[k as usize] = 0.0;
        }
        if self.has_nl {
            // the linear part only depends on (h, be, switch states): keep it per key and scatter it back
            let k = self.key(h, be);
            if let Some(c) = self.lin_cache.get(&k) {
                for (e, &v) in c.iter().enumerate() {
                    let idx = self.m.pat.list[e] as usize;
                    self.m.a[idx] = v;
                }
            } else {
                for &i in &self.p_els {
                    if !self.caps[i].dyn_ {
                        self.insts[i].stamp_g(&mut self.m, h, be, &self.e);
                    }
                }
                for &t in &self.ties {
                    self.m.a(t, t, 1e-3);
                }
                let v: Vec<f64> = self.m.pat.list.iter().map(|&p| self.m.a[p as usize]).collect();
                if self.lin_cache.len() > 96 {
                    self.lin_cache.clear();
                }
                self.lin_cache.insert(k, v);
            }
            for &i in &self.dyn_els {
                self.insts[i].stamp_g(&mut self.m, h, be, &self.e);
            }
            return;
        }
        for &i in &self.p_els {
            self.insts[i].stamp_g(&mut self.m, h, be, &self.e);
        }
        for &t in &self.ties {
            self.m.a(t, t, 1e-3);
        }
    }

    fn rhs_into(&self, t: f64, h: f64, be: bool, b: &mut [f64]) {
        for v in b.iter_mut() {
            *v = 0.0;
        }
        for &i in &self.p_els {
            self.insts[i].stamp_b(b, t, h, be, &self.e);
        }
    }

    fn factor_now(&mut self, h: f64, be: bool) -> R<Lu> {
        self.assemble(h, be);
        self.m.pat.commit();
        let sym = match &self.sym {
            Some(s) if s.ver == self.m.pat.ver && s.nnz == self.m.pat.list.len() && s.pat_id == self.m.pat.id => Some(s.clone()),
            _ => None,
        };
        let mut lu = Lu::new(self.n, sym);
        if !lu.factor(&self.m.a, &self.m.pat) {
            return Err(SimError::new(singular_msg(lu.bad)));
        }
        self.sym = lu.sym.clone();
        lu.keep(&self.m.a, &self.m.pat);
        Ok(lu)
    }

    /// Solve the power network at time t over interval h with the present switch states (into self.x).
    fn solve_at(&mut self, t: f64, h: f64, be: bool) -> R<()> {
        if !self.has_nl {
            let mut b = std::mem::take(&mut self.b);
            self.rhs_into(t, h, be, &mut b);
            self.b = b;
            if self.has_dyn {
                let mut lu = self.factor_now(h, be)?;
                solve_lin(&mut lu, &self.b, &mut self.x, &mut self.r, &mut self.d, self.o.refine);
                return Ok(());
            }
            let k = self.key(h, be);
            self.cache_tick += 1;
            let tick = self.cache_tick;
            if let Some(ent) = self.cache.get_mut(&k) {
                ent.1 = tick; // least recently used eviction order
            } else {
                let lu = self.factor_now(h, be)?;
                if self.cache.len() >= 96 {
                    if let Some(old) = self.cache.iter().min_by_key(|(_, v)| v.1).map(|(k, _)| k.clone()) {
                        self.cache.remove(&old);
                    }
                }
                self.cache.insert(k.clone(), (lu, tick));
            }
            let ent = self.cache.get_mut(&k).unwrap();
            solve_lin(&mut ent.0, &self.b, &mut self.x, &mut self.r, &mut self.d, self.o.refine);
            return Ok(());
        }
        // Newton iteration for nonlinear elements. Initial guess: linear extrapolation of the two last
        // accepted solutions (SPICE predictor) for a regular continuation, otherwise the previous solution.
        let n = self.n;
        let mut pred = self.pred_ok && !be && !self.first_step && !self.topo && self.h_p > 0.0;
        if pred {
            let f = h / self.h_p;
            for i in 0..n {
                self.x[i] = self.e.x_p[i] + (self.e.x_p[i] - self.x_pp[i]) * f;
            }
        } else {
            self.x.copy_from_slice(&self.e.x_p);
        }
        let mut b0 = std::mem::take(&mut self.b0);
        self.rhs_into(t, h, be, &mut b0); // the linear right-hand side does not depend on x
        self.b0 = b0;
        let mut lu = self.nlu.take().unwrap_or_else(|| Lu::new(n, None));
        let mut x = std::mem::take(&mut self.x);
        let mut xn = std::mem::take(&mut self.x_t);
        let res = (|| -> R<bool> {
            let (mut err_p, mut err_pp) = (f64::INFINITY, f64::INFINITY);
            let mut lim = if pred { 12 } else { 100 };
            let mut it: i32 = 0;
            while it < lim {
                if it == 0 {
                    // junction limiting is referenced to the last accepted solution, not the predictor
                    let xp = std::mem::take(&mut self.e.x_p);
                    for &i in &self.nl_els {
                        let xr: &[f64] = if pred { &xp } else { &x };
                        self.insts[i].nl_guess(&x, xr, &mut self.e);
                    }
                    self.e.x_p = xp;
                }
                self.assemble(h, be);
                self.b.copy_from_slice(&self.b0);
                for &i in &self.nl_els {
                    self.insts[i].stamp_nl(&mut self.m, &mut self.b, &x, t, h, be, false, &mut self.e);
                }
                self.m.pat.commit();
                if !lu.factor(&self.m.a, &self.m.pat) {
                    return Err(SimError::new(singular_msg(lu.bad)));
                }
                lu.keep(&self.m.a, &self.m.pat);
                solve_lin(&mut lu, &self.b, &mut xn, &mut self.r, &mut self.d, self.o.refine);
                let (mut err, mut sc) = (0.0f64, 0.0f64);
                for i in 0..n {
                    let d = (xn[i] - x[i]).abs();
                    if d > err {
                        err = d;
                    }
                    let a = xn[i].abs();
                    if a > sc {
                        sc = a;
                    }
                }
                x.copy_from_slice(&xn);
                // converged, or stalled at the round-off floor of a badly scaled matrix
                // (with very small steps the capacitor companions make the matrix ill-conditioned and the
                // round-off of the solve can make the iterates alternate instead of settling: no real progress
                // over two iterations at a level far below the signal is the same floor)
                let small = err <= 1e-6 * sc.max(1.0) + 1e-9;
                let stall = small && ((err > 0.5 * err_p && err_p > 0.5 * err_pp) || (err_pp.is_finite() && err.max(err_p) > 0.25 * err_pp && err_p <= 1e-6 * sc.max(1.0) + 1e-9 && err_pp <= 1e-6 * sc.max(1.0) + 1e-9));
                if err <= 1e-9 * sc.max(1.0) + 1e-12 || stall {
                    let ok = self.nl_els.iter().all(|&i| self.insts[i].nl_ok());
                    if ok {
                        return Ok(true);
                    }
                }
                err_pp = err_p;
                err_p = err;
                // the extrapolated start did not converge quickly: restart from the previous solution
                if pred && it == lim - 1 {
                    pred = false;
                    lim = 100;
                    it = -1;
                    x.copy_from_slice(&self.e.x_p);
                    err_p = f64::INFINITY;
                    err_pp = f64::INFINITY;
                }
                it += 1;
            }
            Ok(false)
        })();
        self.x = x;
        self.x_t = xn;
        self.nlu = Some(lu);
        if !res? {
            self.non_conv += 1;
            self.warn(format!("非線性元件在 t={} s 附近未完全收斂。", js_to_exponential(t, 4)));
        }
        Ok(())
    }

    /// Apply latched gate values from the control circuit (except forced ones)
    fn apply_gates(&mut self, t: f64) {
        let mut ch = false;
        let eps = self.e.eps_t;
        for &i in &self.timed_sw {
            let st = self.insts[i].state_at(t + eps);
            let so = self.insts[i].so();
            if st as f64 != self.e.ps[so] {
                self.e.ps[so] = st as f64;
                ch = true;
            }
        }
        for &i in &self.gated {
            let so = self.insts[i].so();
            if self.e.ps[so + 2] > t + eps {
                continue; // forced until this time
            }
            let gn = self.insts[i].gate_node() as usize;
            let g = if self.e.cv[gn] > 0.5 { 1 } else { 0 };
            if g as f64 != self.e.ps[so + 1] {
                self.e.ps[so + 1] = g as f64;
                if self.insts[i].on_gate(g, false, &mut self.e) {
                    ch = true;
                }
            }
        }
        if ch {
            self.be = true;
            self.topo = true;
            self.sw_ver += 1;
        }
    }

    fn changes(&self, x: &[f64], t: f64) -> Vec<Change> {
        let mut out = Vec::new();
        for &i in &self.sw_els {
            let ns = self.insts[i].desired(x, t, &self.e);
            if ns != self.insts[i].state(&self.e) {
                out.push(Change { m: i, ns, tau: NAN });
            }
        }
        out
    }

    /// Power solution over [ta,tb] with state consistency and event localization.
    fn solve_power(&mut self, ta: f64, tb: f64, depth: u32, no_split: bool) -> R<()> {
        let h = tb - ta;
        let eps = self.e.eps_t;
        let mut be = if self.o.damp { self.be } else { false };
        if self.first_step {
            be = true;
        }
        // damping after a switching event: two short backward-Euler steps (h/8 each) reset the derivative
        // history that makes the trapezoidal rule ring; the rest of the interval is trapezoidal again
        if be && !no_split && self.o.be_split && depth < 12 && h > 32.0 * eps {
            let t1 = ta + h / 8.0;
            let t2 = ta + h / 4.0;
            self.solve_power(ta, t1, depth + 1, true)?;
            self.be = true;
            self.solve_power(t1, t2, depth + 1, true)?;
            self.solve_power(t2, tb, depth + 1, false)?;
            return Ok(());
        }
        self.solve_at(tb, h, be)?;
        if self.sw_els.is_empty() {
            self.accept(tb, h, be);
            return Ok(());
        }
        let mut ch = self.changes(&self.x, tb);
        let topo = self.topo;
        self.topo = false;
        if ch.is_empty() {
            self.accept(tb, h, be);
            return Ok(());
        }
        if self.o.interp && !topo && depth < 12 && h > eps * 4.0 {
            let mut tmin = 2.0f64;
            for c in ch.iter_mut() {
                let tau = self.insts[c.m].tau(&self.e.x_p, &self.x, c.ns, &self.e);
                c.tau = tau;
                if tau > 1e-6 && tau < 1.0 - 1e-6 && tau < tmin {
                    tmin = tau;
                }
            }
            if tmin < 1.0 {
                let mut te = ta + tmin * h;
                self.solve_at(te, te - ta, be)?;
                // refine the switching instant with secant corrections until the crossing sits at te
                let mut lead = Vec::new();
                let mut t_next = tb;
                for c in &ch {
                    if (c.tau - tmin).abs() < 1e-6 {
                        if self.caps[c.m].qq {
                            lead.push(c.clone());
                        }
                    } else if c.tau > tmin && c.tau < 1.0 {
                        t_next = t_next.min(ta + c.tau * h);
                    }
                }
                if self.o.refine_evt && !lead.is_empty() {
                    for _ in 0..4 {
                        let mut f = NAN;
                        for c in &lead {
                            let q = self.insts[c.m].qq(&self.e.x_p, &self.x, c.ns, &self.e);
                            let (qa, qb) = match q {
                                Some(q) => q,
                                None => continue,
                            };
                            if qa == qb {
                                continue;
                            }
                            let fr = qa / (qa - qb);
                            if !(fr > 0.0) {
                                continue;
                            }
                            if !(fr < f) {
                                if f.is_nan() {
                                    f = fr;
                                }
                            } else {
                                f = fr;
                            }
                        }
                        if !(f > 0.0) || (f - 1.0).abs() < 1e-4 {
                            break;
                        }
                        let tn = ta + f.min(1.5) * (te - ta);
                        if tn <= ta + 2.0 * eps || tn >= t_next - 2.0 * eps {
                            break;
                        }
                        te = tn;
                        self.solve_at(te, te - ta, be)?;
                        self.evt_refined += 1;
                    }
                }
                self.accept(te, te - ta, be);
                self.first_step = false;
                let mut any = false;
                for c in &ch {
                    if (c.tau - tmin).abs() < 1e-6 {
                        self.insts[c.m].set_state(c.ns, &mut self.e);
                        any = true;
                    }
                }
                self.sw_ver += 1;
                if any {
                    self.be = true;
                    self.topo = true;
                    self.events += 1;
                }
                return self.solve_power(te, tb, depth + 1, false);
            }
        }
        // plain consistency iteration (switch at ta)
        let mut it = 0;
        while !ch.is_empty() && it < 40 {
            for c in &ch {
                self.insts[c.m].set_state(c.ns, &mut self.e);
            }
            self.sw_ver += 1;
            be = if self.o.damp { true } else { be };
            self.solve_at(tb, h, be)?;
            ch = self.changes(&self.x, tb);
            it += 1;
        }
        if !ch.is_empty() {
            self.non_conv += 1;
            self.warn(format!("開關狀態在 t={} s 附近未收斂（可能振盪），請縮小時間步長。", js_to_exponential(tb, 4)));
        }
        // states changed at ta: redo the interval with the short damping step instead of one full BE step
        if it > 0 && be && !no_split && self.o.be_split && depth < 12 && h > 32.0 * eps {
            self.be = true;
            self.topo = true;
            return self.solve_power(ta, tb, depth + 1, false);
        }
        self.accept(tb, h, be);
        Ok(())
    }

    fn accept(&mut self, t: f64, h: f64, be: bool) {
        let x = std::mem::take(&mut self.x);
        for &i in &self.p_els {
            self.insts[i].accept(&x, t, h, be, &mut self.e);
        }
        self.x = x;
        std::mem::swap(&mut self.x4, &mut self.x_ppp); // x4 <- xPPP (old x4 buffer reused)
        self.h_ppp = self.h_pp;
        self.x_ppp.copy_from_slice(&self.x_pp);
        self.h_pp = self.h_p;
        self.x_pp.copy_from_slice(&self.e.x_p);
        self.h_p = h;
        self.pred_ok = !be && !self.first_step;
        self.e.x_p.copy_from_slice(&self.x);
        if self.o.lte {
            let (mut gv, mut gi) = (self.sc_v, self.sc_i);
            let nn = self.n_node;
            for i in 0..self.n {
                let a = self.x[i].abs();
                if a > self.x_scale[i] {
                    self.x_scale[i] = a;
                    if i < nn {
                        if a > gv {
                            gv = a;
                        }
                    } else if a > gi {
                        gi = a;
                    }
                }
            }
            self.sc_v = gv;
            self.sc_i = gi;
        }
        self.last_be = be || self.first_step;
        self.hist_n = if self.last_be { 0 } else { self.hist_n + 1 };
        self.be = false;
        if self.e.damp_req {
            self.e.damp_req = false;
            self.be = self.o.damp;
        }
        self.first_step = false;
    }

    /* ---------- control circuit ---------- */
    fn eval_control(&mut self, t: f64, h: f64) -> R<()> {
        for v in self.e.cev.iter_mut() {
            *v = NAN;
        }
        let groups = std::mem::take(&mut self.c_groups);
        let res = (|| -> R<()> {
            for g in &groups {
                if !g.lp {
                    for &b in &g.blks {
                        self.insts[b].eval(t, h, &mut self.e, &self.x)?;
                    }
                    continue;
                }
                let n = g.outs.len();
                let mut prev = vec![0.0; n];
                let mut ok = false;
                // 1) Gauss-Seidel sweeps
                for it in 0..40 {
                    for (i, &o) in g.outs.iter().enumerate() {
                        prev[i] = self.e.cv[o as usize];
                    }
                    for &b in &g.blks {
                        self.insts[b].eval(t, h, &mut self.e, &self.x)?;
                    }
                    let mut err = false;
                    for (i, &o) in g.outs.iter().enumerate() {
                        let d = (self.e.cv[o as usize] - prev[i]).abs();
                        if d > 1e-12 * prev[i].abs().max(1.0) {
                            err = true;
                        }
                    }
                    if !err && it > 0 {
                        ok = true;
                        break;
                    }
                }
                // 2) Newton iteration on the loop variables with a finite-difference Jacobian
                if !ok && n <= 60 {
                    ok = self.loop_newton(g, t, h)?;
                }
                if !ok {
                    self.non_conv += 1;
                    self.warn(format!("控制電路代數迴路在 t={} s 未收斂。", js_to_exponential(t, 4)));
                }
            }
            Ok(())
        })();
        self.c_groups = groups;
        res
    }
    /// g(z): evaluate every block of the loop with all loop variables held at z (Jacobi form)
    fn loop_map(&mut self, g: &CGroup, z: &[f64], t: f64, h: f64, out: &mut [f64]) -> R<()> {
        for &b in &g.blks {
            for (i, &o) in g.outs.iter().enumerate() {
                self.e.cv[o as usize] = z[i];
            }
            self.insts[b].eval(t, h, &mut self.e, &self.x)?;
            for o in self.insts[b].outs() {
                if let Some(i) = g.outs.iter().position(|&q| q == o) {
                    out[i] = self.e.cv[o as usize];
                }
            }
        }
        Ok(())
    }
    fn loop_newton(&mut self, g: &CGroup, t: f64, h: f64) -> R<bool> {
        let n = g.outs.len();
        let mut z: Vec<f64> = g.outs.iter().map(|&o| self.e.cv[o as usize]).collect();
        let mut gz = vec![0.0; n];
        let mut gp = vec![0.0; n];
        let mut jm = vec![0.0; n * n];
        let mut r = vec![0.0; n];
        let mut dz = vec![0.0; n];
        for _ in 0..30 {
            self.loop_map(g, &z, t, h, &mut gz)?;
            let mut err = 0.0f64;
            for i in 0..n {
                r[i] = gz[i] - z[i];
                err = err.max(r[i].abs() / z[i].abs().max(1.0));
            }
            if err < 1e-12 {
                let gzc = gz.clone();
                self.loop_map(g, &gzc, t, h, &mut gp)?;
                for (i, &o) in g.outs.iter().enumerate() {
                    self.e.cv[o as usize] = gz[i];
                }
                return Ok(true);
            }
            for j in 0..n {
                let d = 1e-7 * z[j].abs().max(1.0);
                let zj = z[j];
                z[j] = zj + d;
                let zc = z.clone();
                self.loop_map(g, &zc, t, h, &mut gp)?;
                z[j] = zj;
                for i in 0..n {
                    jm[i * n + j] = (gp[i] - gz[i]) / d - if i == j { 1.0 } else { 0.0 };
                }
            }
            let mut lu = Lu::new(n, None);
            if !lu.factor_dense(&jm) {
                return Ok(false);
            }
            for v in r.iter_mut() {
                *v = -*v;
            }
            lu.solve(&r, &mut dz);
            for i in 0..n {
                z[i] += dz[i];
            }
        }
        Ok(false)
    }
    fn commit_control(&mut self, t: f64, h: f64) {
        for &b in &self.c_blks {
            self.insts[b].commit(t, h, &mut self.e);
        }
        // a NaN in a control signal would silently poison every integrator downstream: say which block produced it, once
        if !self.nan_seen {
            for i in 1..self.n_c {
                if self.e.cv[i].is_nan() {
                    self.nan_seen = true;
                    let d = self.drv.get(i).copied().unwrap_or(-1);
                    let src = if d >= 0 { format!("，來源：{}", self.el_name(d as usize)) } else { String::new() };
                    self.warn(format!(
                        "控制訊號在 t={} s 出現 NaN（非數值）{}。常見原因：除以 0（例如 x/Vd 在 Vd = 0 時）、負數開根號或取對數；請加 max()/min() 保護。",
                        js_to_exponential(t, 4),
                        src
                    ));
                    break;
                }
            }
        }
        self.e.cv_p.copy_from_slice(&self.e.cv);
    }

    /* ---------- interval and stepping ---------- */
    fn snapshot(&mut self, depth: usize) {
        while self.snaps.len() <= depth {
            self.snaps.push(Snap::default());
        }
        let sn = &mut self.snaps[depth];
        sn.ps.clone_from(&self.e.ps);
        sn.x.clone_from(&self.x);
        sn.xp.clone_from(&self.e.x_p);
        sn.xpp.clone_from(&self.x_pp);
        sn.xppp.clone_from(&self.x_ppp);
        sn.x4.clone_from(&self.x4);
        sn.cv.clone_from(&self.e.cv_p);
        sn.be = self.be;
        sn.fs = self.first_step;
        sn.topo = self.topo;
        sn.hp = self.h_p;
        sn.hpp = self.h_pp;
        sn.hppp = self.h_ppp;
        sn.hist_n = self.hist_n;
        sn.last_be = self.last_be;
        sn.pred_ok = self.pred_ok;
    }
    fn restore(&mut self, depth: usize) {
        let sn = &self.snaps[depth];
        self.e.ps.copy_from_slice(&sn.ps);
        self.x.copy_from_slice(&sn.x);
        self.e.x_p.copy_from_slice(&sn.xp);
        self.x_pp.copy_from_slice(&sn.xpp);
        self.x_ppp.copy_from_slice(&sn.xppp);
        self.x4.copy_from_slice(&sn.x4);
        self.e.cv.copy_from_slice(&sn.cv);
        self.be = sn.be;
        self.first_step = sn.fs;
        self.topo = sn.topo;
        self.h_p = sn.hp;
        self.h_pp = sn.hpp;
        self.h_ppp = sn.hppp;
        self.hist_n = sn.hist_n;
        self.last_be = sn.last_be;
        self.pred_ok = sn.pred_ok;
        self.sw_ver += 1;
    }
    /// local truncation error of the trapezoidal step that produced x, from the third divided difference
    /// of the last four accepted points (SPICE's estimate), as the largest ratio to the tolerance
    fn lte_ratio(&self) -> f64 {
        let (x, x1, x2, x3) = (&self.e.x_p, &self.x_pp, &self.x_ppp, &self.x4);
        let (h, h1, h2) = (self.h_p, self.h_pp, self.h_ppp);
        let rel = self.o.lte_tol;
        let nn = self.n_node;
        let av = self.o.lte_abs_v + 1e-3 * rel * self.sc_v;
        let aiv = self.o.lte_abs_i + 1e-3 * rel * self.sc_i;
        let c = h * h * h / 2.0;
        let mut worst = 0.0f64;
        for i in 0..self.n {
            let d1a = (x[i] - x1[i]) / h;
            let d1b = (x1[i] - x2[i]) / h1;
            let d1c = (x2[i] - x3[i]) / h2;
            let d2a = (d1a - d1b) / (h + h1);
            let d2b = (d1b - d1c) / (h1 + h2);
            let d3 = (d2a - d2b) / (h + h1 + h2);
            let err = (c * d3).abs() * self.lte_w[i];
            let tol = rel * self.x_scale[i] + if i < nn { av } else { aiv };
            let r = err / tol;
            if r > worst {
                worst = r;
            }
        }
        worst
    }
    fn interval(&mut self, ta: f64, tb: f64, depth: usize) -> R<()> {
        let h = tb - ta;
        let eps = self.e.eps_t;
        let lte_on = self.o.lte && self.lte_depth < self.o.lte_max_depth && h > 32.0 * eps;
        let need_snap = ((self.o.interp && !self.gated.is_empty()) || lte_on) && depth < 10;
        if need_snap {
            self.snapshot(depth);
        }
        self.apply_gates(ta);
        self.solve_power(ta, tb, 0, false)?;
        self.eval_control(tb, h)?;
        if depth == 0 {
            self.steps += 1;
        }
        if lte_on && need_snap && !self.last_be && self.hist_n >= 5 && self.lte_ratio() > 1.0 {
            // the step is too long for the requested accuracy: redo it as two halves
            self.restore(depth);
            self.refined += 1;
            self.lte_depth += 1;
            let tm = ta + h / 2.0;
            self.interval(ta, tm, depth + 1)?;
            self.interval(tm, tb, depth + 1)?;
            self.lte_depth -= 1;
            return Ok(());
        }
        if !lte_on && self.o.lte && self.lte_depth >= self.o.lte_max_depth && !self.last_be && self.hist_n >= 5 && self.lte_ratio() > 1.0 {
            self.refine_max += 1;
        }
        if need_snap && self.o.interp && !self.gated.is_empty() {
            let mut te = f64::INFINITY;
            let (lo, hi) = (ta + eps, tb - eps);
            for &i in &self.gated {
                let so = self.insts[i].so();
                if self.e.ps[so + 2] > ta + eps {
                    continue;
                }
                let gn = self.insts[i].gate_node() as usize;
                let g = if self.e.cv[gn] > 0.5 { 1.0 } else { 0.0 };
                if g == self.e.ps[so + 1] {
                    continue;
                }
                let ev = self.e.cev[gn];
                if ev > lo && ev < hi && ev < te {
                    te = ev;
                }
            }
            if te < f64::INFINITY {
                let tol = eps.max(1e-9 * h);
                let mut forced = Vec::new();
                for &i in &self.gated {
                    let so = self.insts[i].so();
                    if self.e.ps[so + 2] > ta + eps {
                        continue;
                    }
                    let gn = self.insts[i].gate_node() as usize;
                    let g = if self.e.cv[gn] > 0.5 { 1 } else { 0 };
                    if g as f64 != self.e.ps[so + 1] && (self.e.cev[gn] - te).abs() <= tol {
                        forced.push((i, g));
                    }
                }
                self.restore(depth);
                self.interval(ta, te, depth + 1)?;
                let mut ch = false;
                for (i, g) in forced {
                    let so = self.insts[i].so();
                    if self.e.ps[so + 1] != g as f64 {
                        self.e.ps[so + 1] = g as f64;
                        if self.insts[i].on_gate(g, false, &mut self.e) {
                            ch = true;
                        }
                    }
                    self.e.ps[so + 2] = tb;
                }
                if ch {
                    self.be = true;
                    self.topo = true;
                    self.sw_ver += 1;
                }
                self.events += 1;
                self.interval(te, tb, depth + 1)?;
                return Ok(());
            }
        }
        self.commit_control(tb, h);
        self.t = tb;
        Ok(())
    }
    fn next_break(&self, t: f64) -> f64 {
        let mut tb = f64::INFINITY;
        for &i in &self.bp_els {
            let v = self.insts[i].next_break(t + self.e.eps_t, &self.e);
            if v < tb {
                tb = v;
            }
        }
        tb
    }
    fn jump_at(&self, t: f64) -> bool {
        self.bp_els.iter().any(|&i| self.caps[i].jump && self.insts[i].jump_at(t, &self.e))
    }

    /// Advance from self.t to t_end (a grid point), splitting at breakpoints.
    pub fn advance_to(&mut self, t_end: f64) -> R<()> {
        let mut t = self.t;
        let eps = self.e.eps_t;
        let mut guard = 0u64;
        while t < t_end - eps {
            let bp = self.next_break(t);
            let mut tn = t_end;
            if bp < t_end - eps && bp > t + eps {
                tn = bp;
            }
            self.interval(t, tn, 0)?;
            let at_bp = if tn != t_end { true } else { (bp - t_end).abs() <= eps };
            if at_bp {
                if self.jump_at(tn) {
                    self.be = true;
                }
                self.hist_n = 0; // a source corner: the step-error history is not smooth across it
            }
            t = tn;
            self.t = t;
            guard += 1;
            if guard > 1_000_000 {
                return Err(SimError::new("單一步長內的斷點過多，請檢查訊號源頻率。"));
            }
        }
        self.t = t_end;
        Ok(())
    }

    /* ---------- initialisation ---------- */
    pub fn init(&mut self) -> R<()> {
        self.e.eps_t = self.o.eps_t();
        for k in 0..self.p_els.len() {
            let i = self.p_els[k];
            self.insts[i].init(&mut self.e)?;
        }
        for k in 0..self.c_blks.len() {
            let i = self.c_blks[k];
            self.insts[i].init(&mut self.e)?;
        }
        // initial power solution (capacitors as voltage sources, inductors as current sources)
        let mut x0 = vec![0.0; self.n];
        let mut ok = true;
        if self.init_solve(&mut x0, false).is_err() {
            // second attempt: inductors whose initial current is 0 are taken as DC shorts
            match self.init_solve(&mut x0, true) {
                Ok(()) => {
                    let mut named = Vec::new();
                    for &i in &self.p_els {
                        if let Some(k) = self.insts[i].relaxed_short() {
                            let v = x0[k as usize];
                            if v.abs() > 1e-2 {
                                named.push(format!("{} = {} A", self.el_name(i), js_to_precision(v, 4)));
                            }
                        }
                    }
                    if !named.is_empty() {
                        self.warn(format!("t = 0 的初始條件以直流穩態求得（電感與電壓源串聯或節點僅由電感連接時，初始電流為 0 的電感視為短路）：{}，與設定的 i0 = 0 不同。", named.join("、")));
                    }
                }
                Err(_) => {
                    self.warn("初始條件無法一致求解（例如電容與電壓源並聯但初始電壓不同），t=0 的輸出以近似值顯示。".into());
                    x0 = self.init_fallback();
                    ok = false;
                    self.e.init_currents = None;
                }
            }
        }
        self.x.copy_from_slice(&x0);
        self.e.x_p.copy_from_slice(&x0);
        if ok {
            for &i in &self.p_els {
                self.insts[i].accept_init(&x0, &mut self.e);
            }
        }
        // control at t=0
        self.eval_control(0.0, 0.0)?;
        self.commit_control(0.0, 0.0);
        let mut ch = false;
        for &i in &self.gated {
            let gn = self.insts[i].gate_node() as usize;
            let g = if self.e.cv[gn] > 0.5 { 1 } else { 0 };
            let so = self.insts[i].so();
            self.e.ps[so + 1] = g as f64;
            self.e.ps[so + 2] = -1.0;
            if self.insts[i].on_gate(g, true, &mut self.e) {
                ch = true;
            }
        }
        let eps = self.e.eps_t;
        for &i in &self.timed_sw {
            let st = self.insts[i].state_at(eps);
            let so = self.insts[i].so();
            if st as f64 != self.e.ps[so] {
                self.e.ps[so] = st as f64;
                ch = true;
            }
        }
        self.sw_ver += 1;
        self.t = 0.0;
        self.be = !ok || ch;
        self.first_step = !ok || ch;
        Ok(())
    }

    fn init_solve(&mut self, x0: &mut [f64], relax: bool) -> R<()> {
        // extended system: one variable per capacitor for its initial voltage constraint
        let extra: Vec<usize> = self.p_els.iter().copied().filter(|&i| self.insts[i].init_var()).collect();
        let n0 = self.n;
        let n = n0 + extra.len();
        for (k, &i) in extra.iter().enumerate() {
            self.insts[i].set_iv((n0 + k) as i32);
        }
        let mut m = Mat::new(n);
        let mut b = vec![0.0; n];
        let mut xs = vec![0.0; n];
        let mut xo = vec![0.0; n];
        let nli: Vec<usize> = self.nl_els.iter().copied().filter(|&i| self.caps[i].nl_init).collect();
        for &i in &nli {
            let xc = xs.clone();
            self.insts[i].nl_guess(&xc, &xc, &mut self.e);
        }
        let dt = self.o.dt;
        let mut nl_it = 0;
        let mut lu = Lu::new(n, None);
        let mut r = vec![0.0; n];
        let mut d = vec![0.0; n];
        for it in 0..400 {
            for v in m.a.iter_mut() {
                *v = 0.0;
            }
            for v in b.iter_mut() {
                *v = 0.0;
            }
            for &i in &self.p_els {
                let c = self.caps[i];
                if c.init_g {
                    self.insts[i].stamp_init_g(&mut m, relax, &self.e);
                } else {
                    self.insts[i].stamp_g(&mut m, dt, true, &self.e);
                }
                if c.init_b {
                    self.insts[i].stamp_init_b(&mut b, relax, &self.e);
                } else {
                    self.insts[i].stamp_b(&mut b, 0.0, dt, true, &self.e);
                }
            }
            for &t in &self.ties {
                m.a(t, t, 1e-3);
            }
            // devices solved by Newton at t = 0 (true DC operating point, charges open)
            for &i in &nli {
                self.insts[i].stamp_nl(&mut m, &mut b, &xs, 0.0, dt, true, true, &mut self.e);
            }
            // a node that only inductors (current sources at t = 0) and leakage-level conductances touch has
            // no defined voltage in this pass: ask for the relaxed pass (inductors as shorts)
            if it == 0 && !relax && self.weak_init_node(&m.a, n) {
                return Err(SimError::new("浮接節點"));
            }
            m.pat.commit();
            if !lu.factor(&m.a, &m.pat) {
                return Err(SimError::new(singular_msg(lu.bad)));
            }
            xo.copy_from_slice(&xs);
            lu.solve(&b, &mut xs);
            if self.o.refine {
                lu.keep(&m.a, &m.pat);
                if lu.residual(&b, &xs, &mut r) {
                    lu.solve(&r, &mut d);
                    for i in 0..n {
                        xs[i] += d[i];
                    }
                }
            }
            x0.copy_from_slice(&xs[..n0]);
            if !nli.is_empty() {
                let (mut err, mut sc) = (0.0f64, 0.0f64);
                for i in 0..n0 {
                    err = err.max((xs[i] - xo[i]).abs());
                    sc = sc.max(xs[i].abs());
                }
                let lim = nli.iter().any(|&i| !self.insts[i].nl_ok());
                nl_it += 1;
                if nl_it < 300 && (it == 0 || lim || err > 1e-9 * sc.max(1.0) + 1e-12) {
                    continue;
                }
                if nl_it >= 300 {
                    self.warn("t = 0 的非線性工作點未完全收斂，初始值可能有誤差。".into());
                }
                nl_it = 0;
            }
            if self.sw_els.is_empty() {
                break;
            }
            let ch = self.changes(x0, 0.0);
            if ch.is_empty() {
                break;
            }
            for c in &ch {
                self.insts[c.m].set_state(c.ns, &mut self.e);
            }
            self.sw_ver += 1;
        }
        self.e.init_currents = Some(xs);
        Ok(())
    }

    fn weak_init_node(&self, a: &[f64], n: usize) -> bool {
        // a node is defined at t = 0 when a chain of real conductances (≥ 1 nS) and voltage-type branches
        // connects it to ground; inductors (current sources in this pass) and leakage do not count
        let nn = self.n_node;
        let gidx = nn;
        let mut par: Vec<usize> = (0..=nn).collect();
        fn find(par: &mut [usize], mut a: usize) -> usize {
            while par[a] != a {
                par[a] = par[par[a]];
                a = par[a];
            }
            a
        }
        fn uni(par: &mut [usize], a: usize, b: usize) {
            let a = find(par, a);
            let b = find(par, b);
            if a != b {
                par[a] = b;
            }
        }
        let th = 1e-9;
        for i in 0..nn {
            let mut off = 0.0;
            for j in 0..nn {
                if j == i {
                    continue;
                }
                let v = a[i * n + j].abs();
                if v >= th {
                    uni(&mut par, i, j);
                    off += v;
                }
            }
            if a[i * n + i].abs() - off >= th {
                uni(&mut par, i, gidx);
            }
        }
        for k in nn..n {
            let (mut first, mut cnt) = (usize::MAX, 0);
            for j in 0..nn {
                if a[k * n + j] != 0.0 {
                    cnt += 1;
                    if first == usize::MAX {
                        first = j;
                    } else {
                        uni(&mut par, first, j);
                    }
                }
            }
            if cnt == 1 {
                uni(&mut par, first, gidx);
            }
        }
        let g = find(&mut par, gidx);
        (0..nn).any(|i| find(&mut par, i) != g)
    }

    fn init_fallback(&mut self) -> Vec<f64> {
        let mut x0 = vec![0.0; self.n];
        let h = self.o.dt * 1e-3;
        if let Ok(mut lu) = self.factor_now(h, true) {
            let mut b = std::mem::take(&mut self.b);
            self.rhs_into(0.0, h, true, &mut b);
            lu.solve(&b, &mut x0);
            self.b = b;
        }
        self.cache.clear();
        x0
    }

    pub fn find_source(&self, name: &str) -> Option<usize> {
        self.sources.get(&name.trim().to_lowercase()).copied()
    }

    /// Prepare a copy restored from a saved full state for a new AC-sweep frequency.
    pub fn after_restore(&mut self) {
        self.first_step = false;
        self.pred_ok = false;
        self.sw_ver += 1;
        self.kkey = None;
    }

    pub fn finish(&mut self) {
        let t = self.t;
        for k in 0..self.p_els.len() {
            let i = self.p_els[k];
            self.insts[i].finish(t, &mut self.e);
        }
    }

    pub fn probe_value(&self, i: usize) -> f64 {
        self.insts[i].value(&self.e, &self.x)
    }
    pub fn ac_vals(&self, i: usize) -> (f64, f64) {
        self.insts[i].ac_vals(&self.e, &self.x)
    }
    pub fn n_ac_check(&self) -> R<()> {
        for &i in &self.p_els {
            if !self.caps[i].ac {
                return Err(SimError::el(format!("{}：此元件為非線性或切換元件，無法使用線性頻域分析，請改用小訊號注入法（Method = 0）。", self.el_name(i)), self.el_id(i)));
            }
        }
        for &i in &self.c_blks {
            if !self.caps[i].ac {
                return Err(SimError::el(format!("{}：此控制元件為非線性或離散元件，無法使用線性頻域分析，請改用小訊號注入法（Method = 0）。", self.el_name(i)), self.el_id(i)));
            }
        }
        Ok(())
    }
    /// Initialisation used by the linear AC analysis (element init only).
    pub fn init_elements(&mut self) -> R<()> {
        self.e.eps_t = self.o.eps_t();
        for k in 0..self.p_els.len() {
            let i = self.p_els[k];
            self.insts[i].init(&mut self.e)?;
        }
        for k in 0..self.c_blks.len() {
            let i = self.c_blks[k];
            self.insts[i].init(&mut self.e)?;
        }
        Ok(())
    }
    pub fn stamp_ac_all(&self, c: &mut AcCtx, src: usize) -> R<()> {
        for &i in &self.p_els {
            self.insts[i].stamp_ac(c, i == src, &self.e)?;
        }
        for &i in &self.c_blks {
            // an instance that is both a power element and a control block (current sensor) is stamped once
            if self.caps[i].p {
                continue;
            }
            self.insts[i].stamp_ac(c, i == src, &self.e)?;
        }
        for &t in &self.ties {
            c.a(t, t, 1e-3, 0.0);
        }
        // undriven control nodes -> 0
        for cn in 1..self.n_c {
            if self.drv[cn] < 0 {
                let r = c.ci(cn as i32);
                c.a(r, r, 1.0, 0.0);
            }
        }
        Ok(())
    }
}
