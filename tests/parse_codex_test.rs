use agmag::model::*;
use agmag::parse::{LogParser, ParseError, codex::CodexParser, parser_for};

const MINI: &str = include_str!("fixtures/codex/mini.jsonl");

fn line(index: usize) -> &'static str {
    MINI.lines().nth(index).unwrap()
}

fn events(outcome: LineOutcome) -> Vec<ParseEvent> {
    match outcome {
        LineOutcome::Events(events) => events,
        other => panic!("expected events, got {other:?}"),
    }
}

#[test]
fn codex_usage_normalized() {
    let mut parser = parser_for(Source::Codex);
    parser.parse_line(line(1), 0).unwrap();
    let parsed = events(parser.parse_line(line(3), 123).unwrap());
    assert_eq!(
        parsed,
        vec![ParseEvent::Usage(UsageRecord {
            dedupe_key: "resp_1".into(),
            ts_ms: 1794960003000,
            model: Some("gpt-6.1-sol".into()),
            usage: Usage {
                input_uncached: 15688,
                cache_read: 13312,
                cache_write: 0,
                output: 285,
                reasoning: 50,
            },
        })]
    );
}

#[test]
fn token_count_with_null_info() {
    let parsed = events(CodexParser::default().parse_line(line(4), 0).unwrap());
    assert_eq!(
        parsed,
        vec![ParseEvent::Quota(QuotaSnapshot {
            provider: "codex".into(),
            ts_ms: 1794960004000,
            windows: vec![
                QuotaWindow {
                    window_minutes: 300,
                    used_percent: 22.0,
                    resets_at_s: 1794990587,
                },
                QuotaWindow {
                    window_minutes: 10080,
                    used_percent: 79.0,
                    resets_at_s: 1795408985,
                },
            ],
        })]
    );
}

#[test]
fn context_snapshot() {
    assert_eq!(
        events(CodexParser::default().parse_line(line(5), 0).unwrap()),
        vec![ParseEvent::Context(ContextSnapshot {
            ts_ms: 1794960005000,
            used_tokens: 116029,
            window_tokens: Some(475000),
        })]
    );
}

#[test]
fn meta_and_turn() {
    let mut parser = CodexParser::default();
    assert_eq!(
        events(parser.parse_line(line(0), 0).unwrap()),
        vec![ParseEvent::Meta(SessionMeta {
            session_id: "session_1".into(),
            cwd: Some(r"D:\Project\demo".into()),
            cli_version: Some("0.1.0".into()),
            started_ms: Some(1794960000000),
        })]
    );
    assert_eq!(
        events(parser.parse_line(line(2), 456).unwrap()),
        vec![ParseEvent::Turn(TurnSummary {
            ts_ms: 1794960002000,
            role: Role::User,
            summary: "hello".into(),
            byte_offset: 456,
        })]
    );
}

#[test]
fn seeded_model_used() {
    let mut parser = CodexParser::default();
    parser.seed_model("gpt-6.1-sol");
    let parsed = events(parser.parse_line(line(3), 0).unwrap());
    let ParseEvent::Usage(record) = &parsed[0] else {
        panic!("expected usage");
    };
    assert_eq!(record.model.as_deref(), Some("gpt-6.1-sol"));
}

#[test]
fn unknown_type_reported() {
    let mut parser = CodexParser::default();
    assert_eq!(
        parser.parse_line(line(6), 0).unwrap(),
        LineOutcome::Unknown("brand_new_kind".into())
    );
    assert!(matches!(
        parser.parse_line("{oops", 0),
        Err(ParseError::BadJson(_))
    ));
}

#[test]
fn combined_context_and_quota_preserve_missing_window() {
    let line = r#"{"type":"event_msg","payload":{"type":"token_count","info":{"last_token_usage":{"input_tokens":42}},"rate_limits":{"primary":null,"secondary":{"window_minutes":10080,"used_percent":79.0,"resets_at":1795408985}}}}"#;
    assert_eq!(
        events(CodexParser::default().parse_line(line, 0).unwrap()),
        vec![
            ParseEvent::Context(ContextSnapshot {
                ts_ms: 0,
                used_tokens: 42,
                window_tokens: None,
            }),
            ParseEvent::Quota(QuotaSnapshot {
                provider: "codex".into(),
                ts_ms: 0,
                windows: vec![QuotaWindow {
                    window_minutes: 10080,
                    used_percent: 79.0,
                    resets_at_s: 1795408985,
                }],
            }),
        ]
    );
}

#[test]
fn assistant_and_tools_are_summarized() {
    let mut parser = CodexParser::default();
    let long = "ế".repeat(250);
    let line = serde_json::json!({
        "type": "response_item",
        "payload": {"type": "message", "role": "assistant",
            "content": [{"text": format!(" \n{long}  ")}, {"text": "not included"}]}
    });
    let parsed = events(parser.parse_line(&line.to_string(), 99).unwrap());
    assert_eq!(
        parsed,
        vec![ParseEvent::Turn(TurnSummary {
            ts_ms: 0,
            role: Role::Assistant,
            summary: format!("{}…", "ế".repeat(199)),
            byte_offset: 99,
        })]
    );
    for kind in ["function_call", "custom_tool_call"] {
        let line = serde_json::json!({
            "type": "response_item", "payload": {"type": kind, "name": "shell"}
        });
        assert_eq!(
            events(parser.parse_line(&line.to_string(), 12).unwrap()),
            vec![ParseEvent::Turn(TurnSummary {
                ts_ms: 0,
                role: Role::Tool,
                summary: "[tool] shell".into(),
                byte_offset: 12,
            })]
        );
    }
}

#[test]
fn known_noise_is_ignored() {
    let mut parser = CodexParser::default();
    for line in [
        r#"{"type":"response_item","payload":{"type":"message","role":"developer","content":[{"text":"secret"}]}}"#,
        r#"{"type":"response_item","payload":{"type":"reasoning","encrypted_content":"secret"}}"#,
        r#"{"type":"response_item","payload":{"type":"function_call_output"}}"#,
        r#"{"type":"response_item","payload":{"type":"custom_tool_call_output"}}"#,
        r#"{"type":"response_item","payload":{"type":"additional_tools"}}"#,
        r#"{"type":"response_item","payload":{"type":"world_state"}}"#,
        r#"{"type":"event_msg","payload":{"type":"other_event"}}"#,
        r#"{"type":"event_msg","payload":{"type":"token_count","info":null}}"#,
        r#"{"type":"token_usage_record","payload":{"usage":{"input_tokens":29000}}}"#,
    ] {
        assert_eq!(parser.parse_line(line, 0).unwrap(), LineOutcome::Ignored);
    }
}

#[test]
#[ignore = "Task 1 real_sanitized.jsonl is not present in this worktree"]
fn real_fixture_smoke() {
    let path = std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
        .join("tests")
        .join("fixtures")
        .join("codex")
        .join("real_sanitized.jsonl");
    let fixture = std::fs::read_to_string(path).unwrap();
    let mut parser = CodexParser::default();
    let (mut errors, mut usages, mut quotas) = (0, 0, 0);
    let mut offset = 0;
    for line in fixture.split_inclusive('\n') {
        match parser.parse_line(line, offset) {
            Ok(LineOutcome::Events(events)) => {
                for event in events {
                    usages += usize::from(matches!(event, ParseEvent::Usage(_)));
                    quotas += usize::from(matches!(event, ParseEvent::Quota(_)));
                }
            }
            Err(_) => errors += 1,
            _ => {}
        }
        offset += line.len() as u64;
    }
    assert_eq!(errors, 0);
    assert!(usages >= 1);
    assert!(quotas >= 1);
}
