//! Đọc và sửa khối `### <nhánh>` trong BRANCHES.md — chỉ là hàm thuần trên văn bản,
//! không đụng tệp, không khóa.

use std::path::{Path, PathBuf};
use std::time::{Duration, SystemTime, UNIX_EPOCH};

use anyhow::{Context, Result, bail};
use serde::Serialize;

use crate::git::Git;
use crate::store::Store;

pub const STATUSES: [&str; 8] = [
    "wip",
    "blocked",
    "ready",
    "merging",
    "merged",
    "deployed",
    "needs-rebase",
    "failed",
];

pub const FIELDS: [&str; 14] = [
    "status",
    "lane",
    "tier",
    "head",
    "base",
    "depends_on",
    "migration",
    "touches_hotspot",
    "summary",
    "verify",
    "review",
    "docs",
    "registered",
    "git",
];

fn valid_part(s: &str) -> bool {
    (1..=64).contains(&s.len())
        && s.bytes().all(|b| {
            b.is_ascii_lowercase() || b.is_ascii_digit() || matches!(b, b'.' | b'_' | b'-')
        })
}

/// Tên mục dạng `<repo>/<nhánh>`: đúng một `/`, mỗi phần 1–64 ký tự `[a-z0-9._-]`.
pub fn validate_entry(entry: &str) -> Result<()> {
    let parts: Vec<&str> = entry.split('/').collect();
    if parts.len() != 2 || !parts.iter().all(|p| valid_part(p)) {
        bail!(
            "tên mục không hợp lệ: {entry:?} (cần dạng repo/nhánh, mỗi phần 1-64 ký tự a-z 0-9 . _ -)"
        );
    }
    Ok(())
}

pub fn validate_fields(fields: &[(String, String)]) -> Result<()> {
    for (i, (k, v)) in fields.iter().enumerate() {
        if !FIELDS.contains(&k.as_str()) {
            bail!("khóa trường không hợp lệ: {k:?}");
        }
        if fields[..i].iter().any(|(k2, _)| k2 == k) {
            bail!("khóa trường bị trùng: {k:?}");
        }
        if v.trim().is_empty() {
            bail!("giá trị của {k} đang trống");
        }
        if v.contains('\n') || v.contains('\r') {
            bail!("giá trị của {k} không được chứa xuống dòng");
        }
        if v.contains(" # ") || v.contains("\t# ") {
            bail!("giá trị của {k} không được chứa ' # ' (sẽ bị cắt như chú thích khi đọc lại)");
        }
        if k == "status" && !STATUSES.contains(&v.trim()) {
            bail!(
                "trạng thái không hợp lệ: {v:?} (hợp lệ: {})",
                STATUSES.join(" ")
            );
        }
    }
    Ok(())
}

fn content(line: &str) -> &str {
    line.trim_end_matches(['\n', '\r'])
}

/// Phân tích dòng trường `- khóa: giá trị`, trả khóa và phần sau dấu `:` (chưa cắt chú thích).
fn parse_field(line: &str) -> Option<(&str, &str)> {
    let rest = content(line).strip_prefix("- ")?;
    let (k, v) = rest.split_once(':')?;
    if k.is_empty() || !k.bytes().all(|b| b.is_ascii_lowercase() || b == b'_') {
        return None;
    }
    Some((k, v))
}

fn strip_comment(v: &str) -> &str {
    let cut = [v.find(" # "), v.find("\t# ")]
        .into_iter()
        .flatten()
        .min()
        .unwrap_or(v.len());
    v[..cut].trim()
}

/// Tìm khối của `entry`: `(chỉ số dòng tiêu đề, chỉ số dòng kết thúc không gồm)`.
fn find_block(lines: &[&str], entry: &str) -> Result<Option<(usize, usize)>> {
    let heading = format!("### {entry}");
    let mut hits = lines
        .iter()
        .enumerate()
        .filter(|(_, l)| content(l).trim_end() == heading)
        .map(|(i, _)| i);
    let Some(start) = hits.next() else {
        return Ok(None);
    };
    if hits.next().is_some() {
        bail!("mục {entry} xuất hiện nhiều lần trong BRANCHES.md");
    }
    let end = lines[start + 1..]
        .iter()
        .position(|l| l.starts_with('#'))
        .map_or(lines.len(), |p| start + 1 + p);
    Ok(Some((start, end)))
}

pub fn read_entry(text: &str, entry: &str) -> Result<Option<Vec<(String, String)>>> {
    let lines: Vec<&str> = text.split_inclusive('\n').collect();
    let Some((start, end)) = find_block(&lines, entry)? else {
        return Ok(None);
    };
    let fields = lines[start + 1..end]
        .iter()
        .filter_map(|l| parse_field(l))
        .map(|(k, v)| (k.to_string(), strip_comment(v).to_string()))
        .collect();
    Ok(Some(fields))
}

/// Cập nhật (hoặc tạo) khối `entry`; trả `(văn bản mới, created)`.
pub fn upsert_entry(
    text: &str,
    entry: &str,
    updates: &[(String, String)],
) -> Result<(String, bool)> {
    // khóa trùng trong `updates`: giữ bản cuối cùng
    let deduped: Vec<(String, String)> = updates
        .iter()
        .enumerate()
        .filter(|(i, (k, _))| !updates[i + 1..].iter().any(|(k2, _)| k2 == k))
        .map(|(_, kv)| kv.clone())
        .collect();
    let updates = deduped.as_slice();
    let eol = if text.contains("\r\n") { "\r\n" } else { "\n" };
    let field_line = |k: &str, v: &str| format!("- {k}: {}{eol}", v.trim());
    let lines: Vec<&str> = text.split_inclusive('\n').collect();

    let Some((start, end)) = find_block(&lines, entry)? else {
        let mut out = text.trim_end_matches(['\n', '\r']).to_string();
        if !out.is_empty() {
            out.push_str(eol);
            out.push_str(eol);
        }
        out.push_str(&format!("### {entry}{eol}"));
        for (k, v) in updates {
            out.push_str(&field_line(k, v));
        }
        return Ok((out, true));
    };

    let mut out: Vec<String> = lines.iter().map(|l| l.to_string()).collect();
    let mut seen = vec![false; updates.len()];
    let mut last_field = start;
    for (i, line) in lines.iter().enumerate().take(end).skip(start + 1) {
        let Some((k, _)) = parse_field(line) else {
            continue;
        };
        last_field = i;
        if let Some(j) = updates.iter().rposition(|(uk, _)| uk == k) {
            out[i] = field_line(k, &updates[j].1);
            seen[j] = true;
            // khóa trùng trong cùng khối: các cập nhật khác cùng khóa cũng coi như đã xử lý
            for (s, (uk, _)) in seen.iter_mut().zip(updates) {
                if uk == k {
                    *s = true;
                }
            }
        }
    }
    let new_lines: String = updates
        .iter()
        .zip(&seen)
        .filter(|(_, s)| !**s)
        .map(|((k, v), _)| field_line(k, v))
        .collect();
    if !new_lines.is_empty() {
        // dòng liền trước chỗ chèn có thể là dòng cuối tệp thiếu xuống dòng
        if !out[last_field].ends_with('\n') {
            out[last_field].push_str(eol);
        }
        out[last_field].push_str(&new_lines);
    }
    Ok((out.concat(), false))
}

pub const LOCK_TIMEOUT: Duration = Duration::from_secs(30);
pub const LOCK_STALE: Duration = Duration::from_secs(60);

/// Thư mục khóa: `<thư mục chứa BRANCHES.md>/.agents/.runtime/locks/branches`.
pub fn lock_dir(file: &Path) -> PathBuf {
    let parent = file
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .unwrap_or_else(|| Path::new("."));
    parent.join(".agents/.runtime/locks/branches")
}

/// Giữ khóa; thả (xóa thư mục khóa) khi bị drop.
#[derive(Debug)]
pub struct BranchLock {
    path: PathBuf,
}

impl Drop for BranchLock {
    fn drop(&mut self) {
        let _ = std::fs::remove_dir_all(&self.path);
    }
}

fn wall_ms() -> i64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map_or(0, |d| d.as_millis() as i64)
}

/// Khóa bằng `create_dir` (nguyên tử); khóa cũ hơn `stale_after` bị thu hồi.
pub fn acquire_lock(dir: &Path, timeout: Duration, stale_after: Duration) -> Result<BranchLock> {
    acquire_lock_with(dir, timeout, stale_after, &|from, to| {
        std::fs::rename(from, to)
    })
}

/// Như `acquire_lock` nhưng cho phép thay bước đổi tên (để test lỗi đổi tên).
#[doc(hidden)]
pub fn acquire_lock_with(
    dir: &Path,
    timeout: Duration,
    stale_after: Duration,
    rename: &dyn Fn(&Path, &Path) -> std::io::Result<()>,
) -> Result<BranchLock> {
    let mut last_err: Option<std::io::Error> = None;
    if let Some(parent) = dir.parent() {
        std::fs::create_dir_all(parent)?;
    }
    let start = std::time::Instant::now();
    loop {
        match std::fs::create_dir(dir) {
            Ok(()) => {
                let _ = std::fs::write(
                    dir.join("owner"),
                    format!("{} {}", std::process::id(), wall_ms()),
                );
                return Ok(BranchLock {
                    path: dir.to_path_buf(),
                });
            }
            Err(e) if e.kind() == std::io::ErrorKind::AlreadyExists => {}
            Err(e) => {
                return Err(e).with_context(|| format!("không tạo được khóa {}", dir.display()));
            }
        }
        let age = std::fs::metadata(dir)
            .and_then(|m| m.modified())
            .ok()
            .map(|t| {
                SystemTime::now()
                    .duration_since(t)
                    .unwrap_or(Duration::ZERO)
            });
        if age.is_some_and(|a| a >= stale_after) {
            // đổi tên để chỉ một tiến trình thắng; kẻ thua gặp lỗi rồi thử lại
            let nanos = SystemTime::now()
                .duration_since(UNIX_EPOCH)
                .map_or(0, |d| d.as_nanos());
            let gone = dir.with_extension(format!("stale-{}-{nanos}", std::process::id()));
            match rename(dir, &gone) {
                Ok(()) => {
                    let _ = std::fs::remove_dir_all(&gone);
                    continue;
                }
                // thư mục đã biến mất (tiến trình khác thu hồi trước): thử lại ngay
                Err(e) if e.kind() == std::io::ErrorKind::NotFound => continue,
                // không đổi tên được: rơi xuống kiểm timeout và ngủ, không quay vòng
                Err(e) => last_err = Some(e),
            }
        }
        if start.elapsed() >= timeout {
            let why = last_err
                .map(|e| format!("; không thu hồi được khóa cũ: {e}"))
                .unwrap_or_default();
            bail!(
                "BRANCHES.md đang bị khóa tại {} (quá {}s){why}",
                dir.display(),
                timeout.as_secs()
            );
        }
        std::thread::sleep(Duration::from_millis(100));
    }
}

/// Thợ không được ghi sổ nhánh; terminal khác hoặc người dùng thì được.
pub fn ensure_writer(store: &Store, caller_terminal: Option<&str>) -> Result<()> {
    if let Some(t) = caller_terminal
        && let Some(row) = store.terminal(t)?
        && row.role.as_deref() == Some("worker")
    {
        bail!("thợ ({t}) không được ghi BRANCHES.md — báo BOT-CEO qua agm send");
    }
    Ok(())
}

#[derive(Debug, Clone)]
pub struct BranchUpdate {
    pub file: PathBuf,
    pub entry: String,
    pub fields: Vec<(String, String)>,
    pub worktree: Option<String>,
}

#[derive(Debug, Clone, Serialize)]
pub struct UpdateOutcome {
    pub entry: String,
    pub created: bool,
    pub fields: Vec<(String, String)>,
}

/// Đọc một mục từ tệp; tệp thiếu coi như không có mục.
pub fn get(file: &Path, entry: &str) -> Result<Option<Vec<(String, String)>>> {
    validate_entry(entry)?;
    let text = match std::fs::read_to_string(file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => return Ok(None),
        Err(e) => return Err(e).with_context(|| format!("không đọc được {}", file.display())),
    };
    read_entry(&text, entry)
}

fn is_sha(s: &str) -> bool {
    (12..=40).contains(&s.len()) && s.bytes().all(|b| matches!(b, b'0'..=b'9' | b'a'..=b'f'))
}

/// Bảo vệ ready: head hợp lệ, có worktree, worktree sạch và HEAD khớp head đã khai.
fn check_ready(git: &dyn Git, merged: &[(String, String)], worktree: Option<&str>) -> Result<()> {
    let head = merged
        .iter()
        .find(|(k, _)| k == "head")
        .map(|(_, v)| v.as_str())
        .unwrap_or("");
    if !is_sha(head) {
        bail!("không thể đặt ready: head phải là SHA 12-40 ký tự hex thường (đang là {head:?})");
    }
    let Some(wt) = worktree else {
        bail!("không thể đặt ready: thiếu worktree để kiểm tra git");
    };
    if !git.is_clean(wt)? {
        bail!("không thể đặt ready: worktree {wt} còn thay đổi chưa commit");
    }
    let actual = git.head(wt)?;
    if !actual.starts_with(head) {
        bail!("không thể đặt ready: head khai {head} không khớp HEAD thật {actual}");
    }
    Ok(())
}

fn do_update(
    store: &Store,
    git: &dyn Git,
    caller_terminal: Option<&str>,
    req: &BranchUpdate,
    timeout: Duration,
) -> Result<UpdateOutcome> {
    validate_entry(&req.entry)?;
    validate_fields(&req.fields)?;
    if req.file.file_name().and_then(|n| n.to_str()) != Some("BRANCHES.md") {
        bail!(
            "chỉ được ghi tệp tên BRANCHES.md (nhận {})",
            req.file.display()
        );
    }
    ensure_writer(store, caller_terminal)?;
    let _lock = acquire_lock(&lock_dir(&req.file), timeout, LOCK_STALE)?;
    let text = match std::fs::read_to_string(&req.file) {
        Ok(t) => t,
        Err(e) if e.kind() == std::io::ErrorKind::NotFound => String::new(),
        Err(e) => {
            return Err(e).with_context(|| format!("không đọc được {}", req.file.display()));
        }
    };
    let (new_text, created) = upsert_entry(&text, &req.entry, &req.fields)?;
    let merged = read_entry(&new_text, &req.entry)?.unwrap_or_default();
    let to_ready = req
        .fields
        .iter()
        .rfind(|(k, _)| k == "status")
        .is_some_and(|(_, v)| v.trim() == "ready");
    if to_ready {
        check_ready(git, &merged, req.worktree.as_deref())?;
    }
    let tmp = req
        .file
        .with_file_name(format!("BRANCHES.md.tmp-{}", std::process::id()));
    std::fs::write(&tmp, &new_text)?;
    if let Err(e) = std::fs::rename(&tmp, &req.file) {
        let _ = std::fs::remove_file(&tmp);
        return Err(e).with_context(|| format!("không ghi được {}", req.file.display()));
    }
    Ok(UpdateOutcome {
        entry: req.entry.clone(),
        created,
        fields: merged,
    })
}

/// Cập nhật một mục có khóa, bảo vệ ready và ghi nhật ký (cả khi lỗi).
pub fn update(
    store: &Store,
    git: &dyn Git,
    caller_terminal: Option<&str>,
    req: &BranchUpdate,
    timeout: Duration,
    now_ms: i64,
) -> Result<UpdateOutcome> {
    let result = do_update(store, git, caller_terminal, req, timeout);
    let detail = req
        .fields
        .iter()
        .map(|(k, v)| format!("{k}={v}"))
        .collect::<Vec<_>>()
        .join(", ");
    let _ = store.log_action(
        caller_terminal.unwrap_or("user"),
        "branch",
        &req.entry,
        &detail,
        result.is_ok(),
        now_ms,
    );
    result
}
