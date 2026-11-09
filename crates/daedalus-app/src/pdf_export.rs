//! Vector PDF output of what egui paints. Drawing code paints with an ordinary egui `Painter` on an off-screen
//! layer; the shapes are taken out of that layer (so nothing appears on screen) and written as PDF paths and
//! text, with the Windows fonts embedded as subsets. A printout therefore looks like the screen, uses the same
//! drawing code and follows the same font and line-width settings.

use egui::epaint::{ClippedShape, ColorMode, Shape, TextShape};
use egui::{Color32, Pos2, Rect, Vec2};
use krilla::color::rgb;
use krilla::geom::{Path, PathBuilder, Point, Transform};
use krilla::num::NormalizedF32;
use krilla::page::PageSettings;
use krilla::paint::{Fill, FillRule, LineCap, LineJoin, Stroke};
use krilla::surface::Surface;
use krilla::text::{Font, GlyphId, KrillaGlyph};
use krilla::Document;
use skrifa::MetadataProvider;
use std::collections::HashMap;
use std::sync::Arc;

/// A4 portrait in PDF points
pub const A4: (f32, f32) = (595.28, 841.89);

/// Paints on an off-screen layer of the current viewport; `finish` takes the shapes out.
pub struct Capture {
    ctx: egui::Context,
    layer: egui::LayerId,
    pub painter: egui::Painter,
}

impl Capture {
    pub fn new(ctx: &egui::Context, key: impl std::hash::Hash + std::fmt::Debug, area: Rect) -> Self {
        let layer = egui::LayerId::new(egui::Order::Background, egui::Id::new(("pdf_capture", key)));
        ctx.graphics_mut(|g| {
            if let Some(l) = g.get_mut(layer) {
                *l = Default::default();
            }
        });
        Capture { ctx: ctx.clone(), layer, painter: egui::Painter::new(ctx.clone(), layer, area) }
    }
    pub fn finish(self) -> Vec<ClippedShape> {
        let list = self.ctx.graphics_mut(|g| g.get_mut(self.layer).map(std::mem::take));
        list.map(|l| l.all_entries().cloned().collect()).unwrap_or_default()
    }
}

/// shapes placed on a page: page point = offset + scale × shape point
pub struct Part {
    pub shapes: Vec<ClippedShape>,
    pub scale: f32,
    pub offset: Vec2,
}

pub struct Page {
    pub size: (f32, f32),
    pub parts: Vec<Part>,
}

/// Fonts embedded in the PDF, as on screen: the CJK UI font first, then Latin / symbol fallbacks for glyphs it
/// lacks; monospace text (tick labels, measured values) prefers Consolas.
struct FontSet {
    fonts: Vec<(Font, Arc<Vec<u8>>)>,
    /// index of the monospace font in `fonts`
    mono: Option<usize>,
    cache: HashMap<(char, bool), (usize, u32)>,
}

impl FontSet {
    fn load() -> Result<Self, String> {
        let dir = std::path::Path::new(&std::env::var("WINDIR").unwrap_or_else(|_| r"C:\Windows".to_string())).join("Fonts");
        let mut fonts: Vec<(Font, Arc<Vec<u8>>)> = Vec::new();
        let mut add = |name: &str| -> bool {
            if let Ok(b) = std::fs::read(dir.join(name)) {
                let data = Arc::new(b);
                if let Some(f) = Font::new(data.clone().into(), 0) {
                    fonts.push((f, data));
                    return true;
                }
            }
            false
        };
        for n in ["msjh.ttc", "msjhl.ttc", "msyh.ttc", "mingliu.ttc", "simsun.ttc"] {
            if add(n) {
                break;
            }
        }
        for n in ["segoeui.ttf", "seguisym.ttf", "arial.ttf"] {
            add(n);
        }
        let mono = if add("consola.ttf") { Some(fonts.len() - 1) } else { None };
        if fonts.is_empty() {
            return Err("找不到可嵌入 PDF 的字型（Windows\\Fonts）".into());
        }
        Ok(FontSet { fonts, mono, cache: HashMap::new() })
    }
    fn lookup(&self, i: usize, c: char) -> Option<u32> {
        let f = skrifa::FontRef::from_index(self.fonts[i].1.as_slice(), 0).ok()?;
        f.charmap().map(c).map(|g| g.to_u32()).filter(|&g| g != 0)
    }
    /// (font index, glyph id) for a character: the monospace font first for monospace text, then the first
    /// font that has it
    fn glyph(&mut self, c: char, mono: bool) -> (usize, u32) {
        if let Some(&v) = self.cache.get(&(c, mono)) {
            return v;
        }
        let first = if mono { self.mono } else { None };
        let found = first
            .into_iter()
            .chain(0..self.fonts.len())
            .find_map(|i| self.lookup(i, c).map(|g| (i, g)))
            .unwrap_or((0, 0));
        self.cache.insert((c, mono), found);
        found
    }
}

/// the pages as a PDF file
pub fn write_pdf(pages: &[Page]) -> Result<Vec<u8>, String> {
    let mut fonts = FontSet::load()?;
    let mut doc = Document::new();
    for pg in pages {
        let settings = PageSettings::from_wh(pg.size.0, pg.size.1).ok_or_else(|| "頁面尺寸錯誤".to_string())?;
        let mut page = doc.start_page_with(settings);
        let mut s = page.surface();
        for part in &pg.parts {
            s.push_transform(&Transform::from_row(part.scale, 0.0, 0.0, part.scale, part.offset.x, part.offset.y));
            draw_shapes(&mut s, &part.shapes, &mut fonts);
            s.pop();
        }
        s.finish();
        page.finish();
    }
    doc.finish().map_err(|e| format!("PDF 產生失敗：{:?}", e))
}

fn draw_shapes(s: &mut Surface, shapes: &[ClippedShape], fonts: &mut FontSet) {
    // consecutive shapes with the same clip rectangle share one clip path
    let mut cur: Option<Rect> = None;
    let mut pushed = false;
    let mut skip = false;
    for cs in shapes {
        if cur != Some(cs.clip_rect) {
            if pushed {
                s.pop();
            }
            cur = Some(cs.clip_rect);
            let big = Rect::from_min_max(Pos2::new(-1e6, -1e6), Pos2::new(1e6, 1e6));
            let r = cs.clip_rect.intersect(big);
            match krilla::geom::Rect::from_ltrb(r.min.x, r.min.y, r.max.x, r.max.y) {
                Some(kr) if r.width() > 0.0 && r.height() > 0.0 => {
                    let mut pb = PathBuilder::new();
                    pb.push_rect(kr);
                    match pb.finish() {
                        Some(p) => {
                            s.push_clip_path(&p, &FillRule::NonZero);
                            pushed = true;
                            skip = false;
                        }
                        None => {
                            pushed = false;
                            skip = true;
                        }
                    }
                }
                _ => {
                    pushed = false;
                    skip = true;
                }
            }
        }
        if !skip {
            shape(s, &cs.shape, fonts);
        }
    }
    if pushed {
        s.pop();
    }
}

fn rgba(c: Color32) -> (rgb::Color, NormalizedF32, bool) {
    let [r, g, b, a] = c.to_srgba_unmultiplied();
    (rgb::Color::new(r, g, b), NormalizedF32::new(a as f32 / 255.0).unwrap_or(NormalizedF32::ONE), a > 0)
}

fn fill(s: &mut Surface, p: &Path, c: Color32) {
    let (col, op, vis) = rgba(c);
    if !vis {
        return;
    }
    s.set_stroke(None);
    s.set_fill(Some(Fill { paint: col.into(), opacity: op, rule: FillRule::NonZero }));
    s.draw_path(p);
}

fn stroke(s: &mut Surface, p: &Path, width: f32, c: Color32) {
    let (col, op, vis) = rgba(c);
    if !vis || !(width > 0.0) {
        return;
    }
    s.set_fill(None);
    s.set_stroke(Some(Stroke { paint: col.into(), width, line_cap: LineCap::Round, line_join: LineJoin::Round, opacity: op, ..Default::default() }));
    s.draw_path(p);
}

fn poly(pts: &[Pos2], closed: bool) -> Option<Path> {
    if pts.len() < 2 {
        return None;
    }
    let mut pb = PathBuilder::new();
    pb.move_to(pts[0].x, pts[0].y);
    for p in &pts[1..] {
        pb.line_to(p.x, p.y);
    }
    if closed {
        pb.close();
    }
    pb.finish()
}

fn ellipse_path(c: Pos2, r: Vec2) -> Option<Path> {
    if !(r.x > 0.0 && r.y > 0.0) {
        return None;
    }
    const K: f32 = 0.552_284_8;
    let (kx, ky) = (r.x * K, r.y * K);
    let mut pb = PathBuilder::new();
    pb.move_to(c.x + r.x, c.y);
    pb.cubic_to(c.x + r.x, c.y + ky, c.x + kx, c.y + r.y, c.x, c.y + r.y);
    pb.cubic_to(c.x - kx, c.y + r.y, c.x - r.x, c.y + ky, c.x - r.x, c.y);
    pb.cubic_to(c.x - r.x, c.y - ky, c.x - kx, c.y - r.y, c.x, c.y - r.y);
    pb.cubic_to(c.x + kx, c.y - r.y, c.x + r.x, c.y - ky, c.x + r.x, c.y);
    pb.close();
    pb.finish()
}

fn rect_path(r: Rect) -> Option<Path> {
    let kr = krilla::geom::Rect::from_ltrb(r.min.x, r.min.y, r.max.x, r.max.y)?;
    let mut pb = PathBuilder::new();
    pb.push_rect(kr);
    pb.finish()
}

fn path_stroke(s: &mut Surface, p: &Path, st: &egui::epaint::PathStroke) {
    if let ColorMode::Solid(c) = st.color {
        stroke(s, p, st.width, c);
    }
}

fn shape(s: &mut Surface, sh: &Shape, fonts: &mut FontSet) {
    match sh {
        Shape::Noop | Shape::Band(_) | Shape::Callback(_) => {}
        Shape::Vec(v) => {
            for x in v {
                shape(s, x, fonts);
            }
        }
        Shape::LineSegment { points, stroke: st } => {
            if let Some(p) = poly(points, false) {
                stroke(s, &p, st.width, st.color);
            }
        }
        Shape::Path(ps) => {
            if let Some(p) = poly(&ps.points, ps.closed) {
                if ps.closed && ps.points.len() >= 3 {
                    fill(s, &p, ps.fill);
                }
                path_stroke(s, &p, &ps.stroke);
            }
        }
        Shape::Circle(c) => {
            if let Some(p) = ellipse_path(c.center, Vec2::splat(c.radius)) {
                fill(s, &p, c.fill);
                stroke(s, &p, c.stroke.width, c.stroke.color);
            }
        }
        Shape::Ellipse(e) => {
            if let Some(p) = ellipse_path(e.center, e.radius) {
                fill(s, &p, e.fill);
                stroke(s, &p, e.stroke.width, e.stroke.color);
            }
        }
        Shape::Rect(r) => {
            if let Some(p) = rect_path(r.rect) {
                fill(s, &p, r.fill);
            }
            let w = r.stroke.width;
            let sr = match r.stroke_kind {
                egui::StrokeKind::Inside => r.rect.shrink(w / 2.0),
                egui::StrokeKind::Outside => r.rect.expand(w / 2.0),
                egui::StrokeKind::Middle => r.rect,
            };
            if let Some(p) = rect_path(sr) {
                stroke(s, &p, w, r.stroke.color);
            }
        }
        Shape::Text(t) => text(s, t, fonts),
        Shape::Mesh(m) => {
            for tri in m.indices.chunks_exact(3) {
                let v: Vec<&egui::epaint::Vertex> = tri.iter().filter_map(|&i| m.vertices.get(i as usize)).collect();
                if v.len() == 3 {
                    if let Some(p) = poly(&[v[0].pos, v[1].pos, v[2].pos], true) {
                        fill(s, &p, v[0].color);
                    }
                }
            }
        }
        Shape::QuadraticBezier(q) => {
            let [a, b, c] = q.points;
            let mut pb = PathBuilder::new();
            pb.move_to(a.x, a.y);
            pb.quad_to(b.x, b.y, c.x, c.y);
            if q.closed {
                pb.close();
            }
            if let Some(p) = pb.finish() {
                if q.closed {
                    fill(s, &p, q.fill);
                }
                path_stroke(s, &p, &q.stroke);
            }
        }
        Shape::CubicBezier(cb) => {
            let [a, b, c, d] = cb.points;
            let mut pb = PathBuilder::new();
            pb.move_to(a.x, a.y);
            pb.cubic_to(b.x, b.y, c.x, c.y, d.x, d.y);
            if cb.closed {
                pb.close();
            }
            if let Some(p) = pb.finish() {
                if cb.closed {
                    fill(s, &p, cb.fill);
                }
                path_stroke(s, &p, &cb.stroke);
            }
        }
    }
}

/// a run of glyphs on one baseline with the same font, size and colour
struct Run {
    font: usize,
    size: f32,
    col: Color32,
    start: Pos2,
    text: String,
    glyphs: Vec<KrillaGlyph>,
}

fn flush(s: &mut Surface, run: Option<Run>, fonts: &FontSet) {
    let Some(r) = run else { return };
    let (col, op, vis) = rgba(r.col);
    if !vis || r.glyphs.is_empty() {
        return;
    }
    s.set_stroke(None);
    s.set_fill(Some(Fill { paint: col.into(), opacity: op, rule: FillRule::NonZero }));
    s.draw_glyphs(Point::from_xy(r.start.x, r.start.y), &r.glyphs, fonts.fonts[r.font].0.clone(), &r.text, r.size, false);
}

/// Text at exactly the glyph positions egui laid out (so mixed fonts and right-aligned numbers stay in place);
/// each glyph is drawn with the first embedded font that has it.
fn text(s: &mut Surface, t: &TextShape, fonts: &mut FontSet) {
    let g = &t.galley;
    let job = &g.job;
    if job.sections.is_empty() {
        return;
    }
    // section (font size, family) of each glyph: glyphs follow the characters of the text, newlines excepted
    let mut chars = job.text.char_indices().filter(|(_, c)| *c != '\n');
    let section = |byte: usize| &job.sections.iter().find(|sec| sec.byte_range.start.0 <= byte && byte < sec.byte_range.end.0).unwrap_or(&job.sections[0]).format.font_id;
    let rotated = t.angle != 0.0;
    if rotated {
        s.push_transform(&Transform::from_rotate_at(t.angle.to_degrees(), t.pos.x, t.pos.y));
    }
    for placed in &g.rows {
        let row = &placed.row;
        let mut run: Option<Run> = None;
        for (k, gl) in row.glyphs.iter().enumerate() {
            let byte = chars.next().map(|(b, _)| b).unwrap_or(usize::MAX);
            let fid = if byte == usize::MAX { &job.sections[0].format.font_id } else { section(byte) };
            let size = fid.size;
            let mono = fid.family == egui::FontFamily::Monospace;
            if !(size > 0.0) {
                continue;
            }
            let vcol = row.visuals.mesh.vertices.get(gl.first_vertex as usize).map(|v| v.color);
            let mut col = t.override_text_color.or(vcol).unwrap_or(t.fallback_color);
            if col == Color32::PLACEHOLDER {
                col = t.fallback_color;
            }
            let (fi, gid) = fonts.glyph(gl.chr, mono);
            let next_x = row.glyphs.get(k + 1).map(|n| n.pos.x).unwrap_or(gl.pos.x + gl.advance_width);
            let adv = (next_x - gl.pos.x) / size;
            let pos = t.pos + placed.pos.to_vec2() + gl.pos.to_vec2();
            let same = run.as_ref().map(|r| r.font == fi && r.size == size && r.col == col && (r.start.y - pos.y).abs() < 0.01).unwrap_or(false);
            if !same {
                flush(s, run.take(), fonts);
                run = Some(Run { font: fi, size, col, start: pos, text: String::new(), glyphs: vec![] });
            }
            if let Some(r) = run.as_mut() {
                let a = r.text.len();
                r.text.push(gl.chr);
                r.glyphs.push(KrillaGlyph::new(GlyphId::new(gid), adv, 0.0, 0.0, 0.0, a..r.text.len(), None));
            }
        }
        flush(s, run.take(), fonts);
    }
    if rotated {
        s.pop();
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// a headless egui pass: paint a few shapes and text, capture them and write a PDF
    #[test]
    fn captured_shapes_become_a_pdf() {
        let ctx = egui::Context::default();
        let mut shapes = vec![];
        let _ = ctx.run_ui(egui::RawInput::default(), |ui| {
            let cap = Capture::new(ui.ctx(), "test", Rect::from_min_size(Pos2::ZERO, egui::vec2(A4.0, A4.1)));
            let p = &cap.painter;
            p.line_segment([Pos2::new(10.0, 10.0), Pos2::new(200.0, 50.0)], egui::Stroke::new(1.5, Color32::RED));
            p.circle(Pos2::new(100.0, 100.0), 20.0, Color32::LIGHT_BLUE, egui::Stroke::new(1.0, Color32::BLACK));
            p.rect_stroke(Rect::from_min_size(Pos2::new(50.0, 150.0), egui::vec2(100.0, 40.0)), 0.0, egui::Stroke::new(1.0, Color32::GRAY), egui::StrokeKind::Inside);
            p.text(Pos2::new(60.0, 160.0), egui::Align2::LEFT_TOP, "量測 Vout = 1.23 µV ∫dt Δ", egui::FontId::proportional(12.0), Color32::DARK_GRAY);
            p.with_clip_rect(Rect::from_min_size(Pos2::new(0.0, 0.0), egui::vec2(80.0, 80.0))).add(egui::Shape::line(vec![Pos2::new(0.0, 0.0), Pos2::new(300.0, 300.0)], egui::Stroke::new(1.0, Color32::GREEN)));
            shapes = cap.finish();
        });
        assert!(shapes.len() >= 5, "captured {} shapes", shapes.len());
        let pdf = write_pdf(&[Page { size: A4, parts: vec![Part { shapes, scale: 1.0, offset: Vec2::ZERO }] }]).expect("pdf");
        assert!(pdf.starts_with(b"%PDF"), "not a PDF");
        assert!(pdf.len() > 1000);
    }
}
