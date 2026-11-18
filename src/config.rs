use std::{collections::BTreeMap, io::ErrorKind, path::Path};

use anyhow::{Context, Result};
use serde::Deserialize;

use crate::model::{Source, Usage};

#[derive(Deserialize, Clone, Copy, PartialEq, Debug)]
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
        }
    }
}

/// Chờ người dùng triển khai trong bài tập Learn by Doing.
pub fn cost_usd(_u: &Usage, _p: &Price) -> f64 {
    todo!("TODO(human)")
}

pub fn context_pct(used: u64, window: Option<u64>) -> Option<f64> {
    let window = window.filter(|&window| window != 0)?;
    Some((used as f64 / window as f64 * 1000.0).round() / 10.0)
}
