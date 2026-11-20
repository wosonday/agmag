# Review: codex-quota-stale

Reviewer độc lập. Không sửa mã sản phẩm.
Đối tượng: nhánh `fix/codex-quota-stale`, HEAD `9324932` (diff so với `main`).

## Tiến độ kiểm tra

- [x] Đọc tệp tác vụ + log + git diff
- [x] (1) Quy tắc stale theo tuổi + usage mới cùng gói
- [x] (2) policy::provider_percent coi stale như không có dữ liệu
- [x] (3) src/codex_quota.rs (fetch, timeout, kill tree, lỗi, giả 0/100%, auth.json)
- [ ] (4) Test treo FETCH_TIMEOUT=8s vs elapsed <= 8
- [ ] (5) Bảng chữ 'agm quota' có hiện stale
- [ ] (6) TUI 60/110 cột
- [ ] cargo test --offline

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

## KẾT LUẬN

CHƯA ĐẠT (đang hoàn thiện các mục 4/5/6).
