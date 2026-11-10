//! Schematic canvas: drawing and mouse interaction.

use crate::editor::{l_path, label_lines, wbox, Editor, Group, Sel};
use crate::symbols::{local_to_world, Prim, SymCache};
use crate::theme::Pal;
use daedalus_core::doc::{Doc, Element};
use daedalus_core::library::{lib, tpt, wpins};
use egui::{epaint::TextShape, pos2, vec2, Align2, Color32, CornerRadius, FontId, Pos2, Rect, Sense, Shape, Stroke, StrokeKind, Ui, Vec2};
use std::collections::HashSet;

pub enum Mode {
    Select,
    Wire,
    /// hand tool: left-drag moves the view
    Hand,
    Place(Box<Element>),
    Paste(Group),
}

enum Drag {
    None,
    Move { base: Doc, start: (f64, f64), moved: bool },
    Band { start: Pos2 },
    Pan,
    WireFrom { start: (f64, f64) },
}

/// requests from the canvas to the application
pub enum CanvasEvent {
    OpenProps(i64),
    QuickAdd(Pos2, (f64, f64)),
    Toast(String),
    InlineEdit(i64, String, Pos2),
    ContextMenu,
}

pub struct Canvas {
    pub off: Vec2,
    pub scale: f32,
    pub mode: Mode,
    drag: Drag,
    pub wire_pts: Vec<(f64, f64)>,
    pub mouse_world: Option<(f64, f64)>,
    pub err_els: HashSet<i64>,
    pub find_hits: Vec<i64>,
    pub pin_marks: bool,
    pub rect: Rect,
    pub label_rects: Vec<(i64, String, Rect)>,
    pub want_fit: bool,
    pub space_down: bool,
    /// size of the element name / parameter labels relative to the default (File → Settings)
    pub label_k: f32,
    /// a double click on an empty spot opens the quick-add box (File → Settings; off by default)
    pub dbl_quick: bool,
}

const PIN_KIND: &[(&str, &str)] = &[("p", "功率"), ("ci", "控制輸入"), ("g", "閘極（控制輸入）"), ("co", "控制輸出"), ("a", "功率或控制"), ("gnd", "接地"), ("lbl", "節點標籤")];

impl Canvas {
    pub fn new() -> Self {
        Canvas { off: vec2(400.0, 300.0), scale: 20.0, mode: Mode::Select, drag: Drag::None, wire_pts: vec![], mouse_world: None, err_els: HashSet::new(), find_hits: vec![], pin_marks: true, rect: Rect::NOTHING, label_rects: vec![], want_fit: true, space_down: false, label_k: 1.0, dbl_quick: false }
    }
    #[inline]
    pub fn w2s(&self, x: f64, y: f64) -> Pos2 {
        pos2(self.rect.min.x + self.off.x + x as f32 * self.scale, self.rect.min.y + self.off.y + y as f32 * self.scale)
    }
    #[inline]
    pub fn s2w(&self, p: Pos2) -> (f64, f64) {
        (((p.x - self.rect.min.x - self.off.x) / self.scale) as f64, ((p.y - self.rect.min.y - self.off.y) / self.scale) as f64)
    }
    pub fn zoom_at(&mut self, f: f32, at: Pos2) {
        let ns = (self.scale * f).clamp(2.0, 200.0);
        let w = self.s2w(at);
        self.scale = ns;
        self.off = vec2(at.x - self.rect.min.x - w.0 as f32 * ns, at.y - self.rect.min.y - w.1 as f32 * ns);
    }
    pub fn fit(&mut self, ed: &Editor) {
        if self.rect.width() < 10.0 {
            self.want_fit = true;
            return;
        }
        match ed.bounds() {
            Some(b) => {
                let (w, h) = ((b.x1 - b.x0 + 6.0) as f32, (b.y1 - b.y0 + 6.0) as f32);
                self.scale = (self.rect.width() / w).min(self.rect.height() / h).clamp(4.0, 40.0);
                let cx = ((b.x0 + b.x1) / 2.0) as f32;
                let cy = ((b.y0 + b.y1) / 2.0) as f32;
                self.off = vec2(self.rect.width() / 2.0 - cx * self.scale, self.rect.height() / 2.0 - cy * self.scale);
            }
            None => {
                self.scale = 20.0;
                self.off = vec2(self.rect.width() / 2.0, self.rect.height() / 2.0);
            }
        }
        self.want_fit = false;
    }
    pub fn center_on(&mut self, x: f64, y: f64) {
        self.off = vec2(self.rect.width() / 2.0 - x as f32 * self.scale, self.rect.height() / 2.0 - y as f32 * self.scale);
    }
    pub fn cancel(&mut self) {
        self.mode = Mode::Select;
        self.wire_pts.clear();
        self.drag = Drag::None;
    }

    fn is_ctl_el(el: &Element) -> bool {
        let ps = lib().pins_for(el);
        !ps.is_empty() && ps.iter().all(|p| p.k == "ci" || p.k == "co" || p.k == "g")
    }

    /* ---------------------------------------------------------------- drawing helpers */
    fn draw_symbol(&self, painter: &egui::Painter, el: &Element, syms: &mut SymCache, col: Color32, dashed: bool) {
        let s = syms.for_el(el);
        let st = |thin: bool| Stroke::new(if thin { 1.0 } else { 1.55 }, col);
        for p in &s.prims {
            match p {
                Prim::Poly { pts, closed, fill, thin, dash } => {
                    let sp: Vec<Pos2> = pts.iter().map(|q| {
                        let w = local_to_world(el, *q);
                        self.w2s(w[0], w[1])
                    }).collect();
                    if *fill && sp.len() >= 3 {
                        painter.add(Shape::convex_polygon(sp, col, Stroke::new(1.0, col)));
                    } else if *dash || dashed {
                        let mut v = sp.clone();
                        if *closed {
                            v.push(sp[0]);
                        }
                        painter.extend(Shape::dashed_line(&v, st(*thin), 3.0, 3.0));
                    } else if *closed {
                        painter.add(Shape::closed_line(sp, st(*thin)));
                    } else {
                        painter.add(Shape::line(sp, st(*thin)));
                    }
                }
                Prim::Circle { c, r, fill, thin } => {
                    let w = local_to_world(el, *c);
                    let cp = self.w2s(w[0], w[1]);
                    if *fill {
                        painter.circle_filled(cp, r * self.scale, col);
                    } else {
                        painter.circle_stroke(cp, r * self.scale, st(*thin));
                    }
                }
            }
        }
        // symbol texts drawn upright
        if self.scale >= 6.0 {
            let dx = tpt(el, 1.0, 0.0).0 - el.x;
            for t in &s.texts {
                if t.s.is_empty() {
                    continue;
                }
                let yc = t.y - t.sz * 0.36;
                let (x, y) = tpt(el, t.x, yc);
                let mut a = t.a.as_str();
                if a != "middle" {
                    if dx < 0.0 {
                        a = if a == "start" { "end" } else { "start" };
                    } else if dx == 0.0 {
                        a = "middle";
                    }
                }
                let al = match a {
                    "start" => Align2::LEFT_CENTER,
                    "end" => Align2::RIGHT_CENTER,
                    _ => Align2::CENTER_CENTER,
                };
                painter.text(self.w2s(x, y), al, &t.s, FontId::proportional(t.sz as f32 * self.scale), col);
            }
        }
    }

    fn pin_dir(el: &Element, p: &daedalus_core::library::PinDef) -> (f64, f64) {
        let d = lib().get(&el.typ).unwrap();
        let [x0, mut y0, x1, mut y1] = d.bb;
        if el.typ == "CBLOCK" {
            let pins = lib().pins_for(el);
            let n = pins.iter().filter(|q| q.k == "ci").count().max(1) as f64;
            let m = pins.iter().filter(|q| q.k == "co").count().max(1) as f64;
            let hh = n.max(m) / 2.0 + 0.4;
            y0 = -hh;
            y1 = hh;
        }
        let e = 1e-6;
        if p.x <= x0 + e {
            (1.0, 0.0)
        } else if p.x >= x1 - e {
            (-1.0, 0.0)
        } else if p.y <= y0 + e {
            (0.0, 1.0)
        } else if p.y >= y1 - e {
            (0.0, -1.0)
        } else {
            let (cx, cy) = ((x0 + x1) / 2.0, (y0 + y1) / 2.0);
            if (p.x - cx).abs() >= (p.y - cy).abs() {
                (if p.x < cx { 1.0 } else { -1.0 }, 0.0)
            } else {
                (0.0, if p.y < cy { 1.0 } else { -1.0 })
            }
        }
    }

    fn draw_pin_marks(&self, painter: &egui::Painter, el: &Element, pal: &Pal) {
        let d = lib().get(&el.typ).unwrap();
        if d.has("label") || d.has("text") {
            return;
        }
        for p in lib().pins_for(el) {
            if p.k == "gnd" || p.k == "lbl" {
                continue;
            }
            let (dx, dy) = Self::pin_dir(el, &p);
            let (px, py) = tpt(el, p.x, p.y);
            let (qx, qy) = tpt(el, p.x + dx, p.y + dy);
            let (ux, uy) = (qx - px, qy - py);
            let (cx, cy) = (px + ux * 0.42, py + uy * 0.42);
            let sc = |x: f64, y: f64| self.w2s(x, y);
            match p.k.as_str() {
                "p" => {
                    let r = Rect::from_center_size(sc(cx, cy), vec2(0.22 * self.scale, 0.22 * self.scale));
                    painter.rect_filled(r, 0.0, pal.sym);
                }
                "a" => {
                    let pts = vec![sc(cx, cy - 0.15), sc(cx + 0.15, cy), sc(cx, cy + 0.15), sc(cx - 0.15, cy)];
                    painter.add(Shape::convex_polygon(pts, Color32::TRANSPARENT, Stroke::new(1.0, pal.muted)));
                }
                k => {
                    let o = if k == "co" { -1.0 } else { 1.0 };
                    let (vx, vy) = (ux * o, uy * o);
                    let pts = vec![sc(cx + vx * 0.16, cy + vy * 0.16), sc(cx - vx * 0.16 - vy * 0.15, cy - vy * 0.16 + vx * 0.15), sc(cx - vx * 0.16 + vy * 0.15, cy - vy * 0.16 - vx * 0.15)];
                    painter.add(Shape::convex_polygon(pts, pal.ctl, Stroke::NONE));
                }
            }
        }
    }

    /// grey preview of an element that follows the mouse (placement and paste)
    fn draw_ghost(&self, painter: &egui::Painter, el: &Element, syms: &mut SymCache, pal: &Pal) {
        let d = match lib().get(&el.typ) {
            Some(d) => d,
            None => return,
        };
        if d.has("label") {
            self.draw_label_el(painter, el, pal, true);
        } else if d.has("text") {
            let t = el.params.get("text").cloned().unwrap_or_default();
            for (li, l) in t.split('\n').enumerate() {
                painter.text(self.w2s(el.x, el.y + li as f64 * 1.1), Align2::LEFT_BOTTOM, l, FontId::proportional(0.9 * self.scale), pal.ghost);
            }
        } else {
            self.draw_symbol(painter, el, syms, pal.ghost, false);
        }
    }

    fn draw_label_el(&self, painter: &egui::Painter, el: &Element, pal: &Pal, ghost: bool) {
        let t = el.params.get("text").cloned().unwrap_or_default();
        let wd = (0.62 * t.chars().count() as f64 + 0.9).max(1.6);
        let pts: Vec<Pos2> = [(0.0, 0.0), (0.5, -0.55), (wd, -0.55), (wd, 0.55), (0.5, 0.55)].iter().map(|&(a, b)| {
            let (x, y) = tpt(el, a, b);
            self.w2s(x, y)
        }).collect();
        let col = if ghost { pal.ghost } else { pal.accent };
        painter.add(Shape::convex_polygon(pts, if ghost { Color32::TRANSPARENT } else { pal.accent_soft }, Stroke::new(1.2, col)));
        let (cx, cy) = tpt(el, (0.5 + wd) / 2.0 + 0.05, 0.0);
        let q = tpt(el, 1.0, 0.0);
        let uy = q.1 - el.y;
        let angle = if uy > 0.5 { std::f32::consts::FRAC_PI_2 } else if uy < -0.5 { -std::f32::consts::FRAC_PI_2 } else { 0.0 };
        let font = FontId::proportional(0.85 * self.scale);
        let galley = painter.layout_no_wrap(t, font, col);
        let size = galley.size();
        let c = self.w2s(cx, cy);
        // rotate around the text centre
        let (s, co) = angle.sin_cos();
        let half = vec2(size.x / 2.0, size.y / 2.0);
        let rot = vec2(half.x * co - half.y * s, half.x * s + half.y * co);
        painter.add(TextShape::new(c - rot, galley, col).with_angle(angle));
    }

    /// Wires, junctions, elements and their labels. `export` (PDF): no selection or open-pin marks.
    pub fn draw_doc(&mut self, painter: &egui::Painter, ed: &mut Editor, syms: &mut SymCache, pal: &Pal, rect: Rect, export: bool) {
        // connectivity info
        let conn_ctl: Vec<bool>;
        let junc: Vec<(f64, f64)>;
        let open: Vec<(f64, f64)>;
        let el_ctl: Vec<bool>;
        ed.ensure_conn();
        {
            let c = ed.conn();
            conn_ctl = ed.doc.wires.iter().map(|w| c.is_ctl_root(c.root(w.x1, w.y1))).collect();
            junc = c.junctions.clone();
            open = c.open.clone();
            el_ctl = ed
                .doc
                .elements
                .iter()
                .map(|el| {
                    if Self::is_ctl_el(el) {
                        return true;
                    }
                    let pk = lib().pins_for(el);
                    if pk.is_empty() || !pk.iter().any(|p| p.k == "a") {
                        return false;
                    }
                    let mut any = false;
                    for p in wpins(el) {
                        let r = c.root(p.x, p.y);
                        if c.is_ctl_root(r) {
                            any = true;
                            continue;
                        }
                        if c.is_gnd_root(r) {
                            continue;
                        }
                        return false;
                    }
                    any
                })
                .collect();
        }
        let junc_ctl: Vec<bool> = {
            let c = ed.conn();
            junc.iter().map(|&(x, y)| c.is_ctl_root(c.root(x, y))).collect()
        };
        // wires
        for (i, w) in ed.doc.wires.iter().enumerate() {
            let sel = !export && ed.sel.contains(&Sel::W(w.id));
            let col = if sel { pal.sel } else if conn_ctl[i] { pal.ctl } else { pal.wire };
            painter.line_segment([self.w2s(w.x1, w.y1), self.w2s(w.x2, w.y2)], Stroke::new(if sel { 2.4 } else { 1.6 }, col));
        }
        for (k, &(x, y)) in junc.iter().enumerate() {
            painter.circle_filled(self.w2s(x, y), (0.2 * self.scale).max(2.5), if junc_ctl[k] { pal.ctl } else { pal.wire });
        }
        // elements
        self.label_rects.clear();
        for (i, el) in ed.doc.elements.iter().enumerate() {
            let d = match lib().get(&el.typ) {
                Some(d) => d,
                None => continue,
            };
            let sel = !export && ed.sel.contains(&Sel::El(el.id));
            let b = wbox(el);
            let brect = Rect::from_min_max(self.w2s(b.x0, b.y0), self.w2s(b.x1, b.y1));
            if !brect.expand(200.0).intersects(rect) {
                continue;
            }
            if d.has("label") {
                self.draw_label_el(&painter, el, pal, el.dis);
            } else if d.has("text") {
                let t = el.params.get("text").cloned().unwrap_or_default();
                for (li, l) in t.split('\n').enumerate() {
                    painter.text(self.w2s(el.x, el.y + li as f64 * 1.1), Align2::LEFT_BOTTOM, l, FontId::proportional(0.9 * self.scale), if sel { pal.sel } else { pal.muted });
                }
            } else {
                let mut col = if el_ctl[i] { pal.ctl } else { pal.sym };
                if el.dis {
                    col = pal.ghost;
                }
                if sel {
                    col = pal.sel;
                }
                if self.err_els.contains(&el.id) {
                    col = pal.err;
                }
                self.draw_symbol(&painter, el, syms, col, el.dis);
                if self.pin_marks && self.scale >= 8.0 {
                    self.draw_pin_marks(&painter, el, pal);
                }
                // name and parameter labels
                let lines = label_lines(el);
                if !lines.is_empty() && self.scale >= 6.0 {
                    let wide = (b.x1 - b.x0) >= (b.y1 - b.y0);
                    let (fs, lh) = (0.78f32 * self.label_k, 0.95f64 * self.label_k as f64);
                    let off = if d.has("lblRight") && el.rot & 1 == 0 { 0.5 } else { 0.0 };
                    let n = lines.len();
                    for (li, (key, txt)) in lines.iter().enumerate() {
                        let (x, y, al) = if wide {
                            ((b.x0 + b.x1) / 2.0 + off, b.y0 - 0.35 - (n - 1 - li) as f64 * lh, if off > 0.0 { Align2::LEFT_BOTTOM } else { Align2::CENTER_BOTTOM })
                        } else {
                            (b.x1 + 0.35, (b.y0 + b.y1) / 2.0 - (n - 1) as f64 * lh / 2.0 + li as f64 * lh, Align2::LEFT_CENTER)
                        };
                        let r = painter.text(self.w2s(x, y), al, txt, FontId::proportional(fs * self.scale), if el.dis { pal.ghost } else { pal.muted });
                        self.label_rects.push((el.id, key.clone(), r));
                    }
                }
            }
            if self.find_hits.contains(&el.id) {
                painter.rect_stroke(brect.expand(0.45 * self.scale), CornerRadius::same(4), Stroke::new(2.0, pal.warn), StrokeKind::Outside);
            }
            if sel {
                let r = brect.expand(0.3 * self.scale);
                let pts = vec![r.left_top(), r.right_top(), r.right_bottom(), r.left_bottom(), r.left_top()];
                painter.extend(Shape::dashed_line(&pts, Stroke::new(1.0, pal.sel), 3.0, 3.0));
            }
            if self.err_els.contains(&el.id) {
                painter.rect(brect.expand(0.4 * self.scale), CornerRadius::same(4), pal.err.gamma_multiply(0.08), Stroke::new(1.4, pal.err), StrokeKind::Outside);
            }
        }
        for &(x, y) in open.iter().filter(|_| !export) {
            let c = self.w2s(x, y);
            let r = (0.2 * self.scale).max(2.5);
            painter.circle(c, r, pal.canvas, Stroke::new(1.2, pal.err));
        }
    }

    /* ---------------------------------------------------------------- main */
    pub fn ui(&mut self, ui: &mut Ui, ed: &mut Editor, syms: &mut SymCache, pal: &Pal, events: &mut Vec<CanvasEvent>) {
        let rect = ui.available_rect_before_wrap();
        self.rect = rect;
        if self.want_fit {
            self.fit(ed);
        }
        let resp = ui.allocate_rect(rect, Sense::click_and_drag());
        let painter = ui.painter_at(rect);
        painter.rect_filled(rect, 0.0, pal.canvas);
        // grid
        {
            let (wx0, wy0) = self.s2w(rect.min);
            let (wx1, wy1) = self.s2w(rect.max);
            let step = if self.scale >= 8.0 { 1.0 } else if self.scale >= 3.0 { 5.0 } else { 10.0 };
            let mut x = (wx0 / step).floor() * step;
            let dot = if self.scale >= 14.0 { 1.3 } else { 1.0 };
            while x <= wx1 {
                let mut y = (wy0 / step).floor() * step;
                let major = (x / 5.0).round() * 5.0 == x;
                while y <= wy1 {
                    let mj = major && (y / 5.0).round() * 5.0 == y;
                    painter.circle_filled(self.w2s(x, y), if mj { dot + 0.4 } else { dot * 0.7 }, if mj { pal.grid } else { pal.grid2.lerp_to_gamma(pal.grid, 0.5) });
                    y += step;
                }
                x += step;
            }
        }
        self.draw_doc(&painter, ed, syms, pal, rect, false);

        /* ------------------------------------------------------------ interaction */
        let ptr = ui.input(|i| i.pointer.hover_pos());
        let in_rect = ptr.map(|p| rect.contains(p)).unwrap_or(false);
        self.mouse_world = if in_rect { ptr.map(|p| self.s2w(p)) } else { None };
        let snap = |w: (f64, f64)| (w.0.round(), w.1.round());
        let (shift, ctrl) = ui.input(|i| (i.modifiers.shift, i.modifiers.command));
        // zoom / pan with the wheel
        if resp.hovered() {
            let (scroll, zoom) = ui.input(|i| (i.smooth_scroll_delta, i.zoom_delta()));
            if let Some(p) = ptr {
                if zoom != 1.0 {
                    self.zoom_at(zoom, p);
                } else if scroll.y != 0.0 && !shift {
                    self.zoom_at((scroll.y / 400.0).exp(), p);
                } else if scroll.y != 0.0 && shift {
                    self.off.x += scroll.y;
                }
                if scroll.x != 0.0 {
                    self.off.x += scroll.x;
                }
            }
        }
        // middle-button (or space+left) pan
        let hand = matches!(self.mode, Mode::Hand);
        // middle drag, right drag (a right click without dragging opens the menu), space/hand + left drag
        let pan_btn = resp.dragged_by(egui::PointerButton::Middle) || resp.dragged_by(egui::PointerButton::Secondary) || ((self.space_down || hand) && resp.dragged_by(egui::PointerButton::Primary));
        if pan_btn {
            self.off += resp.drag_delta();
            self.drag = Drag::Pan;
        }
        let tol_pin = (6.0 / self.scale as f64).max(0.25).min(0.5);
        let mut hover_text: Option<String> = None;
        if let Some(w) = self.mouse_world {
            if let Some((id, pi, _, _)) = ed.near_pin(w.0, w.1, tol_pin) {
                if let Some(el) = ed.el(id) {
                    let pins = lib().pins_for(el);
                    if let Some(p) = pins.get(pi) {
                        let k = PIN_KIND.iter().find(|q| q.0 == p.k).map(|q| q.1).unwrap_or("");
                        hover_text = Some(format!("{} 接腳 {}（{}）", if el.name.is_empty() { &el.typ } else { &el.name }, p.n, k));
                    }
                }
            }
        }
        // placement modes (the mode is taken out while it is handled, and put back unless replaced)
        let mut mode = std::mem::replace(&mut self.mode, Mode::Select);
        let mut next_mode: Option<Mode> = None;
        match &mut mode {
            Mode::Place(proto) => {
                if let Some(w) = self.mouse_world {
                    let s = snap(w);
                    proto.x = s.0;
                    proto.y = s.1;
                    let g = (**proto).clone();
                    self.draw_ghost(&painter, &g, syms, pal);
                }
                if resp.clicked() {
                    let g = (**proto).clone();
                    match ed.place_element(&g, g.x, g.y) {
                        Ok(_) => {
                            if !shift {
                                next_mode = Some(Mode::Select);
                            }
                        }
                        Err(m) => events.push(CanvasEvent::Toast(m)),
                    }
                }
                if resp.secondary_clicked() {
                    next_mode = Some(Mode::Select);
                }
            }
            Mode::Paste(g) => {
                if let Some(w) = self.mouse_world {
                    let s = snap(w);
                    for e in &g.elements {
                        let mut e2 = e.clone();
                        e2.x += s.0;
                        e2.y += s.1;
                        self.draw_ghost(&painter, &e2, syms, pal);
                    }
                    for wi in &g.wires {
                        painter.line_segment([self.w2s(wi.x1 + s.0, wi.y1 + s.1), self.w2s(wi.x2 + s.0, wi.y2 + s.1)], Stroke::new(1.4, pal.ghost));
                    }
                    if resp.clicked() {
                        let g2 = g.clone();
                        ed.place_group(&g2, s.0, s.1);
                        next_mode = Some(Mode::Select);
                    }
                }
                if resp.secondary_clicked() {
                    next_mode = Some(Mode::Select);
                }
            }
            Mode::Hand => {
                hover_text = hover_text.or(Some("手形工具：拖曳移動視圖；Esc 或選取工具離開".into()));
                if resp.secondary_clicked() {
                    next_mode = Some(Mode::Select);
                }
            }
            Mode::Wire => {
                if let Some(w) = self.mouse_world {
                    let s = snap(w);
                    if let Some(&last) = self.wire_pts.last() {
                        let pts = l_path(last, s);
                        let sp: Vec<Pos2> = pts.iter().map(|p| self.w2s(p.0, p.1)).collect();
                        painter.extend(Shape::dashed_line(&sp, Stroke::new(1.6, pal.accent), 4.0, 3.0));
                    }
                    painter.circle_stroke(self.w2s(s.0, s.1), 4.0, Stroke::new(1.0, pal.accent));
                    if resp.clicked() {
                        if let Some(&last) = self.wire_pts.last() {
                            if last != s {
                                let pts = l_path(last, s);
                                ed.add_route(&pts);
                                self.wire_pts.push(s);
                                if ed.is_conn_point(s.0, s.1) && self.wire_pts.len() > 1 {
                                    self.wire_pts.clear();
                                }
                            }
                        } else {
                            self.wire_pts.push(s);
                        }
                    }
                    if resp.double_clicked() || resp.secondary_clicked() {
                        self.wire_pts.clear();
                    }
                }
                hover_text = hover_text.or(Some("導線模式：點選起點與轉折點，點到接腳或導線即結束；雙擊或右鍵結束，Esc 離開".into()));
            }
            Mode::Select => {
                let w = self.mouse_world;
                if resp.drag_started_by(egui::PointerButton::Primary) && !self.space_down {
                    if let Some(p0) = resp.interact_pointer_pos() {
                        // the press position (not the current one) decides what is being dragged
                        let p0 = ui.input(|i| i.pointer.press_origin()).unwrap_or(p0);
                        let w0 = self.s2w(p0);
                        if let Some((_, _, px, py)) = ed.near_pin(w0.0, w0.1, tol_pin) {
                            self.drag = Drag::WireFrom { start: (px, py) };
                        } else if let Some(id) = ed.hit_element(w0.0, w0.1).map(Sel::El).or_else(|| ed.hit_wire(w0.0, w0.1, 0.3).map(Sel::W)) {
                            if !ed.sel.contains(&id) {
                                if !(shift || ctrl) {
                                    ed.sel.clear();
                                }
                                ed.sel.insert(id);
                            }
                            self.drag = Drag::Move { base: ed.doc.clone(), start: snap(w0), moved: false };
                        } else {
                            self.drag = Drag::Band { start: p0 };
                        }
                    }
                }
                let mut drag = std::mem::replace(&mut self.drag, Drag::None);
                let mut drag_done = false;
                match &mut drag {
                    Drag::Move { base, start, moved } => {
                        if let Some(w) = w {
                            let s = snap(w);
                            let (dx, dy) = (s.0 - start.0, s.1 - start.1);
                            if dx != 0.0 || dy != 0.0 || *moved {
                                if !*moved {
                                    ed.push_undo();
                                    *moved = true;
                                }
                                let b = base.clone();
                                ed.move_selection(&b, dx, dy);
                            }
                        }
                        if resp.drag_stopped() {
                            if *moved {
                                ed.finish_move();
                            }
                            drag_done = true;
                        }
                    }
                    Drag::Band { start } => {
                        if let Some(p) = ptr {
                            let r = Rect::from_two_pos(*start, p);
                            painter.rect(r, 0.0, pal.accent_soft, Stroke::new(1.0, pal.accent), StrokeKind::Inside);
                            if resp.drag_stopped() {
                                let a = self.s2w(r.min);
                                let b = self.s2w(r.max);
                                ed.select_in_rect(a.0, a.1, b.0, b.1, shift || ctrl);
                                drag_done = true;
                            }
                        }
                        if resp.drag_stopped() {
                            drag_done = true;
                        }
                    }
                    Drag::WireFrom { start } => {
                        if let Some(w) = w {
                            let s = snap(w);
                            let pts = l_path(*start, s);
                            let sp: Vec<Pos2> = pts.iter().map(|p| self.w2s(p.0, p.1)).collect();
                            painter.extend(Shape::dashed_line(&sp, Stroke::new(1.6, pal.accent), 4.0, 3.0));
                            if resp.drag_stopped() {
                                if s != *start {
                                    ed.add_route(&pts);
                                }
                                drag_done = true;
                            }
                        } else if resp.drag_stopped() {
                            drag_done = true;
                        }
                    }
                    Drag::Pan => {
                        if !pan_btn {
                            drag_done = true;
                        }
                    }
                    Drag::None => {}
                }
                self.drag = if drag_done { Drag::None } else { drag };
                if resp.clicked() {
                    if let Some(w) = w {
                        // a click on a name / parameter text edits it in place
                        let p = ptr.unwrap();
                        if let Some((id, key, r)) = self.label_rects.iter().rev().find(|(_, _, r)| r.contains(p)).cloned() {
                            if !(shift || ctrl) {
                                // the first click selects the element (so copy / paste / rotate act on it); a click on
                                // the text of the element that is already selected edits that text in place
                                let already = ed.sel.len() == 1 && ed.sel.contains(&Sel::El(id));
                                ed.sel.clear();
                                ed.sel.insert(Sel::El(id));
                                if already && !resp.double_clicked() {
                                    events.push(CanvasEvent::InlineEdit(id, key, r.left_bottom()));
                                }
                            } else if !ed.sel.remove(&Sel::El(id)) {
                                ed.sel.insert(Sel::El(id));
                            }
                        } else {
                            let hit = ed.hit_element(w.0, w.1).map(Sel::El).or_else(|| ed.hit_wire(w.0, w.1, (5.0 / self.scale as f64).max(0.2)).map(Sel::W));
                            match hit {
                                Some(h) => {
                                    if shift || ctrl {
                                        if !ed.sel.remove(&h) {
                                            ed.sel.insert(h);
                                        }
                                    } else {
                                        ed.sel.clear();
                                        ed.sel.insert(h);
                                    }
                                }
                                None => {
                                    if !(shift || ctrl) {
                                        ed.sel.clear();
                                    }
                                }
                            }
                        }
                    }
                }
                if resp.double_clicked() {
                    if let (Some(w), Some(p)) = (w, ptr) {
                        match ed.hit_element(w.0, w.1) {
                            Some(id) => events.push(CanvasEvent::OpenProps(id)),
                            None if self.dbl_quick => events.push(CanvasEvent::QuickAdd(p, snap(w))),
                            None => {}
                        }
                    }
                }
                if resp.secondary_clicked() {
                    if let Some(w) = w {
                        if let Some(h) = ed.hit_element(w.0, w.1).map(Sel::El).or_else(|| ed.hit_wire(w.0, w.1, 0.3).map(Sel::W)) {
                            if !ed.sel.contains(&h) {
                                ed.sel.clear();
                                ed.sel.insert(h);
                            }
                        }
                    }
                    events.push(CanvasEvent::ContextMenu);
                }
                if hover_text.is_none() {
                    if let Some(w) = w {
                        hover_text = pin_hover(ed, w, (6.0 / self.scale as f64).max(0.25));
                    }
                }
                if hover_text.is_none() {
                    if let Some(w) = w {
                        if let Some(id) = ed.hit_element(w.0, w.1) {
                            if let Some(el) = ed.el(id) {
                                if let Some(d) = lib().get(&el.typ) {
                                    hover_text = Some(format!("{}{}（{}）", if el.name.is_empty() { String::new() } else { format!("{} — ", el.name) }, d.name, d.zh));
                                }
                            }
                        }
                    }
                }
            }
        }
        self.mode = next_mode.unwrap_or(mode);
        if let Some(t) = hover_text {
            if self.space_down || matches!(self.drag, Drag::Move { .. } | Drag::Band { .. }) {
            } else {
                resp.clone().on_hover_text_at_pointer(t);
            }
        }
        if matches!(self.drag, Drag::Pan) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grabbing);
        } else if self.space_down || matches!(self.mode, Mode::Hand) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Grab);
        } else if matches!(self.mode, Mode::Wire) || matches!(self.drag, Drag::WireFrom { .. }) {
            ui.ctx().set_cursor_icon(egui::CursorIcon::Crosshair);
        }
    }
}

/// the pin under the pointer (within `tol` grid units): "R1 的接腳 I+：電流由此流入…"
fn pin_hover(ed: &Editor, w: (f64, f64), tol: f64) -> Option<String> {
    let mut best: Option<(f64, String)> = None;
    for el in &ed.doc.elements {
        let d = match lib().get(&el.typ) {
            Some(d) => d,
            None => continue,
        };
        if d.has("label") || d.has("text") {
            continue;
        }
        for p in lib().pins_for(el) {
            if p.k == "gnd" || p.k == "lbl" {
                continue;
            }
            let (px, py) = tpt(el, p.x, p.y);
            let dist = ((px - w.0).powi(2) + (py - w.1).powi(2)).sqrt();
            if dist <= tol && best.as_ref().map(|b| dist < b.0).unwrap_or(true) {
                let who = if el.name.is_empty() { d.zh.clone() } else { format!("{}（{}）", el.name, d.zh) };
                best = Some((dist, format!("{} 的接腳「{}」：{}", who, p.n, pin_role(&el.typ, &p.n, &p.k))));
            }
        }
    }
    best.map(|b| b.1)
}

/// what a pin is for (hover text)
pub fn pin_role(typ: &str, name: &str, kind: &str) -> String {
    let kind_txt = match kind {
        "p" => "功率接腳",
        "a" => "功率或控制接腳",
        "ci" => "控制輸入",
        "g" => "閘極（接控制訊號）",
        "co" => "控制輸出",
        _ => "接腳",
    };
    let role = match (typ, name) {
        ("WATT", "I+") => "電流由此流入；量測電壓為 I+ 相對 V−",
        ("WATT", "I-") => "電流由此流出",
        ("WATT", "V-") => "電壓的參考端（負端）",
        ("WH3", "A+") | ("WH3", "B+") | ("WH3", "C+") => "該相電流由此流入；該相電壓為此點相對 N",
        ("WH3", "A-") | ("WH3", "B-") | ("WH3", "C-") => "該相電流由此流出",
        ("WH3", "N") => "電壓參考點（中性點；三相三線時可接任一點）",
        ("DVDT", "in") => "輸入電壓（對地）",
        ("DVDT", "out") => "輸出：輸入電壓的時間微分（對地電壓源）",
        ("CTOP", "out") => "輸出：對地的電壓源",
        (_, "in") | (_, "out") if kind == "p" => if name == "in" { "電流由此流入（量測方向：in → out）" } else { "電流由此流出" },
        (_, "+") | (_, "in+") | (_, "c+") | (_, "IN+") | (_, "V+") => "正端",
        (_, "-") | (_, "in-") | (_, "c-") | (_, "IN-") | (_, "V-") => "負端",
        (_, "vin1") => "輸入 1",
        (_, "vin2") => "輸入 2",
        (_, "dot") => "注入點之後（圓點端）",
        (_, "x") if typ == "ACP2" => "注入點之前",
        _ => "",
    };
    if role.is_empty() {
        kind_txt.to_string()
    } else {
        format!("{}（{}）", role, kind_txt)
    }
}
