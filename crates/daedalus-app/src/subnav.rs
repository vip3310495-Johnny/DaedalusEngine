//! Subcircuits in the editor (PSIM style): the definitions live in the main circuit; entering a block (or a
//! definition) puts its definition in the editor, with the circuits above it kept on a stack, and returning
//! writes the definition back (one undo step in the main circuit) and updates the pins of every block.

use super::*;
use daedalus_core::library::ports_str;
use daedalus_core::subckt;
use std::collections::BTreeMap;

/// a circuit above the one in the editor
pub struct Level {
    pub ed: Editor,
    /// the definition it is (None: the main circuit)
    pub def: Option<String>,
    off: egui::Vec2,
    scale: f32,
}

pub enum SubDlg {
    /// choose a definition to place a block of
    Pick,
    /// name a new definition (then place a block of it)
    New { name: String, err: String },
    Rename { from: String, name: String, err: String },
    Manage,
}

/// a free name based on `base` (base, base_2, base_3, …)
fn free_name(defs: &BTreeMap<String, Doc>, base: &str) -> String {
    if !defs.contains_key(base) {
        return base.to_string();
    }
    (2..).map(|i| format!("{}_{}", base, i)).find(|n| !defs.contains_key(n)).unwrap()
}

/// a new definition: an input port on the left, an output port on the right and a hint
fn starter_def() -> Doc {
    let mut ed = Editor::new();
    // the pin is the port's pointed end: the input points right (outline to the left), the output points left
    let mut a = ed.new_element("SUBPORT", -8.0, 0.0, 2, false);
    a.params.insert("text".into(), "in".into());
    a.params.insert("side".into(), "L".into());
    let mut b = ed.new_element("SUBPORT", 8.0, 0.0, 0, false);
    b.params.insert("text".into(), "out".into());
    b.params.insert("side".into(), "R".into());
    let mut t = ed.new_element("TEXT", -8.0, -5.0, 0, false);
    t.params.insert("text".into(), "子電路：在這裡畫電路，用「子電路埠」做為方塊的接腳（埠的名稱與方塊邊可在屬性中修改）。\n放一個參數檔定義子電路參數（例如 R = 10），每個方塊可個別指定數值。完成後按「返回上層」。".into());
    ed.doc.elements.extend([a, b, t]);
    ed.doc
}

impl App {
    pub(super) fn in_sub(&self) -> bool {
        !self.stack.is_empty()
    }
    fn root_ed(&self) -> &Editor {
        self.stack.first().map(|l| &l.ed).unwrap_or(&self.ed)
    }
    fn root_ed_mut(&mut self) -> &mut Editor {
        match self.stack.first_mut() {
            Some(l) => &mut l.ed,
            None => &mut self.ed,
        }
    }
    /// the definitions (the main circuit's map; the ones being edited may have newer copies on the stack)
    pub(super) fn defs(&self) -> &BTreeMap<String, Doc> {
        &self.root_ed().doc.subckts
    }
    /// definitions being edited, outermost first
    fn chain(&self) -> Vec<String> {
        self.stack.iter().filter_map(|l| l.def.clone()).chain(self.cur_def.clone()).collect()
    }
    /// the whole circuit as it would be saved: the main circuit with every definition being edited written back
    pub(super) fn root_doc(&self) -> Doc {
        if self.stack.is_empty() {
            return self.ed.doc.clone();
        }
        let mut root = self.stack[0].ed.doc.clone();
        let levels = self.stack.iter().skip(1).map(|l| (l.def.clone(), &l.ed.doc)).chain(std::iter::once((self.cur_def.clone(), &self.ed.doc)));
        for (def, doc) in levels {
            if let Some(n) = def {
                let mut d = doc.clone();
                d.subckts.clear();
                root.subckts.insert(n, d);
            }
        }
        root.sync_subckt_ports();
        root
    }
    pub(super) fn any_dirty(&self) -> bool {
        self.ed.dirty || self.stack.iter().any(|l| l.ed.dirty)
    }
    pub(super) fn any_content(&self) -> bool {
        !self.root_ed().doc.elements.is_empty()
    }
    pub(super) fn clear_dirty(&mut self) {
        self.ed.dirty = false;
        for l in self.stack.iter_mut() {
            l.ed.dirty = false;
        }
    }
    /// "main › A › B" for the title and the status bar
    pub(super) fn sub_path(&self) -> String {
        self.chain().iter().map(|d| format!(" › {}", d)).collect()
    }
    /// parameter-file text of the main circuit (what a definition's values can use besides its own)
    fn root_params(&self) -> String {
        let mut t = String::new();
        for e in self.root_ed().doc.elements.iter().filter(|e| e.typ == "PARAMS" && !e.dis) {
            t += e.params.get("text").map(|s| s.as_str()).unwrap_or("");
            t.push('\n');
        }
        t
    }
    fn close_level_dialogs(&mut self) {
        self.props = None;
        self.simctrl = None;
        self.inline = None;
        self.quick = None;
        self.ctx_menu = None;
        self.find = None;
        self.canvas.find_hits.clear();
        self.canvas.err_els.clear();
        self.canvas.cancel();
    }

    /// open definition `name` in the editor
    pub(super) fn enter_def(&mut self, name: &str) {
        let Some(def) = self.defs().get(name).cloned() else {
            self.toast(format!("找不到子電路定義「{}」", name));
            return;
        };
        if self.chain().iter().any(|d| d == name) {
            self.toast(format!("「{}」已在編輯中（上層）", name));
            return;
        }
        self.close_level_dialogs();
        let mut ed = Editor::new();
        ed.doc = def;
        ed.scope_params = self.root_params();
        ed.clip = self.ed.clip.take();
        let parent = std::mem::replace(&mut self.ed, ed);
        self.stack.push(Level { ed: parent, def: self.cur_def.take(), off: self.canvas.off, scale: self.canvas.scale });
        self.cur_def = Some(name.to_string());
        self.canvas.want_fit = true;
        self.toast(format!("正在編輯子電路「{}」：完成後按「返回上層」（Ctrl+Shift+E）", name));
    }
    /// open the definition of block `id`
    pub(super) fn enter_block(&mut self, id: i64) {
        let def = self.ed.el(id).filter(|e| e.typ == "SUBCKT").and_then(|e| e.params.get("def").cloned());
        match def {
            Some(d) => self.enter_def(&d),
            None => self.toast("請先選取一個子電路方塊"),
        }
    }
    /// the selected block, if exactly one is selected
    pub(super) fn sel_block(&self) -> Option<i64> {
        let s = self.ed.selected_els();
        (s.len() == 1 && self.ed.el(s[0]).map(|e| e.typ == "SUBCKT").unwrap_or(false)).then(|| s[0])
    }
    /// back to the circuit above, writing the definition back when it changed
    pub(super) fn leave(&mut self) {
        let Some(up) = self.stack.pop() else { return };
        self.close_level_dialogs();
        let mut child = std::mem::replace(&mut self.ed, up.ed);
        let name = std::mem::replace(&mut self.cur_def, up.def).unwrap_or_default();
        self.canvas.off = up.off;
        self.canvas.scale = up.scale;
        self.ed.clip = child.clip.take();
        self.toast(format!("已返回{}", self.cur_def.as_ref().map(|d| format!("子電路「{}」", d)).unwrap_or_else(|| "主電路".into())));
        child.doc.subckts.clear();
        let changed = self.defs().get(&name) != Some(&child.doc);
        if changed {
            let ports = ports_str(&subckt::def_ports(&child.doc));
            let r = self.root_ed_mut();
            // a definition saved while it was open is in the file already: no "modified" mark for it
            let was = r.dirty;
            r.push_undo();
            r.doc.subckts.insert(name.clone(), child.doc);
            r.doc.sync_subckt_ports();
            r.changed();
            r.dirty = was || child.dirty;
            // the definitions still open above may have blocks of this one
            if self.in_sub() {
                let mut eds: Vec<&mut Editor> = self.stack.iter_mut().skip(1).map(|l| &mut l.ed).collect();
                eds.push(&mut self.ed);
                for ed in eds {
                    let mut n = 0;
                    for i in 0..ed.doc.elements.len() {
                        let e = &ed.doc.elements[i];
                        if e.typ == "SUBCKT" && e.params.get("def") == Some(&name) && e.params.get("ports") != Some(&ports) {
                            ed.doc.set_block_ports(i, &ports);
                            n += 1;
                        }
                    }
                    if n > 0 {
                        let d = ed.dirty || child.dirty;
                        ed.changed();
                        ed.dirty = d;
                    }
                }
            }
        }
    }
    pub(super) fn leave_all(&mut self) {
        while self.in_sub() {
            self.leave();
        }
    }

    /// a block of definition `def` may be placed here (not inside itself, directly or through other blocks)
    fn can_place(&self, def: &str) -> Result<(), String> {
        let chain = self.chain();
        if chain.is_empty() {
            return Ok(());
        }
        let root = self.root_doc();
        let Some(d) = root.subckts.get(def) else { return Err(format!("找不到子電路定義「{}」", def)) };
        let mut uses = subckt::used_defs(&root, &d.elements);
        uses.push(def.to_string());
        match uses.iter().find(|u| chain.contains(u)) {
            Some(u) => Err(format!("不能在子電路「{}」裡放「{}」：子電路不能包含自己", chain.last().unwrap(), if u == def { def.to_string() } else { format!("{}（它包含 {}）", def, u) })),
            None => Ok(()),
        }
    }
    /// start placing a block of definition `def` (it follows the mouse until clicked)
    pub(super) fn start_place_sub(&mut self, def: &str) {
        if let Err(m) = self.can_place(def) {
            self.toast(m);
            return;
        }
        let ports = self.root_doc().subckts.get(def).map(|d| ports_str(&subckt::def_ports(d))).unwrap_or_default();
        let mut el = self.ed.new_element("SUBCKT", 0.0, 0.0, 0, false);
        self.ed.doc.nid -= 1; // prototype only
        el.params.insert("def".into(), def.to_string());
        el.params.insert("ports".into(), ports);
        el.params.insert("vars".into(), String::new());
        self.canvas.mode = Mode::Place(Box::new(el));
        self.toast(format!("放置子電路「{}」：左鍵放下；雙擊方塊設定參數，右鍵「進入子電路」編輯內容", def));
    }
    /// library / quick add: a block needs a definition first
    pub(super) fn pick_sub(&mut self) {
        self.sub_dlg = Some(if self.defs().is_empty() { SubDlg::New { name: free_name(self.defs(), "SUB1"), err: String::new() } } else { SubDlg::Pick });
    }
    pub(super) fn script_sub_new(&mut self, name: &str) -> Result<(), String> {
        self.create_def(name)
    }
    fn create_def(&mut self, name: &str) -> Result<(), String> {
        let name = subckt::valid_def_name(name)?;
        if self.defs().contains_key(&name) {
            return Err(format!("已有名為「{}」的子電路", name));
        }
        let r = self.root_ed_mut();
        r.push_undo();
        r.doc.subckts.insert(name.clone(), starter_def());
        r.changed();
        self.start_place_sub(&name);
        Ok(())
    }
    /// a definition from a circuit file: its elements become the definition, its own definitions join this circuit's
    fn import_def(&mut self) {
        let Some(p) = rfd::FileDialog::new().add_filter("電路檔", &["psimx", "json"]).pick_file() else { return };
        let r = std::fs::read_to_string(&p).map_err(|e| e.to_string()).and_then(|t| Doc::from_json(&t).map(|d| d.0));
        let mut doc = match r {
            Ok(d) => d,
            Err(e) => {
                self.show_msgs("無法匯入子電路", vec![Msg { err: true, text: format!("無法讀取 {}：{}", p.display(), e), els: vec![] }]);
                return;
            }
        };
        if !doc.elements.iter().any(|e| e.typ == "SUBPORT" && !e.dis) {
            self.toast("提醒：這個電路沒有「子電路埠」，方塊不會有接腳；可進入子電路加上埠");
        }
        let stem = p.file_stem().map(|s| s.to_string_lossy().to_string()).unwrap_or_else(|| "SUB".into());
        let name = free_name(self.defs(), subckt::valid_def_name(&stem).as_deref().unwrap_or("SUB"));
        let nested = std::mem::take(&mut doc.subckts);
        let mut skipped = vec![];
        let r = self.root_ed_mut();
        r.push_undo();
        for (k, d) in nested {
            match r.doc.subckts.get(&k) {
                None => {
                    r.doc.subckts.insert(k, d);
                }
                Some(o) if *o == d => {}
                Some(_) => skipped.push(k),
            }
        }
        r.doc.subckts.insert(name.clone(), doc);
        r.doc.sync_subckt_ports();
        r.changed();
        if !skipped.is_empty() {
            self.show_msgs("匯入子電路", vec![Msg { err: false, text: format!("檔案中的子電路 {} 與本電路同名但內容不同，沿用本電路的版本。", skipped.join("、")), els: vec![] }]);
        }
        self.start_place_sub(&name);
    }
    /// the definition being edited as a circuit file of its own (with the definitions it uses)
    fn export_def(&mut self) {
        let Some(name) = self.cur_def.clone() else { return };
        let Some(p) = rfd::FileDialog::new().add_filter("電路檔", &["psimx"]).set_file_name(format!("{}.psimx", name)).save_file() else { return };
        let root = self.root_doc();
        let mut doc = self.ed.doc.clone();
        for u in subckt::used_defs(&root, &doc.elements) {
            if let Some(d) = root.subckts.get(&u) {
                doc.subckts.insert(u, d.clone());
            }
        }
        match std::fs::write(&p, doc.to_json(daedalus_core::VERSION, None)) {
            Ok(()) => self.toast(format!("已匯出子電路「{}」：{}", name, p.display())),
            Err(e) => self.show_msgs("無法匯出", vec![Msg { err: true, text: format!("無法寫入檔案：{}", e), els: vec![] }]),
        }
    }
    /// copy: the definitions of copied blocks go along, so they can be pasted into another circuit
    pub(super) fn clip_defs(&mut self) {
        let Some(c) = self.ed.clip.as_ref() else { return };
        let root = self.root_doc();
        let defs: BTreeMap<String, Doc> = subckt::used_defs(&root, &c.elements).into_iter().filter_map(|n| root.subckts.get(&n).map(|d| (n, d.clone()))).collect();
        if let Some(c) = self.ed.clip.as_mut() {
            c.defs = defs;
        }
    }
    /// paste: definitions this circuit does not have yet are added
    pub(super) fn paste_defs(&mut self) {
        let Some(defs) = self.ed.clip.as_ref().map(|c| c.defs.clone()) else { return };
        let missing: Vec<(String, Doc)> = defs.into_iter().filter(|(k, _)| !self.defs().contains_key(k)).collect();
        if missing.is_empty() {
            return;
        }
        let names: Vec<String> = missing.iter().map(|m| m.0.clone()).collect();
        let r = self.root_ed_mut();
        for (k, d) in missing {
            r.doc.subckts.insert(k, d);
        }
        r.changed();
        self.toast(format!("已加入子電路定義：{}", names.join("、")));
    }
    /// netlist error elements as ids of this level: the block in the main circuit, or the element inside the
    /// definition being edited
    pub(super) fn level_ids(&self, top: &[i64], inner: &[(String, i64)]) -> Vec<i64> {
        match &self.cur_def {
            None => top.to_vec(),
            Some(d) => inner.iter().filter(|(def, _)| def == d).map(|(_, id)| *id).collect(),
        }
    }

    /* ---------------------------------------------------------------- user interface */
    pub(super) fn sub_menu(&mut self, ui: &mut Ui) {
        if ui.button("新增子電路…").on_hover_text("建立一個新的子電路定義並放置一個方塊").clicked() {
            self.sub_dlg = Some(SubDlg::New { name: free_name(self.defs(), "SUB1"), err: String::new() });
            ui.close();
        }
        let names: Vec<String> = self.defs().keys().cloned().collect();
        ui.add_enabled_ui(!names.is_empty(), |ui| {
            ui.menu_button("放置子電路", |ui| {
                for n in &names {
                    if ui.button(n).clicked() {
                        self.start_place_sub(n);
                        ui.close();
                    }
                }
            });
        });
        if ui.button("從檔案匯入子電路…").on_hover_text("把一個電路檔（.psimx）當作子電路定義加入本電路，並放置一個方塊").clicked() {
            self.import_def();
            ui.close();
        }
        ui.separator();
        let blk = self.sel_block();
        if ui.add_enabled(blk.is_some(), egui::Button::new("進入子電路　Ctrl+E")).on_hover_text("編輯選取方塊的子電路定義（所有用這個定義的方塊都會改變）").clicked() {
            if let Some(id) = blk {
                self.enter_block(id);
            }
            ui.close();
        }
        if ui.add_enabled(self.in_sub(), egui::Button::new("返回上層　Ctrl+Shift+E")).clicked() {
            self.leave();
            ui.close();
        }
        if ui.add_enabled(self.cur_def.is_some(), egui::Button::new("匯出子電路定義…")).on_hover_text("把正在編輯的子電路存成獨立的電路檔，可在其他電路「從檔案匯入子電路」").clicked() {
            self.export_def();
            ui.close();
        }
        ui.separator();
        if ui.button("管理子電路…").clicked() {
            self.sub_dlg = Some(SubDlg::Manage);
            ui.close();
        }
    }

    /// "主電路 › A › B   ⬑ 返回上層" above the schematic while a definition is being edited
    pub(super) fn breadcrumb(&mut self, ui: &mut Ui, pal: &Pal) {
        let chain = self.chain();
        let mut go: Option<usize> = None;
        ui.horizontal(|ui| {
            if ui.button("⬑ 返回上層").on_hover_text("Ctrl+Shift+E").clicked() {
                go = Some(chain.len() - 1);
            }
            ui.separator();
            if ui.link("主電路").clicked() {
                go = Some(0);
            }
            for (i, d) in chain.iter().enumerate() {
                ui.label("›");
                if i + 1 == chain.len() {
                    ui.label(RichText::new(format!("子電路 {}", d)).strong().color(pal.accent));
                } else if ui.link(d).clicked() {
                    go = Some(i + 1);
                }
            }
            ui.separator();
            ui.weak("修改會套用到所有使用此子電路的方塊");
        });
        if let Some(level) = go {
            while self.stack.len() > level {
                self.leave();
            }
        }
    }

    pub(super) fn sub_dialogs(&mut self, ctx: &egui::Context) {
        let Some(dlg) = self.sub_dlg.take() else { return };
        let mut keep = true;
        let next = match dlg {
            SubDlg::Pick => {
                let mut out = None;
                egui::Window::new("放置子電路").collapsible(false).resizable(false).open(&mut keep).show(ctx, |ui| {
                    ui.label("選擇要放置的子電路：");
                    let names: Vec<(String, usize)> = self.defs().iter().map(|(k, d)| (k.clone(), subckt::def_ports(d).len())).collect();
                    egui::ScrollArea::vertical().max_height(300.0).show(ui, |ui| {
                        for (n, np) in &names {
                            if ui.button(format!("{}　（{} 隻接腳）", n, np)).clicked() {
                                out = Some(Some(n.clone()));
                            }
                        }
                    });
                    ui.separator();
                    ui.horizontal(|ui| {
                        if ui.button("新增子電路…").clicked() {
                            out = Some(None);
                        }
                        if ui.button("從檔案匯入…").clicked() {
                            out = Some(Some(String::new()));
                        }
                    });
                });
                match out {
                    Some(Some(n)) if n.is_empty() => {
                        self.import_def();
                        None
                    }
                    Some(Some(n)) => {
                        self.start_place_sub(&n);
                        None
                    }
                    Some(None) => Some(SubDlg::New { name: free_name(self.defs(), "SUB1"), err: String::new() }),
                    None => Some(SubDlg::Pick),
                }
            }
            SubDlg::New { mut name, err } => {
                let mut ok = false;
                egui::Window::new("新增子電路").collapsible(false).resizable(false).open(&mut keep).show(ctx, |ui| {
                    ui.label("子電路名稱（顯示在方塊上）：");
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
                    if !r.has_focus() && err.is_empty() {
                        r.request_focus();
                    }
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        ok = true;
                    }
                    ui.weak("建立後先放置一個方塊；右鍵方塊「進入子電路」畫內容。");
                    if !err.is_empty() {
                        ui.colored_label(self.pal().err, &err);
                    }
                    ui.horizontal(|ui| {
                        if ui.button("建立").clicked() {
                            ok = true;
                        }
                    });
                });
                if ok {
                    match self.create_def(&name) {
                        Ok(()) => None,
                        Err(e) => Some(SubDlg::New { name, err: e }),
                    }
                } else {
                    Some(SubDlg::New { name, err })
                }
            }
            SubDlg::Rename { from, mut name, err } => {
                let mut ok = false;
                egui::Window::new("重新命名子電路").collapsible(false).resizable(false).open(&mut keep).show(ctx, |ui| {
                    ui.label(format!("「{}」的新名稱：", from));
                    let r = ui.add(egui::TextEdit::singleline(&mut name).desired_width(240.0));
                    if r.lost_focus() && ui.input(|i| i.key_pressed(Key::Enter)) {
                        ok = true;
                    }
                    if !err.is_empty() {
                        ui.colored_label(self.pal().err, &err);
                    }
                    if ui.button("確定").clicked() {
                        ok = true;
                    }
                });
                if ok {
                    match subckt::valid_def_name(&name) {
                        Err(e) => Some(SubDlg::Rename { from, name, err: e }),
                        Ok(n) if n == from => Some(SubDlg::Manage),
                        Ok(n) if self.defs().contains_key(&n) => Some(SubDlg::Rename { from, err: format!("已有名為「{}」的子電路", n), name }),
                        Ok(n) => {
                            let r = self.root_ed_mut();
                            r.push_undo();
                            subckt::rename_def(&mut r.doc, &from, &n);
                            r.changed();
                            // blocks in the definitions being edited follow the new name
                            if self.in_sub() {
                                let mut eds: Vec<&mut Editor> = self.stack.iter_mut().skip(1).map(|l| &mut l.ed).collect();
                                eds.push(&mut self.ed);
                                for ed in eds {
                                    let mut hit = false;
                                    for e in ed.doc.elements.iter_mut().filter(|e| e.typ == "SUBCKT" && e.params.get("def") == Some(&from)) {
                                        e.params.insert("def".into(), n.clone());
                                        hit = true;
                                    }
                                    if hit {
                                        ed.changed();
                                    }
                                }
                            }
                            Some(SubDlg::Manage)
                        }
                    }
                } else {
                    Some(SubDlg::Rename { from, name, err })
                }
            }
            SubDlg::Manage => {
                let mut act: Option<(u8, String)> = None;
                let chain = self.chain();
                let root = self.root_doc();
                egui::Window::new("管理子電路").collapsible(false).resizable(true).default_width(520.0).open(&mut keep).show(ctx, |ui| {
                    if root.subckts.is_empty() {
                        ui.weak("這個電路還沒有子電路。用「子電路 › 新增子電路…」建立，或「從檔案匯入子電路…」。");
                    }
                    egui::Grid::new("sub_mgr").num_columns(5).striped(true).spacing([12.0, 6.0]).show(ui, |ui| {
                        if !root.subckts.is_empty() {
                            ui.strong("名稱");
                            ui.strong("接腳");
                            ui.strong("元件");
                            ui.strong("使用中的方塊");
                            ui.label("");
                            ui.end_row();
                        }
                        for (n, d) in &root.subckts {
                            let busy = chain.contains(n);
                            let users = subckt::users(&root, n);
                            ui.label(n);
                            ui.label(subckt::def_ports(d).iter().map(|p| p.name.as_str()).collect::<Vec<_>>().join("、"));
                            ui.label(format!("{}", d.elements.iter().filter(|e| !matches!(e.typ.as_str(), "SUBPORT" | "TEXT" | "LABEL" | "GND")).count()));
                            ui.label(format!("{}", users));
                            ui.horizontal(|ui| {
                                if ui.add_enabled(!busy, egui::Button::new("編輯")).clicked() {
                                    act = Some((0, n.clone()));
                                }
                                if ui.add_enabled(!busy && self.can_place(n).is_ok(), egui::Button::new("放置")).clicked() {
                                    act = Some((1, n.clone()));
                                }
                                if ui.add_enabled(!busy, egui::Button::new("重新命名")).clicked() {
                                    act = Some((2, n.clone()));
                                }
                                if ui.add_enabled(!busy && users == 0, egui::Button::new("刪除")).on_hover_text("只能刪除沒有方塊使用的子電路（Ctrl+Z 可復原）").clicked() {
                                    act = Some((3, n.clone()));
                                }
                            });
                            ui.end_row();
                        }
                    });
                });
                match act {
                    Some((0, n)) => {
                        self.enter_def(&n);
                        None
                    }
                    Some((1, n)) => {
                        self.start_place_sub(&n);
                        None
                    }
                    Some((2, n)) => Some(SubDlg::Rename { name: n.clone(), from: n, err: String::new() }),
                    Some((_, n)) => {
                        let r = self.root_ed_mut();
                        r.push_undo();
                        r.doc.subckts.remove(&n);
                        r.changed();
                        self.toast(format!("已刪除子電路「{}」（Ctrl+Z 可復原）", n));
                        Some(SubDlg::Manage)
                    }
                    None => Some(SubDlg::Manage),
                }
            }
        };
        if keep {
            self.sub_dlg = next;
        }
    }
}

/// the element prototype type names that need a definition before they can be placed
pub fn needs_def(typ: &str) -> bool {
    typ == "SUBCKT"
}
