//! Schematic document editing (no drawing): selection, undo/redo, placement, wires, clipboard.

use daedalus_core::doc::{Doc, Element, Wire};
use daedalus_core::library::{lib, tpt, wpins};
use daedalus_core::netlist::{connectivity, Conn};
use std::collections::{BTreeMap, HashMap, HashSet};

#[derive(Clone, Copy, PartialEq, Eq, Hash, Debug)]
pub enum Sel {
    El(i64),
    W(i64),
}

#[derive(Clone, Debug, Default)]
pub struct Group {
    pub elements: Vec<Element>,
    pub wires: Vec<Wire>,
}

pub struct Editor {
    pub doc: Doc,
    pub sel: HashSet<Sel>,
    undo: Vec<Doc>,
    redo: Vec<Doc>,
    pub dirty: bool,
    pub rev: u64,
    conn: Option<(u64, Conn)>,
    pub clip: Option<Group>,
}

pub struct BBox {
    pub x0: f64,
    pub y0: f64,
    pub x1: f64,
    pub y1: f64,
}

/// element bounding box in world coordinates
pub fn wbox(el: &Element) -> BBox {
    let d = match lib().get(&el.typ) {
        Some(d) => d,
        None => return BBox { x0: el.x, y0: el.y, x1: el.x, y1: el.y },
    };
    let [mut x0, mut y0, mut x1, mut y1] = d.bb;
    if el.typ == "LABEL" {
        x1 = (0.62 * el.params.get("text").map(|s| s.chars().count()).unwrap_or(0) as f64 + 0.9).max(1.6);
    }
    if el.typ == "TEXT" {
        let t = el.params.get("text").cloned().unwrap_or_default();
        let lines: Vec<&str> = t.split('\n').collect();
        x1 = (lines.iter().map(|l| l.chars().count()).max().unwrap_or(0) as f64 * 0.62 + 0.4).max(1.0);
        y1 = 0.4 + (lines.len() as f64 - 1.0) * 1.1;
    }
    if el.typ == "CBLOCK" {
        let pins = lib().pins_for(el);
        let n = pins.iter().filter(|p| p.k == "ci").count().max(1) as f64;
        let m = pins.iter().filter(|p| p.k == "co").count().max(1) as f64;
        let hh = n.max(m) / 2.0 + 0.4;
        y0 = -hh;
        y1 = hh;
    }
    let c = [tpt(el, x0, y0), tpt(el, x1, y0), tpt(el, x0, y1), tpt(el, x1, y1)];
    BBox { x0: c.iter().map(|p| p.0).fold(f64::INFINITY, f64::min), y0: c.iter().map(|p| p.1).fold(f64::INFINITY, f64::min), x1: c.iter().map(|p| p.0).fold(f64::NEG_INFINITY, f64::max), y1: c.iter().map(|p| p.1).fold(f64::NEG_INFINITY, f64::max) }
}

/// L-shaped route from a to b (horizontal first when the horizontal distance dominates)
pub fn l_path(a: (f64, f64), b: (f64, f64)) -> Vec<(f64, f64)> {
    if a.0 == b.0 || a.1 == b.1 {
        return vec![a, b];
    }
    if (b.0 - a.0).abs() >= (b.1 - a.1).abs() {
        vec![a, (b.0, a.1), b]
    } else {
        vec![a, (a.0, b.1), b]
    }
}

fn dist_seg(px: f64, py: f64, w: &Wire) -> f64 {
    let (dx, dy) = (w.x2 - w.x1, w.y2 - w.y1);
    let l2 = dx * dx + dy * dy;
    let t = if l2 > 0.0 { (((px - w.x1) * dx + (py - w.y1) * dy) / l2).clamp(0.0, 1.0) } else { 0.0 };
    let (cx, cy) = (w.x1 + t * dx, w.y1 + t * dy);
    ((px - cx).powi(2) + (py - cy).powi(2)).sqrt()
}

impl Editor {
    pub fn new() -> Self {
        Editor { doc: Doc { elements: vec![], wires: vec![], nid: 1 }, sel: HashSet::new(), undo: vec![], redo: vec![], dirty: false, rev: 0, conn: None, clip: None }
    }
    pub fn set_doc(&mut self, doc: Doc) {
        self.push_undo();
        self.doc = doc;
        self.sel.clear();
        self.changed();
        self.dirty = false;
    }
    pub fn push_undo(&mut self) {
        self.undo.push(self.doc.clone());
        if self.undo.len() > 200 {
            self.undo.remove(0);
        }
        self.redo.clear();
    }
    pub fn changed(&mut self) {
        self.dirty = true;
        self.rev += 1;
    }
    pub fn can_undo(&self) -> bool {
        !self.undo.is_empty()
    }
    pub fn can_redo(&self) -> bool {
        !self.redo.is_empty()
    }
    pub fn undo(&mut self) {
        if let Some(d) = self.undo.pop() {
            self.redo.push(std::mem::replace(&mut self.doc, d));
            self.sel.clear();
            self.changed();
        }
    }
    pub fn redo(&mut self) {
        if let Some(d) = self.redo.pop() {
            self.undo.push(std::mem::replace(&mut self.doc, d));
            self.sel.clear();
            self.changed();
        }
    }
    /// recompute connectivity when the document changed
    pub fn ensure_conn(&mut self) {
        let stale = self.conn.as_ref().map(|c| c.0 != self.rev).unwrap_or(true);
        if stale {
            let els: Vec<&Element> = self.doc.elements.iter().collect();
            let c = connectivity(&self.doc, &els);
            self.conn = Some((self.rev, c));
        }
    }
    /// connectivity (call ensure_conn first)
    pub fn conn(&self) -> &Conn {
        &self.conn.as_ref().expect("ensure_conn").1
    }
    pub fn el(&self, id: i64) -> Option<&Element> {
        self.doc.elements.iter().find(|e| e.id == id)
    }
    pub fn el_mut(&mut self, id: i64) -> Option<&mut Element> {
        self.doc.elements.iter_mut().find(|e| e.id == id)
    }
    pub fn unique_name(&self, pre: &str) -> String {
        let used: HashSet<&str> = self.doc.elements.iter().map(|e| e.name.as_str()).collect();
        for i in 1.. {
            let n = format!("{}{}", pre, i);
            if !used.contains(n.as_str()) {
                return n;
            }
        }
        unreachable!()
    }
    pub fn new_element(&mut self, typ: &str, x: f64, y: f64, rot: i64, mir: bool) -> Element {
        let d = lib().get(typ).unwrap();
        let id = self.doc.nid;
        self.doc.nid += 1;
        let name = match typ {
            "ACSWEEP" => "ACSWEEP".to_string(),
            "SIMCTRL" => "Simulation Control".to_string(),
            _ if d.noname() => String::new(),
            _ => self.unique_name(&d.pre),
        };
        Element { id, typ: typ.into(), x, y, rot, mir, dis: false, name, params: lib().default_params(typ), disp: None }
    }
    /// place a copy of a prototype element; returns the new id or an error message
    pub fn place_element(&mut self, proto: &Element, x: f64, y: f64) -> Result<i64, String> {
        let d = lib().get(&proto.typ).unwrap();
        if d.has("single") && self.doc.elements.iter().any(|e| e.typ == proto.typ) {
            return Err(format!("每個電路只能有一個「{}」元件", d.name));
        }
        self.push_undo();
        let mut el = self.new_element(&proto.typ, x, y, proto.rot, proto.mir);
        el.dis = proto.dis;
        for (k, v) in &proto.params {
            el.params.insert(k.clone(), v.clone());
        }
        el.disp = proto.disp.clone();
        let id = el.id;
        self.doc.elements.push(el);
        self.sel.clear();
        self.sel.insert(Sel::El(id));
        self.changed();
        Ok(id)
    }
    pub fn add_wire(&mut self, x1: f64, y1: f64, x2: f64, y2: f64) -> Option<i64> {
        if x1 == x2 && y1 == y2 {
            return None;
        }
        let id = self.doc.nid;
        self.doc.nid += 1;
        self.doc.wires.push(Wire { id, x1, y1, x2, y2 });
        Some(id)
    }
    /// add an L-shaped wire path a -> b (one undo step)
    pub fn add_route(&mut self, pts: &[(f64, f64)]) -> usize {
        if pts.len() < 2 {
            return 0;
        }
        self.push_undo();
        let mut n = 0;
        for w in pts.windows(2) {
            if self.add_wire(w[0].0, w[0].1, w[1].0, w[1].1).is_some() {
                n += 1;
            }
        }
        if n > 0 {
            self.merge_collinear();
            self.changed();
        }
        n
    }
    /// join consecutive collinear wire segments that share an endpoint not used by anything else
    fn merge_collinear(&mut self) {
        loop {
            let mut changed = false;
            let mut uses: HashMap<(i64, i64), usize> = HashMap::new();
            let k = |x: f64, y: f64| ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64);
            for w in &self.doc.wires {
                *uses.entry(k(w.x1, w.y1)).or_insert(0) += 1;
                *uses.entry(k(w.x2, w.y2)).or_insert(0) += 1;
            }
            for e in &self.doc.elements {
                for p in wpins(e) {
                    *uses.entry(k(p.x, p.y)).or_insert(0) += 10;
                }
            }
            'outer: for i in 0..self.doc.wires.len() {
                for j in 0..self.doc.wires.len() {
                    if i == j {
                        continue;
                    }
                    let (a, b) = (&self.doc.wires[i], &self.doc.wires[j]);
                    let ha = a.y1 == a.y2;
                    let hb = b.y1 == b.y2;
                    if ha != hb {
                        continue;
                    }
                    if ha && a.y1 != b.y1 {
                        continue;
                    }
                    if !ha && a.x1 != b.x1 {
                        continue;
                    }
                    // shared endpoint
                    let ends_a = [(a.x1, a.y1), (a.x2, a.y2)];
                    let ends_b = [(b.x1, b.y1), (b.x2, b.y2)];
                    for (ia, pa) in ends_a.iter().enumerate() {
                        for (ib, pb) in ends_b.iter().enumerate() {
                            if pa == pb && uses.get(&k(pa.0, pa.1)).copied().unwrap_or(0) == 2 {
                                let oa = ends_a[1 - ia];
                                let ob = ends_b[1 - ib];
                                // must extend in opposite directions (not overlap)
                                let da = (oa.0 - pa.0, oa.1 - pa.1);
                                let db = (ob.0 - pb.0, ob.1 - pb.1);
                                if da.0 * db.0 + da.1 * db.1 >= 0.0 {
                                    continue;
                                }
                                let aid = a.id;
                                let nw = Wire { id: aid, x1: oa.0, y1: oa.1, x2: ob.0, y2: ob.1 };
                                let bid = b.id;
                                self.doc.wires[i] = nw;
                                self.doc.wires.retain(|w| w.id != bid);
                                if self.sel.remove(&Sel::W(bid)) {
                                    self.sel.insert(Sel::W(aid));
                                }
                                changed = true;
                                break 'outer;
                            }
                        }
                    }
                }
            }
            if !changed {
                break;
            }
        }
    }
    pub fn selected_els(&self) -> Vec<i64> {
        self.doc.elements.iter().filter(|e| self.sel.contains(&Sel::El(e.id))).map(|e| e.id).collect()
    }
    pub fn selected_wires(&self) -> Vec<i64> {
        self.doc.wires.iter().filter(|w| self.sel.contains(&Sel::W(w.id))).map(|w| w.id).collect()
    }
    pub fn delete_sel(&mut self) -> (usize, usize) {
        if self.sel.is_empty() {
            return (0, 0);
        }
        self.push_undo();
        let (ne, nw) = (self.doc.elements.len(), self.doc.wires.len());
        let sel = self.sel.clone();
        self.doc.elements.retain(|e| !sel.contains(&Sel::El(e.id)));
        self.doc.wires.retain(|w| !sel.contains(&Sel::W(w.id)));
        self.sel.clear();
        self.changed();
        (ne - self.doc.elements.len(), nw - self.doc.wires.len())
    }
    pub fn rotate_sel(&mut self, dir: i64) {
        let els = self.selected_els();
        let ws = self.selected_wires();
        if els.is_empty() && ws.is_empty() {
            return;
        }
        self.push_undo();
        if els.len() == 1 && ws.is_empty() {
            let e = self.el_mut(els[0]).unwrap();
            e.rot = (e.rot + dir + 4) & 3;
        } else {
            let mut pts = vec![];
            for e in &self.doc.elements {
                if els.contains(&e.id) {
                    pts.push((e.x, e.y));
                }
            }
            for w in &self.doc.wires {
                if ws.contains(&w.id) {
                    pts.push((w.x1, w.y1));
                    pts.push((w.x2, w.y2));
                }
            }
            let cx = (pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64).round();
            let cy = (pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64).round();
            let r = |x: f64, y: f64| if dir > 0 { (cx - (y - cy), cy + (x - cx)) } else { (cx + (y - cy), cy - (x - cx)) };
            for e in self.doc.elements.iter_mut() {
                if els.contains(&e.id) {
                    let (x, y) = r(e.x, e.y);
                    e.x = x;
                    e.y = y;
                    e.rot = (e.rot + dir + 4) & 3;
                }
            }
            for w in self.doc.wires.iter_mut() {
                if ws.contains(&w.id) {
                    let (a, b) = r(w.x1, w.y1);
                    let (c, d) = r(w.x2, w.y2);
                    w.x1 = a;
                    w.y1 = b;
                    w.x2 = c;
                    w.y2 = d;
                }
            }
        }
        self.changed();
    }
    pub fn mirror_sel(&mut self, vertical: bool) {
        let els = self.selected_els();
        if els.is_empty() {
            return;
        }
        self.push_undo();
        for e in self.doc.elements.iter_mut() {
            if els.contains(&e.id) {
                e.mir = !e.mir;
                if vertical {
                    e.rot = (e.rot + 2) & 3;
                }
            }
        }
        self.changed();
    }
    pub fn toggle_disable(&mut self) -> Option<(bool, usize)> {
        let els = self.selected_els();
        if els.is_empty() {
            return None;
        }
        self.push_undo();
        let to = !els.iter().all(|id| self.el(*id).map(|e| e.dis).unwrap_or(false));
        for e in self.doc.elements.iter_mut() {
            if els.contains(&e.id) {
                e.dis = to;
            }
        }
        self.changed();
        Some((to, els.len()))
    }
    pub fn copy_sel(&mut self) -> Option<(usize, usize)> {
        let els = self.selected_els();
        let ws = self.selected_wires();
        if els.is_empty() && ws.is_empty() {
            return None;
        }
        let mut pts = vec![];
        for e in &self.doc.elements {
            if els.contains(&e.id) {
                pts.push((e.x, e.y));
            }
        }
        for w in &self.doc.wires {
            if ws.contains(&w.id) {
                pts.push((w.x1, w.y1));
            }
        }
        let cx = pts.iter().map(|p| p.0).fold(f64::INFINITY, f64::min).round();
        let cy = pts.iter().map(|p| p.1).fold(f64::INFINITY, f64::min).round();
        let g = Group {
            elements: self.doc.elements.iter().filter(|e| els.contains(&e.id)).map(|e| Element { x: e.x - cx, y: e.y - cy, ..e.clone() }).collect(),
            wires: self.doc.wires.iter().filter(|w| ws.contains(&w.id)).map(|w| Wire { id: 0, x1: w.x1 - cx, y1: w.y1 - cy, x2: w.x2 - cx, y2: w.y2 - cy }).collect(),
        };
        self.clip = Some(g);
        Some((els.len(), ws.len()))
    }
    pub fn place_group(&mut self, g: &Group, mx: f64, my: f64) {
        self.push_undo();
        self.sel.clear();
        for e0 in &g.elements {
            let mut el = e0.clone();
            el.id = self.doc.nid;
            self.doc.nid += 1;
            el.x += mx;
            el.y += my;
            let d = lib().get(&el.typ).unwrap();
            if !d.noname() && el.typ != "SIMCTRL" && el.typ != "ACSWEEP" && self.doc.elements.iter().any(|o| o.name == el.name) {
                el.name = self.unique_name(&d.pre);
            }
            if d.has("single") && self.doc.elements.iter().any(|o| o.typ == el.typ) {
                continue;
            }
            let id = el.id;
            self.doc.elements.push(el);
            self.sel.insert(Sel::El(id));
        }
        for w in &g.wires {
            if let Some(id) = self.add_wire(w.x1 + mx, w.y1 + my, w.x2 + mx, w.y2 + my) {
                self.sel.insert(Sel::W(id));
            }
        }
        self.changed();
    }
    /// move the selection by (dx, dy) relative to `base`, stretching attached wires (rubber banding)
    pub fn move_selection(&mut self, base: &Doc, dx: f64, dy: f64) {
        let key = |x: f64, y: f64| ((x * 1000.0).round() as i64, (y * 1000.0).round() as i64);
        let mut moved: HashSet<(i64, i64)> = HashSet::new();
        for e in &base.elements {
            if self.sel.contains(&Sel::El(e.id)) {
                for p in wpins(e) {
                    moved.insert(key(p.x, p.y));
                }
            }
        }
        for w in &base.wires {
            if self.sel.contains(&Sel::W(w.id)) {
                moved.insert(key(w.x1, w.y1));
                moved.insert(key(w.x2, w.y2));
            }
        }
        self.doc.elements = base.elements.iter().map(|e| if self.sel.contains(&Sel::El(e.id)) { Element { x: e.x + dx, y: e.y + dy, ..e.clone() } } else { e.clone() }).collect();
        let mut out: Vec<Wire> = Vec::new();
        let mut nid = base.nid.max(self.doc.nid);
        for w0 in &base.wires {
            if self.sel.contains(&Sel::W(w0.id)) {
                out.push(Wire { id: w0.id, x1: w0.x1 + dx, y1: w0.y1 + dy, x2: w0.x2 + dx, y2: w0.y2 + dy });
                continue;
            }
            let m1 = moved.contains(&key(w0.x1, w0.y1));
            let m2 = moved.contains(&key(w0.x2, w0.y2));
            if !m1 && !m2 {
                out.push(w0.clone());
                continue;
            }
            let horiz = w0.y1 == w0.y2;
            let mut a = (w0.x1, w0.y1);
            let mut b = (w0.x2, w0.y2);
            if m1 {
                a = (a.0 + dx, a.1 + dy);
            }
            if m2 {
                b = (b.0 + dx, b.1 + dy);
            }
            if a.0 == b.0 || a.1 == b.1 || (m1 && m2) {
                out.push(Wire { id: w0.id, x1: a.0, y1: a.1, x2: b.0, y2: b.1 });
                continue;
            }
            // keep the fixed end's direction, add a corner
            let (fixed, mov) = if m1 { (b, a) } else { (a, b) };
            let corner = if horiz { (mov.0, fixed.1) } else { (fixed.0, mov.1) };
            out.push(Wire { id: w0.id, x1: fixed.0, y1: fixed.1, x2: corner.0, y2: corner.1 });
            out.push(Wire { id: nid, x1: corner.0, y1: corner.1, x2: mov.0, y2: mov.1 });
            nid += 1;
        }
        out.retain(|w| !(w.x1 == w.x2 && w.y1 == w.y2));
        self.doc.wires = out;
        self.doc.nid = nid;
        self.rev += 1;
    }
    pub fn finish_move(&mut self) {
        self.merge_collinear();
        self.changed();
    }

    /* ---------- hit testing (world coordinates) ---------- */
    pub fn hit_element(&self, x: f64, y: f64) -> Option<i64> {
        for e in self.doc.elements.iter().rev() {
            let b = wbox(e);
            let m = 0.15;
            if x >= b.x0 - m && x <= b.x1 + m && y >= b.y0 - m && y <= b.y1 + m {
                return Some(e.id);
            }
        }
        None
    }
    pub fn hit_wire(&self, x: f64, y: f64, tol: f64) -> Option<i64> {
        let mut best: Option<(f64, i64)> = None;
        for w in &self.doc.wires {
            let d = dist_seg(x, y, w);
            if d <= tol && best.map(|b| d < b.0).unwrap_or(true) {
                best = Some((d, w.id));
            }
        }
        best.map(|b| b.1)
    }
    /// nearest pin within tol: (element id, pin index, x, y)
    pub fn near_pin(&self, x: f64, y: f64, tol: f64) -> Option<(i64, usize, f64, f64)> {
        let mut best: Option<(f64, (i64, usize, f64, f64))> = None;
        for e in &self.doc.elements {
            for (i, p) in wpins(e).iter().enumerate() {
                let d = ((p.x - x).powi(2) + (p.y - y).powi(2)).sqrt();
                if d <= tol && best.as_ref().map(|b| d < b.0).unwrap_or(true) {
                    best = Some((d, (e.id, i, p.x, p.y)));
                }
            }
        }
        best.map(|b| b.1)
    }
    /// a grid point where a wire may end (pin, wire end or point on a wire)
    pub fn is_conn_point(&self, x: f64, y: f64) -> bool {
        if self.near_pin(x, y, 1e-6).is_some() {
            return true;
        }
        self.doc.wires.iter().any(|w| dist_seg(x, y, w) < 1e-6)
    }
    pub fn select_in_rect(&mut self, x0: f64, y0: f64, x1: f64, y1: f64, add: bool) {
        if !add {
            self.sel.clear();
        }
        let (ax, bx, ay, by) = (x0.min(x1), x0.max(x1), y0.min(y1), y0.max(y1));
        for e in &self.doc.elements {
            let b = wbox(e);
            if b.x0 >= ax && b.x1 <= bx && b.y0 >= ay && b.y1 <= by {
                self.sel.insert(Sel::El(e.id));
            }
        }
        for w in &self.doc.wires {
            let inside = |x: f64, y: f64| x >= ax && x <= bx && y >= ay && y <= by;
            if inside(w.x1, w.y1) && inside(w.x2, w.y2) {
                self.sel.insert(Sel::W(w.id));
            }
        }
    }
    pub fn select_all(&mut self) {
        self.sel = self.doc.elements.iter().map(|e| Sel::El(e.id)).chain(self.doc.wires.iter().map(|w| Sel::W(w.id))).collect();
    }
    pub fn bounds(&self) -> Option<BBox> {
        let mut b = BBox { x0: f64::INFINITY, y0: f64::INFINITY, x1: f64::NEG_INFINITY, y1: f64::NEG_INFINITY };
        let mut any = false;
        for e in &self.doc.elements {
            let q = wbox(e);
            b.x0 = b.x0.min(q.x0);
            b.y0 = b.y0.min(q.y0);
            b.x1 = b.x1.max(q.x1);
            b.y1 = b.y1.max(q.y1);
            any = true;
        }
        for w in &self.doc.wires {
            b.x0 = b.x0.min(w.x1.min(w.x2));
            b.y0 = b.y0.min(w.y1.min(w.y2));
            b.x1 = b.x1.max(w.x1.max(w.x2));
            b.y1 = b.y1.max(w.y1.max(w.y2));
            any = true;
        }
        if any {
            Some(b)
        } else {
            None
        }
    }
    /// set one parameter (undoable); returns false when unchanged
    pub fn set_params(&mut self, id: i64, name: Option<String>, params: BTreeMap<String, String>, disp: Option<BTreeMap<String, bool>>) {
        self.push_undo();
        if let Some(e) = self.el_mut(id) {
            if let Some(n) = name {
                e.name = n;
            }
            e.params = params;
            if disp.is_some() {
                e.disp = disp;
            }
        }
        self.changed();
    }
}

/// name/parameter lines shown next to an element: (key, text)
pub fn label_lines(el: &Element) -> Vec<(String, String)> {
    let d = match lib().get(&el.typ) {
        Some(d) => d,
        None => return vec![],
    };
    let disp = el.disp.clone().unwrap_or_default();
    let mut out = vec![];
    if !d.noname() && disp.get("name").copied() != Some(false) && !el.name.is_empty() {
        out.push(("__name".to_string(), el.name.clone()));
    }
    for p in &d.params {
        let on = disp.get(&p.k).copied().unwrap_or_else(|| d.show.contains(&p.k));
        let v = el.params.get(&p.k).cloned().unwrap_or_default();
        // an empty list (a new Monte Carlo element's tolerances) shows nothing rather than "vars="
        if on && !(v.trim().is_empty() && p.typ.as_deref() == Some("code")) {
            let numeric = !v.is_empty() && v.chars().all(|c| c.is_ascii_digit() || "-+.eE".contains(c));
            let u = if !p.u.is_empty() && numeric { p.u.clone() } else { String::new() };
            out.push((p.k.clone(), format!("{}={}{}", p.k, v, u)));
        }
    }
    out
}
