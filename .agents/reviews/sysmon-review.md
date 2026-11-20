# Review độc lập — `feat/sysmon` (`agm sys`, theo dõi RAM/CPU/ổ đĩa theo agent)

- Nhánh: `feat/sysmon`
- Task: `.agents/tasks/sysmon-review.md`
- Reviewer: independent reviewer
- Phạm vi: `git diff main HEAD` (13 tệp, +1554 dòng; commit `003d024` trên `main` `0385149`)
- Ngày: 2026-11-20

> Tệp này được tạo từ đầu. Mỗi mục kiểm sẽ được ghi phát hiện kèm `tệp:dòng` + cách tái hiện, rồi **commit ngay** (tiền tố `sysmon-review:`), để người thay thế làm tiếp nếu model bị chặn. Không sửa mã sản phẩm. Mức phát hiện: `chặn` / `nên sửa` / `nhỏ`.

## Tiến độ kiểm

- [x] (1) Phụ thuộc mới `sysinfo 0.40.4`: feature, số crate kéo thêm, trùng phiên bản lớn, build offline; đọc số liệu không cần crate (ghi nhận)
- [x] (2) Đúng số liệu: % CPU baseline, RAM/swap, `merge_disks`, tốc độ đọc/ghi, `attribute` theo cây tiến trình, `agent_of`
- [x] (3) Chi phí/treo: refresh TUI, sleep chặn luồng, lệnh ngoài không hạn chót
- [x] (4) Hiển thị: `agm sys` (màu/NO_COLOR/--json), TUI header, bảng `system` qua `view`, 60/110 cột, 64 nhân, lọc CSI/OSC
- [x] (5) Không đụng CSDL: không migration, không ghi `~/.agmag`
- [x] (6) Ghép với `main`: `quota`, `view` mặc định, `opencode-go` rows; đối chiếu `feat/sysmon-orig`
- [ ] (7) Docs: README.md, README_vi.md, CHANGELOG `[Unreleased]`
- [ ] (8) `cargo test --offline` + `cargo clippy --offline --all-targets -- -D warnings`

## Phát hiện

### (1) Phụ thuộc mới `sysinfo 0.40.4` — ĐẠT (1 phát hiện nhỏ)

- `Cargo.toml:19`: `sysinfo = { version = "0.40.4", default-features = false, features = ["system", "disk"] }` — đúng yêu cầu "chỉ bật `system`, `disk`"; `component`/`gpu`/`network`/`user` bị tắt.
- **Kéo thêm bao nhiêu crate:** `Cargo.lock` có **15 `[[package]]` mới**: `sysinfo 0.40.4`, `ntapi 0.4.3`, `windows 0.62.2`, `windows-core 0.62.2`, `windows-collections 0.3.2`, `windows-future 0.3.2`, `windows-implement 0.60.2`, `windows-interface 0.59.3`, `windows-numerics 0.3.1`, `windows-result 0.4.1`, `windows-strings 0.5.1`, `windows-threading 0.2.1`, `objc2-core-foundation 0.3.2`, `objc2-io-kit 0.3.2`, `syn 2.0.119`. (`windows-link 0.2.1`, `windows-sys 0.61.2`, `libc`, `memchr` **đã có** trong `main`.) Lưu ý: dù đã `default-features = false`, chính hai feature `system` + `disk` vẫn khai `dep:windows` và `dep:objc2-*` (`sysinfo-0.40.4/Cargo.toml` features `system`/`disk`) — các crate này là **target-gated** (`windows-*` chỉ build trên Windows, `objc2-*` chỉ trên macOS); trên Linux phần tăng thực tế là `sysinfo` + `libc`/`memchr` có sẵn.
- **Trùng phiên bản lớn:** có — `syn 2.0.119` (`Cargo.lock:623`) mới cạnh `syn 3.0.10` đã có trong `main` (`Cargo.lock` dòng ~593). Đây là dependency build-time (proc-macro, qua `windows-implement`/`windows-interface`), không ảnh hưởng runtime binary, chỉ tăng thời gian build.
- **Build offline:** `cargo build --offline` → `Finished dev profile ... in 0.18s`, `EXIT=0`, không cần mạng.
- **Đọc số liệu không cần crate (ghi nhận):** có thể — Linux đọc trực tiếp `/proc/stat`, `/proc/meminfo`, `/proc/mounts`, `/proc/diskstats`; Windows dùng Win32 PDH / `GetProcessMemoryInfo` / `GetDiskFreeSpaceEx` qua FFI. Không bắt buộc đổi; `sysinfo` lựa chọn hợp lý để đa nền tảng.

**Phát hiện 1.1 — mức: nhỏ.** `Cargo.lock:623`: `syn 2.0.119` trùng **major** với `syn 3.0.10` đã có. Chỉ là proc-macro build-time, không chặn.
- Cách tái hiện: `git show main:Cargo.lock | Select-String 'name = "syn"' -Context 0,1` → `3.0.10`; `Select-String 'name = "syn"' Cargo.lock` → thêm `2.0.119`.

### (2) Đúng số liệu — ĐẠT có điều kiện (1 nên sửa, 1 nhỏ)

- **% CPU / baseline — đạt.** `src/sysmon.rs:315` lấy baseline CPU thứ nhất, `:317` baseline tiến trình thứ nhất, `:320` `sleep(MINIMUM_CPU_UPDATE_INTERVAL)` (200 ms), `:321` refresh tiến trình lần hai → `Process::cpu_usage()` có mốc so sánh thật (sysinfo ghi rõ "cần refresh **hai lần**", `common/system.rs:2044-2051`). `sample():342` refresh CPU, `:362`/`:368` lấy `global_cpu_usage()`/từng core. `:334-340` trả bản cũ nếu gọi lại trong vòng 200 ms và `cpu_ready=false` ở frame đầu → không có số 0% giả (test `tests/sysmon_test.rs:284-291` khẳng định `cpu n/a`).
- **RAM/swap — đạt.** `:316`/`:344` `MemoryRefreshKind::everything()`, `:353-359` lấy `total/used memory` + `total/used swap`; `pct()` (`:140-142`) trả `None` khi total=0 và kẹp ≤100 → không bao giờ in RAM ảo.
- **Tốc độ đọc/ghi — đạt.** sysinfo `DiskUsage` định nghĩa `read_bytes`/`written_bytes` là *"số byte kể từ lần refresh trước"* (`sysinfo-0.40.4/src/common/mod.rs:50-59`), không phải cumulative. Code refresh disk ở `:348`, đọc `io` ở `:412-414`, chia cho `secs` lấy từ `refreshed_at.elapsed()` đo ở `:334` = đúng khoảng giữa hai lần refresh; `:396` chặn mẫu <50 ms. (Đã đối chiếu mã nguồn sysinfo tại máy để loại trừ khả năng chia tổng tích lũy cho interval.)
- **`attribute` theo cây — đạt, không đếm đôi.** `:213-257`: mỗi tiến trình đi ngược lên **agent gần nhất** rồi cộng đúng **một** lần vào agent đó (`:234-239`); `MAX_DEPTH=64` (`:209`), tự-loops bị loại (`:226`), pid reused (cha trẻ hơn con) bị loại (`:229-232`). Ca "opencode chạy dưới claude-code" nằm trong test `tests/sysmon_test.rs:114-141`: con thuộc `opencode`, cha/ancestor thuộc `claude-code`, tổng không chồng. CPU chia cho `logical_cores` (`:241-248`) đúng quy ước sysinfo "100 = một core" (`common/system.rs:2040-2044`).

**Phát hiện 2.1 — mức: nên sửa.** `src/sysmon.rs:262-279` `merge_disks` gộp hai đĩa **vật lý khác nhau** nếu cùng `file_system` + `total_bytes` + `available_bytes` (`:268-271`). Comment `:260-261` cho rằng "two real disks never match to the byte" — **sai**: hai ổ cùng dung lượng, cùng định dạng, cùng mức đầy (vd hai SSD NTFS 1 TB mới cài, hoặc hai thành viên RAID/clone VM) có total/available **bằng đúng từng byte**. Hệ quả: DISKS section và header đếm thiếu một ổ, và ổ bị nuốt bị ghi nhầm thành `(also D:)` của ổ kia (`:273`). Test `tests/sysmon_test.rs:185-202` chỉ tránh được ca này bằng cách cho ổ thứ tư dùng `exFAT` (`:194`) — tức chính test thừa nhận cùng kích thước + cùng free + **cùng fs** sẽ bị gộp. Đây là lỗi hiển thị/đếm, không mất dữ liệu nên không chặn.
- Cách tái hiện (đã chạy chương trình Rust độc lập copy nguyên hàm `merge_disks` vào `$env:TEMP`, không sửa repo): truyền `DiskStats { mount: "C:\\", fs: "NTFS", total: 1000GB, avail: 250GB }` và `DiskStats { mount: "D:\\", fs: "NTFS", total: 1000GB, avail: 250GB }` → kết quả `so dia sau merge = 1`, `mount=C:\ also_mounted_at=["D:"]`, dù là hai đĩa khác nhau. Hướng sửa gợi ý: gộp theo volume identity (số serial/UUID thiết bị) thay vì bộ ba fs+kích thước+free; nếu không có ID thì tối thiểu không gộp hai mount khác nhau khi có nhiều hơn một ổ cùng thông số.

**Phát hiện 2.2 — mức: nhỏ.** `src/sysmon.rs:185` (`c.contains("/opencode/")`) và `:183` (`"@openai/codex"`) khớp trên toàn bộ cmd line đã lower-case. Một tiến trình `node` chạy script trong thư mục **có tên là `opencode`** nhưng không phải agent (vd `node D:/projects/opencode/server.js`) → cmd chứa `/opencode/` → bị gán nhãn `"opencode"`; tương tự `node -e "console.log('@openai/codex')"`. Test `tests/sysmon_test.rs:101-112` chỉ phủ MCP server và tên gần giống `claude-helper`, chưa có ca path/project trùng tên. Không chặn vì chỉ sai nhãn thống kê.
- Cách tái hiện: `agent_of("node", None, "node D:/projects/opencode/server.js")` → `Some("opencode")` (kỳ vọng `None`); so sánh với ca `node @modelcontextprotocol/server-filesystem` đã trả `None`.

### (3) Chi phí / treo — ĐẠT (2 phát hiện nhỏ)

- **Không có lệnh ngoài nào.** `git diff main HEAD -- '*.rs'` chỉ có hai dòng `std::thread::sleep` mới (`src/sysmon.rs:320`, `:432`); không có `Command::new` / `.spawn()` nào trong `main.rs`/`dashboard.rs`/`tui.rs`/`sysmon.rs` (đã `git grep`). Không có rủi ro lệnh ngoài thiếu hạn chót.
- **Không có `sleep` trong vòng lặp TUI.** `sleep` 200 ms nằm trong `SysSampler::new()` (`src/sysmon.rs:320`), được gọi **một lần** lúc khởi động TUI (`src/tui.rs:220`, trước `loop` ở `:226`). `sleep` 500 ms nằm trong `read_once()` (`:432`), chỉ dùng bởi `agm sys` (`src/main.rs:267`). Trong `loop`, TUI chờ có chủ đích qua `rx.recv_timeout(Duration::from_secs(refresh.max(1)))` (`src/tui.rs:257`) — không phải sleep chặn vô hạn.
- **Có refresh toàn bộ tiến trình mỗi lần vẽ — nhưng được chặn nhịp.** `src/tui.rs:232` gọi `sampler.sample()` trên **mọi** iteration (kể cả khi bỏ qua ghi frame ở `:249`), và `sample()` dùng `ProcessesToUpdate::All` (`src/sysmon.rs:346`) + `attribute()` (`:422`). Tuy `:334-339` trả bản cache nếu chưa tới 200 ms, nên các lần gõ phím liên tiếp không gây refresh lại; nhịp refresh thực chất = `refresh` (mặc định ≥1 s).

**Phát hiện 3.1 — mức: nhỏ.** `src/tui.rs:232` + `src/sysmon.rs:346`: mỗi lần sample quét toàn bộ bảng tiến trình (kèm `with_cpu`/`with_exe`/`with_cmd` ở `:298-304`) và tính lại cây agent, dù header (`src/dashboard.rs:574-595`) chỉ cần số máy-wide CPU/RAM/đĩa; phần per-agent chỉ hiện khi `Section::System` được chọn. Đo thực tế bằng chương trình Rust ngoài repo link trực tiếp rlib `agmag`: `new(): ~689 ms` (đã gồm 200 ms sleep), `sample(): 70.7 / 46.3 / 43.9 ms` trên máy 12 core này. Nghĩa là TUI bị chặn ~45 ms mỗi chu kỳ refresh ~1 s (~4.5% duty cycle) — không treo, nhưng có thể thấy khựng nhẹ; nếu người dùng bật `view` không có `system` thì công việc per-agent bị lãng phí.
- Cách tái hiện: chạy `agm sys` trực tiếp (`target/debug/agm.exe sys`) đo bằng Stopwatch được ~1.24–1.28 s/lần (trong đó 700 ms là hai `sleep`, phần còn lại là quét tiến trình); hoặc chương trình bench gọi `SysSampler::new()`/`sample()` như trên. Hướng sửa gợi ý: chỉ tính `attribute()` khi `Section::System` nằm trong `view`, hoặc giảm tần suất refresh tiến trình so với refresh CPU/RAM.

**Phát hiện 3.2 — mức: nhỏ.** `src/tui.rs:220` gọi `SysSampler::new()` đồng bộ trước vòng lặp → khởi động TUI bị chặn ~689 ms (đo ở trên, riêng phần refresh ~489 ms ngoài sleep). Chỉ xảy ra một lần, không chặn, nhưng là độ trễ có thể cảm nhận được trước frame đầu tiên.
- Cách tái hiện: thêm timestamp trước/sau dòng `SysSampler::new()` trong `src/tui.rs:220` và chạy `agm tui` (cần TTY) — reviewer không chạy TUI theo quy tắc, số liệu lấy từ bench `new()` ở trên.

### (4) Hiển thị — ĐẠT (1 phát hiện nhỏ)

- **`agm sys` màu / NO_COLOR / --json — đạt.** `src/main.rs:265-279`: lệnh `sys` được xử lý trước `run(cli)` (không cần CSDL). `:266` tự bật JSON khi stdout không phải terminal; `:268` dùng `emit(json, &stats, || print_sys(...))`, `emit` (`:684-691`) in `serde_json::to_string_pretty` hoặc gọi hàm human. `print_sys` (`:998-1005`) render qua `render_sys_report(s, tui::pane_width())`, và nếu `NO_COLOR` khác rỗng thì `strip_sgr()` (`:979-995`, bỏ CSI kết thúc bằng `m`) → plain text. Đã chạy `target/debug/agm.exe sys --json`: keys `memory, cpu, disks, agents`, `cores=12, per_core=12`; không có byte `\x1b` trong output JSON, cả khi đặt `NO_COLOR=1`. `agm sys --help` ghi rõ "--json (mặc định khi đầu ra không phải terminal)".
- **Header + bảng `system` qua `view` — đạt.** `src/dashboard.rs:574-575` chèn `system_summary` vào header **mọi view**; `:586-605` in `cpu n/a | ram n/a | <đĩa đầy nhất> n/a` khi chưa đo, mỗi giá trị tô màu theo ngưỡng. `Section::System` nằm trong `SELECTABLE` (`:1046-1053`), không nằm trong `DEFAULT` (`:1054`), `parse_view("system quota")` được test `tests/sysmon_test.rs:393-401`; nội dung section render qua `render_system` (`:939-951`). Test `tests/sysmon_test.rs:234-301` kiểm header `cpu 23% ram 91% C: 71%`, màu đỏ RAM >85% (`\x1b[91m`), đủ các dòng CPU/RAM/swap/đĩa/agent và SUM.
- **60/110 cột — đạt (đã đo thực tế).** Chạy chương trình Rust ngoài repo gọi trực tiếp `render_sys_report` và `render_view` với dữ liệu giả (4 và 64 core, đĩa có `also_mounted_at`, 2 agent): ở cả `width=60` và `110`, dòng dài nhất = 59 / 109 (tức `< width`) cho cả `agm sys` lẫn TUI SYSTEM. `pane_width()` (`src/tui.rs:85-90`) đọc `COLUMNS`, clamp 60..200. Mọi dòng report đi qua `bounded_block`→`bounded_styled_line` (`src/dashboard.rs:931-936,1657+`), giữ SGR và cắt theo **cell** Unicode, không cắt grapheme.
- **64 nhân — đạt.** `core_grid_lines` (`:750-778`) tính `label_w` từ `n-1`, `cell_w`, `cols` rồi `chunks(cols)` → với 64 core ở width 110 ra **13 dòng** lưới `#n ███████▌░ 81%`, mỗi dòng vẫn ≤109 cell (đo ở trên); ở width 60 tự xuống 3 cột/dòng. TUI dùng `core_spark_line` (`:728-747`) một dòng, cũng bị bound bởi `bounded_styled_line`. Không vỡ lưới/spark.
- **Lọc CSI/OSC — đạt.** Nhãn đĩa trong header (`system_summary:595`), trong DISKS (`:856`) và `also_mounted_at` (`:850`) đều qua `sanitize_one_line`; tên agent (`:909`) qua `one_line` → gọi `sanitize_one_line` (`:1631-1632`). Hàm này (`:1583-1628`) xử lý CSI (`ESC [ ... @..~`), OSC (`ESC ] ... BEL / ESC \`) và cả escape kiểu `ESC SP`; thay control char bằng space. Thử mount giả dài 110 ký tự chứa đường dẫn: header vẫn bị giới hạn 109 cell, không tràn.

**Phát hiện 4.1 — mức: nhỏ.** `src/tui.rs:89` `.clamp(60, 200)` khiến `agm sys` (dùng `pane_width()` ở `src/main.rs:999`) **không bao giờ render dưới 60 cột** dù hàm render `render_sys_report` hỗ trợ width nhỏ hơn (test `tests/sysmon_test.rs:350` tự render ở 40). Trên terminal <60 cột (vd `COLUMNS=40`), các dòng 59 cell bị tràn sang dòng mới. Không ảnh hưởng 60/110 theo yêu cầu, nên nhỏ.
- Cách tái hiện: `COLUMNS=40 target/debug/agm.exe sys` (cần TTY để lấy human output) → vẫn render ở width 60; so sánh `render_sys_report(&stats, 40)` trực tiếp cho dòng ≤39.

### (5) Không đụng CSDL — ĐẠT (0 phát hiện)

- **Không migration / không tệp schema mới.** `git diff --name-only main HEAD` (13 tệp, cộng thêm tệp review này) không có tệp migration, `store.rs`, `schema*`, hay tệp CSDL nào. `src/lib.rs` chỉ thêm `pub mod sysmon;` (1 dòng) — không đụng module store.
- **`agm sys` chạy trước khi mở CSDL.** `src/main.rs:264-280`: nhánh `Cmd::Sys` nằm trước `run(cli)` (`:281`) — nơi `agm_home()`/config/`Store::open` bắt đầu (`:290-294`). Comment `:264` ghi rõ "No database needed". `read_once()`/`SysSampler` (`src/sysmon.rs:290-434`) chỉ gọi sysinfo, không có `store`/`conn`/`INSERT`/`execute` nào (đã `git grep`).
- **Số liệu chỉ nằm trong bộ nhớ.** `Snapshot.system: Option<SysStats>` (`src/dashboard.rs:79`) được gán ở `src/tui.rs:232` rồi đọc để render (`:574-575`, `:1537`); `snapshot()` (`:82+`) và `render_view()` (`:1124+`) chỉ `query_row` đọc dữ liệu cũ. `git diff main HEAD -- '*.rs' | Select-String '^\+.*(set_setting|INSERT INTO|UPDATE |execute\(|store\.)'` → **không có dòng nào**. Hai `store.set_setting` trong `src/tui.rs:421,442` là mã cũ (`policy`, `view`) không nằm trong diff.
- **Thử nghiệm:** đặt `AGM_HOME` trỏ tới một **tệp thường** (CSDL không thể mở) rồi chạy `agm.exe sys --json` → vẫn in JSON đầy đủ, `EXIT=0`. Khớp nhật ký `.agents/log/sysmon.md`: "No history in the database (no migration)" và "agm sys (handled before the database opens)".

### (6) Ghép với `main` / đối chiếu bản gốc — ĐẠT (0 phát hiện)

- **`src/main.rs` — chỉ thêm, không đụng lệnh cũ.** Diff có đúng 4 hunk: thêm biến thể `Cmd::Sys` (`:191`), nhánh xử lý `sys` trước `run(cli)` (`:261-280`), nhánh `Cmd::Sys => unreachable!()` trong match (`:340`), và hai hàm mới `strip_sgr`/`print_sys` (`:979-1005`) đặt **trước** `#[cfg(test)] mod tests` (`:1007`) đúng như brief. Không có dòng nào sửa `Cmd::Quota`, `quota_table`, hay `emit`.
- **`src/tui.rs` — 3 dòng đổi.** `pane_width()` đổi `fn` → `pub(crate) fn` (`:85`); thêm `let mut sampler = SysSampler::new()` (`:220`); trong vòng lặp đổi `Ok(s) => render_view(...)` thành `Ok(mut s) => { s.system = ...; render_view(...) }` (`:230-234`). Không đụng `scan`, `snapshot`, `load_view`, xử lý phím hay command bar.
- **`src/dashboard.rs` — mở rộng, không xóa logic.** Các dòng bị xóa trong toàn file chỉ là: `let items = [...]` → `let mut items = vec![...]` trong `render_header` (`:569-577`), `ALL: [Section; 6]` → `[Section; 7]` và `SELECTABLE: [Section; 5]` → `[Section; 6]` để thêm `System` (`:1036-1053`). `Section::DEFAULT` giữ nguyên `[Quota, Terminals, Inbox]` (`:1054`), khớp test `tests/sysmon_test.rs:400`. Không có hunk nào gần code `quota`/`opencode-go`.
- **Test xác nhận luồng cũ:** `cargo test --offline --test dashboard_test --bin agm` → `render_opencode_go_quota_labels_limited_and_stale_fit_60_110 ... ok`, `render_stale_quota_line_fits_60_and_110 ... ok`, `view_command_picks_up_to_three_sections ... ok`, `plan_quota_shows_percent_left ... ok`; bin `agm` 5/5, dashboard 25/25. Cộng `sysmon_test` 9/9 → tổng 39 test nhắm mục này đạt.
- **Đối chiếu `feat/sysmon-orig` (`9de7b5c`, gốc từ `a7ce50f`)**: `git diff feat/sysmon-orig HEAD -- src/sysmon.rs` chỉ là **thay đổi định dạng `cargo fmt`** (gộp/tách dòng, sắp xếp import `MINIMUM_CPU_UPDATE_INTERVAL`) — không khác biệt ngữ nghĩa. Tên 12 hàm test trong `tests/sysmon_test.rs` **giống hệt** hai nhánh; tập hàm sys-specific trong `dashboard.rs` (`render_sys_report`, `render_system`, `machine_lines`, `core_grid_lines`, `core_spark_line`, `disk_lines`, `agent_lines`, `system_summary`, `system_verdict`) **giống hệt**. Không thấy phần nào bị rơi khi apply lại lên `main` `0385149`.

_(các mục còn lại sẽ điền dần)_

## Kết luận

_(chưa xác định)_
