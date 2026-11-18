use agmag::ask::record;
use agmag::store::Store;
use serde_json::json;

#[test]
fn ask_records_agy_usage() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let out = json!({"status": "SUCCESS", "response": "Kết luận — https://x", "conversation_id": "conv-1",
        "usage": {"input_tokens": 10682, "output_tokens": 357, "thinking_tokens": 54, "cache_read_tokens": 682, "total_tokens": 11039}});
    let text = record(&store, &out, "tra cứu X", Some("gemini-4-argon-medium"), 5)?;
    assert_eq!(text, "Kết luận — https://x");
    let row: (String, String, i64, i64, i64, i64) = store.conn().query_row(
        "SELECT s.path, u.model, u.input_uncached, u.cache_read, u.output, u.reasoning FROM usage u JOIN sessions s ON s.id = u.session",
        [],
        |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?, r.get(3)?, r.get(4)?, r.get(5)?)),
    )?;
    // thinking không nằm trong output_tokens của agy (input + output = total) → cộng vào output.
    assert_eq!(row, ("agy://conv-1".into(), "gemini-4-argon-medium".into(), 10000, 682, 411, 54));
    let turns: i64 = store.conn().query_row("SELECT COUNT(*) FROM turns", [], |r| r.get(0))?;
    assert_eq!(turns, 2);
    Ok(())
}

#[test]
fn ask_error_status_is_error() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let out = json!({"status": "ERROR", "response": "quota"});
    assert!(record(&store, &out, "q", None, 1).is_err());
    Ok(())
}
