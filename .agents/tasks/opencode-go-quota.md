---
id: opencode-go-quota
tier: mid
skills: test-driven-development
branch: feat/opencode-go-quota
worktree: D:/Project/agmag.wt/opencode-go-quota
depends_on: fix/codex-quota-stale
---

# Hiển thị hạn mức OpenCode Go (5h / tuần / tháng) trong `agm quota` và `agm tui`

## Hiện trạng (Claude CEO kiểm 2026-11-20)

- Nhánh này tách từ `fix/codex-quota-stale` (`9324932`, đang review) để dùng lại: đánh dấu stale, `agm quota --refresh`, khung poller (`src/codex_quota.rs`: `parse_usage`, `record_usage`, `fetch_usage` có hạn chót chung + `kill_tree`, `CodexQuotaPoller`), cấu hình `[policy].codex_quota_poll_min`.
- Nguồn đã khảo sát và chốt: `.agents/decisions/opencode-go-quota.md` — `opencode api GET /api/experimental/usage/limits`, 0 token, ~0,8 s.
- CEO chạy thật (Git Bash cần `MSYS_NO_PATHCONV=1`; từ Rust `Command` không bị đổi đường dẫn):
  `{"data":{"workspace":{…},"go":{"rolling":{"percent":0,"limited":false,"resetsAt":1795…},"weekly":{"percent":40,…},"monthly":{"percent":20,…}},"budgets":[]}}` — `percent` là phần **đã dùng**.

## Quyết định của CEO

- Provider `opencode-go`. Cửa sổ: `rolling` → **300**, `weekly` → **10080**, `monthly` → **43200** (30 ngày, chỉ làm nhãn/khóa ngân sách; thời điểm reset lấy đúng từ `resetsAt`). Không đổi schema.
- TUI/bảng hiển thị cửa sổ 43200 là `mo` (không phải `30d`); 300 → `5h`, 10080 → `7d` như cũ.

## Yêu cầu

1. Module mới `src/opencode_quota.rs` theo đúng khuôn `src/codex_quota.rs`:
   - `parse_usage(stdout, now_ms)`: đọc `data.go.{rolling,weekly,monthly}`; `used_percent = percent` (kẹp 0–100), `resets_at_s = resetsAt / 1000`. `data: null` (chưa đăng nhập), thiếu `go`, meter lạ hoặc thiếu trường → bỏ qua phần đó, đếm unknown, **không ghi 0% hay 100% thay dữ liệu thiếu**.
   - `fetch_usage()`: chạy `opencode api GET /api/experimental/usage/limits` (cho phép đổi binary bằng `AGM_OPENCODE_BIN` để test; dùng lại cách tìm binary của `src/opencode_models.rs` nếu có), **một hạn chót chung** cho mọi bước, quá hạn thì `kill_tree` và không `join` luồng đọc. Không đọc `auth.json`, không đọc bảng `credential`/`account`/`control_account`, không mở trình duyệt.
   - `OpenCodeQuotaPoller` + `[policy].opencode_quota_poll_min` (mặc định 15; 0 = tắt), gắn vào daemon cạnh agy/codex; `agm quota --refresh` làm tươi cả `opencode-go`.
2. Hiển thị: `agm quota` (bảng + `--json`) và TUI mục PLAN QUOTA có 3 dòng `opencode-go` (`5h`, `7d`, `mo`), dùng chung logic stale/màu/`resets in` hiện có. Khi `limited: true` thì thêm nhãn `limited` (đỏ). Kiểm 60 và 110 cột không tràn.
3. Stale cho `opencode-go`: theo tuổi (`quota_stale_min`) **và** có usage mới cùng gói sau `observed_ms` — gói `opencode-go` ⇐ phiên OpenCode có model tiền tố `opencode-go/`. Không đánh dấu stale chỉ vì usage của các model `opencode-go/*-free` nếu API cho thấy chúng không tính vào Go — nếu không chắc, coi mọi `opencode-go/*` là cùng gói và ghi rõ trong decision.
4. Ngân sách: `agm budget set provider:opencode-go:300 --soft 80 --hard 95` hoạt động (thêm test); dữ liệu stale xử lý như không có dữ liệu, giống Codex.
5. Cập nhật `.agents/decisions/opencode-go-quota.md` (trạng thái: đã cài, quy ước tháng 43200), `README.md`, `README_vi.md`, `CHANGELOG.md`, `HERDR-CORE-BOT.md` (bỏ câu "chưa có Go trong `agm quota`", ghi rằng `agm quota` đã đọc được Go; Go Console vẫn là nơi xem Extra Usage/budget).

## Phạm vi tệp

Được sửa: tệp mới `src/opencode_quota.rs`, `src/lib.rs`, `src/main.rs`, `src/config.rs`, `src/dashboard.rs`, `src/report.rs`, `src/policy.rs`, `src/ops.rs`, `src/opencode_models.rs` (chỉ để dùng chung cách tìm binary/`kill_tree`, không đổi hành vi), `tests/**`, `README.md`, `README_vi.md`, `CHANGELOG.md`, `HERDR-CORE-BOT.md`, `.agents/decisions/opencode-go-quota.md`, `.agents/log/opencode-go-quota.md`.
Không sửa: `src/store/**` schema, `Cargo.toml`. Không fmt cả cây — chỉ `cargo fmt` tệp bạn sửa.

## Tiêu chí nghiệm thu

- Test parse: 3 meter đúng cửa sổ/dấu; `data: null` → không ghi; meter lạ → unknown; `percent` ngoài 0–100 → kẹp; JSON hỏng → lỗi, không ghi.
- Test treo: binary giả giữ stdout → `fetch_usage` trả lỗi trong **hạn chót + ít nhất 3 s dư** (vd hạn chót 5 s, assert ≤ 8 s) — không đặt ngưỡng test bằng đúng hạn chót.
- Test hiển thị: `5h`/`7d`/`mo`, nhãn `limited`, stale, ở 60 và 110 cột.
- Test ngân sách `provider:opencode-go:300`.
- `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- Chạy thật với `AGM_HOME` tạm (không ghi `~/.agmag` thật): `cargo run --offline -- quota --refresh` hiện 3 dòng `opencode-go` khớp `opencode api …/usage/limits`; ghi lệnh + kết quả vào log.
- **Commit sau mỗi bước** (tiền tố `opencode-go-quota:`), ghi `.agents/log/opencode-go-quota.md`.

## Báo cáo

Xong: `agm send user --type "BÁO CÁO" --task opencode-go-quota "Xong: <SHA>"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task opencode-go-quota "<câu hỏi ngắn>"`.
