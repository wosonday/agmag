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
