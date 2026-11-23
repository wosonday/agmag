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
    /// Nội dung tệp review ở HEAD (None = HEAD không chứa tệp).
    review_text: Option<String>,
    /// Tệp ngoài `.agents/` đổi từ commit đã review tới HEAD.
    changed: Vec<String>,
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
    fn file_at_head(&self, _: &str, path: &str) -> anyhow::Result<Option<String>> {
        Ok(self.review_text.clone().filter(|_| path == REVIEW_PATH))
    }
    fn changed_outside_agents(&self, _: &str, _: &str) -> anyhow::Result<Vec<String>> {
        Ok(self.changed.clone())
    }
}

const REVIEW_PATH: &str = ".agents/reviews/x-1.md";
/// Review đã chạy trên commit `aaaa11112222`; sau đó chỉ commit tệp review (HEAD deadbeef…).
const REVIEW: &str = ".agents/reviews/x-1.md@aaaa11112222";

fn fake(clean: bool) -> FakeGit {
    FakeGit {
        head: "deadbeef0000ffff".into(),
        clean,
        review_text: Some("- Đối tượng: HEAD `aaaa11112222`\n## KẾT LUẬN: ĐẠT\n".into()),
        changed: vec![],
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
        &[
            ("status", "ready"),
            ("head", "deadbeef0000"),
            ("review", REVIEW),
        ],
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

// ---- Follow-up A: khóa và git, ghi tệp, BOM ----

struct LockSpyGit {
    file: PathBuf,
    head: String,
}

impl Git for LockSpyGit {
    fn file_at_head(&self, _: &str, path: &str) -> anyhow::Result<Option<String>> {
        assert!(
            !lock_dir(&self.file).exists(),
            "git file_at_head chạy khi đang giữ khóa"
        );
        Ok((path == REVIEW_PATH).then(|| "HEAD aaaa11112222 ĐẠT".to_string()))
    }
    fn changed_outside_agents(&self, _: &str, _: &str) -> anyhow::Result<Vec<String>> {
        assert!(
            !lock_dir(&self.file).exists(),
            "git changed_outside_agents chạy khi đang giữ khóa"
        );
        Ok(vec![])
    }
    fn head(&self, _: &str) -> anyhow::Result<String> {
        assert!(
            !lock_dir(&self.file).exists(),
            "git head chạy khi đang giữ khóa"
        );
        Ok(self.head.clone())
    }
    fn is_clean(&self, _: &str) -> anyhow::Result<bool> {
        assert!(
            !lock_dir(&self.file).exists(),
            "git is_clean chạy khi đang giữ khóa"
        );
        Ok(true)
    }
    fn touched_since(&self, _: &str, _: &str, _: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

struct FailGit;

impl Git for FailGit {
    fn head(&self, _: &str) -> anyhow::Result<String> {
        anyhow::bail!("git hỏng")
    }
    fn is_clean(&self, _: &str) -> anyhow::Result<bool> {
        anyhow::bail!("git hỏng")
    }
    fn touched_since(&self, _: &str, _: &str, _: &str) -> anyhow::Result<Option<String>> {
        Ok(None)
    }
}

#[test]
fn git_calls_happen_outside_the_lock() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(&file, "### a/x\n- status: wip\n").unwrap();
    let store = Store::open_in_memory().unwrap();
    let git = LockSpyGit {
        file: file.clone(),
        head: "deadbeef0000ffff".into(),
    };
    update(
        &store,
        &git,
        None,
        &req(
            &file,
            "a/x",
            &[
                ("status", "ready"),
                ("head", "deadbeef0000"),
                ("review", REVIEW),
            ],
            Some("/wt"),
        ),
        T,
        1,
    )
    .unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("- status: ready")
    );
}

#[test]
fn git_error_before_lock_writes_nothing_and_leaves_no_lock() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let before = "### a/x\n- status: wip\n";
    std::fs::write(&file, before).unwrap();
    let store = Store::open_in_memory().unwrap();
    let e = update(
        &store,
        &FailGit,
        None,
        &req(
            &file,
            "a/x",
            &[("status", "ready"), ("head", "deadbeef0000")],
            Some("/wt"),
        ),
        T,
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("git hỏng"), "{e}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    assert!(!dir.path().join(".agents").exists());
}

#[test]
fn head_change_on_ready_entry_runs_the_guard() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let before = "### a/x\n- status: ready\n- head: cafebabe0000\n- review: .agents/reviews/x-1.md@aaaa11112222\n";
    std::fs::write(&file, before).unwrap();
    let store = Store::open_in_memory().unwrap();
    let e = update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("head", "deadbeef0000")], None),
        T,
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("worktree"), "{e}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    // worktree bẩn cũng bị chặn
    assert!(
        update(
            &store,
            &fake(false),
            None,
            &req(&file, "a/x", &[("head", "deadbeef0000")], Some("/wt")),
            T,
            1
        )
        .is_err()
    );
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("head", "deadbeef0000")], Some("/wt")),
        T,
        1,
    )
    .unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("- head: deadbeef0000")
    );
}

#[test]
fn head_change_on_non_ready_entry_needs_no_worktree() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(&file, "### a/x\n- status: wip\n").unwrap();
    let store = Store::open_in_memory().unwrap();
    update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("head", "abc")], None),
        T,
        1,
    )
    .unwrap();
}

#[test]
fn drop_does_not_remove_a_lock_owned_by_someone_else() {
    let dir = tempfile::tempdir().unwrap();
    let ld = lock_dir(&dir.path().join("BRANCHES.md"));
    let g = acquire_lock(&ld, T, Duration::from_secs(60)).unwrap();
    std::fs::write(ld.join("owner"), "người khác").unwrap();
    drop(g);
    assert!(ld.exists());
    assert_eq!(
        std::fs::read_to_string(ld.join("owner")).unwrap(),
        "người khác"
    );
}

#[test]
fn reclaim_gives_back_a_lock_that_became_fresh_before_rename() {
    let dir = tempfile::tempdir().unwrap();
    let ld = lock_dir(&dir.path().join("BRANCHES.md"));
    std::fs::create_dir_all(&ld).unwrap();
    std::thread::sleep(Duration::from_millis(2200));
    let fired = std::cell::Cell::new(false);
    let e = acquire_lock_with(
        &ld,
        Duration::from_millis(150),
        Duration::from_millis(2000),
        &|d| std::fs::create_dir(d),
        &|from, to| {
            if !fired.replace(true) {
                // tiến trình khác thu hồi rồi tạo khóa mới ngay trước khi ta đổi tên
                std::fs::remove_dir_all(from)?;
                std::fs::create_dir(from)?;
                std::fs::write(from.join("owner"), "tươi")?;
            }
            std::fs::rename(from, to)
        },
    )
    .err()
    .unwrap();
    assert!(e.to_string().contains("đang bị khóa"), "{e}");
    assert!(ld.exists());
    assert_eq!(std::fs::read_to_string(ld.join("owner")).unwrap(), "tươi");
}

fn tmp_leftovers(dir: &Path) -> usize {
    std::fs::read_dir(dir)
        .unwrap()
        .filter_map(|e| e.ok())
        .filter(|e| e.file_name().to_string_lossy().contains(".tmp-"))
        .count()
}

#[test]
fn write_retries_permission_denied_then_succeeds() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(&file, "cũ").unwrap();
    let calls = std::cell::Cell::new(0);
    write_atomic_with(&file, "mới", &|a, b| {
        calls.set(calls.get() + 1);
        if calls.get() <= 2 {
            return Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied));
        }
        std::fs::rename(a, b)
    })
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mới");
    assert_eq!(tmp_leftovers(dir.path()), 0);
}

#[test]
fn write_gives_up_with_path_and_removes_tmp() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(&file, "cũ").unwrap();
    let e = write_atomic_with(&file, "mới", &|_, _| {
        Err(std::io::Error::from(std::io::ErrorKind::PermissionDenied))
    })
    .unwrap_err();
    assert!(e.to_string().contains("BRANCHES.md"), "{e}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "cũ");
    assert_eq!(tmp_leftovers(dir.path()), 0);
}

#[test]
fn write_tmp_failure_names_tmp_path() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("không-có-thư-mục").join("BRANCHES.md");
    let e = write_atomic_with(&file, "x", &|a, b| std::fs::rename(a, b)).unwrap_err();
    assert!(e.to_string().contains("tệp tạm"), "{e}");
    assert!(e.to_string().contains(".tmp-"), "{e}");
}

#[test]
fn bom_is_ignored_for_matching_and_preserved_in_output() {
    let t = "\u{feff}### a/x\n- status: wip\n";
    assert_eq!(
        read_entry(t, "a/x").unwrap().unwrap(),
        vec![kv("status", "wip")]
    );
    let (out, created) = upsert_entry(t, "a/x", &[kv("status", "blocked")]).unwrap();
    assert!(!created);
    assert_eq!(out, "\u{feff}### a/x\n- status: blocked\n");
    let (out, created) = upsert_entry("\u{feff}# Sổ\n", "a/x", &[kv("status", "wip")]).unwrap();
    assert!(created);
    assert_eq!(out, "\u{feff}# Sổ\n\n### a/x\n- status: wip\n");
}

#[test]
fn heading_with_trailing_comment_matches_entry() {
    let t = "### a/x   # chú thích\n- status: wip\n\n### a/xy\n- status: ready\n";
    assert_eq!(
        read_entry(t, "a/x").unwrap().unwrap(),
        vec![kv("status", "wip")]
    );
    let (out, created) = upsert_entry(t, "a/x", &[kv("status", "blocked")]).unwrap();
    assert!(!created);
    assert!(out.starts_with("### a/x   # chú thích\n- status: blocked\n"));
}

#[test]
fn write_retries_sharing_and_lock_violations() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(&file, "cũ").unwrap();
    let calls = std::cell::Cell::new(0);
    write_atomic_with(&file, "mới", &|a, b| {
        calls.set(calls.get() + 1);
        match calls.get() {
            1 => Err(std::io::Error::from_raw_os_error(32)),
            2 => Err(std::io::Error::from_raw_os_error(33)),
            _ => std::fs::rename(a, b),
        }
    })
    .unwrap();
    assert_eq!(calls.get(), 3);
    assert_eq!(std::fs::read_to_string(&file).unwrap(), "mới");
    assert_eq!(tmp_leftovers(dir.path()), 0);
}

#[test]
fn ready_without_head_update_checks_head_from_file() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let before = "### a/x\n- status: wip\n- head: deadbeef0000\n- review: .agents/reviews/x-1.md@aaaa11112222\n";
    std::fs::write(&file, before).unwrap();
    let store = Store::open_in_memory().unwrap();
    let r = req(&file, "a/x", &[("status", "ready")], Some("/wt"));
    let other = FakeGit {
        head: "cafebabe0000ffff".into(),
        ..fake(true)
    };
    assert!(update(&store, &other, None, &r, T, 1).is_err());
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
    update(&store, &fake(true), None, &r, T, 1).unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("- status: ready")
    );
}

// ---- Guard ready đòi bằng chứng review ----

fn ready_with(review: Option<&str>) -> Vec<(&str, &str)> {
    let mut f = vec![("status", "ready"), ("head", "deadbeef0000")];
    if let Some(r) = review {
        f.push(("review", r));
    }
    f
}

#[test]
fn ready_requires_committed_review_of_the_same_code() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let before = "### a/x\n- status: wip\n";
    std::fs::write(&file, before).unwrap();
    let store = Store::open_in_memory().unwrap();
    let wt = Some("/wt");
    let not_committed = FakeGit {
        review_text: None,
        ..fake(true)
    };
    let other_sha = FakeGit {
        review_text: Some("HEAD `bbbb2222` ĐẠT".into()),
        ..fake(true)
    };
    let code_after_review = FakeGit {
        changed: vec!["src/lib.rs".into()],
        ..fake(true)
    };
    let cases: Vec<(Option<&str>, FakeGit, &str)> = vec![
        (None, fake(true), "review"),
        (Some("opencode, 0 blocker"), fake(true), "@"),
        (Some(".agents/reviews/x-1.md@abc"), fake(true), "SHA"),
        (
            Some("src/x-1.md@aaaa11112222"),
            fake(true),
            ".agents/reviews/",
        ),
        (
            Some(".agents/reviews/../../x.md@aaaa11112222"),
            fake(true),
            ".agents/reviews/",
        ),
        (Some(REVIEW), not_committed, "chưa commit"),
        (Some(REVIEW), other_sha, "aaaa111"),
        (Some(REVIEW), code_after_review, "src/lib.rs"),
    ];
    for (review, g, want) in &cases {
        let e = update(
            &store,
            g,
            None,
            &req(&file, "a/x", &ready_with(*review), wt),
            T,
            1,
        )
        .unwrap_err();
        assert!(e.to_string().contains(want), "{review:?}: {e}");
        assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
        assert!(!lock_dir(&file).exists());
    }
    // Có ghi chú sau bằng chứng vẫn hợp lệ.
    let noted = format!("{REVIEW} opencode-go/deepseek-v4-pro, 0 blocker");
    update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &ready_with(Some(&noted)), wt),
        T,
        1,
    )
    .unwrap();
    assert!(
        std::fs::read_to_string(&file)
            .unwrap()
            .contains("- status: ready")
    );
}

#[test]
fn ready_uses_review_already_recorded_in_the_entry() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(
        &file,
        format!("### a/x\n- status: wip\n- review: {REVIEW}\n"),
    )
    .unwrap();
    let store = Store::open_in_memory().unwrap();
    update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &ready_with(None), Some("/wt")),
        T,
        1,
    )
    .unwrap();
}

#[test]
fn changing_review_on_a_ready_entry_runs_the_guard() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    let before = format!("### a/x\n- status: ready\n- head: deadbeef0000\n- review: {REVIEW}\n");
    std::fs::write(&file, &before).unwrap();
    let store = Store::open_in_memory().unwrap();
    let e = update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("review", "đã review miệng")], Some("/wt")),
        T,
        1,
    )
    .unwrap_err();
    assert!(e.to_string().contains("@"), "{e}");
    assert_eq!(std::fs::read_to_string(&file).unwrap(), before);
}

#[test]
fn review_on_non_ready_entry_is_free_text() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("BRANCHES.md");
    std::fs::write(&file, "### a/x\n- status: wip\n").unwrap();
    let store = Store::open_in_memory().unwrap();
    update(
        &store,
        &fake(true),
        None,
        &req(&file, "a/x", &[("review", "đang chờ reviewer")], None),
        T,
        1,
    )
    .unwrap();
}
