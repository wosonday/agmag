- Đối tượng: HEAD `070a3e24930a` (phần `d` từ `3cd6762` và phần `e` từ `36ac084` / `32b6481` trên `feat/tui-project`)

## Đối chiếu Thiết kế & Các điểm cần soi (d + e)

1. **Cách tách lệnh `cd`/`show` (`src/dashboard.rs:1871-1930`):** **Đạt.** `parse_command` tách `head`/`rest` theo khoảng trắng đầu tiên, khớp lệnh qua `head == *name || (!alias.is_empty() && head == *alias)` nên đầu vào rỗng không khớp nhầm vào `cd`/`show` (có `alias = ""`), `cd` / `cd  shop ` / `cd ..` / `cd /` cho đúng `Cd(None)` / `Cd(Some(...))`, và `show 43` / `show #43` trả `Show(43)` còn `show` / `show x` / `show #` báo lỗi `usage: show <#>`.
2. **Footer khi lệnh không có alias (`src/dashboard.rs:969-1018, 1719-1731`):** **Đạt.** `TUI_COMMANDS` đặt `("cd", "", "[project|..]")` và `("show", "", "<#>")` sau `view`; `render_footer` chỉ dùng `(k, _, args)` nên không sinh dấu ngăn thừa `"·  ·"`, được kiểm chứng bởi `footer_lists_cd_and_show`.
3. **Agent đã thoát chỉ được giữ khi còn mục inbox mở (`src/project.rs:421-431`):** **Đạt về logic chính** (terminal `live` luôn giữ, `exited` chỉ giữ khi có mục inbox chưa xử lý khớp `address` hoặc `name`). Còn 1 điểm nên siết khi `t.name == Some("")` (xem *Nên sửa 1*).
4. **Khớp terminal theo địa chỉ hoặc theo tên (`src/project.rs:426-428, 433-445`):** **Chưa đạt hoàn toàn ở phần `messages`.** Bộ lọc `kept` và `inbox` khớp cả `address` lẫn `name`, nhưng `messages_involving` lại truyền `parties(&kept)` thay vì toàn bộ thành viên dự án nên bỏ sót tin nhắn của agent đã thoát không có mục inbox mở (xem *Blocker 2*).
5. **`stage` đọc từ `BRANCHES.md` (`src/project.rs:447, 457-464`):** **Đạt về chức năng chính** (`branch_entry_for` đổi `<lane>--<slug>` từ `t.cwd` hoặc `t.task` sang `<lane>/<slug>`, đọc an toàn qua `branches::get(&branches_file, &entry).ok().flatten()`, thiếu tệp/mục trả `None`). Còn 1 điểm nhỏ không fallback sang `view.cwd` nếu `t.cwd` là `None` (xem *Nên sửa 2*).
6. **Không bịa số liệu (`src/project.rs:451-485`):** **Đạt ở số liệu/trạng thái chưa biết** (`exited` đặt `model=""`, `effort=""`, `last_msg=None`, `context_pct=None`, `tokens=None`; thiếu `session` cho `doing=None`, `spark=None`; thiếu `t.status` cho `"-"`). Tuy nhiên trường `status` đang sao chép thô `t.status` chữ thường nên gãy hiển thị trạng thái `blocked` bên renderer (xem *Blocker 1*).

---

## Task 4 (`d`): `src/dashboard.rs`, `src/tui.rs` & `tests/dashboard_test.rs`

- **Đánh giá:** Gọn, đúng phạm vi, không có Blocker. Nhánh tạm `Ok(TuiCmd::Cd(_) | TuiCmd::Show(_))` tại `src/tui.rs:477-479` giúp biên dịch sạch trong lúc chờ Task 5b (`g`) nối vòng lặp TUI.
- Điểm nhỏ: `show` dùng `.parse::<i64>()` nên chấp nhận số `<= 0` (`show 0`, `show -1`), tương tự `reply` (`id.parse()`), không gây lỗi (khi tra cứu sẽ báo không thấy) nhưng có thể siết `> 0` nếu muốn.

---

## Task 5a (`e`): `src/project.rs` (`compute_roots`, `load_project_view`) & `tests/project_test.rs`

### Blockers

1. **`src/project.rs:475` — `AgentView.status` lấy thô `t.status` chữ thường, làm gãy nhận diện `BLOCKED` và mất `kind` / `[HANDED OFF]` trong `project_render`**
   - **Hậu quả:** Trong CSDL (`terminals.status`), trạng thái bị chặn được lưu là `"blocked"` (chữ thường), còn `dashboard::snapshot` (`src/dashboard.rs:124-132`) định dạng thành `"BLOCKED"` / `"BLOCKED (<kind>)"` và nối `"[HANDED OFF]"`. Cả `project_render::status_parts` (`src/project_render.rs:164`) và `dashboard::status_color` (`src/dashboard.rs:296`) đều kiểm `status.starts_with("BLOCKED")` (chữ hoa). Do `load_project_view` gán `t.status.clone().unwrap_or_else(|| "-".into())`, mọi agent đang bị chặn sẽ bị vẽ thành **`? blocked` màu trắng** thay vì **`◐ blocked` màu đỏ (`BRIGHT_RED`)**, đồng thời mất cờ `[HANDED OFF]`.
   - **Cách sửa tối thiểu:** Với agent `live` có `view: Some(v)`, lấy `v.status.clone()`; với agent `exited` (hoặc thiếu `view`), nếu `t.status.as_deref() == Some("blocked")` thì trả `"BLOCKED"` (hoặc tách một helper `format_terminal_status(t)` dùng chung với `src/dashboard.rs:124-132`, vẫn giữ `None` → `"-"` để không làm vỡ test `a3.status == "-"`).

2. **`src/project.rs:433, 437` — `messages_involving` lọc theo `parties(&kept)` nên bỏ sót tin nhắn của các agent đã thoát không có mục inbox mở**
   - **Hậu quả:** `kept` chỉ gồm agent `live` và agent `exited` còn mục inbox mở. Theo đặc tả (`docs/superpowers/specs/2026-11-23-tui-project-scope-design.md:121-123`), mục `MESSAGES` phải lấy 20 tin nhắn mới nhất liên quan tới terminal của dự án (**bao gồm cả terminal đã thoát**). Nếu một worker của dự án hoàn tất việc, gửi `BÁO CÁO` (ví dụ tới `user` hoặc tới một agent khác đã thoát) rồi thoát mà không mở inbox, tin nhắn đó bị loại khỏi `MESSAGES` của dự án.
   - **Cách sửa tối thiểu:** Tách danh sách tất cả thành viên của dự án để lọc `messages`:
     ```rust
     let all_members = members(&terms, roots, key);
     let all_ps = parties(&all_members);
     ...
     let messages = store.messages_involving(&all_ps, 20)?;
     ```
     (Giữ nguyên `kept` cho `AGENTS` và `ps = parties(&kept)` cho `inbox`).

---

## Nên sửa

1. **`src/project.rs:427` — Lọc bỏ tên rỗng `Some("")` khi kiểm mục inbox cho terminal đã thoát:**
   - Dòng 427 viết `t.name.as_deref() == Some(term)` chưa có `.filter(|n| !n.is_empty())` như `c48804f` đã sửa cho `list_projects`, `parties` và `tree_order`. Nên đổi thành `t.name.as_deref().filter(|n| !n.is_empty()) == Some(term)`.
2. **`src/project.rs:457` — `branch_entry_for` nên fallback sang `view.cwd` khi `t.cwd` là `None`:**
   - `compute_roots` (`src/project.rs:395-402`) lấy `t.cwd` rồi fallback sang `TermView.cwd`, nhưng `load_project_view` chỉ truyền `t.cwd.as_deref()` vào `branch_entry_for`. Nếu terminal chỉ có `session_cwd` trong worktree `.wt/<lane>--<slug>`, `stage` sẽ bị `None`.

---

## Nhỏ

1. **`src/project.rs:436` — Tránh trừ tràn dưới với `now_ms`:** Dùng `now_ms.saturating_sub(3_600_000)` thay cho `now_ms - 3_600_000`.
2. **`src/dashboard.rs:1922-1927` — `show` nhận số `<= 0`:** Có thể kiểm `id > 0` nếu muốn báo `usage: show <#>` sớm cho `show 0` / `show -1`.

---

## Kiểm chứng chạy trên `070a3e24930a`

- `cargo fmt --check`: Đạt (exit 0)
- `cargo clippy --all-targets -- -D warnings`: Đạt (exit 0)
- `cargo test --test dashboard_test --test project_test --quiet`: Đạt (`dashboard_test`: 27 passed, 0 failed; `project_test`: 11 passed, 0 failed)

## KẾT LUẬN: CHƯA ĐẠT (2 Blocker tích hợp ở phần `e`: `status` chữ thường làm hỏng hiển thị `blocked` và `messages` thiếu agent đã thoát; phần `d` ĐẠT)
