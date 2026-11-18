"""Gửi mã agmag cho Gemini Argon (qua agy) review theo từng nhóm mô-đun.

agy chạy ở chế độ chỉ đọc (--mode plan --sandbox) trong thư mục tạm, không có quyền công cụ:
mã được nhét thẳng vào prompt, nên mỗi nhóm phải < ~28 000 ký tự (giới hạn dòng lệnh Windows 32 767).

Dùng:  python -I tools/argon_review.py [--model gemini-4-argon-high] [--out docs/reviews/argon.md] [--only G1,G3]
"""
import argparse
import concurrent.futures as cf
import json
import os
import subprocess
import sys
import tempfile
from pathlib import Path

ROOT = Path(__file__).resolve().parent.parent

CONTEXT = """Bạn là reviewer Rust cấp cao. Dự án agmag (binary `agm`, Windows 11) là công cụ gắn kèm herdr/herdr-core:
- đọc log Codex/Claude (jsonl) và OpenCode (opencode.db, chỉ đọc) để đo token, hạn mức gói tháng (5h/7d), % ngữ cảnh;
- theo dõi terminal của herdr-core qua `hc events --json`, ghi CSDL SQLite (WAL) dùng chung giữa daemon và CLI;
- hộp thư người dùng (agent bị chặn, câu hỏi), gửi/trả lời tin giữa agent có ghi lại, sinh agent có hạn mức;
- ngân sách (task: token; provider: % hạn mức), cảnh báo ngữ cảnh 50%, xoay vòng thợ OpenCode ở 70%
  (yêu cầu bàn giao → kiểm commit chạm .agents/log/<ID>.md + worktree sạch → kill → spawn lại cùng template/model).
Quy tắc cứng của người dùng:
1. Codex và Claude chỉ bị kill/ngắt khi NGƯỜI DÙNG ra lệnh (TTY + không phải agent + xác nhận). Agent chỉ được kill thợ khác (OpenCode).
2. Không bao giờ ghi vào thư mục log của các CLI; không đọc bảng credential/account của opencode.db.
3. Không số liệu giả: thiếu giá/cửa sổ → None, không phải 0.
4. Hành động tự động tắt được bằng `policy off`; không kill khi bàn giao thất bại.
Hãy review đoạn mã dưới đây. KHÔNG dùng công cụ nào, chỉ đọc mã trong prompt.
Báo lỗi THẬT (đúng/sai, an toàn, mất dữ liệu, race, Windows) theo mức Critical / Important / Minor,
mỗi lỗi: tệp:dòng-gần-đúng, kịch bản cụ thể gây lỗi, cách sửa ngắn. Bỏ qua góp ý phong cách.
Nếu không thấy lỗi đáng kể, nói rõ. Trả lời bằng tiếng Việt, tối đa 500 từ.
"""

GROUPS = {
    "G1 an toàn (actions, policy)": ["src/actions.rs", "src/policy.rs"],
    "G2 xoay vòng (rotation, git, hc)": ["src/rotation.rs", "src/git.rs", "src/hc.rs"],
    "G3 sự kiện & ghép (daemon, correlate, hook, ask)": ["src/daemon.rs", "src/correlate.rs", "src/hook.rs", "src/ask.rs"],
    "G4 đọc log (opencode, ingest)": ["src/opencode.rs", "src/ingest.rs", "src/parse/mod.rs"],
    "G5 CSDL v2 (store/v2, schema)": ["src/store/v2.rs", "src/store/schema_v2.sql"],
    "G6 lệnh CLI (ops, tui)": ["src/ops.rs", "src/tui.rs"],
    "G7 báo cáo (dashboard, report)": ["src/dashboard.rs", "src/report.rs"],
    "G8 main": ["src/main.rs"],
}
LIMIT = 28_000


def build_prompt(files):
    parts = [CONTEXT]
    for f in files:
        text = (ROOT / f).read_text(encoding="utf-8")
        numbered = "\n".join(f"{i:4} {line}" for i, line in enumerate(text.splitlines(), 1))
        parts.append(f"\n===== {f} =====\n{numbered}")
    prompt = "".join(parts)
    if len(prompt) > LIMIT:
        # Bỏ số dòng nếu quá dài.
        prompt = CONTEXT + "".join(f"\n===== {f} =====\n" + (ROOT / f).read_text(encoding="utf-8") for f in files)
    return prompt


def review(name, files, model, timeout):
    prompt = build_prompt(files)
    if len(prompt) > 32_000:
        return name, f"(bỏ qua: prompt {len(prompt)} ký tự vượt giới hạn dòng lệnh)", None
    with tempfile.TemporaryDirectory() as tmp:
        cmd = ["agy", "--model", model, "--mode", "plan", "--sandbox", "--print-timeout", f"{timeout}s",
               "--output-format", "json", "-p", prompt]
        try:
            out = subprocess.run(cmd, cwd=tmp, capture_output=True, timeout=timeout + 60)
        except subprocess.TimeoutExpired:
            return name, "(hết thời gian)", None
    try:
        data = json.loads(out.stdout.decode("utf-8", "replace").strip())
    except json.JSONDecodeError:
        return name, "(đầu ra không phải JSON) " + out.stdout.decode("utf-8", "replace")[:300], None
    return name, data.get("response", "") if data.get("status") == "SUCCESS" else f"(lỗi agy: {data})", data.get("usage")


def main():
    ap = argparse.ArgumentParser()
    ap.add_argument("--model", default="gemini-4-argon-high")
    ap.add_argument("--out", default="docs/reviews/argon-v1.md")
    ap.add_argument("--only", default="")
    ap.add_argument("--timeout", type=int, default=420)
    ap.add_argument("--jobs", type=int, default=3)
    a = ap.parse_args()
    groups = {k: v for k, v in GROUPS.items() if not a.only or k.split()[0] in a.only.split(",")}
    results = {}
    with cf.ThreadPoolExecutor(max_workers=a.jobs) as ex:
        futs = {ex.submit(review, k, v, a.model, a.timeout): k for k, v in groups.items()}
        for f in cf.as_completed(futs):
            name, text, usage = f.result()
            results[name] = (text, usage)
            print(f"xong: {name} ({usage})", file=sys.stderr)
    out = ROOT / a.out
    out.parent.mkdir(parents=True, exist_ok=True)
    lines = [f"# Review của {a.model} (agy) — agmag v1\n"]
    for name in groups:
        text, usage = results[name]
        lines.append(f"\n## {name}\n\nTệp: {', '.join(groups[name])} · token: {usage}\n\n{text}\n")
    out.write_text("".join(lines), encoding="utf-8")
    print(out)


if __name__ == "__main__":
    main()
