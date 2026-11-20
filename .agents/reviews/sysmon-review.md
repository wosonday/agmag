# Review độc lập — `feat/sysmon` (`agm sys`, theo dõi RAM/CPU/ổ đĩa theo agent)

- Nhánh: `feat/sysmon`
- Task: `.agents/tasks/sysmon-review.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (13 tệp, +1554 dòng; commit `003d024` trên `main` `0385149`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện kèm `tệp:dòng` + cách tái hiện, rồi **commit ngay** (tiền tố `sysmon-review:`), để người thay thế làm tiếp nếu model bị chặn. Không sửa mã sản phẩm. Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`.

## Tiến độ kiểm

- [ ] (1) Phụ thuộc mới `sysinfo 0.40.4`: feature, số crate kéo thêm, trùng phiên bản lớn, build offline; đọc số liệu không cần crate (ghi nhận)
- [ ] (2) Đúng số liệu: % CPU baseline, RAM/swap, `merge_disks`, tốc độ đọc/ghi, `attribute` theo cây tiến trình, `agent_of`
- [ ] (3) Chi phí/treo: refresh TUI, sleep chặn luồng, lệnh ngoài không hạn chót
- [ ] (4) Hiển thị: `agm sys` (màu/NO_COLOR/--json), TUI header, bảng `system` qua `view`, 60/110 cột, 64 nhân, lọc CSI/OSC
- [ ] (5) Không đụng CSDL: không migration, không ghi `~/.agmag`
- [ ] (6) Ghép với `main`: `quota`, `view` mặc định, `opencode-go` rows; đối chiếu `feat/sysmon-orig`
- [ ] (7) Docs: README.md, README_vi.md, CHANGELOG `[Unreleased]`
- [ ] (8) `cargo test --offline` + `cargo clippy --offline --all-targets -- -D warnings`

## Phát hiện

_(sẽ điền dần theo từng mục)_

## Kết luận

_(chưa xác định)_
