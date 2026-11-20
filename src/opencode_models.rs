//! Danh mục model của OpenCode (chỉ đọc): `providerID/id` → `limit.context`.
//! Nguồn: `opencode api GET /api/model` (server đang chạy), với cache trong tiến trình.
//! Không gọi mạng mỗi vòng daemon; không phụ thuộc `~/.cache/opencode/models.json` (có thể cũ).

use std::collections::BTreeMap;
use std::io::Read;
use std::process::{Command, Stdio};
use std::sync::OnceLock;
use std::thread;
use std::time::Duration;

use anyhow::{Result, bail};
use serde_json::Value;

#[derive(Debug, Clone, PartialEq, Default)]
pub struct ModelCatalog {
    /// Khóa `providerID/id` → cửa sổ ngữ cảnh (token). 0/missing bị bỏ.
    entries: BTreeMap<String, u64>,
}

impl ModelCatalog {
    /// Phân tích envelope `{ "location": ..., "data": [ { "providerID", "id", "limit": { "context" } } ] }`.
    /// Cũng chấp nhận mảng trần.
    pub fn from_json(s: &str) -> Result<Self> {
        let v: Value = serde_json::from_str(s)?;
        Ok(Self::from_value(&v))
    }

    pub fn from_value(v: &Value) -> Self {
        let mut entries = BTreeMap::new();
        let items: &[Value] = if let Some(arr) = v.get("data").and_then(|d| d.as_array()) {
            arr
        } else if let Some(arr) = v.as_array() {
            arr
        } else {
            &[]
        };
        for item in items {
            let provider = item
                .get("providerID")
                .and_then(|x| x.as_str())
                .unwrap_or("");
            let id = item.get("id").and_then(|x| x.as_str()).unwrap_or("");
            if provider.is_empty() || id.is_empty() {
                continue;
            }
            let context = item
                .get("limit")
                .and_then(|l| l.get("context"))
                .and_then(|c| c.as_u64())
                .unwrap_or(0);
            if context == 0 {
                continue;
            }
            entries.insert(format!("{provider}/{id}"), context);
        }
        Self { entries }
    }

    /// Tra cứu cửa sổ ngữ cảnh theo khóa `providerID/id`. Không có → None.
    pub fn context(&self, name: &str) -> Option<u64> {
        self.entries.get(name).copied().filter(|&w| w != 0)
    }

    /// Chạy `opencode api GET /api/model` một lần (cache trong tiến trình).
    pub fn cached() -> Option<Self> {
        static CACHE: OnceLock<Option<ModelCatalog>> = OnceLock::new();
        CACHE.get_or_init(|| Self::load().ok()).clone()
    }

    /// Đọc danh mục từ server OpenCode đang chạy. Lỗi → Err.
    pub fn load() -> Result<Self> {
        // Windows: `opencode` là shim `.cmd`/`.ps1`; CreateProcess không chạy `.ps1` trực tiếp.
        let mut last_err = None;
        for bin in ["opencode", "opencode.cmd"] {
            match load_with(bin) {
                Ok(catalog) => return Ok(catalog),
                Err(err) => last_err = Some(err),
            }
        }
        Err(last_err.unwrap_or_else(|| anyhow::anyhow!("opencode not found")))
    }

    /// Số model trong danh mục (tiện cho debug/log).
    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }
}

/// Chạy một tiến trình `opencode api GET /api/model`, giới hạn 5 giây rồi kill.
fn load_with(bin: &str) -> Result<ModelCatalog> {
    let mut child = Command::new(bin)
        .args(["api", "GET", "/api/model"])
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .spawn()?;
    let mut stdout = child.stdout.take().expect("stdout is piped");
    let (tx, rx) = std::sync::mpsc::channel();
    // Read on a thread: the catalog exceeds a pipe buffer, so waiting for exit before reading would deadlock.
    let reader = thread::spawn(move || {
        let mut buf = String::new();
        let read = stdout.read_to_string(&mut buf).map(|_| buf);
        let _ = tx.send(read);
    });
    match rx.recv_timeout(Duration::from_secs(5)) {
        Ok(Ok(text)) => {
            let status = child.wait()?;
            let _ = reader.join();
            if !status.success() {
                bail!("`{bin} api GET /api/model` exited with {status}");
            }
            ModelCatalog::from_json(&text)
        }
        Ok(Err(err)) => {
            let _ = child.wait();
            let _ = reader.join();
            Err(err.into())
        }
        Err(_) => {
            let _ = child.kill();
            let _ = child.wait();
            let _ = reader.join();
            bail!("`{bin} api GET /api/model` timed out after 5 s");
        }
    }
}
