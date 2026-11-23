//! Circuit document (.psimx, JSON) — compatible with files saved by the original web version.

use serde::{Deserialize, Deserializer, Serialize};
use std::collections::BTreeMap;

fn de_params<'de, D: Deserializer<'de>>(d: D) -> Result<BTreeMap<String, String>, D::Error> {
    let m: Option<BTreeMap<String, serde_json::Value>> = Option::deserialize(d)?;
    Ok(m.unwrap_or_default().into_iter().map(|(k, v)| (k, value_str(&v))).collect())
}
/// JavaScript String(value) for parameter values stored as JSON.
pub fn value_str(v: &serde_json::Value) -> String {
    match v {
        serde_json::Value::String(s) => s.clone(),
        serde_json::Value::Number(n) => n.as_f64().map(crate::util::js_num_str).unwrap_or_else(|| n.to_string()),
        serde_json::Value::Bool(b) => b.to_string(),
        serde_json::Value::Null => String::new(),
        other => other.to_string(),
    }
}
fn de_f64<'de, D: Deserializer<'de>>(d: D) -> Result<f64, D::Error> {
    let v: Option<serde_json::Value> = Option::deserialize(d)?;
    Ok(match v {
        Some(serde_json::Value::Number(n)) => n.as_f64().unwrap_or(0.0),
        Some(serde_json::Value::String(s)) => s.trim().parse().unwrap_or(0.0),
        _ => 0.0,
    })
}
fn de_i64<'de, D: Deserializer<'de>>(d: D) -> Result<i64, D::Error> {
    Ok(de_f64(d)? as i64)
}
fn de_bool<'de, D: Deserializer<'de>>(d: D) -> Result<bool, D::Error> {
    let v: Option<serde_json::Value> = Option::deserialize(d)?;
    Ok(match v {
        Some(serde_json::Value::Bool(b)) => b,
        Some(serde_json::Value::Number(n)) => n.as_f64().map(|x| x != 0.0).unwrap_or(false),
        Some(serde_json::Value::String(s)) => !s.is_empty(),
        _ => false,
    })
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Element {
    #[serde(deserialize_with = "de_i64", default)]
    pub id: i64,
    #[serde(rename = "type")]
    pub typ: String,
    #[serde(deserialize_with = "de_f64", default)]
    pub x: f64,
    #[serde(deserialize_with = "de_f64", default)]
    pub y: f64,
    #[serde(deserialize_with = "de_i64", default)]
    pub rot: i64,
    #[serde(deserialize_with = "de_bool", default)]
    pub mir: bool,
    #[serde(deserialize_with = "de_bool", default)]
    pub dis: bool,
    #[serde(default)]
    pub name: String,
    #[serde(deserialize_with = "de_params", default)]
    pub params: BTreeMap<String, String>,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub disp: Option<BTreeMap<String, bool>>,
}

#[derive(Serialize, Deserialize, Clone, Debug, PartialEq)]
pub struct Wire {
    #[serde(deserialize_with = "de_i64", default)]
    pub id: i64,
    #[serde(deserialize_with = "de_f64", default)]
    pub x1: f64,
    #[serde(deserialize_with = "de_f64", default)]
    pub y1: f64,
    #[serde(deserialize_with = "de_f64", default)]
    pub x2: f64,
    #[serde(deserialize_with = "de_f64", default)]
    pub y2: f64,
}

#[derive(Serialize, Deserialize, Clone, Debug, Default, PartialEq)]
pub struct Doc {
    #[serde(default)]
    pub elements: Vec<Element>,
    #[serde(default)]
    pub wires: Vec<Wire>,
    #[serde(deserialize_with = "de_i64", default)]
    pub nid: i64,
    /// subcircuit definitions by name (only in the main circuit; a definition's own map stays empty)
    #[serde(default, skip_serializing_if = "BTreeMap::is_empty")]
    pub subckts: BTreeMap<String, Doc>,
}

#[derive(Serialize, Deserialize, Clone, Debug)]
pub struct PsimFile {
    pub app: String,
    pub version: String,
    pub doc: Doc,
    #[serde(default, skip_serializing_if = "Option::is_none")]
    pub screens: Option<Vec<Vec<String>>>,
}

impl Doc {
    /// Parse a saved file (wrapped {app, doc, screens} or a bare document); fills missing
    /// parameters from the library defaults. Returns the document and the waveform screen layout.
    pub fn from_json(txt: &str) -> Result<(Doc, Option<Vec<Vec<String>>>), String> {
        let v: serde_json::Value = serde_json::from_str(txt).map_err(|e| format!("JSON 格式錯誤：{}", e))?;
        let (docv, screens) = if v.get("doc").is_some() {
            let screens = v.get("screens").and_then(|s| serde_json::from_value::<Vec<Vec<String>>>(s.clone()).ok());
            (v["doc"].clone(), screens)
        } else {
            (v, None)
        };
        if !docv.get("elements").map(|e| e.is_array()).unwrap_or(false) || !docv.get("wires").map(|e| e.is_array()).unwrap_or(false) {
            return Err("不是有效的電路檔".into());
        }
        let mut doc: Doc = serde_json::from_value(docv).map_err(|e| format!("不是有效的電路檔：{}", e))?;
        // definitions nested in a definition (not written by this program) join the main map
        let mut extra = BTreeMap::new();
        for d in doc.subckts.values_mut() {
            extra.append(&mut d.subckts);
        }
        for (k, v) in extra {
            doc.subckts.entry(k).or_insert(v);
        }
        doc.normalize();
        for d in doc.subckts.values_mut() {
            d.normalize();
        }
        doc.sync_subckt_ports();
        Ok((doc, screens))
    }
    /// drop unknown element types, fill missing parameters from the library defaults, fix the id counter
    fn normalize(&mut self) {
        let lib = crate::library::lib();
        self.elements.retain(|e| lib.get(&e.typ).is_some());
        for e in self.elements.iter_mut() {
            let mut p = lib.default_params(&e.typ);
            for (k, v) in std::mem::take(&mut e.params) {
                p.insert(k, v);
            }
            e.params = p;
        }
        let mx = self.elements.iter().map(|e| e.id).chain(self.wires.iter().map(|w| w.id)).max().unwrap_or(0);
        self.nid = self.nid.max(1).max(1 + mx.max(0));
    }
    /// pin lists of every subcircuit block (in the main circuit and in the definitions) from the ports of its
    /// definition; returns the number of blocks whose pins changed
    pub fn sync_subckt_ports(&mut self) -> usize {
        let ports: BTreeMap<String, String> = self.subckts.iter().map(|(k, d)| (k.clone(), crate::library::ports_str(&crate::subckt::def_ports(d)))).collect();
        let mut n = 0;
        let mut fix = |d: &mut Doc| {
            for i in 0..d.elements.len() {
                let e = &d.elements[i];
                if e.typ != "SUBCKT" {
                    continue;
                }
                let Some(p) = e.params.get("def").and_then(|k| ports.get(k)) else { continue };
                if e.params.get("ports") != Some(p) {
                    d.set_block_ports(i, p);
                    n += 1;
                }
            }
        };
        fix(self);
        for d in self.subckts.values_mut() {
            fix(d);
        }
        n
    }
    /// Give block `i` a new pin list. A pin keeps its name but may move (ports added, removed or renamed, the
    /// block wider or taller): wires that ended on a pin follow it, with a corner where needed, as when an
    /// element is dragged.
    pub fn set_block_ports(&mut self, i: usize, ports: &str) {
        use crate::library::{sub_geom, tpt};
        let world = |e: &Element| {
            let g = sub_geom(e);
            let [x0, y0, x1, _] = g.bb;
            g.pins
                .iter()
                .map(|p| {
                    // the direction away from the box, in world coordinates
                    let (dx, dy) = if p.x <= x0 { (-1.0, 0.0) } else if p.x >= x1 { (1.0, 0.0) } else if p.y <= y0 { (0.0, -1.0) } else { (0.0, 1.0) };
                    let a = tpt(e, p.x, p.y);
                    let b = tpt(e, p.x + dx, p.y + dy);
                    (p.n.clone(), a, (b.0 - a.0, b.1 - a.1))
                })
                .collect::<Vec<_>>()
        };
        let old = world(&self.elements[i]);
        self.elements[i].params.insert("ports".into(), ports.to_string());
        let new = world(&self.elements[i]);
        let key = |x: f64, y: f64| ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64);
        // old place -> (new place, outward direction)
        let moves: std::collections::HashMap<(i64, i64), ((f64, f64), (f64, f64))> = old
            .iter()
            .filter_map(|(n, o, _)| new.iter().find(|q| &q.0 == n).filter(|q| q.1 != *o).map(|q| (key(o.0, o.1), (q.1, q.2))))
            .collect();
        if moves.is_empty() {
            return;
        }
        let mut out = Vec::with_capacity(self.wires.len());
        let mut nid = self.nid;
        let mut seg = |out: &mut Vec<Wire>, id: Option<i64>, a: (f64, f64), b: (f64, f64)| {
            if a != b {
                let id = id.unwrap_or_else(|| {
                    nid += 1;
                    nid - 1
                });
                out.push(Wire { id, x1: a.0, y1: a.1, x2: b.0, y2: b.1 });
            }
        };
        for w in &self.wires {
            let m1 = moves.get(&key(w.x1, w.y1)).copied();
            let m2 = moves.get(&key(w.x2, w.y2)).copied();
            let (fixed, (to, dir)) = match (m1, m2) {
                (None, None) => {
                    out.push(w.clone());
                    continue;
                }
                (Some(a), Some(b)) => {
                    seg(&mut out, Some(w.id), a.0, b.0);
                    continue;
                }
                (Some(m), None) => ((w.x2, w.y2), m),
                (None, Some(m)) => ((w.x1, w.y1), m),
            };
            let horiz = w.y1 == w.y2;
            // the pin slid along the wire: just stretch it
            if (horiz && to.1 == fixed.1) || (!horiz && to.0 == fixed.0) {
                seg(&mut out, Some(w.id), fixed, to);
                continue;
            }
            // otherwise approach the pin from one grid unit outside the block, so the new path never runs over
            // another pin of the block (that would connect it)
            let stub = (to.0 + dir.0, to.1 + dir.1);
            let corner = if horiz { (stub.0, fixed.1) } else { (fixed.0, stub.1) };
            seg(&mut out, Some(w.id), fixed, corner);
            seg(&mut out, None, corner, stub);
            seg(&mut out, None, stub, to);
        }
        self.nid = nid;
        self.wires = out;
    }
    pub fn to_json(&self, version: &str, screens: Option<Vec<Vec<String>>>) -> String {
        let f = PsimFile { app: "psimweb".into(), version: version.into(), doc: self.clone(), screens };
        serde_json::to_string_pretty(&f).unwrap_or_default()
    }
    pub fn el_by_id(&self, id: i64) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
}
