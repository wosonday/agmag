//! Task 2: scoped, read-only store queries for the TUI project view.
//! Seeds an in-memory store and pins `messages_involving`, `latest_turns`, `usage_buckets`.

use agmag::model::{Role, Source, TurnSummary, Usage, UsageRecord};
use agmag::project::TurnRow;
use agmag::store::{MessageRow, Store};

fn msg(ts_ms: i64, from: &str, to: &str, text: &str) -> MessageRow {
    MessageRow {
        ts_ms,
        from: from.into(),
        to: to.into(),
        kind: "assign".into(),
        text: text.into(),
        status: "sent".into(),
        ..Default::default()
    }
}

fn turn(ts_ms: i64, role: Role, summary: &str, byte_offset: u64) -> TurnSummary {
    TurnSummary {
        ts_ms,
        role,
        summary: summary.into(),
        byte_offset,
    }
}

/// Each row sums to `1 + 2 + 3 + 4 = 10`; the large `cache_read` must be excluded.
fn usage(ts_ms: i64, key: &str) -> UsageRecord {
    UsageRecord {
        dedupe_key: key.into(),
        ts_ms,
        model: None,
        usage: Usage {
            input_uncached: 1,
            cache_read: 1000,
            cache_write: 2,
            output: 3,
            reasoning: 4,
        },
    }
}

#[test]
fn messages_involving_matches_any_listed_party() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    // 30 messages. Every 5th is only between `x9` and `t2` (outside the project);
    // the rest involve `t1` (an address) or `ceo-name` (a name) on one side or the other.
    for i in 0..30i64 {
        let (from, to) = match i % 5 {
            0 => ("x9", "t2"),
            1 => ("t1", "t2"),
            2 => ("t2", "ceo-name"),
            3 => ("x9", "t1"),
            _ => ("ceo-name", "x9"),
        };
        store.add_message(&msg(i * 1000, from, to, &format!("m{i:02}")))?;
    }

    let parties = vec!["t1".to_string(), "ceo-name".to_string()];
    let rows = store.messages_involving(&parties, 20)?;

    assert_eq!(rows.len(), 20);
    // Newest first.
    assert!(rows.windows(2).all(|w| w[0].ts_ms > w[1].ts_ms));
    // Every returned row involves at least one listed party (so none is only x9 <-> t2).
    for r in &rows {
        assert!(
            parties.contains(&r.from) || parties.contains(&r.to),
            "row {} ({} -> {}) involves no listed party",
            r.id,
            r.from,
            r.to
        );
    }
    // Exactly the 20 newest matching messages, newest first.
    let expected: Vec<String> = (0..30i64)
        .filter(|i| i % 5 != 0)
        .rev()
        .take(20)
        .map(|i| format!("m{i:02}"))
        .collect();
    let got: Vec<String> = rows.iter().map(|r| r.text.clone()).collect();
    assert_eq!(got, expected);

    // Empty parties -> empty result, no SQL error.
    assert!(store.messages_involving(&[], 20)?.is_empty());
    Ok(())
}

#[test]
fn latest_turns_picks_newest_assistant_or_tool() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let s1 = store.session_id_for_path(Source::Codex, "p1")?;
    let s2 = store.session_id_for_path(Source::Codex, "p2")?;
    let s3 = store.session_id_for_path(Source::Codex, "p3")?; // no turns
    let s4 = store.session_id_for_path(Source::Codex, "p4")?; // tie on ts_ms

    // s1: newest overall is a user turn (must be ignored); tool is the newest assistant/tool.
    store.insert_turn(s1, &turn(300, Role::User, "user-newest", 30))?;
    store.insert_turn(s1, &turn(100, Role::Assistant, "a100", 10))?;
    store.insert_turn(s1, &turn(200, Role::Tool, "[tool] Bash", 20))?;
    // s2: assistant is the newest assistant/tool.
    store.insert_turn(s2, &turn(600, Role::User, "user-newest-2", 60))?;
    store.insert_turn(s2, &turn(400, Role::Tool, "[tool] Read", 40))?;
    store.insert_turn(s2, &turn(500, Role::Assistant, "a500", 50))?;
    // s4: equal ts_ms -> higher byte_offset wins.
    store.insert_turn(s4, &turn(700, Role::Assistant, "tie-low", 10))?;
    store.insert_turn(s4, &turn(700, Role::Assistant, "tie-high", 20))?;

    let map = store.latest_turns(&[s1, s2, s3, s4])?;
    assert_eq!(map.len(), 3); // s3 (no turns) is absent
    assert!(!map.contains_key(&s3));
    assert_eq!(
        map[&s1],
        TurnRow {
            ts_ms: 200,
            role: "tool".into(),
            summary: "[tool] Bash".into()
        }
    );
    assert_eq!(
        map[&s2],
        TurnRow {
            ts_ms: 500,
            role: "assistant".into(),
            summary: "a500".into()
        }
    );
    assert_eq!(
        map[&s4],
        TurnRow {
            ts_ms: 700,
            role: "assistant".into(),
            summary: "tie-high".into()
        }
    );

    // Empty sessions -> empty map.
    assert!(store.latest_turns(&[])?.is_empty());
    Ok(())
}

#[test]
fn usage_buckets_boundaries() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let s1 = store.session_id_for_path(Source::Codex, "p1")?;
    let s2 = store.session_id_for_path(Source::Codex, "p2")?; // no usage

    for ts in [0i64, 299_999, 300_000, 3_599_999, 3_600_000] {
        store.upsert_usage(s1, &usage(ts, &format!("k{ts}")))?;
    }

    let map = store.usage_buckets(&[s1, s2], 0, 300_000, 12)?;
    assert_eq!(map.len(), 2);
    // bucket0 = [0, 300000): ts 0 & 299999 -> 20; bucket1: ts 300000 -> 10;
    // bucket11 = [3300000, 3600000): ts 3599999 -> 10; ts 3600000 is outside the window.
    assert_eq!(map[&s1], vec![20u64, 10, 0, 0, 0, 0, 0, 0, 0, 0, 0, 10]);
    // Session with no usage -> 12 zeros, still present.
    assert_eq!(map[&s2], vec![0u64; 12]);

    // Empty sessions -> empty map.
    assert!(store.usage_buckets(&[], 0, 300_000, 12)?.is_empty());
    Ok(())
}

#[test]
fn usage_buckets_extreme_bounds_do_not_overflow() -> anyhow::Result<()> {
    let store = Store::open_in_memory()?;
    let s1 = store.session_id_for_path(Source::Codex, "p1")?;
    store.upsert_usage(s1, &usage(i64::MAX, "k"))?;

    // `since_ms + bucket_ms * n` overflows i64; the window end must saturate.
    let map = store.usage_buckets(&[s1], i64::MAX - 10, i64::MAX, 12)?;
    assert_eq!(map[&s1], vec![0u64; 12]);

    let map = store.usage_buckets(&[s1], 0, i64::MAX, 12)?;
    assert_eq!(map[&s1], vec![0u64; 12]);
    Ok(())
}
