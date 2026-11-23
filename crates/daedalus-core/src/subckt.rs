//! Subcircuits (PSIM-style): a definition is a circuit of its own, kept by name in the main circuit's
//! `subckts` map. Its Subcircuit Port elements (SUBPORT) become the pins of every Subcircuit block (SUBCKT)
//! that uses it. Before simulation the hierarchy is flattened into one circuit:
//!
//! * the elements of each block are copied (far to the right of everything else, so no wire or pin of one
//!   block can touch another) and named `X1.R1`, `X1.X2.C3`, …;
//! * the ports and the block's pins become pairs of labels with a name no user label can have, labels inside a
//!   block are local to it (a label named like a port joins that port), ground is the common ground;
//! * the variables of the definition's parameter files are local to each block: they get names of their own
//!   (`_x1_r`) and the block's own values ("vars": `R = 20`, evaluated where the block sits) replace the
//!   definition's defaults; element values in the block that use them are rewritten to the new names.
//!
//! Simulation Control, AC Sweep, Parameter Sweep and Monte Carlo elements of a definition are ignored: the main
//! circuit decides how the whole circuit is simulated.

use crate::doc::{Doc, Element, Wire};
use crate::expr::{self, Syms};
use crate::library::{lib, wpins, SubPort};
use crate::netlist::NlError;
use std::collections::{BTreeMap, HashMap, HashSet};

/// marks the labels made from ports and block pins (no label typed by a user contains it)
const SEP: char = '\u{1}';
/// a block's elements are moved this far to the right of the previous block
const SPACING: f64 = 100_000.0;
const MAX_DEPTH: usize = 16;
const MAX_ELEMENTS: usize = 200_000;

/// element types of a definition that the flattened circuit does not use
fn skipped(typ: &str) -> bool {
    matches!(typ, "SIMCTRL" | "ACSWEEP" | "PSWEEP" | "MONTECARLO" | "PARAMS" | "TEXT")
}

/// names that element parameters use for their own arguments (time, inputs): a subcircuit variable must not hide them
fn reserved(name: &str) -> bool {
    let n = name.to_lowercase();
    if matches!(n.as_str(), "t" | "x" | "v" | "i" | "pi" | "delt") {
        return true;
    }
    let mut c = n.chars();
    matches!(c.next(), Some('x') | Some('y')) && !n[1..].is_empty() && n[1..].chars().all(|d| d.is_ascii_digit())
}

/// the ports of a definition in pin order: left and right side by their height in the definition (top first),
/// top and bottom side from left to right; a name used twice is one port
pub fn def_ports(def: &Doc) -> Vec<SubPort> {
    let mut ports: Vec<(char, f64, f64, String)> = Vec::new();
    for e in def.elements.iter().filter(|e| e.typ == "SUBPORT" && !e.dis) {
        let name = port_name(e);
        if name.is_empty() || ports.iter().any(|p| p.3 == name) {
            continue;
        }
        let side = e.params.get("side").and_then(|s| s.chars().next()).filter(|c| "LRTB".contains(*c)).unwrap_or('L');
        ports.push((side, e.x, e.y, name));
    }
    let rank = |c: char| "LRTB".find(c).unwrap_or(0);
    ports.sort_by(|a, b| {
        let (ka, kb) = if "LR".contains(a.0) { ((a.2, a.1), (b.2, b.1)) } else { ((a.1, a.2), (b.1, b.2)) };
        rank(a.0).cmp(&rank(b.0)).then(ka.partial_cmp(&kb).unwrap_or(std::cmp::Ordering::Equal))
    });
    // the pin kind from what the port connects to inside: power, a control output, control inputs, or either
    let els: Vec<&Element> = def.elements.iter().filter(|e| !e.dis).collect();
    let conn = crate::netlist::connectivity(def, &els);
    let kind = |name: &str| {
        let mut ks: HashSet<String> = HashSet::new();
        for e in def.elements.iter().filter(|e| e.typ == "SUBPORT" && !e.dis && port_name(e) == name) {
            if let Some(k) = conn.kinds_at(e.x, e.y) {
                ks.extend(k.iter().cloned());
            }
        }
        if ks.contains("p") {
            "p"
        } else if ks.contains("co") {
            "co"
        } else if ks.contains("ci") || ks.contains("g") {
            "ci"
        } else {
            "a"
        }
    };
    ports.into_iter().map(|(side, _, _, name)| SubPort { kind: kind(&name).into(), name, side }).collect()
}

/// the port name of a SUBPORT element
pub fn port_name(e: &Element) -> String {
    e.params.get("text").map(|s| s.trim().replace(SEP, "")).unwrap_or_default()
}

/// variable names defined by the parameter files of a definition, in order (first spelling kept), with their
/// expressions
pub fn def_vars(def: &Doc) -> Vec<(String, String)> {
    let mut out: Vec<(String, String)> = Vec::new();
    for e in def.elements.iter().filter(|e| e.typ == "PARAMS" && !e.dis) {
        for line in e.params.get("text").map(|s| s.as_str()).unwrap_or("").split('\n') {
            if let Some(Some((n, ex))) = expr::param_line(line) {
                if !out.iter().any(|(m, _)| m.eq_ignore_ascii_case(&n)) {
                    out.push((n, ex.trim().to_string()));
                }
            }
        }
    }
    out
}

/// "name = expr" lines of a block's own values
pub fn parse_vars(text: &str) -> Vec<(String, String)> {
    text.split('\n').filter_map(|l| expr::param_line(l).flatten()).map(|(n, e)| (n, e.trim().to_string())).collect()
}

/// where an element of the flattened circuit came from
#[derive(Clone, Debug, PartialEq)]
pub struct Origin {
    /// the block of the main circuit that contains it
    pub top: i64,
    /// its definition and its id there
    pub def: String,
    pub local: i64,
}

pub struct Flat {
    pub doc: Doc,
    /// parameter-file lines of the blocks' local variables (after the main circuit's own)
    pub params: String,
    pub origin: HashMap<i64, Origin>,
    pub errors: Vec<NlError>,
}

pub fn has_blocks(doc: &Doc) -> bool {
    doc.elements.iter().any(|e| e.typ == "SUBCKT" && !e.dis)
}

struct Ctx<'a> {
    root: &'a Doc,
    out: Doc,
    params: String,
    syms: Syms,
    origin: HashMap<i64, Origin>,
    errors: Vec<NlError>,
    nid: i64,
    blocks: usize,
}

/// The main circuit with every block replaced by its contents (see the module notes). `global_params` is the
/// main circuit's parameter-file text; the block variables are checked against it.
pub fn flatten(root: &Doc, global_params: &str) -> Flat {
    let mx = root.elements.iter().map(|e| e.id).chain(root.wires.iter().map(|w| w.id)).max().unwrap_or(0);
    let mut cx = Ctx {
        root,
        out: Doc { elements: root.elements.clone(), wires: root.wires.clone(), nid: 0, subckts: BTreeMap::new() },
        params: String::new(),
        syms: expr::parse_param_text(global_params).0,
        origin: HashMap::new(),
        errors: Vec::new(),
        nid: root.nid.max(mx + 1),
        blocks: 0,
    };
    // instance names must be unique among their level's names; the main circuit's are checked by the netlist
    let tops: Vec<Element> = root.elements.iter().filter(|e| e.typ == "SUBCKT" && !e.dis).cloned().collect();
    for b in &tops {
        expand(&mut cx, b, "", &HashMap::new(), b.id, &mut Vec::new());
    }
    cx.out.nid = cx.nid;
    Flat { doc: cx.out, params: cx.params, origin: cx.origin, errors: cx.errors }
}

fn label(id: i64, x: f64, y: f64, text: String) -> Element {
    let mut params = BTreeMap::new();
    params.insert("text".to_string(), text);
    Element { id, typ: "LABEL".into(), x, y, rot: 0, mir: false, dis: false, name: String::new(), params, disp: None }
}

/// copy the contents of block `b` (already in the flattened circuit, at its final position) into it
fn expand(cx: &mut Ctx, b: &Element, prefix: &str, scope: &HashMap<String, String>, top: i64, path: &mut Vec<String>) {
    let full = format!("{}{}", prefix, b.name);
    let def_name = b.params.get("def").cloned().unwrap_or_default();
    let err = |cx: &mut Ctx, m: String| cx.errors.push(NlError { msg: format!("子電路 {}：{}", full, m), els: vec![top] });
    let Some(def) = cx.root.subckts.get(&def_name) else {
        err(cx, if def_name.is_empty() { "沒有指定子電路定義".into() } else { format!("找不到子電路定義「{}」", def_name) });
        return;
    };
    if path.contains(&def_name) {
        err(cx, format!("子電路「{}」直接或間接包含自己（{} → {}）", def_name, path.join(" → "), def_name));
        return;
    }
    if path.len() >= MAX_DEPTH {
        err(cx, format!("子電路層數超過 {} 層", MAX_DEPTH));
        return;
    }
    if cx.out.elements.len() + def.elements.len() > MAX_ELEMENTS {
        err(cx, format!("展開後的元件超過 {} 個", MAX_ELEMENTS));
        return;
    }
    // the block's pins must be the definition's ports (the program keeps them in step; a hand-edited file may not)
    let ports = def_ports(def);
    let pins = wpins(b);
    let pin_names: HashSet<&str> = pins.iter().map(|p| p.n.as_str()).collect();
    let port_names: HashSet<&str> = ports.iter().map(|p| p.name.as_str()).collect();
    if pin_names != port_names {
        err(cx, format!("方塊的接腳（{}）與子電路「{}」的埠（{}）不一致；請重新開啟電路檔或進入子電路後返回以更新接腳", pins.iter().map(|p| p.n.as_str()).collect::<Vec<_>>().join("、"), def_name, ports.iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("、")));
        return;
    }
    let mut dup: Vec<String> = Vec::new();
    let mut seen: HashSet<String> = HashSet::new();
    for e in def.elements.iter().filter(|e| e.typ == "SUBPORT" && !e.dis) {
        let n = port_name(e);
        if n.is_empty() {
            err(cx, format!("子電路「{}」有沒有名稱的埠", def_name));
            return;
        }
        if !seen.insert(n.clone()) && !dup.contains(&n) {
            dup.push(n);
        }
    }
    if !dup.is_empty() {
        err(cx, format!("子電路「{}」的埠名稱重複：{}", def_name, dup.join("、")));
        return;
    }
    cx.blocks += 1;
    let k = cx.blocks;
    let ox = SPACING * k as f64;
    let lp = format!("{}{}{}", SEP, full, SEP);

    // local variables: the definition's parameter files, with the block's own values in their place
    let defaults = def_vars(def);
    let own = parse_vars(b.params.get("vars").map(|s| s.as_str()).unwrap_or(""));
    let local = |n: &str| format!("_x{}_{}", k, n.to_lowercase());
    let mut map: HashMap<String, String> = HashMap::new();
    for (n, _) in defaults.iter().chain(own.iter()) {
        map.insert(n.to_lowercase(), local(n));
    }
    for (n, _) in &defaults {
        if reserved(n) {
            err(cx, format!("子電路「{}」的參數名稱「{}」保留給元件內的運算式使用（t、x、v、i、x1…、y1…），請改名", def_name, n));
            return;
        }
    }
    let emit = |cx: &mut Ctx, n: &str, ex: String, what: &str| match expr::eval_const(&ex, &cx.syms) {
        Ok(v) => {
            cx.syms.insert(local(n), v);
            cx.params += &format!("{} = {}\n", local(n), ex);
        }
        Err(e) => cx.errors.push(NlError { msg: format!("子電路 {}（{}）的參數 {}{}：{}", full, def_name, n, what, e.0), els: vec![top] }),
    };
    for (n, ex) in &own {
        if !defaults.iter().any(|(m, _)| m.eq_ignore_ascii_case(n)) {
            emit(cx, n, expr::rename_idents(ex, scope), "（方塊指定）");
        }
    }
    for e in def.elements.iter().filter(|e| e.typ == "PARAMS" && !e.dis) {
        let mut done: HashSet<String> = HashSet::new();
        for line in e.params.get("text").map(|s| s.as_str()).unwrap_or("").split('\n') {
            match expr::param_line(line) {
                None => {}
                Some(None) => cx.errors.push(NlError { msg: format!("子電路「{}」的參數檔格式錯誤：{}", def_name, line.trim()), els: vec![top] }),
                Some(Some((n, ex))) => {
                    let key = n.to_lowercase();
                    match own.iter().find(|(m, _)| m.eq_ignore_ascii_case(&n)) {
                        // the block's value replaces the first definition; a later one would undo it
                        Some((_, v)) => {
                            if done.insert(key) {
                                emit(cx, &n, expr::rename_idents(v, scope), "（方塊指定）");
                            }
                        }
                        None => emit(cx, &n, expr::rename_idents(&ex, &map), ""),
                    }
                }
            }
        }
    }

    // elements and wires
    let mut inner: Vec<Element> = Vec::new();
    for e in def.elements.iter().filter(|e| !e.dis && !skipped(&e.typ)) {
        let Some(d) = lib().get(&e.typ) else { continue };
        let id = cx.nid;
        cx.nid += 1;
        let el = match e.typ.as_str() {
            "SUBPORT" => label(id, e.x + ox, e.y, format!("{}{}", lp, port_name(e))),
            "LABEL" => label(id, e.x + ox, e.y, format!("{}{}", lp, e.params.get("text").map(|s| s.trim()).unwrap_or(""))),
            _ => {
                let mut c = e.clone();
                c.id = id;
                c.x += ox;
                if !d.noname() {
                    c.name = format!("{}.{}", full, e.name);
                }
                if e.typ != "SUBCKT" {
                    for v in c.params.values_mut() {
                        *v = expr::rename_idents(v, &map);
                    }
                }
                c
            }
        };
        cx.origin.insert(id, Origin { top, def: def_name.clone(), local: e.id });
        inner.push(el);
    }
    for w in &def.wires {
        let id = cx.nid;
        cx.nid += 1;
        cx.out.wires.push(Wire { id, x1: w.x1 + ox, y1: w.y1, x2: w.x2 + ox, y2: w.y2 });
    }
    // the block's pins: labels named like the ports inside
    for p in &pins {
        let id = cx.nid;
        cx.nid += 1;
        cx.out.elements.push(label(id, p.x, p.y, format!("{}{}", lp, p.n)));
        cx.origin.insert(id, Origin { top, def: def_name.clone(), local: b.id });
    }
    let nested: Vec<Element> = inner.iter().filter(|e| e.typ == "SUBCKT").cloned().collect();
    cx.out.elements.extend(inner);
    path.push(def_name.clone());
    for n in &nested {
        let local_name = n.name.rsplit('.').next().unwrap_or("").to_string();
        let mut nb = n.clone();
        nb.name = local_name;
        expand(cx, &nb, &format!("{}.", full), &map, top, path);
    }
    path.pop();
}

/// names of the definitions a document uses, directly or through other definitions (for copying blocks
/// between circuits)
pub fn used_defs(root: &Doc, els: &[Element]) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut todo: Vec<String> = els.iter().filter(|e| e.typ == "SUBCKT").filter_map(|e| e.params.get("def").cloned()).collect();
    while let Some(n) = todo.pop() {
        if out.contains(&n) {
            continue;
        }
        if let Some(d) = root.subckts.get(&n) {
            todo.extend(d.elements.iter().filter(|e| e.typ == "SUBCKT").filter_map(|e| e.params.get("def").cloned()));
        }
        out.push(n);
    }
    out
}

/// blocks (in the main circuit and in definitions) that use definition `name`
pub fn users(root: &Doc, name: &str) -> usize {
    let uses = |els: &Vec<Element>| els.iter().filter(|e| e.typ == "SUBCKT" && e.params.get("def").map(|d| d == name).unwrap_or(false)).count();
    uses(&root.elements) + root.subckts.iter().filter(|(k, _)| k.as_str() != name).map(|(_, d)| uses(&d.elements)).sum::<usize>()
}

/// rename a definition and every block that uses it
pub fn rename_def(root: &mut Doc, from: &str, to: &str) -> bool {
    if from == to || root.subckts.contains_key(to) {
        return false;
    }
    let Some(d) = root.subckts.remove(from) else { return false };
    root.subckts.insert(to.to_string(), d);
    let fix = |els: &mut Vec<Element>| {
        for e in els.iter_mut().filter(|e| e.typ == "SUBCKT") {
            if e.params.get("def").map(|d| d == from).unwrap_or(false) {
                e.params.insert("def".into(), to.to_string());
            }
        }
    };
    fix(&mut root.elements);
    for d in root.subckts.values_mut() {
        fix(&mut d.elements);
    }
    true
}

/// a definition name: not empty, no line breaks or control characters
pub fn valid_def_name(s: &str) -> Result<String, String> {
    let t = s.trim();
    if t.is_empty() {
        return Err("名稱不可空白".into());
    }
    if t.chars().any(|c| c.is_control()) {
        return Err("名稱不可含控制字元".into());
    }
    if t.chars().count() > 40 {
        return Err("名稱最多 40 個字".into());
    }
    Ok(t.to_string())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::netlist::build_netlist;

    fn el(id: i64, typ: &str, name: &str, x: f64, y: f64, rot: i64, params: &[(&str, &str)]) -> Element {
        let mut p = lib().default_params(typ);
        for (k, v) in params {
            p.insert(k.to_string(), v.to_string());
        }
        Element { id, typ: typ.into(), x, y, rot, mir: false, dis: false, name: name.into(), params: p, disp: None }
    }
    fn w(id: i64, x1: f64, y1: f64, x2: f64, y2: f64) -> Wire {
        Wire { id, x1, y1, x2, y2 }
    }
    /// a resistor divider definition: port in (left) – R1 (Rt) – port out (right) – R2 (Rb) – ground
    fn divider() -> Doc {
        Doc {
            elements: vec![
                el(1, "SUBPORT", "", -4.0, 0.0, 0, &[("text", "in"), ("side", "L")]),
                el(2, "R", "R1", -2.0, 0.0, 0, &[("R", "Rt")]),
                el(3, "SUBPORT", "", 4.0, 0.0, 2, &[("text", "out"), ("side", "R")]),
                el(4, "R", "R2", 2.0, 2.0, 1, &[("R", "Rb")]),
                el(5, "GND", "", 2.0, 4.0, 0, &[]),
                el(6, "PARAMS", "", 0.0, -4.0, 0, &[("text", "Rt = 10\nRb = Rt")]),
            ],
            // R1 spans -4..0; R2 (rot 1) spans y 0..4 at x = 2
            wires: vec![w(10, 0.0, 0.0, 4.0, 0.0)],
            nid: 20,
            subckts: BTreeMap::new(),
        }
    }
    fn main_with(blocks: &[(&str, &str)]) -> Doc {
        let mut d = Doc { elements: vec![], wires: vec![], nid: 1, subckts: BTreeMap::new() };
        d.subckts.insert("DIV".into(), divider());
        // V1: + at (-10, 0), − at (-10, 4); a bus at x = -6 feeds the blocks
        d.elements.push(el(1, "VDC", "V1", -10.0, 2.0, 0, &[("V", "12")]));
        d.elements.push(el(2, "GND", "", -10.0, 4.0, 0, &[]));
        d.wires.push(w(3, -10.0, 0.0, -6.0, 0.0));
        let mut id = 10;
        for (i, (name, vars)) in blocks.iter().enumerate() {
            let y = i as f64 * 10.0;
            let mut b = el(id, "SUBCKT", name, 0.0, y, 0, &[("def", "DIV"), ("vars", vars)]);
            b.params.insert("ports".into(), crate::library::ports_str(&def_ports(&d.subckts["DIV"])));
            let pins = wpins(&b);
            d.elements.push(b);
            if y != 0.0 {
                d.wires.push(w(id + 4, -6.0, y - 10.0, -6.0, y));
            }
            d.wires.push(w(id + 1, -6.0, y, pins[0].x, pins[0].y));
            // a voltage probe (pin 2 below its origin) on the output
            d.elements.push(el(id + 2, "VP", &format!("Vo{}", i + 1), pins[1].x + 2.0, pins[1].y - 2.0, 0, &[]));
            d.wires.push(w(id + 3, pins[1].x, pins[1].y, pins[1].x + 2.0, pins[1].y));
            id += 10;
        }
        d.elements.push(el(100, "SIMCTRL", "Simulation Control", 20.0, -10.0, 0, &[("dt", "1u"), ("tt", "10u")]));
        d.nid = 200;
        d
    }

    #[test]
    fn ports_in_order() {
        let p = def_ports(&divider());
        assert_eq!(p, vec![SubPort { name: "in".into(), side: 'L', kind: "p".into() }, SubPort { name: "out".into(), side: 'R', kind: "p".into() }]);
    }

    #[test]
    fn flattens_with_local_variables() {
        let d = main_with(&[("X1", ""), ("X2", "Rb = 30")]);
        let br = build_netlist(&d);
        assert!(br.errors.is_empty(), "{:?}", br.errors);
        let names: Vec<&str> = br.net.elements.iter().map(|e| e.name.as_str()).collect();
        for n in ["X1.R1", "X1.R2", "X2.R1", "X2.R2", "Vo1", "Vo2"] {
            assert!(names.contains(&n), "{} missing in {:?}", n, names);
        }
        let (syms, perr) = expr::parse_param_text(&br.net.param_text);
        assert!(perr.is_empty(), "{:?}", perr);
        let val = |n: &str, k: &str| {
            let e = br.net.elements.iter().find(|e| e.name == n).unwrap();
            expr::eval_const(&e.params[k], &syms).unwrap()
        };
        assert_eq!(val("X1.R1", "R"), 10.0);
        assert_eq!(val("X1.R2", "R"), 10.0);
        assert_eq!(val("X2.R2", "R"), 30.0);
        // the divider outputs: both blocks see 12 V at the input
        let net_of = |n: &str, pin: usize| br.net.elements.iter().find(|e| e.name == n).unwrap().pins[pin];
        let v1p = net_of("V1", 0);
        assert_eq!(net_of("X1.R1", 0), v1p);
        assert_eq!(net_of("X2.R1", 0), v1p);
        assert_eq!(net_of("Vo1", 0), net_of("X1.R2", 0));
        assert_ne!(net_of("Vo1", 0), net_of("Vo2", 0));
    }

    #[test]
    fn simulates_divider() {
        let d = main_with(&[("X1", ""), ("X2", "Rb = 30")]);
        let br = build_netlist(&d);
        assert!(br.errors.is_empty(), "{:?}", br.errors);
        let sc = crate::runner::eval_sim_ctrl(&d, &br.syms).unwrap();
        let mut r = crate::runner::TransientRunner::new(&br.net, &sc).unwrap();
        while !r.run_steps(10_000).unwrap() {}
        let res = r.result();
        let last = |n: &str| *res.data[res.names.iter().position(|m| m == n).unwrap()].last().unwrap();
        assert!((last("Vo1") - 6.0).abs() < 1e-9, "{}", last("Vo1"));
        assert!((last("Vo2") - 9.0).abs() < 1e-9, "{}", last("Vo2"));
    }

    /// a port added above "in" moves "in" down: the wire on it follows, nothing lands on the new pin
    #[test]
    fn wires_follow_moved_pins() {
        let mut d = main_with(&[("X1", "")]);
        d.subckts.get_mut("DIV").unwrap().elements.push(el(30, "SUBPORT", "", -4.0, -6.0, 0, &[("text", "P1"), ("side", "L")]));
        assert_eq!(d.sync_subckt_ports(), 1);
        let b = d.elements.iter().find(|e| e.name == "X1").unwrap();
        let pins = wpins(b);
        assert_eq!(pins.iter().map(|p| p.n.as_str()).collect::<Vec<_>>(), vec!["P1", "in", "out"]);
        let br = build_netlist(&d);
        assert!(br.errors.is_empty(), "{:?}", br.errors);
        let net_of = |n: &str, pin: usize| br.net.elements.iter().find(|e| e.name == n).unwrap().pins[pin];
        assert_eq!(net_of("X1.R1", 0), net_of("V1", 0));
        assert_eq!(net_of("Vo1", 0), net_of("X1.R2", 0));
        // every wire is still horizontal or vertical, and the new pin P1 is connected to nothing
        assert!(d.wires.iter().all(|w| w.x1 == w.x2 || w.y1 == w.y2));
        let els: Vec<&Element> = d.elements.iter().collect();
        let c = crate::netlist::connectivity(&d, &els);
        assert_eq!(c.members(c.root(pins[0].x, pins[0].y).unwrap()), 1, "P1 touches a wire");
        assert_eq!(c.root(pins[1].x, pins[1].y), c.root(-10.0, 0.0), "in lost its wire");
    }

    #[test]
    fn nested_and_recursive() {
        let mut d = main_with(&[("X1", "")]);
        // OUTER contains one DIV block whose ports are wired to OUTER's own ports; Rt is passed down
        let mut outer = Doc { elements: vec![], wires: vec![], nid: 50, subckts: BTreeMap::new() };
        outer.elements.push(el(1, "SUBPORT", "", -10.0, 0.0, 0, &[("text", "a"), ("side", "L")]));
        outer.elements.push(el(2, "SUBPORT", "", 10.0, 0.0, 2, &[("text", "b"), ("side", "R")]));
        outer.elements.push(el(3, "PARAMS", "", 0.0, -6.0, 0, &[("text", "Rtop = 20")]));
        let mut inner = el(4, "SUBCKT", "X9", 0.0, 0.0, 0, &[("def", "DIV"), ("vars", "Rt = Rtop")]);
        inner.params.insert("ports".into(), crate::library::ports_str(&def_ports(&d.subckts["DIV"])));
        let ip = wpins(&inner);
        outer.wires.push(w(5, -10.0, 0.0, ip[0].x, ip[0].y));
        outer.wires.push(w(6, ip[1].x, ip[1].y, 10.0, 0.0));
        outer.elements.push(inner);
        d.subckts.insert("OUTER".into(), outer);
        let b = d.elements.iter_mut().find(|e| e.name == "X1").unwrap();
        b.params.insert("def".into(), "OUTER".into());
        d.sync_subckt_ports();
        let br = build_netlist(&d);
        assert!(br.errors.is_empty(), "{:?}", br.errors);
        let (syms, _) = expr::parse_param_text(&br.net.param_text);
        let r = br.net.elements.iter().find(|e| e.name == "X1.X9.R1").expect("nested name");
        assert_eq!(expr::eval_const(&r.params["R"], &syms).unwrap(), 20.0);
        let r2 = br.net.elements.iter().find(|e| e.name == "X1.X9.R2").unwrap();
        assert_eq!(expr::eval_const(&r2.params["R"], &syms).unwrap(), 20.0);
        // DIV containing OUTER: a loop
        let mut lp = el(30, "SUBCKT", "X5", 0.0, 20.0, 0, &[("def", "OUTER")]);
        lp.params.insert("ports".into(), crate::library::ports_str(&def_ports(&d.subckts["OUTER"])));
        d.subckts.get_mut("DIV").unwrap().elements.push(lp);
        let br = build_netlist(&d);
        assert!(br.errors.iter().any(|e| e.msg.contains("包含自己")), "{:?}", br.errors);
        assert!(br.errors.iter().all(|e| e.els == vec![10]), "{:?}", br.errors);
    }

    #[test]
    fn errors_point_at_the_block() {
        let mut d = main_with(&[("X1", "Rb = nosuch")]);
        let br = build_netlist(&d);
        assert!(br.errors.iter().any(|e| e.msg.contains("X1") && e.els == vec![10]), "{:?}", br.errors);
        d.elements.iter_mut().find(|e| e.name == "X1").unwrap().params.insert("def".into(), "NONE".into());
        let br = build_netlist(&d);
        assert!(br.errors.iter().any(|e| e.msg.contains("找不到子電路定義")), "{:?}", br.errors);
    }

    #[test]
    fn file_round_trip_keeps_definitions() {
        let d = main_with(&[("X1", "")]);
        let txt = d.to_json("V0", None);
        let (back, _) = Doc::from_json(&txt).unwrap();
        assert_eq!(back.subckts.len(), 1);
        assert_eq!(back.subckts["DIV"].elements.len(), divider().elements.len());
        // a file without definitions is written as before (no "subckts" key)
        let plain = Doc { elements: vec![], wires: vec![], nid: 1, subckts: BTreeMap::new() };
        assert!(!plain.to_json("V0", None).contains("subckts"));
    }
}
