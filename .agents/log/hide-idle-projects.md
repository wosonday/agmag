# hide-idle-projects log

Task: `.agents/tasks/hide-idle-projects.md`. Ẩn dự án idle (`live_agents == 0 && open_inbox == 0`) khỏi hai danh sách hiển thị; giữ nguyên phân giải tên (`cd <name>`, `match_project`).

## 2026-11-24

### State before
- Worktree: D:/Project/agmag.wt/ghost-projects, branch `feat/hide-idle-projects` (base 3d1d880).
- Cây làm việc sạch. `git ls-files --eol`: CHANGELOG.md, src/main.rs, src/tui.rs = CRLF; src/project.rs, src/board/*, src/tui_cmd.rs, tests/*, .agents/log/* = LF.
- Không push / merge / đổi branch; mỗi bước commit riêng.

### Step 1: `split_idle` + unit tests (RED → GREEN)
- `tests/project_test.rs`: thêm 6 test `split_idle_*` bao phủ: idle bị ẩn, live được giữ,
  inbox-only được giữ, `keep` được giữ dù idle, số đếm chỉ tính bản bị ẩn, thứ tự input giữ nguyên.
- Đỏ: `error[E0432]: unresolved import agmag::project::split_idle` (`cargo test --test project_test split_idle`).
- `src/project.rs`: hàm thuần `split_idle(list, keep) -> (visible, hidden)` đặt ngay sau `list_projects`;
  idle = `live_agents == 0 && open_inbox == 0`, `keep` so khớp đúng `ProjectKey`. Không đổi
  `list_projects` / `match_project` / `load_project_view`.
- Xanh: 6/6 test `split_idle_*` (`cargo test --test project_test split_idle`).
- Commit: `feat(project): add split_idle for display lists`.

### Step 2: danh sách `cd` dùng `split_idle` (RED → GREEN)
- `tests/project_render_test.rs`: thêm `project_list_reports_hidden_idle_projects` — dòng
  `N idle project(s) hidden — cd <name> still opens them` có khi N>0, vắng khi N=0, và vẫn còn
  `(no projects)` khi mọi dự án đều bị ẩn. Đỏ: `E0061` (hàm nhận 3 đối số).
- `src/tui_cmd.rs`: thêm `cd_list_hides_idle_projects_and_reports_them` — store có một dự án live
  (`shop`) và một dự án exited (`ghost`); gõ `cd` → `Outcome::Shown` chứa `shop`, không chứa
  `ghost`, có dòng báo `1 idle project(s) hidden …`. Đỏ: danh sách vẫn in `ghost`.
- `src/project_render.rs`: `render_project_list` nhận thêm `idle_hidden: usize`; thêm dòng báo
  cuối danh sách (style `paint(GRAY, …)` như các dòng khác), giữ `(no projects)` khi rỗng.
- `src/tui_cmd.rs`: nhánh `TuiCmd::Cd(None)` gọi `split_idle(all, None)` rồi truyền số bị ẩn
  vào renderer. `cd <name>` / `match_project` vẫn nhận full list, không đổi.
- Xanh: 14/14 `project_render_test`, 1/1 test `cd_list_*` (`--bin agm`).
- Commit: `feat(tui): hide idle projects in the cd listing`.

### Step 3: cột dự án của bảng dùng `split_idle` (RED → GREEN)
- `tests/board_runtime_test.rs`: thêm `load_data_hides_idle_projects_but_keeps_the_scoped_one` —
  3 thư mục `live` (1 agent live), `ghost` (exited), `scoped` (exited). Không scope → chỉ `live`;
  scope tới `scoped` → `live` + `scoped`, `ghost` vẫn bị ẩn và `data.view` mở được.
  Đỏ: danh sách có cả 3 (`left: 3, right: 1`).
- `src/board/mod.rs`: `load_data` gọi `split_idle(all_projects, state.project.as_ref())`; dự án
  đang scope được giữ lại dù idle. Không đổi `resolve_project` (~100), `match_project`, hay
  `src/board/app.rs` (~302) — phân giải tên vẫn dùng full list.
- Xanh: 7/7 `board_runtime_test`.
- Commit: `feat(board): hide idle projects in the project column`.
