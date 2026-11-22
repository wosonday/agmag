//! Máy chủ MCP qua stdio: JSON-RPC 2.0, mỗi dòng một thông điệp. Đồng bộ, không tokio.
//! Không in gì ra stdout ngoài writer của `serve`; log (nếu có) ra stderr.
use std::io::{BufRead, Write};
use std::path::PathBuf;

use anyhow::Result;
use serde_json::{Value, json};

use crate::git::Git;
use crate::hc::Hc;
use crate::store::Store;

pub const PROTOCOL_VERSIONS: [&str; 3] = ["2025-06-18", "2025-03-26", "2024-11-05"];

pub struct McpCtx<'a> {
    pub store: &'a Store,
    pub hc: &'a dyn Hc,
    pub git: &'a dyn Git,
    pub cfg_path: PathBuf,
    pub caller_terminal: Option<String>,
    pub clock: fn() -> i64,
}

/// Lỗi khi chạy tool: tên lạ (→ lỗi JSON-RPC) hoặc tool chạy thất bại (→ isError).
#[allow(dead_code)] // `Failed` sẽ được dùng khi có tool (Task 5-6)
enum ToolError {
    Unknown,
    Failed(anyhow::Error),
}

pub fn tool_definitions() -> Vec<Value> {
    Vec::new()
}

/// Nhánh cho từng tool; Task sau thêm arm trước `_`.
#[allow(clippy::match_single_binding)]
fn call_tool(_ctx: &McpCtx, name: &str, _args: &Value) -> Result<Value, ToolError> {
    match name {
        _ => Err(ToolError::Unknown),
    }
}

fn ok(id: &Value, result: Value) -> Value {
    json!({"jsonrpc":"2.0","id":id,"result":result})
}

fn err(id: &Value, code: i64, message: &str) -> Value {
    json!({"jsonrpc":"2.0","id":id,"error":{"code":code,"message":message}})
}

/// Gói kết quả tools/call ở một chỗ duy nhất.
fn tools_call(ctx: &McpCtx, id: &Value, params: &Value) -> Value {
    let name = params.get("name").and_then(Value::as_str).unwrap_or("");
    let empty = json!({});
    let args = params.get("arguments").unwrap_or(&empty);
    match call_tool(ctx, name, args) {
        Ok(v) => {
            let text = serde_json::to_string_pretty(&v).unwrap_or_default();
            ok(
                id,
                json!({"content":[{"type":"text","text":text}],"structuredContent":v,"isError":false}),
            )
        }
        Err(ToolError::Unknown) => err(id, -32602, &format!("unknown tool: {name}")),
        Err(ToolError::Failed(e)) => ok(
            id,
            json!({"content":[{"type":"text","text":format!("{e:#}")}],"isError":true}),
        ),
    }
}

pub fn handle_message(ctx: &McpCtx, msg: &Value) -> Option<Value> {
    let id = msg.get("id")?;
    let method = msg.get("method").and_then(Value::as_str).unwrap_or("");
    let params = msg.get("params").cloned().unwrap_or(Value::Null);
    Some(match method {
        "initialize" => {
            let want = params.get("protocolVersion").and_then(Value::as_str);
            let ver = want
                .filter(|v| PROTOCOL_VERSIONS.contains(v))
                .unwrap_or(PROTOCOL_VERSIONS[0]);
            ok(
                id,
                json!({
                    "protocolVersion": ver,
                    "capabilities": {"tools": {"listChanged": false}},
                    "serverInfo": {"name": "agm", "version": env!("CARGO_PKG_VERSION")},
                }),
            )
        }
        "ping" => ok(id, json!({})),
        "tools/list" => ok(id, json!({"tools": tool_definitions()})),
        "tools/call" => tools_call(ctx, id, &params),
        _ => err(id, -32601, &format!("method not found: {method}")),
    })
}

fn write_line(output: &mut impl Write, v: &Value) -> Result<()> {
    writeln!(output, "{v}")?;
    output.flush()?;
    Ok(())
}

pub fn serve(ctx: &McpCtx, input: impl BufRead, mut output: impl Write) -> Result<()> {
    for line in input.lines() {
        let line = line?;
        if line.trim().is_empty() {
            continue;
        }
        let reply = match serde_json::from_str::<Value>(&line) {
            Err(e) => Some(err(&Value::Null, -32700, &format!("parse error: {e}"))),
            Ok(Value::Array(_)) => Some(err(&Value::Null, -32600, "batch not supported")),
            Ok(msg) => handle_message(ctx, &msg),
        };
        if let Some(r) = reply {
            write_line(&mut output, &r)?;
        }
    }
    Ok(())
}
