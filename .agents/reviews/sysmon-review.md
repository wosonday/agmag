# Review độc lập — `feat/sysmon` (`agm sys`, theo dõi RAM/CPU/ổ đĩa theo agent)

- Nhánh: `feat/sysmon`
- Task: `.agents/tasks/sysmon-review.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (13 tệp, +1554 dòng; commit `003d024` trên `main` `0385149`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện kèm `tệp:dòng` + cách tái hiện, rồi **commit ngay** (tiền tố `sysmon-review:`), để người thay thế làm tiếp nếu model bị chặn. Không sửa mã sản phẩm. Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`.

## Tiến độ kiểm

- [x] (1) Phụ thuộc mới `sysinfo 0.40.4`: feature, số crate kéo thêm, trùng phiên bản lớn, build offline; đọc số liệu không cần crate (ghi nhận)
- [ ] (2) Đúng số liệu: % CPU baseline, RAM/swap, `merge_disks`, tốc độ đọc/ghi, `attribute` theo cây tiến trình, `agent_of`
- [ ] (3) Chi phí/treo: refresh TUI, sleep chặn luồng, lệnh ngoài không hạn chót
- [ ] (4) Hiển thị: `agm sys` (màu/NO_COLOR/--json), TUI header, bảng `system` qua `view`, 60/110 cột, 64 nhân, lọc CSI/OSC
- [ ] (5) Không đụng CSDL: không migration, không ghi `~/.agmag`
- [ ] (6) Ghép với `main`: `quota`, `view` mặc định, `opencode-go` rows; đối chiếu `feat/sysmon-orig`
- [ ] (7) Docs: README.md, README_vi.md, CHANGELOG `[Unreleased]`
- [ ] (8) `cargo test --offline` + `cargo clippy --offline --all-targets -- -D warnings`

## Phát hiện

### (1) Phụ thuộc mới `sysinfo 0.40.4` — ĐẠT (1 phát hiện nhỏ)

- `Cargo.toml:19`: `sysinfo = { version = "0.40.4", default-features = false, features = ["system", "disk"] }` — đúng yêu cầu "chỉ bật `system`, `disk`"; `component`/`gpu`/`network`/`user` bị tắt.
- **Kéo thêm bao nhiêu crate:** `Cargo.lock` có **15 `[[package]]` mới**: `sysinfo 0.40.4`, `ntapi 0.4.3`, `windows 0.62.2`, `windows-core 0.62.2`, `windows-collections 0.3.2`, `windows-future 0.3.2`, `windows-implement 0.60.2`, `windows-interface 0.59.3`, `windows-numerics 0.3.1`, `windows-result 0.4.1`, `windows-strings 0.5.1`, `windows-threading 0.2.1`, `objc2-core-foundation 0.3.2`, `objc2-io-kit 0.3.2`, `syn 2.0.119`. (`windows-link 0.2.1`, `windows-sys 0.61.2`, `libc`, `memchr` **đã có** trong `main`.) Lưu ý: dù đã `default-features = false`, chính hai feature `system` + `disk` vẫn khai `dep:windows` và `dep:objc2-*` (`sysinfo-0.40.4/Cargo.toml` features `system`/`disk`) — các crate này là **target-gated** (`windows-*` chỉ build trên Windows, `objc2-*` chỉ trên macOS); trên Linux phần tăng thực tế là `sysinfo` + `libc`/`memchr` có sẵn.
- **Trùng phiên bản lớn:** có — `syn 2.0.119` (`Cargo.lock:623`) mới cạnh `syn 3.0.10` đã có trong `main` (`Cargo.lock` dòng ~593). Đây là dependency build-time (proc-macro, qua `windows-implement`/`windows-interface`), không ảnh hưởng runtime binary, chỉ tăng thời gian build.
- **Build offline:** `cargo build --offline` → `Finished dev profile ... in 0.18s`, `EXIT=0`, không cần mạng.
- **Đọc số liệu không cần crate (ghi nhận):** có thể — Linux đọc trực tiếp `/proc/stat`, `/proc/meminfo`, `/proc/mounts`, `/proc/diskstats`; Windows dùng Win32 PDH / `GetProcessMemoryInfo` / `GetDiskFreeSpaceEx` qua FFI. Không bắt buộc đổi; `sysinfo` lựa chọn hợp lý để đa nền tảng.

**Phát hiện 1.1 — mức: nhỏ.** `Cargo.lock:623`: `syn 2.0.119` trùng **major** với `syn 3.0.10` đã có. Chỉ là proc-macro build-time, không chặn.
- Cách tái hiện: `git show main:Cargo.lock | Select-String 'name = "syn"' -Context 0,1` → `3.0.10`; `Select-String 'name = "syn"' Cargo.lock` → thêm `2.0.119`.

_(các mục còn lại sẽ điền dần)_

## Kết luận

_(chưa xác định)_
