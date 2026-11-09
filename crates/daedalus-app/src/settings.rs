//! User settings, saved with the app settings: the schematic window (File → Settings) and the waveform viewer.

use serde::{Deserialize, Serialize};

/// egui's default body text size, the reference for the interface text size
const BODY: f32 = 13.0;

/// text styles of egui scaled so that the body text is `body` points
pub fn text_styles(body: f32) -> std::collections::BTreeMap<egui::TextStyle, egui::FontId> {
    let k = body / BODY;
    egui::style::default_text_styles()
        .into_iter()
        .map(|(t, mut f)| {
            f.size *= k;
            (t, f)
        })
        .collect()
}

#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct AppSettings {
    /// interface text size (body text, points)
    pub ui_font: f32,
    /// names and parameters next to the elements on the schematic, % of the default size
    pub label_pct: f32,
    /// a double click on an empty spot of the schematic opens the quick-add box
    pub dbl_quick: bool,
}

impl Default for AppSettings {
    fn default() -> Self {
        AppSettings { ui_font: BODY, label_pct: 100.0, dbl_quick: false }
    }
}

impl AppSettings {
    pub fn clamped(mut self) -> Self {
        self.ui_font = if self.ui_font.is_finite() { self.ui_font.clamp(9.0, 24.0) } else { BODY };
        self.label_pct = if self.label_pct.is_finite() { self.label_pct.clamp(50.0, 250.0) } else { 100.0 };
        self
    }
    /// interface text sizes (light and dark style)
    pub fn apply(&self, ctx: &egui::Context) {
        let ts = text_styles(self.ui_font);
        ctx.all_styles_mut(|s| s.text_styles = ts.clone());
    }
}

/// File → Settings; returns false once closed
pub fn app_settings_ui(ctx: &egui::Context, s: &mut AppSettings) -> bool {
    let mut open = true;
    egui::Window::new("設定").id(egui::Id::new("app_settings")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
        egui::Grid::new("app_set").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
            ui.label("介面文字大小");
            ui.add(egui::Slider::new(&mut s.ui_font, 9.0..=24.0).step_by(0.5).suffix(" pt"));
            ui.end_row();
            ui.label("電路圖元件文字大小");
            ui.add(egui::Slider::new(&mut s.label_pct, 50.0..=250.0).step_by(5.0).suffix(" %"));
            ui.end_row();
            ui.label("空白處雙擊");
            ui.checkbox(&mut s.dbl_quick, "開啟快速加入元件");
            ui.end_row();
        });
        ui.weak("元件文字是電路圖上元件旁的名稱與參數，會隨電路圖縮放一起變大變小。");
        ui.weak("快速加入元件也可以按 / 開啟。");
        ui.separator();
        if ui.button("還原預設").clicked() {
            *s = AppSettings::default();
        }
    });
    *s = s.clamped();
    open
}

/// waveform viewer settings (also used for the exported report)
#[derive(Clone, Copy, PartialEq, Debug, Serialize, Deserialize)]
#[serde(default)]
pub struct WaveStyle {
    /// left column: signal list, readouts, measurement tables (points)
    pub side_font: f32,
    /// text in the waveform screens: legend, cursor labels
    pub plot_font: f32,
    /// X / Y axis tick labels
    pub axis_font: f32,
    /// curve line width
    pub line_w: f32,
}

impl Default for WaveStyle {
    fn default() -> Self {
        WaveStyle { side_font: BODY, plot_font: 13.0, axis_font: 11.0, line_w: 1.4 }
    }
}

impl WaveStyle {
    pub fn clamped(mut self) -> Self {
        let c = |v: f32, lo: f32, hi: f32, d: f32| if v.is_finite() { v.clamp(lo, hi) } else { d };
        let d = WaveStyle::default();
        self.side_font = c(self.side_font, 8.0, 24.0, d.side_font);
        self.plot_font = c(self.plot_font, 8.0, 28.0, d.plot_font);
        self.axis_font = c(self.axis_font, 8.0, 24.0, d.axis_font);
        self.line_w = c(self.line_w, 0.5, 6.0, d.line_w);
        self
    }
    /// waveform viewer settings window; returns false once closed
    pub fn ui(&mut self, ctx: &egui::Context) -> bool {
        let mut open = true;
        egui::Window::new("波形檢視器設定").id(egui::Id::new("wave_style")).open(&mut open).collapsible(false).resizable(false).show(ctx, |ui| {
            egui::Grid::new("wave_set").num_columns(2).spacing([14.0, 8.0]).show(ui, |ui| {
                ui.label("欄位字體大小");
                ui.add(egui::Slider::new(&mut self.side_font, 8.0..=24.0).step_by(0.5).suffix(" pt"));
                ui.end_row();
                ui.label("波形畫面字體大小");
                ui.add(egui::Slider::new(&mut self.plot_font, 8.0..=28.0).step_by(0.5).suffix(" pt"));
                ui.end_row();
                ui.label("X / Y 軸字體大小");
                ui.add(egui::Slider::new(&mut self.axis_font, 8.0..=24.0).step_by(0.5).suffix(" pt"));
                ui.end_row();
                ui.label("波形線寬");
                ui.add(egui::Slider::new(&mut self.line_w, 0.5..=6.0).step_by(0.1).suffix(" px"));
                ui.end_row();
            });
            ui.weak("欄位：左側訊號清單、讀值與量測表；波形畫面：圖例與游標標籤。");
            ui.weak("這些設定也會套用到匯出的報告（PDF、Excel）。");
            ui.separator();
            if ui.button("還原預設").clicked() {
                *self = WaveStyle::default();
            }
        });
        *self = self.clamped();
        open
    }
}
