//! Lớp gọi herdr-core (`hc`). Mọi lời gọi đi qua trait `Hc` để test bằng bản giả.
//! `HcCli` chạy `hc <lệnh> --json`: đầu ra là đúng các đối tượng của API (`hc schema`), không phải văn bản.

use std::collections::BTreeMap;
use std::process::Command;

use anyhow::{Context, Result, bail};
use serde_json::Value;

use crate::store::TerminalRow;

#[derive(Debug, thiserror::Error)]
#[error("hc {kind}: {message}")]
pub struct HcFailure {
    pub kind: String,
    pub message: String,
}

pub fn is_missing_sh(error: &anyhow::Error) -> bool {
    error.downcast_ref::<HcFailure>().is_some_and(|e| {
        e.kind == "spawn_failed"
            && (e.message.contains("\"sh\"")
                || e.message.contains("'sh'")
                || e.message.contains("sh:"))
    })
}

pub struct HcTemplate {
    pub argv: Vec<String>,
    pub body: String,
}

/// Read only: never rewrite the user's hc template.
pub fn load_template(name: &str) -> Result<HcTemplate> {
    if name.is_empty()
        || !name
            .chars()
            .all(|c| c.is_ascii_alphanumeric() || matches!(c, '-' | '_'))
    {
        bail!("tên template không an toàn: {name}");
    }
    let dir = if let Some(dir) = std::env::var_os("AGM_HC_TEMPLATES") {
        std::path::PathBuf::from(dir)
    } else if cfg!(windows) {
        std::path::PathBuf::from(
            std::env::var_os("APPDATA").context("thiếu APPDATA để tìm template hc")?,
        )
        .join("herdr-core")
        .join("templates")
    } else {
        dirs::home_dir()
            .context("không tìm thấy home")?
            .join(".config")
            .join("herdr-core")
            .join("templates")
    };
    let path = dir.join(format!("{name}.md"));
    let raw = std::fs::read_to_string(&path)
        .with_context(|| format!("đọc template {}", path.display()))?;
    let normalized = raw.replace("\r\n", "\n");
    let rest = normalized
        .strip_prefix("---\n")
        .context("template thiếu front matter")?;
    let (front, body) = rest
        .split_once("\n---")
        .context("template thiếu dấu đóng front matter")?;
    if !body.is_empty() && !body.starts_with('\n') {
        bail!("dấu đóng front matter không hợp lệ");
    }
    let commands: Vec<&str> = front
        .lines()
        .filter_map(|l| l.strip_prefix("command:").map(str::trim))
        .collect();
    let [command] = commands.as_slice() else {
        bail!("template cần đúng một command:")
    };
    if command.contains(['\'', '"', '|', '&', ';', '<', '>', '$', '`']) {
        bail!(
            "command: chứa dấu nháy hoặc ký tự shell; dự phòng chỉ hỗ trợ argv tách theo khoảng trắng"
        );
    }
    let argv: Vec<String> = command.split_whitespace().map(String::from).collect();
    if argv.is_empty() {
        bail!("command: rỗng");
    }
    Ok(HcTemplate {
        argv,
        body: body.trim().to_string(),
    })
}

#[derive(Debug, Clone, Default, PartialEq)]
pub struct RunSpec {
    /// Template hoặc lệnh (argv[0]) và các tham số.
    pub argv: Vec<String>,
    pub name: Option<String>,
    pub cwd: Option<String>,
    pub labels: BTreeMap<String, String>,
}

pub trait Hc {
    fn templates(&self) -> Result<Vec<Value>> {
        bail!("hc templates không được hỗ trợ")
    }
    fn ls(&self, all: bool) -> Result<Vec<Value>>;
    fn get(&self, terminal: &str) -> Result<Value>;
    /// `if_idle`: chỉ gõ khi đích đang idle. `force`: gõ cả khi đích đang bị chặn (trả lời hộp thoại).
    fn send(&self, terminal: &str, text: &str, if_idle: bool, force: bool) -> Result<Value>;
    /// Chạy nền, sống tiếp khi cha kết thúc; trả đối tượng terminal mới (có `address`).
    fn run(&self, spec: &RunSpec) -> Result<Value>;
    fn kill(&self, terminal: &str) -> Result<()>;
    fn interrupt(&self, terminal: &str) -> Result<()>;
}

pub struct HcCli {
    pub bin: String,
}

impl Default for HcCli {
    fn default() -> Self {
        HcCli {
            bin: std::env::var("AGM_HC_BIN").unwrap_or_else(|_| "hc".into()),
        }
    }
}

impl HcCli {
    /// `args` đã gồm `--json` ngay sau tên lệnh con. hc báo lỗi bằng `{"error": …}` kể cả khi mã thoát 0.
    fn call(&self, args: &[String]) -> Result<Value> {
        let out = Command::new(&self.bin)
            .args(args)
            .output()
            .with_context(|| format!("chạy {} {}", self.bin, args.join(" ")))?;
        let stdout = String::from_utf8_lossy(&out.stdout);
        let stderr = String::from_utf8_lossy(&out.stderr);
        let cmd = args.first().map_or("", String::as_str);
        let v: Value =
            serde_json::from_str(stdout.trim()).unwrap_or(Value::String(stdout.trim().to_string()));
        let err_output: Value = serde_json::from_str(stderr.trim()).unwrap_or(Value::Null);
        if let Some(err) = v.get("error").or_else(|| err_output.get("error")) {
            return Err(HcFailure {
                kind: err["kind"].as_str().unwrap_or("unknown").into(),
                message: err["message"]
                    .as_str()
                    .map(String::from)
                    .unwrap_or_else(|| err.to_string()),
            }
            .into());
        }
        if !out.status.success() {
            bail!(
                "hc {cmd} lỗi ({}): {}{}",
                out.status,
                stdout.trim(),
                String::from_utf8_lossy(&out.stderr).trim()
            );
        }
        Ok(v)
    }
}

fn s(v: &str) -> String {
    v.to_string()
}

/// Tham số cho `hc run`: `--json` và mọi cờ đứng trước `--`, argv của chương trình đứng sau.
pub fn run_args(spec: &RunSpec) -> Vec<String> {
    let mut args = vec![s("run"), s("--json"), s("--bg"), s("--outlive-parent")];
    if let Some(n) = &spec.name {
        args.extend([s("--name"), n.clone()]);
    }
    if let Some(c) = &spec.cwd {
        args.extend([s("--cwd"), c.clone()]);
    }
    for (k, v) in &spec.labels {
        args.extend([s("--label"), format!("{k}={v}")]);
    }
    args.push(s("--"));
    args.extend(spec.argv.iter().cloned());
    args
}

/// Tham số cho `hc send`; `--` để tin bắt đầu bằng `-` không bị hiểu là cờ.
pub fn send_args(terminal: &str, text: &str, if_idle: bool, force: bool) -> Vec<String> {
    let mut args = vec![s("send"), s("--json")];
    if if_idle {
        args.extend([s("--if"), s("idle")]);
    }
    if force {
        args.push(s("--force"));
    }
    args.extend([s("--"), s(terminal), s(text)]);
    args
}

impl Hc for HcCli {
    fn templates(&self) -> Result<Vec<Value>> {
        let v = self.call(&[s("templates"), s("--json")])?;
        v["templates"]
            .as_array()
            .cloned()
            .context("hc templates không trả danh sách templates")
    }

    fn ls(&self, all: bool) -> Result<Vec<Value>> {
        let mut args = vec![s("ls"), s("--json")];
        if all {
            args.push(s("--all"));
        }
        let v = self.call(&args)?;
        Ok(v["terminals"].as_array().cloned().unwrap_or_default())
    }

    fn get(&self, terminal: &str) -> Result<Value> {
        self.call(&[s("get"), s("--json"), s("--"), s(terminal)])
    }

    fn send(&self, terminal: &str, text: &str, if_idle: bool, force: bool) -> Result<Value> {
        self.call(&send_args(terminal, text, if_idle, force))
    }

    fn run(&self, spec: &RunSpec) -> Result<Value> {
        self.call(&run_args(spec))
    }

    fn kill(&self, terminal: &str) -> Result<()> {
        self.call(&[s("kill"), s("--json"), s("--"), s(terminal)])
            .map(|_| ())
    }

    fn interrupt(&self, terminal: &str) -> Result<()> {
        self.call(&[s("interrupt"), s("--json"), s("--"), s(terminal)])
            .map(|_| ())
    }
}

/// Đoán app từ argv khi chương trình chưa tự báo trạng thái 7501.
pub fn app_from_argv(argv: &[String]) -> Option<String> {
    let first = argv.first().filter(|a| !a.trim().is_empty())?;
    let base = first
        .rsplit(['\\', '/'])
        .next()
        .unwrap_or(first)
        .to_lowercase();
    // Shim của npm/pnpm/bun trên Windows: claude.cmd, codex.ps1, …
    let base = [".exe", ".cmd", ".bat", ".ps1"]
        .iter()
        .find_map(|ext| base.strip_suffix(ext))
        .unwrap_or(&base);
    Some(match base {
        "claude" => "claude-code".into(),
        "agy" => "antigravity".into(),
        other => other.to_string(),
    })
}

/// Đối tượng Terminal của hc (ảnh chụp, sự kiện `created`, `hc get`, `hc ls`) → hàng CSDL.
pub fn terminal_row_from(v: &Value) -> TerminalRow {
    let str_of = |k: &str| v[k].as_str().map(String::from);
    let argv: Vec<String> = v["argv"]
        .as_array()
        .map(|a| {
            a.iter()
                .filter_map(|x| x.as_str().map(String::from))
                .collect()
        })
        .unwrap_or_default();
    let mut labels: BTreeMap<String, String> = v["labels"]
        .as_object()
        .map(|o| {
            o.iter()
                .filter_map(|(k, x)| x.as_str().map(|x| (k.clone(), x.to_string())))
                .collect()
        })
        .unwrap_or_default();
    // herdr's location ("workspace / tab"): a label in snapshots/events, a top-level field in `hc ls`.
    if let Some(place) = v["where"].as_str().filter(|w| !w.trim().is_empty()) {
        labels
            .entry("where".into())
            .or_insert_with(|| place.to_string());
    }
    // `hc ls` cho `status` là chuỗi + `agent`; `hc get`/sự kiện cho `status` là đối tượng.
    let reported_app = v["status"]["app"]
        .as_str()
        .filter(|a| !a.trim().is_empty())
        .map(String::from)
        .or_else(|| {
            v["agent"]
                .as_str()
                .and_then(|a| app_from_argv(&[a.to_string()]))
        });
    TerminalRow {
        address: str_of("address").unwrap_or_default(),
        name: str_of("name"),
        app: reported_app.or_else(|| app_from_argv(&argv)),
        cwd: str_of("cwd"),
        task: labels.get("agm.task").cloned(),
        role: labels.get("agm.role").cloned(),
        labels,
        template: str_of("template"),
        state: match v["state"].as_str() {
            Some("live") | None => "live".into(),
            Some(_) => "finished".into(),
        },
        status: v["status"]["state"]
            .as_str()
            .or_else(|| v["status"].as_str())
            .map(String::from),
        kind: v["status"]["kind"].as_str().map(String::from),
        msg: v["status"]["msg"].as_str().map(String::from),
        turn: v["turn"].as_i64().unwrap_or(0),
        reported: v["reported"].as_bool().unwrap_or(false),
        parent: str_of("parent"),
        created_ms: v["created_ms"].as_i64().unwrap_or(0),
        finished_ms: v["finished_ms"].as_i64(),
        exit_code: v["exit_code"].as_i64(),
        session: None,
        link_source: None,
        ..Default::default()
    }
}
