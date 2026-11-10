//! DaedalusEngine setup program: installs per-user (no administrator rights), and doubles as the uninstaller.
//!
//! Usage:
//!   DaedalusEngine-Setup.exe                       wizard
//!   DaedalusEngine-Setup.exe --silent [--dir P] [--no-desktop] [--no-assoc] [--launch]   install or update
//!   DaedalusEngine-Setup.exe --silent --repair     re-write the installed copy in place (keeps its choices)
//!   uninstall.exe --maintain                       maintenance page (Settings › Apps › 修改)
//!   uninstall.exe --uninstall [--silent] [--purge] [--dir P]
//!
//! The wizard asks the user to accept the license terms and disclaimer (NOTICE.txt, LICENSE) before installing;
//! a silent install accepts them on behalf of whoever runs it (logged), as documented in the README.
#![windows_subsystem = "windows"]

mod setup;
mod ui;
mod win;

use std::path::PathBuf;
use std::sync::Arc;

struct Args {
    uninstall: bool,
    silent: bool,
    dir: Option<PathBuf>,
    /// None: keep the existing installation's choice (on for a new install)
    desktop: Option<bool>,
    assoc: Option<bool>,
    launch: bool,
    purge: bool,
    repair: bool,
    shot: Option<String>,
}

fn parse_args() -> Args {
    let v: Vec<String> = std::env::args().skip(1).collect();
    let mut a = Args { uninstall: false, silent: false, dir: None, desktop: None, assoc: None, launch: false, purge: false, repair: false, shot: None };
    let mut i = 0;
    while i < v.len() {
        let s = v[i].as_str();
        let l = s.to_ascii_lowercase();
        match l.as_str() {
            "--uninstall" | "/uninstall" | "/u" => a.uninstall = true,
            "--silent" | "/s" | "/silent" | "/verysilent" | "-s" => a.silent = true,
            "--no-desktop" => a.desktop = Some(false),
            "--desktop" => a.desktop = Some(true),
            "--no-assoc" => a.assoc = Some(false),
            "--assoc" => a.assoc = Some(true),
            "--repair" | "/repair" => a.repair = true,
            "--maintain" | "--modify" => {}
            "--launch" => a.launch = true,
            "--purge" => a.purge = true,
            "--shot" => {
                i += 1;
                a.shot = v.get(i).cloned();
            }
            "--dir" => {
                i += 1;
                a.dir = v.get(i).map(PathBuf::from);
            }
            _ if l.starts_with("/d=") => a.dir = Some(PathBuf::from(&s[3..])),
            _ if l.starts_with("--dir=") => a.dir = Some(PathBuf::from(&s[6..])),
            _ => {}
        }
        i += 1;
    }
    a
}

/// Traditional Chinese UI font from Windows (Microsoft JhengHei), falling back to other CJK fonts.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    for f in ["msjh.ttc", "msjhl.ttc", "msyh.ttc", "mingliu.ttc", "simsun.ttc"] {
        if let Ok(bytes) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join(f)) {
            fonts.font_data.insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "cjk".into());
            fonts.families.entry(egui::FontFamily::Monospace).or_default().push("cjk".into());
            break;
        }
    }
    if let Ok(bytes) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join("consola.ttf")) {
        fonts.font_data.insert("consolas".into(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);
}

fn icon() -> egui::IconData {
    match image::load_from_memory(include_bytes!("../assets/icon_256.png")) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
        }
        Err(_) => egui::IconData::default(),
    }
}

/// shortcut / association choices: explicit flags, else the existing installation's, else on
fn prefs(a: &Args) -> (bool, bool) {
    let (d, s) = setup::existing_prefs();
    (a.desktop.unwrap_or(d), a.assoc.unwrap_or(s))
}

fn run_silent(a: &Args) -> i32 {
    let quiet = |_: f32, _: &str| {};
    if a.uninstall {
        let dir = setup::uninstall_dir(a.dir.clone());
        setup::log(&format!("uninstall {}", dir.display()));
        let r = setup::uninstall(&dir, a.purge, &quiet);
        setup::schedule_self_delete(&dir);
        match r {
            Ok(w) => {
                w.iter().for_each(|x| setup::log(x));
                0
            }
            Err(e) => {
                setup::log(&e);
                1
            }
        }
    } else {
        let (desktop, assoc) = prefs(a);
        let existing = setup::existing_install();
        if a.repair && existing.is_none() {
            setup::log("repair: DaedalusEngine is not installed");
            return 1;
        }
        let dir = if a.repair { existing.as_ref().map(|x| x.0.clone()).unwrap_or_default() } else { a.dir.clone().unwrap_or_else(setup::default_dir) };
        let o = setup::Options { dir, desktop, assoc };
        setup::log("terms: license and disclaimer (NOTICE.txt, LICENSE) accepted by the silent install");
        match &existing {
            Some((_, v)) if a.repair => {
                setup::log(&format!("repair {} (installed {}) {}", setup::VERSION, setup::ver_label(v), o.dir.display()));
                let found = setup::diagnose(&o);
                if found.is_empty() {
                    setup::log("check: no problems found");
                }
                found.iter().for_each(|x| setup::log(&format!("fix: {}", x)));
            }
            Some((_, v)) => setup::log(&format!("update {} -> {} {}", setup::ver_label(v), setup::VERSION, o.dir.display())),
            None => setup::log(&format!("install {} {}", setup::VERSION, o.dir.display())),
        }
        match setup::install(&o, &quiet) {
            Ok(w) => {
                w.iter().for_each(|x| setup::log(x));
                setup::log("ok");
                if a.launch {
                    let _ = setup::launch(&o.dir);
                }
                0
            }
            Err(e) => {
                setup::log(&e);
                1
            }
        }
    }
}

/// used only if the graphical wizard cannot start (e.g. no OpenGL driver)
fn run_fallback(a: &Args, err: &str) -> i32 {
    let title = format!("DaedalusEngine {} 安裝程式", setup::VERSION);
    if a.uninstall {
        let dir = setup::uninstall_dir(a.dir.clone());
        if !win::msg_yes_no(&title, &format!("要從 {} 解除安裝 DaedalusEngine 嗎？", dir.display())) {
            return 2;
        }
        let r = setup::uninstall(&dir, false, &|_, _| {});
        setup::schedule_self_delete(&dir);
        match r {
            Ok(_) => win::msg_info(&title, "DaedalusEngine 已解除安裝。"),
            Err(e) => win::msg_error(&title, &e),
        }
        return 0;
    }
    let dir = a.dir.clone().unwrap_or_else(setup::default_dir);
    let q = format!("無法開啟安裝畫面（{}）。\n\n要以預設設定將 DaedalusEngine 安裝到\n{}\n嗎？（含開始功能表與桌面捷徑、.psimx 檔案關聯）\n\n按「是」即表示您同意本程式的授權條款與免責聲明：依 GPL-3.0 授權、不提供任何擔保，模擬結果僅供工程參考，著作權人不對使用本程式所生的任何損害負責。全文見安裝資料夾中的 NOTICE.txt 與 LICENSE。", err, dir.display());
    if !win::msg_yes_no(&title, &q) {
        return 2;
    }
    let (desktop, assoc) = prefs(a);
    match setup::install(&setup::Options { dir: dir.clone(), desktop, assoc }, &|_, _| {}) {
        Ok(_) => {
            win::msg_info(&title, &format!("DaedalusEngine 已安裝到 {}。", dir.display()));
            0
        }
        Err(e) => {
            win::msg_error(&title, &e);
            1
        }
    }
}

fn main() {
    let a = parse_args();
    if a.silent {
        std::process::exit(run_silent(&a));
    }
    let mode = if a.uninstall { ui::Mode::Uninstall } else { ui::Mode::Install };
    let dir = if a.uninstall { setup::uninstall_dir(a.dir.clone()) } else { a.dir.clone().unwrap_or_else(setup::default_dir) };
    let title = if a.uninstall { "解除安裝 DaedalusEngine".to_string() } else { format!("DaedalusEngine {} 安裝程式", setup::VERSION) };
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title(&title).with_inner_size([700.0, 450.0]).with_resizable(false).with_maximize_button(false).with_icon(Arc::new(icon())),
        centered: true,
        ..Default::default()
    };
    let (desktop, assoc) = prefs(&a);
    let shot = a.shot.clone();
    let r = eframe::run_native(
        "DaedalusEngine-Setup",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            let mut w = ui::Wizard::new(cc, mode, dir, desktop, assoc);
            w.shot = shot;
            Ok(Box::new(w))
        }),
    );
    if let Err(e) = r {
        std::process::exit(run_fallback(&a, &e.to_string()));
    }
}
