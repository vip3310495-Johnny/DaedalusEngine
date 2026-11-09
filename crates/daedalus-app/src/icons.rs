//! Vector toolbar icons (drawn with the painter, independent of fonts).

use egui::{pos2, vec2, Color32, Pos2, Rect, Response, Sense, Shape, Stroke, Ui};

#[derive(Clone, Copy, PartialEq)]
pub enum Icon {
    New,
    Open,
    Save,
    Undo,
    Redo,
    Rotate,
    Mirror,
    /// flip upside down
    MirrorV,
    /// hand tool (pan the schematic)
    Hand,
    Delete,
    Select,
    Wire,
    Fit,
    Run,
    Stop,
    Clock,
    Wave,
    ZoomIn,
    ZoomOut,
    Cursor,
    Pan,
    BoxZoom,
    XZoom,
    Export,
    Plus,
    Minus,
    Search,
}

fn arc(c: Pos2, r: f32, a0: f32, a1: f32, n: usize) -> Vec<Pos2> {
    (0..=n).map(|i| {
        let a = a0 + (a1 - a0) * i as f32 / n as f32;
        pos2(c.x + r * a.cos(), c.y + r * a.sin())
    }).collect()
}

pub fn paint(ui: &Ui, r: Rect, icon: Icon, col: Color32) {
    let p = ui.painter();
    let s = r.width().min(r.height());
    let c = r.center();
    let u = s / 16.0; // icon unit (16 x 16 design grid)
    let at = |x: f32, y: f32| pos2(c.x + (x - 8.0) * u, c.y + (y - 8.0) * u);
    let st = Stroke::new(1.4, col);
    let line = |pts: &[(f32, f32)]| Shape::line(pts.iter().map(|&(x, y)| at(x, y)).collect(), st);
    match icon {
        Icon::New => {
            p.add(line(&[(4.0, 2.0), (10.0, 2.0), (13.0, 5.0), (13.0, 14.0), (4.0, 14.0), (4.0, 2.0)]));
            p.add(line(&[(10.0, 2.0), (10.0, 5.0), (13.0, 5.0)]));
        }
        Icon::Open => {
            p.add(line(&[(2.0, 4.0), (6.0, 4.0), (7.5, 5.5), (13.0, 5.5), (13.0, 13.0), (2.0, 13.0), (2.0, 4.0)]));
            p.add(line(&[(2.0, 13.0), (4.5, 8.0), (15.0, 8.0), (13.0, 13.0)]));
        }
        Icon::Save => {
            p.add(line(&[(2.5, 2.5), (11.5, 2.5), (13.5, 4.5), (13.5, 13.5), (2.5, 13.5), (2.5, 2.5)]));
            p.add(line(&[(5.0, 2.5), (5.0, 6.0), (10.5, 6.0), (10.5, 2.5)]));
            p.add(line(&[(4.5, 13.5), (4.5, 9.0), (11.5, 9.0), (11.5, 13.5)]));
        }
        Icon::Undo | Icon::Redo => {
            let flip = icon == Icon::Redo;
            let fx = |x: f32| if flip { 16.0 - x } else { x };
            let pts: Vec<Pos2> = arc(at(fx(8.5), 9.0), 4.5 * u, if flip { -0.9 } else { std::f32::consts::PI + 0.9 }, if flip { -std::f32::consts::PI * 1.6 } else { std::f32::consts::PI * 2.6 }, 18);
            p.add(Shape::line(pts, st));
            let (ax, ay) = (fx(4.0), 5.0);
            p.add(Shape::convex_polygon(vec![at(ax, ay - 2.2), at(ax, ay + 2.2), at(fx(1.8), ay + 0.2)], col, Stroke::NONE));
        }
        Icon::Rotate => {
            p.add(Shape::line(arc(c, 5.0 * u, -0.3, 4.6, 24), st));
            let e = arc(c, 5.0 * u, -0.3, -0.3, 1)[0];
            p.add(Shape::convex_polygon(vec![e + vec2(-2.6 * u, -0.6 * u), e + vec2(1.6 * u, -1.6 * u), e + vec2(0.6 * u, 2.4 * u)], col, Stroke::NONE));
        }
        Icon::Mirror => {
            p.add(Shape::convex_polygon(vec![at(7.0, 3.0), at(7.0, 13.0), at(2.0, 13.0)], Color32::TRANSPARENT, st));
            p.add(Shape::convex_polygon(vec![at(9.0, 3.0), at(9.0, 13.0), at(14.0, 13.0)], col.gamma_multiply(0.5), st));
        }
        Icon::MirrorV => {
            p.add(Shape::convex_polygon(vec![at(3.0, 7.0), at(13.0, 7.0), at(13.0, 2.0)], Color32::TRANSPARENT, st));
            p.add(Shape::convex_polygon(vec![at(3.0, 9.0), at(13.0, 9.0), at(13.0, 14.0)], col.gamma_multiply(0.5), st));
        }
        Icon::Hand => {
            // open hand: four fingers, thumb and palm
            for (x, top) in [(5.0, 4.0), (7.3, 2.5), (9.6, 3.0), (11.8, 4.8)] {
                p.add(line(&[(x, 9.0), (x, top)]));
            }
            p.add(line(&[(5.0, 9.5), (3.0, 7.8), (2.2, 8.6), (5.2, 13.2), (7.0, 14.5), (10.5, 14.5), (12.2, 12.8), (12.6, 10.5), (11.8, 4.8)]));
        }
        Icon::Delete => {
            p.add(line(&[(3.0, 4.0), (13.0, 4.0)]));
            p.add(line(&[(6.0, 4.0), (6.5, 2.0), (9.5, 2.0), (10.0, 4.0)]));
            p.add(line(&[(4.0, 4.0), (5.0, 14.0), (11.0, 14.0), (12.0, 4.0)]));
            p.add(line(&[(6.8, 6.5), (7.0, 12.0)]));
            p.add(line(&[(9.2, 6.5), (9.0, 12.0)]));
        }
        Icon::Select => {
            p.add(Shape::convex_polygon(vec![at(4.0, 2.0), at(4.0, 13.0), at(7.0, 10.0), at(11.5, 10.0)], col, Stroke::NONE));
            p.add(line(&[(7.0, 10.0), (9.5, 14.5)]));
        }
        Icon::Wire => {
            p.add(line(&[(2.0, 12.0), (8.0, 12.0), (8.0, 4.0), (14.0, 4.0)]));
            p.circle_filled(at(2.0, 12.0), 1.6 * u, col);
            p.circle_filled(at(14.0, 4.0), 1.6 * u, col);
        }
        Icon::Fit => {
            for (x, y, dx, dy) in [(2.0, 2.0, 1.0, 1.0), (14.0, 2.0, -1.0, 1.0), (2.0, 14.0, 1.0, -1.0), (14.0, 14.0, -1.0, -1.0)] {
                p.add(line(&[(x + 4.0 * dx, y), (x, y), (x, y + 4.0 * dy)]));
            }
            p.rect_stroke(Rect::from_min_max(at(5.5, 5.5), at(10.5, 10.5)), 0.0, Stroke::new(1.0, col), egui::StrokeKind::Middle);
        }
        Icon::Run => {
            p.add(Shape::convex_polygon(vec![at(4.0, 2.5), at(4.0, 13.5), at(13.5, 8.0)], col, Stroke::NONE));
        }
        Icon::Stop => {
            p.rect_filled(Rect::from_min_max(at(3.5, 3.5), at(12.5, 12.5)), 1.5, col);
        }
        Icon::Clock => {
            p.circle_stroke(c, 6.0 * u, st);
            p.add(line(&[(8.0, 4.0), (8.0, 8.0), (11.0, 10.0)]));
        }
        Icon::Wave => {
            p.add(line(&[(2.0, 2.0), (2.0, 14.0), (14.5, 14.0)]));
            let pts: Vec<Pos2> = (0..=24).map(|i| {
                let x = 3.0 + 11.0 * i as f32 / 24.0;
                let y = 8.0 - 4.0 * (x * 0.9).sin();
                at(x, y)
            }).collect();
            p.add(Shape::line(pts, st));
        }
        Icon::ZoomIn | Icon::ZoomOut | Icon::Search => {
            p.circle_stroke(at(6.5, 6.5), 4.5 * u, st);
            p.add(Shape::line(vec![at(9.8, 9.8), at(14.0, 14.0)], Stroke::new(2.2, col)));
            if icon != Icon::Search {
                p.add(line(&[(4.5, 6.5), (8.5, 6.5)]));
                if icon == Icon::ZoomIn {
                    p.add(line(&[(6.5, 4.5), (6.5, 8.5)]));
                }
            }
        }
        Icon::Cursor => {
            p.add(Shape::dashed_line(&[at(5.0, 1.5), at(5.0, 14.5)], Stroke::new(1.2, col), 2.0 * u, 1.5 * u));
            p.add(Shape::dashed_line(&[at(11.0, 1.5), at(11.0, 14.5)], Stroke::new(1.2, col), 2.0 * u, 1.5 * u));
            p.add(line(&[(5.0, 8.0), (11.0, 8.0)]));
        }
        Icon::Pan => {
            p.add(line(&[(8.0, 1.5), (8.0, 14.5)]));
            p.add(line(&[(1.5, 8.0), (14.5, 8.0)]));
            for (a, b, cc) in [((8.0, 1.5), (6.0, 3.5), (10.0, 3.5)), ((8.0, 14.5), (6.0, 12.5), (10.0, 12.5)), ((1.5, 8.0), (3.5, 6.0), (3.5, 10.0)), ((14.5, 8.0), (12.5, 6.0), (12.5, 10.0))] {
                p.add(Shape::convex_polygon(vec![at(a.0, a.1), at(b.0, b.1), at(cc.0, cc.1)], col, Stroke::NONE));
            }
        }
        Icon::BoxZoom => {
            let pts = [at(2.5, 2.5), at(13.5, 2.5), at(13.5, 13.5), at(2.5, 13.5), at(2.5, 2.5)];
            p.add(Shape::dashed_line(&pts, Stroke::new(1.2, col), 2.0 * u, 1.5 * u));
            p.add(line(&[(6.0, 8.0), (10.0, 8.0)]));
            p.add(line(&[(8.0, 6.0), (8.0, 10.0)]));
        }
        Icon::XZoom => {
            p.add(line(&[(2.0, 8.0), (14.0, 8.0)]));
            p.add(Shape::convex_polygon(vec![at(1.5, 8.0), at(4.5, 5.5), at(4.5, 10.5)], col, Stroke::NONE));
            p.add(Shape::convex_polygon(vec![at(14.5, 8.0), at(11.5, 5.5), at(11.5, 10.5)], col, Stroke::NONE));
            p.add(line(&[(1.5, 3.0), (1.5, 13.0)]));
            p.add(line(&[(14.5, 3.0), (14.5, 13.0)]));
        }
        Icon::Export => {
            p.add(line(&[(8.0, 2.0), (8.0, 10.0)]));
            p.add(Shape::convex_polygon(vec![at(8.0, 11.5), at(4.5, 7.5), at(11.5, 7.5)], col, Stroke::NONE));
            p.add(line(&[(2.5, 10.0), (2.5, 14.0), (13.5, 14.0), (13.5, 10.0)]));
        }
        Icon::Plus | Icon::Minus => {
            p.rect_stroke(Rect::from_min_max(at(2.0, 3.0), at(14.0, 13.0)), 1.5, Stroke::new(1.2, col), egui::StrokeKind::Middle);
            p.add(line(&[(5.5, 8.0), (10.5, 8.0)]));
            if icon == Icon::Plus {
                p.add(line(&[(8.0, 5.5), (8.0, 10.5)]));
            }
        }
    }
}

/// toolbar button with an icon (and optional text)
pub fn button(ui: &mut Ui, icon: Icon, text: &str, tip: &str, enabled: bool, selected: bool, col: Option<Color32>) -> Response {
    let font = egui::FontId::proportional(13.0);
    let tw = if text.is_empty() { 0.0 } else { ui.painter().layout_no_wrap(text.to_string(), font.clone(), Color32::WHITE).size().x + 6.0 };
    let (rect, resp) = ui.allocate_exact_size(vec2(28.0 + tw, 26.0), if enabled { Sense::click() } else { Sense::hover() });
    let vis = ui.visuals();
    if selected {
        ui.painter().rect_filled(rect, 4.0, vis.selection.bg_fill.gamma_multiply(0.6));
    } else if enabled && resp.hovered() {
        ui.painter().rect_filled(rect, 4.0, vis.widgets.hovered.weak_bg_fill);
    }
    let base = col.unwrap_or(vis.text_color());
    let c = if enabled { base } else { base.gamma_multiply(0.35) };
    let ir = Rect::from_min_size(rect.min + vec2(5.0, 4.0), vec2(18.0, 18.0));
    paint(ui, ir, icon, c);
    if !text.is_empty() {
        ui.painter().text(pos2(ir.max.x + 4.0, rect.center().y), egui::Align2::LEFT_CENTER, text, font, if enabled { vis.text_color() } else { vis.weak_text_color() });
    }
    resp.on_hover_text(tip)
}
