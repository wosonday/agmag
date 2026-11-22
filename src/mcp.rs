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
enum ToolError {
    Unknown,
    Failed(anyhow::Error),
}

impl From<anyhow::Error> for ToolError {
    fn from(e: anyhow::Error) -> Self {
        ToolError::Failed(e)
    }
}

fn tool(name: &str, description: &str, properties: Value, required: &[&str]) -> Value {
    json!({
        "name": name,
        "description": description,
        "inputSchema": {
            "type": "object",
            "properties": properties,
            "required": required,
            "additionalProperties": false,
        },
    })
}

pub fn tool_definitions() -> Vec<Value> {
    vec![
        tool(
            "agm_send",
            "Send a message to a terminal, or to the user's inbox with to=\"user\". The sender is this agent's terminal, never the user.",
            json!({
                "to": {"type": "string"},
                "kind": {"type": "string", "enum": ["ASSIGN", "REPORT", "ERROR", "QUESTION", "BLOCKED", "PASS", "FAIL"]},
                "task": {"type": "string"},
                "text": {"type": "string"},
            }),
            &["to", "kind", "text"],
        ),
        tool(
            "agm_inbox",
            "List inbox items (open only unless all=true).",
            json!({"all": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "agm_ps",
            "List terminals (live only unless all=true).",
            json!({"all": {"type": "boolean"}}),
            &[],
        ),
        tool(
            "agm_quota",
            "Show provider quota from the DB; with task, also the task's token total.",
            json!({"task": {"type": "string"}}),
            &[],
        ),
        tool(
            "agm_spawn",
            "Spawn a new terminal from a template for a task (respects max_workers and the task budget).",
            json!({
                "template": {"type": "string"},
                "task": {"type": "string"},
                "role": {"type": "string"},
                "name": {"type": "string"},
                "cwd": {"type": "string"},
                "text": {"type": "string"},
            }),
            &["template", "task"],
        ),
        tool(
            "agm_branch_get",
            "Read one entry's fields from a BRANCHES.md file.",
            json!({"branches_file": {"type": "string"}, "entry": {"type": "string"}}),
            &["branches_file", "entry"],
        ),
        tool(
            "agm_branch_update",
            "Update (or create) one entry in a BRANCHES.md file under a lock; refused for worker terminals.",
            json!({
                "branches_file": {"type": "string"},
                "entry": {"type": "string"},
                "fields": {"type": "object", "additionalProperties": {"type": "string"}},
                "worktree": {"type": "string"},
            }),
            &["branches_file", "entry", "fields"],
        ),
    ]
}

/// Lấy tham số tùy chọn kiểu bool; sai kiểu → lỗi nêu tên tham số.
fn opt_bool(args: &Value, key: &str) -> Result<bool, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(false),
        Some(Value::Bool(b)) => Ok(*b),
        Some(_) => Err(anyhow::anyhow!("Tham số `{key}` phải là boolean").into()),
    }
}

fn opt_str<'v>(args: &'v Value, key: &str) -> Result<Option<&'v str>, ToolError> {
    match args.get(key) {
        None | Some(Value::Null) => Ok(None),
        Some(Value::String(s)) => Ok(Some(s)),
        Some(_) => Err(anyhow::anyhow!("Tham số `{key}` phải là chuỗi").into()),
    }
}

fn req_str<'v>(args: &'v Value, key: &str) -> Result<&'v str, ToolError> {
    opt_str(args, key)?.ok_or_else(|| anyhow::anyhow!("Thiếu tham số bắt buộc `{key}`").into())
}

/// Danh tính người gọi MCP: terminal gọi, hoặc "agent" nếu thiếu/rỗng/"user" (không bao giờ giả danh người dùng).
fn mcp_actor<'a>(ctx: &'a McpCtx) -> &'a str {
    match ctx.caller_terminal.as_deref().map(str::trim) {
        Some(t) if !t.is_empty() && !t.eq_ignore_ascii_case("user") => t,
        _ => "agent",
    }
}

/// Từ chối khóa tham số không có trong schema của tool (additionalProperties: false).
fn reject_unknown_keys(name: &str, args: &Value) -> Result<(), ToolError> {
    let Some(def) = tool_definitions().into_iter().find(|t| t["name"] == name) else {
        return Err(ToolError::Unknown);
    };
    let props = &def["inputSchema"]["properties"];
    if let Some(obj) = args.as_object() {
        for key in obj.keys() {
            if props.get(key).is_none() {
                return Err(anyhow::anyhow!("Tham số lạ `{key}` cho tool {name}").into());
            }
        }
    }
    Ok(())
}

/// Nhánh cho từng tool.
fn call_tool(ctx: &McpCtx, name: &str, args: &Value) -> Result<Value, ToolError> {
    reject_unknown_keys(name, args)?;
    match name {
        "agm_send" => {
            let to = req_str(args, "to")?;
            let kind = req_str(args, "kind")?;
            let task = opt_str(args, "task")?;
            let text = req_str(args, "text")?;
            let stored = crate::actions::kind_from_english(kind)
                .or_else(|| crate::actions::KINDS.iter().copied().find(|k| *k == kind))
                .ok_or_else(|| {
                    anyhow::anyhow!(
                        "loại tin '{kind}' không hợp lệ; dùng một trong: ASSIGN, REPORT, ERROR, QUESTION, BLOCKED, PASS, FAIL"
                    )
                })?;
            // Người gửi luôn là terminal gọi hoặc "agent"; không bao giờ là "user".
            let from = mcp_actor(ctx);
            let now = (ctx.clock)();
            let row = crate::actions::send(ctx.store, ctx.hc, from, to, stored, task, text, now)?;
            Ok(serde_json::to_value(row).map_err(anyhow::Error::from)?)
        }
        "agm_spawn" => {
            let req = crate::actions::SpawnReq {
                template: req_str(args, "template")?.into(),
                task: req_str(args, "task")?.into(),
                role: opt_str(args, "role")?.unwrap_or("worker").into(),
                name: opt_str(args, "name")?.map(String::from),
                cwd: opt_str(args, "cwd")?.map(String::from),
                text: opt_str(args, "text")?
                    .map(String::from)
                    .into_iter()
                    .collect(),
            };
            let cfg = crate::config::Config::load(&ctx.cfg_path)?;
            let address = crate::actions::spawn(ctx.store, ctx.hc, &cfg, &req, (ctx.clock)())?;
            Ok(json!({"address": address}))
        }
        "agm_branch_update" => {
            let file = req_str(args, "branches_file")?;
            let entry = req_str(args, "entry")?;
            let Some(Value::Object(map)) = args.get("fields") else {
                return Err(anyhow::anyhow!("Tham số `fields` phải là object").into());
            };
            let mut fields = Vec::new();
            for (k, v) in map {
                let Value::String(s) = v else {
                    return Err(anyhow::anyhow!("Trường `{k}` trong `fields` phải là chuỗi").into());
                };
                fields.push((k.clone(), s.clone()));
            }
            let req = crate::branches::BranchUpdate {
                file: PathBuf::from(file),
                entry: entry.into(),
                fields,
                worktree: opt_str(args, "worktree")?.map(String::from),
            };
            let out = crate::branches::update(
                ctx.store,
                ctx.git,
                Some(mcp_actor(ctx)),
                &req,
                crate::branches::LOCK_TIMEOUT,
                (ctx.clock)(),
            )?;
            Ok(serde_json::to_value(out).map_err(anyhow::Error::from)?)
        }
        "agm_ps" => {
            let all = opt_bool(args, "all")?;
            Ok(json!({"terminals": ctx.store.terminals(!all)?}))
        }
        "agm_inbox" => {
            let all = opt_bool(args, "all")?;
            Ok(json!({"items": ctx.store.inbox(!all)?}))
        }
        "agm_quota" => {
            let task = opt_str(args, "task")?;
            let cfg = crate::config::Config::load(&ctx.cfg_path)?;
            let quota = crate::report::quota_with_config(ctx.store, &cfg, (ctx.clock)() / 1000)?;
            let task = match task {
                Some(t) => json!({"id": t, "tokens": crate::policy::task_tokens(ctx.store, t)?}),
                None => Value::Null,
            };
            Ok(json!({"quota": quota, "task": task}))
        }
        "agm_branch_get" => {
            let file = req_str(args, "branches_file")?;
            let entry = req_str(args, "entry")?;
            if std::path::Path::new(file)
                .file_name()
                .and_then(|n| n.to_str())
                != Some("BRANCHES.md")
            {
                return Err(
                    anyhow::anyhow!("chỉ được đọc tệp tên BRANCHES.md (nhận {file})").into(),
                );
            }
            let got = crate::branches::get(std::path::Path::new(file), entry)?;
            Ok(json!({
                "entry": entry,
                "found": got.is_some(),
                "fields": got.unwrap_or_default(),
            }))
        }
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

pub fn serve(ctx: &McpCtx, mut input: impl BufRead, mut output: impl Write) -> Result<()> {
    let mut buf = Vec::new();
    loop {
        buf.clear();
        if input.read_until(b'\n', &mut buf)? == 0 {
            break;
        }
        let reply = match std::str::from_utf8(&buf) {
            Err(e) => Some(err(&Value::Null, -32700, &format!("parse error: {e}"))),
            Ok(line) if line.trim().is_empty() => continue,
            Ok(line) => match serde_json::from_str::<Value>(line) {
                Err(e) => Some(err(&Value::Null, -32700, &format!("parse error: {e}"))),
                Ok(Value::Array(_)) => Some(err(&Value::Null, -32600, "batch not supported")),
                Ok(msg @ Value::Object(_)) => handle_message(ctx, &msg),
                Ok(_) => Some(err(&Value::Null, -32600, "invalid request")),
            },
        };
        if let Some(r) = reply {
            write_line(&mut output, &r)?;
        }
    }
    Ok(())
}
