mod common;

use agmag::git::Git;
use agmag::mcp::{McpCtx, handle_message, serve};
use agmag::store::{Store, TerminalRow};
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

fn call(ctx: &McpCtx, name: &str, args: Value) -> Value {
    handle_message(
        ctx,
        &json!({"jsonrpc":"2.0","id":1,"method":"tools/call","params":{"name":name,"arguments":args}}),
    )
    .unwrap()
}

fn live_term(store: &Store, addr: &str, state: &str, task: Option<&str>) {
    store
        .upsert_terminal(&TerminalRow {
            address: addr.into(),
            state: state.into(),
            task: task.map(String::from),
            ..Default::default()
        })
        .unwrap();
}

#[test]
fn ps_lists_live_terminals_only_by_default() {
    with_ctx(|ctx| {
        live_term(ctx.store, "t1", "live", None);
        live_term(ctx.store, "t2", "exited", None);
        let r = call(ctx, "agm_ps", json!({}));
        assert_eq!(r["result"]["isError"], false);
        assert_eq!(
            r["result"]["structuredContent"]["terminals"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let r = call(ctx, "agm_ps", json!({"all": true}));
        assert_eq!(
            r["result"]["structuredContent"]["terminals"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    });
}

#[test]
fn inbox_returns_open_items() {
    with_ctx(|ctx| {
        let id = ctx
            .store
            .open_inbox(Some("t1"), "blocked", None, "hoi", 1)
            .unwrap()
            .unwrap();
        ctx.store
            .open_inbox(Some("t2"), "blocked", None, "khac", 1)
            .unwrap();
        agmag::actions::dismiss(ctx.store, id, 2).unwrap();
        let r = call(ctx, "agm_inbox", json!({}));
        assert_eq!(
            r["result"]["structuredContent"]["items"]
                .as_array()
                .unwrap()
                .len(),
            1
        );
        let r = call(ctx, "agm_inbox", json!({"all": true}));
        assert_eq!(
            r["result"]["structuredContent"]["items"]
                .as_array()
                .unwrap()
                .len(),
            2
        );
    });
}

#[test]
fn quota_includes_task_tokens_when_task_given() {
    with_ctx(|ctx| {
        let r = call(ctx, "agm_quota", json!({}));
        let sc = &r["result"]["structuredContent"];
        assert!(sc["quota"].is_array());
        assert!(sc["task"].is_null());
        let r = call(ctx, "agm_quota", json!({"task": "a"}));
        let sc = &r["result"]["structuredContent"];
        assert_eq!(sc["task"]["id"], "a");
        assert_eq!(sc["task"]["tokens"], 0);
    });
}

#[test]
fn branch_get_reports_found_false_for_missing_entry() {
    with_ctx(|ctx| {
        let dir = std::env::temp_dir().join(format!("agm-mcp-bg-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("BRANCHES.md");
        std::fs::write(&file, "").unwrap();
        let r = call(
            ctx,
            "agm_branch_get",
            json!({"branches_file": file.to_string_lossy(), "entry": "feat/x"}),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        let sc = &r["result"]["structuredContent"];
        assert_eq!(sc["entry"], "feat/x");
        assert_eq!(sc["found"], false);
        assert_eq!(sc["fields"], json!([]));
        let r = call(ctx, "agm_branch_get", json!({"entry": "feat/x"}));
        assert_eq!(r["result"]["isError"], true);
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("branches_file")
        );
        let _ = std::fs::remove_dir_all(&dir);
    });
}

#[test]
fn wrong_argument_type_is_tool_error() {
    with_ctx(|ctx| {
        let r = call(ctx, "agm_ps", json!({"all": "yes"}));
        assert_eq!(r["result"]["isError"], true);
        assert!(
            r["result"]["content"][0]["text"]
                .as_str()
                .unwrap()
                .contains("all")
        );
    });
}

#[test]
fn tools_list_has_schema_for_each_tool() {
    with_ctx(|ctx| {
        let r =
            handle_message(ctx, &json!({"jsonrpc":"2.0","id":4,"method":"tools/list"})).unwrap();
        let tools = r["result"]["tools"].as_array().unwrap();
        for n in ["agm_ps", "agm_inbox", "agm_quota", "agm_branch_get"] {
            let t = tools
                .iter()
                .find(|t| t["name"] == n)
                .unwrap_or_else(|| panic!("{n}"));
            assert!(t["description"].as_str().is_some_and(|d| !d.is_empty()));
            assert_eq!(t["inputSchema"]["type"], "object");
            assert_eq!(t["inputSchema"]["additionalProperties"], false);
        }
    });
}

#[test]
fn serve_answers_invalid_utf8_line_and_keeps_going() {
    with_ctx(|ctx| {
        let mut input: Vec<u8> = b"\xff\xfe\n".to_vec();
        input.extend_from_slice(b"{\"jsonrpc\":\"2.0\",\"id\":9,\"method\":\"ping\"}\n");
        let mut out = Vec::new();
        serve(ctx, &input[..], &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["error"]["code"], -32700);
        assert_eq!(lines[1]["id"], 9);
    });
}

#[test]
fn non_object_json_gets_invalid_request() {
    with_ctx(|ctx| {
        let mut out = Vec::new();
        serve(ctx, "5\n\"x\"\n".as_bytes(), &mut out).unwrap();
        let lines: Vec<Value> = String::from_utf8(out)
            .unwrap()
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        for l in &lines {
            assert_eq!(l["error"]["code"], -32600);
            assert_eq!(l["id"], Value::Null);
        }
    });
}

fn err_text(r: &Value) -> String {
    assert_eq!(r["result"]["isError"], true, "{r}");
    r["result"]["content"][0]["text"]
        .as_str()
        .unwrap()
        .to_string()
}

#[test]
fn tool_list_is_exactly_the_phase1_set() {
    with_ctx(|ctx| {
        let r =
            handle_message(ctx, &json!({"jsonrpc":"2.0","id":4,"method":"tools/list"})).unwrap();
        let names: Vec<&str> = r["result"]["tools"]
            .as_array()
            .unwrap()
            .iter()
            .map(|t| t["name"].as_str().unwrap())
            .collect();
        assert_eq!(
            names,
            [
                "agm_send",
                "agm_inbox",
                "agm_ps",
                "agm_quota",
                "agm_spawn",
                "agm_branch_get",
                "agm_branch_update"
            ]
        );
    });
}

#[test]
fn send_without_terminal_id_is_from_agent_never_user() {
    with_ctx(|ctx| {
        let r = call(
            ctx,
            "agm_send",
            json!({"to": "user", "kind": "REPORT", "text": "xong"}),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        let sc = &r["result"]["structuredContent"];
        assert_eq!(sc["from"], "agent");
        assert_eq!(sc["kind"], "BÁO CÁO");
        let items = ctx.store.inbox(true).unwrap();
        assert_eq!(items.len(), 1);
        assert_eq!(items[0].terminal.as_deref(), Some("agent"));
    });
}

#[test]
fn send_accepts_vietnamese_alias_and_rejects_unknown_kind() {
    with_ctx(|ctx| {
        let r = call(
            ctx,
            "agm_send",
            json!({"to": "user", "kind": "HỎI", "text": "ok?"}),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(r["result"]["structuredContent"]["kind"], "HỎI");
        let r = call(
            ctx,
            "agm_send",
            json!({"to": "user", "kind": "PING", "text": "x"}),
        );
        assert!(err_text(&r).contains("PING"));
    });
}

#[test]
fn spawn_respects_max_workers() {
    let dir = std::env::temp_dir().join(format!("agm-mcp-sp-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let cfg = dir.join("providers.toml");
    std::fs::write(&cfg, "[policy]\nmax_workers = 1\n").unwrap();
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc::default();
    let git = FakeGit;
    let ctx = McpCtx {
        store: &store,
        hc: &hc,
        git: &git,
        cfg_path: cfg,
        caller_terminal: None,
        clock: || 0,
    };
    store
        .upsert_terminal(&TerminalRow {
            address: "t1".into(),
            state: "live".into(),
            role: Some("worker".into()),
            ..Default::default()
        })
        .unwrap();
    let r = call(&ctx, "agm_spawn", json!({"template": "w", "task": "abc"}));
    assert!(err_text(&r).contains("max_workers"));
    assert!(!hc.calls.borrow().iter().any(|c| c.starts_with("run")));
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn branch_update_refuses_worker_caller() {
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc::default();
    let git = FakeGit;
    let ctx = McpCtx {
        store: &store,
        hc: &hc,
        git: &git,
        cfg_path: std::path::PathBuf::from("providers.toml"),
        caller_terminal: Some("t9".into()),
        clock: || 0,
    };
    store
        .upsert_terminal(&TerminalRow {
            address: "t9".into(),
            state: "live".into(),
            role: Some("worker".into()),
            ..Default::default()
        })
        .unwrap();
    let dir = std::env::temp_dir().join(format!("agm-mcp-bu-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    let file = dir.join("BRANCHES.md");
    let r = call(
        &ctx,
        "agm_branch_update",
        json!({"branches_file": file.to_string_lossy(), "entry": "feat/x", "fields": {"status": "wip"}}),
    );
    assert!(err_text(&r).contains("thợ"));
    assert!(!file.exists());
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn branch_update_writes_for_non_worker() {
    with_ctx(|ctx| {
        let dir = std::env::temp_dir().join(format!("agm-mcp-bw-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("BRANCHES.md");
        std::fs::write(&file, "").unwrap();
        let r = call(
            ctx,
            "agm_branch_update",
            json!({"branches_file": file.to_string_lossy(), "entry": "feat/x", "fields": {"status": "wip"}}),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        assert_eq!(r["result"]["structuredContent"]["created"], true);
        let r = call(
            ctx,
            "agm_branch_update",
            json!({"branches_file": file.to_string_lossy(), "entry": "feat/x", "fields": {"status": 5}}),
        );
        assert!(err_text(&r).contains("status"));
        let _ = std::fs::remove_dir_all(&dir);
    });
}

#[test]
fn no_tool_call_ever_kills() {
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
    let dir = std::env::temp_dir().join(format!("agm-mcp-nk-{}", std::process::id()));
    std::fs::create_dir_all(&dir).unwrap();
    std::fs::write(dir.join("BRANCHES.md"), "").unwrap();
    let file = dir.join("BRANCHES.md").to_string_lossy().to_string();
    live_term(&store, "t1", "live", None);
    let calls = [
        ("agm_send", json!({"to": "t1", "kind": "GIAO", "text": "x"})),
        ("agm_inbox", json!({})),
        ("agm_ps", json!({})),
        ("agm_quota", json!({})),
        ("agm_spawn", json!({"template": "w", "task": "abc"})),
        (
            "agm_branch_get",
            json!({"branches_file": file, "entry": "feat/x"}),
        ),
        (
            "agm_branch_update",
            json!({"branches_file": file, "entry": "feat/x", "fields": {"status": "wip"}}),
        ),
    ];
    for (n, a) in calls {
        call(&ctx, n, a);
    }
    let recorded = hc.calls.borrow();
    assert!(
        !recorded
            .iter()
            .any(|c| c.starts_with("kill") || c.starts_with("interrupt")),
        "{recorded:?}"
    );
    let _ = std::fs::remove_dir_all(&dir);
}

#[test]
fn branch_get_rejects_non_branches_file_name() {
    with_ctx(|ctx| {
        let r = call(
            ctx,
            "agm_branch_get",
            json!({"branches_file": "secrets.txt", "entry": "feat/x"}),
        );
        assert!(err_text(&r).contains("BRANCHES.md"));
    });
}

#[test]
fn unknown_argument_keys_are_rejected() {
    with_ctx(|ctx| {
        let r = call(ctx, "agm_ps", json!({"bogus": 1}));
        assert!(err_text(&r).contains("bogus"));
        let r = call(
            ctx,
            "agm_send",
            json!({"to": "user", "kind": "GIAO", "text": "x", "from": "user"}),
        );
        assert!(err_text(&r).contains("from"));
        assert!(ctx.store.inbox(false).unwrap().is_empty());
    });
}

#[test]
fn caller_terminal_user_is_never_used_as_sender() {
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc::default();
    let git = FakeGit;
    let ctx = McpCtx {
        store: &store,
        hc: &hc,
        git: &git,
        cfg_path: std::path::PathBuf::from("providers.toml"),
        caller_terminal: Some("User".into()),
        clock: || 0,
    };
    let r = call(
        &ctx,
        "agm_send",
        json!({"to": "user", "kind": "REPORT", "text": "x"}),
    );
    assert_eq!(r["result"]["isError"], false, "{r}");
    assert_eq!(r["result"]["structuredContent"]["from"], "agent");
    let items = store.inbox(true).unwrap();
    assert_eq!(items[0].terminal.as_deref(), Some("agent"));
}

#[test]
fn branch_update_without_terminal_logs_actor_agent() {
    with_ctx(|ctx| {
        let dir = std::env::temp_dir().join(format!("agm-mcp-la-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("BRANCHES.md");
        std::fs::write(&file, "").unwrap();
        let r = call(
            ctx,
            "agm_branch_update",
            json!({"branches_file": file.to_string_lossy(), "entry": "feat/x", "fields": {"status": "wip"}}),
        );
        assert_eq!(r["result"]["isError"], false, "{r}");
        let actor: String = ctx
            .store
            .conn()
            .query_row(
                "SELECT actor FROM actions WHERE kind = 'branch'",
                [],
                |row| row.get(0),
            )
            .unwrap();
        assert_eq!(actor, "agent");
        let _ = std::fs::remove_dir_all(&dir);
    });
}

#[test]
fn branch_tools_reject_relative_path_and_update_rejects_missing_file() {
    with_ctx(|ctx| {
        for (n, a) in [
            (
                "agm_branch_get",
                json!({"branches_file": "BRANCHES.md", "entry": "feat/x"}),
            ),
            (
                "agm_branch_update",
                json!({"branches_file": "BRANCHES.md", "entry": "feat/x", "fields": {"status": "wip"}}),
            ),
        ] {
            let r = call(ctx, n, a);
            assert_eq!(r["result"]["isError"], true, "{r}");
            assert!(err_text(&r).contains("branches_file"), "{r}");
        }
        let dir = std::env::temp_dir().join(format!("agm-mcp-miss-{}", std::process::id()));
        std::fs::create_dir_all(&dir).unwrap();
        let file = dir.join("BRANCHES.md");
        let r = call(
            ctx,
            "agm_branch_update",
            json!({"branches_file": file.to_string_lossy(), "entry": "feat/x", "fields": {"status": "wip"}}),
        );
        assert_eq!(r["result"]["isError"], true, "{r}");
        assert!(!file.exists());
        let r = call(
            ctx,
            "agm_branch_get",
            json!({"branches_file": file.to_string_lossy(), "entry": "feat/x"}),
        );
        assert_eq!(r["result"]["structuredContent"]["found"], false, "{r}");
        let _ = std::fs::remove_dir_all(&dir);
    });
}

#[test]
fn send_failed_delivery_is_tool_error() {
    let store = Store::open_in_memory().unwrap();
    let hc = FakeHc {
        fail_send: true,
        ..Default::default()
    };
    let git = FakeGit;
    let ctx = McpCtx {
        store: &store,
        hc: &hc,
        git: &git,
        cfg_path: std::path::PathBuf::from("providers.toml"),
        caller_terminal: None,
        clock: || 0,
    };
    live_term(&store, "t1", "live", None);
    let r = call(
        &ctx,
        "agm_send",
        json!({"to": "t1", "kind": "ASSIGN", "text": "x"}),
    );
    assert_eq!(r["result"]["isError"], true, "{r}");
    assert!(err_text(&r).contains("refused"), "{r}");
}

#[test]
fn serve_answers_last_line_without_trailing_newline() {
    with_ctx(|ctx| {
        let input = "{\"jsonrpc\":\"2.0\",\"id\":3,\"method\":\"ping\"}";
        let mut out = Vec::new();
        serve(ctx, input.as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let v: Value = serde_json::from_str(text.trim()).unwrap();
        assert_eq!(v["id"], 3);
    });
}

#[test]
fn serve_handles_crlf_lines() {
    with_ctx(|ctx| {
        let input = "{\"jsonrpc\":\"2.0\",\"id\":1,\"method\":\"ping\"}\r\n{\"jsonrpc\":\"2.0\",\"id\":2,\"method\":\"ping\"}\r\n";
        let mut out = Vec::new();
        serve(ctx, input.as_bytes(), &mut out).unwrap();
        let text = String::from_utf8(out).unwrap();
        let lines: Vec<Value> = text
            .lines()
            .map(|l| serde_json::from_str(l).unwrap())
            .collect();
        assert_eq!(lines.len(), 2);
        assert_eq!(lines[0]["id"], 1);
        assert_eq!(lines[1]["id"], 2);
        assert!(lines[0].get("error").is_none());
    });
}
