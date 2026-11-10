//! Physical (SPICE-grade) semiconductor models
//!   - Diode / LED / Zener : SPICE level-1 diode (IS, N, RS, BV, IBV, CJO, VJ, M, FC, TT)
//!   - BJT                 : Gummel-Poon
//!   - MOSFET              : Shichman-Hodges level 1 + body diode, nonlinear Cgd / Cds options
//!   - Comparator          : open-collector comparator (LM393 / AS393 class)
//!   - TL431, optocoupler, non-ideal gate-driven MOSFET / IGBT, device probe
//! All are solved by Newton iteration with SPICE junction-voltage limiting (pnjlim).
//! Temperature is fixed at 27 °C.

use super::power::Amm;
use crate::clone_box;
use crate::sim::*;
use crate::util::{js_num_str, js_to_precision};

pub const VT: f64 = 1.380649e-23 * 300.15 / 1.602176634e-19; // 25.865 mV at 27 °C
const GMIN: f64 = 1e-12;
const EXPLIM: f64 = 80.0;

/* ------------------------------------------------------------------ SPICE .model parser */
/// ordered SPICE parameter set (KEY uppercase)
#[derive(Clone, Debug, Default)]
pub struct SpP(pub Vec<(String, f64)>);
impl SpP {
    pub fn get(&self, k: &str) -> Option<f64> {
        self.0.iter().find(|(a, _)| a == k).map(|(_, v)| *v)
    }
    pub fn g(&self, k: &str, d: f64) -> f64 {
        self.get(k).unwrap_or(d)
    }
    pub fn set(&mut self, k: &str, v: f64) {
        if let Some(e) = self.0.iter_mut().find(|(a, _)| a == k) {
            e.1 = v;
        } else {
            self.0.push((k.to_string(), v));
        }
    }
    pub fn from(list: &[(&str, f64)]) -> Self {
        let mut p = SpP::default();
        for (k, v) in list {
            p.set(k, *v);
        }
        p
    }
}

/// SPICE number with suffix (M = milli, MEG = mega, MIL)
pub fn spice_num(s: &str) -> f64 {
    let c: Vec<char> = s.trim().chars().collect();
    let n = c.len();
    let mut i = 0;
    if i < n && (c[i] == '+' || c[i] == '-') {
        i += 1;
    }
    let st = i;
    let dig = |k: usize| k < n && c[k].is_ascii_digit();
    if dig(i) {
        while dig(i) {
            i += 1;
        }
        if i < n && c[i] == '.' {
            i += 1;
            while dig(i) {
                i += 1;
            }
        }
    } else if i < n && c[i] == '.' && dig(i + 1) {
        i += 1;
        while dig(i) {
            i += 1;
        }
    } else {
        return f64::NAN;
    }
    if i < n && (c[i] == 'e' || c[i] == 'E') {
        let mut k = i + 1;
        if k < n && (c[k] == '+' || c[k] == '-') {
            k += 1;
        }
        if dig(k) {
            while dig(k) {
                k += 1;
            }
            i = k;
        }
    }
    let num: String = c[st..i].iter().collect();
    let mut v = crate::expr::parse_js_float(&num);
    if c[0] == '-' {
        v = -v;
    }
    let rest: String = c[i..].iter().collect();
    if !rest.chars().enumerate().all(|(j, ch)| ch.is_ascii_alphabetic() || (j == 0 && ch == 'µ')) {
        return f64::NAN;
    }
    let rl = rest.to_lowercase();
    let f = if rl.starts_with("meg") {
        1e6
    } else if rl.starts_with("mil") {
        25.4e-6
    } else {
        match rl.chars().next() {
            Some('t') => 1e12,
            Some('g') => 1e9,
            Some('k') => 1e3,
            Some('m') => 1e-3,
            Some('u') | Some('µ') => 1e-6,
            Some('n') => 1e-9,
            Some('p') => 1e-12,
            Some('f') => 1e-15,
            _ => 1.0,
        }
    };
    v * f
}

/// Parse a .model statement or KEY=VALUE list. Returns (params, unparsable parts).
pub fn parse_spice_model(txt: &str) -> (SpP, Vec<String>) {
    let mut lines = Vec::new();
    for l in txt.split('\n') {
        let l = l.strip_suffix('\r').unwrap_or(l);
        let l = match l.find(';') {
            Some(p) => &l[..p],
            None => l,
        };
        if l.trim_start().starts_with('*') {
            continue;
        }
        let t = l.trim_start();
        let l2 = if let Some(r) = t.strip_prefix('+') { format!(" {}", r) } else { l.to_string() };
        lines.push(l2);
    }
    let mut s = lines.join(" ");
    // ^\s*\.model\s+\S+\s+[a-z]+\s*\(?
    {
        let ts = s.trim_start();
        let low = ts.to_lowercase();
        if low.starts_with(".model") {
            let c: Vec<char> = ts.chars().collect();
            let mut i = 6;
            let ws = |k: usize| k < c.len() && c[k].is_whitespace();
            if ws(i) {
                while ws(i) {
                    i += 1;
                }
                let st = i;
                while i < c.len() && !c[i].is_whitespace() {
                    i += 1;
                }
                if i > st && ws(i) {
                    while ws(i) {
                        i += 1;
                    }
                    let st2 = i;
                    while i < c.len() && c[i].is_ascii_alphabetic() {
                        i += 1;
                    }
                    if i > st2 {
                        while ws(i) {
                            i += 1;
                        }
                        if i < c.len() && c[i] == '(' {
                            i += 1;
                        }
                        s = c[i..].iter().collect();
                    }
                }
            }
        }
    }
    let tr = s.trim_end();
    if let Some(r) = tr.strip_suffix(')') {
        s = r.to_string();
    }
    let s: String = s.chars().map(|ch| if ch == '(' || ch == ')' { ' ' } else { ch }).collect();
    let c: Vec<char> = s.chars().collect();
    let mut out = SpP::default();
    let mut bad = Vec::new();
    let mut rest = String::new();
    let mut i = 0;
    let mut last = 0;
    while i < c.len() {
        if c[i].is_ascii_alphabetic() || c[i] == '_' {
            let st = i;
            let mut j = i;
            while j < c.len() && (c[j].is_ascii_alphanumeric() || c[j] == '_') {
                j += 1;
            }
            let key: String = c[st..j].iter().collect();
            let mut k = j;
            while k < c.len() && c[k].is_whitespace() {
                k += 1;
            }
            if k < c.len() && c[k] == '=' {
                k += 1;
                while k < c.len() && c[k].is_whitespace() {
                    k += 1;
                }
                let vs = k;
                while k < c.len() && !c[k].is_whitespace() && c[k] != '=' && c[k] != ',' {
                    k += 1;
                }
                if k > vs {
                    let val: String = c[vs..k].iter().collect();
                    let whole: String = c[st..k].iter().collect();
                    rest.push_str(&c[last..st].iter().collect::<String>());
                    rest.push(' ');
                    last = k;
                    let ku = key.to_uppercase();
                    if !["LEVEL", "MFG", "TYPE", "VCEO", "ICRATING", "IAVE", "VPK", "IPK", "IRATING"].contains(&ku.as_str()) {
                        let v = spice_num(&val);
                        if !v.is_finite() {
                            bad.push(whole);
                        } else {
                            out.set(&ku, v);
                        }
                    }
                    i = k;
                    continue;
                }
            }
            i += 1;
            continue;
        }
        i += 1;
    }
    rest.push_str(&c[last..].iter().collect::<String>());
    let rest = rest.trim();
    if !rest.is_empty() && rest.chars().any(|ch| ch.is_ascii_alphanumeric()) {
        bad.push(rest.chars().take(40).collect());
    }
    (out, bad)
}

/* ------------------------------------------------------------------ junction helpers */
#[inline]
pub fn pnjlim(mut vnew: f64, vold: f64, vt: f64, vcrit: f64) -> f64 {
    if vnew > vcrit && (vnew - vold).abs() > 2.0 * vt {
        if vold > 0.0 {
            let arg = 1.0 + (vnew - vold) / vt;
            vnew = if arg > 0.0 { vold + vt * arg.ln() } else { vcrit };
        } else {
            vnew = vt * (vnew / vt).ln();
        }
    }
    vnew
}
/// safe exp with linear continuation (keeps Newton finite)
#[inline]
pub fn sexp(x: f64) -> (f64, f64) {
    if x > EXPLIM {
        let e = EXPLIM.exp();
        return (e * (1.0 + x - EXPLIM), e);
    }
    let e = x.exp();
    (e, e)
}
/// depletion charge and capacitance (SPICE)
pub fn dep_q(v: f64, cj0: f64, vj: f64, m: f64, fc: f64) -> (f64, f64) {
    if !(cj0 > 0.0) {
        return (0.0, 0.0);
    }
    let m = m.min(0.99);
    let vj = vj.max(0.05);
    if v < fc * vj {
        let arg = 1.0 - v / vj;
        let sarg = arg.powf(-m);
        return (vj * cj0 * (1.0 - arg * sarg) / (1.0 - m), cj0 * sarg);
    }
    let f1 = vj * (1.0 - (1.0 - fc).powf(1.0 - m)) / (1.0 - m);
    let f2 = (1.0 - fc).powf(1.0 + m);
    let f3 = 1.0 - fc * (1.0 + m);
    let v0 = fc * vj;
    (cj0 * f1 + cj0 / f2 * (f3 * (v - v0) + m / (2.0 * vj) * (v * v - v0 * v0)), cj0 / f2 * (f3 + m * v / vj))
}
fn ignored_warn(b: &mut Builder, name: &str, p: &SpP, known: &[&str]) {
    let ig: Vec<&str> = p.0.iter().map(|(k, _)| k.as_str()).filter(|k| !known.contains(k)).collect();
    if !ig.is_empty() {
        b.warn(format!("{}：已忽略 SPICE 參數 {}（溫度相關與高階效應未建模，模擬溫度固定 27 °C）。", name, ig.join(", ")));
    }
}

/* charge bookkeeping: for charge j at offset o: s[o] = q at last accepted point, s[o+1] = its current, s[o+2] = flip flag */
#[inline]
fn qk(h: f64, be: bool) -> f64 {
    (if be { 1.0 } else { 2.0 }) / h
}
#[inline]
fn q_cur(s: &[f64], o: usize, q: f64, h: f64, be: bool) -> f64 {
    qk(h, be) * (q - s[o]) - if be { 0.0 } else { s[o + 1] }
}
/// accept a charge state; resets trapezoidal ringing when the charge current flips sign on consecutive steps
fn q_acc(s: &mut [f64], o: usize, q: f64, h: f64, be: bool) {
    let it = q_cur(s, o, q, h, be);
    let flip = !be && h > 0.0 && it * s[o + 1] < 0.0;
    if flip && s[o + 2] > 0.0 {
        let ib = (q - s[o]) / h;
        if ib.abs() < 0.25 * it.abs() {
            s[o] = q;
            s[o + 1] = ib;
            s[o + 2] = 0.0;
            return;
        }
    }
    s[o] = q;
    s[o + 1] = it;
    s[o + 2] = if flip { 1.0 } else { 0.0 };
}

/* ------------------------------------------------------------------ Diode */
const D_KNOWN: &[&str] = &["IS", "N", "RS", "BV", "IBV", "NBV", "CJO", "CJ0", "VJ", "M", "FC", "TT", "EG", "XTI"];
#[derive(Clone)]
pub struct SpDiode {
    a: i32,
    pub k: i32,
    pub ai: i32,
    pub is: f64,
    n: f64,
    rs: f64,
    bv: f64,
    ibv: f64,
    nbv: f64,
    cjo: f64,
    vj: f64,
    m: f64,
    fc: f64,
    tt: f64,
    pub vte: f64,
    vcrit: f64,
    has_q: bool,
    so: usize,
    v_old: f64,
    limited: bool,
    pub slot: Option<usize>,
}
impl SpDiode {
    pub fn new(b: &mut Builder, name: &str, id: i64, a: i32, k: i32, p: &SpP, bv_opt: Option<f64>) -> R<Self> {
        let is = p.g("IS", 1e-14);
        let n = p.g("N", 1.0);
        let rs = p.g("RS", 0.0);
        let mut bv = p.get("BV").unwrap_or(bv_opt.unwrap_or(f64::INFINITY));
        let ibv = p.g("IBV", 1e-3);
        let nbv = p.g("NBV", n);
        let cjo = p.get("CJO").unwrap_or(p.g("CJ0", 0.0));
        if !(is > 0.0) || !(n > 0.0) {
            return Err(SimError::el(format!("{}：SPICE 參數 IS、N 必須大於 0", name), id));
        }
        if !(bv > 0.0) {
            bv = f64::INFINITY;
        }
        let vte = n * VT;
        let vcrit = vte * (vte / (std::f64::consts::SQRT_2 * is)).ln();
        let ai = if rs > 0.0 { b.node() } else { a };
        let tt = p.g("TT", 0.0);
        let has_q = cjo > 0.0 || tt > 0.0;
        let so = b.req_p(3);
        Ok(SpDiode { a, k, ai, is, n, rs, bv, ibv, nbv, cjo, vj: p.g("VJ", 1.0), m: p.g("M", 0.5), fc: p.g("FC", 0.5), tt, vte, vcrit, has_q, so, v_old: 0.0, limited: false, slot: None })
    }
    fn vd(&self, x: &[f64]) -> f64 {
        vx(x, self.ai) - vx(x, self.k)
    }
    fn stat(&self, v: f64) -> (f64, f64) {
        let (e, de) = sexp(v / self.vte);
        let mut i = self.is * (e - 1.0);
        let mut g = self.is * de / self.vte;
        if self.bv < f64::INFINITY {
            let vtb = self.nbv * VT;
            let (e2, de2) = sexp(-(v + self.bv) / vtb);
            i -= self.ibv * e2;
            g += self.ibv * de2 / vtb;
        }
        (i + GMIN * v, g + GMIN)
    }
    fn charge(&self, v: f64, id: f64, gd: f64) -> (f64, f64) {
        let (q, c) = dep_q(v, self.cjo, self.vj, self.m, self.fc);
        (q + self.tt * id, c + self.tt * gd)
    }
    fn limit(&self, v: f64) -> f64 {
        let mut vl = pnjlim(v, self.v_old, self.vte, self.vcrit);
        if self.bv < f64::INFINITY && v < 0f64.min(-self.bv + 10.0 * self.nbv * VT) {
            let vtb = self.nbv * VT;
            let r = -(v + self.bv);
            let r = pnjlim(r, -(self.v_old + self.bv), vtb, vtb * (vtb / (std::f64::consts::SQRT_2 * self.ibv)).ln());
            vl = -(r + self.bv);
        }
        vl
    }
    fn set_vold(&mut self, v: f64, e: &mut E) {
        self.v_old = v;
        if let Some(s) = self.slot {
            e.scratch[s] = v;
        }
    }
}
impl Inst for SpDiode {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, nl: true, nl_init: true, ..Default::default() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.ai, self.k]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for i in 0..3 {
            e.ps[self.so + i] = 0.0;
        }
        Ok(())
    }
    fn nl_guess(&mut self, _x: &[f64], xr: &[f64], e: &mut E) {
        let v = self.vd(xr);
        self.set_vold(v, e);
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        if self.rs > 0.0 {
            m.g(self.a, self.ai, 1.0 / self.rs);
        }
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, h: f64, be: bool, init: bool, e: &mut E) {
        let v0 = self.vd(x);
        let v = self.limit(v0);
        self.limited = (v - v0).abs() > 1e-9 * v0.abs().max(1.0);
        self.set_vold(v, e);
        let (mut i, mut g) = self.stat(v);
        if self.has_q && !init {
            let (q, c) = self.charge(v, i, g);
            let k = qk(h, be);
            i += q_cur(&e.ps, self.so, q, h, be);
            g += k * c;
        }
        m.g(self.ai, self.k, g);
        ai(b, self.ai, self.k, i - g * v);
    }
    fn nl_ok(&self) -> bool {
        !self.limited
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        if !self.has_q {
            return;
        }
        let v = self.vd(x);
        let (i, g) = self.stat(v);
        let (q, _) = self.charge(v, i, g);
        q_acc(&mut e.ps, self.so, q, h, be);
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        if !self.has_q {
            return;
        }
        let v = self.vd(x);
        let (i, g) = self.stat(v);
        e.ps[self.so] = self.charge(v, i, g).0;
        e.ps[self.so + 1] = 0.0;
        e.ps[self.so + 2] = 0.0;
    }
}

/* ------------------------------------------------------------------ Gummel-Poon BJT */
const Q_KNOWN: &[&str] = &["IS", "BF", "BR", "NF", "NR", "VAF", "VA", "VAR", "VB", "IKF", "IK", "IKR", "ISE", "NE", "ISC", "NC", "RB", "RC", "RE", "CJE", "VJE", "PE", "MJE", "ME", "CJC", "VJC", "PC", "MJC", "MC", "FC", "TF", "TR", "XCJC"];
#[derive(Clone)]
pub struct SpBjt {
    pol: f64,
    c: i32,
    b: i32,
    e_: i32,
    is: f64,
    bf: f64,
    br: f64,
    nf: f64,
    nr: f64,
    i_vaf: f64,
    i_var: f64,
    i_ikf: f64,
    i_ikr: f64,
    ise: f64,
    ne: f64,
    isc: f64,
    nc: f64,
    rb: f64,
    rc: f64,
    re: f64,
    cje: f64,
    vje: f64,
    mje: f64,
    cjc: f64,
    vjc: f64,
    mjc: f64,
    fc: f64,
    tf: f64,
    tr: f64,
    ci: i32,
    bi: i32,
    ei: i32,
    vcrit: f64,
    so: usize,
    has_q: bool,
    vbe_o: f64,
    vbc_o: f64,
    limited: bool,
}
impl SpBjt {
    #[allow(clippy::too_many_arguments)]
    pub fn new(bd: &mut Builder, name: &str, id: i64, c: i32, b: i32, e: i32, pol: f64, p: &SpP) -> R<Self> {
        let is = p.g("IS", 1e-16);
        let bf = p.g("BF", 100.0);
        let br = p.g("BR", 1.0);
        let vaf = p.get("VAF").unwrap_or(p.g("VA", 0.0));
        let var_ = p.get("VAR").unwrap_or(p.g("VB", 0.0));
        let ikf = p.get("IKF").unwrap_or(p.g("IK", 0.0));
        let ikr = p.g("IKR", 0.0);
        if !(is > 0.0) || !(bf > 0.0) || !(br > 0.0) {
            return Err(SimError::el(format!("{}：SPICE 參數 IS、BF、BR 必須大於 0", name), id));
        }
        let (rb, rc, re) = (p.g("RB", 0.0), p.g("RC", 0.0), p.g("RE", 0.0));
        let ci = if rc > 0.0 { bd.node() } else { c };
        let bi = if rb > 0.0 { bd.node() } else { b };
        let ei = if re > 0.0 { bd.node() } else { e };
        let so = bd.req_p(6);
        let (cje, cjc, tf, tr) = (p.g("CJE", 0.0), p.g("CJC", 0.0), p.g("TF", 0.0), p.g("TR", 0.0));
        Ok(SpBjt {
            pol,
            c,
            b,
            e_: e,
            is,
            bf,
            br,
            nf: p.g("NF", 1.0),
            nr: p.g("NR", 1.0),
            i_vaf: if vaf > 0.0 { 1.0 / vaf } else { 0.0 },
            i_var: if var_ > 0.0 { 1.0 / var_ } else { 0.0 },
            i_ikf: if ikf > 0.0 { 1.0 / ikf } else { 0.0 },
            i_ikr: if ikr > 0.0 { 1.0 / ikr } else { 0.0 },
            ise: p.g("ISE", 0.0),
            ne: p.g("NE", 1.5),
            isc: p.g("ISC", 0.0),
            nc: p.g("NC", 2.0),
            rb,
            rc,
            re,
            cje,
            vje: p.get("VJE").unwrap_or(p.g("PE", 0.75)),
            mje: p.get("MJE").unwrap_or(p.g("ME", 0.33)),
            cjc,
            vjc: p.get("VJC").unwrap_or(p.g("PC", 0.75)),
            mjc: p.get("MJC").unwrap_or(p.g("MC", 0.33)),
            fc: p.g("FC", 0.5),
            tf,
            tr,
            ci,
            bi,
            ei,
            vcrit: VT * (VT / (std::f64::consts::SQRT_2 * is)).ln(),
            so,
            has_q: cje > 0.0 || cjc > 0.0 || tf > 0.0 || tr > 0.0,
            vbe_o: 0.0,
            vbc_o: 0.0,
            limited: false,
        })
    }
    fn jv(&self, x: &[f64]) -> (f64, f64) {
        let vb = vx(x, self.bi);
        (self.pol * (vb - vx(x, self.ei)), self.pol * (vb - vx(x, self.ci)))
    }
    /// static model with analytic derivatives (NPN sense, currents into C and B).
    /// F = [Ic, Ib, Qbe, Qbc], J = d[Ic,Ib,Qbe,Qbc]/d[vbe,vbc] (row-major 4×2).
    pub fn f(&self, vbe: f64, vbc: f64) -> ([f64; 4], [f64; 8]) {
        let mut ff = [0.0; 4];
        let mut j = [0.0; 8];
        let vtf = self.nf * VT;
        let vtr = self.nr * VT;
        let (ebe, _) = sexp(vbe / vtf);
        let (ebc, _) = sexp(vbc / vtr);
        let cbe = self.is * (ebe - 1.0);
        let cbc = self.is * (ebc - 1.0);
        let gbe = self.is * ebe / vtf;
        let gbc = self.is * ebc / vtr;
        let (mut cben, mut gben, mut cbcn, mut gbcn) = (0.0, 0.0, 0.0, 0.0);
        if self.ise > 0.0 {
            let vt = self.ne * VT;
            let (e, _) = sexp(vbe / vt);
            cben = self.ise * (e - 1.0);
            gben = self.ise * e / vt;
        }
        if self.isc > 0.0 {
            let vt = self.nc * VT;
            let (e, _) = sexp(vbc / vt);
            cbcn = self.isc * (e - 1.0);
            gbcn = self.isc * e / vt;
        }
        let den = 1.0 - vbc * self.i_vaf - vbe * self.i_var;
        let (q1, dq1e, dq1c) = if den > 1e-6 {
            let q1 = 1.0 / den;
            (q1, q1 * q1 * self.i_var, q1 * q1 * self.i_vaf)
        } else {
            (1e6, 0.0, 0.0)
        };
        let q2 = cbe * self.i_ikf + cbc * self.i_ikr;
        let dq2e = gbe * self.i_ikf;
        let dq2c = gbc * self.i_ikr;
        let sq = (1.0 + 4.0 * q2).sqrt();
        let qb = q1 * (1.0 + sq) / 2.0;
        let dqbe = dq1e * (1.0 + sq) / 2.0 + q1 * dq2e / sq;
        let dqbc = dq1c * (1.0 + sq) / 2.0 + q1 * dq2c / sq;
        let iqb = 1.0 / qb;
        let dif = cbe - cbc;
        ff[0] = dif * iqb - cbc / self.br - cbcn - GMIN * vbc;
        ff[1] = cbe / self.bf + cben + cbc / self.br + cbcn + GMIN * vbe + GMIN * vbc;
        j[0] = gbe * iqb - dif * iqb * iqb * dqbe;
        j[1] = -gbc * iqb - dif * iqb * iqb * dqbc - gbc / self.br - gbcn - GMIN;
        j[2] = gbe / self.bf + gben + GMIN;
        j[3] = gbc / self.br + gbcn + GMIN;
        if self.has_q {
            let (qe, ce) = dep_q(vbe, self.cje, self.vje, self.mje, self.fc);
            let (qc, cc) = dep_q(vbc, self.cjc, self.vjc, self.mjc, self.fc);
            ff[2] = qe + self.tf * cbe * iqb;
            ff[3] = qc + self.tr * cbc;
            j[4] = ce + self.tf * (gbe * iqb - cbe * iqb * iqb * dqbe);
            j[5] = self.tf * (-cbe * iqb * iqb * dqbc);
            j[6] = 0.0;
            j[7] = cc + self.tr * gbc;
        }
        (ff, j)
    }
}
impl Inst for SpBjt {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, nl: true, nl_init: true, ..Default::default() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.c, self.ci, self.b, self.bi, self.e_, self.ei]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for i in 0..6 {
            e.ps[self.so + i] = 0.0;
        }
        Ok(())
    }
    fn nl_guess(&mut self, _x: &[f64], xr: &[f64], _e: &mut E) {
        let (a, b) = self.jv(xr);
        self.vbe_o = a;
        self.vbc_o = b;
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        if self.rc > 0.0 {
            m.g(self.c, self.ci, 1.0 / self.rc);
        }
        if self.rb > 0.0 {
            m.g(self.b, self.bi, 1.0 / self.rb);
        }
        if self.re > 0.0 {
            m.g(self.e_, self.ei, 1.0 / self.re);
        }
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, h: f64, be: bool, init: bool, e: &mut E) {
        let (b0, c0) = self.jv(x);
        let vbe = pnjlim(b0, self.vbe_o, VT * self.nf, self.vcrit);
        let vbc = pnjlim(c0, self.vbc_o, VT * self.nr, self.vcrit);
        self.limited = (vbe - b0).abs() > 1e-9 || (vbc - c0).abs() > 1e-9;
        self.vbe_o = vbe;
        self.vbc_o = vbc;
        let (ff, j) = self.f(vbe, vbc);
        let (mut ic, mut ib, mut gcbe, mut gcbc, mut gbbe, mut gbbc) = (ff[0], ff[1], j[0], j[1], j[2], j[3]);
        if self.has_q && !init {
            let k = qk(h, be);
            let iqbe = q_cur(&e.ps, self.so, ff[2], h, be);
            let iqbc = q_cur(&e.ps, self.so + 3, ff[3], h, be);
            ic -= iqbc;
            ib += iqbe + iqbc;
            gcbe -= k * j[6];
            gcbc -= k * j[7];
            gbbe += k * (j[4] + j[6]);
            gbbc += k * (j[5] + j[7]);
        }
        let p = self.pol;
        let (bi, ei, ci) = (self.bi, self.ei, self.ci);
        let mut row = |xn: i32, i: f64, gbe: f64, gbc: f64| -> f64 {
            m.a(xn, bi, gbe + gbc);
            m.a(xn, ei, -gbe);
            m.a(xn, ci, -gbc);
            p * (i - gbe * vbe - gbc * vbc)
        };
        let r0 = row(ci, ic, gcbe, gcbc);
        let r1 = row(bi, ib, gbbe, gbbc);
        let r2 = row(ei, -(ic + ib), -(gcbe + gbbe), -(gcbc + gbbc));
        if ci >= 0 {
            b[ci as usize] -= r0;
        }
        if bi >= 0 {
            b[bi as usize] -= r1;
        }
        if ei >= 0 {
            b[ei as usize] -= r2;
        }
    }
    fn nl_ok(&self) -> bool {
        !self.limited
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        if !self.has_q {
            return;
        }
        let (vbe, vbc) = self.jv(x);
        let (ff, _) = self.f(vbe, vbc);
        q_acc(&mut e.ps, self.so, ff[2], h, be);
        q_acc(&mut e.ps, self.so + 3, ff[3], h, be);
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        if !self.has_q {
            return;
        }
        let (vbe, vbc) = self.jv(x);
        let (ff, _) = self.f(vbe, vbc);
        for i in 0..6 {
            e.ps[self.so + i] = 0.0;
        }
        e.ps[self.so] = ff[2];
        e.ps[self.so + 3] = ff[3];
    }
}

/* ------------------------------------------------------------------ Level-1 MOSFET */
const M_KNOWN: &[&str] = &["VTO", "VT0", "KP", "W", "L", "LAMBDA", "RD", "RS", "CGS", "CGD", "CDS", "CGSO", "CGDO", "CBD", "IS", "N", "RB", "CJO", "TT", "BV"];
#[derive(Clone, Copy)]
enum QF {
    Lin(f64),
    /// gate-drain: two-level capacitance Cmin (saturation, drain far above gate) → Cmax (linear region).
    /// The step is centred on Vgd = vc = Vth, the saturation / linear boundary (Vds = Vgs − Vth), so the
    /// Miller-plateau charge between Vds = Vq and the end of the plateau equals the datasheet Qgd.
    Gd { cmin: f64, cmax: f64, vk: f64, vc: f64 },
    /// drain-source junction-like 1/sqrt(1 + v/V1), constant below 0 V
    Ds { c0: f64, v1: f64 },
}
impl QF {
    fn q(&self, v: f64) -> (f64, f64) {
        match *self {
            QF::Lin(c) => (c * v, c),
            QF::Gd { cmin, cmax, vk, vc } => {
                let lncosh = |x: f64| {
                    let a = x.abs();
                    a + (-2.0 * a).exp().ln_1p() - std::f64::consts::LN_2
                };
                // charge referenced to Q(0) = 0
                let step = |u: f64| 0.5 * (u + vk * lncosh(u / vk));
                (cmin * v + (cmax - cmin) * (step(v - vc) - step(-vc)), cmin + (cmax - cmin) * 0.5 * (1.0 + ((v - vc) / vk).tanh()))
            }
            QF::Ds { c0, v1 } => {
                if v > 0.0 {
                    (2.0 * c0 * v1 * ((1.0 + v / v1).sqrt() - 1.0), c0 / (1.0 + v / v1).sqrt())
                } else {
                    (c0 * v, c0)
                }
            }
        }
    }
}
#[derive(Clone)]
pub struct SpMos {
    pol: f64,
    d: i32,
    g: i32,
    sn: i32,
    vto: f64,
    beta: f64,
    lam: f64,
    rd: f64,
    rs: f64,
    cgs: f64,
    cgd: f64,
    cds: f64,
    di: i32,
    si: i32,
    vs: f64,
    so: usize,
    has_q: bool,
    qf: [QF; 3],
}
impl SpMos {
    #[allow(clippy::too_many_arguments)]
    pub fn new(bd: &mut Builder, name: &str, id: i64, d: i32, g: i32, sn: i32, pol: f64, p: &SpP) -> R<Self> {
        let vto = p.get("VTO").unwrap_or(p.g("VT0", if pol > 0.0 { 2.0 } else { -2.0 })) * pol;
        let beta = p.g("KP", 2e-5) * p.g("W", 1.0) / p.g("L", 1.0);
        let lam = p.g("LAMBDA", 0.0);
        let (rd, rs) = (p.g("RD", 0.0), p.g("RS", 0.0));
        let cgs = p.get("CGS").unwrap_or(p.g("CGSO", 0.0));
        let cgd = p.get("CGD").unwrap_or(p.g("CGDO", 0.0));
        let cds = p.get("CDS").unwrap_or(p.g("CBD", 0.0));
        if !(beta > 0.0) {
            return Err(SimError::el(format!("{}：SPICE 參數 KP 必須大於 0", name), id));
        }
        let di = if rd > 0.0 { bd.node() } else { d };
        let si = if rs > 0.0 { bd.node() } else { sn };
        let so = bd.req_p(9);
        let mut qgd = QF::Lin(cgd);
        let mut qds = QF::Lin(cds);
        let qgd_ = p.g("QGD", 0.0);
        let vq = p.g("VQ", 400.0);
        let vk = 2.0;
        if qgd_ > 0.0 && cgd > 0.0 {
            let cmin = cgd;
            let cmax = cmin.max(cmin + (qgd_ - cmin * vq) / (vk * std::f64::consts::LN_2));
            qgd = QF::Gd { cmin, cmax, vk, vc: vto.max(0.0) };
            if cmax == cmin {
                bd.warn(format!("{}：Qgd 小於 Crss·Vq，米勒電容視為定值 Crss。", name));
            }
        }
        let vcap = p.g("VCAP", 0.0);
        if vcap > 0.0 && cds > 0.0 {
            let v1 = 1.0;
            qds = QF::Ds { c0: cds * (1.0 + vcap / v1).sqrt(), v1 };
        }
        Ok(SpMos { pol, d, g, sn, vto, beta, lam, rd, rs, cgs, cgd, cds, di, si, vs: 1.5 * VT, so, has_q: cgs > 0.0 || cgd > 0.0 || cds > 0.0, qf: [QF::Lin(cgs), qgd, qds] })
    }
    fn cap_q(&self, j: usize, v: f64) -> (f64, f64) {
        let (q, c) = self.qf[j].q(self.pol * v);
        (self.pol * q, c)
    }
    /// device sense, vds >= 0 : (id, gm, gds)
    fn ids(&self, vgs: f64, vds: f64) -> (f64, f64, f64) {
        let u = (vgs - self.vto) / self.vs;
        let sp = if u > 30.0 { u } else { u.exp().ln_1p() };
        let dsp = if u > 30.0 { 1.0 } else { 1.0 / (1.0 + (-u).exp()) };
        let vov = self.vs * sp;
        let dvov = dsp;
        let cl = 1.0 + self.lam * vds;
        let b = self.beta;
        if vds < vov {
            let i0 = b * (vov * vds - vds * vds / 2.0);
            return (i0 * cl, b * vds * cl * dvov, b * (vov - vds) * cl + i0 * self.lam);
        }
        let i0 = b / 2.0 * vov * vov;
        (i0 * cl, b * vov * cl * dvov, i0 * self.lam)
    }
    fn caps_list(&self) -> [(i32, i32, f64, usize); 3] {
        [(self.g, self.si, self.cgs, 0), (self.g, self.di, self.cgd, 3), (self.di, self.si, self.cds, 6)]
    }
}
impl Inst for SpMos {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, nl: true, nl_init: true, ..Default::default() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.d, self.di, self.sn, self.si]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for i in 0..9 {
            e.ps[self.so + i] = 0.0;
        }
        Ok(())
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        if self.rd > 0.0 {
            m.g(self.d, self.di, 1.0 / self.rd);
        }
        if self.rs > 0.0 {
            m.g(self.sn, self.si, 1.0 / self.rs);
        }
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, h: f64, be: bool, init: bool, e: &mut E) {
        let p = self.pol;
        let (vd, vg, vsn) = (vx(x, self.di), vx(x, self.g), vx(x, self.si));
        let mut vds = p * (vd - vsn);
        let mut vgs = p * (vg - vsn);
        let (mut xn, mut yn) = (self.di, self.si);
        if vds < 0.0 {
            vgs = p * (vg - vd);
            vds = -vds;
            xn = self.si;
            yn = self.di;
        }
        let (id, gm, gds) = self.ids(vgs, vds);
        // current flows X -> Y; I = p*id(vgs, vds), vgs = p*(vG - vY), vds = p*(vX - vY)
        m.a(xn, self.g, gm);
        m.a(xn, yn, -gm - gds);
        m.a(xn, xn, gds);
        m.a(yn, self.g, -gm);
        m.a(yn, yn, gm + gds);
        m.a(yn, xn, -gds);
        m.g(self.di, self.si, GMIN);
        let ieq = p * (id - gm * vgs - gds * vds);
        if xn >= 0 {
            b[xn as usize] -= ieq;
        }
        if yn >= 0 {
            b[yn as usize] += ieq;
        }
        if self.has_q && !init {
            let k = qk(h, be);
            for (j, (a, c, cc, o)) in self.caps_list().into_iter().enumerate() {
                if !(cc > 0.0) {
                    continue;
                }
                let v = vx(x, a) - vx(x, c);
                let (q, cv) = self.cap_q(j, v);
                let i = q_cur(&e.ps, self.so + o, q, h, be);
                let g = k * cv;
                m.g(a, c, g);
                let r = i - g * v;
                if a >= 0 {
                    b[a as usize] -= r;
                }
                if c >= 0 {
                    b[c as usize] += r;
                }
            }
        }
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        if !self.has_q {
            return;
        }
        for (j, (a, c, cc, o)) in self.caps_list().into_iter().enumerate() {
            if !(cc > 0.0) {
                continue;
            }
            let q = self.cap_q(j, vx(x, a) - vx(x, c)).0;
            q_acc(&mut e.ps, self.so + o, q, h, be);
        }
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        if !self.has_q {
            return;
        }
        for (j, (a, c, cc, o)) in self.caps_list().into_iter().enumerate() {
            e.ps[self.so + o] = if cc > 0.0 { self.cap_q(j, vx(x, a) - vx(x, c)).0 } else { 0.0 };
            e.ps[self.so + o + 1] = 0.0;
            e.ps[self.so + o + 2] = 0.0;
        }
    }
}

/* ------------------------------------------------------------------ Open-collector comparator */
#[derive(Clone, Default)]
struct CmSt {
    on: bool,
    t0: f64,
    tot: f64,
    w: i64,
}
#[derive(Clone)]
pub struct CompOc {
    name: String,
    ip: i32,
    inn: i32,
    out: i32,
    vcc: i32,
    gnd: i32,
    vos: f64,
    ib: f64,
    isc: f64,
    vcmr: f64,
    vsmin: f64,
    iq: f64,
    roff: f64,
    a: f64,
    vk: f64,
    k: i32,
    so: usize,
    cm: [CmSt; 2],
    vce_o: f64,
    limited: bool,
}
impl CompOc {
    fn vd(&self, x: &[f64]) -> f64 {
        vx(x, self.ip) - vx(x, self.inn) + self.vos
    }
    fn fs(&self, vs: f64) -> (f64, f64) {
        let z = ((vs - self.vsmin) / 0.1).tanh();
        (0.5 * (1.0 + z), 0.5 * (1.0 - z * z) / 0.1)
    }
    fn report(&mut self, i: usize, t: f64, e: &mut E) {
        let st = &self.cm[i];
        let tot = st.tot + if st.on { t - st.t0 } else { 0.0 };
        let lbl = if i == 1 { "IN−" } else { "IN+" };
        let msg = format!(
            "{}：{} 超出輸入共模範圍上限（V+ − {} V），累計約 {} ms。超出範圍時實際元件的輸出狀態不保證（另一輸入在範圍內時通常仍正確）。",
            self.name,
            lbl,
            js_num_str(self.vcmr),
            js_to_precision(tot * 1e3, 3)
        );
        let prefix = format!("{}：{}", self.name, lbl);
        let w = self.cm[i].w;
        if w >= 0 && (w as usize) < e.warnings.len() && e.warnings[w as usize].starts_with(&prefix) {
            e.warnings[w as usize] = msg;
        } else {
            e.warn(msg.clone());
            self.cm[i].w = e.warnings.iter().position(|m| *m == msg).map(|p| p as i64).unwrap_or(-1);
        }
    }
}
impl Inst for CompOc {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, nl: true, nl_init: true, sw: true, qq: true, ..Default::default() }
    }
    fn fix_vars(&mut self, n: i32) {
        self.k += n;
    }
    fn lte_skip(&self) -> Vec<i32> {
        vec![self.k] // internal decision state, not a physical waveform
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.ip, self.vcc, self.gnd], vec![self.inn, self.vcc, self.gnd], vec![self.out, self.gnd]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = 0.0;
        e.ps[self.so + 1] = 0.0;
        Ok(())
    }
    fn state(&self, e: &E) -> i32 {
        e.ps[self.so + 1] as i32
    }
    fn set_state(&mut self, v: i32, e: &mut E) {
        e.ps[self.so + 1] = v as f64;
    }
    fn skey(&self, e: &E) -> u32 {
        e.ps[self.so + 1] as u32
    }
    fn desired(&self, x: &[f64], _t: f64, e: &E) -> i32 {
        let v = self.vd(x);
        if v > 0.0 {
            1
        } else if v < 0.0 {
            0
        } else {
            e.ps[self.so + 1] as i32
        }
    }
    fn qq(&self, xa: &[f64], xb: &[f64], _ns: i32, _e: &E) -> Option<(f64, f64)> {
        Some((self.vd(xa), self.vd(xb)))
    }
    fn tau(&self, xa: &[f64], xb: &[f64], _ns: i32, _e: &E) -> f64 {
        let (a, b) = (self.vd(xa), self.vd(xb));
        if a == b || (a > 0.0) == (b > 0.0) {
            NAN
        } else {
            a / (a - b)
        }
    }
    fn nl_guess(&mut self, x: &[f64], _xr: &[f64], _e: &mut E) {
        self.vce_o = vx(x, self.out) - vx(x, self.gnd);
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, h: f64, _be: bool, init: bool, e: &mut E) {
        let (k, g) = (self.k, self.gnd);
        // y equation: target ±3 with the state clamped to ±1 (crossing at tpd, rail reached in finite time)
        let tgt = if e.ps[self.so + 1] != 0.0 { 3.0 } else { -3.0 };
        let by;
        if init {
            m.a(k, k, 1.0);
            by = tgt / 3.0;
        } else {
            let ha = h * self.a;
            m.a(k, k, 1.0 + ha);
            by = e.ps[self.so] + ha * tgt;
        }
        // output sink current out -> gnd
        let y = x[k as usize].clamp(-1.0, 1.0);
        let ty = (y / 0.05).tanh();
        let sg = 0.5 * (1.0 - ty);
        let dsg = -0.5 * (1.0 - ty * ty) / 0.05;
        let vs = vx(x, self.vcc) - vx(x, g);
        let vce_r = vx(x, self.out) - vx(x, g);
        // limit the output-stage linearisation point (current-source region would let Newton overshoot)
        let vce = vce_r.max((-0.3f64).min(self.vce_o - 0.5));
        self.limited = vce != vce_r;
        self.vce_o = vce;
        let (f, df) = self.fs(vs);
        let tc = (vce / self.vk).tanh();
        let dtc = (1.0 - tc * tc) / self.vk;
        let i = self.isc * sg * f * tc;
        let diy = self.isc * dsg * f * tc;
        let dice = self.isc * sg * f * dtc + 1.0 / self.roff;
        let dis = self.isc * sg * df * tc;
        for (r, sgn) in [(self.out, 1.0), (g, -1.0)] {
            m.a(r, k, sgn * diy);
            m.a(r, self.out, sgn * dice);
            m.a(r, g, -sgn * dice);
            m.a(r, self.vcc, sgn * dis);
            m.a(r, g, -sgn * dis);
        }
        let ieq = i + vce / self.roff - diy * y - dice * vce - dis * vs;
        // quiescent supply current vcc -> gnd and input bias currents (vcc -> inputs)
        let iqv = self.iq * f;
        let diq = self.iq * df;
        m.a(self.vcc, self.vcc, diq);
        m.a(self.vcc, g, -diq);
        m.a(g, self.vcc, -diq);
        m.a(g, g, diq);
        b[k as usize] += by;
        ai(b, self.out, self.gnd, ieq);
        ai(b, self.vcc, self.gnd, iqv - diq * vs);
        let ibf = self.ib * f;
        ai(b, self.vcc, self.ip, ibf);
        ai(b, self.vcc, self.inn, ibf);
    }
    fn nl_ok(&self) -> bool {
        !self.limited
    }
    fn accept(&mut self, x: &[f64], t: f64, _h: f64, _be: bool, e: &mut E) {
        e.ps[self.so] = x[self.k as usize].clamp(-1.0, 1.0);
        // common-mode range monitor
        let g = vx(x, self.gnd);
        let vs = vx(x, self.vcc) - g;
        if !(vs > self.vsmin) {
            for i in 0..2 {
                if self.cm[i].on {
                    self.cm[i].on = false;
                    self.cm[i].tot += t - self.cm[i].t0;
                    self.report(i, t, e);
                }
            }
            return;
        }
        let lim = vs - self.vcmr;
        for (i, n) in [self.ip, self.inn].into_iter().enumerate() {
            let over = vx(x, n) - g > lim;
            if over && !self.cm[i].on {
                self.cm[i].on = true;
                self.cm[i].t0 = t;
            } else if !over && self.cm[i].on {
                self.cm[i].on = false;
                self.cm[i].tot += t - self.cm[i].t0;
                self.report(i, t, e);
            }
        }
    }
    fn finish(&mut self, t: f64, e: &mut E) {
        for i in 0..2 {
            if self.cm[i].on || self.cm[i].tot > 0.0 {
                self.report(i, t, e);
            }
        }
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        e.ps[self.so] = x[self.k as usize].clamp(-1.0, 1.0);
    }
}

/* ------------------------------------------------------------------ TL431 shunt regulator (behavioural) */
#[derive(Clone)]
pub struct Tl431 {
    kn: i32,
    an: i32,
    rn: i32,
    iref: f64,
    kvk: f64,
    k: i32,
    so: usize,
    gm: f64,
    w2: f64,
    glow: f64,
    wl: f64,
    c1: f64,
    goff: f64,
    tau: f64,
    r: f64,
    uc: f64,
    y_o: Option<f64>,
    v_o: f64,
    limited: bool,
}
fn spl(z: f64) -> f64 {
    if z > 40.0 {
        z
    } else if z < -40.0 {
        z.exp()
    } else {
        z.exp().ln_1p()
    }
}
fn lgs(z: f64) -> f64 {
    if z > 40.0 {
        1.0
    } else if z < -40.0 {
        z.exp()
    } else {
        1.0 / (1.0 + (-z).exp())
    }
}
impl Tl431 {
    fn core(&self, y: f64) -> (f64, f64) {
        let z1 = (y - self.c1) / self.wl;
        let z2 = y / self.w2;
        (self.glow * self.wl * spl(z1) + (self.gm - self.glow) * self.w2 * spl(z2), self.glow * lgs(z1) + (self.gm - self.glow) * lgs(z2))
    }
    fn isat(&self, v: f64) -> (f64, f64) {
        let z = (v - 1.6) / 0.05;
        (2.0 * 0.05 * spl(z) + 1e-12, 2.0 * lgs(z))
    }
    fn lv(&self, x: &[f64]) -> f64 {
        let va = vx(x, self.an);
        vx(x, self.rn) - va - self.kvk * (vx(x, self.kn) - va)
    }
}
impl Inst for Tl431 {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, nl: true, nl_init: true, ..Default::default() }
    }
    fn fix_vars(&mut self, n: i32) {
        self.k += n;
    }
    fn lte_skip(&self) -> Vec<i32> {
        vec![self.k]
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.kn, self.an, self.rn]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = 0.0;
        e.ps[self.so + 1] = 0.0;
        Ok(())
    }
    fn nl_guess(&mut self, x: &[f64], _xr: &[f64], _e: &mut E) {
        let xx = x[self.k as usize];
        self.y_o = Some((1.0 - self.r) * xx + self.r * (self.lv(x) + self.uc));
        self.v_o = vx(x, self.kn) - vx(x, self.an);
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, h: f64, be: bool, init: bool, e: &mut E) {
        let (k, kn, an, rn, kv) = (self.k, self.kn, self.an, self.rn, self.kvk);
        let s0 = e.ps[self.so];
        let s1 = e.ps[self.so + 1];
        // error-amplifier state equation:  cX*X - a*L(v) = rhs
        let (a, rhs);
        if init {
            a = 1.0;
            rhs = self.uc;
            m.a(k, k, 1.0);
        } else if be {
            a = h / self.tau;
            rhs = s0 + a * self.uc;
            m.a(k, k, 1.0 + a);
        } else {
            a = 0.5 * h / self.tau;
            rhs = s0 + a * (self.uc + s1 - s0);
            m.a(k, k, 1.0 + a);
        }
        m.a(k, rn, -a);
        m.a(k, an, a * (1.0 - kv));
        m.a(k, kn, a * kv);
        // cathode current; output command Y = (1-r)·X + r·u
        let xx = x[k as usize];
        let vka_r = vx(x, kn) - vx(x, an);
        let l0 = self.lv(x);
        let r = self.r;
        let yr = (1.0 - r) * xx + r * (l0 + self.uc);
        let (mut y, mut vka) = (yr, vka_r);
        if let Some(yo) = self.y_o {
            let lim = |vn: f64, vo: f64, th: f64, w: f64| if vn > vo && vo < th && vn - vo > 2.0 * w { (vo + w * (1.0 + (vn - vo) / w).ln()).max(vn.min(th)) } else { vn };
            y = lim(y, yo, self.c1, self.wl);
            y = lim(y, yo, 0.0, self.w2 * 50.0);
            vka = lim(vka, self.v_o, 1.6, 0.05);
        }
        self.limited = (y - yr).abs() > 1e-12 || (vka - vka_r).abs() > 1e-9;
        self.y_o = Some(y);
        self.v_o = vka;
        let (c, dc) = self.core(y);
        let (is, dis) = self.isat(vka);
        let q = c + is;
        let i = c * is / q + vka * self.goff;
        let gy = dc * is * is / (q * q);
        let gv = dis * c * c / (q * q) + self.goff;
        let gx = gy * (1.0 - r);
        let gl = gy * r;
        for (row, sg) in [(kn, 1.0), (an, -1.0)] {
            m.a(row, k, sg * gx);
            m.a(row, rn, sg * gl);
            m.a(row, an, sg * gl * (-1.0 + kv));
            m.a(row, kn, -sg * gl * kv);
        }
        m.g(kn, an, gv);
        let ieq = i + gy * (r * self.uc - y) - gv * vka;
        // REF input bias current (REF -> A)
        let vr = vx(x, rn) - vx(x, an);
        let t2 = ((vr - 1.0) / 0.2).tanh();
        let ir = self.iref * 0.5 * (1.0 + t2) + GMIN * vr;
        let gr = self.iref * 0.5 * (1.0 - t2 * t2) / 0.2 + GMIN;
        m.g(rn, an, gr);
        b[k as usize] += rhs;
        ai(b, kn, an, ieq);
        ai(b, rn, an, ir - gr * vr);
    }
    fn nl_ok(&self) -> bool {
        !self.limited
    }
    fn accept(&mut self, x: &[f64], _t: f64, _h: f64, _be: bool, e: &mut E) {
        e.ps[self.so] = x[self.k as usize];
        e.ps[self.so + 1] = self.lv(x) + self.uc;
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        self.accept(x, 0.0, 0.0, true, e);
    }
}

/* ------------------------------------------------------------------ Optocoupler coupling */
#[derive(Clone)]
pub struct OptoLink {
    slot: usize,
    lai: i32,
    lk: i32,
    lvte: f64,
    lis: f64,
    outs: Vec<(i32, i32, f64)>,
}
impl Inst for OptoLink {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, nl: true, nl_init: true, ..Default::default() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        self.outs.iter().map(|o| vec![o.0, o.1]).collect()
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], _x: &[f64], _t: f64, _h: f64, _be: bool, _i: bool, e: &mut E) {
        let v = e.scratch[self.slot];
        let (ee, de) = sexp(v / self.lvte);
        let (i, g) = (self.lis * (ee - 1.0), self.lis * de / self.lvte);
        for &(c, bb, eta) in &self.outs {
            let gg = eta * g;
            m.a(c, self.lai, gg);
            m.a(c, self.lk, -gg);
            m.a(bb, self.lai, -gg);
            m.a(bb, self.lk, gg);
            ai(b, c, bb, eta * i - gg * v);
        }
    }
}
pub fn opto(ctx: &mut ElCtx, n_out: usize) -> R<Vec<Box<dyn Inst>>> {
    let (a, k) = (ctx.p(0)?, ctx.p(1)?);
    let vf = ctx.pos("vf")?;
    let ifv = ctx.pos("ifv")?;
    let ifn = ctx.pos("ifn")?;
    let bf = ctx.posd("bf", 400.0)?;
    let ccb = ctx.numd("ccb", 5e-12)?;
    let tf = ctx.numd("tf", 5.5e-9)?;
    let (n, rs) = (1.5, 1.0);
    let vj = vf - ifv * rs;
    if !(vj > 0.3) {
        return Err(ctx.err("LED Vf 與測試電流不合理（扣除串聯電阻後的接面電壓過低）"));
    }
    let name = ctx.name.clone();
    let id = ctx.id;
    let mut led = SpDiode::new(ctx.b, &name, id, a, k, &SpP::from(&[("IS", ifv / (vj / (n * VT)).exp()), ("N", n), ("RS", rs), ("CJO", 30e-12), ("VJ", 1.2), ("M", 0.4)]), None)?;
    let slot = ctx.b.req_scratch();
    led.slot = Some(slot);
    let (lai, lk, lvte, lis) = (led.ai, led.k, led.vte, led.is);
    let mut list: Vec<Box<dyn Inst>> = vec![Box::new(led)];
    let mut outs = Vec::new();
    for j in 0..n_out {
        let key = if n_out == 1 { "ctr".to_string() } else { format!("ctr{}", j + 1) };
        let ctr = ctx.num(&key)? / 100.0;
        if !(ctr > 0.0) {
            return Err(ctx.err("CTR 必須大於 0"));
        }
        let (c, e) = (ctx.p(2 + 2 * j)?, ctx.p(3 + 2 * j)?);
        let bn = ctx.b.node();
        let q = SpBjt::new(
            ctx.b,
            &name,
            id,
            c,
            bn,
            e,
            1.0,
            &SpP::from(&[("IS", 1e-14), ("BF", bf), ("BR", 0.1), ("NF", 1.0), ("VAF", 100.0), ("IKF", 0.1), ("RC", 5.0), ("CJE", 10e-12), ("CJC", ccb), ("VJC", 0.75), ("MJC", 0.33), ("TF", tf), ("TR", 100e-9)]),
        )?;
        // calibrate eta so that Ic(terminal) = CTR*IF at IF = ifn, VCE = 5 V (base open)
        let ic_of = |ip: f64| {
            let (mut lo, mut hi) = (0.0, 1.3);
            for _ in 0..80 {
                let m = 0.5 * (lo + hi);
                let f = q.f(m, m - 5.0).0;
                if f[1] > ip {
                    hi = m;
                } else {
                    lo = m;
                }
            }
            let vb = 0.5 * (lo + hi);
            q.f(vb, vb - 5.0).0[0] + ip
        };
        let target = ctr * ifn;
        let (mut lo, mut hi) = ((target * 1e-6).ln(), (target * 2.0).ln());
        for _ in 0..60 {
            let m = 0.5 * (lo + hi);
            if ic_of(m.exp()) > target {
                hi = m;
            } else {
                lo = m;
            }
        }
        outs.push((c, bn, (0.5 * (lo + hi)).exp() / ifn));
        list.push(Box::new(q));
    }
    list.push(Box::new(OptoLink { slot, lai, lk, lvte, lis, outs }));
    Ok(list)
}

pub fn tl431(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let vref = ctx.pos("vref")?;
    let iref = ctx.numd("iref", 2e-6)?.max(0.0);
    let imin = ctx.pos("imin")?;
    let zka = ctx.pos("zka")?;
    let kvk = ctx.numd("kvk", -1.4e-3)?;
    let fp = ctx.pos("fp")?;
    let zhf = ctx.posd("zhf", 10.0)?;
    let (kn, an, rn) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?);
    let k = ctx.b.var();
    let so = ctx.b.req_p(2);
    let mut t = Tl431 {
        kn,
        an,
        rn,
        iref,
        kvk,
        k,
        so,
        gm: 1.0 / zka,
        w2: 2e-5,
        glow: 0.03,
        wl: 1.5e-3,
        c1: -imin / 0.03,
        goff: 1e-9,
        tau: 1.0 / (2.0 * std::f64::consts::PI * fp),
        r: 0.5f64.min(zka / zhf),
        uc: 0.0,
        y_o: None,
        v_o: 0.0,
        limited: false,
    };
    // x0: core current = 10 mA
    let (mut lo, mut hi) = (-0.2, 0.2);
    for _ in 0..200 {
        let m = 0.5 * (lo + hi);
        if t.core(m).0 > 10e-3 {
            hi = m;
        } else {
            lo = m;
        }
    }
    let x0 = 0.5 * (lo + hi);
    t.uc = -vref + kvk * vref + x0;
    let name = ctx.name.clone();
    let d = SpDiode::new(ctx.b, &name, ctx.id, an, kn, &SpP::from(&[("IS", 1e-14), ("N", 1.0)]), None)?;
    Ok(vec![Box::new(t), Box::new(d)])
}

/* ------------------------------------------------------------------ Gate driver for the non-ideal switches */
#[derive(Clone)]
pub struct GateDrv {
    gi: i32,
    sn: i32,
    gn: i32,
    pub von: f64,
    pub voff: f64,
    ginv: bool,
    pub gc: f64,
    pub so: usize,
}
impl Inst for GateDrv {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { p: true, ..Default::default() }
    }
    fn cins(&self) -> Vec<i32> {
        vec![self.gn]
    }
    fn gate_node(&self) -> i32 {
        self.gn
    }
    fn so(&self) -> usize {
        self.so
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.gi, self.sn]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = 0.0;
        e.ps[self.so + 1] = 0.0;
        e.ps[self.so + 2] = -1.0;
        Ok(())
    }
    fn on_gate(&mut self, g: i32, _init: bool, e: &mut E) -> bool {
        let on = if self.ginv { 1 - g } else { g };
        if on as f64 != e.ps[self.so] {
            e.ps[self.so] = on as f64;
            return true;
        }
        false
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        m.g(self.gi, self.sn, self.gc);
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        let v = if e.ps[self.so] != 0.0 { self.von } else { self.voff };
        ai(b, self.sn, self.gi, v * self.gc);
    }
}
fn dt_warn(ctx: &mut ElCtx, tg: f64, what: &str) {
    let dt = ctx.b.dt;
    if dt > tg {
        let msg = format!(
            "{}：{} ≈ {} ns，時間步長 {} ns 太大，切換暫態（米勒平台、切換損失、反向恢復）無法解析；結果仍穩定，但切換細節與損失不準。建議時間步長 ≤ {} ns。",
            ctx.name,
            what,
            js_to_precision(tg * 1e9, 3),
            js_to_precision(dt * 1e9, 3),
            js_to_precision(tg / 5.0 * 1e9, 2)
        );
        ctx.b.warn(msg);
    }
}
fn body_diode(ctx: &mut ElCtx, a: i32, k: i32) -> R<SpDiode> {
    let vf = ctx.pos("vfd")?;
    let iff = ctx.pos("ifd")?;
    let tt = ctx.numd("ttd", 0.0)?.max(0.0);
    let name = ctx.name.clone();
    SpDiode::new(ctx.b, &name, ctx.id, a, k, &SpP::from(&[("IS", iff / (vf / VT).exp()), ("N", 1.0), ("TT", tt)]), None)
}
pub fn ni_mos(ctx: &mut ElCtx, pol: f64) -> R<Vec<Box<dyn Inst>>> {
    let (d, s, gn) = (ctx.p(0)?, ctx.p(1)?, ctx.c(2)?);
    let ron = ctx.pos("ron")?;
    let vth = ctx.pos("vth")?;
    let von = ctx.num("von")?;
    let voff = ctx.numd("voff", 0.0)?;
    let rg = ctx.pos("rg")?;
    let ciss = ctx.pos("ciss")?;
    let crss = ctx.num("crss")?;
    let coss = ctx.num("coss")?;
    if !(crss >= 0.0 && crss < ciss) {
        return Err(ctx.err("Crss 必須介於 0 與 Ciss 之間"));
    }
    if !(coss >= crss) {
        return Err(ctx.err("Coss 不可小於 Crss"));
    }
    if !(von > vth) {
        return Err(ctx.err("閘極驅動電壓必須大於臨界電壓"));
    }
    let kp = 1.0 / (ron * (von - vth));
    let gi = ctx.b.node();
    let name = ctx.name.clone();
    let id = ctx.id;
    let mut dd = d;
    let mut amm = None;
    if ctx.b.dev_want.contains(&name) {
        dd = ctx.b.node();
        amm = Some(Amm::new(ctx.b, d, dd));
    }
    let lam = ctx.numd("lam", 0.0)?;
    let qgd = ctx.numd("qgd", 0.0)?;
    let vq = ctx.numd("vq", 400.0)?;
    let vcap = ctx.numd("vcap", 0.0)?;
    let ch = SpMos::new(ctx.b, &name, id, dd, gi, s, pol, &SpP::from(&[("VTO", pol * vth), ("KP", kp), ("LAMBDA", lam), ("CGS", ciss - crss), ("CGD", crss), ("CDS", coss - crss), ("QGD", qgd), ("VQ", vq), ("VCAP", vcap)]))?;
    let drv = GateDrv { gi, sn: s, gn, von: pol * von, voff: pol * voff, ginv: pol < 0.0, gc: 1.0 / rg, so: ctx.b.req_p(3) };
    dt_warn(ctx, rg * ciss, "閘極時間常數 Rg·Ciss");
    let bd = if pol > 0.0 { body_diode(ctx, s, dd)? } else { body_diode(ctx, dd, s)? };
    let mut list: Vec<Box<dyn Inst>> = Vec::new();
    if let Some(am) = amm {
        ctx.b.devs.insert(name, DevInfo { g: gi, d, s, amm_k: am.k, drv_so: drv.so, drv_gc: drv.gc, drv_von: drv.von, drv_voff: drv.voff });
        list.push(Box::new(am));
    }
    list.push(Box::new(drv));
    list.push(Box::new(ch));
    list.push(Box::new(bd));
    Ok(list)
}
/// IGBT = MOSFET driving the base of a wide-base PNP (emitter = IGBT collector)
pub fn ni_igbt(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (c, e, gn) = (ctx.p(0)?, ctx.p(1)?, ctx.c(2)?);
    let vth = ctx.pos("vth")?;
    let vce = ctx.pos("vce")?;
    let icn = ctx.pos("icn")?;
    let beta = ctx.posd("beta", 0.5)?;
    let ttail = ctx.pos("ttail")?;
    let cies = ctx.pos("cies")?;
    let cres = ctx.num("cres")?;
    let coes = ctx.num("coes")?;
    let von = ctx.num("von")?;
    let voff = ctx.numd("voff", 0.0)?;
    let rg = ctx.pos("rg")?;
    if !(cres >= 0.0 && cres < cies) {
        return Err(ctx.err("Cres 必須介於 0 與 Cies 之間"));
    }
    if !(coes >= cres) {
        return Err(ctx.err("Coes 不可小於 Cres"));
    }
    if !(von > vth) {
        return Err(ctx.err("閘極驅動電壓必須大於臨界電壓"));
    }
    let veb = 0.8;
    let vds = vce - veb;
    let vov = von - vth;
    if !(vds > 0.05) {
        return Err(ctx.err("Vce(sat) 必須大於約 0.85 V（內部 PNP 射-基極壓降約 0.8 V）"));
    }
    if !(vds < vov) {
        return Err(ctx.err("Vce(sat) 過大或驅動電壓過低（MOS 通道未在線性區）"));
    }
    let imos = icn / (1.0 + beta);
    let kp = imos / (vov * vds - vds * vds / 2.0);
    let bn = ctx.b.node();
    let gi = ctx.b.node();
    let name = ctx.name.clone();
    let id = ctx.id;
    let mut cc = c;
    let mut amm = None;
    if ctx.b.dev_want.contains(&name) {
        cc = ctx.b.node();
        amm = Some(Amm::new(ctx.b, c, cc));
    }
    let qgd = ctx.numd("qgd", 0.0)?;
    let vq = ctx.numd("vq", 600.0)?;
    let vcap = ctx.numd("vcap", 0.0)?;
    let mos = SpMos::new(ctx.b, &name, id, bn, gi, e, 1.0, &SpP::from(&[("VTO", vth), ("KP", kp), ("CGS", cies - cres), ("CGD", cres), ("CDS", coes - cres), ("QGD", qgd), ("VQ", vq), ("VCAP", vcap)]))?;
    let pnp = SpBjt::new(ctx.b, &name, id, e, bn, cc, -1.0, &SpP::from(&[("IS", beta * imos / (veb / VT).exp()), ("BF", beta), ("BR", 0.1), ("NF", 1.0), ("TF", ttail / beta)]))?;
    let drv = GateDrv { gi, sn: e, gn, von, voff, ginv: false, gc: 1.0 / rg, so: ctx.b.req_p(3) };
    dt_warn(ctx, rg * cies, "閘極時間常數 Rg·Cies");
    let bd = body_diode(ctx, e, cc)?;
    let mut list: Vec<Box<dyn Inst>> = Vec::new();
    if let Some(am) = amm {
        ctx.b.devs.insert(name, DevInfo { g: gi, d: c, s: e, amm_k: am.k, drv_so: drv.so, drv_gc: drv.gc, drv_von: drv.von, drv_voff: drv.voff });
        list.push(Box::new(am));
    }
    list.push(Box::new(drv));
    list.push(Box::new(mos));
    list.push(Box::new(pnp));
    list.push(Box::new(bd));
    Ok(list)
}

/* ------------------------------------------------------------------ Device probe */
#[derive(Clone)]
pub struct DevProbe {
    name: String,
    target: String,
    q: String,
    o: i32,
    so: usize,
    dev: Option<DevInfo>,
    pv: f64,
    pe: f64,
}
impl Inst for DevProbe {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { c: true, probe: true, ac: true, ..Default::default() }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn dev_target(&self) -> Option<String> {
        Some(self.target.clone())
    }
    fn set_dev(&mut self, d: DevInfo) {
        self.dev = Some(d);
    }
    fn fix_vars(&mut self, n: i32) {
        if let Some(d) = &mut self.dev {
            d.amm_k += n;
        }
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        e.cs[self.so + 1] = 0.0;
        Ok(())
    }
    fn eval(&mut self, _t: f64, h: f64, e: &mut E, x: &[f64]) -> R<()> {
        let dv = self.dev.as_ref().unwrap();
        let vg = vx(x, dv.g) - vx(x, dv.s);
        let vd = vx(x, dv.d) - vx(x, dv.s);
        let id = x[dv.amm_k as usize];
        let ig = dv.drv_gc * ((if e.ps[dv.drv_so] != 0.0 { dv.drv_von } else { dv.drv_voff }) - vg);
        let p = vd * id;
        let en = if h > 0.0 { e.cs[self.so] + 0.5 * h * (p + e.cs[self.so + 1]) } else { e.cs[self.so] };
        self.pv = p;
        self.pe = en;
        e.cv[self.o as usize] = match self.q.as_str() {
            "vg" => vg,
            "vd" => vd,
            "id" => id,
            "ig" => ig,
            "e" => en,
            _ => p,
        };
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.pe;
        e.cs[self.so + 1] = self.pv;
    }
    fn pname(&self) -> String {
        self.name.clone()
    }
    fn value(&self, e: &E, _x: &[f64]) -> f64 {
        e.cv[self.o as usize]
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        c.a(r, r, 1.0, 0.0);
        Ok(())
    }
}
pub fn devp(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let target = ctx.raw_opt("el").unwrap_or_default().trim().to_string();
    let q = ctx.raw_opt("q").filter(|s| !s.is_empty()).unwrap_or_else(|| "p".into());
    let o = ctx.b.cnode();
    let so = ctx.b.req_c(2);
    Ok(vec![Box::new(DevProbe { name: ctx.name.clone(), target, q, o, so, dev: None, pv: 0.0, pe: 0.0 })])
}

/* ------------------------------------------------------------------ Presets */
pub fn preset(kind: &str, model: &str) -> Option<&'static str> {
    let list = presets(kind);
    list.iter().find(|(k, _)| *k == model).map(|(_, v)| *v)
}
pub fn presets(kind: &str) -> &'static [(&'static str, &'static str)] {
    match kind {
        "D" => &[
            ("1N4148", "IS=2.52n RS=0.568 N=1.752 CJO=4p M=0.4 TT=20n BV=100 IBV=100u"),
            ("1N4007", "IS=7.02767n RS=0.0341512 N=1.80803 BV=1100 IBV=0.1 CJO=39p VJ=0.7 M=0.5 FC=0.5 TT=100n"),
            ("1N5819", "IS=31.7u RS=0.051 N=1.373 CJO=259p M=0.35 VJ=0.38 BV=40 IBV=1m"),
            ("LEDRED", "IS=93.2p RS=42m N=3.73 CJO=2.97p VJ=0.75 M=0.333 TT=4.32u"),
            ("ZENER", "IS=5n N=1.2 RS=2 CJO=100p VJ=0.75 M=0.33 IBV=5m"),
        ],
        "NPN" => &[
            ("2N3904", "IS=6.734f VAF=74.03 BF=416.4 NE=1.259 ISE=6.734f IKF=66.78m BR=.7371 NC=2 ISC=0 IKR=0 RC=1 CJC=3.638p MJC=.3085 VJC=.75 FC=.5 CJE=4.493p MJE=.2593 VJE=.75 TR=239.5n TF=301.2p RB=10"),
            ("2N4401", "IS=26.03f VAF=90.7 BF=4.292K NE=1.244 ISE=26.03f IKF=.2061 BR=1.01 NC=2 ISC=0 IKR=0 RC=.5 CJC=11.01p MJC=.3763 VJC=.75 FC=.5 CJE=24.07p MJE=.3641 VJE=.75 TR=233.7n TF=466.5p RB=4.7"),
            ("2N2222A", "IS=14.34f VAF=74.03 BF=255.9 NE=1.307 ISE=14.34f IKF=.2847 BR=6.092 NC=2 ISC=0 IKR=0 RC=1 CJC=7.306p MJC=.3416 VJC=.75 FC=.5 CJE=22.01p MJE=.377 VJE=.75 TR=46.91n TF=411.1p RB=10"),
        ],
        "PNP" => &[("2N3906", "IS=1.41f VAF=18.7 BF=180.7 NE=1.5 ISE=0 IKF=80m BR=4.977 NC=2 ISC=0 IKR=0 RC=2.5 CJC=9.728p MJC=.5776 VJC=.75 FC=.5 CJE=8.063p MJE=.3677 VJE=.75 TR=33.42n TF=179.3p RB=10")],
        "NMOS" => &[("GENERIC_N", "VTO=2.1 KP=0.105 LAMBDA=0.01 CGS=25p CGD=5p CDS=10p")],
        "PMOS" => &[("GENERIC_P", "VTO=-2 KP=0.05 LAMBDA=0.01 CGS=30p CGD=6p CDS=12p")],
        _ => &[],
    }
}
/// comparator presets: vos, ib, tpd, isc, vol, iol, vcmr, vsmin, iq
pub fn comp_preset(model: &str) -> [f64; 9] {
    match model {
        "LM2903" => [0.0, 25e-9, 1.5e-6, 16e-3, 0.15, 4e-3, 1.5, 2.0, 0.2e-3],
        _ => [0.0, 25e-9, 1.3e-6, 16e-3, 0.15, 4e-3, 1.5, 2.0, 0.2e-3],
    }
}

/// Transit time TT for which a charge-control diode carrying `if_` and turned off at a constant `a` (A/s)
/// delivers the reverse-recovery charge `qrr` (the datasheet Qrr test): with i = IF − a·t,
/// Q(t) = TT·(IF − a·t) + a·TT²·(1 − e^(−t/TT)); recovery ends when Q(ts) = 0 and Qrr = a·(ts − t0)²/2, t0 = IF/a.
pub fn tt_from_qrr(qrr: f64, if_: f64, a: f64) -> Option<f64> {
    if !(qrr > 0.0 && if_ > 0.0 && a > 0.0) {
        return None;
    }
    let t0 = if_ / a;
    let q_of = |tt: f64| -> f64 {
        let q = |t: f64| tt * (if_ - a * t) + a * tt * tt * (-(-t / tt).exp_m1());
        // Q is positive at t0 and reaches zero before t0 + TT
        let (mut lo, mut hi) = (t0, t0 + tt);
        for _ in 0..200 {
            let m = 0.5 * (lo + hi);
            if q(m) > 0.0 {
                lo = m;
            } else {
                hi = m;
            }
        }
        let d = 0.5 * (lo + hi) - t0;
        0.5 * a * d * d
    };
    // Qrr grows monotonically with TT: bisection on log(TT)
    let (mut lo, mut hi) = (1e-15f64.ln(), 1.0f64.ln());
    if q_of(hi.exp()) < qrr {
        return None;
    }
    for _ in 0..200 {
        let m = 0.5 * (lo + hi);
        if q_of(m.exp()) < qrr {
            lo = m;
        } else {
            hi = m;
        }
    }
    Some((0.5 * (lo + hi)).exp())
}

fn spice_params(ctx: &mut ElCtx, kind: &str) -> R<SpP> {
    let model = ctx.raw("model", "ideal");
    if kind == "D" && model == "ds" {
        // datasheet parameters -> SPICE level-1 diode
        let vf = ctx.pos("vfd")?;
        let iff = ctx.pos("ifd")?;
        let rd = ctx.numd("rd", 0.0)?.max(0.0);
        let trr = ctx.numd("trr", 0.0)?.max(0.0);
        let qrr = ctx.numd("qrr", 0.0)?.max(0.0);
        let irr = if ctx.numd("irr", 0.0)? > 0.0 { ctx.num("irr")? } else { iff };
        let cj = ctx.numd("cjd", 0.0)?.max(0.0);
        let vcj = ctx.numd("vcj", 0.0)?.max(0.0);
        let bv = ctx.numd("bvd", 0.0)?;
        let vj = vf - iff * rd;
        if !(vj > 0.05) {
            return Err(ctx.err("Vf − If·Rd 必須大於約 0.05 V（Rd 太大）"));
        }
        let didt = ctx.numd("didt", 0.0)?.max(0.0) * 1e6; // A/µs → A/s
        let tt = if qrr > 0.0 && didt > 0.0 {
            tt_from_qrr(qrr, irr, didt).ok_or_else(|| ctx.err("無法由 Qrr、Irr 與 di/dt 求得 TT（請檢查數值）"))?
        } else if qrr > 0.0 {
            qrr / irr
        } else {
            trr
        };
        let mut p = SpP::from(&[("IS", iff / (vj / VT).exp_m1()), ("N", 1.0), ("RS", rd), ("TT", tt), ("CJO", cj * (1.0 + vcj / 0.7).sqrt()), ("VJ", 0.7), ("M", 0.5)]);
        if bv > 0.0 {
            p.set("BV", bv);
            p.set("IBV", 1e-3);
        }
        return Ok(p);
    }
    let mut txt = ctx.raw("spice", "");
    if txt.trim().is_empty() {
        if let Some(t) = preset(kind, &model) {
            txt = t.to_string();
        } else if model == "custom" {
            let msg = format!("{}：選擇自訂模型但未填 SPICE 參數，使用 SPICE 預設值。", ctx.name);
            ctx.b.warn(msg);
        } else {
            txt = presets(kind).first().map(|p| p.1.to_string()).unwrap_or_default();
        }
    }
    let (p, bad) = parse_spice_model(&txt);
    if !bad.is_empty() {
        return Err(ctx.err(format!("SPICE 參數無法解析：{}", bad.join("、"))));
    }
    Ok(p)
}
pub fn dev_diode(ctx: &mut ElCtx, a: i32, k: i32, bv: Option<f64>) -> R<Vec<Box<dyn Inst>>> {
    let p = spice_params(ctx, "D")?;
    let name = ctx.name.clone();
    ignored_warn(ctx.b, &name, &p, D_KNOWN);
    Ok(vec![Box::new(SpDiode::new(ctx.b, &name, ctx.id, a, k, &p, bv)?)])
}
/// LED as a SPICE diode without reverse breakdown: the LED is assumed never to be damaged, so it does not
/// conduct in reverse at any voltage (BV = ∞); a BV of a pasted model is ignored with a warning
pub fn dev_led(ctx: &mut ElCtx, a: i32, k: i32) -> R<Vec<Box<dyn Inst>>> {
    let mut p = spice_params(ctx, "D")?;
    let name = ctx.name.clone();
    if p.get("BV").is_some() {
        p.0.retain(|(key, _)| !matches!(key.as_str(), "BV" | "IBV" | "NBV"));
        ctx.b.warn(format!("{}：發光二極體不模擬逆向崩潰（BV 視為無限大），已忽略 BV、IBV。", name));
    }
    ignored_warn(ctx.b, &name, &p, D_KNOWN);
    Ok(vec![Box::new(SpDiode::new(ctx.b, &name, ctx.id, a, k, &p, None)?)])
}
pub fn dev_bjt(ctx: &mut ElCtx, pol: f64) -> R<Vec<Box<dyn Inst>>> {
    let p = spice_params(ctx, if pol > 0.0 { "NPN" } else { "PNP" })?;
    let name = ctx.name.clone();
    ignored_warn(ctx.b, &name, &p, Q_KNOWN);
    let (c, b, e) = (ctx.p(0)?, ctx.p(2)?, ctx.p(1)?);
    Ok(vec![Box::new(SpBjt::new(ctx.b, &name, ctx.id, c, b, e, pol, &p)?)])
}
pub fn dev_mos(ctx: &mut ElCtx, pol: f64) -> R<Vec<Box<dyn Inst>>> {
    let p = spice_params(ctx, if pol > 0.0 { "NMOS" } else { "PMOS" })?;
    let name = ctx.name.clone();
    ignored_warn(ctx.b, &name, &p, M_KNOWN);
    let (d, s, g) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?);
    let ch = SpMos::new(ctx.b, &name, ctx.id, d, g, s, pol, &p)?;
    let mut bd = SpP::from(&[("IS", p.g("IS", 1e-14)), ("N", p.g("N", 1.0)), ("RS", p.g("RB", 0.0)), ("CJO", p.g("CJO", 0.0)), ("TT", p.g("TT", 0.0))]);
    if let Some(bv) = p.get("BV") {
        if bv != 0.0 {
            bd.set("BV", bv);
        }
    }
    let body = if pol > 0.0 { SpDiode::new(ctx.b, &name, ctx.id, s, d, &bd, None)? } else { SpDiode::new(ctx.b, &name, ctx.id, d, s, &bd, None)? };
    Ok(vec![Box::new(ch), Box::new(body)])
}
pub fn comp_oc(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let model = ctx.raw("model", "LM393");
    let pre = comp_preset(&model);
    let (ip, inn, out, vcc, gnd) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?, ctx.p(3)?, ctx.p(4)?);
    let vos = ctx.numd("vos", pre[0])?;
    let ib = ctx.numd("ib", pre[1])?;
    let tau_y = 1e-12f64.max(ctx.numd("tpd", pre[2])? / (4.0f64 / 3.0).ln());
    let isc = 1e-6f64.max(ctx.numd("isc", pre[3])?);
    let vol = 1e-3f64.max(ctx.numd("vol", pre[4])?);
    let iol = 1e-9f64.max(ctx.numd("iol", pre[5])?);
    let vcmr = ctx.numd("vcmr", pre[6])?;
    let vsmin = ctx.numd("vsmin", pre[7])?;
    let iq = ctx.numd("iq", pre[8])?;
    if iol >= isc {
        return Err(ctx.err("Vol 量測電流必須小於短路吸入電流"));
    }
    let k = ctx.b.var();
    let so = ctx.b.req_p(2);
    let c = CompOc {
        name: ctx.name.clone(),
        ip,
        inn,
        out,
        vcc,
        gnd,
        vos,
        ib,
        isc,
        vcmr,
        vsmin,
        iq,
        roff: 1e10,
        a: 1.0 / tau_y,
        vk: vol / (0.95f64.min(iol / isc)).atanh(),
        k,
        so,
        cm: [CmSt { w: -1, ..Default::default() }, CmSt { w: -1, ..Default::default() }],
        vce_o: 0.0,
        limited: false,
    };
    // input clamp (substrate) diodes: GND -> each input
    let cl = SpP::from(&[("IS", 1e-14), ("N", 1.0)]);
    let name = ctx.name.clone();
    let d1 = SpDiode::new(ctx.b, &name, ctx.id, gnd, ip, &cl, None)?;
    let d2 = SpDiode::new(ctx.b, &name, ctx.id, gnd, inn, &cl, None)?;
    Ok(vec![Box::new(c), Box::new(d1), Box::new(d2)])
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn spice() {
        let (p, bad) = parse_spice_model(".model D1N4148 D(IS=2.52n RS=0.568 N=1.752 CJO=4p M=0.4 TT=20n BV=100 IBV=100u)");
        assert!(bad.is_empty(), "{:?}", bad);
        assert!((p.g("IS", 0.0) - 2.52e-9).abs() < 1e-20);
        assert_eq!(p.g("BV", 0.0), 100.0);
        assert!((spice_num("10MEG") - 1e7).abs() < 1e-6);
        assert!((spice_num("10M") - 1e-2).abs() < 1e-15);
        assert!((spice_num("4.292K") - 4292.0).abs() < 1e-9);
        let (_, bad) = parse_spice_model("IS=1n foo");
        assert_eq!(bad.len(), 1);
    }

    /// three-phase source (100 V line-to-line, 50 Hz) into a balanced 10 Ω star: the watt-hour meter reads
    /// P·t with P = 100² / 10 = 1 kW (J and Wh, with an initial reading)
    #[test]
    fn three_phase_watt_hour_meter() {
        use crate::netlist::{NetElement, Netlist};
        use crate::runner::{SimCtrl, TransientRunner};
        let el = |id: i64, typ: &str, name: &str, params: &[(&str, &str)], pins: &[i64]| NetElement { id, typ: typ.into(), name: name.into(), params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), pins: pins.to_vec() };
        // nets: 0 ground (source neutral), 1-3 phases, 4-6 after the meter, 7 load star
        let mut els = vec![el(1, "VSIN3", "S1", &[("V", "100"), ("f", "50"), ("ph", "0")], &[0, 1, 2, 3])];
        els.push(el(2, "WH3", "Wh", &[("unit", "J"), ("e0", "0")], &[1, 4, 2, 5, 3, 6, 0]));
        els.push(el(3, "WH3", "Wh2", &[("unit", "Wh"), ("e0", "2")], &[4, 8, 5, 9, 6, 10, 7]));
        for (j, a) in [8i64, 9, 10].iter().enumerate() {
            els.push(el(10 + j as i64, "R", &format!("R{}", j), &[("R", "10")], &[*a, 7]));
        }
        let net = Netlist { nets: vec!['G', 'P', 'P', 'P', 'P', 'P', 'P', 'P', 'P', 'P', 'P'], elements: els, param_text: String::new() };
        let sc = SimCtrl { dt: 10e-6, t_total: 0.1, t_print: 0.0, print_step: 1, interp: true, damp: true, lte: true, lte_tol: 1e-3, acc: false, etol: 1e-6, max_lv: 8 };
        let mut r = TransientRunner::new(&net, &sc).expect("netlist");
        while !r.run_steps(1_000_000).expect("run") {}
        let res = r.result();
        let col = |n: &str| res.data[res.names.iter().position(|x| x == n).expect(n)].clone();
        let (e, e2) = (col("Wh"), col("Wh2"));
        let t = |i: usize| res.t0 + i as f64 * res.dt;
        // whole cycles: exactly 1000 W · t (the star point at 0 V, so the meter referred to it reads the same)
        for i in [res.n / 2, res.n - 1] {
            assert!((e[i] - 1000.0 * t(i)).abs() < 1e-6 * 1000.0 * t(i).max(1e-3), "J at {}: {} vs {}", t(i), e[i], 1000.0 * t(i));
            assert!((e2[i] - (2.0 + 1000.0 * t(i) / 3600.0)).abs() < 1e-6, "Wh at {}: {}", t(i), e2[i]);
        }
        assert_eq!(e[0], 0.0);
    }

    /// 10 V across 1 kΩ and a reversed LED: the LED blocks (node at 10 V), for the ideal model, the red-LED
    /// preset and a pasted model with BV = 4 V
    #[test]
    fn led_never_breaks_down() {
        use crate::netlist::{NetElement, Netlist};
        use crate::runner::{SimCtrl, TransientRunner};
        let el = |id: i64, typ: &str, name: &str, params: &[(&str, &str)], pins: &[i64]| NetElement { id, typ: typ.into(), name: name.into(), params: params.iter().map(|(k, v)| (k.to_string(), v.to_string())).collect(), pins: pins.to_vec() };
        for model in [vec![("model", "ideal")], vec![("model", "LEDRED")], vec![("model", "custom"), ("spice", "IS=93.2p RS=42m N=3.73 BV=4 IBV=10u")]] {
            let mut led = vec![("vf", "2"), ("ron", "10u"), ("roff", "1M")];
            led.extend(model);
            let net = Netlist {
                nets: vec!['G', 'P', 'P'],
                elements: vec![el(1, "VDC", "V1", &[("V", "10")], &[1, 0]), el(2, "R", "R1", &[("R", "1k")], &[1, 2]), el(3, "LED", "D1", &led, &[0, 2]), el(4, "VP", "Vk", &[], &[2])],
                param_text: String::new(),
            };
            let sc = SimCtrl { dt: 1e-6, t_total: 1e-4, t_print: 0.0, print_step: 1, interp: true, damp: true, lte: true, lte_tol: 1e-3, acc: false, etol: 1e-6, max_lv: 8 };
            let mut r = TransientRunner::new(&net, &sc).expect("netlist");
            while !r.run_steps(1_000_000).expect("run") {}
            let res = r.result();
            let k = res.names.iter().position(|n| n == "Vk").expect("probe");
            let v = res.data[k][res.n - 1];
            assert!(v > 9.9, "{:?}: reverse-biased LED conducts (cathode at {} V)", led, v);
        }
    }
}
