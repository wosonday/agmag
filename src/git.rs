//! Truy vấn git tối thiểu cho việc kiểm bàn giao (chỉ đọc).

use std::process::Command;

use anyhow::{Result, bail};

pub trait Git {
    fn head(&self, cwd: &str) -> Result<String>;
    /// Worktree không còn thay đổi chưa commit, kể cả tệp mới (bỏ qua `.agents/tasks/`, `.agents/.runtime/`).
    fn is_clean(&self, cwd: &str) -> Result<bool>;
    /// Commit mới nhất kể từ `since` có chạm `path`; None nếu không có.
    fn touched_since(&self, cwd: &str, since: &str, path: &str) -> Result<Option<String>>;
}

pub struct GitCli;

fn git(cwd: &str, args: &[&str]) -> Result<String> {
    let out = Command::new("git").arg("-C").arg(cwd).args(args).output()?;
    if !out.status.success() {
        bail!(
            "git {} lỗi: {}",
            args.join(" "),
            String::from_utf8_lossy(&out.stderr).trim()
        );
    }
    Ok(String::from_utf8_lossy(&out.stdout).trim().to_string())
}

impl Git for GitCli {
    fn head(&self, cwd: &str) -> Result<String> {
        git(cwd, &["rev-parse", "HEAD"])
    }

    fn is_clean(&self, cwd: &str) -> Result<bool> {
        // Tệp mới chưa commit (mã nguồn) cũng tính là bẩn; chỉ bỏ qua tệp điều phối chép vào worktree.
        let status = git(
            cwd,
            &[
                "status",
                "--porcelain",
                "--untracked-files=all",
                "--",
                ".",
                ":(exclude).agents/tasks",
                ":(exclude).agents/.runtime",
            ],
        )?;
        Ok(status.is_empty())
    }

    fn touched_since(&self, cwd: &str, since: &str, path: &str) -> Result<Option<String>> {
        let range = format!("{since}..HEAD");
        let out = git(cwd, &["log", "-1", "--format=%H", &range, "--", path])?;
        Ok((!out.is_empty()).then_some(out))
    }
}

/// Thêm `.agents/handoff/` vào tệp `info/exclude` của repo (hỗ trợ cả git worktree).
/// Trả `Ok(false)` nếu `cwd` không phải git repo.
pub fn ensure_handoff_excluded(cwd: &str) -> Result<bool> {
    let out = Command::new("git")
        .arg("-C")
        .arg(cwd)
        .args(["rev-parse", "--git-path", "info/exclude"])
        .output()?;
    if !out.status.success() {
        return Ok(false);
    }
    let rel = String::from_utf8_lossy(&out.stdout).trim().to_string();
    if rel.is_empty() {
        return Ok(false);
    }
    let rel_path = std::path::Path::new(&rel);
    let exclude_path = if rel_path.is_absolute() {
        rel_path.to_path_buf()
    } else {
        std::path::Path::new(cwd).join(rel_path)
    };
    const PATTERN: &str = ".agents/handoff/";
    if let Ok(existing) = std::fs::read_to_string(&exclude_path)
        && existing.lines().any(|l| l.trim() == PATTERN)
    {
        return Ok(true);
    }
    if let Some(parent) = exclude_path.parent() {
        std::fs::create_dir_all(parent)?;
    }
    use std::io::Write;
    let needs_nl = std::fs::read(&exclude_path)
        .map(|b| !b.is_empty() && !b.ends_with(b"\n"))
        .unwrap_or(false);
    let mut f = std::fs::OpenOptions::new()
        .create(true)
        .append(true)
        .open(&exclude_path)?;
    if needs_nl {
        writeln!(f)?;
    }
    writeln!(f, "{PATTERN}")?;
    Ok(true)
}
