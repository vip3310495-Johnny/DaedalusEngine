//! Circuit files for the AI tools: loading and saving .psimx, pin references ("Q1.G"), nets with names, static
//! checks, automatic layout of a netlist, and editing operations whose effect on the connectivity is verified
//! with the simulator's own connectivity rules (shared points, T junctions, labels with the same text).
use daedalus_core::doc::{Doc, Element, Wire};
use daedalus_core::library::{lib, tpt, wpins, ElDef, PinDef};
use daedalus_core::netlist::{build_netlist, connectivity};
use serde_json::Value;
use std::collections::{BTreeMap, HashMap, HashSet};
use std::path::Path;

/// length of the wire from a pin to its net label or ground symbol
pub const STUB: f64 = 2.0;
const GROUND: [&str; 5] = ["0", "gnd", "GND", "Gnd", "地"];

pub fn is_ground(n: &str) -> bool {
    GROUND.contains(&n.trim())
}

pub struct Circuit {
    pub doc: Doc,
    pub screens: Option<Vec<Vec<String>>>,
    /// net names given by the AI that have no label in the drawing (name -> pins of the net); kept in the file
    /// as "nets" so later edits can still say "SW"; the program itself does not keep it when it saves
    pub nets: BTreeMap<String, Vec<String>>,
}

fn fix_nid(doc: &mut Doc) {
    let m = doc.elements.iter().map(|e| e.id).chain(doc.wires.iter().map(|w| w.id)).max().unwrap_or(0);
    if doc.nid <= m {
        doc.nid = m + 1;
    }
}

impl Circuit {
    pub fn empty() -> Circuit {
        Circuit { doc: Doc { elements: vec![], wires: vec![], nid: 1 }, screens: None, nets: BTreeMap::new() }
    }
    pub fn load(path: &Path) -> Result<Circuit, String> {
        let txt = std::fs::read_to_string(path).map_err(|e| format!("無法讀取 {}：{}", path.display(), e))?;
        let (mut doc, screens) = Doc::from_json(&txt).map_err(|e| format!("{} 不是有效的電路檔：{}", path.display(), e))?;
        fix_nid(&mut doc);
        let mut nets = BTreeMap::new();
        if let Ok(v) = serde_json::from_str::<Value>(&txt) {
            if let Some(o) = v.get("nets").and_then(|x| x.as_object()) {
                for (k, p) in o {
                    let pins: Vec<String> = match p {
                        Value::Array(a) => a.iter().filter_map(|x| x.as_str().map(String::from)).collect(),
                        Value::String(s) => vec![s.clone()],
                        _ => vec![],
                    };
                    if !pins.is_empty() {
                        nets.insert(k.clone(), pins);
                    }
                }
            }
        }
        Ok(Circuit { doc, screens, nets })
    }
    /// written to a temporary file first, so a DaedalusEngine window that reloads the file never sees half of it
    pub fn save(&self, path: &Path) -> Result<(), String> {
        if let Some(dir) = path.parent() {
            if !dir.as_os_str().is_empty() {
                std::fs::create_dir_all(dir).map_err(|e| format!("無法建立資料夾 {}：{}", dir.display(), e))?;
            }
        }
        let mut txt = self.doc.to_json(daedalus_core::VERSION, self.screens.clone());
        // the AI's net names, refreshed to the pins the nets have now
        let a = self.analyze();
        let mut names: BTreeMap<String, Vec<String>> = BTreeMap::new();
        for (name, pins) in &self.nets {
            if let Some(i) = pins.iter().find_map(|p| a.pin_net.get(p).copied()) {
                if a.nets[i].labels.is_empty() && !a.nets[i].ground {
                    names.insert(name.clone(), a.nets[i].pins.clone());
                }
            }
        }
        if !names.is_empty() {
            if let Ok(mut v) = serde_json::from_str::<Value>(&txt) {
                v["nets"] = serde_json::json!(names);
                txt = serde_json::to_string_pretty(&v).unwrap_or(txt);
            }
        }
        let tmp = path.with_extension("psimx.tmp");
        if std::fs::write(&tmp, &txt).is_ok() && std::fs::rename(&tmp, path).is_ok() {
            return Ok(());
        }
        let _ = std::fs::remove_file(&tmp);
        std::fs::write(path, txt).map_err(|e| format!("無法寫入 {}：{}", path.display(), e))
    }
    /// the analysis with the AI's net names for nets that have no label
    pub fn analyze(&self) -> Analysis {
        analyze_named(&self.doc, &self.nets)
    }
    fn new_id(&mut self) -> i64 {
        let id = self.doc.nid;
        self.doc.nid += 1;
        id
    }
    /// a new element with the library defaults, then `params` on top
    pub fn add_element(&mut self, typ: &str, name: &str, x: f64, y: f64, rot: i64, mir: bool, params: &BTreeMap<String, String>) -> usize {
        let mut p = lib().default_params(typ);
        for (k, v) in params {
            p.insert(k.clone(), v.clone());
        }
        let id = self.new_id();
        self.doc.elements.push(Element { id, typ: typ.into(), x, y, rot: rot & 3, mir, dis: false, name: name.into(), params: p, disp: None });
        self.doc.elements.len() - 1
    }
    pub fn add_wire(&mut self, a: (f64, f64), b: (f64, f64)) {
        if a == b {
            return;
        }
        let id = self.new_id();
        self.doc.wires.push(Wire { id, x1: a.0, y1: a.1, x2: b.0, y2: b.1 });
    }
}

/* ------------------------------------------------------------------ names, pins, geometry */

pub fn def(typ: &str) -> Option<&'static ElDef> {
    lib().get(typ)
}

/// the name used in pin references and messages
pub fn el_name(e: &Element) -> String {
    if e.name.is_empty() {
        format!("{}#{}", e.typ, e.id)
    } else {
        e.name.clone()
    }
}

pub fn find_el(doc: &Doc, name: &str) -> Result<usize, String> {
    let n = name.trim();
    if let Some(i) = doc.elements.iter().position(|e| e.name == n).or_else(|| doc.elements.iter().position(|e| !e.name.is_empty() && e.name.eq_ignore_ascii_case(n))) {
        return Ok(i);
    }
    if let Some(i) = doc.elements.iter().position(|e| el_name(e).eq_ignore_ascii_case(n)) {
        return Ok(i);
    }
    let mut names: Vec<String> = doc.elements.iter().filter(|e| !e.name.is_empty()).map(|e| e.name.clone()).collect();
    names.sort();
    Err(format!("找不到元件「{}」。現有元件：{}", n, names.join("、")))
}

/// "Q1.G" -> (element index, pin, world position)
pub fn pin_of(doc: &Doc, r: &str) -> Result<(usize, PinDef, (f64, f64)), String> {
    let (en, pn) = r.trim().rsplit_once('.').ok_or_else(|| format!("接腳的寫法是「元件名稱.接腳名稱」，例如 Q1.G：{}", r))?;
    let i = find_el(doc, en)?;
    let e = &doc.elements[i];
    let pins = lib().pins_for(e);
    let p = pins
        .iter()
        .find(|p| p.n == pn)
        .or_else(|| pins.iter().find(|p| p.n.eq_ignore_ascii_case(pn)))
        .ok_or_else(|| format!("{}（{}）沒有接腳「{}」。接腳：{}", en, e.typ, pn, pins.iter().map(|p| p.n.as_str()).collect::<Vec<_>>().join("、")))?;
    let w = tpt(e, p.x, p.y);
    Ok((i, p.clone(), w))
}

/// symbol box in element coordinates (label and text boxes follow their text; C blocks their pins)
pub fn local_bb(el: &Element) -> [f64; 4] {
    let d = match def(&el.typ) {
        Some(d) => d,
        None => return [-1.0, -1.0, 1.0, 1.0],
    };
    let [x0, mut y0, mut x1, mut y1] = d.bb;
    match el.typ.as_str() {
        "LABEL" => x1 = label_width(el.params.get("text").map(|s| s.as_str()).unwrap_or("")),
        "TEXT" => {
            let t = el.params.get("text").cloned().unwrap_or_default();
            let lines: Vec<&str> = t.split('\n').collect();
            x1 = (lines.iter().map(|l| text_w(l)).fold(0.0, f64::max) + 0.4).max(1.0);
            y1 = 0.4 + (lines.len() as f64 - 1.0) * 1.1;
        }
        "CBLOCK" => {
            let pins = lib().pins_for(el);
            let n = pins.iter().filter(|p| p.k == "ci").count().max(1) as f64;
            let m = pins.iter().filter(|p| p.k == "co").count().max(1) as f64;
            let hh = n.max(m) / 2.0 + 0.4;
            y0 = -hh;
            y1 = hh;
        }
        _ => {}
    }
    [x0, y0, x1, y1]
}

/// width of text in grid units (CJK characters are about twice as wide)
pub fn text_w(s: &str) -> f64 {
    s.chars().map(|c| if (c as u32) > 0x2E80 { 1.0 } else { 0.62 }).sum()
}

pub fn label_width(t: &str) -> f64 {
    (text_w(t) + 0.9).max(1.6)
}

/// box of the element in world coordinates
pub fn world_box(el: &Element) -> [f64; 4] {
    let [x0, y0, x1, y1] = local_bb(el);
    let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
    for (x, y) in [(x0, y0), (x1, y0), (x0, y1), (x1, y1)] {
        let (u, v) = tpt(el, x, y);
        b[0] = b[0].min(u);
        b[1] = b[1].min(v);
        b[2] = b[2].max(u);
        b[3] = b[3].max(v);
    }
    b
}

/// unit vector from the pin away from the element body, in world coordinates
pub fn pin_dir(el: &Element, p: &PinDef) -> (f64, f64) {
    let [x0, y0, x1, y1] = local_bb(el);
    let (dx, dy) = (p.x - (x0 + x1) / 2.0, p.y - (y0 + y1) / 2.0);
    let (lx, ly) = if dx == 0.0 && dy == 0.0 {
        (0.0, 1.0)
    } else if dx.abs() >= dy.abs() {
        (dx.signum(), 0.0)
    } else {
        (0.0, dy.signum())
    };
    let a = tpt(el, p.x, p.y);
    let b = tpt(el, p.x + lx, p.y + ly);
    (b.0 - a.0, b.1 - a.1)
}

/// name and shown parameters drawn next to the element (as the program does)
pub fn label_lines(el: &Element) -> Vec<String> {
    let d = match def(&el.typ) {
        Some(d) => d,
        None => return vec![],
    };
    let disp = el.disp.clone().unwrap_or_default();
    let mut out = vec![];
    if !d.noname() && disp.get("name").copied() != Some(false) && !el.name.is_empty() {
        out.push(el.name.clone());
    }
    for p in &d.params {
        let on = disp.get(&p.k).copied().unwrap_or_else(|| d.show.contains(&p.k));
        let v = el.params.get(&p.k).cloned().unwrap_or_default();
        if on && !(v.trim().is_empty() && p.typ.as_deref() == Some("code")) {
            let numeric = !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || "-+.eE".contains(c));
            let u = if !p.u.is_empty() && numeric { p.u.clone() } else { String::new() };
            out.push(format!("{}={}{}", p.k, v, u));
        }
    }
    out
}

/// rotation of a net label whose flag points along `d` (right 0, down 1, left 2, up 3); note f64::signum(0.0) is
/// +1, so the components are compared, not their signs
fn dir_rot_label(d: (f64, f64)) -> i64 {
    if d.0 > 0.5 {
        0
    } else if d.1 > 0.5 {
        1
    } else if d.0 < -0.5 {
        2
    } else {
        3
    }
}
/// rotation of a ground symbol drawn along `d` (down 0, left 1, up 2, right 3)
fn dir_rot_gnd(d: (f64, f64)) -> i64 {
    if d.1 > 0.5 {
        0
    } else if d.0 < -0.5 {
        1
    } else if d.1 < -0.5 {
        2
    } else {
        3
    }
}

/* ------------------------------------------------------------------ nets and checks */

#[derive(Clone, Debug, Default)]
pub struct Net {
    pub name: String,
    pub ground: bool,
    pub control: bool,
    pub labels: Vec<String>,
    /// "Q1.G"
    pub pins: Vec<String>,
    pub kinds: Vec<String>,
}

#[derive(Default)]
pub struct Analysis {
    pub nets: Vec<Net>,
    pub pin_net: BTreeMap<String, usize>,
    /// connectivity root (netlist::connectivity) -> net index
    pub root_net: HashMap<usize, usize>,
    pub problems: Vec<String>,
    pub warnings: Vec<String>,
}

impl Analysis {
    pub fn net_of(&self, pin: &str) -> Option<&Net> {
        self.pin_net.get(pin).map(|&i| &self.nets[i])
    }
    pub fn net_named(&self, name: &str) -> Option<usize> {
        if is_ground(name) {
            return self.nets.iter().position(|n| n.ground);
        }
        self.nets.iter().position(|n| n.name == name || n.labels.iter().any(|l| l == name))
    }
}

pub fn analyze(doc: &Doc) -> Analysis {
    let els: Vec<&Element> = doc.elements.iter().filter(|e| !e.dis).collect();
    let c = connectivity(doc, &els);
    let mut a = Analysis::default();
    let mut by_root: HashMap<usize, usize> = HashMap::new();
    // every ground symbol is the same node (0 V), though each is its own connectivity root
    let mut ground: Option<usize> = None;
    let mut get = |r: usize, nets: &mut Vec<Net>| -> usize {
        if let Some(&i) = by_root.get(&r) {
            return i;
        }
        let gnd = c.is_gnd_root(Some(r));
        let i = match (gnd, ground) {
            (true, Some(g)) => g,
            _ => {
                nets.push(Net { ground: gnd, control: c.is_ctl_root(Some(r)), ..Default::default() });
                if gnd {
                    ground = Some(nets.len() - 1);
                }
                nets.len() - 1
            }
        };
        by_root.insert(r, i);
        i
    };
    for e in &els {
        if e.typ != "LABEL" {
            continue;
        }
        let t = e.params.get("text").map(|s| s.trim().to_string()).unwrap_or_default();
        if let (false, Some(r)) = (t.is_empty(), c.root(e.x, e.y)) {
            let ni = get(r, &mut a.nets);
            if !a.nets[ni].labels.contains(&t) {
                a.nets[ni].labels.push(t);
            }
        }
    }
    for e in &els {
        if e.typ == "LABEL" || e.typ == "GND" {
            continue;
        }
        for p in wpins(e) {
            if let Some(r) = c.root(p.x, p.y) {
                let ni = get(r, &mut a.nets);
                let pr = format!("{}.{}", el_name(e), p.n);
                a.nets[ni].pins.push(pr.clone());
                a.nets[ni].kinds.push(p.k.clone());
                a.pin_net.insert(pr, ni);
            }
        }
    }
    for e in &els {
        if e.typ == "GND" {
            if let Some(r) = c.root(e.x, e.y) {
                get(r, &mut a.nets);
            }
        }
    }
    a.root_net = by_root;
    for n in a.nets.iter_mut() {
        n.name = if n.ground {
            "0".into()
        } else if let Some(l) = n.labels.first() {
            l.clone()
        } else if let Some(p) = n.pins.first() {
            format!("~{}", p)
        } else {
            "~".into()
        };
    }
    // problems: the simulator's own netlist errors first
    for e in build_netlist(doc).errors {
        a.problems.push(e.msg);
    }
    for e in &doc.elements {
        if def(&e.typ).is_none() {
            a.problems.push(format!("未知的元件類型「{}」（{}）：程式會忽略它", e.typ, el_name(e)));
        }
    }
    if !els.iter().any(|e| e.typ == "SIMCTRL") {
        a.problems.push("沒有模擬控制元件（SIMCTRL）：暫態模擬需要它設定步長 dt 與總時間 tt".into());
    }
    for n in &a.nets {
        if n.ground {
            continue;
        }
        if n.pins.len() == 1 {
            if n.kinds[0] == "co" {
                a.warnings.push(format!("{} 是控制輸出，沒有接到任何輸入", n.pins[0]));
            } else {
                a.problems.push(format!("{} 沒有連接：節點「{}」只有這一隻接腳", n.pins[0], n.name));
            }
        } else if n.pins.is_empty() && !n.labels.is_empty() {
            a.warnings.push(format!("標籤「{}」沒有接到任何元件接腳", n.labels.join("、")));
        }
    }
    if !els.iter().any(|e| def(&e.typ).map(|d| d.has("probe")).unwrap_or(false)) {
        a.warnings.push("沒有探棒或量表：暫態模擬不會記錄任何波形".into());
    }
    // wire ends that touch nothing
    let mut cnt: HashMap<(i64, i64), u32> = HashMap::new();
    let k = |x: f64, y: f64| ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64);
    for e in &els {
        for p in wpins(e) {
            *cnt.entry(k(p.x, p.y)).or_insert(0) += 1;
        }
    }
    for w in &doc.wires {
        *cnt.entry(k(w.x1, w.y1)).or_insert(0) += 1;
        *cnt.entry(k(w.x2, w.y2)).or_insert(0) += 1;
    }
    let inside = |x: f64, y: f64| doc.wires.iter().any(|w| on_open_segment(x, y, w));
    let mut loose = 0;
    for w in &doc.wires {
        for (x, y) in [(w.x1, w.y1), (w.x2, w.y2)] {
            if cnt[&k(x, y)] == 1 && !inside(x, y) {
                loose += 1;
                if loose <= 8 {
                    a.warnings.push(format!("導線端點懸空：({}, {})", x, y));
                }
            }
        }
    }
    let mut seen: HashMap<(i64, i64), String> = HashMap::new();
    for e in &els {
        if matches!(e.typ.as_str(), "LABEL" | "GND" | "TEXT") {
            continue;
        }
        if let Some(o) = seen.insert(k(e.x, e.y), el_name(e)) {
            a.warnings.push(format!("{} 與 {} 放在同一個位置 ({}, {})", o, el_name(e), e.x, e.y));
        }
    }
    a
}

/// the analysis, with names from `names` (name -> pins) for nets that have no label
pub fn analyze_named(doc: &Doc, names: &BTreeMap<String, Vec<String>>) -> Analysis {
    let mut a = analyze(doc);
    for (name, pins) in names {
        if let Some(i) = pins.iter().find_map(|p| a.pin_net.get(p).copied()) {
            if a.nets[i].labels.is_empty() && !a.nets[i].ground && a.nets[i].name.starts_with('~') {
                a.nets[i].name = name.clone();
            }
        }
    }
    a
}

/// point strictly inside a wire (the T-junction rule)
fn on_open_segment(x: f64, y: f64, w: &Wire) -> bool {
    if w.y1 == w.y2 {
        y == w.y1 && x > w.x1.min(w.x2) && x < w.x1.max(w.x2)
    } else if w.x1 == w.x2 {
        x == w.x1 && y > w.y1.min(w.y2) && y < w.y1.max(w.y2)
    } else {
        false
    }
}

/// point on a closed segment
fn on_segment(x: f64, y: f64, a: (f64, f64), b: (f64, f64)) -> bool {
    if a.1 == b.1 {
        y == a.1 && x >= a.0.min(b.0) && x <= a.0.max(b.0)
    } else {
        x == a.0 && y >= a.1.min(b.1) && y <= a.1.max(b.1)
    }
}

/// pins of every element, before and after an edit: which pins share a net
fn partition(a: &Analysis) -> BTreeMap<String, usize> {
    a.pin_net.clone()
}

/// the edit may only merge the nets listed in `merge` (names before the edit); with `split` it may also
/// separate pins (disconnect); `ignore`: a pin whose connection is the subject of the edit
fn check_partition(before: &Analysis, after: &Analysis, merge: &[(String, String)], split: bool, ignore: Option<&str>) -> Result<(), String> {
    let pb = partition(before);
    let pa = partition(after);
    // classes of the nets before: allowed merges joined
    let mut cls: Vec<usize> = (0..before.nets.len()).collect();
    fn find(c: &[usize], mut i: usize) -> usize {
        while c[i] != i {
            i = c[i];
        }
        i
    }
    for (x, y) in merge {
        if let (Some(i), Some(j)) = (before.net_named(x), before.net_named(y)) {
            let (ri, rj) = (find(&cls, i), find(&cls, j));
            cls[ri] = rj;
        }
    }
    let mut after_to_class: HashMap<usize, (usize, String)> = HashMap::new();
    let mut class_to_after: HashMap<usize, (usize, String)> = HashMap::new();
    for (pin, &nb) in &pb {
        if Some(pin.as_str()) == ignore {
            continue;
        }
        let Some(&na) = pa.get(pin) else { continue };
        let c = find(&cls, nb);
        match after_to_class.get(&na) {
            Some((c0, first)) if *c0 != c => {
                return Err(format!("這個修改會讓不該相連的接腳接在一起：{} 與 {} 變成同一個節點「{}」", pin, first, after.nets[na].name));
            }
            Some(_) => {}
            None => {
                after_to_class.insert(na, (c, pin.clone()));
            }
        }
        if !split {
            match class_to_after.get(&c) {
                Some((a0, first)) if *a0 != na => {
                    return Err(format!("這個修改會讓原本相連的接腳斷開：{} 與 {}（原本的節點「{}」）", pin, first, before.nets[nb].name));
                }
                Some(_) => {}
                None => {
                    class_to_after.insert(c, (na, pin.clone()));
                }
            }
        }
    }
    Ok(())
}

/* ------------------------------------------------------------------ free space and stubs */

/// what an edit must not touch: pins, wire ends, wires, element boxes
struct Occupancy {
    points: HashSet<(i64, i64)>,
    wires: Vec<((f64, f64), (f64, f64))>,
    boxes: Vec<[f64; 4]>,
}
fn kp(x: f64, y: f64) -> (i64, i64) {
    ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64)
}

impl Occupancy {
    fn of(doc: &Doc) -> Occupancy {
        let mut points = HashSet::new();
        let mut boxes = vec![];
        for e in &doc.elements {
            for p in wpins(e) {
                points.insert(kp(p.x, p.y));
            }
            if !matches!(e.typ.as_str(), "TEXT") {
                boxes.push(world_box(e));
            }
        }
        let mut wires = vec![];
        for w in &doc.wires {
            points.insert(kp(w.x1, w.y1));
            points.insert(kp(w.x2, w.y2));
            wires.push(((w.x1, w.y1), (w.x2, w.y2)));
        }
        Occupancy { points, wires, boxes }
    }
    /// cost of a new wire segment from `a` to `b` that is joined to the existing drawing only at `a`
    /// (and at `b` when `b_joins`); None if it would connect to anything else
    fn segment_cost(&self, a: (f64, f64), b: (f64, f64), b_joins: bool) -> Option<f64> {
        let (minx, maxx, miny, maxy) = (a.0.min(b.0), a.0.max(b.0), a.1.min(b.1), a.1.max(b.1));
        // existing points on the new segment (other than where it is meant to join)
        for &(px, py) in &self.points {
            let (x, y) = (px as f64 / 1000.0, py as f64 / 1000.0);
            if (x, y) == a || (b_joins && (x, y) == b) {
                continue;
            }
            if on_segment(x, y, a, b) {
                return None;
            }
        }
        let mut cost = 0.0;
        for &(p, q) in &self.wires {
            let horiz_new = a.1 == b.1;
            let horiz_old = p.1 == q.1;
            if horiz_new == horiz_old {
                // collinear overlap joins them
                let same_line = if horiz_new { p.1 == a.1 } else { p.0 == a.0 };
                if same_line {
                    let (lo, hi) = if horiz_new { (p.0.min(q.0), p.0.max(q.0)) } else { (p.1.min(q.1), p.1.max(q.1)) };
                    let (nlo, nhi) = if horiz_new { (minx, maxx) } else { (miny, maxy) };
                    if lo < nhi && nlo < hi {
                        return None;
                    }
                }
            } else {
                // the far end may not lie inside an old wire (that is a T junction)
                let w = Wire { id: 0, x1: p.0, y1: p.1, x2: q.0, y2: q.1 };
                if !b_joins && on_open_segment(b.0, b.1, &w) {
                    return None;
                }
                // a crossing without a shared point does not connect, but is hard to read
                let crosses = if horiz_new {
                    p.0 > minx && p.0 < maxx && a.1 > p.1.min(q.1) && a.1 < p.1.max(q.1)
                } else {
                    p.1 > miny && p.1 < maxy && a.0 > p.0.min(q.0) && a.0 < p.0.max(q.0)
                };
                if crosses {
                    cost += 10.0;
                }
            }
        }
        for b0 in &self.boxes {
            let mid = ((a.0 + b.0) / 2.0, (a.1 + b.1) / 2.0);
            if b.0 > b0[0] && b.0 < b0[2] && b.1 > b0[1] && b.1 < b0[3] {
                cost += 6.0;
            } else if mid.0 > b0[0] && mid.0 < b0[2] && mid.1 > b0[1] && mid.1 < b0[3] {
                cost += 3.0;
            }
        }
        Some(cost)
    }
    fn add_segment(&mut self, a: (f64, f64), b: (f64, f64)) {
        self.points.insert(kp(a.0, a.1));
        self.points.insert(kp(b.0, b.1));
        self.wires.push((a, b));
    }
}

/// a wire from a pin to a net label (or ground symbol); tries the pin's outward direction first
fn add_stub(c: &mut Circuit, occ: &mut Occupancy, at: (f64, f64), out: (f64, f64), net: &str) -> Result<(), String> {
    let dirs = [out, (out.1, out.0), (-out.1, -out.0), (-out.0, -out.1)];
    let mut best: Option<(f64, (f64, f64), (f64, f64))> = None;
    for (di, &d) in dirs.iter().enumerate() {
        for len in [STUB, STUB + 1.0, STUB + 2.0, STUB + 4.0] {
            let end = (at.0 + d.0 * len, at.1 + d.1 * len);
            if let Some(cost) = occ.segment_cost(at, end, false) {
                // room for the label or ground symbol beyond the end
                let w = if is_ground(net) { 1.2 } else { label_width(net) };
                let tip = (end.0 + d.0 * w, end.1 + d.1 * w);
                let extra = occ.segment_cost(end, tip, false).map(|c| c * 0.5).unwrap_or(8.0);
                let total = cost + extra + di as f64 * 4.0 + (len - STUB);
                if best.map(|b| total < b.0).unwrap_or(true) {
                    best = Some((total, end, d));
                }
            }
        }
    }
    let (_, end, d) = best.ok_or_else(|| format!("({}, {}) 周圍沒有空間拉出標籤線", at.0, at.1))?;
    c.add_wire(at, end);
    occ.add_segment(at, end);
    if is_ground(net) {
        c.add_element("GND", "", end.0, end.1, dir_rot_gnd(d), false, &BTreeMap::new());
    } else {
        let mut p = BTreeMap::new();
        p.insert("text".to_string(), net.trim().to_string());
        c.add_element("LABEL", "", end.0, end.1, dir_rot_label(d), false, &p);
    }
    Ok(())
}

/// the wires that serve only one pin: from the pin along wires up to (and including the wire into) the next
/// element pin or junction, and the labels or ground symbols at their ends. A wire that other pins or wires
/// touch in its middle (a rail) is never part of it.
struct Chain {
    wires: Vec<i64>,
    marks: Vec<i64>,
    /// the chain reaches other element pins or a junction: it is not just a label stub
    joins: bool,
    /// the pin sits where several wires or pins meet: its connection cannot be separated from the others
    shared: bool,
}

fn chain_from(doc: &Doc, start: (f64, f64)) -> Chain {
    let mut at_point: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    for (i, w) in doc.wires.iter().enumerate() {
        at_point.entry(kp(w.x1, w.y1)).or_default().push(i);
        at_point.entry(kp(w.x2, w.y2)).or_default().push(i);
    }
    let mut pins: HashMap<(i64, i64), Vec<usize>> = HashMap::new();
    let mut all_points: Vec<(f64, f64)> = vec![];
    for (i, e) in doc.elements.iter().enumerate() {
        for p in wpins(e) {
            pins.entry(kp(p.x, p.y)).or_default().push(i);
            all_points.push((p.x, p.y));
        }
    }
    for w in &doc.wires {
        all_points.push((w.x1, w.y1));
        all_points.push((w.x2, w.y2));
    }
    // wires with something attached in their middle
    let rail = |w: &Wire| all_points.iter().any(|&(x, y)| on_open_segment(x, y, w));
    let mut ch = Chain { wires: vec![], marks: vec![], joins: false, shared: false };
    let k0 = kp(start.0, start.1);
    let at_start = at_point.get(&k0).cloned().unwrap_or_default();
    let real_at_start = pins.get(&k0).map(|v| v.iter().filter(|&&i| !matches!(doc.elements[i].typ.as_str(), "LABEL" | "GND")).count()).unwrap_or(0);
    if at_start.len() > 1 || real_at_start > 1 || doc.wires.iter().any(|w| on_open_segment(start.0, start.1, w)) {
        ch.shared = true;
        ch.joins = true;
        return ch;
    }
    let mut seen: HashSet<usize> = HashSet::new();
    let mut stack = vec![start];
    let mut first = true;
    while let Some(q) = stack.pop() {
        let k = kp(q.0, q.1);
        let here = at_point.get(&k).cloned().unwrap_or_default();
        if !first {
            let els = pins.get(&k).cloned().unwrap_or_default();
            let real = els.iter().any(|&i| !matches!(doc.elements[i].typ.as_str(), "LABEL" | "GND"));
            let inside = doc.wires.iter().any(|w| on_open_segment(q.0, q.1, w));
            if real || here.len() > 2 || inside {
                ch.joins = true;
                continue;
            }
            for &i in &els {
                ch.marks.push(doc.elements[i].id);
            }
        }
        first = false;
        for wi in here {
            if seen.insert(wi) {
                let w = &doc.wires[wi];
                if rail(w) {
                    ch.joins = true;
                    continue;
                }
                ch.wires.push(w.id);
                let other = if kp(w.x1, w.y1) == k { (w.x2, w.y2) } else { (w.x1, w.y1) };
                stack.push(other);
            }
        }
    }
    ch
}

fn remove_chain(doc: &mut Doc, ch: &Chain) {
    doc.wires.retain(|w| !ch.wires.contains(&w.id));
    doc.elements.retain(|e| !ch.marks.contains(&e.id));
}

/* ------------------------------------------------------------------ layout of a netlist */

fn str_map(v: Option<&Value>, what: &str) -> Result<BTreeMap<String, String>, String> {
    let mut m = BTreeMap::new();
    match v {
        None | Some(Value::Null) => {}
        Some(Value::Object(o)) => {
            for (k, x) in o {
                let s = match x {
                    Value::String(s) => s.clone(),
                    Value::Number(n) => n.to_string(),
                    Value::Bool(b) => (if *b { "1" } else { "0" }).to_string(),
                    _ => return Err(format!("{} 的「{}」必須是文字或數字", what, k)),
                };
                m.insert(k.clone(), s);
            }
        }
        _ => return Err(format!("{} 必須是物件，例如 {{\"R\": \"10k\"}}", what)),
    }
    Ok(m)
}

fn check_params(typ: &str, params: &BTreeMap<String, String>, who: &str) -> Result<(), String> {
    let d = def(typ).ok_or_else(|| format!("未知的元件類型「{}」", typ))?;
    for k in params.keys() {
        if d.param(k).is_none() {
            return Err(format!(
                "{}（{}）沒有參數「{}」。可用參數：{}",
                who,
                typ,
                k,
                d.params.iter().map(|p| p.k.as_str()).collect::<Vec<_>>().join("、")
            ));
        }
    }
    Ok(())
}

fn rot_of(v: Option<&Value>) -> i64 {
    let r = v.and_then(|x| x.as_i64()).unwrap_or(0);
    if r.abs() >= 4 {
        (r / 90).rem_euclid(4)
    } else {
        r.rem_euclid(4)
    }
}

/// extents of an element plus its stubs, labels and texts, relative to its origin
fn cell_box(el: &Element, nets: &BTreeMap<String, Option<String>>) -> [f64; 4] {
    let mut o = el.clone();
    o.x = 0.0;
    o.y = 0.0;
    let mut b = world_box(&o);
    for p in lib().pins_for(&o) {
        let Some(Some(net)) = nets.get(&p.n) else { continue };
        let d = pin_dir(&o, &p);
        let (px, py) = tpt(&o, p.x, p.y);
        let w = if is_ground(net) { 1.3 } else { label_width(net) };
        let tip = (px + d.0 * (STUB + w), py + d.1 * (STUB + w));
        let side = if d.0 != 0.0 { (0.0, 0.7) } else { (0.7, 0.0) };
        for (x, y) in [(tip.0 - side.0, tip.1 - side.1), (tip.0 + side.0, tip.1 + side.1), (px - side.0, py - side.1), (px + side.0, py + side.1)] {
            b[0] = b[0].min(x);
            b[1] = b[1].min(y);
            b[2] = b[2].max(x);
            b[3] = b[3].max(y);
        }
    }
    let lines = label_lines(&o);
    if !lines.is_empty() {
        let wb = world_box(&o);
        if wb[2] - wb[0] >= wb[3] - wb[1] {
            b[1] = b[1].min(wb[1] - 0.6 - lines.len() as f64 * 0.95);
        } else {
            let w = lines.iter().map(|l| text_w(l) * 0.8).fold(0.0, f64::max);
            b[2] = b[2].max(wb[2] + 0.5 + w);
        }
    }
    [b[0] - 0.5, b[1] - 0.5, b[2] + 0.5, b[3] + 0.5]
}

pub struct Built {
    pub circuit: Circuit,
    pub notes: Vec<String>,
}

/// a circuit from a netlist: every element is placed on its own, each connected pin gets a short wire to a net
/// label (or a ground symbol for net "0"); the result is checked against the requested connections
pub fn from_netlist(v: &Value) -> Result<Built, String> {
    let mut notes = vec![];
    let els = v.get("elements").and_then(|x| x.as_array()).ok_or("netlist.elements 必須是元件陣列")?;
    struct Item {
        typ: String,
        name: String,
        params: BTreeMap<String, String>,
        rot: i64,
        mir: bool,
        nets: BTreeMap<String, Option<String>>,
    }
    let mut items: Vec<Item> = vec![];
    let mut used: HashSet<String> = HashSet::new();
    let mut counters: HashMap<String, usize> = HashMap::new();
    // explicit names first, so automatic names never collide with them
    for e in els {
        if let Some(n) = e.get("name").and_then(|x| x.as_str()) {
            if !n.trim().is_empty() && !used.insert(n.trim().to_lowercase()) {
                return Err(format!("元件名稱重複：{}（名稱不分大小寫）", n));
            }
        }
    }
    for (i, e) in els.iter().enumerate() {
        let typ = e.get("type").and_then(|x| x.as_str()).ok_or_else(|| format!("第 {} 個元件缺少 type", i + 1))?.trim().to_string();
        let d = def(&typ).ok_or_else(|| format!("第 {} 個元件：未知的元件類型「{}」（可用 daedalus_elements 查詢）", i + 1, typ))?;
        if typ == "LABEL" || typ == "GND" {
            return Err(format!("第 {} 個元件：不需要 {}，請在 pins 中直接寫節點名稱（接地寫 \"0\"）", i + 1, typ));
        }
        let params = str_map(e.get("params"), &format!("第 {} 個元件（{}）的 params", i + 1, typ))?;
        let mut name = e.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
        if d.noname() {
            name.clear();
        } else if name.is_empty() {
            let pre = if d.pre.is_empty() { typ.clone() } else { d.pre.clone() };
            loop {
                let k = counters.entry(pre.clone()).or_insert(0);
                *k += 1;
                let cand = format!("{}{}", pre, k);
                if used.insert(cand.to_lowercase()) {
                    name = cand;
                    break;
                }
            }
            notes.push(format!("第 {} 個元件（{}）自動命名為 {}", i + 1, typ, name));
        }
        let who = if name.is_empty() { typ.clone() } else { name.clone() };
        check_params(&typ, &params, &who)?;
        let probe = Element { id: 0, typ: typ.clone(), x: 0.0, y: 0.0, rot: 0, mir: false, dis: false, name: name.clone(), params: params.clone(), disp: None };
        let pins = lib().pins_for(&probe);
        let mut nets: BTreeMap<String, Option<String>> = BTreeMap::new();
        if let Some(pv) = e.get("pins") {
            let o = pv.as_object().ok_or_else(|| format!("{} 的 pins 必須是物件，例如 {{\"+\": \"VIN\", \"-\": \"0\"}}", who))?;
            for (pn, nv) in o {
                let p = pins
                    .iter()
                    .find(|p| p.n == *pn)
                    .or_else(|| pins.iter().find(|p| p.n.eq_ignore_ascii_case(pn)))
                    .ok_or_else(|| format!("{}（{}）沒有接腳「{}」。接腳：{}", who, typ, pn, pins.iter().map(|p| p.n.as_str()).collect::<Vec<_>>().join("、")))?;
                let net = match nv {
                    Value::Null => None,
                    Value::String(s) if s.trim().is_empty() => None,
                    Value::String(s) => Some(s.trim().to_string()),
                    Value::Number(n) => Some(n.to_string()),
                    _ => return Err(format!("{}.{} 的節點名稱必須是文字", who, pn)),
                };
                nets.insert(p.n.clone(), net);
            }
        }
        let missing: Vec<&str> = pins.iter().filter(|p| !nets.contains_key(&p.n)).map(|p| p.n.as_str()).collect();
        if !missing.is_empty() {
            return Err(format!(
                "{}（{}）的接腳 {} 沒有指定節點。每隻接腳都要寫節點名稱（不接的寫 null）；接腳：{}",
                who,
                typ,
                missing.join("、"),
                pins.iter().map(|p| format!("{}［{}］", p.n, kind_zh(&p.k))).collect::<Vec<_>>().join("、")
            ));
        }
        for (pn, n) in &nets {
            if n.is_none() {
                notes.push(format!("{}.{} 不接（null）", who, pn));
            }
        }
        items.push(Item { typ, name, params, rot: rot_of(e.get("rot")), mir: e.get("mir").and_then(|x| x.as_bool()).unwrap_or(false), nets });
    }
    // simulation control and parameter file
    let mut header: Vec<crate::layout::Item> = vec![];
    let sim = str_map(v.get("sim"), "netlist.sim")?;
    check_params("SIMCTRL", &sim, "sim")?;
    if !items.iter().any(|i| i.typ == "SIMCTRL") {
        if sim.is_empty() {
            notes.push("沒有給 sim：模擬控制使用預設值 dt = 10 µs、tt = 0.1 s".into());
        }
        header.push(crate::layout::Item { typ: "SIMCTRL".into(), name: String::new(), params: sim, rot: 0, mir: false, nets: BTreeMap::new() });
    }
    if let Some(t) = v.get("params").and_then(|x| x.as_str()) {
        let mut p = BTreeMap::new();
        p.insert("text".to_string(), t.to_string());
        header.push(crate::layout::Item { typ: "PARAMS".into(), name: String::new(), params: p, rot: 0, mir: false, nets: BTreeMap::new() });
    }
    // what the drawing must connect
    let mut want: BTreeMap<String, Vec<String>> = BTreeMap::new();
    for it in &items {
        for (pn, net) in &it.nets {
            if let Some(net) = net {
                let key = if is_ground(net) { "0".to_string() } else { net.clone() };
                want.entry(key).or_default().push(format!("{}.{}", it.name, pn));
            }
        }
    }
    let title = v.get("title").and_then(|x| x.as_str());
    let laid = crate::layout::build(title, header, items.into_iter().map(|i| crate::layout::Item { typ: i.typ, name: i.name, params: i.params, rot: i.rot, mir: i.mir, nets: i.nets }).collect());
    let mut c = laid.circuit;
    notes.extend(laid.notes);
    if let Some(s) = v.get("screens") {
        c.screens = Some(screens_of(s)?);
    }
    // the drawing must connect exactly what was asked
    let a = analyze(&c.doc);
    let mut seen_nets: HashMap<usize, String> = HashMap::new();
    for (net, pins) in &want {
        let ids: HashSet<usize> = pins.iter().filter_map(|p| a.pin_net.get(p).copied()).collect();
        if ids.len() != 1 {
            return Err(format!("內部錯誤：節點 {} 的接腳沒有接在一起（{}）", net, pins.join("、")));
        }
        let id = *ids.iter().next().unwrap();
        if let Some(o) = seen_nets.insert(id, net.clone()) {
            return Err(format!("內部錯誤：節點 {} 與 {} 被接在一起", o, net));
        }
    }
    c.nets = want.iter().filter(|(n, _)| *n != "0").map(|(n, p)| (n.clone(), p.clone())).collect();
    for (net, pins) in &want {
        if pins.len() == 1 && net != "0" {
            notes.push(format!("節點 {} 只接了 {}：這隻接腳等於沒有連接", net, pins[0]));
        }
    }
    Ok(Built { circuit: c, notes })
}

pub fn kind_zh(k: &str) -> &'static str {
    match k {
        "p" => "功率",
        "a" => "功率或控制",
        "ci" => "控制輸入",
        "co" => "控制輸出",
        "g" => "閘極控制",
        "gnd" => "接地",
        "lbl" => "標籤",
        _ => "其他",
    }
}

pub fn screens_of(s: &Value) -> Result<Vec<Vec<String>>, String> {
    let a = s.as_array().ok_or("screens 必須是陣列的陣列，例如 [[\"Vo\"], [\"IL\"]]")?;
    let mut out = vec![];
    for x in a {
        match x {
            Value::String(n) => out.push(vec![n.clone()]),
            Value::Array(v) => out.push(v.iter().filter_map(|y| y.as_str().map(String::from)).collect()),
            _ => return Err("screens 的每個畫面是探棒名稱的陣列".into()),
        }
    }
    Ok(out)
}

/* ------------------------------------------------------------------ editing */

/// where elements added without a position go: to the right of the drawing, one under the other
struct Spot {
    x: f64,
    y: f64,
    col_w: f64,
}
impl Spot {
    fn of(doc: &Doc) -> Spot {
        let mut b = [f64::INFINITY, f64::INFINITY, f64::NEG_INFINITY, f64::NEG_INFINITY];
        for e in &doc.elements {
            let w = world_box(e);
            b = [b[0].min(w[0]), b[1].min(w[1]), b[2].max(w[2]), b[3].max(w[3])];
        }
        for w in &doc.wires {
            b = [b[0].min(w.x1.min(w.x2)), b[1].min(w.y1.min(w.y2)), b[2].max(w.x1.max(w.x2)), b[3].max(w.y1.max(w.y2))];
        }
        if !b[0].is_finite() {
            return Spot { x: 4.0, y: 4.0, col_w: 0.0 };
        }
        Spot { x: (b[2] + 6.0).ceil(), y: b[1].floor().max(2.0), col_w: 0.0 }
    }
    fn take(&mut self, cell: [f64; 4]) -> (f64, f64) {
        let p = ((self.x - cell[0]).ceil(), (self.y - cell[1]).ceil());
        self.y += cell[3] - cell[1] + 2.0;
        self.col_w = self.col_w.max(cell[2] - cell[0]);
        if self.y > 80.0 {
            self.x += self.col_w + 3.0;
            self.y = 2.0;
            self.col_w = 0.0;
        }
        p
    }
}

pub struct EditLog {
    pub done: Vec<String>,
}

fn net_target(a: &Analysis, doc: &Doc, s: &str) -> Result<NetTarget, String> {
    let s = s.trim();
    if is_ground(s) {
        return Ok(NetTarget::Name("0".into()));
    }
    if s.contains('.') {
        if let Ok((i, p, _)) = pin_of(doc, s) {
            let pr = format!("{}.{}", el_name(&doc.elements[i]), p.n);
            return Ok(NetTarget::Pin(pr));
        }
    }
    if s.starts_with('~') {
        let ni = a.net_named(s).ok_or_else(|| format!("找不到節點 {}", s))?;
        return Ok(NetTarget::Pin(a.nets[ni].pins[0].clone()));
    }
    Ok(NetTarget::Name(s.to_string()))
}

enum NetTarget {
    Name(String),
    Pin(String),
}

/// label text that is not yet used in the document
fn fresh_label(a: &Analysis, base: &str) -> String {
    let base: String = base.chars().map(|c| if c.is_alphanumeric() { c } else { '_' }).collect();
    let mut n = format!("N_{}", base);
    let mut k = 2;
    while a.net_named(&n).is_some() {
        n = format!("N_{}_{}", base, k);
        k += 1;
    }
    n
}

/// connect a pin to a net: by name (a label, or ground for "0"), or to the net of another pin
/// `via`: "auto" (a wire when a short clean path exists, else a label), "wire" or "label"
fn connect(c: &mut Circuit, pin: &str, target: &str, via: &str) -> Result<String, String> {
    let before = c.analyze();
    let (i, p, at) = pin_of(&c.doc, pin)?;
    let el = c.doc.elements[i].clone();
    let pr = format!("{}.{}", el_name(&el), p.n);
    let (name, other) = match net_target(&before, &c.doc, target)? {
        NetTarget::Name(n) => (n, None),
        NetTarget::Pin(q) => {
            let net = before.net_of(&q).cloned().unwrap_or_default();
            if net.ground {
                ("0".to_string(), None)
            } else if let Some(l) = net.labels.first() {
                (l.clone(), None)
            } else {
                (fresh_label(&before, &q), Some(q))
            }
        }
    };
    if let Some(cur) = before.net_of(&pr) {
        if cur.name == name || (is_ground(&name) && cur.ground) {
            return Ok(format!("{} 已經在節點 {}", pr, cur.name));
        }
    }
    let old = before.net_of(&pr).map(|n| n.name.clone()).unwrap_or_default();
    // a wire to the target net when a short clean path exists (ground keeps its symbol)
    let target = match &other {
        Some(q) => before.pin_net.get(q).copied(),
        None => before.net_named(&name),
    };
    if let (Some(ti), false, true) = (target, is_ground(&name), via != "label") {
        if let Some(path) = crate::layout::route_existing(&c.doc, &before, at, ti) {
            let n0 = c.doc.wires.len();
            for s in path.windows(2) {
                c.add_wire(s[0], s[1]);
            }
            let after = c.analyze();
            let tname = before.nets[ti].name.clone();
            if check_partition(&before, &after, &[(old.clone(), tname.clone())], false, None).is_ok() {
                let joined = after.net_of(&pr).map(|n| n.name.clone()).unwrap_or(tname);
                return Ok(format!("{} 以導線接到節點 {}", pr, joined));
            }
            c.doc.wires.truncate(n0);
        }
    }
    if via == "wire" {
        return Err(format!("{} 到節點 {} 沒有可行的導線路徑（會碰到其他接腳或導線）；可改用 via=\"label\"", pr, name));
    }
    // a net known only by name (no label in the drawing): its label goes on both ends
    let (name, other) = match (target, &other) {
        (Some(ti), None) if !before.nets[ti].ground && before.nets[ti].labels.is_empty() && !before.nets[ti].pins.is_empty() => (name.clone(), Some(before.nets[ti].pins[0].clone())),
        _ => (name, other),
    };
    let mut occ = Occupancy::of(&c.doc);
    add_stub(c, &mut occ, at, pin_dir(&el, &p), &name)?;
    if let Some(q) = &other {
        let (j, qp, qat) = pin_of(&c.doc, q)?;
        let qe = c.doc.elements[j].clone();
        add_stub(c, &mut occ, qat, pin_dir(&qe, &qp), &name)?;
    }
    let after = c.analyze();
    let mut merge = vec![];
    if before.net_named(&name).is_some() {
        merge.push((old.clone(), name.clone()));
    }
    if let Some(q) = &other {
        let qn = before.net_of(q).map(|n| n.name.clone()).unwrap_or_default();
        merge.push((old.clone(), qn));
    }
    check_partition(&before, &after, &merge, false, None)?;
    let joined = after.net_of(&pr).map(|n| n.name.clone()).unwrap_or_default();
    let was_alone = before.net_of(&pr).map(|n| n.pins.len() <= 1).unwrap_or(true);
    Ok(if was_alone { format!("{} 接到節點 {}", pr, joined) } else { format!("{} 接到節點 {}（原本的節點 {} 一併併入）", pr, joined, old) })
}

fn wire(c: &mut Circuit, from: &str, to: &str) -> Result<String, String> {
    let before = c.analyze();
    let (i, p, a) = pin_of(&c.doc, from)?;
    let (j, q, b) = pin_of(&c.doc, to)?;
    let (pa, pb) = (format!("{}.{}", el_name(&c.doc.elements[i]), p.n), format!("{}.{}", el_name(&c.doc.elements[j]), q.n));
    let (na, nb) = (before.net_of(&pa).map(|n| n.name.clone()).unwrap_or_default(), before.net_of(&pb).map(|n| n.name.clone()).unwrap_or_default());
    if before.pin_net.get(&pa) == before.pin_net.get(&pb) && before.pin_net.contains_key(&pa) {
        return Ok(format!("{} 與 {} 已經相連（節點 {}）", pa, pb, na));
    }
    let occ = Occupancy::of(&c.doc);
    let mut routes: Vec<Vec<(f64, f64)>> = vec![];
    if a.0 == b.0 || a.1 == b.1 {
        routes.push(vec![a, b]);
    }
    routes.push(vec![a, (b.0, a.1), b]);
    routes.push(vec![a, (a.0, b.1), b]);
    // with a step out of each pin first
    let da = pin_dir(&c.doc.elements[i], &p);
    let db = pin_dir(&c.doc.elements[j], &q);
    let a1 = (a.0 + da.0 * 2.0, a.1 + da.1 * 2.0);
    let b1 = (b.0 + db.0 * 2.0, b.1 + db.1 * 2.0);
    routes.push(vec![a, a1, (b1.0, a1.1), b1, b]);
    routes.push(vec![a, a1, (a1.0, b1.1), b1, b]);
    let mut best: Option<(f64, Vec<(f64, f64)>)> = None;
    for r in routes {
        let r: Vec<(f64, f64)> = r.into_iter().fold(vec![], |mut v: Vec<(f64, f64)>, p| {
            if v.last() != Some(&p) {
                v.push(p);
            }
            v
        });
        let mut cost = 0.0;
        let mut ok = true;
        let mut tmp = Occupancy { points: occ.points.clone(), wires: occ.wires.clone(), boxes: occ.boxes.clone() };
        for (k, s) in r.windows(2).enumerate() {
            let last = k + 2 == r.len();
            match tmp.segment_cost(s[0], s[1], last) {
                Some(cst) => cost += cst,
                None => {
                    ok = false;
                    break;
                }
            }
            tmp.add_segment(s[0], s[1]);
        }
        if ok {
            cost += r.len() as f64;
            if best.as_ref().map(|x| cost < x.0).unwrap_or(true) {
                best = Some((cost, r));
            }
        }
    }
    let (_, route) = best.ok_or_else(|| format!("{} 與 {} 之間無法直接拉線（路徑會碰到其他接腳或導線）。請改用 connect 以節點標籤連接。", pa, pb))?;
    for s in route.windows(2) {
        c.add_wire(s[0], s[1]);
    }
    let after = c.analyze();
    if let Err(e) = check_partition(&before, &after, &[(na.clone(), nb.clone())], false, None) {
        return Err(format!("拉線失敗：{}。請改用 connect 以節點標籤連接。", e));
    }
    Ok(format!("以導線連接 {} 與 {}（節點 {}）", pa, pb, after.net_of(&pa).map(|n| n.name.clone()).unwrap_or_default()))
}

fn disconnect(c: &mut Circuit, pin: &str) -> Result<String, String> {
    let before = c.analyze();
    let (i, p, at) = pin_of(&c.doc, pin)?;
    let pr = format!("{}.{}", el_name(&c.doc.elements[i]), p.n);
    // another element's pin on the very same point: only moving one of them separates them
    let touching: Vec<String> = c
        .doc
        .elements
        .iter()
        .enumerate()
        .filter(|(j, e)| *j != i && !matches!(e.typ.as_str(), "LABEL" | "GND"))
        .flat_map(|(_, e)| wpins(e).into_iter().filter(|q| (q.x, q.y) == at).map(move |q| format!("{}.{}", el_name(e), q.n)))
        .collect();
    if !touching.is_empty() {
        return Err(format!("{} 直接疊在 {} 上（沒有導線），無法只拆掉連線；請移動其中一個元件", pr, touching.join("、")));
    }
    let ch = chain_from(&c.doc, at);
    if ch.shared {
        return Err(format!("{} 位在導線的交會點或導線中間，與其他連線共用同一點，無法只拆這隻接腳；請移動元件（set 的 x、y）後再連接", pr));
    }
    let marks: Vec<i64> = c.doc.elements.iter().filter(|e| matches!(e.typ.as_str(), "LABEL" | "GND") && (e.x, e.y) == at).map(|e| e.id).collect();
    if ch.wires.is_empty() && marks.is_empty() {
        return Ok(format!("{} 本來就沒有連接", pr));
    }
    remove_chain(&mut c.doc, &ch);
    c.doc.elements.retain(|e| !marks.contains(&e.id));
    let after = c.analyze();
    // only this pin may leave its net; every other pin keeps its connections
    check_partition(&before, &after, &[], false, Some(&pr))?;
    Ok(format!("拆掉 {} 的連線（原本在節點 {}）", pr, before.net_of(&pr).map(|n| n.name.clone()).unwrap_or_default()))
}

/// removes an element; unless `keep_wires`, also the wires that served only its pins (label stubs, the wire
/// up to the next junction); wires shared with other pins stay
fn remove(c: &mut Circuit, name: &str, wires_too: bool) -> Result<String, String> {
    let before = c.analyze();
    let i = find_el(&c.doc, name)?;
    let e = c.doc.elements[i].clone();
    let mut n_w = 0;
    if wires_too {
        for p in wpins(&e) {
            let ch = chain_from(&c.doc, (p.x, p.y));
            if !ch.shared {
                n_w += ch.wires.len();
                remove_chain(&mut c.doc, &ch);
            }
        }
    }
    c.doc.elements.retain(|x| x.id != e.id);
    let after = c.analyze();
    check_partition(&before, &after, &[], false, None).map_err(|m| format!("刪除 {} 時：{}", el_name(&e), m))?;
    if let Some(sc) = c.screens.as_mut() {
        for s in sc.iter_mut() {
            s.retain(|n| n != &e.name);
        }
        sc.retain(|s| !s.is_empty());
    }
    Ok(format!("刪除 {}（{}）{}", el_name(&e), e.typ, if n_w > 0 { format!("與 {} 段導線", n_w) } else { String::new() }))
}

/// changes an element; moving or turning one whose pins only end in labels or ground symbols re-draws those
fn set(c: &mut Circuit, op: &Value) -> Result<String, String> {
    let name = op.get("name").and_then(|x| x.as_str()).ok_or("set 需要 name")?;
    let i = find_el(&c.doc, name)?;
    let old = c.doc.elements[i].clone();
    let mut msgs = vec![];
    let params = str_map(op.get("params"), "params")?;
    check_params(&old.typ, &params, &el_name(&old))?;
    for (k, v) in &params {
        msgs.push(format!("{}={}", k, v));
        c.doc.elements[i].params.insert(k.clone(), v.clone());
    }
    if let Some(n) = op.get("rename").and_then(|x| x.as_str()) {
        let n = n.trim();
        if c.doc.elements.iter().any(|e| e.id != old.id && e.name.eq_ignore_ascii_case(n)) {
            return Err(format!("名稱 {} 已經被使用", n));
        }
        c.doc.elements[i].name = n.to_string();
        let (oldp, newp) = (format!("{}.", old.name), format!("{}.", n));
        for pins in c.nets.values_mut() {
            for p in pins.iter_mut() {
                if let Some(rest) = p.strip_prefix(&oldp) {
                    *p = format!("{}{}", newp, rest);
                }
            }
        }
        if let Some(sc) = c.screens.as_mut() {
            for s in sc.iter_mut() {
                for x in s.iter_mut() {
                    if *x == old.name {
                        *x = n.to_string();
                    }
                }
            }
        }
        msgs.push(format!("改名為 {}", n));
    }
    if let Some(d) = op.get("dis").and_then(|x| x.as_bool()) {
        c.doc.elements[i].dis = d;
        msgs.push(if d { "停用".into() } else { "啟用".to_string() });
    }
    let geo = op.get("x").is_some() || op.get("y").is_some() || op.get("rot").is_some() || op.get("mir").is_some();
    if geo {
        let before = c.analyze();
        let cur = el_name(&c.doc.elements[i]);
        let mut chains = vec![];
        let mut relabel: Vec<(PinDef, String)> = vec![];
        for p in lib().pins_for(&old) {
            let at = tpt(&old, p.x, p.y);
            let ch = chain_from(&c.doc, at);
            let pr = format!("{}.{}", cur, p.n);
            let net = before.net_of(&pr).cloned().unwrap_or_default();
            let only_me = net.pins.len() <= 1;
            if ch.wires.is_empty() && only_me {
                continue;
            }
            if ch.joins || (ch.wires.is_empty() && !only_me) {
                return Err(format!("{} 以導線直接連到其他元件，移動或旋轉會斷線。請先 disconnect 這隻接腳，改好後再 connect", pr));
            }
            relabel.push((p.clone(), if net.ground { "0".into() } else { net.labels.first().cloned().unwrap_or_default() }));
            chains.push(ch);
        }
        for ch in &chains {
            remove_chain(&mut c.doc, ch);
        }
        let e = &mut c.doc.elements[i];
        if let Some(x) = op.get("x").and_then(|v| v.as_f64()) {
            e.x = x.round();
        }
        if let Some(y) = op.get("y").and_then(|v| v.as_f64()) {
            e.y = y.round();
        }
        if op.get("rot").is_some() {
            e.rot = rot_of(op.get("rot"));
        }
        if let Some(m) = op.get("mir").and_then(|v| v.as_bool()) {
            e.mir = m;
        }
        let e = e.clone();
        let mut occ = Occupancy::of(&c.doc);
        for (p, net) in relabel.iter().filter(|x| !x.1.is_empty()) {
            add_stub(c, &mut occ, tpt(&e, p.x, p.y), pin_dir(&e, p), net)?;
        }
        let after = c.analyze();
        check_partition(&before, &after, &[], false, None)?;
        msgs.push(format!("位置 ({}, {})、旋轉 {}°{}", e.x, e.y, e.rot * 90, if e.mir { "、鏡像" } else { "" }));
    }
    if msgs.is_empty() {
        return Err("set 沒有要修改的項目（params、rename、x、y、rot、mir、dis）".into());
    }
    Ok(format!("{}：{}", el_name(&c.doc.elements[i]), msgs.join("，")))
}

fn add(c: &mut Circuit, op: &Value, spot: &mut Spot) -> Result<String, String> {
    let typ = op.get("type").and_then(|x| x.as_str()).ok_or("add 需要 type")?.trim().to_string();
    let d = def(&typ).ok_or_else(|| format!("未知的元件類型「{}」", typ))?;
    let params = str_map(op.get("params"), "params")?;
    let mut name = op.get("name").and_then(|x| x.as_str()).unwrap_or("").trim().to_string();
    if d.noname() {
        name.clear();
    } else {
        if name.is_empty() {
            let pre = if d.pre.is_empty() { typ.clone() } else { d.pre.clone() };
            let mut k = 1;
            loop {
                let cand = format!("{}{}", pre, k);
                if find_el(&c.doc, &cand).is_err() {
                    name = cand;
                    break;
                }
                k += 1;
            }
        } else if c.doc.elements.iter().any(|e| e.name.eq_ignore_ascii_case(&name)) {
            return Err(format!("名稱 {} 已經被使用", name));
        }
    }
    let who = if name.is_empty() { typ.clone() } else { name.clone() };
    check_params(&typ, &params, &who)?;
    let rot = rot_of(op.get("rot"));
    let mir = op.get("mir").and_then(|x| x.as_bool()).unwrap_or(false);
    let probe = Element { id: 0, typ: typ.clone(), x: 0.0, y: 0.0, rot, mir, dis: false, name: name.clone(), params: params.clone(), disp: None };
    let pins = lib().pins_for(&probe);
    let mut nets: BTreeMap<String, Option<String>> = BTreeMap::new();
    if let Some(o) = op.get("pins").and_then(|x| x.as_object()) {
        for (pn, nv) in o {
            let p = pins
                .iter()
                .find(|p| p.n == *pn || p.n.eq_ignore_ascii_case(pn))
                .ok_or_else(|| format!("{}（{}）沒有接腳「{}」。接腳：{}", who, typ, pn, pins.iter().map(|p| p.n.as_str()).collect::<Vec<_>>().join("、")))?;
            nets.insert(p.n.clone(), nv.as_str().map(|s| s.trim().to_string()).filter(|s| !s.is_empty()));
        }
    }
    // nets given by pin reference ("Q1.S") become the target's label
    let (x, y) = match (op.get("x").and_then(|v| v.as_f64()), op.get("y").and_then(|v| v.as_f64())) {
        (Some(x), Some(y)) => (x.round(), y.round()),
        _ => {
            let label_nets: BTreeMap<String, Option<String>> = nets.iter().map(|(k, v)| (k.clone(), v.clone().map(|s| if s.contains('.') { "N_xxxxxxxx".into() } else { s }))).collect();
            let cell = cell_box(&probe, &label_nets);
            // next to the nets it joins, so that wires can reach them; else to the right of the drawing
            let a = c.analyze();
            let mut pts: Vec<(f64, f64)> = vec![];
            for n in nets.values().flatten() {
                if is_ground(n) {
                    continue;
                }
                let idx = if n.contains('.') { a.pin_net.get(n.as_str()).copied() } else { a.net_named(n) };
                if let Some(ni) = idx {
                    for pr in &a.nets[ni].pins {
                        if let Ok((_, _, pt)) = pin_of(&c.doc, pr) {
                            pts.push(pt);
                        }
                    }
                }
            }
            let near = if pts.is_empty() { None } else { Some((pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64, pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64)) };
            match near.and_then(|n| crate::layout::free_spot(&c.doc, cell, n)) {
                Some(p) => p,
                None => spot.take(cell),
            }
        }
    };
    let i = c.add_element(&typ, &name, x, y, rot, mir, &params);
    let mut msg = format!("新增 {}（{}）於 ({}, {})", who, typ, x, y);
    if !name.is_empty() {
        for (pn, net) in nets {
            if let Some(n) = net {
                let r = connect(c, &format!("{}.{}", name, pn), &n, op.get("via").and_then(|x| x.as_str()).unwrap_or("auto"))?;
                msg += &format!("；{}", r);
            }
        }
    }
    let _ = i;
    Ok(msg)
}

/// applies the operations in order; any error leaves the circuit as it was before the failing operation
pub fn apply_ops(c: &mut Circuit, ops: &[Value]) -> Result<EditLog, String> {
    let mut log = EditLog { done: vec![] };
    let mut spot = Spot::of(&c.doc);
    // the whole batch or nothing
    let saved = (c.doc.elements.clone(), c.doc.wires.clone(), c.doc.nid, c.screens.clone());
    for (k, op) in ops.iter().enumerate() {
        let kind = op.get("op").and_then(|x| x.as_str()).unwrap_or("");
        let r = (|| -> Result<String, String> { match kind {
            "add" => add(c, op, &mut spot),
            "set" => set(c, op),
            "remove" => op.get("name").and_then(|x| x.as_str()).ok_or("remove 需要 name".to_string()).and_then(|n| remove(c, n, op.get("keep_wires").and_then(|x| x.as_bool()) != Some(true))),
            "connect" => {
                let pin = op.get("pin").and_then(|x| x.as_str()).ok_or("connect 需要 pin")?;
                let t = op.get("net").or_else(|| op.get("to")).and_then(|x| x.as_str()).ok_or("connect 需要 net（節點名稱、\"0\" 接地）或 to（另一隻接腳，例如 \"L1.1\"）")?;
                connect(c, pin, t, op.get("via").and_then(|x| x.as_str()).unwrap_or("auto"))
            }
            "disconnect" => op.get("pin").and_then(|x| x.as_str()).ok_or("disconnect 需要 pin".to_string()).and_then(|p| disconnect(c, p)),
            "wire" => {
                let a = op.get("from").and_then(|x| x.as_str()).ok_or("wire 需要 from")?;
                let b = op.get("to").and_then(|x| x.as_str()).ok_or("wire 需要 to")?;
                wire(c, a, b)
            }
            "sim" => sim_op(c, op),
            "params" => params_op(c, op),
            "screens" => op.get("screens").ok_or("screens 需要 screens".to_string()).and_then(screens_of).map(|s| {
                let d = s.iter().map(|x| x.join("+")).collect::<Vec<_>>().join(" | ");
                c.screens = Some(s);
                format!("波形畫面：{}", d)
            }),
            "text" => {
                let t = op.get("text").and_then(|x| x.as_str()).ok_or("text 需要 text")?;
                let mut p = BTreeMap::new();
                p.insert("text".to_string(), t.to_string());
                let probe = Element { id: 0, typ: "TEXT".into(), x: 0.0, y: 0.0, rot: 0, mir: false, dis: false, name: String::new(), params: p.clone(), disp: None };
                let (x, y) = match (op.get("x").and_then(|v| v.as_f64()), op.get("y").and_then(|v| v.as_f64())) {
                    (Some(x), Some(y)) => (x.round(), y.round()),
                    _ => spot.take(cell_box(&probe, &BTreeMap::new())),
                };
                c.add_element("TEXT", "", x, y, 0, false, &p);
                Ok(format!("加入註解於 ({}, {})", x, y))
            }
            "" => Err("每個操作需要 op（add、set、remove、connect、disconnect、wire、sim、params、screens、text）".into()),
            o => Err(format!("不認得的操作「{}」（可用：add、set、remove、connect、disconnect、wire、sim、params、screens、text）", o)),
        } })();
        match r {
            Ok(m) => log.done.push(m),
            Err(e) => {
                c.doc.elements = saved.0.clone();
                c.doc.wires = saved.1.clone();
                c.doc.nid = saved.2;
                c.screens = saved.3.clone();
                return Err(format!("第 {} 個操作（{}）失敗：{}{}", k + 1, kind, e, if log.done.is_empty() { String::new() } else { format!("\n（先前完成的 {} 個操作也一併取消，檔案沒有改變）", log.done.len()) }));
            }
        }
    }
    Ok(log)
}

fn single(c: &mut Circuit, typ: &str) -> usize {
    if let Some(i) = c.doc.elements.iter().position(|e| e.typ == typ && !e.dis) {
        return i;
    }
    let mut b = [f64::INFINITY, f64::INFINITY];
    for e in &c.doc.elements {
        let w = world_box(e);
        b = [b[0].min(w[0]), b[1].min(w[1])];
    }
    let (x, y) = if b[0].is_finite() { (b[0].floor() + 2.0, (b[1] - 4.0).floor()) } else { (4.0, 3.0) };
    c.add_element(typ, "", x, y, 0, false, &BTreeMap::new())
}

fn sim_op(c: &mut Circuit, op: &Value) -> Result<String, String> {
    let p = str_map(op.get("params"), "sim 的 params")?;
    check_params("SIMCTRL", &p, "模擬控制")?;
    let i = single(c, "SIMCTRL");
    for (k, v) in &p {
        c.doc.elements[i].params.insert(k.clone(), v.clone());
    }
    Ok(format!("模擬控制：{}", p.iter().map(|(k, v)| format!("{}={}", k, v)).collect::<Vec<_>>().join("，")))
}

fn params_op(c: &mut Circuit, op: &Value) -> Result<String, String> {
    let t = op.get("text").and_then(|x| x.as_str()).ok_or("params 需要 text（參數檔內容，例如 \"Vin = 24\\nfsw = 50k\"）")?;
    let (_, errs) = daedalus_core::expr::parse_param_text(t);
    if !errs.is_empty() {
        return Err(format!("參數檔有錯誤：{}", errs.join("；")));
    }
    let i = single(c, "PARAMS");
    c.doc.elements[i].params.insert("text".into(), t.to_string());
    Ok(format!("參數檔：{}", t.replace('\n', "；")))
}

/// pins whose net changed between two analyses, for the edit report
pub fn net_changes(before: &Analysis, after: &Analysis) -> Vec<String> {
    let mut out = vec![];
    for (pin, &nb) in &before.pin_net {
        let old = &before.nets[nb];
        match after.pin_net.get(pin) {
            None => out.push(format!("{}：已刪除", pin)),
            Some(&na) => {
                let new = &after.nets[na];
                let (o, n) = (if old.pins.len() <= 1 && !old.ground { "（未接）".to_string() } else { old.name.clone() }, if new.pins.len() <= 1 && !new.ground { "（未接）".to_string() } else { new.name.clone() });
                if o != n {
                    out.push(format!("{}：{} → {}", pin, o, n));
                }
            }
        }
    }
    for (pin, &na) in &after.pin_net {
        if !before.pin_net.contains_key(pin) {
            let n = &after.nets[na];
            out.push(format!("{}：新增，{}", pin, if n.pins.len() <= 1 && !n.ground { "（未接）".to_string() } else { n.name.clone() }));
        }
    }
    out
}

#[cfg(test)]
pub mod tests {
    use super::*;
    use serde_json::json;

    pub fn buck_netlist() -> Value {
        json!({
            "title": "24 V → 12 V Buck",
            "sim": {"dt": "0.2u", "tt": "10m"},
            "params": "Vin = 24\nfsw = 50k",
            "elements": [
                {"type": "VDC", "name": "Vin", "params": {"V": "Vin"}, "pins": {"+": "VIN", "-": "0"}},
                {"type": "MOSFET", "name": "Q1", "pins": {"D": "VIN", "S": "SW", "G": "GATE"}},
                {"type": "DIODE", "name": "Dfw", "pins": {"A": "0", "K": "SW"}},
                {"type": "L", "name": "L1", "params": {"L": "100u"}, "pins": {"1": "SW", "2": "LO"}},
                {"type": "IP", "name": "IL", "pins": {"in": "LO", "out": "VO"}},
                {"type": "RC", "name": "Co", "params": {"R": "10m", "C": "100u"}, "pins": {"1": "VO", "2": "0"}},
                {"type": "R", "name": "RL", "params": {"R": "6"}, "pins": {"1": "VO", "2": "0"}},
                {"type": "VP", "name": "Vo", "pins": {"n": "VO"}},
                {"type": "CONST", "name": "Duty", "params": {"v": "0.5"}, "pins": {"out": "D"}},
                {"type": "VTRI", "name": "Vcar", "params": {"Vpp": "1", "f": "fsw"}, "pins": {"+": "CAR", "-": "0"}},
                {"type": "COMP", "name": "PWM", "pins": {"in+": "D", "in-": "CAR", "out": "GATE"}}
            ],
            "screens": [["Vo"], ["IL"]]
        })
    }

    #[test]
    fn netlist_layout_connects_exactly_what_was_asked() {
        let b = from_netlist(&buck_netlist()).expect("layout");
        // the AI's net names survive although the rails carry no labels
        let a = b.circuit.analyze();
        let labels = b.circuit.doc.elements.iter().filter(|e| e.typ == "LABEL").count();
        assert!(labels <= 2, "{} labels: only the gate signal crosses between power and control", labels);
        assert!(a.problems.is_empty(), "{:?}", a.problems);
        let net = |p: &str| a.net_of(p).map(|n| n.name.clone()).unwrap_or_default();
        assert_eq!(net("Q1.S"), "SW");
        assert_eq!(net("Dfw.K"), "SW");
        assert_eq!(net("Vin.-"), "0");
        assert_eq!(net("Vcar.-"), "0");
        assert_eq!(net("PWM.out"), "GATE");
        assert!(a.net_of("PWM.out").unwrap().control && !a.net_of("Q1.D").unwrap().control);
        assert!(build_netlist(&b.circuit.doc).errors.is_empty());
        // control parts are placed below the power stage
        let y = |n: &str| b.circuit.doc.elements[find_el(&b.circuit.doc, n).unwrap()].y;
        assert!(y("PWM") > y("Q1"), "control below power");
        // and it simulates: open-loop buck, D = 0.5 -> 12 V, 2 A
        let run = crate::run::run_doc(&b.circuit.doc, false, None, 60.0).expect("sim");
        let crate::run::Outcome::Tran(r) = run.out else { panic!("transient expected") };
        let t1 = r.t0 + r.dt * (r.n - 1) as f64;
        let vo = r.names.iter().position(|n| n == "Vo").unwrap();
        let il = r.names.iter().position(|n| n == "IL").unwrap();
        let m = |ci: usize| daedalus_core::measure::measure(&r, ci, t1 * 0.8, t1, &[daedalus_core::measure::Meas::Mean])[0];
        assert!((m(vo) - 12.0).abs() < 0.5, "Vo {}", m(vo));
        assert!((m(il) - 2.0).abs() < 0.1, "IL {}", m(il));
    }

    #[test]
    fn netlist_errors_are_explained() {
        let mut n = buck_netlist();
        n["elements"][1]["pins"] = json!({"D": "VIN", "S": "SW"});
        assert!(from_netlist(&n).err().unwrap().contains("G"));
        let mut n = buck_netlist();
        n["elements"][3]["params"] = json!({"Lx": "1u"});
        assert!(from_netlist(&n).err().unwrap().contains("沒有參數"));
        let mut n = buck_netlist();
        n["elements"][0]["type"] = json!("VDCX");
        assert!(from_netlist(&n).err().unwrap().contains("未知"));
    }

    fn example_buck() -> Circuit {
        let txt = include_str!("../../daedalus-app/assets/examples/buck.psimx");
        let (doc, screens) = Doc::from_json(txt).unwrap();
        let mut c = Circuit { doc, screens, nets: BTreeMap::new() };
        fix_nid(&mut c.doc);
        c
    }

    #[test]
    fn edits_on_a_hand_drawn_circuit_keep_other_connections() {
        let mut c = example_buck();
        let before = analyze(&c.doc);
        assert!(before.problems.is_empty(), "{:?}", before.problems);
        // a probe on a rail: removing it must not take the rail with it
        apply_ops(&mut c, &[json!({"op": "remove", "name": "Vo"})]).expect("remove");
        let a = analyze(&c.doc);
        assert_eq!(a.net_of("Co.1").map(|n| n.pins.len()), before.net_of("Co.1").map(|n| n.pins.len() - 1));
        // a new probe on the switch node, given by a pin reference
        let log = apply_ops(&mut c, &[json!({"op": "add", "type": "VP", "name": "Vsw", "pins": {"n": "D1.K"}}), json!({"op": "set", "name": "RL", "params": {"R": "12"}})]).expect("add");
        assert_eq!(log.done.len(), 2);
        let a = analyze(&c.doc);
        assert_eq!(a.pin_net.get("Vsw.n"), a.pin_net.get("D1.K"));
        assert_eq!(c.doc.elements[find_el(&c.doc, "RL").unwrap()].params["R"], "12");
        // and away again
        apply_ops(&mut c, &[json!({"op": "disconnect", "pin": "Vsw.n"})]).expect("disconnect");
        let a = analyze(&c.doc);
        assert_ne!(a.pin_net.get("Vsw.n"), a.pin_net.get("D1.K"));
        assert!(a.problems.iter().any(|p| p.contains("Vsw.n")));
        // a failing batch changes nothing
        let n0 = c.doc.elements.len();
        assert!(apply_ops(&mut c, &[json!({"op": "remove", "name": "Vsw"}), json!({"op": "connect", "pin": "Q1.X", "net": "A"})]).is_err());
        assert_eq!(c.doc.elements.len(), n0);
    }

    #[test]
    fn connect_and_wire() {
        let b = from_netlist(&buck_netlist()).unwrap();
        let mut c = b.circuit;
        apply_ops(&mut c, &[json!({"op": "add", "type": "VP", "name": "Vg", "pins": {"n": "GATE"}})]).expect("probe on a control net");
        let a = analyze(&c.doc);
        assert_eq!(a.net_of("Vg.n").unwrap().name, "GATE");
        // a resistor wired straight between its own two pins
        apply_ops(&mut c, &[json!({"op": "add", "type": "R", "name": "Rb", "params": {"R": "1k"}}), json!({"op": "wire", "from": "Rb.1", "to": "Rb.2"})]).expect("wire");
        let a = analyze(&c.doc);
        assert_eq!(a.pin_net.get("Rb.1"), a.pin_net.get("Rb.2"));
    }
}

#[cfg(test)]
mod rot_tests {
    use super::*;
    #[test]
    fn label_and_ground_follow_the_stub() {
        // f64::signum(0.0) is +1: the rotations must not depend on the sign of a zero component
        assert_eq!([(1.0, 0.0), (0.0, 1.0), (-1.0, 0.0), (0.0, -1.0)].map(dir_rot_label), [0, 1, 2, 3]);
        assert_eq!([(0.0, 1.0), (-1.0, 0.0), (0.0, -1.0), (1.0, 0.0)].map(dir_rot_gnd), [0, 1, 2, 3]);
    }
}
