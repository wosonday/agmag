---
id: spawn-fixes
tier: mid
skills: test-driven-development, systematic-debugging
branch: fix/spawn-argv-template
worktree: D:/Project/agmag.wt/spawn-fixes
---

# Sửa `agm spawn`: argv bị gộp, template lỗi `sh` trên Windows, xoay vòng kill trước khi chắc mở được phiên mới

## Hiện trạng (Claude CEO tái hiện ngày 2026-11-20, Windows 11, `hc 0.1.21`, `opencode v2.3.2`)

### Lỗi A — `agm spawn` gộp mọi đối số sau `--` thành một chuỗi

- Tái hiện: `agm spawn --task probe --role probe --name argv-probe2 python -- -c "import sys;print(sys.argv)" --prompt "Task ID: x. Bạn là thợ"` → Python nhận **một** đối số `"import sys;print(sys.argv) --prompt Task ID: x. Bạn là thợ"` (IndentationError).
- Đối chứng: `hc run --json --bg -- python -c "import sys;print(sys.argv)" --prompt "hello world"` in đúng `['-c', '--prompt', 'hello world']` → `hc` không gộp.
- Nguyên nhân: `src/main.rs:323` `text.join(" ")`, và `SpawnReq.text: Option<String>` (`src/actions.rs:177`); `actions::spawn` (`src/actions.rs:271`) đẩy chuỗi đó thành một phần tử argv.
- Hệ quả thực tế: `agm spawn ... opencode -- --prompt "<brief>"` → OpenCode báo `Unrecognized flag: --prompt <brief>` và thoát. Cách tạm hiện dùng: `-- "--prompt=<brief>"`.
- Ghi chú thiết kế: với **template** của hc, cú pháp là `hc run <template> "<task>"` — task là **một** chuỗi, nên gộp là đúng ở trường hợp đó. Chỉ sai khi `<TEMPLATE>` thật ra là tên chương trình.

### Lỗi B — template của hc không chạy được trên Windows

- `hc run --json --bg --name tpl-probe reviewer "noop"` → `{"error":{"kind":"spawn_failed","message":"cannot start \"sh\": sh: not found on PATH"}}`. Cùng lỗi với `worker-mid`, `worker-fast`, `auditor`.
- Template nằm ở `%APPDATA%\herdr-core\templates\<tên>.md` (không phải `~/.config/herdr-core/templates` như `HERDR-CORE-BOT.md` §1.4 ghi). Front matter có `command: opencode`, `ready: quiet`; phần thân là chỉ dẫn mở đầu, hc nối task vào cuối thân và gõ vào terminal khi agent sẵn sàng.
- Đây là lỗi của `hc` (chạy `command:` qua `sh -c`), không có mã nguồn hc trên máy. `agm` phải tự lách và không được làm thay đổi hành vi trên Linux/macOS, nơi hc chạy được.

### Lỗi C — xoay vòng thợ OpenCode kill phiên cũ trước khi biết chắc mở được phiên mới

- `src/rotation.rs:307-319`: `spawn_precheck` chỉ kiểm tên/max_workers/ngân sách, rồi **kill** phiên cũ, rồi mới `hc run` bằng `t.template`. Với Lỗi B (template) hoặc khi `template` là tên chương trình (vd `opencode`, argv `["opencode", resume_text]` → OpenCode hiểu `resume_text` là thư mục), thợ cũ bị kill mà không có phiên thay.

## Yêu cầu

1. **Lỗi A.** `SpawnReq` giữ argv dạng `Vec<String>` (không gộp ở `main.rs`). Khi `<TEMPLATE>` là tên một template của hc (tra bằng `hc templates --json`, thêm vào trait `Hc` để test được bằng mock), gộp các phần sau `--` thành một chuỗi task như cũ; còn lại truyền nguyên từng phần tử. Không đổi cú pháp CLI `agm spawn`.
2. **Lỗi B.** Khi `hc run <template>` trả `spawn_failed` có `"sh"` trong thông điệp (chỉ đúng trường hợp này, không nuốt lỗi khác), dùng đường dự phòng:
   - Đọc template từ thư mục của hc: `AGM_HC_TEMPLATES` nếu có, nếu không thì `%APPDATA%\herdr-core\templates` (Windows) / `~/.config/herdr-core/templates` (nơi khác). Parse front matter `command:`; tách theo khoảng trắng; nếu có dấu nháy hoặc ký tự shell (`|&;<>$\``) → báo lỗi rõ ràng, không đoán.
   - `hc run` thẳng argv đó với cùng `--name`, `--cwd`, nhãn `agm.task`/`agm.role`.
   - Gửi `thân template + "\n\n" + task` vào terminal khi agent sẵn sàng (dùng cơ chế `hc send` sẵn có, có thời hạn chờ). Hết thời hạn mà chưa gửi được → tạo mục `agm inbox`, không im lặng bỏ qua.
   - Ghi `template` = tên template (không phải `command`) vào hàng `terminals`, để xoay vòng dùng lại đúng đường này.
3. **Lỗi C.** Trước khi kill trong `rotation.rs`, xác minh mở được phiên kế nhiệm: template tồn tại (qua hc hoặc đường dự phòng của Lỗi B), hoặc chương trình đã biết cách nhận brief. Không xác minh được → `fail` rotation **mà không kill**, đẩy lý do vào inbox. Không đổi quy tắc không bao giờ tự kill Claude/Codex.
4. Viết `.agents/decisions/hc-template-sh-windows.md`: cách tái hiện tối giản, lỗi thực tế, hành vi mong đợi — để người dùng báo lên `herdr-core`. Sửa đường dẫn template trong `HERDR-CORE-BOT.md` §1.4 (ghi rõ đường dẫn trên Windows). Cập nhật mục `spawn` trong `README.md`/`README_vi.md` và `CHANGELOG.md`.

## Phạm vi tệp

Được sửa: `src/main.rs`, `src/actions.rs`, `src/hc.rs`, `src/rotation.rs`, `src/ops.rs`, `src/launch.rs` (nếu cần), `tests/**`, `README.md`, `README_vi.md`, `CHANGELOG.md`, `HERDR-CORE-BOT.md` (chỉ §1.4), `.agents/log/spawn-fixes.md`, `.agents/decisions/hc-template-sh-windows.md`.
Không sửa: `src/store/**` (schema) — nếu bắt buộc thì dừng và hỏi; `Cargo.toml` (không thêm crate); `src/opencode.rs`, `src/report.rs`, `src/policy.rs` (đang có task `opencode-metrics` khác sửa song song).
Không sửa hay ghi vào thư mục template thật của người dùng; test dùng thư mục tạm qua `AGM_HC_TEMPLATES`.

## Tiêu chí nghiệm thu

- Test (mock `Hc`): chương trình + nhiều đối số → argv giữ nguyên từng phần tử, kể cả phần tử có khoảng trắng và tiếng Việt; template → task là một chuỗi.
- Test: `hc run` template trả lỗi `sh` → dự phòng chạy đúng argv của `command:`, gửi đúng thân + task; lỗi `spawn_failed` khác → báo lỗi, không dự phòng; `command:` có ký tự shell → lỗi rõ ràng.
- Test: rotation không kill khi không xác minh được phiên kế nhiệm; vẫn kill + spawn đúng khi xác minh được.
- `cargo test`, `cargo clippy --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- Chạy thật trên máy (ghi lệnh + trích kết quả vào `.agents/log/spawn-fixes.md`, rồi **tự dọn** terminal thử bằng `hc kill`): (1) `cargo run -- spawn --task probe --role probe --name argv-t python -- -c "import sys;print(sys.argv)" --prompt "a b"` in `['-c', '--prompt', 'a b']`; (2) `cargo run -- spawn --task probe --role probe --name tpl-t auditor -- "chỉ trả lời OK rồi dừng"` mở được OpenCode và thấy brief đã gõ vào. Không chạy thử với `--role worker` (đang có thợ khác, `max_workers = 2`).

## Báo cáo

Xong: `agm send t4v --type "BÁO CÁO" --task spawn-fixes "Xong: <SHA cuối>, xem .agents/log/spawn-fixes.md"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task spawn-fixes "<câu hỏi ngắn>"`.
