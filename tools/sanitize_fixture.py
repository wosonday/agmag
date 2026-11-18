"""Create bounded, sanitized JSONL fixtures without modifying source logs."""

import argparse
import json
from pathlib import Path


PRIVATE_KEYS = {
    "encrypted_content", "base_instructions", "creator_user_id",
    "creator_account_id", "accountUuid", "organizationUuid",
}
STRING_KEYS = {
    "type", "role", "model", "id", "sessionId", "session_id", "thread_id",
    "turn_id", "response_id", "timestamp", "version", "cli_version", "name",
    "limit_id", "stop_reason", "state", "status", "originator", "source",
    "model_provider", "uuid", "parentUuid",
}
DISCARDED_TYPES = {
    "world_state", "api-request-blob", "api-request", "api-request-shape",
    "attachment", "session-owner", "custom-title", "agent-name",
    "last-prompt", "queue-operation",
}


def discard_line(value):
    if isinstance(value, dict):
        return (
            "world_state" in value
            or (isinstance(value.get("type"), str)
                and value["type"] in DISCARDED_TYPES)
            or any(v == "api-request-blob" for v in value.values()
                   if isinstance(v, str))
            or any(discard_line(v) for v in value.values())
        )
    if isinstance(value, list):
        return any(discard_line(v) for v in value)
    return False


def sanitize(value, key=None):
    if isinstance(value, dict):
        return {
            key: sanitize(item, key)
            for key, item in value.items()
            if key not in PRIVATE_KEYS
        }
    if isinstance(value, list):
        return [sanitize(item) for item in value]
    if isinstance(value, str):
        if key == "cwd":
            return r"D:\fixture\project"
        return value if key in STRING_KEYS else "lorem"
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("input", type=Path)
    parser.add_argument("output", type=Path)
    parser.add_argument("--max-lines", type=int, default=400)
    args = parser.parse_args()
    if args.max_lines < 0:
        parser.error("--max-lines must be nonnegative")
    if args.input.resolve() == args.output.resolve():
        parser.error("input and output must differ")
    args.output.parent.mkdir(parents=True, exist_ok=True)
    with args.input.open(encoding="utf-8-sig") as source:
        with args.output.open("w", encoding="utf-8", newline="\n") as target:
            written = 0
            for line in source:
                if written >= args.max_lines:
                    break
                if not line.strip():
                    continue
                record = json.loads(line)
                if discard_line(record):
                    continue
                target.write(json.dumps(sanitize(record), ensure_ascii=False) + "\n")
                written += 1


if __name__ == "__main__":
    main()
