- Đối tượng: `a0b40e5` / `234b588` (Task 5b/5c — nối TUI của worker `g`) trên `feat/tui-project`

## Kiểm tra các điểm chính của Task 5 (nối TUI)

1. **`src/tui.rs` (vòng lặp TUI & prompt):** **ĐẠT.**
   - `scope: Option<ProjectKey>`, `roots_cache: RootCache`, `shown: Option<String>` được giữ trong phiên TUI; `shown` được xóa mỗi lượt lệnh (`src/tui.rs:318`) và nối vào `text` ngay trên `cleanup_list` / `note` / prompt (`src/tui.rs:291-294`).
   - `render_frame` (`src/tui.rs:167-176`) hiển thị `agm[<name>]> ` khi scoped và `agm> ` khi global; tên dự án đi qua `one_line(name, max_w.saturating_sub(7))` nên loại bỏ chuỗi escape OSC/CSI và không tràn chiều rộng (kiểm chứng bởi `prompt_shows_scope`).
   - `Cd(None)` gọi `compute_roots` + `list_projects` + `render_project_list` và hiển thị qua `shown`.
   - `Cd(Some(".." | "/"))` đặt `scope = None` và báo `"back to all projects"`.
   - `Cd(Some(q))` khớp dự án qua `match_project`: `Found` đặt `scope = Some(key)` (và giữ scope kể cả khi dự án hết thành viên); `Ambiguous` và `NotFound` hiện `Warn`/`Err` đúng thiết kế.
   - `Show(id)` gọi `store.inbox_item(id)` → `render_inbox_item(&item, w)` vào `shown`, hoặc báo `no inbox item #<id>` / `error: ...`.
   - Khi `snapshot()` lỗi (`snap == None`), `Cd(None)` và `Cd(Some(_))` báo `"failed to read database"` an toàn.
2. **`src/dashboard.rs:1166-1177` (`render_scoped`):** **ĐẠT.**
   - Ghép `render_header(s, width)` + từng dòng `body` + `render_footer(width)` rồi bọc mọi dòng qua `bounded_styled_line(line, max_w)` (kiểm chứng bởi `render_scoped_wraps_body_with_header_and_footer` ở 60 cột).
3. **Gỡ `#![allow(dead_code, unused_variables)]` (`src/project.rs`, `src/project_render.rs`, `src/store/project.rs`):** **ĐẠT.**
   - Cả 3 tệp đã bỏ `#![allow(...)]`; `cargo clippy --all-targets -- -D warnings` sạch trên bản `234b588` / `tui-verify`.

## Nên sửa / Nhỏ

- **Nhỏ (`src/tui.rs:533, 558`):** `Cd(None)` và `Cd(Some(_))` gọi lại `store.terminals(false)` và `store.inbox(true)` dù `snapshot()` ở đầu vòng lặp đã đọc `inbox(true)`; không gây lỗi (SQLite WAL chỉ đọc nhanh, `roots_cache` tái dùng được) nên không cần chặn merge.

## Kiểm chứng trên `tui-verify` (`234b5889c7e6`)

- `cargo test --test dashboard_test --test project_render_test --quiet`: 28/28 + 8/8 đạt.
- `cargo test --bin agm --quiet`: 7/7 đạt (`prompt_shows_scope`, `frame_note_strips_untrusted_escapes_and_bounds_wide_text`, v.v.).

## KẾT LUẬN: ĐẠT (0 Blocker ở phần nối TUI của worker `g`)
