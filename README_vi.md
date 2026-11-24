# agmag

[English](README.md) · **Tiếng Việt**

**Bảng điều khiển model cho đội agent chạy trong [herdr](https://github.com/yigitkonur/awesome-herdr) / herdr-core.**

`agm` đứng cạnh herdr và cho bạn biết, trong một chỗ:

- mỗi agent (Claude Code, Codex, OpenCode, agy/Gemini) đã tiêu **bao nhiêu token**, ngữ cảnh đầy **bao nhiêu %**;
- gói tháng (ChatGPT, Claude) còn **bao nhiêu % hạn mức** và **khi nào reset**;
- token đã dùng trong **24 giờ / hôm nay / tháng này** và biểu đồ **4 tháng** gần nhất (theo giờ máy);
- agent nào **đang dừng lại hỏi bạn** (hộp thư);
- các agent **đã nói gì với nhau**.

Và cho bạn can thiệp có kiểm soát: giao việc có giới hạn số thợ và ngân sách, trả lời agent, dừng agent (Codex/Claude chỉ khi **bạn** ra lệnh), tự **xoay vòng** thợ OpenCode khi ngữ cảnh đầy, và **bàn giao sang phiên mới** cho Claude Code/Codex mà không bao giờ tự kill phiên cũ.

> Nền tảng chính: Windows 11 (cũng chạy trên Linux/macOS). Viết bằng Rust, một tệp `agm.exe`, không cần runtime. Phiên bản hiện tại: **0.4.1** — xem [CHANGELOG.md](CHANGELOG.md).

---

## Mục lục

1. [Cài đặt](#1-cài-đặt)
2. [Dùng ngay trong 1 phút](#2-dùng-ngay-trong-1-phút)
3. [agm hoạt động thế nào — có phải cài vào từng session không?](#3-agm-hoạt-động-thế-nào)
4. [Chạy nền và bảng điều khiển](#4-chạy-nền-và-bảng-điều-khiển)
5. [Khi nào dùng `hc`, khi nào dùng `agm`](#5-khi-nào-dùng-hc-khi-nào-dùng-agm)
6. [Cho agent dùng agm](#6-cho-agent-dùng-agm)
7. [Tất cả lệnh](#7-tất-cả-lệnh)
8. [Thống kê token theo thời gian](#8-thống-kê-token-theo-thời-gian)
9. [Ngân sách, policy và xoay vòng](#9-ngân-sách-policy-và-xoay-vòng)
10. [Cấu hình](#10-cấu-hình)
11. [An toàn và dữ liệu](#11-an-toàn-và-dữ-liệu)
12. [Giới hạn đã biết](#12-giới-hạn-đã-biết)
13. [Phát triển](#13-phát-triển)

---

## 1. Cài đặt

Cần [Rust](https://rustup.rs) (có `cargo`).

```powershell
git clone <địa chỉ repo này> agmag
cd agmag
cargo install --path .          # chép agm.exe vào ~/.cargo/bin (đã có trong PATH)
agm --version                   # agm 0.4.1
```

> `cargo build --release` **chỉ** tạo `target\release\agm.exe` — gõ `agm` sẽ báo "not recognized". Hãy dùng `cargo install --path .`.
> Mỗi lần cập nhật mã: chạy lại `cargo install --path .`. Mở lại cửa sổ PowerShell nếu vẫn chưa thấy lệnh.

Dữ liệu của agm nằm ở `~/.agmag/` (`agm.db`, `providers.toml`). Đổi bằng biến môi trường `AGM_HOME` (các biến khác: xem [mục 10](#10-cấu-hình)).

> Nâng cấp từ 0.1.x: CSDL tự migrate sang schema v3 ở lần chạy đầu (có rollback nếu lỗi), không cần làm gì.

## 2. Dùng ngay trong 1 phút

Không cần cấu hình gì:

```powershell
agm quota          # hạn mức gói: 5 giờ / 7 ngày, còn bao lâu tới reset
agm ls             # phiên hôm nay: model, token, % ngữ cảnh
agm cost           # tổng token theo model  (--by day | month | source | cwd, --days 7)
agm cost --summary # 24h / hôm nay / tháng này + biểu đồ 4 tháng
agm sys            # RAM / CPU / ổ đĩa đang dùng + phần do từng agent chiếm
```

Ví dụ `agm quota` (cột: plan, window, phần còn lại, trạng thái, tới reset). Dòng stale hiện `stale · seen <tuổi> ago` thay vì thanh/% như số tươi — cùng nhãn với TUI:

```
plan    window  left         status                   reset
codex   5h      █████████░   86%                     in 4h38m
codex   7d      █░░░░░░░░░   7%                      in 115h41m
codex   5h      ··········   stale · seen 1d05h ago  reset passed
```

Lần đầu hơi chậm (đọc log 7 ngày gần nhất); sau đó chỉ đọc phần mới. Muốn có số liệu cũ hơn cho thống kê tháng: `agm scan --since 2026-08-01` (hoặc để `agm tui` tự backfill).

## 3. agm hoạt động thế nào

**Không phải cài vào từng session.** agm cài một lần cho cả máy và *đọc dấu vết mà mỗi công cụ vốn đã ghi*, nên mọi session — cũ, mới, mở bằng herdr hay mở tay — đều được đếm:

```
 Claude Code ──ghi──▶ ~/.claude/projects/*.jsonl ──────────────────────┐
 Codex       ──ghi──▶ ~/.codex/sessions/*.jsonl  ──────────────────────┤
 OpenCode    ──ghi──▶ ~/.local/share/opencode/opencode.db ─────────────┼──▶ agm ──▶ ~/.agmag/agm.db
 agy (CLI)   ──ghi──▶ ~/.gemini/antigravity-cli/brain/*/…/transcript.jsonl ┘     ▲
 herdr-core  ──sự kiện (hc events)──▶ agm daemon ────────────────────────────────┘
                                          │
                        bạn xem ở: agm quota / ls / cost / inbox / tui
```

| Công cụ | Phải làm gì? | Thấy số liệu ở đâu |
|---|---|---|
| **Claude Code** | Không bắt buộc. Muốn có **% hạn mức gói 5h/7d** (và để dùng `agm manage`) thì chạy **một lần** `agm hook install` (áp dụng cho mọi session Claude) | Thanh trạng thái dưới cùng của Claude Code: `Opus 5.5 · ctx 34% · 5h 76% left · 7d 59% left`; và mọi lệnh `agm` |
| **Codex** | Không gì cả (hạn mức đọc từ log Codex; daemon cũng có thể làm tươi từ `codex app-server` mỗi 15 phút, hoặc đọc ngay bằng `agm quota --refresh`) | `agm quota / ls / tui` |
| **OpenCode** | Không gì cả (agm đọc `opencode.db`, chỉ đọc). Hạn mức gói Go (5h/7d/30 ngày) do daemon đọc 15 phút/lần hoặc bằng `agm quota --refresh` qua `opencode api GET /api/experimental/usage/limits`; [Go Console](https://opencode.ai/console/go) vẫn là nơi xem Extra Usage/budget | `agm quota / ls / cost / tui` |
| **agy (Gemini)** | Không gì cả. agm đọc `transcript.jsonl` của mọi phiên tương tác `antigravity-cli` và `conversation_summaries.db` (chỉ đọc, để biết thư mục làm việc); `agm ask` được ghi riêng và không bị đếm trùng. **Hạn mức gói** (Gemini / Claude-GPT qua agy / Argon) do daemon đọc 15 phút một lần qua `agy --print /usage` (không tốn token), hoặc đọc ngay bằng `agm quota --refresh` | `agm ls / cost / quota / tui` |

> `agm hook install` thêm `statusLine` vào `~/.claude/settings.json`. Nếu bạn đã có statusLine riêng, lệnh từ chối và không ghi đè — gọi `agm hook statusline` từ script của bạn.

## 4. Chạy nền và bảng điều khiển

Các lệnh ở mục 2 đủ để **xem**. Muốn agm **theo dõi liên tục** (terminal nào đang chạy, ai đang hỏi bạn, ngân sách, xoay vòng) thì chạy daemon — nên chạy trong herdr-core để nó sống độc lập với cửa sổ:

```powershell
hc run --bg --no-parent --name agm-daemon agm daemon
hc read agm-daemon            # xem nó đang làm gì
hc kill agm-daemon            # dừng
```

Rồi mở bảng điều khiển trong một pane của herdr:

```powershell
agm tui
```

```
 ◆ agmag  ● POLICY ON │ 7 terminals (1 blocked) │ inbox 1 │ cpu 23% ram 57% C: 71% │ UTC+07:00
── ◈ PLAN QUOTA (left) ───────────────────────────────────────────
  codex      5h  █████████░  86% left  resets in 4h38m
  codex      7d  █░░░░░░░░░   7% left  resets in 115h41m
── ▪ USAGE (UTC+07:00) ───────────────────────────────────────────
  24h 125k │ Today 480k │ This month 4.2M
  Aug ▂ 1.1M │ Sep ▄ 2.3M │ Oct ▆ 3.4M │ Nov █ 4.2M
── ▸ TERMINALS ───────────────────────────────────────────────────
┌───────────────┬─────────────┬─────────────────────────────┬────────┬─────────┬─────────┬────────┬─────┬────────────┬────────────────────┐
│ name          │ agent       │ model                       │ effort │ where   │ task    │ tokens │ ctx │ last msg   │ status             │
╞═══════════════╪═════════════╪═════════════════════════════╪════════╪═════════╪═════════╪════════╪═════╪════════════╪════════════════════╡
│ ● t1q         │ codex       │ gpt-6.1-sol                 │ high   │ app / 1 │ -       │  21.2M │ 58% │ ← ASSIGN   │ working            │
│ ● t3          │ claude-code │ claude-opus-5-5             │ max    │ app / 2 │ -       │   1.4M │ 71% │ → REPORT   │ idle [HANDED OFF]  │
│ ● w-login-7-1 │ opencode    │ opencode-go/deepseek-v4-pro │ high   │ app / 3 │ login-7 │   340k │ 50% │ → QUESTION │ BLOCKED (question) │
└───────────────┴─────────────┴─────────────────────────────┴────────┴─────────┴─────────┴────────┴─────┴────────────┴────────────────────┘
── ◆ BUDGETS ─────────────────────────────────────────────────────
  task:login-7             █░░░░░░░░░  11%  340k / 3.0M tokens soft
── ● INBOX ───────────────────────────────────────────────────────
  #1 [blocked] t5 — Chạy test?
── » RECENT MESSAGES ─────────────────────────────────────────────
  (no messages)
──────────────────────────────────────────────────────────────────
cmd: reply <#> <text> · done <#|a-b|kind:K|term:T> · send <terminal> <TYPE> <message> · cleanup [1,2|all]
     interrupt <terminal> · kill <terminal> · cd [project|..] · show <#> · policy on|off · view <up to 3 sections> · quit
     TYPE: ASSIGN REPORT ERROR QUESTION BLOCKED PASS FAIL
```

Lệnh trong TUI (gõ rồi Enter):

| Lệnh | Việc |
|---|---|
| `reply 1 có` | Trả lời mục 1 của hộp thư (gõ vào terminal liên quan) |
| `done 1` | Đóng mục 1 (không gõ gì vào terminal nào) |
| `send t5 REPORT All tests passed` | Gửi tin có ghi lại |
| `cleanup` · `cleanup 1,2` · `cleanup all` | Liệt kê / chọn phiên đã bàn giao để dọn (phải xác nhận `y`) |
| `interrupt t5` · `kill t5` | Ngắt / dừng (cả hai đều phải gõ thêm `y` xác nhận) |
| `policy off` | Tắt mọi hành động tự động |
| `view` · `view terminals inbox` · `view default` | Xem bố cục hiện tại / chọn tối đa 3 bảng (`quota`, `usage`, `terminals`, `budgets`, `inbox`, `system`); được nhớ cho lần mở sau. RECENT MESSAGES luôn hiện và không tính vào 3 bảng |
| `cd` · `cd agmag` · `cd ..` | Liệt kê dự án / vào một dự án / quay lại chế độ xem toàn cục |
| `show 1` | In đầy đủ mục hộp thư 1 |
| `quit` | Thoát |

Mỗi lệnh vẫn nhận chữ cái đầu làm phím tắt (`r`, `d`, `s`, `c`, `i`, `k`, `p`, `q`). Loại tin cho `send`: `ASSIGN`, `REPORT`, `ERROR`, `QUESTION`, `BLOCKED`, `PASS`, `FAIL` (không phân biệt hoa/thường). Mã tiếng Việt cũ (`GIAO`, `BÁO CÁO`, `LỖI`, `HỎI`, `CHẶN`, `ĐẠT`, `CHƯA ĐẠT`) vẫn dùng được trong TUI; tin vẫn được lưu và chuyển đi với mã tiếng Việt, `agm send --type` không đổi.

Cột TERMINALS: chấm `●` tô màu theo trạng thái (xanh lá working, xanh ngọc idle/done, đỏ blocked, vàng đã bàn giao, xám đã thoát), sau đó là tên terminal, rút gọn còn 12 ký tự (không có tên thì hiện địa chỉ; lệnh nhận cả hai). `agent` là CLI, `model`/`effort` lấy từ log phiên đã ghép (Codex `effort`, Claude Code `effort`, variant của OpenCode, hậu tố tên model agy như `-high`), `where` là vị trí trong herdr giống `hc ls` (`MATHY-AI / 1`, `agmag / agm-ceo`); không có thì hiện tên thư mục làm việc. `last msg` là tin gần nhất liên quan tới terminal, ghi bằng tiếng Anh: `→ REPORT` là terminal đã gửi báo cáo, `← ASSIGN` là terminal vừa nhận việc. Màu giống RECENT MESSAGES (xanh ngọc ASSIGN, xanh lá REPORT/PASS, vàng QUESTION, đỏ ERROR/FAIL, đỏ đậm BLOCKED).

Bảng điều khiển in đầy đủ mọi dòng (không cắt theo chiều cao pane); lướt chuột lên để xem phần trên. Mỗi lần vẽ lại sẽ xoá màn hình và lịch sử cuộn trước, nên không có khung cũ bị in lặp; và chỉ vẽ lại khi dữ liệu thay đổi hoặc sau khi bạn gõ lệnh, để lúc đang lướt lên đọc không bị kéo xuống mỗi lần làm mới. Chỉ hiện tối đa 3 bảng tự chọn (mặc định `quota terminals inbox`) cùng RECENT MESSAGES luôn ghim ngay trên thanh lệnh. Bề rộng pane được tự nhận; `COLUMNS` dùng để ghi đè (giới hạn 60–200). Tên, đường dẫn và tin nhắn dài được rút gọn theo bề rộng đó. Unicode được đo theo số cột terminal, không cắt giữa một cụm ký tự; chuỗi điều khiển terminal trong dữ liệu được loại bỏ. Việc rút gọn chỉ ảnh hưởng hiển thị, không đổi tin nhắn đã lưu hay đích thao tác.

### Xem theo dự án (`cd`)

Gõ `cd` để liệt kê dự án: mỗi dòng hiện tên dự án, số agent đang sống, số mục
hộp thư đang mở, và một dòng `(no project)` đếm các mục hộp thư có terminal bị
thiếu hoặc không khớp. `cd <tên>` vào một dự án (không phân biệt hoa/thường,
khớp tiền tố duy nhất; worktree được gộp về repo chính); `cd ..` hoặc `cd /`
quay lại chế độ xem toàn cục. Dấu nhắc hiện phạm vi, ví dụ `agm[agmag]>`.

Chế độ xem dự án chỉ hiện cho dự án đó:

- **Agents** — cây với ký hiệu + chữ trạng thái, `doing:` hoạt động mới nhất từ
  log phiên kèm tuổi, thanh `ctx`, sparkline token 60 phút, và giai đoạn nhánh
  từ `BRANCHES.md`.
- **Inbox** — mỗi mục gói tối đa 4 dòng.
- **Messages** — 20 tin mới nhất, mỗi tin gói tối đa 3 dòng, kèm tuổi.

`show <#>` in đầy đủ một mục hộp thư. `reply`, `done`, và `send` vẫn dùng `#ID`
toàn cục, nên dùng được cả trong lẫn ngoài dự án. Giá trị chưa biết hiện `-`.

TUI hiển thị bằng tiếng Anh có màu sắc ANSI, cảnh báo **đỏ** khi phiên đã bàn giao và phiên kế nhiệm cùng chạy trên một thư mục, và tự backfill các tháng còn thiếu cho thống kê. TUI chỉ mở được bởi **bạn** ở terminal (agent không dùng được).

## 5. Khi nào dùng `hc`, khi nào dùng `agm`

`hc` là **tay chân** (chạy, nhìn, gõ vào terminal). `agm` là **sổ sách và luật** (ghi lại, kiểm hạn mức, chặn hành động nguy hiểm) — agm gọi `hc` bên dưới.

| Việc | Dùng |
|---|---|
| Mở agent để **bạn tự làm việc cùng** | `hc run` / mở trong herdr (rồi `agm manage` nếu muốn agm xoay vòng nó) |
| Vào xem / gõ trực tiếp | `hc attach t5`, `hc read t5` |
| Chạy lệnh, test nền (không phải agent) | `hc run --bg …`, `hc exec …` |
| **Giao việc cho thợ** (sinh agent theo đầu việc) | `agm spawn worker-mid --task login-7 -- "…"` |
| **Gửi tin cho agent** | `agm send w-login-7-1 --type GIAO --task login-7 "…"` |
| **Trả lời agent đang hỏi / xin quyền** | `agm reply 1 "có"` (hoặc `r` trong TUI) |
| **Dừng agent** | `agm kill t5` (có bảo vệ; `hc kill` thì không) |
| Xem token, hạn mức, ai đang chờ bạn | `agm quota / ls / cost / inbox / tui` |

Quy tắc ngắn:
1. Việc của **đội agent** (giao, nhắn, trả lời, dừng, sinh thêm) → `agm`.
2. **Bạn tự thao tác tay** → `hc`.
3. Agent nhắn nhau → luôn `agm send` (để có hồ sơ và để câu hỏi tới được hộp thư của bạn).

Agent sinh bằng `hc run` vẫn được **đo token**, nhưng không có nhãn đầu việc → agm không áp ngân sách đầu việc. Muốn agm quản lý đầy đủ thì sinh bằng `agm spawn`; với phiên Claude Code/Codex mở tay, bật xoay vòng bằng `agm manage <t>`.

## 6. Cho agent dùng agm

Thêm đoạn này vào `AGENTS.md` (Codex, OpenCode đọc) và `CLAUDE.md` (Claude đọc) của dự án:

```markdown
## Liên lạc qua agmag
- Gửi tin cho agent khác: `agm send <tên-terminal> --type GIAO|BÁO CÁO|LỖI|HỎI|CHẶN|ĐẠT|CHƯA ĐẠT --task <ID> "<nội dung ngắn>"`
- Cần người dùng quyết định: `agm send user --type HỎI --task <ID> "<câu hỏi>"` (vào hộp thư của người dùng)
- Xem hộp thư (Codex quản lý): `agm inbox --json`
- Không dùng `agm kill` với phiên Codex/Claude — chỉ người dùng được làm việc đó.
```

agm tự biết ai gửi nhờ biến `HC_TERMINAL_ID` mà herdr-core đặt trong mỗi terminal. Mọi lệnh in JSON khi đầu ra không phải terminal (hoặc thêm `--json`) để agent đọc được. Xem thêm hướng dẫn chi tiết cho agent ở [AGENTS-TUTORIAL.md](AGENTS-TUTORIAL.md).

### MCP server

`agm mcp` là MCP server qua stdio (JSON-RPC 2.0, mỗi dòng một thông điệp; phiên bản giao thức 2025-06-18, 2025-03-26, 2024-11-05) để Claude Code và OpenCode gọi agm như tool. Đăng ký:

```bash
# Claude Code
claude mcp add -s user agm -- agm mcp
```

```json
// opencode.json
{"mcp":{"agm":{"type":"local","command":["agm","mcp"]}}}
```

| Tool | Việc |
|---|---|
| `agm_send` | Gửi tin có ghi lại (`kind`: `ASSIGN\|REPORT\|ERROR\|QUESTION\|BLOCKED\|PASS\|FAIL`; vẫn nhận mã tiếng Việt) |
| `agm_inbox` | Đọc hộp thư |
| `agm_ps` | Liệt kê terminal của herdr-core |
| `agm_quota` | Hạn mức gói như đang lưu trong DB, do `agm daemon` giữ mới (không quét log, không refresh) |
| `agm_spawn` | Sinh agent (kiểm tra như `agm spawn`) |
| `agm_branch_get` | Đọc một mục trong `BRANCHES.md` (`branches_file` phải là đường dẫn tuyệt đối) |
| `agm_branch_update` | Cập nhật một mục trong `BRANCHES.md` đã tồn tại (`branches_file` tuyệt đối; cùng quy tắc với `agm branch set`) |

Người gửi là `HC_TERMINAL_ID` của bên gọi, hoặc `agent` — không bao giờ là `user`. Đối số lạ của tool bị từ chối. Chỉ bật MCP server `agm` trong profile quản lý/review (BOT-CEO, Copilot, reviewer), không bật trong profile thợ thường: `agm_spawn` có thể mở terminal giống `agm spawn`. `agm mcp` không có tool kill/interrupt/manage/cleanup (cũng không có reply/done); những việc đó chỉ người dùng làm.

### BRANCHES.md

`agm branch get <ENTRY> [--file PATH]` in một mục; `agm branch set <ENTRY> <KEY=VALUE>... [--file PATH] [--worktree PATH]` cập nhật mục đó. `--file` mặc định `BRANCHES.md`, tệp này phải có sẵn (agm không tự tạo; hãy tạo trước với một dòng tiêu đề); tool MCP cần đường dẫn tuyệt đối. Mục là khối `### <lane>/<slug>` gồm các dòng `- key: value`. Khi ghi, agm giữ thư mục khóa `<thư mục chứa BRANCHES.md>/.agents/.runtime/locks/branches` (agm chỉ giữ vài mili-giây, chờ tối đa 30 s và thu hồi khóa cũ hơn 60 s, nên không được tự `mkdir` khóa bằng tay: giữ tay quá 60 s có thể bị thu hồi và mất cập nhật) và ghi bằng tệp tạm + đổi tên. Trạng thái hợp lệ: `wip blocked ready merging merged deployed needs-rebase failed`. Khóa hợp lệ: `status lane tier head base depends_on migration touches_hotspot summary verify review docs registered git`. `status: ready` cần `head` hex 12–40 ký tự, `--worktree`, worktree sạch, HEAD khớp và bằng chứng `review=.agents/reviews/<ID>-<n>.md@<sha>` (token đầu; phía sau được ghi chú tự do): tệp review phải đã commit ở HEAD, nhắc 7 ký tự đầu của `<sha>`, và từ `<sha>` tới HEAD không đổi tệp nào ngoài `.agents/`. Guard không đọc verdict. Đổi `head` hoặc `review` của mục đã `ready` cũng chạy lại guard. Terminal có vai trò `worker` bị từ chối.

## 7. Tất cả lệnh

| Lệnh | Việc |
|---|---|
| `agm quota [--refresh]` | Hạn mức gói còn lại theo cửa sổ thời gian (`--refresh`: đọc ngay hạn mức agy, Codex và OpenCode Go; dòng cũ/stale được đánh dấu trong bảng, trong TUI và trong `--json`) |
| `agm ls [--days N]` | Các phiên: nguồn, model, token, % ngữ cảnh, cảnh báo |
| `agm cost [--by model\|day\|month\|source\|cwd] [--days N \| --window 24h\|today\|month\|4months] [--summary]` | Tổng token, có hàng **SUM** in đậm; tóm tắt 24h/hôm nay/tháng này + bar chart 4 tháng theo giờ máy. **USD** = token × giá API niêm yết của từng model, tính cho mọi model (kể cả gói tháng, như số tiền nếu dùng qua API); `≥ $x` khi có token chưa biết giá, `?` khi không token nào có giá. Cột `billed (API)` trong `--summary` là tiền thực trả của model tính theo API |
| `agm log <session> [--limit N] [--full]` | Tóm tắt lượt hội thoại (`--full`: dòng log gốc, chỉ Codex/Claude) |
| `agm scan [--days N \| --since YYYY-MM-DD]` | Quét log Codex/Claude/agy/OpenCode ngay (các lệnh xem tự quét) |
| `agm daemon [--interval 10] [--replay <tệp>]` | Chạy nền: sự kiện hc, quét log, ghép terminal↔phiên, ngân sách, xoay vòng (`--replay`: nạp sự kiện từ tệp để gỡ lỗi) |
| `agm sys` | Tải máy hiện tại: CPU % kèm lưới **từng nhân logic** (`#0 ██████▏░ 76%`, dòng CPU ghi nhân bận nhất và số nhân rảnh), RAM (và swap), từng ổ đĩa (đã dùng/còn trống, đọc/ghi mỗi giây) và **cây tiến trình** của từng agent (`claude-code`, `codex`, `opencode`, `agy`; app desktop là `claude-app`, `codex-app`, `agy-app`): số tiến trình, RAM, % CPU toàn máy, kèm hàng **Σ SUM**. Báo cáo có màu gồm ba phần (MACHINE · DISKS · AGENTS), mở đầu bằng dòng đánh giá tải: xanh `● OK` (còn chỗ mở thêm thợ), vàng `● BUSY` (CPU hoặc RAM ≥ 60%), đỏ `● HIGH LOAD` (≥ 85%, tạm dừng mở thợ) và `⚠ <ổ> nearly full` khi ổ đầy từ 90%. Mỗi agent giữ màu như bảng TERMINALS và có thanh tỉ lệ RAM. Mất khoảng 1–1,5 giây (CPU % cần hai lần đo, cộng thời gian quét bảng tiến trình); không lưu gì. `--json` cho agent, `NO_COLOR=1` để in không màu |
| `agm tui [--refresh 2]` | Bảng điều khiển. Thanh tiêu đề luôn hiện `cpu · ram · ổ đầy nhất` (xanh < 60%, vàng < 85%, đỏ); `view system` thêm bảng SYSTEM đầy đủ, mỗi nhân một cột màu (`┊ cores ▆▃▆▅▅▅`) |
| `agm ps [--all]` | Terminal của herdr-core mà agm biết |
| `agm inbox [--all]` • `agm reply <#> <text>` • `agm done <IDS...> [--kind K] [--terminal T] [--source S]` | Hộp thư; trả lời câu hỏi của agent; đóng một hoặc nhiều mục |
| `agm send <đích> --type <LOẠI> [--task ID] <tin>` | Gửi tin có ghi lại; đích `user` → hộp thư |
| `agm spawn <template> --task <ID> [--role worker] [--name …] [--cwd …] -- <đối số…>` | Sinh agent (`hc run --bg --outlive-parent`), giữ nguyên từng đối số sau `--`; nếu `<template>` là template của hc, các đối số được gộp thành một chuỗi nhiệm vụ. Trên Windows, khi `hc run <template>` lỗi `cannot start "sh"`, agm dự phòng chạy thẳng `command:` của template và gửi thân template cùng nhiệm vụ vào terminal; nếu không gửi được brief đó (agent chưa sẵn sàng sau 20 giây, terminal đã thoát), brief vào hộp thư (`brief_failed`), địa chỉ vẫn được in và agm thoát mã **3** — terminal vẫn chạy, nên không dọn worktree và không spawn lại. Kiểm `max_workers` và ngân sách |
| `agm branch get <ENTRY> \| set <ENTRY> <KEY=VALUE>...` `[--file PATH] [--worktree PATH]` | Đọc / cập nhật một mục của `BRANCHES.md` dưới khóa, ghi nguyên tử |
| `agm mcp` | MCP server qua stdio với 7 tool (xem [MCP server](#mcp-server)) |
| `agm kill <t> [--yes]` · `agm interrupt <t> [--yes]` | Dừng / ngắt — Codex, Claude, app chưa rõ: **chỉ bạn**, luôn phải gõ `y` (`--yes` không áp dụng) |
| `agm manage <t> [--off] [--cmd … \| -- <argv>…]` | Bật/tắt quản lý xoay vòng cho phiên Claude Code hoặc Codex mở tay |
| `agm rotate <t> [--yes]` | Xoay vòng ngay: OpenCode worker (bàn giao + dừng + sinh mới) hoặc Claude/Codex đã `manage` (bàn giao + mở mới, **không** dừng phiên cũ) |
| `agm cleanup [targets…] [--all] [--yes]` | Dọn thủ công các phiên cũ đã bàn giao (1 lần gõ xác nhận `y/N`; `--yes` vẫn được nhận để tương thích nhưng không bỏ qua bước này) |
| `agm budget set <phạm vi> --soft X --hard Y [--action notify\|interrupt\|kill]` · `ls` · `rm <phạm vi>` | Ngân sách: `task:<ID>` (token) hoặc `provider:<tên>:<phút>` (% hạn mức) |
| `agm policy on\|off\|status` | Bật/tắt mọi hành động tự động (nút dừng khẩn) |
| `agm link <t> <session>` | Ghép tay terminal ↔ phiên khi agm không chắc |
| `agm ask [--model gemini-4-argon-high] [--timeout 300] <câu hỏi>` | Hỏi agy một lần (chỉ đọc, chạy trong thư mục tạm ngoài repo), ghi lại token |
| `agm hook install` · `agm hook statusline` | Gắn / chạy hook statusline của Claude Code |

Cờ chung: `--json` (mặc định khi đầu ra không phải terminal).

## 8. Thống kê token theo thời gian

Mọi mốc thời gian tính theo **giờ máy** (múi giờ địa phương), không phải UTC.

```powershell
agm cost --summary              # bảng 24h / Hôm nay / Tháng này + bar chart 4 tháng gần nhất
agm cost --by month             # tổng theo tháng
agm cost --window today --by source
agm scan --since 2026-08-01     # backfill log cũ (TUI tự làm việc này cho các tháng còn thiếu)
```

**Bảng giá.** Giá niêm yết dựng sẵn nằm ở `assets/prices.toml` (chụp ngày 2026-11-20 từ giá API công khai của nhà cung cấp; bậc cơ bản, không tính phụ phí ngữ cảnh dài). Tên dạng `provider/id` của OpenCode, id có hậu tố ngày và hậu tố effort của agy (`-high`) được tự khớp; id kết thúc bằng `-free` tính $0. Ghi đè hoặc thêm model trong `~/.agmag/providers.toml`:

```toml
[models."codex-auto-review"]
provider = "codex"
price = { input = 2.0, output = 10.0, cache_read = 0.1, cache_write = 2.5 }   # USD / 1M tokens
```

- Số tháng thống kê/backfill do `stats_months` trong `providers.toml` quyết định (mặc định 4 tháng dương lịch gần nhất).
- Nếu một khoảng thời gian chưa được quét đủ, agm đánh dấu là chưa đầy đủ thay vì hiện số thiếu.
- USD chỉ tính cho model `billing = "api"` có khai giá; nếu có model API chưa khai giá, tổng USD hiện `?` (không bịa số 0).

## 9. Ngân sách, policy và xoay vòng

**Ngân sách** (daemon kiểm mỗi chu kỳ):

```powershell
agm budget set task:login-7 --soft 2000000 --hard 5000000              # token cho một đầu việc
agm budget set provider:codex:10080 --soft 80 --hard 95                 # % ĐÃ DÙNG của hạn mức tuần (10080 phút); TUI/quota hiển thị phần còn lại
agm budget set task:login-7 --soft 2000000 --hard 5000000 --action kill # vượt cứng → dừng thợ OpenCode của việc đó
```

- Vượt **mềm** → một mục trong hộp thư (và tin cho terminal quản lý nếu cấu hình `manager`).
- Vượt **cứng** → hộp thư + hành động (`notify` mặc định); `agm spawn` cho đầu việc đó bị từ chối.
- Mỗi mức chỉ kích hoạt **một lần**; đóng mục hộp thư không làm nó báo lại. Codex/Claude không bao giờ bị dừng tự động — chỉ có đề nghị trong hộp thư.

**Xoay vòng thợ OpenCode** (thợ = sinh bằng `agm spawn` với `--role worker`):

1. Ngữ cảnh ≥ `ctx_warn` (50%) → cảnh báo trong hộp thư.
2. Ngữ cảnh ≥ `ctx_rotate` (70%) và thợ đang rảnh → agm yêu cầu ghi bàn giao vào `.agents/log/<ID>.md` và commit.
3. Khi thợ xong lượt: agm kiểm **worktree sạch** và **có commit mới chạm tệp đó**, kiểm luôn điều kiện mở phiên mới → dừng phiên cũ → mở phiên mới **cùng template/model/thư mục** với nhiệm vụ "đọc tasks/log/git log rồi làm tiếp".
4. Chưa bàn giao xong thì chờ; quá `handoff_timeout_min` → báo hộp thư, **giữ nguyên phiên cũ**. Thất bại rồi thì không tự thử lại — dùng `agm rotate <t>`.

**Xoay vòng phiên Claude Code & Codex (không bao giờ tự kill phiên cũ):**

1. Phiên mở tay cần bật quản lý một lần qua `agm manage <t>` — chỉ **bạn** bật được. agm lưu `launch_spec` (lệnh khởi chạy đã lọc an toàn theo app; có thể chỉ định bằng `--cmd "…"` hoặc `-- <argv>…`). Claude yêu cầu đã cài `agm hook install`. Tắt bằng `agm manage <t> --off`.
2. Khi đạt `interactive_ctx_rotate` (mặc định 50%; cảnh báo ở `interactive_ctx_warn`, 40%) và rảnh (`idle` hoặc `done`), agm yêu cầu ghi bàn giao vào `.agents/handoff/<id>-<ts>.md` với marker cuối `<!-- agm:handoff-done -->`. agm tự thêm `.agents/handoff/` vào `info/exclude` của repo (hỗ trợ cả git worktree), không ép commit WIP.
3. Khi tệp bàn giao hợp lệ, agm mở phiên mới đọc tệp đó rồi **chờ bạn** gõ tiếp; phiên cũ giữ nguyên ở trạng thái `[HANDED OFF]` để bạn kiểm tra và dọn thủ công bằng `agm cleanup` (hoặc `c` trong TUI). Đây là quy ước điều phối (không khóa ghi cấp hệ điều hành trên phiên cũ) — tránh sửa trùng trên cả hai cửa sổ; TUI cảnh báo đỏ khi cả hai cùng chạy một thư mục.

`agm policy off` tắt toàn bộ hành động tự động (vẫn chạy được cả khi `providers.toml` hỏng).

## 10. Cấu hình

`~/.agmag/providers.toml` (không có tệp → dùng mặc định trong [assets/providers.default.toml](assets/providers.default.toml)):

```toml
[providers.codex]
billing = "subscription"        # gói tháng: đo bằng % hạn mức; USD chỉ là giá trị theo bảng giá, không phải tiền trả
[providers.claude]
billing = "subscription"
[providers.openrouter]
billing = "api"                 # trả theo token: USD này là tiền thực trả (cột `billed (API)`)

[models."gpt-6.1-sol"]
provider = "codex"              # OpenCode dùng openai/gpt-6.1-sol cũng tiêu hạn mức ChatGPT
context_window = 475000
[models."some-api-model"]
provider = "openrouter"
price = { input = 3.0, cache_read = 0.3, cache_write = 3.75, output = 15.0 }   # USD / 1 triệu token

[aliases]
"opus" = "claude-opus-5-5"

[policy]
max_workers = 3                 # số thợ chạy cùng lúc tối đa
manager = "codex-mgr"           # terminal nhận cảnh báo ngân sách; bỏ trống = chỉ hộp thư
ctx_warn = 50                   # % ngữ cảnh: cảnh báo (thợ OpenCode và app khác)
ctx_rotate = 70                 # % ngữ cảnh: xoay vòng phiên (thợ OpenCode và app khác)
interactive_ctx_warn = 40       # % ngữ cảnh: cảnh báo, phiên Claude Code/Codex
interactive_ctx_rotate = 50     # % ngữ cảnh: xoay vòng, phiên Claude Code/Codex
handoff_timeout_min = 10
link_timeout_min = 5
stats_months = 4                # số tháng dương lịch gần nhất cho thống kê & backfill
agy_quota_poll_min = 15         # phút giữa hai lần đọc hạn mức agy (`agy --print /usage`); 0 = tắt
codex_quota_poll_min = 15       # phút giữa hai lần đọc hạn mức Codex (`codex app-server`); 0 = tắt
quota_stale_min = 60            # phút coi dữ liệu hạn mức là cũ/stale (cũng stale nếu có usage mới cùng gói sau đó)
```

Model chưa khai giá hoặc cửa sổ → agm hiện `?`, không bịa số 0.

**Cửa sổ ngữ cảnh OpenCode.** Với model OpenCode, mục `[models."<providerID/id>"]` (hoặc `[models."<id>"]`) trong `providers.toml` luôn thắng; nếu không có, agm đọc giới hạn từ danh mục model của OpenCode (`opencode api GET /api/model`, cache trong tiến trình). Không đoán số, nên model chưa biết vẫn hiện `?`. Phiên OpenCode có tổng token bằng 0 (tin nhắn assistant lỗi/rỗng) không còn bị gắn `unknown format?`.

**Biến môi trường** (tùy chọn):

| Biến | Mặc định | Ý nghĩa |
|---|---|---|
| `AGM_HOME` | `~/.agmag` | Nơi lưu `agm.db`, `providers.toml` |
| `AGM_CODEX_DIR` · `AGM_CLAUDE_DIR` | `~/.codex/sessions` · `~/.claude/projects` | Nơi đọc log Codex / Claude Code |
| `AGM_OPENCODE_DB` | `~/.local/share/opencode/opencode.db` | CSDL OpenCode |
| `AGM_AGY_DIR` | `~/.gemini/antigravity-cli` | Thư mục gốc của Antigravity CLI |
| `AGM_CLAUDE_HOME` | `~` | Thư mục home chứa `.claude/settings.json` (kiểm hook khi `manage`) |
| `AGM_HC_BIN` · `AGM_AGY_BIN` | `hc` · `agy` | Đường dẫn tệp chạy `hc` / `agy` |

## 11. An toàn và dữ liệu

**Quy tắc dừng agent**

- **Codex và Claude không bao giờ bị dừng tự động** — kể cả khi xoay vòng. Agent hay policy chỉ dừng được terminal mà `hc get` xác nhận là OpenCode (danh sách trắng); app chưa rõ → từ chối, đề nghị chuyển vào hộp thư của bạn.
- Người gọi được xác định *không chắc thì coi là agent*: có `HC_TERMINAL_ID` mà không đọc được terminal, terminal là app agent, hoặc không có TTY → agent.
- Dừng Codex/Claude: phải gõ `y` tại terminal; `--yes` không áp dụng cho chúng.
- `agm manage` và `agm cleanup` chỉ người dùng chạy được và luôn cần xác nhận.

**Dữ liệu**

- agm **chỉ đọc** log của Claude/Codex, `transcript.jsonl` + `conversation_summaries.db` của agy, và `opencode.db` (chỉ hai bảng phiên/tin nhắn — không bao giờ đọc bảng chứa thông tin đăng nhập). CSDL của các công cụ đó được mở ở chế độ `READ_ONLY`.
- Ngoài `~/.agmag/`, agm chỉ ghi ở năm chỗ: `BRANCHES.md` và thư mục khóa `.agents/.runtime/locks/branches` (qua `agm branch set` hoặc tool MCP `agm_branch_update`), `statusLine` trong `~/.claude/settings.json` (khi bạn chạy `agm hook install`), dòng `.agents/handoff/` trong `info/exclude` của repo (khi xoay vòng phiên Claude/Codex đã `manage`; tệp bàn giao do chính agent ghi), và thư mục tạm `agmag-ask` cho `agm ask`.
- Mọi thứ agm lưu nằm ở `~/.agmag/agm.db` trên máy bạn: token, hạn mức, tóm tắt ≤ 200 ký tự mỗi lượt, tin nhắn qua `agm send`, hộp thư, nhật ký can thiệp. Không gửi đi đâu.

## 12. Giới hạn đã biết

- Phiên chạy trong HERDR APP được đo token qua log, nhưng chỉ terminal của **herdr-core** mới can thiệp được.
- Token của agy ngoài `antigravity-cli` (ví dụ phiên IDE không ghi `transcript.jsonl`) chỉ đo được qua `agm ask`.
- Hạn mức ChatGPT do OpenCode tiêu không có trong log; agm thấy gián tiếp qua lần ghi `rate_limits` kế tiếp của Codex.
- Ngân sách theo provider chỉ cảnh báo, chưa chặn `spawn` (agm chưa biết template dùng provider nào).
- `agm sys` nhận agent theo tên tiến trình (herdr không cho PID), nên đếm mọi tiến trình Claude Code/Codex/OpenCode/agy trên máy chứ không chỉ terminal herdr; tiến trình con tính cho agent tổ tiên gần nhất (thợ do CEO mở tính cho thợ). Dung lượng tính 1 GB = 1024³ byte như Task Manager của Windows.
- Bàn giao Claude/Codex là quy ước, không khóa ghi phiên cũ — nếu bạn gõ tiếp vào phiên cũ, hai phiên có thể sửa trùng.
- Agent có TTY, tự xóa `HC_TERMINAL_ID` rồi tự gõ `y` về lý thuyết vẫn được coi là người dùng (agm không kiểm cây tiến trình cha).
- `agm tui` là bảng vẽ lại theo chu kỳ + dòng lệnh, chưa có điều hướng bằng phím.

## 13. Phát triển

```powershell
cargo test                                    # ~190 test
cargo clippy --all-targets
python -I tools/argon_review.py               # nhờ Gemini Argon (qua agy) review theo nhóm mô-đun
python -I tools/sanitize_fixture.py <log.jsonl> <fixture.jsonl>   # làm sạch log thật thành fixture
```

| Thư mục / tệp | Nội dung |
|---|---|
| `src/parse/`, `src/ingest.rs`, `src/opencode.rs`, `src/agy.rs` | Đọc log Codex/Claude/agy (jsonl) và OpenCode (SQLite) |
| `src/store/` | CSDL SQLite (WAL, schema v3) dùng chung giữa daemon và CLI |
| `src/daemon.rs`, `src/hc.rs`, `src/correlate.rs` | Sự kiện herdr-core, client `hc`, ghép terminal ↔ phiên |
| `src/actions.rs`, `src/policy.rs`, `src/rotation.rs`, `src/launch.rs`, `src/git.rs` | Gửi/trả lời/sinh/dừng có bảo vệ, ngân sách, xoay vòng & bàn giao, `launch_spec`, kiểm git |
| `src/hook.rs`, `src/ask.rs` | Hook statusline Claude, agy một lần |
| `src/report.rs`, `src/dashboard.rs`, `src/main.rs`, `src/ops.rs`, `src/tui.rs` | Báo cáo & thống kê, TUI, CLI |
| `docs/superpowers/specs/` · `docs/superpowers/plans/` · `docs/reviews/` | Thiết kế, kế hoạch, báo cáo review |
| `.agents/decisions/p0-research.md` | Kết quả xác minh định dạng log, sự kiện hc, statusline |
| [AGENTS-TUTORIAL.md](AGENTS-TUTORIAL.md) · [HERDR-CORE-BOT.md](HERDR-CORE-BOT.md) | Hướng dẫn cho agent, ghi chú herdr-core |
| [CHANGELOG.md](CHANGELOG.md) | Lịch sử phiên bản |
