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
