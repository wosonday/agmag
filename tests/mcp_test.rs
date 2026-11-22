mod common;

use agmag::git::Git;
use agmag::mcp::{McpCtx, handle_message, serve};
use agmag::store::Store;
use anyhow::Result;
use common::FakeHc;
use serde_json::{Value, json};

struct FakeGit;

impl Git for FakeGit {
    fn head(&self, _cwd: &str) -> Result<String> {
        Ok("abc".into())
    }
    fn is_clean(&self, _cwd: &str) -> Result<bool> {
        Ok(true)
    }
    fn touched_since(&self, _cwd: &str, _since: &str, _path: &str) -> Result<Option<String>> {
        Ok(None)
    }
}

fn with_ctx<R>(f: impl FnOnce(&McpCtx) -> R) -> R {
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc::default();
    let git = FakeGit;
    let ctx = McpCtx {
        store: &store,
        hc: &hc,
        git: &git,
        cfg_path: std::path::PathBuf::from("providers.toml"),
        caller_terminal: None,
        clock: || 0,
    };
    f(&ctx)
}

#[test]
fn initialize_echoes_supported_version_else_latest() {
    with_ctx(|ctx| {
        let req = |v: &str| json!({"jsonrpc":"2.0","id":1,"method":"initialize","params":{"protocolVersion":v}});
        let r = handle_message(ctx, &req("2025-03-26")).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-03-26");
        let r = handle_message(ctx, &req("1999-01-01")).unwrap();
        assert_eq!(r["result"]["protocolVersion"], "2025-06-18");
        assert_eq!(r["result"]["serverInfo"]["name"], "agm");
        assert_eq!(r["result"]["capabilities"]["tools"]["listChanged"], false);
    });
}

#[test]
fn notification_gets_no_response() {
    with_ctx(|ctx| {
        assert!(
            handle_message(
                ctx,
                &json!({"jsonrpc":"2.0","method":"notifications/initialized"})
            )
            .is_none()
        );
    });
}

#[test]
fn unknown_method_and_unknown_tool_codes() {
    with_ctx(|ctx| {
        let r = handle_message(ctx, &json!({"jsonrpc":"2.0","id":2,"method":"foo/bar"})).unwrap();
        assert_eq!(r["error"]["code"], -32601);
        let r = handle_message(
            ctx,
            &json!({"jsonrpc":"2.0","id":3,"method":"tools/call","params":{"name":"agm_kill","arguments":{}}}),
        )
        .unwrap();
        assert_eq!(r["error"]["code"], -32602);
        assert_eq!(r["error"]["message"], "unknown tool: agm_kill");
    });
}

#[test]
fn serve_survives_garbage_and_every_output_line_is_json() {
    with_ctx(|ctx| {
        let input = "not json\n\n[]\n{\"jsonrpc\":\"2.0\",\"id\":7,\"method\":\"ping\"}\n";
        let mut out = Vec::new();
        serve(ctx, input.as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 3);
        assert_eq!(lines[0]["error"]["code"], -32700);
        assert_eq!(lines[0]["id"], Value::Null);
        assert_eq!(lines[1]["error"]["code"], -32600);
        assert_eq!(lines[2]["id"], 7);
        assert_eq!(lines[2]["result"], json!({}));
    });
}

#[test]
fn tools_list_is_an_array() {
    with_ctx(|ctx| {
        let r =
            handle_message(ctx, &json!({"jsonrpc":"2.0","id":4,"method":"tools/list"})).unwrap();
        assert!(r["result"]["tools"].is_array());
    });
}

#[test]
fn mcp_source_never_builds_user_caller() {
    assert!(!include_str!("../src/mcp.rs").contains("Caller::User"));
}
