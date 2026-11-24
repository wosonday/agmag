---
id: created-ms-identity
tier: mid
skills: test-driven-development, systematic-debugging
branch: fix/created-ms-identity
worktree: D:/Project/agmag.wt/created-ms
---

# Sửa: `agm cleanup` không dọn được phiên đã bàn giao vì `created_ms` trong DB sai

## Triệu chứng (dữ liệu thật, 2026-11-23)

`agm cleanup` báo `[skipped] t7p: không xác minh được phiên kế nhiệm còn live` dù phiên kế nhiệm t8c đang live.
DB ghi t8c `created_ms=1795450665328`, còn `hc get t8c` trả `1795450665482` (lệch 154 ms).

## Nguyên nhân (CEO đã kiểm trên HEAD 79d8df4)

1. Daemon mở phiên kế nhiệm ở `src/rotation.rs` (~dòng 740–760). Phản hồi `hc run` không có `created_ms`, nên code ghi
   `row.created_ms = now`, mà `now` là giờ của tick agm, lấy TRƯỚC lúc gọi hc. Nhánh phục hồi `"spawning"` (~dòng 809)
   cũng làm như vậy. `src/actions.rs:688` (`spawn`) cũng vậy.
2. `Store::upsert_terminal` (`src/store/v2.rs` ~164) chỉ cập nhật `created_ms` khi giá trị cũ bằng 0
   (`CASE WHEN created_ms = 0`). Vì thế các lần daemon sync sau đó từ `hc ls`/events (`src/daemon.rs` `apply_terminal`)
   không bao giờ sửa lại giá trị sai.
3. `agm cleanup` (`src/actions.rs` `fresh_cleanup_terminal` ~1031, và điều kiện phiên kế nhiệm ~1118
   `Some(nt.created_ms) == item.new_created_ms`) đòi `created_ms` khớp tuyệt đối với hc. Điều kiện kiểm tra này đúng;
   dữ liệu ghi vào mới sai. Hệ quả: MỌI lần xoay phiên Claude/Codex có quản lý đều bị cleanup bỏ qua.

## Việc cần làm (TDD với `FakeHc` trong `tests/common/mod.rs`; mỗi bước: test đỏ → code → xanh → commit)

1. **Ghi đúng khi spawn.** Tạo một hàm dùng chung (ví dụ trong `src/actions.rs` hoặc `src/hc.rs`). Sau `hc.run`, nếu
   `row.created_ms == 0` thì gọi `hc.get(&row.address)` và lấy `created_ms` thật của herdr (> 0). Chỉ khi `get` lỗi
   hoặc không có giá trị mới dùng `now`. Áp dụng ở cả 3 chỗ: `actions::spawn` (~688), nhánh spawn successor của
   rotation (~755) và nhánh phục hồi `"spawning"` (~809; ở nhánh này row lấy từ `hc ls` thì thường đã có
   `created_ms`, nhưng row lấy từ store thì có thể sai, nên cũng nên ưu tiên `hc.get`).
   Test: FakeHc trả `run` không có `created_ms` và `get` có `created_ms = X ≠ now` → DB phải ghi X.
   Thêm test: `get` lỗi → ghi `now`.
2. **Sửa hàng đã ghi sai (để cặp t7p → t8c đang có dọn được).** Khi daemon áp dữ liệu terminal từ hc
   (`apply_terminal`, đi qua cả snapshot lẫn event), nếu hàng trong DB có `created_ms > 0` khác `created_ms > 0` của hc,
   thì chỉ cập nhật `created_ms` khi THOẢ HẾT các điều kiện sau:
   - cùng address;
   - hc báo `state` là live;
   - cùng `app` (cả hai khác rỗng);
   - cùng cwd theo `crate::correlate::norm_cwd`, hoặc DB không có cwd;
   - hàng do agm mở: `managed == true`, hoặc có `launch_spec`, hoặc có `template` (các field của `TerminalRow` trong
     `src/store/v2.rs`; `agm spawn` thường đặt `template`, còn rotation đặt `managed`/`launch_spec` qua
     `set_terminal_launch`);
   - `0 < hc_created − db_created ≤ 60_000` ms, vì giá trị sai luôn là `now` lấy trước khi herdr tạo terminal.

   Mỗi lần sửa thì ghi `store.log_action("agm", "resync_created_ms", addr, "<cũ> → <mới>", true, now)`.
   Thêm hàm store riêng (ví dụ `set_terminal_created_ms`). KHÔNG đổi hành vi chung của `upsert_terminal`.
   Test (mỗi điều kiện một test âm): khác app, khác cwd, lệch âm, lệch > 60 s, không phải hàng agm mở → không sửa.
   Test dương: thoả hết → sửa và có action log.
3. **Test đầu-cuối cho cleanup:** dựng rotation old → new với `created_ms` của new trong DB lệch 154 ms so với FakeHc.
   Sau một lần áp snapshot daemon, `agm cleanup` (hàm trong actions) phải dọn được old, không còn `skipped`.
4. **KHÔNG** nới điều kiện so khớp trong `fresh_cleanup_terminal` hay ở ~1118.

## Ràng buộc

- Chỉ sửa trong worktree này (`D:/Project/agmag.wt/created-ms`, branch `fix/created-ms-identity`). Không push, không
  merge, không đổi branch.
- Ghi log từng bước vào `.agents/log/created-ms-identity.md` (theo mẫu `.agents/log/tui-project-k.md`) và commit sau MỖI
  bước, để người thay thế có thể làm tiếp từ đĩa nếu model bị giới hạn giữa chừng.
- Cổng cuối: `cargo test`, `cargo fmt --check`, `cargo clippy --all-targets -- -D warnings` đều sạch. Ghi kết quả vào log.
- Comment trong code theo phong cách sẵn có của file (tiếng Việt ngắn gọn).
