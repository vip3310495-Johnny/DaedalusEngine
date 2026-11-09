//! Waveforms shared by power and control sources.

use crate::expr::CompiledFn;
use crate::sim::{ElCtx, Inj, R};
use crate::util::js_to_u32;
use std::f64::consts::PI;

const TWO_PI: f64 = 2.0 * PI;

#[inline]
pub fn frac(x: f64) -> f64 {
    x - x.floor()
}

/// next multiple of period T after t of the phase offsets (times o + k*T)
pub fn next_periodic(t: f64, tp: f64, offs: &[f64]) -> f64 {
    let mut best = f64::INFINITY;
    let k0 = (t / tp).floor();
    for kk in 0..2 {
        let k = k0 + kk as f64;
        for &o in offs {
            let tt = k * tp + o;
            if tt > t && tt < best {
                best = tt;
            }
        }
    }
    best
}

#[derive(Clone)]
pub enum WK {
    Dc(f64),
    Sin { vm: f64, w: f64, p: f64, off: f64, ts: f64 },
    Squ { vpp: f64, tp: f64, d: f64, off: f64, td: f64, e: [f64; 2] },
    Tri { vpp: f64, tp: f64, d: f64, off: f64, td: f64, e: [f64; 2], is_j: bool },
    Step { v: f64, ts: f64 },
    Step2 { v1: f64, v2: f64, ts: f64, tr: f64 },
    Pwl { tp: f64, ts: Vec<f64>, vs: Vec<f64>, offs: Vec<f64> },
    Rand { vpp: f64, off: f64, seed: f64, dt: f64 },
    Math { f: CompiledFn, ts: f64 },
}

#[derive(Clone)]
pub struct Wave {
    pub k: WK,
    pub neg: bool,
    pub inj: Option<Inj>,
}

impl Wave {
    pub fn new(k: WK) -> Self {
        Wave { k, neg: false, inj: None }
    }
    /// value at a step end t: exactly at t, except where the waveform jumps at t, where side = −1 picks the
    /// value just before the jump (power sources: the step ending at a breakpoint) and side = +1 the value
    /// just after it (control sources), as one-sided limits. Shifting every evaluation by ±ε/2 instead would put a systematic
    /// ~ε·|dv/dt| error (≈ 1e-9 relative) on every smooth waveform.
    pub fn val_side(&self, t: f64, side: f64, eps: f64) -> f64 {
        if self.jump(t, eps) {
            // one-sided limit by linear extrapolation from two points on that side: exact for piecewise-linear
            // waveforms (steps, squares, saws, PWL), O(ε²) otherwise
            2.0 * self.val(t + side * 0.5 * eps) - self.val(t + side * eps)
        } else {
            self.val(t)
        }
    }
    pub fn val(&self, t: f64) -> f64 {
        let v = self.val0(t);
        if self.neg {
            -v
        } else {
            v
        }
    }
    fn val0(&self, t: f64) -> f64 {
        match &self.k {
            WK::Dc(v) => *v,
            WK::Sin { vm, w, p, off, ts } => {
                if let Some(j) = &self.inj {
                    let u = t - j.t0;
                    let env = if j.tr > 0.0 && u < j.tr {
                        if u <= 0.0 {
                            0.0
                        } else {
                            0.5 * (1.0 - (PI * u / j.tr).cos())
                        }
                    } else {
                        1.0
                    };
                    return off + j.a * env * (TWO_PI * j.f * u).sin();
                }
                if t < *ts {
                    return 0.0;
                }
                vm * (w * t + p).sin() + off
            }
            WK::Squ { vpp, tp, d, off, td, .. } => {
                let ph = frac((t - td) / tp);
                if ph < *d {
                    off + vpp
                } else {
                    *off
                }
            }
            WK::Tri { vpp, tp, d, off, td, .. } => {
                let mut ph = frac((t - td) / tp);
                if ph > 1.0 - 1e-12 {
                    ph = 0.0;
                }
                if *d >= 1.0 {
                    return off + vpp * ph;
                }
                if *d <= 0.0 {
                    return off + vpp * (1.0 - ph);
                }
                if ph < *d {
                    off + vpp * ph / d
                } else {
                    off + vpp * (1.0 - ph) / (1.0 - d)
                }
            }
            WK::Step { v, ts } => {
                if t < *ts {
                    0.0
                } else {
                    *v
                }
            }
            WK::Step2 { v1, v2, ts, tr } => {
                if t < *ts {
                    return *v1;
                }
                if *tr > 0.0 && t < ts + tr {
                    return v1 + (v2 - v1) * (t - ts) / tr;
                }
                *v2
            }
            WK::Pwl { tp, ts, vs, .. } => {
                let u = if *tp > 0.0 { frac(t / tp) * tp } else { t };
                let n = ts.len();
                if u <= ts[0] {
                    return vs[0];
                }
                for i in 1..n {
                    if u < ts[i] {
                        let dt = ts[i] - ts[i - 1];
                        return if dt > 0.0 { vs[i - 1] + (vs[i] - vs[i - 1]) * (u - ts[i - 1]) / dt } else { vs[i] };
                    }
                }
                vs[n - 1]
            }
            WK::Rand { vpp, off, seed, dt } => {
                let k = (t / dt - 1e-6).ceil();
                off + vpp * rand_hash(k, *seed)
            }
            WK::Math { f, ts } => {
                if t < *ts {
                    0.0
                } else {
                    f.call(&[t])
                }
            }
        }
    }
    pub fn next(&self, t: f64) -> f64 {
        match &self.k {
            WK::Sin { ts, .. } => {
                if self.inj.is_none() && *ts > t {
                    *ts
                } else {
                    f64::INFINITY
                }
            }
            WK::Squ { tp, e, .. } | WK::Tri { tp, e, .. } => next_periodic(t, *tp, e),
            WK::Step { ts, .. } | WK::Math { ts, .. } => {
                if *ts > t {
                    *ts
                } else {
                    f64::INFINITY
                }
            }
            WK::Step2 { ts, tr, .. } => {
                if *ts > t {
                    return *ts;
                }
                if *tr > 0.0 && ts + tr > t {
                    return ts + tr;
                }
                f64::INFINITY
            }
            WK::Pwl { tp, ts, offs, .. } => {
                if *tp > 0.0 {
                    return next_periodic(t, *tp, offs);
                }
                for &x in ts {
                    if x > t {
                        return x;
                    }
                }
                f64::INFINITY
            }
            _ => f64::INFINITY,
        }
    }
    pub fn jump(&self, t: f64, eps: f64) -> bool {
        match &self.k {
            WK::Sin { vm, w, p, off, ts } => self.inj.is_none() && *ts > 0.0 && (t - ts).abs() <= eps && (vm * (w * ts + p).sin() + off) != 0.0,
            WK::Squ { tp, e, .. } => {
                let r = frac(t / tp) * tp;
                e.iter().any(|&e| (r - e).abs() <= eps || (r - e - tp).abs() <= eps || (r - e + tp).abs() <= eps)
            }
            WK::Tri { tp, e, is_j, .. } => {
                if !*is_j {
                    return false;
                }
                let r = frac(t / tp) * tp;
                (r - e[0]).abs() <= eps || (r - e[0] - tp).abs() <= eps || r <= eps
            }
            WK::Step { v, ts } => (t - ts).abs() <= eps && *v != 0.0,
            WK::Step2 { v1, v2, ts, tr } => !(*tr > 0.0) && (t - ts).abs() <= eps && v1 != v2,
            WK::Pwl { tp, ts, vs, .. } => {
                let u = if *tp > 0.0 { frac(t / tp) * tp } else { t };
                for i in 1..ts.len() {
                    if ts[i] == ts[i - 1] && (u - ts[i]).abs() <= eps && vs[i] != vs[i - 1] {
                        return true;
                    }
                }
                false
            }
            WK::Math { ts, .. } => *ts > 0.0 && (t - ts).abs() <= eps,
            _ => false,
        }
    }
}

impl Wave {
    /// value jump or slope discontinuity at t: a power-circuit source corner. Inductors driven by a current
    /// source (or capacitors by a voltage source) have an algebraic voltage/current there, and the trapezoidal
    /// rule rings without end unless the next step starts with backward Euler (as SPICE does at breakpoints).
    pub fn corner(&self, t: f64, eps: f64) -> bool {
        if self.jump(t, eps) {
            return true;
        }
        let at = |x: f64| (t - x).abs() <= eps;
        match &self.k {
            WK::Sin { ts, .. } => self.inj.is_none() && *ts > 0.0 && at(*ts),
            WK::Tri { tp, e, is_j, .. } => {
                if *is_j {
                    return false;
                }
                let r = frac(t / tp) * tp;
                e.iter().any(|&x| (r - x).abs() <= eps || (r - x - tp).abs() <= eps || (r - x + tp).abs() <= eps)
            }
            WK::Step2 { v1, v2, ts, tr } => *tr > 0.0 && v1 != v2 && (at(*ts) || at(ts + tr)),
            WK::Pwl { tp, ts, vs, .. } => {
                let u = if *tp > 0.0 { frac(t / tp) * tp } else { t };
                let n = ts.len();
                let slope = |i: usize| -> f64 {
                    // slope of segment i → i+1; constant before the first and after the last point
                    if i + 1 >= n || ts[i + 1] <= ts[i] {
                        0.0
                    } else {
                        (vs[i + 1] - vs[i]) / (ts[i + 1] - ts[i])
                    }
                };
                for i in 0..n {
                    if (u - ts[i]).abs() <= eps || (*tp > 0.0 && (u - ts[i] + tp).abs() <= eps) {
                        let before = if i == 0 { 0.0 } else { slope(i - 1) };
                        if (before - slope(i)).abs() > 1e-12 * (before.abs() + slope(i).abs()) {
                            return true;
                        }
                    }
                }
                false
            }
            _ => false,
        }
    }
}

fn rand_hash(k: f64, seed: f64) -> f64 {
    let mut x = js_to_u32(k * 2654435761.0 + seed * 97.0);
    x ^= x >> 16;
    x = x.wrapping_mul(0x45d9f3b);
    x ^= x >> 16;
    x = x.wrapping_mul(0x45d9f3b);
    x ^= x >> 16;
    x as f64 / 4294967296.0
}

pub fn w_sin(vm: f64, f: f64, ph: f64, off: f64, ts: f64) -> Wave {
    Wave::new(WK::Sin { vm, w: TWO_PI * f, p: ph * PI / 180.0, off, ts })
}
pub fn w_squ(vpp: f64, f: f64, d: f64, off: f64, th: f64) -> Wave {
    let tp = 1.0 / f;
    let td = th / 360.0 * tp;
    let e1 = frac(td / tp) * tp;
    let e2 = frac((td + d * tp) / tp) * tp;
    Wave::new(WK::Squ { vpp, tp, d, off, td, e: [e1, e2] })
}
pub fn w_tri(vpp: f64, f: f64, d: f64, off: f64, th: f64) -> Wave {
    let tp = 1.0 / f;
    let td = th / 360.0 * tp;
    let e1 = frac(td / tp) * tp;
    let e2 = frac((td + d * tp) / tp) * tp;
    Wave::new(WK::Tri { vpp, tp, d, off, td, e: [e1, e2], is_j: d <= 0.0 || d >= 1.0 })
}
pub fn w_pwl(f: f64, ts: Vec<f64>, vs: Vec<f64>) -> Wave {
    let tp = if f > 0.0 { 1.0 / f } else { 0.0 };
    let offs = if tp > 0.0 { ts.iter().map(|&x| frac(x / tp) * tp).collect() } else { vec![] };
    Wave::new(WK::Pwl { tp, ts, vs, offs })
}

/// waveform from element params by type
pub fn make_wave(ctx: &ElCtx, t: &str) -> R<Wave> {
    Ok(match t {
        "DC" => Wave::new(WK::Dc(ctx.num("V")?)),
        "SIN" => w_sin(ctx.num("Vm")?, ctx.num("f")?, ctx.numd("ph", 0.0)?, ctx.numd("off", 0.0)?, ctx.numd("ts", 0.0)?),
        "SQU" => {
            let f = ctx.pos("f")?;
            w_squ(ctx.num("Vpp")?, f, ctx.numd("D", 0.5)?, ctx.numd("off", 0.0)?, ctx.numd("ph", 0.0)?)
        }
        "TRI" => {
            let f = ctx.pos("f")?;
            let d = ctx.numd("D", 0.5)?;
            if !(0.0..=1.0).contains(&d) {
                return Err(ctx.err("Duty Cycle 必須在 0 到 1 之間"));
            }
            w_tri(ctx.num("Vpp")?, f, d, ctx.numd("off", 0.0)?, ctx.numd("ph", 0.0)?)
        }
        "SAW" => {
            let vpk = ctx.num("Vpk")?;
            let f = ctx.pos("f")?;
            w_tri(vpk, f, 1.0, ctx.numd("off", 0.0)?, ctx.numd("ph", 0.0)?)
        }
        "STEP" => Wave::new(WK::Step { v: ctx.num("V")?, ts: ctx.num("ts")? }),
        "STEP2" => Wave::new(WK::Step2 { v1: ctx.num("V1")?, v2: ctx.num("V2")?, ts: ctx.num("ts")?, tr: ctx.numd("tr", 0.0)? }),
        "PWL" => {
            let vs = ctx.list("vals")?;
            let ts = ctx.list("times")?;
            if vs.len() != ts.len() || vs.is_empty() {
                return Err(ctx.err("數值與時間的點數必須相同且至少一點"));
            }
            for i in 1..ts.len() {
                if ts[i] < ts[i - 1] {
                    return Err(ctx.err("時間必須遞增"));
                }
            }
            w_pwl(ctx.numd("f", 0.0)?, ts, vs)
        }
        "PWL2" => {
            let pr = ctx.pairs("pairs")?;
            for i in 1..pr.len() {
                if pr[i].0 < pr[i - 1].0 {
                    return Err(ctx.err("時間必須遞增"));
                }
            }
            w_pwl(ctx.numd("f", 0.0)?, pr.iter().map(|q| q.0).collect(), pr.iter().map(|q| q.1).collect())
        }
        "RAND" => Wave::new(WK::Rand { vpp: ctx.num("Vpp")?, off: ctx.numd("off", 0.0)?, seed: ctx.int("seed", 1.0)? as f64, dt: ctx.b.dt }),
        "MATH" => Wave::new(WK::Math { f: ctx.func("expr", &["t"])?, ts: ctx.numd("ts", 0.0)? }),
        _ => return Err(ctx.err("未知的波形")),
    })
}
