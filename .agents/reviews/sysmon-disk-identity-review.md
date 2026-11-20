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
