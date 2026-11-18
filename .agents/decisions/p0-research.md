# P0 — Kết quả xác minh (2026-11-18)

agy (`--mode plan -p`) chỉ trả lời câu mở đầu rồi dừng (chờ duyệt quyền công cụ); lần chạy kèm tự duyệt quyền bị chế độ auto của Claude Code chặn. Mọi mục dưới đây do Claude điều phối tự kiểm trên máy hoặc từ tài liệu chính thức.

## (1) OpenCode v2 — [ĐÃ KIỂM]
- Lưu tại `%USERPROFILE%\.local\share\opencode\opencode.db` (SQLite, WAL, migration drizzle).
- `session_v2`: `id, project_id, parent_id, directory, title, version, agent, model, cost, tokens_input, tokens_output, tokens_reasoning, tokens_cache_read, tokens_cache_write, time_created, time_updated, time_idle, time_busy, …` — **tổng theo phiên có sẵn**.
- `session_message(id, session_id, type, seq, time_created, time_updated, data JSON)`. `type` ∈ {assistant, user, idle, synthetic, agent-switched, model-switched, location-switched, system}.
- `assistant.data`: `model{id, providerID, variant}`, `tokens{input, output, reasoning, cache{read, write}}`, `cost`, `native.responseId`, `time{created, firstToken, completed}`, `finish`.
- `input` **không gồm** cache (input 191, cache.read 45440) → chuẩn hóa như Claude: `input_uncached=input`, `cache_read=cache.read`, `cache_write=cache.write`, `output=output`, `reasoning=reasoning` (**[CHƯA]** reasoning có nằm trong output không — kiểm khi viết parser).
- **Phát hiện:** OpenCode đang dùng `openai/gpt-6.1-sol` → **tiêu chung hạn mức gói ChatGPT với Codex**. Hạn mức theo `providerID`, không theo CLI.
- **Cảnh báo bảo mật:** DB có bảng `credential`, `account`, `control_account` (token truy cập). Parser chỉ được đọc `session_v2`, `session_message`; mở `mode=ro`.
- Hệ quả Kế hoạch 5: parser OpenCode đọc SQLite (không phải jsonl); con trỏ = `max(time_updated)` hoặc `seq` theo phiên.

## (2) Claude Code — hạn mức gói và ngữ cảnh — [ĐÃ KIỂM: tài liệu chính thức https://code.claude.com/docs/en/statusline]
- JSON statusline (stdin) có: `session_id`, `transcript_path`, `context_window.context_window_size`, `context_window.used_percentage`, `rate_limits.five_hour.{used_percentage, resets_at}`, `rate_limits.seven_day.{used_percentage, resets_at}` (epoch giây). `rate_limits` chỉ có với gói Pro/Max sau phản hồi API đầu tiên; mỗi cửa sổ có thể vắng.
- `used_percentage` của ngữ cảnh = `input + cache_creation + cache_read` (không tính output) — khớp công thức trong kế hoạch.
- Máy hiện **chưa cấu hình statusLine** → `agm hook statusline` có thể cài thẳng, không cần bọc script cũ.
- Hệ quả: hạn mức Claude **đo chính xác**, không cần ước lượng; `transcript_path` cho phép ghép chắc chắn phiên ↔ tệp log; `context_window_size` thay cho cấu hình tay.

## (3) agy — [ĐÃ KIỂM]
- `~/.gemini/antigravity/conversations/` rỗng. `conversation_summaries.db` (SQLite) chỉ có tóm tắt: `conversation_id, title, preview, step_count, last_modified_time, workspace_uris, status, source, agent_name, …` — **không có token**.
- Hệ quả: token agy chỉ đo được khi gọi qua `agm ask` (`--output-format json` trả `usage{input_tokens, output_tokens, thinking_tokens, cache_read_tokens, total_tokens}` — đã thấy thật). Phiên agy tương tác: chỉ hiện số bước/trạng thái, token "không đo được".
- Ghi chú: `agy --mode plan -p` không chạy công cụ khi không có quyền tự duyệt → `agm ask` cần quyết định của người dùng về cờ quyền.

## (4) Ghép terminal ↔ phiên — [ĐÃ KIỂM]
- `hc` đặt trong mọi terminal: `HC_TERMINAL_ID` (vd `t1z`), `HC_TERMINAL_SOCKET` (`\\.\pipe\herdr-core-<user>-default.sock`), `HC_SCOPE`.
- Codex `session_meta` không lưu biến môi trường → ghép Codex vẫn theo cwd + thời điểm. Claude: dùng `transcript_path` từ hook statusline (chắc chắn) + `HC_TERMINAL_ID` đọc trong hook → **ghép chính xác terminal ↔ phiên Claude**.

## (6) Người gửi của `agm send` — [ĐÃ KIỂM]
- `terminal.send.caller`: "Who is sending (`HC_TERMINAL_ID`, else the calling process's parent)". → `agm send` đọc `HC_TERMINAL_ID`.
- "Người dùng ra lệnh": terminal gọi có `HC_TERMINAL_ID` nhưng `hc get` cho thấy không phải agent (`reported=false`, app là shell), hoặc không có `HC_TERMINAL_ID`; cộng TTY + xác nhận y/N.

## (5) Sự kiện của hc — [ĐÃ KIỂM: bắt 5 phút, 1297 dòng, 2026-11-18]
- Dòng đầu: ảnh chụp `{channel, seq, terminals:[Terminal đầy đủ]}`. Sau đó `{"jsonrpc":"2.0","method":"event","params":{type, terminal, seq, data}}`; `{"method":"heartbeat","params":{seq}}` mỗi 15 s.
- Loại đã thấy: `created`, `finished`, `status`, `turn`, `title`, `labels`, `size`, `foreground`, `doc`.
- `status.data`: `{reported, status:{app, state, msg?, kind?}, records:{<id>:{app, state, title}}}`.
  - Bị chặn: `state:"blocked"`, `kind:"question"` và `msg` = **nguyên văn câu hỏi + các lựa chọn** (Claude Code); `kind:"permission"` (Codex).
  - `records`: Claude Code báo từng tác vụ nền của nó (id, title, state).
- `turn.data`: `{state, turn}` — `turn` tăng mỗi lần rời `working`.
- `foreground.data`: `{argv, cwd, name, pid}` — **chương trình đang chạy thật** trong terminal (vd `codex.EXE`), dùng để biết app khi chưa báo trạng thái và để ghép theo pid.
- `finished.data`: `{exit_code, finished_ms, reason ("killed"…), signal}`.
- `labels` có `where` (vd `"MATHY-AI / Codex"`) và `herdr.space`/`herdr.tab` → **terminal của HERDR app hiện trong core** (khác ghi chú cũ §4.3).
- App tự báo 7501 đã thấy: `claude-code`, `codex`, `antigravity` (và `opencode` qua `hc ls`).
- Hệ quả Kế hoạch 2: hộp thư người dùng lấy trực tiếp từ `status` blocked (`kind`, `msg`); app của terminal lấy từ `status.app` hoặc `foreground.name`.
