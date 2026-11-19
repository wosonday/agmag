use std::{collections::BTreeMap, io::ErrorKind, path::Path};

use anyhow::{Context, Result};
use serde::{Deserialize, Serialize};

use crate::model::{Source, Usage};

#[derive(Deserialize, Serialize, Clone, Copy, PartialEq, Debug)]
#[serde(rename_all = "lowercase")]
pub enum Billing {
    Subscription,
    Api,
}

#[derive(Deserialize, Debug)]
pub struct ProviderCfg {
    pub billing: Billing,
}

#[derive(Deserialize, Debug)]
pub struct Price {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
}

#[derive(Deserialize, Debug)]
pub struct ModelCfg {
    pub provider: String,
    pub context_window: Option<u64>,
    pub price: Option<Price>,
}

#[derive(Deserialize, Debug)]
#[serde(deny_unknown_fields)]
pub struct Config {
    #[serde(default)]
    pub providers: BTreeMap<String, ProviderCfg>,
    #[serde(default)]
    pub models: BTreeMap<String, ModelCfg>,
    #[serde(default)]
    pub aliases: BTreeMap<String, String>,
    #[serde(default)]
    pub policy: Policy,
}

/// Ngưỡng và hạn mức cho policy (ngân sách, cảnh báo ngữ cảnh, xoay vòng thợ).
#[derive(Deserialize, Debug, Clone, PartialEq)]
#[serde(default, deny_unknown_fields)]
pub struct Policy {
    /// Số thợ (nhãn agm.role=worker) sống cùng lúc tối đa.
    pub max_workers: u32,
    /// Tên terminal quản lý nhận cảnh báo; None = chỉ hộp thư người dùng.
    pub manager: Option<String>,
    /// % ngữ cảnh: cảnh báo.
    pub ctx_warn: f64,
    /// % ngữ cảnh: xoay vòng phiên.
    pub ctx_rotate: f64,
    /// Phút chờ agent bàn giao trước khi bỏ cuộc và báo người dùng.
    pub handoff_timeout_min: u64,
    /// Phút chờ ghép phiên kế nhiệm trước khi chuyển `needs_attention`.
    pub link_timeout_min: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            max_workers: 3,
            manager: None,
            ctx_warn: 50.0,
            ctx_rotate: 70.0,
            handoff_timeout_min: 10,
            link_timeout_min: 5,
        }
    }
}

impl Config {
    pub fn load(path: &Path) -> Result<Config> {
        let text = match std::fs::read_to_string(path) {
            Ok(text) => text,
            Err(err) if err.kind() == ErrorKind::NotFound => return Ok(Self::builtin()),
            Err(err) => {
                return Err(err).with_context(|| format!("Read config {}", path.display()));
            }
        };
        toml::from_str(&text).with_context(|| format!("Parse config {}", path.display()))
    }

    pub fn builtin() -> Config {
        // Embedded data is checked by config integration tests.
        toml::from_str(include_str!("../assets/providers.default.toml"))
            .expect("embedded provider config must be valid")
    }

    pub fn model(&self, name: &str) -> Option<&ModelCfg> {
        let resolved = self.aliases.get(name).map(String::as_str).unwrap_or(name);
        self.models.get(resolved)
    }

    pub fn billing_for_model(&self, name: &str) -> Option<Billing> {
        let model = self.model(name)?;
        self.providers.get(&model.provider).map(|cfg| cfg.billing)
    }

    pub fn provider_for_source(source: Source) -> &'static str {
        match source {
            Source::Codex => "codex",
            Source::Claude => "claude",
            Source::OpenCode => "opencode",
            Source::Agy => "agy",
        }
    }
}

/// Chi phí USD của một lượng token đã chuẩn hóa, theo giá USD / 1M token.
/// Chỉ được gọi cho provider `billing = "api"` có giá (report bỏ qua gói tháng).
/// `reasoning` đã nằm trong `output` nên không tính thêm.
pub fn cost_usd(u: &Usage, p: &Price) -> f64 {
    // Nhân trước rồi chia một lần để không mất độ chính xác với lượng token nhỏ.
    (u.input_uncached as f64 * p.input
        + u.cache_read as f64 * p.cache_read
        + u.cache_write as f64 * p.cache_write
        + u.output as f64 * p.output)
        / 1_000_000.0
}

pub fn context_pct(used: u64, window: Option<u64>) -> Option<f64> {
    let window = window.filter(|&window| window != 0)?;
    Some((used as f64 / window as f64 * 1000.0).round() / 10.0)
}
