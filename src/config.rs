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

#[derive(Deserialize, Debug, Clone, PartialEq)]
pub struct Price {
    pub input: f64,
    pub cache_read: f64,
    pub cache_write: f64,
    pub output: f64,
}

impl Price {
    pub const FREE: Price = Price {
        input: 0.0,
        cache_read: 0.0,
        cache_write: 0.0,
        output: 0.0,
    };
}

/// Reasoning-effort levels; agy appends one to its model ids (`gemini-4-argon-high`).
pub const EFFORT_LEVELS: [&str; 7] = [
    "minimal", "low", "medium", "high", "xhigh", "max", "thinking",
];

#[derive(Deserialize)]
struct PriceTable {
    prices: BTreeMap<String, Price>,
}

/// Built-in list prices (`assets/prices.toml`), always available even when the user has a providers.toml.
pub fn builtin_prices() -> &'static BTreeMap<String, Price> {
    static TABLE: std::sync::OnceLock<BTreeMap<String, Price>> = std::sync::OnceLock::new();
    TABLE.get_or_init(|| {
        toml::from_str::<PriceTable>(include_str!("../assets/prices.toml"))
            .expect("embedded price table must be valid")
            .prices
    })
}

/// Ids to try for a logged model name, most specific first: as logged, without the OpenCode provider, without a
/// date suffix (`claude-haiku-4-5-20251001`), without an agy effort suffix (`claude-opus-5-5-high`).
pub fn model_id_candidates(name: &str) -> Vec<String> {
    let mut out: Vec<String> = Vec::new();
    let mut push = |s: &str| {
        if !s.is_empty() && !out.iter().any(|o| o == s) {
            out.push(s.to_string());
        }
    };
    let bare = split_provider(name).map_or(name, |(_, id)| id);
    for base in [name, bare] {
        push(base);
        let undated = match base.rsplit_once('-') {
            Some((head, tail)) if tail.len() == 8 && tail.bytes().all(|b| b.is_ascii_digit()) => {
                head
            }
            _ => base,
        };
        push(undated);
        if let Some((head, level)) = undated.rsplit_once('-')
            && EFFORT_LEVELS.contains(&level)
        {
            push(head);
        }
    }
    out
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
    /// Danh mục model OpenCode (chỉ đọc, cache trong tiến trình). Không nằm trong providers.toml.
    #[serde(skip)]
    pub opencode_models: Option<crate::opencode_models::ModelCatalog>,
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
    /// % ngữ cảnh: cảnh báo cho phiên Claude Code/Codex (thay `ctx_warn`).
    pub interactive_ctx_warn: f64,
    /// % ngữ cảnh: xoay vòng phiên Claude Code/Codex (thay `ctx_rotate`).
    pub interactive_ctx_rotate: f64,
    /// Phút chờ agent bàn giao trước khi bỏ cuộc và báo người dùng.
    pub handoff_timeout_min: u64,
    /// Phút chờ ghép phiên kế nhiệm trước khi chuyển `needs_attention`.
    pub link_timeout_min: u64,
    /// Số tháng dương lịch gần nhất cho thống kê & backfill mặc định.
    pub stats_months: u32,
    /// Phút giữa hai lần daemon đọc hạn mức agy (`agy --print /usage`); 0 = tắt.
    pub agy_quota_poll_min: u64,
    /// Phút coi dữ liệu hạn mức gói là cũ (quota stale); 0 → mặc định 60.
    #[serde(default = "default_quota_stale_min")]
    pub quota_stale_min: u64,
    /// Phút giữa hai lần daemon đọc hạn mức Codex (`codex app-server`); 0 = tắt.
    #[serde(default = "default_codex_quota_poll_min")]
    pub codex_quota_poll_min: u64,
    /// Phút giữa hai lần daemon đọc hạn mức OpenCode Go (`opencode api …/usage/limits`); 0 = tắt.
    #[serde(default = "default_opencode_quota_poll_min")]
    pub opencode_quota_poll_min: u64,
}

impl Default for Policy {
    fn default() -> Self {
        Policy {
            max_workers: 3,
            manager: None,
            ctx_warn: 50.0,
            ctx_rotate: 70.0,
            interactive_ctx_warn: 40.0,
            interactive_ctx_rotate: 50.0,
            handoff_timeout_min: 10,
            link_timeout_min: 5,
            stats_months: 4,
            agy_quota_poll_min: 15,
            quota_stale_min: 60,
            codex_quota_poll_min: 15,
            opencode_quota_poll_min: 15,
        }
    }
}

impl Policy {
    /// Ngưỡng `(cảnh báo, xoay vòng)` theo app: Claude Code/Codex dùng cặp `interactive_*`,
    /// còn lại (thợ OpenCode, shell, app chưa rõ) dùng `ctx_warn`/`ctx_rotate` chung.
    pub fn ctx_thresholds(&self, app: Option<&str>) -> (f64, f64) {
        match app {
            Some("claude-code" | "codex") => {
                (self.interactive_ctx_warn, self.interactive_ctx_rotate)
            }
            _ => (self.ctx_warn, self.ctx_rotate),
        }
    }

    /// Each threshold must be finite and in (0, 100], and warn ≤ rotate in each pair. Otherwise the daemon
    /// would start a rotation at `rotate` and cancel it below `warn` on every tick, or never act at all.
    pub fn validate(&self) -> Result<()> {
        for (warn_key, warn, rotate_key, rotate) in [
            ("ctx_warn", self.ctx_warn, "ctx_rotate", self.ctx_rotate),
            (
                "interactive_ctx_warn",
                self.interactive_ctx_warn,
                "interactive_ctx_rotate",
                self.interactive_ctx_rotate,
            ),
        ] {
            for (key, value) in [(warn_key, warn), (rotate_key, rotate)] {
                if !value.is_finite() || value <= 0.0 || value > 100.0 {
                    anyhow::bail!("{key} = {value} must be a number in (0, 100]");
                }
            }
            if warn > rotate {
                anyhow::bail!("{warn_key} = {warn} must not exceed {rotate_key} = {rotate}");
            }
        }
        Ok(())
    }
}

fn default_quota_stale_min() -> u64 {
    60
}

fn default_codex_quota_poll_min() -> u64 {
    15
}

fn default_opencode_quota_poll_min() -> u64 {
    15
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
        let cfg: Config =
            toml::from_str(&text).with_context(|| format!("Parse config {}", path.display()))?;
        cfg.policy
            .validate()
            .with_context(|| format!("Invalid [policy] in {}", path.display()))?;
        Ok(cfg)
    }

    pub fn builtin() -> Config {
        // Embedded data is checked by config integration tests.
        toml::from_str(include_str!("../assets/providers.default.toml"))
            .expect("embedded provider config must be valid")
    }

    /// Looks up `name`, then (for OpenCode's `provider/id` keys) the id without its first segment, so
    /// `openai/gpt-6.1-sol` reuses `[models."gpt-6.1-sol"]`. An exact `[models."provider/id"]` entry wins.
    pub fn model(&self, name: &str) -> Option<&ModelCfg> {
        let find = |n: &str| {
            self.models
                .get(self.aliases.get(n).map(String::as_str).unwrap_or(n))
        };
        find(name).or_else(|| find(split_provider(name)?.1))
    }

    /// Context window for a model: providers.toml wins (an exact `provider/id` key, then the bare id, both via
    /// `model`), then the OpenCode catalog for OpenCode sources. Nothing is guessed.
    pub fn context_window(&self, source: &Source, name: &str) -> Option<u64> {
        if let Some(window) = self.model(name).and_then(|m| m.context_window) {
            return Some(window);
        }
        if source != &Source::OpenCode {
            return None;
        }
        match &self.opencode_models {
            Some(catalog) => catalog.context(name),
            None => crate::opencode_models::ModelCatalog::cached().and_then(|c| c.context(name)),
        }
    }

    /// List price of a model: a price set in providers.toml wins, then the built-in table, then `-free`/`:free`
    /// ids at $0. Unknown models → None (shown as "?", never guessed).
    pub fn price_for(&self, name: &str) -> Option<Price> {
        let candidates = model_id_candidates(name);
        if let Some(p) = candidates
            .iter()
            .find_map(|c| self.model(c).and_then(|m| m.price.clone()))
        {
            return Some(p);
        }
        if let Some(p) = candidates.iter().find_map(|c| builtin_prices().get(c)) {
            return Some(p.clone());
        }
        let last = name.rsplit('/').next().unwrap_or(name);
        (last.ends_with("-free") || last.ends_with(":free")).then_some(Price::FREE)
    }

    /// Billing of a model. Used by `agm cost`/`agm ls` to decide whether USD can be shown.
    pub fn billing_for_model(&self, name: &str) -> Option<Billing> {
        // A declared model wins (e.g. `openai/gpt-6.1-sol` via ChatGPT login = Codex subscription). Otherwise an
        // OpenCode key's provider prefix decides, so future models need no [models] entry. Unknown → None, never guessed.
        let provider = match self.model(name) {
            Some(model) => model.provider.as_str(),
            None => split_provider(name)?.0,
        };
        self.providers.get(provider).map(|cfg| cfg.billing)
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

/// Splits an OpenCode model key `provider/id` at the first `/` (`nvidia-kimi/moonshotai/kimi-k3.1-pro` →
/// `("nvidia-kimi", "moonshotai/kimi-k3.1-pro")`). Bare ids (`gpt-6.1-sol`) → None.
pub fn split_provider(name: &str) -> Option<(&str, &str)> {
    name.split_once('/')
        .filter(|(p, id)| !p.is_empty() && !id.is_empty())
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
