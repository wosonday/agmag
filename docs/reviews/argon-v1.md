# Review của gemini-4-argon-high (agy) — agmag v1

## G1 an toàn (actions, policy)

Tệp: src/actions.rs, src/policy.rs · token: {'input_tokens': 16793, 'output_tokens': 12523, 'thinking_tokens': 10916, 'cache_read_tokens': 0, 'total_tokens': 29316}

### Critical

1. **`src/actions.rs:164-173` (và `149-159`) — Fail-open trong `authorize` cho phép Agent kill/interrupt Codex/Claude (vi phạm Quy tắc 1)**
   - **Kịch bản**: Terminal Codex/Claude mới sinh qua `spawn` (chỉ gán `row.template`, chưa có `row.app`), chưa kịp phát sự kiện 7501, chưa đồng bộ vào CSDL, báo `app` khác chuẩn (`"claude"`, `"codex.exe"` trên Windows), hoặc `target` truyền bằng tên trong khi `store.terminal(target)` tra theo `address` trả về `None`. Khi đó `app` thành `""`, `protected = false`, và `Caller::Agent` vượt qua kiểm tra để kill/interrupt Codex/Claude.
   - **Cách sửa**: Fail-closed với `Caller::Agent`: bắt buộc tìm thấy terminal trong `store`, kiểm tra cả `app` lẫn `template` (chuẩn hóa chữ thường, bỏ `.exe`), và chỉ cho phép khi xác nhận đúng là thợ được phép can thiệp (vd `app == "opencode"` và `role == "worker"`).

2. **`src/actions.rs:23-29` — Agent giả mạo `Caller::User` trên Windows ConPTY để kill Codex/Claude (vi phạm Quy tắc 1)**
   - **Kịch bản**: Trên Windows 11, tiến trình con do agent gọi vẫn gắn với ConPTY nên `tty == true`. Nếu agent xóa biến môi trường `HC_TERMINAL_ID` trong tiến trình con rồi gọi `agm kill <target> --yes`, `hc_terminal` là `None` (dẫn tới `terminal_is_agent == false`), kết hợp `tty == true` và `confirmed == true` (từ `--yes`), `classify_caller` phân loại nhầm agent thành `Caller::User { confirmed: true }`.
   - **Cách sửa**: Không cho phép `--yes` bỏ qua xác nhận khi `hc_terminal.is_none()` hoặc kiểm tra cây tiến trình cha (parent PID) có thuộc terminal của `herdr-core` hay không.

3. **`src/policy.rs:117-138` — Thiếu toàn bộ logic xoay vòng thợ OpenCode ở 70% ngữ cảnh (spec §5.9 & Quy tắc 4)**
   - **Kịch bản**: Docstring đầu tệp và `tick()` ghi rõ vòng policy gồm xoay vòng thợ OpenCode, nhưng `context_tick` chỉ mở inbox cảnh báo ở `ctx_warn` (50%). Khi thợ đạt 70% ngữ cảnh, không có bước yêu cầu bàn giao, kiểm tra commit `.agents/log/<ID>.md` + worktree sạch, kill và spawn lại.
   - **Cách sửa**: Bổ sung máy trạng thái xoay vòng trong `policy::tick`: ở ngưỡng 70% gửi yêu cầu bàn giao; vòng sau kiểm tra git commit chạm `.agents/log/<ID>.md` và `git status --porcelain` sạch mới `kill` + `spawn` lại (nếu bàn giao lỗi thì mở inbox, tuyệt đối không kill).

---

### Important

4. **`src/policy.rs:76-112` — Ngân sách `provider` không bao giờ kích hoạt `kill`/`interrupt`; lỗi gộp/lặp thông báo**
   - **Kịch bản**: (a) Dòng 93 `b.scope.strip_prefix("task:")` luôn là `None` với ngân sách `provider:<p>:<w>`, nên dòng 98 `let (Some(task), "kill" | "interrupt")` không bao giờ khớp. (b) `open_inbox(None, "budget", Some(level), ...)` dùng chung `terminal = None`: nếu dedup theo `(terminal, source, kind)` thì 1 ngân sách vượt ngưỡng sẽ nuốt thông báo của các ngân sách khác; nếu người dùng đóng mục inbox khi `v >= b.hard` (token tích lũy không giảm), `tick` kế tiếp sẽ mở lại inbox và kill/interrupt lặp lại.
   - **Cách sửa**: Lưu trạng thái ngưỡng đã kích hoạt trên từng dòng `budgets`; truyền `b.scope` vào khóa dedup và ánh xạ `provider` sang các terminal liên quan khi thực thi `action`.

5. **`src/policy.rs:54-63` & `src/actions.rs:114-137` — `spawn` bỏ qua ngân sách `provider`, lỗi tràn tên tự sinh và race `max_workers`**
   - **Kịch bản**: (a) `spawn_blocked_by_budget` chỉ kiểm tra `task:<task>`, cho phép spawn cả khi `provider` đã vượt ngưỡng cứng. (b) `req.task` dài 29–32 ký tự qua được dòng 114 nhưng tên tự sinh `w-<task>-<n>` vượt 32 ký tự và lỗi ở dòng 135. (c) Hai lệnh `spawn` chạy song song hoặc có terminal cũ bị xóa khiến `count() + 1` trùng tên và vượt `max_workers` (TOCTOU).
   - **Cách sửa**: Kiểm tra thêm ngân sách `provider` khi spawn; giới hạn `req.task` tối đa 24 ký tự; kiểm tra `max_workers` và cấp phát tên trong transaction SQLite (`BEGIN IMMEDIATE`).

6. **`src/policy.rs:19-27, 40-45, 66-74` — Số liệu giả khi chưa có dữ liệu hoặc `ctx_window <= 0` (vi phạm Quy tắc 3)**
   - **Kịch bản**: (a) Task chưa có bản ghi `usage` vẫn bị `COALESCE(SUM(...), 0)` biến thành `Some(0.0)` thay vì `None`. (b) Dòng 72 `window.map(|w| w as u64)` không lọc `w > 0`: nếu `ctx_window` trong CSDL là `0` hoặc âm, hàm không fallback sang `cfg.model(m).context_window` mà chia cho 0 hoặc tràn `u64`.
   - **Cách sửa**: Bỏ `COALESCE` để trả `Option<u64>` (`None` khi chưa có dòng `usage`); đổi dòng 72 thành `window.filter(|&w| w > 0).map(|w| w as u64)`.

---

### Minor

7. **`src/actions.rs:87-95` — `reply` không gửi Enter (`false`) và cho phép trả lời nhầm vào mục `request` / mục đã đóng**
   - **Kịch bản**: `hc.send(term, text, false, ...)` truyền cờ thứ 3 là `false` (khác `send` ở dòng 71 truyền `true`), khiến văn bản chỉ gõ vào terminal mà không nhấn Enter; ngoài ra trả lời vào mục `source == "request"` (đề nghị kill từ `authorize`) hoặc mục đã resolve sẽ bơm thẳng chữ vào terminal mục tiêu.
   - **Cách sửa**: Kiểm tra mục inbox chưa resolve, xử lý riêng `source == "request"`, và đặt cờ submit/enter là `true`.


## G2 xoay vòng (rotation, git, hc)

Tệp: src/rotation.rs, src/git.rs, src/hc.rs · token: {'input_tokens': 17282, 'output_tokens': 9623, 'thinking_tokens': 8156, 'cache_read_tokens': 0, 'total_tokens': 26905}

### Critical

1. **`src/rotation.rs:83-96, 103-110, 178` — Lặp vô hạn khi xoay vòng `failed` và báo lỗi giả sau khi `done`**
   - **Kịch bản:** `start()` chỉ chặn khi có bản ghi `state IN ('pending', 'requested')`.
     - Khi xoay vòng chuyển sang `'failed'` (worktree bẩn, quá hạn, thiếu commit), ở `tick()` kế tiếp phiên cũ vẫn `live` và `pct >= ctx_rotate`, nên `start()` tạo ngay vòng mới, gửi lại lệnh bàn giao và mở inbox lỗi lặp vô hạn mỗi tick.
     - Khi vừa `'done'`, phiên cũ bị `hc kill` nhưng CSDL chỉ cập nhật `state = 'finished'` bất đồng bộ qua `hc events`. Nếu `tick()` chạy trước khi sự kiện về tới DB, `start()` lại tạo vòng mới cho terminal vừa bị kill rồi báo lỗi giả vào inbox.
   - **Cách sửa:** Không tự động `start()` lại nếu terminal đã có bản ghi `'done'`, hoặc đã `'failed'` ở cùng `turn` (hoặc cần cooldown / yêu cầu thủ công). Cập nhật trạng thái terminal trong DB ngay khi `kill` thành công.

2. **`src/hc.rs:112-121, 132-138` — Nhận diện sai ứng dụng trên Windows (`.cmd`/`.bat`), làm hỏng quy tắc bảo vệ Claude/Codex và xoay vòng OpenCode**
   - **Kịch bản:** Trên Windows 11, các CLI cài qua npm/pnpm/bun thường chạy qua shim `claude.cmd`, `codex.cmd`, `opencode.cmd`. `app_from_argv()` chỉ cắt `.exe`, nên trả về `"claude.cmd"`, `"codex.cmd"`, `"opencode.cmd"`. Ngoài ra `v["agent"]` (dòng 133) không đi qua hàm chuẩn hóa tên. Hệ quả: khi chưa có trạng thái 7501, `opencode.cmd` không bao giờ khớp `app == Some("opencode")` để xoay vòng, còn `claude.cmd`/`codex.cmd` có nguy cơ lọt qua bộ lọc bảo vệ Quy tắc 1.
   - **Cách sửa:** Cắt cả `.cmd`, `.bat`, `.ps1`, `.exe` và chuẩn hóa cả `v["agent"]` qua cùng hàm ánh xạ tên ứng dụng.

3. **`src/rotation.rs:164-183` & `74-78` — Kill phiên cũ trước khi kiểm tra/spawn phiên mới, báo sai trạng thái**
   - **Kịch bản:** `verify_and_respawn()` gọi `actions::kill` trước `actions::spawn`. Nếu `spawn` thất bại (vượt ngân sách token/provider 5h-7d, lỗi template, lỗi `hc run`), thợ cũ đã bị giết khiến đầu việc mồ côi, đồng thời `fail()` ghi vào inbox thông báo sai sự thật: *"Phiên cũ được giữ nguyên"*.
   - **Cách sửa:** Kiểm tra điều kiện ngân sách/template (hoặc `spawn` phiên mới thành công) trước khi `kill` phiên cũ; cập nhật nội dung `fail()` đúng trạng thái thực tế.

---

### Important

4. **`src/git.rs:9-10, 30-33` — `is_clean()` bỏ qua toàn bộ tệp untracked thay vì chỉ `.agents/tasks/`**
   - **Kịch bản:** Tham số `--untracked-files=no` làm `git status` lờ đi **mọi** tệp mới chưa `git add` trong repo. Nếu thợ tạo tệp mã nguồn mới nhưng chỉ commit `.agents/log/<task>.md`, `is_clean()` vẫn trả `true`, gây mất ngữ cảnh/tệp chưa commit khi xoay vòng.
   - **Cách sửa:** Bỏ `--untracked-files=no`, dùng `--untracked-files=normal` và lọc bỏ các dòng đường dẫn thuộc `.agents/tasks/` (hoặc dùng pathspec `:(exclude).agents/tasks/`).

5. **`src/rotation.rs:82-97, 113-138` — `start()` không kiểm tra `rotatable()`, cho phép xoay nhầm Codex/Claude hoặc terminal không có `task`**
   - **Kịch bản:** Khi gọi qua `agm rotate <addr>`, `start()` không kiểm tra `rotatable()`. Nếu gọi lên terminal Codex/Claude hoặc terminal có `task == None`, `tick()` vẫn gửi lệnh bàn giao vào `.agents/log/.md` và gọi `actions::kill` với quyền `Agent`.
   - **Cách sửa:** Kiểm tra `rotatable(&t)` (hoặc tối thiểu `app == opencode`, `task.is_some()`, `template.is_some()`) ngay trong `start()` và `tick()`.

6. **`src/hc.rs:44-47, 75-100` — Đặt `--json` sau `spec.argv` và thiếu `--` trong `hc run` / `hc send`**
   - **Kịch bản:** `HcCli::call()` luôn nối `--json` ở cuối mảng tham số. Với `hc run ... <argv>`, `--json` bị đẩy vào sau lệnh con của terminal mới (thành tham số của tiến trình con thay vì `hc`). Với `hc send`, nếu `text` bắt đầu bằng `-`, parser của `hc` sẽ hiểu nhầm là cờ CLI.
   - **Cách sửa:** Đặt `--json` ngay sau tên lệnh con (`vec![s("run"), s("--json"), ...]`) và thêm `"--"` trước các tham số vị trí (`spec.argv`, `text`).

---

### Minor

7. **`src/rotation.rs:41-43, 125-137` — Lệch điều kiện `idle` và gửi bàn giao khi `git.head` lỗi**
   - **Kịch bản:** `idle(&t)` chấp nhận cả `"done"`, nhưng `hc.send(..., if_idle=true)` truyền `--if idle`; nếu terminal ở trạng thái `"done"`, `hc send` lỗi liên tục mỗi tick và ghi tràn `log_action`. Ngoài ra dòng 126 lấy `git.head(&cwd).ok()` trả `None` vẫn gửi lệnh bàn giao, để rồi chắc chắn `fail` ở dòng 156.
   - **Cách sửa:** Chỉ coi `status == Some("idle")` là rảnh để gửi lệnh, và kiểm tra `git.head(&cwd)` thành công trước khi gọi `hc.send`.


## G3 sự kiện & ghép (daemon, correlate, hook, ask)

Tệp: src/daemon.rs, src/correlate.rs, src/hook.rs, src/ask.rs · token: {'input_tokens': 15679, 'output_tokens': 9951, 'thinking_tokens': 8655, 'cache_read_tokens': 0, 'total_tokens': 25630}

### Critical

1. **`src/correlate.rs:32-55` — Ghép nhầm phiên khi nhiều terminal cùng `(app, cwd)` hoặc phiên của terminal đã kết thúc**
   - **Kịch bản:**
     1. Hai terminal `T1`, `T2` cùng `app` và `cwd` được tạo gần nhau. `T1` chưa kịp ghi file log, chỉ mới có phiên `S2` của `T2` trong CSDL. Vòng lặp xét `T1` thấy `candidates` có đúng 1 phần tử `[S2]` nên ghép nhầm `S2` cho `T1` (vi phạm quy tắc "nhiều ứng viên ngang nhau → không đoán").
     2. Truy vấn chỉ lọc `NOT EXISTS (... AND x.state = 'live')`. Nếu `T_old` vừa kết thúc (`state = 'finished'`) trong cửa sổ `[-1p, +10p]` của `T_new` cùng `cwd`, phiên cũ của `T_old` vẫn lọt vào `candidates` và bị ghép lại cho `T_new`.
   - **Cách sửa:** Bỏ điều kiện `AND x.state = 'live'` (một phiên đã gắn với bất kỳ terminal nào thì không ghép lại); đồng thời đếm số terminal chưa có phiên cùng `(source, want)` có cửa sổ thời gian chứa `s.started_ms` — chỉ ghép khi cả chiều terminal→phiên và phiên→terminal đều là 1–1.

2. **`src/hook.rs:38-44` & `src/daemon.rs:64-73` — Race condition đa tiến trình làm mất metadata terminal hoặc mất liên kết phiên**
   - **Kịch bản:** `agm hook statusline` chạy song song với daemon. Nếu `store.terminal(addr)` trả `None`, rồi daemon kịp xử lý sự kiện `created` (ghi đủ `cwd`, `created_ms`, `pid`), lệnh `upsert_terminal(..Default::default())` ngay sau đó của hook sẽ ghi đè các trường này về mặc định (`created_ms = 0`, `cwd = None`). Ngược lại, nếu hook chạy trước rồi daemon nhận snapshot/`created`, `upsert_terminal` từ `terminal_row_from(t)` (không có `session`) có nguy cơ xóa liên kết `hook` hoặc `app`.
   - **Cách sửa:** Dùng một câu SQL nguyên tử `INSERT INTO terminals(address, state, app, session, link_source) ... ON CONFLICT(address) DO UPDATE SET app = excluded.app, session = excluded.session, link_source = 'hook'` thay vì đọc-rồi-ghi; đảm bảo `upsert_terminal` của daemon dùng `COALESCE` để không xóa `session`/`app` đã có.

---

### Important

3. **`src/daemon.rs:78-95` — Kẹt hộp thư `"blocked"` khi trạng thái terminal bị xóa (`status: null`) hoặc terminal bị `removed`**
   - **Kịch bản:** Khi sự kiện `"status"` gửi `data.status = null` để xóa trạng thái chặn, `state` là `None`. Nhánh `else if state.is_some()` (dòng 93) bị bỏ qua, khiến mục inbox `"blocked"` không bao giờ được đóng dù trạng thái terminal đã xóa. Tương tự, sự kiện `"removed"` (dòng 42) xóa terminal nhưng không gọi `resolve_inbox_for_terminal`.
   - **Cách sửa:** Đổi `else if state.is_some()` ở dòng 93 thành `else`; gọi `store.resolve_inbox_for_terminal(addr, "blocked", now)?` ở nhánh `"removed"`.

4. **`src/ask.rs:14-42` — Mất dữ liệu token khi `agy` lỗi/timeout và ghi dữ liệu giả khi thiếu model/usage (vi phạm Quy tắc 3)**
   - **Kịch bản:** Khi `agy` chạm `--print-timeout` hoặc lỗi giữa chừng (`status != "SUCCESS"`), JSON trả về vẫn có `usage` thực tế nhưng hàm `bail!` ngay ở dòng 15 làm mất toàn bộ số token đã tiêu thụ. Ngoài ra, khi không truyền `model`, mã gán chuỗi giả `"agy-default"` (dòng 21) và nếu thiếu `out["usage"]` vẫn ghi bản ghi 0 token thay vì `None`.
   - **Cách sửa:** Ghi `usage` (chỉ khi `out["usage"].is_object()`) trước khi kiểm tra `out["status"]`; lấy model thật từ JSON đầu ra hoặc để `Option<String>` là `None` thay vì `"agy-default"`.

---

### Minor

5. **`src/correlate.rs:14-18` — Chuẩn hóa đường dẫn Windows lệch tiền tố UNC/extended-length (`\\?\`)**
   - **Kịch bản:** Trên Windows, nếu một phía trả `\\?\C:\repo` (từ `canonicalize`) và phía kia trả `C:\repo`, `norm_cwd` không cắt `\\?\` nên so sánh thất bại.
   - **Cách sửa:** Dùng `.strip_prefix(r"\\?\")` sau khi đổi `/` sang `\`.

6. **`src/ask.rs:60-62` — Nuốt `stderr` và mã thoát khi `agy` thất bại**
   - **Kịch bản:** Khi `agy` lỗi khởi chạy (sai cờ, lỗi quyền, thiếu binary `.cmd`) và in lỗi ra `stderr` với `stdout` rỗng, thông báo lỗi chỉ in `"đầu ra agy không phải JSON: "` mà mất hoàn toàn `stderr` và `exit code`.
   - **Cách sửa:** Kiểm tra `!out.status.success()` kết hợp đính kèm `String::from_utf8_lossy(&out.stderr)` vào `with_context`.


## G4 đọc log (opencode, ingest)

Tệp: src/opencode.rs, src/ingest.rs, src/parse/mod.rs · token: {'input_tokens': 16341, 'output_tokens': 17185, 'thinking_tokens': 15972, 'cache_read_tokens': 0, 'total_tokens': 33526}

### Critical

1. **`src/opencode.rs:44-50, 58-60, 117` — Mất dữ liệu do con trỏ chỉ dùng `time_updated > ?1`**
   - **Kịch bản**: OpenCode cập nhật/chèn nhiều `session_message` trong cùng một mili-giây `T`, hoặc ghi bản ghi B có `time_updated = T` ngay sau khi câu `SELECT` đọc bản ghi A (cũng ở mốc `T`). Cursor lưu `offset = T`, nên lần quét sau `WHERE time_updated > T` bỏ qua vĩnh viễn bản ghi B (mất token, context, turn).
   - **Cách sửa**: Lưu con trỏ kép `(time_updated, seq, id)` và truy vấn `WHERE time_updated > ?1 OR (time_updated = ?1 AND (seq > ?2 OR (seq = ?2 AND id > ?3))) ORDER BY time_updated, seq, id`.

2. **`src/ingest.rs:112-138, 184-190` — Race condition khi tệp đang được ghi làm lệch `head_hash` và xóa nhầm phiên**
   - **Kịch bản**: Tại dòng 113, `metadata.len()` đọc được `size = 500` (< 4096). Ngay sau đó Codex/Claude flush thêm dữ liệu lên 2000 byte. Dòng 124 đọc `head` 2000 byte, vòng lặp đọc tới `offset = 2000`, nhưng dòng 184 lưu cursor với `size = 500` và `head_hash = hash(2000 byte)`. Ở lần quét kế tiếp, dòng 132 tính `hash(&head[..min(len, 500)])` (hash của 500 byte) không khớp `c.head_hash` (hash của 2000 byte) → `offset` về `0` và `reset_session(sid)` xóa sạch dữ liệu phiên.
   - **Cách sửa**: Giới hạn việc đọc trong phạm vi `size` ban đầu hoặc lưu `size = offset.max(head.len() as u64)` và băm đúng độ dài prefix `head.len()`.

3. **`src/ingest.rs:46-55, 63-72` — Bỏ sót tệp vĩnh viễn do lọc `modified >= since_ms` và cơ chế `mtime` trên Windows**
   - **Kịch bản**: (1) `ingest_file` gặp lỗi tạm thời (file lock/SQLite busy trên Windows) ở dòng 67 nên bị `continue`; hoặc (2) dòng cuối chưa kịp có `\n`; hoặc (3) trên Windows NTFS, tiến trình giữ file handle mở khiến `mtime` chưa cập nhật ngay. Khi chu kỳ sau nâng `since_ms` vượt quá `modified` của tệp và phiên đã kết thúc, `discover` bỏ qua tệp đó vĩnh viễn.
   - **Cách sửa**: Đối chiếu `size`/`offset` thực tế với `Cursor` trong DB thay vì lọc cứng bằng `since_ms` toàn cục.

### Important

4. **`src/opencode.rs:80-113` — Chèn trùng `TurnSummary` và ghi `Usage` toàn `0` khi message streaming**
   - **Kịch bản**: Một hàng `session_message` trong OpenCode được `UPDATE` nhiều lần khi streaming (`time_updated` tăng). Khác với `upsert_usage` có `dedupe_key`, `insert_turn` (dòng 106, 111) chạy lại mỗi lần cập nhật gây nhân bản lượt hội thoại. Ngoài ra, khi `data["tokens"]` là `{}` hoặc toàn `0`, `tok.is_object()` vẫn ghi `Usage` 0 (vi phạm quy tắc 3).
   - **Cách sửa**: Dùng `upsert_turn` theo `(sid, m.id)` và chỉ ghi `Usage` khi tổng token `> 0`.

5. **`src/opencode.rs:60-80, 100-102` — Ghi đè lùi `model` và `context` khi message cũ được cập nhật sau**
   - **Kịch bản**: Duyệt theo `ORDER BY time_updated`: nếu một message cũ trong phiên được cập nhật sau message mới, `set_model` và `set_context` sẽ ghi đè trạng thái hiện tại của phiên bằng dữ liệu cũ.
   - **Cách sửa**: Chỉ cập nhật `model`/`context` nếu `ts` (hoặc `seq`) mới hơn bản ghi hiện có của phiên.

### Minor

6. **`src/opencode.rs:17-18, 43` — Đường dẫn Windows không chuẩn hóa và fallback `home_dir` rỗng**
   - **Kịch bản**: Dòng 43 dùng `db.to_string_lossy()` thay vì `ingest::path_key(db)`, khiến khác biệt hoa/thường hoặc `/` và `\` trên Windows tạo hai cursor khác nhau. Dòng 18 dùng `unwrap_or_default()` trỏ nhầm vào `.local\...` tại CWD nếu thiếu biến môi trường home.
   - **Cách sửa**: Dùng `path_key(db)` cho cursor key và trả `Option<PathBuf>`/`Result` khi thiếu `home_dir`.


## G5 CSDL v2 (store/v2, schema)

Tệp: src/store/v2.rs, src/store/schema_v2.sql · token: {'input_tokens': 17651, 'output_tokens': 5596, 'thinking_tokens': 4453, 'cache_read_tokens': 0, 'total_tokens': 23247}

### Critical

1. **`src/store/v2.rs:238-252` & `src/store/schema_v2.sql:38-47` — Race condition (TOCTOU) tạo trùng mục `inbox` giữa daemon và CLI**
   - **Kịch bản:** Cả daemon và CLI cùng mở DB SQLite (WAL). Khi 2 tiến trình gọi `open_inbox` đồng thời với cùng `(terminal, source, text)`, cả hai cùng chạy `SELECT EXISTS` (chỉ giữ read snapshot) và nhận `false`, sau đó lần lượt `INSERT`, tạo ra 2 mục inbox mở trùng hệt nhau.
   - **Cách sửa:** Gộp thành một câu lệnh nguyên tử `INSERT INTO inbox (ts_ms, terminal, source, kind, text) SELECT ?1, ?2, ?3, ?4, ?5 WHERE NOT EXISTS (...)` rồi kiểm tra số dòng thay đổi (`changes == 0` trả `Ok(None)`), hoặc tạo partial unique index:
     `CREATE UNIQUE INDEX IF NOT EXISTS idx_inbox_open ON inbox(IFNULL(terminal, ''), source, text) WHERE resolved_ms IS NULL;`.

---

### Important

2. **`src/store/v2.rs:118-130` — `upsert_terminal` ghi đè mất `labels` và hồi sinh `state` của terminal đã kết thúc**
   - **Kịch bản:**
     1. Nhãn được gán qua `set_terminal_labels`. Sau đó một sự kiện cập nhật gọi `upsert_terminal` với `t.labels` rỗng (`{}`) → `labels = excluded.labels` xóa sạch nhãn đã lưu (trong khi `task`/`role` lại dùng `COALESCE`).
     2. Terminal đã `finish_terminal` (`state = 'finished'`), nhưng sự kiện `upsert_terminal` đến trễ có `t.state = "live"` và `finished_ms = None` → ghi đè `state = 'live'` nhưng vẫn giữ `finished_ms`/`exit_code` cũ, tạo bản ghi mâu thuẫn (`state = 'live'` nhưng có `exit_code`).
   - **Cách sửa:** Chỉ cập nhật `labels` khi `excluded.labels != '{}'`; không cho `state` quay lại `'live'` nếu `state IN ('finished', 'removed')`, và cập nhật cả `updated_ms`.

3. **`src/store/v2.rs:160-184` — Sự kiện lệch thứ tự làm bẩn `status`/`kind`/`msg` giữa các `turn`**
   - **Kịch bản:**
     1. Trong `set_terminal_status`, SQL dùng `turn = MAX(turn, ?5)` để chống lùi `turn`, nhưng vẫn gán thẳng `status = ?2, kind = ?3, msg = ?4`. Nếu báo cáo của `turn` cũ đến trễ sau khi terminal đã sang `turn` mới, `turn` giữ nguyên nhưng `status/kind/msg` bị ghi đè bởi trạng thái cũ.
     2. Khi `set_terminal_turn` tăng `turn` sang lượt mới và đổi `status`, `kind` và `msg` của lượt trước (VD: báo lỗi/câu hỏi chặn) không được xóa.
     3. `set_terminal_turn` dùng `UPDATE` thay vì `UPSERT` như `set_terminal_status`, nên nếu sự kiện turn đến trước `upsert_terminal`, dữ liệu bị mất âm thầm.
   - **Cách sửa:** Chỉ cập nhật `status, kind, msg` khi `?5 >= turn`; trong `set_terminal_turn`, xóa `kind = NULL, msg = NULL` khi `?2 > turn`.

4. **`src/store/v2.rs:219-228` — `terminal()` trả sai terminal khi trùng `name` hoặc trùng chéo `address`/`name`**
   - **Kịch bản:** Khi xoay vòng (rotation) thợ OpenCode nhiều lần với cùng `name`, hoặc truy vấn theo `addr`, câu lệnh `WHERE address = ?1 OR name = ?1 ORDER BY state = 'live' DESC LIMIT 1` không ưu tiên khớp chính xác `address = ?1` trước `name = ?1`, và khi cùng `state` (VD: đều `'finished'` hoặc đều `'live'`) không sắp xếp theo `created_ms DESC`, dẫn đến trả về bản ghi cũ.
   - **Cách sửa:** `ORDER BY (address = ?1) DESC, (state = 'live') DESC, created_ms DESC LIMIT 1`.

---

### Minor

5. **`src/store/v2.rs:262-268` — `resolve_inbox` ghi đè mục đã đóng và không báo khi `id` không tồn tại**
   - **Kịch bản:** Gọi `resolve_inbox` lần hai trên mục đã xử lý sẽ ghi đè `resolved_ms` mới; gọi với `id` sai vẫn trả `Ok(())`.
   - **Cách sửa:** Thêm `AND resolved_ms IS NULL` (hoặc `COALESCE(resolved_ms, ?3)`) và trả `Result<bool>` theo số dòng cập nhật.


## G6 lệnh CLI (ops, tui)

Tệp: src/ops.rs, src/tui.rs · token: {'input_tokens': 16935, 'output_tokens': 5813, 'thinking_tokens': 4662, 'cache_read_tokens': 0, 'total_tokens': 22748}

### Critical

1. **`src/tui.rs:53-60` — Giả mạo `Caller::User` qua TUI, vi phạm Quy tắc 1**
- **Kịch bản:** `tui::run` gán cứng `Caller::User { confirmed: true }` khi nhận `"y"` mà không kiểm tra `stdin().is_terminal()` hay terminal gọi có phải agent không. Một agent có thể chạy `printf "k <codex_id>\ny\nq\n" | agm tui` để tự động kill/ngắt Codex hoặc Claude.
- **Cách sửa:** Kiểm tra `std::io::stdin().is_terminal()` ngay đầu `tui::run` và gọi `ops::caller(store, true, None)` thay vì gán cứng `Caller::User`.

2. **`src/ops.rs:24-40` — Logic nhận diện agent (`caller`) dạng fail-open, dễ bị vượt qua (Quy tắc 1)**
- **Kịch bản:**
  1. Agent chạy trong ConPTY (`is_terminal() == true`) xóa biến môi trường (`$env:HC_TERMINAL_ID=""`) rồi gọi `agm kill <codex> --yes` → `term = None` → `is_agent = false`, `confirmed = true` → được quyền `Caller::User`.
  2. Khi terminal chưa kịp ghi vào DB hoặc `r.reported == false`, `.unwrap_or(false)` mặc định coi không phải agent.
  3. Khi agent gọi tool shell, `r.app` có thể đổi thành `"pwsh"`/`"cmd"`/`"bash"`, khiến `is_agent` thành `false`.
- **Cách sửa:** Không cho phép `--yes` bỏ qua câu hỏi `[y/N]` khi dừng Codex/Claude; fail-closed khi `term` chưa `reported` hoặc terminal đã từng gắn với phiên agent trong `store`.

3. **`src/ops.rs:251-261` — Mất sạch dữ liệu `.claude/settings.json` khi lỗi đọc tệp**
- **Kịch bản:** `match std::fs::read_to_string(&path)` gom mọi `Err(_)` về `json!({})`. Trên Windows, nếu tệp bị khóa tạm thời (sharing violation) hoặc lỗi quyền đọc, hàm tạo object rỗng rồi ghi đè ở dòng 260, xóa toàn bộ cấu hình của người dùng. Ngoài ra, nếu thư mục `~/.claude` chưa tồn tại, `fs::write` sẽ lỗi.
- **Cách sửa:** Chỉ dùng `json!({})` khi `err.kind() == ErrorKind::NotFound` (các lỗi khác phải `bail!`), kiểm tra `v.is_object()`, và gọi `fs::create_dir_all` cho thư mục cha trước khi ghi.

---

### Important

4. **`src/tui.rs:89-92` — Nuốt lỗi khi tắt `policy`, vi phạm Quy tắc 4**
- **Kịch bản:** `let _ = store.set_setting("policy", ...)` bỏ qua lỗi SQLite (ví dụ `SQLITE_BUSY`) nhưng vẫn báo `"policy TẮT"`. Người dùng tưởng đã tắt tự động hóa nhưng daemon vẫn tiếp tục xoay vòng/kill agent.
- **Cách sửa:** Kiểm tra `Result` của `set_setting`; nếu lỗi thì hiển thị lỗi trong `note`.

5. **`src/ops.rs:85-111` — Race/Stale state giữa luồng sự kiện `hc` và vòng định kỳ**
- **Kịch bản:** Khởi tạo `last = now - interval` khiến `wait = 0` ở vòng đầu, làm `policy::tick` và `rotation::tick` chạy ngay trước khi các sự kiện `hc` đầu tiên kịp nạp. Ngoài ra, mỗi vòng chỉ đọc 1 dòng từ `rx`; nếu sự kiện dồn về nhiều, `rotation::tick` sẽ chạy trên trạng thái terminal cũ (ví dụ tưởng agent đang rảnh rồi yêu cầu bàn giao/kill).
- **Cách sửa:** Rút cạn kênh bằng `while let Ok(line) = rx.try_recv()` trước mỗi lần chạy `step()`.

6. **`src/ops.rs:67-77` — Rò rỉ tiến trình con `hc events` trên Windows**
- **Kịch bản:** Khi `tx.send(line).is_err()`, luồng nền `return` ngay mà không `child.kill()` và `child.wait()`, để lại tiến trình `hc` chạy mồ côi.
- **Cách sửa:** Gọi `let _ = child.kill(); let _ = child.wait();` trước khi `return`.

---

### Minor

7. **`src/ops.rs:194-205` — Không chặn `NaN` / số âm trong `budget_set`**
- **Kịch bản:** Nếu `soft` hoặc `hard` là `NaN`, `soft > hard` là `false`, lọt qua kiểm tra và lưu `NULL`/`NaN` vào DB.
- **Cách sửa:** Kiểm tra `!soft.is_finite() || !hard.is_finite() || soft < 0.0 || soft > hard`.


## G7 báo cáo (dashboard, report)

Tệp: src/dashboard.rs, src/report.rs · token: {'input_tokens': 19116, 'output_tokens': 10285, 'thinking_tokens': 9097, 'cache_read_tokens': 0, 'total_tokens': 29401}

Không phát hiện lỗi mức **Critical** (hai tệp này chỉ đọc CSDL và dựng chuỗi giao diện, không trực tiếp thực thi lệnh kill/ghi log). Dưới đây là các lỗi **Important** và **Minor** cần sửa:

### Important

1. **`src/dashboard.rs:152-166` — Hiển thị `0%` giả khi ngân sách thiếu số liệu (`None`) và ép kiểu mất phần thập phân (vi phạm Quy tắc #3)**
   - **Kịch bản**: Khi ngân sách `provider` chưa có dữ liệu hạn mức (hoặc đã qua thời điểm reset), `b.value` là `None` và `pct` là `None`. Tuy nhiên dòng 161–162 gọi `pct.unwrap_or(0.0)`, khiến TUI vẽ thanh `░░░░░░░░░░   0%  ? / 80 %` (số liệu giả `0%`). Ngoài ra, `tokens(v as u64)` làm tròn xuống thành số nguyên với ngân sách tính bằng `%`.
   - **Cách sửa**: Khi `pct` là `None`, hiển thị `"   ?"` và thanh rỗng riêng thay vì `0%`; chỉ gọi `tokens()` khi `b.unit == "tokens"`.

2. **`src/dashboard.rs:212-221` — `parse_command` phân tích sai lệnh `s` khi có khoảng trắng thừa hoặc từ cùng tiền tố**
   - **Kịch bản**:
     1. `rest.split_once(' ')` không `.trim_start()` phần `after`: gõ `s t1  GIAO việc` (2 dấu cách) làm `after.starts_with(*k)` thất bại.
     2. Không kiểm tra ranh giới từ sau `kind`: gõ `s t1 GIAO_VIỆC nội dung` khớp nhầm loại `"GIAO"` và gửi tin `"_VIỆC nội dung"`.
   - **Cách sửa**: `let after = after.trim_start();` và kiểm tra phần còn lại sau `after.strip_prefix(*k)` phải bắt đầu bằng khoảng trắng.

3. **`src/report.rs:215-218` — `quota()` trả về trùng dòng cho cùng `(provider, window_minutes)`**
   - **Kịch bản**: Nếu hai bản ghi hạn mức của cùng `(provider, window_minutes)` có cùng `MAX(ts_ms)` (ghi trong cùng mili-giây), điều kiện `WHERE q.ts_ms = (SELECT MAX(q2.ts_ms) ...)` trả về cả hai hàng thay vì 1 hàng mới nhất.
   - **Cách sửa**: Lọc theo khóa chính: `WHERE q.id = (SELECT q2.id FROM quota q2 WHERE q2.provider = q.provider AND q2.window_minutes = q.window_minutes ORDER BY q2.ts_ms DESC, q2.id DESC LIMIT 1)`.

4. **`src/report.rs:164-193` — `cost()` làm mất `cost_usd` khi `u.model` là `NULL` và tách nhóm sai `Cwd` trên Windows**
   - **Kịch bản**:
     1. Nếu bản ghi `usage` có `u.model = NULL` nhưng `sessions.model` có giá trị, `model` thành `None` → `row_cost = None` → toàn bộ `cost_usd` của nhóm (`Day`/`Source`/`Cwd`) bị kéo về `None`.
     2. Trên Windows 11, `GroupBy::Cwd` gom theo chuỗi thô nên `C:\repo`, `c:\repo` và `C:/repo` bị tách thành các nhóm khác nhau.
   - **Cách sửa**: Dùng `COALESCE(u.model, s.model)` trong SQL; chuẩn hóa đường dẫn Windows (chữ thường ổ đĩa, dấu `\`, bỏ `\` cuối) khi gom theo `Cwd`.

### Minor

5. **`src/report.rs:235-260` — `resolve_session` khớp toàn bộ bảng khi `prefix` rỗng; `turns()` sắp xếp không tất định**
   - **Kịch bản**: Truyền `prefix = ""` khớp mọi phiên; trong `turns()`, các lượt có cùng `byte_offset` (ví dụ từ `opencode.db`) không có khóa phụ nên thứ tự trả về không cố định.
   - **Cách sửa**: Chặn `prefix.trim().is_empty()` và đổi SQL thành `ORDER BY byte_offset DESC, ts_ms DESC, id DESC LIMIT ?2`.

6. **`src/dashboard.rs:49, 174, 179` — Bỏ sót `reasoning` và ký tự xuống dòng phá vỡ giao diện TUI**
   - **Kịch bản**: Dòng 49 tính tổng token không có `reasoning` (lệch với `USAGE_SUMS` ở `report.rs:55`, trừ khi `output` đã gồm `reasoning`). Dòng 174, 179 in thẳng `i.text` / `m.text` có `\r`/`\n` hoặc mã điều khiển ANSI làm lệch khung TUI.
   - **Cách sửa**: Đồng bộ công thức tổng token với `report.rs` và lọc bỏ `\r`, `\n`, ký tự điều khiển khi render một dòng.


## G8 main

Tệp: src/main.rs · token: {'input_tokens': 18442, 'output_tokens': 8348, 'thinking_tokens': 7292, 'cache_read_tokens': 0, 'total_tokens': 26790}

Dưới đây là kết quả review cho `src/main.rs` (không phát hiện lỗi mức **Critical** trong phạm vi tệp này; các kiểm tra quy tắc cứng về kill/sandbox/DB nằm ở các module `ops`, `ingest`, `opencode`, `store` chưa có mã nguồn):

### Important

1. **`src/main.rs:230-235, 297-302` — `ls --days` và `cost --days` bỏ sót log khi `days > 7` và có nguy cơ tràn số**
   - **Kịch bản:** Khi chạy `agm ls --days 30` hoặc `agm cost --days 30`, biến `scan_days` vẫn rơi vào nhánh `_ => SCAN_DAYS` (7 ngày). Hàm `scan()` chỉ quét và nạp log của 7 ngày gần nhất trước khi `report::sessions` / `report::cost` truy vấn 30 ngày, làm thiếu dữ liệu của các ngày 8–30 nếu chưa được quét trước đó. Ngoài ra `days: i64` nhận cả giá trị âm hoặc cực lớn gây tràn số tại `days * DAY_MS`.
   - **Cách sửa:** Lấy `scan_days` từ cả `Cmd::Scan { days } | Cmd::Ls { days } | Cmd::Cost { days, .. } => days.max(SCAN_DAYS)` (hoặc `days`), đổi `days` sang kiểu không âm (`u32`/`u64`) và dùng `saturating_mul` / `saturating_sub`.

2. **`src/main.rs:308-317, 379-385` — `agm log <session> --full` đọc theo `byte_offset` làm lỗi hoặc trả rác với phiên OpenCode**
   - **Kịch bản:** Phiên của OpenCode được nạp từ CSDL SQLite (`opencode.db`), không phải tệp văn bản JSONL. Khi chạy `agm log <opencode_session> --full`, `raw_line(&path, t.byte_offset)` mở đường dẫn phiên như tệp văn bản, `seek` tới `byte_offset` và gọi `read_line()`. Nếu `path` trỏ tới `opencode.db`, `read_line` đọc trúng byte nhị phân SQLite và trả lỗi UTF-8 (`InvalidData`) hoặc đọc sai nội dung.
   - **Cách sửa:** Kiểm tra nguồn phiên trả về từ `report::resolve_session(&store, &session)`: chỉ gọi `raw_line` cho nguồn JSONL (Codex/Claude, đồng thời mở `File` một lần ngoài vòng lặp); với OpenCode, truy vấn bản ghi gốc hoặc báo không hỗ trợ `--full` theo byte offset.

---

### Minor

3. **`src/main.rs:224-225, 259-280` — Lỗi `providers.toml` làm chặn luôn nút dừng khẩn `policy off` và `kill`/`interrupt`**
   - **Kịch bản:** `Config::load(&cfg_path)?` chạy ngay đầu hàm `run()` cho mọi lệnh. Nếu `providers.toml` bị hỏng cú pháp hoặc thiếu, người dùng không thể chạy `agm policy off`, `agm kill`, hay `agm interrupt` để dừng khẩn cấp agent.
   - **Cách sửa:** Chỉ nạp `Config::load` ở các nhánh thực sự cần (`Daemon`, `Spawn`, `Tui`, `Ls`, `Cost`).

4. **`src/main.rs:289, 340-343` — `eprintln!` trong `scan()` làm vỡ giao diện `agm tui`**
   - **Kịch bản:** `tui::run` gọi `scan()` định kỳ mỗi `refresh` giây (mặc định 2s). Nếu `opencode.db` lỗi hoặc không đọc được, `eprintln!("agm: bỏ qua opencode.db: ...")` ghi trực tiếp ra stderr đè lên màn hình alternate/raw của TUI.
   - **Cách sửa:** Trả lỗi/cảnh báo trong `IngestStats` thay vì `eprintln!` trực tiếp bên trong `scan()`.

5. **`src/main.rs:24-26, 275-280` — `agm policy on|off` không xuất JSON khi có `--json` hoặc stdout không phải TTY**
   - **Kịch bản:** Nhánh `policy status` dùng `emit(json, ...)`, nhưng nhánh `policy on|off` dùng `println!` thuần, làm lỗi công cụ gọi tự động chờ JSON.
   - **Cách sửa:** Dùng `emit(json, &serde_json::json!({ "enabled": s == "on" }), || println!(...))`.

