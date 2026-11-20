use std::path::Path;

use agmag::opencode::ingest_opencode;
use agmag::store::Store;
use rusqlite::{params, Connection};

/// Dựng một opencode.db tối thiểu theo cấu trúc OpenCode v2 đã thấy ở P0.
fn make_db(path: &Path) -> Connection {
    let c = Connection::open(path).unwrap();
    c.execute_batch(
        "CREATE TABLE session_v2 (id TEXT PRIMARY KEY, project_id TEXT, directory TEXT NOT NULL,
             title TEXT, version TEXT NOT NULL, agent TEXT, model TEXT,
             time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL);
         CREATE TABLE session_message (id TEXT PRIMARY KEY, session_id TEXT NOT NULL, type TEXT NOT NULL,
             seq INTEGER NOT NULL, time_created INTEGER NOT NULL, time_updated INTEGER NOT NULL, data TEXT NOT NULL);
         CREATE TABLE credential (id TEXT PRIMARY KEY, value TEXT NOT NULL);
         INSERT INTO credential VALUES ('c1', 'SECRET');
         INSERT INTO session_v2 VALUES ('ses_a', 'p', 'D:\\Dự án\\wt', 'agmag t1', '2.3.2', 'agm-worker', 'gpt-6.1-sol', 1000, 5000);",
    )
    .unwrap();
    c
}

fn add(c: &Connection, id: &str, seq: i64, kind: &str, t: i64, data: &str) {
    c.execute(
        "INSERT INTO session_message VALUES (?1, 'ses_a', ?2, ?3, ?4, ?4, ?5)",
        params![id, kind, seq, t, data],
    )
    .unwrap();
}

const ASSISTANT: &str = r#"{"time":{"created":2000},"agent":"agm-worker","model":{"id":"gpt-6.1-sol","providerID":"openai"},
  "content":[{"type":"reasoning","text":""},{"type":"text","text":"Đã commit t1"}],
  "tokens":{"input":191,"output":404,"reasoning":12,"cache":{"read":45440,"write":3}}}"#;

fn count(store: &Store, sql: &str) -> i64 {
    store.conn().query_row(sql, [], |r| r.get(0)).unwrap()
}

#[test]
fn opencode_usage_normalized() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    add(&c, "msg_u", 1, "user", 1500, r#"{"time":{"created":1500},"text":"làm t1"}"#);
    add(&c, "msg_a", 2, "assistant", 2000, ASSISTANT);
    let store = Store::open_in_memory()?;
    ingest_opencode(&store, &db)?;
    let row: (String, String, String, i64, i64, i64, i64, i64) = store.conn().query_row(
        "SELECT s.path, s.cwd, u.model, u.input_uncached, u.cache_read, u.cache_write, u.output, u.reasoning
         FROM usage u JOIN sessions s ON s.id = u.session",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?, r.get(6)?, r.get(7)?)),
    )?;
    assert_eq!(row, ("opencode://ses_a".into(), r"D:\Dự án\wt".into(), "openai/gpt-6.1-sol".into(), 191, 45440, 3, 404, 12));
    // Ngữ cảnh = input + cache đọc + cache ghi của lượt cuối.
    assert_eq!(count(&store, "SELECT ctx_used FROM sessions"), 191 + 45440 + 3);
    Ok(())
}

#[test]
fn opencode_turns() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    add(&c, "msg_u", 1, "user", 1500, r#"{"time":{"created":1500},"text":"làm t1"}"#);
    add(&c, "msg_a", 2, "assistant", 2000, ASSISTANT);
    add(&c, "msg_i", 3, "idle", 2100, r#"{"outcome":"succeeded"}"#);
    let store = Store::open_in_memory()?;
    ingest_opencode(&store, &db)?;
    let turns: Vec<(String, String)> = store
        .conn()
        .prepare("SELECT role, summary FROM turns ORDER BY byte_offset")?
        .query_map([], |r| Ok((r.get(0)?, r.get(1)?)))?
        .collect::<Result<_, _>>()?;
    assert_eq!(turns, [("user".into(), "làm t1".into()), ("assistant".into(), "Đã commit t1".into())]);
    Ok(())
}

#[test]
fn opencode_incremental() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    add(&c, "msg_a", 2, "assistant", 2000, ASSISTANT);
    let store = Store::open_in_memory()?;
    let first = ingest_opencode(&store, &db)?;
    let second = ingest_opencode(&store, &db)?;
    assert_eq!((first.lines, second.lines), (1, 0));
    add(&c, "msg_b", 3, "assistant", 3000, &ASSISTANT.replace("\"output\":404", "\"output\":10"));
    let third = ingest_opencode(&store, &db)?;
    assert_eq!(third.lines, 1);
    assert_eq!(count(&store, "SELECT SUM(output) FROM usage"), 414);
    Ok(())
}

#[test]
fn opencode_bad_json_counted() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    add(&c, "msg_x", 1, "assistant", 2000, "{oops");
    let store = Store::open_in_memory()?;
    assert_eq!(ingest_opencode(&store, &db)?.bad, 1);
    assert_eq!(count(&store, "SELECT bad_lines FROM sessions"), 1);
    Ok(())
}

#[test]
fn opencode_missing_db_is_ok() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let stats = ingest_opencode(&store, Path::new("Z:/khong/ton/tai/opencode.db"))?;
    assert_eq!(stats.files, 0);
    Ok(())
}

#[test]
fn opencode_same_millisecond_message_not_skipped() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    add(&c, "msg_a", 2, "assistant", 2000, ASSISTANT);
    let store = Store::open_in_memory()?;
    ingest_opencode(&store, &db)?;
    // Message khác commit sau lần quét nhưng có cùng time_updated = 2000.
    add(&c, "msg_b", 3, "assistant", 2000, &ASSISTANT.replace("\"output\":404", "\"output\":10"));
    ingest_opencode(&store, &db)?;
    assert_eq!(count(&store, "SELECT SUM(output) FROM usage"), 414);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM turns"), 2);
    Ok(())
}

/// Model mới (Go, NVIDIA, ...) không cần khai báo trước: lưu đủ `providerID/id` để phân biệt nguồn tính phí.
#[test]
fn opencode_model_keeps_provider_prefix() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    let go = ASSISTANT.replace(r#""id":"gpt-6.1-sol","providerID":"openai""#, r#""id":"deepseek-v4-pro","providerID":"opencode-go""#);
    let nv = ASSISTANT.replace(r#""id":"gpt-6.1-sol","providerID":"openai""#, r#""id":"moonshotai/kimi-k3.1-pro","providerID":"nvidia-kimi""#);
    let bare = ASSISTANT.replace(r#","providerID":"openai""#, "");
    add(&c, "msg_go", 1, "assistant", 2000, &go);
    add(&c, "msg_nv", 2, "assistant", 2001, &nv);
    add(&c, "msg_bare", 3, "assistant", 2002, &bare);
    let store = Store::open_in_memory()?;
    ingest_opencode(&store, &db)?;
    let models: Vec<String> = store
        .conn()
        .prepare("SELECT model FROM usage ORDER BY ts_ms")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    assert_eq!(models, ["opencode-go/deepseek-v4-pro", "nvidia-kimi/moonshotai/kimi-k3.1-pro", "gpt-6.1-sol"]);
    assert_eq!(store.conn().query_row("SELECT model FROM sessions", [], |r| r.get::<_, String>(0))?, "gpt-6.1-sol");
    Ok(())
}

/// CSDL agm cũ đã lưu model trần (không provider) → lần quét đầu sau nâng cấp nạp lại toàn bộ, đúng một lần.
#[test]
fn opencode_legacy_bare_models_are_rescanned_once() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let db = tmp.path().join("opencode.db");
    let c = make_db(&db);
    add(&c, "msg_a", 2, "assistant", 2000, ASSISTANT);
    let store = Store::open_in_memory()?;
    ingest_opencode(&store, &db)?;
    // Giả lập dữ liệu do bản agm cũ ghi: model trần và chưa có cờ phiên bản khóa model.
    store.conn().execute_batch("UPDATE usage SET model = 'gpt-6.1-sol'; DELETE FROM settings WHERE key = 'opencode_model_key';")?;
    assert_eq!(ingest_opencode(&store, &db)?.lines, 1);
    assert_eq!(store.conn().query_row("SELECT model FROM usage", [], |r| r.get::<_, String>(0))?, "openai/gpt-6.1-sol");
    assert_eq!(count(&store, "SELECT COUNT(*) FROM usage"), 1);
    assert_eq!(count(&store, "SELECT COUNT(*) FROM turns"), 1);
    assert_eq!(ingest_opencode(&store, &db)?.lines, 0);
    Ok(())
}
