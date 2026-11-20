---
id: codex-quota-stale
tier: mid
skills: test-driven-development, systematic-debugging
branch: fix/codex-quota-stale
worktree: D:/Project/agmag.wt/codex-quota-stale
---

# Hạn mức Codex trong `agm` bị cũ: TUI ghi 84% còn lại, thực tế khoảng 2x%

## Hiện trạng (Claude CEO kiểm ngày 2026-11-20 ~11:40 +07)

- `agm quota --json` cho `codex`: 5h `left 100%` (`reset_passed: true`), 7d `used 16% / left 84%`, **`observed_ms` = 2026-11-19 06:30 +07** — cũ hơn một ngày. Người dùng thấy Codex `/status` còn khoảng 2x% tuần.
- Nguồn duy nhất của quota Codex là sự kiện `token_count.rate_limits` trong log Codex CLI (`src/parse/codex.rs:82`, `~/.codex/sessions/YYYY/MM/DD/rollout-*.jsonl`). Từ sáng 19/11 không còn phiên Codex CLI nào có `rate_limits` (tệp mới nhất `rollout-2026-11-19T19-53-01-….jsonl` chỉ có 3 dòng, 0 `rate_limits`; không có thư mục `2026/11/20`).
- Trong khi đó gói Codex/ChatGPT vẫn bị tiêu rất nhiều **qua OpenCode** (`openai/gpt-6.1-sol` ≈ 124 triệu token, `openai/gpt-6.2-sol`, `openai/gpt-6.2-astra`). OpenCode không ghi `rate_limits` nào mà `agm` đọc được, nên con số 84% đứng yên.
- `src/dashboard.rs` (Section::Quota, ~dòng 755–790) chỉ hiện `% left` + `resets in …`; **không có dấu hiệu dữ liệu cũ**. `policy::provider_percent` cũng dùng bản ghi này cho ngân sách `provider:codex:*`.

## Yêu cầu

1. **Đánh dấu dữ liệu cũ (bắt buộc).** Một dòng quota là *stale* khi (a) `observed_ms` cũ hơn `[policy].quota_stale_min` (mặc định 60 phút; thêm vào `Config`, có test parse), **hoặc** (b) sau `observed_ms` đã có usage mới thuộc cùng gói. Ánh xạ gói ↔ usage: `codex` ⇐ phiên nguồn Codex **và** phiên OpenCode có model tiền tố `openai/` hoặc model khai `provider = "codex"` trong `providers.toml`; `claude` ⇐ phiên Claude Code và OpenCode `anthropic/`. Không có ánh xạ → chỉ dùng điều kiện (a).
   - `agm quota` (bảng + `--json` thêm `stale: bool`, `observed_age_s`), `agm tui`: dòng stale hiện `stale · seen 1d05h ago` màu xám, **không** tô màu xanh như số tươi.
   - `policy::provider_percent`: dữ liệu stale xử lý **như không có dữ liệu** (đúng nhánh "unknown" hiện có); ghi rõ trong test. Không tự chặn hay tự nới.
2. **Khảo sát nguồn tươi cho Codex — 0 token.** Thử, theo thứ tự: `codex app-server` (JSON-RPC qua stdio; tìm method đọc rate limits của tài khoản, ví dụ dạng `account/rateLimits/read` — kiểm bằng schema/`--help` của chính bản `codex-cli 0.170.0` trên máy, không đoán), `codex debug …`, rồi các lệnh khác không mở lượt hội thoại. Ghi `.agents/decisions/codex-quota-source.md` theo mẫu `.agents/decisions/agy-quota-usage.md` (lệnh chính xác, thời gian, số token, JSON đã xóa định danh, kết luận).
   - Nếu có nguồn 0 token đáng tin: cài như `src/agy_quota.rs` — module `src/codex_quota.rs`, daemon đọc theo `[policy].codex_quota_poll_min` (mặc định 15), `agm quota --refresh` đọc ngay, ghi vào bảng `quota` provider `codex` với cửa sổ 300/10080. Lỗi/không đăng nhập/timeout → không ghi gì, cảnh báo một lần, **không bao giờ** ghi 0% hay 100% thay dữ liệu thiếu.
   - Nếu không có: chỉ làm mục 1 + tài liệu quyết định.
   - **Cấm:** đọc `~/.codex/auth.json` hay token, tự gọi HTTP tới backend ChatGPT/OpenAI, mở phiên `codex exec` có prompt (tốn token).
3. Cập nhật `README.md`, `README_vi.md` (mục quota/TUI), `CHANGELOG.md`.

## Phạm vi tệp

Được sửa: `src/dashboard.rs`, `src/ops.rs` (lệnh `quota`), `src/policy.rs`, `src/config.rs`, `src/daemon.rs`, `src/main.rs`, `src/lib.rs`, tệp mới `src/codex_quota.rs`, `tests/**`, `README.md`, `README_vi.md`, `CHANGELOG.md`, `.agents/log/codex-quota-stale.md`, `.agents/decisions/codex-quota-source.md`. Truy vấn đọc trong `src/store/**` được phép nếu cần; **không đổi schema** — cần thì dừng và hỏi.
Lưu ý song song: `opencode-metrics` cũng sửa `src/policy.rs`, `src/config.rs`, `src/main.rs`, `src/lib.rs`; `spawn-fixes` sửa `src/main.rs`. Giữ thay đổi ở các tệp này nhỏ, gom thành hàm riêng để dễ gộp.

## Tiêu chí nghiệm thu

- Test: quota cũ hơn ngưỡng → stale; quota mới nhưng có usage OpenCode `openai/*` sau `observed_ms` → stale; quota mới, không usage sau đó → không stale; provider không có ánh xạ chỉ theo tuổi.
- Test: `provider_percent` trả "unknown" với dữ liệu stale; hồi quy ngân sách với dữ liệu tươi không đổi.
- Test hiển thị TUI ở 60 và 110 cột: dòng stale có `stale`, không tràn khung.
- Nếu cài nguồn tươi: test parse JSON mẫu (fixture tự tạo), lỗi → không ghi.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- Chạy thật: `cargo run -- quota` trên máy → dòng `codex` hiện stale (hoặc số tươi nếu cài được nguồn); ghi lệnh + kết quả vào `.agents/log/codex-quota-stale.md`.

## Báo cáo

Xong: `agm send t4v --type "BÁO CÁO" --task codex-quota-stale "Xong: <SHA cuối>, xem .agents/log/codex-quota-stale.md"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task codex-quota-stale "<câu hỏi ngắn>"`.

## Phần còn lại (CEO, 2026-11-20 sau khi hai thợ free chạm giới hạn)

Trạng thái: nhánh đã được dựng lại trên `main` `1465336` chỉ với phần sửa thật (`f09f974`; bản cũ giữ ở `fix/codex-quota-stale-wip`). `cargo test --offline` 289 đạt. Đã có: đánh dấu stale (`report.rs`, `policy.rs`, `dashboard.rs`), `src/codex_quota.rs` (`codex app-server` → `account/rateLimits/read`), poller trong daemon, `agm quota --refresh`. CEO chạy thật `AGM_HOME=<tạm> cargo run -- quota --refresh` → `codex` 7d `used 92%`, `stale: false` (khớp `/status`).

Việc còn lại:
1. Sửa lỗi clippy `consider using sort_by_key` trong `tests/codex_quota_test.rs`; `cargo clippy --offline --all-targets -- -D warnings` sạch.
2. Viết `.agents/decisions/codex-quota-source.md` theo mẫu `agy-quota-usage.md`: lệnh/phương thức chính xác, thời gian, xác nhận 0 token (không tạo rollout mới trong `~/.codex/sessions`), JSON mẫu đã xóa định danh, ánh xạ cửa sổ, xử lý lỗi.
3. Rà lại `src/codex_quota.rs`: mọi đường chờ (`app-server` khởi động, đọc phản hồi, thoát) có hạn chót chung và diệt cả cây tiến trình khi quá hạn — **cùng lớp lỗi đã chặn `opencode-metrics`** (xem `src/opencode_models.rs::load_with`, `kill_tree`). Thêm test fake binary treo giữ stdout → trả lỗi trong ≤ 8 s.
4. README.md, README_vi.md (quota/TUI: stale, `--refresh` cho Codex, `[policy].quota_stale_min`, `codex_quota_poll_min`), CHANGELOG.md.
5. Ghi `.agents/log/codex-quota-stale.md`; **commit sau mỗi mục** (tiền tố `codex-quota-stale:`).

Không fmt cả cây (đã fmt); chỉ `cargo fmt` các tệp bạn sửa. Không ghi vào `~/.agmag` thật khi chạy thử: dùng `AGM_HOME` tạm.
Báo cáo: `agm send user --type "BÁO CÁO" --task codex-quota-stale "Xong: <SHA>"`.
