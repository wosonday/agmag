# agmag

**Bảng điều khiển model cho đội agent chạy trong [herdr](https://github.com/yigitkonur/awesome-herdr) / herdr-core.**

`agm` đứng cạnh herdr và cho bạn biết, trong một chỗ:

- mỗi agent (Claude Code, Codex, OpenCode, agy/Gemini) đã tiêu **bao nhiêu token**, ngữ cảnh đầy **bao nhiêu %**;
- gói tháng (ChatGPT, Claude) còn **bao nhiêu % hạn mức** và **khi nào reset**;
- agent nào **đang dừng lại hỏi bạn** (hộp thư);
- các agent **đã nói gì với nhau**.

Và cho bạn can thiệp có kiểm soát: giao việc có giới hạn số thợ và ngân sách, trả lời agent, dừng agent (Codex/Claude chỉ khi **bạn** ra lệnh), tự **xoay vòng** thợ OpenCode khi ngữ cảnh đầy để tiết kiệm hạn mức của model đắt tiền.

> Nền tảng chính: Windows 11 (cũng chạy trên Linux/macOS). Viết bằng Rust, một tệp `agm.exe`, không cần runtime.

---

## Mục lục

1. [Cài đặt](#1-cài-đặt)
2. [Dùng ngay trong 1 phút](#2-dùng-ngay-trong-1-phút)
3. [agm hoạt động thế nào — có phải cài vào từng session không?](#3-agm-hoạt-động-thế-nào)
4. [Chạy nền và bảng điều khiển](#4-chạy-nền-và-bảng-điều-khiển)
5. [Khi nào dùng `hc`, khi nào dùng `agm`](#5-khi-nào-dùng-hc-khi-nào-dùng-agm)
6. [Cho agent dùng agm](#6-cho-agent-dùng-agm)
7. [Tất cả lệnh](#7-tất-cả-lệnh)
8. [Ngân sách, policy và xoay vòng thợ](#8-ngân-sách-policy-và-xoay-vòng-thợ)
9. [Cấu hình](#9-cấu-hình)
10. [An toàn và dữ liệu](#10-an-toàn-và-dữ-liệu)
11. [Giới hạn đã biết](#11-giới-hạn-đã-biết)
12. [Phát triển](#12-phát-triển)

---

## 1. Cài đặt

Cần [Rust](https://rustup.rs) (có `cargo`).

```powershell
git clone <địa chỉ repo này> agmag
cd agmag
cargo install --path .          # chép agm.exe vào ~/.cargo/bin (đã có trong PATH)
agm --version                   # agm 0.1.0
```

> `cargo build --release` **chỉ** tạo `target\release\agm.exe` — gõ `agm` sẽ báo "not recognized". Hãy dùng `cargo install --path .`.
> Mỗi lần cập nhật mã: chạy lại `cargo install --path .`. Mở lại cửa sổ PowerShell nếu vẫn chưa thấy lệnh.

Dữ liệu của agm nằm ở `~/.agmag/` (`agm.db`, `providers.toml`). Đổi bằng biến môi trường `AGM_HOME`.

## 2. Dùng ngay trong 1 phút

Không cần cấu hình gì:

```powershell
agm quota        # hạn mức gói: 5 giờ / 7 ngày, còn bao lâu tới reset
agm ls           # phiên hôm nay: model, token, % ngữ cảnh
agm cost         # tổng token theo model  (--by day | source | cwd, --days 7)
```

Ví dụ `agm quota`:

```
gói     cửa sổ  đã dùng
codex   5h      █░░░░░░░░░  14%   còn 4h38m
codex   7d      █████████░  93%   còn 115h41m
```

Lần đầu hơi chậm (đọc log 7 ngày gần nhất); sau đó chỉ đọc phần mới.

## 3. agm hoạt động thế nào

**Không phải cài vào từng session.** agm cài một lần cho cả máy và *đọc dấu vết mà mỗi công cụ vốn đã ghi*, nên mọi session — cũ, mới, mở bằng herdr hay mở tay — đều được đếm:

```
 Claude Code ──ghi──▶ ~/.claude/projects/*.jsonl ──────────────┐
 Codex       ──ghi──▶ ~/.codex/sessions/*.jsonl  ──────────────┼──▶ agm ──▶ ~/.agmag/agm.db
 OpenCode    ──ghi──▶ ~/.local/share/opencode/opencode.db ─────┘     ▲
 herdr-core  ──sự kiện (hc events)──▶ agm daemon ────────────────────┘
                                         │
                       bạn xem ở: agm quota / ls / cost / inbox / tui
```

| Công cụ | Phải làm gì? | Thấy số liệu ở đâu |
|---|---|---|
| **Claude Code** | Không bắt buộc. Muốn có **% hạn mức gói 5h/7d** thì chạy **một lần** `agm hook install` (áp dụng cho mọi session Claude) | Thanh trạng thái dưới cùng của Claude Code: `Opus 5.5 · ctx 34% · 5h 24% · 7d 41%`; và mọi lệnh `agm` |
| **Codex** | Không gì cả (hạn mức gói có sẵn trong log Codex) | `agm quota / ls / tui` |
| **OpenCode** | Không gì cả (agm đọc `opencode.db`, chỉ đọc) | `agm ls / cost / tui` |
| **agy (Gemini)** | agy không lưu token trên máy → chỉ đo được khi hỏi qua `agm ask "…"` | `agm cost` |

> `agm hook install` thêm `statusLine` vào `~/.claude/settings.json`. Nếu bạn đã có statusLine riêng, lệnh từ chối và không ghi đè — gọi `agm hook statusline` từ script của bạn.

## 4. Chạy nền và bảng điều khiển

Các lệnh ở mục 2 đủ để **xem**. Muốn agm **theo dõi liên tục** (terminal nào đang chạy, ai đang hỏi bạn, ngân sách, xoay vòng thợ) thì chạy daemon — nên chạy trong herdr-core để nó sống độc lập với cửa sổ:

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
agmag · policy bật · 7 terminal · hộp thư 1
── HẠN MỨC GÓI ───────────────────────────────
  codex     5h  █░░░░░░░░░  14%  còn 4h38m
  codex     7d  █████████░  93%  còn 115h41m
── TERMINALS ─────────────────────────────────
 t    tên          app       trạng thái          task     token  ngữ cảnh
 t1q               codex     working                      21.2M  58%
 t5   w-login-7-1  opencode  BLOCKED (question)  login-7  340k   50%
── NGÂN SÁCH ─────────────────────────────────
  task:login-7  ██░░░░░░░░  11%  340k / 3.0M tokens
── HỘP THƯ ───────────────────────────────────
  #1 [blocked] t5 — Chạy test?
lệnh: r <#> <trả lời> · d <#> · s <t> <LOẠI> <tin> · i <t> · k <t> · p on|off · q
```

Lệnh trong TUI (gõ rồi Enter): `r 1 có` trả lời mục 1 · `d 1` đóng mục 1 · `s t5 GIAO làm tiếp` gửi tin · `i t5` ngắt · `k t5` dừng (phải gõ thêm `y`) · `p off` tắt hành động tự động · `q` thoát. TUI chỉ mở được bởi **bạn** ở terminal (agent không dùng được).

## 5. Khi nào dùng `hc`, khi nào dùng `agm`

`hc` là **tay chân** (chạy, nhìn, gõ vào terminal). `agm` là **sổ sách và luật** (ghi lại, kiểm hạn mức, chặn hành động nguy hiểm) — agm gọi `hc` bên dưới.

| Việc | Dùng |
|---|---|
| Mở agent để **bạn tự làm việc cùng** | `hc run` / mở trong herdr |
| Vào xem / gõ trực tiếp | `hc attach t5`, `hc read t5` |
| Chạy lệnh, test nền (không phải agent) | `hc run --bg …`, `hc exec …` |
| **Giao việc cho thợ** (sinh agent theo đầu việc) | `agm spawn worker-mid --task login-7 -- "…"` |
| **Gửi tin cho agent** | `agm send w-login-7-1 --type GIAO --task login-7 "…"` |
| **Trả lời agent đang hỏi / xin quyền** | `agm reply 1 "có"` (hoặc `r` trong TUI) |
| **Dừng agent** | `agm kill t5` (có bảo vệ; `hc kill` thì không) |
| Xem token, hạn mức, ai đang chờ bạn | `agm quota / ls / inbox / tui` |

Quy tắc ngắn:
1. Việc của **đội agent** (giao, nhắn, trả lời, dừng, sinh thêm) → `agm`.
2. **Bạn tự thao tác tay** → `hc`.
3. Agent nhắn nhau → luôn `agm send` (để có hồ sơ và để câu hỏi tới được hộp thư của bạn).

Agent sinh bằng `hc run` vẫn được **đo token**, nhưng không có nhãn đầu việc → agm không áp ngân sách đầu việc và không tự xoay vòng nó. Muốn agm quản lý đầy đủ thì sinh bằng `agm spawn`.

## 6. Cho agent dùng agm

Thêm đoạn này vào `AGENTS.md` (Codex, OpenCode đọc) và `CLAUDE.md` (Claude đọc) của dự án:

```markdown
## Liên lạc qua agmag
- Gửi tin cho agent khác: `agm send <tên-terminal> --type GIAO|BÁO CÁO|LỖI|HỎI|CHẶN|ĐẠT|CHƯA ĐẠT --task <ID> "<nội dung ngắn>"`
- Cần người dùng quyết định: `agm send user --type HỎI --task <ID> "<câu hỏi>"` (vào hộp thư của người dùng)
- Xem hộp thư (Codex quản lý): `agm inbox --json`
- Không dùng `agm kill` với phiên Codex/Claude — chỉ người dùng được làm việc đó.
```

agm tự biết ai gửi nhờ biến `HC_TERMINAL_ID` mà herdr-core đặt trong mỗi terminal. Mọi lệnh in JSON khi đầu ra không phải terminal (hoặc thêm `--json`) để agent đọc được.

## 7. Tất cả lệnh

| Lệnh | Việc |
|---|---|
| `agm quota` | Hạn mức gói theo cửa sổ thời gian |
| `agm ls [--days N]` | Các phiên: nguồn, model, token, % ngữ cảnh, cảnh báo |
| `agm cost [--by model\|day\|month\|source\|cwd] [--days N \| --window 24h\|today\|month\|4months] [--summary]` | Tổng token, tóm tắt 24h/ngày/4 tháng theo giờ máy và bar chart; USD chỉ cho model trả theo API có khai giá |
| `agm log <session> [--limit N] [--full]` | Tóm tắt lượt hội thoại (`--full`: dòng log gốc, chỉ Codex/Claude) |
| `agm scan [--days N \| --since YYYY-MM-DD]` | Quét log ngay (các lệnh xem tự quét) |
| `agm daemon [--interval 10]` | Chạy nền: sự kiện hc, quét log, ghép terminal↔phiên, ngân sách, xoay vòng |
| `agm tui [--refresh 2]` | Bảng điều khiển |
| `agm ps [--all]` | Terminal của herdr-core mà agm biết |
| `agm inbox [--all]` · `agm reply <#> <text>` · `agm done <#>` | Hộp thư; trả lời câu hỏi của agent; đóng mục |
| `agm send <đích> --type <LOẠI> [--task ID] <tin>` | Gửi tin có ghi lại; đích `user` → hộp thư |
| `agm spawn <template> --task <ID> [--role worker] [--name …] [--cwd …] -- <nhiệm vụ>` | Sinh agent (`hc run --bg --outlive-parent`) có kiểm `max_workers` và ngân sách |
| `agm kill <t>` · `agm interrupt <t>` | Dừng / ngắt — Codex, Claude, app chưa rõ: **chỉ bạn**, luôn phải gõ `y` |
| `agm manage <t> [--off] [--cmd … \| -- <argv>…]` | Bật/tắt quản lý xoay vòng cho phiên Claude Code hoặc Codex mở tay |
| `agm rotate <t>` | Xoay vòng ngay OpenCode worker hoặc phiên Claude/Codex đã `manage` |
| `agm cleanup [targets…] [--all] [--yes]` | Dọn dẹp thủ công các phiên cũ đã bàn giao (1 lần xác nhận `y/N`) |
| `agm budget set <phạm vi> --soft X --hard Y [--action notify\|interrupt\|kill]` · `ls` · `rm` | Ngân sách |
| `agm policy on\|off\|status` | Bật/tắt mọi hành động tự động (nút dừng khẩn) |
| `agm link <t> <session>` | Ghép tay terminal ↔ phiên khi agm không chắc |
| `agm ask [--model gemini-4-argon-high] <câu hỏi>` | Hỏi agy một lần (chỉ đọc), ghi lại token |
| `agm hook install` · `agm hook statusline` | Gắn / chạy hook statusline của Claude Code |

## 8. Ngân sách, policy và xoay vòng thợ

**Ngân sách** (daemon kiểm mỗi chu kỳ):

```powershell
agm budget set task:login-7 --soft 2000000 --hard 5000000              # token cho một đầu việc
agm budget set provider:codex:10080 --soft 80 --hard 95                 # % hạn mức gói tuần (10080 phút)
agm budget set task:login-7 --soft 2000000 --hard 5000000 --action kill # vượt cứng → dừng thợ OpenCode của việc đó
```

- Vượt **mềm** → một mục trong hộp thư (và tin cho terminal quản lý nếu cấu hình `manager`).
- Vượt **cứng** → hộp thư + hành động (`notify` mặc định); `agm spawn` cho đầu việc đó bị từ chối.
- Mỗi mức chỉ kích hoạt **một lần**; đóng mục hộp thư không làm nó báo lại. Codex/Claude không bao giờ bị dừng tự động — chỉ có đề nghị trong hộp thư.

**Xoay vòng thợ OpenCode** (thợ = sinh bằng `agm spawn` với `--role worker`):

1. Ngữ cảnh ≥ 50% → cảnh báo trong hộp thư.
2. Ngữ cảnh ≥ 70% và thợ đang rảnh → agm yêu cầu ghi bàn giao vào `.agents/log/<ID>.md` và commit.
3. Khi thợ xong lượt: agm kiểm **worktree sạch** và **có commit mới chạm tệp đó**, kiểm luôn điều kiện mở phiên mới → dừng phiên cũ → mở phiên mới **cùng template/model/thư mục** với nhiệm vụ "đọc tasks/log/git log rồi làm tiếp".
4. Chưa bàn giao xong thì chờ; quá `handoff_timeout_min` → báo hộp thư, **giữ nguyên phiên cũ**. Thất bại rồi thì không tự thử lại — dùng `agm rotate <t>`.

**Xoay vòng phiên Claude Code & Codex (không tự kill phiên cũ):**

1. Phiên mở tay cần bật quản lý một lần qua `agm manage <t>` (lưu `launch_spec` an toàn theo app; Claude yêu cầu đã cài `agm hook install`).
2. Khi đạt `ctx_rotate` và rảnh (`idle` hoặc `done`), agm yêu cầu ghi bàn giao vào `.agents/handoff/<id>-<ts>.md` (tự thêm `.agents/handoff/` vào `info/exclude` của repo, không ép commit WIP) với marker cuối `<!-- agm:handoff-done -->`.
3. Khi tệp bàn giao hợp lệ, agm mở phiên mới đọc tệp đó rồi **chờ bạn** gõ tiếp; phiên cũ giữ nguyên ở trạng thái `[ĐÃ BÀN GIAO]` để bạn kiểm tra và dọn thủ công bằng `agm cleanup` (hoặc `c` trong TUI). Đây là quy ước điều phối (không khóa ghi cấp hệ điều hành trên phiên cũ — tránh sửa trùng trên cả hai cửa sổ).

`agm policy off` tắt toàn bộ hành động tự động (vẫn chạy được cả khi `providers.toml` hỏng).

## 9. Cấu hình

`~/.agmag/providers.toml` (không có tệp → dùng mặc định):

```toml
[providers.codex]
billing = "subscription"        # gói tháng: đo bằng % hạn mức, không quy ra USD
[providers.claude]
billing = "subscription"
[providers.openrouter]
billing = "api"                 # trả theo token: agm cost tính USD nếu model có giá

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
ctx_warn = 50                   # % ngữ cảnh: cảnh báo
ctx_rotate = 70                 # % ngữ cảnh: xoay vòng phiên
handoff_timeout_min = 10
link_timeout_min = 5
stats_months = 4                # số tháng dương lịch gần nhất cho thống kê & backfill
```

Model chưa khai giá hoặc cửa sổ → agm hiện `?`, không bịa số 0.

## 10. An toàn và dữ liệu

**Quy tắc dừng agent**

- **Codex và Claude không bao giờ bị dừng tự động.** Agent hay policy chỉ dừng được terminal mà `hc get` xác nhận là OpenCode (danh sách trắng); app chưa rõ → từ chối, đề nghị chuyển vào hộp thư của bạn.
- Người gọi được xác định *không chắc thì coi là agent*: có `HC_TERMINAL_ID` mà không đọc được terminal, terminal là app agent, hoặc không có TTY → agent.
- Dừng Codex/Claude: phải gõ `y` tại terminal; `--yes` không áp dụng cho chúng.

**Dữ liệu**

- agm **chỉ đọc** log của Claude/Codex và `opencode.db` (chỉ hai bảng phiên/tin nhắn — không bao giờ đọc bảng chứa thông tin đăng nhập). agm không ghi vào thư mục của các công cụ đó.
- Mọi thứ agm lưu nằm ở `~/.agmag/agm.db` trên máy bạn: token, hạn mức, tóm tắt ≤ 200 ký tự mỗi lượt, tin nhắn qua `agm send`, hộp thư, nhật ký can thiệp. Không gửi đi đâu.

## 11. Giới hạn đã biết

- Phiên chạy trong HERDR TUI (app) được đo token qua log, nhưng chỉ terminal của **herdr-core** mới can thiệp được.
- Token của agy chỉ đo được qua `agm ask`.
- Hạn mức ChatGPT do OpenCode tiêu không có trong log; agm thấy gián tiếp qua lần ghi `rate_limits` kế tiếp của Codex.
- Ngân sách theo provider chỉ cảnh báo, chưa chặn `spawn` (agm chưa biết template dùng provider nào).
- Agent có TTY, tự xóa `HC_TERMINAL_ID` rồi tự gõ `y` về lý thuyết vẫn được coi là người dùng (agm không kiểm cây tiến trình cha).
- `agm tui` là bảng vẽ lại theo chu kỳ + dòng lệnh, chưa có điều hướng bằng phím.

## 12. Phát triển

```powershell
cargo test                                    # ~160 test
cargo clippy --all-targets
python -I tools/argon_review.py               # nhờ Gemini Argon (qua agy) review theo nhóm mô-đun
python -I tools/sanitize_fixture.py <log.jsonl> <fixture.jsonl>   # làm sạch log thật thành fixture
```

| Thư mục / tệp | Nội dung |
|---|---|
| `src/parse/`, `src/ingest.rs`, `src/opencode.rs` | Đọc log Codex/Claude (jsonl) và OpenCode (SQLite) |
| `src/store/` | CSDL SQLite (WAL) dùng chung giữa daemon và CLI |
| `src/daemon.rs`, `src/hc.rs`, `src/correlate.rs` | Sự kiện herdr-core, client `hc`, ghép terminal ↔ phiên |
| `src/actions.rs`, `src/policy.rs`, `src/rotation.rs` | Gửi/trả lời/sinh/dừng có bảo vệ, ngân sách, xoay vòng |
| `src/hook.rs`, `src/ask.rs` | Hook statusline Claude, agy một lần |
| `src/report.rs`, `src/dashboard.rs`, `src/main.rs`, `src/ops.rs`, `src/tui.rs` | Báo cáo, TUI, CLI |
| `docs/superpowers/specs/` · `docs/superpowers/plans/` · `docs/reviews/` | Thiết kế, kế hoạch, báo cáo review |
| `.agents/decisions/p0-research.md` | Kết quả xác minh định dạng log, sự kiện hc, statusline |
