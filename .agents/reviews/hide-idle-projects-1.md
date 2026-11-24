model: agy/gemini-4-hydrogen-high (--mode plan)

Đối tượng: HEAD 3941d24
Kết luận: ĐẠT

### Bắt buộc sửa
Không có.

### Nên sửa
- `src/tui_cmd.rs:cd_list_hides_idle_projects_and_reports_them` và `tests/board_runtime_test.rs:load_data_hides_idle_projects_but_keeps_the_scoped_one`: Chưa có test kiểm tra đường dẫn phân giải tên (`cd <name>`, `cd -t <name>` qua `src/tui_cmd.rs:lookup_project` và `src/board/mod.rs:resolve_project`) đối với một dự án **idle**. Nếu sau này có sửa đổi vô tình gọi `project::split_idle` trước `project::match_project` trong `lookup_project` hoặc `resolve_project`, toàn bộ test hiện tại (`cd_found_opens_board_both_fronts` chỉ dùng dự án live `shop`) vẫn pass trong khi `cd <idle_name>` và `agm board <idle_name>` sẽ trả về `no project matching`.
- `tests/project_render_test.rs:project_list_reports_hidden_idle_projects` / `zero_width_does_not_panic` và `src/tui_cmd.rs:cd_list_hides_idle_projects_and_reports_them`: `render_project_list` với `idle_hidden > 0` mới được kiểm tra ở `width = 100` (chưa test ở `width = 0` hoặc `width` hẹp hơn độ dài dòng chứa ký tự UTF-8 `—`), và test `cd` trong `tui_cmd.rs` chưa kiểm tra trường hợp `idle_hidden == 0`. Nếu `bounded_block` hoặc nhánh `outcome` có hồi quy cắt chuỗi/hiển thị thừa dòng ghi chú khi không có dự án idle, test `tui_cmd` sẽ không phát hiện sớm.
- `tests/board_runtime_test.rs:load_data_hides_idle_projects_but_keeps_the_scoped_one`: Mới gọi `board::load_data` trên `BoardState::new(...)` mới khởi tạo, chưa kiểm tra chu kỳ refresh khi danh sách `BoardData.projects` thu hẹp do một dự án chuyển sang idle trong lúc con trỏ chọn dự án hoặc `Mode::PickProject { idx }` đang trỏ vào vị trí cuối danh sách cũ.

### Ghi chú
- **Phạm vi lọc và giữ dự án scoped (`src/project.rs:split_idle`, `src/tui_cmd.rs:outcome`, `src/board/mod.rs:load_data`)**: `split_idle` lọc đúng điều kiện `live_agents == 0 && open_inbox == 0 && keep.is_none_or(|key| key != &p.key)`, giữ nguyên thứ tự và đếm chính xác số dự án bị ẩn. Chỉ hai danh sách hiển thị (`TuiCmd::Cd(None)` với `keep = None`, và `BoardData.projects` trong `load_data` với `keep = state.project.as_ref()`) bị lọc; `lookup_project` và `resolve_project` vẫn gọi `match_project` trên danh sách đầy đủ từ `list_projects`.
- **Hiển thị dòng ẩn (`src/project_render.rs:render_project_list`)**: Dòng `  {idle_hidden} idle project(s) hidden — cd <name> still opens them` được bọc `paint(GRAY, ...)` đúng kiểu sẵn có, chỉ thêm khi `idle_hidden > 0`, vẫn được in ngay cả khi toàn bộ dự án đều idle (`list.is_empty() && no_project == 0`, kèm `  (no projects)`), và đi qua `bounded_block(lines, width)` (`bounded_styled_line(..., width.saturating_sub(1))`).
- **Giới hạn hồ sơ review tĩnh (`src/board/mod.rs:load_data`, `apply_refresh`)**: Trong `src/board/mod.rs`, `load_data` luôn gọi `state.sync(&data)` trên `BoardData.projects` đã lọc (cả hai nhánh `Some(key)` và `None`) trước khi `apply_refresh` thay `self.data`. Tuy nhiên các file `src/board/state.rs`, `src/board/view.rs` và `src/board/app.rs` không nằm trong tập file được cung cấp nên việc clamp chỉ số trong `BoardState::sync` / `Mode::PickProject { idx }` / picker `p` không được kiểm tra trực tiếp.


## CEO follow-up
- Added `cd_name_still_opens_an_idle_project` (tui_cmd.rs) for the name-resolution guard.
- Index clamping checked in src/board/state.rs: `sync` clamps `sel_project`; `PickProject` Enter uses `.get(idx)` (no panic). Narrow-width/refresh-cycle tests left as is.
