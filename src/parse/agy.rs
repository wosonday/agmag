//! Bộ đọc `transcript.jsonl` của Antigravity CLI (`agy`).
//! Mỗi dòng là một bước hội thoại: `USER_INPUT`, `PLANNER_RESPONSE`, `GENERIC`, …
//! Trong `transcript.jsonl`, `input_tokens` là phần chưa cache và `output_tokens` đã gồm cả thinking.

use serde_json::Value;

use super::{LogParser, ParseError};
use crate::model::*;

#[derive(Default)]
pub struct AgyParser {
    model: Option<String>,
    last_ts: Option<i64>,
}

impl LogParser for AgyParser {
    fn seed_model(&mut self, model: &str) {
        self.model = Some(model.to_owned());
    }

    fn seed_last_ts(&mut self, ts_ms: i64) {
        if self.last_ts.is_none_or(|prev| ts_ms > prev) {
            self.last_ts = Some(ts_ms);
        }
    }

    fn parse_line(&mut self, line: &str, byte_offset: u64) -> Result<LineOutcome, ParseError> {
        let value: Value =
            serde_json::from_str(line).map_err(|error| ParseError::BadJson(error.to_string()))?;
        let kind = value["type"].as_str().unwrap_or_default();
        let parsed_ts = value["created_at"].as_str().and_then(parse_rfc3339_ms);
        if let Some(ts) = parsed_ts {
            self.last_ts = Some(ts);
        }

        let mut events = Vec::new();
        match kind {
            "USER_INPUT" => {
                let ts_ms = parsed_ts.or(self.last_ts).ok_or(ParseError::MissingTimestamp)?;
                if let Some(raw) = value["content"].as_str() {
                    let text = extract_user_request(raw);
                    let summary = summarize(text);
                    if !summary.is_empty() {
                        events.push(ParseEvent::Turn(TurnSummary {
                            ts_ms,
                            role: Role::User,
                            summary,
                            byte_offset,
                        }));
                    }
                }
            }
            "PLANNER_RESPONSE" => {
                let ts_ms = parsed_ts.or(self.last_ts).ok_or(ParseError::MissingTimestamp)?;
                if let Some(model) = value["model"].as_str().filter(|m| !m.is_empty())
                    && self.model.as_deref() != Some(model)
                {
                    self.seed_model(model);
                    events.push(ParseEvent::Model(model.to_owned()));
                }
                let has_usage = value.get("input_tokens").is_some()
                    || value.get("cache_read_tokens").is_some()
                    || value.get("output_tokens").is_some()
                    || value.get("thinking_tokens").is_some();
                if has_usage {
                    let count = |key: &str| value[key].as_u64().unwrap_or_default();
                    let input_uncached = count("input_tokens");
                    let cache_read = count("cache_read_tokens");
                    let thinking = count("thinking_tokens");
                    // `output_tokens` của agy đã bao gồm thinking; giữ bất biến `reasoning ⊂ output` (config.rs).
                    let output = count("output_tokens").max(thinking);
                    if input_uncached + cache_read + output > 0 {
                        let dedupe_key = value["step_index"]
                            .as_u64()
                            .map(|s| format!("step:{s}"))
                            .unwrap_or_else(|| format!("off:{byte_offset}"));
                        events.push(ParseEvent::Usage(UsageRecord {
                            dedupe_key,
                            ts_ms,
                            model: self.model.clone(),
                            usage: Usage {
                                input_uncached,
                                cache_read,
                                cache_write: 0,
                                output,
                                reasoning: thinking,
                            },
                        }));
                        let used_tokens = input_uncached.saturating_add(cache_read);
                        if used_tokens > 0 {
                            events.push(ParseEvent::Context(ContextSnapshot {
                                ts_ms,
                                used_tokens,
                                window_tokens: None,
                            }));
                        }
                    }
                }
                if let Some(summary) = planner_summary(&value) {
                    events.push(ParseEvent::Turn(TurnSummary {
                        ts_ms,
                        role: Role::Assistant,
                        summary,
                        byte_offset,
                    }));
                }
            }
            "GENERIC" | "SYSTEM" | "SYSTEM_MESSAGE" | "ERROR" | "ERROR_MESSAGE" | "CHECKPOINT"
            | "USER_SETTINGS_CHANGE" => {}
            _ => return Ok(LineOutcome::Unknown(kind.to_owned())),
        }

        Ok(if events.is_empty() {
            LineOutcome::Ignored
        } else {
            LineOutcome::Events(events)
        })
    }
}

fn extract_user_request(raw: &str) -> &str {
    const START: &str = "<USER_REQUEST>";
    const END: &str = "</USER_REQUEST>";
    if let Some(after_start) = raw.find(START).map(|i| &raw[i + START.len()..])
        && let Some(end) = after_start.find(END)
    {
        return after_start[..end].trim();
    }
    raw
}

fn planner_summary(value: &Value) -> Option<String> {
    if let Some(text) = value["content"].as_str() {
        let s = summarize(text);
        if !s.is_empty() {
            return Some(s);
        }
    }
    let tools = value["tool_calls"].as_array()?;
    let names: Vec<&str> = tools
        .iter()
        .filter_map(|t| t["name"].as_str())
        .filter(|n| !n.is_empty())
        .collect();
    if names.is_empty() {
        None
    } else {
        Some(summarize(&format!("[tool] {}", names.join(", "))))
    }
}
