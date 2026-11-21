# Review — manage-foreground-cwd

- Đối tượng: HEAD `bffff7ed9e3e005ee388940384d34da8cb9856a6`, phạm vi `git diff main...HEAD`.
- Reviewer: độc lập. Ngày 2026-11-21. Không sửa mã sản phẩm; chỉ ghi review và commit review.
- Thang đánh giá: **CHẶN** (phải sửa mới nghiệm thu) / **NÊN SỬA** (nợ kỹ thuật đáng xử lý) / **NHỎ** (nit, không ảnh hưởng nghiệm thu).

---

## Mục 1 — `src/hc.rs::effective_cwd`: đúng 3 ca của brief, xử lý rỗng/khoảng trắng

**Mức: không có vấn đề (ĐẠT).**

- `src/hc.rs:261-266`: hàm thuần `pub fn effective_cwd(&Value) -> Option<String>`.
  - `foreground.cwd` qua `as_str()` → `non_blank` (`!s.trim().is_empty()`) → `or_else` → `cwd` gốc. Đúng 3 ca brief: có foreground → foreground (`src/hc.rs:265`); foreground rỗng/thiếu → cwd; cả hai thiếu → `None`.
  - Chuỗi chỉ toàn khoảng trắng bị loại ở **cả hai** nguồn nhờ `trim()` dùng chung trong closure `non_blank` (`src/hc.rs:262-264`), không chỉ riêng foreground.
  - `foreground` không phải đối tượng (vd chuỗi `"pwsh"`) → `v["foreground"]["cwd"].as_str() == None` → rơi về cwd; không panic (Index của `serde_json::Value` trả `Null` an toàn).
- Test phủ đủ: `tests/daemon_test.rs:175` `effective_cwd_prefers_foreground_then_root` — foreground thắng (`181-185`), foreground `"  "` → cwd (`187-190`), thiếu foreground (`192-195`), cả hai thiếu → `None` (`197`), cả hai rỗng → `None` (`199-202`), foreground sai kiểu (`204-207`).
- `terminal_row_from` (`src/hc.rs:268+`) không bị động đến trong diff — đúng quyết định tách hàm riêng.

Kết luận mục 1: đúng brief, không có ghi chú.

---

## Mục 2 — `src/ops.rs::manage`: cwd từ foreground, câu hỏi `[y/N]`, không nới quy tắc xác nhận

**Mức: không có vấn đề (ĐẠT).**

- `src/ops.rs:318`: vẫn gọi `terminal_row_from(v)` để lấy `app` (`fresh.app`); thay `fresh.cwd` bằng `effective_cwd(v)` tại `src/ops.rs:324-326` → `t.cwd` trong bộ nhớ là thư mục foreground (là `cwd` gốc nếu foreground rỗng). Đây đúng nguồn "thư mục hiệu lực" của brief.
- `src/ops.rs:339`: vẫn có bước chặn `t.cwd.as_deref().filter(|c| !c.trim().is_empty())` → nếu `effective_cwd` trả `None` (cả hai thiếu/rỗng) thì báo lỗi "không biết cwd", không dựng spec với cwd rỗng. `src/ops.rs:366`: `parse_launch_spec(app, &argv, cwd)` — cwd đã được kiểm tra chắc chắn không rỗng (xem thêm `src/launch.rs:128-130`).
- Câu hỏi xác nhận in đúng cwd: `src/ops.rs:367-370` format `cwd={}` từ `spec.cwd` (chính là cwd hiệu lực), gọi qua `caller(store, false, Some(&question))` (`src/ops.rs:372`); `caller` in `"{question} [y/N] "` tại `src/ops.rs:46`. Người dùng thấy đúng thư mục trước khi gõ `y`.
- **Không nới quy tắc xác nhận**: diff `main...HEAD` trên `src/ops.rs` chỉ có 2 hunk — đổi import (`src/ops.rs:15`) và đổi nguồn cwd (`src/ops.rs:322-326`). `caller` (`src/ops.rs:33-56`) và `actions::set_managed_launch` (`src/actions.rs:562-...`) **không đổi**: vẫn `caller(store, false, ...)` (không nhận `--yes`, `src/ops.rs:371`), vẫn chỉ `Caller::User { confirmed: true }` mới lưu spec (`src/actions.rs:574-580`); agent bị từ chối (`src/actions.rs:585+`).
- Không đổi cách daemon lưu `terminals.cwd`: `t.cwd = Some(cwd)` ở `src/ops.rs:325` chỉ sửa bản sao `TerminalRow` trong bộ nhớ; `set_terminal_launch` (`src/store/v2.rs:280-291`) chỉ `UPDATE terminals SET managed = ?2, launch_spec = ...`, **không** đụng cột `cwd`. Nên thư mục gốc lưu trong CSDL vẫn nguyên như cũ — đúng quyết định của CEO.
- Lưu ý phạm vi (đã ghi trong log, không phải lỗi): không có test CLI giả `hc get` cho `manage` trên Windows; `manage` được phủ gián tiếp qua `effective_cwd` + các test rotation. Chấp nhận theo log `.agents/log/manage-foreground-cwd.md:22-24`.

Kết luận mục 2: đúng brief, quy tắc xác nhận nguyên vẹn.

---

## Mục 3 — `src/rotation.rs::start`: nguồn `rotations.cwd`, nhất quán requested/handoff_ready, ca spec hỏng / cwd rỗng

**Mức: 1 × NHỎ (spec.cwd rỗng), không có CHẶN / NÊN SỬA.**

### 3a. Hai ca chính — ĐẠT

- `src/rotation.rs:220-226` tính `rotation_cwd`, truyền vào `INSERT ... cwd` tại `src/rotation.rs:227-231` (thay `t.cwd` cũ).
  - Có `launch_spec` parse được → `.map(|spec| spec.cwd)` (`src/rotation.rs:223-224`) → `rotations.cwd = spec.cwd` — thư mục người dùng đã xác nhận ở `manage`. Đúng brief.
  - Không có `launch_spec` (OpenCode do hc sinh) → `.or_else(|| t.cwd.clone())` (`src/rotation.rs:225`) → `terminals.cwd` như cũ. Đúng brief.
  - Test: `tests/rotation_test.rs:966` (spec.cwd `D:\Project\other` thắng `terminals.cwd = ...\shell-root`), `tests/rotation_test.rs:988` (không spec → giữ `D:\w`) — pass.
- Không đọc `foreground` trong `start` (`src/rotation.rs:219` ghi chú rõ lý do: cần thư mục ổn định cho các bước sau). Đúng quyết định.

### 3b. Nhất quán các bước — ĐẠT

- Bước `pending → requested`: `src/rotation.rs:525-532` lấy `r.cwd.clone().or_else(|| t.cwd.clone()).filter(...)`. Vì `start` đã ghi `r.cwd = spec.cwd`, bàn giao được tạo dưới `<spec.cwd>/.agents/handoff/`; nếu `r.cwd` NULL thì mới rơi về `t.cwd` (hành vi cũ). Test `tests/rotation_test.rs:1008 managed_handoff_targets_launch_spec_cwd` kiểm cả `rotations.cwd`, `rel` nằm dưới `<project>/.agents/handoff/`, **không** tạo `.agents/` trong shell-root, và lệnh gửi chứa `rel` — pass.
- Bước `handoff_ready`: `src/rotation.rs:678-680` `if let Some(cwd) = &r.cwd { spec.cwd = cwd.clone(); }`. Với `r.cwd = spec.cwd` từ `start`, phép gán này là no-op → `spec.cwd` giữ nguyên thư mục đã xác nhận; phiên kế nhiệm mở đúng dự án. Nhất quán với `requested`. Đúng như brief dự báo ("tự nhất quán").
- `src/rotation.rs:669-677` vẫn fail-closed khi thiếu/hỏng `launch_spec` ở bước mở phiên mới — không bị nới.

### 3c. Ca `launch_spec` hỏng — ĐẠT

- JSON hỏng/version sai/thiếu executable → `LaunchSpec::from_json(...).ok()` (`src/rotation.rs:223`) trả `None` → `.or_else(|| t.cwd.clone())` → rơi về `terminals.cwd`. Đúng ý log `.agents/log/manage-foreground-cwd.md:34` ("spec hỏng → rơi về terminals.cwd"); không crash, không ghi cwd rỗng.

### 3d. Ca `spec.cwd` rỗng — **NHỎ**

- Vị trí: `src/rotation.rs:226` `.filter(|c| !c.trim().is_empty())` đặt **sau** `.or_else(...)` (`src/rotation.rs:225`). Chuỗi suy luận:
  - spec parse được nhưng `cwd == ""` → `.map` cho `Some("")` → `.or_else` **không** chạy (đã có `Some`) → `.filter` loại `""` → `rotation_cwd = None` → `rotations.cwd = NULL`.
  - Kỳ vọng theo mẫu "dữ liệu spec không dùng được → rơi về `t.cwd`" là `Some(t.cwd)`, nhưng ở đây không rơi.
- **Đánh giá mức độ: NHỎ, không chặn**, vì:
  1. Không thể phát sinh qua luồng sản phẩm: `agm manage` dựng spec bằng `parse_launch_spec`, hàm chặn cwd rỗng tại `src/launch.rs:128-130`; bản thân `manage` cũng đã chặn ở `src/ops.rs:339`. `set_managed_launch` (`src/actions.rs:562`) chỉ nhận spec từ `manage`.
  2. `LaunchSpec::from_json` (`src/launch.rs:23-32`) không validate `cwd`, nên spec cwd-rỗng lý thuyết chỉ có thể xuất hiện nếu ghi trực tiếp vào CSDL / fixture test — không phải ca thực tế.
  3. Hậu quả nếu xảy ra vẫn fail-soft, không crash: `requested` (`src/rotation.rs:525-529`) rơi về `t.cwd`, tức trở lại hành vi cũ (ghi bàn giao vào thư mục shell) — đúng là lỗi đang được sửa, nhưng chỉ trong ca dữ liệu hỏng ngoài luồng.
- **Gợi ý (không bắt buộc nghiệm thu)**: chuyển filter lên trước `or_else` để "spec.cwd rỗng" cũng rơi về `t.cwd`, vd:
  `.and_then(|json| LaunchSpec::from_json(json).ok()).map(|s| s.cwd).filter(|c| !c.trim().is_empty()).or_else(|| t.cwd.clone().filter(|c| !c.trim().is_empty()))`
  (bỏ `.filter` cuối, hoặc giữ cả hai — miễn filter nằm trước `or_else`). Nên kèm 1 test "spec.cwd rỗng → `rotations.cwd == t.cwd`" nếu sửa.

Kết luận mục 3: hai ca chính và hai bước requested/handoff_ready đúng; ca spec hỏng đúng; chỉ có 1 nit ở ca spec.cwd rỗng (nhỏ, không chặn).

---

## Mục 4 — daemon / `terminal_row_from` không đổi; tests đúng hành vi; test đỏ trước sửa

**Mức: không có vấn đề (ĐẠT).**

- `git diff main...HEAD --stat`: các tệp sản phẩm chỉ `src/hc.rs` (+11, thêm hàm), `src/ops.rs` (+6/−1), `src/rotation.rs` (+12/−1), `CHANGELOG.md` (+4); **không có `src/daemon.rs`, `src/store/**`, schema**. `terminal_row_from` (`src/hc.rs:268+`) chỉ được thêm hàm mới ở ngay trước đó, thân hàm không đổi — đúng điều cấm của brief (`...tasks/manage-foreground-cwd.md:36`).
- Cách daemon lưu `terminals.cwd` vẫn như cũ: `set_terminal_launch` (`src/store/v2.rs:280-291`) không update cột `cwd`; bản sao `t.cwd` sửa trong `manage` chỉ dùng để dựng `LaunchSpec` (xem mục 2). Không có giá trị "nhảy qua lại" giữa sự kiện có/không có `foreground`. Các chỗ khác vẫn dùng `terminals.cwd` (`correlate`, `dashboard`, `advance_opencode`, fallback `hc get` lỗi) đã được ghi trong log `.agents/log/manage-foreground-cwd.md:45-58` đúng yêu cầu "chỉ ghi log, không sửa".
- Test mới đúng hành vi, không phải test đúng implementation:
  - `tests/daemon_test.rs:175` (7 assert) — bảng ca `effective_cwd` (xem mục 1).
  - `tests/rotation_test.rs:966` — spec.cwd thắng terminals.cwd.
  - `tests/rotation_test.rs:988` — không spec → terminals.cwd.
  - `tests/rotation_test.rs:1008` — bàn giao trúng thư mục spec, tick tới `requested`, kiểm `handoff_path` tương đối + vắng `.agents` ở shell-root + lệnh gửi chứa `rel`.
  - Cả 4 test chạy pass ở HEAD (`daemon_test` 13, `rotation_test` 30 — xem mục 6).
- **Test đỏ trước sửa**: commit `c57e007 manage-foreground-cwd: red tests for effective cwd and rotation cwd source` chứa đủ 4 test (tên thứ ba lúc đó là `managed_handoff_file_lands_in_launch_spec_cwd`, đổi thành `managed_handoff_targets_launch_spec_cwd` ở `fd97620` với lý do do log `.agents/log/manage-foreground-cwd.md:37-40` giải thích — agent tự ghi tệp, agm chỉ gửi lệnh). Log ghi trạng thái đỏ đúng `error[E0432]: unresolved import agmag::hc::effective_cwd` (`.agents/log/manage-foreground-cwd.md:19-20`). Thứ tự commit `c57e007` → `fd97620` → ... → `bffff7e` khớp TDD đỏ→xanh.

Kết luận mục 4: không đụng daemon/terminal_row_from; test phủ đúng và có bằng chứng đỏ trước sửa.

---

## Mục 5 — `CHANGELOG.md`: LF, mục mới nằm ở `[Unreleased]` (không ở `[0.3.2]`)

**Mức: không có vấn đề (ĐẠT).**

- Đọc byte thô `CHANGELOG.md`: `CR = 0`, `LF = 84` → file thuần LF, không bị CRLF hóa (đúng yêu cầu "giữ LF").
- Diff `main...HEAD -- CHANGELOG.md`: +4/−0, chèn ngay sau `## [Unreleased]` (`CHANGELOG.md:3`):
  - `### Sửa lỗi` (`CHANGELOG.md:5`)
  - một dòng mô tả (`CHANGELOG.md:7`) nói rõ cả 2 thay đổi: `manage` lấy thư mục hiệu lực; `rotate` ghi `rotations.cwd = launch_spec.cwd`; nhắc OpenCode không spec giữ hành vi cũ.
- Nhánh có merge `60b60ba` (đưa release `0.3.2` từ `main` vào) và commit sửa `bffff7e manage-foreground-cwd: changelog entry back under [Unreleased] after 0.3.2 release`. Ở HEAD, mục mới nằm **trước** `## [0.3.2] - 2026-11-21` (`CHANGELOG.md:10`); phần `### Thay đổi` / `### Sửa lỗi` của `[0.3.2]` không bị thêm/sửa bởi diff này (chỉ bị đẩy xuống). Đúng yêu cầu nghiệm thu.

Kết luận mục 5: đúng vị trí và đúng định dạng LF.
