//! Setup / uninstall wizard window.

use crate::setup::{self, Options};
use egui::{pos2, vec2, Color32, Mesh, Rect, RichText, Shape};
use std::path::PathBuf;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

#[derive(Clone, Copy, PartialEq)]
pub enum Mode {
    Install,
    Uninstall,
}

/// what the install-mode wizard does; with an existing installation the start page offers a choice
#[derive(Clone, Copy, PartialEq)]
enum Action {
    Install,
    Update,
    Repair,
    Downgrade,
    Uninstall,
}

enum Page {
    Start,
    Busy,
    Done(Vec<String>),
    Failed(String),
}

type Outcome = Arc<Mutex<Option<Result<Vec<String>, String>>>>;

pub struct Wizard {
    mode: Mode,
    page: Page,
    dir: String,
    desktop: bool,
    assoc: bool,
    launch: bool,
    purge: bool,
    existing: Option<(PathBuf, String)>,
    action: Action,
    /// repair: problems found (and fixed) by the check before re-writing
    report: Arc<Mutex<Vec<String>>>,
    prog: Arc<Mutex<(f32, String)>>,
    outcome: Outcome,
    logo: Option<egui::TextureHandle>,
    running: bool,
    checked: Option<Instant>,
    close_after: bool,
    /// development aid: save a PNG of the first page, then exit
    pub shot: Option<String>,
    shot_frame: u32,
}

const SKY0: Color32 = Color32::from_rgb(0x2c, 0x43, 0xa8);
const SKY1: Color32 = Color32::from_rgb(0x0a, 0x12, 0x38);
const SUN: Color32 = Color32::from_rgb(0xff, 0xc8, 0x3d);
const ACCENT: Color32 = Color32::from_rgb(0x2c, 0x43, 0xa8);

impl Wizard {
    pub fn new(cc: &eframe::CreationContext, mode: Mode, dir: PathBuf, desktop: bool, assoc: bool) -> Self {
        let logo = image::load_from_memory(include_bytes!("../assets/icon_256.png")).ok().map(|img| {
            let rgba = img.to_rgba8();
            let ci = egui::ColorImage::from_rgba_unmultiplied([rgba.width() as usize, rgba.height() as usize], rgba.as_raw());
            cc.egui_ctx.load_texture("logo", ci, egui::TextureOptions::LINEAR)
        });
        let mut v = egui::Visuals::light();
        v.selection.bg_fill = ACCENT;
        v.hyperlink_color = ACCENT;
        cc.egui_ctx.set_theme(egui::Theme::Light);
        cc.egui_ctx.set_visuals_of(egui::Theme::Light, v);
        cc.egui_ctx.all_styles_mut(|s| {
            s.spacing.item_spacing = vec2(8.0, 8.0);
            s.spacing.button_padding = vec2(14.0, 6.0);
            s.text_styles.insert(egui::TextStyle::Body, egui::FontId::proportional(14.5));
            s.text_styles.insert(egui::TextStyle::Button, egui::FontId::proportional(14.5));
        });
        let existing = setup::existing_install();
        let action = match &existing {
            None => Action::Install,
            Some((_, v)) => match setup::compare_installed(v) {
                std::cmp::Ordering::Greater => Action::Update,
                std::cmp::Ordering::Equal => Action::Repair,
                std::cmp::Ordering::Less => Action::Downgrade,
            },
        };
        Wizard {
            mode,
            page: Page::Start,
            dir: dir.display().to_string(),
            desktop,
            assoc,
            launch: true,
            purge: false,
            existing,
            action,
            report: Arc::new(Mutex::new(vec![])),
            prog: Arc::new(Mutex::new((0.0, String::new()))),
            outcome: Arc::new(Mutex::new(None)),
            logo,
            running: false,
            checked: None,
            close_after: false,
            shot: None,
            shot_frame: 0,
        }
    }

    fn dir(&self) -> PathBuf {
        PathBuf::from(self.dir.trim().trim_matches('"'))
    }

    fn start(&mut self, ctx: &egui::Context) {
        let prog = self.prog.clone();
        let out = self.outcome.clone();
        let ctx2 = ctx.clone();
        if self.mode == Mode::Install && self.action == Action::Uninstall {
            self.mode = Mode::Uninstall;
        }
        let mode = self.mode;
        let repair = self.action == Action::Repair;
        let report = self.report.clone();
        let o = Options { dir: self.dir(), desktop: self.desktop, assoc: self.assoc };
        let purge = self.purge;
        *self.outcome.lock().unwrap() = None;
        self.page = Page::Busy;
        std::thread::spawn(move || {
            let rep = |f: f32, m: &str| {
                *prog.lock().unwrap() = (f, m.to_string());
                ctx2.request_repaint();
            };
            let r = match mode {
                Mode::Install => {
                    if repair {
                        rep(0.01, "檢查已安裝的檔案與設定…");
                        *report.lock().unwrap() = setup::diagnose(&o);
                    }
                    setup::install(&o, &rep)
                }
                Mode::Uninstall => setup::uninstall(&o.dir, purge, &rep),
            };
            *out.lock().unwrap() = Some(r);
            ctx2.request_repaint();
        });
    }

    fn sidebar(&self, ui: &mut egui::Ui, rect: Rect) {
        // vertical sky gradient
        let mut m = Mesh::default();
        m.colored_vertex(rect.left_top(), SKY0);
        m.colored_vertex(rect.right_top(), SKY0);
        m.colored_vertex(rect.right_bottom(), SKY1);
        m.colored_vertex(rect.left_bottom(), SKY1);
        m.add_triangle(0, 1, 2);
        m.add_triangle(0, 2, 3);
        let p = ui.painter();
        p.add(Shape::mesh(m));
        let cx = rect.center().x;
        if let Some(t) = &self.logo {
            let r = Rect::from_center_size(pos2(cx, rect.top() + 110.0), vec2(132.0, 132.0));
            p.image(t.id(), r, Rect::from_min_max(pos2(0.0, 0.0), pos2(1.0, 1.0)), Color32::WHITE);
        }
        p.text(pos2(cx, rect.top() + 200.0), egui::Align2::CENTER_CENTER, "DaedalusEngine", egui::FontId::proportional(21.0), Color32::WHITE);
        p.text(pos2(cx, rect.top() + 226.0), egui::Align2::CENTER_CENTER, setup::VERSION, egui::FontId::proportional(14.0), SUN);
        p.text(pos2(cx, rect.top() + 252.0), egui::Align2::CENTER_CENTER, "電力電子電路模擬器", egui::FontId::proportional(13.0), Color32::from_rgb(0xc8, 0xd2, 0xf5));
        p.text(pos2(cx, rect.bottom() - 18.0), egui::Align2::CENTER_CENTER, format!("© {}", setup::PUBLISHER), egui::FontId::proportional(11.5), Color32::from_rgb(0x9a, 0xa6, 0xd8));
    }

    fn primary(ui: &mut egui::Ui, text: &str, enabled: bool) -> bool {
        let b = egui::Button::new(RichText::new(text).color(Color32::WHITE).strong()).fill(if enabled { ACCENT } else { Color32::from_gray(170) }).min_size(vec2(96.0, 32.0));
        ui.add_enabled(enabled, b).clicked()
    }

    fn secondary(ui: &mut egui::Ui, text: &str) -> bool {
        ui.add(egui::Button::new(text).min_size(vec2(84.0, 32.0))).clicked()
    }

    fn note(ui: &mut egui::Ui, text: &str, warn: bool) {
        let (bg, fg) = if warn { (Color32::from_rgb(0xfd, 0xec, 0xc8), Color32::from_rgb(0x7a, 0x4a, 0x00)) } else { (Color32::from_rgb(0xe6, 0xeb, 0xfa), Color32::from_rgb(0x1d, 0x2b, 0x6b)) };
        egui::Frame::new().fill(bg).corner_radius(6.0).inner_margin(egui::Margin::symmetric(10, 8)).show(ui, |ui| {
            ui.set_width(ui.available_width());
            ui.label(RichText::new(text).color(fg));
        });
    }

    /// one choice of the maintenance page: radio button with a short explanation under it
    fn choice(ui: &mut egui::Ui, cur: &mut Action, a: Action, title: &str, desc: &str) {
        ui.radio_value(cur, a, RichText::new(title).strong());
        ui.horizontal(|ui| {
            ui.add_space(26.0);
            ui.add(egui::Label::new(RichText::new(desc).weak()).wrap());
        });
    }

    /// start page when DaedalusEngine is already installed: update / repair / downgrade, or uninstall
    fn page_maintain(&mut self, ui: &mut egui::Ui) -> Option<bool> {
        let (p, v) = self.existing.clone().unwrap_or_default();
        let old = setup::ver_label(&v);
        let mut go = None;
        ui.heading(RichText::new(format!("已安裝 DaedalusEngine {}", old)).strong());
        ui.label(RichText::new(format!("位置：{}", p.display())).weak());
        ui.label("請選擇要執行的動作：");
        let first = match self.action {
            Action::Uninstall => match setup::compare_installed(&v) {
                std::cmp::Ordering::Greater => Action::Update,
                std::cmp::Ordering::Equal => Action::Repair,
                std::cmp::Ordering::Less => Action::Downgrade,
            },
            a => a,
        };
        let mut a = self.action;
        match first {
            Action::Update => Self::choice(ui, &mut a, Action::Update, &format!("更新為 {}（建議）", setup::VERSION), &format!("以新版取代 {} 的程式檔案，並更新捷徑、檔案關聯與登錄資訊；電路檔與使用者設定保留。", old)),
            Action::Downgrade => Self::choice(ui, &mut a, Action::Downgrade, &format!("降級為 {}", setup::VERSION), &format!("已安裝的 {} 比此安裝程式新；降級後較新版本的功能將無法使用。電路檔與使用者設定保留。", old)),
            _ => Self::choice(ui, &mut a, Action::Repair, &format!("修復 {}", setup::VERSION), "檢查並重新寫入程式檔案、開始功能表／桌面捷徑、.psimx 檔案關聯與登錄資訊，並列出修復的項目；也可在此變更捷徑與檔案關聯設定。"),
        }
        Self::choice(ui, &mut a, Action::Uninstall, &format!("解除安裝 {}", old), "移除程式檔案、捷徑與 .psimx 檔案關聯；您的電路檔不會被刪除。");
        self.action = a;
        ui.add_space(2.0);
        if self.action == Action::Uninstall {
            ui.checkbox(&mut self.purge, "同時刪除使用者設定（最近開啟的檔案、視窗與介面偏好）");
        } else {
            ui.horizontal(|ui| {
                ui.checkbox(&mut self.desktop, "桌面捷徑");
                ui.checkbox(&mut self.assoc, "以 DaedalusEngine 開啟 .psimx 電路檔");
            });
        }
        if self.running {
            let t = if self.action == Action::Uninstall { "DaedalusEngine 正在執行中，請先關閉程式再解除安裝。" } else { "DaedalusEngine 正在執行中。建議先關閉程式；若不關閉，新的程式檔會在下次啟動時生效。" };
            Self::note(ui, t, true);
        } else if self.action == Action::Downgrade {
            Self::note(ui, "降級後若以較新版本的功能（新元件、新參數）存檔過電路，舊版可能無法完整開啟這些檔案。", true);
        }
        let label = match self.action {
            Action::Update => "更新",
            Action::Downgrade => "降級",
            Action::Uninstall => "解除安裝",
            _ => "修復",
        };
        let ok = !(self.action == Action::Uninstall && self.running);
        bottom_buttons(ui, |ui| {
            if Self::primary(ui, label, ok) {
                go = Some(true);
            }
            if Self::secondary(ui, "取消") {
                go = Some(false);
            }
        });
        go
    }

    fn page_start(&mut self, ui: &mut egui::Ui, ctx: &egui::Context) -> Option<bool> {
        let mut go = None;
        match self.mode {
            Mode::Install if self.existing.is_some() => go = self.page_maintain(ui),
            Mode::Install => {
                ui.heading(RichText::new(format!("安裝 DaedalusEngine {}", setup::VERSION)).strong());
                ui.label("DaedalusEngine 是一套電力電子電路模擬器。程式將安裝在您的使用者目錄中，不需要系統管理員權限。");
                ui.add_space(4.0);
                ui.label(RichText::new("安裝位置").strong());
                ui.horizontal(|ui| {
                    let w = ui.available_width() - 92.0;
                    ui.add(egui::TextEdit::singleline(&mut self.dir).desired_width(w));
                    if ui.add(egui::Button::new("瀏覽…").min_size(vec2(82.0, 0.0))).clicked() {
                        let d = self.dir();
                        let start = if d.exists() { d } else { d.parent().map(|p| p.to_path_buf()).unwrap_or_default() };
                        if let Some(p) = rfd::FileDialog::new().set_title("選擇安裝資料夾").set_directory(start).pick_folder() {
                            self.dir = setup::normalize_pick(p).display().to_string();
                        }
                    }
                });
                let me = std::env::current_exe().ok().and_then(|p| std::fs::metadata(p).ok()).map(|m| m.len()).unwrap_or(0);
                ui.weak(format!("所需磁碟空間：約 {:.1} MB", (setup::payload_size() + me) as f64 / 1048576.0));
                ui.add_space(4.0);
                ui.checkbox(&mut self.desktop, "建立桌面捷徑");
                ui.checkbox(&mut self.assoc, "以 DaedalusEngine 開啟 .psimx 電路檔");
                ui.weak("開始功能表捷徑一律建立；可在「設定 › 應用程式」中解除安裝。");
                if self.running {
                    Self::note(ui, "DaedalusEngine 正在執行中。建議先關閉程式；若不關閉，新版本會在下次啟動時生效。", true);
                }
                let ok = !self.dir.trim().is_empty();
                bottom_buttons(ui, |ui| {
                    if Self::primary(ui, "安裝", ok) {
                        go = Some(true);
                    }
                    if Self::secondary(ui, "取消") {
                        go = Some(false);
                    }
                });
            }
            Mode::Uninstall => {
                ui.heading(RichText::new("解除安裝 DaedalusEngine").strong());
                let ver = self.existing.as_ref().map(|x| x.1.clone()).filter(|v| !v.is_empty()).map(|v| format!(" V{}", v.trim_start_matches(['V', 'v']))).unwrap_or_default();
                ui.label(format!("將從以下位置移除 DaedalusEngine{}，並刪除開始功能表、桌面捷徑與 .psimx 檔案關聯：", ver));
                ui.label(RichText::new(&self.dir).monospace());
                ui.add_space(6.0);
                ui.checkbox(&mut self.purge, "同時刪除使用者設定（最近開啟的檔案、視窗與介面偏好）");
                ui.weak("您自己的 .psimx 電路檔不會被刪除。");
                if self.running {
                    Self::note(ui, "DaedalusEngine 正在執行中，請先關閉程式再解除安裝。", true);
                }
                let ok = !self.running;
                bottom_buttons(ui, |ui| {
                    if Self::primary(ui, "解除安裝", ok) {
                        go = Some(true);
                    }
                    if Self::secondary(ui, "取消") {
                        go = Some(false);
                    }
                });
            }
        }
        let _ = ctx;
        go
    }
}

/// lay buttons out right-to-left at the bottom of the remaining space
fn bottom_buttons(ui: &mut egui::Ui, f: impl FnOnce(&mut egui::Ui)) {
    let h = 40.0;
    let space = ui.available_height() - h;
    if space > 0.0 {
        ui.add_space(space);
    }
    ui.with_layout(egui::Layout::right_to_left(egui::Align::Center), f);
}

impl eframe::App for Wizard {
    fn ui(&mut self, ui: &mut egui::Ui, _frame: &mut eframe::Frame) {
        let ctx = ui.ctx().clone();
        if let Some(path) = self.shot.clone() {
            self.shot_frame += 1;
            if self.shot_frame == 1 {
                if self.existing.is_some() {
                    match std::env::var("DAEDALUS_SETUP_ACTION").as_deref() {
                        Ok("update") => self.action = Action::Update,
                        Ok("repair") => self.action = Action::Repair,
                        Ok("downgrade") => self.action = Action::Downgrade,
                        Ok("uninstall") => self.action = Action::Uninstall,
                        _ => {}
                    }
                }
                if let Ok(r) = std::env::var("DAEDALUS_SETUP_REPORT") {
                    *self.report.lock().unwrap() = r.split('|').filter(|x| !x.is_empty()).map(String::from).collect();
                }
                if let Ok(p) = std::env::var("DAEDALUS_SETUP_PAGE") {
                    self.page = match p.as_str() {
                        "busy" => {
                            *self.prog.lock().unwrap() = (0.46, "複製 DaedalusEngine.exe…".into());
                            Page::Busy
                        }
                        "done" => Page::Done(vec![]),
                        "failed" => Page::Failed(r"無法寫入 C:\Program Files\DaedalusEngine：此位置需要系統管理員權限。".into()),
                        _ => Page::Start,
                    };
                }
            }
            if self.shot_frame == 20 {
                let c = ctx.clone();
                ctx.send_viewport_cmd(egui::ViewportCommand::Screenshot(egui::ScreenshotCallback::new(move |im| {
                    let [w, h] = im.size;
                    let buf: Vec<u8> = im.pixels.iter().flat_map(|p| p.to_array()).collect();
                    if let Some(rgba) = image::RgbaImage::from_raw(w as u32, h as u32, buf) {
                        let _ = rgba.save(&path);
                    }
                    c.send_viewport_cmd(egui::ViewportCommand::Close);
                })));
            }
            ctx.request_repaint();
        }
        // poll whether the application is running (it blocks uninstalling / replacing the exe)
        if matches!(self.page, Page::Start) && self.checked.map(|t| t.elapsed() > Duration::from_millis(800)).unwrap_or(true) {
            self.running = setup::app_running(&self.dir());
            self.checked = Some(Instant::now());
            ctx.request_repaint_after(Duration::from_millis(900));
        }
        if let Page::Busy = self.page {
            if let Some(r) = self.outcome.lock().unwrap().take() {
                self.page = match r {
                    Ok(w) => Page::Done(w),
                    Err(e) => Page::Failed(e),
                };
            }
        }
        // never close in the middle of an install
        if ctx.input(|i| i.viewport().close_requested()) && matches!(self.page, Page::Busy) {
            ctx.send_viewport_cmd(egui::ViewportCommand::CancelClose);
        }

        let full = ui.max_rect();
        ui.painter().rect_filled(full, 0.0, Color32::from_rgb(0xf7, 0xf8, 0xfc));
        let side = Rect::from_min_max(full.min, pos2(full.min.x + 210.0, full.max.y));
        self.sidebar(ui, side);
        let content = Rect::from_min_max(pos2(side.max.x + 26.0, full.min.y + 22.0), pos2(full.max.x - 24.0, full.max.y - 18.0));
        let mut child = ui.new_child(egui::UiBuilder::new().max_rect(content).layout(egui::Layout::top_down(egui::Align::Min)));
        let ui = &mut child;
        let mut close = false;
        match &self.page {
            Page::Start => match self.page_start(ui, &ctx) {
                Some(true) => self.start(&ctx),
                Some(false) => close = true,
                None => {}
            },
            Page::Busy => {
                let (f, m) = self.prog.lock().unwrap().clone();
                let h = match (self.mode, self.action) {
                    (Mode::Uninstall, _) => "正在解除安裝…",
                    (_, Action::Update) => "正在更新…",
                    (_, Action::Repair) => "正在修復…",
                    (_, Action::Downgrade) => "正在降級…",
                    _ => "正在安裝…",
                };
                ui.heading(RichText::new(h).strong());
                ui.add_space(12.0);
                ui.add(egui::ProgressBar::new(f).desired_height(18.0).fill(ACCENT).text(RichText::new(format!("{:.0}%", f * 100.0)).color(Color32::WHITE)));
                ui.label(m);
            }
            Page::Done(w) => {
                let w = w.clone();
                match self.mode {
                    Mode::Install => {
                        let old = self.existing.as_ref().map(|x| setup::ver_label(&x.1)).unwrap_or_default();
                        match self.action {
                            Action::Update | Action::Downgrade => {
                                ui.heading(RichText::new(if self.action == Action::Update { "更新完成" } else { "降級完成" }).strong());
                                ui.label(format!("DaedalusEngine 已由 {} 換成 {}，位置：", old, setup::VERSION));
                                ui.label(RichText::new(self.dir().display().to_string()).monospace());
                                ui.label("電路檔與使用者設定都已保留。");
                            }
                            Action::Repair => {
                                ui.heading(RichText::new("修復完成").strong());
                                let found = self.report.lock().unwrap().clone();
                                if found.is_empty() {
                                    ui.label(format!("已檢查 DaedalusEngine {} 的程式檔案、捷徑、檔案關聯與登錄資訊：全部完好，並已重新寫入一次。", setup::VERSION));
                                } else {
                                    ui.label(format!("檢查發現並已修復 {} 個項目：", found.len()));
                                    egui::ScrollArea::vertical().max_height(110.0).show(ui, |ui| {
                                        for x in &found {
                                            ui.label(format!("• {}", x));
                                        }
                                    });
                                }
                            }
                            _ => {
                                ui.heading(RichText::new("安裝完成").strong());
                                ui.label(format!("DaedalusEngine {} 已安裝到：", setup::VERSION));
                                ui.label(RichText::new(self.dir().display().to_string()).monospace());
                                ui.label("可從開始功能表或桌面捷徑啟動，或直接按兩下 .psimx 電路檔開啟。");
                            }
                        }
                        for x in &w {
                            Self::note(ui, x, true);
                        }
                        ui.add_space(6.0);
                        ui.checkbox(&mut self.launch, "立即啟動 DaedalusEngine");
                        bottom_buttons(ui, |ui| {
                            if Self::primary(ui, "完成", true) {
                                close = true;
                            }
                        });
                        if close && self.launch {
                            if let Err(e) = setup::launch(&self.dir()) {
                                crate::win::msg_error("DaedalusEngine", &e);
                            }
                        }
                    }
                    Mode::Uninstall => {
                        ui.heading(RichText::new("已解除安裝").strong());
                        ui.label("DaedalusEngine 已從這台電腦移除。感謝您的使用！");
                        for x in &w {
                            Self::note(ui, x, true);
                        }
                        bottom_buttons(ui, |ui| {
                            if Self::primary(ui, "關閉", true) {
                                close = true;
                            }
                        });
                        self.close_after = true;
                    }
                }
            }
            Page::Failed(e) => {
                let e = e.clone();
                let h = match (self.mode, self.action) {
                    (Mode::Uninstall, _) => "解除安裝失敗",
                    (_, Action::Update) => "更新失敗",
                    (_, Action::Repair) => "修復失敗",
                    (_, Action::Downgrade) => "降級失敗",
                    _ => "安裝失敗",
                };
                ui.heading(RichText::new(h).color(Color32::from_rgb(0xb0, 0x20, 0x20)).strong());
                egui::ScrollArea::vertical().max_height(200.0).show(ui, |ui| {
                    ui.add(egui::Label::new(&e).selectable(true));
                });
                bottom_buttons(ui, |ui| {
                    if Self::primary(ui, "返回", true) {
                        if self.existing.is_some() && self.action == Action::Uninstall {
                            self.mode = Mode::Install; // back to the maintenance page
                        }
                        self.page = Page::Start;
                    }
                    if Self::secondary(ui, "關閉") {
                        close = true;
                    }
                });
            }
        }
        if close {
            ctx.send_viewport_cmd(egui::ViewportCommand::Close);
        }
    }

    fn on_exit(&mut self, _gl: Option<&eframe::glow::Context>) {
        if self.mode == Mode::Uninstall && self.close_after {
            setup::schedule_self_delete(&self.dir());
        }
    }
}
