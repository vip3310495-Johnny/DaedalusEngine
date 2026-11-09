//! Connectivity (nets, junctions, open pins) and netlist builder.

use crate::doc::{Doc, Element};
use crate::expr::{self, Syms};
use crate::library::{lib, wpins};
use serde::{Deserialize, Serialize};
use std::collections::{BTreeMap, HashMap, HashSet};

/// Netlist passed to the simulator.
#[derive(Clone, Debug, Default)]
pub struct Netlist {
    /// kind per net: 'P' power, 'C' control, 'G' ground
    pub nets: Vec<char>,
    pub elements: Vec<NetElement>,
    pub param_text: String,
}
#[derive(Clone, Debug)]
pub struct NetElement {
    pub id: i64,
    pub typ: String,
    pub name: String,
    pub params: HashMap<String, String>,
    /// net index per pin, -1 = unconnected
    pub pins: Vec<i64>,
}

/// JSON netlist as produced by the original buildNetlist (for cross-validation).
#[derive(Deserialize, Serialize)]
pub struct JsNetlist {
    pub nets: Vec<JsNet>,
    pub elements: Vec<JsEl>,
    #[serde(rename = "paramText", default)]
    pub param_text: String,
}
#[derive(Deserialize, Serialize)]
pub struct JsNet {
    pub kind: String,
}
#[derive(Deserialize, Serialize)]
pub struct JsEl {
    pub id: i64,
    #[serde(rename = "type")]
    pub typ: String,
    #[serde(default)]
    pub name: String,
    #[serde(default)]
    pub params: BTreeMap<String, serde_json::Value>,
    pub pins: Vec<i64>,
}
impl JsNetlist {
    pub fn to_netlist(&self) -> Netlist {
        Netlist {
            nets: self.nets.iter().map(|n| n.kind.chars().next().unwrap_or('C')).collect(),
            elements: self
                .elements
                .iter()
                .map(|e| NetElement { id: e.id, typ: e.typ.clone(), name: e.name.clone(), params: e.params.iter().map(|(k, v)| (k.clone(), crate::doc::value_str(v))).collect(), pins: e.pins.clone() })
                .collect(),
            param_text: self.param_text.clone(),
        }
    }
}

#[derive(Clone, Debug, PartialEq)]
pub struct NlError {
    pub msg: String,
    pub els: Vec<i64>,
}

type Key = (u64, u64);
#[inline]
fn key(x: f64, y: f64) -> Key {
    ((x + 0.0).to_bits(), (y + 0.0).to_bits())
}

/// Connectivity of a set of elements and all wires.
pub struct Conn {
    idx: HashMap<Key, usize>,
    par: Vec<usize>,
    pub junctions: Vec<(f64, f64)>,
    pub open: Vec<(f64, f64)>,
    kinds: HashMap<usize, HashSet<String>>,
    members: HashMap<usize, usize>,
    wire_members: HashMap<usize, usize>,
}
impl Conn {
    fn find_i(par: &mut [usize], mut i: usize) -> usize {
        while par[i] != i {
            par[i] = par[par[i]];
            i = par[i];
        }
        i
    }
    pub fn root(&self, x: f64, y: f64) -> Option<usize> {
        let mut i = *self.idx.get(&key(x, y))?;
        while self.par[i] != i {
            i = self.par[i];
        }
        Some(i)
    }
    pub fn is_ctl_root(&self, r: Option<usize>) -> bool {
        match r.and_then(|r| self.kinds.get(&r)) {
            Some(s) => !s.contains("p") && !s.contains("gnd") && (s.contains("ci") || s.contains("co") || s.contains("g")),
            None => false,
        }
    }
    pub fn is_gnd_root(&self, r: Option<usize>) -> bool {
        r.and_then(|r| self.kinds.get(&r)).map(|s| s.contains("gnd")).unwrap_or(false)
    }
    pub fn members(&self, r: usize) -> usize {
        self.members.get(&r).copied().unwrap_or(0)
    }
}

pub fn connectivity(doc: &Doc, els: &[&Element]) -> Conn {
    let mut idx: HashMap<Key, usize> = HashMap::new();
    let mut par: Vec<usize> = Vec::new();
    let mut cnt: HashMap<Key, u32> = HashMap::new();
    let add = |k: Key, idx: &mut HashMap<Key, usize>, par: &mut Vec<usize>| -> usize {
        *idx.entry(k).or_insert_with(|| {
            par.push(par.len());
            par.len() - 1
        })
    };
    let mut pts: Vec<(f64, f64)> = Vec::new();
    let mut el_pins = Vec::new();
    let mut order: Vec<Key> = Vec::new();
    let bump = |k: Key, cnt: &mut HashMap<Key, u32>, order: &mut Vec<Key>| {
        let c = cnt.entry(k).or_insert(0);
        if *c == 0 {
            order.push(k);
        }
        *c += 1;
    };
    for el in els {
        let wp = wpins(el);
        for p in &wp {
            let k = key(p.x, p.y);
            add(k, &mut idx, &mut par);
            bump(k, &mut cnt, &mut order);
            pts.push((p.x, p.y));
        }
        el_pins.push((*el, wp));
    }
    fn uni(par: &mut [usize], a: usize, b: usize) {
        let ra = Conn::find_i(par, a);
        let rb = Conn::find_i(par, b);
        if ra != rb {
            par[ra] = rb;
        }
    }
    for w in &doc.wires {
        let (a, b) = (key(w.x1, w.y1), key(w.x2, w.y2));
        let ia = add(a, &mut idx, &mut par);
        let ib = add(b, &mut idx, &mut par);
        uni(&mut par, ia, ib);
        bump(a, &mut cnt, &mut order);
        bump(b, &mut cnt, &mut order);
        pts.push((w.x1, w.y1));
        pts.push((w.x2, w.y2));
    }
    // T junctions: a point lying strictly inside a wire segment
    for w in &doc.wires {
        let (minx, maxx, miny, maxy) = (w.x1.min(w.x2), w.x1.max(w.x2), w.y1.min(w.y2), w.y1.max(w.y2));
        let mut tj: HashSet<Key> = HashSet::new();
        let ka = key(w.x1, w.y1);
        for &(x, y) in &pts {
            if (x == w.x1 && y == w.y1) || (x == w.x2 && y == w.y2) {
                continue;
            }
            let inside = if w.y1 == w.y2 { y == w.y1 && x > minx && x < maxx } else { x == w.x1 && y > miny && y < maxy };
            if inside {
                let k = key(x, y);
                if tj.insert(k) {
                    let ik = add(k, &mut idx, &mut par);
                    let iw = idx[&ka];
                    uni(&mut par, ik, iw);
                    bump(k, &mut cnt, &mut order);
                    bump(k, &mut cnt, &mut order);
                }
            }
        }
    }
    // labels
    let mut lbl: HashMap<String, usize> = HashMap::new();
    for (el, wp) in &el_pins {
        if el.typ == "LABEL" && !el.dis {
            let t = el.params.get("text").map(|s| s.trim().to_string()).unwrap_or_default();
            if t.is_empty() || wp.is_empty() {
                continue;
            }
            let k = idx[&key(wp[0].x, wp[0].y)];
            if let Some(&o) = lbl.get(&t) {
                uni(&mut par, k, o);
            } else {
                lbl.insert(t, k);
            }
        }
    }
    let mut junctions = Vec::new();
    for k in &order {
        if cnt[k] >= 3 {
            junctions.push((f64::from_bits(k.0), f64::from_bits(k.1)));
        }
    }
    let mut members: HashMap<usize, usize> = HashMap::new();
    for (_, wp) in &el_pins {
        for p in wp {
            let r = Conn::find_i(&mut par, idx[&key(p.x, p.y)]);
            *members.entry(r).or_insert(0) += 1;
        }
    }
    let mut wire_members: HashMap<usize, usize> = HashMap::new();
    for w in &doc.wires {
        let r = Conn::find_i(&mut par, idx[&key(w.x1, w.y1)]);
        *members.entry(r).or_insert(0) += 1;
        *wire_members.entry(r).or_insert(0) += 1;
    }
    let mut open = Vec::new();
    for (el, wp) in &el_pins {
        if el.typ == "LABEL" || el.typ == "GND" {
            continue;
        }
        for p in wp {
            let r = Conn::find_i(&mut par, idx[&key(p.x, p.y)]);
            if members.get(&r).copied().unwrap_or(0) <= 1 {
                open.push((p.x, p.y));
            }
        }
    }
    let mut kinds: HashMap<usize, HashSet<String>> = HashMap::new();
    for (el, wp) in &el_pins {
        if el.dis {
            continue;
        }
        for p in wp {
            let r = Conn::find_i(&mut par, idx[&key(p.x, p.y)]);
            kinds.entry(r).or_default().insert(p.k.clone());
        }
    }
    // flatten so that root() can walk without path compression
    for i in 0..par.len() {
        let r = Conn::find_i(&mut par, i);
        par[i] = r;
    }
    Conn { idx, par, junctions, open, kinds, members, wire_members }
}

pub struct BuildResult {
    pub net: Netlist,
    pub errors: Vec<NlError>,
    pub syms: Syms,
}

pub fn build_netlist(doc: &Doc) -> BuildResult {
    let lb = lib();
    let mut errors = Vec::new();
    let els: Vec<&Element> = doc.elements.iter().filter(|e| !e.dis).collect();
    let c = connectivity(doc, &els);
    struct N {
        kinds: HashSet<String>,
        pins: Vec<(i64, usize, String)>,
        kind: Option<char>,
    }
    let mut nets: Vec<N> = Vec::new();
    let mut net_of: HashMap<usize, usize> = HashMap::new();
    let mut root_of_net: Vec<usize> = Vec::new();
    let mut elems: Vec<(&Element, Vec<i64>)> = Vec::new();
    for el in &els {
        let d = match lb.get(&el.typ) {
            Some(d) => d,
            None => continue,
        };
        if d.nosim() {
            continue;
        }
        let wp = wpins(el);
        let mut pins = Vec::new();
        for (i, p) in wp.iter().enumerate() {
            let r = c.root(p.x, p.y).unwrap();
            let ni = *net_of.entry(r).or_insert_with(|| {
                nets.push(N { kinds: HashSet::new(), pins: vec![], kind: None });
                root_of_net.push(r);
                nets.len() - 1
            });
            nets[ni].kinds.insert(p.k.clone());
            nets[ni].pins.push((el.id, i, p.k.clone()));
            pins.push(ni as i64);
        }
        elems.push((el, pins));
    }
    let has_ctl = |s: &HashSet<String>| s.contains("ci") || s.contains("co") || s.contains("g");
    for n in nets.iter_mut() {
        n.kind = if n.kinds.contains("gnd") {
            Some('G')
        } else if n.kinds.contains("p") && has_ctl(&n.kinds) {
            Some('X')
        } else if n.kinds.contains("p") {
            Some('P')
        } else if has_ctl(&n.kinds) {
            Some('C')
        } else {
            None
        };
    }
    // propagate power domain through 'any' pins of the same element
    let mut ch = true;
    while ch {
        ch = false;
        for (el, pins) in &elems {
            let ps = lb.pins_for(el);
            let any_idx: Vec<usize> = ps.iter().enumerate().filter(|(_, p)| p.k == "a").map(|(i, _)| i).collect();
            if any_idx.is_empty() {
                continue;
            }
            let has_p = any_idx.iter().any(|&i| nets[pins[i] as usize].kind == Some('P'));
            if has_p {
                for &i in &any_idx {
                    let n = &mut nets[pins[i] as usize];
                    if n.kind.is_none() {
                        n.kind = Some('P');
                        ch = true;
                    }
                }
            }
        }
    }
    for n in nets.iter_mut() {
        if n.kind.is_none() {
            n.kind = Some('C');
        }
    }
    let name_of = |id: i64| -> String {
        doc.el_by_id(id).map(|e| if e.name.is_empty() { e.typ.clone() } else { e.name.clone() }).unwrap_or_default()
    };
    for n in &nets {
        if n.kind == Some('X') {
            let mut ids: Vec<i64> = Vec::new();
            for p in &n.pins {
                if !ids.contains(&p.0) {
                    ids.push(p.0);
                }
            }
            let names: Vec<String> = ids.iter().map(|&i| name_of(i)).collect();
            errors.push(NlError { msg: format!("功率電路與控制電路直接相連（{}）。請透過感測器、開關控制器或 Control-Power Interface 連接。", names.join("、")), els: ids });
        }
    }
    let net_list: Vec<char> = nets.iter().map(|n| if n.kind == Some('X') { 'P' } else { n.kind.unwrap() }).collect();
    // dangling pins: net with a single pin and no wires/labels
    let mut out_els = Vec::new();
    for (el, pins) in &elems {
        let pins: Vec<i64> = pins
            .iter()
            .map(|&ni| {
                let n = &nets[ni as usize];
                let wm = c.wire_members.get(&root_of_net[ni as usize]).copied().unwrap_or(0);
                if n.pins.len() <= 1 && wm == 0 && n.kind != Some('G') {
                    -1
                } else {
                    ni
                }
            })
            .collect();
        out_els.push(NetElement { id: el.id, typ: el.typ.clone(), name: el.name.clone(), params: el.params.iter().map(|(k, v)| (k.clone(), v.clone())).collect(), pins });
    }
    // checks
    let sc: Vec<i64> = doc.elements.iter().filter(|e| e.typ == "SIMCTRL" && !e.dis).map(|e| e.id).collect();
    if sc.len() > 1 {
        errors.push(NlError { msg: "只能有一個 Simulation Control 元件。".into(), els: sc });
    }
    let mut names: HashMap<String, i64> = HashMap::new();
    for e in &doc.elements {
        if e.dis || lb.get(&e.typ).map(|d| d.noname()).unwrap_or(true) {
            continue;
        }
        if let Some(&o) = names.get(&e.name) {
            errors.push(NlError { msg: format!("元件名稱重複：{}", e.name), els: vec![e.id, o] });
        } else {
            names.insert(e.name.clone(), e.id);
        }
    }
    let mut param_text = String::new();
    for e in &els {
        if e.typ == "PARAMS" {
            param_text += e.params.get("text").map(|s| s.as_str()).unwrap_or("");
            param_text.push('\n');
        }
    }
    let (syms, perr) = expr::parse_param_text(&param_text);
    let pids: Vec<i64> = els.iter().filter(|e| e.typ == "PARAMS").map(|e| e.id).collect();
    for m in perr {
        errors.push(NlError { msg: format!("參數檔：{}", m), els: pids.clone() });
    }
    BuildResult { net: Netlist { nets: net_list, elements: out_els, param_text }, errors, syms }
}

/// Parameter-file symbols of a document (enabled PARAMS elements).
pub fn doc_syms(doc: &Doc) -> Syms {
    let mut txt = String::new();
    for e in &doc.elements {
        if e.typ == "PARAMS" && !e.dis {
            txt += e.params.get("text").map(|s| s.as_str()).unwrap_or("");
            txt.push('\n');
        }
    }
    expr::parse_param_text(&txt).0
}
