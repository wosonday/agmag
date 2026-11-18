#[test]
fn sanitizer_removes_private_data_and_preserves_structure() {
    let dir = tempfile::tempdir().unwrap();
    let input = dir.path().join("in.jsonl");
    let output = dir.path().join("out.jsonl");
    std::fs::write(&input, concat!(
        "{\"type\":\"event\",\"usage\":{\"output\":42},\"content\":[{\"text\":\"secret\",\"n\":2}],\"arguments\":\"secret\",\"nested\":{\"accountUuid\":\"secret\",\"creator_account_id\":\"secret\",\"encrypted_content\":\"secret\",\"base_instructions\":\"secret\",\"creator_user_id\":\"secret\",\"organizationUuid\":\"secret\"}}\n",
        "{\"payload\":{\"world_state\":{}}}\n",
        "{\"type\":\"api-request-blob\",\"data\":\"secret\"}\n",
        "{\"text\":\"secret\",\"input\":{\"a\":\"secret\"},\"output\":[\"secret\",12]}\n",
        "{\"text\":\"not included\"}\n",
    )).unwrap();
    let status = std::process::Command::new("python")
        .arg("-I")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tools/sanitize_fixture.py"
        ))
        .arg(&input)
        .arg(&output)
        .args(["--max-lines", "2"])
        .status()
        .unwrap();
    assert!(status.success());
    let text = std::fs::read_to_string(output).unwrap();
    let rows: Vec<serde_json::Value> = text
        .lines()
        .map(|line| serde_json::from_str(line).unwrap())
        .collect();
    assert_eq!(
        rows,
        vec![
            serde_json::json!({"type":"event","usage":{"output":42},"content":[{"text":"lorem","n":2}],"arguments":"lorem","nested":{}}),
            serde_json::json!({"text":"lorem","input":{"a":"lorem"},"output":["lorem",12]}),
        ]
    );
    assert!(!text.contains("secret"));
}

use agmag::model::{Usage, parse_rfc3339_ms, summarize};

#[test]
fn usage_total_and_add() {
    let mut usage = Usage {
        input_uncached: 1,
        cache_read: 2,
        cache_write: 3,
        output: 4,
        reasoning: 0,
    };
    assert_eq!(usage.total(), 10);
    usage.add(&Usage {
        input_uncached: 10,
        cache_read: 20,
        cache_write: 30,
        output: 40,
        reasoning: 5,
    });
    assert_eq!(
        usage,
        Usage {
            input_uncached: 11,
            cache_read: 22,
            cache_write: 33,
            output: 44,
            reasoning: 5,
        }
    );
    assert_eq!(usage.total(), 110);
}

#[test]
fn summarize_truncates_by_chars() {
    let result = summarize(&"ệ".repeat(300));
    assert_eq!(result.chars().count(), 200);
    assert!(result.ends_with('…'));
    assert_eq!(summarize("a \n  b"), "a b");
    assert_eq!(summarize(&"ệ".repeat(200)), "ệ".repeat(200));
    assert_eq!(summarize(" \t\n"), "");
}

#[test]
fn parse_rfc3339() {
    assert_eq!(
        parse_rfc3339_ms("2026-11-18T05:27:51.441Z"),
        Some(1794979671441)
    );
    assert_eq!(parse_rfc3339_ms("x"), None);
    assert_eq!(parse_rfc3339_ms("1970-01-01T00:00:00Z"), Some(0));
    assert_eq!(parse_rfc3339_ms("1969-12-31T23:59:59.999Z"), Some(-1));
    assert_eq!(parse_rfc3339_ms("2000-02-29T00:00:00Z"), Some(951782400000));
    for invalid in [
        "2026-02-29T00:00:00Z",
        "2026-04-31T00:00:00Z",
        "2026-00-01T00:00:00Z",
        "2026-01-01T24:00:00Z",
        "2026-01-01T00:60:00Z",
        "2026-01-01T00:00:60Z",
        "2026-01-01T00:00:00.Z",
        "2026-01-01T00:00:00+00:00",
        "ệệệệ-01-01T00:00:00Z",
    ] {
        assert_eq!(parse_rfc3339_ms(invalid), None, "{invalid}");
    }
}
