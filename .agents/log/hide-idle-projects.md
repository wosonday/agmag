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
