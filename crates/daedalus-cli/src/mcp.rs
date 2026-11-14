//! Model Context Protocol server over stdio: `daedalus-cli mcp`. JSON-RPC 2.0, one message per line on stdin and
//! stdout (stderr only for diagnostics). Tools only: initialize, ping, tools/list, tools/call.
use crate::tools::{self, Reply, Server};
use serde_json::{json, Value};
use std::io::{BufRead, Write};

/// protocol revisions this server speaks (newest first); a client asking for another one gets the newest
const VERSIONS: [&str; 4] = ["2025-11-25", "2025-06-18", "2025-03-26", "2024-11-05"];

pub const INSTRUCTIONS: &str = "DaedalusEngine 是 Windows 上的電力電子電路模擬器（PSIM 風格），電路檔為 .psimx。\
流程：daedalus_elements 查元件的接腳與參數 → daedalus_new 以網表建立電路（同名節點相連，接地寫 \"0\"），或 daedalus_edit 修改既有電路 → \
daedalus_inspect 確認節點連接、問題清單為空 → daedalus_simulate 模擬並與手算比對 → daedalus_waveform、daedalus_render 看波形與電路圖 → daedalus_open 在程式中開給使用者看。\
第一次使用先讀 daedalus_guide（topic=workflow、modeling）。功率節點與控制節點不可直接相連（功率→控制用 VSEN／ISEN，控制→功率用開關閘極、ONCTRL、CTOP）。\
檔案路徑一律用絕對路徑；修改使用者的檔案前先確認，或用 save_as 另存。";

fn error(id: Value, code: i64, msg: &str) -> Value {
    json!({"jsonrpc": "2.0", "id": id, "error": {"code": code, "message": msg}})
}

fn initialize(p: &Value) -> Value {
    let asked = p.get("protocolVersion").and_then(|v| v.as_str()).unwrap_or(VERSIONS[1]);
    let version = if VERSIONS.contains(&asked) { asked } else { VERSIONS[0] };
    json!({
        "protocolVersion": version,
        "capabilities": {"tools": {"listChanged": false}},
        "serverInfo": {"name": "daedalus", "title": "DaedalusEngine 電力電子電路模擬器", "version": daedalus_core::VERSION.trim_start_matches('V')},
        "instructions": INSTRUCTIONS
    })
}

fn panic_text(e: Box<dyn std::any::Any + Send>) -> String {
    if let Some(s) = e.downcast_ref::<&str>() {
        s.to_string()
    } else if let Some(s) = e.downcast_ref::<String>() {
        s.clone()
    } else {
        "未知的錯誤".into()
    }
}

fn call(srv: &mut Server, p: &Value) -> Result<Value, (i64, String)> {
    let name = p.get("name").and_then(|x| x.as_str()).ok_or((-32602, "tools/call: missing name".to_string()))?;
    if !tools::list().iter().any(|t| t["name"] == name) {
        return Err((-32602, format!("Unknown tool: {}", name)));
    }
    let args = match p.get("arguments") {
        Some(Value::Object(_)) => p["arguments"].clone(),
        _ => json!({}),
    };
    // a panic in a tool must not end the server
    let rep = std::panic::catch_unwind(std::panic::AssertUnwindSafe(|| srv.call(name, &args))).unwrap_or_else(|e| Reply::err(format!("內部錯誤：{}", panic_text(e))));
    Ok(json!({"content": rep.content, "isError": rep.error}))
}

fn handle(srv: &mut Server, m: &Value) -> Option<Value> {
    // responses (we send no requests) and notifications (initialized, cancelled) need no answer
    let method = m.get("method").and_then(|x| x.as_str())?;
    let id = m.get("id").cloned()?;
    let params = m.get("params").cloned().unwrap_or(json!({}));
    let r = match method {
        "initialize" => Ok(initialize(&params)),
        "ping" => Ok(json!({})),
        "tools/list" => Ok(json!({"tools": tools::list()})),
        "tools/call" => call(srv, &params),
        "resources/list" => Ok(json!({"resources": []})),
        "resources/templates/list" => Ok(json!({"resourceTemplates": []})),
        "prompts/list" => Ok(json!({"prompts": []})),
        "logging/setLevel" => Ok(json!({})),
        _ => Err((-32601, format!("Method not found: {}", method))),
    };
    Some(match r {
        Ok(v) => json!({"jsonrpc": "2.0", "id": id, "result": v}),
        Err((c, msg)) => error(id, c, &msg),
    })
}

pub fn serve() -> i32 {
    let mut srv = Server::new();
    let stdin = std::io::stdin();
    let mut input = stdin.lock();
    let mut buf: Vec<u8> = Vec::new();
    loop {
        buf.clear();
        match input.read_until(b'\n', &mut buf) {
            Ok(0) | Err(_) => break,
            Ok(_) => {}
        }
        let line = String::from_utf8_lossy(&buf);
        let t = line.trim();
        if t.is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(t) {
            Ok(Value::Array(batch)) => {
                let v: Vec<Value> = batch.iter().filter_map(|m| handle(&mut srv, m)).collect();
                (!v.is_empty()).then_some(Value::Array(v))
            }
            Ok(m) => handle(&mut srv, &m),
            Err(e) => Some(error(Value::Null, -32700, &format!("Parse error: {}", e))),
        };
        if let Some(r) = reply {
            let stdout = std::io::stdout();
            let mut out = stdout.lock();
            if writeln!(out, "{}", r).and_then(|_| out.flush()).is_err() {
                break;
            }
        }
    }
    0
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn protocol() {
        let mut s = Server::new();
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 1, "method": "initialize", "params": {"protocolVersion": "2025-06-18", "capabilities": {}, "clientInfo": {"name": "t", "version": "0"}}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert!(r["result"]["capabilities"]["tools"].is_object());
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 2, "method": "initialize", "params": {"protocolVersion": "1999-01-01"}})).unwrap();
        assert_eq!(r["result"]["protocolVersion"], VERSIONS[0]);
        assert!(handle(&mut s, &json!({"jsonrpc": "2.0", "method": "notifications/initialized"})).is_none());
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": "a", "method": "tools/list"})).unwrap();
        assert_eq!(r["id"], "a");
        assert_eq!(r["result"]["tools"].as_array().unwrap().len(), 10);
        for t in r["result"]["tools"].as_array().unwrap() {
            assert_eq!(t["inputSchema"]["type"], "object", "{}", t["name"]);
        }
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 3, "method": "nope"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 4, "method": "tools/call", "params": {"name": "daedalus_elements", "arguments": {"type": "MOSFET"}}})).unwrap();
        assert_eq!(r["result"]["isError"], false);
        assert!(r["result"]["content"][0]["text"].as_str().unwrap().contains("閘極"));
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 5, "method": "tools/call", "params": {"name": "daedalus_inspect", "arguments": {"path": "Z:/no/such/file.psimx"}}})).unwrap();
        assert_eq!(r["result"]["isError"], true);
        let r = handle(&mut s, &json!({"jsonrpc": "2.0", "id": 6, "method": "tools/call", "params": {"name": "nope", "arguments": {}}})).unwrap();
        assert_eq!(r["error"]["code"], -32602);
    }
}
