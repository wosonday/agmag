use agmag::branches::*;

fn kv(k: &str, v: &str) -> (String, String) {
    (k.to_string(), v.to_string())
}

#[test]
fn read_entry_strips_inline_comment_but_keeps_hash_in_value() {
    let t = "### tap-hoa/low-stock\n- head: 9f8e7d6c5b4a      # SHA cuối\n- summary: sửa issue #12\n- tier: vừa\n";
    let f = read_entry(t, "tap-hoa/low-stock").unwrap().unwrap();
    assert_eq!(
        f,
        vec![
            kv("head", "9f8e7d6c5b4a"),
            kv("summary", "sửa issue #12"),
            kv("tier", "vừa")
        ]
    );
}

#[test]
fn read_entry_missing_returns_none() {
    assert!(read_entry("# Sổ\n", "a/x").unwrap().is_none());
}

#[test]
fn upsert_replaces_existing_and_appends_new_keys_inside_block_only() {
    let t = "# Sổ\n\n### a/x\n- status: wip\n\n### b/y\n- status: wip\n";
    let (out, created) =
        upsert_entry(t, "a/x", &[kv("status", "blocked"), kv("summary", "chờ")]).unwrap();
    assert!(!created);
    assert_eq!(
        out,
        "# Sổ\n\n### a/x\n- status: blocked\n- summary: chờ\n\n### b/y\n- status: wip\n"
    );
}

#[test]
fn upsert_drops_old_comment_on_replaced_line() {
    let t = "### a/x\n- head: abc # cũ\n";
    let (out, _) = upsert_entry(t, "a/x", &[kv("head", "def")]).unwrap();
    assert_eq!(out, "### a/x\n- head: def\n");
}

#[test]
fn upsert_new_entry_appends_with_one_blank_line() {
    let (out, created) = upsert_entry("# Sổ\n", "a/x", &[kv("status", "wip")]).unwrap();
    assert!(created);
    assert_eq!(out, "# Sổ\n\n### a/x\n- status: wip\n");
}

#[test]
fn upsert_new_entry_when_text_lacks_trailing_newline_or_has_blank_lines() {
    let (out, _) = upsert_entry("# Sổ", "a/x", &[kv("status", "wip")]).unwrap();
    assert_eq!(out, "# Sổ\n\n### a/x\n- status: wip\n");
    let (out, _) = upsert_entry("# Sổ\n\n\n", "a/x", &[kv("status", "wip")]).unwrap();
    assert_eq!(out, "# Sổ\n\n### a/x\n- status: wip\n");
}

#[test]
fn upsert_into_empty_heading_inserts_after_heading() {
    let (out, _) = upsert_entry("### a/x\n\n### b/y\n", "a/x", &[kv("status", "wip")]).unwrap();
    assert_eq!(out, "### a/x\n- status: wip\n\n### b/y\n");
}

#[test]
fn upsert_preserves_crlf() {
    let (out, _) = upsert_entry(
        "### a/x\r\n- status: wip\r\n",
        "a/x",
        &[kv("status", "ready")],
    )
    .unwrap();
    assert_eq!(out, "### a/x\r\n- status: ready\r\n");
}

#[test]
fn duplicate_heading_is_an_error() {
    let t = "### a/x\n- status: wip\n\n### a/x\n- status: wip\n";
    let e = upsert_entry(t, "a/x", &[kv("status", "ready")]).unwrap_err();
    assert!(e.to_string().contains("nhiều lần"));
    assert!(read_entry(t, "a/x").is_err());
}

#[test]
fn validation_rejects_bad_input() {
    assert!(validate_entry("a/b/c").is_err());
    assert!(validate_entry("A/x").is_err());
    assert!(validate_entry("a/x").is_ok());
    assert!(validate_entry("ax").is_err());
    assert!(validate_entry("a/").is_err());
    assert!(validate_fields(&[kv("status", "done")]).is_err());
    assert!(validate_fields(&[kv("status", "wip")]).is_ok());
    assert!(validate_fields(&[kv("owner", "x")]).is_err());
    assert!(validate_fields(&[kv("summary", "a\nb")]).is_err());
    assert!(validate_fields(&[kv("summary", "  ")]).is_err());
    assert!(validate_fields(&[kv("summary", "a"), kv("summary", "b")]).is_err());
}

#[test]
fn validate_fields_rejects_value_that_would_be_cut_as_comment() {
    let e = validate_fields(&[kv("summary", "a # b")]).unwrap_err();
    assert!(e.to_string().contains("summary"));
    assert!(validate_fields(&[kv("summary", "a\t# b")]).is_err());
    assert!(validate_fields(&[kv("summary", "issue #12")]).is_ok());
}

#[test]
fn validate_fields_rejects_value_that_reads_back_empty() {
    assert!(validate_fields(&[kv("summary", "# x")]).is_err());
    assert!(validate_fields(&[kv("summary", "  # x")]).is_err());
    assert!(validate_fields(&[kv("summary", "#")]).is_err());
    assert!(validate_fields(&[kv("summary", "#12")]).is_ok());
}

#[test]
fn update_missing_file_errors_and_creates_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let store = Store::open_in_memory().unwrap();
    let e = update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("status", "wip")], None),
        T,
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("không thấy"), "{e}");
    assert!(!file.exists());
    assert!(!dir.path().join(".agents").exists());
}

#[test]
fn upsert_duplicate_update_keys_last_wins() {
    let t = "### a/x\n- status: wip\n";
    let (out, _) = upsert_entry(t, "a/x", &[kv("lane", "1"), kv("lane", "2")]).unwrap();
    assert_eq!(out, "### a/x\n- status: wip\n- lane: 2\n");
    let (out, _) =
        upsert_entry(t, "a/x", &[kv("status", "ready"), kv("status", "blocked")]).unwrap();
    assert_eq!(out, "### a/x\n- status: blocked\n");
}

#[test]
fn crlf_file_keeps_crlf_on_insert_and_create() {
    let t = "# So\r\n\r\n### a/x\r\n- status: wip\r\n";
    let (out, created) = upsert_entry(t, "a/x", &[kv("lane", "1")]).unwrap();
    assert!(!created);
    assert_eq!(out, "# So\r\n\r\n### a/x\r\n- status: wip\r\n- lane: 1\r\n");
    let (out, created) = upsert_entry(t, "b/y", &[kv("status", "wip")]).unwrap();
    assert!(created);
    assert_eq!(
        out,
        "# So\r\n\r\n### a/x\r\n- status: wip\r\n\r\n### b/y\r\n- status: wip\r\n"
    );
    assert!(!out.replace("\r\n", "").contains('\n'));
}

// ---- Task 2: khóa, ghi nguyên tử, update ----

use agmag::git::Git;
use agmag::store::{Store, TerminalRow};
use std::path::{Path, PathBuf};
use std::time::Duration;

struct FakeGit {
    head: String,
    clean: bool,
}

impl Git for FakeGit {
    fn head(&self, _: &str) -> anyhow::Result<String> {
        Ok(self.head.clone())
    }
    fn is_clean(&self, _: &str) -> anyhow::Result<bool> {
        Ok(self.clean)
    }
    fn touched_since(&self, _: &str, _: &str, _: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

fn fake(clean: bool) -> FakeGit {
    FakeGit {
        head: "deadbeef0000ffff".into(),
        clean,
    }
}

fn req(file: &Path, entry: &str, fields: &[(&str, &str)], wt: Option<&str>) -> BranchUpdate {
    BranchUpdate {
        file: file.to_path_buf(),
        entry: entry.into(),
        fields: fields.iter().map(|(k, v)| kv(k, v)).collect(),
        worktree: wt.map(String::from),
    }
}

const T: Duration = Duration::from_secs(5);

#[test]
fn update_writes_entry_and_logs_action() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(
        &file,
        "# Branches
",
    )
    .unwrap();
    let store = Store::open_in_memory().unwrap();
    let out = update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("status", "wip")], None),
        T,
        1,
    )
    .unwrap();
    assert!(out.created);
    assert_eq!(out.fields, vec![kv("status", "wip")]);
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("### a/x\n- status: wip")
    );
    assert!(!lock_dir(&file).exists());
    assert_eq!(
        get(&file, "a/x").unwrap().unwrap(),
        vec![kv("status", "wip")]
    );
}

#[test]
fn update_rejects_wrong_file_name_and_logs_failure() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("OTHER.md");
    let store = Store::open_in_memory().unwrap();
    let e = update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("status", "wip")], None),
        T,
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("BRANCHES.md"));
    assert!(!file.exists());
}

#[test]
fn ready_requires_head_worktree_clean_and_matching_sha() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let before = "### a/x\n- status: wip\n";
    std::fs::write(&file, before).unwrap();
    let store = Store::open_in_memory().unwrap();
    let wt = Some("/wt");
    let bad: Vec<(BranchUpdate, FakeGit)> = vec![
        (
            req(&file, "a/x", &[("status", "ready"), ("head", "abc")], wt),
            fake(true),
        ),
        (
            req(
                &file,
                "a/x",
                &[("status", "ready"), ("head", "deadbeef0000")],
                None,
            ),
            fake(true),
        ),
        (
            req(
                &file,
                "a/x",
                &[("status", "ready"), ("head", "deadbeef0000")],
                wt,
            ),
            fake(false),
        ),
        (
            req(
                &file,
                "a/x",
                &[("status", "ready"), ("head", "cafebabe0000")],
                wt,
            ),
            fake(true),
        ),
        (req(&file, "a/x", &[("status", "ready")], wt), fake(true)),
    ];
    for (r, g) in &bad {
        assert!(update(&store, g, None, r, T, 1).is_err());
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        assert!(!lock_dir(&file).exists());
    }
    let ok = req(
        &file,
        "a/x",
        &[("status", "ready"), ("head", "deadbeef0000")],
        wt,
    );
    update(&store, &fake(true), None, &ok, T, 1).unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("- status: ready")
    );
}

#[test]
fn worker_terminal_is_refused() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let store = Store::open_in_memory().unwrap();
    store
        .upsert_terminal(&TerminalRow {
            address: "t9".into(),
            role: Some("worker".into()),
            ..Default::default()
        })
        .unwrap();
    let e = update(
        &store,
        &fake(true),
        Some("t9"),
        &req(&file, "a/x", &[("status", "wip")], None),
        T,
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("thợ"));
    assert!(!file.exists());
}

#[test]
fn stale_lock_is_reclaimed_fresh_lock_times_out() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let ld: PathBuf = lock_dir(&file);
    std::fs::create_dir_all(&ld).unwrap();
    let e = acquire_lock(&ld, Duration::from_millis(300), Duration::from_secs(60))
        .err()
        .unwrap();
    assert!(e.to_string().contains("đang bị khóa"));
    let g = acquire_lock(&ld, Duration::from_secs(2), Duration::ZERO).unwrap();
    assert!(ld.exists());
    drop(g);
    assert!(!ld.exists());
}

#[test]
fn concurrent_updates_lose_nothing() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(
        &file,
        "# Branches
",
    )
    .unwrap();
    let db = dir.path().join("agm.db");
    // mỗi thread một Store riêng, nhưng mở tuần tự: khởi tạo schema đồng thời là chuyện của store, không phải của khóa nhánh
    let stores: Vec<Store> = (0..8).map(|_| Store::open(&db).unwrap()).collect();
    let handles: Vec<_> = stores
        .into_iter()
        .enumerate()
        .map(|(i, store)| {
            let i = i as i64;
            let file = file.clone();
            std::thread::spawn(move || {
                update(
                    &store,
                    &fake(true),
                    None,
                    &req(&file, &format!("l/s{i}"), &[("status", "wip")], None),
                    Duration::from_secs(20),
                    i,
                )
                .unwrap();
            })
        })
        .collect();
    for h in handles {
        h.join().unwrap();
    }
    let text = std::fs::read_to_string(&file).unwrap();
    for i in 0..8 {
        assert!(
            read_entry(&text, &format!("l/s{i}")).unwrap().is_some(),
            "thiếu s{i}"
        );
    }
    let leftovers: Vec<_> = std::fs::read_dir(dir.path())
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .collect();
    assert!(leftovers.is_empty());
}

#[test]
fn stale_lock_with_failing_rename_times_out_instead_of_spinning() {
    let dir = tempfile::tempdir().unwrap();
    let ld = lock_dir(&dir.path().join("BRANCHES.md"));
    std::fs::create_dir_all(&ld).unwrap();
    let start = std::time::Instant::now();
    let e = acquire_lock_with(
        &ld,
        Duration::from_millis(400),
        Duration::ZERO,
        &|d| std::fs::create_dir(d),
        &|_, _| {
            Err(std::io::Error::new(
                std::io::ErrorKind::PermissionDenied,
                "bị giữ handle",
            ))
        },
    )
    .err()
    .unwrap();
    assert!(start.elapsed() < Duration::from_secs(5));
    let msg = e.to_string();
    assert!(
        msg.contains("đang bị khóa") && msg.contains("bị giữ handle"),
        "{msg}"
    );
    assert!(ld.exists());
}

#[test]
fn create_dir_permission_denied_is_retried_then_acquires() {
    let dir = tempfile::tempdir().unwrap();
    let ld = lock_dir(&dir.path().join("BRANCHES.md"));
    let calls = std::cell::Cell::new(0);
    let g = acquire_lock_with(
        &ld,
        Duration::from_secs(5),
        Duration::from_secs(60),
        &|d| {
            calls.set(calls.get() + 1);
            if calls.get() == 1 {
                return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
            }
            std::fs::create_dir(d)
        },
        &|a, b| std::fs::rename(a, b),
    )
    .unwrap();
    assert_eq!(calls.get(), 2);
    assert!(ld.exists());
    drop(g);
}

#[test]
fn create_dir_permission_denied_forever_times_out_with_reason() {
    let dir = tempfile::tempdir().unwrap();
    let ld = lock_dir(&dir.path().join("BRANCHES.md"));
    let e = acquire_lock_with(
        &ld,
        Duration::from_millis(300),
        Duration::from_secs(60),
        &|_| Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied)),
        &|a, b| std::fs::rename(a, b),
    )
    .err()
    .unwrap();
    assert!(e.to_string().contains("đang bị khóa"));
}
