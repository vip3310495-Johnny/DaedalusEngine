//! Pictures for the AI tools: the schematic and waveform plots as SVG, rasterised to PNG with resvg.
//! Symbols come from the element library (SVG fragments), so the schematic looks like the program's.
use crate::circuit::{analyze, label_lines, label_width, local_bb, pin_dir, world_box};
use daedalus_core::doc::{Doc, Element};
use daedalus_core::library::{lib, tpt};
use daedalus_core::netlist::connectivity;
use daedalus_core::util::si;
use resvg::{tiny_skia, usvg};
use std::fmt::Write;
use std::sync::{Arc, OnceLock};

const WIRE: &str = "#1f2a44";
const CTL: &str = "#1c6fd1";
const SYM: &str = "#22304a";
const TXT: &str = "#4b5563";
const LBL: &str = "#0f766e";
const HIL: &str = "#f97316";
const FONT: &str = "Microsoft JhengHei, Segoe UI, Arial, sans-serif";
pub const COLORS: [&str; 8] = ["#1c6fd1", "#d1495b", "#2a9d8f", "#e9a03b", "#8e5cd9", "#3d405b", "#c2185b", "#5d8a00"];

fn esc(s: &str) -> String {
    s.replace('&', "&amp;").replace('<', "&lt;").replace('>', "&gt;").replace('"', "&quot;")
}
fn f(v: f64) -> String {
    let s = format!("{:.3}", v);
    let s = s.trim_end_matches('0').trim_end_matches('.');
    if s == "-0" {
        "0".into()
    } else {
        s.to_string()
    }
}

pub struct SchOpts {
    /// a net name, a pin reference or an element name to emphasise
    pub highlight: Option<String>,
    /// write every pin's name next to it
    pub pin_names: bool,
    /// largest side of the picture in pixels
    pub max_px: f64,
}

/// the schematic as SVG (world units: one grid step = one unit)
pub fn schematic_svg(doc: &Doc, o: &SchOpts) -> String {
    let els: Vec<&Element> = doc.elements.iter().filter(|e| !e.dis).collect();
    let conn = connectivity(doc, &els);
    // bounds
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    let mut grow = |x: f64, y: f64| {
        b[0] = b[0].min(x);
        b[1] = b[1].min(y);
        b[2] = b[2].max(x);
        b[3] = b[3].max(y);
    };
    for e in &doc.elements {
        let w = world_box(e);
        grow(w[0], w[1]);
        grow(w[2], w[3]);
        let n = label_lines(e).len() as f64;
        if n > 0.0 {
            grow(w[0], w[1] - n * 0.95 - 0.5);
            grow(w[2] + 6.0, w[3]);
        }
    }
    for w in &doc.wires {
        grow(w.x1, w.y1);
        grow(w.x2, w.y2);
    }
    if !b[0].is_finite() {
        b = [0.0, 0.0, 10.0, 6.0];
    }
    let (x0, y0) = ((b[0] - 2.0).floor(), (b[1] - 2.0).floor());
    let (x1, y1) = ((b[2] + 2.0).ceil(), (b[3] + 2.0).ceil());
    let (w, h) = (x1 - x0, y1 - y0);
    let s = (o.max_px / w.max(h)).clamp(6.0, 22.0);
    let mut out = String::new();
    let _ = write!(
        out,
        r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="{} {} {} {}" font-family="{}">"##,
        (w * s).round(),
        (h * s).round(),
        f(x0),
        f(y0),
        f(w),
        f(h),
        FONT
    );
    let _ = write!(out, r##"<style>.f{{fill:{SYM}}}.thin{{stroke-width:.05}}.dash{{stroke-dasharray:.2 .15}}</style>"##);
    let _ = write!(out, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#ffffff"/>"##, f(x0), f(y0), f(w), f(h));
    // grid dots and a coordinate ruler every 5 units, so positions can be read off the picture
    let _ = write!(out, r##"<defs><pattern id="g" x="0" y="0" width="1" height="1" patternUnits="userSpaceOnUse"><circle cx="0" cy="0" r="0.045" fill="#cfd6e2"/><circle cx="1" cy="0" r="0.045" fill="#cfd6e2"/><circle cx="0" cy="1" r="0.045" fill="#cfd6e2"/><circle cx="1" cy="1" r="0.045" fill="#cfd6e2"/></pattern></defs>"##);
    let _ = write!(out, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="url(#g)"/>"##, f(x0), f(y0), f(w), f(h));
    let mut gx = (x0 / 5.0).ceil() * 5.0;
    while gx <= x1 {
        let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.55" fill="#9aa3b2" text-anchor="middle">{}</text>"##, f(gx), f(y0 + 0.75), gx);
        gx += 5.0;
    }
    let mut gy = (y0 / 5.0).ceil() * 5.0;
    while gy <= y1 {
        let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.55" fill="#9aa3b2">{}</text>"##, f(x0 + 0.2), f(gy + 0.2), gy);
        gy += 5.0;
    }
    // highlight: a net (by name or pin) or an element
    let mut hi_root: Option<usize> = None;
    let mut hi_el: Option<i64> = None;
    let mut hi_gnd = false;
    if let Some(hl) = o.highlight.as_deref().map(str::trim).filter(|s| !s.is_empty()) {
        let a = analyze(doc);
        let pin = if hl.contains('.') { a.pin_net.get(hl).map(|&i| &a.nets[i]) } else { None };
        let net = pin.or_else(|| a.net_named(hl).map(|i| &a.nets[i]));
        if let Some(n) = net {
            hi_gnd = n.ground;
            if let Some(p) = n.pins.first() {
                if let Ok((i, pd, _)) = crate::circuit::pin_of(doc, p) {
                    let (px, py) = tpt(&doc.elements[i], pd.x, pd.y);
                    hi_root = conn.root(px, py);
                }
            } else if let Some(l) = doc.elements.iter().find(|e| e.typ == "LABEL" && n.labels.iter().any(|t| Some(t) == e.params.get("text"))) {
                hi_root = conn.root(l.x, l.y);
            }
        } else if let Ok(i) = crate::circuit::find_el(doc, hl) {
            hi_el = Some(doc.elements[i].id);
        }
    }
    // wires
    for wi in &doc.wires {
        let r = conn.root(wi.x1, wi.y1);
        let hit = (hi_root.is_some() && r == hi_root) || (hi_gnd && conn.is_gnd_root(r));
        let (col, sw) = if hit { (HIL, 0.26) } else if conn.is_ctl_root(r) { (CTL, 0.12) } else { (WIRE, 0.12) };
        let _ = write!(out, r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="{}" stroke-width="{}" stroke-linecap="round"/>"##, f(wi.x1), f(wi.y1), f(wi.x2), f(wi.y2), col, sw);
    }
    for &(x, y) in &conn.junctions {
        let r = conn.root(x, y);
        let col = if (hi_root.is_some() && r == hi_root) || (hi_gnd && conn.is_gnd_root(r)) { HIL } else if conn.is_ctl_root(r) { CTL } else { WIRE };
        let _ = write!(out, r##"<circle cx="{}" cy="{}" r="0.2" fill="{}"/>"##, f(x), f(y), col);
    }
    // elements
    for e in &doc.elements {
        let d = match lib().get(&e.typ) {
            Some(d) => d,
            None => {
                let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.8" fill="#dc2626">?{}</text>"##, f(e.x), f(e.y), esc(&e.typ));
                continue;
            }
        };
        let op = if e.dis { r#" opacity="0.35""# } else { "" };
        let tr = format!("translate({} {}) rotate({}) scale({} 1)", f(e.x), f(e.y), (e.rot & 3) * 90, if e.mir { -1 } else { 1 });
        match e.typ.as_str() {
            "LABEL" => {
                let t = e.params.get("text").cloned().unwrap_or_default();
                let wd = label_width(&t);
                let pts: Vec<String> = [(0.0, 0.0), (0.5, -0.55), (wd, -0.55), (wd, 0.55), (0.5, 0.55)]
                    .iter()
                    .map(|&(a, b)| {
                        let (x, y) = tpt(e, a, b);
                        format!("{},{}", f(x), f(y))
                    })
                    .collect();
                let (cx, cy) = tpt(e, (0.5 + wd) / 2.0 + 0.05, 0.0);
                let q = tpt(e, 1.0, 0.0);
                let ang = if q.1 - e.y > 0.5 { 90 } else if q.1 - e.y < -0.5 { -90 } else { 0 };
                let _ = write!(out, r##"<g{op}><polygon points="{}" fill="#e6f6f3" stroke="{LBL}" stroke-width="0.08"/><text x="{}" y="{}" font-size="0.8" fill="{LBL}" text-anchor="middle" dominant-baseline="central" transform="rotate({} {} {})">{}</text></g>"##, pts.join(" "), f(cx), f(cy), ang, f(cx), f(cy), esc(&t));
                continue;
            }
            "TEXT" => {
                let t = e.params.get("text").cloned().unwrap_or_default();
                for (i, l) in t.split('\n').enumerate() {
                    let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.9" fill="#334155"{op}>{}</text>"##, f(e.x), f(e.y + i as f64 * 1.1), esc(l));
                }
                continue;
            }
            "CBLOCK" => {
                let [_, by0, _, by1] = local_bb(e);
                let _ = write!(out, r##"<g transform="{tr}" fill="none" stroke="{SYM}" stroke-width="0.09"{op}><rect x="-1.4" y="{}" width="2.8" height="{}"/>"##, f(by0), f(by1 - by0));
                for p in lib().pins_for(e) {
                    let ex = if p.x < 0.0 { -1.4 } else { 1.4 };
                    let _ = write!(out, r##"<line x1="{}" y1="{}" x2="{}" y2="{}"/>"##, f(p.x), f(p.y), f(ex), f(p.y));
                }
                let _ = write!(out, "</g>");
                let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.9" fill="{SYM}" text-anchor="middle" dominant-baseline="central">C</text>"##, f(e.x), f(e.y));
            }
            _ => {
                let _ = write!(out, r##"<g transform="{tr}" fill="none" stroke="{SYM}" stroke-width="0.09" stroke-linecap="round" stroke-linejoin="round"{op}>{}</g>"##, d.sym.g);
                for t in &d.sym.t {
                    let (x, y) = tpt(e, t.x, t.y);
                    let anchor = match t.a.as_str() {
                        "start" if e.mir => "end",
                        "end" if e.mir => "start",
                        a => a,
                    };
                    let _ = write!(out, r##"<text x="{}" y="{}" font-size="{}" fill="{SYM}" text-anchor="{}"{op}>{}</text>"##, f(x), f(y), f(t.sz), anchor, esc(&t.s));
                }
            }
        }
        // name and shown parameters
        let lines = label_lines(e);
        if !lines.is_empty() {
            let wb = world_box(e);
            let wide = (wb[2] - wb[0]) >= (wb[3] - wb[1]);
            let n = lines.len();
            for (li, t) in lines.iter().enumerate() {
                let (x, y, anchor) = if wide {
                    ((wb[0] + wb[2]) / 2.0, wb[1] - 0.35 - (n - 1 - li) as f64 * 0.95, "middle")
                } else {
                    (wb[2] + 0.35, (wb[1] + wb[3]) / 2.0 - (n - 1) as f64 * 0.95 / 2.0 + li as f64 * 0.95 + 0.3, "start")
                };
                let fw = if li == 0 && !d.noname() { r#" font-weight="bold""# } else { "" };
                let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.78" fill="{TXT}" text-anchor="{}"{fw}{op}>{}</text>"##, f(x), f(y), anchor, esc(t));
            }
        }
        if o.pin_names && !matches!(e.typ.as_str(), "LABEL" | "GND" | "TEXT") {
            for p in lib().pins_for(e) {
                let (px, py) = tpt(e, p.x, p.y);
                let dv = pin_dir(e, &p);
                let (tx, ty) = (px + dv.0 * 0.45 - dv.1 * 0.0, py + dv.1 * 0.45 - 0.2);
                let _ = write!(out, r##"<text x="{}" y="{}" font-size="0.48" fill="#9333ea" text-anchor="middle">{}</text>"##, f(tx), f(ty), esc(&p.n));
            }
        }
        if hi_el == Some(e.id) {
            let wb = world_box(e);
            let _ = write!(out, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="none" stroke="{HIL}" stroke-width="0.14" stroke-dasharray="0.4 0.25" rx="0.3"/>"##, f(wb[0] - 0.5), f(wb[1] - 0.5), f(wb[2] - wb[0] + 1.0), f(wb[3] - wb[1] + 1.0));
        }
    }
    // pins that are not connected
    for &(x, y) in &conn.open {
        let _ = write!(out, r##"<circle cx="{}" cy="{}" r="0.38" fill="none" stroke="#dc2626" stroke-width="0.1"/>"##, f(x), f(y));
    }
    out.push_str("</svg>");
    out
}

/* ------------------------------------------------------------------ plots */

pub struct Trace {
    pub name: String,
    pub x: Vec<f64>,
    /// lowest and highest value of each point (a band where the signal moves faster than the resolution)
    pub lo: Vec<f64>,
    pub hi: Vec<f64>,
}

pub struct Panel {
    pub traces: Vec<Trace>,
    pub unit: String,
}

pub struct Axis {
    pub lo: f64,
    pub hi: f64,
    pub log: bool,
    pub unit: String,
}

fn nice_ticks(a: f64, b: f64, n: usize) -> Vec<f64> {
    if !(b > a) || !a.is_finite() || !b.is_finite() {
        return vec![a];
    }
    let raw = (b - a) / n as f64;
    let p = 10f64.powf(raw.log10().floor());
    let m = raw / p;
    let step = p * if m < 1.5 { 1.0 } else if m < 3.5 { 2.0 } else if m < 7.5 { 5.0 } else { 10.0 };
    let mut v = (a / step).ceil() * step;
    let mut out = vec![];
    while v <= b + step * 1e-9 {
        out.push(if v.abs() < step * 1e-9 { 0.0 } else { v });
        v += step;
    }
    out
}

/// stacked panels over one x axis (time, or frequency on a log axis)
pub fn plot_svg(title: &str, panels: &[Panel], ax: &Axis, width: f64) -> String {
    let (ml, mr, mt, mb, ph, gap) = (78.0, 18.0, 30.0, 34.0, 170.0, 14.0);
    let h = mt + panels.len() as f64 * (ph + gap) - gap + mb;
    let pw = width - ml - mr;
    let mut out = String::new();
    let _ = write!(out, r##"<svg xmlns="http://www.w3.org/2000/svg" width="{}" height="{}" viewBox="0 0 {} {}" font-family="{}">"##, width, h.round(), width, h.round(), FONT);
    let _ = write!(out, r##"<rect width="{}" height="{}" fill="#ffffff"/>"##, width, h.round());
    let _ = write!(out, r##"<text x="{}" y="20" font-size="14" fill="#1f2937" font-weight="bold">{}</text>"##, ml, esc(title));
    let xf = |x: f64| -> f64 {
        let r = if ax.log { (x.max(1e-300).log10() - ax.lo.log10()) / (ax.hi.log10() - ax.lo.log10()) } else { (x - ax.lo) / (ax.hi - ax.lo) };
        ml + r * pw
    };
    let xticks: Vec<f64> = if ax.log {
        let (a, b) = (ax.lo.log10().floor() as i32, ax.hi.log10().ceil() as i32);
        (a..=b).map(|k| 10f64.powi(k)).filter(|&v| v >= ax.lo * 0.999 && v <= ax.hi * 1.001).collect()
    } else {
        nice_ticks(ax.lo, ax.hi, 8)
    };
    // one colour per signal name, the same in every panel (gain and phase of one curve)
    let mut seen: Vec<String> = vec![];
    for (pi, p) in panels.iter().enumerate() {
        let top = mt + pi as f64 * (ph + gap);
        let (mut lo, mut hi) = (f64::INFINITY, f64::NEG_INFINITY);
        for t in &p.traces {
            for (k, &x) in t.x.iter().enumerate() {
                if x >= ax.lo && x <= ax.hi && t.lo[k].is_finite() && t.hi[k].is_finite() {
                    lo = lo.min(t.lo[k]);
                    hi = hi.max(t.hi[k]);
                }
            }
        }
        if !lo.is_finite() {
            lo = 0.0;
            hi = 1.0;
        }
        if hi - lo < 1e-12 * hi.abs().max(1e-12) {
            let d = hi.abs().max(1e-3) * 0.1;
            lo -= d;
            hi += d;
        }
        let pad = (hi - lo) * 0.06;
        let (lo, hi) = (lo - pad, hi + pad);
        let yf = |v: f64| top + ph - (v - lo) / (hi - lo) * ph;
        let _ = write!(out, r##"<rect x="{}" y="{}" width="{}" height="{}" fill="#fbfcfe" stroke="#cbd5e1" stroke-width="1"/>"##, ml, top, pw, ph);
        for &v in &nice_ticks(lo, hi, 5) {
            let y = yf(v);
            let _ = write!(out, r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e5e9f0" stroke-width="1"/><text x="{}" y="{}" font-size="11" fill="#64748b" text-anchor="end">{}</text>"##, ml, f(y), ml + pw, f(y), ml - 6.0, f(y + 4.0), esc(&si(v, &p.unit, 3)));
        }
        for &x in &xticks {
            let px = xf(x);
            let _ = write!(out, r##"<line x1="{}" y1="{}" x2="{}" y2="{}" stroke="#e5e9f0" stroke-width="1"/>"##, f(px), top, f(px), top + ph);
        }
        let mut legend_x = ml + 8.0;
        for t in &p.traces {
            let ci = seen.iter().position(|n| *n == t.name).unwrap_or_else(|| {
                seen.push(t.name.clone());
                seen.len() - 1
            });
            let col = COLORS[ci % COLORS.len()];
            let mut up = String::new();
            let mut dn = String::new();
            let mut band = false;
            for (k, &x) in t.x.iter().enumerate() {
                if x < ax.lo || x > ax.hi || !t.lo[k].is_finite() || !t.hi[k].is_finite() {
                    continue;
                }
                let px = xf(x);
                let (a, b) = (yf(t.hi[k]), yf(t.lo[k]));
                if (b - a).abs() > 1.2 {
                    band = true;
                }
                let _ = write!(up, "{}{},{}", if up.is_empty() { "M" } else { "L" }, f(px), f(a));
                dn = format!("L{},{}", f(px), f(b)) + &dn;
            }
            if up.is_empty() {
                continue;
            }
            if band {
                let _ = write!(out, r##"<path d="{}{}Z" fill="{}" fill-opacity="0.35" stroke="{}" stroke-width="0.8"/>"##, up, dn, col, col);
            } else {
                let _ = write!(out, r##"<path d="{}" fill="none" stroke="{}" stroke-width="1.5" stroke-linejoin="round"/>"##, up, col);
            }
            let lw = 22.0 + crate::circuit::text_w(&t.name) * 7.5;
            let _ = write!(out, r##"<rect x="{}" y="{}" width="{}" height="17" rx="3" fill="#ffffff" fill-opacity="0.85"/>"##, f(legend_x - 3.0), top + 2.0, f(lw));
            let _ = write!(out, r##"<rect x="{}" y="{}" width="12" height="3" fill="{}"/><text x="{}" y="{}" font-size="12" fill="{}">{}</text>"##, f(legend_x), top + 9.0, col, f(legend_x + 16.0), top + 14.0, col, esc(&t.name));
            legend_x += 30.0 + crate::circuit::text_w(&t.name) * 7.5;
        }
        if pi + 1 == panels.len() {
            for &x in &xticks {
                let px = xf(x);
                let _ = write!(out, r##"<text x="{}" y="{}" font-size="11" fill="#64748b" text-anchor="middle">{}</text>"##, f(px), top + ph + 16.0, esc(&si(x, &ax.unit, 3)));
            }
        }
    }
    out.push_str("</svg>");
    out
}

/* ------------------------------------------------------------------ rasterising */

fn fontdb() -> Arc<usvg::fontdb::Database> {
    static DB: OnceLock<Arc<usvg::fontdb::Database>> = OnceLock::new();
    DB.get_or_init(|| {
        let mut db = usvg::fontdb::Database::new();
        let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".into());
        for f in ["msjh.ttc", "msjhbd.ttc", "segoeui.ttf", "segoeuib.ttf", "arial.ttf", "consola.ttf", "seguisym.ttf"] {
            if let Ok(b) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join(f)) {
                db.load_font_data(b);
            }
        }
        db.set_sans_serif_family("Microsoft JhengHei");
        Arc::new(db)
    })
    .clone()
}

pub fn png(svg: &str) -> Result<Vec<u8>, String> {
    let mut opt = usvg::Options::default();
    opt.fontdb = fontdb();
    opt.font_family = "Microsoft JhengHei".into();
    let tree = usvg::Tree::from_str(svg, &opt).map_err(|e| format!("SVG：{}", e))?;
    let size = tree.size().to_int_size();
    let mut pm = tiny_skia::Pixmap::new(size.width().max(1), size.height().max(1)).ok_or("圖片太大")?;
    resvg::render(&tree, tiny_skia::Transform::default(), &mut pm.as_mut());
    pm.encode_png().map_err(|e| format!("PNG：{}", e))
}

pub fn base64(b: &[u8]) -> String {
    const T: &[u8; 64] = b"ABCDEFGHIJKLMNOPQRSTUVWXYZabcdefghijklmnopqrstuvwxyz0123456789+/";
    let mut s = String::with_capacity(b.len().div_ceil(3) * 4);
    for c in b.chunks(3) {
        let n = (c[0] as u32) << 16 | (*c.get(1).unwrap_or(&0) as u32) << 8 | *c.get(2).unwrap_or(&0) as u32;
        s.push(T[(n >> 18) as usize & 63] as char);
        s.push(T[(n >> 12) as usize & 63] as char);
        s.push(if c.len() > 1 { T[(n >> 6) as usize & 63] as char } else { '=' });
        s.push(if c.len() > 2 { T[n as usize & 63] as char } else { '=' });
    }
    s
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn base64_matches_rfc4648() {
        assert_eq!(base64(b""), "");
        assert_eq!(base64(b"f"), "Zg==");
        assert_eq!(base64(b"fo"), "Zm8=");
        assert_eq!(base64(b"foo"), "Zm9v");
        assert_eq!(base64(b"foobar"), "Zm9vYmFy");
    }
    #[test]
    fn ticks() {
        assert_eq!(nice_ticks(0.0, 10.0, 5), vec![0.0, 2.0, 4.0, 6.0, 8.0, 10.0]);
    }
}

#[cfg(test)]
mod render_tests {
    use super::*;
    #[test]
    fn schematic_and_plot_rasterise() {
        let b = crate::circuit::from_netlist(&crate::circuit::tests::buck_netlist()).unwrap();
        let svg = schematic_svg(&b.circuit.doc, &SchOpts { highlight: Some("SW".into()), pin_names: true, max_px: 900.0 });
        let p = png(&svg).expect("png");
        assert_eq!(&p[..4], b"\x89PNG");
        assert!(p.len() > 5000, "picture too small: {} bytes", p.len());
        let tr = Trace { name: "v".into(), x: vec![0.0, 1.0, 2.0], lo: vec![0.0, 1.0, 0.5], hi: vec![0.0, 2.0, 0.5] };
        let svg = plot_svg("t", &[Panel { traces: vec![tr], unit: "V".into() }], &Axis { lo: 0.0, hi: 2.0, log: false, unit: "s".into() }, 800.0);
        assert_eq!(&png(&svg).unwrap()[..4], b"\x89PNG");
    }
}
