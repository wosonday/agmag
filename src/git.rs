//! Truy vấn git tối thiểu cho việc kiểm bàn giao (chỉ đọc).

use std::process::Command;

use anyhow::{bail, Result};

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
        bail!("git {} lỗi: {}", args.join(" "), String::from_utf8_lossy(&out.stderr).trim());
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
            &["status", "--porcelain", "--untracked-files=all", "--", ".", ":(exclude).agents/tasks", ":(exclude).agents/.runtime"],
        )?;
        Ok(status.is_empty())
    }

    fn touched_since(&self, cwd: &str, since: &str, path: &str) -> Result<Option<String>> {
        let range = format!("{since}..HEAD");
        let out = git(cwd, &["log", "-1", "--format=%H", &range, "--", path])?;
        Ok((!out.is_empty()).then_some(out))
    }
}
