# Review độc lập — `cli-help-english`

- Reviewer: reviewer độc lập (không phải thợ); **không sửa mã sản phẩm**.
- Đối tượng: HEAD `b66ee613f8d80a011fe6aa90b5c4e6f65b55aa89`, phạm vi `git diff main...HEAD`.
- Brief: `.agents/tasks/cli-help-english.md`; log: `.agents/log/cli-help-english.md`.
- Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`.
- Kết luận ghi ở cuối mục 5.
- Tiền tố commit dùng `cli-help-english-review:` theo chỉ thị của đầu việc review này (lệch với quy ước `t<N>:` trong Global Constraints của plan, vì đây là luồng review riêng, không phải đầu việc thi công).

## Danh sách mục kiểm

1. Phạm vi diff `src/main.rs` — chỉ đổi help/doc comment/about của clap.
2. Help tiếng Anh đúng nghĩa với code hiện tại (kill/interrupt/cleanup/rotate/manage/budget/policy/send/sys…), không làm yếu cảnh báo an toàn.
3. Test `cli_help_is_english_for_every_command` phủ mọi lệnh và lệnh con lồng so với enum trong `src/main.rs`.
4. `CHANGELOG.md` giữ LF, dòng mới nằm ở `[Unreleased]`.
5. Chạy `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check`.

## 1. Phạm vi diff `src/main.rs` — ĐẠT

Đã liệt kê toàn bộ dòng `+`/`-` của `git diff --unified=0 main...HEAD -- src/main.rs`; mọi dòng thay đổi chỉ rơi vào ba nhóm sau:

- `about` của root: `src/main.rs:28` — tiếng Việt → `"Track agent tokens, subscription quotas, and conversations in herdr"`. Là thuộc tính help của clap, được phép.
- Doc comment `///` cho `Cli`, toàn bộ variant của `Cmd`/`BudgetCmd`/`HookCmd` và mọi field/arg: `src/main.rs:31-279`. Không còn doc comment tiếng Việt nào trong phần định nghĩa clap; chuỗi có dấu tiếng Việt duy nhất trong các dòng thêm là 7 mã giao thức kèm giải nghĩa ở `src/main.rs:102` — đúng brief mục 3.
- Reflow cấu trúc thuần túy để gắn doc comment cho field: `Done { id: i64 }` → khối nhiều dòng (`src/main.rs:123-126`) và `Link { terminal: String, session: String }` → khối nhiều dòng (`src/main.rs:216-221`). Tên field, kiểu (`i64`, `String`) và thứ tự không đổi — hệ quả của rustfmt khi chèn `///`, được brief/log chấp nhận (`.agents/log/cli-help-english.md:35`).

Không có dòng nào đổi tên lệnh/cờ/alias, `default_value*`, `value_parser`, `conflicts_with`, `#[arg(...)]` khác, khóa JSON hay chuỗi runtime. Kiểm chứng ngược: các chuỗi runtime tiếng Việt vẫn nguyên và **không** nằm trong diff, ví dụ `src/main.rs:349` (`đọc cấu hình …`), `:506` (`không có ngân sách {scope}`), `:629` (`--full chỉ dùng cho phiên Codex/Claude …`).

Ghi nhận (không tính lỗi): `git diff --name-status main...HEAD` cũng có thêm 2 tệp điều phối `.agents/tasks/cli-help-english.md` (brief) và `.agents/log/cli-help-english.md` (log), ngoài 4 tệp liệt kê trong brief; đây không phải mã sản phẩm.

`chặn`: 0. `nên sửa`: 0. `nhỏ`: 0.

## 2. Help đúng nghĩa code hiện tại — ĐẠT (1 nhận xét `nhỏ`)

Đối chiếu từng câu help với dispatch ở `src/main.rs:394-648` và implementation tương ứng:

| Lệnh | Help | Code hiện tại | Kết luận |
|---|---|---|---|
| `kill` / `interrupt` | `src/main.rs:155,159,163,167` — "Codex/Claude and unknown apps still require a user to type y"; `--yes` "still requires a user caller" | `src/ops.rs:224-239`: `let yes = yes && app == "opencode"` (`:230`); `caller()` (`src/ops.rs:33-56`) chỉ tự xác nhận khi `detect_caller` trả `User{confirmed}`; agent bị chặn bởi `AGENT_KILLABLE_APPS = ["opencode"]` (`src/actions.rs:65-67,81-102`). | Đúng; cảnh báo an toàn **không bị yếu đi**. |
| `cleanup` | `src/main.rs:193-202` — "requires one typed y/N confirmation"; `--yes` "Compatibility flag; cleanup still requires one typed confirmation" | `src/ops.rs:379-386` nhận `_yes` rồi bỏ qua; `src/ops.rs:444-455` luôn gọi `caller(store, false, …)`; `--all` chọn mọi ứng viên (`:399-401`). | Đúng. |
| `rotate` | `src/main.rs:171-177` — OpenCode handoff+kill+spawn; Claude/Codex handoff+open, không kill | `src/ops.rs:241-287`; OpenCode → `rotation::start` rồi daemon kill+spawn (`src/rotation.rs:401-429`); Claude/Codex managed → handoff + successor, "old session is not killed" (`src/ops.rs:267`, `src/rotation.rs:1-4`). | Đúng, trừ câu `--yes` — xem `nhỏ` bên dưới. |
| `manage` | `src/main.rs:179-191` — bật/tắt quản lý cho phiên Claude Code/Codex mở tay; `--off`, `--cmd`, argv sau `--` | `src/ops.rs:296-377`: chỉ nhận `claude-code`/`codex` (`:326-336`), `off` → `clear_terminal_managed` (`:307-313`), **không nhận `--yes`** (`:369`), argv lấy từ `raw_argv` hoặc `parse_cmd_tokens(cmd)` (`:345-363`). | Đúng. |
| `budget` | `src/main.rs:204,251-270` — `task:<ID>` (tokens) / `provider:<name>:<minutes>` (quota percent), soft/hard, action | `src/ops.rs:520-545`: `task` → `"tokens"`, `provider` → `"percent"`; action chỉ `notify|interrupt|kill`, mặc định `notify`; `ls`/`rm` ở `:548-569` và `src/main.rs:502-509`. | Đúng. |
| `policy` | `src/main.rs:209-213` — bật/tắt mọi hành động tự động (emergency stop); một trong `on`/`off`/`status` | `src/main.rs:510-538`: `value_parser = ["on","off","status"]`, ghi setting `policy`; `status` đọc `policy::enabled`. | Đúng. |
| `send` | `src/main.rs:102-114` — 7 mã kèm giải nghĩa + alias `ASSIGN|REPORT|ERROR|QUESTION|BLOCKED|PASS|FAIL`, đích `user` → inbox | `src/actions.rs:12-23`: `KINDS` và `KINDS_EN` khớp từng cặp; `kind_from_english` chấp nhận mọi chữ hoa/thường (`:33-39`); `src/ops.rs:153-182` gửi qua actions. | Đúng; alias liệt kê đúng giá trị và thứ tự. |
| `sys` | `src/main.rs:239` — RAM/CPU/disk + phần process tree của từng agent, live, không lưu | `src/main.rs:318-334` đọc `sysmon::read_once()` ngay cả khi thiếu/khóa DB; `src/sysmon.rs:1,106-121` định nghĩa đúng các trường. (Câu này đã có trên `main`, không đổi trong diff.) | Đúng. |

Các câu còn lại cũng khớp: `scan` (`src/main.rs:40` ↔ `:673-703`: Codex/Claude/agy + OpenCode DB), `quota --refresh` (`:72` ↔ `:585-614`: agy/Codex/OpenCode), `cost` (`:55,57,63,66` ↔ `:568-583` + `src/report.rs:28-45`), `log --full` (`:83` ↔ `:625-637`, chặn `opencode://` và `agy://`), `hook install` (`:278` ↔ `src/ops.rs:595-623`, đã có statusLine thì bails, không ghi đè), `ps --all` (`:98` ↔ `:436-439`), `inbox`/`done`/`reply` (`:116-133` ↔ `:454-461`).

`nhỏ` — `rotate --yes` mô tả hơi rộng so với code: `src/main.rs:175` ghi "Skip the interactive confirmation prompt", nhưng trong `src/ops.rs:241-287` biến `yes` chỉ được dùng ở nhánh app không được hỗ trợ (`:275`) qua `caller(store, yes, None)` — với `needs_confirm = None` hàm này trả về ngay, không hỏi (`src/ops.rs:43-45`). Hai nhánh được hỗ trợ (OpenCode `:246-255`, Claude/Codex managed `:256-273`) gọi `rotation::start`, hàm không nhận `yes` (`src/rotation.rs:189-195`). Nói cách khác `--yes` của `rotate` không thực sự bỏ qua một lời nhắc nào. Đề xuất (không chặn): đổi thành "Accepted for compatibility; supported rotations do not prompt" hoặc bỏ câu confirmation. Không ảnh hưởng an toàn: các lời nhắc thật vẫn được mô tả đúng ở `kill`/`interrupt`/`cleanup`/`manage`.

`chặn`: 0. `nên sửa`: 0. `nhỏ`: 1.

## 3. Phủ test help cho mọi lệnh/con lồng — ĐẠT

Enum nguồn: `Cmd` có 24 variant (`src/main.rs:39-247`), `BudgetCmd` có 3 (`src/main.rs:249-272`: `Set`, `Ls`, `Rm`), `HookCmd` có 2 (`src/main.rs:274-280`: `Statusline`, `Install`) — tổng 29; cộng root `agm --help` → 30 đường cần kiểm.

Danh sách ở `tests/cli_test.rs:661-692` có đúng 30 phần tử và khớp từng một:

- Root `&[]` (`:662`).
- 24 lệnh cấp một: `scan, ls, cost, quota, log, daemon, ps, send, inbox, done, reply, spawn, kill, interrupt, rotate, manage, cleanup, budget, policy, link, ask, hook, sys, tui` (`:663-680`, `:684-691`).
- 5 lệnh con lồng: `budget set`, `budget ls`, `budget rm` (`:681-683`), `hook statusline`, `hook install` (`:688-689`).

Đã đối chiếu cơ học (tách tên 29 variant bằng regex `^    ([A-Z]\w*)\s*(\{|,)` trên `src/main.rs:39-280`, so với 30 chuỗi trong mảng test): không có variant nào thiếu, không có đường lệnh nào trong test là lệnh không tồn tại. Các "khác biệt" chỉ là 5 con lồng được viết có tiền tố cha (`budget set`…), đúng kỳ vọng.

Vòng lặp `tests/cli_test.rs:693-712`: thêm `--help`, assert exit success (`:697-701`); nếu help có `--json` thì assert chứa "Print JSON" (`:703-705`) — hợp lệ vì `--json` là cờ global (`src/main.rs:31-33`), và thực tế mọi help đều có; xóa 7 mã giao thức rồi assert ASCII (`:706-711`) — đúng cách loại trừ giá trị protocol theo brief mục 3/5.

Hai test bổ sung đúng yêu cầu brief: `cli_stop_help_preserves_user_confirmation_warning` (`tests/cli_test.rs:715-727`) kiểm cả `kill` lẫn `interrupt`; `cli_send_help_explains_existing_message_codes` (`:729-746`) kiểm đủ 7 cặp "mã (giải nghĩa)". Test cũ `cli_cleanup_help_documents_mandatory_confirmation_even_with_yes` (`:600-609`) được giữ lại và pass — tiếp tục bảo vệ cảnh báo cleanup.

`chặn`: 0. `nên sửa`: 0. `nhỏ`: 0.

## 4. `CHANGELOG.md` — ĐẠT

`git diff main...HEAD -- CHANGELOG.md` chỉ có một dòng thêm: `CHANGELOG.md:8` = `- Help CLI chuyển sang tiếng Anh.`, nằm dưới `## [Unreleased]` (`:3`) → `### Thay đổi` (`:5`), ngay sau bullet `agm cost` có sẵn (`:7`). Đúng brief mục 6; không sửa mục bản phát hành nào khác.

Kiểm trực tiếp trên byte: toàn tệp có 74 ký tự `0x0A` (LF) và **0** ký tự `0x0D` (CR) → LF thuần, không bị CRLF hóa. 3 byte đầu `23 20 43` = `# C` → không có BOM; 12 byte cuối kết thúc bằng `0A` → dòng cuối có newline.

`chặn`: 0. `nên sửa`: 0. `nhỏ`: 0.

## 5. Lệnh kiểm — ĐẠT

Chạy trong worktree `D:\Project\agmag.wt\cli-help-english` (PowerShell) trên HEAD `b66ee61`:

| Lệnh | Kết quả |
|---|---|
| `cargo test --offline` | exit 0 — 31 suite, **313 passed / 0 failed / 0 ignored**. Riêng `tests/cli_test.rs`: 22/22, gồm `cli_help_is_english_for_every_command`, `cli_stop_help_preserves_user_confirmation_warning`, `cli_send_help_explains_existing_message_codes` và `cli_cleanup_help_documents_mandatory_confirmation_even_with_yes`. |
| `cargo clippy --offline --all-targets -- -D warnings` | exit 0 (`Finished dev profile`), không warning. |
| `cargo fmt --check` | exit 0, không output — sạch. |

`git status --short --branch` chỉ còn `## feat/cli-help-english` và tệp review này; reviewer **không sửa mã sản phẩm**. Không push, không merge.

## Kết luận

**ĐẠT** — HEAD `b66ee613f8d80a011fe6aa90b5c4e6f65b55aa89` được duyệt.

Tổng phát hiện: **chặn / nên sửa / nhỏ = 0 / 0 / 1**. Phát hiện duy nhất là gợi ý chỉnh câu help của `rotate --yes` (`src/main.rs:175`, phân tích ở mục 2): không chặn nghiệm thu, không làm yếu cảnh báo an toàn, và không bắt buộc phải sửa mã sản phẩm để đạt.
