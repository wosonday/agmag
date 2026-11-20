# Review: codex-quota-stale

Reviewer độc lập. Không sửa mã sản phẩm.
Đối tượng: nhánh `fix/codex-quota-stale`, HEAD `9324932` (diff so với `main`).

## Tiến độ kiểm tra

- [x] Đọc tệp tác vụ + log + git diff
- [x] (1) Quy tắc stale theo tuổi + usage mới cùng gói
- [x] (2) policy::provider_percent coi stale như không có dữ liệu
- [x] (3) src/codex_quota.rs (fetch, timeout, kill tree, lỗi, giả 0/100%, auth.json)
- [x] (4) Test treo FETCH_TIMEOUT=8s vs elapsed <= 8
- [x] (5) Bảng chữ 'agm quota' có hiện stale
- [x] (6) TUI 60/110 cột
- [x] cargo test --offline

## Phát hiện — mục (1) quy tắc stale

**Đạt yêu cầu chính.** `src/report.rs:728` `quota_is_stale` đúng cả hai điều kiện:
- (a) `src/report.rs:735` so `now_ms - observed_ms > quota_stale_min*60_000`;
- (b) `src/report.rs:746-777` `package_has_usage_after` ánh xạ đúng: `codex` ⇐ `source=="codex"` hoặc OpenCode `openai/*` / `provider=="codex"`; `claude` ⇐ `source=="claude"` hoặc OpenCode `anthropic/*` / `provider=="claude"`; provider khác (agy) chỉ theo tuổi. Khớp tệp tác vụ dòng 20.

**Không báo stale nhầm ngay sau `--refresh`:** `src/main.rs:519-527` dùng cùng một biến `now` cho cả `record_usage(..., now)` và `quota_with_config(..., now/1000)`, nên `observed_ms == now`, `observed_age_s == 0`, không có usage nào có `ts_ms > now` trong điều kiện (b). Đã kiểm bằng mã, không cần chạy thật.

| file:dòng | mức | phát hiện / cách tái hiện |
|---|---|---|
| `src/report.rs:735` | nhỏ | `.max(1)` biến `quota_stale_min = 0` thành **1 phút**, nhưng `src/config.rs:128` ghi chú `0 → mặc định 60`. Tái hiện: đặt `[policy] quota_stale_min = 0`, dữ liệu 2 phút tuổi vẫn bị đánh stale dù comment hứa ngưỡng 60'. Không ảnh hưởng giá trị mặc định 60. Nên đổi comment hoặc xử lý 0 về 60. |

## Phát hiện — mục (2) policy::provider_percent

**Đạt yêu cầu.** `src/policy.rs:32-57` query thêm `ts_ms`, gọi `quota_is_stale` và trả `Ok(None)` ngay khi stale → đúng nhánh "unknown" của `budgets_tick` (`src/policy.rs:161`), không tự chặn/nới. Test `tests/policy_test.rs:197-263` phủ cả 3 case (cũ theo tuổi, usage `openai/*` mới, tươi giữ nguyên) + hồi quy reset (`tests/policy_test.rs:177-188`).

| file:dòng | mức | phát hiện / cách tái hiện |
|---|---|---|
| `src/ops.rs:551`, `src/policy.rs:83` | nhỏ | `budget_ls` / `spawn_blocked_by_budget` gọi `budget_value` với `Config::builtin()` thay vì config đã tải → ngưỡng `quota_stale_min` tùy chỉnh của người dùng không được tôn trọng ở hai đường này. Tái hiện: config `quota_stale_min = 10080`, quota codex 61' tuổi; `agm budget ls --json` vẫn trả `value: null` (dùng ngưỡng 60 của builtin) thay vì giá trị. `spawn_blocked_by_budget` chỉ dùng scope `task:` nên vô hại; `budget_ls` mới là chỗ lệch. |

## Phát hiện — mục (3) src/codex_quota.rs

**Đạt yêu cầu chính.** Đã kiểm từng ý:
- Phương thức đúng `account/rateLimits/read` (`src/codex_quota.rs` lệnh `write` thứ hai); 0 token/không mở hội thoại (chỉ JSON-RPC đọc); tài liệu `.agents/decisions/codex-quota-source.md` ghi rõ lệnh/thời gian/0 token.
- Hạn chót chung: `src/codex_quota.rs:103` `FETCH_TIMEOUT=8s`, `deadline` tạo tại `:107` trước spawn, truyền qua `fetch_usage_inner`/`wait_for_id` cho cả `initialize` lẫn `account/rateLimits/read`.
- Kill cả cây: `kill_tree` dùng `taskkill /T /F /PID` trên Windows, rồi `child.kill()`/`wait()`; không join luồng đọc khi quá hạn.
- Lỗi thì không ghi gì: `record_usage` gọi `parse_usage` trước vòng lặp `insert_quota` (`src/codex_quota.rs:~88`); `--refresh` (`src/main.rs:519-524`) và poller (`tick`, `res.and_then(...)`) chỉ ghi khi `Ok`. Test `tests/codex_quota_test.rs:bad_input_records_nothing` khẳng định `COUNT(*)=0`.
- Không ghi 0%/100% giả: `parse_usage` chỉ nhận cửa sổ 300/10080, bỏ bucket lạ/ngoài `0..=100`, `bail!` nếu không có cửa sổ hợp lệ; không bao giờ chèn giá trị mặc định.
- Không đọc `auth.json`: đã grep `auth.json|auth_token|access_token` trong `src/codex_quota.rs` → không có kết quả.

| file:dòng | mức | phát hiện / cách tái hiện |
|---|---|---|
| `src/codex_quota.rs:~108` (spawn), `:~205` (`taskkill ... .status()`) | nhỏ | Hai bước này nằm ngoài `deadline`: `Command::spawn()` và lời gọi `taskkill .status()` đồng bộ không có hạn chót. Trong điều kiện thường thì trả về ngay, nhưng về mặt lý thuyết vẫn là "đường chờ không có hạn chót chung". Tái hiện: thay `AGM_CODEX_BIN` bằng wrapper chặn ngay trong lúc spawn (khó dựng); chỉ là ghi chú độ bền, không chặn. |
| `src/codex_quota.rs:40-50` | nhỏ | Biến `snap_val` được tính rồi `let _ = snap_val;` (dead code), trong khi `snap` mới là giá trị dùng. Gây nhầm khi đọc; clippy hiện không bắt. Nên xóa để rõ ràng. |

## Phát hiện — mục (4) test treo / timeout

| file:dòng | mức | phát hiện / cách tái hiện |
|---|---|---|
| `tests/codex_quota_test.rs:124`; `src/codex_quota.rs:103` | nên sửa | `FETCH_TIMEOUT = 8s`, test assert `elapsed.as_secs() <= 8` — tức chỉ chấp nhận `elapsed < 9.0s` nhờ phép làm tròn xuống của `as_secs()`. Thực đo: cả binary `codex_quota_test` chạy hết **8.33s** (test treo chiếm ~8.3s), tức `as_secs()==8` → vừa đậu, biên an toàn chỉ ~0.7s. `fetch_usage` hết hạn ở 8s rồi mới gọi `kill_tree` (`taskkill /T /F` khởi tạo tiến trình + `wait()`), nên trên máy tải cao / CI chậm, tổng thời gian có thể vượt 9.0s → `as_secs() >= 9` → test **chập chờn (flaky)**. **Cách tái hiện:** chạy lặp `cargo test --offline --test codex_quota_test` dưới tải CPU cao, hoặc giả lập độ trễ `taskkill`; quan sát elapsed tiệm 9s. **Đề xuất:** để biên rõ ràng — assert `elapsed < FETCH_TIMEOUT + Duration::from_secs(2)` (hoặc giảm `FETCH_TIMEOUT` xuống 5s rồi vẫn giữ ngưỡng 8s của tệp tác vụ). |

Ghi chú thêm: requirements tệp tác vụ (dòng 55) nói "trả lỗi trong ≤ 8 s", nhưng assert hiện thực lại cho phép tới gần 9s; nên thống nhất lại con số.

## Phát hiện — mục (5) bảng chữ `agm quota` (không `--json`)

| file:dòng | mức | phát hiện / cách tái hiện |
|---|---|---|
| `src/main.rs:884-913` (`print_quota`), đặc biệt dòng `900-910`; yêu cầu tại `.agents/tasks/codex-quota-stale.md:21` | **nên sửa** (lệch yêu cầu rõ ràng) | Hàm `print_quota` dựng bảng từ `QuotaRow` nhưng **không đọc `r.stale` cũng không đọc `r.observed_age_s`** — vẫn in thanh `█/░` + `{:.0}%` + reset như cũ. Vì vậy chạy `agm quota` trong terminal (không `--json`) với dữ liệu cũ vẫn hiện con số "tươi" màu xanh, **không có nhãn `stale` / `seen … ago`**. Tệp tác vụ dòng 21 yêu cầu `agm quota` (bảng + `--json`) thêm stale; chỉ có `--json` (`src/main.rs:526-528` → serialize `QuotaRow`) và TUI là có. `.agents/log/codex-quota-stale.md:32` cũng đã tự ghi nhận đây là lệch. **Cách tái hiện:** dùng fixture 2 ngày tuổi (`tests/fixtures/codex/mini.jsonl`), chạy trong TTY thật: `AGM_HOME=<tạm> target/debug/agm quota` → bảng hiện `21%` không có chữ `stale`; còn `agm quota --json` thì có `"stale": true`. (Lưu ý: `src/main.rs:279` ép JSON khi stdout không phải terminal, nên chạy trong TTY hoặc gọi trực tiếp `print_quota` để thấy bảng.) Đề xuất: thêm cột/nhãn `stale · seen … ago` vào `print_quota` giống TUI. |

→ Đây là điểm **lệch yêu cầu duy nhất ở mức nên sửa**; không phải lỗi logic stale.

## Phát hiện — mục (6) TUI 60/110 cột

**Đạt.** `src/dashboard.rs:840-849` in dòng stale với `stale · seen {age} ago` tô `GRAY` (`\x1b[90m`), không vẽ thanh/% tươi; độ dài ~51 ký tự nên vừa khung 60. Test `tests/dashboard_test.rs:403-451` (`render_stale_quota_line_fits_60_and_110`) kiểm cả 60 lẫn 110: có chữ `stale · seen 1d01h ago`, không có `% left`, có mã màu xám, và mọi dòng đều `< width`. Đã chạy test → đậu (xem mục test bên dưới). Không có phát hiện.

## cargo test --offline

Lệnh: `cargo test --offline` (chạy 2 lần; lần 2 gom tóm tắt).

**Kết quả: 290 test đạt, 0 lỗi, 0 ignored, 0 filtered** (29 binary/doc-test, mọi dòng `test result: ok`). Trong đó `tests/codex_quota_test` = 5/5 (đã chạy 8.33s, khớp nhận định mục 4), `tests/dashboard_test`, `tests/report_test`, `tests/policy_test`, `tests/config_test`, `tests/cli_test` đều đậu. Không ghi vào `~/.agmag` thật (các test CLI dùng `AGM_HOME` tạm; lần thử ngoài cũng dùng thư mục temp `%TEMP%\agmag-review-quota2`).

## KẾT LUẬN

CHƯA ĐẠT (đang hoàn thiện phần cuối).
