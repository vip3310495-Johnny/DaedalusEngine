//! DaedalusEngine — power-electronics circuit simulator (GUI).
#![windows_subsystem = "windows"]

mod app;
mod bode;
mod canvas;
mod dialogs;
mod editor;
mod icons;
mod libpanel;
mod plot;
mod simrun;
mod symbols;
mod theme;
mod wave;

use std::sync::Arc;

pub fn app_icon() -> egui::IconData {
    match image::load_from_memory(include_bytes!("../assets/icon_256.png")) {
        Ok(img) => {
            let rgba = img.to_rgba8();
            egui::IconData { width: rgba.width(), height: rgba.height(), rgba: rgba.into_raw() }
        }
        Err(_) => egui::IconData::default(),
    }
}

/// Traditional Chinese UI font from Windows (Microsoft JhengHei), falling back to other CJK fonts.
fn setup_fonts(ctx: &egui::Context) {
    let mut fonts = egui::FontDefinitions::default();
    let windir = std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string());
    let cands = ["msjh.ttc", "msjhl.ttc", "msyh.ttc", "mingliu.ttc", "simsun.ttc"];
    for f in cands {
        let p = std::path::Path::new(&windir).join("Fonts").join(f);
        if let Ok(bytes) = std::fs::read(&p) {
            fonts.font_data.insert("cjk".into(), Arc::new(egui::FontData::from_owned(bytes)));
            fonts.families.entry(egui::FontFamily::Proportional).or_default().insert(0, "cjk".into());
            fonts.families.entry(egui::FontFamily::Monospace).or_default().push("cjk".into());
            break;
        }
    }
    // symbols (▶ ■ ✕ ⚠ …) that the CJK font lacks
    if let Ok(bytes) = std::fs::read(std::path::Path::new(&windir).join("Fonts").join("seguisym.ttf")) {
        fonts.font_data.insert("symbols".into(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Proportional).or_default().push("symbols".into());
        fonts.families.entry(egui::FontFamily::Monospace).or_default().push("symbols".into());
    }
    let mono = std::path::Path::new(&windir).join("Fonts").join("consola.ttf");
    if let Ok(bytes) = std::fs::read(mono) {
        fonts.font_data.insert("consolas".into(), Arc::new(egui::FontData::from_owned(bytes)));
        fonts.families.entry(egui::FontFamily::Monospace).or_default().insert(0, "consolas".into());
    }
    ctx.set_fonts(fonts);
}

fn main() -> eframe::Result<()> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let open = args.iter().enumerate().find(|(i, a)| !a.starts_with("--") && (*i == 0 || args[*i - 1] != "--shot")).map(|(_, a)| std::path::PathBuf::from(a)).filter(|p| p.exists());
    let autorun = args.iter().any(|a| a == "--run");
    let shot = args.iter().position(|a| a == "--shot").and_then(|i| args.get(i + 1)).cloned();
    let options = eframe::NativeOptions {
        viewport: egui::ViewportBuilder::default().with_title("DaedalusEngine").with_inner_size([1440.0, 900.0]).with_min_inner_size([900.0, 600.0]).with_icon(Arc::new(app_icon())),
        persist_window: true,
        ..Default::default()
    };
    eframe::run_native(
        "DaedalusEngine",
        options,
        Box::new(move |cc| {
            setup_fonts(&cc.egui_ctx);
            cc.egui_ctx.options_mut(|o| o.zoom_with_keyboard = false);
            let mut a = app::App::new(cc, open);
            a.autorun = autorun;
            if shot.is_some() {
                cc.egui_ctx.set_embed_viewports(true);
            }
            a.shot = shot.clone();
            Ok(Box::new(a))
        }),
    )
}
