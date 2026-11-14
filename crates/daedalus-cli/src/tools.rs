//! The MCP tools: definitions (JSON schemas) and implementations. Results are text written for a model to read
//! (compact tables, SI units), plus PNG pictures of schematics and waveforms.
use crate::circuit::{self, analyze, el_name, Analysis, Circuit};
use crate::render;
use crate::run::{self, Outcome, Run};
use daedalus_core::doc::Doc;
use daedalus_core::library::lib;
use daedalus_core::measure::{measure, Meas};
use daedalus_core::runner::{AcCurve, AcResult, TranResult};
use daedalus_core::util::si;
use serde_json::{json, Value};
use std::collections::HashMap;
use std::fmt::Write;
use std::path::{Path, PathBuf};
use std::time::SystemTime;

pub struct Example {
    pub key: &'static str,
    pub name: &'static str,
    pub cat: &'static str,
    pub json: &'static str,
}
include!("../../daedalus-app/src/examples_list.rs");

const GUIDE: &str = include_str!("guide.md");

pub struct Reply {
    pub content: Vec<Value>,
    pub error: bool,
}
impl Reply {
    fn text(s: impl Into<String>) -> Reply {
        Reply { content: vec![json!({"type": "text", "text": s.into()})], error: false }
    }
    pub fn err(s: impl Into<String>) -> Reply {
        Reply { content: vec![json!({"type": "text", "text": s.into()})], error: true }
    }
    fn image(mut self, png: &[u8]) -> Reply {
        self.content.push(json!({"type": "image", "data": render::base64(png), "mimeType": "image/png"}));
        self
    }
}

struct Cached {
    stamp: (Option<SystemTime>, u64),
    ac: bool,
    tt: Option<String>,
    run: Run,
    doc: Doc,
}

pub struct Server {
    cache: HashMap<PathBuf, Cached>,
}

/* ------------------------------------------------------------------ definitions */

fn tool(name: &str, title: &str, desc: &str, schema: Value, read_only: bool) -> Value {
    json!({
        "name": name,
        "title": title,
        "description": desc,
        "inputSchema": schema,
        "annotations": {"title": title, "readOnlyHint": read_only, "destructiveHint": false, "openWorldHint": false}
    })
}

pub fn list() -> Vec<Value> {
    let path = json!({"type": "string", "description": "電路檔 .psimx 的絕對路徑"});
    vec![
        tool(
            "daedalus_guide",
            "使用指引",
            "DaedalusEngine 的使用指引：workflow（流程，第一次使用先讀）、netlist（daedalus_new 的網表格式）、edit（daedalus_edit 的操作）、modeling（建模規則：功率／控制介面、開關、SPICE 模型、步長）、measure（量測定義）、troubleshoot（常見錯誤）。",
            json!({"type": "object", "properties": {"topic": {"type": "string", "enum": ["workflow", "netlist", "edit", "modeling", "measure", "troubleshoot", "all"], "description": "主題，預設 workflow"}}}),
            true,
        ),
        tool(
            "daedalus_elements",
            "查詢元件",
            "查詢元件庫（221 種）：不帶參數列出全部型別代碼；query 以中英文關鍵字搜尋；category 列出一個類別；type 取得一個元件的完整資料（接腳名稱、位置與類型［功率／控制輸入／控制輸出…］、全部參數的預設值、單位、說明與選項）。寫網表或修改參數前先查，不要猜接腳或參數名稱。",
            json!({"type": "object", "properties": {
                "query": {"type": "string", "description": "關鍵字，例如 \"mosfet\"、\"比較器\"、\"電流探棒\""},
                "category": {"type": "string", "description": "類別名稱的一部分，例如 \"Switches\"、\"Sources/Voltage\""},
                "type": {"type": "string", "description": "型別代碼，例如 \"MOSFET\"、\"VSIN\""}
            }}),
            true,
        ),
        tool(
            "daedalus_new",
            "由網表建立電路",
            "由網表建立新的電路檔：每個元件給 type、name、params，pins 以節點名稱指定連線（同名相連，接地寫 \"0\"，不接寫 null）。程式自動排版（功率區、控制區），每隻接腳拉短線到節點標籤或接地符號，並以程式本身的連線規則驗證結果與網表一致。格式見 daedalus_guide topic=netlist。回傳節點表與問題清單。",
            json!({"type": "object", "required": ["path", "netlist"], "properties": {
                "path": path.clone(),
                "netlist": {"type": "object", "description": "{title?, sim?{dt, tt,…}, params?（參數檔文字）, elements[{type, name?, params?, rot?, mir?, pins{接腳: 節點}}], screens?}",
                    "properties": {
                        "title": {"type": "string"},
                        "sim": {"type": "object", "description": "模擬控制參數，例如 {\"dt\": \"0.2u\", \"tt\": \"10m\"}"},
                        "params": {"type": "string", "description": "參數檔，每行 名稱 = 運算式"},
                        "elements": {"type": "array", "items": {"type": "object", "required": ["type"], "properties": {
                            "type": {"type": "string"}, "name": {"type": "string"}, "params": {"type": "object"},
                            "rot": {"type": "integer"}, "mir": {"type": "boolean"}, "pins": {"type": "object"}}}},
                        "screens": {"type": "array", "items": {"type": "array", "items": {"type": "string"}}}
                    }, "required": ["elements"]},
                "overwrite": {"type": "boolean", "description": "檔案已存在時覆寫（預設 false）"},
                "render": {"type": "boolean", "description": "一併回傳電路圖 PNG（預設 false）"}
            }}),
            false,
        ),
        tool(
            "daedalus_inspect",
            "檢查電路",
            "讀取電路檔（包括使用者在程式裡畫的）：元件與非預設參數、每隻接腳接到哪個節點、每個節點連到哪些接腳、模擬設定、問題（未接的接腳、功率與控制直接相連、名稱重複…）與警告。element 指定時列出該元件的全部參數與接腳。修改或模擬前先看這個。",
            json!({"type": "object", "required": ["path"], "properties": {"path": path.clone(), "element": {"type": "string", "description": "只看一個元件的詳細資料"}}}),
            true,
        ),
        tool(
            "daedalus_edit",
            "修改電路",
            "修改電路檔：ops 依序執行 add（新增元件並以節點名稱連接）、set（參數、改名、移動、旋轉、停用）、remove、connect（接腳接到節點或另一隻接腳）、disconnect、wire（兩接腳間拉導線）、sim、params、screens、text。每個操作都以程式的連線規則驗證，不會誤接或斷開其他接腳；任一操作失敗整批不存檔。格式見 daedalus_guide topic=edit。回傳節點變化與問題清單。",
            json!({"type": "object", "required": ["path", "ops"], "properties": {
                "path": path.clone(),
                "ops": {"type": "array", "items": {"type": "object", "required": ["op"], "properties": {"op": {"type": "string", "enum": ["add", "set", "remove", "connect", "disconnect", "wire", "sim", "params", "screens", "text"]}}},
                    "description": "例如 [{\"op\":\"set\",\"name\":\"R1\",\"params\":{\"R\":\"4.7k\"}}, {\"op\":\"add\",\"type\":\"VP\",\"name\":\"Vsw\",\"pins\":{\"n\":\"SW\"}}, {\"op\":\"connect\",\"pin\":\"Q1.G\",\"net\":\"GATE\"}]"},
                "dry_run": {"type": "boolean", "description": "只回報結果，不存檔"},
                "save_as": {"type": "string", "description": "另存的絕對路徑（原檔不變）"},
                "render": {"type": "boolean", "description": "一併回傳修改後的電路圖 PNG"}
            }}),
            false,
        ),
        tool(
            "daedalus_simulate",
            "模擬",
            "以 DaedalusEngine 的模擬核心執行電路（與程式按 F5 相同）：暫態（含參數掃描、蒙地卡羅、高精度模式）或 AC 掃頻。回傳每支探棒在統計區間（預設最後 20 %）的平均、RMS、最小、最大、峰對峰、終值，指定的量測（Freq、DutyP、Rise…），AC 的增益、頻寬、穿越頻率與相位裕度，以及模擬警告。結果會暫存給 daedalus_waveform 使用。",
            json!({"type": "object", "required": ["path"], "properties": {
                "path": path.clone(),
                "analysis": {"type": "string", "enum": ["auto", "transient", "ac"], "description": "auto（預設）：有 ACSWEEP 元件時做 AC 掃頻，否則暫態"},
                "tt": {"type": "string", "description": "暫時改用的總時間，例如 \"5m\"（不改檔案）"},
                "from": {"type": "string", "description": "統計區間起點（時間，例如 \"8m\"）"},
                "to": {"type": "string", "description": "統計區間終點"},
                "measures": {"type": "array", "items": {"type": "string"}, "description": "額外量測，例如 [\"Freq\", \"DutyP\", \"CycRms\", \"Rise\"]（見 topic=measure）"},
                "probes": {"type": "array", "items": {"type": "string"}, "description": "只列這些探棒"},
                "csv": {"type": "string", "description": "把完整波形另存成 CSV 的絕對路徑"},
                "image": {"type": "boolean", "description": "一併回傳波形圖 PNG（預設 false）"},
                "timeout": {"type": "number", "description": "時間上限（秒，預設 120）"}
            }}),
            true,
        ),
        tool(
            "daedalus_waveform",
            "波形",
            "最近一次模擬（檔案改過則重新模擬）的波形：預設回傳波形圖 PNG（依檔案的畫面配置或每支探棒一格；快速切換的部分以上下包絡顯示），table=true 另回傳降取樣的數值表（每點的最小～最大）。from／to 可放大一段時間。",
            json!({"type": "object", "required": ["path"], "properties": {
                "path": path.clone(),
                "probes": {"type": "array", "items": {"type": "string"}},
                "from": {"type": "string", "description": "起點時間（例如 \"9m\"）或頻率"},
                "to": {"type": "string"},
                "image": {"type": "boolean", "description": "回傳波形圖（預設 true）"},
                "table": {"type": "boolean", "description": "回傳數值表（預設 false）"},
                "points": {"type": "integer", "description": "數值表的點數（預設 40）"},
                "width": {"type": "integer", "description": "圖寬像素（預設 1000）"}
            }}),
            true,
        ),
        tool(
            "daedalus_render",
            "電路圖",
            "把電路畫成 PNG（與程式相同的元件符號；功率線深色、控制線藍色、未接的接腳紅圈、邊緣標有座標）。highlight 可標出一個節點（名稱或接腳，例如 \"VO\"、\"Q1.G\"）或元件；pins=true 標出每隻接腳的名稱。save 可另存 .png 或 .svg。用來目視檢查版面與連線。",
            json!({"type": "object", "required": ["path"], "properties": {
                "path": path.clone(),
                "highlight": {"type": "string"},
                "pins": {"type": "boolean"},
                "save": {"type": "string", "description": "另存圖片的絕對路徑（.png 或 .svg）"},
                "max_px": {"type": "integer", "description": "圖片長邊像素（預設 1400）"}
            }}),
            true,
        ),
        tool(
            "daedalus_examples",
            "範例電路",
            "列出程式內建、已驗證的範例電路（Buck、整流、閘流體、控制方塊、濾波、SPICE 電晶體、蒙地卡羅…）；key 加 copy_to 把範例存成檔案，可作為起點或參考建模方式。",
            json!({"type": "object", "properties": {"key": {"type": "string"}, "copy_to": {"type": "string", "description": "存檔的絕對路徑"}}}),
            false,
        ),
        tool(
            "daedalus_open",
            "在程式中開啟",
            "在 DaedalusEngine 視窗中開啟電路檔給使用者看。程式開著時，之後用 MCP 修改並存檔，畫面會自動重新載入。",
            json!({"type": "object", "required": ["path"], "properties": {"path": path}}),
            false,
        ),
    ]
}

/* ------------------------------------------------------------------ helpers */

fn s<'a>(a: &'a Value, k: &str) -> Option<&'a str> {
    a.get(k).and_then(|v| v.as_str()).map(str::trim).filter(|v| !v.is_empty())
}
fn b(a: &Value, k: &str, d: bool) -> bool {
    a.get(k).and_then(|v| v.as_bool()).unwrap_or(d)
}
fn path_arg(a: &Value) -> Result<PathBuf, String> {
    let p = s(a, "path").ok_or("需要 path（電路檔的絕對路徑）")?;
    let p = PathBuf::from(p);
    Ok(std::path::absolute(&p).unwrap_or(p))
}
fn strs(a: &Value, k: &str) -> Vec<String> {
    a.get(k).and_then(|v| v.as_array()).map(|v| v.iter().filter_map(|x| x.as_str().map(String::from)).collect()).unwrap_or_default()
}
fn stamp(p: &Path) -> (Option<SystemTime>, u64) {
    std::fs::metadata(p).map(|m| (m.modified().ok(), m.len())).unwrap_or((None, 0))
}

/// unit of a probe column ("Vo", "Vo [R=1k]")
fn unit_of(doc: &Doc, col: &str) -> String {
    let base = col.split(" [").next().unwrap_or(col);
    let Some(e) = doc.elements.iter().find(|e| e.name == base) else { return String::new() };
    match e.typ.as_str() {
        "VP" | "VP2" | "VDCM" | "VACM" => "V".into(),
        "IP" | "ADCM" | "AACM" => "A".into(),
        "WATT" => "W".into(),
        "WH3" => e.params.get("unit").map(|u| match u.as_str() {
            "1" => "kWh",
            "2" => "J",
            _ => "Wh",
        }).unwrap_or("Wh").into(),
        "DEVP" => match e.params.get("q").map(|s| s.as_str()).unwrap_or("") {
            "vg" | "vd" => "V".into(),
            "id" | "ig" => "A".into(),
            "p" => "W".into(),
            "e" => "J".into(),
            _ => String::new(),
        },
        _ => String::new(),
    }
}

fn eval_time(doc: &Doc, t: &str) -> Result<f64, String> {
    let syms = daedalus_core::netlist::doc_syms(doc);
    daedalus_core::expr::eval_const(t, &syms).map_err(|e| format!("無法解讀「{}」：{}", t, e.0))
}

fn parse_meas(names: &[String]) -> Result<Vec<Meas>, String> {
    let all: Vec<Meas> = Meas::LEVEL.iter().chain(Meas::TIME.iter()).copied().collect();
    names
        .iter()
        .map(|n| {
            let k = n.trim().to_lowercase().replace(['-', ' ', '_'], "");
            all.iter()
                .copied()
                .find(|m| format!("{:?}", m).to_lowercase() == k || (k == "pp" && *m == Meas::Pp) || (k == "duty" && *m == Meas::DutyP) || (k == "width" && *m == Meas::WidthP) || (k == "frequency" && *m == Meas::Freq))
                .ok_or_else(|| format!("不認得的量測「{}」。可用：{}", n, all.iter().map(|m| format!("{:?}", m)).collect::<Vec<_>>().join("、")))
        })
        .collect()
}

fn guide_section(topic: &str) -> String {
    let mut out = String::new();
    let mut on = topic == "all";
    for line in GUIDE.lines() {
        if let Some(t) = line.strip_prefix("## ") {
            on = topic == "all" || t.trim() == topic;
            continue;
        }
        if on {
            out.push_str(line);
            out.push('\n');
        }
    }
    out
}

/* ------------------------------------------------------------------ circuit summaries */

fn short_params(e: &daedalus_core::doc::Element) -> String {
    let Some(d) = lib().get(&e.typ) else { return String::new() };
    let mut v = vec![];
    for p in &d.params {
        let cur = e.params.get(&p.k).cloned().unwrap_or_default();
        if cur != p.d && !(p.typ.as_deref() == Some("code") && cur.trim().is_empty()) {
            let one = cur.replace('\n', "⏎");
            let one = if one.chars().count() > 40 { format!("{}…", one.chars().take(40).collect::<String>()) } else { one };
            v.push(format!("{}={}", p.k, one));
        }
    }
    v.join(" ")
}

fn sim_line(doc: &Doc) -> String {
    match doc.elements.iter().find(|e| e.typ == "SIMCTRL" && !e.dis) {
        Some(e) => {
            let g = |k: &str| e.params.get(k).cloned().unwrap_or_default();
            let mut s = format!("dt={} tt={}", g("dt"), g("tt"));
            if g("tp") != "0" && !g("tp").is_empty() {
                s += &format!(" tp={}", g("tp"));
            }
            if g("ps") != "1" && !g("ps").is_empty() {
                s += &format!(" ps={}", g("ps"));
            }
            if g("acc") == "1" {
                s += &format!(" 高精度 etol={}", g("etol"));
            }
            s
        }
        None => "（沒有 SIMCTRL）".into(),
    }
}

fn nets_text(a: &Analysis) -> String {
    let mut out = String::new();
    let mut nets: Vec<&circuit::Net> = a.nets.iter().filter(|n| !n.pins.is_empty()).collect();
    nets.sort_by(|x, y| (!x.ground, x.name.starts_with('~'), x.name.to_lowercase()).cmp(&(!y.ground, y.name.starts_with('~'), y.name.to_lowercase())));
    for n in nets {
        let tag = if n.ground {
            "（接地）"
        } else if n.control {
            "［控制］"
        } else {
            ""
        };
        let _ = writeln!(out, "  {}{}：{}", n.name, tag, n.pins.join("、"));
    }
    out
}

fn problems_text(a: &Analysis) -> String {
    let mut out = String::new();
    if a.problems.is_empty() {
        out += "問題：無\n";
    } else {
        let _ = writeln!(out, "問題（{}）：", a.problems.len());
        for p in &a.problems {
            let _ = writeln!(out, "  - {}", p);
        }
    }
    if !a.warnings.is_empty() {
        let _ = writeln!(out, "警告（{}）：", a.warnings.len());
        for p in &a.warnings {
            let _ = writeln!(out, "  - {}", p);
        }
    }
    out
}

fn summary(c: &Circuit, path: &Path) -> String {
    let doc = &c.doc;
    let a = analyze(doc);
    let real: Vec<&daedalus_core::doc::Element> = doc.elements.iter().filter(|e| !matches!(e.typ.as_str(), "LABEL" | "GND" | "TEXT")).collect();
    let mut out = String::new();
    let _ = writeln!(out, "{}", path.display());
    let _ = writeln!(out, "元件 {}、導線 {}、節點 {}；模擬控制 {}", real.len(), doc.wires.len(), a.nets.iter().filter(|n| !n.pins.is_empty()).count(), sim_line(doc));
    out += "元件（名稱 型別 非預設參數 @位置 接腳→節點）：\n";
    for e in &real {
        let pins: Vec<String> = lib()
            .pins_for(e)
            .iter()
            .map(|p| {
                let pr = format!("{}.{}", el_name(e), p.n);
                let n = a.net_of(&pr).map(|n| if n.pins.len() <= 1 && !n.ground { "（未接）".to_string() } else { n.name.clone() }).unwrap_or_else(|| "（停用）".into());
                format!("{}→{}", p.n, n)
            })
            .collect();
        let mut line = format!("  {} {}", el_name(e), e.typ);
        let sp = short_params(e);
        if !sp.is_empty() {
            line += &format!(" {}", sp);
        }
        line += &format!(" @({},{})", e.x, e.y);
        if e.rot & 3 != 0 {
            line += &format!(" rot{}", e.rot & 3);
        }
        if e.mir {
            line += " mir";
        }
        if e.dis {
            line += " ［停用］";
        }
        if !pins.is_empty() {
            line += &format!("  {}", pins.join(" "));
        }
        let _ = writeln!(out, "{}", line);
    }
    out += "節點：\n";
    out += &nets_text(&a);
    if let Some(sc) = &c.screens {
        let _ = writeln!(out, "波形畫面：{}", sc.iter().map(|s| s.join("+")).collect::<Vec<_>>().join(" | "));
    }
    out += &problems_text(&a);
    out
}

fn element_detail(c: &Circuit, name: &str) -> Result<String, String> {
    let i = circuit::find_el(&c.doc, name)?;
    let e = &c.doc.elements[i];
    let d = lib().get(&e.typ).ok_or_else(|| format!("未知的元件類型 {}", e.typ))?;
    let a = analyze(&c.doc);
    let mut out = format!("{}：{}（{}，{}）@({},{}) rot{}{}{}\n", el_name(e), d.zh, e.typ, d.name, e.x, e.y, e.rot & 3, if e.mir { " mir" } else { "" }, if e.dis { " ［停用］" } else { "" });
    out += "接腳：\n";
    for p in lib().pins_for(e) {
        let (x, y) = daedalus_core::library::tpt(e, p.x, p.y);
        let pr = format!("{}.{}", el_name(e), p.n);
        let n = a.net_of(&pr).map(|n| if n.pins.len() <= 1 && !n.ground { "（未接）".to_string() } else { format!("{}（{}）", n.name, n.pins.iter().filter(|q| **q != pr).cloned().collect::<Vec<_>>().join("、")) }).unwrap_or_default();
        let _ = writeln!(out, "  {} ［{}］ 位置 ({},{}) → {}", p.n, circuit::kind_zh(&p.k), x, y, n);
    }
    out += "參數：\n";
    for p in &d.params {
        let v = e.params.get(&p.k).cloned().unwrap_or_default();
        let _ = writeln!(out, "  {} = {}{}{}  — {}", p.k, v.replace('\n', "⏎"), if p.u.is_empty() { String::new() } else { format!(" [{}]", p.u) }, if v != p.d { format!("（預設 {}）", p.d.replace('\n', "⏎")) } else { String::new() }, p.l);
    }
    Ok(out)
}

/* ------------------------------------------------------------------ elements */

fn el_line(d: &daedalus_core::library::ElDef) -> String {
    format!("{}｜{}｜{}｜接腳 {}", d.typ, d.zh, d.name, if d.pins.is_empty() { if d.typ == "CBLOCK" { "x1…、y1…（依 nin／nout）".to_string() } else { "無".to_string() } } else { d.pins.iter().map(|p| p.n.as_str()).collect::<Vec<_>>().join("、") })
}

fn elements(a: &Value) -> Reply {
    let defs = &lib().defs;
    if let Some(t) = s(a, "type") {
        let Some(d) = defs.iter().find(|d| d.typ.eq_ignore_ascii_case(t)) else {
            let near: Vec<String> = defs.iter().filter(|d| d.typ.to_lowercase().contains(&t.to_lowercase()) || d.zh.contains(t)).take(10).map(|d| format!("{}（{}）", d.typ, d.zh)).collect();
            return Reply::err(format!("沒有型別代碼「{}」。{}", t, if near.is_empty() { "請用 query 搜尋".to_string() } else { format!("相近：{}", near.join("、")) }));
        };
        let mut out = format!("{}｜{}｜{}\n類別：{}\n", d.typ, d.zh, d.name, d.cat);
        let desc = d.desc.trim();
        if !desc.is_empty() {
            let _ = writeln!(out, "說明：{}", desc.replace('\n', " "));
        }
        let mut flags = vec![];
        if d.has("probe") {
            flags.push("探棒／量表：名稱就是波形名稱");
        }
        if d.has("src") {
            flags.push("電源");
        }
        if d.has("single") {
            flags.push("每個電路只能一個");
        }
        if d.nosim() {
            flags.push("不參與模擬（設定或註解）");
        }
        if d.noname() {
            flags.push("不需要名稱");
        }
        if !flags.is_empty() {
            let _ = writeln!(out, "性質：{}", flags.join("；"));
        }
        if d.pins.is_empty() {
            out += if d.typ == "CBLOCK" { "接腳：輸入 x1…xn 在左（x=−2）、輸出 y1…ym 在右（x=2），數量由 nin／nout 決定，自 y = −⌊(n−1)/2⌋ 起每格一隻\n" } else { "接腳：無（不接線）\n" };
        } else {
            out += "接腳（rot=0 時相對元件中心的位置，y 向下）：\n";
            for p in &d.pins {
                let _ = writeln!(out, "  {} ［{}］ ({},{})", p.n, circuit::kind_zh(&p.k), p.x, p.y);
            }
        }
        out += "參數（鍵 = 預設值 [單位] 名稱：說明）：\n";
        for p in &d.params {
            let mut line = format!("  {} = {}", p.k, if p.d.contains('\n') { format!("\"{}\"", p.d.replace('\n', "\\n")) } else { p.d.clone() });
            if !p.u.is_empty() {
                line += &format!(" [{}]", p.u);
            }
            line += &format!(" {}", p.l);
            if p.adv {
                line += "（進階）";
            }
            if !p.desc.trim().is_empty() {
                line += &format!("：{}", p.desc.trim().replace('\n', " "));
            }
            if let Some(o) = &p.opts {
                line += &format!("　選項：{}", o.iter().map(|(k, v)| format!("{}＝{}", k, v)).collect::<Vec<_>>().join("；"));
            }
            let _ = writeln!(out, "{}", line);
        }
        return Reply::text(out);
    }
    if let Some(q) = s(a, "query") {
        let terms: Vec<String> = q.to_lowercase().split_whitespace().map(String::from).collect();
        let mut hits: Vec<(i32, &daedalus_core::library::ElDef)> = defs
            .iter()
            .filter_map(|d| {
                let (t, zh, en, cat, desc) = (d.typ.to_lowercase(), d.zh.to_lowercase(), d.name.to_lowercase(), d.cat.to_lowercase(), d.desc.to_lowercase());
                let mut score = 0;
                for w in &terms {
                    let sc = if t == *w {
                        100
                    } else if t.contains(w.as_str()) {
                        40
                    } else if zh.contains(w.as_str()) || en.contains(w.as_str()) {
                        30
                    } else if cat.contains(w.as_str()) {
                        10
                    } else if desc.contains(w.as_str()) {
                        5
                    } else {
                        return None;
                    };
                    score += sc;
                }
                Some((score, d))
            })
            .collect();
        hits.sort_by(|x, y| y.0.cmp(&x.0).then(x.1.typ.cmp(&y.1.typ)));
        if hits.is_empty() {
            return Reply::text(format!("找不到「{}」。可改用其他關鍵字（中英文皆可），或不帶參數列出全部元件。", q));
        }
        let mut out = format!("「{}」找到 {} 個（型別｜中文｜英文｜接腳）：\n", q, hits.len());
        for (_, d) in hits.iter().take(30) {
            let _ = writeln!(out, "  {}", el_line(d));
        }
        out += "用 type 取得接腳類型與參數。";
        return Reply::text(out);
    }
    let cat = s(a, "category").map(|c| c.to_lowercase());
    let mut by_cat: Vec<(&str, Vec<&daedalus_core::library::ElDef>)> = vec![];
    for d in defs.iter() {
        if let Some(c) = &cat {
            if !d.cat.to_lowercase().contains(c.as_str()) {
                continue;
            }
        }
        match by_cat.iter_mut().find(|x| x.0 == d.cat) {
            Some(x) => x.1.push(d),
            None => by_cat.push((d.cat.as_str(), vec![d])),
        }
    }
    let mut out = String::new();
    if cat.is_some() {
        for (c, v) in &by_cat {
            let _ = writeln!(out, "{}：", c);
            for d in v {
                let _ = writeln!(out, "  {}", el_line(d));
            }
        }
    } else {
        let _ = writeln!(out, "全部 {} 種元件（型別代碼（中文名）），依類別：", defs.len());
        for (c, v) in &by_cat {
            let _ = writeln!(out, "{}：{}", c.trim_start_matches("Elements/"), v.iter().map(|d| format!("{}（{}）", d.typ, d.zh)).collect::<Vec<_>>().join("、"));
        }
        out += "用 type 取得一個元件的接腳與參數；query 以關鍵字搜尋。";
    }
    if out.is_empty() {
        return Reply::text("沒有符合的類別。");
    }
    Reply::text(out)
}

/* ------------------------------------------------------------------ simulation output */

fn tran_window(r: &TranResult, doc: &Doc, a: &Value) -> Result<(f64, f64, String), String> {
    let t_end = r.t0 + r.dt * (r.n.max(1) - 1) as f64;
    let from = match s(a, "from") {
        Some(t) => eval_time(doc, t)?,
        None => r.t0 + (t_end - r.t0) * 0.8,
    };
    let to = match s(a, "to") {
        Some(t) => eval_time(doc, t)?,
        None => t_end,
    };
    let (from, to) = (from.max(r.t0), to.min(t_end));
    if !(to >= from) {
        return Err(format!("統計區間 {} ～ {} 不在模擬時間 {} ～ {} 內", si(from, "s", 4), si(to, "s", 4), si(r.t0, "s", 4), si(t_end, "s", 4)));
    }
    let note = if s(a, "from").is_none() && s(a, "to").is_none() { "（最後 20 %）".to_string() } else { String::new() };
    Ok((from, to, note))
}

fn cols(names: &[String], want: &[String]) -> Vec<usize> {
    if want.is_empty() {
        return (0..names.len()).collect();
    }
    (0..names.len()).filter(|&i| want.iter().any(|w| names[i] == *w || names[i].starts_with(&format!("{} [", w)))).collect()
}

fn tran_text(run: &Run, r: &TranResult, doc: &Doc, a: &Value) -> Result<String, String> {
    let mut out = String::new();
    let _ = writeln!(
        out,
        "{}模擬完成：{:.2} 秒，{} 步（細分 {}、未收斂 {}），切換事件 {}，dt = {}，總時間 {}，記錄 {} 點",
        run.kind,
        run.secs,
        r.stats.steps,
        r.stats.refined,
        r.stats.non_conv,
        r.events,
        si(run.sc.dt, "s", 4),
        si(run.sc.t_total, "s", 4),
        r.n
    );
    if let Some(acc) = &r.accuracy {
        let _ = writeln!(out, "高精度：估計相對誤差 {:.2e}（目標 {:.0e}，{}），最終步長 {}", acc.est, acc.target, if acc.met { "達成" } else { "未達成" }, si(acc.dt, "s", 3));
    }
    if r.n == 0 || r.names.is_empty() {
        out += "沒有記錄任何波形（電路中沒有探棒或量表）。\n";
        return Ok(out);
    }
    let (from, to, note) = tran_window(r, doc, a)?;
    let _ = writeln!(out, "統計區間 {} ～ {}{}", si(from, "s", 4), si(to, "s", 4), note);
    let want = strs(a, "probes");
    let idx = cols(&r.names, &want);
    if idx.is_empty() {
        return Err(format!("找不到探棒 {}。可用：{}", want.join("、"), r.names.join("、")));
    }
    let base = [Meas::Mean, Meas::Rms, Meas::Min, Meas::Max, Meas::Pp];
    out += "| 探棒 | 平均 | RMS | 最小 | 最大 | 峰對峰 | 終值 |\n|---|---|---|---|---|---|---|\n";
    for &ci in &idx {
        let u = unit_of(doc, &r.names[ci]);
        let v = measure(r, ci, from, to, &base);
        let last = r.data[ci].get(r.n.saturating_sub(1)).copied().unwrap_or(f64::NAN);
        let _ = writeln!(out, "| {} | {} | {} | {} | {} | {} | {} |", r.names[ci], si(v[0], &u, 4), si(v[1], &u, 4), si(v[2], &u, 4), si(v[3], &u, 4), si(v[4], &u, 4), si(last, &u, 4));
    }
    let ms = parse_meas(&strs(a, "measures"))?;
    if !ms.is_empty() {
        out += "量測：\n";
        for &ci in &idx {
            let u = unit_of(doc, &r.names[ci]);
            let v = measure(r, ci, from, to, &ms);
            let items: Vec<String> = ms
                .iter()
                .zip(&v)
                .map(|(m, x)| {
                    let val = match m.unit() {
                        daedalus_core::measure::Unit::Sig => if x.is_finite() { si(*x, &u, 4) } else { "—".into() },
                        daedalus_core::measure::Unit::SigTime => if x.is_finite() { si(*x, &format!("{}·s", u), 4) } else { "—".into() },
                        _ => m.fmt(*x),
                    };
                    format!("{} {}", m.head(), val)
                })
                .collect();
            let _ = writeln!(out, "  {}：{}", r.names[ci], items.join("，"));
        }
    }
    if let Some(mc) = &r.mc {
        let _ = writeln!(out, "蒙地卡羅：{} 次抽樣，種子 {}；變數 {}", mc.samples.len(), mc.seed, mc.vars.join("、"));
        if !mc.stats.is_empty() {
            out += "| 探棒 | 指標 | 平均 | 標準差 | 最小 | 最大 |\n|---|---|---|---|---|---|\n";
            for st in &mc.stats {
                let u = unit_of(doc, &st.probe);
                let _ = writeln!(out, "| {} | {} | {} | {} | {} | {} |", st.probe, st.metric, si(st.mean, &u, 4), si(st.std, &u, 3), si(st.min, &u, 4), si(st.max, &u, 4));
            }
        }
    }
    if let Some(sw) = &r.sweep {
        let _ = writeln!(out, "參數掃描 {}：{}（{} 個執行緒）", sw.var, sw.labels.join("、"), sw.threads);
    }
    Ok(out)
}

/// crossover, margins and bandwidth (as the program's Bode window)
fn bode_summary(c: &AcCurve, fs: &[f64]) -> Vec<String> {
    let mut out = vec![];
    if fs.is_empty() {
        return out;
    }
    let interp = |f0: f64, f1: f64, r: f64| f0 * (f1 / f0).powf(r);
    for k in 1..fs.len() {
        let (a, b) = (c.mag[k - 1], c.mag[k]);
        if (a >= 0.0 && b < 0.0) || (a < 0.0 && b >= 0.0) {
            let r = a / (a - b);
            let f = interp(fs[k - 1], fs[k], r);
            let ph = c.ph[k - 1] + (c.ph[k] - c.ph[k - 1]) * r;
            let pm = ((ph + 180.0) % 360.0 + 540.0) % 360.0 - 180.0;
            if c.lp {
                out.push(format!("穿越頻率 fc = {}，相位 {:.2}°，相位裕度 PM = {:.2}°", si(f, "Hz", 4), ph, pm));
            } else {
                out.push(format!("0 dB 穿越 @ {}（相位 {:.2}°）", si(f, "Hz", 4), ph));
            }
            break;
        }
    }
    if c.lp {
        let m = |x: f64| ((x % 360.0) + 540.0) % 360.0 - 180.0;
        for k in 1..fs.len() {
            let (pa, pb) = (m(c.ph[k - 1] + 180.0), m(c.ph[k] + 180.0));
            if (pa - pb).abs() < 90.0 && ((pa >= 0.0 && pb < 0.0) || (pa < 0.0 && pb >= 0.0)) {
                let r = pa / (pa - pb);
                let g = c.mag[k - 1] + (c.mag[k] - c.mag[k - 1]) * r;
                out.push(format!("相位 −180° 於 {}，增益裕度 GM = {:.2} dB", si(interp(fs[k - 1], fs[k], r), "Hz", 4), -g));
                break;
            }
        }
    }
    let pk = (0..c.mag.len()).fold(0, |p, k| if c.mag[k] > c.mag[p] { k } else { p });
    out.push(format!("低頻增益 {:.2} dB @ {}，峰值 {:.2} dB @ {}", c.mag[0], si(fs[0], "Hz", 4), c.mag[pk], si(fs[pk], "Hz", 4)));
    if !c.lp {
        let rf = c.mag[0] - 3.0;
        for k in 1..fs.len() {
            if c.mag[k - 1] >= rf && c.mag[k] < rf {
                let r = (c.mag[k - 1] - rf) / (c.mag[k - 1] - c.mag[k]);
                out.push(format!("−3 dB 頻寬 {}", si(interp(fs[k - 1], fs[k], r), "Hz", 4)));
                break;
            }
        }
    }
    out
}

fn ac_text(run: &Run, r: &AcResult, a: &Value) -> String {
    let mut out = format!("AC 掃頻完成：{:.2} 秒，{} 個頻率點（{} ～ {}）\n", run.secs, r.freqs.len(), si(*r.freqs.first().unwrap_or(&0.0), "Hz", 4), si(*r.freqs.last().unwrap_or(&0.0), "Hz", 4));
    let want = strs(a, "probes");
    for c in r.curves.iter().filter(|c| want.is_empty() || want.contains(&c.name)) {
        let _ = writeln!(out, "{}{}：", c.name, if c.lp { "（迴路增益）" } else { "" });
        for l in bode_summary(c, &r.freqs) {
            let _ = writeln!(out, "  {}", l);
        }
        // a dozen log-spaced points
        let n = r.freqs.len();
        if n > 1 {
            let k = 12.min(n);
            let mut rows = vec![];
            for j in 0..k {
                let i = ((j as f64) * (n - 1) as f64 / (k - 1) as f64).round() as usize;
                rows.push(format!("{} {:.2} dB {:.1}°", si(r.freqs[i], "Hz", 3), c.mag[i], c.ph[i]));
            }
            let _ = writeln!(out, "  頻率響應：{}", rows.join("；"));
        }
    }
    out
}

/* ------------------------------------------------------------------ waveforms */

fn tran_traces(r: &TranResult, ci: usize, from: f64, to: f64, n: usize) -> render::Trace {
    let mut x = vec![];
    let mut lo = vec![];
    let mut hi = vec![];
    let d = &r.data[ci];
    let i0 = (((from - r.t0) / r.dt).floor().max(0.0) as usize).min(r.n.saturating_sub(1));
    let i1 = (((to - r.t0) / r.dt).ceil().max(0.0) as usize).min(r.n.saturating_sub(1));
    let span = (i1 - i0).max(1);
    let n = n.max(2).min(span + 1);
    for b in 0..n {
        let a = i0 + span * b / n;
        let e = (i0 + span * (b + 1) / n).max(a + 1).min(i1 + 1);
        let (mut l, mut h) = (f64::INFINITY, f64::NEG_INFINITY);
        for &v in &d[a..e] {
            if v.is_finite() {
                l = l.min(v);
                h = h.max(v);
            }
        }
        x.push(r.t0 + r.dt * ((a + e - 1) as f64 / 2.0));
        lo.push(l);
        hi.push(h);
    }
    render::Trace { name: r.names[ci].clone(), x, lo, hi }
}

fn waveform_reply(run: &Run, doc: &Doc, screens: &Option<Vec<Vec<String>>>, a: &Value, title: &str) -> Result<Reply, String> {
    let want = strs(a, "probes");
    let width = a.get("width").and_then(|v| v.as_u64()).unwrap_or(1000).clamp(500, 2400) as f64;
    let image = b(a, "image", true);
    let table = b(a, "table", false);
    match &run.out {
        Outcome::Tran(r) => {
            if r.n == 0 || r.names.is_empty() {
                return Err("沒有記錄任何波形（電路中沒有探棒或量表）".into());
            }
            let t_end = r.t0 + r.dt * (r.n - 1) as f64;
            let from = match s(a, "from") {
                Some(t) => eval_time(doc, t)?.max(r.t0),
                None => r.t0,
            };
            let to = match s(a, "to") {
                Some(t) => eval_time(doc, t)?.min(t_end),
                None => t_end,
            };
            if !(to > from) {
                return Err("from 必須小於 to，且在模擬時間內".into());
            }
            // panels: the file's screens, else one per probe
            let groups: Vec<Vec<String>> = if !want.is_empty() {
                want.iter().map(|w| vec![w.clone()]).collect()
            } else if let Some(sc) = screens.as_ref().filter(|s| !s.is_empty()) {
                sc.clone()
            } else {
                r.names.iter().map(|n| vec![n.split(" [").next().unwrap_or(n).to_string()]).fold(vec![], |mut v: Vec<Vec<String>>, g| {
                    if !v.contains(&g) {
                        v.push(g);
                    }
                    v
                })
            };
            let mut panels = vec![];
            for g in groups.iter().take(8) {
                let idx = cols(&r.names, g);
                if idx.is_empty() {
                    continue;
                }
                let unit = unit_of(doc, &r.names[idx[0]]);
                panels.push(render::Panel { traces: idx.iter().map(|&ci| tran_traces(r, ci, from, to, 700)).collect(), unit });
            }
            if panels.is_empty() {
                return Err(format!("找不到探棒 {}。可用：{}", want.join("、"), r.names.join("、")));
            }
            let mut txt = format!("{}：{} ～ {}，{} 個畫面", title, si(from, "s", 4), si(to, "s", 4), panels.len());
            if table {
                let n = a.get("points").and_then(|v| v.as_u64()).unwrap_or(40).clamp(5, 400) as usize;
                let idx: Vec<usize> = panels.iter().flat_map(|p| p.traces.iter().map(|t| r.names.iter().position(|n| *n == t.name).unwrap())).collect();
                let tr: Vec<render::Trace> = idx.iter().map(|&ci| tran_traces(r, ci, from, to, n)).collect();
                let _ = write!(txt, "\n數值表（每點為該時間區段的最小～最大）：\n| t | {} |\n|{}", tr.iter().map(|t| format!("{} ({})", t.name, unit_of(doc, &t.name))).collect::<Vec<_>>().join(" | "), "---|".repeat(tr.len() + 1));
                for k in 0..tr[0].x.len() {
                    let row: Vec<String> = tr
                        .iter()
                        .map(|t| {
                            let (l, h) = (t.lo.get(k).copied().unwrap_or(f64::NAN), t.hi.get(k).copied().unwrap_or(f64::NAN));
                            if (h - l).abs() <= 1e-9 * h.abs().max(l.abs()).max(1e-30) {
                                format!("{:.5e}", l)
                            } else {
                                format!("{:.4e}～{:.4e}", l, h)
                            }
                        })
                        .collect();
                    let _ = write!(txt, "\n| {:.5e} | {} |", tr[0].x[k], row.join(" | "));
                }
            }
            let mut rep = Reply::text(txt);
            if image {
                let svg = render::plot_svg(title, &panels, &render::Axis { lo: from, hi: to, log: false, unit: "s".into() }, width);
                rep = rep.image(&render::png(&svg)?);
            }
            Ok(rep)
        }
        Outcome::Ac(r) => {
            let curves: Vec<&AcCurve> = r.curves.iter().filter(|c| want.is_empty() || want.contains(&c.name)).collect();
            if curves.is_empty() || r.freqs.is_empty() {
                return Err("沒有 AC 結果曲線".into());
            }
            let (f0, f1) = (r.freqs[0], *r.freqs.last().unwrap());
            let from = s(a, "from").map(|t| eval_time(doc, t)).transpose()?.unwrap_or(f0).max(f0);
            let to = s(a, "to").map(|t| eval_time(doc, t)).transpose()?.unwrap_or(f1).min(f1);
            let mk = |f: &dyn Fn(&AcCurve, usize) -> f64| -> Vec<render::Trace> {
                curves.iter().map(|c| render::Trace { name: c.name.clone(), x: r.freqs.clone(), lo: (0..r.freqs.len()).map(|i| f(c, i)).collect(), hi: (0..r.freqs.len()).map(|i| f(c, i)).collect() }).collect()
            };
            let panels = vec![render::Panel { traces: mk(&|c, i| c.mag[i]), unit: "dB".into() }, render::Panel { traces: mk(&|c, i| c.ph[i]), unit: "°".into() }];
            let mut txt = format!("{}：波德圖 {} ～ {}（上：增益 dB，下：相位 °）", title, si(from, "Hz", 3), si(to, "Hz", 3));
            if table {
                for c in &curves {
                    let _ = write!(txt, "\n{}：{}", c.name, (0..r.freqs.len()).filter(|&i| r.freqs[i] >= from && r.freqs[i] <= to).map(|i| format!("{} {:.2}dB {:.1}°", si(r.freqs[i], "Hz", 3), c.mag[i], c.ph[i])).collect::<Vec<_>>().join("；"));
                }
            }
            let mut rep = Reply::text(txt);
            if image {
                let svg = render::plot_svg(title, &panels, &render::Axis { lo: from, hi: to, log: true, unit: "Hz".into() }, width);
                rep = rep.image(&render::png(&svg)?);
            }
            Ok(rep)
        }
    }
}

/* ------------------------------------------------------------------ the server */

impl Server {
    pub fn new() -> Server {
        Server { cache: HashMap::new() }
    }

    pub fn call(&mut self, name: &str, a: &Value) -> Reply {
        let r = match name {
            "daedalus_guide" => Ok(Reply::text(guide_section(s(a, "topic").unwrap_or("workflow")))),
            "daedalus_elements" => Ok(elements(a)),
            "daedalus_new" => self.new_circuit(a),
            "daedalus_inspect" => self.inspect(a),
            "daedalus_edit" => self.edit(a),
            "daedalus_simulate" => self.simulate(a),
            "daedalus_waveform" => self.waveform(a),
            "daedalus_render" => self.render(a),
            "daedalus_examples" => self.examples(a),
            "daedalus_open" => self.open(a),
            _ => Err(format!("沒有工具 {}", name)),
        };
        r.unwrap_or_else(Reply::err)
    }

    fn new_circuit(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        if p.exists() && !b(a, "overwrite", false) {
            return Err(format!("{} 已存在。要覆寫請設 overwrite=true；修改既有電路請用 daedalus_edit。", p.display()));
        }
        let nl = a.get("netlist").ok_or("需要 netlist")?;
        let built = circuit::from_netlist(nl)?;
        built.circuit.save(&p)?;
        self.cache.remove(&p);
        let mut txt = format!("已建立電路檔。\n{}", summary(&built.circuit, &p));
        if !built.notes.is_empty() {
            txt += &format!("備註：\n{}\n", built.notes.iter().map(|n| format!("  - {}", n)).collect::<Vec<_>>().join("\n"));
        }
        txt += "下一步：daedalus_simulate 模擬；daedalus_render 看電路圖。";
        let mut rep = Reply::text(txt);
        if b(a, "render", false) {
            rep = rep.image(&render::png(&render::schematic_svg(&built.circuit.doc, &render::SchOpts { highlight: None, pin_names: false, max_px: 1400.0 }))?);
        }
        Ok(rep)
    }

    fn inspect(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        let c = Circuit::load(&p)?;
        if let Some(e) = s(a, "element") {
            return element_detail(&c, e).map(Reply::text);
        }
        Ok(Reply::text(summary(&c, &p)))
    }

    fn edit(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        let ops = a.get("ops").and_then(|v| v.as_array()).ok_or("需要 ops（操作陣列）")?;
        let mut c = Circuit::load(&p)?;
        let before = analyze(&c.doc);
        let log = circuit::apply_ops(&mut c, ops)?;
        let after = analyze(&c.doc);
        let mut txt = String::new();
        let target = match s(a, "save_as") {
            Some(x) => {
                let q = PathBuf::from(x);
                std::path::absolute(&q).unwrap_or(q)
            }
            None => p.clone(),
        };
        if b(a, "dry_run", false) {
            txt += "（dry_run：沒有存檔）\n";
        } else {
            c.save(&target)?;
            self.cache.remove(&target);
            let _ = writeln!(txt, "已存檔：{}", target.display());
        }
        txt += "完成的操作：\n";
        for d in &log.done {
            let _ = writeln!(txt, "  - {}", d);
        }
        let ch = circuit::net_changes(&before, &after);
        if ch.is_empty() {
            txt += "連線沒有變化\n";
        } else {
            let _ = writeln!(txt, "連線變化（{}）：", ch.len());
            for x in ch.iter().take(60) {
                let _ = writeln!(txt, "  {}", x);
            }
        }
        txt += &problems_text(&after);
        let mut rep = Reply::text(txt);
        if b(a, "render", false) {
            rep = rep.image(&render::png(&render::schematic_svg(&c.doc, &render::SchOpts { highlight: None, pin_names: false, max_px: 1400.0 }))?);
        }
        Ok(rep)
    }

    /// runs (or reuses) a simulation of the file
    fn run(&mut self, p: &Path, a: &Value) -> Result<(&Cached, Option<Vec<Vec<String>>>), String> {
        let c = Circuit::load(p)?;
        let has_ac = c.doc.elements.iter().any(|e| e.typ == "ACSWEEP" && !e.dis);
        let ac = match s(a, "analysis").unwrap_or("auto") {
            "ac" => true,
            "transient" | "tran" => false,
            _ => has_ac,
        };
        let tt = s(a, "tt").map(String::from);
        let st = stamp(p);
        let fresh = self.cache.get(p).map(|x| x.stamp == st && x.ac == ac && x.tt == tt).unwrap_or(false);
        if !fresh {
            let limit = a.get("timeout").and_then(|v| v.as_f64()).unwrap_or(120.0).clamp(1.0, 3600.0);
            let run = run::run_doc(&c.doc, ac, tt.as_deref(), limit)?;
            self.cache.insert(p.to_path_buf(), Cached { stamp: st, ac, tt, run, doc: c.doc.clone() });
        }
        Ok((self.cache.get(p).unwrap(), c.screens))
    }

    fn simulate(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        let (cached, screens) = self.run(&p, a)?;
        let (run, doc) = (&cached.run, &cached.doc);
        let mut txt = match &run.out {
            Outcome::Tran(r) => tran_text(run, r, doc, a)?,
            Outcome::Ac(r) => ac_text(run, r, a),
        };
        let warns = match &run.out {
            Outcome::Tran(r) => &r.warnings,
            Outcome::Ac(r) => &r.warnings,
        };
        if !warns.is_empty() {
            let _ = writeln!(txt, "模擬警告（{}）：", warns.len());
            for w in warns.iter().take(20) {
                let _ = writeln!(txt, "  - {}", w);
            }
        }
        if let Some(csv) = s(a, "csv") {
            let q = PathBuf::from(csv);
            write_csv(&run.out, &q)?;
            let _ = writeln!(txt, "完整波形已存成 {}", q.display());
        }
        let mut rep = Reply::text(txt);
        if b(a, "image", false) {
            let w = waveform_reply(run, doc, &screens, &json!({"image": true}), &format!("{} 模擬結果", p.file_stem().and_then(|x| x.to_str()).unwrap_or("")))?;
            rep.content.extend(w.content.into_iter().filter(|c| c["type"] == "image"));
        }
        Ok(rep)
    }

    fn waveform(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        // reuse whatever was simulated last for this file, unless the file changed
        let st = stamp(&p);
        let prev = self.cache.get(&p).filter(|x| x.stamp == st).map(|x| (x.ac, x.tt.clone()));
        let args = match prev {
            Some((ac, tt)) => json!({"analysis": if ac { "ac" } else { "transient" }, "tt": tt}),
            None => json!({}),
        };
        let (cached, screens) = self.run(&p, &args)?;
        let title = format!("{} 波形", p.file_stem().and_then(|x| x.to_str()).unwrap_or(""));
        waveform_reply(&cached.run, &cached.doc, &screens, a, &title)
    }

    fn render(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        let c = Circuit::load(&p)?;
        let o = render::SchOpts { highlight: s(a, "highlight").map(String::from), pin_names: b(a, "pins", false), max_px: a.get("max_px").and_then(|v| v.as_f64()).unwrap_or(1400.0).clamp(400.0, 4000.0) };
        let svg = render::schematic_svg(&c.doc, &o);
        let png = render::png(&svg)?;
        let mut txt = format!("{} 的電路圖（深色：功率線；藍色：控制線；紅圈：未接的接腳；邊緣數字：座標）", p.display());
        if let Some(h) = &o.highlight {
            let _ = write!(txt, "；橘色標出 {}", h);
        }
        if let Some(out) = s(a, "save") {
            let q = PathBuf::from(out);
            let data = if q.extension().map(|e| e.eq_ignore_ascii_case("svg")).unwrap_or(false) { svg.clone().into_bytes() } else { png.clone() };
            std::fs::write(&q, data).map_err(|e| format!("無法寫入 {}：{}", q.display(), e))?;
            let _ = write!(txt, "；已存成 {}", q.display());
        }
        Ok(Reply::text(txt).image(&png))
    }

    fn examples(&mut self, a: &Value) -> Result<Reply, String> {
        if let Some(k) = s(a, "key") {
            let ex = EXAMPLES.iter().find(|e| e.key.eq_ignore_ascii_case(k)).ok_or_else(|| format!("沒有範例 {}（不帶參數可列出全部）", k))?;
            let Some(dst) = s(a, "copy_to") else {
                return Err("需要 copy_to（存檔的絕對路徑）".into());
            };
            let q = PathBuf::from(dst);
            let q = std::path::absolute(&q).unwrap_or(q);
            if q.exists() {
                return Err(format!("{} 已存在", q.display()));
            }
            if let Some(d) = q.parent() {
                let _ = std::fs::create_dir_all(d);
            }
            std::fs::write(&q, ex.json).map_err(|e| format!("無法寫入 {}：{}", q.display(), e))?;
            let c = Circuit::load(&q)?;
            let notes: Vec<String> = c.doc.elements.iter().filter(|e| e.typ == "TEXT").filter_map(|e| e.params.get("text").cloned()).collect();
            return Ok(Reply::text(format!("範例「{}」已存成檔案。\n說明：{}\n{}", ex.name, notes.join(" / ").replace('\n', " "), summary(&c, &q))));
        }
        let mut out = format!("內建範例 {} 個（key：名稱［類別］）：\n", EXAMPLES.len());
        for e in EXAMPLES {
            let _ = writeln!(out, "  {}：{}［{}］", e.key, e.name, e.cat);
        }
        out += "用 key 加 copy_to 存成檔案。";
        Ok(Reply::text(out))
    }

    fn open(&mut self, a: &Value) -> Result<Reply, String> {
        let p = path_arg(a)?;
        if !p.exists() {
            return Err(format!("{} 不存在", p.display()));
        }
        let mut cands = vec![];
        if let Ok(me) = std::env::current_exe() {
            if let Some(d) = me.parent() {
                cands.push(d.join("DaedalusEngine.exe"));
            }
        }
        if let Ok(la) = std::env::var("LOCALAPPDATA") {
            cands.push(Path::new(&la).join("Programs").join("DaedalusEngine").join("DaedalusEngine.exe"));
        }
        let exe = cands.into_iter().find(|c| c.is_file()).ok_or("找不到 DaedalusEngine.exe（未安裝或不在 daedalus-cli.exe 旁邊）")?;
        std::process::Command::new(&exe).arg(&p).spawn().map_err(|e| format!("無法啟動 {}：{}", exe.display(), e))?;
        Ok(Reply::text(format!("已在 DaedalusEngine 開啟 {}。之後用 MCP 修改並存檔，程式畫面會自動重新載入。", p.display())))
    }
}

fn write_csv(out: &Outcome, q: &Path) -> Result<(), String> {
    use std::io::Write as _;
    let f = std::fs::File::create(q).map_err(|e| format!("無法寫入 {}：{}", q.display(), e))?;
    let mut w = std::io::BufWriter::new(f);
    let e = |x: std::io::Error| x.to_string();
    match out {
        Outcome::Tran(r) => {
            write!(w, "Time").map_err(e)?;
            for n in &r.names {
                write!(w, ",{}", n).map_err(e)?;
            }
            writeln!(w).map_err(e)?;
            for i in 0..r.n {
                write!(w, "{}", r.t0 + i as f64 * r.dt).map_err(e)?;
                for d in &r.data {
                    write!(w, ",{}", d[i]).map_err(e)?;
                }
                writeln!(w).map_err(e)?;
            }
        }
        Outcome::Ac(r) => {
            write!(w, "Freq").map_err(e)?;
            for c in &r.curves {
                write!(w, ",{} (dB),{} (deg)", c.name, c.name).map_err(e)?;
            }
            writeln!(w).map_err(e)?;
            for (i, f) in r.freqs.iter().enumerate() {
                write!(w, "{}", f).map_err(e)?;
                for c in &r.curves {
                    write!(w, ",{},{}", c.mag[i], c.ph[i]).map_err(e)?;
                }
                writeln!(w).map_err(e)?;
            }
        }
    }
    Ok(())
}
