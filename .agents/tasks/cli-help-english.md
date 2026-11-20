---
id: cli-help-english
tier: mid
skills: test-driven-development
branch: feat/cli-help-english
worktree: D:/Project/agmag.wt/cli-help-english
---

# `agm --help` vẫn tiếng Việt — chuyển help CLI sang tiếng Anh

## Hiện trạng (Claude CEO kiểm 2026-11-21)

- `a7ce50f` đã chuyển mọi bảng/dòng kết quả CLI và TUI sang tiếng Anh, `README.md` tiếng Anh (bản Việt ở `README_vi.md`). Nhưng **help của clap** (`agm --help`, `agm <lệnh> --help`) vẫn tiếng Việt: toàn bộ doc comment `///` và `about`/`help` trong `src/main.rs`.
- Nhánh cũ `docs/english-help` (commit `0694e7f`, base `ede5da6`, rất cũ) đã dịch sẵn help của **các lệnh có lúc đó**. Dùng nó làm nguồn: `git show 0694e7f:src/main.rs`. Phần README/AGENTS-TUTORIAL/HERDR-CORE-BOT của nhánh đó **bỏ** (README đã tiếng Anh; hai tutorial là tài liệu làm việc tiếng Việt, ngoài phạm vi).
- Từ đó đến nay `main` có thêm lệnh/cờ mới (vd `sys`, `manage`, `cleanup`, các cờ của `cost`/`quota`…). Những chỗ này phải tự dịch.

## Yêu cầu

1. Mọi doc comment / `about` / `help` / `long_about` của clap trong `src/main.rs` (root, mọi lệnh, mọi lệnh con lồng, mọi cờ và đối số vị trí) → tiếng Anh, ngắn gọn, đúng nghĩa với code **hiện tại** (không chép mù từ `0694e7f` nếu hành vi đã đổi — đọc code).
2. **Không đổi hành vi**: tên lệnh/cờ, alias, giá trị mặc định, parser, khóa JSON, chuỗi runtime (thông báo lỗi, bảng, output) giữ nguyên. Chỉ đổi văn bản help.
3. Mã tin nhắn (`GIAO`, `BÁO CÁO`, `LỖI`, `HỎI`, `CHẶN`, `ĐẠT`, `CHƯA ĐẠT`) là giá trị giao thức: giữ nguyên, kèm giải nghĩa tiếng Anh, vd `GIAO (assign)`, `"BÁO CÁO" (report)`. Nếu `send --type` hiện đã nhận alias tiếng Anh (xem code TUI/`send`), nêu cả alias trong help.
4. Giữ nguyên các cảnh báo an toàn trong help, vd `kill`/`interrupt`: Codex/Claude và app không rõ vẫn cần người dùng gõ `y` — dịch sát nghĩa, không làm yếu đi.
5. Test trong `tests/cli_test.rs` (port từ `0694e7f`, cập nhật danh sách lệnh theo `main` hiện tại — **lấy danh sách từ code**, gồm cả lệnh con lồng):
   - `cli_help_is_english_for_every_command`: mỗi `agm [cmd…] --help` thành công và, sau khi bỏ các mã tin nhắn ở trên, toàn bộ là ASCII. (Bỏ assert `"Print JSON"` nếu có lệnh không có cờ `--json`; giữ khi phù hợp.)
   - `cli_stop_help_preserves_user_confirmation_warning` cho `kill`, `interrupt`.
   - `cli_send_help_explains_existing_message_codes`.
   Viết test trước (đỏ), rồi dịch đến khi xanh.
6. `CHANGELOG.md` mục `[Unreleased]` → `### Thay đổi` thêm một dòng: help CLI chuyển sang tiếng Anh. Giữ LF.

## Phạm vi tệp

Được sửa: `src/main.rs` (chỉ văn bản help của clap), `tests/cli_test.rs`, `CHANGELOG.md`, `.agents/log/cli-help-english.md`.
Không sửa: README*, AGENTS-TUTORIAL.md, HERDR-CORE-BOT.md, BOT-CEO.md, docs/**, mọi module khác, nhánh `docs/english-help` / `support/agy-review`. Chỉ `cargo fmt` tệp bạn sửa.

## Tiêu chí nghiệm thu

- `cargo run --offline -q -- --help` và `--help` của từng lệnh: tiếng Anh (ghi vài mẫu vào log).
- `git diff main -- src/main.rs` chỉ chạm doc comment / thuộc tính help (ghi vào log cách bạn kiểm điều này).
- `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- **Commit sau mỗi bước** (tiền tố `cli-help-english:`), ghi `.agents/log/cli-help-english.md`.

## Báo cáo

Xong: `agm send user --type "BÁO CÁO" --task cli-help-english "Xong: <SHA>"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task cli-help-english "<câu hỏi ngắn>"`.
