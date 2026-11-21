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
