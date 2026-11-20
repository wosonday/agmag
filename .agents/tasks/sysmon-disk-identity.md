---
id: sysmon-disk-identity
tier: mid
skills: test-driven-development
branch: fix/sysmon-disk-identity
worktree: D:/Project/agmag.wt/sysmon-disk-identity
---

# `merge_disks` gộp ổ theo số byte — sai cả hai chiều; gộp theo định danh volume thật

## Hiện trạng (Claude CEO kiểm 2026-11-20 trên máy này)

- `src/sysmon.rs::merge_disks` coi hai mount là cùng một volume khi trùng `file_system` + `total_bytes` + `available_bytes`. Review `.agents/reviews/sysmon-review.md` mục 2.1: hai ổ vật lý thật cùng fs/dung lượng/free → bị gộp, ổ thứ hai biến mất.
- Thực tế trên máy còn tệ hơn: `G:\`, `H:\`, `I:\` là **ba tài khoản Google Drive khác nhau** (FAT32 ảo, cả ba báo dung lượng của ổ cache `C:`). Windows cho mỗi ổ một volume GUID riêng:
  ```
  C: \\?\Volume{fdebdb19-…}\    D: \\?\Volume{560d7284-…}\
  G: \\?\Volume{c49daed4-…}\    H: \\?\Volume{b02a7157-…}\    I: \\?\Volume{b02a7163-…}\
  ```
  `agm sys --json` hiện tại: `G:` gộp `I:` (`also_mounted_at: ["I:\\"]`), còn `H:` đứng riêng vì free lệch 64 KB — gộp sai **và** không nhất quán.
- `windows-sys 0.61.2` đã là phụ thuộc (`Cargo.toml`, feature `Win32_System_Console`); `GetVolumeNameForVolumeMountPointW` nằm trong feature `Win32_Storage_FileSystem` của **cùng crate** — không thêm crate mới, build offline được.

## Quyết định của CEO

- "Cùng volume" chỉ khi **cùng mount** hoặc **cùng định danh volume thật**. Bỏ hẳn so sánh số byte.
- Windows: định danh = chuỗi trả về từ `GetVolumeNameForVolumeMountPointW(mount)` (`\\?\Volume{GUID}\`). Unix: `std::os::unix::fs::MetadataExt::dev()` của mount point (chỉ std). Không lấy được → `None` → **không gộp** (chỉ gộp khi trùng mount).
- **Không bao giờ đọc hay hiển thị nhãn volume** (`GetVolumeInformationW` / `VolumeName`): với Google Drive nhãn chính là email tài khoản.

## Yêu cầu

1. `DiskStats` thêm `volume_id: Option<String>` — **không** đưa vào JSON (`#[serde(skip)]` hoặc tương đương; nếu `DiskStats` không derive serde thì giữ nguyên cách xuất JSON hiện có). Có cách đặt giá trị cho test (builder `with_volume_id` hoặc tham số), không phá các lời gọi `DiskStats::new` cũ.
2. Hàm `volume_id(mount: &str) -> Option<String>` (cfg windows / cfg unix / còn lại `None`), gọi khi `SysSampler` dựng danh sách ổ. Windows: chuỗi wide có `\0` kết thúc, buffer 50+ ký tự (MAX GUID path = 49), lỗi → `None`. Không có lời gọi chặn/treo (API này không chạm mạng; nếu một mount mạng làm nó chậm thì vẫn chấp nhận được vì sysinfo đã liệt kê sẵn — ghi nhận trong log nếu thấy).
3. `merge_disks`: gộp khi `x.mount == d.mount` **hoặc** (`x.volume_id.is_some() && x.volume_id == d.volume_id`). Sửa doc comment cho đúng (bỏ câu "two real disks never match to the byte").
4. Header TUI "ổ đầy nhất" và mục DISKS giữ nguyên cách tính, chỉ nhận danh sách đã gộp đúng.
5. Docs: `README.md`, `README_vi.md` nếu có câu nói về gộp ổ/ổ ảo; `CHANGELOG.md` mục `[Unreleased]` → `### Sửa lỗi` một dòng.

## Phạm vi tệp

Được sửa: `src/sysmon.rs`, `Cargo.toml` (**chỉ** thêm `"Win32_Storage_FileSystem"` vào features của `windows-sys` sẵn có), `Cargo.lock` (chỉ nếu cargo tự đổi — không được thêm crate), `tests/sysmon_test.rs`, `README.md`, `README_vi.md`, `CHANGELOG.md`, `.agents/log/sysmon-disk-identity.md`.
Không sửa: `src/store/**`, các module khác. Chỉ `cargo fmt` tệp bạn sửa.

## Tiêu chí nghiệm thu

- Test thuần cho `merge_disks` (thay test `one_volume_under_several_letters_is_counted_once`, bỏ mẹo `exFAT`):
  - hai ổ cùng fs/size/free, `volume_id` khác nhau → **2 dòng**;
  - hai mount khác nhau, cùng `volume_id` → 1 dòng, `also_mounted_at` đúng, sắp theo mount;
  - `volume_id` đều `None`, số liệu trùng hết → **không gộp**;
  - cùng mount liệt kê hai lần → 1 dòng; danh sách rỗng → rỗng.
- Test Windows (`#[cfg(windows)]`): `volume_id("C:\\")` là `Some(s)` với `s` bắt đầu `\\?\Volume{`; mount không tồn tại (vd `"Q:\\"` nếu không có, hoặc chuỗi rác) → `None`, không panic.
- Chạy thật: `cargo run --offline -- sys --json` trên máy này có **5** ổ `C:\ D:\ G:\ H:\ I:\`, cả 5 `also_mounted_at` rỗng; JSON không có trường nhãn/email. Ghi lệnh + kết quả (chỉ mount/fs/also_mounted_at, **không** ghi nhãn volume) vào log.
- `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`, `cargo fmt --check` sạch.
- **Commit sau mỗi bước** (tiền tố `sysmon-disk-identity:`), ghi `.agents/log/sysmon-disk-identity.md`.

## Báo cáo

Xong: `agm send user --type "BÁO CÁO" --task sysmon-disk-identity "Xong: <SHA>"`.
Bị chặn / cần quyết định: `agm send user --type HỎI --task sysmon-disk-identity "<câu hỏi ngắn>"`.
