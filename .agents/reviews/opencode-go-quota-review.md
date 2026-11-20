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
- [x] (4) Stale opencode-go theo usage `opencode-go/*`
- [x] (5) Ngân sách `provider:opencode-go:300`, stale = không có dữ liệu
- [x] (6) Không có định danh tài khoản (org_..., tên workspace, usageURL) trong tệp diff
- [x] (7) Docs/decision khớp mã
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

### (4) Stale `opencode-go` theo usage `opencode-go/*` — ĐẠT (1 phát hiện nhỏ)

- `src/report.rs:761` nhận diện provider `opencode-go`, `:762` đưa vào nhánh xét usage mới; `:795-798` — `source == "opencode" && model.starts_with("opencode-go/")`, tức **mọi** biến thể `opencode-go/*` (kể cả `*-free`) đều cùng gói, đúng quy ước dự phòng trong decision `.agents/decisions/opencode-go-quota.md:39` và task `:31`.
- Cùng dùngcondition tuổi `quota_stale_min` (`src/report.rs:745-748`) như các provider khác, và `quota_with_config` truyền `stale`/`observed_age_s` ra dòng hiển thị (`:728-729`).
- Test `tests/report_test.rs:699-788`: quota cũ 61 phút → stale; quota tươi nhưng có usage `opencode-go/gpt-5.4` sau `observed_ms` → stale; usage `openai/gpt-6.1-sol` (gói khác) → **không** stale. Đã chạy xanh.

**Phát hiện 4.1 — mức: nhỏ.** `tests/report_test.rs:737` chỉ dùng model `opencode-go/gpt-5.4`; chưa có ca test nào dùng biến thể `opencode-go/*-free` dù decision `:39` nêu rõ "mọi `opencode-go/*` được coi là cùng gói". Code dùng `starts_with` nên hành vi đúng, chỉ thiếu test bảo vệ quy ước này.
- Cách tái hiện: thêm/sửa ca trong `opencode_go_quota_stale_by_age_and_new_usage`, đổi model thành `opencode-go/deepseek-v4-pro-free` — vẫn phải ra `stale == true`; hiện chưa có ca này trong diff.

### (5) Ngân sách `provider:opencode-go:300` / stale = không có dữ liệu — ĐẠT (1 phát hiện nhỏ)

- `src/ops.rs:520-545`: `budget_set` chấp nhận mọi scope `provider:<tên>:<phút>` với tên không rỗng và phút parse được `u32` (`:523`) → `agm budget set provider:opencode-go:300 --soft 80 --hard 95` hợp lệ, `unit = "percent"`; không có whitelist provider. (Tệp `src/ops.rs` không đổi trong diff vì đã generic — phù hợp phạm vi task `:37`.)
- `src/policy.rs:67-75`: `budget_value` tách `["provider", p, w]` → `provider_percent`. `src/policy.rs:32-58`: truy `quota` theo `(provider, window)`; `:50-52` — nếu `quota_is_stale(...)` thì trả `None` (**không có dữ liệu**, không chặn/nới giả); `:53-57` trả phần đã dùng còn hiệu lực (sau reset → 0). Dùng chung hàm stale với bảng/TUI nên nhất quán với mục (3),(4).
- Test `tests/report_test.rs:790-824`: `provider_percent(..., "opencode-go", 300, now)` = `Some(84.0)` lúc tươi, `None` khi quota 61 phút (stale). `tests/policy_test.rs:170-188` phủ đường scope string `provider:codex:10080` qua `budget_value`. Đã chạy xanh.

**Phát hiện 5.1 — mức: nhỏ.** `tests/report_test.rs:804,818` gọi trực tiếp `provider_percent`, không có ca nào tạo `BudgetRow { scope: "provider:opencode-go:300", ... }` rồi gọi `budget_value` (đường mà CLI/TUI dùng). Logic `budget_value` đã generic và được test ở chỗ khác (`tests/policy_test.rs:170-188`), nên đây là khe hở bảo vệ, không phải lỗi mã.
- Cách tái hiện: thêm test gieo quota `opencode-go:300` tươi 84% + `set_budget("provider:opencode-go:300")`, assert `budget_value(...) == Some(84.0)`; lặp lại sau 61 phút assert `None`. Hiện ca này chưa có.

### (6) Định danh tài khoản trong diff — ĐẠT về bản chất (1 nên sửa, 1 nhỏ)

Kiểm `git diff main 2913a4d` (diff sản phẩm, trước khi tôi thêm tệp review) bằng regex `org_|usageURL|workspace|"name"`:

- `.agents/decisions/opencode-go-quota.md` — bản cũ chứa `workspace.id/name/usageURL` và URL Console đã bị **xoá** trong commit; bản mới (`:10-22`) chỉ còn `data.go.{rolling,weekly,monthly}` đã ẩn định danh. Task `.agents/tasks/opencode-go-quota.md:17` dùng chỗ giữ chỗ `workspace:{…}`. ✓
- **Không có** ID org thật, tên workspace thật hay URL thật nào trong diff; giá trị nhạy cảm đều là `<redacted>` hoặc giả. Mã sản phẩm (`src/**`, `tests/**`) không đọc `auth.json`/bảng credential theo `src/opencode_quota.rs:115-199`.

**Phát hiện 6.1 — mức: nên sửa.** `.agents/log/opencode-go-quota.md:144`: dòng "Reference" chép nguyên envelope API vào log đã commit, gồm đường dẫn local thật `C:\\Users\\thanh`, khoá `id: "org_<redacted>"`, `"name": "<redacted>"` và `"usageURL": "<redacted>"`. Giá trị đã redacted nên **không lộ định danh thật**, nhưng vẫn còn tiền tố `org_`, khoá `usageURL` và tên user máy — đúng vào các chuỗi mà yêu cầu (6) liệt kê. Nên thay toàn bộ envelope bằng `{"data":{"workspace":{…},"go":{…}}}` như task `:17` để sạch cả về mặt hình thức. Không chặn vì không có giá trị thật.
- Cách tái hiện: `git diff main 2913a4d | Select-String 'org_|usageURL|Users\\\\'` → trúng dòng `.agents/log/opencode-go-quota.md:144`.

**Phát hiện 6.2 — mức: nhỏ.** `tests/opencode_quota_test.rs:6`: fixture có `"id": "org_x", "name": "x"` — `org_x` là giá trị giả, trùng mẫu `org_...`. Parser bỏ qua `workspace` (`src/opencode_quota.rs:34-45`), nên không ảnh hưởng hành vi; chỉ là tàn dư hình thức.
- Cách tái hiện: `git diff main 2913a4d -- tests/opencode_quota_test.rs | Select-String 'org_'` → trúng dòng 6. Có thể bỏ hẳn trường `workspace` trong fixture vì `parse_usage` không đọc nó.

### (7) Docs / decision khớp mã — ĐẠT (1 phát hiện nhỏ)

| Decision/tài liệu | Mã | Khớp |
|---|---|---|
| `.agents/decisions/opencode-go-quota.md:3` "đã cài" | `src/opencode_quota.rs`, `src/main.rs:351-354,530-539`, `src/config.rs:134-136` | ✓ |
| Decision `:29` `rolling→300/5h`, `weekly→10080/7d`, `monthly→43200/mo` | `src/opencode_quota.rs:50`, `src/main.rs:889-899`, `src/dashboard.rs:824-831` | ✓ |
| Decision `:30` `clamp(0..100)`, `resetsAt/1000` | `src/opencode_quota.rs:74,78` | ✓ |
| Decision `:31` null/thiếu/lạ → skip, không số giả | `src/opencode_quota.rs:36-73` | ✓ (xem phát hiện 1.1) |
| Decision `:32` setting `quota_limited:opencode-go:<window>` + nhãn đỏ | `src/opencode_quota.rs:104-108`, `src/main.rs:929-940`, `src/dashboard.rs:844-848` | ✓ |
| Decision `:36` `AGM_OPENCODE_BIN`, `opencode`/`opencode.cmd`, hạn chót 5 s, kill_tree, không join | `src/opencode_quota.rs:113-199` | ✓ (xem phát hiện 2.1/2.2) |
| Decision `:37` `[policy].opencode_quota_poll_min` mặc định 15, 0=tắt, daemon + `--refresh` | `src/config.rs:134-136,152,165-167`, `src/main.rs:351-359,511-543`, `src/opencode_quota.rs:225-228` | ✓ |
| Decision `:39` mọi `opencode-go/*` cùng gói | `src/report.rs:795-798` | ✓ |
| Decision `:40` ngân sách provider, stale = không có dữ liệu | `src/policy.rs:50-52,67-75` | ✓ |
| `README.md:99,211`, `README_vi.md:97,207`, `CHANGELOG.md:29`, `HERDR-CORE-BOT.md:27,102` — quota đọc Go, `--refresh` có Go, Go Console xem Extra Usage/budget, bỏ câu "chưa có Go" | nội dung diff | ✓ |

**Phát hiện 7.1 — mức: nhỏ.** `README.md:99` ghi "Go plan quota (5h/7d/**30d**)", trong khi task `:22` và decision `:29` chốt nhãn cửa sổ 43200 là **`mo`, không phải `30d`**. `README_vi.md:97` dùng "30 ngày" (mang tính mô tả, chấp nhận được). Đây là sai lệch tài liệu, không ảnh hưởng hiển thị thực tế (`src/main.rs:890-891` trả `mo`).
- Cách tái hiện: so sánh `README.md:99` với `src/main.rs:889-899` — chạy `agm quota --json` với dữ liệu `window_minutes: 43200` → dòng JSON có `window_minutes: 43200` và bảng in nhãn `mo`; README ghi `30d`. Nên sửa thành `5h/7d/mo (30 ngày)`.

## KẾT LUẬN

_(chưa xác định)_
