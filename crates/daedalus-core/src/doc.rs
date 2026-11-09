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
        let lib = crate::library::lib();
        doc.elements.retain(|e| lib.get(&e.typ).is_some());
        for e in doc.elements.iter_mut() {
            let mut p = lib.default_params(&e.typ);
            for (k, v) in std::mem::take(&mut e.params) {
                p.insert(k, v);
            }
            e.params = p;
        }
        let mx = doc.elements.iter().map(|e| e.id).chain(doc.wires.iter().map(|w| w.id)).max().unwrap_or(0);
        doc.nid = doc.nid.max(1).max(1 + mx.max(0));
        Ok((doc, screens))
    }
    pub fn to_json(&self, version: &str, screens: Option<Vec<Vec<String>>>) -> String {
        let f = PsimFile { app: "psimweb".into(), version: version.into(), doc: self.clone(), screens };
        serde_json::to_string_pretty(&f).unwrap_or_default()
    }
    pub fn el_by_id(&self, id: i64) -> Option<&Element> {
        self.elements.iter().find(|e| e.id == id)
    }
}
