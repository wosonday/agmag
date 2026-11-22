#![cfg(windows)]
mod common;

use std::cell::RefCell;

use agmag::actions::{self, SpawnReq};
use agmag::config::Config;
use agmag::hc::{Hc, HcFailure, RunSpec};
use agmag::store::Store;
use anyhow::Result;
use common::FakeHc;
use serde_json::{Value, json};

#[derive(Default)]
struct TemplateHc {
    inner: FakeHc,
    runs: RefCell<Vec<RunSpec>>,
    failure: Option<(&'static str, &'static str)>,
}

impl Hc for TemplateHc {
    fn templates(&self) -> Result<Vec<Value>> {
        if std::env::var_os("AGM_TEMPLATES_UNAVAILABLE").is_some() {
            anyhow::bail!("hc templates unavailable");
        }
        Ok(vec![json!({"name":"auditor"})])
    }
    fn ls(&self, all: bool) -> Result<Vec<Value>> {
        self.inner.ls(all)
    }
    fn get(&self, t: &str) -> Result<Value> {
        let app = if std::env::var_os("AGM_NOT_READY").is_some() {
            "pwsh"
        } else {
            "opencode"
        };
        Ok(
            json!({"address":t, "state":"live", "reported":true, "status":{"app":app,"state":"idle"}}),
        )
    }
    fn send(&self, t: &str, text: &str, idle: bool, force: bool) -> Result<Value> {
        self.inner.send(t, text, idle, force)
    }
    fn run(&self, spec: &RunSpec) -> Result<Value> {
        self.runs.borrow_mut().push(spec.clone());
        if spec.argv[0] == "auditor" {
            let (kind, message) = self
                .failure
                .unwrap_or(("spawn_failed", "cannot start \"sh\": sh: not found on PATH"));
            return Err(HcFailure {
                kind: kind.into(),
                message: message.into(),
            }
            .into());
        }
        let mut v = self.inner.run(spec)?;
        v["template"] = json!("opencode");
        Ok(v)
    }
    fn kill(&self, t: &str) -> Result<()> {
        self.inner.kill(t)
    }
    fn interrupt(&self, t: &str) -> Result<()> {
        self.inner.interrupt(t)
    }
}

// Isolate the environment override in a child test process, not unsafe process-global mutation.
#[test]
fn template_sh_fallback() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("auditor.md"),
        "---\ncommand: opencode --model test\nready: quiet\n---\nBạn là kiểm toán.\n",
    )?;
    let out = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "fallback_child", "--nocapture"])
        .env("AGM_HC_TEMPLATES", dir.path())
        .env("AGM_FALLBACK_CHILD", "1")
        .output()?;
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[test]
fn fallback_child() -> Result<()> {
    if std::env::var_os("AGM_FALLBACK_CHILD").is_none() {
        return Ok(());
    }
    let store = Store::open_in_memory()?;
    let hc = TemplateHc::default();
    let mut req = SpawnReq {
        template: "auditor".into(),
        task: "probe".into(),
        role: "probe".into(),
        name: Some("tpl-t".into()),
        cwd: Some("D:\\Dự án".into()),
        text: vec!["chỉ trả lời".into(), "OK".into()],
    };
    actions::prepare_rotation_spawn(&hc, &mut req)?;
    actions::spawn(&store, &hc, &Config::builtin(), &req, 1)?;
    let runs = hc.runs.borrow();
    assert_eq!(runs.len(), 2);
    assert_eq!(runs[0].argv, ["auditor", "chỉ trả lời OK"]);
    assert_eq!(runs[1].argv, ["opencode", "--model", "test"]);
    assert_eq!(
        (&runs[1].name, &runs[1].cwd, &runs[1].labels),
        (&runs[0].name, &runs[0].cwd, &runs[0].labels)
    );
    assert_eq!(
        hc.inner.calls.borrow()[1],
        "send t99 if_idle=true force=false Bạn là kiểm toán.\n\nchỉ trả lời OK"
    );
    assert_eq!(
        store.terminal("t99")?.unwrap().template.as_deref(),
        Some("auditor")
    );
    Ok(())
}

#[test]
fn unrelated_errors_do_not_fallback() -> Result<()> {
    for failure in [
        ("spawn_failed", "cannot start opencode: file not found"),
        ("spawn_failed", "application crashed"),
        ("permission_denied", "cannot start \"sh\""),
    ] {
        let hc = TemplateHc {
            failure: Some(failure),
            ..Default::default()
        };
        let store = Store::open_in_memory()?;
        let req = SpawnReq {
            template: "auditor".into(),
            task: "probe".into(),
            role: "probe".into(),
            ..Default::default()
        };
        let err = actions::spawn(&store, &hc, &Config::builtin(), &req, 1).unwrap_err();
        assert!(err.to_string().contains(failure.1));
        assert_eq!(hc.runs.borrow().len(), 1);
    }
    Ok(())
}

#[test]
fn unsafe_template_commands_are_rejected() -> Result<()> {
    for command in [
        "opencode 'quoted'",
        "opencode \"quoted\"",
        "opencode | cat",
        "opencode & x",
        "opencode; x",
        "opencode < x",
        "opencode > x",
        "opencode $X",
        "opencode `x`",
    ] {
        let dir = tempfile::tempdir()?;
        std::fs::write(
            dir.path().join("auditor.md"),
            format!("---\ncommand: {command}\n---\nbody"),
        )?;
        let out = std::process::Command::new(std::env::current_exe()?)
            .args(["--exact", "unsafe_command_child", "--nocapture"])
            .env("AGM_HC_TEMPLATES", dir.path())
            .env("AGM_FALLBACK_CHILD", "1")
            .output()?;
        assert!(
            out.status.success(),
            "{}{}",
            String::from_utf8_lossy(&out.stdout),
            String::from_utf8_lossy(&out.stderr)
        );
    }
    Ok(())
}

#[test]
fn unsafe_command_child() -> Result<()> {
    if std::env::var_os("AGM_FALLBACK_CHILD").is_none() {
        return Ok(());
    }
    let store = Store::open_in_memory()?;
    let hc = TemplateHc::default();
    let req = SpawnReq {
        template: "auditor".into(),
        task: "probe".into(),
        role: "probe".into(),
        ..Default::default()
    };
    let err = actions::spawn(&store, &hc, &Config::builtin(), &req, 1).unwrap_err();
    assert!(err.to_string().contains("ký tự shell"), "{err}");
    assert_eq!(hc.runs.borrow().len(), 1);
    Ok(())
}

#[test]
fn hc_cli_preserves_structured_stderr_failure() -> Result<()> {
    let dir = tempfile::tempdir()?;
    let bin = dir.path().join("fake-hc.cmd");
    std::fs::write(
        &bin,
        "@echo off\r\necho {\"error\":{\"kind\":\"spawn_failed\",\"message\":\"cannot start \\\"sh\\\": sh: not found on PATH\"}} 1>&2\r\nexit /b 1\r\n",
    )?;
    let hc = agmag::hc::HcCli {
        bin: bin.to_string_lossy().into_owned(),
    };
    let err = hc
        .run(&RunSpec {
            argv: vec!["auditor".into()],
            ..Default::default()
        })
        .unwrap_err();
    assert!(agmag::hc::is_missing_sh(&err), "{err:#}");
    Ok(())
}

#[test]
fn brief_timeout_goes_to_inbox_without_typing_into_shell() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("auditor.md"),
        "---\ncommand: opencode\n---\nBạn là kiểm toán.",
    )?;
    let out = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "timeout_child", "--nocapture"])
        .env("AGM_HC_TEMPLATES", dir.path())
        .env("AGM_NOT_READY", "1")
        .output()?;
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}

#[test]
fn timeout_child() -> Result<()> {
    if std::env::var_os("AGM_NOT_READY").is_none() {
        return Ok(());
    }
    let store = Store::open_in_memory()?;
    let hc = TemplateHc::default();
    let req = SpawnReq {
        template: "auditor".into(),
        task: "probe".into(),
        role: "probe".into(),
        text: vec!["chỉ trả lời OK".into()],
        ..Default::default()
    };
    let start = std::time::Instant::now();
    actions::spawn(&store, &hc, &Config::builtin(), &req, 1)?;
    assert!(start.elapsed().as_secs() < 25);
    assert!(
        !hc.inner
            .calls
            .borrow()
            .iter()
            .any(|c| c.starts_with("send"))
    );
    assert!(
        store
            .inbox(true)?
            .iter()
            .any(|i| i.source == "spawn" && i.text.contains("Bạn là kiểm toán.\n\nchỉ trả lời OK"))
    );
    Ok(())
}

#[test]
fn local_template_can_verify_and_spawn_when_listing_is_unavailable() -> Result<()> {
    let dir = tempfile::tempdir()?;
    std::fs::write(
        dir.path().join("auditor.md"),
        "---\ncommand: opencode --model test\n---\nBạn là kiểm toán.",
    )?;
    let out = std::process::Command::new(std::env::current_exe()?)
        .args(["--exact", "fallback_child", "--nocapture"])
        .env("AGM_HC_TEMPLATES", dir.path())
        .env("AGM_FALLBACK_CHILD", "1")
        .env("AGM_TEMPLATES_UNAVAILABLE", "1")
        .output()?;
    assert!(
        out.status.success(),
        "{}{}",
        String::from_utf8_lossy(&out.stdout),
        String::from_utf8_lossy(&out.stderr)
    );
    Ok(())
}
