# Review độc lập: inbox-done-bulk

- Người duyệt: thợ cơ khí (reviewer độc lập); không sửa mã sản phẩm.
- Đối tượng: HEAD `686028efece835f193433bdbc61c95cb625b0349` (`686028e`), phạm vi `git diff main...HEAD` (nhánh đã merge main).
- Tài liệu: `.agents/tasks/inbox-done-bulk.md` (mục "Quyết định của CEO" là thiết kế bắt buộc) và `.agents/log/inbox-done-bulk.md`.
- Thang mức: **CHẶN** / **NÊN SỬA** / **NHỎ**. Mọi lần chạy thử dùng `AGM_HOME` tạm; không chạy `agm done` trên inbox thật.

## 1. Parser ID / khoảng / danh sách — CHẶN

- Đạt: `N`, `N-M`, `N,M-K` và nhiều đối số (`src/actions.rs:123-168`; test `tests/actions_test.rs:549-574`). Chạy tay với `AGM_HOME` tạm:
  - `done 9-3` → `agm: error: invalid id range: 9-3` (exit 1);
  - `done a` → `invalid id selector: "a"` (exit 1);
  - `done 1-` → `invalid id selector: ""` (exit 1);
  - `done ""` → `invalid id selector: empty` (exit 1);
  - `done 1-10002` → lỗi rõ `id range too large` (`src/actions.rs:140-143`, test `tests/actions_test.rs:582-583`).
- **CHẶN (F1)** — tràn số với số rất lớn: `src/actions.rs:140` `let count = end - start + 1;`. Với `start = 0`, `end = i64::MAX` (`9223372036854775807`) phép `+ 1` tràn i64. Chạy thử thực tế: `agm done 0-9223372036854775807 --json` → `thread 'main' panicked at src\actions.rs:140:25: attempt to add with overflow`, exit 101 (bản debug). Ở bản release phép cộng wrap sang số âm, vượt kiểm tra `count > 10_000` rồi vòng lặp `src/actions.rs:144-146` chạy ~9,2×10^18 lần (treo/DoS). Vi phạm trực tiếp yêu cầu "không tràn số/overflow với số rất lớn" và yêu cầu khoảng quá lớn phải "lỗi rõ". Gợi ý sửa: thay bằng `if end - start >= 10_000` (tương đương `count > 10_000` nhưng không cộng 1) hoặc dùng `checked_add`/`u128` trước khi sinh danh sách.
- ID đơn cực lớn xử lý đúng: `done 9223372036854775807` → lỗi `không có mục hộp thư #…`, `done 9223372036854775808` → `invalid id selector` (đều exit 1, không panic) — `src/actions.rs:114-121`.
- Test chưa phủ ca cực đại: `tests/actions_test.rs:577-584` mới chỉ có `1-10002`; nên thêm `0-9223372036854775807` vào danh sách `bad`.

## 2. An toàn: `source=blocked`, đường TUI, không gõ terminal — ĐẠT (1 nên sửa)

- Khoảng ID và bộ lọc không đóng mục `blocked`: `src/actions.rs:298-304` (đường có ID) và `src/actions.rs:312-318` (đường chỉ lọc). Chạy thử với `AGM_HOME` tạm và các dòng inbox cấy sẵn:
  - `done 1-5 --json` → `done: [1,2,4,5]`, `#3` nằm trong `skipped` với reason `blocked (use --source blocked)`;
  - `done --kind QUESTION --json` → mục `blocked` có kind khớp vẫn bị skip (`done: [1,3]`, skipped `#2`).
- Ngoại lệ cho phép đúng thiết kế: `--source blocked` (`explicit_source_blocked` ở `src/main.rs:476-478`, dùng ở `src/actions.rs:312`) đóng được blocked (`done --source blocked` → `done: [3,6]`); ID đơn của blocked cũng đóng được (`legacy_single` ở `src/main.rs:486`, bypass ở `src/actions.rs:298`) → `done 2` trả `{"done":2}`.
- Đường TUI: `src/dashboard.rs:1785-1856` nhận `kind:`/`term:`/`source:`(`src:`), đặt `explicit_source_blocked` khi `source:blocked` và chỉ đặt `legacy_single` cho ID đơn không kèm filter; mọi nhánh đều đi vào chung `actions::dismiss_done` (`src/tui.rs:412-444`) nên cùng cơ chế bảo vệ. `done` không gõ vào terminal nào: `src/actions.rs:247-325` và `src/tui.rs:412-444` chỉ thao tác `Store`, không gọi `Hc`.
- **NÊN SỬA (F4)** — lỗ hổng test: `tests/cli_test.rs:748-798` không tạo mục `blocked`; `tests/dashboard_test.rs:280-385` test `kind:`/`term:` nhưng không có ca `source:`/`src:` hay ca blocked bị skip. Quy tắc an toàn mới chỉ được khóa ở tầng actions (`tests/actions_test.rs:587-645` và `648+`). Nên thêm (a) test CLI range/filter có mục blocked ở giữa và (b) test `parse_command("done source:blocked")` để bảo vệ hợp đồng an toàn hai đường.

## 3. Tương thích `agm done <một ID>` — ĐẠT

- `src/main.rs:521-527` (nhánh `legacy_single`) emit đúng JSON `{"done": id}` và text `Dismissed #id.`; `src/actions.rs:328-333` giữ nguyên hàm `dismiss` và vẫn bail `không có mục hộp thư #id` khi ID không tồn tại.
- Chạy tay: `done 999999` → exit 1; `done <id có>` → `{"done": <id>}`; single ID trùng với mục blocked → vẫn đóng (đúng ngoại lệ thiết kế).
- Test `tests/cli_test.rs:748-790` xanh; hai test cũ `cli_done_dismisses_inbox_item` và `cli_help_is_english_for_every_command` vẫn xanh trong `cargo test --offline`.
- Ghi chú nhỏ (không đủ mức lỗi): `done 5 --kind REPORT` với `#5` không tồn tại đi vào đường bulk và trả `{"done":[],"skipped":[…]}` thay vì exit ≠ 0; tuy nhiên "hành vi cũ" chỉ áp cho `done <ID>` không kèm filter, nên chấp nhận.

## 4. `--kind` alias / AND / giao ID+bộ lọc — ĐẠT

- `src/actions.rs:197-206` gọi lại `kind_from_english` và `KINDS` sẵn có (`src/actions.rs:12-39`) — **không** chép bảng mới, đúng yêu cầu. `REPORT` và `BÁO CÁO` cùng chuẩn hóa về `BÁO CÁO`; test tương đương ở `tests/actions_test.rs:648+`.
- `done_item_matches_filters` (`src/actions.rs:216-241`) áp đồng thời kind/terminal/source → AND giữa các bộ lọc; đường có ID dùng chính hàm này (`src/actions.rs:287-297`) → ID và filter là giao. Test AND `terminal + kind` ở `tests/actions_test.rs:~675+`.
- `normalize_done_source` (`src/actions.rs:208-214`) chấp nhận `message|context|rotation|blocked`, không phân biệt hoa thường ASCII; source lạ → lỗi rõ từ `src/main.rs:499-503`.
- Nhận xét nhỏ (không phải lỗi theo spec): `eq_ignore_ascii_case` ở `src/actions.rs:204` và `src/actions.rs:221` không gập được dấu tiếng Việt, nên `báo cáo` viết thường không khớp `BÁO CÁO`. Yêu cầu chỉ bắt buộc "không phân biệt hoa thường với alias" (alias tiếng Anh), nên đạt.

## 5. Một transaction SQLite / `resolve_inbox` cũ / schema / daemon — ĐẠT

- `src/store/v2.rs:411-436`: `resolve_inbox_many` mở `SAVEPOINT agm_done_bulk`, thực hiện toàn bộ `UPDATE … WHERE resolved_ms IS NULL` rồi `RELEASE`; trên lỗi `ROLLBACK TO` + `RELEASE`. Tất cả đóng nằm trong **một** transaction; không ghi đè `reply`/`resolved_ms` cũ.
- `resolve_inbox` cũ giữ nguyên (`src/actions.rs:328-333` vẫn gọi `store.resolve_inbox`); diff `main...HEAD` không đổi schema SQL (`src/store/schema*.sql` không xuất hiện trong danh sách tệp) và không đụng daemon — đúng phạm vi thiết kế điểm 5 của CEO.
- `dismiss_done` (`src/actions.rs:323`) gọi `resolve_inbox_many` đúng một lần sau khi đã lọc xong danh sách `done`, kể cả khi danh sách rỗng (hàm trả về sớm, `src/store/v2.rs:412-414`).

## 6. Help tiếng Anh / README / CHANGELOG — NÊN SỬA

- Help: `src/main.rs:122-137` mô tả đúng cú pháp mới. Chạy `agm done --help` cho kết quả tiếng Anh, đủ `IDS...`, `--kind`, `--terminal`, `--source`; test `cli_help_is_english_for_every_command` xanh.
- CHANGELOG: `CHANGELOG.md:5-8` nằm đúng dưới `[Unreleased] → ### Thêm mới`, **không** nằm trong `[0.3.2]`; EOL LF (`git ls-files --eol` → `i/lf w/lf`).
- `README.md:144` (thanh `cmd:` TUI) và `README.md:221` (bảng lệnh CLI) cập nhật đúng cú pháp; `README_vi.md:217` (bảng lệnh) cũng đúng.
- **NÊN SỬA (F2)**: `README_vi.md:142` vẫn là `cmd: reply <#> <text> · done <#> · send …` trong khi bản Anh đã đổi thành `done <#|a-b|kind:K|term:T>`. Tệp tác vụ yêu cầu cập nhật dòng `done <#>` ở mục TUI trong **cả hai** README (`.agents/tasks/inbox-done-bulk.md:36`); phần này bị sót, chỉ mới sửa bảng lệnh.

## 7. Lệnh kiểm & EOL — ĐẠT (trừ tệp log)

- `cargo test --offline`: **325 passed, 0 failed** trên toàn workspace (riêng `actions_test` 23, `cli_test` 23, `dashboard_test` 25 — chạy lại riêng để chắc chắn).
- `cargo clippy --offline --all-targets -- -D warnings`: sạch, exit 0.
- `cargo fmt --check`: sạch, exit 0.
- `git ls-files --eol` trên 13 tệp trong `git diff main...HEAD`: 12 tệp `i/lf w/lf`; duy nhất `.agents/log/inbox-done-bulk.md` là `i/mixed w/mixed` → **NÊN SỬA (F3)**. Kiểm byte: tệp có 20 LF và 1 CRLF, CRLF nằm ở cuối dòng 11; yêu cầu review là "phải lf" cho các tệp đã sửa.

## 8. Log & các điểm lệch so với tệp tác vụ — NHỎ

- `.agents/log/inbox-done-bulk.md` **không có** mục "Lệch so với tệp tác vụ" (toàn tệp 20 dòng, kết thúc ở note dòng 20) dù brief reviewer kỳ vọng có mục này và yêu cầu "đánh giá từng điểm lệch". Các lệch thực tế tìm được (F2 README_vi, F3 EOL log) vì thế không được ghi nhận trong log. **NHỎ (F6)** — nên bổ sung mục này khi sửa log.
- **NHỎ (F5)**: ký tự điều khiển lạ trong log (đọc byte UTF-8): TAB `0x09` tại dòng 15 (`kind:, <TAB>erm:`) và dòng 17 (`<TAB>ests/dashboard_test.rs`, `<TAB>ests/cli_test.rs`); BEL `0x07` tại dòng 16 (`<BEL>ctions::dismiss_done`) và dòng 20 (`<BEL>gm done`). Không ảnh hưởng mã nhưng làm sai lệch tên tệp/lệnh trong log; nên thay bằng chữ thường.
- Tiến trình TDD/commit:log dòng 9-19 ghi đủ các bước 0→3 với kết quả `cargo test --offline` từng bước; lịch sử commit dùng đúng tiền tố `inbox-done-bulk:` (`ce3cf89`, `7d25bf1`, `a28e71e`, (sửa EOL) `686028e`). Không có lệch thiết kế nào khác so với "Quyết định của CEO" (điểm 1-7): bộ lọc AND, ID+filter giao, blocked protection, một transaction, TUI, docs — đều khớp; chỉ có các lệch F1-F3 ở trên.
- Phạm vi tệp:diff `main...HEAD` chỉ chạm đúng các tệp được phép sửa cộng với `.agents/tasks/...`, `.agents/log/...`, review; không đụng schema/daemon/module khác. Không worktree khác, không merge/push trong lần review này.

## Kết luận

**CHƯA ĐẠT** — SHA HEAD được duyệt: `686028efece835f193433bdbc61c95cb625b0349` (`686028e`).

Chặn / nên sửa / nhỏ = **1 / 3 / 2**.

- Chặn: **F1** — panic tràn i64 tại `src/actions.rs:140` với khoảng `0-9223372036854775807` (exit 101 ở debug, vòng lặp vô hạn ở release); vi phạm yêu cầu chống overflow và lỗi rõ cho khoảng quá lớn.
- Nên sửa: **F2** `README_vi.md:142` cú pháp TUI cũ; **F3** `.agents/log/inbox-done-bulk.md:11` CRLF/EOL mixed; **F4** thiếu test CLI/TUI cho quy tắc blocked.
- Nhỏ: **F5** ký tự BEL/TAB trong log; **F6** log thiếu mục "Lệch so với tệp tác vụ".

Sau khi sửa F1 (bắt buộc) và các mục nên sửa, cần chạy lại `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check` và `git ls-files --eol` trước khi trình duyệt lại.

## Xem lại vòng sửa (fbe9a57)

- Phạm vi: `git diff 686028e..fbe9a57`; đỉnh mã được duyệt `fbe9a5795e9f0b1e916a9b4c02f015c552324357` (`fbe9a57`). Reviewer không sửa mã sản phẩm, chỉ đối soát và ghi kết quả vào tệp này.
- **F1 (đã xử lý, đúng)**: `src/actions.rs:140-143` thay `end - start + 1 > 10_000` bằng `end - start >= 10_000` (tương đương về ngữ nghĩa nhưng bỏ phép `+ 1` tràn i64). `parse_id_token` chỉ nhận chữ số ASCII nên `start`, `end` đều ≥ 0, lại có `if end < start { bail }` → `end - start` không tràn. Chạy thử với `AGM_HOME` tạm: `agm done 0-9223372036854775807 --json` → `agm: error: id range too large: 0-9223372036854775807`, exit 1, không panic. Biên CLI: `1-10000` exit 0 (10000 mục), `1-10001` exit 1 lỗi rõ; `9223372036854775806-9223372036854775807` exit 0, trả `skipped/missing` đúng 2 ID. Test `tests/actions_test.rs` phủ biên 10000/10001, `0-i64::MAX`, dãy `i64::MAX-1..i64::MAX`, số vượt i64::MAX.
- **F2 (đã xử lý)**: `README_vi.md:142` dòng `cmd:` của TUI dùng `done <#|a-b|kind:K|term:T>`, khớp bản Anh (`README.md:144`).
- **F3/F5 (đã xử lý)**: `.agents/log/inbox-done-bulk.md` — kiểm byte: 31 LF, 0 CR/CRLF, 0 TAB, 0 BEL; các chuỗi `kind:, term:`, `actions::dismiss_done`, `tests/...`, `agm done` đã sạch ký tự điều khiển. `git ls-files --eol`: cả 14 tệp trong `git diff main...HEAD` đều `i/lf w/lf`.
- **F4 (đã xử lý, đúng)**: `tests/cli_test.rs` thêm `cli_done_range_and_filters_never_close_blocked_items` — cấy mục blocked ở giữa khoảng, kiểm tra khoảng ID đóng 2 mục thường và đưa blocked vào `skipped`, filter `--terminal` trùng blocked vẫn bỏ qua, chỉ `--source blocked` đóng được. `tests/dashboard_test.rs` thêm ca `done source:blocked` / `done src:blocked 1-3` (`explicit_source_blocked=true`, `source=blocked`) và `done 1-3 source:message` (`explicit=false`) — khóa đúng quy tắc an toàn trên cả hai đường CLI/TUI.
- **F6 (đã xử lý)**: log có mục `## Lệch so với tệp tác vụ (bổ sung sau review, CEO)` ghi nhận 3 lệch thực tế.

- Lệnh kiểm đã chạy: `cargo test --offline` → **326 passed, 0 failed** (tăng 1 so với 325 ở vòng trước, đúng test CLI mới); `cargo clippy --offline --all-targets -- -D warnings` → sạch, exit 0; `cargo fmt --check` → sạch, exit 0; `git ls-files --eol` trên 14 tệp → `i/lf` hết. Các test mới (`cli_done_range_and_filters_never_close_blocked_items`, `parse_done_id_selectors_rejects_bad_input`, `parse_commands`) đều hiện diện và xanh.
- Không hồi quy: diff vòng sửa chỉ đụng `src/actions.rs` (1 dòng F1), `README_vi.md` (1 dòng F2), 3 tệp test (F1/F4) và log (F3/F5/F6), cộng tệp review này; không đổi schema/daemon/module khác. Không chạy `agm done` trên inbox thật.

### Kết luận xem lại

**ĐẠT** — SHA HEAD được duyệt: `fbe9a5795e9f0b1e916a9b4c02f015c552324357` (`fbe9a57`).

Chặn / nên sửa / nhỏ = **0 / 0 / 0**.
