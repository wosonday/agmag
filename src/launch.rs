//! Phân tích, lọc an toàn và dựng lệnh khởi chạy phiên kế nhiệm (`launch_spec`) cho Claude Code và Codex.

use std::path::Path;

use anyhow::{Context, Result, bail};
use serde::{Deserialize, Serialize};
use serde_json::Value;

use crate::hc::app_from_argv;

pub const LAUNCH_SPEC_VERSION: u32 = 1;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct LaunchSpec {
    pub version: u32,
    pub app: String,
    pub executable: String,
    pub argv: Vec<String>,
    pub cwd: String,
}

impl LaunchSpec {
    pub fn from_json(s: &str) -> Result<Self> {
        let spec: Self = serde_json::from_str(s).context("launch_spec JSON không hợp lệ")?;
        if spec.version != LAUNCH_SPEC_VERSION {
            bail!("phiên bản launch_spec không hỗ trợ: {}", spec.version);
        }
        if spec.executable.trim().is_empty() {
            bail!("launch_spec thiếu executable");
        }
        Ok(spec)
    }

    pub fn to_json(&self) -> Result<String> {
        Ok(serde_json::to_string(self)?)
    }
}

/// Tách chuỗi `--cmd "..."` thành danh sách đối số (không chạy qua shell).
pub fn parse_cmd_tokens(s: &str) -> Result<Vec<String>> {
    let mut out = Vec::new();
    let mut cur = String::new();
    let mut in_token = false;
    let mut quote: Option<char> = None;
    let mut chars = s.chars().peekable();
    while let Some(c) = chars.next() {
        match quote {
            Some('"') => match c {
                '"' => quote = None,
                '\\' if matches!(chars.peek(), Some('"' | '\\')) => {
                    cur.push(chars.next().unwrap());
                }
                other => cur.push(other),
            },
            Some('\'') => {
                if c == '\'' {
                    quote = None;
                } else {
                    cur.push(c);
                }
            }
            Some(_) => unreachable!(),
            None => match c {
                '"' | '\'' => {
                    quote = Some(c);
                    in_token = true;
                }
                c if c.is_whitespace() => {
                    if in_token {
                        out.push(std::mem::take(&mut cur));
                        in_token = false;
                    }
                }
                other => {
                    cur.push(other);
                    in_token = true;
                }
            },
        }
    }
    if quote.is_some() {
        bail!("chuỗi lệnh có dấu nháy chưa đóng");
    }
    if in_token {
        out.push(cur);
    }
    if out.is_empty() {
        bail!("chuỗi lệnh rỗng");
    }
    Ok(out)
}

pub fn is_shell(exe: &str) -> bool {
    let base = exe.rsplit(['\\', '/']).next().unwrap_or(exe).to_lowercase();
    let base = [".exe", ".cmd", ".bat", ".ps1"]
        .iter()
        .find_map(|ext| base.strip_suffix(ext))
        .unwrap_or(&base);
    matches!(
        base,
        "pwsh" | "powershell" | "cmd" | "bash" | "sh" | "zsh" | "fish" | "nu" | "wsl"
    )
}

/// Rút lệnh chạy agent thật từ `hc get` (`foreground.argv` trước, rồi `argv`); trả `None` nếu chỉ thấy shell.
pub fn extract_launch_argv(hc_get: &Value, expected_app: &str) -> Option<Vec<String>> {
    for candidate in [&hc_get["foreground"]["argv"], &hc_get["argv"]] {
        let Some(arr) = candidate.as_array() else {
            continue;
        };
        let argv: Vec<String> = arr
            .iter()
            .filter_map(|v| v.as_str().map(String::from))
            .collect();
        let Some(first) = argv.first() else { continue };
        if is_shell(first) {
            continue;
        }
        if app_from_argv(&argv).as_deref() == Some(expected_app) {
            return Some(argv);
        }
    }
    None
}

/// Phân tích và lọc `raw_argv` theo adapter riêng của `app` (`codex` | `claude-code`).
pub fn parse_launch_spec(app: &str, raw_argv: &[String], cwd: &str) -> Result<LaunchSpec> {
    if cwd.trim().is_empty() {
        bail!("thiếu cwd để lưu launch_spec");
    }
    let Some(exe) = raw_argv.first().map(|s| s.trim()).filter(|s| !s.is_empty()) else {
        bail!("thiếu tệp thực thi");
    };
    if is_shell(exe) {
        bail!(
            "chỉ phát hiện shell '{exe}' — hãy truyền lệnh agent tường minh qua `-- {app} ...` hoặc `--cmd`"
        );
    }
    if app_from_argv(raw_argv).as_deref() != Some(app) {
        bail!("lệnh '{exe}' không khớp với app '{app}'");
    }
    let filtered = match app {
        "codex" => filter_codex(&raw_argv[1..])?,
        "claude-code" => filter_claude(&raw_argv[1..])?,
        other => bail!("agm manage chưa hỗ trợ app '{other}'"),
    };
    Ok(LaunchSpec {
        version: LAUNCH_SPEC_VERSION,
        app: app.into(),
        executable: exe.into(),
        argv: filtered,
        cwd: cwd.into(),
    })
}

/// Dựng `argv` mở phiên kế nhiệm: luôn chèn `--` trước prompt để các cờ variadic (như `--add-dir`) không nuốt prompt.
pub fn build_successor_argv(spec: &LaunchSpec, prompt: &str) -> Vec<String> {
    let mut out = Vec::with_capacity(spec.argv.len() + 3);
    out.push(spec.executable.clone());
    out.extend(spec.argv.iter().cloned());
    out.push("--".into());
    out.push(prompt.into());
    out
}

/// Kiểm tra Claude Code đã cài `agm hook statusline` trong `~/.claude/settings.json` chưa.
pub fn check_claude_hook_installed(home_dir: &Path) -> Result<()> {
    let path = home_dir.join(".claude").join("settings.json");
    let text = std::fs::read_to_string(&path).with_context(|| {
        format!(
            "chưa thấy {} — chạy `agm hook install` trước khi bật quản lý Claude Code",
            path.display()
        )
    })?;
    let v: Value = serde_json::from_str(&text)
        .with_context(|| format!("{} không phải JSON hợp lệ", path.display()))?;
    let cmd = v["statusLine"]["command"]
        .as_str()
        .or_else(|| v["statusLine"].as_str())
        .unwrap_or_default();
    if !cmd.contains("hook statusline") {
        bail!(
            "{} chưa cấu hình `agm hook statusline` — chạy `agm hook install` (hoặc gọi `agm hook statusline` trong script statusLine) trước",
            path.display()
        );
    }
    Ok(())
}

fn split_eq(arg: &str) -> Option<(&str, &str)> {
    arg.starts_with("--").then(|| arg.split_once('=')).flatten()
}

fn filter_codex(args: &[String]) -> Result<Vec<String>> {
    const SINGLE: &[&str] = &[
        "-c",
        "--config",
        "-p",
        "--profile",
        "-m",
        "--model",
        "-s",
        "--sandbox",
        "-a",
        "--ask-for-approval",
        "--add-dir",
        "--enable",
        "--disable",
        "-i",
        "--image",
        "--local-provider",
        "--remote",
        "--remote-auth-token-env",
        "--cyber-access-program",
    ];
    const STRIP_SINGLE: &[&str] = &["-C", "--cd"];
    const ZERO: &[&str] = &[
        "--search",
        "--no-alt-screen",
        "--no-daemon",
        "--approve-for-me",
        "--dangerously-bypass-approvals-and-sandbox",
        "--dangerously-bypass-hook-trust",
        "--oss",
        "--strict-config",
        "--worktree",
    ];
    const BAD_SUBCMDS: &[&str] = &[
        "exec",
        "e",
        "review",
        "login",
        "logout",
        "mcp",
        "plugin",
        "app-server",
        "remote-control",
        "app",
        "completion",
        "update",
        "doctor",
        "sandbox",
        "debug",
        "apply",
        "a",
        "queue",
        "archive",
        "delete",
        "migrate-rollouts",
        "migrate-nfs-db",
        "unarchive",
        "cloud",
        "exec-server",
        "features",
        "help",
    ];

    let mut out = Vec::new();
    let mut i = 0;
    let mut saw_positional = false;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            // Mọi thứ sau `--` là prompt cũ → bỏ khi mở phiên mới.
            break;
        }
        if let Some((flag, val)) = split_eq(a) {
            if SINGLE.contains(&flag) {
                out.push(format!("{flag}={val}"));
            } else if STRIP_SINGLE.contains(&flag) {
                // Bỏ `-C`/`--cd` vì `hc run --cwd` đã đặt thư mục làm việc.
            } else {
                bail!("cờ Codex không được hỗ trợ trong launch_spec: {a}");
            }
            i += 1;
            continue;
        }
        if a.starts_with('-') {
            if ZERO.contains(&a.as_str()) {
                out.push(a.clone());
                i += 1;
            } else if SINGLE.contains(&a.as_str()) {
                let Some(val) = args.get(i + 1) else {
                    bail!("cờ Codex '{a}' thiếu giá trị");
                };
                out.extend([a.clone(), val.clone()]);
                i += 2;
            } else if STRIP_SINGLE.contains(&a.as_str()) {
                if args.get(i + 1).is_none() {
                    bail!("cờ Codex '{a}' thiếu giá trị");
                }
                i += 2;
            } else {
                bail!("cờ Codex không được hỗ trợ trong launch_spec: {a}");
            }
            continue;
        }
        if i == 0 && matches!(a.as_str(), "resume" | "fork") {
            // Bỏ subcommand `resume`/`fork`, cờ `--last`/`--all`, hoặc ID phiên cũ đi kèm.
            i += 1;
            while let Some(next) = args.get(i) {
                if next == "--last" || next == "--all" {
                    i += 1;
                } else if !next.starts_with('-') && !saw_positional {
                    saw_positional = true;
                    i += 1;
                } else {
                    break;
                }
            }
            continue;
        }
        if i == 0 && BAD_SUBCMDS.contains(&a.as_str()) {
            bail!("không thể quản lý xoay vòng cho subcommand không tương tác của Codex: '{a}'");
        }
        if saw_positional {
            bail!(
                "đối số vị trí nhập nhằng trong lệnh Codex: '{a}' — hãy truyền lệnh sạch qua `-- codex ...`"
            );
        }
        // Prompt cũ ở cuối lệnh: bỏ qua để phiên mới nhận prompt bàn giao.
        saw_positional = true;
        i += 1;
    }
    Ok(out)
}

fn filter_claude(args: &[String]) -> Result<Vec<String>> {
    const SINGLE: &[&str] = &[
        "--model",
        "--permission-mode",
        "--agent",
        "--effort",
        "--fallback-model",
        "--append-system-prompt",
        "--append-system-prompt-file",
        "--autocompact",
        "--max-budget-usd",
        "-n",
        "--name",
        "--settings",
    ];
    const VARIADIC: &[&str] = &[
        "--add-dir",
        "--allowedTools",
        "--allowed-tools",
        "--disallowedTools",
        "--disallowed-tools",
        "--tools",
        "--betas",
        "--mcp-config",
        "--plugin-dir",
        "--plugin-url",
        "--file",
    ];
    const ZERO: &[&str] = &[
        "--dangerously-skip-permissions",
        "--allow-dangerously-skip-permissions",
        "--chrome",
        "--no-chrome",
        "--verbose",
        "--bare",
        "--ax-screen-reader",
        "--exclude-dynamic-system-prompt-sections",
        "--ide",
    ];
    const STRIP_ZERO: &[&str] = &["-c", "--continue", "--fork-session"];
    const STRIP_OPT_ARG: &[&str] = &["-r", "--resume", "--from-pr"];
    const BAD_SUBCMDS: &[&str] = &[
        "agents",
        "attach",
        "auth",
        "auto-mode",
        "doctor",
        "gateway",
        "import",
        "install",
        "logs",
        "mcp",
        "plugin",
        "plugins",
        "purge",
        "respawn",
        "rm",
        "setup-token",
        "stop",
        "kill",
        "ultrareview",
        "update",
        "upgrade",
    ];

    let mut out = Vec::new();
    let mut i = 0;
    let mut saw_positional = false;
    while i < args.len() {
        let a = &args[i];
        if a == "--" {
            break;
        }
        if a == "-p" || a == "--print" {
            bail!("không hỗ trợ chế độ không tương tác '{a}' của Claude Code cho phiên xoay vòng");
        }
        if let Some((flag, val)) = split_eq(a) {
            if SINGLE.contains(&flag) || VARIADIC.contains(&flag) {
                out.push(format!("{flag}={val}"));
            } else if STRIP_OPT_ARG.contains(&flag) || STRIP_ZERO.contains(&flag) {
                // Bỏ qua.
            } else {
                bail!("cờ Claude Code không được hỗ trợ trong launch_spec: {a}");
            }
            i += 1;
            continue;
        }
        if a.starts_with('-') {
            if ZERO.contains(&a.as_str()) {
                out.push(a.clone());
                i += 1;
            } else if STRIP_ZERO.contains(&a.as_str()) {
                i += 1;
            } else if SINGLE.contains(&a.as_str()) {
                let Some(val) = args.get(i + 1) else {
                    bail!("cờ Claude Code '{a}' thiếu giá trị");
                };
                out.extend([a.clone(), val.clone()]);
                i += 2;
            } else if VARIADIC.contains(&a.as_str()) {
                i += 1;
                let start = i;
                while let Some(next) = args.get(i) {
                    if next.starts_with('-') {
                        break;
                    }
                    i += 1;
                }
                if i == start {
                    bail!("cờ Claude Code '{a}' thiếu giá trị");
                }
                out.push(a.clone());
                out.extend(args[start..i].iter().cloned());
            } else if STRIP_OPT_ARG.contains(&a.as_str()) {
                i += 1;
                if let Some(next) = args.get(i)
                    && !next.starts_with('-')
                {
                    i += 1;
                }
            } else {
                bail!("cờ Claude Code không được hỗ trợ trong launch_spec: {a}");
            }
            continue;
        }
        if i == 0 && BAD_SUBCMDS.contains(&a.as_str()) {
            bail!("không thể quản lý xoay vòng cho subcommand của Claude Code: '{a}'");
        }
        if saw_positional {
            bail!(
                "đối số vị trí nhập nhằng trong lệnh Claude Code: '{a}' — hãy truyền lệnh sạch qua `-- claude ...`"
            );
        }
        saw_positional = true;
        i += 1;
    }
    Ok(out)
}
