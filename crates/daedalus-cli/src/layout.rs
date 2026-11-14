//! Readable automatic layout of a netlist, drawn with wires rather than labels:
//! - power stage: the main current path along a top rail, shunt branches hanging between the rail and a ground
//!   rail, probes on the rail, parallel elements above the element they bridge; with two or more half-bridge
//!   legs, the source net becomes a DC bus with the legs as columns and the load between their midpoints;
//! - control: columns by signal flow (inputs left, outputs right);
//! - everything else is joined by an orthogonal grid router that never touches another net's points;
//! - labels only for signals that cross between the power and the control area (gates, sensor outputs) and for
//!   connections the router cannot make.
//! Ground pins outside the rails get ground symbols. Connectivity follows the simulator's rules; the caller checks
//! the result against the requested nets.
use crate::circuit::{is_ground, label_lines, label_width, pin_dir, text_w, world_box, Circuit, STUB};
use daedalus_core::doc::Element;
use daedalus_core::library::{lib, tpt, PinDef};
use std::cmp::Reverse;
use std::collections::{BTreeMap, BinaryHeap, HashMap, HashSet};

const DROP: f64 = 2.0;
const GAP: f64 = 3.0;
const CURRENT: [&str; 4] = ["IP", "ADCM", "AACM", "ISEN"];
const METER: [&str; 7] = ["VP2", "VDCM", "VACM", "VSEN", "ACP", "ACP2", "WATT"];

pub struct Item {
    pub typ: String,
    pub name: String,
    pub params: BTreeMap<String, String>,
    pub rot: i64,
    pub mir: bool,
    /// pin name -> net (None: not connected)
    pub nets: BTreeMap<String, Option<String>>,
}

#[derive(Clone, Copy, PartialEq)]
enum Zone {
    Head,
    Power,
    Ctl,
}
#[derive(Clone, Copy, PartialEq)]
enum Kind {
    Tap,
    Shunt,
    Series,
    Other,
}

struct It {
    typ: String,
    name: String,
    params: BTreeMap<String, String>,
    rot: i64,
    mir: bool,
    pins: Vec<(PinDef, Option<String>)>,
    zone: Zone,
    kind: Kind,
    /// shunt: (net pin, ground pin); series: (pin a, pin b)
    ends: Option<(PinDef, PinDef)>,
    el: Option<usize>,
    done: bool,
}

fn probe_el(typ: &str, rot: i64, mir: bool, params: &BTreeMap<String, String>, name: &str) -> Element {
    Element { id: 0, typ: typ.into(), x: 0.0, y: 0.0, rot, mir, dis: false, name: name.into(), params: params.clone(), disp: None }
}

type Pt = (i64, i64);
fn ip(x: f64, y: f64) -> Pt {
    (x.round() as i64, y.round() as i64)
}

/* ------------------------------------------------------------------ occupancy and router */

#[derive(Default)]
struct Grid {
    hard: HashMap<Pt, String>,
    h: HashMap<Pt, String>,
    v: HashMap<Pt, String>,
    boxc: HashSet<Pt>,
    soft: HashMap<Pt, u32>,
}

impl Grid {
    fn pin(&mut self, p: Pt, net: &str) {
        self.hard.insert(p, net.to_string());
    }
    fn wire(&mut self, a: Pt, b: Pt, net: &str) {
        self.hard.insert(a, net.to_string());
        self.hard.insert(b, net.to_string());
        if a.1 == b.1 {
            for x in a.0.min(b.0) + 1..a.0.max(b.0) {
                self.h.insert((x, a.1), net.to_string());
            }
        } else {
            for y in a.1.min(b.1) + 1..a.1.max(b.1) {
                self.v.insert((a.0, y), net.to_string());
            }
        }
    }
    fn element(&mut self, el: &Element) {
        if el.typ == "TEXT" {
            return;
        }
        let [x0, y0, x1, y1] = world_box(el);
        for x in x0.ceil() as i64..=x1.floor() as i64 {
            for y in y0.ceil() as i64..=y1.floor() as i64 {
                if (x as f64) > x0 && (x as f64) < x1 && (y as f64) > y0 && (y as f64) < y1 {
                    self.boxc.insert((x, y));
                }
            }
        }
        // name and parameter texts: may be crossed, at a cost
        let lines = label_lines(el);
        if !lines.is_empty() && el.typ != "LABEL" {
            let w = lines.iter().map(|l| text_w(l)).fold(0.0, f64::max) * 0.8;
            let n = lines.len() as f64;
            let r = if x1 - x0 >= y1 - y0 { [(x0 + x1) / 2.0 - w / 2.0, y0 - 0.4 - n * 0.95, (x0 + x1) / 2.0 + w / 2.0, y0] } else { [x1, (y0 + y1) / 2.0 - n * 0.5, x1 + 0.4 + w, (y0 + y1) / 2.0 + n * 0.5] };
            for x in r[0].floor() as i64..=r[2].ceil() as i64 {
                for y in r[1].floor() as i64..=r[3].ceil() as i64 {
                    self.soft.insert((x, y), 3);
                }
            }
        }
    }
}

/// cheapest orthogonal path from `start` to a point of `goals`; it never touches another net's pins or wire ends,
/// never runs along another net's wire, and crosses other wires only straight through (no junction there)
fn route(g: &Grid, net: &str, start: Pt, bounds: [i64; 4], goals: &HashSet<Pt>) -> Option<(Vec<Pt>, u32)> {
    const DX: [Pt; 4] = [(1, 0), (-1, 0), (0, 1), (0, -1)];
    const TURN: u32 = 4;
    const CROSS: u32 = 10;
    let mut dist: HashMap<(i64, i64, u8), u32> = HashMap::new();
    let mut prev: HashMap<(i64, i64, u8), (i64, i64, u8)> = HashMap::new();
    let mut heap = BinaryHeap::new();
    let s0 = (start.0, start.1, 4u8);
    dist.insert(s0, 0);
    heap.push(Reverse((0u32, s0)));
    let other = |m: &HashMap<Pt, String>, k: &Pt| m.get(k).map(|n| n != net).unwrap_or(false);
    let mut n = 0;
    while let Some(Reverse((c, s))) = heap.pop() {
        n += 1;
        if n > 400_000 {
            break;
        }
        if c > *dist.get(&s).unwrap_or(&u32::MAX) {
            continue;
        }
        let (x, y, d) = s;
        for nd in 0..4u8 {
            if d != 4 && (nd ^ 1) == d {
                continue;
            }
            let turning = d != 4 && nd != d;
            if turning && (other(&g.h, &(x, y)) || other(&g.v, &(x, y))) {
                continue; // a corner inside another net's wire would join it
            }
            let (nx, ny) = (x + DX[nd as usize].0, y + DX[nd as usize].1);
            if nx < bounds[0] || nx > bounds[2] || ny < bounds[1] || ny > bounds[3] {
                continue;
            }
            let k = (nx, ny);
            if g.boxc.contains(&k) {
                continue;
            }
            let horiz = nd < 2;
            let (along, across) = if horiz { (g.h.get(&k), g.v.get(&k)) } else { (g.v.get(&k), g.h.get(&k)) };
            let mut cost = c + 1 + if turning { TURN } else { 0 } + g.soft.get(&k).copied().unwrap_or(0);
            let goal = goals.contains(&k);
            if !goal {
                if g.hard.contains_key(&k) || along.is_some() {
                    continue;
                }
                if across.is_some() {
                    cost += CROSS;
                }
            }
            let ns = (nx, ny, nd);
            if goal {
                prev.insert(ns, s);
                let mut pts = vec![(nx, ny)];
                let mut cur = Some(s);
                while let Some(q) = cur {
                    pts.push((q.0, q.1));
                    cur = prev.get(&q).copied();
                }
                pts.reverse();
                let mut out = vec![pts[0]];
                for i in 1..pts.len() - 1 {
                    let a = *out.last().unwrap();
                    let (b, cc) = (pts[i], pts[i + 1]);
                    if !((a.0 == b.0 && b.0 == cc.0) || (a.1 == b.1 && b.1 == cc.1)) {
                        out.push(b);
                    }
                }
                out.push(*pts.last().unwrap());
                return Some((out, cost));
            }
            if cost < *dist.get(&ns).unwrap_or(&u32::MAX) {
                dist.insert(ns, cost);
                prev.insert(ns, s);
                heap.push(Reverse((cost, ns)));
            }
        }
    }
    None
}

/* ------------------------------------------------------------------ the layout */

struct Lay {
    c: Circuit,
    /// net of each wire drawn so far (parallel to c.doc.wires)
    wnet: Vec<String>,
    its: Vec<It>,
}

impl Lay {
    fn wire(&mut self, a: (f64, f64), b: (f64, f64), net: &str) {
        if a == b {
            return;
        }
        self.c.add_wire(a, b);
        self.wnet.push(net.to_string());
    }
    fn place(&mut self, i: usize, x: f64, y: f64, rot: i64, mir: bool) -> usize {
        let it = &self.its[i];
        let (typ, name, params) = (it.typ.clone(), it.name.clone(), it.params.clone());
        let e = self.c.add_element(&typ, &name, x.round(), y.round(), rot, mir, &params);
        self.its[i].el = Some(e);
        self.its[i].done = true;
        e
    }
    fn el(&self, i: usize) -> &Element {
        &self.c.doc.elements[self.its[i].el.unwrap()]
    }
    /// the first rotation (and mirror) for which `want` holds; positions of the pins at the origin
    fn rotations(&self, i: usize, want: impl Fn(&HashMap<String, (f64, f64)>, &Element) -> bool) -> Option<(i64, bool, HashMap<String, (f64, f64)>)> {
        let it = &self.its[i];
        for mir in [false, true] {
            for rot in [0, 1, 3, 2] {
                let el = probe_el(&it.typ, rot, mir, &it.params, &it.name);
                let pos: HashMap<String, (f64, f64)> = lib().pins_for(&el).iter().map(|p| (p.n.clone(), tpt(&el, p.x, p.y))).collect();
                if want(&pos, &el) {
                    return Some((rot, mir, pos));
                }
            }
        }
        None
    }
    fn ctl_pins(&self, i: usize) -> Vec<(PinDef, String)> {
        self.its[i].pins.iter().filter(|(p, n)| p.k != "p" && p.k != "a" && n.is_some()).map(|(p, n)| (p.clone(), n.clone().unwrap())).collect()
    }
    fn terms(&self, i: usize) -> Vec<(PinDef, String)> {
        self.its[i].pins.iter().filter(|(p, n)| (p.k == "p" || p.k == "a") && n.is_some()).map(|(p, n)| (p.clone(), n.clone().unwrap())).collect()
    }
    /// extents relative to the origin, with the element's texts and the labels of its control pins
    fn ext(&self, i: usize, rot: i64, mir: bool) -> [f64; 4] {
        let it = &self.its[i];
        let el = probe_el(&it.typ, rot, mir, &it.params, &it.name);
        let mut b = world_box(&el);
        let lines = label_lines(&el);
        if !lines.is_empty() {
            if b[2] - b[0] >= b[3] - b[1] {
                b[1] -= 0.6 + lines.len() as f64 * 0.95;
            } else {
                b[2] += 0.6 + lines.iter().map(|l| text_w(l)).fold(0.0, f64::max) * 0.8;
            }
        }
        for (p, net) in self.ctl_pins(i) {
            let (px, py) = tpt(&el, p.x, p.y);
            let d = pin_dir(&el, &p);
            let w = STUB + if is_ground(&net) { 1.3 } else { label_width(&net) };
            let tip = (px + d.0 * w, py + d.1 * w);
            b = [b[0].min(tip.0 - 0.7), b[1].min(tip.1 - 0.7), b[2].max(tip.0 + 0.7), b[3].max(tip.1 + 0.7)];
        }
        b
    }
    fn vert_rot(&self, i: usize, top: &str, bot: &str) -> Option<(i64, bool, HashMap<String, (f64, f64)>)> {
        let (t, b) = (top.to_string(), bot.to_string());
        self.rotations(i, move |pos, _| match (pos.get(&t), pos.get(&b)) {
            (Some(a), Some(c)) => a.0 == c.0 && a.1 < c.1,
            _ => false,
        })
    }
    fn hor_rot(&self, i: usize, a: &str, b: &str) -> Option<(i64, bool, HashMap<String, (f64, f64)>)> {
        let ctl: Vec<String> = self.ctl_pins(i).into_iter().map(|(p, _)| p.n).collect();
        let (an, bn) = (a.to_string(), b.to_string());
        let ok = move |pos: &HashMap<String, (f64, f64)>| match (pos.get(&an), pos.get(&bn)) {
            (Some(p), Some(q)) => p.1 == q.1 && p.0 < q.0,
            _ => false,
        };
        let ok2 = ok.clone();
        let an2 = a.to_string();
        // gates toward the control area (down) when possible
        self.rotations(i, move |pos, _| ok(pos) && ctl.iter().all(|c| pos.get(c).map(|q| q.1 > pos[&an2].1).unwrap_or(true))).or_else(|| self.rotations(i, move |pos, _| ok2(pos)))
    }
    fn left_of(&self, i: usize, top: &str, bot: &str) -> f64 {
        match self.vert_rot(i, top, bot) {
            Some((r, m, pos)) => pos[top].0 - self.ext(i, r, m)[0],
            None => 0.0,
        }
    }
    fn right_of(&self, i: usize, top: &str, bot: &str) -> f64 {
        match self.vert_rot(i, top, bot) {
            Some((r, m, pos)) => self.ext(i, r, m)[2] - pos[top].0,
            None => 0.0,
        }
    }
    /// a vertical element hanging from the line at y_top (net `net`), top pin `top`; returns its bottom pin
    fn place_vert(&mut self, i: usize, top: &str, bot: &str, x: f64, y_top: f64, net: &str) -> Option<(f64, f64)> {
        let (r, m, pos) = self.vert_rot(i, top, bot)?;
        let t = pos[top];
        self.place(i, x - t.0, y_top + DROP - t.1, r, m);
        self.wire((x, y_top), (x, y_top + DROP), net);
        Some((x - t.0 + pos[bot].0, y_top + DROP - t.1 + pos[bot].1))
    }
    fn pin_pos(&self, i: usize, pn: &str) -> (f64, f64) {
        let el = self.el(i);
        let p = lib().pins_for(el).into_iter().find(|p| p.n == pn).unwrap();
        tpt(el, p.x, p.y)
    }
}

pub struct Out {
    pub circuit: Circuit,
    pub notes: Vec<String>,
    pub labels: usize,
}

pub fn build(title: Option<&str>, head_items: Vec<Item>, items: Vec<Item>) -> Out {
    let mut lay = Lay { c: Circuit::empty(), wnet: vec![], its: vec![] };
    let mut notes = vec![];
    // power nets: those with a power pin
    let mut power_net: HashSet<String> = HashSet::new();
    for it in &items {
        let el = probe_el(&it.typ, 0, false, &it.params, &it.name);
        for p in lib().pins_for(&el) {
            if p.k == "p" {
                if let Some(Some(n)) = it.nets.get(&p.n) {
                    if !is_ground(n) {
                        power_net.insert(n.clone());
                    }
                }
            }
        }
    }
    for it in head_items.into_iter().chain(items) {
        let el = probe_el(&it.typ, 0, false, &it.params, &it.name);
        let pins: Vec<(PinDef, Option<String>)> = lib().pins_for(&el).into_iter().map(|p| {
            let n = it.nets.get(&p.n).cloned().flatten().map(|n| if is_ground(&n) { "0".to_string() } else { n });
            (p, n)
        }).collect();
        let zone = if pins.is_empty() {
            Zone::Head
        } else if pins.iter().any(|(p, n)| p.k == "p" || (p.k == "a" && n.as_ref().map(|n| power_net.contains(n)).unwrap_or(false))) {
            Zone::Power
        } else {
            Zone::Ctl
        };
        lay.its.push(It { typ: it.typ, name: it.name, params: it.params, rot: it.rot, mir: it.mir, pins, zone, kind: Kind::Other, ends: None, el: None, done: false });
    }
    // header: title, simulation control, parameter file, analyses
    let mut y = 1.0;
    if let Some(t) = title.filter(|t| !t.trim().is_empty()) {
        let mut p = BTreeMap::new();
        p.insert("text".to_string(), t.to_string());
        lay.c.add_element("TEXT", "", 2.0, y + 0.8, 0, false, &p);
        y += 2.5 + t.matches('\n').count() as f64 * 1.1;
    }
    let (mut hx, mut hb) = (4.0f64, y);
    for i in 0..lay.its.len() {
        if lay.its[i].zone != Zone::Head {
            continue;
        }
        let e = lay.place(i, 0.0, 0.0, 0, false);
        let b = world_box(&lay.c.doc.elements[e]);
        let lines = label_lines(&lay.c.doc.elements[e]).len() as f64;
        lay.c.doc.elements[e].x = (hx - b[0]).ceil();
        lay.c.doc.elements[e].y = (y + 1.0 - b[1]).ceil() + (lines * 0.95).ceil();
        let wb = world_box(&lay.c.doc.elements[e]);
        hx = wb[2] + 4.0 + if lines > 0.0 { 5.0 } else { 0.0 };
        hb = hb.max(wb[3]);
    }
    let bottom = power(&mut lay, hb);
    control(&mut lay, bottom);
    let labels = connect_all(&mut lay, &mut notes);
    Out { circuit: lay.c, notes, labels }
}

/// the power stage; returns the bottom edge
fn power(lay: &mut Lay, hb: f64) -> f64 {
    let pw: Vec<usize> = (0..lay.its.len()).filter(|&i| lay.its[i].zone == Zone::Power).collect();
    for &i in &pw {
        let t = lay.terms(i);
        let it = &lay.its[i];
        let probe = lib().get(&it.typ).map(|d| d.has("probe")).unwrap_or(false);
        let (kind, ends) = if t.len() == 1 && probe {
            (Kind::Tap, None)
        } else if t.len() == 2 && t[0].1 != t[1].1 {
            let meter = METER.contains(&it.typ.as_str()) || (probe && !CURRENT.contains(&it.typ.as_str()));
            if t[0].1 == "0" || t[1].1 == "0" {
                let (a, g) = if t[0].1 == "0" { (&t[1], &t[0]) } else { (&t[0], &t[1]) };
                (Kind::Shunt, Some((a.0.clone(), g.0.clone())))
            } else if meter {
                (Kind::Other, None)
            } else {
                (Kind::Series, Some((t[0].0.clone(), t[1].0.clone())))
            }
        } else {
            (Kind::Other, None)
        };
        lay.its[i].kind = kind;
        lay.its[i].ends = ends;
    }
    let net_of = |lay: &Lay, i: usize, pin: &str| -> String { lay.its[i].pins.iter().find(|(p, _)| p.n == pin).and_then(|(_, n)| n.clone()).unwrap_or_default() };
    let is_src = |lay: &Lay, i: usize| lib().get(&lay.its[i].typ).map(|d| d.has("src")).unwrap_or(false);
    let src = pw.iter().copied().find(|&i| lay.its[i].kind == Kind::Shunt && is_src(lay, i)).or_else(|| pw.iter().copied().find(|&i| lay.its[i].kind == Kind::Shunt));
    let series: Vec<usize> = pw.iter().copied().filter(|&i| lay.its[i].kind == Kind::Series).collect();
    let shunt_at = |lay: &Lay, i: usize| -> (String, String, String) {
        let (a, g) = lay.its[i].ends.clone().unwrap();
        (a.n.clone(), g.n.clone(), net_of(lay, i, &a.n))
    };
    // half-bridge legs: a node X joined to the source net by switches or diodes, and to ground
    struct Leg {
        x: String,
        up: Vec<(usize, String, String)>,
        down: Vec<(usize, String, String)>,
    }
    let mut legs: Vec<Leg> = vec![];
    if let Some(s) = src {
        let top = shunt_at(lay, s).2;
        for &i in &series {
            let (a, b) = lay.its[i].ends.clone().unwrap();
            let (na, nb) = (net_of(lay, i, &a.n), net_of(lay, i, &b.n));
            let (tp, xp, xn) = if na == top { (a.n.clone(), b.n.clone(), nb) } else if nb == top { (b.n.clone(), a.n.clone(), na) } else { continue };
            match legs.iter_mut().find(|l| l.x == xn) {
                Some(l) => l.up.push((i, tp, xp)),
                None => legs.push(Leg { x: xn, up: vec![(i, tp, xp)], down: vec![] }),
            }
        }
        for l in legs.iter_mut() {
            l.down = pw.iter().copied().filter(|&i| lay.its[i].kind == Kind::Shunt && shunt_at(lay, i).2 == l.x).map(|i| {
                let (a, g, _) = shunt_at(lay, i);
                (i, a, g)
            }).collect();
        }
        legs.retain(|l| !l.down.is_empty());
    }
    let tap_h = if pw.iter().any(|&i| lay.its[i].kind == Kind::Tap) { 3.0 } else { 0.0 };
    let rail = (hb + 4.0 + f64::max(tap_h, 3.0)).ceil();
    let mut drops: Vec<(f64, f64)> = vec![];
    let mut segs: Vec<(String, f64, f64)> = vec![];
    let mut cursor = 4.0f64;
    if legs.len() >= 2 {
        // a DC bus: the source net on the rail, the legs as columns, the loads between their midpoints
        let s = src.unwrap();
        let top = shunt_at(lay, s).2;
        let mut rail_x: Vec<f64> = vec![];
        let shunts: Vec<usize> = pw.iter().copied().filter(|&i| lay.its[i].kind == Kind::Shunt && shunt_at(lay, i).2 == top).collect();
        for i in shunts {
            let (a, g, _) = shunt_at(lay, i);
            let x = (cursor + GAP + lay.left_of(i, &a, &g)).ceil();
            if let Some(gp) = lay.place_vert(i, &a, &g, x, rail, &top) {
                drops.push(gp);
                rail_x.push(x);
                cursor = x + lay.right_of(i, &a, &g);
            }
        }
        let used: HashSet<usize> = legs.iter().flat_map(|l| l.up.iter().chain(l.down.iter()).map(|u| u.0)).collect();
        let free: Vec<usize> = series.iter().copied().filter(|i| !used.contains(i)).collect();
        let mut leg_mid = rail + DROP + 1.0;
        for l in &legs {
            for u in &l.up {
                if let Some((_, _, pos)) = lay.vert_rot(u.0, &u.1, &u.2) {
                    leg_mid = leg_mid.max(rail + DROP + pos[&u.2].1 - pos[&u.1].1 + 1.0);
                }
            }
        }
        let mut join_from: Option<(f64, String)> = None;
        for li in 0..legs.len() {
            let mut xs: Vec<(f64, (f64, f64))> = vec![];
            for u in legs[li].up.clone() {
                let x = (cursor + GAP + lay.left_of(u.0, &u.1, &u.2)).ceil();
                if let Some(bp) = lay.place_vert(u.0, &u.1, &u.2, x, rail, &top) {
                    rail_x.push(x);
                    xs.push((x, bp));
                    cursor = x + lay.right_of(u.0, &u.1, &u.2);
                }
            }
            if xs.is_empty() {
                continue;
            }
            let xnet = legs[li].x.clone();
            for &(x, bp) in &xs {
                lay.wire(bp, (x, leg_mid), &xnet);
            }
            let lx0 = xs[0].0;
            let mut lx1 = xs.last().unwrap().0;
            for (k, dn) in legs[li].down.clone().into_iter().enumerate() {
                let x = if k < xs.len() { xs[k].0 } else { (cursor + GAP + lay.left_of(dn.0, &dn.1, &dn.2)).ceil() };
                if let Some(gp) = lay.place_vert(dn.0, &dn.1, &dn.2, x, leg_mid, &xnet) {
                    drops.push(gp);
                }
                if k >= xs.len() {
                    cursor = x + lay.right_of(dn.0, &dn.1, &dn.2);
                }
                lx1 = lx1.max(x);
            }
            if lx1 > lx0 {
                lay.wire((lx0, leg_mid), (lx1, leg_mid), &xnet);
            }
            if let Some((jx, jn)) = join_from.take() {
                lay.wire((jx, leg_mid), (lx0, leg_mid), &jn);
            }
            // the load path to the next leg's midpoint, along the midpoint line
            if li + 1 < legs.len() {
                let target = legs[li + 1].x.clone();
                let mut prev: HashMap<String, Option<(usize, String, String, String)>> = HashMap::new();
                prev.insert(xnet.clone(), None);
                let mut q = std::collections::VecDeque::from(vec![xnet.clone()]);
                while let Some(n) = q.pop_front() {
                    if prev.contains_key(&target) {
                        break;
                    }
                    for &i in &free {
                        if lay.its[i].done {
                            continue;
                        }
                        let (a, b) = lay.its[i].ends.clone().unwrap();
                        let (na, nb) = (net_of(lay, i, &a.n), net_of(lay, i, &b.n));
                        let (from, to, on) = if na == n { (a.n.clone(), b.n.clone(), nb) } else if nb == n { (b.n.clone(), a.n.clone(), na) } else { continue };
                        if !prev.contains_key(&on) {
                            prev.insert(on.clone(), Some((i, from, to, n.clone())));
                            q.push_back(on);
                        }
                    }
                }
                if prev.contains_key(&target) {
                    let mut path = vec![];
                    let mut n = target.clone();
                    while let Some(Some(st)) = prev.get(&n) {
                        path.push(st.clone());
                        n = st.3.clone();
                    }
                    path.reverse();
                    let mut x = lx1;
                    let mut net_prev = xnet.clone();
                    for (i, from, to, _) in path {
                        let Some((r, m, pos)) = lay.hor_rot(i, &from, &to) else { continue };
                        let e = lay.ext(i, r, m);
                        let (a, b2) = (pos[&from], pos[&to]);
                        let left = (x + GAP + (a.0 - e[0])).ceil();
                        lay.wire((x, leg_mid), (left, leg_mid), &net_prev);
                        lay.place(i, left - a.0, leg_mid - a.1, r, m);
                        x = left - a.0 + b2.0;
                        net_prev = net_of(lay, i, &to);
                    }
                    cursor = cursor.max(x + GAP);
                    join_from = Some((x, net_prev));
                }
            }
        }
        if !rail_x.is_empty() {
            segs.push((top, rail_x.iter().cloned().fold(f64::INFINITY, f64::min), rail_x.iter().cloned().fold(f64::NEG_INFINITY, f64::max)));
        }
    } else {
        // the main current path: longest chain of series elements from the source's net
        let mut adj: HashMap<String, Vec<(usize, String, String, String)>> = HashMap::new();
        for &i in &series {
            let (a, b) = lay.its[i].ends.clone().unwrap();
            let (na, nb) = (net_of(lay, i, &a.n), net_of(lay, i, &b.n));
            adj.entry(na.clone()).or_default().push((i, a.n.clone(), b.n.clone(), nb.clone()));
            adj.entry(nb).or_default().push((i, b.n.clone(), a.n.clone(), na));
        }
        let start = match src {
            Some(s) => Some(shunt_at(lay, s).2),
            None => series.first().map(|&i| net_of(lay, i, &lay.its[i].ends.as_ref().unwrap().0.n)),
        };
        let mut best: (Vec<String>, Vec<(usize, String, String)>) = (start.iter().cloned().collect(), vec![]);
        if let Some(st) = start.clone() {
            let mut steps = 0u32;
            #[allow(clippy::too_many_arguments)]
            fn dfs(net: &str, nets: &mut Vec<String>, edges: &mut Vec<(usize, String, String)>, used: &mut HashSet<usize>, adj: &HashMap<String, Vec<(usize, String, String, String)>>, best: &mut (Vec<String>, Vec<(usize, String, String)>), steps: &mut u32) {
                *steps += 1;
                if *steps > 50_000 {
                    return;
                }
                if nets.len() > best.0.len() {
                    *best = (nets.clone(), edges.clone());
                }
                for (i, from, to, on) in adj.get(net).cloned().unwrap_or_default() {
                    if used.contains(&i) || nets.contains(&on) {
                        continue;
                    }
                    used.insert(i);
                    nets.push(on.clone());
                    edges.push((i, from, to));
                    dfs(&on, nets, edges, used, adj, best, steps);
                    used.remove(&i);
                    nets.pop();
                    edges.pop();
                }
            }
            dfs(&st, &mut vec![st.clone()], &mut vec![], &mut HashSet::new(), &adj, &mut best, &mut steps);
        }
        let (chain, edges) = best;
        let mut seg: Option<usize> = None;
        for (ci, net) in chain.iter().enumerate() {
            if ci > 0 {
                let (i, from, to) = edges[ci - 1].clone();
                if let Some((r, m, pos)) = lay.hor_rot(i, &from, &to) {
                    let (a, b) = (pos[&from], pos[&to]);
                    let left = cursor.ceil();
                    if let Some(s) = seg {
                        segs[s].2 = left;
                    }
                    lay.place(i, left - a.0, rail - a.1, r, m);
                    // elements in parallel with it (antiparallel diode, snubber): above it, joined by the router
                    let (nf, nt) = (net_of(lay, i, &from), net_of(lay, i, &to));
                    let par: Vec<usize> = series.iter().copied().filter(|&o| {
                        if o == i || lay.its[o].done {
                            return false;
                        }
                        let (pa, pb) = lay.its[o].ends.clone().unwrap();
                        let (x1, x2) = (net_of(lay, o, &pa.n), net_of(lay, o, &pb.n));
                        (x1 == nf && x2 == nt) || (x1 == nt && x2 == nf)
                    }).collect();
                    for (k, o) in par.into_iter().enumerate() {
                        let (pa, pb) = lay.its[o].ends.clone().unwrap();
                        let (f2, t2) = if net_of(lay, o, &pa.n) == nf { (pa.n.clone(), pb.n.clone()) } else { (pb.n.clone(), pa.n.clone()) };
                        if let Some((r2, m2, p2)) = lay.hor_rot(o, &f2, &t2) {
                            let e2 = lay.ext(o, r2, m2);
                            let cx = left - a.0 + (a.0 + b.0) / 2.0;
                            lay.place(o, (cx - (p2[&f2].0 + p2[&t2].0) / 2.0).round(), (rail - 3.0 - k as f64 * 4.0 - (e2[3] - p2[&f2].1)).floor(), r2, m2);
                        }
                    }
                    cursor = left - a.0 + b.0;
                    segs.push((net.clone(), cursor, cursor));
                    seg = Some(segs.len() - 1);
                }
            }
            let slots: Vec<usize> = pw.iter().copied().filter(|&i| {
                !lay.its[i].done
                    && match lay.its[i].kind {
                        Kind::Shunt => shunt_at(lay, i).2 == *net,
                        Kind::Tap => lay.terms(i).first().map(|t| t.1 == *net).unwrap_or(false),
                        _ => false,
                    }
            }).collect();
            for i in slots {
                if lay.its[i].kind == Kind::Tap {
                    let pk = lay.terms(i)[0].0.n.clone();
                    let Some((r, m, pos)) = lay.rotations(i, |pos, el| {
                        let p = pos.get(&pk).copied().unwrap_or((0.0, 0.0));
                        let b = world_box(el);
                        p.1 >= b[3] - 1e-9 && (p.0 - (b[0] + b[2]) / 2.0).abs() < 1.01
                    }) else {
                        continue;
                    };
                    let pp = pos[&pk];
                    let e = lay.ext(i, r, m);
                    let ax = (cursor + GAP - (e[0] - pp.0)).ceil();
                    lay.place(i, ax - pp.0, rail - pp.1, r, m);
                    match seg {
                        Some(s) => segs[s].2 = ax,
                        None => {
                            segs.push((net.clone(), ax, ax));
                            seg = Some(segs.len() - 1);
                        }
                    }
                    cursor = ax + (e[2] - pp.0);
                } else {
                    let (a, g, _) = shunt_at(lay, i);
                    let ax = (cursor + GAP + lay.left_of(i, &a, &g)).ceil();
                    let Some(gp) = lay.place_vert(i, &a, &g, ax, rail, net) else { continue };
                    drops.push(gp);
                    match seg {
                        Some(s) => segs[s].2 = ax,
                        None => {
                            segs.push((net.clone(), ax, ax));
                            seg = Some(segs.len() - 1);
                        }
                    }
                    cursor = ax + lay.right_of(i, &a, &g);
                }
            }
            if ci + 1 < chain.len() {
                cursor += GAP;
            }
        }
    }
    for (net, x1, x2) in segs.clone() {
        if x2 > x1 {
            lay.wire((x1, rail), (x2, rail), &net);
        }
    }
    // the ground rail
    let mut gnd_y = rail + 10.0;
    for d in &drops {
        gnd_y = gnd_y.max(d.1 + 2.0);
    }
    if !drops.is_empty() {
        let xmin = drops.iter().map(|d| d.0).fold(f64::INFINITY, f64::min);
        let xmax = drops.iter().map(|d| d.0).fold(f64::NEG_INFINITY, f64::max);
        for d in drops.clone() {
            lay.wire(d, (d.0, gnd_y), "0");
        }
        if xmax > xmin {
            lay.wire((xmin, gnd_y), (xmax, gnd_y), "0");
        }
        lay.c.add_element("GND", "", xmin, gnd_y, 0, false, &BTreeMap::new());
    }
    let mut bottom = (gnd_y + 1.0).max(rail + 4.0);
    // anything else: next to what it connects to, in free space; the router joins it
    for i in pw.iter().copied().filter(|&i| !lay.its[i].done).collect::<Vec<_>>() {
        let mut pts: Vec<(f64, f64)> = vec![];
        for (_, n) in lay.terms(i).into_iter().filter(|t| t.1 != "0") {
            for j in 0..lay.its.len() {
                if lay.its[j].el.is_none() {
                    continue;
                }
                for (p, pn) in lay.its[j].pins.clone() {
                    if pn.as_deref() == Some(n.as_str()) {
                        pts.push(lay.pin_pos(j, &p.n));
                    }
                }
            }
            for (k, w) in lay.c.doc.wires.iter().enumerate() {
                if lay.wnet[k] == n {
                    pts.push((w.x1, w.y1));
                    pts.push((w.x2, w.y2));
                }
            }
        }
        let (cx, cy) = if pts.is_empty() { (10.0, bottom + 4.0) } else { (pts.iter().map(|p| p.0).sum::<f64>() / pts.len() as f64, pts.iter().map(|p| p.1).sum::<f64>() / pts.len() as f64) };
        let (rot, mir) = (lay.its[i].rot, lay.its[i].mir);
        let e = lay.ext(i, rot, mir);
        let boxes: Vec<[f64; 4]> = lay.c.doc.elements.iter().filter(|e| e.typ != "TEXT").map(world_box).collect();
        let mut spot = None;
        'search: for r in 0..40i64 {
            for dx in -r..=r {
                for dy in [-r, r] {
                    for (ox, oy) in [(dx, dy), (dy, dx)] {
                        let (x, yy) = ((cx + ox as f64 * 2.0).round(), (cy + oy as f64 * 2.0).round());
                        let bb = [x + e[0] - 1.5, yy + e[1] - 1.5, x + e[2] + 1.5, yy + e[3] + 1.5];
                        if boxes.iter().any(|o| o[0] < bb[2] && bb[0] < o[2] && o[1] < bb[3] && bb[1] < o[3]) {
                            continue;
                        }
                        if lay.c.doc.wires.iter().any(|w| w.x1.max(w.x2) >= bb[0] && w.x1.min(w.x2) <= bb[2] && w.y1.max(w.y2) >= bb[1] && w.y1.min(w.y2) <= bb[3]) {
                            continue;
                        }
                        if yy + e[1] < hb + 1.0 {
                            continue;
                        }
                        spot = Some((x, yy));
                        break 'search;
                    }
                }
            }
        }
        let (x, yy) = spot.unwrap_or((10.0, bottom + 6.0));
        let el = lay.place(i, x, yy, rot, mir);
        bottom = bottom.max(world_box(&lay.c.doc.elements[el])[3] + 1.0);
    }
    bottom
}

/// control blocks in columns by signal flow
fn control(lay: &mut Lay, bottom: f64) {
    let cs: Vec<usize> = (0..lay.its.len()).filter(|&i| lay.its[i].zone == Zone::Ctl).collect();
    if cs.is_empty() {
        return;
    }
    let mut drv: HashMap<String, Vec<usize>> = HashMap::new();
    let mut sinks: HashMap<String, Vec<usize>> = HashMap::new();
    for &i in &cs {
        let src = lib().get(&lay.its[i].typ).map(|d| d.has("src")).unwrap_or(false);
        for (p, n) in &lay.its[i].pins {
            let Some(n) = n else { continue };
            if n == "0" {
                continue;
            }
            let out = p.k == "co" || (p.k == "a" && src);
            if out { drv.entry(n.clone()).or_default().push(i) } else { sinks.entry(n.clone()).or_default().push(i) }
        }
    }
    let mut preds: HashMap<usize, Vec<usize>> = cs.iter().map(|&i| (i, vec![])).collect();
    for (n, ss) in &sinks {
        for &s in ss {
            for &d in drv.get(n).map(|v| v.as_slice()).unwrap_or(&[]) {
                if d != s {
                    preds.get_mut(&s).unwrap().push(d);
                }
            }
        }
    }
    // longest-path layers, back edges ignored
    let mut layer: HashMap<usize, i64> = HashMap::new();
    let mut state: HashMap<usize, u8> = HashMap::new();
    fn lay_of(i: usize, preds: &HashMap<usize, Vec<usize>>, layer: &mut HashMap<usize, i64>, state: &mut HashMap<usize, u8>) -> i64 {
        match state.get(&i) {
            Some(2) => return layer[&i],
            Some(1) => return -1,
            _ => {}
        }
        state.insert(i, 1);
        let mut l = 0;
        for &p in &preds[&i] {
            let lp = lay_of(p, preds, layer, state);
            if lp >= 0 {
                l = l.max(lp + 1);
            }
        }
        state.insert(i, 2);
        layer.insert(i, l);
        l
    }
    for &i in &cs {
        lay_of(i, &preds, &mut layer, &mut state);
    }
    let maxl = cs.iter().map(|i| layer[i]).max().unwrap_or(0);
    let top = bottom + 6.0;
    let mut cx = 4.0f64;
    let mut y_of: HashMap<usize, f64> = HashMap::new();
    for l in 0..=maxl {
        let mut col: Vec<usize> = cs.iter().copied().filter(|i| layer[i] == l).collect();
        if col.is_empty() {
            continue;
        }
        let mean = |i: usize, y_of: &HashMap<usize, f64>| -> f64 {
            let ys: Vec<f64> = preds[&i].iter().filter_map(|p| y_of.get(p).copied()).collect();
            if ys.is_empty() { f64::INFINITY } else { ys.iter().sum::<f64>() / ys.len() as f64 }
        };
        col.sort_by(|&a, &b| mean(a, &y_of).partial_cmp(&mean(b, &y_of)).unwrap_or(std::cmp::Ordering::Equal));
        let (mut cy, mut cw) = (top, 0.0f64);
        for i in col {
            let (rot, mir) = (lay.its[i].rot, lay.its[i].mir);
            let pe = probe_el(&lay.its[i].typ, rot, mir, &lay.its[i].params, &lay.its[i].name);
            let b = world_box(&pe);
            let lines = label_lines(&pe).len() as f64;
            let el = lay.place(i, (cx - b[0]).ceil(), (cy - b[1] + lines * 0.95 + 0.5).ceil(), rot, mir);
            let wb = world_box(&lay.c.doc.elements[el]);
            y_of.insert(i, (wb[1] + wb[3]) / 2.0);
            cy = wb[3] + 4.0;
            cw = cw.max(wb[2] - wb[0]);
        }
        cx += cw + 8.0;
    }
}

/// a short wire from a pin to a label or ground symbol; false when there is no room
fn stub(lay: &mut Lay, g: &mut Grid, pt: Pt, dir: (f64, f64), net: &str) -> bool {
    let d0 = (dir.0.round() as i64, dir.1.round() as i64);
    let dirs = [d0, (d0.1, d0.0), (-d0.1, -d0.0)];
    for len in [2i64, 3, 4] {
        for d in dirs {
            let w = (if is_ground(net) { 1.2 } else { label_width(net) }).ceil() as i64;
            let mut ok = true;
            for s in 1..=len + w {
                let k = (pt.0 + d.0 * s, pt.1 + d.1 * s);
                if g.boxc.contains(&k) || g.hard.contains_key(&k) || (s <= len && (g.h.contains_key(&k) || g.v.contains_key(&k))) {
                    ok = false;
                    break;
                }
            }
            if !ok {
                continue;
            }
            let end = (pt.0 + d.0 * len, pt.1 + d.1 * len);
            lay.wire((pt.0 as f64, pt.1 as f64), (end.0 as f64, end.1 as f64), net);
            g.wire(pt, end, net);
            let fd = (d.0 as f64, d.1 as f64);
            let e = if is_ground(net) {
                let rot = if fd.1 > 0.5 { 0 } else if fd.0 < -0.5 { 1 } else if fd.1 < -0.5 { 2 } else { 3 };
                lay.c.add_element("GND", "", end.0 as f64, end.1 as f64, rot, false, &BTreeMap::new())
            } else {
                let rot = if fd.0 > 0.5 { 0 } else if fd.1 > 0.5 { 1 } else if fd.0 < -0.5 { 2 } else { 3 };
                let mut p = BTreeMap::new();
                p.insert("text".to_string(), net.to_string());
                lay.c.add_element("LABEL", "", end.0 as f64, end.1 as f64, rot, false, &p)
            };
            g.element(&lay.c.doc.elements[e]);
            g.pin(end, net);
            for s in 1..=w {
                g.boxc.insert((end.0 + d.0 * s, end.1 + d.1 * s));
            }
            return true;
        }
    }
    false
}

/// joins every net: wires by the router, labels across the power/control boundary or where no route exists
fn connect_all(lay: &mut Lay, notes: &mut Vec<String>) -> usize {
    let mut g = Grid::default();
    for e in &lay.c.doc.elements {
        g.element(e);
    }
    for (k, w) in lay.c.doc.wires.iter().enumerate() {
        g.wire(ip(w.x1, w.y1), ip(w.x2, w.y2), &lay.wnet[k]);
    }
    struct Pin {
        it: usize,
        p: PinDef,
        pt: Pt,
        label: bool,
    }
    let mut by_net: BTreeMap<String, Vec<Pin>> = BTreeMap::new();
    for i in 0..lay.its.len() {
        if lay.its[i].el.is_none() {
            continue;
        }
        for (p, n) in lay.its[i].pins.clone() {
            let Some(n) = n else { continue };
            let (x, y) = lay.pin_pos(i, &p.n);
            g.pin(ip(x, y), &n);
            by_net.entry(n).or_default().push(Pin { it: i, p, pt: ip(x, y), label: false });
        }
    }
    let mut bounds = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
    for e in &lay.c.doc.elements {
        let b = world_box(e);
        bounds = [bounds[0].min(b[0].floor() as i64), bounds[1].min(b[1].floor() as i64), bounds[2].max(b[2].ceil() as i64), bounds[3].max(b[3].ceil() as i64)];
    }
    let bounds = [bounds[0] - 6, bounds[1] - 2, bounds[2] + 10, bounds[3] + 8];
    let mut labels = 0;
    let mut nets: Vec<String> = by_net.keys().cloned().collect();
    nets.sort_by_key(|n| by_net[n].len());
    let on_wire = |lay: &Lay, net: &str, q: Pt| {
        lay.c.doc.wires.iter().enumerate().any(|(k, w)| {
            if lay.wnet[k] != net {
                return false;
            }
            let (a, b) = (ip(w.x1, w.y1), ip(w.x2, w.y2));
            q == a || q == b || (a.1 == b.1 && q.1 == a.1 && q.0 > a.0.min(b.0) && q.0 < a.0.max(b.0)) || (a.0 == b.0 && q.0 == a.0 && q.1 > a.1.min(b.1) && q.1 < a.1.max(b.1))
        })
    };
    for net in nets {
        let mut ps = by_net.remove(&net).unwrap();
        if net == "0" {
            for q in &ps {
                if on_wire(lay, "0", q.pt) {
                    continue;
                }
                let d = pin_dir(lay.el(q.it), &q.p);
                stub(lay, &mut g, q.pt, d, "0");
            }
            continue;
        }
        let zone_of = |lay: &Lay, i: usize| if lay.its[i].zone == Zone::Ctl { Zone::Ctl } else { Zone::Power };
        let mut zones: Vec<Zone> = vec![];
        for q in &ps {
            let z = zone_of(lay, q.it);
            if !zones.contains(&z) {
                zones.push(z);
            }
        }
        let cross = zones.len() > 1;
        for z in zones {
            let idx: Vec<usize> = (0..ps.len()).filter(|&k| zone_of(lay, ps[k].it) == z).collect();
            // a control signal that also reaches the power stage (gate, sensor output): a label at each power pin
            if cross && z == Zone::Power {
                for &k in &idx {
                    let d = pin_dir(lay.el(ps[k].it), &ps[k].p);
                    if stub(lay, &mut g, ps[k].pt, d, &net) {
                        ps[k].label = true;
                        labels += 1;
                    }
                }
                continue;
            }
            let mut tree: Vec<usize> = idx.iter().copied().filter(|&k| on_wire(lay, &net, ps[k].pt)).collect();
            let joined = !tree.is_empty();
            if tree.is_empty() {
                tree.push(idx[0]);
            }
            let mut goals: HashSet<Pt> = tree.iter().map(|&k| ps[k].pt).collect();
            let add_seg = |goals: &mut HashSet<Pt>, a: Pt, b: Pt| {
                for x in a.0.min(b.0)..=a.0.max(b.0) {
                    for y in a.1.min(b.1)..=a.1.max(b.1) {
                        goals.insert((x, y));
                    }
                }
            };
            if joined {
                for (k, w) in lay.c.doc.wires.iter().enumerate() {
                    if lay.wnet[k] == net {
                        add_seg(&mut goals, ip(w.x1, w.y1), ip(w.x2, w.y2));
                    }
                }
            }
            let mut todo: Vec<usize> = idx.iter().copied().filter(|k| !tree.contains(k)).collect();
            while !todo.is_empty() {
                let dist = |k: usize, tree: &[usize]| tree.iter().map(|&t| (ps[t].pt.0 - ps[k].pt.0).abs() + (ps[t].pt.1 - ps[k].pt.1).abs()).min().unwrap_or(0);
                todo.sort_by_key(|&k| dist(k, &tree));
                let k = todo.remove(0);
                let span = dist(k, &tree);
                match route(&g, &net, ps[k].pt, bounds, &goals) {
                    Some((pts, cost)) if (cost as i64) < 3 * span + 30 => {
                        for s in pts.windows(2) {
                            lay.wire((s[0].0 as f64, s[0].1 as f64), (s[1].0 as f64, s[1].1 as f64), &net);
                            g.wire(s[0], s[1], &net);
                            add_seg(&mut goals, s[0], s[1]);
                        }
                    }
                    _ => {
                        if !tree.iter().any(|&t| ps[t].label) {
                            let t0 = tree[0];
                            let d = pin_dir(lay.el(ps[t0].it), &ps[t0].p);
                            if stub(lay, &mut g, ps[t0].pt, d, &net) {
                                ps[t0].label = true;
                                labels += 1;
                            }
                        }
                        let d = pin_dir(lay.el(ps[k].it), &ps[k].p);
                        if stub(lay, &mut g, ps[k].pt, d, &net) {
                            ps[k].label = true;
                            labels += 1;
                        }
                        notes.push(format!("節點 {}：{}.{} 以標籤連接（導線路徑太遠或被擋住）", net, lay.its[ps[k].it].name, ps[k].p.n));
                    }
                }
                goals.insert(ps[k].pt);
                tree.push(k);
            }
            if cross {
                // one label for this net in the control area
                if !idx.iter().any(|&k| ps[k].label) {
                    let t0 = idx[0];
                    let d = pin_dir(lay.el(ps[t0].it), &ps[t0].p);
                    if stub(lay, &mut g, ps[t0].pt, d, &net) {
                        ps[t0].label = true;
                        labels += 1;
                    }
                }
            }
        }
    }
    labels
}

/* ------------------------------------------------------------------ edits of an existing drawing */

/// a wire path in an existing drawing from `from` to net `target` (an index of the analysis), avoiding every
/// other net; None when there is no reasonably short path
pub fn route_existing(doc: &daedalus_core::doc::Doc, a: &crate::circuit::Analysis, from: (f64, f64), target: usize) -> Option<Vec<(f64, f64)>> {
    let els: Vec<&Element> = doc.elements.iter().filter(|e| !e.dis).collect();
    let conn = daedalus_core::netlist::connectivity(doc, &els);
    let key_of = |x: f64, y: f64| -> String { conn.root(x, y).and_then(|r| a.root_net.get(&r)).map(|n| n.to_string()).unwrap_or_else(|| "?".into()) };
    let mut g = Grid::default();
    let mut goals: HashSet<Pt> = HashSet::new();
    let tkey = target.to_string();
    for e in &els {
        g.element(e);
        for p in daedalus_core::library::wpins(e) {
            let k = key_of(p.x, p.y);
            if k == tkey {
                goals.insert(ip(p.x, p.y));
            }
            g.pin(ip(p.x, p.y), &k);
        }
    }
    for w in &doc.wires {
        let k = key_of(w.x1, w.y1);
        let (p, q) = (ip(w.x1, w.y1), ip(w.x2, w.y2));
        g.wire(p, q, &k);
        if k == tkey {
            for x in p.0.min(q.0)..=p.0.max(q.0) {
                for y in p.1.min(q.1)..=p.1.max(q.1) {
                    goals.insert((x, y));
                }
            }
        }
    }
    if goals.is_empty() {
        return None;
    }
    let start = ip(from.0, from.1);
    // the starting pin belongs to its own net: free it for the search
    let own = g.hard.remove(&start);
    let mut b = [i64::MAX, i64::MAX, i64::MIN, i64::MIN];
    for e in &els {
        let w = world_box(e);
        b = [b[0].min(w[0].floor() as i64), b[1].min(w[1].floor() as i64), b[2].max(w[2].ceil() as i64), b[3].max(w[3].ceil() as i64)];
    }
    let span = goals.iter().map(|p| (p.0 - start.0).abs() + (p.1 - start.1).abs()).min().unwrap_or(0);
    let r = route(&g, own.as_deref().unwrap_or("?"), start, [b[0] - 6, b[1] - 6, b[2] + 6, b[3] + 6], &goals);
    match r {
        Some((pts, cost)) if span <= 60 && (cost as i64) < 3 * span + 30 => Some(pts.into_iter().map(|p| (p.0 as f64, p.1 as f64)).collect()),
        _ => None,
    }
}

/// a free spot near `near` for an element whose extents (relative to its origin) are `ext`
pub fn free_spot(doc: &daedalus_core::doc::Doc, ext: [f64; 4], near: (f64, f64)) -> Option<(f64, f64)> {
    let boxes: Vec<[f64; 4]> = doc.elements.iter().filter(|e| e.typ != "TEXT").map(world_box).collect();
    for r in 0..40i64 {
        for dx in -r..=r {
            for dy in [-r, r] {
                for (ox, oy) in [(dx, dy), (dy, dx)] {
                    let (x, y) = ((near.0 + ox as f64 * 2.0).round(), (near.1 + oy as f64 * 2.0).round());
                    let bb = [x + ext[0] - 1.5, y + ext[1] - 1.5, x + ext[2] + 1.5, y + ext[3] + 1.5];
                    if boxes.iter().any(|o| o[0] < bb[2] && bb[0] < o[2] && o[1] < bb[3] && bb[1] < o[3]) {
                        continue;
                    }
                    if doc.wires.iter().any(|w| w.x1.max(w.x2) >= bb[0] && w.x1.min(w.x2) <= bb[2] && w.y1.max(w.y2) >= bb[1] && w.y1.min(w.y2) <= bb[3]) {
                        continue;
                    }
                    return Some((x, y));
                }
            }
        }
    }
    None
}
