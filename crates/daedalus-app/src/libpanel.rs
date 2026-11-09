//! Component library browser: category tree, search, symbol previews, description.

use crate::dialogs::search_lib;
use crate::symbols::{Prim, SymCache};
use crate::theme::Pal;
use daedalus_core::doc::Element;
use daedalus_core::library::{lib, ElDef};
use egui::{pos2, vec2, Color32, Rect, Sense, Shape, Stroke, Ui};

pub struct LibPanel {
    pub search: String,
    pub hover: Option<String>,
    pub selected: Option<String>,
}

pub fn cat_label(seg: &str) -> String {
    let zh = match seg {
        "Elements" => "元件",
        "Power" => "功率元件",
        "RLC Branches" => "RLC 支路",
        "Switches" => "開關",
        "Non-ideal Switches" => "非理想開關",
        "Transformers" => "變壓器",
        "Other" => "其他",
        "Control" => "控制元件",
        "Filters" => "濾波器",
        "Computational Blocks" => "運算方塊",
        "Other Function Blocks" => "其他功能方塊",
        "Logic Elements" => "邏輯元件",
        "Digital Control Module" => "數位控制模組",
        "Switch Controllers" => "開關控制器",
        "Sensors" => "感測器",
        "Probes" => "探棒",
        "Function Blocks" => "函數方塊",
        "Sources" => "電源",
        "Voltage" => "電壓源",
        "Current" => "電流源",
        "Symbols" => "符號",
        _ => "",
    };
    if zh.is_empty() {
        seg.to_string()
    } else {
        format!("{} {}", zh, seg)
    }
}

/// draw a small symbol preview of an element type into rect
pub fn draw_icon(ui: &Ui, rect: Rect, d: &ElDef, syms: &mut SymCache, col: Color32) {
    let el = Element { id: 0, typ: d.typ.clone(), x: 0.0, y: 0.0, rot: 0, mir: false, dis: false, name: String::new(), params: lib().default_params(&d.typ), disp: None };
    let s = syms.for_el(&el);
    let [x0, y0, x1, y1] = d.bb;
    let (w, h) = ((x1 - x0).max(1.0) as f32, (y1 - y0).max(1.0) as f32);
    let sc = (rect.width() / w).min(rect.height() / h) * 0.9;
    let c = rect.center();
    let (cx, cy) = (((x0 + x1) / 2.0) as f32, ((y0 + y1) / 2.0) as f32);
    let tf = |p: [f32; 2]| pos2(c.x + (p[0] - cx) * sc, c.y + (p[1] - cy) * sc);
    let p = ui.painter_at(rect);
    for pr in &s.prims {
        match pr {
            Prim::Poly { pts, closed, fill, .. } => {
                let v: Vec<egui::Pos2> = pts.iter().map(|q| tf(*q)).collect();
                if *fill && v.len() >= 3 {
                    p.add(Shape::convex_polygon(v, col, Stroke::NONE));
                } else if *closed {
                    p.add(Shape::closed_line(v, Stroke::new(1.0, col)));
                } else {
                    p.add(Shape::line(v, Stroke::new(1.0, col)));
                }
            }
            Prim::Circle { c: cc, r, fill, .. } => {
                if *fill {
                    p.circle_filled(tf(*cc), r * sc, col);
                } else {
                    p.circle_stroke(tf(*cc), r * sc, Stroke::new(1.0, col));
                }
            }
        }
    }
    if d.typ == "LABEL" {
        p.text(c, egui::Align2::CENTER_CENTER, "Vo", egui::FontId::proportional(10.0), col);
    }
}

impl LibPanel {
    pub fn new() -> Self {
        LibPanel { search: String::new(), hover: None, selected: None }
    }
    fn item(&mut self, ui: &mut Ui, d: &'static ElDef, syms: &mut SymCache, pal: &Pal, picked: &mut Option<String>) {
        let ctl = d.cat.starts_with("Elements/Control") || ["Switch Controllers", "Function Blocks"].iter().any(|s| d.cat.ends_with(s));
        let (rect, resp) = ui.allocate_exact_size(vec2(ui.available_width(), 26.0), Sense::click_and_drag());
        let sel = self.selected.as_deref() == Some(d.typ.as_str());
        if resp.hovered() || sel {
            ui.painter().rect_filled(rect, 3.0, if sel { pal.accent_soft } else { ui.visuals().widgets.hovered.weak_bg_fill });
        }
        let icon = Rect::from_min_size(rect.min + vec2(4.0, 2.0), vec2(30.0, 22.0));
        draw_icon(ui, icon, d, syms, if ctl { pal.ctl } else { pal.sym });
        ui.painter().text(pos2(icon.max.x + 6.0, rect.center().y), egui::Align2::LEFT_CENTER, format!("{}  {}", d.name, d.zh), egui::FontId::proportional(13.0), ui.visuals().text_color());
        if resp.hovered() {
            self.hover = Some(d.typ.clone());
        }
        if resp.clicked() || resp.drag_started() {
            self.selected = Some(d.typ.clone());
            *picked = Some(d.typ.clone());
        }
        resp.on_hover_text(format!("{}（{}）\n{}", d.name, d.typ, d.desc));
    }
    /// returns the element type picked for placement
    pub fn ui(&mut self, ui: &mut Ui, syms: &mut SymCache, pal: &Pal) -> Option<String> {
        let mut picked = None;
        self.hover = None;
        ui.add_space(4.0);
        ui.horizontal(|ui| {
            let (r, _) = ui.allocate_exact_size(egui::vec2(18.0, 18.0), egui::Sense::hover());
            crate::icons::paint(ui, r, crate::icons::Icon::Search, ui.visuals().weak_text_color());
            ui.add(egui::TextEdit::singleline(&mut self.search).hint_text("搜尋元件：mosfet、比較器、PWM…").desired_width(f32::INFINITY));
        });
        ui.separator();
        let desc_h = 150.0;
        egui::ScrollArea::vertical().max_height((ui.available_height() - desc_h).max(100.0)).auto_shrink([false, false]).show(ui, |ui| {
            if !self.search.trim().is_empty() {
                for d in search_lib(&self.search).into_iter().take(80) {
                    self.item(ui, d, syms, pal, &mut picked);
                }
                return;
            }
            // category tree
            let mut cats: Vec<&str> = vec![];
            for d in &lib().defs {
                if !cats.contains(&d.cat.as_str()) {
                    cats.push(&d.cat);
                }
            }
            let mut tops: Vec<&str> = vec![];
            for c in &cats {
                let t = c.split('/').nth(1).unwrap_or("");
                if !tops.contains(&t) {
                    tops.push(t);
                }
            }
            for top in tops {
                egui::CollapsingHeader::new(cat_label(top)).id_salt(("cat", top)).default_open(top == "Power").show(ui, |ui| {
                    for c in cats.iter().filter(|c| c.split('/').nth(1) == Some(top)) {
                        let sub = c.split('/').nth(2);
                        let items: Vec<&'static ElDef> = lib().defs.iter().filter(|d| d.cat == *c).collect();
                        match sub {
                            None => {
                                for d in items {
                                    self.item(ui, d, syms, pal, &mut picked);
                                }
                            }
                            Some(s) => {
                                egui::CollapsingHeader::new(cat_label(s)).id_salt(("cat", *c)).default_open(*c == "Elements/Power/RLC Branches" || *c == "Elements/Power/Switches").show(ui, |ui| {
                                    for d in items {
                                        self.item(ui, d, syms, pal, &mut picked);
                                    }
                                });
                            }
                        }
                    }
                });
            }
        });
        ui.separator();
        let show = self.hover.clone().or_else(|| self.selected.clone());
        egui::ScrollArea::vertical().id_salt("libdesc").max_height(desc_h).show(ui, |ui| match show.and_then(|t| lib().get(&t)) {
            Some(d) => {
                ui.strong(format!("{}（{}）", d.name, d.zh));
                ui.weak(&d.typ);
                ui.label(&d.desc);
                if !d.pins.is_empty() {
                    let kinds = |k: &str| match k {
                        "p" => "■",
                        "ci" | "g" => "▶",
                        "co" => "◀",
                        "a" => "◇",
                        _ => "·",
                    };
                    ui.weak(format!("接腳：{}", d.pins.iter().map(|p| format!("{}{}", kinds(&p.k), p.n)).collect::<Vec<_>>().join("  ")));
                }
            }
            None => {
                ui.weak("點選元件後在電路圖上左鍵放置；按住 Shift 可連續放置。");
            }
        });
        picked
    }
}
