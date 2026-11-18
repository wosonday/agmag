"""Regression tests for the fixture sanitizer's public CLI."""

import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest


class SanitizeFixtureTests(unittest.TestCase):
    def run_sanitizer(self, records):
        with tempfile.TemporaryDirectory() as directory:
            source = Path(directory) / "in.jsonl"
            target = Path(directory) / "out.jsonl"
            source.write_text(
                "".join(json.dumps(record) + "\n" for record in records),
                encoding="utf-8",
            )
            subprocess.run(
                [
                    sys.executable, "-I",
                    str(Path(__file__).with_name("sanitize_fixture.py")),
                    str(source), str(target), "--max-lines", "400",
                ],
                check=True,
            )
            return [json.loads(line) for line in target.read_text(encoding="utf-8").splitlines()]

    def test_private_event_types_discard_the_entire_line(self):
        for event_type in (
            "world_state", "api-request-blob", "api-request",
            "api-request-shape", "attachment", "session-owner",
            "custom-title", "agent-name", "last-prompt", "queue-operation",
        ):
            with self.subTest(event_type=event_type):
                self.assertEqual(
                    self.run_sanitizer([{"type": event_type}, {"type": "user"}]),
                    [{"type": "user"}],
                )

    def test_unknown_strings_are_redacted_and_usage_is_preserved(self):
        result = self.run_sanitizer([{
            "type": "user",
            "weird": {"deep": "someone@example.com"},
            "usage": {"input_tokens": 123},
        }])
        self.assertNotIn("@example.com", json.dumps(result))
        self.assertEqual(result, [{
            "type": "user",
            "weird": {"deep": "lorem"},
            "usage": {"input_tokens": 123},
        }])

    def test_recursive_values_and_direct_key_whitelist(self):
        metadata = {
            key: "kept"
            for key in (
                "type", "role", "model", "id", "sessionId", "session_id",
                "thread_id", "turn_id", "response_id", "timestamp", "version",
                "cli_version", "name", "limit_id", "stop_reason", "state",
                "status", "originator", "source", "model_provider", "uuid",
                "parentUuid",
            )
        }
        result = self.run_sanitizer([{
            "metadata": metadata,
            "cwd": r"C:\Users\private",
            "model": ["someone@example.com", {"deep": "someone@example.com"}],
            "content": {"model": "kept", "nested": ["private", 5, True, None]},
            "encrypted_content": "private",
            "base_instructions": "private",
            "creator_user_id": "private",
            "creator_account_id": "private",
            "accountUuid": "private",
            "organizationUuid": "private",
        }])
        self.assertEqual(result, [{
            "metadata": metadata,
            "cwd": r"D:\fixture\project",
            "model": ["lorem", {"deep": "lorem"}],
            "content": {"model": "kept", "nested": ["lorem", 5, True, None]},
        }])


if __name__ == "__main__":
    unittest.main()
