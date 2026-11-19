# Hướng dẫn sử dụng chi tiết cho từng model

**Tiếng Việt** · [English](en/README.md)

Thư mục này cụ thể hóa cách khởi chạy, phạm vi quyền, lệnh `agm`/`hc` và mẹo tiết kiệm hạn mức cho từng model/CLI trong bộ tài liệu [AGENTS-TUTORIAL.md](../AGENTS-TUTORIAL.md), [BOT-CEO.md](../BOT-CEO.md) và [HERDR-CORE-BOT.md](../HERDR-CORE-BOT.md).

## Mục lục

| Tài liệu | Model / Công cụ | Vai trò chính trong đội |
|---|---|---|
| [CLAUDE-ARCHITECT-CEO.md](CLAUDE-ARCHITECT-CEO.md) | Claude Code (`claude-opus-5-5`, `claude-fable-5-2`) | Kiến trúc sư / BOT-CEO theo từng lane, leo thang rủi ro cao |
| [CODEX-ENGINEERING.md](CODEX-ENGINEERING.md) | Codex CLI / `openai/gpt-6.1-sol` | Engineering khó, sửa lỗi sau 2 lần fail, review chéo ca phức tạp, quản lý ở dự án 1 luồng |
| [AGY-GEMINI-OPUS.md](AGY-GEMINI-OPUS.md) | Antigravity CLI `agy` (3.8 Flash, Hydrogen Low/Medium, Argon, Opus High) | Research nhanh (`3.8-flash-low`), docs/report nhẹ (`3.8-flash-medium` / `hydrogen-low`), cố vấn & harness (`argon`), Đồng Kiến trúc sư & Reviewer cấp cao (`Opus High`) |
| [OPENCODE-WORKERS-REVIEWER.md](OPENCODE-WORKERS-REVIEWER.md) | OpenCode v2 (`@opencode/cli`: `:free`, Ollama, OpenRouter, Ollama Cloud) | Audit, `explore`/tóm tắt log, `gitnexus-sync`, thợ cơ khí (`fast` mở rộng nhờ ESLint + `mid/strong`), Reviewer mặc định (non-OpenAI) |
| [COPILOT-GIT.md](COPILOT-GIT.md) | GitHub Copilot CLI (`copilot`) | Quản lý phiên bản Git: đọc `BRANCHES.md`, merge đúng SHA, kiểm lại sau merge, revert, dọn worktree/nhánh |
| [ESLINT-CI-GUARDRAILS.md](ESLINT-CI-GUARDRAILS.md) | ESLint (`--max-warnings 0`) + Git Diff Check + GitHub Actions CI | **Hàng rào bắt buộc cho mọi model:** chống `any`, `@ts-ignore`, `eslint-disable`, hàm phình to, quên `await`; chặn ngay ở Audit/CI để yên tâm giao việc cho model nhẹ |

## Quy tắc điều phối hạn mức nhanh qua `agm`

Trước khi giao đợt việc nặng, kiểm tra nhanh trong `agm tui` hoặc CLI:

```powershell
agm quota                     # xem % hạn mức 5h / 7d của Claude và Codex
agm cost --by source          # xem tỷ lệ token đang dồn vào claude / codex / opencode / agy
```

| Tình huống | Hành động điều chỉnh tải |
|---|---|
| `codex` 7d > 70–80% | Giữ `openai/gpt-6.1-sol` cho ca engineering thực sự khó; chuyển Reviewer mặc định sang OpenRouter/Ollama Cloud hoặc `agy` (`Argon/Hydrogen High`), và chuyển `worker-strong` sang OpenRouter |
| `claude` 5h/7d tăng nhanh | Mở phiên tương tác `agy` Opus High trong repo (`--mode plan --sandbox`, giữ `conversation_id` để ăn prompt cache) gánh thay bước bàn thiết kế / chia việc và phân xử review ca khó; không mở Claude cho Reviewer thường |
| Endpoint `:free` gặp `429` | Đổi sang slug `:free` dự phòng khác hoặc Ollama cục bộ (`num_ctx ≥ 64k`) |
