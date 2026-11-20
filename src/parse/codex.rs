use serde_json::Value;

use super::{LogParser, ParseError};
use crate::model::*;

#[derive(Default)]
pub struct CodexParser {
    model: Option<String>,
}

impl LogParser for CodexParser {
    fn seed_model(&mut self, model: &str) {
        self.model = Some(model.to_owned());
    }

    fn parse_line(&mut self, line: &str, byte_offset: u64) -> Result<LineOutcome, ParseError> {
        let value: Value =
            serde_json::from_str(line).map_err(|error| ParseError::BadJson(error.to_string()))?;
        let kind = value["type"].as_str().unwrap_or_default();
        let payload = &value["payload"];
        let ts_ms = value["timestamp"]
            .as_str()
            .and_then(parse_rfc3339_ms)
            .unwrap_or_default();
        let event = match kind {
            "session_meta" => ParseEvent::Meta(SessionMeta {
                session_id: payload["id"].as_str().unwrap_or_default().to_owned(),
                cwd: payload["cwd"].as_str().map(str::to_owned),
                cli_version: payload["cli_version"].as_str().map(str::to_owned),
                started_ms: payload["timestamp"].as_str().and_then(parse_rfc3339_ms),
            }),
            "turn_context" => {
                let Some(model) = payload["model"].as_str() else {
                    return Ok(LineOutcome::Ignored);
                };
                self.seed_model(model);
                let mut events = vec![ParseEvent::Model(model.to_owned())];
                // Codex writes the effort chosen for this turn (`/model` can change it mid-session).
                let effort = payload["effort"].as_str().or_else(|| {
                    payload["collaboration_mode"]["settings"]["reasoning_effort"].as_str()
                });
                if let Some(effort) = effort.filter(|e| !e.is_empty()) {
                    events.push(ParseEvent::Effort(effort.to_owned()));
                }
                return Ok(LineOutcome::Events(events));
            }
            "token_usage_record" => {
                let Some(response_id) = payload["response_id"].as_str() else {
                    return Ok(LineOutcome::Ignored);
                };
                let usage = &payload["usage"];
                let count = |key: &str| usage[key].as_u64().unwrap_or_default();
                let cache_read = count("cached_input_tokens");
                ParseEvent::Usage(UsageRecord {
                    dedupe_key: response_id.to_owned(),
                    ts_ms,
                    model: self.model.clone(),
                    usage: Usage {
                        input_uncached: count("input_tokens").saturating_sub(cache_read),
                        cache_read,
                        cache_write: count("cache_write_input_tokens"),
                        output: count("output_tokens"),
                        reasoning: count("reasoning_output_tokens"),
                    },
                })
            }
            "event_msg" => {
                if payload["type"].as_str() != Some("token_count") {
                    return Ok(LineOutcome::Ignored);
                }
                let mut events = Vec::new();
                let info = &payload["info"];
                if !info.is_null() {
                    events.push(ParseEvent::Context(ContextSnapshot {
                        ts_ms,
                        used_tokens: info["last_token_usage"]["input_tokens"]
                            .as_u64()
                            .unwrap_or_default(),
                        window_tokens: info["model_context_window"].as_u64(),
                    }));
                }
                let limits = &payload["rate_limits"];
                if !limits.is_null() {
                    let windows = ["primary", "secondary"]
                        .into_iter()
                        .filter_map(|name| {
                            let window = &limits[name];
                            if window.is_null() {
                                return None;
                            }
                            Some(QuotaWindow {
                                window_minutes: u32::try_from(window["window_minutes"].as_u64()?)
                                    .ok()?,
                                used_percent: window["used_percent"].as_f64()?,
                                resets_at_s: window["resets_at"].as_i64()?,
                            })
                        })
                        .collect();
                    events.push(ParseEvent::Quota(QuotaSnapshot {
                        provider: "codex".to_owned(),
                        ts_ms,
                        windows,
                    }));
                }
                return Ok(if events.is_empty() {
                    LineOutcome::Ignored
                } else {
                    LineOutcome::Events(events)
                });
            }
            "response_item" => {
                let item_type = payload["type"].as_str().unwrap_or_default();
                let (role, summary) = match item_type {
                    "message" => {
                        let role = match payload["role"].as_str() {
                            Some("user") => Role::User,
                            Some("assistant") => Role::Assistant,
                            _ => return Ok(LineOutcome::Ignored),
                        };
                        let text = payload["content"]
                            .as_array()
                            .and_then(|content| {
                                content.iter().find_map(|item| item["text"].as_str())
                            })
                            .unwrap_or_default();
                        (role, summarize(text))
                    }
                    "function_call" | "custom_tool_call" => (
                        Role::Tool,
                        summarize(&format!(
                            "[tool] {}",
                            payload["name"].as_str().unwrap_or_default()
                        )),
                    ),
                    "reasoning"
                    | "function_call_output"
                    | "custom_tool_call_output"
                    | "additional_tools"
                    | "world_state"
                    // Codex 0.170: tin giữa agent con (nội dung mã hóa) và bản nén ngữ cảnh; không mang token.
                    | "agent_message"
                    | "compaction" => return Ok(LineOutcome::Ignored),
                    _ => return Ok(LineOutcome::Unknown(item_type.to_owned())),
                };
                ParseEvent::Turn(TurnSummary {
                    ts_ms,
                    role,
                    summary,
                    byte_offset,
                })
            }
            "reasoning"
            | "function_call_output"
            | "custom_tool_call_output"
            | "additional_tools"
            | "world_state"
            | "compacted"
            | "inter_agent_communication_metadata" => return Ok(LineOutcome::Ignored),
            _ => return Ok(LineOutcome::Unknown(kind.to_owned())),
        };
        Ok(LineOutcome::Events(vec![event]))
    }
}
