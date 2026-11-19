use agmag::launch::{
    build_successor_argv, check_claude_hook_installed, extract_launch_argv, parse_cmd_tokens,
    parse_launch_spec, LaunchSpec,
};
use serde_json::json;

fn sv(items: &[&str]) -> Vec<String> {
    items.iter().map(|s| (*s).to_string()).collect()
}

#[test]
fn codex_keeps_dash_c_and_strips_resume_and_old_prompt() {
    let spec = parse_launch_spec(
        "codex",
        &sv(&[
            "codex", "resume", "sess-99", "-c", "model=\"o3\"", "-p", "fast",
            "-s", "workspace-write", "-C", "D:\\old", "--search",
        ]),
        r"D:\repo",
    )
    .unwrap();
    assert_eq!(
        spec.argv,
        sv(&["-c", "model=\"o3\"", "-p", "fast", "-s", "workspace-write", "--search"])
    );

    let spec2 = parse_launch_spec("codex", &sv(&["codex.EXE", "-m", "gpt-6.1-sol", "old prompt"]), r"D:\repo").unwrap();
    assert_eq!(spec2.argv, sv(&["-m", "gpt-6.1-sol"]));
}

#[test]
fn claude_strips_dash_c_and_resume_and_rejects_print_mode() {
    let spec = parse_launch_spec(
        "claude-code",
        &sv(&[
            "C:\\bin\\claude.cmd", "-c", "--resume", "abc-123", "--fork-session",
            "--model", "opus", "--permission-mode", "auto", "--add-dir", "D:\\a", "D:\\b",
        ]),
        r"D:\repo",
    )
    .unwrap();
    assert_eq!(
        spec.argv,
        sv(&["--model", "opus", "--permission-mode", "auto", "--add-dir", "D:\\a", "D:\\b"])
    );

    assert!(parse_launch_spec("claude-code", &sv(&["claude", "-p", "hi"]), r"D:\repo").is_err());
    assert!(parse_launch_spec("claude-code", &sv(&["claude", "--print"]), r"D:\repo").is_err());
}

#[test]
fn claude_variadic_flags_do_not_swallow_successor_prompt() {
    let spec = parse_launch_spec(
        "claude-code",
        &sv(&["claude", "--model", "opus", "--add-dir", "foo"]),
        r"D:\repo",
    )
    .unwrap();
    let argv = build_successor_argv(&spec, "Read .agents/handoff/1-10.md");
    assert_eq!(
        argv,
        sv(&["claude", "--model", "opus", "--add-dir", "foo", "--", "Read .agents/handoff/1-10.md"])
    );
    let roundtrip = LaunchSpec::from_json(&spec.to_json().unwrap()).unwrap();
    assert_eq!(roundtrip, spec);
}

#[test]
fn rejects_shell_and_unknown_flags() {
    assert!(parse_launch_spec("claude-code", &sv(&["pwsh.exe"]), r"D:\repo").is_err());
    assert!(parse_launch_spec("codex", &sv(&["codex", "--mystery-flag"]), r"D:\repo").is_err());
    assert!(parse_launch_spec("claude-code", &sv(&["claude", "--mystery-flag"]), r"D:\repo").is_err());
}

#[test]
fn extract_prefers_foreground_and_skips_shell() {
    let v = json!({
        "argv": ["pwsh.exe"],
        "foreground": {"argv": ["C:\\npm\\claude.cmd", "--model", "opus"]}
    });
    assert_eq!(
        extract_launch_argv(&v, "claude-code"),
        Some(sv(&["C:\\npm\\claude.cmd", "--model", "opus"]))
    );
    let shell_only = json!({"argv": ["pwsh.exe"], "foreground": {"argv": ["pwsh.exe"]}});
    assert_eq!(extract_launch_argv(&shell_only, "claude-code"), None);
}

#[test]
fn parse_cmd_tokens_handles_quotes_without_shell() {
    assert_eq!(
        parse_cmd_tokens(r#"claude --model opus --add-dir "D:\My Dir""#).unwrap(),
        sv(&["claude", "--model", "opus", "--add-dir", r"D:\My Dir"])
    );
    assert!(parse_cmd_tokens(r#"claude "unclosed"#).is_err());
}

#[test]
fn check_claude_hook_installed_validates_settings() -> anyhow::Result<()> {
    let tmp = tempfile::tempdir()?;
    assert!(check_claude_hook_installed(tmp.path()).is_err());
    let dir = tmp.path().join(".claude");
    std::fs::create_dir_all(&dir)?;
    std::fs::write(dir.join("settings.json"), r#"{"statusLine":{"type":"command","command":"other"}}"#)?;
    assert!(check_claude_hook_installed(tmp.path()).is_err());
    std::fs::write(
        dir.join("settings.json"),
        r#"{"statusLine":{"type":"command","command":"\"C:\\bin\\agm.exe\" hook statusline"}}"#,
    )?;
    check_claude_hook_installed(tmp.path())?;
    Ok(())
}
