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

## 9. BỊ CHẶN — có tiến trình khác sửa cùng worktree
- Sau lệnh trên, đọc lại `src/actions.rs` thấy `prepare_rotation_spawn` đã bị di chuyển và thêm `.unwrap_or(false)` + `load_template(...).is_ok()` mà phiên này chưa viết. `spawn_fallback_test.rs` và nhiều tệp bị format dù phiên này chưa chạy cargo fmt.
- `git status --short` hiện có 51 tệp tracked đổi, gồm src/opencode.rs, src/report.rs, src/policy.rs, src/store/** ngoài phạm vi; README/CHANGELOG/decision cũng được tiến trình khác tạo/sửa. Không hoàn tác, không stage những thay đổi đó.
- Dừng để tránh ghi đè/commit nhầm mã của tiến trình khác. Chỉ commit log này. Mã sản phẩm và test để nguyên chưa commit, cần người quản lý xác định ai đang chạy cùng worktree.
- Các tệp phiên này trực tiếp đã sửa: src/main.rs, src/actions.rs, src/hc.rs, src/rotation.rs, tests/actions_test.rs, tests/common/mod.rs, tests/rotation_test.rs, tests/spawn_fallback_test.rs, .agents/log/spawn-fixes.md.
- Kết quả GREEN gần nhất trước nhiễu: actions spawn 4/0, rotation 20/0, fallback 8/0. Smoke Python argv và template brief đều đạt, terminal t5b/t5g đã hc kill. Template agent gặp quota Go sau khi brief đã gửi (không sửa mã).
- Chưa chạy toàn bộ cargo test --offline, clippy, fmt check; chưa thể xác nhận phạm vi sạch hay hoàn tất task. Không tạo subagent, không push/merge/rebase, không đổi phụ thuộc, không stage runtime.
- Lệch task: dừng giữa chừng, chưa commit sản phẩm/tài liệu và chưa chạy gates cuối do concurrent writer; slice local-template chưa có RED nên chưa coi là TDD hoàn tất.
