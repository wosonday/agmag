---
id: sysmon-disk-identity-review
branch: fix/sysmon-disk-identity
worktree: D:/Project/agmag.wt/sysmon-disk-identity
---

# Review độc lập — `fix/sysmon-disk-identity`

Brief của thợ: `.agents/tasks/sysmon-disk-identity.md` (đọc trước). Phạm vi: `git diff main HEAD` (~190 dòng). CEO đã chạy: 309 test đạt, clippy/fmt sạch, `agm sys --json` có 5 ổ riêng `C D G H I`, không trường nhãn/GUID.

## Mục kiểm (mỗi mục: phát hiện kèm `tệp:dòng` + cách tái hiện, rồi commit)

1. **FFI Windows** `volume_id` (`src/sysmon.rs`): chuỗi wide kết thúc `\0`; kích thước buffer và đối số độ dài đúng đơn vị (ký tự u16); xử lý lỗi `== 0`; không UB; mount không có `\` cuối hoặc mount mạng/ổ trống (đầu đọc thẻ không thẻ) → `None` hay hiện hộp thoại lỗi Windows? Có lời gọi nào có thể treo lâu không.
2. **Unix** `dev()`: bind mount / cùng fs → gộp (đúng); hai fs khác → không gộp. Có nhánh nào không biên dịch trên Linux/macOS (đọc kỹ cfg, không cần chạy).
3. **`merge_disks`**: đúng 4 ca trong brief; `None` không bao giờ gộp với `None`; thứ tự `also_mounted_at` ổn định.
4. **JSON / riêng tư**: `volume_id` không xuất hiện trong `--json`; serde (`skip_serializing`) không làm hỏng deserialize nếu `DiskStats` có `Deserialize`; không có chỗ nào đọc nhãn volume (`GetVolumeInformationW`, `VolumeName`, `Win32_LogicalDisk`).
5. **Test**: test thay thế có kiểm đúng hành vi (không còn mẹo `exFAT`), test Windows có `#[cfg(windows)]`.
6. **Phạm vi/Docs**: chỉ các tệp được phép; `Cargo.lock` không đổi; CHANGELOG `[Unreleased]` đúng.
7. Chạy lại `cargo test --offline`, `cargo clippy --offline --all-targets -- -D warnings`; ghi số đạt/lỗi.

## Quy tắc
- Không sửa mã sản phẩm; chỉ tạo `.agents/reviews/sysmon-disk-identity-review.md`. Tạo tệp ngay từ đầu, commit sau mỗi mục (tiền tố `sysmon-disk-identity-review:`).
- Mức: `chặn` / `nên sửa` / `nhỏ`. Kết luận **ĐẠT** (không `chặn`) hoặc **CHƯA ĐẠT**.
- Không ghi nhãn volume/email vào tệp review.

Báo cáo: `agm send user --type "BÁO CÁO" --task sysmon-disk-identity-review "ĐẠT|CHƯA ĐẠT: <SHA>, <chặn>/<nên sửa>/<nhỏ>"`.
