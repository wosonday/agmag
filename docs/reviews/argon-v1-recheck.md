# Review của gemini-4-argon-high (agy) — agmag v1

## G1 an toàn (actions, policy)

Tệp: src/actions.rs, src/policy.rs · token: {'input_tokens': 17999, 'output_tokens': 9083, 'thinking_tokens': 7656, 'cache_read_tokens': 0, 'total_tokens': 27082}

### Critical

1. **`src/actions.rs:44-56` — Agent vượt chốt bảo vệ để kill/interrupt Codex/Claude (vi phạm Quy tắc 1)**
- **Kịch bản:** Trên Windows 11, mọi terminal của `herdr-core` chạy trong ConPTY nên `tty == true`. Một agent (Codex/OpenCode) chỉ cần xóa biến môi trường `HC_TERMINAL_ID` (`Remove-Item Env:HC_TERMINAL_ID`) rồi gọi `agm kill --yes <codex-term>`: `hc_terminal` là `None` → `is_agent = false` → được phân loại là `Caller::User { confirmed: true }` và kill được Codex/Claude. Ngoài ra, dòng 51-52 không fallback sang `store.terminal(t)` để kiểm tra `reported` hoặc `role`/`template` khi `hc get` thiếu `reported`/`app`.
- **Cách sửa:** Không cho `--yes` bỏ qua xác nhận tương tác khi đích là `PROTECTED_APPS`; kiểm tra cây tiến trình cha (PID) hoặc bắt buộc đối chiếu `reported`/`role` từ cả `hc get` lẫn `store`.

2. **`src/policy.rs:130-137` & `src/actions.rs:193` — Thiếu hoàn toàn cơ chế xoay vòng thợ OpenCode ở 70% ngữ cảnh (vi phạm đặc tả & Quy tắc 4)**
- **Kịch bản:** Dù doc-comment ở `policy.rs:1,130` và tham số `replacing` của `spawn_precheck` (`actions.rs:157`) mô tả việc xoay vòng thợ ở 70%, `policy::tick()` chỉ gọi `budgets_tick` và `context_tick`. Khi thợ OpenCode đạt ≥70% ngữ cảnh, hệ thống không hề yêu cầu bàn giao, không kiểm tra commit `.agents/log/<ID>.md` + worktree sạch, và không xoay vòng.
- **Cách sửa:** Bổ sung `rotate_tick` vào `policy::tick`: kiểm tra `pct >= cfg.policy.ctx_rotate` cho thợ OpenCode, gửi yêu cầu bàn giao, xác minh commit `.agents/log/<ID>.md` và `git status --porcelain` sạch trước khi `kill` và `spawn` lại (truyền `replacing = Some(&t.address)`).

### Important

3. **`src/policy.rs:19-27, 41-45` — Trả về `0` thay vì `None` khi chưa có dữ liệu token (vi phạm Quy tắc 3) và mất lịch sử phiên**
- **Kịch bản:** Khi đầu việc mới tạo hoặc chưa có bản ghi `usage`, `COALESCE(SUM(...), 0)` khiến `budget_value` luôn trả `Some(0.0)` thay vì `None`. Ngoài ra, truy vấn chỉ lấy `terminals.session` hiện tại; khi terminal đổi phiên hoặc bị xóa, token của các phiên cũ thuộc task bị mất khỏi tổng ngân sách.
- **Cách sửa:** Dùng `SUM(...)` trả `Option<i64>` (không `COALESCE`) để trả `Ok(None)` khi chưa có dữ liệu; gắn `task` trực tiếp vào bảng `sessions`/`usage` thay vì chỉ qua `terminals.session` hiện tại.

4. **`src/policy.rs:54-63` & `src/actions.rs:174` — `spawn_blocked_by_budget` bỏ qua ngân sách `provider`**
- **Kịch bản:** Khi ngân sách `provider:<p>:<window>` đã vượt ngưỡng cứng (`hard`), `spawn_blocked_by_budget` chỉ lọc `scope == "task:<task>"` nên vẫn cho phép `spawn` thêm thợ mới tiêu thụ hạn mức provider đó.
- **Cách sửa:** Kiểm tra thêm các ngân sách `provider:*` tương ứng với template/model sắp `spawn`.

5. **`src/policy.rs:76-127` — Gộp nhầm khóa hộp thư ngân sách và lặp vô hạn sau `dismiss`**
- **Kịch bản:** `notify` gọi `open_inbox(None, "budget", Some(level), ...)`. Nếu `open_inbox` chống trùng theo `(terminal=None, source="budget", kind=level)`, việc một ngân sách vượt ngưỡng sẽ chặn thông báo/hành động `kill` của mọi ngân sách khác cùng mức `hard`/`soft`. Ngược lại, khi người dùng `agm done` đóng cảnh báo `budget`/`context` lúc số liệu vẫn vượt ngưỡng, vòng `tick` kế tiếp sẽ mở lại cảnh báo và bắn lại lệnh `kill`/`LỖI`.
- **Cách sửa:** Lưu trạng thái đã kích hoạt (`triggered_level` / `warned_session`) theo từng `scope` ngân sách và từng `session` thay vì dựa vào trạng thái mở của `inbox`.

6. **`src/actions.rs:133` — `reply` không gửi phím Enter khi trả lời câu hỏi của agent**
- **Kịch bản:** `hc.send(term, text, false, ...)` truyền tham số thứ 3 (`enter`/`submit`) là `false` (khác với `send` ở dòng 107 truyền `true`). Khi trả lời mục `message`, chuỗi chỉ được gõ vào bộ đệm terminal mà không nhấn Enter, khiến agent treo chờ mãi dù mục hộp thư đã đóng.
- **Cách sửa:** Truyền `true` cho tham số submit khi trả lời (hoặc ít nhất với `item.source == "message"`).

### Minor

7. **`src/actions.rs:180-183, 202-216` — Trùng tên terminal, thiếu `row.name` và rò rỉ tiến trình khi lỗi DB**
- **Kịch bản:** Nếu `hc.run` trả JSON không kèm `name`, `row.name` bị `None` vì thiếu `row.name.get_or_insert(name)`, làm lần `spawn` sau tính `used = 0` và đặt trùng tên `w-<task>-1`. Đồng thời nếu `log_action`/`upsert_terminal` lỗi sau khi `hc.run` thành công, tiến trình nền bị bỏ rơi không được quản lý.
- **Cách sửa:** Gán `row.name.get_or_insert(name)` và gọi `hc.kill` dọn dẹp nếu ghi DB sau `hc.run` thất bại.


## G2 xoay vòng (rotation, git, hc)

Tệp: src/rotation.rs, src/git.rs, src/hc.rs · token: {'input_tokens': 18369, 'output_tokens': 9287, 'thinking_tokens': 7955, 'cache_read_tokens': 0, 'total_tokens': 27656}

### Critical

1. **`src/rotation.rs:198-209` — Lỗi DB sau khi `kill` làm mất thợ hoặc báo sai trạng thái**
   - **Kịch bản**: Trong `verify_and_respawn`, sau khi `actions::kill` (dòng 198) đã dừng thợ cũ, nếu `store.finish_terminal` (dòng 202) lỗi (ví dụ `SQLITE_BUSY` khi tranh chấp WAL với CLI), toán tử `?` thoát hàm trước khi gọi `actions::spawn` — thợ cũ chết nhưng không có thợ mới thay thế. Hoặc nếu `spawn` (dòng 203) thành công nhưng `UPDATE new_terminal` / `set_state("done")` (dòng 205-206) lỗi, bản ghi vẫn ở `'requested'`. Ở `tick` kế tiếp, nhánh `t.state != "live"` (dòng 125) gọi `fail()` và gửi hộp thư báo *"Phiên cũ được giữ nguyên"* dù phiên cũ đã bị kill.
   - **Cách sửa**: Sau khi `kill` thành công, không dùng `?` làm đứt luồng trước `spawn`; nếu lỗi ở bước hậu kiểm/spawn thì gọi `fail_with` báo rõ phiên cũ đã dừng. Gộp cập nhật `new_terminal` và `state = 'done'` vào một lệnh `UPDATE` duy nhất.

---

### Important

2. **`src/rotation.rs:41-43, 148-166` — Gửi đè lệnh khi thợ đang bận và đánh `failed` sớm**
   - **Kịch bản**:
     1. `idle()` nhận cả `"done"`, nhưng dòng 148 đặt `if_idle = t.status.as_deref() == Some("idle")`. Khi `status == "done"`, `hc.send` chạy với `if_idle = false`; nếu thợ vừa sang lượt mới mà DB chưa kịp nhận sự kiện `hc`, tin bàn giao bị chèn thẳng vào lượt đang chạy.
     2. Ở nhánh `"requested"` (dòng 162), ngay khi `t.turn > r.turn_before && idle(&t)`, hàm gọi `verify_and_respawn`. Nếu thợ cần hơn 1 lượt để hoàn tất bàn giao + commit (hoặc `turn_before` trong DB trễ 1 lượt), `verify_and_respawn` sẽ gọi `fail()` ngay ở lượt đầu tiên, khóa luôn xoay vòng tự động về sau (vì `start` dòng 89 chặn trạng thái `'failed'`).
   - **Cách sửa**: Luôn truyền `if_idle = true` khi gửi bàn giao tự động. Trong `verify_and_respawn`, nếu chưa thấy commit/worktree sạch và chưa quá `timeout` thì giữ `'requested'` chờ tiếp thay vì `fail()` ngay.

3. **`src/git.rs:30-43` & `src/rotation.rs:179-184` — Sai đường dẫn git khi `cwd` là thư mục con & nuốt lỗi `index.lock`**
   - **Kịch bản**: `is_clean` dùng `"."`, `":(exclude).agents/tasks"` và `touched_since` dùng `-- .agents/log/{task}.md` với `git -C <cwd>`. Khi `cwd` là thư mục con của repo:
     - `is_clean` bỏ sót thay đổi chưa commit ở ngoài `cwd` trong cùng worktree (nguy cơ mất dữ liệu khi kill).
     - `touched_since` tìm `<cwd>/.agents/log/{task}.md` thay vì gốc repo nên luôn trả `None`, làm bàn giao thất bại.
     - `unwrap_or(false/None)` ở `rotation.rs:179, 182` biến lỗi chạy `git` tạm thời (như `.git/index.lock` trên Windows) thành `failed` vĩnh viễn.
   - **Cách sửa**: Thêm tiền tố `:(top)` cho mọi pathspec (`":(top)"`, `":(top,exclude).agents/tasks"`, `format!(":(top).agents/log/{task}.md")`). Tách riêng lỗi `Err` của `git` để thử lại ở `tick` sau.

4. **`src/hc.rs:128-156` — Chuỗi rỗng làm mất nhận diện `app` (ảnh hưởng nhận diện OpenCode/Claude/Codex)**
   - **Kịch bản**: `app_from_argv(&["".into()])` trả về `Some("")` thay vì `None`. Trong `terminal_row_from` (dòng 149-155), nếu `v["status"]["app"]` hoặc `v["agent"]` là `""`, `reported_app` thành `Some("")` và bỏ qua `.or_else(|| app_from_argv(&argv))`, khiến `TerminalRow.app` mang giá trị `Some("")`.
   - **Cách sửa**: Dùng `.filter(|s| !s.trim().is_empty())` khi đọc `status.app`, `agent` và đầu hàm `app_from_argv`.

---

### Minor

5. **`src/rotation.rs:88-104` & `src/hc.rs:55` — Race khi `start()` và bắt nhầm `"error": null`**
   - **Kịch bản**: `start()` kiểm tra `SELECT EXISTS` rồi mới `INSERT` ngoài transaction, nên daemon và CLI `agm rotate` chạy song song có thể tạo 2 vòng xoay cho cùng một terminal. Ở `hc.rs:55`, `v.get("error")` khớp cả khi JSON có `"error": null`.
   - **Cách sửa**: Gộp thành `INSERT INTO rotations ... WHERE NOT EXISTS (...)` (hoặc tạo unique partial index); đổi dòng 55 thành `v.get("error").filter(|e| !e.is_null())`.


## G6 lệnh CLI (ops, tui)

Tệp: src/ops.rs, src/tui.rs · token: {'input_tokens': 17448, 'output_tokens': 7667, 'thinking_tokens': 6413, 'cache_read_tokens': 0, 'total_tokens': 25115}

### Critical

1. **`src/tui.rs:31-37, 61-71, 88-112` — Agent có thể qua mặt quyền người dùng qua stdin của TUI (vi phạm Quy tắc 1 & 4)**
   - **Kịch bản:** `tui::run` không từ chối chạy khi `who` là `Caller::Agent` (hoặc khi `stdin` bị pipe).
     - Ở dòng 63–70, khi `who` là `Caller::Agent`, TUI vẫn truyền `&user` (`Caller::Agent`) vào `actions::kill` / `actions::interrupt`. Agent chỉ cần chạy `printf "k <opencode>\ny\nq\n" | agm tui` là kill được thợ thông qua TUI (trái với cam kết ở dòng 30).
     - Ở dòng 88–112, các lệnh `Policy`, `Send`, `Reply`, `Done` không kiểm tra `who`. Agent có thể chạy `printf "p off\nq\n" | agm tui` để tự tắt `policy`, tự đóng/trả lời hộp thư người dùng (`r`, `d`), hoặc giả mạo tin nhắn từ `"user"` (dòng 95 hardcode `"user"`).
   - **Cách sửa:** Ngay sau dòng 37, nếu `!matches!(who, Caller::User { .. })` thì `bail!("chỉ người dùng trên TTY mới được mở TUI")`.

2. **`src/ops.rs:172-186` — `rotate` fail-open với terminal chưa rõ `app` và bỏ qua kiểm tra `caller`**
   - **Kịch bản:** Khác với `stop()` (dòng 159–161 chỉ coi `app == "opencode"` là thợ), `rotate()` ở dòng 174 chỉ chặn `Some("codex" | "claude-code")`. Nếu terminal của Codex/Claude chưa kịp gán `app` (`t.app == None`) hoặc là terminal khác, biến `protected` bằng `false`. Khi đó agent gọi `agm rotate <target>` sẽ vượt qua kiểm tra mà không gọi `caller()` hay xét `yes`, xếp terminal đó vào hàng đợi xoay vòng để daemon kill.
   - **Cách sửa:** Đổi điều kiện bảo vệ thành `let is_opencode = t.app.as_deref() == Some("opencode"); if !is_opencode { ... }` và kiểm tra `caller(store, yes, ...)` trước khi gọi `rotation::start`.

---

### Important

3. **`src/ops.rs:64-79` — Rò rỉ tiến trình con `hc events --json` khi `daemon_loop` thoát**
   - **Kịch bản:** Ở dòng 69–71, khi `rx` bị drop (daemon dừng hoặc lỗi), `tx.send(line).is_err()` khiến luồng đọc `return` ngay mà không `child.kill()` và `child.wait()`. Trên Windows, tiến trình `hc.exe` bị bỏ rơi chạy ngầm vô thời hạn.
   - **Cách sửa:** Gọi `let _ = child.kill(); let _ = child.wait();` trước khi `return`.

4. **`src/ops.rs:99-108` — Lỗi một bước trong `step()` làm liệt toàn bộ các bước định kỳ sau đó**
   - **Kịch bản:** Trong `daemon_loop`, `periodic`, `correlate::link_all`, `policy::tick`, và `rotation::tick` nối chuỗi bằng `?`. Nếu `periodic` hoặc `policy::tick` gặp lỗi tạm thời, `rotation::tick` bị bỏ qua hoàn toàn, làm kẹt các phiên đang xoay vòng.
   - **Cách sửa:** Thực thi độc lập từng bước và log lỗi riêng cho mỗi bước thay vì thoát sớm bằng `?`.

5. **`src/ops.rs:200` — `budget_set` chấp nhận `NaN` / số âm / `Infinity`**
   - **Kịch bản:** Với `soft = f64::NAN`, biểu thức `soft > hard` trả về `false`. Giá trị `NaN` lọt vào SQLite (thành `NULL`), gây lỗi kiểu dữ liệu khi đọc lại `store.budgets()` hoặc làm sai lệch logic ngân sách.
   - **Cách sửa:** Kiểm tra `if !soft.is_finite() || !hard.is_finite() || soft < 0.0 || soft > hard { bail!(...); }`.

---

### Minor

6. **`src/ops.rs:83` — Nguy cơ panic khi trừ `Duration` khỏi `Instant::now()` trên Windows**
   - **Kịch bản:** Trên Windows, `Instant` tính từ lúc boot máy. Nếu `interval` lớn hơn uptime hiện tại của máy, `Instant::now() - Duration::from_secs(interval)` sẽ panic do underflow.
   - **Cách sửa:** Dùng `checked_sub` hoặc dùng biến cờ `first_tick = true`.

7. **`src/tui.rs:41, 48-55` — Nuốt lỗi `scan(store)` và xóa màn hình khi đang gõ lệnh**
   - **Kịch bản:** `let _ = scan(store);` giấu lỗi quét log khiến số liệu TUI cũ mà không cảnh báo; đồng thời mỗi `refresh` giây `\x1b[2J` xóa mất dòng lệnh người dùng đang gõ dở trên terminal.
   - **Cách sửa:** Ghi lỗi của `scan(store)` vào `note` và chỉ vẽ lại hoặc tăng chu kỳ khi đang nhập lệnh.

