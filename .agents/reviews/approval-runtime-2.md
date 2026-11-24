model: agy/gemini-4-hydrogen-high (--mode plan)

Đối tượng: HEAD 58202e7
Kết luận: ĐẠT

### Bắt buộc sửa

Không có.

---

### Nên sửa

1. **`src/actions.rs:grant_human_line` (`L162-182`) và `tests/actions_test.rs` (`fix.diff:661-668`) — Nhãn trạng thái khi `agm approval ls --all` trên TTY gắn `"active"` cho grant đã hết hạn chưa thu hồi**
   - `grant_human_line` chỉ kiểm `if g.revoked_ms.is_some() { "revoked" } else { "active" }` mà không nhận `now: i64` để đối chiếu `g.expires_ms`. Trong `tests/actions_test.rs`, biến `let expired` lại gán cả `revoked_ms: Some(999)` nên bỏ sót nhánh `expires_ms <= now` và `revoked_ms == None`.
   - Khi chạy `agm approval ls --all` ở chế độ người đọc, một grant đã quá hạn nhưng chưa revoke vẫn in `active` (dù vẫn kèm `expires=<ms>`). Không ảnh hưởng quyết định quyền (`decide` và `approval_grants(true, now)` đều lọc `expires_ms > ?`) hay `--json`, nhưng nên truyền `now` vào `grant_human_line` để phân biệt `expired` với `active`.

2. **Các điểm nhỏ về tính nhất quán và thứ tự kiểm tra (không ảnh hưởng an toàn)**
   - `src/main.rs` (`ApprovalCmd::Add`): `validate_expires_ms(expires_ms, now)?` chạy trước `require_human(...)`, nên agent gọi `agm approval add ... --expires-ms 60000` nhận lỗi `--expires-ms` thay vì lỗi từ chối agent (vẫn thoát mã `1` và không ghi DB; test `approval_add_rejects_past_absolute_expiry` hiện cũng dựa vào thứ tự này vì chạy không TTY).
   - `src/main.rs` (`emit`, `fix.diff:444-456`): nhánh `json` dùng `writeln!(std::io::stdout().lock(), ...)?` để trả lỗi ghi stdout, còn nhánh `human()` vẫn gọi `println!` (panic với mã khác `0` khi stdout lỗi thay vì trả `Err`).
   - `BOT-CEO.md` (`fix.diff:11`): đoạn mô tả vẫn giữ cụm `(agm approval add|ls|revoke)` trong khi `ApprovalCmd::Ls` cố ý cho phép chạy không cần TTY (đồng bộ với `README.md` và `README_vi.md`).

---

### Lỗi phát sinh mới

Không phát hiện lỗi an toàn hoặc hồi quy nghiêm trọng trong `fix.diff`. (Lỗi hiển thị `"active"` cho grant đã hết hạn ở `grant_human_line` đã ghi nhận tại mục `### Nên sửa` #1.)

**Đánh giá chi tiết các trọng tâm an toàn trên `58202e7`:**

- **Chuẩn hóa đầu vào (kể cả khoảng trắng và chữ hoa/thường ngoài ASCII):**
  - `normalize_action` (`s.trim().to_ascii_lowercase()`) và `normalize_key` (`s.trim().to_string()`) được áp dụng đồng nhất ở cả ghi và đọc: `Policy::validate` (`src/config.rs:197`), `decide` (`src/actions.rs:100-117`), `validate_new_grant` (`src/actions.rs:143-159`), `revoke_grant` (`src/actions.rs:198-203`), `ApprovalCmd::{Add, Revoke, Check}` (`src/main.rs`), và MCP `agm_approval_check` (`src/mcp.rs:260-267`).
  - Khoảng trắng Unicode (NBSP, U+3000, `\n`, `\t`, ...) đều bị `str::trim()` cắt giống nhau ở mọi nơi.
  - Với chữ ngoài ASCII (như chữ full-width `ＰＵＳＨ`, `İ` Thổ Nhĩ Kỳ, hay ký tự homoglyph như `K`): `to_ascii_lowercase()` giữ nguyên các ký tự ngoài ASCII nên đúng là một biến thể như `"ＰＵＳＨ"` hoặc `"Kill_claude"` có thể lọt qua `HARD_PROHIBITED_ACTIONS` khi cấu hình `delegated_actions` hoặc `add`. Tuy nhiên đây **không phải đường vượt rào thật**: vì cả lúc lưu/nạp cấu hình lẫn lúc `decide` kiểm tra đều dùng chính `normalize_action`, `"ＰＵＳＨ"` hay `"Kill_claude"` **không bao giờ** khớp khi agent kiểm tra hành động chuẩn `"push"` / `"kill_claude"` (ngược lại, nếu dùng Unicode `to_lowercase()` ở một phía và không dùng ở phía kia mới phát sinh lệch chuẩn). Chỉ khi cả config lẫn agent đều chủ ý dùng cùng chuỗi homoglyph phi chuẩn thì mới khớp nhau, tương đương đặt một tên action tùy biến khác hoàn toàn. Vì vậy `trim() + to_ascii_lowercase()` đồng nhất ở mọi điểm là đủ.

- **`require_human` và đối chiếu với `agm board`:**
  - `ApprovalCmd::Add` và `ApprovalCmd::Revoke` gọi trực tiếp `actions::require_human` (`src/actions.rs:333-347`) với `tty = stdin().is_terminal() && stdout().is_terminal()` và `ops::hc_terminal()` — **giống hệt** `board::require_human` (`src/board/mod.rs:260-277`).
  - Agent chạy trong PTY của `hc` (có `HC_TERMINAL_ID`, app thuộc `AGENT_APPS` hoặc `reported = true`, không ở trạng thái `idle_shell_foreground`) bị từ chối; `HC_TERMINAL_ID` không rõ với `hc` (`hc.get` trả `Err`) cũng fail-closed thành `Caller::Agent` và bị từ chối.
  - Giới hạn cố hữu dùng chung với `board`: (1) tiến trình không có `HC_TERMINAL_ID` (`None`) nhưng tự cấp PTY (`tty == true`) vẫn được `detect_caller` xếp vào `Caller::User` (vì người dùng ngoài `hc` cũng có đặc điểm này); (2) một terminal do `hc` quản lý nhưng `hc get` trả `Ok` với app không nằm trong `AGENT_APPS`, chưa tự báo 7501 (`reported = false`), không phải shell rỗi và có PTY cũng đi qua nhánh `is_agent = false`. Đây là ranh giới chung của `detect_caller`, không phải lỗi riêng của `approval`.

- **Transaction trong `Store::add_approval_grant` (`src/store/v2.rs:562-586`):**
  - Ngữ nghĩa thu hồi trước rồi mới chèn (`UPDATE ... WHERE lane = ?1 AND task = ?2 AND action = ?3 AND revoked_ms IS NULL` trước `INSERT`) ngăn được grant cũ không thời hạn sống lại khi grant mới hết hạn. (Lưu ý nhỏ: câu `WHERE lane = ?1 AND task = ?2` dùng `=` thay vì `IS`, nên chỉ an toàn với `lane`/`task` khác `None`; hiện mọi đường gọi từ CLI đều đi qua `validate_new_grant` nên luôn là `Some`).
  - Giá trị `revoked_ms = grant.approved_ms` nhất quán với mốc tạo grant thay thế và khớp truy vấn `revoked_ms IS NULL` ở `approval_grant` / `approval_grants`.
  - `unchecked_transaction()`: an toàn trong ngữ cảnh hiện tại vì được gọi độc lập trên `&Store`, câu lệnh đầu tiên là `UPDATE` (lấy khóa ghi ngay trong `BEGIN DEFERRED`, tránh bẫy nâng cấp khóa đọc→ghi gây `SQLITE_BUSY`), và nếu `INSERT` lỗi thì `Drop` của `tx` tự rollback `UPDATE`. Chỉ lưu ý rằng khác với `resolve_inbox_many` (`SAVEPOINT`, `src/store/v2.rs:452`), `unchecked_transaction()` sẽ lỗi nếu sau này bị gọi lồng trong một transaction khác trên cùng `Connection`.

- **Mã thoát và tính chỉ-đọc của `check`:**
  - `ApprovalCmd::Check` không gọi `store.log_action` (cả khi `Allowed`, `RequiresHuman`, `HardProhibited`, hay lỗi); MCP `agm_approval_check` cũng chỉ đọc.
  - `emit` ở cả 4 nhánh `Add`, `Ls`, `Revoke`, `Check` đều truyền lỗi bằng `?`; `exit_code` ánh xạ `ApprovalRefused` với `Allowed => 1` (chặn thoát `0` nếu bọc nhầm), `RequiresHuman => 5`, `HardProhibited => 6`, lỗi khác `1`.

---

### Đối chiếu vòng 1

**Bắt buộc sửa (vòng 1):**
1. **So sánh phân biệt hoa/thường với `HARD_PROHIBITED_ACTIONS` (`is_hard_prohibited`, `Policy::validate`)** — **Đã sửa** (chấp nhận được). `normalize_action` được dùng ở cả `is_hard_prohibited`, `Policy::validate`, `decide`, `validate_new_grant`, `revoke_grant`; có unit test và CLI test cho `PUSH`, `Push`, ` push\n`, `MERGE`.
2. **`ApprovalCmd::Add` và `ApprovalCmd::Revoke` chỉ kiểm `stdin().is_terminal()`** — **Đã sửa** (chấp nhận được). Tách `actions::require_human` dùng chung với `board::require_human`, kiểm cả `stdin && stdout` TTY và `detect_caller` qua `HC_TERMINAL_ID`; có unit test từ chối agent trong PTY của `hc` và khi `hc get` lỗi.
3. **`ApprovalCmd::Revoke` không trim và `revoke_approval_grant` bỏ qua số dòng cập nhật** — **Đã sửa** (chấp nhận được). `revoke_approval_grant` trả `Result<usize>`; `actions::revoke_grant` chuẩn hóa `lane`/`task`/`action` và báo lỗi khi `revoked == 0`, tránh ghi `log_action` và báo thành công giả.
4. **Bất đồng nhất trim trên `delegated_actions` và `lane`/`task`** — **Đã sửa** (chấp nhận được). `validate_new_grant` trả `ValidatedGrant` đã chuẩn hóa cả 3 trường; `decide` chuẩn hóa `lane`, `task`, `action` và từng phần tử `delegated_actions`; CLI `check` và MCP `agm_approval_check` chuẩn hóa trước khi xuất JSON/text.
5. **`upsert_approval_grant` luôn `INSERT` làm grant cũ không hạn sống lại** — **Đã sửa** (chấp nhận được). Đổi tên thành `add_approval_grant`, chạy trong `unchecked_transaction` để `UPDATE ... SET revoked_ms = ?4 WHERE ... AND revoked_ms IS NULL` trước khi `INSERT`; có test `approval_new_grant_supersedes_older_ones`.

**Nên sửa (vòng 1):**
1. **`ApprovalCmd::Check` ghi bảng `actions` với `?` sau khi đã in kết quả** — **Đã sửa** (chấp nhận được). Đã bỏ `store.log_action` khỏi `ApprovalCmd::Check`, cập nhật `README.md` / `README_vi.md`, có test kiểm tra `COUNT(*) FROM actions` không đổi ở cả nhánh cho phép và từ chối (`6`, `5`).
2. **`exit_code` ánh xạ `ApprovalDecision::Allowed => 0` trong `ApprovalRefused`** — **Đã sửa** (chấp nhận được). Đã đổi thành `ApprovalDecision::Allowed => 1` và có test `approval_refused_never_exits_zero`.
3. **`Cmd::Approval::*` nuốt lỗi từ `emit(...)`** — **Đã sửa** (chấp nhận được). Cả 4 nhánh đều dùng `emit(...)?` và `emit` nhánh `json` dùng `writeln!(std::io::stdout().lock(), ...)?`; có test `approval_emit_failure_exits_1_not_swallowed`.
4. **Thiếu test migration trực tiếp từ DB schema v4 lên v5** — **Đã sửa** (chấp nhận được). Đã bổ sung `migrates_v4_to_v5_and_keeps_v4_data` trong `tests/store_v2_test.rs`.

**Ghi chú (vòng 1):**
1. **Đơn vị và kiểm tra biên `--expires-ms`** — **Đã sửa** (chấp nhận được). Đã thêm `validate_expires_ms` chặn `expires <= now`, ghi chú trên help CLI (`--expires-ms`) và tài liệu.
2. **Định dạng TTY của `ApprovalCmd::Ls`** — **Đã sửa** (chấp nhận được). Đã thay `{:?}` bằng `grant_human_line` (còn điểm nhỏ về nhãn `"active"` cho grant hết hạn chưa revoke đã nêu ở trên).
3. **Khớp JSON CLI/MCP và tài liệu** — **Đã sửa** (chấp nhận được). CLI, MCP và tài liệu (`BOT-CEO.md`, `README.md`, `README_vi.md`) đã đồng bộ về chuẩn hóa, `require_human`, `check` chỉ đọc không ghi log, và thay thế grant cũ.


## CEO follow-up
- `approval ls` labels a lapsed, unrevoked grant `expired` (grant_human_line takes `now`); test added.
- `approval add` checks the caller before any input validation; the CLI test now asserts the non-human refusal.
- BOT-CEO.md: `ls` is readable by anyone; only add/revoke are human-only.
- Not changed: human branch of `emit` still uses println! (same as every other command); `=` vs `IS` in the supersede UPDATE is safe because add always passes Some(lane/task).
