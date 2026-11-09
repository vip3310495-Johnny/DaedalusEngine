//! Model registry: element type -> model instances.

pub mod cblock;
pub mod control;
pub mod devices;
pub mod power;
pub mod wave;

use crate::sim::{ElCtx, Inst, R};
use power::*;

type Out = R<Option<Vec<Box<dyn Inst>>>>;

fn phys(ctx: &ElCtx, d: &str) -> bool {
    let m = ctx.raw("model", d);
    m != "ideal" && m != "3state"
}
fn sw(ctx: &mut ElCtx, a: i32, k: i32, t: SwT, o: SwO) -> Box<dyn Inst> {
    Box::new(Sw::new(ctx.b, a, k, t, o))
}
fn init01(ctx: &ElCtx) -> R<i32> {
    Ok(if ctx.int("init", 0.0)? != 0 { 1 } else { 0 })
}

/// Thyristor / diode bridges
fn bridge(ctx: &mut ElCtx, list: &[(usize, usize)], kind: char, ct_pin: usize, shifts: &[f64]) -> R<Vec<Box<dyn Inst>>> {
    let vf = ctx.numd("vf", 0.0)?;
    let mut out: Vec<Box<dyn Inst>> = Vec::new();
    if kind == 'D' {
        for &(a, k) in list {
            let (pa, pk) = (ctx.p(a)?, ctx.p(k)?);
            out.push(sw(ctx, pa, pk, SwT::D, SwO { vf, ..Default::default() }));
        }
        return Ok(out);
    }
    let ct = ctx.c(ct_pin)?;
    let gs = GateShift::new(ctx, ct, shifts.to_vec());
    let gouts = gs.outs();
    out.push(Box::new(gs));
    for (i, &(a, k)) in list.iter().enumerate() {
        let (pa, pk) = (ctx.p(a)?, ctx.p(k)?);
        out.push(sw(ctx, pa, pk, SwT::Thy, SwO { vf, gn: Some(gouts[i]), ..Default::default() }));
    }
    Ok(out)
}

pub fn make(ctx: &mut ElCtx) -> Out {
    let t = ctx.typ.clone();
    let ron = |c: &ElCtx| c.numd("ron", RON);
    let roff = |c: &ElCtx| c.numd("roff", ROFF);
    let v: Vec<Box<dyn Inst>> = match t.as_str() {
        // ---------------- sources
        "VDC" | "VBAT" => vsrc(ctx, "DC", false)?,
        "VSIN" => vsrc(ctx, "SIN", false)?,
        "VSQU" => vsrc(ctx, "SQU", false)?,
        "VTRI" => vsrc(ctx, "TRI", false)?,
        "VSAW" => vsrc(ctx, "SAW", false)?,
        "VSTEP" => vsrc(ctx, "STEP", false)?,
        "VSTEP2" => vsrc(ctx, "STEP2", false)?,
        "VPWL" => vsrc(ctx, "PWL", false)?,
        "VPWL2" => vsrc(ctx, "PWL2", false)?,
        "VRAND" => vsrc(ctx, "RAND", false)?,
        "VMATH" => vsrc(ctx, "MATH", false)?,
        "VGDC" | "VGDCT" => vsrc(ctx, "DC", true)?,
        "IDC" => isrc(ctx, "DC")?,
        "ISIN" => isrc(ctx, "SIN")?,
        "ISQU" => isrc(ctx, "SQU")?,
        "ITRI" => isrc(ctx, "TRI")?,
        "ISTEP" => isrc(ctx, "STEP")?,
        "ISTEP2" => isrc(ctx, "STEP2")?,
        "IPWL" => isrc(ctx, "PWL")?,
        "IPWL2" => isrc(ctx, "PWL2")?,
        "IRAND" => isrc(ctx, "RAND")?,
        "IMATH" => isrc(ctx, "MATH")?,
        "VSIN3" => vsin3(ctx)?,
        // ---------------- RLC
        "R" => {
            let r = ctx.num("R")?;
            if r < 0.0 {
                return Err(ctx.err("電阻不可為負值"));
            }
            let (a, b) = (ctx.p(0)?, ctx.p(1)?);
            vec![Box::new(Res::new(a, b, r))]
        }
        "C" => cap(ctx)?,
        "L" => {
            let l = ctx.num("L")?;
            if !(l > 0.0) {
                return Err(ctx.err("電感必須大於 0"));
            }
            let (a, b) = (ctx.p(0)?, ctx.p(1)?);
            let i0 = ctx.numd("i0", 0.0)?;
            vec![Box::new(SerBr::new(ctx.b, a, b, 0.0, l, 0.0, i0, 0.0))]
        }
        "RL" => br_mk(ctx, true, true, false)?,
        "RC" => br_mk(ctx, true, false, true)?,
        "LC" => br_mk(ctx, false, true, true)?,
        "RLC" => br_mk(ctx, true, true, true)?,
        "R3" => br3(ctx, true, false, false)?,
        "RL3" => br3(ctx, true, true, false)?,
        "RC3" => br3(ctx, true, false, true)?,
        "RLC3" => br3(ctx, true, true, true)?,
        "RHEO" => {
            let r = ctx.pos("R")?;
            let tap = ctx.numd("tap", 0.5)?;
            if !(0.0..=1.0).contains(&tap) {
                return Err(ctx.err("Tap 位置必須在 0 到 1 之間"));
            }
            let (a, b, tp) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?);
            vec![Box::new(Res::new(a, tp, r * tap)), Box::new(Res::new(tp, b, r * (1.0 - tap)))]
        }
        "LSAT" => lsat(ctx)?,
        "NONI" => noni(ctx)?,
        "NONV" => nonv(ctx)?,
        // ---------------- switches
        "DIODE" => {
            if phys(ctx, "ideal") {
                let (a, k) = (ctx.p(0)?, ctx.p(1)?);
                devices::dev_diode(ctx, a, k, None)?
            } else {
                let (a, k) = (ctx.p(0)?, ctx.p(1)?);
                let o = SwO { vf: ctx.numd("vf", 0.0)?, ron: ron(ctx)?, roff: roff(ctx)?, init: init01(ctx)?, ..Default::default() };
                vec![sw(ctx, a, k, SwT::D, o)]
            }
        }
        "LED" => {
            let (a, k) = (ctx.p(0)?, ctx.p(1)?);
            if phys(ctx, "ideal") {
                devices::dev_diode(ctx, a, k, None)?
            } else {
                let o = SwO { vf: ctx.numd("vf", 2.0)?, ron: ron(ctx)?, roff: roff(ctx)?, ..Default::default() };
                vec![sw(ctx, a, k, SwT::D, o)]
            }
        }
        "ZENER" => {
            let (a, k) = (ctx.p(0)?, ctx.p(1)?);
            if phys(ctx, "ideal") {
                let bv = ctx.num("vb")?;
                devices::dev_diode(ctx, a, k, Some(bv))?
            } else {
                let o = SwO { vf: ctx.numd("vf", 0.0)?, vb: ctx.num("vb")?, ron: ron(ctx)?, roff: roff(ctx)?, ..Default::default() };
                vec![sw(ctx, a, k, SwT::Zen, o)]
            }
        }
        "DIAC" => {
            let (a, k) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { vb: ctx.num("vbo")?, vbb: ctx.numd("vbb", 0.0)?, ..Default::default() };
            vec![sw(ctx, a, k, SwT::Diac, o)]
        }
        "THY" => {
            let (a, k) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { vf: ctx.numd("vf", 0.0)?, ih: ctx.numd("ih", 0.0)?, il: ctx.numd("il", 0.0)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ..Default::default() };
            vec![sw(ctx, a, k, SwT::Thy, o)]
        }
        "TRIAC" => {
            let (a, k) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { vf: ctx.numd("vf", 0.0)?, gn: Some(ctx.c(2)?), ..Default::default() };
            vec![sw(ctx, a, k, SwT::Triac, o)]
        }
        "GTO" => {
            let (a, k) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { vf: ctx.numd("vf", 0.0)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ..Default::default() };
            vec![sw(ctx, a, k, SwT::Gto, o)]
        }
        "NPN" | "PNP" => {
            let pnp = t == "PNP";
            let (a, k) = if pnp { (ctx.p(1)?, ctx.p(0)?) } else { (ctx.p(0)?, ctx.p(1)?) };
            let o = SwO { vf: ctx.numd("vf", 0.0)?, roff: roff(ctx)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ginv: pnp, ..Default::default() };
            vec![sw(ctx, a, k, SwT::Gto, o)]
        }
        "IGBT" => {
            let (c, e) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { vf: ctx.numd("vf", 0.0)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ..Default::default() };
            let s1 = sw(ctx, c, e, SwT::Gto, o);
            let o2 = SwO { vf: ctx.numd("vd", 0.0)?, roff: roff(ctx)?, ..Default::default() };
            vec![s1, sw(ctx, e, c, SwT::D, o2)]
        }
        "MOSFET" => {
            let (d, s) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { ron: ron(ctx)?, roff: roff(ctx)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ..Default::default() };
            let s1 = sw(ctx, d, s, SwT::Gto, o);
            let o2 = SwO { vf: ctx.numd("vd", 0.0)?, roff: roff(ctx)?, ..Default::default() };
            vec![s1, sw(ctx, s, d, SwT::D, o2)]
        }
        "PMOS" => {
            let (d, s) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { ron: ron(ctx)?, roff: roff(ctx)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ginv: true, ..Default::default() };
            let s1 = sw(ctx, s, d, SwT::Gto, o);
            let o2 = SwO { vf: ctx.numd("vd", 0.0)?, roff: roff(ctx)?, ..Default::default() };
            vec![s1, sw(ctx, d, s, SwT::D, o2)]
        }
        "SSWI" => {
            let (a, b) = (ctx.p(0)?, ctx.p(1)?);
            let o = SwO { ron: ron(ctx)?, init: init01(ctx)?, gn: Some(ctx.c(2)?), ..Default::default() };
            vec![sw(ctx, a, b, SwT::Ssw, o)]
        }
        "SSWI3" => {
            let r = ron(ctx)?;
            let g = ctx.c(6)?;
            let mut out = vec![];
            for i in 0..3 {
                let (a, b) = (ctx.p(i)?, ctx.p(i + 3)?);
                out.push(sw(ctx, a, b, SwT::Ssw, SwO { ron: r, gn: Some(g), ..Default::default() }));
            }
            out
        }
        "SPDT" => {
            let (r, init, g) = (ron(ctx)?, init01(ctx)?, ctx.c(3)?);
            let (a, b, c) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?);
            vec![sw(ctx, a, b, SwT::Ssw, SwO { ron: r, init, gn: Some(g), ..Default::default() }), sw(ctx, a, c, SwT::Ssw, SwO { ron: r, init: 1 - init, gn: Some(g), ginv: true, ..Default::default() })]
        }
        "DPST" => {
            let (r, init, g) = (ron(ctx)?, init01(ctx)?, ctx.c(4)?);
            let mut out = vec![];
            for i in 0..2 {
                let (a, b) = (ctx.p(2 * i)?, ctx.p(2 * i + 1)?);
                out.push(sw(ctx, a, b, SwT::Ssw, SwO { ron: r, init, gn: Some(g), ..Default::default() }));
            }
            out
        }
        "DPDT" => {
            let (r, init, g) = (ron(ctx)?, init01(ctx)?, ctx.c(6)?);
            let mut out = vec![];
            for o in [0usize, 3] {
                let (a, b, c) = (ctx.p(o)?, ctx.p(o + 1)?, ctx.p(o + 2)?);
                out.push(sw(ctx, a, b, SwT::Ssw, SwO { ron: r, init, gn: Some(g), ..Default::default() }));
                out.push(sw(ctx, a, c, SwT::Ssw, SwO { ron: r, init: 1 - init, gn: Some(g), ginv: true, ..Default::default() }));
            }
            out
        }
        "PBSW" => {
            let (a, b) = (ctx.p(0)?, ctx.p(1)?);
            let init = init01(ctx)?;
            let times = ctx.list_opt("times", Some(vec![]))?;
            vec![Box::new(Sw::timed(ctx.b, a, b, init, times))]
        }
        "BDIODE1" => bridge(ctx, &[(0, 2), (3, 1), (1, 2), (3, 0)], 'D', 0, &[])?,
        "BDIODE3" => bridge(ctx, &[(0, 3), (1, 3), (2, 3), (4, 0), (4, 1), (4, 2)], 'D', 0, &[])?,
        "BTHY1" => bridge(ctx, &[(0, 2), (3, 1), (1, 2), (3, 0)], 'T', 4, &[0.0, 0.0, 180.0, 180.0])?,
        "BTHY3" => bridge(ctx, &[(0, 3), (4, 2), (1, 3), (4, 0), (2, 3), (4, 1)], 'T', 5, &[0.0, 60.0, 120.0, 180.0, 240.0, 300.0])?,
        "BTHY3H" => bridge(ctx, &[(0, 3), (1, 3), (2, 3)], 'T', 4, &[0.0, 120.0, 240.0])?,
        "BTHY6H" => bridge(ctx, &[(0, 6), (1, 6), (2, 6), (3, 6), (4, 6), (5, 6)], 'T', 7, &[0.0, 60.0, 120.0, 180.0, 240.0, 300.0])?,
        // ---------------- three-state devices
        "MOS3" | "PMOS3" => {
            let pmos = t == "PMOS3";
            if phys(ctx, if pmos { "GENERIC_P" } else { "GENERIC_N" }) {
                devices::dev_mos(ctx, if pmos { -1.0 } else { 1.0 })?
            } else {
                let (d, s, g) = (ctx.p(0)?, ctx.p(1)?, ctx.p(2)?);
                let (vth, gm, r) = (ctx.num("vth")?, ctx.pos("gm")?, ctx.pos("ron")?);
                let vd = ctx.numd("vd", 0.0)?;
                if pmos {
                    let ch = MosCh::new(ctx.b, s, d, g, s, -1.0, vth, gm, r);
                    vec![Box::new(ch), sw(ctx, d, s, SwT::D, SwO { vf: vd, ..Default::default() })]
                } else {
                    let ch = MosCh::new(ctx.b, d, s, g, s, 1.0, vth, gm, r);
                    vec![Box::new(ch), sw(ctx, s, d, SwT::D, SwO { vf: vd, ..Default::default() })]
                }
            }
        }
        "NPN3" | "PNP3" => {
            let pnp = t == "PNP3";
            if phys(ctx, if pnp { "2N3906" } else { "2N3904" }) {
                devices::dev_bjt(ctx, if pnp { -1.0 } else { 1.0 })?
            } else {
                let beta = ctx.pos("beta")?;
                let (vr, vsat, rf) = (ctx.numd("vr", 0.7)?, ctx.numd("vsat", 0.2)?, roff(ctx)?);
                let b = if pnp {
                    let (p1, p2, p0) = (ctx.p(1)?, ctx.p(2)?, ctx.p(0)?);
                    Bjt3::new(ctx.b, p1, p2, p1, p0, beta, vr, vsat, rf)
                } else {
                    let (p2, p1, p0) = (ctx.p(2)?, ctx.p(1)?, ctx.p(0)?);
                    Bjt3::new(ctx.b, p2, p1, p0, p1, beta, vr, vsat, rf)
                };
                vec![Box::new(b)]
            }
        }
        // ---------------- coupled inductors and transformers
        "MUT2" => {
            let (l11, l12, l22) = (ctx.num("l11")?, ctx.num("l12")?, ctx.num("l22")?);
            if l11 * l22 - l12 * l12 < -1e-12 * l11 * l22 {
                return Err(ctx.err("互感過大：需 L12² ≤ L11·L22"));
            }
            let w = vec![(ctx.p(0)?, ctx.p(1)?), (ctx.p(2)?, ctx.p(3)?)];
            let i0 = vec![ctx.numd("i1", 0.0)?, ctx.numd("i2", 0.0)?];
            vec![Box::new(MutL::new(ctx.b, w, vec![vec![l11, l12], vec![l12, l22]], i0))]
        }
        "MUT3" => {
            let (l11, l12, l13, l22, l23, l33) = (ctx.num("l11")?, ctx.num("l12")?, ctx.num("l13")?, ctx.num("l22")?, ctx.num("l23")?, ctx.num("l33")?);
            let l = vec![vec![l11, l12, l13], vec![l12, l22, l23], vec![l13, l23, l33]];
            let w = vec![(ctx.p(0)?, ctx.p(1)?), (ctx.p(2)?, ctx.p(3)?), (ctx.p(4)?, ctx.p(5)?)];
            let i0 = vec![ctx.numd("i1", 0.0)?, ctx.numd("i2", 0.0)?, ctx.numd("i3", 0.0)?];
            vec![Box::new(MutL::new(ctx.b, w, l, i0))]
        }
        "TFI" => {
            let (np, ns) = (ctx.pos("np")?, ctx.pos("ns")?);
            let w = vec![(ctx.p(0)?, ctx.p(1)?, np), (ctx.p(2)?, ctx.p(3)?, ns)];
            vec![Box::new(Xfmr::new(ctx.b, w))]
        }
        "TF1" => {
            let w = [(ctx.p(0)?, ctx.p(1)?, ctx.numd("rp", 0.0)?, ctx.numd("lp", 0.0)?, ctx.pos("np")?), (ctx.p(2)?, ctx.p(3)?, ctx.numd("rs", 0.0)?, ctx.numd("ls", 0.0)?, ctx.pos("ns")?)];
            let lm = ctx.numd("lm", 0.0)?;
            real_xfmr(ctx.b, &w, lm)
        }
        "TF3W" => {
            let w = [
                (ctx.p(0)?, ctx.p(1)?, ctx.numd("rp", 0.0)?, ctx.numd("lp", 0.0)?, ctx.pos("np")?),
                (ctx.p(2)?, ctx.p(3)?, ctx.numd("rs", 0.0)?, ctx.numd("ls", 0.0)?, ctx.pos("ns")?),
                (ctx.p(4)?, ctx.p(5)?, ctx.numd("rt", 0.0)?, ctx.numd("lt", 0.0)?, ctx.pos("nt")?),
            ];
            let lm = ctx.numd("lm", 0.0)?;
            real_xfmr(ctx.b, &w, lm)
        }
        "TF3YY" | "TF3YD" => {
            let yd = t == "TF3YD";
            let mut out = vec![];
            for i in 0..3 {
                let sec_b = if yd { ctx.p(4 + (i + 1) % 3)? } else { ctx.p(7)? };
                let w = [(ctx.p(i)?, ctx.p(3)?, ctx.numd("rp", 0.0)?, ctx.numd("lp", 0.0)?, ctx.pos("np")?), (ctx.p(4 + i)?, sec_b, ctx.numd("rs", 0.0)?, ctx.numd("ls", 0.0)?, ctx.pos("ns")?)];
                let lm = ctx.numd("lm", 0.0)?;
                out.extend(real_xfmr(ctx.b, &w, lm));
            }
            out
        }
        // ---------------- other power
        "OPAMP" => opamp(ctx, false)?,
        "OPAMP1" => opamp(ctx, true)?,
        "DVDT" => dvdt(ctx)?,
        "COMPOC" => devices::comp_oc(ctx)?,
        "TL431" => devices::tl431(ctx)?,
        "OPTO1" => devices::opto(ctx, 1)?,
        "OPTO2" => devices::opto(ctx, 2)?,
        "MOSNI" => devices::ni_mos(ctx, 1.0)?,
        "PMOSNI" => devices::ni_mos(ctx, -1.0)?,
        "IGBTNI" => devices::ni_igbt(ctx)?,
        "DEVP" => devices::devp(ctx)?,
        // ---------------- controlled sources
        "VVCVS" => vctl(ctx, false)?,
        "IVCCS" => vctl(ctx, true)?,
        "VCCVS1" => cctl(ctx, false)?,
        "ICCCS1" => cctl(ctx, true)?,
        "VCCVS" => cctlx(ctx, false)?,
        "ICCCS" => cctlx(ctx, true)?,
        "VNONM" => vnon(ctx, 'M', false)?,
        "VNOND" => vnon(ctx, 'D', false)?,
        "VNONSQ" => vnon(ctx, 'Q', false)?,
        "VPOWERS" => vnon(ctx, 'P', false)?,
        "VVCVSV" => vnon(ctx, 'V', false)?,
        "INONM" => vnon(ctx, 'M', true)?,
        "INOND" => vnon(ctx, 'D', true)?,
        "INONSQ" => vnon(ctx, 'Q', true)?,
        "IVCCSV" => vnon(ctx, 'V', true)?,
        "CTOP" => ctop(ctx)?,
        // ---------------- sensors and probes
        "VSEN" => vsen(ctx)?,
        "ISEN" => isen(ctx)?,
        "VP" => vprobe(ctx, false)?,
        "VP2" => vprobe(ctx, true)?,
        "IP" => iprobe(ctx)?,
        "VDCM" | "VACM" | "ADCM" | "AACM" | "WATT" => meter(ctx, &t)?,
        "ACP" => acprobe(ctx, false)?,
        "ACP2" => acprobe(ctx, true)?,
        _ => return control::make(ctx),
    };
    Ok(Some(v))
}
