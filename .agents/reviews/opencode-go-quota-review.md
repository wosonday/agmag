# Review độc lập — opencode-go-quota

- Nhánh: `feat/opencode-go-quota`
- Task: `.agents/tasks/opencode-go-quota.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (1 commit squash, đã giải xung đột với `fix/codex-quota-stale` ở `src/main.rs quota_table` và `README`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện và commit riêng (tiền tố `opencode-go-quota-review:`). Không sửa mã sản phẩm.

## Tiến độ kiểm

- [x] (1) Parse: percent = phần đã dùng; 300/10080/43200; kẹp 0–100; null/thiếu/meter lạ → không ghi, không giả 0%/100%
- [x] (2) Fetch: hạn chót chung mọi bước, kill cả cây, không join luồng đọc khi quá hạn, test treo có biên dư
- [x] (3) Bảng + TUI: nhãn 5h/7d/mo, limited đỏ, stale, dòng stale không hiện như số tươi, 60/110 cột
- [ ] (4) Stale opencode-go theo usage `opencode-go/*`
- [ ] (5) Ngân sách `provider:opencode-go:300`, stale = không có dữ liệu
- [ ] (6) Không có định danh tài khoản (org_..., tên workspace, usageURL) trong tệp diff
- [ ] (7) Docs/decision khớp mã
- [ ] Chạy `cargo test --offline`

## Phát hiện

### (1) Parse — ĐẠT (1 phát hiện nhỏ)

Đối chiếu `src/opencode_quota.rs:31-97` với task/decision:

- `src/opencode_quota.rs:74` — `used_percent = percent.clamp(0.0, 100.0)`: lưu nguyên phần **đã dùng**, không đảo. Khớp decision `...:10,30` và task `:17,27`.
- `src/opencode_quota.rs:50` — ánh xạ đúng `rolling→300`, `weekly→10080`, `monthly→43200`; `:78` `resets_at_s = resetsAt / 1000`.
- `src/opencode_quota.rs:36-38` (`data: null`), `:39-45` (thiếu `go`), `:52-56` (meter lạ), `:61-73` (meter null / thiếu `percent`/`resetsAt`/`limited`) → đều `continue` hoặc trả default, **không tạo snapshot giả**. `src/opencode_quota.rs:32-33` — JSON hỏng trả `Err`. `tests/opencode_quota_test.rs:16-99` phủ cả 5 nhóm; `cargo test --offline` xanh.

**Phát hiện 1.1 — mức: nhỏ.** `src/opencode_quota.rs:36-38`: nhánh `data.is_null()` trả `UsageParse::default()` nên `unknown = 0`, trong khi decision `.agents/decisions/opencode-go-quota.md:31` ghi `data: null` → "đếm unknown". Hệ quả: `agm quota --refresh` (`src/main.rs:533-536`) không in cảnh báo "skipped … meter(s)" khi chưa đăng nhập. Không có ghi 0%/100% giả, nên không chặn.
- Cách tái hiện: `parse_usage(r#"{"data":null}"#, 1)` → `p.unknown == 0`, `p.snapshots.is_empty() == true`. Kỳ vọng theo decision: `unknown == 1`.

### (2) Fetch / timeout / kill — ĐẠT (1 nên sửa, 1 nhỏ)

- **Hạn chót chung mọi bước — đạt.** `src/opencode_quota.rs:117` tạo `deadline` **một lần** trong `fetch_usage`, truyền vào `fetch_with` (`:135`); cùng `deadline` dùng cho `recv_timeout` (`:152-153`) và vòng `try_wait` (`:165-177`), kể cả khi thử binary dự phòng `opencode.cmd` (`:126-131`). Không có hạn chót riêng từng bước.
- **Không join luồng đọc khi quá hạn — đạt.** Nhánh timeout `:159-163` và `:168-171` gọi `kill_tree` rồi `bail!`, **không** `reader.join()`; `join()` chỉ ở đường thành công `:179` (khi stdout đã đóng). Lỗi đọc `:155-158` cũng kill và trả về ngay.
- **Test treo có biên dư — đạt.** `tests/opencode_quota_test.rs:107-149`: binary giả giữ stdout 999 s; hạn chót 5 s (`src/opencode_quota.rs:113`), assert `elapsed.as_secs() <= 8` (`:141`) = dư 3 s, không đặt ngưỡng bằng đúng hạn chót. Đã tự chạy: test `fetch_usage_hanging_binary_times_out_and_kills_tree` pass trong ~5 s.

**Phát hiện 2.1 — mức: nên sửa.** `src/opencode_quota.rs:187-199`: `kill_tree` chỉ diệt **cả cây** trên Windows (`#[cfg(windows)]` + `taskkill /T /F /PID`, `:188-196`). Trên Linux/macOS (code vẫn có nhánh `#[cfg(not(windows))]` ở `:122-124` và test `:118-128`) chỉ `child.kill() + child.wait()` (`:197-198`) → tiến trình cháu (vd `sleep 999` trong script giả) tiếp tục giữ pipe stdout; luồng đọc bị bỏ rơi (không join nên không treo agm, nhưng process/tài nguyên bị rò rỉ). Không chặn trên môi trường Windows của dự án, nhưng không đúng yêu cầu "kill cả cây" đa nền tảng.
- Cách tái hiện (trên Linux/macOS): tạo script `fake-opencode` có nội dung `#!/bin/sh\nsleep 999\n`, đặt `AGM_OPENCODE_BIN` tới script, gọi `fetch_usage()`; sau khi hàm trả lỗi ~5 s, `pgrep -f 'sleep 999'` vẫn thấy tiến trình cháu. Trên Windows dùng script `.cmd` như test thì `taskkill /T` diệt sạch.

**Phát hiện 2.2 — mức: nhỏ.** `src/opencode_quota.rs:162` và `:170`: thông báo lỗi ghi **"quá thời hạn sau 8 s"** trong khi hạn chót thực tế là 5 s (`:113`) và decision `.agents/decisions/opencode-go-quota.md:36` cũng ghi 5 s. Số 8 trong thông báo trùng với ngưỡng assert của test, dễ gây hiểu nhầm là hạn chót 8 s.
- Cách tái hiện: chạy `cargo test --offline --test opencode_quota_test fetch_usage_hanging_binary_times_out_and_kills_tree -- --nocapture`; lỗi in ra "... quá thời hạn sau 8 s" dù hàm trả về sau ~5 s.

### (3) Bảng `agm quota` + TUI — ĐẠT (1 phát hiện nhỏ)

- **Nhãn 5h/7d/mo — đạt.** CLI `src/main.rs:889-899` (`43200 → "mo"` đứng trước nhánh `is_multiple_of(1440)`, `10080 → "7d"`, `300 → "5h"`); TUI `src/dashboard.rs:824-831` cùng quy ước. Test `tests/dashboard_test.rs:933-997` kiểm cả 3 nhãn cho `opencode-go`.
- **limited đỏ — đạt.** CLI `src/main.rs:929-940` dùng `\x1b[91m`; TUI `src/dashboard.rs:844-848` dùng `BRIGHT_RED` (`\x1b[91m`). Test `tests/dashboard_test.rs:984-987` assert có mã đỏ.
- **Stale không hiện như số tươi — đạt.** CLI `src/main.rs:911-922`: thay bar/`%` bằng `··········` + `stale · seen <age> ago`; TUI `src/dashboard.rs:849-857`: empty bar + chữ xám `GRAY`, không in `% left`. Test `src/main.rs:975-997` và `tests/dashboard_test.rs:403-450,984-993` khẳng định không có `78%`/`% left` trên dòng stale.
- **60/110 cột — đạt.** `tests/dashboard_test.rs:933-997` render TUI ở 60 và 110, assert mọi dòng `line.chars().count() <= w`; `:403-450` kiểm riêng dòng stale ở 60/110; `:367-399` kiểm command bar. Không tràn.

**Phát hiện 3.1 — mức: nhỏ.** `src/main.rs:911-922`: nhánh stale của bảng CLI `continue` **trước** khi tính `limited` (`:929`), nên một dòng vừa `stale` vừa `limited: true` **không** hiện nhãn `limited` đỏ trên `agm quota`. TUI (`src/dashboard.rs:852`) vẫn chèn `limited_tag` vào cả dòng stale → hai chế độ hiển thị không nhất quán, và yêu cầu "khi `limited: true` thì thêm nhãn `limited`" không được thỏa mãn đầy đủ ở CLI. Dữ liệu stale đã bị coi như "không có số tươi", nên không chặn.
- Cách tái hiện: gieo `QuotaRow { provider: "opencode-go", window_minutes: 300, stale: true, limited: true, ... }`, chạy `agm quota` (không `--json`) → dòng đó chỉ có `stale · seen … ago`; chạy `agm tui` → cùng dòng có thêm `limited` đỏ. Có thể gọi hàm `quota_table` trong unit test của `src/main.rs` để so sánh chuỗi.

## KẾT LUẬN

_(chưa xác định)_
