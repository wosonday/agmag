# Nhật ký codex-quota-stale

## 2026-11-20 — Hoàn tất "Phần còn lại"

Nhánh: `fix/codex-quota-stale` (dựng lại trên `main` 1465336, commit nền `f09f974`). Không push/merge/rebase, không fmt cả cây, không ghi `~/.agmag` thật.

### 1. Sửa clippy `sort_by_key` (`tests/codex_quota_test.rs`)
- TDD/kiểm tra: `cargo clippy --offline --all-targets -- -D warnings` FAIL (`clippy::unnecessary_sort_by` tại `tests/codex_quota_test.rs:28`) → sửa `sort_by(|a,b| a.0.cmp(&b.0))` thành `sort_by_key(|w| w.0)` → clippy PASS (sạch).
- Commit: `3ea9dcf` `codex-quota-stale: fix clippy sort_by_key in codex_quota_test`

### 2. Quyết định nguồn hạn mức Codex
- Đã viết `.agents/decisions/codex-quota-source.md` (theo mẫu `agy-quota-usage.md`): phương thức/lệnh chính xác (`codex app-server --stdio`, JSON-RPC `initialize` → `account/rateLimits/read`, `codex-cli 0.170.0` kiểm 2026-11-20 13:20 +07), xác nhận 0 token/không tạo rollout mới (`~/.codex/sessions` không có thư mục `2026/11/20`, rollout mới nhất `2026/11/19`), JSON mẫu đã xóa định danh, ánh xạ cửa sổ 300/10080 phút, xử lý lỗi (không ghi 0%/100% khi lỗi/timeout).
- Commit: `95e8485` `codex-quota-stale: document codex app-server quota source`

### 3. `src/codex_quota.rs`: hạn chót chung + diệt cây tiến trình
- TDD: thêm test `fetch_usage_hanging_binary_times_out_and_kills_tree` (fake `.cmd` treo giữ stdout, `AGM_CODEX_BIN`) yêu cầu lỗi trong ≤ 8 s. Lần chạy đầu (RED): FAIL, thực tế ~10.02s với thông báo `codex app-server quá thời hạn chờ phản hồi id=1` (mỗi lần chờ 10s riêng, không kill tree khi thoát sớm).
- Sửa: `FETCH_TIMEOUT = 8s` dùng chung cho toàn bộ lần đọc; `fetch_usage`/`fetch_usage_inner` dùng một `deadline`, `wait_for_id` chờ theo `deadline` chung; lỗi/timeout → `kill_tree` (Windows `taskkill /T /F` + `kill`/`wait`), không join luồng đọc khi quá hạn (cùng lớp `opencode_models.rs::load_with`). Không đổi logic parse/ghi.
- Kiểm tra (GREEN): `cargo test --offline --test codex_quota_test` → 5 passed, 0 failed (gồm test treo mới).
- Commit: `a5cf995` `codex-quota-stale: add common deadline and kill_tree for codex quota fetch`

### 4. Tài liệu
- Cập nhật `README.md`, `README_vi.md`: `agm quota --refresh` làm tươi agy và Codex; nêu nguồn `codex app-server`/poll daemon; bổ sung `[policy].codex_quota_poll_min` và `[policy].quota_stale_min`; ghi chú dữ liệu stale được đánh dấu trong TUI và `--json`.
- Cập nhật `CHANGELOG.md` (`[Unreleased]`): thêm mục nguồn tươi Codex + config; thêm mục sửa lỗi đánh dấu `stale` (JSON `stale`/`observed_age_s`, TUI xám, policy coi stale như không có dữ liệu).
- Commit: `35711b9` `codex-quota-stale: document stale quota and codex refresh/policy settings`

### 5. Kiểm tra tổng hợp
- `cargo clippy --offline --all-targets -- -D warnings`: sạch.
- `cargo test --offline`: toàn bộ test đạt, 0 failed (bao gồm `codex_quota_test` 5/5 và các test stale/quota/policy/dashboard hiện có).
- Không chạy thử ghi vào `~/.agmag` thật (yêu cầu dùng `AGM_HOME` tạm khi test); số liệu thực tế đã được CEO xác nhận trước đó trong brief (`quota --refresh` → codex 7d `used 92%`, `stale: false`).

### Ghi chú lệch/giới hạn
- CLI bảng `agm quota` (text) chưa in nhãn `stale` riêng; trạng thái stale được phản ánh trong `--json` (`stale`, `observed_age_s`) và trong TUI (`stale · seen … ago`, xám) theo triển khai hiện có. Tài liệu đã mô tả đúng phạm vi hiển thị này.
- Mục 3 bổ sung `kill_tree` + deadline chung 8s cho `fetch_usage`; không thay đổi hành vi parse hay điều kiện ghi (vẫn không ghi 0%/100% khi lỗi).
