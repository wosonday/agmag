# Review độc lập — `feat/sysmon` (`agm sys`, theo dõi RAM/CPU/ổ đĩa theo agent)

- Nhánh: `feat/sysmon`
- Task: `.agents/tasks/sysmon-review.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (13 tệp, +1554 dòng; commit `003d024` trên `main` `0385149`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện kèm `tệp:dòng` + cách tái hiện, rồi **commit ngay** (tiền tố `sysmon-review:`), để người thay thế làm tiếp nếu model bị chặn. Không sửa mã sản phẩm. Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`.

## Tiến độ kiểm

- [x] (1) Phụ thuộc mới `sysinfo 0.40.4`: feature, số crate kéo thêm, trùng phiên bản lớn, build offline; đọc số liệu không cần crate (ghi nhận)
- [x] (2) Đúng số liệu: % CPU baseline, RAM/swap, `merge_disks`, tốc độ đọc/ghi, `attribute` theo cây tiến trình, `agent_of`
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

### (2) Đúng số liệu — ĐẠT có điều kiện (1 nên sửa, 1 nhỏ)

- **% CPU / baseline — đạt.** `src/sysmon.rs:315` lấy baseline CPU thứ nhất, `:317` baseline tiến trình thứ nhất, `:320` `sleep(MINIMUM_CPU_UPDATE_INTERVAL)` (200 ms), `:321` refresh tiến trình lần hai → `Process::cpu_usage()` có mốc so sánh thật (sysinfo ghi rõ "cần refresh **hai lần**", `common/system.rs:2044-2051`). `sample():342` refresh CPU, `:362`/`:368` lấy `global_cpu_usage()`/từng core. `:334-340` trả bản cũ nếu gọi lại trong vòng 200 ms và `cpu_ready=false` ở frame đầu → không có số 0% giả (test `tests/sysmon_test.rs:284-291` khẳng định `cpu n/a`).
- **RAM/swap — đạt.** `:316`/`:344` `MemoryRefreshKind::everything()`, `:353-359` lấy `total/used memory` + `total/used swap`; `pct()` (`:140-142`) trả `None` khi total=0 và kẹp ≤100 → không bao giờ in RAM ảo.
- **Tốc độ đọc/ghi — đạt.** sysinfo `DiskUsage` định nghĩa `read_bytes`/`written_bytes` là *"số byte kể từ lần refresh trước"* (`sysinfo-0.40.4/src/common/mod.rs:50-59`), không phải cumulative. Code refresh disk ở `:348`, đọc `io` ở `:412-414`, chia cho `secs` lấy từ `refreshed_at.elapsed()` đo ở `:334` = đúng khoảng giữa hai lần refresh; `:396` chặn mẫu <50 ms. (Đã đối chiếu mã nguồn sysinfo tại máy để loại trừ khả năng chia tổng tích lũy cho interval.)
- **`attribute` theo cây — đạt, không đếm đôi.** `:213-257`: mỗi tiến trình đi ngược lên **agent gần nhất** rồi cộng đúng **một** lần vào agent đó (`:234-239`); `MAX_DEPTH=64` (`:209`), tự-loops bị loại (`:226`), pid reused (cha trẻ hơn con) bị loại (`:229-232`). Ca "opencode chạy dưới claude-code" nằm trong test `tests/sysmon_test.rs:114-141`: con thuộc `opencode`, cha/ancestor thuộc `claude-code`, tổng không chồng. CPU chia cho `logical_cores` (`:241-248`) đúng quy ước sysinfo "100 = một core" (`common/system.rs:2040-2044`).

**Phát hiện 2.1 — mức: nên sửa.** `src/sysmon.rs:262-279` `merge_disks` gộp hai đĩa **vật lý khác nhau** nếu cùng `file_system` + `total_bytes` + `available_bytes` (`:268-271`). Comment `:260-261` cho rằng "two real disks never match to the byte" — **sai**: hai ổ cùng dung lượng, cùng định dạng, cùng mức đầy (vd hai SSD NTFS 1 TB mới cài, hoặc hai thành viên RAID/clone VM) có total/available **bằng đúng từng byte**. Hệ quả: DISKS section và header đếm thiếu một ổ, và ổ bị nuốt bị ghi nhầm thành `(also D:)` của ổ kia (`:273`). Test `tests/sysmon_test.rs:185-202` chỉ tránh được ca này bằng cách cho ổ thứ tư dùng `exFAT` (`:194`) — tức chính test thừa nhận cùng kích thước + cùng free + **cùng fs** sẽ bị gộp. Đây là lỗi hiển thị/đếm, không mất dữ liệu nên không chặn.
- Cách tái hiện (đã chạy chương trình Rust độc lập copy nguyên hàm `merge_disks` vào `$env:TEMP`, không sửa repo): truyền `DiskStats { mount: "C:\\", fs: "NTFS", total: 1000GB, avail: 250GB }` và `DiskStats { mount: "D:\\", fs: "NTFS", total: 1000GB, avail: 250GB }` → kết quả `so dia sau merge = 1`, `mount=C:\ also_mounted_at=["D:"]`, dù là hai đĩa khác nhau. Hướng sửa gợi ý: gộp theo volume identity (số serial/UUID thiết bị) thay vì bộ ba fs+kích thước+free; nếu không có ID thì tối thiểu không gộp hai mount khác nhau khi có nhiều hơn một ổ cùng thông số.

**Phát hiện 2.2 — mức: nhỏ.** `src/sysmon.rs:185` (`c.contains("/opencode/")`) và `:183` (`"@openai/codex"`) khớp trên toàn bộ cmd line đã lower-case. Một tiến trình `node` chạy script trong thư mục **có tên là `opencode`** nhưng không phải agent (vd `node D:/projects/opencode/server.js`) → cmd chứa `/opencode/` → bị gán nhãn `"opencode"`; tương tự `node -e "console.log('@openai/codex')"`. Test `tests/sysmon_test.rs:101-112` chỉ phủ MCP server và tên gần giống `claude-helper`, chưa có ca path/project trùng tên. Không chặn vì chỉ sai nhãn thống kê.
- Cách tái hiện: `agent_of("node", None, "node D:/projects/opencode/server.js")` → `Some("opencode")` (kỳ vọng `None`); so sánh với ca `node @modelcontextprotocol/server-filesystem` đã trả `None`.

_(các mục còn lại sẽ điền dần)_

## Kết luận

_(chưa xác định)_
