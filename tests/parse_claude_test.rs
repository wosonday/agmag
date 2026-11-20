use agmag::model::*;
use agmag::parse::{ParseError, parser_for};

const MINI: &str = include_str!("fixtures/claude/mini.jsonl");

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
fn claude_usage_normalized() {
    let parsed = events(parser_for(Source::Claude).parse_line(line(2), 123).unwrap());
    assert!(parsed.contains(&ParseEvent::Usage(UsageRecord {
        dedupe_key: "msg_1".into(),
        ts_ms: 1794960002000,
        model: Some("claude-opus-5-5".into()),
        usage: Usage {
            input_uncached: 2,
            cache_read: 356264,
            cache_write: 281,
            output: 1208,
            reasoning: 348,
        },
    })));
}

#[test]
fn duplicate_message_id_last_wins() {
    let mut parser = parser_for(Source::Claude);
    let records: Vec<_> = [1, 2]
        .into_iter()
        .flat_map(|i| events(parser.parse_line(line(i), 0).unwrap()))
        .filter_map(|event| match event {
            ParseEvent::Usage(record) => Some(record),
            _ => None,
        })
        .collect();
    assert_eq!(records.len(), 2);
    assert!(records.iter().all(|record| record.dedupe_key == "msg_1"));
    assert_eq!(records[0].usage.output, 10);
    assert_eq!(records[1].usage.output, 1208);
}

#[test]
fn context_from_last_turn() {
    let mut parser = parser_for(Source::Claude);
    parser.parse_line(line(1), 0).unwrap();
    let parsed = events(parser.parse_line(line(2), 0).unwrap());
    assert!(parsed.contains(&ParseEvent::Context(ContextSnapshot {
        ts_ms: 1794960002000,
        used_tokens: 356547,
        window_tokens: None,
    })));
}

#[test]
fn meta_unicode_cwd() {
    let mut parser = parser_for(Source::Claude);
    assert_eq!(
        events(parser.parse_line(line(0), 17).unwrap()),
        vec![
            ParseEvent::Meta(SessionMeta {
                session_id: "s-1".into(),
                cwd: Some(r"D:\Dự án\x".into()),
                cli_version: Some("2.0.0".into()),
                started_ms: Some(1794960000000),
            }),
            ParseEvent::Turn(TurnSummary {
                ts_ms: 1794960000000,
                role: Role::User,
                summary: "xin chào".into(),
                byte_offset: 17,
            }),
        ]
    );
    for i in [0, 1, 2] {
        assert!(
            !events(parser.parse_line(line(i), 0).unwrap())
                .iter()
                .any(|event| matches!(event, ParseEvent::Meta(_)))
        );
    }
}

#[test]
fn ignored_and_unknown() {
    let mut parser = parser_for(Source::Claude);
    assert_eq!(parser.parse_line(line(3), 0).unwrap(), LineOutcome::Ignored);
    assert_eq!(
        parser.parse_line(line(4), 0).unwrap(),
        LineOutcome::Unknown("weird".into())
    );
    for kind in [
        "custom-title",
        "agent-name",
        "session-owner",
        "queue-operation",
        "attachment",
        "last-prompt",
        "atis-latch",
        "api-request-shape",
        "api-request",
        "tools-used",
        "summary",
        "system",
        "file-history-snapshot",
    ] {
        let line = serde_json::json!({"type": kind}).to_string();
        assert_eq!(parser.parse_line(&line, 0).unwrap(), LineOutcome::Ignored);
    }
    assert!(matches!(
        parser.parse_line("{oops", 0),
        Err(ParseError::BadJson(_))
    ));
}

#[test]
fn model_changes_and_seed_model() {
    let mut parser = parser_for(Source::Claude);
    parser.seed_model("claude-opus-5-5");
    let parsed = events(parser.parse_line(line(1), 0).unwrap());
    assert!(
        !parsed
            .iter()
            .any(|event| matches!(event, ParseEvent::Model(_)))
    );
    parser.seed_model("other");
    let parsed = events(parser.parse_line(line(2), 0).unwrap());
    assert_eq!(
        parsed.first(),
        Some(&ParseEvent::Model("claude-opus-5-5".into()))
    );
    let parsed = events(parser.parse_line(line(2), 0).unwrap());
    assert!(
        !parsed
            .iter()
            .any(|event| matches!(event, ParseEvent::Model(_)))
    );
}

#[test]
fn synthetic_only_emits_turn_and_tool_summary_is_unicode_safe() {
    let mut parser = parser_for(Source::Claude);
    let line = r#"{"type":"assistant","message":{"model":"<synthetic>","content":[{"type":"text","text":"  lỗi\n mạng " }],"usage":{"output_tokens":100}}}"#;
    assert_eq!(
        events(parser.parse_line(line, 42).unwrap()),
        vec![ParseEvent::Turn(TurnSummary {
            ts_ms: 0,
            role: Role::Assistant,
            summary: "lỗi mạng".into(),
            byte_offset: 42,
        })]
    );
    let line = serde_json::json!({"type":"assistant","message":{
        "model":"<synthetic>", "content":[{"type":"tool_use","name":"ế".repeat(250)}]
    }});
    let parsed = events(parser.parse_line(&line.to_string(), 7).unwrap());
    let ParseEvent::Turn(turn) = &parsed[0] else {
        panic!("expected turn")
    };
    assert_eq!(turn.role, Role::Assistant);
    assert_eq!(turn.summary, format!("[tool] {}…", "ế".repeat(192)));
    assert_eq!(turn.byte_offset, 7);
}

#[test]
fn user_text_blocks_and_tool_results() {
    let mut parser = parser_for(Source::Claude);
    assert_eq!(parser.parse_line(r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"secret"}]}}"#, 0).unwrap(), LineOutcome::Ignored);
    let parsed = events(parser.parse_line(r#"{"type":"user","message":{"content":[{"type":"tool_result","content":"secret"},{"type":"text","text":"  xin\n chào "},{"type":"text","text":"not included"}]}}"#, 9).unwrap());
    assert_eq!(
        parsed,
        vec![ParseEvent::Turn(TurnSummary {
            ts_ms: 0,
            role: Role::User,
            summary: "xin chào".into(),
            byte_offset: 9,
        })]
    );
}

#[test]
fn missing_thinking_defaults_to_zero_and_text_precedes_tools() {
    let mut parser = parser_for(Source::Claude);
    let parsed = events(parser.parse_line(r#"{"type":"assistant","message":{"id":"m","model":"claude","content":[{"type":"tool_use","name":"shell"},{"type":"text","text":"first"},{"type":"text","text":"second"}],"usage":{"input_tokens":3,"output_tokens":4}}}"#, 8).unwrap());
    assert_eq!(
        parsed,
        vec![
            ParseEvent::Model("claude".into()),
            ParseEvent::Usage(UsageRecord {
                dedupe_key: "m".into(),
                ts_ms: 0,
                model: Some("claude".into()),
                usage: Usage {
                    input_uncached: 3,
                    cache_read: 0,
                    cache_write: 0,
                    output: 4,
                    reasoning: 0
                },
            }),
            ParseEvent::Context(ContextSnapshot {
                ts_ms: 0,
                used_tokens: 3,
                window_tokens: None
            }),
            ParseEvent::Turn(TurnSummary {
                ts_ms: 0,
                role: Role::Assistant,
                summary: "first".into(),
                byte_offset: 8
            }),
        ]
    );
}

#[test]
fn real_fixture_smoke() {
    let fixture = include_str!("fixtures/claude/real_sanitized.jsonl");
    let mut parser = parser_for(Source::Claude);
    let (mut errors, mut usages) = (0, 0);
    let mut offset = 0;
    for line in fixture.split_inclusive('\n') {
        match parser.parse_line(line, offset) {
            Ok(LineOutcome::Events(events)) => {
                usages += events
                    .iter()
                    .filter(|event| matches!(event, ParseEvent::Usage(_)))
                    .count();
            }
            Err(_) => errors += 1,
            _ => {}
        }
        offset += line.len() as u64;
    }
    assert_eq!(errors, 0);
    assert!(usages >= 1);
}

#[test]
fn assistant_effort_emitted_once_per_change() {
    let mut parser = parser_for(Source::Claude);
    let line = |effort: &str, id: &str| {
        format!(
            r#"{{"type":"assistant","timestamp":"2026-11-20T02:11:33.160Z","effort":"{effort}","message":{{"id":"{id}","model":"claude-opus-5-5","usage":{{"input_tokens":1,"output_tokens":1}},"content":[]}}}}"#
        )
    };
    let count = |evs: Vec<ParseEvent>| {
        evs.iter()
            .filter(|e| matches!(e, ParseEvent::Effort(_)))
            .count()
    };
    let first = events(parser.parse_line(&line("high", "m1"), 0).unwrap());
    assert!(first.contains(&ParseEvent::Effort("high".into())));
    assert_eq!(
        count(events(parser.parse_line(&line("high", "m2"), 1).unwrap())),
        0
    );
    assert!(
        events(parser.parse_line(&line("medium", "m3"), 2).unwrap())
            .contains(&ParseEvent::Effort("medium".into()))
    );
}
