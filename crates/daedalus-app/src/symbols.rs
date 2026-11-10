//! Element symbols: the library stores each symbol as a small SVG fragment (path / circle / rect /
//! g transform) in local grid units. This module parses it once into vector primitives.

use daedalus_core::doc::Element;
use daedalus_core::library::{lib, tpt, ElDef, SymText};
use std::collections::HashMap;

#[derive(Clone, Debug)]
pub enum Prim {
    Poly { pts: Vec<[f32; 2]>, closed: bool, fill: bool, thin: bool, dash: bool },
    Circle { c: [f32; 2], r: f32, fill: bool, thin: bool },
}

#[derive(Clone, Debug, Default)]
pub struct Symbol {
    pub prims: Vec<Prim>,
    pub texts: Vec<SymText>,
}

type Aff = [f32; 6]; // x' = a x + c y + e ; y' = b x + d y + f
const ID: Aff = [1.0, 0.0, 0.0, 1.0, 0.0, 0.0];
fn mul(m: &Aff, n: &Aff) -> Aff {
    [m[0] * n[0] + m[2] * n[1], m[1] * n[0] + m[3] * n[1], m[0] * n[2] + m[2] * n[3], m[1] * n[2] + m[3] * n[3], m[0] * n[4] + m[2] * n[5] + m[4], m[1] * n[4] + m[3] * n[5] + m[5]]
}
fn ap(m: &Aff, x: f32, y: f32) -> [f32; 2] {
    [m[0] * x + m[2] * y + m[4], m[1] * x + m[3] * y + m[5]]
}

fn nums(s: &str) -> Vec<f32> {
    let c: Vec<char> = s.chars().collect();
    let mut out = Vec::new();
    let mut i = 0;
    while i < c.len() {
        let ch = c[i];
        if ch.is_ascii_digit() || ch == '-' || ch == '+' || ch == '.' {
            let st = i;
            if ch == '-' || ch == '+' {
                i += 1;
            }
            let mut dot = false;
            while i < c.len() && (c[i].is_ascii_digit() || (c[i] == '.' && !dot)) {
                if c[i] == '.' {
                    dot = true;
                }
                i += 1;
            }
            if i < c.len() && (c[i] == 'e' || c[i] == 'E') {
                i += 1;
                if i < c.len() && (c[i] == '-' || c[i] == '+') {
                    i += 1;
                }
                while i < c.len() && c[i].is_ascii_digit() {
                    i += 1;
                }
            }
            let t: String = c[st..i].iter().collect();
            if let Ok(v) = t.parse::<f32>() {
                out.push(v);
            }
            continue;
        }
        i += 1;
    }
    out
}

fn attr<'a>(tag: &'a str, name: &str) -> Option<&'a str> {
    let pat = format!(" {}=\"", name);
    let p = tag.find(&pat)? + pat.len();
    let e = tag[p..].find('"')? + p;
    Some(&tag[p..e])
}

fn parse_transform(t: &str) -> Aff {
    let mut m = ID;
    let mut rest = t;
    while let Some(p) = rest.find('(') {
        let name = rest[..p].trim();
        let e = match rest[p..].find(')') {
            Some(e) => e + p,
            None => break,
        };
        let a = nums(&rest[p + 1..e]);
        let n: Aff = match name {
            "translate" => [1.0, 0.0, 0.0, 1.0, a.first().copied().unwrap_or(0.0), a.get(1).copied().unwrap_or(0.0)],
            "scale" => {
                let sx = a.first().copied().unwrap_or(1.0);
                [sx, 0.0, 0.0, a.get(1).copied().unwrap_or(sx), 0.0, 0.0]
            }
            "rotate" => {
                let r = a.first().copied().unwrap_or(0.0).to_radians();
                [r.cos(), r.sin(), -r.sin(), r.cos(), 0.0, 0.0]
            }
            _ => ID,
        };
        m = mul(&m, &n);
        rest = &rest[e + 1..];
    }
    m
}

/// SVG elliptical arc (endpoint parameterisation) flattened into points (excluding the start point).
fn arc_pts(p0: [f32; 2], rx: f32, ry: f32, phi: f32, large: bool, sweep: bool, p1: [f32; 2]) -> Vec<[f32; 2]> {
    let (mut rx, mut ry) = (rx.abs(), ry.abs());
    if rx == 0.0 || ry == 0.0 {
        return vec![p1];
    }
    let (sp, cp) = phi.to_radians().sin_cos();
    let dx = (p0[0] - p1[0]) / 2.0;
    let dy = (p0[1] - p1[1]) / 2.0;
    let x1 = cp * dx + sp * dy;
    let y1 = -sp * dx + cp * dy;
    let lam = (x1 * x1) / (rx * rx) + (y1 * y1) / (ry * ry);
    if lam > 1.0 {
        rx *= lam.sqrt();
        ry *= lam.sqrt();
    }
    let num = rx * rx * ry * ry - rx * rx * y1 * y1 - ry * ry * x1 * x1;
    let den = rx * rx * y1 * y1 + ry * ry * x1 * x1;
    let mut co = (num / den).max(0.0).sqrt();
    if large == sweep {
        co = -co;
    }
    let cx1 = co * rx * y1 / ry;
    let cy1 = -co * ry * x1 / rx;
    let cx = cp * cx1 - sp * cy1 + (p0[0] + p1[0]) / 2.0;
    let cy = sp * cx1 + cp * cy1 + (p0[1] + p1[1]) / 2.0;
    let ang = |ux: f32, uy: f32, vx: f32, vy: f32| {
        let d = (ux * vx + uy * vy) / ((ux * ux + uy * uy).sqrt() * (vx * vx + vy * vy).sqrt());
        let a = d.clamp(-1.0, 1.0).acos();
        if ux * vy - uy * vx < 0.0 {
            -a
        } else {
            a
        }
    };
    let t1 = ang(1.0, 0.0, (x1 - cx1) / rx, (y1 - cy1) / ry);
    let mut dt = ang((x1 - cx1) / rx, (y1 - cy1) / ry, (-x1 - cx1) / rx, (-y1 - cy1) / ry);
    if !sweep && dt > 0.0 {
        dt -= std::f32::consts::TAU;
    } else if sweep && dt < 0.0 {
        dt += std::f32::consts::TAU;
    }
    let n = ((dt.abs() / (std::f32::consts::PI / 12.0)).ceil() as usize).max(2);
    (1..=n)
        .map(|k| {
            let t = t1 + dt * k as f32 / n as f32;
            let (st, ct) = t.sin_cos();
            [cp * rx * ct - sp * ry * st + cx, sp * rx * ct + cp * ry * st + cy]
        })
        .collect()
}

fn parse_path(d: &str, m: &Aff, fill: bool, thin: bool, dash: bool, out: &mut Vec<Prim>) {
    // split into commands
    let c: Vec<char> = d.chars().collect();
    let mut cmds: Vec<(char, Vec<f32>)> = Vec::new();
    let mut i = 0;
    while i < c.len() {
        if c[i].is_ascii_alphabetic() && c[i] != 'e' && c[i] != 'E' {
            let cmd = c[i];
            let st = i + 1;
            let mut j = st;
            while j < c.len() && !(c[j].is_ascii_alphabetic() && c[j] != 'e' && c[j] != 'E') {
                j += 1;
            }
            let s: String = c[st..j].iter().collect();
            cmds.push((cmd, nums(&s)));
            i = j;
        } else {
            i += 1;
        }
    }
    let mut cur = [0.0f32, 0.0];
    let mut start = cur;
    let mut pts: Vec<[f32; 2]> = Vec::new();
    let flush = |pts: &mut Vec<[f32; 2]>, closed: bool, out: &mut Vec<Prim>| {
        if pts.len() >= 2 {
            let p: Vec<[f32; 2]> = pts.iter().map(|q| ap(m, q[0], q[1])).collect();
            out.push(Prim::Poly { pts: p, closed, fill: fill && closed, thin, dash });
        }
        pts.clear();
    };
    for (cmd, a) in cmds {
        let rel = cmd.is_ascii_lowercase();
        match cmd.to_ascii_uppercase() {
            'M' => {
                flush(&mut pts, false, out);
                let mut k = 0;
                while k + 1 < a.len() {
                    let p = if rel { [cur[0] + a[k], cur[1] + a[k + 1]] } else { [a[k], a[k + 1]] };
                    cur = p;
                    if k == 0 {
                        start = p;
                    }
                    pts.push(p);
                    k += 2;
                }
            }
            'L' => {
                let mut k = 0;
                while k + 1 < a.len() {
                    let p = if rel { [cur[0] + a[k], cur[1] + a[k + 1]] } else { [a[k], a[k + 1]] };
                    if pts.is_empty() {
                        pts.push(cur);
                    }
                    pts.push(p);
                    cur = p;
                    k += 2;
                }
            }
            'H' => {
                for &v in &a {
                    let p = [if rel { cur[0] + v } else { v }, cur[1]];
                    if pts.is_empty() {
                        pts.push(cur);
                    }
                    pts.push(p);
                    cur = p;
                }
            }
            'V' => {
                for &v in &a {
                    let p = [cur[0], if rel { cur[1] + v } else { v }];
                    if pts.is_empty() {
                        pts.push(cur);
                    }
                    pts.push(p);
                    cur = p;
                }
            }
            'Q' => {
                let mut k = 0;
                while k + 3 < a.len() {
                    let (c1, p) = if rel { ([cur[0] + a[k], cur[1] + a[k + 1]], [cur[0] + a[k + 2], cur[1] + a[k + 3]]) } else { ([a[k], a[k + 1]], [a[k + 2], a[k + 3]]) };
                    if pts.is_empty() {
                        pts.push(cur);
                    }
                    for s in 1..=8 {
                        let t = s as f32 / 8.0;
                        let u = 1.0 - t;
                        pts.push([u * u * cur[0] + 2.0 * u * t * c1[0] + t * t * p[0], u * u * cur[1] + 2.0 * u * t * c1[1] + t * t * p[1]]);
                    }
                    cur = p;
                    k += 4;
                }
            }
            'C' => {
                let mut k = 0;
                while k + 5 < a.len() {
                    let o = if rel { cur } else { [0.0, 0.0] };
                    let c1 = [o[0] + a[k], o[1] + a[k + 1]];
                    let c2 = [o[0] + a[k + 2], o[1] + a[k + 3]];
                    let p = [o[0] + a[k + 4], o[1] + a[k + 5]];
                    if pts.is_empty() {
                        pts.push(cur);
                    }
                    for s in 1..=10 {
                        let t = s as f32 / 10.0;
                        let u = 1.0 - t;
                        let b = |i: usize| u * u * u * cur[i] + 3.0 * u * u * t * c1[i] + 3.0 * u * t * t * c2[i] + t * t * t * p[i];
                        pts.push([b(0), b(1)]);
                    }
                    cur = p;
                    k += 6;
                }
            }
            'A' => {
                let mut k = 0;
                while k + 6 < a.len() {
                    let p = if rel { [cur[0] + a[k + 5], cur[1] + a[k + 6]] } else { [a[k + 5], a[k + 6]] };
                    if pts.is_empty() {
                        pts.push(cur);
                    }
                    pts.extend(arc_pts(cur, a[k], a[k + 1], a[k + 2], a[k + 3] != 0.0, a[k + 4] != 0.0, p));
                    cur = p;
                    k += 7;
                }
            }
            'Z' => {
                if !pts.is_empty() {
                    flush(&mut pts, true, out);
                }
                cur = start;
            }
            _ => {}
        }
    }
    flush(&mut pts, false, out);
}

pub fn parse_svg(g: &str) -> Vec<Prim> {
    let mut out = Vec::new();
    let mut stack: Vec<Aff> = vec![ID];
    let mut rest = g;
    while let Some(p) = rest.find('<') {
        let e = match rest[p..].find('>') {
            Some(e) => e + p,
            None => break,
        };
        let tag = &rest[p..=e];
        rest = &rest[e + 1..];
        if tag.starts_with("</g") {
            if stack.len() > 1 {
                stack.pop();
            }
            continue;
        }
        let m = *stack.last().unwrap();
        let cls = attr(tag, "class").unwrap_or("");
        let fill = cls.split_whitespace().any(|c| c == "f");
        let thin = cls.split_whitespace().any(|c| c == "thin");
        let dash = cls.split_whitespace().any(|c| c == "dash");
        if tag.starts_with("<g") {
            let t = attr(tag, "transform").map(parse_transform).unwrap_or(ID);
            if !tag.ends_with("/>") {
                stack.push(mul(&m, &t));
            }
        } else if tag.starts_with("<path") {
            if let Some(d) = attr(tag, "d") {
                parse_path(d, &m, fill, thin, dash, &mut out);
            }
        } else if tag.starts_with("<circle") {
            let n = |k: &str| attr(tag, k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            let c = ap(&m, n("cx"), n("cy"));
            let s = (m[0] * m[0] + m[1] * m[1]).sqrt();
            out.push(Prim::Circle { c, r: n("r") * s, fill, thin });
        } else if tag.starts_with("<rect") {
            let n = |k: &str| attr(tag, k).and_then(|v| v.parse::<f32>().ok()).unwrap_or(0.0);
            let (x, y, w, h) = (n("x"), n("y"), n("width"), n("height"));
            let pts = vec![ap(&m, x, y), ap(&m, x + w, y), ap(&m, x + w, y + h), ap(&m, x, y + h)];
            out.push(Prim::Poly { pts, closed: true, fill, thin, dash });
        }
    }
    out
}

/// generic control block body: inputs left, outputs right (port of the library's blk())
pub fn blk(h: f32, ins: &[[f32; 2]], outs: &[[f32; 2]]) -> Vec<Prim> {
    let w = 1.3f32;
    let mut out = Vec::new();
    for &[x, y] in ins {
        if x < -w {
            out.push(Prim::Poly { pts: vec![[x, y], [-w, y]], closed: false, fill: false, thin: false, dash: false });
        }
    }
    for &[x, y] in outs {
        if x > w {
            out.push(Prim::Poly { pts: vec![[w, y], [x, y]], closed: false, fill: false, thin: false, dash: false });
        }
    }
    out.push(Prim::Poly { pts: vec![[-w, -h], [w, -h], [w, h], [-w, h]], closed: true, fill: false, thin: false, dash: false });
    out
}

#[derive(Default)]
pub struct SymCache {
    map: HashMap<String, Symbol>,
}
impl SymCache {
    pub fn get(&mut self, d: &ElDef) -> &Symbol {
        self.map.entry(d.typ.clone()).or_insert_with(|| Symbol { prims: parse_svg(&d.sym.g), texts: d.sym.t.clone() })
    }
    /// symbol for an element (some symbols depend on parameters)
    pub fn for_el(&mut self, el: &Element) -> Symbol {
        let d = match lib().get(&el.typ) {
            Some(d) => d,
            None => return Symbol::default(),
        };
        match el.typ.as_str() {
            "CONST" => {
                let mut v = el.params.get("v").cloned().unwrap_or_else(|| "1".into());
                if v.chars().count() > 6 {
                    v = v.chars().take(5).collect::<String>() + "…";
                }
                let mut s = self.get(d).clone();
                let sz = if v.chars().count() > 3 { 0.5 } else { 0.62 };
                if let Some(t) = s.texts.first_mut() {
                    t.s = v;
                    t.sz = sz;
                } else {
                    s.texts.push(SymText { x: 0.0, y: 0.3, s: v, sz, a: "middle".into() });
                }
                s
            }
            "CBLOCK" => {
                let pins = lib().pins_for(el);
                let n = pins.iter().filter(|p| p.k == "ci").count().max(1) as f32;
                let m = pins.iter().filter(|p| p.k == "co").count().max(1) as f32;
                let h = n.max(m) / 2.0 + 0.4;
                let ins: Vec<[f32; 2]> = pins.iter().filter(|p| p.k == "ci").map(|p| [p.x as f32, p.y as f32]).collect();
                let outs: Vec<[f32; 2]> = pins.iter().filter(|p| p.k == "co").map(|p| [p.x as f32, p.y as f32]).collect();
                let mut texts = vec![SymText { x: 0.0, y: 0.3, s: "C".into(), sz: 0.9, a: "middle".into() }];
                for (k, p) in ins.iter().enumerate() {
                    texts.push(SymText { x: -1.2, y: p[1] as f64 + 0.15, s: format!("x{}", k + 1), sz: 0.36, a: "start".into() });
                }
                for (k, p) in outs.iter().enumerate() {
                    texts.push(SymText { x: 1.2, y: p[1] as f64 + 0.15, s: format!("y{}", k + 1), sz: 0.36, a: "end".into() });
                }
                Symbol { prims: blk(h, &ins, &outs), texts }
            }
            _ => self.get(d).clone(),
        }
    }
}

/// element-local -> world point
#[inline]
pub fn local_to_world(el: &Element, p: [f32; 2]) -> [f64; 2] {
    let (x, y) = tpt(el, p[0] as f64, p[1] as f64);
    [x, y]
}
