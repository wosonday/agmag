# Review độc lập — spawn-fixes (fix/spawn-argv-template)

Người review: Reviewer độc lập. Phạm vi review: `git diff style/cargo-fmt HEAD` (4 commit: `12f3c53`, `7c5d98e`, `05e0f56`, `a566b03`). Không sửa mã sản phẩm.

## Kiểm tra theo yêu cầu

| # | Yêu cầu | Kết quả |
|---|---------|---------|
| 1 | `SpawnReq.text: Vec<String>`, `main.rs` không join; template (qua `Hc::templates()`) gộp task, chương trình giữ từng argv | Đúng. `src/actions.rs:224`, `src/main.rs:385-395`, `src/actions.rs:353-366`. Test `spawn_program_preserves_each_argument` (khoảng trắng + tiếng Việt + phần tử rỗng) và `fallback_child` (2 phần → 1 chuỗi) kiểm được. |
| 2 | Fallback chỉ khi `spawn_failed` + `"sh"`; đọc template `AGM_HC_TEMPLATES`/`%APPDATA%`/`~/.config`; parse `command:`, chối nháy/ký tự shell; giữ name/cwd/labels; gửi thân+task khi sẵn sàng; hết hạn → inbox; lưu `template` = tên template | Đúng phần lớn. `src/hc.rs:12-83`, `src/actions.rs:377-392`, `src/actions.rs:406`, `src/actions.rs:444-502`. Có phát hiện nhỏ ở mục "Phát hiện" (matcher `sh`). Không đổi hành vi Linux/macOS (`cfg!(windows)` tại `src/actions.rs:379`). |
| 3 | Rotation xác minh trước kill; fail-closed không kill; không tự kill Claude/Codex | Đúng cho các nhánh có test. `src/rotation.rs:401-407` gọi precheck + `prepare_rotation_spawn` trước `kill` (`:408`); `fail()` ghi inbox (`src/rotation.rs:158-170`). Quy tắc bảo vệ Claude/Codex không đổi (`src/actions.rs:40,67,504-533`). Có phát hiện "nên sửa" ở mục dưới. |
| 4 | Không đổi schema `src/store`, không thêm crate; tài liệu/đường dẫn/decision | Đúng. Diff không chạm `src/store/**`, `Cargo.toml`, `Cargo.lock`; `thiserror`/`dirs`/`tempfile` đã có sẵn. `.agents/decisions/hc-template-sh-windows.md`, `HERDR-CORE-BOT.md` §1.4, `README.md`, `README_vi.md`, `CHANGELOG.md` đều cập nhật. |
| 5 | Test thật sự kiểm các điều trên | Có. `actions_test` (A), `spawn_fallback_test` (B, 9 test), `rotation_test` (C, 2 test mới). Thiếu 1 nhánh — xem phát hiện #2. |

## Kiểm lệnh

- `cargo test --offline` → EXIT 0; 27 suite, **265 passed / 0 failed** (riêng `spawn_fallback_test` 9/9, `rotation_test` 20/20, `actions_test` 19/19).
- `cargo clippy --offline --all-targets -- -D warnings` → EXIT 0, sạch.
- `cargo fmt --check` → EXIT 0, sạch.

## Phát hiện

### 1. Mức: nên sửa — Preflight rotation chưa xác minh được đường dự phòng trên Windows

`src/actions.rs:421-428` (nhánh `listed` trong `prepare_rotation_spawn`) và `src/actions.rs:377-390` (fallback trong `spawn`).

Trên Windows, mọi template của hc đều cần đường dự phòng để mở được (`hc run <template>` lỗi `sh`). Nhưng khi template có tên trong `hc templates --json`, `prepare_rotation_spawn` trả `Ok` ngay chỉ vì `listed == true`, không kiểm tra `load_template()` — thứ mà `spawn()` bắt buộc phải đọc được ở `src/actions.rs:380`. Hệ quả: preflight "đạt", `rotation.rs:408` kill phiên cũ, rồi `spawn()` trả `Err` tại `load_template(...)?` → rotation chuyển `failed` **sau khi đã kill**, tái hiện đúng Lỗi C.

Cách tái hiện:
1. Windows; đặt `AGM_HC_TEMPLATES` tới thư mục tạm **không có** `auditor.md` (hoặc `auditor.md` có `command: opencode | cat`).
2. Dùng `setup("opencode","idle")`, đổi `terminals.template='auditor'`; `FakeHc.templates()` trả `{"name":"auditor"}`; `FakeHc.run(["auditor",…])` trả `HcFailure{spawn_failed, "cannot start \"sh\": sh: not found on PATH"}`.
3. `git` sạch + có commit mới; chạy 2 tick rotation.
4. Quan sát: `hc.calls` có `"kill t1"` dù state cuối là `failed` và terminal cũ đã chết. Kỳ vọng theo yêu cầu 3: `failed`, **không kill**, có inbox.

Đề xuất: trên Windows, với template dù có listed vẫn yêu cầu `load_template(&req.template).is_ok()` trong `prepare_rotation_spawn` (fail-closed); hoặc gộp phép kiểm này vào trước nhánh `listed`.

### 2. Mức: nhỏ — Nhánh "chương trình biết nhận brief" chưa có test rotation

`src/actions.rs:433-436` và `tests/rotation_test.rs:113-141,264-293`.

`prepare_rotation_spawn` có nhánh viết lại `req.text = ["--prompt", join(text)]` khi `template` là `opencode`, nhưng không có test rotation nào gán `terminals.template='opencode'`; tiêu chí "vẫn kill + spawn đúng khi xác minh được" chỉ được chứng minh cho nhánh template (`worker-mid`). Thêm nữa, nếu trên máy có tệp `opencode.md` trong thư mục template, `src/actions.rs:357-366` sẽ phân loại `opencode` là template và gộp `["--prompt", brief]` thành **một** phần tử argv, làm hỏng cờ `--prompt`.

Cách tái hiện: `grep -n "template='opencode'\|template: \"opencode\"" tests/rotation_test.rs` → không có kết quả. Thêm test đặt template=`opencode` và assert `run` có argv `["opencode","--prompt", <brief>]` sau khi kill.

### 3. Mức: nhỏ — Matcher `is_missing_sh` chặt hơn đặc tả

`src/hc.rs:15-23`.

Đặc tả: `spawn_failed` có `"sh"` trong thông điệp. Code chỉ nhận `"sh"`, `'sh'` hoặc `sh:`. Một thông điệp như `cannot start sh not found on PATH` (không nháy/không dấu hai chấm) sẽ bị bỏ qua, dù đúng trường hợp lỗi `sh`. Thông điệp thực tế quan sát được (`cannot start "sh": sh: not found on PATH`) thì khớp, nên rủi ro thấp; giúp giảm giả âm tính nên cân nhắc mở rộng bằng kiểm từ `sh` nhưng vẫn giữ điều kiện `kind == "spawn_failed"`.

Cách tái hiện: đơn vị test với `HcFailure{kind:"spawn_failed", message:"sh not found on PATH"}` → `is_missing_sh()` trả `false`.

### 4. Mức: nhỏ — Nhật ký bị lỗi thời, mâu thuẫn với trạng thái đã commit

`.agents/log/spawn-fixes.md:81-88`.

Mục cuối ghi "BỊ CHẶN — có tiến trình khác sửa cùng worktree", "chưa commit sản phẩm/tài liệu", "Chưa chạy toàn bộ `cargo test --offline`, clippy, fmt check". Thực tế HEAD `a566b03` đã chứa mã/test/tài liệu và 3 lệnh gates hiện đều xanh (xem bảng trên). Đây là tài liệu báo cáo bị lệch; nên cập nhật để tránh hiểu nhầm khi bàn giao (không ảnh hưởng tính đúng của mã).

## KẾT LUẬN: ĐẠT

Các yêu cầu 1–4 và phần lớn tiêu chí nghiệm thu được đáp ứng; test thật sự kiểm được hành vi chính, 265 test đạt, clippy/fmt sạch. Các phát hiện trên là lỗ hổng biên (#1) và thiếu phủ test/tài liệu (#2–#4), không làm sai các ca được đặc tả trực tiếp; riêng phát hiện #1 nên sửa trước khi coi rotation là an toàn tuyệt đối trên Windows.
