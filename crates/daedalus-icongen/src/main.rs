//! icongen: assets/icon.svg (+ icon_small.svg for <= 32 px) -> assets/icon.ico, icon_256.png, icon_1024.png
use resvg::{tiny_skia, usvg};
use std::path::Path;

fn render(svg: &str, size: u32) -> tiny_skia::Pixmap {
    let opt = usvg::Options::default();
    let tree = usvg::Tree::from_str(svg, &opt).expect("svg parse");
    let mut pm = tiny_skia::Pixmap::new(size, size).unwrap();
    let s = size as f32 / tree.size().width();
    resvg::render(&tree, tiny_skia::Transform::from_scale(s, s), &mut pm.as_mut());
    pm
}
fn rgba(pm: &tiny_skia::Pixmap) -> Vec<u8> {
    // tiny-skia stores premultiplied RGBA
    let mut v = Vec::with_capacity(pm.data().len());
    for px in pm.pixels() {
        let c = px.demultiply();
        v.extend_from_slice(&[c.red(), c.green(), c.blue(), c.alpha()]);
    }
    v
}
/// preview sheet: one column per SVG — 256 px, then 64/32/16 px on light and on dark backgrounds
fn sheet(out: &str, svgs: &[String]) {
    let (colw, h) = (300u32, 520u32);
    let mut pm = tiny_skia::Pixmap::new(colw * svgs.len() as u32, h).unwrap();
    pm.fill(tiny_skia::Color::from_rgba8(250, 250, 252, 255));
    let mut paint = tiny_skia::Paint::default();
    for (i, f) in svgs.iter().enumerate() {
        let svg = std::fs::read_to_string(f).unwrap();
        let x0 = (i as u32 * colw) as i32;
        let big = render(&svg, 256);
        pm.draw_pixmap(x0 + 22, 20, big.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
        for (row, bg) in [(0, (243u8, 244u8, 246u8)), (1, (31, 41, 55))] {
            let y = 300 + row * 100;
            paint.set_color_rgba8(bg.0, bg.1, bg.2, 255);
            pm.fill_rect(tiny_skia::Rect::from_xywh(x0 as f32 + 22.0, y as f32, 256.0, 88.0).unwrap(), &paint, tiny_skia::Transform::identity(), None);
            let mut x = x0 + 34;
            for s in [64u32, 32, 16] {
                let small = render(&svg, s);
                pm.draw_pixmap(x, y + (88 - s as i32) / 2, small.as_ref(), &tiny_skia::PixmapPaint::default(), tiny_skia::Transform::identity(), None);
                x += s as i32 + 28;
            }
        }
        // option number as dots under the column (1–5)
        paint.set_color_rgba8(55, 65, 81, 255);
        for d in 0..=i {
            let cx = x0 as f32 + 150.0 - (i as f32) * 9.0 + d as f32 * 18.0;
            let path = tiny_skia::PathBuilder::from_circle(cx, 502.0, 6.0).unwrap();
            pm.fill_path(&path, &paint, tiny_skia::FillRule::Winding, tiny_skia::Transform::identity(), None);
        }
    }
    pm.save_png(out).unwrap();
    println!("sheet written to {}", out);
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    if args.first().map(|a| a == "--sheet").unwrap_or(false) {
        sheet(&args[1], &args[2..]);
        return;
    }
    let dir = std::env::args().nth(1).unwrap_or_else(|| "assets".into());
    let dir = Path::new(&dir);
    let big = std::fs::read_to_string(dir.join("icon.svg")).unwrap();
    let small = std::fs::read_to_string(dir.join("icon_small.svg")).unwrap();
    let mut icon = ico::IconDir::new(ico::ResourceType::Icon);
    for &s in &[16u32, 20, 24, 32, 40, 48, 64, 96, 128, 256] {
        let pm = render(if s <= 32 { &small } else { &big }, s);
        let img = ico::IconImage::from_rgba_data(s, s, rgba(&pm));
        icon.add_entry(ico::IconDirEntry::encode(&img).unwrap());
        if s == 256 {
            pm.save_png(dir.join("icon_256.png")).unwrap();
        }
        if s == 32 {
            pm.save_png(dir.join("icon_32.png")).unwrap();
        }
    }
    icon.write(std::fs::File::create(dir.join("icon.ico")).unwrap()).unwrap();
    render(&big, 1024).save_png(dir.join("icon_1024.png")).unwrap();
    println!("icon.ico, icon_256.png, icon_32.png, icon_1024.png written to {}", dir.display());
}
