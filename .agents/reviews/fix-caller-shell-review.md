- Đối tượng: `D:\Project\agmag.wt\fix-caller-shell` (nhánh `fix/tui-caller-shell`, HEAD `e3e8177` gồm `e715863` + `e3e8177`, so với `f3762bf`)

## Đối chiếu Yêu cầu & Các ca biên

1. **Ca `t7y` (cờ `reported: true` còn sót, `status: null`, `foreground.name: "pwsh.exe"`):** **Đạt.** `idle_shell_foreground` (`src/actions.rs:88-96`) nhận diện `pwsh.exe` qua `is_interactive_shell` (không phân biệt hoa thường, bỏ `.exe`) và thấy không có `status.app`, nên `detect_caller` trả `false` (bỏ qua `reported` cũ và CSDL) → `Caller::User`.
2. **Ca `t7q` (CSDL còn `app: "claude-code"` cũ, `status: null`, `foreground.name: "pwsh.exe"`):** **Đạt.** Nhờ nhánh `if idle_shell_foreground(&v)` chạy trước phần tra `store.terminal(t)`, app cũ trong `agm.db` không chặn nhầm người dùng (`pwsh.exe`, `PowerShell.EXE`, `cmd.exe`, `bash`, `sh`, `zsh`, `fish`, `nu`).
3. **Agent đang sống trong pane shell (`status: {"app": "codex", ...}` + `foreground.name: "pwsh.exe"`):** **Đạt.** `v["status"]["app"]` khác rỗng nên `idle_shell_foreground` trả `false`, rơi về logic cũ (`row.reported || app in AGENT_APPS`) → vẫn là `Caller::Agent`.
4. **Foreground không phải shell (`node.exe`), lỗi `hc get`, hoặc không có TTY (`tty = false`):** **Đạt.**
   - `node.exe` + `reported: true` hoặc CSDL `claude-code` → `Caller::Agent`.
   - `hc.get` lỗi → fail-closed `Caller::Agent`.
   - `tty = false` → `classify_caller` vẫn trả `Caller::Agent` bất kể `idle_shell_foreground`.
5. **Test & tài liệu (`tests/actions_test.rs`, `CHANGELOG.md`, `.agents/log/fix-caller-shell.md`):** **Đạt.** Đủ 6 kịch bản test bám đúng dữ liệu thực tế `t7y`/`t7q`; mục `## [Unreleased]` / `### Sửa lỗi` trong `CHANGELOG.md` đúng tiếng Việt theo yêu cầu.

## Nên sửa / Nhỏ (không chặn merge)

1. **`src/actions.rs:93-95` — Trường `status` khác `null` nhưng thiếu `app`:**
   - Hiện kiểm `v["status"]["app"].as_str().is_none_or(|a| a.trim().is_empty())`. Nếu về sau `hc` trả về một object `status` sống không có khóa `"app"` (ví dụ `{"state": "working"}`), hàm sẽ coi là không có agent sống. Siết thêm `v["status"].is_null()` (hoặc chỉ coi là rỗng khi `status` là `null`) sẽ an toàn hơn.
2. **Ghi chú biên khởi động:** Khi `hc` chạy `pwsh.exe -Command "& 'agent'"` ở khoảnh khắc trước khi agent kịp gửi lần báo trạng thái đầu tiên (`status: null`), `foreground.name` vẫn là `"pwsh.exe"`; tuy nhiên agent chạy không TTY vẫn bị `tty = false` chặn.

## Kiểm chứng trên `D:\Project\agmag.wt\fix-caller-shell` (`e3e8177`)

- `cargo fmt --check`: Đạt (exit 0).
- `cargo clippy --all-targets -- -D warnings`: Đạt (exit 0).
- `cargo test --quiet` (toàn bộ các suite trên `fix-caller-shell`): Đạt (exit 0; `actions_test`: 32 passed, 0 failed).

## KẾT LUẬN: ĐẠT (0 Blocker; đủ điều kiện merge)
