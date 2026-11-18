use serde_json::Value;

use super::{LogParser, ParseError};
use crate::model::*;

#[derive(Default)]
pub struct ClaudeParser {
    model: Option<String>,
    meta_emitted: bool,
}

impl LogParser for ClaudeParser {
    fn seed_model(&mut self, model: &str) {
        self.model = Some(model.to_owned());
    }

    fn parse_line(&mut self, line: &str, byte_offset: u64) -> Result<LineOutcome, ParseError> {
        let value: Value =
            serde_json::from_str(line).map_err(|error| ParseError::BadJson(error.to_string()))?;
        let kind = value["type"].as_str().unwrap_or_default();
        let timestamp = value["timestamp"].as_str().and_then(parse_rfc3339_ms);
        let ts_ms = timestamp.unwrap_or_default();
        let mut events = Vec::new();
        if !self.meta_emitted
            && let (Some(session_id), Some(cwd)) =
                (value["sessionId"].as_str(), value["cwd"].as_str())
        {
            events.push(ParseEvent::Meta(SessionMeta {
                session_id: session_id.to_owned(),
                cwd: Some(cwd.to_owned()),
                cli_version: value["version"].as_str().map(str::to_owned),
                started_ms: timestamp,
            }));
            self.meta_emitted = true;
        }

        let message = &value["message"];
        let content = &message["content"];
        match kind {
            "assistant" => {
                if message["model"].as_str() != Some("<synthetic>") {
                    if let Some(model) = message["model"].as_str()
                        && self.model.as_deref() != Some(model)
                    {
                        self.seed_model(model);
                        events.push(ParseEvent::Model(model.to_owned()));
                    }
                    if let Some(id) = message["id"].as_str() {
                        let usage = &message["usage"];
                        let count = |key: &str| usage[key].as_u64().unwrap_or_default();
                        let usage = Usage {
                            input_uncached: count("input_tokens"),
                            cache_read: count("cache_read_input_tokens"),
                            cache_write: count("cache_creation_input_tokens"),
                            output: count("output_tokens"),
                            reasoning: usage["output_tokens_details"]["thinking_tokens"]
                                .as_u64()
                                .unwrap_or_default(),
                        };
                        let used_tokens = usage
                            .input_uncached
                            .saturating_add(usage.cache_read)
                            .saturating_add(usage.cache_write);
                        events.push(ParseEvent::Usage(UsageRecord {
                            dedupe_key: id.to_owned(),
                            ts_ms,
                            model: self.model.clone(),
                            usage,
                        }));
                        events.push(ParseEvent::Context(ContextSnapshot {
                            ts_ms,
                            used_tokens,
                            window_tokens: None,
                        }));
                    }
                }
                let summary = text_content(content).map(summarize).unwrap_or_else(|| {
                    content
                        .as_array()
                        .and_then(|blocks| blocks.iter().find(|block| block["type"] == "tool_use"))
                        .map(|tool| {
                            summarize(&format!(
                                "[tool] {}",
                                tool["name"].as_str().unwrap_or_default()
                            ))
                        })
                        .unwrap_or_default()
                });
                events.push(ParseEvent::Turn(TurnSummary {
                    ts_ms,
                    role: Role::Assistant,
                    summary,
                    byte_offset,
                }));
            }
            "user" => {
                if let Some(text) = text_content(content) {
                    events.push(ParseEvent::Turn(TurnSummary {
                        ts_ms,
                        role: Role::User,
                        summary: summarize(text),
                        byte_offset,
                    }));
                }
            }
            "custom-title"
            | "agent-name"
            | "session-owner"
            | "queue-operation"
            | "attachment"
            | "last-prompt"
            | "atis-latch"
            | "api-request-shape"
            | "api-request-blob"
            | "api-request"
            | "tools-used"
            | "summary"
            | "system"
            | "file-history-snapshot" => {}
            _ if events.is_empty() => return Ok(LineOutcome::Unknown(kind.to_owned())),
            _ => {}
        }
        Ok(if events.is_empty() {
            LineOutcome::Ignored
        } else {
            LineOutcome::Events(events)
        })
    }
}

fn text_content(content: &Value) -> Option<&str> {
    content.as_str().or_else(|| {
        content.as_array()?.iter().find_map(|block| {
            (block["type"] == "text")
                .then(|| block["text"].as_str())
                .flatten()
        })
    })
}
