//! Colours (same palette as the original web version, light and dark).

use egui::Color32;

#[derive(Clone, Copy)]
pub struct Pal {
    pub canvas: Color32,
    pub grid: Color32,
    pub grid2: Color32,
    pub sym: Color32,
    pub wire: Color32,
    pub ctl: Color32,
    pub accent: Color32,
    pub accent_soft: Color32,
    pub sel: Color32,
    pub ghost: Color32,
    pub err: Color32,
    pub warn: Color32,
    pub ok: Color32,
    pub text: Color32,
    pub muted: Color32,
    pub faint: Color32,
    pub panel: Color32,
    pub panel2: Color32,
    pub line: Color32,
    pub curves: [Color32; 8],
}

const fn hex(v: u32) -> Color32 {
    Color32::from_rgb((v >> 16) as u8, (v >> 8) as u8, v as u8)
}

pub const LIGHT: Pal = Pal {
    canvas: hex(0xfbfcfd),
    grid: hex(0xb9c3cf),
    grid2: hex(0xdde3ea),
    sym: hex(0x1f48b8),
    wire: hex(0x1f48b8),
    ctl: hex(0x0b7a75),
    accent: hex(0xe8590c),
    accent_soft: Color32::from_rgba_premultiplied(28, 11, 1, 30),
    sel: hex(0xe8590c),
    ghost: hex(0x8d97a5),
    err: hex(0xd9363e),
    warn: hex(0xc76b00),
    ok: hex(0x2b8a3e),
    text: hex(0x18202a),
    muted: hex(0x5d6877),
    faint: hex(0x8b95a3),
    panel: hex(0xffffff),
    panel2: hex(0xf6f8fa),
    line: hex(0xd6dce4),
    curves: [hex(0x1c6fd1), hex(0xe8590c), hex(0x2b8a3e), hex(0xc2255c), hex(0x6741d9), hex(0x0c8599), hex(0xb8860b), hex(0x495057)],
};
pub const DARK: Pal = Pal {
    canvas: hex(0x0b0f14),
    grid: hex(0x3a4554),
    grid2: hex(0x1c232c),
    sym: hex(0x7aa7ff),
    wire: hex(0x7aa7ff),
    ctl: hex(0x4fd1c5),
    accent: hex(0xff922b),
    accent_soft: Color32::from_rgba_premultiplied(41, 23, 7, 41),
    sel: hex(0xff922b),
    ghost: hex(0x6c7787),
    err: hex(0xff6b6b),
    warn: hex(0xffb347),
    ok: hex(0x69db7c),
    text: hex(0xe4e9ef),
    muted: hex(0x9aa6b4),
    faint: hex(0x6d7885),
    panel: hex(0x151a21),
    panel2: hex(0x1a2029),
    line: hex(0x2a323d),
    curves: [hex(0x5aa2ff), hex(0xff922b), hex(0x69db7c), hex(0xf783ac), hex(0xb197fc), hex(0x3bc9db), hex(0xffd43b), hex(0xced4da)],
};

pub fn pal(dark: bool) -> &'static Pal {
    if dark {
        &DARK
    } else {
        &LIGHT
    }
}

/// Refined egui visuals for both themes: rounder corners, calm panel colours, soft shadows, a blue selection
/// and a little more room between widgets.
pub fn style_visuals(ctx: &egui::Context) {
    use egui::{CornerRadius, Margin, Shadow, Stroke, Theme};
    for (theme, p) in [(Theme::Light, &LIGHT), (Theme::Dark, &DARK)] {
        let dark = theme == Theme::Dark;
        let pick = |d: u32, l: u32| if dark { hex(d) } else { hex(l) };
        ctx.style_mut_of(theme, |s| {
            let v = &mut s.visuals;
            for w in [&mut v.widgets.noninteractive, &mut v.widgets.inactive, &mut v.widgets.hovered, &mut v.widgets.active, &mut v.widgets.open] {
                w.corner_radius = CornerRadius::same(5);
            }
            v.window_corner_radius = CornerRadius::same(8);
            v.menu_corner_radius = CornerRadius::same(6);
            v.panel_fill = p.panel2;
            v.window_fill = p.panel;
            v.window_stroke = Stroke::new(1.0, p.line);
            v.widgets.noninteractive.bg_stroke = Stroke::new(1.0, p.line);
            v.faint_bg_color = pick(0x1d242e, 0xeef2f6);
            v.extreme_bg_color = pick(0x0f1319, 0xffffff);
            v.selection.bg_fill = pick(0x1f4f8f, 0xcfe2ff);
            v.selection.stroke = Stroke::new(1.0, pick(0xd0e2ff, 0x0b4fae));
            v.hyperlink_color = pick(0x7aa7ff, 0x1c6fd1);
            v.widgets.inactive.weak_bg_fill = pick(0x242c37, 0xe8ecf1);
            v.widgets.inactive.bg_fill = pick(0x242c37, 0xe8ecf1);
            v.widgets.hovered.weak_bg_fill = pick(0x2d3745, 0xdce3eb);
            v.widgets.hovered.bg_fill = pick(0x2d3745, 0xdce3eb);
            v.widgets.hovered.bg_stroke = Stroke::new(1.0, pick(0x4a5a70, 0xb4c0ce));
            v.widgets.active.weak_bg_fill = pick(0x364352, 0xccd6e2);
            v.widgets.active.bg_fill = pick(0x364352, 0xccd6e2);
            v.window_shadow = Shadow { offset: [0, 6], blur: 22, spread: 0, color: Color32::from_black_alpha(if dark { 110 } else { 40 }) };
            v.popup_shadow = Shadow { offset: [0, 4], blur: 14, spread: 0, color: Color32::from_black_alpha(if dark { 100 } else { 34 }) };
            s.spacing.item_spacing = egui::vec2(8.0, 5.0);
            s.spacing.button_padding = egui::vec2(7.0, 3.0);
            s.spacing.menu_margin = Margin::same(6);
            s.spacing.window_margin = Margin::same(10);
        });
    }
}
