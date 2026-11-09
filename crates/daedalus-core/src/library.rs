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
