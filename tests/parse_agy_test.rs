use std::fs;
use std::io::Write;

use rusqlite::Connection;
use serde_json::json;

use agmag::agy::{file_uri_to_path, ingest_agy};
use agmag::ask::{ask_workdir, record};
use agmag::config::Config;
use agmag::correlate::link_all;
use agmag::dashboard::snapshot;
use agmag::ingest::ingest_file;
use agmag::model::{LineOutcome, ParseEvent, Role, Source};
use agmag::parse::{LogParser, ParseError, agy::AgyParser};
use agmag::store::{Store, TerminalRow};

#[test]
fn agy_parser_extracts_usage_context_thinking_and_handles_missing_ts() {
    let mut p = AgyParser::default();
    let missing_ts = json!({
        "step_index": 0,
        "type": "PLANNER_RESPONSE",
        "model": "gemini-4-hydrogen-high",
        "input_tokens": 100,
        "output_tokens": 10
    });
    assert!(matches!(
        p.parse_line(&missing_ts.to_string(), 0),
        Err(ParseError::MissingTimestamp)
    ));

    let user_line = json!({
        "step_index": 0,
        "source": "USER_EXPLICIT",
        "type": "USER_INPUT",
        "status": "DONE",
        "created_at": "2026-11-18T13:09:38Z",
        "content": "<USER_REQUEST>\nKiểm tra token\n</USER_REQUEST>\n<ADDITIONAL_METADATA>\nignored\n</ADDITIONAL_METADATA>"
    });
    match p.parse_line(&user_line.to_string(), 0).unwrap() {
        LineOutcome::Events(ev) => {
            assert!(
                matches!(&ev[0], ParseEvent::Turn(t) if t.role == Role::User && t.summary == "Kiểm tra token")
            );
        }
        other => panic!("expected events, got {other:?}"),
    }

    let planner_line = json!({
        "step_index": 1,
        "source": "MODEL",
        "type": "PLANNER_RESPONSE",
        "status": "DONE",
        "created_at": "2026-11-18T13:09:47Z",
        "model": "gemini-4-hydrogen-high",
        "input_tokens": 2430,
        "cache_read_tokens": 8466,
        "output_tokens": 120,
        "thinking_tokens": 80,
        "tool_calls": [{"name": "read_file", "args": {}}]
    });
    match p.parse_line(&planner_line.to_string(), 250).unwrap() {
        LineOutcome::Events(ev) => {
            assert!(matches!(&ev[0], ParseEvent::Model(m) if m == "gemini-4-hydrogen-high"));
            let ParseEvent::Usage(u) = &ev[1] else {
                panic!("expected usage")
            };
            assert_eq!(u.dedupe_key, "step:1");
            assert_eq!(u.usage.input_uncached, 2430);
            assert_eq!(u.usage.cache_read, 8466);
            assert_eq!(u.usage.output, 120);
            assert_eq!(u.usage.reasoning, 80);
            assert!(matches!(&ev[2], ParseEvent::Context(c) if c.used_tokens == 10896));
            assert!(
                matches!(&ev[3], ParseEvent::Turn(t) if t.role == Role::Assistant && t.summary == "[tool] read_file")
            );
        }
        other => panic!("expected events, got {other:?}"),
    }

    // Dòng kế tiếp thiếu created_at -> kế thừa last_ts thay vì ts=0, và chỉ có thinking_tokens vẫn ghi usage.
    let thinking_only = json!({
        "step_index": 2,
        "type": "PLANNER_RESPONSE",
        "thinking_tokens": 42
    });
    match p.parse_line(&thinking_only.to_string(), 400).unwrap() {
        LineOutcome::Events(ev) => {
            let ParseEvent::Usage(u) = &ev[0] else {
                panic!("expected usage")
            };
            assert!(u.ts_ms > 0);
            assert_eq!(u.usage.output, 42);
            assert_eq!(u.usage.reasoning, 42);
        }
        other => panic!("expected events, got {other:?}"),
    }

    for kind in ["GENERIC", "SYSTEM_MESSAGE", "ERROR_MESSAGE"] {
        let noise = json!({"step_index": 3, "type": kind, "created_at": "2026-11-18T13:09:50Z", "content": "ok"});
        assert_eq!(
            p.parse_line(&noise.to_string(), 500).unwrap(),
            LineOutcome::Ignored
        );
    }
}

fn write_transcript(
    root: &std::path::Path,
    conv: &str,
    lines: &[serde_json::Value],
) -> anyhow::Result<std::path::PathBuf> {
    let dir = root
        .join("brain")
        .join(conv)
        .join(".system_generated")
        .join("logs");
    fs::create_dir_all(&dir)?;
    let p = dir.join("transcript.jsonl");
    let mut body = String::new();
    for l in lines {
        body.push_str(&l.to_string());
        body.push('\n');
    }
    fs::write(&p, body)?;
    Ok(p)
}

#[test]
fn ingest_agy_reads_summaries_correlates_and_avoids_ask_race() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path();

    let ask_uri = format!(
        "file:///{}",
        ask_workdir()
            .to_string_lossy()
            .replace('\\', "/")
            .trim_start_matches('/')
    );
    let db = Connection::open(root.join("conversation_summaries.db"))?;
    db.execute_batch(
        "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, workspace_uris TEXT NOT NULL);",
    )?;
    for (id, uri) in [
        ("conv-live", r#"["file:///D:/Project/agmag"]"#),
        ("conv-ask-first", r#"["file:///D:/Project/agmag"]"#),
        ("conv-ask-race", r#"["file:///D:/Project/agmag"]"#),
        ("conv-ask-dir", &format!(r#"["{ask_uri}"]"#)),
    ] {
        db.execute(
            "INSERT INTO conversation_summaries VALUES (?1, ?2)",
            [id, uri],
        )?;
    }
    drop(db);

    let l0 = json!({"step_index":0,"type":"USER_INPUT","created_at":"2026-11-18T13:09:38Z","content":"<USER_REQUEST>\nhi\n</USER_REQUEST>"});
    let l1 = json!({"step_index":1,"type":"PLANNER_RESPONSE","created_at":"2026-11-18T13:09:40Z","model":"gemini-4-hydrogen-high","input_tokens":1000,"cache_read_tokens":200,"output_tokens":50,"content":"hello"});
    let live_path = write_transcript(root, "conv-live", &[l0.clone(), l1.clone()])?;
    write_transcript(root, "conv-ask-first", &[l0.clone(), l1.clone()])?;
    write_transcript(root, "conv-ask-race", &[l0.clone(), l1.clone()])?;
    write_transcript(root, "conv-ask-dir", &[l0, l1])?;

    let store = Store::open_in_memory()?;
    // Đường 1: `agm ask` ghi trước khi `ingest_agy` chạy
    record(
        &store,
        &json!({
            "status": "SUCCESS",
            "conversation_id": "conv-ask-first",
            "model": "gemini-4-argon-high",
            "response": "ok",
            "usage": {"input_tokens": 500, "cache_read_tokens": 100, "output_tokens": 20, "thinking_tokens": 5}
        }),
        "q",
        None,
        1_795_007_378_000,
    )?;

    // `ingest_agy` chạy trong khi `conv-ask-race` chưa gọi `record()` và `conv-ask-dir` nằm ở thư mục tạm ask
    let stats = ingest_agy(&store, root, 0)?;
    assert_eq!(stats.files, 2); // conv-live + conv-ask-race (bỏ qua conv-ask-first và conv-ask-dir)

    // Đường 2: `conv-ask-race` gọi `record()` SAU khi `ingest_agy` đã chạy -> phải gộp/xóa bản trùng dạng tệp
    record(
        &store,
        &json!({
            "status": "SUCCESS",
            "conversation_id": "conv-ask-race",
            "model": "gemini-4-argon-high",
            "response": "ok",
            "usage": {"input_tokens": 600, "cache_read_tokens": 100, "output_tokens": 25, "thinking_tokens": 10}
        }),
        "q2",
        None,
        1_795_007_390_000,
    )?;
    let race_rows: (i64, i64) = store.conn().query_row(
        "SELECT COUNT(DISTINCT s.id), COALESCE(SUM(u.output), 0)
         FROM sessions s LEFT JOIN usage u ON u.session = s.id
         WHERE s.source = 'agy' AND s.session_id = 'conv-ask-race'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(race_rows, (1, 25));

    // Append thêm 1 lượt rồi ingest lại
    let l2 = json!({"step_index":2,"type":"PLANNER_RESPONSE","created_at":"2026-11-18T13:10:00Z","model":"gemini-4-hydrogen-high","input_tokens":300,"cache_read_tokens":1250,"output_tokens":75,"content":"done"});
    writeln!(
        fs::OpenOptions::new().append(true).open(&live_path)?,
        "{l2}"
    )?;
    let stats2 = ingest_agy(&store, root, 0)?;
    assert_eq!(stats2.lines, 1);

    // Ghép terminal antigravity -> phiên agy và kiểm tra dashboard snapshot
    let started_ms = agmag::model::parse_rfc3339_ms("2026-11-18T13:09:38Z").unwrap();
    store.upsert_terminal(&TerminalRow {
        address: "t34".into(),
        app: Some("antigravity".into()),
        cwd: Some(r"D:\Project\agmag".into()),
        state: "live".into(),
        created_ms: started_ms - 5_000,
        ..Default::default()
    })?;
    assert_eq!(link_all(&store)?, 1);

    let snap = snapshot(&store, &Config::builtin(), started_ms + 30_000)?;
    let tv = snap.terminals.iter().find(|t| t.address == "t34").unwrap();
    // (1000+200+50) + (300+1250+75) = 2875
    assert_eq!(tv.tokens, Some(2875));

    let ask_first_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM sessions WHERE source = 'agy' AND session_id = 'conv-ask-first'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(ask_first_count, 1);
    let ask_dir_count: i64 = store.conn().query_row(
        "SELECT COUNT(*) FROM sessions WHERE source = 'agy' AND session_id = 'conv-ask-dir'",
        [],
        |r| r.get(0),
    )?;
    assert_eq!(ask_dir_count, 0);

    let srow: (Option<String>, i64, Option<i64>) = store.conn().query_row(
        "SELECT cwd, unknown_lines, started_ms FROM sessions WHERE session_id = 'conv-live'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)),
    )?;
    assert!(srow.0.is_some());
    assert_eq!(srow.1, 0);
    assert_eq!(srow.2, Some(started_ms));
    Ok(())
}

#[test]
fn ingest_agy_replaced_file_resets_and_preserves_identity() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path();
    let l0 = json!({"step_index":0,"type":"USER_INPUT","created_at":"2026-11-18T13:09:38Z","content":"<USER_REQUEST>\nold\n</USER_REQUEST>"});
    let l1 = json!({"step_index":1,"type":"PLANNER_RESPONSE","created_at":"2026-11-18T13:09:40Z","model":"gemini-4-hydrogen-high","input_tokens":900,"cache_read_tokens":0,"output_tokens":90,"content":"old"});
    let p = write_transcript(root, "conv-rep", &[l0, l1])?;

    let store = Store::open_in_memory()?;
    ingest_agy(&store, root, 0)?;

    let new_l0 = json!({"step_index":0,"type":"PLANNER_RESPONSE","created_at":"2026-11-18T13:15:00Z","model":"gemini-4-hydrogen-high","input_tokens":111,"cache_read_tokens":0,"output_tokens":11,"content":"new"});
    fs::write(&p, format!("{new_l0}\n"))?;
    let _ = ingest_file(&store, Source::Agy, &p)?;
    ingest_agy(&store, root, 0)?;

    let totals: (String, i64) = store.conn().query_row(
        "SELECT s.session_id, COALESCE(SUM(u.input_uncached), 0)
         FROM sessions s JOIN usage u ON u.session = s.id
         WHERE s.source = 'agy'",
        [],
        |r| Ok((r.get(0)?, r.get(1)?)),
    )?;
    assert_eq!(totals, ("conv-rep".into(), 111));
    Ok(())
}

#[test]
fn ingest_agy_resumed_line_without_created_at_inherits_session_last_ts() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path();
    let l0 = json!({"step_index":0,"type":"USER_INPUT","created_at":"2026-11-18T13:09:38Z","content":"<USER_REQUEST>\nhi\n</USER_REQUEST>"});
    let p = write_transcript(root, "conv-res", &[l0])?;

    let store = Store::open_in_memory()?;
    ingest_agy(&store, root, 0)?;

    let l1 = json!({"step_index":1,"type":"PLANNER_RESPONSE","model":"gemini-4-hydrogen-high","input_tokens":55,"output_tokens":7,"content":"ok"});
    writeln!(fs::OpenOptions::new().append(true).open(&p)?, "{l1}")?;
    let stats = ingest_agy(&store, root, 0)?;
    assert_eq!((stats.lines, stats.bad), (1, 0));

    let ts: i64 = store
        .conn()
        .query_row("SELECT ts_ms FROM usage", [], |r| r.get(0))?;
    assert_eq!(
        ts,
        agmag::model::parse_rfc3339_ms("2026-11-18T13:09:38Z").unwrap()
    );
    Ok(())
}

fn agy_tokens(store: &Store, conv: &str) -> anyhow::Result<i64> {
    Ok(store.conn().query_row(
        "SELECT COALESCE(SUM(u.input_uncached + u.cache_read + u.output), 0)
         FROM usage u JOIN sessions s ON s.id = u.session
         WHERE s.source = 'agy' AND s.session_id = ?1",
        [conv],
        |r| r.get(0),
    )?)
}

#[test]
fn ask_without_usage_never_drops_transcript_tokens() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path();
    let l1 = json!({"step_index":1,"type":"PLANNER_RESPONSE","created_at":"2026-11-18T13:09:40Z","model":"gemini-4-hydrogen-high","input_tokens":1000,"cache_read_tokens":200,"output_tokens":50,"content":"x"});
    write_transcript(root, "conv-a", std::slice::from_ref(&l1))?;
    write_transcript(root, "conv-b", &[l1])?;
    let store = Store::open_in_memory()?;
    let no_usage =
        |conv: &str| json!({"status": "SUCCESS", "conversation_id": conv, "response": "ok"});

    // (a) daemon quét transcript trước, rồi `agm ask` trả JSON thiếu `usage` → token đã có phải còn nguyên.
    ingest_agy(&store, root, 0)?;
    record(&store, &no_usage("conv-a"), "q", None, 1_795_007_378_000)?;
    assert_eq!(agy_tokens(&store, "conv-a")?, 1250);

    // (b) `agm ask` thiếu `usage` ghi trước → lần quét sau vẫn phải đọc transcript.
    record(&store, &no_usage("conv-b"), "q", None, 1_795_007_378_000)?;
    ingest_agy(&store, root, 0)?;
    assert_eq!(agy_tokens(&store, "conv-b")?, 1250);
    Ok(())
}

#[test]
fn ingest_agy_backfills_cwd_written_after_transcript() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let root = tmp.path();
    let l1 = json!({"step_index":1,"type":"PLANNER_RESPONSE","created_at":"2026-11-18T13:09:40Z","model":"gemini-4-hydrogen-high","input_tokens":10,"output_tokens":5,"content":"x"});
    write_transcript(root, "conv-late", &[l1])?;
    let store = Store::open_in_memory()?;
    ingest_agy(&store, root, 0)?; // chưa có conversation_summaries.db
    let db = Connection::open(root.join("conversation_summaries.db"))?;
    db.execute_batch(
        "CREATE TABLE conversation_summaries (conversation_id TEXT PRIMARY KEY, workspace_uris TEXT NOT NULL);
         INSERT INTO conversation_summaries VALUES ('conv-late', '[\"file:///D:/Project/late\"]');",
    )?;
    drop(db);
    // Transcript không đổi (mtime cũ hơn mốc quét) nhưng cwd vẫn phải được bổ sung.
    ingest_agy(&store, root, i64::MAX)?;
    let cwd: Option<String> = store.conn().query_row(
        "SELECT cwd FROM sessions WHERE session_id = 'conv-late'",
        [],
        |r| r.get(0),
    )?;
    assert!(
        cwd.is_some_and(|c| c.ends_with("late")),
        "cwd chưa được bổ sung"
    );
    Ok(())
}

#[test]
fn file_uri_decodes_windows_spaces_localhost_and_unc() {
    let p = file_uri_to_path("file:///D:/My%20Projects/agmag").unwrap();
    let loc = file_uri_to_path("file://localhost/C:/repo/a").unwrap();
    let unc = file_uri_to_path("file://server/share/dir").unwrap();
    if cfg!(windows) {
        assert_eq!(p, r"D:\My Projects\agmag");
        assert_eq!(loc, r"C:\repo\a");
        assert_eq!(unc, r"\\server\share\dir");
    } else {
        assert_eq!(p, "D:/My Projects/agmag");
        assert_eq!(loc, "C:/repo/a");
        assert_eq!(unc, "//server/share/dir");
    }
}
