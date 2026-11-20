use std::fs;
use std::io::Write;

use agmag::ingest::{IngestStats, Roots, discover, ingest_all, ingest_file};
use agmag::model::Source;
use agmag::store::Store;

const CODEX: &str = include_str!("fixtures/codex/mini.jsonl");

fn count(store: &Store, table: &str) -> i64 {
    store
        .conn()
        .query_row(&format!("SELECT COUNT(*) FROM {table}"), [], |r| r.get(0))
        .unwrap()
}

#[test]
fn ingest_mini_codex() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("mini.jsonl");
    fs::write(&path, CODEX)?;
    let store = Store::open_in_memory()?;
    let stats = ingest_file(&store, Source::Codex, &path)?;
    assert_eq!(
        stats,
        IngestStats {
            files: 1,
            lines: 7,
            events: 6,
            unknown: 1,
            bad: 0,
            skipped: 0
        }
    );
    assert_eq!(count(&store, "usage"), 1);
    assert_eq!(count(&store, "turns"), 1);
    assert_eq!(count(&store, "quota"), 2);
    Ok(())
}

#[test]
fn partial_line_is_deferred() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("partial.jsonl");
    let usage = CODEX.lines().nth(3).unwrap();
    let split = usage.len() / 2;
    fs::write(
        &path,
        format!("{}\n{}", CODEX.lines().next().unwrap(), &usage[..split]),
    )?;
    let store = Store::open_in_memory()?;
    let first = ingest_file(&store, Source::Codex, &path)?;
    assert_eq!((first.lines, first.bad), (1, 0));
    writeln!(
        fs::OpenOptions::new().append(true).open(&path)?,
        "{}",
        &usage[split..]
    )?;
    let second = ingest_file(&store, Source::Codex, &path)?;
    assert_eq!((second.lines, second.bad), (1, 0));
    let totals: (i64, i64) =
        store
            .conn()
            .query_row("SELECT COUNT(*), SUM(output) FROM usage", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
    assert_eq!(totals, (1, 285));
    Ok(())
}

#[test]
fn rescan_is_noop() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("mini.jsonl");
    fs::write(&path, CODEX)?;
    let store = Store::open_in_memory()?;
    ingest_file(&store, Source::Codex, &path)?;
    let before = store.cursor(&agmag::ingest::path_key(&path))?.unwrap();
    let second = ingest_file(&store, Source::Codex, &path)?;
    assert_eq!(
        second,
        IngestStats {
            files: 1,
            ..Default::default()
        }
    );
    assert_eq!(
        store.cursor(&agmag::ingest::path_key(&path))?.unwrap(),
        before
    );
    assert_eq!(count(&store, "usage"), 1);
    Ok(())
}

#[test]
fn replaced_file_rereads() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("mini.jsonl");
    fs::write(&path, CODEX)?;
    let store = Store::open_in_memory()?;
    ingest_file(&store, Source::Codex, &path)?;
    fs::write(&path, "{\"type\":\"replacement\"}\n")?;
    let second = ingest_file(&store, Source::Codex, &path)?;
    assert_eq!((second.lines, second.unknown, second.bad), (1, 1, 0));
    // Also detect a changed prefix when length is unchanged.
    fs::write(&path, "{\"type\":\"other_event\"}\n")?;
    assert_eq!(ingest_file(&store, Source::Codex, &path)?.lines, 1);
    Ok(())
}

#[test]
fn model_survives_resume() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("resume.jsonl");
    fs::write(
        &path,
        format!("{}\n", CODEX.lines().take(2).collect::<Vec<_>>().join("\n")),
    )?;
    let store = Store::open_in_memory()?;
    ingest_file(&store, Source::Codex, &path)?;
    writeln!(
        fs::OpenOptions::new().append(true).open(&path)?,
        "{}",
        CODEX.lines().nth(3).unwrap()
    )?;
    assert_eq!(ingest_file(&store, Source::Codex, &path)?.lines, 1);
    let model: Option<String> = store
        .conn()
        .query_row("SELECT model FROM usage", [], |r| r.get(0))?;
    assert_eq!(model.as_deref(), Some("gpt-6.1-sol"));
    Ok(())
}

#[test]
fn discover_filters() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let roots = Roots {
        codex: tmp.path().join("codex"),
        claude: tmp.path().join("claude"),
    };
    assert!(discover(&roots, 0).is_empty());
    let codex = roots
        .codex
        .join("2026")
        .join("11")
        .join("18")
        .join("rollout-a.jsonl");
    let claude = roots.claude.join("p").join("s.jsonl");
    fs::create_dir_all(codex.parent().unwrap())?;
    fs::create_dir_all(claude.parent().unwrap())?;
    fs::write(&codex, CODEX)?;
    fs::write(&claude, "")?;
    fs::write(roots.codex.join("x.txt"), "")?;
    fs::write(roots.codex.join("not-rollout.jsonl"), "")?;
    fs::write(roots.claude.join("old.jsonl"), "")?;
    fs::File::options()
        .write(true)
        .open(roots.claude.join("old.jsonl"))?
        .set_modified(std::time::UNIX_EPOCH + std::time::Duration::from_secs(1))?;
    let found = discover(&roots, 2000);
    assert_eq!(found.len(), 2);
    assert!(found.contains(&(Source::Codex, codex)));
    assert!(found.contains(&(Source::Claude, claude)));
    assert!(discover(&roots, i64::MAX).is_empty());
    Ok(())
}

#[test]
fn ingest_all_aggregates_sources() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let roots = Roots {
        codex: tmp.path().join("codex"),
        claude: tmp.path().join("claude"),
    };
    fs::create_dir_all(&roots.codex)?;
    fs::create_dir_all(&roots.claude)?;
    fs::write(roots.codex.join("rollout-a.jsonl"), CODEX)?;
    fs::write(
        roots.claude.join("s.jsonl"),
        include_str!("fixtures/claude/mini.jsonl"),
    )?;
    let store = Store::open_in_memory()?;
    let stats = ingest_all(&store, &roots, 0)?;
    assert_eq!(
        (stats.files, stats.lines, stats.unknown, stats.bad),
        (2, 12, 2, 0)
    );
    assert_eq!(count(&store, "usage"), 2);
    assert_eq!(ingest_all(&store, &roots, 0)?.lines, 0);
    Ok(())
}

#[test]
fn bad_unknown_and_lossy_utf8_are_counted() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("lossy.jsonl");
    fs::write(
        &path,
        b"not json\n{\"type\":\"unknown\"}\n{\"type\":\"\xff\"}\n",
    )?;
    let store = Store::open_in_memory()?;
    let stats = ingest_file(&store, Source::Codex, &path)?;
    assert_eq!((stats.lines, stats.bad, stats.unknown), (3, 1, 2));
    let stored: (i64, i64) =
        store
            .conn()
            .query_row("SELECT unknown_lines, bad_lines FROM sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
    assert_eq!(stored, (2, 1));
    Ok(())
}

#[test]
fn file_transaction_rolls_back_on_store_error() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("rollback.jsonl");
    fs::write(&path, CODEX)?;
    let store = Store::open_in_memory()?;
    store.conn().execute_batch(
        "CREATE TRIGGER reject_cursor BEFORE INSERT ON cursors BEGIN SELECT RAISE(ABORT, 'test failure'); END;"
    )?;
    assert!(ingest_file(&store, Source::Codex, &path).is_err());
    for table in ["sessions", "usage", "turns", "quota", "cursors"] {
        assert_eq!(count(&store, table), 0, "{table}");
    }
    assert!(store.conn().is_autocommit());
    store.conn().execute_batch("DROP TRIGGER reject_cursor")?;
    assert_eq!(ingest_file(&store, Source::Codex, &path)?.lines, 7);
    Ok(())
}

#[test]
fn reread_from_zero_resets_session_data() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let path = tmp.path().join("mini.jsonl");
    fs::write(
        &path,
        format!(
            "{CODEX}{{oops
"
        ),
    )?;
    let store = Store::open_in_memory()?;
    ingest_file(&store, Source::Codex, &path)?;
    // Thay nội dung (đầu tệp khác) -> đọc lại từ 0; dữ liệu cũ của phiên phải biến mất.
    let usage_line = CODEX.lines().nth(3).unwrap().replace("resp_1", "resp_new");
    fs::write(
        &path,
        format!(
            "{usage_line}
"
        ),
    )?;
    ingest_file(&store, Source::Codex, &path)?;
    let keys: Vec<String> = store
        .conn()
        .prepare("SELECT dedupe_key FROM usage")?
        .query_map([], |r| r.get(0))?
        .collect::<Result<_, _>>()?;
    assert_eq!(keys, ["resp_new"]);
    assert_eq!(count(&store, "turns"), 0);
    let stats: (i64, i64) =
        store
            .conn()
            .query_row("SELECT unknown_lines, bad_lines FROM sessions", [], |r| {
                Ok((r.get(0)?, r.get(1)?))
            })?;
    assert_eq!(stats, (0, 0));
    Ok(())
}

#[test]
fn head_hash_is_stable_fnv1a() {
    assert_eq!(agmag::ingest::head_hash(b""), 0xcbf29ce484222325);
    assert_eq!(agmag::ingest::head_hash(b"a"), 0xaf63dc4c8601ec8c);
}

#[cfg(windows)]
#[test]
fn ingest_all_skips_locked_file() -> anyhow::Result<()> {
    use std::os::windows::fs::OpenOptionsExt;
    let tmp = tempfile::tempdir()?;
    let dir = tmp.path().join("codex/2026/11/18");
    fs::create_dir_all(&dir)?;
    fs::write(dir.join("rollout-a.jsonl"), CODEX)?;
    fs::write(
        dir.join("rollout-b.jsonl"),
        CODEX.replace("resp_1", "resp_b"),
    )?;
    let _lock = fs::OpenOptions::new()
        .read(true)
        .share_mode(0)
        .open(dir.join("rollout-a.jsonl"))?;
    let roots = Roots {
        codex: tmp.path().join("codex"),
        claude: tmp.path().join("claude"),
    };
    let store = Store::open_in_memory()?;
    let stats = ingest_all(&store, &roots, 0)?;
    assert_eq!((stats.files, stats.skipped), (1, 1));
    assert_eq!(count(&store, "usage"), 1);
    Ok(())
}
