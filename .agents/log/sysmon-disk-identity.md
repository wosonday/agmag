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
