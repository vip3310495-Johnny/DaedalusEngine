//! Dialogs: element properties, simulation control, quick add, inline edit, find, help, about.

use crate::editor::Editor;
use daedalus_core::expr::{self, Syms};
use daedalus_core::library::{lib, ElDef, ParamDef};
use daedalus_core::models::devices::{parse_spice_model, preset};
use daedalus_core::netlist::doc_syms;
use egui::{Color32, RichText, Ui};
use std::collections::BTreeMap;

/// Width of the value fields (2.5 × the 40 points a grid column used to give them).
const FIELD_W: f32 = 100.0;

/// A text field in a grid column (other than the last) gets only that column's width from the previous frame,
/// initially `interact_size.x` = 40 points, so `desired_width` alone never widened it. Reserve the width first;
/// the field is at least `min` wide and fills a column that something else (a combo box, a code editor) widened.
fn wide(ui: &mut Ui, min: f32, te: egui::TextEdit) -> egui::Response {
    let w = ui.available_width().max(min);
    ui.vertical(|ui| {
        ui.set_min_width(w);
        ui.add(te.desired_width(w))
    })
    .inner
}

/// validation of one parameter value (same rules as the simulator)
pub fn validate(p: &ParamDef, v: &str, syms: &Syms, typ: &str) -> Option<String> {
    let t = p.typ.as_deref().unwrap_or("num");
    let r = match t {
        "num" => {
            if v.trim().is_empty() {
                return Some("不可空白".into());
            }
            expr::eval_const(v, syms).map(|_| ())
        }
        "list" => {
            if v.trim().is_empty() {
                Ok(())
            } else {
                expr::eval_list(v, syms).map(|_| ())
            }
        }
        "expr" => {
            let args: Vec<String> = if typ == "NONI" {
                vec!["v".into()]
            } else if typ == "NONV" {
                vec!["i".into()]
            } else if typ.starts_with("MATH") {
                let mut a = vec!["t".to_string(), "x".to_string()];
                a.extend((1..=10).map(|i| format!("x{}", i)));
                a
            } else {
                vec!["t".into()]
            };
            let ar: Vec<&str> = args.iter().map(|s| s.as_str()).collect();
            expr::compile_fn(v, &ar, syms).map(|_| ())
        }
        "text" if p.k == "pairs" => expr::eval_pairs(v, syms).map(|_| ()),
        "code" if p.k == "spice" => {
            let (_, bad) = parse_spice_model(v);
            if !bad.is_empty() {
                return Some(format!("無法解析：{}", bad.join("、")));
            }
            Ok(())
        }
        _ => Ok(()),
    };
    r.err().map(|e| e.0)
}

pub struct PropsDlg {
    pub id: i64,
    pub name: String,
    pub vals: BTreeMap<String, String>,
    pub disp: BTreeMap<String, bool>,
    pub err: String,
    syms: Syms,
    /// width of the last value field drawn (GUI test)
    pub field_w: f32,
    /// the tolerance table of a Monte Carlo element
    pub mc: Option<crate::mc_edit::McEdit>,
}
pub enum DlgResult {
    Keep,
    Close,
    Apply,
}

impl PropsDlg {
    pub fn new(ed: &Editor, id: i64) -> Option<Self> {
        let el = ed.el(id)?;
        let d = lib().get(&el.typ)?;
        let mut disp = BTreeMap::new();
        let ed_disp = el.disp.clone().unwrap_or_default();
        disp.insert("name".into(), ed_disp.get("name").copied().unwrap_or(true));
        for p in &d.params {
            disp.insert(p.k.clone(), ed_disp.get(&p.k).copied().unwrap_or_else(|| d.show.contains(&p.k)));
        }
        let mc = (el.typ == "MONTECARLO").then(|| crate::mc_edit::McEdit::new(el.params.get("vars").map(|s| s.as_str()).unwrap_or("")));
        Some(PropsDlg { id, name: el.name.clone(), vals: el.params.clone(), disp, err: String::new(), syms: doc_syms(&ed.doc), field_w: 0.0, mc })
    }
    /// GUI test: edit the Monte Carlo table ("add" target, "push" target unchecked, "rlc", "remove" row)
    pub fn script_mc(&mut self, ed: &Editor, cmd: &str, arg: &str) {
        let Some(m) = self.mc.as_mut() else { return };
        match cmd {
            "add" => {
                m.add(&ed.doc, &self.syms, arg);
            }
            "push" => m.rows.push(crate::mc_edit::McRow { target: arg.to_string(), tol: "5".into(), gauss: true }),
            "rlc" => {
                m.add_rlc(&ed.doc, &self.syms);
            }
            "remove" => {
                if let Ok(i) = arg.parse::<usize>() {
                    if i < m.rows.len() {
                        m.rows.remove(i);
                    }
                }
            }
            _ => return,
        }
        m.store(self.vals.entry("vars".to_string()).or_default());
    }
    fn param_row(&mut self, ui: &mut Ui, d: &ElDef, p: &ParamDef, model_ideal: bool) {
        let model = self.vals.get("model").cloned().unwrap_or_default();
        let mut val = self.vals.get(&p.k).cloned().unwrap_or_default();
        self.param_widgets(ui, d, p, model_ideal, &model, &mut val);
        self.vals.insert(p.k.clone(), val);
    }
    fn param_widgets(&mut self, ui: &mut Ui, d: &ElDef, p: &ParamDef, model_ideal: bool, model: &str, v: &mut String) {
        let lab = ui.label(&p.l);
        if !p.desc.is_empty() {
            lab.on_hover_text(&p.desc);
        }
        let enabled = !(p.ideal_only && !model_ideal) && !(p.k == "spice" && model_ideal);
        let t = p.typ.as_deref().unwrap_or("num");
        let err = if enabled && t != "choice" { validate(p, v, &self.syms, &d.typ) } else { None };
        ui.add_enabled_ui(enabled, |ui| {
            match t {
                "choice" => {
                    let opts = p.opts.clone().unwrap_or_default();
                    let cur = opts.iter().find(|o| o.0 == *v).map(|o| o.1.clone()).unwrap_or_else(|| v.clone());
                    egui::ComboBox::from_id_salt(("prm", &p.k)).selected_text(cur).width(260.0).show_ui(ui, |ui| {
                        for (k, l) in &opts {
                            ui.selectable_value(v, k.clone(), l);
                        }
                    });
                }
                "code" => {
                    let mut te = egui::TextEdit::multiline(v).code_editor().desired_rows(if p.k == "spice" { 3 } else { 5 });
                    if p.k == "spice" {
                        let kind = p.preset_kind.clone().unwrap_or_default();
                        let hint = if model_ideal {
                            "（理想模型不使用 SPICE 參數）".to_string()
                        } else {
                            preset(&kind, model).map(|s| s.replace(' ', "\n")).unwrap_or_else(|| "例如：\n.model D1 D(IS=2.52n N=1.752 RS=0.568 CJO=4p TT=20n BV=100)".into())
                        };
                        te = te.hint_text(hint);
                    }
                    wide(ui, 380.0, te);
                }
                _ => {
                    let mut te = egui::TextEdit::singleline(v);
                    if err.is_some() {
                        te = te.text_color(Color32::from_rgb(0xd9, 0x36, 0x3e));
                    }
                    self.field_w = wide(ui, FIELD_W, te).rect.width();
                }
            }
        });
        ui.label(&p.u);
        let show = self.disp.entry(p.k.clone()).or_insert(false);
        if t != "code" {
            ui.checkbox(show, "").on_hover_text("顯示在電路圖上");
        } else {
            ui.label("");
        }
        ui.end_row();
        if let Some(e) = err {
            ui.label("");
            ui.colored_label(Color32::from_rgb(0xd9, 0x36, 0x3e), e);
            ui.end_row();
        }
    }
    pub fn ui(&mut self, ctx: &egui::Context, ed: &mut Editor) -> DlgResult {
        let el = match ed.el(self.id) {
            Some(e) => e.clone(),
            None => return DlgResult::Close,
        };
        let d = lib().get(&el.typ).unwrap();
        let mut res = DlgResult::Keep;
        let mut open = true;
        let title = format!("{}{} 屬性", d.name, if el.name.is_empty() { String::new() } else { format!(" — {}", el.name) });
        egui::Window::new(title).id(egui::Id::new(("props", self.id))).open(&mut open).collapsible(false).resizable(true).default_width(if self.mc.is_some() { 660.0 } else { 560.0 }).show(ctx, |ui| {
            let model = self.vals.get("model").cloned().unwrap_or_default();
            let model_ideal = model == "ideal" || model == "3state" || d.param("model").is_none();
            egui::ScrollArea::vertical().max_height(ctx.content_rect().height() * 0.7).show(ui, |ui| {
                egui::Grid::new("pgrid").num_columns(4).spacing([10.0, 6.0]).striped(true).show(ui, |ui| {
                    ui.strong("參數");
                    ui.strong("數值");
                    ui.strong("單位");
                    ui.strong("顯示");
                    ui.end_row();
                    if !d.noname() {
                        ui.label("名稱 Name").on_hover_text("波形與激勵源以此名稱識別");
                        wide(ui, FIELD_W, egui::TextEdit::singleline(&mut self.name));
                        ui.label("");
                        let show = self.disp.entry("name".into()).or_insert(true);
                        ui.checkbox(show, "");
                        ui.end_row();
                    }
                    let mc = self.mc.is_some();
                    for p in d.params.iter().filter(|p| !p.adv && !(mc && p.k == "vars")) {
                        self.param_row(ui, d, p, model_ideal);
                    }
                });
                if let Some(m) = self.mc.as_mut() {
                    ui.add_space(8.0);
                    m.ui(ui, &ed.doc, &self.syms, self.vals.entry("vars".to_string()).or_default());
                }
                if d.params.iter().any(|p| p.adv) {
                    egui::CollapsingHeader::new("進階參數").default_open(model == "ds").show(ui, |ui| {
                        egui::Grid::new("pgrid_adv").num_columns(4).spacing([10.0, 6.0]).striped(true).show(ui, |ui| {
                            for p in d.params.iter().filter(|p| p.adv) {
                                self.param_row(ui, d, p, model_ideal);
                            }
                        });
                    });
                }
                ui.add_space(6.0);
                egui::Frame::group(ui.style()).show(ui, |ui| {
                    ui.strong(format!("{}（{}）", d.name, d.zh));
                    ui.label(&d.desc);
                });
            });
            if !self.err.is_empty() {
                ui.colored_label(Color32::from_rgb(0xd9, 0x36, 0x3e), &self.err);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("確定").clicked() || ui.input(|i| i.key_pressed(egui::Key::Enter) && i.modifiers.command) {
                    res = DlgResult::Apply;
                }
                if ui.button("取消").clicked() {
                    res = DlgResult::Close;
                }
            });
        });
        // the window's ✕ keeps the edits (validated like 確定); only 取消 discards them
        if !open && !matches!(res, DlgResult::Close) {
            res = DlgResult::Apply;
        }
        if let DlgResult::Apply = res {
            let mut errs = vec![];
            let model = self.vals.get("model").cloned().unwrap_or_default();
            for p in &d.params {
                let t = p.typ.as_deref().unwrap_or("num");
                if t == "choice" || (t == "code" && p.k != "spice") {
                    continue;
                }
                if (p.ideal_only && !(model == "ideal" || model == "3state")) || (p.k == "spice" && (model == "ideal" || model == "3state")) {
                    continue;
                }
                if let Some(e) = validate(p, self.vals.get(&p.k).map(|s| s.as_str()).unwrap_or(""), &self.syms, &d.typ) {
                    errs.push(format!("{}：{}", p.l.split(' ').next().unwrap_or(""), e));
                }
            }
            if self.mc.is_some() {
                let text = self.vals.get("vars").cloned().unwrap_or_default();
                for (i, c) in daedalus_core::montecarlo::check_lines(&ed.doc, &self.syms, &text).into_iter().enumerate() {
                    if let Err(e) = c.var {
                        errs.push(format!("容差第 {} 項：{}", i + 1, e));
                    }
                }
            }
            if model == "custom" && self.vals.get("spice").map(|s| s.trim().is_empty()).unwrap_or(true) && d.param("spice").is_some() {
                errs.push("自訂模型需填寫 SPICE 參數".into());
            }
            if !d.noname() {
                let nm = self.name.trim();
                if nm.is_empty() {
                    errs.push("名稱不可空白".into());
                } else if ed.doc.elements.iter().any(|o| o.id != self.id && o.name == nm && !lib().get(&o.typ).map(|q| q.noname()).unwrap_or(true)) {
                    errs.push(format!("名稱「{}」已被其他元件使用", nm));
                }
            }
            if !errs.is_empty() {
                self.err = errs.join("；");
                return DlgResult::Keep;
            }
            let name = if d.noname() { None } else { Some(self.name.trim().to_string()) };
            // nothing edited: close without an undo step or a "modified" mark
            let same_vals = self.vals.keys().chain(el.params.keys()).all(|k| self.vals.get(k).map(|s| s.as_str()).unwrap_or("") == el.params.get(k).map(|s| s.as_str()).unwrap_or(""));
            let same_name = name.as_deref().map(|n| n == el.name).unwrap_or(true);
            let ed_disp = el.disp.clone().unwrap_or_default();
            let same_disp = self.disp.iter().all(|(k, v)| ed_disp.get(k).copied().unwrap_or(*v) == *v);
            if !(same_vals && same_name && same_disp) {
                ed.set_params(self.id, name, self.vals.clone(), Some(self.disp.clone()));
            }
            return DlgResult::Close;
        }
        res
    }
}

/* ------------------------------------------------------------------ simulation control */
pub struct SimCtrlDlg {
    pub id: i64,
    pub vals: BTreeMap<String, String>,
    pub err: String,
}
impl SimCtrlDlg {
    pub fn new(ed: &mut Editor) -> Self {
        let id = match ed.doc.elements.iter().find(|e| e.typ == "SIMCTRL") {
            Some(e) => e.id,
            None => {
                let (x, y) = match ed.bounds() {
                    Some(b) => (b.x1.round() + 4.0, b.y0.round() + 1.0),
                    None => (0.0, 0.0),
                };
                let proto = ed.new_element("SIMCTRL", x, y, 0, false);
                ed.doc.nid -= 1;
                ed.place_element(&proto, x, y).unwrap_or(0)
            }
        };
        let vals = ed.el(id).map(|e| e.params.clone()).unwrap_or_default();
        SimCtrlDlg { id, vals, err: String::new() }
    }
    pub fn ui(&mut self, ctx: &egui::Context, ed: &mut Editor) -> bool {
        let syms = doc_syms(&ed.doc);
        let mut keep = true;
        let mut open = true;
        let mut apply = false;
        egui::Window::new("Simulation Control 模擬控制").open(&mut open).collapsible(false).default_width(480.0).show(ctx, |ui| {
            egui::Grid::new("sc").num_columns(3).spacing([10.0, 6.0]).show(ui, |ui| {
                for (k, l, u) in [("dt", "Time step 時間步長", "s"), ("tt", "Total time 總時間", "s"), ("tp", "Print time 輸出起始時間", "s"), ("ps", "Print step 輸出間隔", "")] {
                    ui.label(l);
                    let v = self.vals.entry(k.into()).or_default();
                    let ok = expr::eval_const(v, &syms).is_ok();
                    let mut te = egui::TextEdit::singleline(v);
                    if !ok {
                        te = te.text_color(Color32::from_rgb(0xd9, 0x36, 0x3e));
                    }
                    wide(ui, FIELD_W, te);
                    ui.label(u);
                    ui.end_row();
                }
            });
            egui::CollapsingHeader::new("精度選項").default_open(true).show(ui, |ui| {
                egui::Grid::new("sc2").num_columns(2).spacing([10.0, 6.0]).show(ui, |ui| {
                    let d = lib().get("SIMCTRL").unwrap();
                    for (k, l) in [("interp", "切換瞬間內插 Switching interpolation"), ("damp", "切換後阻尼 Damping after switching"), ("lte", "截斷誤差自動細分 LTE step refinement")] {
                        ui.label(l);
                        let v = self.vals.entry(k.into()).or_insert_with(|| "1".into());
                        let opts = d.param(k).and_then(|p| p.opts.clone()).unwrap_or_else(|| vec![("1".into(), "1：開啟".into()), ("0".into(), "0：關閉".into())]);
                        let cur = opts.iter().find(|o| o.0 == *v).map(|o| o.1.clone()).unwrap_or_else(|| v.clone());
                        egui::ComboBox::from_id_salt(("sc", k)).selected_text(cur).width(260.0).show_ui(ui, |ui| {
                            for (a, b) in &opts {
                                ui.selectable_value(v, a.clone(), b);
                            }
                        });
                        ui.end_row();
                    }
                    ui.label("相對誤差容許值 LTE tolerance");
                    let v = self.vals.entry("ltetol".into()).or_insert_with(|| "1e-3".into());
                    ui.add(egui::TextEdit::singleline(v).desired_width(160.0));
                    ui.end_row();
                    ui.label("高精度模式 High-accuracy mode");
                    let v = self.vals.entry("acc".into()).or_insert_with(|| "0".into());
                    let on = v.trim() == "1";
                    egui::ComboBox::from_id_salt(("sc", "acc")).selected_text(if on { "1：高精度（依目標誤差自動選步長）" } else { "0：標準（固定步長）" }).width(260.0).show_ui(ui, |ui| {
                        ui.selectable_value(v, "0".to_string(), "0：標準（固定步長）");
                        ui.selectable_value(v, "1".to_string(), "1：高精度（依目標誤差自動選步長）");
                    });
                    ui.end_row();
                    if on {
                        ui.label("目標誤差 Target error");
                        let v = self.vals.entry("etol".into()).or_insert_with(|| "1e-6".into());
                        ui.add(egui::TextEdit::singleline(v).desired_width(160.0));
                        ui.end_row();
                        ui.label("最大細分層數 Max. refinement");
                        let v = self.vals.entry("maxlv".into()).or_insert_with(|| "8".into());
                        ui.add(egui::TextEdit::singleline(v).desired_width(160.0));
                        ui.end_row();
                    }
                });
                if self.vals.get("acc").map(|s| s.trim() == "1").unwrap_or(false) {
                    ui.weak("高精度模式：以 dt 與 dt/2 平行模擬估計誤差（相對各探棒滿刻度），未達目標就自動縮小步長並以 Richardson 外插合併。線性、理想開關與平滑非線性電路通常 2～8 倍時間可達 1e-6；含 ns 級切換邊緣的電路受數值精度限制，結果會註明實際達到的誤差。");
                }
            });
            let g = |k: &str| expr::eval_const(self.vals.get(k).map(|s| s.as_str()).unwrap_or(""), &syms);
            match (g("dt"), g("tt"), g("tp"), g("ps")) {
                (Ok(dt), Ok(tt), Ok(tp), Ok(ps)) => {
                    let n = tt / dt;
                    let nr = ((tt - tp) / dt / ps.round().max(1.0)).floor() + 1.0;
                    ui.label(format!("時間步數：{:.3e}　輸出點數：{:.3e}（每個探棒約 {}）", n, nr, crate::plot::si(nr * 8.0, "B", 3)));
                    if n > 5e7 {
                        ui.colored_label(Color32::from_rgb(0xc7, 0x6b, 0x00), "步數很多，模擬會需要較長時間，可隨時在進度視窗中止。");
                    }
                    if dt < 1e-15 * 0.999 {
                        ui.colored_label(Color32::from_rgb(0xd9, 0x36, 0x3e), "時間步長小於 1 fs。");
                    }
                }
                (a, b, c, d) => {
                    for (k, r) in [("dt", a), ("tt", b), ("tp", c), ("ps", d)] {
                        if let Err(e) = r {
                            ui.colored_label(Color32::from_rgb(0xd9, 0x36, 0x3e), format!("{}：{}", k, e.0));
                        }
                    }
                }
            }
            ui.add_space(4.0);
            ui.weak("Time step 為固定模擬步長，最小可設 1 fs（10 GHz 等級訊號建議 ≤ 1 ps）；Print step = N 表示每 N 步儲存一點。開啟「切換瞬間內插」時，二極體、比較器驅動的開關等切換時刻會在步長內精確定位，即使步長較大也能得到正確的占空比與平均值。");
            if !self.err.is_empty() {
                ui.colored_label(Color32::from_rgb(0xd9, 0x36, 0x3e), &self.err);
            }
            ui.separator();
            ui.horizontal(|ui| {
                if ui.button("確定").clicked() {
                    apply = true;
                }
                if ui.button("取消").clicked() {
                    keep = false;
                }
            });
        });
        // the window's ✕ keeps the edits (validated like 確定); only 取消 discards them
        if !open && keep {
            apply = true;
        }
        if apply {
            let g = |k: &str| expr::eval_const(self.vals.get(k).map(|s| s.as_str()).unwrap_or(""), &syms);
            {
                {
                    let mut bad = vec![];
                    for k in ["dt", "tt", "tp", "ps"] {
                        match g(k) {
                            Ok(x) => {
                                if (k == "dt" || k == "tt") && !(x > 0.0) {
                                    bad.push(format!("{}：必須大於 0", k));
                                }
                                if k == "tp" && x < 0.0 {
                                    bad.push(format!("{}：不可為負", k));
                                }
                                if k == "ps" && !(x >= 1.0) {
                                    bad.push(format!("{}：必須 ≥ 1", k));
                                }
                            }
                            Err(e) => bad.push(format!("{}：{}", k, e.0)),
                        }
                    }
                    if bad.is_empty() {
                        let cur = ed.el(self.id).map(|e| e.params.clone()).unwrap_or_default();
                        let same = self.vals.keys().chain(cur.keys()).all(|k| self.vals.get(k).map(|s| s.as_str()).unwrap_or("") == cur.get(k).map(|s| s.as_str()).unwrap_or(""));
                        if !same {
                            ed.set_params(self.id, None, self.vals.clone(), None);
                        }
                        keep = false;
                    } else {
                        // invalid values: stay open and show why
                        self.err = bad.join("；");
                        return true;
                    }
                }
            }
        }
        keep && open
    }
}

/* ------------------------------------------------------------------ quick add */
pub struct QuickAdd {
    pub pos: egui::Pos2,
    pub at: (f64, f64),
    pub q: String,
    pub idx: usize,
    focus: bool,
}
pub fn search_lib(q: &str) -> Vec<&'static ElDef> {
    let q = q.trim().to_lowercase();
    let mut v: Vec<(i32, &ElDef)> = lib()
        .defs
        .iter()
        .filter_map(|d| {
            if q.is_empty() {
                return Some((0, d));
            }
            let t = d.typ.to_lowercase();
            let n = d.name.to_lowercase();
            let score = if t == q {
                100
            } else if t.starts_with(&q) {
                80
            } else if n.starts_with(&q) {
                70
            } else if n.contains(&q) || d.zh.contains(&q) {
                50
            } else if t.contains(&q) {
                40
            } else if d.desc.to_lowercase().contains(&q) || d.cat.to_lowercase().contains(&q) {
                10
            } else {
                return None;
            };
            Some((score, d))
        })
        .collect();
    v.sort_by(|a, b| b.0.cmp(&a.0));
    v.into_iter().map(|x| x.1).collect()
}
pub enum QaResult {
    Keep,
    Close,
    Place(String, (f64, f64)),
}
impl QuickAdd {
    pub fn new(pos: egui::Pos2, at: (f64, f64)) -> Self {
        QuickAdd { pos, at, q: String::new(), idx: 0, focus: true }
    }
    pub fn ui(&mut self, ctx: &egui::Context) -> QaResult {
        let mut r = QaResult::Keep;
        egui::Area::new(egui::Id::new("quickadd")).fixed_pos(self.pos).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.set_width(360.0);
                ui.weak("快速新增：輸入元件名稱（英文或中文），Enter 放置，Esc 取消");
                let te = ui.add(egui::TextEdit::singleline(&mut self.q).hint_text("例如 mosfet、比較器、PWM").desired_width(340.0));
                if self.focus {
                    te.request_focus();
                    self.focus = false;
                }
                if te.changed() {
                    self.idx = 0;
                }
                let list = search_lib(&self.q);
                let (up, down, enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::ArrowUp), i.key_pressed(egui::Key::ArrowDown), i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                if down {
                    self.idx = (self.idx + 1).min(list.len().saturating_sub(1));
                }
                if up {
                    self.idx = self.idx.saturating_sub(1);
                }
                egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                    for (i, d) in list.iter().take(60).enumerate() {
                        let txt = RichText::new(format!("{}  {}（{}）", d.typ, d.name, d.zh));
                        let rr = ui.selectable_label(i == self.idx, txt);
                        if i == self.idx {
                            rr.scroll_to_me(None);
                        }
                        if rr.clicked() {
                            r = QaResult::Place(d.typ.clone(), self.at);
                        }
                    }
                });
                if enter {
                    if let Some(d) = list.get(self.idx) {
                        r = QaResult::Place(d.typ.clone(), self.at);
                    }
                }
                if esc {
                    r = QaResult::Close;
                }
            });
        });
        r
    }
}

/* ------------------------------------------------------------------ inline edit of a name / parameter */
pub struct InlineEdit {
    pub id: i64,
    pub key: String,
    pub pos: egui::Pos2,
    pub text: String,
    pub err: Option<String>,
    focus: bool,
    /// width of the text field (GUI test)
    pub field_w: f32,
}
impl InlineEdit {
    pub fn new(ed: &Editor, id: i64, key: String, pos: egui::Pos2) -> Option<Self> {
        let el = ed.el(id)?;
        let text = if key == "__name" { el.name.clone() } else { el.params.get(&key).cloned().unwrap_or_default() };
        Some(InlineEdit { id, key, pos, text, err: None, focus: true, field_w: 0.0 })
    }
    /// returns false when finished
    pub fn ui(&mut self, ctx: &egui::Context, ed: &mut Editor) -> bool {
        let mut keep = true;
        egui::Area::new(egui::Id::new("inline")).fixed_pos(self.pos).order(egui::Order::Foreground).show(ctx, |ui| {
            egui::Frame::popup(ui.style()).show(ui, |ui| {
                ui.horizontal(|ui| {
                    ui.label(if self.key == "__name" { "名稱".to_string() } else { self.key.clone() });
                    let te = ui.add(egui::TextEdit::singleline(&mut self.text).desired_width(400.0));
                    self.field_w = te.rect.width();
                    if self.focus {
                        te.request_focus();
                        self.focus = false;
                    }
                    let (enter, esc) = ui.input(|i| (i.key_pressed(egui::Key::Enter), i.key_pressed(egui::Key::Escape)));
                    if esc {
                        keep = false;
                    }
                    if enter || ui.button("✔").clicked() {
                        let el = ed.el(self.id).cloned();
                        if let Some(el) = el {
                            let d = lib().get(&el.typ).unwrap();
                            if self.key == "__name" {
                                let nm = self.text.trim().to_string();
                                if nm.is_empty() {
                                    self.err = Some("名稱不可空白".into());
                                } else if ed.doc.elements.iter().any(|o| o.id != el.id && o.name == nm && !lib().get(&o.typ).map(|q| q.noname()).unwrap_or(true)) {
                                    self.err = Some(format!("名稱「{}」已被其他元件使用", nm));
                                } else {
                                    ed.set_params(el.id, Some(nm), el.params.clone(), None);
                                    keep = false;
                                }
                            } else if let Some(p) = d.param(&self.key) {
                                match validate(p, &self.text, &doc_syms(&ed.doc), &d.typ) {
                                    Some(e) => self.err = Some(e),
                                    None => {
                                        let mut pr = el.params.clone();
                                        pr.insert(self.key.clone(), self.text.clone());
                                        ed.set_params(el.id, None, pr, None);
                                        keep = false;
                                    }
                                }
                            } else {
                                keep = false;
                            }
                        } else {
                            keep = false;
                        }
                    }
                });
                if let Some(e) = &self.err {
                    ui.colored_label(Color32::from_rgb(0xd9, 0x36, 0x3e), e);
                }
            });
        });
        keep
    }
}

/* ------------------------------------------------------------------ help / about */
pub fn help_ui(ui: &mut Ui) {
    let rows = [
        ("/（或在空白處雙擊，需在「檔案 → 設定」開啟）", "快速新增：輸入元件名稱（英文或中文），Enter 放在該處"),
        ("點選元件庫項目", "灰色元件跟著游標移動，左鍵放置（按住 Shift 可連續放置）"),
        ("從接腳拖曳", "拉出導線到另一個接腳或導線（自動 L 形繞線）"),
        ("W", "導線模式：逐點畫線，點到接腳或導線即結束，右鍵/雙擊結束"),
        ("選取元件後再點一次名稱 / 參數文字", "就地修改（Enter 確定、Esc 取消）；第一次點文字只會選取元件"),
        ("雙擊元件 / Enter", "開啟完整屬性"),
        ("R / C / L / D / P", "放置電阻 / 電容 / 電感 / 二極體 / 電壓探棒（對地）"),
        ("G / N", "放置接地 / 節點標籤"),
        ("空白鍵 / Ctrl+R", "旋轉（Shift+空白鍵、Ctrl+Shift+R 反向）"),
        ("X / Y", "左右 / 上下鏡像"),
        ("Ctrl+B", "停用 / 啟用元件（不參與模擬）"),
        ("H", "手形工具：拖曳移動視圖"),
        ("Delete", "刪除"),
        ("Ctrl+C / X / V / D", "複製 / 剪下 / 貼上 / 原地複製"),
        ("Ctrl+Z / Ctrl+Y", "復原 / 重做"),
        ("Ctrl+A", "全選"),
        ("方向鍵", "微調位置（導線自動跟隨）"),
        ("滾輪 / F / + / −", "縮放 / 全部顯示 / 放大 / 縮小"),
        ("右鍵或中鍵拖曳 / 空白鍵＋拖曳 / H", "平移（右鍵點一下不拖曳＝右鍵選單）"),
        ("Ctrl+F", "尋找元件"),
        ("F5 / F6 / F7", "執行模擬 / 開啟上次結果 / 模擬控制"),
        ("Ctrl+N / O / S", "新增 / 開啟 / 儲存"),
    ];
    egui::Grid::new("keys").striped(true).num_columns(2).show(ui, |ui| {
        for (k, v) in rows {
            ui.monospace(k);
            ui.label(v);
            ui.end_row();
        }
    });
    ui.add_space(6.0);
    ui.label("接腳類型標記：■ 功率、▶ 控制輸入（指向元件）、▶ 控制輸出（指向外）、◇ 功率或控制皆可。功率電路與控制電路必須透過感測器、On-Off Controller 或 Control-Power Interface 連接。");
    ui.label("波形視窗：開啟時為一個空白畫面，從左側清單勾選訊號；拖曳框選放大時間軸、滾輪縮放、Shift+滾輪縮放 Y 軸、雙擊還原、中鍵拖曳平移；開啟「游標」後左鍵設 C1、右鍵設 C2。");
    ui.label("點左側訊號旁的色塊可更改波形顏色（依訊號名稱記住）。「XY 縮放」工具：按住左鍵左右拖曳縮放時間軸、上下拖曳縮放 Y 軸。");
    ui.label("「匯出報告」把第一個畫面、量測值與 C1/C2 游標讀值存成 PDF 或 Excel；「設定」可調整欄位、波形畫面、X/Y 軸字體大小與波形線寬（也套用到報告）。");
    ui.label("「檔案 → 匯出畫面（PDF）」把整張電路圖存成向量 PDF；「檔案 → 設定」可調整介面與電路圖文字大小。");
    ui.label("蒙地卡羅分析：在 Monte Carlo 元件的屬性視窗按「＋ 加入元件參數或變數…」選擇要隨機變化的數值（例如 R1.R），填入容差與分佈。");
    ui.label("「波形運算」以探棒訊號寫運算式（如 Vo*Io、sqrt(Va^2+Vb^2)）建立新訊號；「FFT」分析游標 C1–C2 之間（或目前顯示範圍）的頻譜、諧波與 THD。");
}
