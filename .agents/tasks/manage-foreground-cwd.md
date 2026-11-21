---
id: manage-foreground-cwd
tier: mid
skills: test-driven-development
branch: fix/manage-foreground-cwd
worktree: D:/Project/agmag.wt/manage-foreground-cwd
---

# `agm manage` / xoay vòng Claude-Codex dùng nhầm thư mục gốc của shell

## Hiện trạng (Claude CEO kiểm 2026-11-21 trên máy này)

Terminal `t3p`: hc mở `pwsh` ở `D:\Project\agmag`, người dùng `cd D:\Project\academic-writing` rồi gõ `claude`. `hc get t3p`:

```
"cwd": "D:\\Project\\agmag",                       ← thư mục lúc tạo shell
"foreground": { "argv": [pwsh…], "cwd": "D:\\Project\\academic-writing", … }   ← thư mục thật hiện tại
"status": { "app": "claude-code", … }
```

Hệ quả (đọc mã):
1. `ops::manage` (`src/ops.rs`) lấy `cwd` qua `hc::terminal_row_from` → trường gốc `cwd` → `LaunchSpec.cwd = D:\Project\agmag` (sai dự án).
2. `rotation::start` (`src/rotation.rs` ~dòng 217) ghi `t.cwd` (cũng là `D:\Project\agmag`) vào `rotations.cwd`; bước `requested` ghi tệp bàn giao vào `<rotations.cwd>/.agents/handoff/…`; bước `handoff_ready` (~dòng 668) **ghi đè** `spec.cwd = r.cwd`. → bàn giao ghi vào repo agmag, phiên kế nhiệm mở ở agmag.
3. Ngoài ra `manage` báo "không đọc được lệnh chạy claude-code" vì foreground là `pwsh` (claude là tiến trình con) — đây là hành vi đúng, người dùng sẽ truyền `-- claude`. Không đổi.

## Quyết định của CEO

- **Thư mục hiệu lực** của một terminal khi cần biết "agent đang làm ở đâu" = `foreground.cwd` nếu có và không rỗng, ngược lại `cwd` gốc. Viết một hàm thuần (vd `hc::effective_cwd(&Value) -> Option<String>`) có test.
- `manage` dùng thư mục hiệu lực cho `LaunchSpec.cwd` (câu hỏi xác nhận `[y/N]` đã in `cwd=` → người dùng thấy đúng thư mục trước khi gõ `y`).
- Với terminal **đã có `launch_spec`** (Claude/Codex được quản lý), `rotation::start` ghi `rotations.cwd` = `spec.cwd` (thư mục người dùng đã xác nhận), **không** dùng `t.cwd`. Terminal không có `launch_spec` (OpenCode do hc sinh): giữ nguyên hành vi cũ. Bước `handoff_ready` sau đó tự nhất quán (r.cwd == spec.cwd).
- **Không** đổi `terminal_row_from` chung / cách daemon lưu `terminals.cwd` (tránh ảnh hưởng ghép phiên theo thư mục và tránh giá trị nhảy qua lại giữa sự kiện có/không có `foreground`). Nếu thấy chỗ khác cũng nên dùng thư mục hiệu lực, chỉ **ghi vào log** như đề xuất, không sửa.

## Phạm vi tệp

Được sửa: `src/hc.rs` (thêm hàm), `src/ops.rs` (`manage`), `src/rotation.rs` (`start`), tests (`tests/actions_test.rs`, `tests/cli_test.rs`, hoặc test rotation hiện có — tìm nơi đang test `rotation::start`/xoay vòng managed), `CHANGELOG.md` (`[Unreleased]` → `### Sửa lỗi`, giữ LF), `.agents/log/manage-foreground-cwd.md`.
Không sửa: schema, daemon, quy tắc xác nhận người dùng (`caller`, `set_managed_launch`) — tuyệt đối không nới.

## Tiêu chí nghiệm thu (test trước — đỏ — rồi mới code)

- `effective_cwd`: có `foreground.cwd` → nó; `foreground.cwd` rỗng/thiếu → `cwd`; cả hai thiếu → `None`.
- Rotation: terminal có `launch_spec` với `cwd = A`, `terminals.cwd = B` → sau `rotation::start`, `rotations.cwd = A`; terminal không có `launch_spec` → `B` (như cũ).
- Nếu có test CLI/fixture `hc get` giả cho `manage`, thêm ca `foreground.cwd` khác `cwd` và kiểm `launch_spec` lưu thư mục foreground (dùng `AGM_HOME` tạm; người gọi là user đã xác nhận như các test sẵn có).
- `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- **Commit sau mỗi bước** (tiền tố `manage-foreground-cwd:`), ghi `.agents/log/manage-foreground-cwd.md`. Không chạy `agm manage` trên terminal thật.

## Báo cáo

Xong: `agm send user --type "BÁO CÁO" --task manage-foreground-cwd "Xong: <SHA>"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task manage-foreground-cwd "<câu hỏi ngắn>"`.
