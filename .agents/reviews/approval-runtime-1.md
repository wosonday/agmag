model: agy/gemini-4-hydrogen-high (--mode plan)

Đối tượng: HEAD c849e34
Kết luận: CHƯA ĐẠT

### Bắt buộc sửa

1. **`src/actions.rs:is_hard_prohibited` và `src/config.rs:Policy::validate` — So sánh phân biệt hoa/thường với `HARD_PROHIBITED_ACTIONS` làm lọt hành động cấm cứng thành `allowed`**
   - `HARD_PROHIBITED_ACTIONS` chỉ chứa chữ thường (`"push"`, `"merge"`, ...), còn `is_hard_prohibited` (`HARD_PROHIBITED_ACTIONS.contains(&action)`) và `Policy::validate` (`HARD_PROHIBITED_ACTIONS.contains(&entry.trim())`) đều so sánh phân biệt hoa/thường.
   - *Kịch bản lỗi*:
     - Cấu hình `providers.toml` có `[policy] delegated_actions = ["PUSH"]` (hoặc `"Push"`): `Policy::validate` cho qua khi nạp config. Khi agent gọi `agm approval check PUSH` (hoặc MCP `agm_approval_check` với `"action": "PUSH"`), `decide` nhận `is_hard_prohibited("PUSH") == false`, khớp `delegated_actions` và trả `allowed` (`source: "global"`, mã thoát `0`) cho hành động cấm cứng.
     - Hoặc qua `agm approval add lane-a task-a PUSH`: `validate_new_grant` không chặn `"PUSH"` và lưu grant vào DB; khi kiểm `PUSH` với `--lane lane-a --task task-a`, `decide` trả `allowed` (`source: "grant:<id>"`, mã thoát `0`).
     - Kể cả khi không có whitelist hay grant, `agm approval check PUSH` trả `requires_human` (mã `5`) thay vì `hard_prohibited` (mã `6`).

2. **`src/main.rs:run` (`Cmd::Approval` — `ApprovalCmd::Add` và `ApprovalCmd::Revoke`) — Chỉ kiểm `std::io::stdin().is_terminal()`, bỏ qua kiểm tra danh tính caller (`HC_TERMINAL_ID` / `detect_caller`)**
   - Khác với `Cmd::Board` (`board::require_human`) hay cơ chế `detect_caller` / `classify_caller` trong `src/actions.rs` (`if tty && !terminal_is_agent`), nhánh `ApprovalCmd::Add` và `ApprovalCmd::Revoke` chỉ kiểm `!std::io::stdin().is_terminal()`.
   - *Kịch bản lỗi*: Agent chạy trong terminal được `hc run` cấp PTY (`stdin().is_terminal() == true`, đồng thời có `HC_TERMINAL_ID` hoặc terminal đã tự báo 7501). Agent gọi `agm approval add lane-a task-a spawn_worker` vượt qua được cổng TTY, tự ghi grant với `approved_by = "user"`, rồi gọi `agm approval check spawn_worker --lane lane-a --task task-a` và nhận `allowed` (mã `0`) mà không cần người dùng thật. Tương tự, agent trong PTY của `hc` cũng gọi được `agm approval revoke`.

3. **`src/main.rs:run` (`ApprovalCmd::Revoke`) và `src/store/v2.rs:Store::revoke_approval_grant` — `revoke` không trim đầu vào và bỏ qua số dòng cập nhật, báo thành công giả khi grant vẫn còn hiệu lực**
   - `ApprovalCmd::Add` trim `action` qua `validate_new_grant` trước khi lưu, nhưng `ApprovalCmd::Revoke` truyền thẳng `&lane, &task, &action` chưa trim vào `store.revoke_approval_grant`; đồng thời `revoke_approval_grant` bỏ qua số dòng bị `UPDATE` ảnh hưởng và luôn trả `Ok(())`.
   - *Kịch bản lỗi*: Người dùng đã tạo grant với `agm approval add lane-a task-a " spawn_worker "` (lưu trong DB là `"spawn_worker"`). Khi thu hồi bằng `agm approval revoke lane-a task-a " spawn_worker "` (hoặc gõ nhầm ký tự ở `lane`/`task`/`action`), câu `UPDATE ... WHERE lane = ?1 AND task = ?2 AND action = ?3 AND revoked_ms IS NULL` khớp `0` dòng nhưng vẫn ghi log `approval_revoke` (`ok = true`), in `Approval revoked.` / `{"revoked": true}` và thoát `0`. Grant vẫn còn hiệu lực trong DB nên agent kiểm `spawn_worker` vẫn nhận `allowed` (mã `0`).

4. **`src/actions.rs:decide` / `src/actions.rs:validate_new_grant` vs `src/config.rs:Policy::validate` — Bất đồng nhất về trim trên `delegated_actions` và `lane`/`task`**
   - `Policy::validate` dùng `entry.trim()` khi kiểm tra rỗng và cấm cứng, nhưng `decide` không trim phần tử trong `cfg.policy.delegated_actions` khi so `a == action` (dù `action` đầu vào đã được trim). Ngoài ra `validate_new_grant` kiểm `lane.trim().is_empty()` và `task.trim().is_empty()` nhưng chỉ trả lại `action` đã trim, nên `ApprovalCmd::Add` lưu `lane`/`task` nguyên khoảng trắng, trong khi `decide` cũng không trim `lane`/`task` trước khi truy vấn `store.approval_grant`.
   - *Kịch bản lỗi*:
     - `providers.toml` có `delegated_actions = [" continue_review "]`: `Config::load` hợp lệ, nhưng `decide(..., "continue_review", ...)` so `" continue_review " == "continue_review"` sai → trả `requires_human` thay vì `allowed` (và không một đầu vào nào có thể khớp vì `action` luôn bị trim trước).
     - `agm approval add " lane-a " " task-a " spawn_worker` lưu `lane = " lane-a "`, `task = " task-a "`; sau đó `agm approval check spawn_worker --lane lane-a --task task-a` không khớp được grant vừa tạo.

5. **`src/store/v2.rs:Store::upsert_approval_grant` — Luôn `INSERT` dòng mới nên grant cũ không thời hạn sống lại khi grant mới hết hạn**
   - Dù tên là `upsert_approval_grant`, hàm luôn thực hiện `INSERT INTO approval_grants ...`, còn `Store::approval_grant` lọc `AND (expires_ms IS NULL OR expires_ms > ?4) ORDER BY approved_ms DESC, id DESC LIMIT 1`.
   - *Kịch bản lỗi*: Người dùng từng tạo grant `(lane-a, task-a, spawn_worker)` không giới hạn thời gian (`expires_ms = None`), rồi chạy lại `agm approval add lane-a task-a spawn_worker --expires-ms <T>` để thu hẹp thời hạn. Khi `now >= T`, dòng mới bị mệnh đề `expires_ms > ?4` loại bỏ và `approval_grant` rơi xuống chọn lại dòng cũ có `expires_ms IS NULL`, tiếp tục trả `allowed` thay vì `requires_human`.

---

### Nên sửa

1. **`src/main.rs:run` (`ApprovalCmd::Check`) và `README.md` / `README_vi.md` — `check` ghi bảng `actions` với `?` sau khi đã in kết quả**
   - Tài liệu (`README.md`, `README_vi.md`) ghi `agm approval check` là `read-only` / `chỉ đọc`, nhưng nhánh `ApprovalCmd::Check` gọi `store.log_action(...)?` sau `emit(...)` và trước khi trả `ApprovalRefused`.
   - *Kịch bản lỗi*: Khi chạy `agm approval check` trên DB chỉ đọc hoặc bị khóa ghi, lệnh đã in JSON (ví dụ `"decision": "hard_prohibited"` hoặc `"allowed"`) ra stdout nhưng `log_action?` thất bại khiến tiến trình thoát mã `1` thay vì `6`/`5` hoặc `0`, khác với MCP `agm_approval_check` vốn không ghi `actions`.

2. **`src/actions.rs:exit_code` — Ánh xạ `ApprovalDecision::Allowed => 0` trong hàm xử lý lỗi**
   - Trong `exit_code(err: &anyhow::Error)`, nhánh `ApprovalRefused` có `ApprovalDecision::Allowed => 0`. Hiện `main.rs` chỉ trả `ApprovalRefused` khi `decision != Allowed`, nhưng `ApprovalRefused.decision` là `pub`; nếu bất kỳ chỗ nào bọc nhầm `ApprovalDecision::Allowed` vào `ApprovalRefused`, một giá trị `Err` sẽ khiến CLI thoát mã `0`. Nên ánh xạ `Allowed` về khác `0` (vd `1`) hoặc tách kiểu chỉ chứa 2 trạng thái từ chối.

3. **`src/main.rs:run` (`Cmd::Approval::*`) — Nuốt lỗi từ `emit(...)`**
   - Cả bốn nhánh `Add`, `Ls`, `Revoke`, `Check` đều viết `let _ = emit(...);`, khác với `Cmd::Policy`, `Cmd::Ask`, `Cmd::Ls`, `Cmd::Cost` trả trực tiếp `emit(...)`.
   - *Kịch bản lỗi*: Nếu ghi stdout hoặc tuần tự hóa JSON lỗi ở `Add`/`Ls`/`Revoke` hoặc `Check` (nhánh `allowed`), lỗi bị bỏ qua và lệnh vẫn thoát `0`.

4. **`tests/store_v2_test.rs` — Chưa có test migration trực tiếp từ DB schema v4 lên v5**
   - `code.diff` cập nhật `assert_eq!(v, 4)` thành `5` trong các test cũ (`migrates_v1_to_v2`, `migrates_v2_to_v3_idempotently`, `opening_migrated_db_does_not_wait_for_write_lock`), nhưng chưa có test khởi tạo DB đúng schema v4 (chưa có `approval_grants` và index `approval_grants_lookup`, có sẵn dữ liệu v4) rồi mở lại để xác nhận nâng cấp v4 → v5 nguyên vẹn. (Đọc tĩnh `src/store/mod.rs` và `src/store/schema_v5.sql` cho thấy `if version < 5` dùng `CREATE TABLE IF NOT EXISTS` và `CREATE INDEX IF NOT EXISTS` là nhất quán.)

---

### Ghi chú

1. **`src/main.rs:run` (`ApprovalCmd::Add`) — Đơn vị và kiểm tra biên của `--expires-ms`**: `now` ở `main.rs` và `(ctx.clock)()` ở `src/mcp.rs` đều thống nhất dùng millisecond (khớp `expires_ms > ?4` trong `src/store/v2.rs`). Tuy nhiên `--expires-ms` là mốc thời gian tuyệt đối (epoch ms) chứ không phải TTL tương đối và không được giải thích trên help CLI; nếu người dùng truyền giá trị nhỏ như `--expires-ms 60000` hoặc `<= 0`, grant bị coi là đã hết hạn ngay lúc tạo mà không có thông báo lỗi.
2. **`src/main.rs:run` (`ApprovalCmd::Ls`) — Định dạng TTY**: Khi không có `--json`, `ApprovalCmd::Ls` dùng `println!("{:?}", grant)` in debug struct thô.
3. **Khớp JSON CLI/MCP và tài liệu (`src/actions.rs`, `src/mcp.rs`, `docs.diff`)**: `agm approval check` và MCP `agm_approval_check` dùng chung `actions::decide` và `Decision::to_json` nên cấu trúc JSON (`decision`, `action`, `lane`, `task`, `source`), yêu cầu có đủ `lane` và `task` để dùng grant, từ chối tham số lạ ở MCP, và việc MCP không có tool tạo/thu hồi grant đều khớp `brief.md`. Các tài liệu (`BOT-CEO.md` §2.1, `AGENTS-TUTORIAL.md`, `HERDR-CORE-BOT.md`, `README.md`, `README_vi.md`, `docs/AGM-0.4.0-GUIDE.md`, `CHANGELOG.md`) đều đã cập nhật số tool MCP `8`, quy tắc quyết định và mã thoát `0`/`5`/`6` khớp mã nguồn (ngoại trừ điểm `check` còn ghi log `actions` đã nêu ở trên).
