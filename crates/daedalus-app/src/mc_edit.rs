//! Monte Carlo tolerance list as a table in the properties dialog: pick an element parameter (or a
//! parameter-file variable) from a list, type the tolerance, choose the distribution. Every row shows
//! the nominal value and the sampling range, or what is wrong with it. The text ("R1.R 5% gauss" per
//! line) stays the file format and can still be edited directly.

use daedalus_core::doc::Doc;
use daedalus_core::expr::Syms;
use daedalus_core::montecarlo::{self as mc, Dist};
use egui::{Color32, RichText, Ui};

const RED: Color32 = Color32::from_rgb(0xd9, 0x36, 0x3e);
const ORANGE: Color32 = Color32::from_rgb(0xc8, 0x7a, 0x00);
/// column widths: target, nominal, tolerance, distribution
const W: [f32; 4] = [190.0, 96.0, 56.0, 118.0];

#[derive(Clone, Debug, PartialEq)]
pub struct McRow {
    pub target: String,
    /// percent, without the % sign
    pub tol: String,
    pub gauss: bool,
}

pub struct McEdit {
    pub rows: Vec<McRow>,
    /// the text the rows were read from or last written to
    text: String,
    /// per row: (ok, message) of the last frame (GUI test)
    pub status: Vec<(bool, String)>,
}

fn row_from_line(line: &str) -> Option<McRow> {
    let t = mc::line_tokens(line);
    let target = t.first()?.clone();
    let tol = match t.get(1) {
        Some(s) => match s.strip_suffix('%') {
            Some(p) => p.trim().to_string(),
            None => match s.parse::<f64>() {
                Ok(v) if v.abs() < 1.0 => mc::fmt_pct(v),
                _ => s.clone(),
            },
        },
        None => String::new(),
    };
    let gauss = t.get(2).map(|d| mc::parse_dist(d) != Some(Dist::Uniform)).unwrap_or(true);
    Some(McRow { target, tol, gauss })
}

impl McEdit {
    pub fn new(text: &str) -> Self {
        McEdit { rows: text.split('\n').filter_map(row_from_line).collect(), text: text.to_string(), status: vec![] }
    }
    pub fn to_text(&self) -> String {
        self.rows
            .iter()
            .map(|r| format!("{} {}% {}", r.target, r.tol.trim().trim_end_matches(['%', '％']).trim(), if r.gauss { "gauss" } else { "uniform" }))
            .collect::<Vec<_>>()
            .join("\n")
    }
    /// add a row unless that target is already listed; true when added
    pub fn add(&mut self, doc: &Doc, syms: &Syms, target: &str) -> bool {
        let t = mc::resolve(doc, syms, target).ok().map(|r| r.target);
        let same = |r: &McRow| t.as_ref().map(|t| mc::resolve(doc, syms, &r.target).map(|x| x.target.same(t)).unwrap_or(false)).unwrap_or(false);
        let dup = self.rows.iter().any(|r| r.target.eq_ignore_ascii_case(target) || same(r));
        if !dup {
            self.rows.push(McRow { target: target.to_string(), tol: "5".into(), gauss: true });
        }
        !dup
    }
    /// every resistor, inductor and capacitor, with its main value
    pub fn add_rlc(&mut self, doc: &Doc, syms: &Syms) -> usize {
        let names: Vec<String> = doc.elements.iter().filter(|e| !e.dis && matches!(e.typ.as_str(), "R" | "L" | "C") && !e.name.is_empty()).map(|e| format!("{}.{}", e.name, e.typ)).collect();
        names.iter().filter(|n| self.add(doc, syms, n)).count()
    }
    /// write the rows back after an edit
    pub fn store(&mut self, vars: &mut String) {
        *vars = self.to_text();
        self.text = vars.clone();
    }

    pub fn ui(&mut self, ui: &mut Ui, doc: &Doc, syms: &Syms, vars: &mut String) {
        if *vars != self.text {
            // edited as text (or reset): read the rows again
            *self = McEdit { status: std::mem::take(&mut self.status), ..McEdit::new(vars) };
        }
        let checks = mc::check_lines(doc, syms, vars);
        let cands = mc::candidates(doc, syms);
        let mut changed = false;
        ui.strong("容差設定 Tolerances");
        ui.label("每一列是一個會隨機變化的數值：從清單選擇元件參數（例如 R1.R）或參數檔中的變數，填入容差並選擇分佈。每次模擬都重新抽樣，第 0 次為標稱值。");
        ui.add_space(2.0);
        let h = ui.spacing().interact_size.y;
        ui.horizontal(|ui| {
            for (t, w) in ["對象（元件.參數 或 變數）", "標稱值", "容差 ±", "分佈"].iter().zip(W) {
                ui.add_sized([w, h], egui::Label::new(RichText::new(*t).strong()));
            }
            ui.add_space(14.0);
        });
        let mut remove = None;
        self.status.clear();
        for (i, row) in self.rows.iter_mut().enumerate() {
            let chk = checks.get(i);
            ui.horizontal(|ui| {
                let shown = if row.target.is_empty() { "（請選擇）".to_string() } else { row.target.clone() };
                egui::ComboBox::from_id_salt(("mc_target", i)).selected_text(shown).width(W[0]).height(380.0).show_ui(ui, |ui| {
                    ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                    let mut group = "";
                    for (t, g, d) in &cands {
                        if g != group {
                            group = g.as_str();
                            ui.label(RichText::new(g.as_str()).small().weak());
                        }
                        if ui.selectable_value(&mut row.target, t.clone(), format!("{}    {}", t, d)).clicked() {
                            changed = true;
                        }
                    }
                });
                let nom = match chk.map(|c| (&c.var, &c.unit)) {
                    Some((Ok(v), u)) => crate::plot::si(v.nominal, u, 4),
                    _ => "—".into(),
                };
                ui.add_sized([W[1], h], egui::Label::new(RichText::new(nom).monospace()));
                let te = egui::TextEdit::singleline(&mut row.tol).desired_width(W[2] - 14.0).align(egui::Align2::RIGHT_CENTER);
                if ui.add(te).on_hover_text("百分比，例如 5 代表 ±5 %").changed() {
                    changed = true;
                }
                ui.label("%");
                egui::ComboBox::from_id_salt(("mc_dist", i)).selected_text(if row.gauss { "常態 gauss" } else { "均勻 uniform" }).width(W[3]).show_ui(ui, |ui| {
                    changed |= ui.selectable_value(&mut row.gauss, true, "常態 gauss").on_hover_text("常態分佈：容差 = 3σ，抽樣截斷在 ±容差內").clicked();
                    changed |= ui.selectable_value(&mut row.gauss, false, "均勻 uniform").on_hover_text("在 ±容差內均勻抽樣").clicked();
                });
                if ui.button("移除").clicked() {
                    remove = Some(i);
                }
            });
            let st = match chk.map(|c| (&c.var, &c.unit, &c.warn)) {
                Some((Ok(v), u, w)) => {
                    let (lo, hi) = (v.nominal * (1.0 - v.tol), v.nominal * (1.0 + v.tol));
                    let (lo, hi) = (lo.min(hi), lo.max(hi));
                    let sig = if v.dist == Dist::Gauss {
                        let s = format!("{:.3}", v.tol * 100.0 / 3.0);
                        format!("，σ = {} %", s.trim_end_matches('0').trim_end_matches('.'))
                    } else {
                        String::new()
                    };
                    let range = format!("{} 的抽樣範圍 {} ~ {}{}", v.name, crate::plot::si(lo, u, 4), crate::plot::si(hi, u, 4), sig);
                    ui.label(RichText::new(&range).weak());
                    if let Some(w) = w {
                        ui.colored_label(ORANGE, w);
                    }
                    (true, w.clone().unwrap_or(range))
                }
                Some((Err(e), _, _)) => {
                    ui.colored_label(RED, e);
                    (false, e.clone())
                }
                None => (false, String::new()),
            };
            self.status.push(st);
        }
        if let Some(i) = remove {
            self.rows.remove(i);
            changed = true;
        }
        if self.rows.is_empty() {
            ui.colored_label(ORANGE, "尚未加入任何容差：按下方「＋ 加入…」選擇要隨機變化的元件參數。");
        }
        ui.horizontal(|ui| {
            let mut pick = None;
            ui.menu_button("＋ 加入元件參數或變數…", |ui| {
                ui.style_mut().wrap_mode = Some(egui::TextWrapMode::Extend);
                egui::ScrollArea::vertical().max_height(420.0).show(ui, |ui| {
                    if cands.is_empty() {
                        ui.label("電路中還沒有可設定容差的元件");
                    }
                    let mut group = "";
                    for (t, g, d) in &cands {
                        if g != group {
                            group = g.as_str();
                            ui.label(RichText::new(g.as_str()).small().weak());
                        }
                        if ui.button(format!("{}    {}", t, d)).clicked() {
                            pick = Some(t.clone());
                        }
                    }
                });
                if pick.is_some() {
                    ui.close();
                }
            });
            if let Some(t) = pick {
                changed |= self.add(doc, syms, &t);
            }
            if ui.button("加入所有 R、L、C").on_hover_text("電路中每個電阻、電感、電容的數值各加一列（±5 % 常態，可再修改）").clicked() {
                changed |= self.add_rlc(doc, syms) > 0;
            }
        });
        if changed {
            self.store(vars);
        }
        egui::CollapsingHeader::new("以文字編輯（進階）").id_salt("mc_text").show(ui, |ui| {
            ui.label("每行一個：對象 容差 分佈，例如「R1.R 5% gauss」、「C1 10% uniform」。對象寫「元件名稱.參數」、只寫元件名稱（代表它的主要數值）或參數檔中的變數；# 之後為註解。");
            ui.add(egui::TextEdit::multiline(vars).code_editor().desired_rows(4).desired_width(f32::INFINITY));
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn rows_round_trip() {
        let m = McEdit::new("R1 5% gauss\n# note\nC1.C 0.1 uniform\n\nL1 2 %");
        assert_eq!(m.rows.len(), 3);
        assert_eq!(m.rows[1], McRow { target: "C1.C".into(), tol: "10".into(), gauss: false });
        assert_eq!(m.to_text(), "R1 5% gauss\nC1.C 10% uniform\nL1 2% gauss");
        assert_eq!(McEdit::new(&m.to_text()).rows, m.rows);
    }
}
