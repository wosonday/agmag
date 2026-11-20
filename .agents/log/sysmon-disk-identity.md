# sysmon-disk-identity log

## Bắt đầu
- Branch: fix/sysmon-disk-identity
- HEAD: f77f19b
- Trạng thái: clean working tree, task file đã commit ở f77f19b.

## Kế hoạch
1. Viết/sửa test cho merge_disks + volume_id (RED) → commit.
2. Triển khai DiskStats.volume_id, volume_id(), merge_disks, Cargo.toml feature, docs/changelog → test PASS → commit.
3. Chạy cargo test/clippy/fmt, chạy cargo run --offline -- sys --json, ghi kết quả vào log → commit.


## Bước 1 - Test đỏ (merge_disks + volume_id)
- Sửa 	ests/sysmon_test.rs: thay one_volume_under_several_letters_is_counted_once bằng merge_disks_uses_volume_identity (4 case theo spec) và thêm #[cfg(windows)] volume_id_reads_windows_guid_path; import olume_id.
- Kỳ vọng: fail compile vì chưa có DiskStats::with_volume_id và sysmon::volume_id, merge logic chưa đổi.
- Chạy: cargo test --offline → **FAIL** (compile errors E0432/E0599, 7 lỗi, test sysmon_test).

## Bước 2 - Triển khai (tests xanh)
- src/sysmon.rs: thêm DiskStats.volume_id: Option<String> (#[serde(skip_serializing)]) và with_volume_id builder; giữ nguyên DiskStats::new.
- Thêm olume_id(mount) cho Windows (GetVolumeNameForVolumeMountPointW, buffer 64 u16, lỗi→None), Unix (MetadataExt::dev()), còn lại None.
- SysSampler::sample: gán s.volume_id = volume_id(&mount) khi dựng danh sách.
- merge_disks: gộp khi cùng mount hoặc cùng olume_id (Some và bằng nhau); không gộp theo fs/size/free; cập nhật doc comment.
- Cargo.toml: thêm feature Win32_Storage_FileSystem cho windows-sys (target windows, crate sẵn có).
- CHANGELOG.md: thêm mục ### Sửa lỗi trong [Unreleased].
- Kiểm tra: cargo test --offline → PASS (toàn bộ suite; sysmon_test 10 passed, gồm merge_disks_uses_volume_identity và olume_id_reads_windows_guid_path).
- cargo fmt -- src/sysmon.rs tests/sysmon_test.rs, cargo fmt --all -- --check → sạch. cargo clippy --offline --all-targets -- -D warnings → sạch.

## Bước 3 - Chạy thật và kiểm tra cuối
- Lệnh: cargo run --offline -- sys --json
- Kết quả ổ đĩa (chỉ mount/fs/also_mounted_at, không ghi nhãn volume):
  - C:\  fs=NTFS  also_mounted_at=[]
  - D:\  fs=NTFS  also_mounted_at=[]
  - G:\  fs=FAT32 also_mounted_at=[]
  - H:\  fs=FAT32 also_mounted_at=[]
  - I:\  fs=FAT32 also_mounted_at=[]
- Đủ 5 ổ, cả 5 lso_mounted_at rỗng. JSON không có trường nhãn/email/volume_id.
- cargo test --offline: PASS (all suites). cargo clippy --offline --all-targets -- -D warnings: PASS. cargo fmt --check: PASS.
