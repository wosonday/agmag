# Review độc lập — `fix/sysmon-disk-identity`

- Reviewer: reviewer độc lập (không phải thợ).
- Phạm vi: `git diff main HEAD` — 7 tệp, 213 dòng thêm / 20 dòng xóa.
- Brief của thợ: `.agents/tasks/sysmon-disk-identity.md`.
- Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`.
- Kết luận: ghi ở cuối mục 7.

## Danh sách mục kiểm

1. FFI Windows `volume_id` (`src/sysmon.rs`).
2. Unix `dev()` — cfg / nhánh biên dịch.
3. `merge_disks` — 4 ca, `None`, thứ tự `also_mounted_at`.
4. JSON / riêng tư — `volume_id` không lọt ra `--json`.
5. Test — hành vi thay thế, `#[cfg(windows)]`.
6. Phạm vi tệp / Docs.
7. Chạy lại `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`.

## 1. FFI Windows `volume_id` — không có lỗi chặn (mức `nhỏ`: một nhận xét)

Đã đối chiếu trực tiếp với chữ ký trong crate đã tải sẵn: `windows-sys-0.61.2/src/Windows/Win32/Storage/FileSystem/mod.rs:239` khai báo `GetVolumeNameForVolumeMountPointW(PCWSTR, PWSTR, u32) -> BOOL`.

- `src/sysmon.rs:292-295`: `mount.encode_utf16()` rồi đẩy `0` nếu chuỗi chưa kết thúc bằng NUL → chuỗi wide luôn có `\0`. Không có trường hợp `wide` rỗng (tối thiểu có 1 phần tử NUL).
- `src/sysmon.rs:297`: `buf = [0u16; 64]` = 64 ký tự u16. Đường dẫn GUID volume dài tối đa 49 ký tự cộng NUL = 50 → 64 đủ margin. Đúng yêu cầu "50+ ký tự" của brief.
- `src/sysmon.rs:299`: đối số độ dài truyền `buf.len() as u32` = **64 ký tự u16**, đúng đơn vị `cchBufferLength` (WCHAR, không phải byte) theo chữ ký API ở trên. Con trỏ `wide.as_ptr()` / `buf.as_mut_ptr()` hợp lệ trong suốt lời gọi; không UB.
- `src/sysmon.rs:301-303`: `BOOL` là `i32`; `ok == 0` là kiểm tra lỗi đúng, trả `None`. Không đọc `GetLastError` là chấp nhận được vì chỉ cần quyết định gộp/không gộp.
- `src/sysmon.rs:304-305`: tìm NUL bằng `position(...).unwrap_or(buf.len())` rồi `from_utf16_lossy(&buf[..len])` — không trượt chỉ số, không panic dù API ghi đầy buffer.
- Mount không có `\` cuối: API yêu cầu đối số kết thúc bằng `\`; code **không** tự thêm `\`. Hệ quả là các mount kiểu folder/đường dẫn lạ trả `FALSE` → `None` → không gộp (đúng quyết định "Không lấy được → `None`"). Cách tái hiện: `volume_id("not-a-mount")` (đã có ở `tests/sysmon_test.rs:231`) → `None`, **không** hiện hộp thoại lỗi Windows — đây là API tra cứu mount manager trong `kernel32.dll`, không gọi UI. Các mount do sysinfo liệt kê trên Windows có dạng `X:\` (`src/sysmon.rs:442`), nên trường hợp thực tế không dính nhánh này.
- mount mạng / đầu đọc thẻ không thẻ: `SysSampler::sample` bỏ qua đĩa có `total_space() == 0` ở `src/sysmon.rs:439` **trước** lời gọi ở `src/sysmon.rs:450`, nên ổ trống không bao giờ gọi FFI. Mount mạng vẫn đi qua API nhưng đây là tra cứu metadata cục bộ, không I/O mạng trên volume cục bộ; brief đã chấp nhận độ trễ này. Không có lời gọi nào có khả năng treo lâu.

`nhỏ` (ghi nhận, không cần sửa): có thể thêm bước chuẩn hóa "đảm bảo kết thúc bằng `\`" trước khi gọi để xử lý mount folder trên Windows, nhưng hiện tại failure mode đã an toàn (`None`) nên không ảnh hưởng nghiệm thu.

## 2. Unix `dev()` — không có lỗi (đúng hành vi, cfg đầy đủ)

- `src/sysmon.rs:308-312`: `#[cfg(unix)] volume_id` dùng `std::os::unix::fs::MetadataExt::dev()` trên `std::fs::metadata(mount)`. `metadata` đi theo symlink; `st_dev` là định danh filesystem của mount point.
  - Bind mount / hai path cùng một fs → cùng `dev()` → `merge_disks` gộp. Đúng kỳ vọng mục 2.
  - Hai fs khác nhau → `dev()` khác nhau → không gộp (bất kể `file_system`/dung lượng). Đúng.
  - `metadata()` lỗi (mount point cũ/không truy cập được) → `.ok()` → `None` → không gộp, đúng quy tắc "không lấy được thì chỉ gộp khi trùng mount".
- Biên dịch đa nền tảng (đọc tĩnh, không chạy cross-compile theo hướng dẫn mục 2):
  - `std::os::unix::fs::MetadataExt` và `MetadataExt::dev()` có trên cả Linux lẫn macOS → nhánh `#[cfg(unix)]` biên dịch được ở cả hai.
  - `src/sysmon.rs:314-317` có nhánh `#[cfg(not(any(windows, unix)))]` trả `None`, nên mọi target đều có ký hiệu `volume_id` — phép `use ... volume_id` không điều kiện ở `tests/sysmon_test.rs:6` không bị lỗi unresolved import.
  - `windows-sys` chỉ được `use` bên trong thân hàm có `#[cfg(windows)]` (`src/sysmon.rs:291`) và chỉ khai báo trong `[target.'cfg(windows)'.dependencies]` (`Cargo.toml:21-22`) → Linux/macOS không biên dịch/link crate này. Không dùng `libc` ở đây nên không phát sinh lỗi cfg.

`nhỏ`: không có.
