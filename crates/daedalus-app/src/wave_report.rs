//! Waveform report as an Excel workbook: the first screen of the waveform viewer as a native chart, its
//! measurements and the cursor readings as numbers (data on a second sheet). The PDF version is painted by the
//! viewer itself (wave.rs), so it looks like the screen.

use crate::settings::WaveStyle;
use daedalus_core::runner::TranResult;
use egui::Color32;
use rust_xlsxwriter::{Chart, ChartAxisCrossing, ChartAxisLabelPosition, ChartFont, ChartLegendPosition, ChartLine, ChartLineDashType, ChartType, Color, Format, FormatBorder, Workbook, XlsxError};

pub struct Curve {
    pub name: String,
    pub color: Color32,
    /// (t, value) over the shown time range, decimated
    pub pts: Vec<(f64, f64)>,
    /// max, min, avg, rms, p-p, integral over the measuring range
    pub stats: [f64; 6],
    pub at_c1: Option<f64>,
    pub at_c2: Option<f64>,
}

pub struct Report {
    pub title: String,
    pub x: (f64, f64),
    pub y: (f64, f64),
    pub curves: Vec<Curve>,
    /// "目前顯示範圍（5 ms）" or "游標 C1–C2 之間（…）"
    pub meas_desc: String,
    /// cursor mode on: (C1, C2)
    pub cursors: Option<(Option<f64>, Option<f64>)>,
    pub cursor_cols: (Color32, Color32),
    pub style: WaveStyle,
}

pub const MEAS_HEAD: [&str; 6] = ["Max", "Min", "Avg", "RMS", "P-P", "∫dt"];

/// Points of one signal over `x`, about 2 × `buckets` at most: the minimum and maximum of each bucket in time
/// order, so narrow spikes survive.
pub fn decimate(r: &TranResult, ci: usize, x: (f64, f64), buckets: usize) -> Vec<(f64, f64)> {
    let d = match r.data.get(ci) {
        Some(d) => d,
        None => return vec![],
    };
    let n = d.len().min(r.n);
    if n == 0 || !(x.1 >= x.0) || buckets == 0 {
        return vec![];
    }
    let i0 = (((x.0 - r.t0) / r.dt).floor().max(0.0) as usize).min(n - 1);
    let i1 = (((x.1 - r.t0) / r.dt).ceil().max(0.0) as usize).min(n - 1);
    if i1 < i0 {
        return vec![];
    }
    let t = |i: usize| r.t0 + i as f64 * r.dt;
    let cnt = i1 - i0 + 1;
    if cnt <= 2 * buckets {
        return (i0..=i1).filter(|&i| d[i].is_finite()).map(|i| (t(i), d[i])).collect();
    }
    let per = cnt as f64 / buckets as f64;
    let mut out = Vec::with_capacity(2 * buckets);
    for b in 0..buckets {
        let a = i0 + (b as f64 * per) as usize;
        let e = (i0 + ((b + 1) as f64 * per) as usize).min(i1 + 1);
        let (mut imn, mut imx) = (usize::MAX, usize::MAX);
        for i in a..e {
            let v = d[i];
            if !v.is_finite() {
                continue;
            }
            if imn == usize::MAX || v < d[imn] {
                imn = i;
            }
            if imx == usize::MAX || v > d[imx] {
                imx = i;
            }
        }
        if imn == usize::MAX {
            continue;
        }
        let (p, q) = if imn <= imx { (imn, imx) } else { (imx, imn) };
        out.push((t(p), d[p]));
        if q != p {
            out.push((t(q), d[q]));
        }
    }
    out
}

const FONT: &str = "Microsoft JhengHei";
const DATA: &str = "波形資料";
/// numbers in engineering notation (exponent a multiple of 3)
const NUM: &str = "##0.000E+0";

fn xcolor(c: Color32) -> Color {
    Color::RGB(((c.r() as u32) << 16) | ((c.g() as u32) << 8) | c.b() as u32)
}

fn chart_font(size: f32) -> ChartFont {
    let mut f = ChartFont::new();
    f.set_size(size).set_name(FONT);
    f
}

pub fn xlsx(d: &Report) -> Result<Vec<u8>, String> {
    let e = |x: XlsxError| format!("Excel 產生失敗：{}", x);
    let st = d.style;
    let mut wb = Workbook::new();
    wb.add_worksheet().set_name("報告").map_err(e)?;
    wb.add_worksheet().set_name(DATA).map_err(e)?;
    let nc = d.curves.len();

    // data sheet: time and value columns for every curve, then the two points of each cursor line
    {
        let ws = wb.worksheet_from_index(1).map_err(e)?;
        let head = Format::new().set_bold().set_font_name(FONT);
        for (k, c) in d.curves.iter().enumerate() {
            let col = (2 * k) as u16;
            ws.write_string_with_format(0, col, "t (s)", &head).map_err(e)?;
            ws.write_string_with_format(0, col + 1, c.name.as_str(), &head).map_err(e)?;
            for (i, &(t, v)) in c.pts.iter().enumerate() {
                ws.write_number(i as u32 + 1, col, t).map_err(e)?;
                ws.write_number(i as u32 + 1, col + 1, v).map_err(e)?;
            }
        }
        if let Some((c1, c2)) = d.cursors {
            for (j, (lab, c)) in [("C1", c1), ("C2", c2)].into_iter().enumerate() {
                if let Some(t) = c {
                    let col = (2 * (nc + j)) as u16;
                    ws.write_string_with_format(0, col, "t (s)", &head).map_err(e)?;
                    ws.write_string_with_format(0, col + 1, lab, &head).map_err(e)?;
                    ws.write_number(1, col, t).map_err(e)?;
                    ws.write_number(1, col + 1, d.y.0).map_err(e)?;
                    ws.write_number(2, col, t).map_err(e)?;
                    ws.write_number(2, col + 1, d.y.1).map_err(e)?;
                }
            }
        }
    }

    // the first waveform screen as a scatter chart with straight lines
    let mut ch = Chart::new(ChartType::ScatterStraight);
    for (k, c) in d.curves.iter().enumerate() {
        if c.pts.is_empty() {
            continue;
        }
        let col = (2 * k) as u16;
        let last = c.pts.len() as u32;
        ch.add_series()
            .set_name((DATA, 0u32, col + 1))
            .set_categories((DATA, 1u32, col, last, col))
            .set_values((DATA, 1u32, col + 1, last, col + 1))
            .set_format(ChartLine::new().set_color(xcolor(c.color)).set_width(st.line_w * 0.75));
    }
    if let Some((c1, c2)) = d.cursors {
        for (j, c, cc) in [(0usize, c1, d.cursor_cols.0), (1usize, c2, d.cursor_cols.1)] {
            if c.is_some() {
                let col = (2 * (nc + j)) as u16;
                ch.add_series()
                    .set_name((DATA, 0u32, col + 1))
                    .set_categories((DATA, 1u32, col, 2u32, col))
                    .set_values((DATA, 1u32, col + 1, 2u32, col + 1))
                    .set_format(ChartLine::new().set_color(xcolor(cc)).set_width(1.0).set_dash_type(ChartLineDashType::Dash));
            }
        }
    }
    ch.title().set_hidden();
    ch.legend().set_position(ChartLegendPosition::Top).set_font(&chart_font(st.plot_font));
    ch.x_axis()
        .set_name("時間 (s)")
        .set_name_font(&chart_font(st.axis_font))
        .set_font(&chart_font(st.axis_font))
        .set_min(d.x.0)
        .set_max(d.x.1)
        .set_major_gridlines(true)
        .set_crossing(ChartAxisCrossing::Min)
        .set_label_position(ChartAxisLabelPosition::Low);
    ch.y_axis().set_font(&chart_font(st.axis_font)).set_min(d.y.0).set_max(d.y.1).set_crossing(ChartAxisCrossing::Min).set_label_position(ChartAxisLabelPosition::Low);
    ch.set_width(760).set_height(420);

    // report sheet: title, chart, measurements, cursor readings
    let ws = wb.worksheet_from_index(0).map_err(e)?;
    let base = Format::new().set_font_name(FONT).set_font_size(st.side_font);
    let bold = base.clone().set_bold();
    let head = bold.clone().set_background_color(Color::RGB(0xEEF1F5)).set_border_bottom(FormatBorder::Thin);
    let num = base.clone().set_num_format(NUM);
    ws.set_column_width(0, 22).map_err(e)?;
    for c in 1..=6u16 {
        ws.set_column_width(c, 14).map_err(e)?;
    }
    ws.write_string_with_format(0, 0, format!("{} 波形報告", d.title), &bold.clone().set_font_size(st.side_font + 5.0)).map_err(e)?;
    ws.write_string_with_format(1, 0, format!("DaedalusEngine {}　·　時間範圍 {} – {}", daedalus_core::VERSION, crate::plot::si(d.x.0, "s", 5), crate::plot::si(d.x.1, "s", 5)), &base.clone().set_font_color(Color::RGB(0x5D6877))).map_err(e)?;
    ws.insert_chart(3, 0, &ch).map_err(e)?;
    // 420 px of chart over rows of 20 px
    let mut r: u32 = 3 + 22;
    ws.write_string_with_format(r, 0, format!("量測：{}", d.meas_desc), &bold).map_err(e)?;
    r += 1;
    ws.write_string_with_format(r, 0, "訊號", &head).map_err(e)?;
    for (k, h) in MEAS_HEAD.iter().enumerate() {
        ws.write_string_with_format(r, k as u16 + 1, *h, &head).map_err(e)?;
    }
    r += 1;
    for c in &d.curves {
        ws.write_string_with_format(r, 0, c.name.as_str(), &base.clone().set_font_color(xcolor(c.color))).map_err(e)?;
        for (k, v) in c.stats.iter().enumerate() {
            if v.is_finite() {
                ws.write_number_with_format(r, k as u16 + 1, *v, &num).map_err(e)?;
            }
        }
        r += 1;
    }
    r += 1;
    ws.write_string_with_format(r, 0, "游標讀值", &bold).map_err(e)?;
    r += 1;
    match d.cursors {
        None => {
            ws.write_string_with_format(r, 0, "未開啟游標", &base).map_err(e)?;
        }
        Some((c1, c2)) => {
            let mut rows: Vec<(&str, f64)> = vec![];
            if let Some(a) = c1 {
                rows.push(("C1 (s)", a));
            }
            if let Some(b) = c2 {
                rows.push(("C2 (s)", b));
            }
            if let (Some(a), Some(b)) = (c1, c2) {
                rows.push(("Δt (s)", b - a));
                if a != b {
                    rows.push(("1/Δt (Hz)", 1.0 / (b - a).abs()));
                }
            }
            for (lab, v) in rows {
                ws.write_string_with_format(r, 0, lab, &base).map_err(e)?;
                ws.write_number_with_format(r, 1, v, &num).map_err(e)?;
                r += 1;
            }
            r += 1;
            ws.write_string_with_format(r, 0, "訊號", &head).map_err(e)?;
            for (k, h) in ["@C1", "@C2", "Δ"].iter().enumerate() {
                ws.write_string_with_format(r, k as u16 + 1, *h, &head).map_err(e)?;
            }
            r += 1;
            for c in &d.curves {
                ws.write_string_with_format(r, 0, c.name.as_str(), &base.clone().set_font_color(xcolor(c.color))).map_err(e)?;
                let delta = match (c.at_c1, c.at_c2) {
                    (Some(a), Some(b)) => Some(b - a),
                    _ => None,
                };
                for (k, v) in [c.at_c1, c.at_c2, delta].into_iter().enumerate() {
                    if let Some(v) = v.filter(|v| v.is_finite()) {
                        ws.write_number_with_format(r, k as u16 + 1, v, &num).map_err(e)?;
                    }
                }
                r += 1;
            }
        }
    }
    wb.save_to_buffer().map_err(e)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn result() -> TranResult {
        let n = 20001;
        let dt = 1e-6;
        let mut r = TranResult::default();
        r.t0 = 0.0;
        r.dt = dt;
        r.n = n;
        r.names = vec!["Vout".into(), "I(L1)".into()];
        r.data = vec![(0..n).map(|i| (i as f64 * dt * 2000.0 * std::f64::consts::PI).sin()).collect(), (0..n).map(|i| if i == 7777 { 5.0 } else { 0.1 }).collect()];
        r
    }

    #[test]
    fn decimation_keeps_spikes_and_bounds() {
        let r = result();
        let p = decimate(&r, 1, (0.0, 0.02), 500);
        assert!(p.len() <= 1000 && p.len() > 400, "{} points", p.len());
        assert!(p.iter().any(|&(_, v)| v == 5.0), "spike lost");
        assert!(p.windows(2).all(|w| w[0].0 <= w[1].0), "not in time order");
        // a short range: every sample, covering the whole range (one sample beyond an edge at most)
        let q = decimate(&r, 0, (0.0, 1e-5), 500);
        assert!((11..=12).contains(&q.len()) && q[0].0 == 0.0 && q[q.len() - 1].0 >= 1e-5 - 1e-12, "{} points", q.len());
    }

    #[test]
    fn workbook_has_report_chart_and_data() {
        let r = result();
        let x = (0.0, 0.02);
        let curves = (0..2)
            .map(|ci| Curve { name: r.names[ci].clone(), color: Color32::from_rgb(0x1c, 0x6f, 0xd1), pts: decimate(&r, ci, x, 2000), stats: [1.0, -1.0, 0.0, 0.707, 2.0, 0.0], at_c1: Some(0.5), at_c2: Some(-0.5) })
            .collect();
        let rep = Report { title: "測試".into(), x, y: (-1.2, 5.2), curves, meas_desc: "目前顯示範圍（20 ms）".into(), cursors: Some((Some(0.001), Some(0.004))), cursor_cols: (Color32::RED, Color32::GREEN), style: WaveStyle::default() };
        let b = xlsx(&rep).expect("xlsx");
        assert!(b.starts_with(b"PK"), "not a zip");
        let has = |s: &str| b.windows(s.len()).any(|w| w == s.as_bytes());
        assert!(has("xl/charts/chart1.xml") && has("xl/worksheets/sheet2.xml"), "chart or data sheet missing");
    }
}
