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
