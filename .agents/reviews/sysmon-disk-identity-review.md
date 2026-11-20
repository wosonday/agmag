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

## 3. `merge_disks` — đúng cả 4 ca, không có lỗi

Logic nằm ở `src/sysmon.rs:271-285`:

- `src/sysmon.rs:272`: sắp xếp input theo `mount` tăng dần → thứ tự xử lý cố định.
- `src/sysmon.rs:275-277`: điều kiện gộp đúng brief — `x.mount == d.mount || (x.volume_id.is_some() && x.volume_id == d.volume_id)`. Không còn so sánh `file_system` / `total_bytes` / `available_bytes`.
- `src/sysmon.rs:278-282`: ba nhánh rõ ràng — mount khác + cùng volume → append vào `also_mounted_at`; **cùng mount** → `Some(_) => {}` (bỏ bản trùng, không tạo dòng thừa, không tự đưa mount vào `also_mounted_at`); không tìm thấy → push dòng mới.

| Ca theo brief | Kết quả code | Tái hiện |
|---|---|---|
| Cùng fs/size/free, `volume_id` khác nhau → 2 dòng | 2 dòng `G:\`, `H:\`, `also_mounted_at` rỗng | `tests/sysmon_test.rs:188-194` |
| Khác mount, cùng `volume_id` → 1 dòng, sắp theo mount | `mount = G:\`, `also_mounted_at = [H:\]` dù input `H` trước `G` | `tests/sysmon_test.rs:197-203` |
| Cả hai `None`, số liệu trùng → không gộp | 2 dòng (nhờ điều kiện `x.volume_id.is_some()` ở `src/sysmon.rs:276`) | `tests/sysmon_test.rs:206-210` |
| Cùng mount liệt kê 2 lần → 1 dòng; danh sách rỗng → rỗng | 1 dòng `C:\`, `also_mounted_at` rỗng; `Vec::new()` → rỗng | `tests/sysmon_test.rs:213-220` |

- `None` không bao giờ gộp với `None`: chỉ vế `x.mount == d.mount` mới có thể gộp khi cả hai `None`, đúng quy định.
- Thứ tự `also_mounted_at`: do input đã sort ở `src/sysmon.rs:272` và append theo thứ tự duyệt ở `src/sysmon.rs:279`, kết quả luôn theo mount tăng dần → ổn định.
- Doc comment `src/sysmon.rs:268-270` đã sửa, không còn câu "two real disks never match to the byte".
- Độc lập với phần hiển thị: `src/dashboard.rs` / header TUI không đổi trong diff, chỉ nhận `Vec<DiskStats>` đã gộp từ `src/sysmon.rs:456`.

`nhỏ`: không có.

## 4. JSON / riêng tư — không có lỗi; một nhận xét `nhỏ`

- `src/sysmon.rs:51`: `DiskStats` chỉ `#[derive(Debug, Clone, PartialEq, Serialize)]` — **không** có `Deserialize`. Vậy câu hỏi "serde `skip_serializing` có làm hỏng deserialize không" là không áp dụng: không có đường deserialize nào cho struct này (đã grep toàn bộ `*.rs`: chỉ các struct ở `config.rs`, `model.rs`, `launch.rs` derive `Deserialize`).
- `src/sysmon.rs:66-67`: `#[serde(skip_serializing)] pub volume_id: Option<String>`. Thuộc tính đúng để giả trường khỏi JSON. Brief cho phép "`#[serde(skip)]` hoặc tương đương" → `skip_serializing` là tương đương hợp lệ.
- Đường xuất JSON: `src/main.rs:266-268` gọi `emit(json, &stats, ..)` với `stats: SysStats`; `src/main.rs:684-688` dùng `serde_json::to_string_pretty`. `SysStats` chứa `Vec<DiskStats>` (`src/sysmon.rs:116-120`) nên trường được bỏ qua ở mọi cấp. Cách tái hiện: `cargo run --offline -- sys --json` → JSON có `mount`, `file_system`, `also_mounted_at`, ... nhưng không có khóa `volume_id`.
- Không có chỗ nào đọc nhãn volume: grep toàn repo cho `GetVolumeInformation` / `VolumeName` / `Win32_LogicalDisk` → **không có kết quả mã nguồn**. Chuỗi `GetVolumeNameForVolumeMountPointW` (`src/sysmon.rs:291`, doc ở `:288`) là API lấy GUID mount, không phải API đọc nhãn; feature `Win32_Storage_FileSystem` (`Cargo.toml:22`) chỉ mở module chứa API đó, không tự gọi API nhãn. Phần hiển thị (`src/dashboard.rs:846-848`, `src/sysmon.rs:101-103`) chỉ dùng `mount` / `also_mounted_at`, không đụng đến `volume_id`.

`nhỏ` (phòng sau này): nếu sau này `DiskStats` được thêm `Deserialize`, nên đổi sang `#[serde(skip)]` để JSON cũ thiếu `volume_id` vẫn deserialize được. Hiện tại không ảnh hưởng nghiệm thu. Không ghi nhãn/email nào vào tệp review này.

## 5. Test — đúng yêu cầu; một nhận xét `nhỏ`

- `tests/sysmon_test.rs:185-221`: test cũ `one_volume_under_several_letters_is_counted_once` đã bị thay bằng `merge_disks_uses_volume_identity`, kiểm tra đúng 4 ca của brief bằng `volume_id` giả lập (`src/sysmon.rs:92-95` `with_volume_id`), không còn dựa vào mẹo `exFAT`. Grep toàn `tests/*.rs`: không còn chuỗi `exFAT`, không còn tên test cũ.
- `tests/sysmon_test.rs:223-232`: test Windows có `#[cfg(windows)]` ở dòng 223; kiểm tra `volume_id("C:\\")` trả `Some` bắt đầu `\\?\Volume{` (dòng 226-228) và `volume_id("not-a-mount")` → `None`, không panic (dòng 231) — đúng brief (dùng "chuỗi rác" được phép). Trên Windows test này được biên dịch và chạy; trên Linux/macOS bị loại bỏ bởi cfg.
- Import `volume_id` ở `tests/sysmon_test.rs:6` không điều kiện nhưng hợp lệ nhờ 3 nhánh cfg ở `src/sysmon.rs:289/308/314` (đã kiểm ở mục 2).
- Không có test nào dùng dữ liệu thật lộ nhãn volume; test chỉ dùng mount giả lập và chuỗi `vol-*` tự đặt.

`nhỏ` (đề xuất, không chặn): chưa có test unit khẳng định `serde_json::to_string(&SysStats{...})` không chứa khóa `volume_id` (grep `tests/*.rs` không thấy). Việc này đang được CEO xác nhận bằng chạy thật `agm sys --json` (mục 4), nhưng một test regression nhỏ sẽ khóa chặt thuộc tính riêng tư này về sau.

## 6. Phạm vi tệp / Docs — đúng; một ghi chú quy trình

Tại thời điểm bắt đầu review, `git diff main HEAD --stat` có 7 tệp:

| Tệp | Trạng thái | Đánh giá |
|---|---|---|
| `src/sysmon.rs` | đổi | được phép |
| `tests/sysmon_test.rs` | đổi | được phép |
| `Cargo.toml` | đổi 1 dòng | được phép — **chỉ** thêm `"Win32_Storage_FileSystem"` vào feature của `windows-sys` có sẵn, không thêm crate |
| `CHANGELOG.md` | đổi | được phép |
| `.agents/log/sysmon-disk-identity.md` | mới | được phép; nội dung chỉ ghi `mount` / fs / `also_mounted_at`, không có nhãn volume hay email |
| `.agents/tasks/sysmon-disk-identity.md` | mới | tệp brief của quy trình (commit `f77f19b`), không phải mã sản phẩm |
| `.agents/tasks/sysmon-disk-identity-review.md` | mới | tệp brief review (commit `a4efcc2`), không phải mã sản phẩm |

- `Cargo.lock`: `git diff main HEAD -- Cargo.lock` → **rỗng**. Thêm feature cho crate có sẵn không làm đổi lock file, đúng brief.
- Không đụng `src/store/**` hay module khác; `git diff main HEAD --name-only` xác nhận chỉ các tệp trên (cộng thêm tệp review này do reviewer tạo theo quy tắc review).
- `CHANGELOG.md:3-11`: nằm đúng `## [Unreleased]` → `### Sửa lỗi` (dòng 9) với đúng **một** dòng mô tả (dòng 11), trước `## [0.3.0]`. ✅
- `README.md` / `README_vi.md`: không đổi. Grep không thấy câu nào nói về gộp ổ / ổ ảo cần cập nhật (chỉ mô tả chung "mỗi ổ đĩa" ở `README.md:218`, `README_vi.md:214`), nên việc giữ nguyên là phù hợp với điều kiện "nếu có câu nói về gộp ổ/ổ ảo" của brief. ✅

`nhỏ` (quy trình, không phải lỗi mã): hai tệp `.agents/tasks/*.md` nằm ngoài danh sách "Phạm vi tệp" của brief nhưng là artifact của khâu giao việc/review đã được commit riêng, không phải thay đổi sản phẩm của thợ.

## 7. Chạy lại test / clippy — sạch; kết luận **ĐẠT**

Lệnh và kết quả (chạy lại độc lập trên worktree này, ngày 2026-11-20):

| Lệnh | Kết quả |
|---|---|
| `cargo test --offline` | `EXIT=0` — **309 test đạt, 0 lỗi, 0 ignored** (30 suites + doc-tests 0). Trong đó `sysmon_test`: 10 passed, 0 failed — gồm `merge_disks_uses_volume_identity` và `volume_id_reads_windows_guid_path` (chạy thật vì máy này là Windows) |
| `cargo clippy --offline --all-targets -- -D warnings` | `EXIT=0` — `Finished dev profile`, không warning, không error |
| `cargo fmt --all -- --check` (kiểm thêm) | `EXIT=0` — không khác biệt định dạng |

Số liệu khớp với báo cáo của CEO (309 test) → không có sai lệch cần điều tra.

## Tổng kết

| Mức | Số lượng | Chi tiết |
|---|---|---|
| `chặn` | **0** | — |
| `nên sửa` | **0** | — |
| `nhỏ` | **4** | mục 1: mount thiếu `\` cuối → trả `None` an toàn, có thể chuẩn hóa thêm; mục 4: cân nhắc `#[serde(skip)]` nếu sau này derive `Deserialize`; mục 5: nên thêm test regression cho việc JSON không chứa `volume_id`; mục 6: hai tệp brief `.agents/tasks/*.md` là artifact quy trình |

**Kết luận: ĐẠT.** Không có phát hiện mức `chặn`. Việc gộp ổ theo định danh volume thật được triển khai đúng 4 ca, FFI Windows đúng đơn vị/ký tự và fail-safe, Unix dùng `dev()` chuẩn, `volume_id` không lọt ra JSON và không có API đọc nhãn volume nào được gọi. Các điểm `nhỏ` chỉ là hardening/đề xuất cho tương lai.

Đối tượng review: implementation HEAD `a4efcc2` (trước các commit review này), phạm vi `git diff main HEAD`.
