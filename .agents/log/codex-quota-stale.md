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

## 2026-11-20 — Vòng sửa 1 (theo `.agents/reviews/codex-quota-stale-review.md`)

Nhánh `fix/codex-quota-stale`. Không push/merge/rebase, không fmt cả cây (chỉ rustfmt các tệp sửa), không ghi `~/.agmag` thật khi chạy thử (mọi lần chạy dùng `AGM_HOME` tạm).

### 1. Bảng chữ `agm quota` hiện stale
- TDD RED: thêm test `quota_table_marks_stale_rows_without_fresh_bar` trong `src/main.rs` (hàm `quota_table` chưa tồn tại) → `cargo test --offline --bin agm` FAIL (`E0425: cannot find function quota_table`).
- GREEN: tách hàm thuần `quota_table(rows, now_s) -> Table`; hàng stale in `··········` + `stale · seen <tuổi> ago` thay vì thanh `█/░` và `%`, dùng lại `dashboard::format_age_s` (đổi thành `pub`) nên cùng định dạng `1d01h` với TUI; `print_quota` chỉ còn `println!`. Hàng tươi giữ nguyên thanh/%.
- Test: `cargo test --offline --bin agm` → 5 passed, 0 failed. `cargo clippy --offline --bin agm -- -D warnings` sạch.
- Commit: `a131af0` `codex-quota-stale: show stale label in agm quota table`.

### 2. Test treo có biên an toàn
- TDD RED: `tests/codex_quota_test.rs` đổi sang so sánh `Duration` với `FETCH_TIMEOUT` công khai (`elapsed >= FETCH_TIMEOUT - 1s`, `elapsed <= FETCH_TIMEOUT + 4s`) → FAIL (`E0603: constant FETCH_TIMEOUT is private`).
- GREEN: công khai `pub const FETCH_TIMEOUT: Duration = Duration::from_secs(8)` trong `src/codex_quota.rs` (hằng số vẫn 8s). Không đổi logic fetch/kill tree.
- Test: `cargo test --offline --test codex_quota_test` → 5 passed, 0 failed (tổng binary ~8.44s, nằm trong biên 7–12s). `cargo clippy --offline --all-targets -- -D warnings` sạch.
- Commit: `7a7fdce` `codex-quota-stale: give timeout test Duration safety bounds`.

### 3. Tài liệu + nghiệm thu
- `README.md` / `README_vi.md`: ví dụ bảng `agm quota` có dòng stale; mục lệnh ghi rõ stale được đánh dấu trong bảng, TUI và `--json`. `CHANGELOG.md`: bổ sung ghi chú bảng chữ không vẽ thanh/% tươi cho dòng stale.
- `cargo test --offline`: **291 passed / 0 failed** (29 binary/doc-test; tăng 1 so với 290 trước vòng sửa). `cargo clippy --offline --all-targets -- -D warnings`: sạch. `cargo fmt --check`: sạch.
- Chạy thật (stdout bị pipe nên CLI tự in JSON; dòng `stale` có trong JSON, bảng được phủ bởi unit test ở mục 1):
  ```
  $tmp = 'C:\Users\thanh\AppData\Local\Temp\opencode\agmag-round1-quota'
  # AGM_HOME=$tmp\home, AGM_CODEX_DIR=$tmp\codex (fixture mini.jsonl 2026-11-18), các dir khác trong $tmp
  cargo run --offline -- quota
  → codex 300m: "stale": true, "observed_age_s": 199580
    codex 10080m: "left_percent": 21.0, "stale": true, "observed_age_s": 199580
    EXIT=0
  ```
- Commit: tài liệu + mục nhật ký này.

### Ghi chú vòng sửa 1
- Lệch nhẹ so với tệp tác vụ "Test CLI": test bảng nằm ở unit test của `quota_table` trong `src/main.rs`, không spawn binary trong TTY giả. Lý do: `src/main.rs:279` ép JSON khi stdout không phải terminal (mọi harness `Command::output()` đều là pipe), và repo không có thư viện pty; test ở hàm render thuần vẫn kiểm đúng hai trường hợp stale/tươi mà không thêm phụ thuộc hay cờ CLI mới.
- Không đụng các phát hiện mức `nhỏ` còn lại của review (`snap_val` dead code, spawn/taskkill ngoài deadline, `quota_stale_min = 0`, `budget_ls` dùng builtin config) vì nằm ngoài phạm vi "Vòng sửa 1" đã được CEO đồng ý.
