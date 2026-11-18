use std::fs;
use std::process::Command;

use agmag::git::{Git, GitCli};

fn git(dir: &std::path::Path, args: &[&str]) {
    let ok = Command::new("git").arg("-C").arg(dir).args(args).output().unwrap().status.success();
    assert!(ok, "git {args:?}");
}

#[test]
fn git_cli_detects_handoff_commit() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.invalid"]);
    git(d, &["config", "user.name", "t"]);
    fs::write(d.join("a.txt"), "1")?;
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "init"]);
    let cwd = d.to_string_lossy();
    let g = GitCli;
    let base = g.head(&cwd)?;
    assert!(g.is_clean(&cwd)?);
    assert_eq!(g.touched_since(&cwd, &base, ".agents/log/x.md")?, None);
    fs::write(d.join("a.txt"), "2")?;
    assert!(!g.is_clean(&cwd)?);
    fs::create_dir_all(d.join(".agents/log"))?;
    fs::write(d.join(".agents/log/x.md"), "## bàn giao")?;
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "x: handoff"]);
    assert!(g.is_clean(&cwd)?);
    assert_eq!(g.touched_since(&cwd, &base, ".agents/log/x.md")?, Some(g.head(&cwd)?));
    Ok(())
}

#[test]
fn untracked_source_files_make_tree_dirty_but_task_files_do_not() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    let d = tmp.path();
    git(d, &["init", "-q"]);
    git(d, &["config", "user.email", "t@example.invalid"]);
    git(d, &["config", "user.name", "t"]);
    fs::write(d.join("a.txt"), "1")?;
    git(d, &["add", "."]);
    git(d, &["commit", "-qm", "init"]);
    let cwd = d.to_string_lossy();
    // Tệp tác vụ do điều phối chép vào (không commit) không tính là bẩn.
    fs::create_dir_all(d.join(".agents/tasks"))?;
    fs::write(d.join(".agents/tasks/a.md"), "việc")?;
    assert!(GitCli.is_clean(&cwd)?);
    // Mã nguồn mới chưa commit → bẩn (xoay vòng không được kill).
    fs::write(d.join("new.rs"), "fn main(){}")?;
    assert!(!GitCli.is_clean(&cwd)?);
    Ok(())
}
