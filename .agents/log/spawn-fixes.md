# spawn-fixes

## 1. Khảo sát
- Đã đọc task, Global Constraints, actions/hc/rotation/main và tests hiện có.
- Nhánh `fix/spawn-argv-template`, worktree sạch. Không tạo agent, không đổi phụ thuộc/schema.
- Thiết kế theo task: mock Hc tại `actions::spawn` và `rotation::tick`; giữ argv, chỉ gộp với template có tên trong `hc templates --json`.
- Thứ tự: A argv → B fallback sh + gửi brief có hạn → C preflight trước kill → tài liệu/kiểm thật/checks/commit.
- Tiền tố commit dùng `spawn-fixes:` theo yêu cầu riêng, thay cho `t<N>:` trong Global Constraints.

## 2. TDD A — viết test
- Thêm test argv chương trình gồm khoảng trắng/tiếng Việt/rỗng; chuyển test template sang nhiều phần task.
- RED: `cargo test --offline --test actions_test spawn_` lỗi biên dịch E0308 (2 vị trí), API còn Option<String>.
- Code: SpawnReq.text thành Vec, main không join; trait Hc.templates + HcCli gọi JSON, chỉ template gộp task.
- GREEN: cùng lệnh đạt 4, lỗi 0.

## 3. TDD B — fallback template
- Thêm mock lỗi có kind/message, test child process với AGM_HC_TEMPLATES tạm (không đổi env toàn cục trong tiến trình test).
- Kiểm argv command, name/cwd/labels giữ nguyên, brief thân + task và template lưu CSDL.
- RED: `cargo test --offline --test spawn_fallback_test` E0432, chưa có HcFailure.
- Code: lỗi hc giữ kind/message (kể cả exit khác 0), fallback chỉ Windows + template + spawn_failed/sh, parser front matter bảo thủ, gửi khi idle/done trong 20s; không gửi được mở inbox kèm brief.
- GREEN: `cargo test --offline --test spawn_fallback_test` đạt 9, lỗi 0.

## 4. TDD C — xác minh trước kill
- Thêm `actions::prepare_rotation_spawn` kiểm template qua hc hoặc đường dự phòng; chương trình OpenCode nhận brief qua `--prompt`; còn lại báo không xác minh.
- `rotation::verify_and_respawn` gọi precheck + prepare trước kill; không kill nếu thất bại.
- GREEN: `cargo test --offline --test rotation_test` đạt 20, lỗi 0.

## 5. Tài liệu
- `.agents/decisions/hc-template-sh-windows.md` đã viết.
- `HERDR-CORE-BOT.md` §1.4 sửa đường dẫn template Windows.
- `README.md`, `README_vi.md`, `CHANGELOG.md` cập nhật mục spawn.

## 6. Kiểm tra
- `cargo test --offline`: toàn bộ đạt, 0 lỗi.
- `cargo clippy --all-targets -- -D warnings`: sạch.
- `cargo fmt --check`: sạch.

## 7. Chạy thật
1. `cargo run --offline -- spawn --task probe --role probe --name argv-t python -- -c "import sys;print(sys.argv)" --prompt "a b"` → terminal `t5h`, `hc read t5h` in `['-c', '--prompt', 'a b']`.
2. `cargo run --offline -- spawn --task probe --role probe --name tpl-t auditor -- "chỉ trả lời OK rồi dừng"` → terminal `t5k`, OpenCode mở và thấy brief `chỉ trả lời OK rồi dừng` đã gõ.
- Dọn: `hc kill t5h`, `hc kill t5k` (cả hai đã finished).
- GREEN: fallback test đạt 2, lỗi 0.

## 4. TDD C — rotation preflight
- Thêm test template/chương trình không biết: rotation phải failed + inbox, phiên cũ live, không kill/run.
- RED: test mới 0 đạt / 1 lỗi, rotation thực tế `done` thay vì `failed`.
- Code: preflight xác nhận tên qua hc; với chương trình OpenCode dùng argv `--prompt`, chương trình chưa biết fail-closed. Không thay quyền kill Claude/Codex.
- GREEN: rotation suite 20 đạt / 0 lỗi.

## 5. TDD bổ sung — metadata và lỗi không liên quan
- Mock trả metadata template=command ở đường direct: tên template gốc phải vẫn được lưu.
- Bổ sung kiểm không fallback cho lỗi khác và từ chối cả 9 dạng nháy/ký tự shell.
- RED: fallback suite 4 đạt / 1 lỗi (metadata `opencode` thay vì `auditor`).
- Code: luôn ghi template người dùng yêu cầu vào hàng terminal.
- GREEN: fallback suite 5 đạt / 0 lỗi.

## 6. Kiểm thật vòng 1 / TDD stderr
- `cargo run --offline -- spawn --task probe --role probe --name argv-t python -- -c "import sys;print(sys.argv)" --prompt "a b"` → t5b.
- `hc read --json argv-t` → `{"complete":true,"how":"screen","text":"['-c', '--prompt', 'a b']"}`.
- Đã `hc kill --json argv-t` (exit 0, argv đúng 5 phần).
- Template smoke: lệnh task với `--offline` trả lỗi sh, chưa tạo terminal. Phát hiện JSON lỗi ở stderr chứ không phải stdout khi exit 1; thêm test HcCli với fake .cmd stderr.
- Có thay đổi §1.4 HERDR-CORE-BOT.md xuất hiện từ ngoài trong lúc thực hiện (status đầu sạch; chưa tự chỉnh tệp này); giữ nguyên nội dung phù hợp task, ghi nhận để tránh ghi đè.
- RED: test stderr 0 đạt / 1 lỗi; lỗi không downcast được về HcFailure.
- Code: đọc error JSON từ cả stdout và stderr trước khi xử lý exit status.
- GREEN: fallback suite 6 đạt / 0 lỗi.
- Kiểm thật template lần 2: `cargo run --offline -- spawn --task probe --role probe --name tpl-t auditor -- "chỉ trả lời OK rồi dừng"` → t5g.
- `hc get --json tpl-t`: argv ["opencode"], nhãn probe/probe, reported=true, status opencode/idle, turn=1.
- `hc read --json tpl-t`: thấy toàn bộ thân "Bạn là Audit..." + dòng "chỉ trả lời OK rồi dừng" đã gõ vào TUI. Model hiện tại báo "OpenCode Go 5 hour usage limit reached"; không đổi model/quota (ngoài phạm vi).
- Đã `hc kill --json tpl-t` → reason=killed, state=finished. Không dùng role worker.

## 7. TDD readiness/timeout
- Thêm test không gõ brief vào shell đang idle thay vì agent; sau thời hạn phải có inbox chứa brief, không mất nội dung.
- RED: timeout test 0 đạt / 1 lỗi, code gõ vào pwsh idle.
- Code: readiness cần đúng app của command và idle/done.
- GREEN: fallback suite 8 đạt / 0 lỗi (timeout 20s).

## 8. TDD xác minh bằng đường dự phòng
- Thêm tình huống `hc templates` không khả dụng nhưng template tạm hợp lệ: preflight và spawn vẫn phải dùng được đường đọc local.
- Lệnh `cargo test --offline --test spawn_fallback_test local_template_can_verify_and_spawn_when_listing_is_unavailable` đạt 1/0 ngay, không RED.

## 10. Vòng sửa 1 (2026-11-20)
- Đọc `.agents/tasks/spawn-fixes.md`, `.agents/reviews/spawn-fixes-review.md`, log cũ, git status/diff/diff --cached: worktree sạch, HEAD `b54f041 spawn-fixes: log`.
- Phạm vi sửa vòng này: `src/actions.rs`, `tests/rotation_test.rs`, `.agents/log/spawn-fixes.md` (không chạm tệp ngoài scope; #3 `is_missing_sh` giữ nguyên).

### #1 Bắt buộc — preflight rotation phải kiểm tra đường dự phòng trên Windows khi template được liệt kê
- Test thêm: `rotation_listed_template_with_bad_disk_fallback` (+ child) qua `AGM_HC_TEMPLATES`, 2 trường hợp tệp `worker-mid.md` thiếu `command:` hoặc có ký tự shell. Nhánh mong đợi `#[cfg(windows)]`/`cfg!(windows)`: preflight lỗi, rotation `failed`, không kill/run, terminal cũ còn live; ngoài Windows giữ hành vi cũ (listed đủ, rotation `done`).
- RED: `cargo test --offline --test rotation_test rotation_listed_template_with_bad_disk_fallback` → FAIL (child: `state left:"done" right:"failed"` trên Windows, chưa kiểm tra `load_template` khi listed).
- Code: `src/actions.rs::prepare_rotation_spawn` — nếu `listed` thì trên Windows gọi `crate::hc::load_template(&req.template)?` trước khi trả `Ok`; ngoài Windows trả `Ok` như cũ.
- GREEN: lệnh trên đạt 2/0.

### #2 Nhỏ — test rotation nhánh OpenCode trần và template thắng
- Thêm `rotation_bare_opencode_uses_prompt_flag` (+ child, `AGM_HC_TEMPLATES` trống): template terminal `opencode` không phải template hc/disk → preflight xác minh, kill + spawn với argv `["opencode", "--prompt", <resume>]`.
- Thêm `rotation_opencode_template_file_wins_over_program_path` (+ child, có `opencode.md`): `load_template`/template treatment thắng, argv kế nhiệm không bị viết lại thành `--prompt` riêng (task gộp theo nhánh template).
- Kết quả: `rotation_bare_opencode_uses_prompt_flag` 2/0, `rotation_opencode_template_file_wins_over_program_path` 1/0 (PASS). Nhánh bare opencode đã đúng sẵn; test ghi rõ hành vi và ưu tiên template.

### #4 Nhật ký
- Bỏ mục "BỊ CHẶN — có tiến trình khác sửa cùng worktree / chưa commit / chưa chạy gates" đã lỗi thời; thêm mục Vòng sửa 1.

### Định dạng và gates
- `rustfmt --edition 2024 src/actions.rs tests/rotation_test.rs`.
- `cargo fmt --check` → sạch.
- `cargo clippy --offline --all-targets -- -D warnings` → sạch.
- `cargo test --offline` → toàn bộ PASS, 0 failed (gồm `rotation_test` 26 passed, `spawn_fallback_test` 9 passed, các suite khác xanh).
