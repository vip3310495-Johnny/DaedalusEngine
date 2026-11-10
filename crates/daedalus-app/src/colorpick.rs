//! Curve colour button: a small swatch that opens a palette of ready colours (one click) and a compact
//! colour picker for any other colour.

use egui::{vec2, Color32, Response, Sense, Stroke, Ui};

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

/// three rows: the light-theme curve colours, the dark-theme ones, and more (all readable on both backgrounds)
pub const PALETTE: [Color32; 24] = [
    hex(0x1c6fd1), hex(0xe8590c), hex(0x2b8a3e), hex(0xc2255c), hex(0x6741d9), hex(0x0c8599), hex(0xb8860b), hex(0x495057),
    hex(0x5aa2ff), hex(0xff922b), hex(0x69db7c), hex(0xf783ac), hex(0xb197fc), hex(0x3bc9db), hex(0xffd43b), hex(0xadb5bd),
    hex(0xe03131), hex(0xf08c00), hex(0x74b816), hex(0x0ca678), hex(0x1971c2), hex(0x9c36b5), hex(0xa61e4d), hex(0x212529),
];

fn swatch(ui: &mut Ui, size: egui::Vec2, c: Color32, sel: bool) -> Response {
    let (rect, resp) = ui.allocate_exact_size(size, Sense::click());
    let vis = ui.visuals();
    let r = rect.shrink(if resp.hovered() { 0.0 } else { 1.0 });
    ui.painter().rect_filled(r, 3.0, c);
    let edge = if sel || resp.hovered() { Stroke::new(2.0, vis.strong_text_color()) } else { Stroke::new(1.0, vis.widgets.noninteractive.bg_stroke.color) };
    ui.painter().rect_stroke(r, 3.0, edge, egui::StrokeKind::Outside);
    resp
}

/// swatch button; returns a response marked changed when the colour was changed
pub fn color_button(ui: &mut Ui, c: &mut Color32) -> Response {
    let h = ui.spacing().interact_size.y.min(18.0);
    let mut resp = swatch(ui, vec2(h, h), *c, false);
    let popup_id = resp.id.with("palette");
    let mut changed = false;
    egui::Popup::menu(&resp).id(popup_id).close_behavior(egui::PopupCloseBehavior::CloseOnClickOutside).show(|ui| {
        ui.spacing_mut().item_spacing = vec2(4.0, 4.0);
        ui.label(egui::RichText::new("快速選色").weak());
        for row in PALETTE.chunks(8) {
            ui.horizontal(|ui| {
                for &p in row {
                    if swatch(ui, vec2(20.0, 20.0), p, p == *c).on_hover_text(format!("#{:02x}{:02x}{:02x}", p.r(), p.g(), p.b())).clicked() {
                        *c = p;
                        changed = true;
                        ui.close();
                    }
                }
            });
        }
        ui.separator();
        ui.label(egui::RichText::new("自訂顏色").weak());
        // a compact picker (egui's own popup is 275 px wide)
        ui.spacing_mut().slider_width = 196.0;
        if egui::color_picker::color_picker_color32(ui, c, egui::color_picker::Alpha::Opaque) {
            changed = true;
        }
    });
    if changed {
        resp.mark_changed();
    }
    resp
}
