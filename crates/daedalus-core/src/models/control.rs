//! Control-circuit models.
//! Each block reads e.cv (values at the end of the present interval) and e.cv_p (values at its start).
//! Outputs are written in eval(); internal state changes only in commit(), so eval() may be repeated
//! (algebraic loops, switching-instant re-solves). e.cev[node] carries the exact time of a discrete jump.

use super::wave::next_periodic;
use crate::clone_box;
use crate::expr::{js_max, js_min, js_pow, js_round, CompiledFn};
use crate::sim::*;
use std::collections::HashMap;
use std::f64::consts::PI;

const D2R: f64 = PI / 180.0;
const R2D: f64 = 180.0 / PI;
#[inline]
fn hi(v: f64) -> bool {
    v > 0.5
}
fn cc() -> Caps {
    Caps { c: true, ..Default::default() }
}

/// event time of a discrete output change, from inputs that jumped
fn in_evt(e: &E, ins: &[i32]) -> f64 {
    let mut ev = NAN;
    for &i in ins {
        if i < 0 {
            continue;
        }
        let v = e.cev[i as usize];
        if !v.is_nan() && !(v <= ev) {
            ev = v;
        }
    }
    ev
}
/// crossing time of (a - b) through 0 within the interval
fn cross_t(e: &E, t: f64, h: f64, a: i32, b: i32, th: f64) -> f64 {
    let ea = e.cev[a as usize];
    let eb = if b >= 0 { e.cev[b as usize] } else { NAN };
    if !ea.is_nan() || !eb.is_nan() {
        return (if ea.is_nan() { f64::NEG_INFINITY } else { ea }).max(if eb.is_nan() { f64::NEG_INFINITY } else { eb });
    }
    let da = e.cv_p[a as usize] - if b >= 0 { e.cv_p[b as usize] } else { th };
    let db = e.cv[a as usize] - if b >= 0 { e.cv[b as usize] } else { th };
    if h > 0.0 && da != db && (da > 0.0) != (db > 0.0) {
        return t - h + h * da / (da - db);
    }
    NAN
}
fn or_t(v: f64, t: f64) -> f64 {
    if v.is_nan() || v == 0.0 {
        t
    } else {
        v
    }
}
/// rising-edge (through 0.5) detection with event time
fn rise(e: &E, t: f64, h: f64, i: i32) -> Option<f64> {
    if i < 0 {
        return None;
    }
    let u = i as usize;
    if e.cv_p[u] <= 0.5 && e.cv[u] > 0.5 {
        return Some(or_t(cross_t(e, t, h, i, -1, 0.5), t));
    }
    None
}
fn fall(e: &E, t: f64, h: f64, i: i32) -> Option<f64> {
    let u = i as usize;
    if e.cv_p[u] > 0.5 && e.cv[u] <= 0.5 {
        return Some(or_t(cross_t(e, t, h, i, -1, 0.5), t));
    }
    None
}
#[inline]
fn set_out(e: &mut E, o: i32, v: f64, ev: f64) {
    if o < 0 {
        return;
    }
    let u = o as usize;
    e.cv[u] = v;
    e.cev[u] = if v != e.cv_p[u] { ev } else { NAN };
}
#[inline]
fn set_plain(e: &mut E, o: i32, v: f64) {
    if o < 0 {
        return;
    }
    e.cv[o as usize] = v;
    e.cev[o as usize] = NAN;
}
fn next_sample(t: f64, fs: f64) -> f64 {
    let mut k = (t * fs).floor() + 1.0;
    let mut ts = k / fs;
    while ts <= t {
        k += 1.0;
        ts = k / fs;
    }
    ts
}
fn is_sample(t: f64, fs: f64, eps: f64) -> bool {
    let k = js_round(t * fs);
    (t - k / fs).abs() <= eps
}
fn safe_div(a: f64, b: f64) -> f64 {
    if b == 0.0 {
        if a == 0.0 {
            0.0
        } else {
            (if a.is_nan() { f64::NAN } else if a > 0.0 { 1.0 } else { -1.0 }) * 1e300
        }
    } else {
        a / b
    }
}

/* ------------------------------------------------------------------ stateless function block */
#[derive(Clone)]
pub enum FnK {
    P(f64),
    Sum2(f64, f64),
    Sub2,
    Sum3(f64, f64, f64),
    Lim(f64, f64),
    Mult,
    Divd,
    Sqrt,
    Exp(f64, f64),
    Pow(f64, f64),
    Log,
    Log10,
    Abs,
    Sign,
    Sin,
    SinR,
    Asin,
    Cos,
    CosR,
    Acos,
    Tan,
    Atan,
    Atan2,
    MaxMin(bool),
    Lkup(Vec<f64>, Vec<f64>),
    LkupTz(f64, f64),
    LkupSq(f64),
    Round(f64, bool),
    Dac(f64),
}
impl FnK {
    fn f(&self, u: &[f64]) -> f64 {
        let a = u[0];
        let b = u.get(1).copied().unwrap_or(NAN);
        match self {
            FnK::P(k) => k * a,
            FnK::Sum2(k1, k2) => k1 * a + k2 * b,
            FnK::Sub2 => a - b,
            FnK::Sum3(k1, k2, k3) => k1 * a + k2 * b + k3 * u[2],
            FnK::Lim(lo, up) => {
                if a < *lo {
                    *lo
                } else if a > *up {
                    *up
                } else {
                    a
                }
            }
            FnK::Mult => a * b,
            FnK::Divd => safe_div(a, b),
            FnK::Sqrt => js_max(&[0.0, a]).sqrt(),
            FnK::Exp(k1, k2) => k1 * js_pow(*k2, a),
            FnK::Pow(k1, k2) => k1 * js_pow(a, *k2),
            FnK::Log => a.ln(),
            FnK::Log10 => a.log10(),
            FnK::Abs => a.abs(),
            FnK::Sign => {
                if a > 0.0 {
                    1.0
                } else if a < 0.0 {
                    -1.0
                } else {
                    0.0
                }
            }
            FnK::Sin => (a * D2R).sin(),
            FnK::SinR => a.sin(),
            FnK::Asin => js_max(&[-1.0, js_min(&[1.0, a])]).asin() * R2D,
            FnK::Cos => (a * D2R).cos(),
            FnK::CosR => a.cos(),
            FnK::Acos => js_max(&[-1.0, js_min(&[1.0, a])]).acos() * R2D,
            FnK::Tan => (a * D2R).tan(),
            FnK::Atan => safe_div(b, a).atan() * R2D,
            FnK::Atan2 => a.atan2(b) * R2D,
            FnK::MaxMin(mn) => {
                if *mn {
                    js_min(&[a, b])
                } else {
                    js_max(&[a, b])
                }
            }
            FnK::Lkup(xs, ys) => {
                let n = xs.len();
                if a <= xs[0] {
                    return ys[0];
                }
                if a >= xs[n - 1] {
                    return ys[n - 1];
                }
                let mut j = 1;
                while j < n - 1 && a > xs[j] {
                    j += 1;
                }
                ys[j - 1] + (ys[j] - ys[j - 1]) * (a - xs[j - 1]) / (xs[j] - xs[j - 1])
            }
            FnK::LkupTz(th, vp) => {
                let mut x = ((a % 360.0) + 360.0) % 360.0;
                let mut sg = 1.0;
                if x >= 180.0 {
                    x -= 180.0;
                    sg = -1.0;
                }
                if x > 90.0 {
                    x = 180.0 - x;
                }
                sg * if x < *th { vp * x / th } else { *vp }
            }
            FnK::LkupSq(th) => {
                let mut x = ((a % 360.0) + 360.0) % 360.0;
                let mut sg = 1.0;
                if x >= 180.0 {
                    x -= 180.0;
                    sg = -1.0;
                }
                if (x - 90.0).abs() < th / 2.0 {
                    sg
                } else {
                    0.0
                }
            }
            FnK::Round(sc, tr) => (if *tr { (a * sc).trunc() } else { js_round(a * sc) }) / sc,
            FnK::Dac(n) => b / js_pow(2.0, *n) * a,
        }
    }
}
#[derive(Clone)]
pub struct FnBlk {
    ins: Vec<i32>,
    out: i32,
    k: FnK,
    ac: Option<Vec<f64>>,
    buf: Vec<f64>,
}
impl Inst for FnBlk {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: self.ac.is_some(), ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.clone()
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.out]
    }
    fn eval(&mut self, _t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        self.buf.clear();
        for &i in &self.ins {
            self.buf.push(e.cv[i as usize]);
        }
        let v = self.k.f(&self.buf);
        set_plain(e, self.out, v);
        Ok(())
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.out);
        c.a(r, r, 1.0, 0.0);
        if let Some(g) = &self.ac {
            for (j, gg) in g.iter().enumerate() {
                let col = c.ci(self.ins[j]);
                c.a(r, col, -gg, 0.0);
            }
        }
        Ok(())
    }
}
fn fnb(ins: Vec<i32>, out: i32, k: FnK, ac: Option<Vec<f64>>) -> Vec<Box<dyn Inst>> {
    vec![Box::new(FnBlk { ins, out, k, ac, buf: Vec::with_capacity(3) })]
}

/* ------------------------------------------------------------------ constant */
#[derive(Clone)]
pub struct Const {
    o: i32,
    v: f64,
}
impl Inst for Const {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cc() }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn eval(&mut self, _t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        set_plain(e, self.o, self.v);
        Ok(())
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        c.a(r, r, 1.0, 0.0);
        Ok(())
    }
}

/* ------------------------------------------------------------------ LTI transfer function */
fn mat_inv(a: &[Vec<f64>], n: usize) -> Option<Vec<Vec<f64>>> {
    let mut m: Vec<Vec<f64>> = a.to_vec();
    let mut inv: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| if i == j { 1.0 } else { 0.0 }).collect()).collect();
    for k in 0..n {
        let mut p = k;
        for i in k + 1..n {
            if m[i][k].abs() > m[p][k].abs() {
                p = i;
            }
        }
        m.swap(k, p);
        inv.swap(k, p);
        let d = m[k][k];
        if d == 0.0 {
            return None;
        }
        for j in 0..n {
            m[k][j] /= d;
            inv[k][j] /= d;
        }
        for i in 0..n {
            if i != k {
                let f = m[i][k];
                if f != 0.0 {
                    for j in 0..n {
                        m[i][j] -= f * m[k][j];
                        inv[i][j] -= f * inv[k][j];
                    }
                }
            }
        }
    }
    Some(inv)
}
#[derive(Clone)]
struct Disc {
    ad: Vec<Vec<f64>>,
    bd: Vec<f64>,
}
/// observable canonical form, trapezoidal discretisation
#[derive(Clone)]
pub struct Lti {
    pub n: usize,
    num: Vec<f64>,
    den: Vec<f64>,
    k: f64,
    a: Vec<Vec<f64>>,
    b: Vec<f64>,
    pub d: f64,
    cache: HashMap<u64, Disc>,
    pub x0: Vec<f64>,
}
impl Lti {
    pub fn new(mut num: Vec<f64>, mut den: Vec<f64>, k: f64, x0: Vec<f64>) -> Result<Self, String> {
        while den.len() > 1 && den[0] == 0.0 {
            den.remove(0);
        }
        if num.len() > den.len() {
            let extra = &num[..num.len() - den.len()];
            if extra.iter().any(|&v| v != 0.0) {
                return Err("分子階數不可大於分母（非真分式）".into());
            }
            num = num[num.len() - den.len()..].to_vec();
        }
        while num.len() < den.len() {
            num.insert(0, 0.0);
        }
        let n = den.len() - 1;
        let an = den[0];
        let bn = k * num[0] / an;
        let mut a = Vec::new();
        let mut c = Vec::new();
        for i in 0..n {
            a.push(den[n - i] / an);
            c.push(k * num[n - i] / an - (den[n - i] / an) * bn);
        }
        let mut am = Vec::new();
        for i in 0..n {
            let mut r = vec![0.0; n];
            if i > 0 {
                r[i - 1] = 1.0;
            }
            r[n - 1] = -a[i];
            am.push(r);
        }
        Ok(Lti { n, num, den, k, a: am, b: c, d: bn, cache: HashMap::new(), x0 })
    }
    fn disc(&mut self, h: f64) -> &Disc {
        let key = h.to_bits();
        if !self.cache.contains_key(&key) {
            let n = self.n;
            let l: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| (if i == j { 1.0 } else { 0.0 }) - 0.5 * h * self.a[i][j]).collect()).collect();
            let r: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| (if i == j { 1.0 } else { 0.0 }) + 0.5 * h * self.a[i][j]).collect()).collect();
            let li = mat_inv(&l, n).unwrap_or_else(|| vec![vec![f64::NAN; n]; n]);
            let ad: Vec<Vec<f64>> = (0..n).map(|i| (0..n).map(|j| (0..n).fold(0.0, |s, k| s + li[i][k] * r[k][j])).collect()).collect();
            let bd: Vec<f64> = (0..n).map(|i| (0..n).fold(0.0, |s, k| s + li[i][k] * self.b[k]) * 0.5 * h).collect();
            if self.cache.len() > 32 {
                self.cache.clear();
            }
            self.cache.insert(key, Disc { ad, bd });
        }
        &self.cache[&key]
    }
    /// next state & output; x committed state, up previous input
    pub fn step(&mut self, x: &[f64], up: f64, u: f64, h: f64, xo: &mut [f64]) -> f64 {
        let n = self.n;
        if n == 0 {
            return self.d * u;
        }
        if h == 0.0 {
            xo[..n].copy_from_slice(&x[..n]);
            return x[n - 1] + self.d * u;
        }
        let dd = self.d;
        let d = self.disc(h);
        for i in 0..n {
            let mut s = d.bd[i] * (u + up);
            for j in 0..n {
                s += d.ad[i][j] * x[j];
            }
            xo[i] = s;
        }
        xo[n - 1] + dd * u
    }
    /// complex gain at s = jw
    pub fn hjw(&self, w: f64) -> (f64, f64) {
        let ev = |p: &[f64]| {
            let (mut re, mut im) = (0.0, 0.0);
            for &c in p {
                let nre = -im * w;
                let nim = re * w;
                re = nre + c;
                im = nim;
            }
            (re, im)
        };
        let (nr, ni) = ev(&self.num);
        let (dr, di) = ev(&self.den);
        let d = dr * dr + di * di;
        (self.k * (nr * dr + ni * di) / d, self.k * (ni * dr - nr * di) / d)
    }
}
#[derive(Clone)]
pub struct TfBlk {
    i: i32,
    o: i32,
    l: Lti,
    so: usize,
    xo: Vec<f64>,
}
impl Inst for TfBlk {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        let n = self.l.n;
        for i in 0..n {
            e.cs[self.so + i] = self.l.x0.get(i).copied().unwrap_or(0.0);
        }
        e.cs[self.so + n] = 0.0;
        Ok(())
    }
    fn eval(&mut self, _t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let n = self.l.n;
        let u = e.cv[self.i as usize];
        let s = &e.cs[self.so..self.so + n + 1];
        let y = self.l.step(s, s[n], u, h, &mut self.xo);
        set_plain(e, self.o, y);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        let n = self.l.n;
        e.cs[self.so..self.so + n].copy_from_slice(&self.xo[..n]);
        e.cs[self.so + n] = e.cv[self.i as usize];
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        let (gr, gi) = self.l.hjw(c.w);
        c.a(r, r, 1.0, 0.0);
        let col = c.ci(self.i);
        c.a(r, col, -gr, -gi);
        Ok(())
    }
}
fn tf(ctx: &mut ElCtx, num: Vec<f64>, den: Vec<f64>, k: f64, x0: Vec<f64>) -> R<Vec<Box<dyn Inst>>> {
    let l = Lti::new(num, den, k, x0).map_err(|m| ctx.err(m))?;
    let (i, o) = (ctx.c(0)?, ctx.c(1)?);
    let so = ctx.b.req_c(l.n + 1);
    let n = l.n;
    Ok(vec![Box::new(TfBlk { i, o, l, so, xo: vec![0.0; n] })])
}

/* ------------------------------------------------------------------ integrator / differentiator */
#[derive(Clone)]
pub struct Integ {
    i: i32,
    o: i32,
    tc: f64,
    y0: f64,
    rst: i32,
    flag: i64,
    so: usize,
    y: f64,
}
impl Inst for Integ {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: self.rst < 0, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        if self.rst >= 0 {
            vec![self.i, self.rst]
        } else {
            vec![self.i]
        }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = self.y0;
        e.cs[self.so + 1] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let u = e.cv[self.i as usize];
        let mut y = e.cs[self.so] + (h / (2.0 * self.tc)) * (u + e.cs[self.so + 1]);
        if self.rst >= 0 {
            if self.flag == 1 {
                if hi(e.cv[self.rst as usize]) {
                    y = 0.0;
                }
            } else if rise(e, t, h, self.rst).is_some() {
                y = 0.0;
            }
        }
        self.y = y;
        set_plain(e, self.o, y);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.y;
        e.cs[self.so + 1] = e.cv[self.i as usize];
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        c.a(r, r, 1.0, 0.0);
        let col = c.ci(self.i);
        let w = c.w;
        c.a(r, col, 0.0, 1.0 / (w * self.tc));
        Ok(())
    }
}
#[derive(Clone)]
pub struct Diff {
    i: i32,
    o: i32,
    tc: f64,
    so: usize,
}
impl Inst for Diff {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = NAN;
        Ok(())
    }
    fn eval(&mut self, _t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let u = e.cv[self.i as usize];
        let up = e.cs[self.so];
        let v = if h > 0.0 && !up.is_nan() { self.tc * (u - up) / h } else { 0.0 };
        set_plain(e, self.o, v);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = e.cv[self.i as usize];
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        c.a(r, r, 1.0, 0.0);
        let col = c.ci(self.i);
        let w = c.w;
        c.a(r, col, 0.0, -w * self.tc);
        Ok(())
    }
}

/* ------------------------------------------------------------------ comparator, limiters */
#[derive(Clone)]
pub struct Comp {
    a: i32,
    b: i32,
    o: i32,
    so: usize,
}
impl Inst for Comp {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.a, self.b]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let (a, b) = (e.cv[self.a as usize], e.cv[self.b as usize]);
        let s0 = e.cs[self.so];
        let y = if a > b {
            1.0
        } else if a < b {
            0.0
        } else {
            s0
        };
        let ev = if y != s0 { cross_t(e, t, h, self.a, self.b, 0.0) } else { NAN };
        e.cv[self.o as usize] = y;
        e.cev[self.o as usize] = ev;
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = e.cv[self.o as usize];
    }
}
#[derive(Clone)]
pub struct DvdtLim {
    i: i32,
    o: i32,
    l: f64,
    so: usize,
    y: f64,
}
impl Inst for DvdtLim {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = NAN;
        Ok(())
    }
    fn eval(&mut self, _t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let u = e.cv[self.i as usize];
        let yp = e.cs[self.so];
        let mut y = u;
        if !yp.is_nan() {
            let d = self.l * h;
            y = js_min(&[yp + d, js_max(&[yp - d, u])]);
        }
        self.y = y;
        set_plain(e, self.o, y);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.y;
    }
}

/* ------------------------------------------------------------------ RMS, FFT */
#[derive(Clone)]
pub struct Rms {
    i: i32,
    o: i32,
    fb: f64,
    so: usize,
    p: [f64; 3],
}
impl Inst for Rms {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for k in 0..3 {
            e.cs[self.so + k] = 0.0;
        }
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let u = e.cv[self.i as usize];
        let s = &e.cs[self.so..self.so + 3];
        let mut ii = s[0] + 0.5 * h * (u * u + s[1] * s[1]);
        let mut y = s[2];
        if t > 0.0 && is_sample(t, self.fb, e.eps_t) {
            y = js_max(&[0.0, ii * self.fb]).sqrt();
            ii = 0.0;
        }
        self.p = [ii, u, y];
        set_out(e, self.o, y, t);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so..self.so + 3].copy_from_slice(&self.p);
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        next_sample(t, self.fb)
    }
}
#[derive(Clone)]
pub struct Fft {
    i: i32,
    o: [i32; 2],
    n: usize,
    fs: f64,
    so: usize,
    p: Option<(usize, f64, f64, f64)>,
}
impl Inst for Fft {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        self.o.to_vec()
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for k in 0..2 * self.n + 3 {
            e.cs[self.so + k] = 0.0;
        }
        Ok(())
    }
    fn eval(&mut self, t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let n = self.n;
        let s = &e.cs[self.so..];
        let (mut amp, mut ph) = (s[2 * n], s[2 * n + 1]);
        self.p = None;
        if is_sample(t, self.fs, e.eps_t) {
            let k = js_round(t * self.fs) as i64;
            let u = e.cv[self.i as usize];
            let mut buf: Vec<f64> = s[..n].to_vec();
            let ki = k.rem_euclid(n as i64) as usize;
            buf[ki] = u;
            let (mut re, mut im) = (0.0, 0.0);
            for (m, &bv) in buf.iter().enumerate() {
                let th = 2.0 * PI * m as f64 / n as f64;
                re += bv * th.cos();
                im -= bv * th.sin();
            }
            re *= 2.0 / n as f64;
            im *= 2.0 / n as f64;
            amp = re.hypot(im);
            ph = im.atan2(re) * R2D + 90.0;
            if ph > 180.0 {
                ph -= 360.0;
            }
            self.p = Some((ki, u, amp, ph));
        }
        set_out(e, self.o[0], amp, t);
        set_out(e, self.o[1], ph, t);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        if let Some((k, u, amp, ph)) = self.p {
            let n = self.n;
            e.cs[self.so + k] = u;
            e.cs[self.so + 2 * n] = amp;
            e.cs[self.so + 2 * n + 1] = ph;
        }
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        next_sample(t, self.fs)
    }
}

/* ------------------------------------------------------------------ sample-and-hold, delays */
#[derive(Clone)]
pub struct SampH {
    i: i32,
    c: i32,
    o: i32,
    so: usize,
    y: f64,
}
impl Inst for SampH {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i, self.c]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let mut y = e.cs[self.so];
        let te = rise(e, t, h, self.c);
        if let Some(te) = te {
            let (ua, ub) = (e.cv_p[self.i as usize], e.cv[self.i as usize]);
            let f = if h > 0.0 { (te - (t - h)) / h } else { 1.0 };
            y = ua + (ub - ua) * js_max(&[0.0, js_min(&[1.0, f])]);
        }
        if t == 0.0 && h == 0.0 && hi(e.cv[self.c as usize]) {
            y = e.cv[self.i as usize];
        }
        self.y = y;
        set_out(e, self.o, y, te.unwrap_or(NAN));
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.y;
    }
}
/// continuous time delay with exact reproduction of jumps
#[derive(Clone)]
pub struct TDelay {
    i: i32,
    o: i32,
    td: f64,
    ft: bool,
    bt: Vec<f64>,
    bv: Vec<f64>,
    pend: Option<(f64, f64, f64, f64)>,
}
impl TDelay {
    fn at(&self, t: f64, u_now: f64, t_now: f64, u_p: f64, t_p: f64) -> f64 {
        let n = self.bt.len();
        if t <= if n > 0 { self.bt[0] } else { t_now } {
            return if n > 0 { self.bv[0] } else { u_now };
        }
        if t >= t_p && t_now > t_p {
            return u_p + (u_now - u_p) * (t - t_p) / (t_now - t_p);
        }
        let (mut lo, mut hi2) = (0usize, n - 1);
        if t >= self.bt[hi2] {
            return self.bv[hi2];
        }
        while hi2 - lo > 1 {
            let m = (lo + hi2) >> 1;
            if self.bt[m] <= t {
                lo = m;
            } else {
                hi2 = m;
            }
        }
        let (t0, t1) = (self.bt[lo], self.bt[hi2]);
        if t1 == t0 {
            return self.bv[hi2];
        }
        self.bv[lo] + (self.bv[hi2] - self.bv[lo]) * (t - t0) / (t1 - t0)
    }
}
impl Inst for TDelay {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ac: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn ft(&self) -> bool {
        self.ft
    }
    fn init(&mut self, _e: &mut E) -> R<()> {
        self.bt.clear();
        self.bv.clear();
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let u = e.cv[self.i as usize];
        let tp = t - h;
        let up = e.cv_p[self.i as usize];
        // before t = Td the delayed signal does not exist yet: the output is 0 (not the input at t = 0)
        let q = t - self.td;
        let y = if q < -e.eps_t { 0.0 } else { self.at(q.max(0.0), u, t, up, tp) };
        let ev = e.cev[self.i as usize];
        self.pend = Some((t, u, if !ev.is_nan() && u != up { ev } else { NAN }, up));
        set_out(e, self.o, y, t);
        Ok(())
    }
    fn commit(&mut self, t: f64, _h: f64, e: &mut E) {
        let (pt, _u_eval, _ev_eval, _up_eval) = match self.pend {
            Some(p) => p,
            None => return,
        };
        let _ = pt;
        // record the input as it is at the end of the step: a non-feedthrough delay is evaluated before the
        // block that drives it, so the value seen in eval() is still the previous step's one
        let u = e.cv[self.i as usize];
        let up = e.cv_p[self.i as usize];
        let ev = { let x = e.cev[self.i as usize]; if !x.is_nan() && u != up { x } else { f64::NAN } };
        if self.bt.is_empty() {
            self.bt.push(t);
            self.bv.push(u);
            return;
        }
        if !ev.is_nan() {
            self.bt.push(ev);
            self.bv.push(up);
            self.bt.push(ev);
            self.bv.push(u);
        }
        if *self.bt.last().unwrap() != t || *self.bv.last().unwrap() != u {
            self.bt.push(t);
            self.bv.push(u);
        }
        let keep = t - self.td - 2.0 * e.dt;
        let mut c = 0;
        while c + 2 < self.bt.len() && self.bt[c + 1] < keep {
            c += 1;
        }
        if c > 4096 {
            self.bt.drain(0..c);
            self.bv.drain(0..c);
        }
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        // the output steps from 0 to the input's initial value at t = Td
        if let (Some(&t0), Some(&v0)) = (self.bt.first(), self.bv.first()) {
            let te = t0 + self.td;
            if v0 != 0.0 && te > t {
                return te;
            }
        }
        for i in 1..self.bt.len() {
            if self.bt[i] == self.bt[i - 1] {
                let te = self.bt[i] + self.td;
                if te > t {
                    return te;
                }
            }
        }
        f64::INFINITY
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        c.a(r, r, 1.0, 0.0);
        let ph = -c.w * self.td;
        let col = c.ci(self.i);
        c.a(r, col, -ph.cos(), -ph.sin());
        Ok(())
    }
}
/// unit time delay: output = input at the previous simulation step
#[derive(Clone)]
pub struct Utd {
    i: i32,
    o: i32,
    so: usize,
}
impl Inst for Utd {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn ft(&self) -> bool {
        false
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        Ok(())
    }
    fn eval(&mut self, _t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let v = e.cs[self.so];
        set_plain(e, self.o, v);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = e.cv[self.i as usize];
    }
}

/* ------------------------------------------------------------------ THD */
#[derive(Clone)]
struct MovRms {
    tp: f64,
    t: Vec<f64>,
    i: Vec<f64>,
}
impl MovRms {
    fn new(tp: f64) -> Self {
        MovRms { tp, t: vec![0.0], i: vec![0.0] }
    }
    fn val(&self, t: f64, ii: f64) -> f64 {
        let t0 = t - self.tp;
        if t0 <= 0.0 {
            return js_max(&[0.0, ii / t.max(1e-300)]).sqrt();
        }
        let n = self.t.len();
        let mut j = n - 1;
        while j > 0 && self.t[j] > t0 {
            j -= 1;
        }
        let ta = self.t[j];
        let tb = if j + 1 < n { self.t[j + 1] } else { t };
        let ia = self.i[j];
        let ib = if j + 1 < n { self.i[j + 1] } else { ii };
        let i0 = ia + (ib - ia) * if tb > ta { (t0 - ta) / (tb - ta) } else { 0.0 };
        js_max(&[0.0, (ii - i0) / self.tp]).sqrt()
    }
    fn push(&mut self, t: f64, ii: f64) {
        self.t.push(t);
        self.i.push(ii);
        let keep = t - 1.5 * self.tp;
        let mut c = 0;
        while c + 2 < self.t.len() && self.t[c + 1] < keep {
            c += 1;
        }
        if c > 2048 {
            self.t.drain(0..c);
            self.i.drain(0..c);
        }
    }
}
#[derive(Clone)]
pub struct Thd {
    i: i32,
    o: [i32; 2],
    l: Lti,
    tp: f64,
    so: usize,
    xo: [f64; 2],
    r1: MovRms,
    r2: MovRms,
    p: [f64; 4],
}
impl Inst for Thd {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        self.o.to_vec()
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for k in 0..6 {
            e.cs[self.so + k] = 0.0;
        }
        self.r1 = MovRms::new(self.tp);
        self.r2 = MovRms::new(self.tp);
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let u = e.cv[self.i as usize];
        let s: Vec<f64> = e.cs[self.so..self.so + 6].to_vec();
        let v1 = self.l.step(&s, s[2], u, h, &mut self.xo);
        let iu = s[3] + 0.5 * h * (u * u + s[2] * s[2]);
        let i1 = s[4] + 0.5 * h * (v1 * v1 + s[5] * s[5]);
        let vr = self.r1.val(t, iu);
        let vv1 = self.r2.val(t, i1);
        // capped at 1000 % (start-up, before the fundamental has settled)
        let thd = if vv1 > 0.0 { js_min(&[10.0, js_max(&[0.0, vr * vr - vv1 * vv1]).sqrt() / vv1]) } else { 0.0 };
        self.p = [u, v1, iu, i1];
        set_plain(e, self.o[0], thd);
        set_plain(e, self.o[1], v1);
        Ok(())
    }
    fn commit(&mut self, t: f64, h: f64, e: &mut E) {
        let s = &mut e.cs[self.so..self.so + 6];
        s[0] = self.xo[0];
        s[1] = self.xo[1];
        s[2] = self.p[0];
        s[3] = self.p[2];
        s[4] = self.p[3];
        s[5] = self.p[1];
        if h > 0.0 {
            self.r1.push(t, self.p[2]);
            self.r2.push(t, self.p[3]);
        }
    }
}

/* ------------------------------------------------------------------ generic small blocks (closures by kind) */
#[derive(Clone)]
pub enum MiscK {
    Mux { d: Vec<i32>, s: Vec<i32> },
    Abc2Dqo,
    Dqo2Abc,
    Gate { n: usize, op: u8 },
    OnCtrl,
    Array(Vec<f64>),
    Conv,
    MemRead(usize),
    Math(CompiledFn, usize),
}
#[derive(Clone)]
pub struct Misc {
    ins: Vec<i32>,
    outs: Vec<i32>,
    k: MiscK,
    buf: Vec<f64>,
}
impl Inst for Misc {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.iter().copied().filter(|&i| i >= 0).collect()
    }
    fn outs(&self) -> Vec<i32> {
        self.outs.clone()
    }
    fn eval(&mut self, t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let cv = |e: &E, i: i32| if i >= 0 { e.cv[i as usize] } else { 0.0 };
        match &self.k {
            MiscK::Mux { d, s } => {
                let mut idx = 0usize;
                for (i, &si) in s.iter().enumerate() {
                    if hi(e.cv[si as usize]) {
                        idx |= 1 << i;
                    }
                }
                let o = self.outs[0];
                let v = e.cv[d[idx] as usize];
                e.cv[o as usize] = v;
                e.cev[o as usize] = if v != e.cv_p[o as usize] { in_evt(e, &self.ins) } else { NAN };
            }
            MiscK::Abc2Dqo => {
                let (va, vb, vc, th) = (cv(e, self.ins[0]), cv(e, self.ins[1]), cv(e, self.ins[2]), cv(e, self.ins[3]));
                let k = 2.0 * PI / 3.0;
                let d = 2.0 / 3.0 * (va * th.cos() + vb * (th - k).cos() + vc * (th + k).cos());
                let q = 2.0 / 3.0 * (va * th.sin() + vb * (th - k).sin() + vc * (th + k).sin());
                let z = (va + vb + vc) / 3.0;
                set_plain(e, self.outs[0], d);
                set_plain(e, self.outs[1], q);
                set_plain(e, self.outs[2], z);
            }
            MiscK::Dqo2Abc => {
                let vd = cv(e, self.ins[0]);
                let vq = cv(e, self.ins[1]);
                let vo = if self.ins[2] > 0 { e.cv[self.ins[2] as usize] } else { 0.0 };
                let th = cv(e, self.ins[3]);
                let k = 2.0 * PI / 3.0;
                let a = vd * th.cos() + vq * th.sin() + vo;
                let b = vd * (th - k).cos() + vq * (th - k).sin() + vo;
                let c = vd * (th + k).cos() + vq * (th + k).sin() + vo;
                set_plain(e, self.outs[0], a);
                set_plain(e, self.outs[1], b);
                set_plain(e, self.outs[2], c);
            }
            MiscK::Gate { n, op } => {
                let a: Vec<bool> = self.ins[..*n].iter().map(|&i| hi(e.cv[i as usize])).collect();
                let v = match op {
                    0 => a.iter().all(|&x| x),
                    1 => a.iter().any(|&x| x),
                    2 => a[0] != a[1],
                    3 => !a[0],
                    4 => !(a[0] && a[1]),
                    _ => !(a[0] || a[1]),
                };
                let ev = in_evt(e, &self.ins);
                set_out(e, self.outs[0], if v { 1.0 } else { 0.0 }, ev);
            }
            MiscK::OnCtrl => {
                let (i, o) = (self.ins[0] as usize, self.outs[0] as usize);
                let v = if hi(e.cv[i]) { 1.0 } else { 0.0 };
                e.cv[o] = v;
                e.cev[o] = e.cev[i];
                if v != e.cv_p[o] && e.cev[i].is_nan() {
                    e.cev[o] = cross_t(e, t, _h, self.ins[0], -1, 0.5);
                }
            }
            MiscK::Array(arr) => {
                let o = self.outs[0] as usize;
                e.cvec[o] = Some(arr.clone());
                e.cv[o] = arr.first().copied().unwrap_or(NAN);
                e.cev[o] = NAN;
            }
            MiscK::Conv => {
                let get = |e: &E, i: i32| e.cvec[i as usize].clone().unwrap_or_else(|| vec![e.cv[i as usize]]);
                let a = get(e, self.ins[0]);
                let b = get(e, self.ins[1]);
                let (m, n) = (a.len(), b.len());
                let mut c = vec![0.0; (m + n).saturating_sub(1).max(1)];
                for i in 0..m {
                    for j in 0..n {
                        c[i + j] += a[i] * b[j];
                    }
                }
                let o = self.outs[0] as usize;
                e.cv[o] = c[0];
                e.cvec[o] = Some(c);
                e.cev[o] = NAN;
            }
            MiscK::MemRead(off) => {
                let i = self.ins[0] as usize;
                let y = match &e.cvec[i] {
                    Some(v) => {
                        let x = v.get(*off).copied().unwrap_or(0.0);
                        if x.is_nan() {
                            0.0
                        } else {
                            x
                        }
                    }
                    None => {
                        if *off == 0 {
                            e.cv[i]
                        } else {
                            0.0
                        }
                    }
                };
                let ev = in_evt(e, &self.ins);
                set_out(e, self.outs[0], y, ev);
            }
            MiscK::Math(f, n) => {
                self.buf.clear();
                self.buf.push(t);
                for &i in &self.ins {
                    self.buf.push(e.cv[i as usize]);
                }
                if *n == 1 {
                    let x1 = self.buf[1];
                    self.buf.push(x1);
                }
                let y = f.call(&self.buf);
                set_plain(e, self.outs[0], y);
            }
        }
        Ok(())
    }
}
fn misc(ins: Vec<i32>, outs: Vec<i32>, k: MiscK) -> Vec<Box<dyn Inst>> {
    vec![Box::new(Misc { ins, outs, k, buf: Vec::new() })]
}

/* ------------------------------------------------------------------ flip-flops */
#[derive(Clone)]
pub enum FfK {
    Sr { s: i32, r: i32, lvl: bool },
    Jk { j: i32, ck: i32, k: i32 },
    JkSr { s: i32, j: i32, ck: i32, k: i32, r: i32 },
    D { d: i32, ck: i32 },
    DSr { s: i32, d: i32, ck: i32, r: i32 },
}
#[derive(Clone)]
pub struct Ff {
    ins: Vec<i32>,
    outs: [i32; 2],
    k: FfK,
    so: usize,
    q: f64,
}
fn jk_logic(e: &E, t: f64, h: f64, q: f64, j: i32, ck: i32, k: i32) -> (f64, f64) {
    let ev = match rise(e, t, h, ck) {
        None => return (q, NAN),
        Some(v) => v,
    };
    let (jj, kk) = (hi(e.cv_p[j as usize]), hi(e.cv_p[k as usize]));
    if jj && kk {
        return (1.0 - q, ev);
    }
    if jj {
        return (1.0, ev);
    }
    if kk {
        return (0.0, ev);
    }
    (q, NAN)
}
impl Inst for Ff {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.clone()
    }
    fn outs(&self) -> Vec<i32> {
        self.outs.to_vec()
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let q = e.cs[self.so];
        let (nq, ev) = match self.k {
            FfK::Sr { s, r, lvl } => {
                if lvl {
                    let (sv, rv) = (hi(e.cv[s as usize]), hi(e.cv[r as usize]));
                    if sv && !rv {
                        (1.0, in_evt(e, &[s]))
                    } else if rv && !sv {
                        (0.0, in_evt(e, &[r]))
                    } else {
                        (q, NAN)
                    }
                } else {
                    let (es, er) = (rise(e, t, h, s), rise(e, t, h, r));
                    match (es, er) {
                        (Some(a), None) => (1.0, a),
                        (None, Some(b)) => (0.0, b),
                        _ => (q, NAN),
                    }
                }
            }
            FfK::Jk { j, ck, k } => jk_logic(e, t, h, q, j, ck, k),
            FfK::JkSr { s, j, ck, k, r } => {
                if hi(e.cv[s as usize]) {
                    (1.0, in_evt(e, &[s]))
                } else if hi(e.cv[r as usize]) {
                    (0.0, in_evt(e, &[r]))
                } else {
                    jk_logic(e, t, h, q, j, ck, k)
                }
            }
            FfK::D { d, ck } => match rise(e, t, h, ck) {
                None => (q, NAN),
                Some(ev) => (if hi(e.cv_p[d as usize]) { 1.0 } else { 0.0 }, ev),
            },
            FfK::DSr { s, d, ck, r } => {
                if hi(e.cv[s as usize]) {
                    (1.0, in_evt(e, &[s]))
                } else if hi(e.cv[r as usize]) {
                    (0.0, in_evt(e, &[r]))
                } else {
                    match rise(e, t, h, ck) {
                        None => (q, NAN),
                        Some(ev) => (if hi(e.cv_p[d as usize]) { 1.0 } else { 0.0 }, ev),
                    }
                }
            }
        };
        self.q = nq;
        set_out(e, self.outs[0], nq, ev);
        if self.outs[1] > 0 {
            set_out(e, self.outs[1], 1.0 - nq, ev);
        }
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.q;
    }
}

/* ------------------------------------------------------------------ monostable, pulse width, counter, A/D */
#[derive(Clone)]
pub struct Mono {
    inp: i32,
    w_in: i32,
    outs: [i32; 2],
    w: f64,
    so: usize,
    p: [f64; 2],
}
impl Inst for Mono {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        if self.w_in >= 0 {
            vec![self.inp, self.w_in]
        } else {
            vec![self.inp]
        }
    }
    fn outs(&self) -> Vec<i32> {
        self.outs.to_vec()
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = f64::NEG_INFINITY;
        e.cs[self.so + 1] = f64::NEG_INFINITY;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let (mut ts, mut te) = (e.cs[self.so], e.cs[self.so + 1]);
        let eps = e.eps_t;
        if let Some(ev) = rise(e, t, h, self.inp) {
            if !(t < te - eps && t - h < te) {
                ts = ev;
                let w = if self.w_in >= 0 { e.cv[self.w_in as usize] } else { self.w };
                te = ev + js_max(&[0.0, w]);
            }
        }
        self.p = [ts, te];
        let on = if t >= ts - eps && t < te - eps { 1.0 } else { 0.0 };
        let ev = if on != 0.0 { ts } else { te };
        set_out(e, self.outs[0], on, ev);
        if self.outs[1] > 0 {
            set_out(e, self.outs[1], 1.0 - on, ev);
        }
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.p[0];
        e.cs[self.so + 1] = self.p[1];
    }
    fn next_break(&self, t: f64, e: &E) -> f64 {
        let te = e.cs[self.so + 1];
        if te > t {
            te
        } else {
            f64::INFINITY
        }
    }
}
#[derive(Clone)]
pub struct Pwct {
    i: i32,
    o: i32,
    so: usize,
    p: [f64; 2],
}
impl Inst for Pwct {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.i]
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = NAN;
        e.cs[self.so + 1] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let (mut ts, mut y) = (e.cs[self.so], e.cs[self.so + 1]);
        let er = rise(e, t, h, self.i);
        let ef = fall(e, t, h, self.i);
        let mut ev = NAN;
        if let Some(r) = er {
            ts = r;
        }
        if let Some(f) = ef {
            if !ts.is_nan() {
                y = f - ts;
                ev = f;
            }
        }
        self.p = [ts, y];
        set_out(e, self.o, y, ev);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.p[0];
        e.cs[self.so + 1] = self.p[1];
    }
}
#[derive(Clone)]
pub struct UpDn {
    u: i32,
    d: i32,
    r: i32,
    o: i32,
    md: f64,
    so: usize,
    c: f64,
}
impl Inst for UpDn {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        [self.u, self.d, self.r].into_iter().filter(|&v| v >= 0).collect()
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let mut c = e.cs[self.so];
        let mut ev = NAN;
        let (eu, ed) = (rise(e, t, h, self.u), rise(e, t, h, self.d));
        if let Some(v) = eu {
            c += 1.0;
            ev = v;
        }
        if let Some(v) = ed {
            c -= 1.0;
            ev = v;
        }
        c = ((c % self.md) + self.md) % self.md;
        if self.r >= 0 && hi(e.cv[self.r as usize]) {
            c = 0.0;
            ev = in_evt(e, &[self.r]);
        }
        self.c = c;
        set_out(e, self.o, c, ev);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.c;
    }
}
#[derive(Clone)]
pub struct Adc {
    ins: [i32; 3],
    o: i32,
    n: f64,
    so: usize,
    y: f64,
}
impl Inst for Adc {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.to_vec()
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let mut y = e.cs[self.so];
        let ev = rise(e, t, h, self.ins[2]);
        if ev.is_some() {
            let vr = e.cv[self.ins[1] as usize];
            let m = js_pow(2.0, self.n);
            y = js_max(&[0.0, js_min(&[m - 1.0, (m / vr * e.cv[self.ins[0] as usize] + 1e-9).floor()])]);
        }
        self.y = y;
        set_out(e, self.o, y, ev.unwrap_or(NAN));
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.y;
    }
}

/* ------------------------------------------------------------------ digital control module (sampled blocks) */
#[derive(Clone)]
pub enum SampK {
    Zoh,
    UDelay,
    IntD { alg: i64, y0: f64, rflag: i64, wrap: Option<(f64, f64)>, r_prev: f64 },
    Dd,
    DiffEq { b: Vec<f64>, a: Vec<f64>, n: usize, u: f64 },
    Quant { n: f64, a: f64, b: f64, c: f64, d: f64, rnd: bool },
    CBuf { l: usize, v: Vec<f64> },
}
#[derive(Clone)]
pub struct Samp {
    ins: Vec<i32>,
    o: i32,
    fs: f64,
    so: usize,
    k: SampK,
    y: f64,
    is_hit: bool,
}
impl Inst for Samp {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.clone()
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn ft(&self) -> bool {
        !matches!(self.k, SampK::UDelay)
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        let so = self.so;
        match &mut self.k {
            SampK::Zoh | SampK::Quant { .. } => e.cs[so] = 0.0,
            SampK::UDelay => {
                e.cs[so] = 0.0;
                e.cs[so + 1] = 0.0;
            }
            SampK::IntD { y0, .. } => {
                e.cs[so] = *y0;
                e.cs[so + 1] = 0.0;
                e.cs[so + 2] = 0.0;
            }
            SampK::Dd => {
                for k in 0..3 {
                    e.cs[so + k] = 0.0;
                }
            }
            SampK::DiffEq { n, .. } => {
                for k in 0..2 * *n + 1 {
                    e.cs[so + k] = 0.0;
                }
            }
            SampK::CBuf { l, .. } => {
                for k in 0..*l + 1 {
                    e.cs[so + k] = 0.0;
                }
                e.cvec[self.o as usize] = Some(vec![0.0; *l]);
            }
        }
        Ok(())
    }
    fn eval(&mut self, t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let so = self.so;
        let hit = is_sample(t, self.fs, e.eps_t);
        self.is_hit = hit;
        let u0 = e.cv[self.ins[0] as usize];
        let mut y = e.cs[so];
        match &mut self.k {
            SampK::Zoh => {
                if hit {
                    y = u0;
                }
            }
            SampK::UDelay => {
                if hit {
                    y = e.cs[so + 1];
                }
            }
            SampK::IntD { alg, rflag, wrap, r_prev, .. } => {
                if hit {
                    let tt = 1.0 / self.fs;
                    if e.cs[so + 2] != 0.0 {
                        if *alg == 0 {
                            y += 0.5 * tt * (u0 + e.cs[so + 1]);
                        } else if *alg == 1 {
                            y += tt * u0;
                        } else {
                            y += tt * e.cs[so + 1];
                        }
                    }
                    if self.ins.len() > 1 {
                        let r = e.cv[self.ins[1] as usize];
                        let rst = if *rflag == 1 { hi(r) } else { hi(r) && !hi(*r_prev) };
                        if rst {
                            y = 0.0;
                        }
                    }
                    if let Some((lo, up)) = wrap {
                        let span = *up - *lo;
                        if span > 0.0 && y.is_finite() {
                            while y > *up {
                                y -= span;
                            }
                            while y < *lo {
                                y += span;
                            }
                        }
                    }
                }
            }
            SampK::Dd => {
                if hit {
                    y = if e.cs[so + 2] != 0.0 { (u0 - e.cs[so + 1]) * self.fs } else { 0.0 };
                }
            }
            SampK::DiffEq { b, a, n, u } => {
                let n = *n;
                y = e.cs[so + 2 * n];
                if hit {
                    let s = &e.cs[so..];
                    let mut acc = b[0] * u0;
                    for i in 1..=n {
                        acc += b[i] * s[i - 1] - a[i] * s[n + i - 1];
                    }
                    y = acc / a[0];
                    *u = u0;
                }
            }
            SampK::Quant { n, a, b, c, d, rnd } => {
                if hit {
                    let vox = *c + (u0 - *a) * (*d - *c) / (*b - *a);
                    let dv = (*d - *c) / (js_pow(2.0, *n) - 1.0);
                    let q = (vox - *c) / dv;
                    let nn = if *rnd { js_round(q) } else { (q + 1e-12).floor() };
                    y = js_max(&[*c, js_min(&[*d, *c + nn * dv])]);
                }
            }
            SampK::CBuf { l, v } => {
                let l = *l;
                v.clear();
                v.extend_from_slice(&e.cs[so..so + l]);
                if hit {
                    let idx = (e.cs[so + l] % l as f64) as usize;
                    v[idx] = u0;
                }
                y = v[0];
                e.cvec[self.o as usize] = Some(v.clone());
            }
        }
        self.y = y;
        set_out(e, self.o, y, t);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        let so = self.so;
        let hit = self.is_hit;
        let u0 = e.cv[self.ins[0] as usize];
        match &mut self.k {
            SampK::Zoh | SampK::Quant { .. } => e.cs[so] = self.y,
            SampK::UDelay => {
                e.cs[so] = self.y;
                if hit {
                    e.cs[so + 1] = u0;
                }
            }
            SampK::IntD { r_prev, .. } => {
                e.cs[so] = self.y;
                if hit {
                    e.cs[so + 1] = u0;
                    e.cs[so + 2] = 1.0;
                }
                if self.ins.len() > 1 && hit {
                    *r_prev = e.cv[self.ins[1] as usize];
                }
            }
            SampK::Dd => {
                e.cs[so] = self.y;
                if hit {
                    e.cs[so + 1] = u0;
                    e.cs[so + 2] = 1.0;
                }
            }
            SampK::DiffEq { n, u, .. } => {
                let n = *n;
                if hit {
                    for i in (1..n).rev() {
                        e.cs[so + i] = e.cs[so + i - 1];
                        e.cs[so + n + i] = e.cs[so + n + i - 1];
                    }
                    if n > 0 {
                        e.cs[so] = *u;
                        e.cs[so + n] = self.y;
                    }
                }
                e.cs[so + 2 * n] = self.y;
            }
            SampK::CBuf { l, v } => {
                if hit {
                    let l = *l;
                    e.cs[so..so + l].copy_from_slice(v);
                    e.cs[so + l] = (e.cs[so + l] + 1.0) % l as f64;
                }
            }
        }
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        next_sample(t, self.fs)
    }
}

/* ------------------------------------------------------------------ stack */
#[derive(Clone)]
pub struct Stack {
    ins: [i32; 3],
    o: i32,
    depth: usize,
    st: Vec<f64>,
    pst: Vec<f64>,
    y: f64,
    last_y: f64,
}
impl Inst for Stack {
    clone_box!();
    fn caps(&self) -> Caps {
        cc()
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.to_vec()
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, _e: &mut E) -> R<()> {
        self.st.clear();
        self.y = 0.0;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let mut st = self.st.clone();
        let ep = rise(e, t, h, self.ins[1]);
        let eo = rise(e, t, h, self.ins[2]);
        if ep.is_some() {
            st.push(e.cv[self.ins[0] as usize]);
            if st.len() > self.depth {
                st.remove(0);
            }
        }
        if eo.is_some() && !st.is_empty() {
            st.pop();
        }
        let ly = if self.last_y.is_nan() { 0.0 } else { self.last_y };
        let y = st.last().copied().unwrap_or(ly);
        self.pst = st;
        self.y = y;
        set_out(e, self.o, y, ep.or(eo).unwrap_or(NAN));
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, _e: &mut E) {
        self.st = self.pst.clone();
        self.last_y = self.y;
    }
}

/* ------------------------------------------------------------------ switch controllers */
#[derive(Clone)]
pub struct Gating {
    o: i32,
    f: f64,
    tp_: f64,
    tp: Vec<f64>,
    offs: Vec<f64>,
}
impl Gating {
    fn val_at(&self, t: f64) -> f64 {
        let u = if self.tp_ > 0.0 { t - (t / self.tp_).floor() * self.tp_ } else { t };
        let mut v = 0.0;
        for &p in &self.tp {
            if p <= u {
                v = 1.0 - v;
            } else {
                break;
            }
        }
        v
    }
}
impl Inst for Gating {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ..cc() }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn gate_freq(&self) -> f64 {
        self.f
    }
    fn eval(&mut self, t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let v = self.val_at(t + 0.5 * e.eps_t);
        set_out(e, self.o, v, t);
        Ok(())
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        if self.tp_ > 0.0 {
            return next_periodic(t, self.tp_, &self.offs);
        }
        for &p in &self.tp {
            if p > t {
                return p;
            }
        }
        f64::INFINITY
    }
}
#[derive(Clone)]
pub struct Alpha {
    ins: [i32; 3],
    o: i32,
    f: f64,
    pw: f64,
    so: usize,
    ts: f64,
}
impl Alpha {
    fn win(&self, ts: f64, alpha: f64) -> (f64, f64) {
        let tp = 1.0 / self.f;
        let a = ts + alpha / 360.0 * tp;
        (a, a + self.pw / 360.0 * tp)
    }
}
impl Inst for Alpha {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nb: true, ..cc() }
    }
    fn ins(&self) -> Vec<i32> {
        self.ins.to_vec()
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn gate_freq(&self) -> f64 {
        self.f
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.cs[self.so] = f64::NEG_INFINITY;
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let mut ts = e.cs[self.so];
        if let Some(ev) = rise(e, t, h, self.ins[1]) {
            ts = ev;
        }
        self.ts = ts;
        let alpha = e.cv[self.ins[0] as usize];
        let en = hi(e.cv[self.ins[2] as usize]);
        let mut on = 0.0;
        let mut ev = t;
        let eps = e.eps_t;
        if en && ts > f64::NEG_INFINITY {
            let (a, b) = self.win(ts, alpha);
            if t >= a - eps && t < b - eps {
                on = 1.0;
            }
            ev = if on != 0.0 { js_max(&[a, t - h]) } else { js_max(&[b, t - h]) };
            if ev > t {
                ev = t;
            }
        }
        set_out(e, self.o, on, ev);
        Ok(())
    }
    fn commit(&mut self, _t: f64, _h: f64, e: &mut E) {
        e.cs[self.so] = self.ts;
    }
    fn next_break(&self, t: f64, e: &E) -> f64 {
        let s0 = e.cs[self.so];
        if !(s0 > f64::NEG_INFINITY) {
            return f64::INFINITY;
        }
        let (a, b) = self.win(s0, e.cv[self.ins[0] as usize]);
        if a > t {
            return a;
        }
        if b > t {
            return b;
        }
        f64::INFINITY
    }
}

/* ------------------------------------------------------------------ factories */
/// largest buffer / array / stack a file may ask for (keeps a crafted .psimx from exhausting memory)
const MAX_VEC: i64 = 1_000_000;

pub fn make(ctx: &mut ElCtx) -> R<Option<Vec<Box<dyn Inst>>>> {
    let t = ctx.typ.clone();
    let c0 = |ctx: &ElCtx| ctx.c(0);
    let unary = |ctx: &mut ElCtx, k: FnK| -> R<Vec<Box<dyn Inst>>> { Ok(fnb(vec![ctx.c(0)?], ctx.c(1)?, k, None)) };
    let binary = |ctx: &mut ElCtx, k: FnK| -> R<Vec<Box<dyn Inst>>> { Ok(fnb(vec![ctx.c(0)?, ctx.c(1)?], ctx.c(2)?, k, None)) };
    let w2pi = |f: f64| 2.0 * PI * f;
    let r: Vec<Box<dyn Inst>> = match t.as_str() {
        "CONST" => {
            let v = ctx.num("v")?;
            vec![Box::new(Const { o: c0(ctx)?, v })]
        }
        "P" => {
            let k = ctx.num("k")?;
            fnb(vec![ctx.c(0)?], ctx.c(1)?, FnK::P(k), Some(vec![k]))
        }
        "I" => {
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let (tc, y0) = (ctx.pos("T")?, ctx.numd("y0", 0.0)?);
            let so = ctx.b.req_c(2);
            vec![Box::new(Integ { i, o, tc, y0, rst: -1, flag: 0, so, y: 0.0 })]
        }
        "RESETI" => {
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let (tc, y0) = (ctx.pos("T")?, ctx.numd("y0", 0.0)?);
            let rst = ctx.c(2)?;
            let flag = ctx.int("flag", 0.0)?;
            let so = ctx.b.req_c(2);
            vec![Box::new(Integ { i, o, tc, y0, rst, flag, so, y: 0.0 })]
        }
        "DIFF" => {
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let tc = ctx.pos("T")?;
            let so = ctx.b.req_c(1);
            vec![Box::new(Diff { i, o, tc, so })]
        }
        "PI" => {
            let (k, tc) = (ctx.num("k")?, ctx.pos("T")?);
            tf(ctx, vec![k * tc, k], vec![tc, 0.0], 1.0, vec![])?
        }
        "TFS" => {
            let (n, d, k) = (ctx.list("num")?, ctx.list("den")?, ctx.numd("k", 1.0)?);
            tf(ctx, n, d, k, vec![])?
        }
        "TFS1" => {
            let (n, d, k) = (ctx.list("num")?, ctx.list("den")?, ctx.numd("k", 1.0)?);
            let mut x0 = ctx.list_opt("x0", Some(vec![]))?;
            x0.reverse();
            tf(ctx, n, d, k, x0)?
        }
        "LP1" => {
            let k = ctx.numd("k", 1.0)?;
            let w = w2pi(ctx.pos("fc")?);
            tf(ctx, vec![0.0, w], vec![1.0, w], k, vec![])?
        }
        "HP1" => {
            let k = ctx.numd("k", 1.0)?;
            let w = w2pi(ctx.pos("fc")?);
            tf(ctx, vec![1.0, 0.0], vec![1.0, w], k, vec![])?
        }
        "LP2" => {
            let (k, z) = (ctx.numd("k", 1.0)?, ctx.numd("z", 0.7071)?);
            let w = w2pi(ctx.pos("fc")?);
            tf(ctx, vec![0.0, 0.0, w * w], vec![1.0, 2.0 * z * w, w * w], k, vec![])?
        }
        "HP2" => {
            let (k, z) = (ctx.numd("k", 1.0)?, ctx.numd("z", 0.7071)?);
            let w = w2pi(ctx.pos("fc")?);
            tf(ctx, vec![1.0, 0.0, 0.0], vec![1.0, 2.0 * z * w, w * w], k, vec![])?
        }
        "BP2" => {
            let k = ctx.numd("k", 1.0)?;
            let w = w2pi(ctx.pos("fo")?);
            let b = w2pi(ctx.pos("fb")?);
            tf(ctx, vec![0.0, b, 0.0], vec![1.0, b, w * w], k, vec![])?
        }
        "BS2" => {
            let k = ctx.numd("k", 1.0)?;
            let w = w2pi(ctx.pos("fo")?);
            let b = w2pi(ctx.pos("fb")?);
            tf(ctx, vec![1.0, 0.0, w * w], vec![1.0, b, w * w], k, vec![])?
        }
        "SUM2" => {
            let (k1, k2) = (ctx.numd("k1", 1.0)?, ctx.numd("k2", 1.0)?);
            fnb(vec![ctx.c(0)?, ctx.c(1)?], ctx.c(2)?, FnK::Sum2(k1, k2), Some(vec![k1, k2]))
        }
        "SUB2" => fnb(vec![ctx.c(0)?, ctx.c(1)?], ctx.c(2)?, FnK::Sub2, Some(vec![1.0, -1.0])),
        "SUM3" => {
            let (k1, k2, k3) = (ctx.numd("k1", 1.0)?, ctx.numd("k2", 1.0)?, ctx.numd("k3", 1.0)?);
            fnb(vec![ctx.c(0)?, ctx.c(1)?, ctx.c(2)?], ctx.c(3)?, FnK::Sum3(k1, k2, k3), Some(vec![k1, k2, k3]))
        }
        "COMP" => {
            let (a, b, o) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?);
            let so = ctx.b.req_c(1);
            vec![Box::new(Comp { a, b, o, so })]
        }
        "LIM" => {
            let (lo, up) = (ctx.num("lo")?, ctx.num("hi")?);
            if lo > up {
                return Err(ctx.err("下限不可大於上限"));
            }
            unary(ctx, FnK::Lim(lo, up))?
        }
        "LIMDVDT" => {
            let l = ctx.num("L")?.abs();
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let so = ctx.b.req_c(1);
            vec![Box::new(DvdtLim { i, o, l, so, y: 0.0 })]
        }
        "MULT" => binary(ctx, FnK::Mult)?,
        "DIVD" => binary(ctx, FnK::Divd)?,
        "SQRT" => unary(ctx, FnK::Sqrt)?,
        "EXP" => {
            let (k1, k2) = (ctx.numd("k1", 1.0)?, ctx.numd("k2", std::f64::consts::E)?);
            unary(ctx, FnK::Exp(k1, k2))?
        }
        "POW" => {
            let (k1, k2) = (ctx.numd("k1", 1.0)?, ctx.numd("k2", 2.0)?);
            unary(ctx, FnK::Pow(k1, k2))?
        }
        "LOG" => unary(ctx, FnK::Log)?,
        "LOG10" => unary(ctx, FnK::Log10)?,
        "ABS" => unary(ctx, FnK::Abs)?,
        "SIGN" => unary(ctx, FnK::Sign)?,
        "SIN" => unary(ctx, FnK::Sin)?,
        "SINR" => unary(ctx, FnK::SinR)?,
        "ASIN" => unary(ctx, FnK::Asin)?,
        "COS" => unary(ctx, FnK::Cos)?,
        "COSR" => unary(ctx, FnK::CosR)?,
        "ACOS" => unary(ctx, FnK::Acos)?,
        "TAN" => unary(ctx, FnK::Tan)?,
        "ATAN" => binary(ctx, FnK::Atan)?,
        "ATAN2" => binary(ctx, FnK::Atan2)?,
        "MAXMIN" => {
            let f = ctx.int("flag", 0.0)? != 0;
            binary(ctx, FnK::MaxMin(f))?
        }
        "RMS" => {
            let fb = ctx.pos("fb")?;
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let so = ctx.b.req_c(3);
            vec![Box::new(Rms { i, o, fb, so, p: [0.0; 3] })]
        }
        "FFT" => {
            let n = ctx.int("N", 64.0)?;
            let fb = ctx.pos("fb")?;
            if !(4..=1024).contains(&n) || (n & (n - 1)) != 0 {
                return Err(ctx.err("取樣點數必須是 4~1024 之間的 2 的次方"));
            }
            let n = n as usize;
            let (i, o0, o1) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?);
            let so = ctx.b.req_c(2 * n + 3);
            vec![Box::new(Fft { i, o: [o0, o1], n, fs: n as f64 * fb, so, p: None })]
        }
        "SAMP" => {
            let (i, c, o) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?);
            let so = ctx.b.req_c(1);
            vec![Box::new(SampH { i, c, o, so, y: 0.0 })]
        }
        "LKUP" => {
            let l = ctx.list("tab")?;
            if l.len() < 4 || l.len() % 2 != 0 {
                return Err(ctx.err("請以 x1 y1 x2 y2 ... 的成對格式輸入表格"));
            }
            let xs: Vec<f64> = l.iter().step_by(2).copied().collect();
            let ys: Vec<f64> = l.iter().skip(1).step_by(2).copied().collect();
            for i in 1..xs.len() {
                if !(xs[i] > xs[i - 1]) {
                    return Err(ctx.err("輸入值必須嚴格遞增"));
                }
            }
            unary(ctx, FnK::Lkup(xs, ys))?
        }
        "LKUPTZ" => {
            let (th, vp) = (ctx.num("th")?, ctx.num("vpk")?);
            unary(ctx, FnK::LkupTz(th, vp))?
        }
        "LKUPSQ" => {
            let th = ctx.num("th")?;
            unary(ctx, FnK::LkupSq(th))?
        }
        "ROUND" => {
            let (n, tr) = (ctx.int("N", 0.0)?, ctx.int("flag", 0.0)? != 0);
            unary(ctx, FnK::Round(js_pow(10.0, n as f64), tr))?
        }
        "TDELAY" => {
            let td = ctx.num("Td")?;
            if !(td >= 0.0) {
                return Err(ctx.err("延遲時間必須 ≥ 0"));
            }
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let ft = td < ctx.b.dt * 0.999;
            vec![Box::new(TDelay { i, o, td, ft, bt: vec![], bv: vec![], pend: None })]
        }
        "UTD" => {
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let so = ctx.b.req_c(1);
            vec![Box::new(Utd { i, o, so })]
        }
        "THD" => {
            let (f, fb) = (ctx.pos("f")?, ctx.pos("fb")?);
            let (i, o0, o1) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?);
            let (w, b) = (w2pi(f), w2pi(fb));
            let l = Lti::new(vec![0.0, b, 0.0], vec![1.0, b, w * w], 1.0, vec![]).map_err(|m| ctx.err(m))?;
            let so = ctx.b.req_c(6);
            vec![Box::new(Thd { i, o: [o0, o1], l, tp: 1.0 / f, so, xo: [0.0; 2], r1: MovRms::new(1.0 / f), r2: MovRms::new(1.0 / f), p: [0.0; 4] })]
        }
        "MUX2" | "MUX4" | "MUX8" => {
            let (n, k) = match t.as_str() {
                "MUX2" => (2, 1),
                "MUX4" => (4, 2),
                _ => (8, 3),
            };
            let mut d = vec![];
            let mut s = vec![];
            for i in 0..n {
                d.push(ctx.c(i)?);
            }
            for i in 0..k {
                s.push(ctx.c(n + i)?);
            }
            let o = ctx.c(n + k)?;
            let mut ins = d.clone();
            ins.extend(&s);
            misc(ins, vec![o], MiscK::Mux { d, s })
        }
        "ABC2DQO" => {
            let ins = vec![ctx.c(0)?, ctx.c(1)?, ctx.c(2)?, ctx.c(3)?];
            let outs = vec![ctx.c(4)?, ctx.c(5)?, ctx.c_opt(6)?];
            misc(ins, outs, MiscK::Abc2Dqo)
        }
        "DQO2ABC" => {
            let ins = vec![ctx.c(0)?, ctx.c(1)?, ctx.c_opt(2)?, ctx.c(3)?];
            let outs = vec![ctx.c(4)?, ctx.c(5)?, ctx.c(6)?];
            misc(ins, outs, MiscK::Dqo2Abc)
        }
        "AND" | "AND3" | "OR" | "OR3" | "XOR" | "NOT" | "NAND" | "NOR" => {
            let (n, op) = match t.as_str() {
                "AND" => (2, 0),
                "AND3" => (3, 0),
                "OR" => (2, 1),
                "OR3" => (3, 1),
                "XOR" => (2, 2),
                "NOT" => (1, 3),
                "NAND" => (2, 4),
                _ => (2, 5),
            };
            let mut ins = vec![];
            for i in 0..n {
                ins.push(ctx.c(i)?);
            }
            let o = ctx.c(n)?;
            misc(ins, vec![o], MiscK::Gate { n, op })
        }
        "SRFF" => {
            let (s, r) = (ctx.c(0)?, ctx.c(1)?);
            let lvl = ctx.int("flag", 0.0)? == 1;
            let outs = [ctx.c(2)?, ctx.c_opt(3)?];
            let so = ctx.b.req_c(1);
            vec![Box::new(Ff { ins: vec![s, r], outs, k: FfK::Sr { s, r, lvl }, so, q: 0.0 })]
        }
        "JKFF" => {
            let (j, ck, k) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?);
            let outs = [ctx.c(3)?, ctx.c_opt(4)?];
            let so = ctx.b.req_c(1);
            vec![Box::new(Ff { ins: vec![j, ck, k], outs, k: FfK::Jk { j, ck, k }, so, q: 0.0 })]
        }
        "JKFFSR" => {
            let (s, j, ck, k, r) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?, ctx.c(3)?, ctx.c(4)?);
            let outs = [ctx.c(5)?, ctx.c_opt(6)?];
            let so = ctx.b.req_c(1);
            vec![Box::new(Ff { ins: vec![s, j, ck, k, r], outs, k: FfK::JkSr { s, j, ck, k, r }, so, q: 0.0 })]
        }
        "DFF" => {
            let (d, ck) = (ctx.c(0)?, ctx.c(1)?);
            let outs = [ctx.c(2)?, ctx.c_opt(3)?];
            let so = ctx.b.req_c(1);
            vec![Box::new(Ff { ins: vec![d, ck], outs, k: FfK::D { d, ck }, so, q: 0.0 })]
        }
        "DFFSR" => {
            let (s, d, ck, r) = (ctx.c(0)?, ctx.c(1)?, ctx.c(2)?, ctx.c(3)?);
            let outs = [ctx.c(4)?, ctx.c_opt(5)?];
            let so = ctx.b.req_c(1);
            vec![Box::new(Ff { ins: vec![s, d, ck, r], outs, k: FfK::DSr { s, d, ck, r }, so, q: 0.0 })]
        }
        "MONO" => {
            let inp = ctx.c(0)?;
            let outs = [ctx.c(1)?, ctx.c_opt(2)?];
            let w = ctx.num("W")?;
            let so = ctx.b.req_c(2);
            vec![Box::new(Mono { inp, w_in: -1, outs, w, so, p: [0.0; 2] })]
        }
        "MONOC" => {
            let (inp, w_in) = (ctx.c(0)?, ctx.c(1)?);
            let outs = [ctx.c(2)?, ctx.c_opt(3)?];
            let so = ctx.b.req_c(2);
            vec![Box::new(Mono { inp, w_in, outs, w: 0.0, so, p: [0.0; 2] })]
        }
        "PWCT" => {
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let so = ctx.b.req_c(2);
            vec![Box::new(Pwct { i, o, so, p: [0.0; 2] })]
        }
        "UPDN" => {
            let (u, d, r) = (ctx.c(0)?, ctx.c(1)?, ctx.c_opt(2)?);
            let o = ctx.c(3)?;
            let bits = ctx.int("bits", 8.0)?;
            let so = ctx.b.req_c(1);
            vec![Box::new(UpDn { u, d, r, o, md: js_pow(2.0, bits as f64), so, c: 0.0 })]
        }
        "ADC" => {
            let ins = [ctx.c(0)?, ctx.c(1)?, ctx.c(2)?];
            let o = ctx.c(3)?;
            let n = ctx.int("bits", 8.0)? as f64;
            let so = ctx.b.req_c(1);
            vec![Box::new(Adc { ins, o, n, so, y: 0.0 })]
        }
        "DAC" => {
            let n = ctx.int("bits", 8.0)? as f64;
            binary(ctx, FnK::Dac(n))?
        }
        "ZOH" | "UDELAY" | "DD" => {
            let fs = ctx.pos("fs")?;
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let (k, len) = match t.as_str() {
                "ZOH" => (SampK::Zoh, 1),
                "UDELAY" => (SampK::UDelay, 2),
                _ => (SampK::Dd, 3),
            };
            let so = ctx.b.req_c(len);
            vec![Box::new(Samp { ins: vec![i], o, fs, so, k, y: 0.0, is_hit: false })]
        }
        "ID" | "IRSTD" | "IRSTI" => {
            let fs = ctx.pos("fs")?;
            let alg = ctx.int("alg", 0.0)?;
            let y0 = ctx.numd("y0", 0.0)?;
            let (rst, rflag, wrap) = match t.as_str() {
                "IRSTD" => (ctx.c(1)?, ctx.int("flag", 0.0)?, None),
                "IRSTI" => {
                    let (lo, up) = (ctx.num("lo")?, ctx.num("hi")?);
                    if !(up > lo) {
                        return Err(ctx.err("上限必須大於下限"));
                    }
                    (-1, 0, Some((lo, up)))
                }
                _ => (-1, 0, None),
            };
            let i0 = ctx.c(0)?;
            let ins = if rst >= 0 { vec![i0, rst] } else { vec![i0] };
            let o = ctx.c(if rst >= 0 { 2 } else { 1 })?;
            let so = ctx.b.req_c(3);
            vec![Box::new(Samp { ins, o, fs, so, k: SampK::IntD { alg, y0, rflag, wrap, r_prev: 0.0 }, y: 0.0, is_hit: false })]
        }
        "TFZ" | "FILTD" | "FIR" => {
            let mut b = ctx.list("b")?;
            let mut a = if t == "FIR" { vec![1.0] } else { ctx.list("a")? };
            if b.is_empty() {
                return Err(ctx.err("至少需要一個分子係數"));
            }
            if t != "FIR" && a.first().copied().unwrap_or(0.0) == 0.0 {
                return Err(ctx.err("a0 不可為 0"));
            }
            let fs = ctx.pos("fs")?;
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let n = b.len().max(a.len()) - 1;
            b.resize(n + 1, 0.0);
            a.resize(n + 1, 0.0);
            let so = ctx.b.req_c(2 * n + 1);
            vec![Box::new(Samp { ins: vec![i], o, fs, so, k: SampK::DiffEq { b, a, n, u: 0.0 }, y: 0.0, is_hit: false })]
        }
        "DIGIT" | "DIGIT2" => {
            let (a, b) = (ctx.num("vimin")?, ctx.num("vimax")?);
            if !(b > a) {
                return Err(ctx.err("Vin_max 必須大於 Vin_min"));
            }
            let n = ctx.int_req("N")? as f64;
            let (c, d) = (ctx.num("vomin")?, ctx.num("vomax")?);
            let fs = ctx.pos("fs")?;
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let so = ctx.b.req_c(1);
            vec![Box::new(Samp { ins: vec![i], o, fs, so, k: SampK::Quant { n, a, b, c, d, rnd: t == "DIGIT2" }, y: 0.0, is_hit: false })]
        }
        "CBUF" => {
            let l = ctx.int_req("L")?.max(1);
            if l > MAX_VEC {
                return Err(ctx.err(format!("長度 L 上限為 {}", MAX_VEC)));
            }
            let l = l as usize;
            let fs = ctx.pos("fs")?;
            let (i, o) = (ctx.c(0)?, ctx.c(1)?);
            let so = ctx.b.req_c(l + 1);
            vec![Box::new(Samp { ins: vec![i], o, fs, so, k: SampK::CBuf { l, v: vec![0.0; l] }, y: 0.0, is_hit: false })]
        }
        "ARRAY" => {
            let vals = ctx.list("vals")?;
            let l = ctx.int("L", vals.len() as f64)?.max(0);
            if l > MAX_VEC {
                return Err(ctx.err(format!("長度 L 上限為 {}", MAX_VEC)));
            }
            let l = l as usize;
            let arr: Vec<f64> = (0..l).map(|i| vals.get(i).copied().filter(|v| *v != 0.0 && !v.is_nan()).unwrap_or(0.0)).collect();
            misc(vec![], vec![ctx.c(0)?], MiscK::Array(arr))
        }
        "CONV" => {
            let ins = vec![ctx.c(0)?, ctx.c(1)?];
            misc(ins, vec![ctx.c(2)?], MiscK::Conv)
        }
        "MEMREAD" => {
            let off = ctx.int("off", 0.0)?.max(0) as usize;
            let i = ctx.c(0)?;
            misc(vec![i], vec![ctx.c(1)?], MiscK::MemRead(off))
        }
        "STACK" => {
            let depth = ctx.int("depth", 8.0)?.max(1);
            if depth > MAX_VEC {
                return Err(ctx.err(format!("堆疊深度上限為 {}", MAX_VEC)));
            }
            let depth = depth as usize;
            let ins = [ctx.c(0)?, ctx.c(1)?, ctx.c(2)?];
            let o = ctx.c(3)?;
            vec![Box::new(Stack { ins, o, depth, st: vec![], pst: vec![], y: 0.0, last_y: NAN })]
        }
        "ONCTRL" => {
            let i = ctx.c(0)?;
            misc(vec![i], vec![ctx.c(1)?], MiscK::OnCtrl)
        }
        "GATING" => {
            let f = ctx.num("f")?;
            let pts = ctx.list("pts")?;
            let n = ctx.int("n", pts.len() as f64)?;
            if n != pts.len() as i64 {
                return Err(ctx.err(format!("No. of Points（{}）與實際切換點數（{}）不符", n, pts.len())));
            }
            for i in 1..pts.len() {
                if pts[i] < pts[i - 1] {
                    return Err(ctx.err("切換點必須遞增"));
                }
            }
            if f > 0.0 && pts.iter().any(|&p| !(0.0..=360.0).contains(&p)) {
                return Err(ctx.err("頻率大於 0 時，切換點以度數表示，須介於 0~360"));
            }
            let tp_ = if f > 0.0 { 1.0 / f } else { 0.0 };
            let tp: Vec<f64> = if f > 0.0 { pts.iter().map(|p| p / 360.0 * tp_).collect() } else { pts.clone() };
            let offs = if tp_ > 0.0 { tp.iter().map(|p| p - (p / tp_).floor() * tp_).collect() } else { vec![] };
            vec![Box::new(Gating { o: ctx.c(0)?, f, tp_, tp, offs })]
        }
        "ACTRL" => {
            let (f, pw) = (ctx.pos("f")?, ctx.num("pw")?);
            let ins = [ctx.c(0)?, ctx.c(1)?, ctx.c(2)?];
            let o = ctx.c(3)?;
            let so = ctx.b.req_c(1);
            vec![Box::new(Alpha { ins, o, f, pw, so, ts: f64::NEG_INFINITY })]
        }
        "MATH1" | "MATH2" | "MATH3" | "MATH5" | "MATH10" => {
            let n: usize = t[4..].parse().unwrap_or(1);
            let mut ins = vec![];
            for i in 0..n {
                ins.push(ctx.c(i)?);
            }
            let mut names: Vec<String> = vec!["t".into()];
            for i in 1..=n {
                names.push(format!("x{}", i));
            }
            if n == 1 {
                names.push("x".into());
            }
            let nr: Vec<&str> = names.iter().map(|s| s.as_str()).collect();
            let f = ctx.func("expr", &nr)?;
            let o = ctx.c(n)?;
            misc(ins, vec![o], MiscK::Math(f, n))
        }
        "CBLOCK" => super::cblock::make(ctx)?,
        _ => return Ok(None),
    };
    Ok(Some(r))
}
