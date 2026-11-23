//! Element library: symbols, pins, parameters, descriptions (exported from the original library).

use crate::doc::Element;
use serde::Deserialize;
use std::collections::{BTreeMap, HashMap};
use std::sync::OnceLock;

#[derive(Deserialize, Clone, Debug)]
pub struct PinDef {
    pub x: f64,
    pub y: f64,
    /// p = power, ci = control input, co = control output, g = gate, a = any, gnd, lbl
    pub k: String,
    pub n: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct ParamDef {
    pub k: String,
    pub l: String,
    pub d: String,
    #[serde(default)]
    pub u: String,
    #[serde(default)]
    pub desc: String,
    #[serde(rename = "type", default)]
    pub typ: Option<String>,
    #[serde(default)]
    pub opts: Option<Vec<(String, String)>>,
    #[serde(default)]
    pub adv: bool,
    #[serde(rename = "idealOnly", default)]
    pub ideal_only: bool,
    #[serde(rename = "presetKind", default)]
    pub preset_kind: Option<String>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct SymText {
    pub x: f64,
    pub y: f64,
    pub s: String,
    pub sz: f64,
    pub a: String,
}
#[derive(Deserialize, Clone, Debug)]
pub struct SymDef {
    pub g: String,
    pub t: Vec<SymText>,
}
#[derive(Deserialize, Clone, Debug)]
pub struct ElDef {
    #[serde(rename = "type")]
    pub typ: String,
    pub name: String,
    pub zh: String,
    pub pre: String,
    pub cat: String,
    pub pins: Vec<PinDef>,
    pub bb: [f64; 4],
    pub show: Vec<String>,
    pub flags: Vec<String>,
    pub params: Vec<ParamDef>,
    pub desc: String,
    pub sym: SymDef,
}
impl ElDef {
    pub fn has(&self, f: &str) -> bool {
        self.flags.iter().any(|x| x == f)
    }
    pub fn nosim(&self) -> bool {
        self.has("nosim")
    }
    pub fn noname(&self) -> bool {
        self.has("noname")
    }
    pub fn param(&self, k: &str) -> Option<&ParamDef> {
        self.params.iter().find(|p| p.k == k)
    }
}

pub struct Library {
    pub defs: Vec<ElDef>,
    by_type: HashMap<String, usize>,
}

static LIB: OnceLock<Library> = OnceLock::new();
pub fn lib() -> &'static Library {
    LIB.get_or_init(|| {
        let defs: Vec<ElDef> = serde_json::from_str(include_str!("../assets/library.json")).expect("library.json");
        let by_type = defs.iter().enumerate().map(|(i, d)| (d.typ.clone(), i)).collect();
        Library { defs, by_type }
    })
}

fn js_plus(s: Option<&String>) -> f64 {
    // JS: +((el.params && el.params.nin) || 1)
    match s {
        None => 1.0,
        Some(v) if v.is_empty() => 1.0,
        Some(v) => {
            let t = v.trim();
            if t.is_empty() {
                0.0
            } else {
                t.parse::<f64>().unwrap_or(f64::NAN)
            }
        }
    }
}

impl Library {
    pub fn get(&self, t: &str) -> Option<&ElDef> {
        self.by_type.get(t).map(|&i| &self.defs[i])
    }
    pub fn default_params(&self, t: &str) -> BTreeMap<String, String> {
        self.get(t).map(|d| d.params.iter().map(|p| (p.k.clone(), p.d.clone())).collect()).unwrap_or_default()
    }
    /// C-block pins for n inputs, m outputs
    pub fn cb_pins(n: f64, m: f64) -> Vec<PinDef> {
        let mut out = Vec::new();
        let cnt = |v: f64| if v > 0.0 { v.ceil() as usize } else { 0 };
        let tn = -((n - 1.0) / 2.0).floor();
        let tm = -((m - 1.0) / 2.0).floor();
        for i in 0..cnt(n) {
            out.push(PinDef { x: -2.0, y: tn + i as f64, k: "ci".into(), n: format!("x{}", i + 1) });
        }
        for i in 0..cnt(m) {
            out.push(PinDef { x: 2.0, y: tm + i as f64, k: "co".into(), n: format!("y{}", i + 1) });
        }
        out
    }
    pub fn pins_for(&self, el: &Element) -> Vec<PinDef> {
        if el.typ == "CBLOCK" {
            return Self::cb_pins(js_plus(el.params.get("nin")), js_plus(el.params.get("nout")));
        }
        if el.typ == "SUBCKT" {
            return sub_geom(el).pins;
        }
        self.get(&el.typ).map(|d| d.pins.clone()).unwrap_or_default()
    }
}

/// local -> world coordinates (rotation in 90° steps, optional mirror)
pub fn tpt(el: &Element, x: f64, y: f64) -> (f64, f64) {
    let x = if el.mir { -x } else { x };
    let (rx, ry) = match el.rot & 3 {
        0 => (el.x + x, el.y + y),
        1 => (el.x - y, el.y + x),
        2 => (el.x - x, el.y - y),
        _ => (el.x + y, el.y - x),
    };
    (rx + 0.0, ry + 0.0)
}

#[derive(Clone, Debug)]
pub struct WPin {
    pub x: f64,
    pub y: f64,
    pub k: String,
    pub n: String,
}
pub fn wpins(el: &Element) -> Vec<WPin> {
    lib()
        .pins_for(el)
        .into_iter()
        .map(|p| {
            let (x, y) = tpt(el, p.x, p.y);
            WPin { x, y, k: p.k, n: p.n }
        })
        .collect()
}

/* ------------------------------------------------------------------ subcircuit blocks */

/// one pin of a subcircuit block: the name of its port, the side of the block (L, R, T, B) and the pin kind
/// (what the port connects to inside: p power, ci control input, co control output, a either)
#[derive(Clone, Debug, PartialEq)]
pub struct SubPort {
    pub name: String,
    pub side: char,
    pub kind: String,
}

/// the pin list a subcircuit block keeps in its `ports` parameter: [["in","L","p"],["out","R","co"]]
pub fn parse_ports(s: &str) -> Vec<SubPort> {
    let v: Vec<Vec<String>> = serde_json::from_str(s).unwrap_or_default();
    v.into_iter()
        .take(200)
        .filter(|q| !q.is_empty())
        .map(|q| SubPort {
            name: q[0].clone(),
            side: q.get(1).and_then(|s| s.chars().next()).filter(|c| "LRTB".contains(*c)).unwrap_or('L'),
            kind: q.get(2).filter(|k| matches!(k.as_str(), "p" | "ci" | "co" | "a")).cloned().unwrap_or_else(|| "a".into()),
        })
        .collect()
}
pub fn ports_str(p: &[SubPort]) -> String {
    let v: Vec<[String; 3]> = p.iter().map(|q| [q.name.clone(), q.side.to_string(), q.kind.clone()]).collect();
    serde_json::to_string(&v).unwrap_or_default()
}

/// approximate width of a text in grid units (CJK characters are about twice as wide as Latin ones)
pub fn text_units(s: &str, sz: f64) -> f64 {
    s.chars().map(|c| if c.is_ascii() { 0.56 } else { 1.0 }).sum::<f64>() * sz
}

/// geometry of a subcircuit block, in element coordinates
#[derive(Clone, Debug)]
pub struct SubGeom {
    pub pins: Vec<PinDef>,
    /// bounding box including the pin stubs
    pub bb: [f64; 4],
    /// the drawn box
    pub body: [f64; 4],
    /// port names and the definition name: (x, y, text, size, anchor)
    pub texts: Vec<(f64, f64, String, f64, &'static str)>,
}
pub const SUB_PORT_SZ: f64 = 0.42;
pub const SUB_NAME_SZ: f64 = 0.6;

/// Pins on the four sides of the box, one grid unit apart (left and right side top to bottom, top and bottom
/// side left to right), centred like the C block's pins; the box is wide enough for the port names and the
/// definition name between them.
pub fn sub_geom(el: &Element) -> SubGeom {
    let ports = parse_ports(el.params.get("ports").map(|s| s.as_str()).unwrap_or(""));
    let def = el.params.get("def").cloned().unwrap_or_default();
    let side = |c: char| ports.iter().filter(|p| p.side == c).collect::<Vec<_>>();
    let (l, r, t, b) = (side('L'), side('R'), side('T'), side('B'));
    let start = |n: usize| -(((n as f64) - 1.0) / 2.0).floor();
    let reach = |n: usize| (((n as f64) - 1.0) / 2.0).ceil().max(0.0);
    let nv = l.len().max(r.len()).max(1);
    let nh = t.len().max(b.len()).max(1);
    let wmax = |v: &Vec<&SubPort>| v.iter().map(|p| text_units(&p.name, SUB_PORT_SZ)).fold(0.0, f64::max);
    let name_w = text_units(&def, SUB_NAME_SZ);
    let inner_w = (wmax(&l) + wmax(&r) + name_w + 1.2).max(2.8);
    let mut ht = (reach(nv) + 1.0).max(2.0);
    if !t.is_empty() {
        ht += 1.0;
    }
    if !b.is_empty() {
        ht += 1.0;
    }
    // the top and bottom pins need room too, and their names must not run into each other
    let tw = t.iter().chain(b.iter()).map(|p| text_units(&p.name, SUB_PORT_SZ)).fold(0.0, f64::max);
    let step = if nh > 1 && tw > 0.9 { (tw + 0.3).ceil() } else { 1.0 };
    let hw = ((inner_w / 2.0 + 0.6).ceil()).max(reach(nh) * step + 1.0).max(2.0);
    let (bx, by) = (hw - 0.6, ht - 0.6);
    let mut pins = Vec::new();
    let mut texts = Vec::new();
    for (i, p) in l.iter().enumerate() {
        let y = start(l.len()) + i as f64;
        pins.push(PinDef { x: -hw, y, k: p.kind.clone(), n: p.name.clone() });
        texts.push((-bx + 0.2, y + 0.15, p.name.clone(), SUB_PORT_SZ, "start"));
    }
    for (i, p) in r.iter().enumerate() {
        let y = start(r.len()) + i as f64;
        pins.push(PinDef { x: hw, y, k: p.kind.clone(), n: p.name.clone() });
        texts.push((bx - 0.2, y + 0.15, p.name.clone(), SUB_PORT_SZ, "end"));
    }
    for (i, p) in t.iter().enumerate() {
        let x = (start(t.len()) + i as f64) * step;
        pins.push(PinDef { x, y: -ht, k: p.kind.clone(), n: p.name.clone() });
        texts.push((x, -by + 0.5, p.name.clone(), SUB_PORT_SZ, "middle"));
    }
    for (i, p) in b.iter().enumerate() {
        let x = (start(b.len()) + i as f64) * step;
        pins.push(PinDef { x, y: ht, k: p.kind.clone(), n: p.name.clone() });
        texts.push((x, by - 0.2, p.name.clone(), SUB_PORT_SZ, "middle"));
    }
    texts.push((0.0, 0.2, if def.is_empty() { "?".into() } else { def }, SUB_NAME_SZ, "middle"));
    SubGeom { pins, bb: [-hw, -ht, hw, ht], body: [-bx, -by, bx, by], texts }
}

/// bounding box of an element in element coordinates (the library box, widened for blocks whose size depends
/// on their parameters)
pub fn local_bb(el: &Element) -> [f64; 4] {
    if el.typ == "SUBCKT" {
        return sub_geom(el).bb;
    }
    lib().get(&el.typ).map(|d| d.bb).unwrap_or([-1.0, -1.0, 1.0, 1.0])
}
