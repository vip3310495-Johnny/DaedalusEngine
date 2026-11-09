//! Power-circuit models.
//! Sign conventions
//!   - MNA rows for nodes: sum of currents leaving the node = injected current
//!   - branch variable current flows from the first terminal to the second
//!   - a current source drives current out of its "+" terminal into the external circuit (PSIM convention)

use super::wave::{make_wave, w_sin, Wave};
use crate::clone_box;
use crate::sim::*;

pub const RON: f64 = 1e-5;
pub const ROFF: f64 = 1e6; // PSIM: 10 µΩ on, 1 MΩ off
const EPSV: f64 = 1e-9;
const EPSI: f64 = 1e-10;

fn cp() -> Caps {
    Caps { p: true, ..Default::default() }
}
fn fix(k: &mut i32, n: i32) {
    if *k >= 0 {
        *k += n;
    }
}

/* ------------------------------------------------------------------ Resistor */
#[derive(Clone)]
pub struct Res {
    pub a: i32,
    pub b: i32,
    pub r: f64,
    pub want_i: bool,
    pub k: i32,
}
impl Res {
    pub fn new(a: i32, b: i32, r: f64) -> Self {
        Res { a, b, r, want_i: r == 0.0, k: -1 }
    }
}
impl Inst for Res {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cp() }
    }
    fn setup(&mut self, b: &mut Builder) {
        if self.want_i {
            self.k = b.var();
        }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn is_rlc(&self) -> bool {
        true
    }
    fn set_want_i(&mut self) {
        self.want_i = true;
    }
    fn branch_k(&self) -> i32 {
        self.k
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.b]]
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        if self.want_i {
            m.br(self.a, self.b, self.k);
            m.v(self.k, self.a, self.b, 1.0);
            m.a(self.k, self.k, -self.r);
        } else {
            m.g(self.a, self.b, 1.0 / self.r);
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        if self.want_i {
            c.a(self.a, self.k, 1.0, 0.0);
            c.a(self.b, self.k, -1.0, 0.0);
            c.a(self.k, self.a, 1.0, 0.0);
            c.a(self.k, self.b, -1.0, 0.0);
            c.a(self.k, self.k, -self.r, 0.0);
        } else {
            c.g(self.a, self.b, 1.0 / self.r, 0.0);
        }
        Ok(())
    }
}

/* ------------------------------------------------------------------ Capacitor */
#[derive(Clone)]
pub struct Cap {
    a: i32,
    b: i32,
    c: f64,
    v0: f64,
    want_i: bool,
    k: i32,
    iv: i32,
    so: usize,
}
impl Cap {
    pub fn new(bd: &mut Builder, a: i32, b: i32, c: f64, v0: f64) -> Self {
        Cap { a, b, c, v0, want_i: false, k: -1, iv: -1, so: bd.req_p(2) }
    }
    fn gg(&self, h: f64, be: bool) -> f64 {
        (if be { 1.0 } else { 2.0 }) * self.c / h
    }
}
impl Inst for Cap {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, init_g: true, init_b: true, ..cp() }
    }
    fn setup(&mut self, b: &mut Builder) {
        if self.want_i {
            self.k = b.var();
        }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn is_rlc(&self) -> bool {
        true
    }
    fn set_want_i(&mut self) {
        self.want_i = true;
    }
    fn branch_k(&self) -> i32 {
        self.k
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        if self.c > 0.0 {
            vec![vec![self.a, self.b]]
        } else {
            vec![]
        }
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = self.v0;
        e.ps[self.so + 1] = 0.0;
        Ok(())
    }
    // with a branch-current variable the initial-voltage constraint uses that variable (no extra unknown)
    fn init_var(&self) -> bool {
        self.c > 0.0 && !self.want_i
    }
    fn set_iv(&mut self, iv: i32) {
        self.iv = iv;
    }
    fn stamp_g(&self, m: &mut Mat, h: f64, be: bool, _e: &E) {
        if !(self.c > 0.0) {
            return;
        }
        let g = self.gg(h, be);
        if self.want_i {
            m.br(self.a, self.b, self.k);
            m.a(self.k, self.k, 1.0);
            m.a(self.k, self.a, -g);
            m.a(self.k, self.b, g);
        } else {
            m.g(self.a, self.b, g);
        }
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, h: f64, be: bool, e: &E) {
        if !(self.c > 0.0) {
            return;
        }
        let g = self.gg(h, be);
        let s = &e.ps[self.so..];
        let i = if be { g * s[0] } else { g * s[0] + s[1] };
        if self.want_i {
            b[self.k as usize] -= i;
        } else {
            ai(b, self.a, self.b, -i);
        }
    }
    fn stamp_init_g(&self, m: &mut Mat, _r: bool, _e: &E) {
        if !(self.c > 0.0) {
            return;
        }
        let k = if self.want_i { self.k } else { self.iv };
        m.br(self.a, self.b, k);
        m.v(k, self.a, self.b, 1.0);
    }
    fn stamp_init_b(&self, b: &mut [f64], _r: bool, _e: &E) {
        if !(self.c > 0.0) {
            return;
        }
        let k = if self.want_i { self.k } else { self.iv };
        b[k as usize] += self.v0;
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        if !(self.c > 0.0) {
            return;
        }
        let v = vx(x, self.a) - vx(x, self.b);
        let g = self.gg(h, be);
        let s = &mut e.ps[self.so..];
        let i = if be { g * (v - s[0]) } else { g * (v - s[0]) - s[1] };
        s[0] = v;
        s[1] = i;
    }
    fn accept_init(&mut self, _x: &[f64], e: &mut E) {
        if !(self.c > 0.0) {
            return;
        }
        let k = if self.want_i { self.k } else { self.iv };
        e.ps[self.so] = self.v0;
        e.ps[self.so + 1] = match &e.init_currents {
            Some(ic) if k >= 0 && (k as usize) < ic.len() => ic[k as usize],
            _ => 0.0,
        };
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        if !(self.c > 0.0) {
            if self.want_i {
                c.a(self.k, self.k, 1.0, 0.0);
            }
            return Ok(());
        }
        let w = c.w;
        if self.want_i {
            c.a(self.a, self.k, 1.0, 0.0);
            c.a(self.b, self.k, -1.0, 0.0);
            c.a(self.k, self.k, 1.0, 0.0);
            c.a(self.k, self.a, 0.0, -w * self.c);
            c.a(self.k, self.b, 0.0, w * self.c);
        } else {
            c.g(self.a, self.b, 0.0, w * self.c);
        }
        Ok(())
    }
}

/* ------------------------------------------------------------------ Series R-L-C branch */
#[derive(Clone)]
pub struct SerBr {
    a: i32,
    b: i32,
    r: f64,
    l: f64,
    c: f64,
    i0: f64,
    vc0: f64,
    k: i32,
    so: usize,
}
impl SerBr {
    #[allow(clippy::too_many_arguments)]
    pub fn new(bd: &mut Builder, a: i32, b: i32, r: f64, l: f64, c: f64, i0: f64, vc0: f64) -> Self {
        let k = bd.var();
        let so = bd.req_p(3);
        SerBr { a, b, r, l, c, i0, vc0, k, so }
    }
    fn req(&self, h: f64, be: bool) -> f64 {
        let mut r = self.r;
        if self.l != 0.0 {
            r += (if be { 1.0 } else { 2.0 }) * self.l / h;
        }
        if self.c != 0.0 {
            r += (if be { 1.0 } else { 0.5 }) * h / self.c;
        }
        r
    }
    fn init_as_i(&self, relax: bool) -> bool {
        self.l != 0.0 && !(relax && self.i0 == 0.0)
    }
}
impl Inst for SerBr {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, init_g: true, init_b: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn is_rlc(&self) -> bool {
        true
    }
    fn branch_k(&self) -> i32 {
        self.k
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.b]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        let s = &mut e.ps[self.so..];
        s[0] = if self.l != 0.0 { self.i0 } else { 0.0 };
        s[1] = 0.0;
        s[2] = self.vc0;
        Ok(())
    }
    fn stamp_g(&self, m: &mut Mat, h: f64, be: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        m.v(self.k, self.a, self.b, 1.0);
        m.a(self.k, self.k, -self.req(h, be));
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, h: f64, be: bool, e: &E) {
        let s = &e.ps[self.so..];
        let mut v = 0.0;
        if self.l != 0.0 {
            v += if be { -(self.l / h) * s[0] } else { -(2.0 * self.l / h) * s[0] - s[1] };
        }
        if self.c != 0.0 {
            v += if be { s[2] } else { s[2] + (h / (2.0 * self.c)) * s[0] };
        }
        b[self.k as usize] += v;
    }
    fn stamp_init_g(&self, m: &mut Mat, relax: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        if self.init_as_i(relax) {
            m.a(self.k, self.k, 1.0);
        } else {
            m.v(self.k, self.a, self.b, 1.0);
            m.a(self.k, self.k, -self.r);
        }
    }
    fn stamp_init_b(&self, b: &mut [f64], relax: bool, _e: &E) {
        if self.init_as_i(relax) {
            b[self.k as usize] += self.i0;
        } else if self.c != 0.0 {
            b[self.k as usize] += self.vc0;
        }
    }
    fn relaxed_short(&self) -> Option<i32> {
        if self.l != 0.0 && self.i0 == 0.0 {
            Some(self.k)
        } else {
            None
        }
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        let s = &mut e.ps[self.so..];
        let i_n = x[self.k as usize];
        let i_p = s[0];
        if self.l != 0.0 {
            s[1] = if be { (self.l / h) * (i_n - i_p) } else { (2.0 * self.l / h) * (i_n - i_p) - s[1] };
        }
        if self.c != 0.0 {
            s[2] = if be { s[2] + (h / self.c) * i_n } else { s[2] + (h / (2.0 * self.c)) * (i_n + i_p) };
        }
        s[0] = i_n;
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        let i = x[self.k as usize];
        let v = vx(x, self.a) - vx(x, self.b);
        let s = &mut e.ps[self.so..];
        s[0] = i;
        if self.l != 0.0 {
            s[1] = v - self.r * i - if self.c != 0.0 { s[2] } else { 0.0 };
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let w = c.w;
        let mut xi = 0.0;
        if self.l != 0.0 {
            xi += w * self.l;
        }
        if self.c != 0.0 {
            xi -= 1.0 / (w * self.c);
        }
        c.a(self.a, self.k, 1.0, 0.0);
        c.a(self.b, self.k, -1.0, 0.0);
        c.a(self.k, self.a, 1.0, 0.0);
        c.a(self.k, self.b, -1.0, 0.0);
        c.a(self.k, self.k, -self.r, -xi);
        Ok(())
    }
}

/* ------------------------------------------------------------------ Coupled inductors */
#[derive(Clone)]
pub struct MutL {
    w: Vec<(i32, i32)>,
    l: Vec<Vec<f64>>,
    i0: Vec<f64>,
    k: Vec<i32>,
    so: usize,
}
impl MutL {
    pub fn new(bd: &mut Builder, w: Vec<(i32, i32)>, l: Vec<Vec<f64>>, i0: Vec<f64>) -> Self {
        let k: Vec<i32> = (0..w.len()).map(|_| bd.var()).collect();
        let so = bd.req_p(2 * w.len());
        MutL { w, l, i0, k, so }
    }
}
impl Inst for MutL {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, init_g: true, init_b: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        for k in self.k.iter_mut() {
            fix(k, n);
        }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        self.w.iter().map(|p| vec![p.0, p.1]).collect()
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        let m = self.w.len();
        for j in 0..m {
            e.ps[self.so + j] = self.i0.get(j).copied().unwrap_or(0.0);
            e.ps[self.so + m + j] = 0.0;
        }
        Ok(())
    }
    fn stamp_g(&self, mt: &mut Mat, h: f64, be: bool, _e: &E) {
        let c = (if be { 1.0 } else { 2.0 }) / h;
        let m = self.w.len();
        for j in 0..m {
            let (a, b) = self.w[j];
            let k = self.k[j];
            mt.br(a, b, k);
            mt.v(k, a, b, 1.0);
            for l in 0..m {
                mt.a(k, self.k[l], -c * self.l[j][l]);
            }
        }
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, h: f64, be: bool, e: &E) {
        let c = (if be { 1.0 } else { 2.0 }) / h;
        let m = self.w.len();
        let s = &e.ps[self.so..];
        for j in 0..m {
            let mut v = 0.0;
            for l in 0..m {
                v -= c * self.l[j][l] * s[l];
            }
            if !be {
                v -= s[m + j];
            }
            b[self.k[j] as usize] += v;
        }
    }
    fn stamp_init_g(&self, mt: &mut Mat, _r: bool, _e: &E) {
        for j in 0..self.w.len() {
            let (a, b) = self.w[j];
            mt.br(a, b, self.k[j]);
            mt.a(self.k[j], self.k[j], 1.0);
        }
    }
    fn stamp_init_b(&self, b: &mut [f64], _r: bool, _e: &E) {
        for j in 0..self.w.len() {
            b[self.k[j] as usize] += self.i0.get(j).copied().unwrap_or(0.0);
        }
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        let m = self.w.len();
        for j in 0..m {
            let (a, b) = self.w[j];
            e.ps[self.so + j] = x[self.k[j] as usize];
            e.ps[self.so + m + j] = vx(x, a) - vx(x, b);
        }
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        let c = (if be { 1.0 } else { 2.0 }) / h;
        let m = self.w.len();
        let mut v = vec![0.0; m];
        {
            let s = &e.ps[self.so..];
            for j in 0..m {
                let mut a = 0.0;
                for l in 0..m {
                    a += c * self.l[j][l] * (x[self.k[l] as usize] - s[l]);
                }
                v[j] = if be { a } else { a - s[m + j] };
            }
        }
        let s = &mut e.ps[self.so..];
        for j in 0..m {
            s[m + j] = v[j];
            s[j] = x[self.k[j] as usize];
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let m = self.w.len();
        for j in 0..m {
            let (a, b) = self.w[j];
            let k = self.k[j];
            c.a(a, k, 1.0, 0.0);
            c.a(b, k, -1.0, 0.0);
            c.a(k, a, 1.0, 0.0);
            c.a(k, b, -1.0, 0.0);
            for l in 0..m {
                let w = c.w;
                c.a(k, self.k[l], 0.0, -w * self.l[j][l]);
            }
        }
        Ok(())
    }
}

/* ------------------------------------------------------------------ Ideal multi-winding transformer */
#[derive(Clone)]
pub struct Xfmr {
    w: Vec<(i32, i32, f64)>,
    ks: Vec<i32>,
}
impl Xfmr {
    pub fn new(bd: &mut Builder, w: Vec<(i32, i32, f64)>) -> Self {
        let ks = (1..w.len()).map(|_| bd.var()).collect();
        Xfmr { w, ks }
    }
    fn st(&self, add: &mut dyn FnMut(i32, i32, f64)) {
        let (a1, b1, n1) = self.w[0];
        for j in 1..self.w.len() {
            let (a, b, n) = self.w[j];
            let k = self.ks[j - 1];
            let r = n / n1;
            add(a, k, 1.0);
            add(b, k, -1.0);
            add(a1, k, -r);
            add(b1, k, r);
            add(k, a, 1.0);
            add(k, b, -1.0);
            add(k, a1, -r);
            add(k, b1, r);
        }
    }
}
impl Inst for Xfmr {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        for k in self.ks.iter_mut() {
            fix(k, n);
        }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        self.w.iter().map(|p| vec![p.0, p.1]).collect()
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        self.st(&mut |i, j, v| m.a(i, j, v));
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        self.st(&mut |i, j, v| c.a(i, j, v, 0.0));
        Ok(())
    }
}

/* ------------------------------------------------------------------ Generic PWL switch */
#[derive(Clone, Copy, PartialEq, Debug)]
pub enum SwT {
    D,
    Zen,
    Diac,
    Thy,
    Triac,
    Gto,
    Ssw,
    Fix,
}
#[derive(Clone, Default)]
pub struct SwO {
    pub ron: f64,
    pub roff: f64,
    pub vf: f64,
    pub vb: f64,
    pub vbb: f64,
    pub ih: f64,
    pub il: f64,
    pub gn: Option<i32>,
    pub ginv: bool,
    pub init: i32,
    /// gate driven by the power circuit (instead of the logic signal gn)
    pub pg: Option<PGate>,
}

/// A base / gate connected to the power circuit: drive = pol·(v(g) − v(r)); the gate is on while the drive exceeds
/// the junction drop vj, i.e. while the junction diode (a separate element between g and r) conducts.
/// `both`: either polarity triggers (TRIAC gate referred to MT1).
#[derive(Clone, Copy, Debug)]
pub struct PGate {
    pub g: i32,
    pub r: i32,
    pub pol: f64,
    pub vj: f64,
    pub both: bool,
}
impl PGate {
    /// > 0 while the gate is driven
    pub fn margin(&self, x: &[f64]) -> f64 {
        let d = self.pol * (vx(x, self.g) - vx(x, self.r));
        if self.both {
            d.abs() - self.vj
        } else {
            d - self.vj
        }
    }
}
/// state 0 = off, 1 = forward on, 2 = reverse on
#[derive(Clone)]
pub struct Sw {
    a: i32,
    k: i32,
    t: SwT,
    ron: f64,
    roff: f64,
    vf: f64,
    vb: f64,
    vbb: f64,
    ih: f64,
    il: f64,
    gn: i32,
    ginv: bool,
    init0: i32,
    so: usize,
    times: Option<Vec<f64>>,
    pg: Option<PGate>,
}
impl Sw {
    pub fn new(bd: &mut Builder, a: i32, k: i32, t: SwT, o: SwO) -> Self {
        let ron = (if o.ron != 0.0 && !o.ron.is_nan() { o.ron } else { RON }).max(1e-9);
        let roff = if o.roff != 0.0 && !o.roff.is_nan() { o.roff } else { ROFF };
        Sw { a, k, t, ron, roff, vf: o.vf, vb: o.vb, vbb: o.vbb, ih: o.ih, il: o.il, gn: if o.pg.is_some() { -1 } else { o.gn.unwrap_or(-1) }, ginv: o.ginv, init0: o.init, so: bd.req_p(4), times: None, pg: o.pg }
    }
    /// push-button / scheduled switch
    pub fn timed(bd: &mut Builder, a: i32, b: i32, init: i32, mut times: Vec<f64>) -> Self {
        times.sort_by(|p, q| p.partial_cmp(q).unwrap_or(std::cmp::Ordering::Equal));
        let mut s = Sw::new(bd, a, b, SwT::Fix, SwO { init, ..Default::default() });
        s.times = Some(times);
        s
    }
    #[inline]
    fn s(&self, e: &E, i: usize) -> f64 {
        e.ps[self.so + i]
    }
    #[inline]
    fn st(&self, e: &E) -> i32 {
        e.ps[self.so] as i32
    }
    #[inline]
    fn v(&self, x: &[f64]) -> f64 {
        vx(x, self.a) - vx(x, self.k)
    }
    fn fwd_drop(&self) -> f64 {
        if self.t == SwT::Diac {
            self.vbb
        } else {
            self.vf
        }
    }
    fn rev_drop(&self) -> f64 {
        match self.t {
            SwT::Zen => self.vb,
            SwT::Diac => self.vbb,
            _ => self.vf,
        }
    }
    fn v0(&self, e: &E) -> f64 {
        match self.st(e) {
            1 => self.fwd_drop(),
            2 => -self.rev_drop(),
            _ => 0.0,
        }
    }
    fn g(&self, e: &E) -> f64 {
        if self.st(e) != 0 {
            1.0 / self.ron
        } else {
            1.0 / self.roff
        }
    }
    fn i(&self, x: &[f64], e: &E) -> f64 {
        self.g(e) * (self.v(x) - self.v0(e))
    }
    fn eg(&self, e: &E) -> bool {
        let g = self.s(e, 1);
        let g = if self.ginv { 1.0 - g } else { g };
        g != 0.0
    }
    /// gate state for the solution x: the logic signal, or the conducting junction of a power-driven gate
    fn gate_on(&self, x: &[f64], e: &E) -> bool {
        match &self.pg {
            Some(p) => p.margin(x) > EPSV,
            None => self.eg(e),
        }
    }
    /// crossing function governing a change of a power-gated switch: turning on needs the gate AND the forward
    /// voltage (the later crossing); a GTO / transistor turns off at gate release OR current reversal (the earlier)
    fn pg_q(&self, xa: &[f64], xb: &[f64], ns: i32, e: &E) -> Option<(f64, f64)> {
        let p = self.pg.as_ref()?;
        let cross = |q: (f64, f64)| if q.0 != q.1 && (q.0 > 0.0) != (q.1 > 0.0) { Some(q.0 / (q.0 - q.1)) } else { None };
        let qg = (p.margin(xa), p.margin(xb));
        let st = self.st(e);
        if st == 0 && ns != 0 {
            let th = self.vf;
            let m = |x: &[f64]| if ns == 2 { -self.v(x) - th } else { self.v(x) - th };
            let qm = (m(xa), m(xb));
            return match (cross(qm), cross(qg)) {
                (Some(a), Some(b)) => Some(if a >= b { qm } else { qg }),
                (Some(_), None) => Some(qm),
                (None, Some(_)) => Some(qg),
                _ => None,
            };
        }
        if st != 0 && ns == 0 && self.t == SwT::Gto {
            let qi = (self.i(xa, e), self.i(xb, e));
            return match (cross(qi), cross(qg)) {
                (Some(a), Some(b)) => Some(if a <= b { qi } else { qg }),
                (Some(_), None) => Some(qi),
                (None, Some(_)) => Some(qg),
                _ => None,
            };
        }
        None
    }
}
impl Inst for Sw {
    clone_box!();
    fn caps(&self) -> Caps {
        let t = self.times.is_some();
        Caps { sw: true, qq: true, timed: t, nb: t, jump: t, ..cp() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.k]]
    }
    fn cins(&self) -> Vec<i32> {
        if self.gn >= 0 {
            vec![self.gn]
        } else {
            vec![]
        }
    }
    fn gate_node(&self) -> i32 {
        self.gn
    }
    fn so(&self) -> usize {
        self.so
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        let s = &mut e.ps[self.so..];
        s[0] = self.init0 as f64;
        s[1] = 0.0;
        s[2] = -1.0;
        s[3] = 0.0;
        if self.times.is_some() {
            e.ps[self.so] = self.state_at(0.0) as f64;
        }
        Ok(())
    }
    fn state(&self, e: &E) -> i32 {
        self.st(e)
    }
    fn set_state(&mut self, v: i32, e: &mut E) {
        e.ps[self.so] = v as f64;
        if v == 0 {
            e.ps[self.so + 3] = 0.0;
        }
    }
    fn skey(&self, e: &E) -> u32 {
        self.st(e) as u32
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, e: &E) {
        m.g(self.a, self.k, self.g(e));
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        let v0 = self.v0(e);
        if v0 != 0.0 {
            let g = self.g(e);
            ai(b, self.a, self.k, -g * v0);
        }
    }
    fn accept(&mut self, x: &[f64], _t: f64, _h: f64, _be: bool, e: &mut E) {
        if self.t == SwT::Thy && self.st(e) == 1 && self.i(x, e) >= self.il {
            e.ps[self.so + 3] = 1.0;
        }
    }
    fn desired(&self, x: &[f64], _t: f64, e: &E) -> i32 {
        if self.times.is_some() {
            return self.st(e);
        }
        let st = self.st(e);
        let v = self.v(x);
        match self.t {
            SwT::D => {
                if st == 0 {
                    return if v > self.vf + EPSV { 1 } else { 0 };
                }
                if self.i(x, e) < -EPSI {
                    0
                } else {
                    1
                }
            }
            SwT::Zen => {
                if st == 0 {
                    return if v > self.vf + EPSV {
                        1
                    } else if -v > self.vb + EPSV {
                        2
                    } else {
                        0
                    };
                }
                if st == 1 {
                    return if self.i(x, e) < -EPSI { 0 } else { 1 };
                }
                if self.i(x, e) > EPSI {
                    0
                } else {
                    2
                }
            }
            SwT::Diac => {
                if st == 0 {
                    return if v > self.vb + EPSV {
                        1
                    } else if -v > self.vb + EPSV {
                        2
                    } else {
                        0
                    };
                }
                if st == 1 {
                    return if self.i(x, e) < -EPSI { 0 } else { 1 };
                }
                if self.i(x, e) > EPSI {
                    0
                } else {
                    2
                }
            }
            SwT::Thy => {
                if st == 0 {
                    return if self.gate_on(x, e) && v > self.vf + EPSV { 1 } else { 0 };
                }
                let i = self.i(x, e);
                if i < self.ih - EPSI {
                    return 0;
                }
                if !self.gate_on(x, e) && self.s(e, 3) == 0.0 && self.il > 0.0 && i < self.il {
                    return 0;
                }
                1
            }
            SwT::Triac => {
                if st == 0 {
                    if !self.gate_on(x, e) {
                        return 0;
                    }
                    return if v > self.vf + EPSV {
                        1
                    } else if -v > self.vf + EPSV {
                        2
                    } else {
                        0
                    };
                }
                if st == 1 {
                    return if self.i(x, e) < -EPSI { 0 } else { 1 };
                }
                if self.i(x, e) > EPSI {
                    0
                } else {
                    2
                }
            }
            SwT::Gto => {
                if !self.gate_on(x, e) {
                    return 0;
                }
                if st == 0 {
                    return if v > self.vf + EPSV { 1 } else { 0 };
                }
                if self.i(x, e) < -EPSI {
                    0
                } else {
                    1
                }
            }
            SwT::Ssw => {
                if self.eg(e) {
                    1
                } else {
                    0
                }
            }
            SwT::Fix => st,
        }
    }
    fn qq(&self, xa: &[f64], xb: &[f64], ns: i32, e: &E) -> Option<(f64, f64)> {
        if self.pg.is_some() {
            if let Some(q) = self.pg_q(xa, xb, ns, e) {
                return Some(q);
            }
        }
        let st = self.st(e);
        if self.t == SwT::Ssw || self.t == SwT::Fix {
            return None;
        }
        if st != 0 && ns == 0 {
            let th = if self.t == SwT::Thy { self.ih } else { 0.0 };
            return Some((self.i(xa, e) - th, self.i(xb, e) - th));
        }
        if st == 0 && ns == 1 {
            let th = if self.t == SwT::Diac { self.vb } else { self.vf };
            return Some((self.v(xa) - th, self.v(xb) - th));
        }
        if st == 0 && ns == 2 {
            let th = if self.t == SwT::Zen || self.t == SwT::Diac { self.vb } else { self.vf };
            return Some((-self.v(xa) - th, -self.v(xb) - th));
        }
        None
    }
    fn tau(&self, xa: &[f64], xb: &[f64], ns: i32, e: &E) -> f64 {
        match self.qq(xa, xb, ns, e) {
            None => NAN,
            Some((qa, qb)) => {
                if qa == qb || (qa > 0.0) == (qb > 0.0) {
                    NAN
                } else {
                    qa / (qa - qb)
                }
            }
        }
    }
    fn on_gate(&mut self, g: i32, _init: bool, e: &mut E) -> bool {
        let eg = if self.ginv { 1 - g } else { g };
        let st = self.st(e);
        let v = self.v(&e.x_p);
        match self.t {
            SwT::Ssw => {
                if st != eg {
                    e.ps[self.so] = eg as f64;
                    return true;
                }
                false
            }
            SwT::Gto => {
                if eg == 0 && st != 0 {
                    self.set_state(0, e);
                    return true;
                }
                if eg != 0 && st == 0 && v > self.vf + EPSV {
                    e.ps[self.so] = 1.0;
                    return true;
                }
                false
            }
            SwT::Thy => {
                if eg != 0 && st == 0 && v > self.vf + EPSV {
                    e.ps[self.so] = 1.0;
                    return true;
                }
                false
            }
            SwT::Triac => {
                if eg != 0 && st == 0 {
                    if v > self.vf + EPSV {
                        e.ps[self.so] = 1.0;
                        return true;
                    }
                    if -v > self.vf + EPSV {
                        e.ps[self.so] = 2.0;
                        return true;
                    }
                }
                false
            }
            _ => false,
        }
    }
    fn state_at(&self, t: f64) -> i32 {
        let mut s = self.init0;
        if let Some(ts) = &self.times {
            for &tt in ts {
                if tt <= t {
                    s = 1 - s;
                }
            }
        }
        s
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        if let Some(ts) = &self.times {
            for &tt in ts {
                if tt > t {
                    return tt;
                }
            }
        }
        f64::INFINITY
    }
    fn jump_at(&self, t: f64, e: &E) -> bool {
        self.times.as_ref().map(|ts| ts.iter().any(|&tt| (tt - t).abs() <= e.eps_t)).unwrap_or(false)
    }
}

/* ------------------------------------------------------------------ Gate phase shifter (thyristor bridges) */
#[derive(Clone)]
pub struct GateShift {
    ct: i32,
    shifts: Vec<f64>,
    outs: Vec<i32>,
    edges: Vec<(f64, i32)>,
    pend: Option<(f64, i32)>,
    tp: f64,
    del: Vec<f64>,
    err: SimError,
}
impl GateShift {
    pub fn new(ctx: &mut ElCtx, ct: i32, shifts: Vec<f64>) -> Self {
        let outs = shifts.iter().map(|_| ctx.b.cnode()).collect();
        GateShift { ct, shifts, outs, edges: vec![], pend: None, tp: 0.0, del: vec![], err: ctx.err("閘極（Ct）必須連接到 Gating Block 或 Alpha Controller，才能推算其他開關的觸發相位") }
    }
    fn val_at(&self, t: f64, eps: f64) -> i32 {
        let mut v = self.edges[0].1;
        for i in (0..self.edges.len()).rev() {
            if self.edges[i].0 <= t + eps {
                v = self.edges[i].1;
                break;
            }
        }
        v
    }
}
impl Inst for GateShift {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { c: true, nb: true, ..Default::default() }
    }
    fn ins(&self) -> Vec<i32> {
        vec![self.ct]
    }
    fn outs(&self) -> Vec<i32> {
        self.outs.clone()
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        let f = e.gate_freq.get(self.ct as usize).copied().unwrap_or(0.0);
        if !(f > 0.0) {
            return Err(self.err.clone());
        }
        self.tp = 1.0 / f;
        self.del = self.shifts.iter().map(|s| s / 360.0 * self.tp).collect();
        self.edges = vec![(f64::NEG_INFINITY, 0)];
        Ok(())
    }
    fn eval(&mut self, t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let ct = self.ct as usize;
        let g = if e.cv[ct] > 0.5 { 1 } else { 0 };
        let last = self.edges.last().unwrap().1;
        self.pend = None;
        if g != last {
            let ev = e.cev[ct];
            self.pend = Some((if ev.is_finite() { ev } else { t }, g));
        }
        for k in 0..self.outs.len() {
            let o = self.outs[k] as usize;
            let d = self.del[k];
            let v;
            if d == 0.0 {
                v = g;
                e.cev[o] = e.cev[ct];
            } else {
                v = self.val_at(t - d, e.eps_t);
                let prev = e.cv_p[o];
                e.cev[o] = if v as f64 != prev { t } else { NAN };
            }
            e.cv[o] = v as f64;
        }
        Ok(())
    }
    fn commit(&mut self, t: f64, _h: f64, _e: &mut E) {
        if let Some(p) = self.pend {
            self.edges.push(p);
        }
        let mx = self.del.iter().cloned().fold(f64::NEG_INFINITY, f64::max);
        let keep = t - mx - 2.0 * self.tp;
        while self.edges.len() > 2 && self.edges[1].0 < keep {
            self.edges.remove(0);
        }
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        let mut tb = f64::INFINITY;
        for &d in &self.del {
            if d == 0.0 {
                continue;
            }
            for e in &self.edges {
                let te = e.0 + d;
                if te > t && te < tb {
                    tb = te;
                }
            }
        }
        tb
    }
}

/* ------------------------------------------------------------------ Sources */
#[derive(Clone)]
pub struct Pvs {
    p: i32,
    n: i32,
    w: Wave,
    k: i32,
    name: Option<String>,
}
impl Pvs {
    pub fn new(bd: &mut Builder, p: i32, n: i32, w: Wave) -> Self {
        Pvs { p, n, w, k: bd.var(), name: None }
    }
}
impl Inst for Pvs {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, nb: true, jump: true, init_b: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.p, self.n]]
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        m.br(self.p, self.n, self.k);
        m.v(self.k, self.p, self.n, 1.0);
    }
    fn stamp_b(&self, b: &mut [f64], t: f64, _h: f64, _be: bool, e: &E) {
        b[self.k as usize] += self.w.val_side(t, -1.0, e.eps_t);
    }
    fn stamp_init_b(&self, b: &mut [f64], _r: bool, _e: &E) {
        b[self.k as usize] += self.w.val(0.0);
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        self.w.next(t)
    }
    fn jump_at(&self, t: f64, e: &E) -> bool {
        // value jumps and slope corners both restart the integration with backward Euler
        self.w.corner(t, e.eps_t)
    }
    fn src_name(&self) -> Option<String> {
        self.name.clone()
    }
    fn can_inject(&self) -> bool {
        true
    }
    fn set_inj(&mut self, inj: Inj) {
        self.w.inj = Some(inj);
    }
    fn stamp_ac(&self, c: &mut AcCtx, is_src: bool, _e: &E) -> R<()> {
        c.a(self.p, self.k, 1.0, 0.0);
        c.a(self.n, self.k, -1.0, 0.0);
        c.a(self.k, self.p, 1.0, 0.0);
        c.a(self.k, self.n, -1.0, 0.0);
        if is_src {
            c.b(self.k, 0.0, -1.0);
        }
        Ok(())
    }
}

#[derive(Clone)]
pub struct Pis {
    p: i32,
    n: i32,
    w: Wave,
    name: Option<String>,
}
impl Inst for Pis {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, nb: true, jump: true, init_b: true, ..cp() }
    }
    fn stamp_b(&self, b: &mut [f64], t: f64, _h: f64, _be: bool, e: &E) {
        let i = self.w.val_side(t, -1.0, e.eps_t);
        ab(b, self.p, i);
        ab(b, self.n, -i);
    }
    fn stamp_init_b(&self, b: &mut [f64], _r: bool, _e: &E) {
        let i = self.w.val(0.0);
        ab(b, self.p, i);
        ab(b, self.n, -i);
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        self.w.next(t)
    }
    fn jump_at(&self, t: f64, e: &E) -> bool {
        // value jumps and slope corners both restart the integration with backward Euler
        self.w.corner(t, e.eps_t)
    }
    fn src_name(&self) -> Option<String> {
        self.name.clone()
    }
    fn can_inject(&self) -> bool {
        true
    }
    fn set_inj(&mut self, inj: Inj) {
        self.w.inj = Some(inj);
    }
    fn stamp_ac(&self, c: &mut AcCtx, is_src: bool, _e: &E) -> R<()> {
        if is_src {
            c.b(self.p, 0.0, -1.0);
            c.b(self.n, 0.0, 1.0);
        }
        Ok(())
    }
}

/// Control-circuit voltage source (signal generator)
#[derive(Clone)]
pub struct CSrc {
    c: i32,
    w: Wave,
    name: Option<String>,
}
impl Inst for CSrc {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { c: true, ac: true, nb: true, ..Default::default() }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.c]
    }
    fn eval(&mut self, t: f64, _h: f64, e: &mut E, _x: &[f64]) -> R<()> {
        let c = self.c as usize;
        e.cv[c] = self.w.val_side(t, 1.0, e.eps_t);
        e.cev[c] = if self.w.jump(t, e.eps_t) { t } else { NAN };
        Ok(())
    }
    fn next_break(&self, t: f64, _e: &E) -> f64 {
        self.w.next(t)
    }
    fn src_name(&self) -> Option<String> {
        self.name.clone()
    }
    fn can_inject(&self) -> bool {
        true
    }
    fn set_inj(&mut self, inj: Inj) {
        self.w.inj = Some(inj);
    }
    fn stamp_ac(&self, c: &mut AcCtx, is_src: bool, _e: &E) -> R<()> {
        let r = c.ci(self.c);
        c.a(r, r, 1.0, 0.0);
        if is_src {
            c.b(r, 0.0, -1.0);
        }
        Ok(())
    }
}

pub fn vsrc(ctx: &mut ElCtx, wtype: &str, single: bool) -> R<Vec<Box<dyn Inst>>> {
    let mut w = make_wave(ctx, wtype)?;
    let nm = if wtype == "SIN" { Some(ctx.name.clone()) } else { None };
    let m: Box<dyn Inst> = if single {
        if ctx.is_ctl(0) {
            Box::new(CSrc { c: ctx.c(0)?, w, name: nm })
        } else {
            let p = ctx.p(0)?;
            let mut s = Pvs::new(ctx.b, p, -1, w);
            s.name = nm;
            Box::new(s)
        }
    } else {
        let (k0, k1) = (ctx.kind(0), ctx.kind(1));
        if k0 == 'C' || k1 == 'C' {
            if k0 == 'C' && k1 == 'G' {
                Box::new(CSrc { c: ctx.c(0)?, w, name: nm })
            } else if k1 == 'C' && k0 == 'G' {
                w.neg = true;
                Box::new(CSrc { c: ctx.c(1)?, w, name: nm })
            } else {
                return Err(ctx.err("在控制電路中使用電壓源時，負端必須接地"));
            }
        } else {
            let (p, n) = (ctx.p(0)?, ctx.p(1)?);
            let mut s = Pvs::new(ctx.b, p, n, w);
            s.name = nm;
            Box::new(s)
        }
    };
    Ok(vec![m])
}
pub fn isrc(ctx: &mut ElCtx, wtype: &str) -> R<Vec<Box<dyn Inst>>> {
    if ctx.is_ctl(0) || ctx.is_ctl(1) {
        return Err(ctx.err("電流源只能用於功率電路"));
    }
    let (p, n) = (ctx.p(0)?, ctx.p(1)?);
    let w = make_wave(ctx, wtype)?;
    let nm = if wtype == "SIN" { Some(ctx.name.clone()) } else { None };
    Ok(vec![Box::new(Pis { p, n, w, name: nm })])
}
pub fn vsin3(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let vll = ctx.num("V")?;
    let f = ctx.num("f")?;
    let th = ctx.numd("ph", 0.0)?;
    let vm = vll * std::f64::consts::SQRT_2 / 3f64.sqrt();
    let n = ctx.p(0)?;
    let mut out: Vec<Box<dyn Inst>> = Vec::new();
    for (i, d) in [0.0, -120.0, 120.0].iter().enumerate() {
        let p = ctx.p(i + 1)?;
        out.push(Box::new(Pvs::new(ctx.b, p, n, w_sin(vm, f, th + d, 0.0, 0.0))));
    }
    Ok(out)
}

/* ------------------------------------------------------------------ Saturable inductor */
#[derive(Clone)]
pub struct SatL {
    a: i32,
    b: i32,
    ip: Vec<f64>,
    lp: Vec<f64>,
    k: i32,
    so: usize,
}
impl SatL {
    pub fn new(bd: &mut Builder, a: i32, b: i32, ii: &[f64], ll: &[f64]) -> Self {
        let mut ip = vec![0.0];
        let mut lp = vec![0.0];
        for j in 0..ii.len() {
            ip.push(ii[j].abs());
            lp.push(ii[j].abs() * ll[j]);
        }
        let k = bd.var();
        let so = bd.req_p(3);
        SatL { a, b, ip, lp, k, so }
    }
    /// index of the λ(i) segment
    fn seg(&self, i: f64) -> usize {
        let ai_ = i.abs();
        let n = self.ip.len();
        let mut j = 1;
        while j < n - 1 && ai_ > self.ip[j] {
            j += 1;
        }
        if i < 0.0 {
            n + j
        } else {
            j
        }
    }
    /// flux linkage lambda(i) and slope
    fn lam(&self, i: f64) -> (f64, f64) {
        let ai_ = i.abs();
        let sg = if i < 0.0 { -1.0 } else { 1.0 };
        let n = self.ip.len();
        let mut j = 1;
        while j < n - 1 && ai_ > self.ip[j] {
            j += 1;
        }
        let den = self.ip[j] - self.ip[j - 1];
        let sl = (self.lp[j] - self.lp[j - 1]) / if den != 0.0 { den } else { 1e-30 };
        (sg * (self.lp[j - 1] + sl * (ai_ - self.ip[j - 1])), sl.max(1e-15))
    }
}
impl Inst for SatL {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nl: true, init_g: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.b]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for i in 0..3 {
            e.ps[self.so + i] = 0.0;
        }
        Ok(())
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        m.v(self.k, self.a, self.b, 1.0);
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, h: f64, be: bool, _init: bool, e: &mut E) {
        let ig = x[self.k as usize];
        let (lg, sl) = self.lam(ig);
        let c = (if be { 1.0 } else { 2.0 }) / h;
        m.a(self.k, self.k, -c * sl);
        let s = &e.ps[self.so..];
        b[self.k as usize] += c * (lg - sl * ig - s[0]) - if be { 0.0 } else { s[1] };
    }
    fn stamp_init_g(&self, m: &mut Mat, _r: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        m.a(self.k, self.k, 1.0);
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        let i = x[self.k as usize];
        let l = self.lam(i).0;
        let s = &mut e.ps[self.so..];
        s[0] = l;
        s[1] = vx(x, self.a) - vx(x, self.b);
        s[2] = i;
    }
    fn accept(&mut self, x: &[f64], _t: f64, h: f64, be: bool, e: &mut E) {
        let i = x[self.k as usize];
        let l = self.lam(i).0;
        let c = (if be { 1.0 } else { 2.0 }) / h;
        if self.seg(i) != self.seg(e.ps[self.so + 2]) {
            e.damp_req = true;
        }
        let s = &mut e.ps[self.so..];
        s[1] = c * (l - s[0]) - if be { 0.0 } else { s[1] };
        s[0] = l;
        s[2] = i;
    }
}

/* ------------------------------------------------------------------ Nonlinear elements */
#[derive(Clone)]
pub struct NonI {
    a: i32,
    b: i32,
    f: crate::expr::CompiledFn,
    df: crate::expr::CompiledFn,
    v0: f64,
    lo: f64,
    hi: f64,
}
impl NonI {
    fn vg(&self, x: &[f64]) -> f64 {
        self.hi.min(self.lo.max(vx(x, self.a) - vx(x, self.b)))
    }
}
impl Inst for NonI {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nl: true, init_g: true, init_b: true, ..cp() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.b]]
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, _h: f64, _be: bool, _i: bool, _e: &mut E) {
        let v = self.vg(x);
        let g = crate::expr::js_max(&[self.df.call(&[v]), 1e-12]);
        m.g(self.a, self.b, g);
        let i = self.f.call(&[v]) - g * v;
        ai(b, self.a, self.b, i);
    }
    fn stamp_init_g(&self, m: &mut Mat, _r: bool, _e: &E) {
        m.g(self.a, self.b, crate::expr::js_max(&[self.df.call(&[self.v0]), 1e-12]));
    }
    fn stamp_init_b(&self, b: &mut [f64], _r: bool, _e: &E) {
        let g = crate::expr::js_max(&[self.df.call(&[self.v0]), 1e-12]);
        ai(b, self.a, self.b, self.f.call(&[self.v0]) - g * self.v0);
    }
}
#[derive(Clone)]
pub struct NonV {
    a: i32,
    b: i32,
    f: crate::expr::CompiledFn,
    df: crate::expr::CompiledFn,
    i0: f64,
    lo: f64,
    hi: f64,
    k: i32,
}
impl NonV {
    fn ig(&self, x: &[f64]) -> f64 {
        self.hi.min(self.lo.max(x[self.k as usize]))
    }
}
impl Inst for NonV {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { nl: true, init_g: true, init_b: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.b]]
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        m.v(self.k, self.a, self.b, 1.0);
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, _h: f64, _be: bool, _i: bool, _e: &mut E) {
        let i = self.ig(x);
        let d = self.df.call(&[i]);
        m.a(self.k, self.k, -d);
        b[self.k as usize] += self.f.call(&[i]) - d * i;
    }
    fn stamp_init_g(&self, m: &mut Mat, _r: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        m.v(self.k, self.a, self.b, 1.0);
        m.a(self.k, self.k, -self.df.call(&[self.i0]));
    }
    fn stamp_init_b(&self, b: &mut [f64], _r: bool, _e: &E) {
        b[self.k as usize] += self.f.call(&[self.i0]) - self.df.call(&[self.i0]) * self.i0;
    }
}
pub fn noni(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (a, b) = (ctx.p(0)?, ctx.p(1)?);
    Ok(vec![Box::new(NonI { a, b, f: ctx.func("f", &["v"])?, df: ctx.func("df", &["v"])?, v0: ctx.numd("x0", 0.0)?, lo: ctx.numd("lo", -1e30)?, hi: ctx.numd("hi", 1e30)? })])
}
pub fn nonv(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (a, b) = (ctx.p(0)?, ctx.p(1)?);
    let (f, df, i0, lo, hi) = (ctx.func("f", &["i"])?, ctx.func("df", &["i"])?, ctx.numd("x0", 0.0)?, ctx.numd("lo", -1e30)?, ctx.numd("hi", 1e30)?);
    let k = ctx.b.var();
    Ok(vec![Box::new(NonV { a, b, f, df, i0, lo, hi, k })])
}

/* ------------------------------------------------------------------ Three-state (linear-region) devices */
/// MOSFET channel: current from X to Y = gm*(pol*(vg - vS) - vth) in the active region
#[derive(Clone)]
pub struct MosCh {
    x: i32,
    y: i32,
    gn: i32,
    s0: i32,
    pol: f64,
    vth: f64,
    gm: f64,
    ron: f64,
    so: usize,
}
impl MosCh {
    #[allow(clippy::too_many_arguments)]
    pub fn new(bd: &mut Builder, x: i32, y: i32, gn: i32, s0: i32, pol: f64, vth: f64, gm: f64, ron: f64) -> Self {
        MosCh { x, y, gn, s0, pol, vth, gm, ron, so: bd.req_p(1) }
    }
}
impl Inst for MosCh {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { sw: true, ..cp() }
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.x, self.y]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = 0.0;
        Ok(())
    }
    fn state(&self, e: &E) -> i32 {
        e.ps[self.so] as i32
    }
    fn set_state(&mut self, v: i32, e: &mut E) {
        e.ps[self.so] = v as f64;
    }
    fn skey(&self, e: &E) -> u32 {
        e.ps[self.so] as u32
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, e: &E) {
        let st = e.ps[self.so] as i32;
        if st == 2 {
            m.g(self.x, self.y, 1.0 / self.ron);
            return;
        }
        m.g(self.x, self.y, 1.0 / ROFF);
        if st == 1 {
            let k = self.gm * self.pol;
            m.a(self.x, self.gn, k);
            m.a(self.x, self.s0, -k);
            m.a(self.y, self.gn, -k);
            m.a(self.y, self.s0, k);
        }
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        if e.ps[self.so] as i32 == 1 {
            let i = self.gm * self.vth;
            ab(b, self.x, i);
            ab(b, self.y, -i);
        }
    }
    fn desired(&self, x: &[f64], _t: f64, _e: &E) -> i32 {
        let vov = self.pol * (vx(x, self.gn) - vx(x, self.s0)) - self.vth;
        if vov <= 0.0 {
            return 0;
        }
        let isat = self.gm * vov;
        let vxy = vx(x, self.x) - vx(x, self.y);
        if vxy >= isat * self.ron {
            1
        } else {
            2
        }
    }
}

/// Linear BJT: states BE (0/1) x CE (0 off, 1 active, 2 sat) encoded st = be*3+ce
#[derive(Clone)]
pub struct Bjt3 {
    pn: i32,
    qn: i32,
    x: i32,
    y: i32,
    beta: f64,
    vr: f64,
    vsat: f64,
    roff: f64,
    kb: i32,
    so: usize,
}
impl Bjt3 {
    #[allow(clippy::too_many_arguments)]
    pub fn new(bd: &mut Builder, pn: i32, qn: i32, x: i32, y: i32, beta: f64, vr: f64, vsat: f64, roff: f64) -> Self {
        let kb = bd.var();
        let so = bd.req_p(1);
        Bjt3 { pn, qn, x, y, beta, vr, vsat, roff: if roff != 0.0 && !roff.is_nan() { roff } else { ROFF }, kb, so }
    }
}
impl Inst for Bjt3 {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { sw: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.kb, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.pn, self.qn, self.x, self.y]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = 0.0;
        Ok(())
    }
    fn state(&self, e: &E) -> i32 {
        e.ps[self.so] as i32
    }
    fn set_state(&mut self, v: i32, e: &mut E) {
        e.ps[self.so] = v as f64;
    }
    fn skey(&self, e: &E) -> u32 {
        65 + e.ps[self.so] as u32
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, e: &E) {
        let st = e.ps[self.so] as i32;
        let (be, ce) = (st / 3, st % 3);
        let kb = self.kb;
        if be != 0 {
            m.br(self.pn, self.qn, kb);
            m.v(kb, self.pn, self.qn, 1.0);
            m.a(kb, kb, -RON);
        } else {
            m.a(kb, kb, 1.0);
        }
        m.g(self.x, self.y, if ce == 2 { 1.0 / RON } else { 1.0 / self.roff });
        if ce == 1 {
            m.a(self.x, kb, self.beta);
            m.a(self.y, kb, -self.beta);
        }
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _bee: bool, e: &E) {
        let st = e.ps[self.so] as i32;
        let (be, ce) = (st / 3, st % 3);
        if be != 0 {
            b[self.kb as usize] += self.vr;
        }
        if ce == 2 {
            ai(b, self.x, self.y, -self.vsat / RON);
        }
    }
    fn desired(&self, x: &[f64], _t: f64, e: &E) -> i32 {
        let st = e.ps[self.so] as i32;
        let (mut be, mut ce) = (st / 3, st % 3);
        let vbe = vx(x, self.pn) - vx(x, self.qn);
        let ib = if be != 0 { x[self.kb as usize] } else { 0.0 };
        if be == 0 {
            if vbe > self.vr + EPSV {
                be = 1;
            }
        } else if ib < -EPSI {
            be = 0;
        }
        if be == 0 {
            return 0;
        }
        let vce = vx(x, self.x) - vx(x, self.y);
        let ic = if ce == 2 {
            (vce - self.vsat) / RON
        } else if ce == 1 {
            self.beta * ib
        } else {
            vce / self.roff
        };
        if ce == 0 {
            ce = 1;
        } else if ce == 1 && vce < self.vsat {
            ce = 2;
        } else if ce == 2 && ic > self.beta * ib.max(0.0) {
            ce = 1;
        }
        be * 3 + ce
    }
}

/* ------------------------------------------------------------------ Op-amp (PSIM model: A=1e5, Ro=80 Ω, output clamped) */
#[derive(Clone)]
pub struct OpAmp {
    vn: i32,
    vp: i32,
    out: i32,
    gnd: i32,
    a: f64,
    ro: f64,
    vsp: f64,
    vsn: f64,
    vi: i32,
    k: i32,
    so: usize,
}
impl OpAmp {
    #[allow(clippy::too_many_arguments)]
    pub fn new(bd: &mut Builder, vn: i32, vp: i32, out: i32, gnd: i32, a: f64, ro: f64, vsp: f64, vsn: f64) -> Self {
        let vi = bd.node();
        let k = bd.var();
        let so = bd.req_p(1);
        OpAmp { vn, vp, out, gnd, a, ro, vsp, vsn, vi, k, so }
    }
}
impl Inst for OpAmp {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { sw: true, ac: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.vi, self.gnd, self.out]]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        e.ps[self.so] = 0.0;
        Ok(())
    }
    fn state(&self, e: &E) -> i32 {
        e.ps[self.so] as i32
    }
    fn set_state(&mut self, v: i32, e: &mut E) {
        e.ps[self.so] = v as f64;
    }
    fn skey(&self, e: &E) -> u32 {
        e.ps[self.so] as u32
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, e: &E) {
        let k = self.k;
        m.br(self.vi, self.gnd, k);
        m.v(k, self.vi, self.gnd, 1.0);
        if e.ps[self.so] as i32 == 0 {
            m.a(k, self.vp, -self.a);
            m.a(k, self.vn, self.a);
        }
        m.g(self.vi, self.out, 1.0 / self.ro);
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        match e.ps[self.so] as i32 {
            1 => b[self.k as usize] += self.vsp,
            2 => b[self.k as usize] += self.vsn,
            _ => {}
        }
    }
    fn desired(&self, x: &[f64], _t: f64, e: &E) -> i32 {
        let st = e.ps[self.so] as i32;
        let u = self.a * (vx(x, self.vp) - vx(x, self.vn));
        let vo = vx(x, self.vi) - vx(x, self.gnd);
        match st {
            0 => {
                if vo > self.vsp + 1e-9 {
                    1
                } else if vo < self.vsn - 1e-9 {
                    2
                } else {
                    0
                }
            }
            1 => {
                if u < self.vsp {
                    0
                } else {
                    1
                }
            }
            _ => {
                if u > self.vsn {
                    0
                } else {
                    2
                }
            }
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let k = self.k;
        c.a(self.vi, k, 1.0, 0.0);
        c.a(self.gnd, k, -1.0, 0.0);
        c.a(k, self.vi, 1.0, 0.0);
        c.a(k, self.gnd, -1.0, 0.0);
        c.a(k, self.vp, -self.a, 0.0);
        c.a(k, self.vn, self.a, 0.0);
        c.g(self.vi, self.out, 1.0 / self.ro, 0.0);
        Ok(())
    }
}

/// dv/dt block in the power circuit: pins [in, out]
#[derive(Clone)]
pub struct Dvdt {
    i: i32,
    o: i32,
    k: i32,
    so: usize,
}
impl Inst for Dvdt {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { dyn_: true, ac: true, init_g: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.o, -1]]
    }
    fn stamp_g(&self, m: &mut Mat, h: f64, _be: bool, _e: &E) {
        m.br(self.o, -1, self.k);
        m.a(self.k, self.o, 1.0);
        m.a(self.k, self.i, -1.0 / h);
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, h: f64, _be: bool, e: &E) {
        b[self.k as usize] += -e.ps[self.so] / h;
    }
    fn stamp_init_g(&self, m: &mut Mat, _r: bool, _e: &E) {
        m.br(self.o, -1, self.k);
        m.a(self.k, self.o, 1.0);
    }
    fn accept_init(&mut self, x: &[f64], e: &mut E) {
        e.ps[self.so] = vx(x, self.i);
    }
    fn accept(&mut self, x: &[f64], _t: f64, _h: f64, _be: bool, e: &mut E) {
        e.ps[self.so] = vx(x, self.i);
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let w = c.w;
        c.a(self.o, self.k, 1.0, 0.0);
        c.a(self.k, self.o, 1.0, 0.0);
        c.a(self.k, self.i, 0.0, -w);
        Ok(())
    }
}
pub fn dvdt(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (i, o) = (ctx.p(0)?, ctx.p(1)?);
    let k = ctx.b.var();
    let so = ctx.b.req_p(1);
    Ok(vec![Box::new(Dvdt { i, o, k, so })])
}

/* ------------------------------------------------------------------ Controlled sources */
/// Linear VCVS / VCCS with differential input (power or control)
#[derive(Clone)]
pub struct VCtl {
    op: i32,
    on: i32,
    ip: InD,
    inn: InD,
    gain: f64,
    is_i: bool,
    ctl: bool,
    k: i32,
}
impl VCtl {
    fn pn(d: &InD) -> i32 {
        d.p().unwrap_or(-1)
    }
}
impl Inst for VCtl {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        if self.is_i {
            vec![]
        } else {
            vec![vec![self.op, self.on]]
        }
    }
    fn cins(&self) -> Vec<i32> {
        let mut v = vec![];
        for d in [self.ip, self.inn] {
            if let InD::C(c) = d {
                if c > 0 {
                    v.push(c);
                }
            }
        }
        v
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        let g = self.gain;
        if !self.is_i {
            m.br(self.op, self.on, self.k);
            m.v(self.k, self.op, self.on, 1.0);
            if !self.ctl {
                m.a(self.k, Self::pn(&self.ip), -g);
                m.a(self.k, Self::pn(&self.inn), g);
            }
        } else if !self.ctl {
            let (a, b) = (Self::pn(&self.ip), Self::pn(&self.inn));
            m.a(self.op, a, -g);
            m.a(self.op, b, g);
            m.a(self.on, a, g);
            m.a(self.on, b, -g);
        }
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        if !self.ctl {
            return;
        }
        let v = self.gain * (self.ip.val(e, &e.x_p) - self.inn.val(e, &e.x_p));
        if self.is_i {
            ab(b, self.op, v);
            ab(b, self.on, -v);
        } else {
            b[self.k as usize] += v;
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let g = self.gain;
        let col = |d: &InD, c: &AcCtx| match d {
            InD::C(x) => c.ci(*x),
            InD::P(p) => *p,
            InD::G => -1,
        };
        let (a, b) = (col(&self.ip, c), col(&self.inn, c));
        if !self.is_i {
            c.a(self.op, self.k, 1.0, 0.0);
            c.a(self.on, self.k, -1.0, 0.0);
            c.a(self.k, self.op, 1.0, 0.0);
            c.a(self.k, self.on, -1.0, 0.0);
            c.a(self.k, a, -g, 0.0);
            c.a(self.k, b, g, 0.0);
        } else {
            c.a(self.op, a, -g, 0.0);
            c.a(self.op, b, g, 0.0);
            c.a(self.on, a, g, 0.0);
            c.a(self.on, b, -g, 0.0);
        }
        Ok(())
    }
}
pub fn vctl(ctx: &mut ElCtx, is_i: bool) -> R<Vec<Box<dyn Inst>>> {
    let (op, on) = (ctx.p(0)?, ctx.p(1)?);
    let (ip, inn) = (ctx.in_desc(2)?, ctx.in_desc(3)?);
    let gain = ctx.numd("k", 1.0)?;
    let ctl = ip.c().is_some() || inn.c().is_some();
    let pw = ip.p().is_some() || inn.p().is_some();
    if ctl && pw {
        return Err(ctx.err("控制輸入的兩端必須同在功率電路或同在控制電路"));
    }
    let k = if is_i { -1 } else { ctx.b.var() };
    Ok(vec![Box::new(VCtl { op, on, ip, inn, gain, is_i, ctl, k })])
}

/// Current-controlled (sense current flows through pins 2 -> 3)
#[derive(Clone)]
pub struct CCtl {
    op: i32,
    on: i32,
    cp: i32,
    cn: i32,
    gain: f64,
    is_i: bool,
    ks: i32,
    ko: i32,
}
impl Inst for CCtl {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.ks, n);
        fix(&mut self.ko, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        if self.is_i {
            vec![vec![self.cp, self.cn]]
        } else {
            vec![vec![self.op, self.on], vec![self.cp, self.cn]]
        }
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        let ks = self.ks;
        m.br(self.cp, self.cn, ks);
        m.v(ks, self.cp, self.cn, 1.0);
        if !self.is_i {
            let ko = self.ko;
            m.br(self.op, self.on, ko);
            m.v(ko, self.op, self.on, 1.0);
            m.a(ko, ks, -self.gain);
        } else {
            m.a(self.op, ks, -self.gain);
            m.a(self.on, ks, self.gain);
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let ks = self.ks;
        c.a(self.cp, ks, 1.0, 0.0);
        c.a(self.cn, ks, -1.0, 0.0);
        c.a(ks, self.cp, 1.0, 0.0);
        c.a(ks, self.cn, -1.0, 0.0);
        if !self.is_i {
            let ko = self.ko;
            c.a(self.op, ko, 1.0, 0.0);
            c.a(self.on, ko, -1.0, 0.0);
            c.a(ko, self.op, 1.0, 0.0);
            c.a(ko, self.on, -1.0, 0.0);
            c.a(ko, ks, -self.gain, 0.0);
        } else {
            c.a(self.op, ks, -self.gain, 0.0);
            c.a(self.on, ks, self.gain, 0.0);
        }
        Ok(())
    }
}
pub fn cctl(ctx: &mut ElCtx, is_i: bool) -> R<Vec<Box<dyn Inst>>> {
    let (op, on, cp_, cn) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?, ctx.p(3)?);
    let gain = ctx.numd("k", 1.0)?;
    let ks = ctx.b.var();
    let ko = if is_i { -1 } else { ctx.b.var() };
    Ok(vec![Box::new(CCtl { op, on, cp: cp_, cn, gain, is_i, ks, ko })])
}

/// Current-controlled by the current of an RLC branch connected across pins 2,3
#[derive(Clone)]
pub struct CCtlX {
    op: i32,
    on: i32,
    n2: i64,
    n3: i64,
    gain: f64,
    is_i: bool,
    ko: i32,
    ref_k: i32,
    sg: f64,
    target: Option<usize>,
}
impl Inst for CCtlX {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.ko, n);
    }
    fn link_rlc(&self) -> Option<(i64, i64)> {
        Some((self.n2, self.n3))
    }
    fn set_link(&mut self, target: usize, sg: f64) {
        self.target = Some(target);
        self.sg = sg;
    }
    fn link_target(&self) -> Option<usize> {
        self.target
    }
    fn set_ref_k(&mut self, k: i32) {
        self.ref_k = k;
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        if self.is_i {
            vec![]
        } else {
            vec![vec![self.op, self.on]]
        }
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        let (ks, g) = (self.ref_k, self.gain * self.sg);
        if !self.is_i {
            let ko = self.ko;
            m.br(self.op, self.on, ko);
            m.v(ko, self.op, self.on, 1.0);
            m.a(ko, ks, -g);
        } else {
            m.a(self.op, ks, -g);
            m.a(self.on, ks, g);
        }
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let (ks, g) = (self.ref_k, self.gain * self.sg);
        if !self.is_i {
            let ko = self.ko;
            c.a(self.op, ko, 1.0, 0.0);
            c.a(self.on, ko, -1.0, 0.0);
            c.a(ko, self.op, 1.0, 0.0);
            c.a(ko, self.on, -1.0, 0.0);
            c.a(ko, ks, -g, 0.0);
        } else {
            c.a(self.op, ks, -g, 0.0);
            c.a(self.on, ks, g, 0.0);
        }
        Ok(())
    }
}
pub fn cctlx(ctx: &mut ElCtx, is_i: bool) -> R<Vec<Box<dyn Inst>>> {
    let (op, on) = (ctx.p(0)?, ctx.p(1)?);
    ctx.p(2)?;
    ctx.p(3)?;
    let gain = ctx.numd("k", 1.0)?;
    let ko = if is_i { -1 } else { ctx.b.var() };
    let (n2, n3) = (ctx.net_pins.get(2).copied().unwrap_or(-1), ctx.net_pins.get(3).copied().unwrap_or(-1));
    Ok(vec![Box::new(CCtlX { op, on, n2, n3, gain, is_i, ko, ref_k: -1, sg: 1.0, target: None })])
}

/// Nonlinear / variable-gain voltage-controlled sources with single-ended inputs
#[derive(Clone, Copy)]
enum NonK {
    M(f64),
    D(f64),
    Q(f64),
    P(f64, f64, f64),
    V(f64),
}
#[inline]
fn safe(v: f64, e: f64) -> f64 {
    if v.abs() < e {
        if v < 0.0 {
            -e
        } else {
            e
        }
    } else {
        v
    }
}
fn math_sign(a: f64) -> f64 {
    if a.is_nan() {
        f64::NAN
    } else if a > 0.0 {
        1.0
    } else if a < 0.0 {
        -1.0
    } else {
        a
    }
}
impl NonK {
    fn f(&self, v: &[f64]) -> f64 {
        let a = v[0];
        let b = v.get(1).copied().unwrap_or(f64::NAN);
        match *self {
            NonK::M(k) => k * a * b,
            NonK::D(k) => k * a / safe(b, 1e-30),
            NonK::Q(k) => k * a.max(0.0).sqrt(),
            NonK::P(k, k1, k2) => math_sign(a) * k * crate::expr::js_pow(k1 * a.abs(), k2),
            NonK::V(k) => k * b * a,
        }
    }
    fn df(&self, v: &[f64]) -> [f64; 2] {
        match *self {
            NonK::M(k) => [k * v[1], k * v[0]],
            NonK::D(k) => {
                let s = safe(v[1], 1e-30);
                [k / s, -k * v[0] / (s * s)]
            }
            NonK::Q(k) => [k * 0.5 / v[0].max(1e-24).sqrt(), 0.0],
            NonK::P(k, k1, k2) => [k * k2 * k1 * crate::expr::js_pow((k1 * v[0].abs()).max(1e-12), k2 - 1.0), 0.0],
            NonK::V(_) => [0.0, 0.0],
        }
    }
}
#[derive(Clone)]
pub struct VNon {
    op: i32,
    on: i32,
    ins: Vec<InD>,
    kind: NonK,
    is_i: bool,
    var_gain: bool,
    pw: bool,
    k: i32,
}
impl VNon {
    fn vals(&self, e: &E, x: &[f64]) -> Vec<f64> {
        self.ins.iter().map(|d| d.val(e, x)).collect()
    }
    fn stamp_vg(&self, m: &mut Mat, e: &E) {
        let g = self.kind.f(&[1.0, self.ins[1].val(e, &e.x_p)]);
        if let InD::P(p) = self.ins[0] {
            if !self.is_i {
                m.a(self.k, p, -g);
            } else {
                m.a(self.op, p, -g);
                m.a(self.on, p, g);
            }
        }
    }
}
impl Inst for VNon {
    clone_box!();
    fn caps(&self) -> Caps {
        let nl = !self.var_gain && self.pw;
        let dy = self.var_gain && self.pw;
        // the t = 0 operating point solves the nonlinear relation too (otherwise the output starts at 0)
        Caps { nl, nl_init: nl, dyn_: dy, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        if self.is_i {
            vec![]
        } else {
            vec![vec![self.op, self.on]]
        }
    }
    fn cins(&self) -> Vec<i32> {
        self.ins.iter().filter_map(|d| d.c()).filter(|&c| c > 0).collect()
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, e: &E) {
        if !self.is_i {
            m.br(self.op, self.on, self.k);
            m.v(self.k, self.op, self.on, 1.0);
        }
        if self.var_gain && self.pw {
            self.stamp_vg(m, e);
        }
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        if !self.var_gain && self.pw {
            return;
        }
        let v = if self.var_gain {
            if self.pw && self.ins[0].p().is_some() {
                return;
            }
            self.kind.f(&[self.ins[0].val(e, &e.x_p), self.ins[1].val(e, &e.x_p)])
        } else {
            self.kind.f(&self.vals(e, &e.x_p))
        };
        if self.is_i {
            ab(b, self.op, v);
            ab(b, self.on, -v);
        } else {
            b[self.k as usize] += v;
        }
    }
    fn stamp_nl(&mut self, m: &mut Mat, b: &mut [f64], x: &[f64], _t: f64, _h: f64, _be: bool, init: bool, e: &mut E) {
        let v = self.vals(e, x);
        // t = 0 Newton starts from x = 0, where 1/vB and 1/√vA are unbounded: linearize there at a bounded
        // point instead (only the iteration path changes; the converged point still satisfies f exactly)
        let d = if init {
            let vb: Vec<f64> = v.iter().enumerate().map(|(j, &u)| match self.kind {
                NonK::D(_) if j == 1 => safe(u, 1e-3),
                NonK::Q(_) | NonK::P(..) if j == 0 => u.abs().max(1e-3) * if u < 0.0 { -1.0 } else { 1.0 },
                _ => u,
            }).collect();
            self.kind.df(&vb)
        } else {
            self.kind.df(&v)
        };
        let mut r = self.kind.f(&v);
        for (j, dd) in self.ins.iter().enumerate() {
            if let InD::P(p) = dd {
                if !self.is_i {
                    m.a(self.k, *p, -d[j]);
                } else {
                    m.a(self.op, *p, -d[j]);
                    m.a(self.on, *p, d[j]);
                }
                r -= d[j] * v[j];
            }
        }
        if self.is_i {
            ab(b, self.op, r);
            ab(b, self.on, -r);
        } else {
            b[self.k as usize] += r;
        }
    }
}
pub fn vnon(ctx: &mut ElCtx, kind: char, is_i: bool) -> R<Vec<Box<dyn Inst>>> {
    let (op, on) = (ctx.p(0)?, ctx.p(1)?);
    let k = ctx.numd("k", 1.0)?;
    let (ins, nk, vg) = match kind {
        'M' => (vec![ctx.in_desc(2)?, ctx.in_desc(3)?], NonK::M(k), false),
        'D' => (vec![ctx.in_desc(2)?, ctx.in_desc(3)?], NonK::D(k), false),
        'Q' => (vec![ctx.in_desc(2)?], NonK::Q(k), false),
        'P' => {
            let (k1, k2) = (ctx.numd("k1", 1.0)?, ctx.numd("k2", 1.0)?);
            (vec![ctx.in_desc(2)?], NonK::P(k, k1, k2), false)
        }
        _ => (vec![ctx.in_desc(2)?, ctx.in_desc(3)?], NonK::V(k), true),
    };
    let pw = ins.iter().any(|d| d.p().is_some());
    let kv = if is_i { -1 } else { ctx.b.var() };
    Ok(vec![Box::new(VNon { op, on, ins, kind: nk, is_i, var_gain: vg, pw, k: kv })])
}

/// Control-to-power interface: pins [in(control), out(power)]
#[derive(Clone)]
pub struct CtoP {
    cn: i32,
    o: i32,
    k: i32,
}
impl Inst for CtoP {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn cins(&self) -> Vec<i32> {
        vec![self.cn]
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.o, -1]]
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        m.br(self.o, -1, self.k);
        m.a(self.k, self.o, 1.0);
    }
    fn stamp_b(&self, b: &mut [f64], _t: f64, _h: f64, _be: bool, e: &E) {
        b[self.k as usize] += e.cv[self.cn as usize];
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        c.a(self.o, self.k, 1.0, 0.0);
        c.a(self.k, self.o, 1.0, 0.0);
        let r = c.ci(self.cn);
        c.a(self.k, r, -1.0, 0.0);
        Ok(())
    }
}
pub fn ctop(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (cn, o) = (ctx.c(0)?, ctx.p(1)?);
    let k = ctx.b.var();
    Ok(vec![Box::new(CtoP { cn, o, k })])
}

/* ------------------------------------------------------------------ Sensors and probes */
/// zero-volt ammeter branch (current from a to b); optionally also a current sensor or current probe
#[derive(Clone)]
pub struct Amm {
    pub a: i32,
    pub b: i32,
    pub k: i32,
    sensor: Option<(i32, f64)>,
    probe: Option<String>,
}
impl Amm {
    pub fn new(bd: &mut Builder, a: i32, b: i32) -> Self {
        Amm { a, b, k: bd.var(), sensor: None, probe: None }
    }
}
impl Inst for Amm {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac: true, c: self.sensor.is_some(), probe: self.probe.is_some(), ..cp() }
    }
    fn fix_vars(&mut self, n: i32) {
        fix(&mut self.k, n);
    }
    fn conn(&self) -> Vec<Vec<i32>> {
        vec![vec![self.a, self.b]]
    }
    fn outs(&self) -> Vec<i32> {
        self.sensor.map(|s| vec![s.0]).unwrap_or_default()
    }
    fn stamp_g(&self, m: &mut Mat, _h: f64, _be: bool, _e: &E) {
        m.br(self.a, self.b, self.k);
        m.v(self.k, self.a, self.b, 1.0);
    }
    fn eval(&mut self, _t: f64, _h: f64, e: &mut E, x: &[f64]) -> R<()> {
        if let Some((o, kg)) = self.sensor {
            e.cv[o as usize] = kg * x[self.k as usize];
        }
        Ok(())
    }
    fn pname(&self) -> String {
        self.probe.clone().unwrap_or_default()
    }
    fn value(&self, _e: &E, x: &[f64]) -> f64 {
        x[self.k as usize]
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        c.a(self.a, self.k, 1.0, 0.0);
        c.a(self.b, self.k, -1.0, 0.0);
        c.a(self.k, self.a, 1.0, 0.0);
        c.a(self.k, self.b, -1.0, 0.0);
        if let Some((o, kg)) = self.sensor {
            let r = c.ci(o);
            c.a(r, r, 1.0, 0.0);
            c.a(r, self.k, -kg, 0.0);
        }
        Ok(())
    }
}
pub fn isen(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (a, b, o) = (ctx.p(0)?, ctx.p(1)?, ctx.c(2)?);
    let k = ctx.numd("k", 1.0)?;
    let mut m = Amm::new(ctx.b, a, b);
    m.sensor = Some((o, k));
    Ok(vec![Box::new(m)])
}
pub fn iprobe(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let (a, b) = (ctx.p(0)?, ctx.p(1)?);
    let mut m = Amm::new(ctx.b, a, b);
    m.probe = Some(ctx.name.clone());
    Ok(vec![Box::new(m)])
}

/// voltage sensor: pins [+, -, out]
#[derive(Clone)]
pub struct VSen {
    p: i32,
    n: i32,
    o: i32,
    k: f64,
}
impl Inst for VSen {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { c: true, ac: true, ..Default::default() }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn eval(&mut self, _t: f64, _h: f64, e: &mut E, x: &[f64]) -> R<()> {
        e.cv[self.o as usize] = self.k * (vx(x, self.p) - vx(x, self.n));
        Ok(())
    }
    fn stamp_ac(&self, c: &mut AcCtx, _s: bool, _e: &E) -> R<()> {
        let r = c.ci(self.o);
        c.a(r, r, 1.0, 0.0);
        c.a(r, self.p, -self.k, 0.0);
        c.a(r, self.n, self.k, 0.0);
        Ok(())
    }
}
pub fn vsen(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    Ok(vec![Box::new(VSen { p: ctx.p(0)?, n: ctx.p(1)?, o: ctx.c(2)?, k: ctx.numd("k", 1.0)? })])
}

#[derive(Clone)]
pub struct VProbe {
    name: String,
    d1: InD,
    d2: InD,
}
impl Inst for VProbe {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { probe: true, ..Default::default() }
    }
    fn cins(&self) -> Vec<i32> {
        [self.d1, self.d2].iter().filter_map(|d| d.c()).filter(|&c| c > 0).collect()
    }
    fn pname(&self) -> String {
        self.name.clone()
    }
    fn value(&self, e: &E, x: &[f64]) -> f64 {
        self.d1.val(e, x) - self.d2.val(e, x)
    }
}
pub fn vprobe(ctx: &mut ElCtx, two: bool) -> R<Vec<Box<dyn Inst>>> {
    let a = ctx.in_desc(0)?;
    let b = if two { ctx.in_desc(1)? } else { InD::G };
    if two && ((a.c().is_some() && b.p().is_some()) || (a.p().is_some() && b.c().is_some())) {
        return Err(ctx.err("兩個量測端必須同在功率電路或同在控制電路"));
    }
    Ok(vec![Box::new(VProbe { name: ctx.name.clone(), d1: a, d2: b })])
}

/// Meters: measured quantity -> filter -> output, implemented as control blocks
#[derive(Clone, Copy)]
pub enum MeterSrc {
    V(i32, i32),
    I(i32),
    P(i32, i32, i32),
}
#[derive(Clone)]
pub struct Meter {
    name: String,
    src: MeterSrc,
    ac: bool,
    f: f64,
    fc: f64,
    o: i32,
    so: usize,
    tau: f64,
    tp: f64,
    bt: Vec<f64>,
    bi: Vec<f64>,
    /// running ∫u dt (AC meters: the DC part over one period is removed exactly)
    bu: Vec<f64>,
    p: Option<(f64, f64, f64, f64, f64)>,
}
impl Meter {
    fn get_v(&self, x: &[f64]) -> f64 {
        match self.src {
            MeterSrc::V(a, b) => vx(x, a) - vx(x, b),
            MeterSrc::I(k) => x[k as usize],
            MeterSrc::P(k, p0, pn) => x[k as usize] * (vx(x, p0) - vx(x, pn)),
        }
    }
}
impl Inst for Meter {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { c: true, probe: true, ac: true, ..Default::default() }
    }
    fn fix_vars(&mut self, n: i32) {
        match &mut self.src {
            MeterSrc::I(k) => fix(k, n),
            MeterSrc::P(k, _, _) => fix(k, n),
            _ => {}
        }
    }
    fn outs(&self) -> Vec<i32> {
        vec![self.o]
    }
    fn init(&mut self, e: &mut E) -> R<()> {
        for i in 0..4 {
            e.cs[self.so + i] = 0.0;
        }
        self.tau = 1.0 / (2.0 * std::f64::consts::PI * self.fc);
        if self.ac {
            self.tp = 1.0 / self.f;
            self.bt = vec![0.0];
            self.bi = vec![0.0];
            self.bu = vec![0.0];
        }
        Ok(())
    }
    fn eval(&mut self, t: f64, h: f64, e: &mut E, x: &[f64]) -> R<()> {
        let u = self.get_v(x);
        let o = self.o as usize;
        if h == 0.0 && t == 0.0 {
            let y = if self.ac { 0.0 } else { u };
            self.p = Some((u, y, 0.0, f64::NAN, f64::NAN));
            e.cv[o] = y;
            return Ok(());
        }
        let s = &e.cs[self.so..self.so + 4];
        let a = h / (2.0 * self.tau);
        let (y, hp) = if h == 0.0 { (s[0], s[1]) } else { ((s[0] * (1.0 - a) + a * (u + s[2])) / (1.0 + a), ((1.0 - a) * s[1] + (u - s[2])) / (1.0 + a)) };
        if !self.ac {
            self.p = Some((u, y, hp, f64::NAN, f64::NAN));
            e.cv[o] = y;
        } else {
            // RMS of the AC part over one operating period: √(⟨u²⟩ − ⟨u⟩²). Removing the period mean takes
            // the DC out exactly (a high-pass filter would also scale the fundamental by f/√(f² + fc²)).
            let n = self.bt.len();
            let up = s[2];
            let it = self.bi[n - 1] + 0.5 * h * (u * u + up * up);
            let iu = self.bu[n - 1] + 0.5 * h * (u + up);
            let t0 = t - self.tp;
            let (i0, u0) = if t0 <= 0.0 {
                (0.0, 0.0)
            } else {
                let mut j = n - 1;
                while j > 0 && self.bt[j] > t0 {
                    j -= 1;
                }
                let ta = self.bt[j];
                let tb = if j + 1 < n { self.bt[j + 1] } else { t };
                let f = if tb > ta { (t0 - ta) / (tb - ta) } else { 0.0 };
                let (ia, ib) = (self.bi[j], if j + 1 < n { self.bi[j + 1] } else { it });
                let (ua, ub) = (self.bu[j], if j + 1 < n { self.bu[j + 1] } else { iu });
                (ia + (ib - ia) * f, ua + (ub - ua) * f)
            };
            let w = self.tp.min(t.max(1e-300));
            let m2 = (it - i0) / w;
            let m1 = (iu - u0) / w;
            self.p = Some((u, y, hp, it, iu));
            e.cv[o] = (m2 - m1 * m1).max(0.0).sqrt();
        }
        Ok(())
    }
    fn commit(&mut self, t: f64, h: f64, e: &mut E) {
        let (u, y, hp, it, iu) = match self.p {
            Some(p) => p,
            None => return,
        };
        let s = &mut e.cs[self.so..];
        s[0] = y;
        s[1] = hp;
        s[2] = u;
        if self.ac && h > 0.0 {
            self.bt.push(t);
            self.bi.push(it);
            self.bu.push(iu);
            let keep = t - self.tp * 1.5;
            let mut cut = 0;
            while cut + 2 < self.bt.len() && self.bt[cut + 1] < keep {
                cut += 1;
            }
            if cut > 1024 {
                self.bt.drain(0..cut);
                self.bi.drain(0..cut);
                self.bu.drain(0..cut);
            }
        }
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
pub fn meter(ctx: &mut ElCtx, typ: &str) -> R<Vec<Box<dyn Inst>>> {
    let mut out: Vec<Box<dyn Inst>> = Vec::new();
    let ac = typ == "VACM" || typ == "AACM";
    let src = match typ {
        "VDCM" | "VACM" => MeterSrc::V(ctx.p(0)?, ctx.p(1)?),
        "ADCM" | "AACM" => {
            let (a, b) = (ctx.p(0)?, ctx.p(1)?);
            let am = Amm::new(ctx.b, a, b);
            let k = am.k;
            out.push(Box::new(am));
            MeterSrc::I(k)
        }
        _ => {
            // Wattmeter pins: [I+, I-, V-]; v = v(I+) - v(V-)
            let (a, b) = (ctx.p(0)?, ctx.p(1)?);
            let am = Amm::new(ctx.b, a, b);
            let k = am.k;
            out.push(Box::new(am));
            MeterSrc::P(k, ctx.p(0)?, ctx.p(2)?)
        }
    };
    let f = if ac { ctx.pos("f")? } else { 0.0 };
    let fc = ctx.pos("fc")?;
    let o = ctx.b.cnode();
    let so = ctx.b.req_c(4);
    out.push(Box::new(Meter { name: ctx.name.clone(), src, ac, f, fc, o, so, tau: 0.0, tp: 0.0, bt: vec![], bi: vec![], bu: vec![], p: None }));
    Ok(out)
}

/// AC sweep probes
#[derive(Clone)]
pub struct AcProbe {
    name: String,
    d1: InD,
    d2: Option<InD>,
    neg: bool,
}
fn phasor(d: &InD, xr: &[f64], xi: &[f64], n: usize) -> (f64, f64) {
    match *d {
        InD::C(c) => {
            if c > 0 {
                let i = n + c as usize - 1;
                (xr[i], xi[i])
            } else {
                (0.0, 0.0)
            }
        }
        InD::P(p) => {
            if p >= 0 {
                (xr[p as usize], xi[p as usize])
            } else {
                (0.0, 0.0)
            }
        }
        InD::G => (0.0, 0.0),
    }
}
impl Inst for AcProbe {
    clone_box!();
    fn caps(&self) -> Caps {
        Caps { ac_probe: true, ..Default::default() }
    }
    fn cins(&self) -> Vec<i32> {
        let mut v = vec![];
        for d in [Some(self.d1), self.d2].iter().flatten() {
            if let Some(c) = d.c() {
                if c > 0 {
                    v.push(c);
                }
            }
        }
        v
    }
    fn pname(&self) -> String {
        self.name.clone()
    }
    fn ac_vals(&self, e: &E, x: &[f64]) -> (f64, f64) {
        (self.d1.val(e, x), self.d2.map(|d| d.val(e, x)).unwrap_or(0.0))
    }
    fn ac_loop(&self) -> bool {
        self.d2.is_some()
    }
    fn ac_neg(&self) -> bool {
        self.neg
    }
    fn ac_phasor(&self, xr: &[f64], xi: &[f64], n: usize) -> [f64; 4] {
        let a = phasor(&self.d1, xr, xi, n);
        match &self.d2 {
            None => [a.0, a.1, 0.0, 0.0],
            Some(d) => {
                let b = phasor(d, xr, xi, n);
                [a.0, a.1, b.0, b.1]
            }
        }
    }
}
pub fn acprobe(ctx: &mut ElCtx, two: bool) -> R<Vec<Box<dyn Inst>>> {
    let d1 = ctx.in_desc(0)?;
    let d2 = if two { Some(ctx.in_desc(1)?) } else { None };
    let neg = if two { ctx.int("neg", 1.0)? == 1 } else { false };
    Ok(vec![Box::new(AcProbe { name: ctx.name.clone(), d1, d2, neg })])
}

/* ------------------------------------------------------------------ factories */
pub fn cap(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let c = ctx.num("C")?;
    if !(c > 0.0) {
        return Err(ctx.err("電容必須大於 0"));
    }
    let (a, b) = (ctx.p(0)?, ctx.p(1)?);
    let v0 = ctx.numd("v0", 0.0)?;
    Ok(vec![Box::new(Cap::new(ctx.b, a, b, c, v0))])
}
pub fn br_mk(ctx: &mut ElCtx, has_r: bool, has_l: bool, has_c: bool) -> R<Vec<Box<dyn Inst>>> {
    let r = if has_r { ctx.num("R")? } else { 0.0 };
    let l = if has_l { ctx.num("L")? } else { 0.0 };
    let c = if has_c { ctx.num("C")? } else { 0.0 };
    if r < 0.0 || l < 0.0 || c < 0.0 {
        return Err(ctx.err("R、L、C 不可為負值"));
    }
    if r == 0.0 && l == 0.0 && c == 0.0 {
        return Err(ctx.err("R、L、C 不可全部為 0"));
    }
    let (a, b) = (ctx.p(0)?, ctx.p(1)?);
    let i0 = if has_l { ctx.numd("i0", 0.0)? } else { 0.0 };
    let v0 = if has_c { ctx.numd("v0", 0.0)? } else { 0.0 };
    Ok(vec![Box::new(SerBr::new(ctx.b, a, b, r, l, c, i0, v0))])
}
pub fn br3(ctx: &mut ElCtx, has_r: bool, has_l: bool, has_c: bool) -> R<Vec<Box<dyn Inst>>> {
    let r = if has_r { ctx.num("R")? } else { 0.0 };
    let l = if has_l { ctx.num("L")? } else { 0.0 };
    let c = if has_c { ctx.num("C")? } else { 0.0 };
    if r == 0.0 && l == 0.0 && c == 0.0 {
        return Err(ctx.err("R、L、C 不可全部為 0"));
    }
    let mut out: Vec<Box<dyn Inst>> = Vec::new();
    for i in 0..3 {
        let (a, b) = (ctx.p(i)?, ctx.p(i + 3)?);
        if has_r && !has_l && !has_c {
            out.push(Box::new(Res::new(a, b, r)));
        } else {
            out.push(Box::new(SerBr::new(ctx.b, a, b, r, l, c, 0.0, 0.0)));
        }
    }
    Ok(out)
}
pub fn lsat(ctx: &mut ElCtx) -> R<Vec<Box<dyn Inst>>> {
    let lst = ctx.list("tab")?;
    if lst.len() < 2 || lst.len() % 2 != 0 {
        return Err(ctx.err("請以 i1 L1 i2 L2 ... 的成對格式輸入電流與電感"));
    }
    let ii: Vec<f64> = lst.iter().step_by(2).copied().collect();
    let ll: Vec<f64> = lst.iter().skip(1).step_by(2).copied().collect();
    for i in 1..ii.len() {
        if !(ii[i] > ii[i - 1]) {
            return Err(ctx.err("電流必須遞增"));
        }
    }
    if !(ii[0] > 0.0) {
        return Err(ctx.err("第一個電流點必須大於 0"));
    }
    let (a, b) = (ctx.p(0)?, ctx.p(1)?);
    Ok(vec![Box::new(SatL::new(ctx.b, a, b, &ii, &ll))])
}

/// Real transformer: windings (a,b,R,L,N), first is primary; Lm across primary (referred to primary)
pub fn real_xfmr(bd: &mut Builder, wins: &[(i32, i32, f64, f64, f64)], lm: f64) -> Vec<Box<dyn Inst>> {
    let mut out: Vec<Box<dyn Inst>> = Vec::new();
    let n1 = wins[0].4;
    let mut xw = Vec::new();
    for (j, &(a, b, r, l, n)) in wins.iter().enumerate() {
        let r2 = (n / n1) * (n / n1);
        let (rr, lr) = if j == 0 { (r, l) } else { (r * r2, l * r2) };
        let inner = if rr != 0.0 || lr != 0.0 {
            let inner = bd.node();
            out.push(Box::new(SerBr::new(bd, a, inner, rr, lr, 0.0, 0.0, 0.0)));
            inner
        } else {
            a
        };
        xw.push((inner, b, n));
    }
    if lm > 0.0 {
        out.push(Box::new(SerBr::new(bd, xw[0].0, xw[0].1, 0.0, lm, 0.0, 0.0, 0.0)));
    }
    out.push(Box::new(Xfmr::new(bd, xw)));
    out
}

pub fn opamp(ctx: &mut ElCtx, with_gnd: bool) -> R<Vec<Box<dyn Inst>>> {
    let (vn, vp, out) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?);
    let g = if with_gnd { ctx.p(3)? } else { -1 };
    let (a, ro, vsp, vsn) = (ctx.numd("A", 1e5)?, ctx.numd("ro", 80.0)?, ctx.numd("vsp", 15.0)?, ctx.numd("vsn", -15.0)?);
    Ok(vec![Box::new(OpAmp::new(ctx.b, vn, vp, out, g, a, ro, vsp, vsn))])
}
