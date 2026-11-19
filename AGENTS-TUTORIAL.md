# Chỉ dẫn làm việc với đội agent (phiên bản 3.4: Claude + Codex + OpenCode + GitHub Copilot + Google Gemini / agy + HERDR + agm)

Chỉ dẫn dùng chung cho dự án code hoặc việc khác (tài liệu, báo cáo, phân tích).

> **Thứ tự ưu tiên tài liệu (từ cao xuống thấp):** `AGENTS.md` / `CLAUDE.md` của từng dự án **>** `AGENTS-TUTORIAL.md` (quy tắc cứng, quyền & cổng phát hành) **>** [BOT-CEO.md](BOT-CEO.md) (mở rộng khi tách nhiều lane) **>** [HERDR-CORE-BOT.md](HERDR-CORE-BOT.md) (hạ tầng `agm`/`hc`). Hướng dẫn chi tiết cho từng model nằm ở [docs/README.md](docs/README.md).

> **Cập nhật 3.4 (2026-11-19):**
> - **Bảo toàn hạn mức Claude Code & Codex:** chỉ mở Claude Code cho **Kiến trúc sư / BOT-CEO** (và ca leo thang rủi ro cao). **Opus High qua `agy`** được giữ dưới dạng **phiên tương tác trong repo** (`--mode plan --sandbox`, giữ `conversation_id` để tận dụng `cache_read_tokens`, **không** dùng `agm ask` one-shot) để làm **Đồng Kiến trúc sư (bàn thiết kế/chia việc thay Claude)** và **Reviewer cấp cao phân xử ca khó** (nhánh thường bỏ bước Pre-review để không tốn 2 lần model lớn). Vì OpenCode chạy `openai/gpt-6.1-sol` **tiêu chung hạn mức ChatGPT (5h/7d) với Codex CLI**, tách **Reviewer mặc định** sang OpenRouter / Ollama Cloud hoặc `agy` (`gemini-4-argon-high` / `gemini-4-hydrogen-high`); giữ Codex (`gpt-6.1-sol`) cho **engineering khó** và **review chéo ca phức tạp**. Khi bật `BOT-CEO.md`, **bỏ tầng `codex-mgr` trung gian thường trực**; còn báo cáo `.agents/reports/<ID>.md` chuyển sang model rẻ / Hydrogen Medium.
> - **Tận dụng tối đa model miễn phí:** giao hẳn **Audit**, **tìm kiếm trong repo / tóm tắt log dài**, **thợ bậc Nhanh (`worker-fast`)**, **`gitnexus-sync`** và **sàng lọc quy ước trước Reviewer** cho model miễn phí (`opencode/deepseek-v4.1-flash-free`, OpenRouter `:free`, Ollama cục bộ `num_ctx ≥ 64k`) theo cơ chế thang leo.
> - **Cập nhật tài liệu tự động theo cổng:** **Gemini 4 Hydrogen Medium** (`gemini-4-hydrogen-medium` qua `agy`) nhận lệnh cập nhật docs khi có thay đổi (và có thể tổng hợp luôn `.agents/reports/<ID>.md`) **sau khi Reviewer đã duyệt/chỉnh sửa và trước khi gọi GitHub Copilot**.
> - **Cố vấn kỹ thuật & tối ưu Harnessing:** **Gemini Argon** (`gemini-4-argon-*` qua `agy`) đóng vai trò cố vấn kỹ thuật giống Codex, với trách nhiệm đặc thù là cập nhật và tối ưu hóa hệ thống harnessing giữa các phiên để khắc phục những vấn đề lớn của phiên trước (bao gồm hiệu chỉnh phân bậc `tier:` cho model miễn phí).
> - **Bỏ phần ảnh 2D/3D và video** khỏi chỉ dẫn chung (nếu dự án nào cần, sẽ tối ưu riêng trong harness của dự án đó).
> - **Điều phối nền bằng `agm` (`agmag` v0.2.0):** thay cơ chế theo dõi/giao tiếp thủ công trong `herdr-core` bằng `agm` (`agm daemon`, `agm spawn`, `agm send`, `agm ask`, `agm inbox`, `agm tui`, xoay vòng và ngân sách). Chi tiết xem [HERDR-CORE-BOT.md](HERDR-CORE-BOT.md).
> - **Mô hình đa mảng (BOT-CEO):** với dự án lớn cần tách nhiều mảng độc lập để tránh phình ngữ cảnh, xem [BOT-CEO.md](BOT-CEO.md).
> - **Hướng dẫn riêng từng model:** xem [docs/README.md](docs/README.md).

> **Cập nhật 3.3 (2026-11-18):** giao quản lý phiên bản Git cho **GitHub Copilot**: tích hợp nhánh, lịch sử, revert và dọn nhánh đã tích hợp. Với MATHY-AI, Copilot nhận vai bot GIT theo `docs/harness/GIT_BOT.md` (hướng dẫn chung xem [docs/COPILOT-GIT.md](docs/COPILOT-GIT.md)).

> **Cập nhật 3.2 (2026-11-18):** cách cài/nâng cấp OpenCode v2 (`@opencode/cli`, tự cập nhật, bẫy dịch vụ nền trên Windows) và nơi v2 lưu đăng nhập — mục 2.1. Với MATHY-AI, bản vận hành chính thức nằm ở `HARNESSING.md` §5.3, §5.7.

> **Cập nhật 3.1 (2026-11-17, bổ sung 2026-11-18):** thêm mục 4.2.1, gồm kết quả chạy `hc` (herdr-core 0.1.21) trên máy thật Windows 11.

## 0. Tóm tắt mô hình

- **Mỗi phiên OpenCode = một lập trình viên riêng**, làm trên một git worktree riêng, nên nhiều feature chạy song song mà không đè lên nhau.
- **Việc cơ khí** (viết code theo đặc tả, sửa lỗi nhỏ, chạy test, tìm trong repo, đổi tên) giao cho OpenCode. Model của từng phiên có thể đổi bất cứ lúc nào theo yêu cầu của Codex hoặc Opus.
- **Kiến trúc sư** (Claude Code: Opus 5.5 Extra hoặc Fable 5.2; khi tách nhiều mảng thì mỗi lane là một **BOT-CEO** theo `BOT-CEO.md`) quyết định hướng đi và rủi ro. **Claude Code chỉ mở cho vai Kiến trúc sư / BOT-CEO** và ca leo thang rủi ro cao.
- **Đồng Kiến trúc sư & Reviewer cấp cao** (**Opus High qua `agy`**, chạy phiên tương tác trong repo `--mode plan --sandbox` / nối `--conversation <id>` để tận dụng `cache_read_tokens`) gánh thay trực tiếp tải của Claude Code khi bàn thiết kế/chia việc và phân xử review các nhánh phức tạp (không dùng làm bước Pre-review thừa cho nhánh thường).
- **Reviewer mặc định** chạy qua **OpenCode** dùng model mạnh **khác OpenAI** (OpenRouter / Ollama Cloud) hoặc **`agy`** (`gemini-4-argon-high` / `gemini-4-hydrogen-high`) để không ăn vào hạn mức 5h/7d của Codex. Chỉ gọi `openai/gpt-6.1-sol` (Codex) để review chéo ca logic phức tạp khi `agm quota` còn rộng, dùng `agy` Opus High cho ca khó, và chỉ leo thang về Claude Opus 5.5 High khi đụng bảo mật, dữ liệu, migration DB, kiến trúc lớn hoặc `CHƯA ĐẠT` 2 lần liên tiếp.
- **Cập nhật tài liệu (Docs)** (**Gemini 4 Hydrogen Medium** qua `agy`) nhận lệnh cập nhật docs khi có thay đổi (và tổng hợp `.agents/reports/<ID>.md` nếu có), thực hiện **sau khi nhánh đã qua Reviewer / chỉnh sửa xong và trước khi gọi GitHub Copilot**.
- **Codex (`openai/gpt-6.1-sol`)** là kỹ sư chuyên sâu cho **engineering khó** (sửa lỗi 2 lần chưa đạt, refactor đa tệp) và **quản lý các phiên OpenCode ở dự án 1 luồng** (khi bật `BOT-CEO.md`, BOT-CEO gọi thẳng thợ, không đặt thêm tầng `codex-mgr` trung gian). Lưu ý: OpenCode dùng `openai/gpt-6.1-sol` tiêu **chung** hạn mức ChatGPT với Codex CLI — luôn theo dõi `agm quota`.
- **Gemini Argon** (`gemini-4-argon-*` qua `agy`) đóng vai trò **cố vấn kỹ thuật** tương tự Codex, nhưng phụ trách phần đặc thù là **cập nhật và tối ưu hóa các harnessing** (`AGENTS.md`, `CLAUDE.md`, `.agents/`, template, skill, cổng kiểm) để các phiên sau cải thiện được những vấn đề lớn của phiên trước.
- **GitHub Copilot quản lý phiên bản Git:** nhận nhánh và SHA đã qua kiểm tra (và đã cập nhật docs nếu có) để tích hợp; xử lý lịch sử, revert/rollback và dọn nhánh trong phạm vi được cho phép.
- **Mọi agent ghi nhật ký và lưu tệp `.md` ngay trên nhánh đang làm**, commit cùng với code, và kiểm các thay đổi bằng git trước khi bàn giao (mục 3.3).
- Các phiên chạy trong **herdr / herdr-core (`hc`)**, được theo dõi, gửi tin có ghi sổ, giới hạn ngân sách và xoay vòng qua **`agm`** (bộ điều khiển `agmag`, mục 3.2 và 4.2); `herdr-mesh` chỉ dùng bổ sung nếu chạy trong HERDR TUI độc lập. Khi trao đổi với Kiến trúc sư/BOT-CEO, người dùng mở thẳng phiên trong HERDR và chat trực tiếp (kèm `agm manage`), còn `agm` lo đo token, hộp thư và `agm ask` một lượt.

### 0.1. Kiến trúc tổng thể

```
             HERDR / herdr-core (hc) + agm (daemon, TUI, sổ sách, policy)
                              |
              ORCHESTRATOR & CỐ VẤN
              (Kiến trúc sư / BOT-CEO · agy Opus High [đồng kiến trúc] · Codex [1 luồng] · Gemini Argon)
                              |
          +-------------------+-------------------+
          |                   |                   |
          v                   v                   v
   Research / Audit     Developer Agent      Reviewer Agent
   (Miễn phí / Gemini)  (Thợ cơ khí)         - Mặc định: OpenCode (non-OpenAI)
   - agy / explore      - worker-fast: :free   hoặc agy (Argon/Hydrogen High)
   - Audit: :free/local - mid: OpenRouter    - Ca khó: agy Opus High / Codex
                        - khó: Codex         - Rủi ro cao: Claude Opus High
          |                   |                   |
          +-------------------+-------------------+
                              |
              GIT WORKSPACE (mỗi phiên một worktree + nhánh)
                              |
               TEST / VALIDATION (Audit + Pre-check: model miễn phí)
                              |
               REVIEWER (1 tầng cho nhánh thường; ca khó dùng agy Opus High)
                              |
               DOCS & REPORT UPDATE (Gemini 4 Hydrogen Medium, nếu có thay đổi)
                              |
               GIT INTEGRATION (GitHub Copilot, đúng SHA)
                              |
               GITNEXUS UPDATE (agent gitnexus-sync, OpenCode :free / --index-only)
                              |
               HARNESS RETRO & OPTIMIZATION (Gemini Argon cho phiên sau)
                              |
               FINAL REPORT (Hydrogen Medium / model rẻ tổng hợp, mục 8)
```

Cách đọc sơ đồ:

1. **HERDR / herdr-core (`hc`)** là nơi chạy và quan sát mọi phiên; **`agm`** là kênh giao tiếp ghi sổ (`agm send`), hộp thư (`agm inbox`), theo dõi token/ngữ cảnh, áp ngân sách và xoay vòng phiên (mục 3.2, 4.2).
2. **Orchestrator & Cố vấn** gồm: Kiến trúc sư (hoặc từng BOT-CEO theo mảng trong `BOT-CEO.md`), **Opus High qua `agy`** (đồng Kiến trúc sư giữ phiên trong repo để gánh bớt việc bàn thiết kế/chia việc cho Claude), Codex (quản lý ở dự án 1 luồng; ở chế độ BOT-CEO thì lùi về vai engineering khó để không trùng vai điều phối), và **Gemini Argon** (cố vấn kỹ thuật + tổng kết vấn đề lớn để cập nhật, tối ưu harnessing cho phiên sau). Orchestrator không tự làm phần việc code chính của thợ.
3. **Ba nhánh thực thi chính:**
   - *Research / Audit Agent* = vai Tìm kiếm & Kiểm tra (chỉ đọc mã / chạy lệnh). Dùng **Google Gemini** (`agy` / `agm ask`) cho tìm kiếm web; tìm trong repo, tóm tắt log dài và **Audit** dùng **model miễn phí** trong OpenCode (`:free`, `deepseek-v4.1-flash-free`, Ollama cục bộ) để tiết kiệm hạn mức.
   - *Developer Agent* = vai Thợ cơ khí. Mặc định giao việc hẹp/rõ cho `worker-fast` (model miễn phí); lỗi 2 lần hoặc việc khó nâng lên `worker-mid`/`strong` (OpenRouter/Ollama Cloud) hoặc **Codex** trong worktree riêng (khi `agm quota` cho phép).
   - *Reviewer Agent:* Nhánh thường chỉ duyệt **1 lần** bằng **OpenCode non-OpenAI (OpenRouter/Ollama Cloud) hoặc `agy` (`Argon/Hydrogen High`)** để không ăn vào hạn mức ChatGPT/Codex và không đốt 2 lần model lớn. Với ca khó / bất đồng mới gọi **`agy` Opus High** (hoặc `gpt-6.1-sol` review chéo), và chỉ leo thang **Claude Code Opus 5.5 High** cho ca bảo mật/dữ liệu/migration/2 lần `CHƯA ĐẠT`.
4. **Git workspace & Docs Update:** mọi kết quả đi qua nhánh/worktree. Sau khi nhánh qua Reviewer (và đã chỉnh sửa xong nếu có lỗi), **Gemini 4 Hydrogen Medium** nhận lệnh cập nhật docs khi có thay đổi và commit trên nhánh đó trước khi chuyển SHA cho GitHub Copilot.
5. **Test / Validation** là cổng Audit ở mục 6; chưa đạt thì quay lại Developer Agent (lỗi chuyển nguyên văn).
6. **Harness Optimization & Final Report:** Gemini Argon rút kinh nghiệm vấn đề lớn để tối ưu harness cho phiên kế tiếp; Codex tổng hợp báo cáo cuối theo mục 8.

## 1. Vai trò và model

| Cấp | Vai | Model đề xuất | Công cụ | Quyền | Việc |
|---|---|---|---|---|---|
| 1 | Kiến trúc sư (hoặc BOT-CEO theo lane) | Opus 5.5 Extra hoặc Fable 5.2 | Claude Code (`/model`, kèm `agm manage`) | Chỉ đọc (hoặc sửa `BRANCHES.md` có khóa) | Phương án, rủi ro, tiêu chí đạt; chẩn đoán lỗi khó; đánh giá SẴN SÀNG trước phát hành. **Đây là vai chính duy nhất mở Claude Code thường xuyên** |
| 1–2 | Đồng Kiến trúc sư & Reviewer cấp cao | **Opus High qua `agy`** (`claude-opus-5-5-high`, đã kiểm `agy models` 2026-11-20) | Antigravity CLI `agy --mode plan --sandbox` (giữ phiên tương tác trong repo / `--conversation <id>`, **không** dùng `agm ask` one-shot) | Chỉ đọc | Bàn thiết kế, phản biện phương án, chia `.agents/tasks/<ID>.md` thay Claude Code khi cần tiết kiệm hạn mức Claude; duyệt / phân xử các nhánh khó hoặc khi Reviewer mặc định phân vân |
| 2 | Reviewer (mặc định) | Model mạnh **khác OpenAI** (OpenRouter / Ollama Cloud) hoặc **`agy`** (`gemini-4-argon-high` / `gemini-4-hydrogen-high`) *(review chéo ca khó: `gpt-6.1-sol`; leo thang rủi ro cao: Claude Opus 5.5 High)* | OpenCode (`opencode run --agent reviewer` / `agm spawn --role review`) hoặc `agy` | Chỉ đọc + ghi `.agents/reviews/` | Duyệt độ đúng, phạm vi, quy ước mà không ăn vào hạn mức ChatGPT/Codex; báo ĐẠT/CHƯA ĐẠT, lỗi theo mức Nghiêm trọng/Lớn/Nhỏ kèm vị trí và cách sửa |
| 2 | Kỹ sư chuyên sâu & Quản lý (Codex) | `openai/gpt-6.1-sol` (Codex CLI / OpenCode) | Codex CLI | Đọc; sửa code **khi nhận việc engineering khó**; quản lý thợ OpenCode ở dự án 1 luồng | Làm các task khó/refactor/fix sau 2 lần fail (khi `agm quota` cho phép); ở dự án 1 luồng (không bật `BOT-CEO.md`) chia đầu việc, giám sát thợ và bàn giao Copilot |
| 2 | Cố vấn kỹ thuật & Tối ưu Harness | **Gemini Argon** (`gemini-4-argon-medium` / `gemini-4-argon-high`) | Antigravity CLI `agy` (chỉ đọc qua `agm ask` để cố vấn/review; phiên `agy` trong worktree riêng khi sửa harness) | Đọc; chỉ ghi vào tệp harness/điều phối khi được giao | Cố vấn kỹ thuật độc lập giống Codex; phân tích các vấn đề lớn của phiên vừa chạy để **cập nhật và tối ưu hóa harnessing** (`.agents/`, `AGENTS.md`, `CLAUDE.md`, template, skill, cổng, ngưỡng `tier:`) cho các phiên sau |
| 2 | Quản lý phiên bản Git | Model chọn trong GitHub Copilot | GitHub Copilot CLI (`copilot`) | Tích hợp đúng SHA đã kiểm chứng; thao tác nhánh/lịch sử theo phạm vi cho phép | Merge, revert/rollback, dọn nhánh đã tích hợp; ghi SHA và kết quả kiểm; push/tag/phát hành cần quyền của người dùng |
| 3 | Cập nhật tài liệu (Docs) & Báo cáo | **Bậc nhẹ trước:** `gemini-3.8-flash-medium` / `gemini-4-hydrogen-low` (docs nhỏ & `.agents/reports/<ID>.md`); **bậc vừa:** `gemini-4-hydrogen-medium` (tài liệu kiến trúc dài/nhiều tệp) | Antigravity CLI `agy` (trong worktree nhánh cần cập nhật) | Chỉ sửa tài liệu / docs & `.agents/reports/` trong phạm vi được giao | Nhận lệnh cập nhật docs + báo cáo cuối **sau khi Reviewer đã chỉnh sửa/duyệt xong và trước khi gọi GitHub Copilot**; commit trên cùng nhánh |
| 3 | Thợ cơ khí | **Mặc định ưu tiên bậc Nhanh (model nhẹ / `:free` / local)** khi có cổng ESLint/CI + test; nâng bậc theo mục 2 | OpenCode trong worktree riêng | Sửa trong phạm vi được giao; cấm sửa cấu hình lint hoặc thêm `eslint-disable`/`@ts-ignore` | Viết/sửa code hoặc nội dung đúng đặc tả, bắt buộc chạy đạt `eslint --max-warnings 0` (hoặc `ruff`/`clippy`) + test trước khi commit |
| 4 | Tìm kiếm & Tóm tắt log | **Mặc định nhẹ:** `gemini-3.8-flash-low/medium` (~7s); tổng hợp sâu mới dùng `gemini-4-argon-low/medium`; trong repo: **model miễn phí** | `agy` / `agm ask` (web); OpenCode `explore` dùng `:free`/local (mã repo & tóm tắt log dài) | Chỉ đọc | Tìm mã, nguồn, số liệu; tóm tắt log test/build dài trước khi đưa vào Kiến trúc sư/BOT-CEO; trả danh sách vị trí kèm bằng chứng |
| 4 | Audit & Cổng ESLint/CI | **Model nhẹ / miễn phí** (`opencode/deepseek-v4.1-flash-free`, `:free` OpenRouter, Ollama local) | OpenCode (worktree riêng) | Chạy lệnh, ghi `.agents/log/`, không sửa mã nguồn | Chạy `eslint --max-warnings 0`, quét diff chặn `eslint-disable`/`@ts-ignore`/sửa cấu hình lint ([docs/ESLINT-CI-GUARDRAILS.md](docs/ESLINT-CI-GUARDRAILS.md)), chạy test/build, kiểm quy ước trước khi gọi Reviewer |
| 4 | Cập nhật GitNexus (`gitnexus-sync`) | **Model miễn phí** (`opencode/deepseek-v4.1-flash-free`, `:free` OpenRouter, Ollama local) | OpenCode (`agm spawn gitnexus-sync --task <ID> --role index` hoặc `hc run --rm`) | Chỉ chạy lệnh `gitnexus` (`status`, `analyze --index-only`, `clean`) & kiểm tra MCP server; không sửa code hay tệp harness | Làm mới chỉ mục đồ thị sau khi Copilot merge vào nhánh chung, khi `staleness` báo `behind`/`diverged`, hoặc khi worktree đổi cấu trúc; báo `BÁO CÁO`/`LỖI` qua `agm send` |

Ghi chú:

1. Tên model cụ thể của OpenCode do người dùng cấu hình trong `opencode.json` (định dạng `nhà-cung-cấp/model`). Chỉ dẫn này chỉ quy định **bậc** (nhanh / vừa / mạnh), không cố định tên.
2. Mỗi vai của OpenCode nên được khai báo thành một agent trong `.opencode/agents/<tên>.md` với trường `model` và quyền (`edit`, `bash`) riêng. Như vậy đổi model chỉ cần sửa một dòng. (Repo MATHY-AI hiện dùng thư mục `.opencode/agent/`, số ít; theo đúng thư mục mà bản OpenCode đang cài nhận ra.)
3. Phần **tạo ảnh 2D/3D và video** không còn nằm trong bộ vai trò dùng chung; nếu dự án nào cần, sẽ bổ sung và tối ưu riêng trong harness của dự án đó.

### 1.1. Skill theo vai

Skill nói **cách làm**; tệp tác vụ nói **làm gì**. Khi mâu thuẫn, quy tắc dự án và `tasks/<ID>.md` thắng.

**Hạn mức khi khởi tạo một phiên (bắt buộc):**

1. Mỗi phiên khi khởi tạo chỉ nạp **1 đến 3 skill**.
2. **Thợ cơ khí tối đa 2 skill.**
3. Cần skill khác thì **thay** skill cũ (hoặc mở phiên mới), không cộng dồn.
4. Skill do Codex (hoặc Kiến trúc sư) chọn theo loại việc và ghi vào trường `skills:` trong `.agents/tasks/<ID>.md`, ví dụ `skills: test-driven-development, verification-before-completion`. Phiên chỉ dùng đúng các skill đã ghi.
5. Không nạp skill kiểu "phải gọi skill trước mọi hành động" cho thợ, Audit, Tìm kiếm, Docs và bot; chúng kéo thêm skill thiết kế và hỏi đáp vào việc cơ khí, và có thể làm phiên chạy nền treo chờ người trả lời.

| Vai | Số skill khi khởi tạo | Gợi ý (chọn theo giai đoạn) |
|---|---|---|
| Kiến trúc sư | 1 đến 3 | `brainstorming` (giai đoạn hiểu yêu cầu), `system-design` hoặc `architecture` (giai đoạn thiết kế/ghi quyết định); đổi skill khi sang giai đoạn khác |
| Đồng trao đổi (`agy` Opus High) | 0 đến 1 | Không cần nạp nhiều; đọc thẳng phương án/diff + `.agents/tasks/<ID>.md` |
| Codex quản lý | 1 đến 3 | `writing-plans`, `dispatching-parallel-agents`, `using-git-worktrees`; `testing-strategy` hoặc `requesting-code-review` khi cần, thay cho skill không còn dùng |
| Gemini Argon (Cố vấn & Harness) | 1 đến 2 | Đọc `AGENTS.md`, `.agents/log/`, `.agents/reviews/`, `.agents/decisions/` để tổng kết lỗi hệ thống và cập nhật harness |
| GitHub Copilot quản lý Git | 0 đến 1 | `verification-before-completion` nếu hỗ trợ; ưu tiên playbook Git của dự án |
| Reviewer | 1 đến 2 | `code-review`, `verification-before-completion` |
| Cập nhật docs (`agy` Hydrogen Medium) | 0 đến 1 | Đọc diff nhánh + `tasks/<ID>.md` + docs hiện hữu |
| Thợ cơ khí | **1 đến 2** | Xem bảng loại việc bên dưới |
| Audit | 0 đến 1 | `verification-before-completion` |
| Tìm kiếm | 0 đến 1 | Skill "exploring" của GitNexus (nếu dùng) |
| Cập nhật GitNexus (`gitnexus-sync`) | 0 | Chỉ chạy `gitnexus` CLI & `agm send` |
| Bot | 0 | Chỉ cần `agm` và `hc` |

**Chọn skill cho thợ theo loại việc (tối đa 2):**

| Loại việc | Skill |
|---|---|
| Thêm tính năng, có đặc tả rõ | `test-driven-development` + `verification-before-completion` |
| Sửa lỗi | `systematic-debugging` + `verification-before-completion` |
| Sửa theo kết quả duyệt (CHƯA ĐẠT) | `receiving-code-review` + `verification-before-completion` |
| Việc cơ khí đơn giản (đổi tên, định dạng) | 1 skill hoặc không có |

**Cách gắn và kiểm soát:**

1. Giữ bộ skill chung trong repo (một nguồn); OpenCode đọc được `.opencode/skills`, `.claude/skills`, `.agents/skills`. **[KIỂM]** đường dẫn skill của Codex.
2. Giới hạn theo vai bằng danh sách cho phép trong chỉ dẫn mở đầu (phần thân template) và trường `skills:`; nếu công cụ có cấu hình quyền skill theo từng agent thì dùng thêm. **[KIỂM]** với OpenCode.
3. Script giao việc từ chối đầu việc có quá 2 skill cho thợ (xem `HERDR-CORE-BOT.md`, mục 1.5).
4. Kiểm chứng: chạy cùng một đầu việc mẫu có và không có skill; giữ skill nào làm giảm số lần CHƯA ĐẠT hoặc số lượt sửa lại. Ngưỡng 1 đến 3 và tối đa 2 là kinh nghiệm, cần hiệu chỉnh theo kết quả thật.

## 2. Bậc model cho thợ OpenCode và cách đổi (Đổi hàng rào ESLint/CI lấy Model nhẹ)

| Bậc | Dùng khi | Ví dụ |
|---|---|---|
| Nhanh (**mặc định ưu tiên**) | Mọi việc rõ đặc tả trong phạm vi **≤ 2–3 tệp** đã có hàng rào `eslint --max-warnings 0` (hoặc `ruff`/`clippy`) + test tự kiểm ([docs/ESLINT-CI-GUARDRAILS.md](docs/ESLINT-CI-GUARDRAILS.md)) | Tìm trong repo, chạy test/lint, đổi tên, định dạng, **CRUD, UI component, API handler, viết unit test/fixture, refactor cục bộ** |
| Vừa | Việc liên kết 3–5 tệp, logic nhiều nhánh phức tạp hoặc vừa fail 2 lần ở bậc Nhanh | Luồng nghiệp vụ đa module, tích hợp dịch vụ ngoài |
| Mạnh | Việc khó đa tệp, thuật toán/đồng thời phức tạp, hoặc phiên vừa thất bại ở bậc Vừa | Lỗi race condition, migration dữ liệu phức tạp, tối ưu hiệu năng |

Quy tắc đổi:

1. Codex hoặc Opus được **yêu cầu đổi model hoặc mở phiên mới** khi: thợ sửa 2 lần chưa đạt; việc rõ ràng khó hơn dự kiến; hoặc việc dễ hơn dự kiến (hạ bậc để tiết kiệm).
2. Khi đổi model, giữ nguyên worktree và nhánh; giao lại bằng bản tóm tắt (đã làm gì, còn gì, lỗi nguyên văn) ghi vào tệp tác vụ (mục 3.2) rồi báo qua `agm send` (hoặc `herdr-mesh` nếu ở TUI).
3. Chỉ Codex hoặc Opus được đổi model của thợ. Thợ không tự đổi.

### 2.1. Nguồn cung cấp model cho thợ cơ khí (OpenCode, Ollama, OpenRouter)

OpenCode là phần mềm chạy thợ; model đến từ các **nhà cung cấp (provider)** cấu hình trong `opencode.json`. Khóa API nhập bằng `/connect` hoặc `opencode auth login`, không nằm trong repo. Mã model có dạng `provider_id/model_id`.

**Cài và cập nhật OpenCode (đã kiểm 2026-11-18, Windows 11):**

1. Cài bản v2 bằng `npm install -g @opencode/cli`. Gói `opencode-ai` trên npm là dòng **v1 cũ**; trình cài bằng `curl` cũng có thể để lại bản v1 ở `~/.opencode/bin`. Kiểm bằng `opencode --version` (phải là 2.x) trong **mọi** shell (PowerShell, Git Bash).
2. Bật tự cập nhật: thêm `"update": "auto"` vào `~/.config/opencode/opencode.json` (giá trị khác: `"notify"`, `"disable"`).
3. Trên Windows, **thoát hẳn OpenCode trước khi nâng cấp**: dịch vụ nền `opencode serve --service` giữ tệp `.exe`, làm `opencode upgrade` / `npm i -g` treo và có thể xóa mất lệnh `opencode`. Nâng cấp cũng dừng mọi phiên thợ đang chạy.
4. v2 lưu thông tin đăng nhập trong SQLite `~/.local/share/opencode/opencode.db` (xem `opencode debug paths`), **không còn** `auth.json`. Script nào cần biết provider đã đăng nhập phải đọc `opencode auth list` (hoặc tệp khai báo riêng), không đọc `auth.json`.

**Ba nguồn:**

| Nguồn | Cách nối | Ưu | Nhược / lưu ý |
|---|---|---|---|
| **Ollama chạy cục bộ** | Khai báo provider `ollama`, gói `@ai-sdk/openai-compatible`, `baseURL` là `http://localhost:11434/v1`, liệt kê model trong `models`; hoặc chạy `ollama launch opencode` để tự cấu hình | Không tốn phí token; mã không rời máy | Phụ thuộc phần cứng. Ollama khuyến nghị cửa sổ ngữ cảnh tối thiểu 64k cho OpenCode; nếu gọi công cụ lỗi, tăng `num_ctx` |
| **Ollama Cloud** | Lấy khóa API tại ollama.com (Settings, Keys), chạy `opencode auth login` rồi chọn Ollama Cloud; hoặc `ollama launch opencode`. Endpoint `https://ollama.com`, biến môi trường `OLLAMA_API_KEY`. Model cloud có hậu tố `:cloud` | Model lớn không cần GPU cục bộ; Ollama gợi ý `glm-4.7:cloud` cho OpenCode | Mã gửi lên dịch vụ ngoài; có giới hạn số model cloud chạy đồng thời theo gói (xem bên dưới) |
| **OpenRouter** | `/connect` rồi tìm OpenRouter; thêm model bằng slug dưới `provider.openrouter.models` trong `opencode.json`; mã dùng là `openrouter/<slug>` | Một khóa truy cập nhiều model; chọn được nhà cung cấp hạ nguồn qua `options.provider` của từng model | Mã gửi lên dịch vụ ngoài; chất lượng và giá phụ thuộc model, nhà cung cấp hạ nguồn |

**Gói và giới hạn của Ollama Cloud (biến động, phải xem lại trên ollama.com/pricing):** các trang tôi đọc không thống nhất. Một số nêu số model cloud chạy đồng thời theo gói (Free 1, Pro 3, Max 10); một số nêu Ollama đã chuyển sang tính theo token từ 31/08/2026, gói Pro 20 USD kèm 60 USD sử dụng, Max 100 USD kèm 300 USD, và bậc 0 USD trả theo mức dùng. Vì thế `max_workers` trong `~/.agmag/providers.toml` **không được vượt giới hạn đồng thời của gói đang dùng**, nếu giới hạn đó còn áp dụng.

**Gán nguồn cho bậc model (mục 2):** đây là gợi ý, tên model phải kiểm theo danh mục hiện hành vì thay đổi liên tục.

| Bậc | Nguồn gợi ý |
|---|---|
| Nhanh (miễn phí / 0 đồng biên) | **OpenCode/OpenRouter `:free`** (`opencode/deepseek-v4.1-flash-free`, `openrouter/thinkingmachines/inkling-2-small:free`, kiểm thêm bằng `opencode models \| findstr :free`) hoặc **Ollama cục bộ** (chủ yếu cho Audit, `explore`/tóm tắt log, sàng lọc quy ước, việc cơ khí 1–2 tệp có test sẵn) |
| Vừa | Ollama Cloud hoặc model code trả phí rẻ trên OpenRouter / `gpt-6.1-terra-fast` |
| Mạnh | Model mạnh trên OpenRouter / `openai/gpt-6.1-sol` (Codex), hoặc leo thang lên Claude Code cho ca kiến trúc/rủi ro cao |

**Chiến lược tận dụng tối đa model nhẹ / miễn phí (giảm tải hạn mức mà không gây vòng lặp `CHƯA ĐẠT`):**

1. **Khóa bằng hàng rào ESLint & CI bắt buộc ([docs/ESLINT-CI-GUARDRAILS.md](docs/ESLINT-CI-GUARDRAILS.md)):** ép mọi model tuân thủ bộ rule cứng (`max-lines ≤ 350`, `max-lines-per-function ≤ 70`, `complexity ≤ 10`, cấm `any`, cấm `@ts-ignore`/`eslint-disable`, cấm `catch {}` rỗng, cấm quên `await`). Trình lint bắt ngay 80% lỗi của model nhẹ trong 1 giây (0 token LLM) để `worker-fast` tự sửa trước khi commit; nếu model nào cố chèn `eslint-disable`/`@ts-ignore` hoặc sửa file cấu hình lint, **`auditor` và CI sẽ chặn cứng ngay lập tức**.
2. **Mở rộng việc cho model nhẹ / miễn phí:** đưa hẳn **Audit (`auditor`)**, **`gitnexus-sync`**, **tìm kiếm trong repo (`explore`) & tóm tắt log dài**, **sàng lọc sơ bộ trước Reviewer**, và **mặc định mọi task code rõ đặc tả ≤ 2–3 tệp (`worker-fast`)** sang `:free`, Ollama cục bộ 7B–14B hoặc `gpt-6.1-terra-fast`. Với `agy`, ưu tiên `gemini-3.8-flash-low/medium` cho research nhanh và tổng hợp `.agents/reports/<ID>.md` / docs nhỏ.
3. **Thang leo 2 lần:** mặc định giao việc rõ ràng cho bậc Nhanh (nhẹ/miễn phí); chỉ khi thợ không vượt qua được cổng ESLint/test sau 2 lần tự sửa mới nâng bậc lên `worker-mid` / Codex.
4. **Dự phòng chống nghẽn `429`:** các endpoint `:free` hay bị giới hạn tốc độ theo phút hoặc gọi công cụ kém ổn định — luôn chuẩn bị sẵn 2–3 slug `:free` (và Ollama local nếu có) để đổi nguồn ngay khi nghẽn.
5. **Cắt token đầu vào bằng GitNexus & theo dõi qua `agm`:** dùng **GitNexus MCP** (`query`, `context`, `impact`) ở chế độ chỉ đọc để agent nhẹ không đọc tràn ngữ cảnh; định kỳ chạy `agm cost --by source` và `agm quota` để đo tỷ lệ token đã chuyển sang `opencode`/`agy`, rồi giao **Gemini Argon** chỉnh lại phân loại `tier:` nếu một nhóm việc dùng model nhẹ hay bị trả lại.

**Khai báo trong repo (mẫu, thay bằng model có thật):**

```json
# opencode.json (không chứa khóa API)
{
  "$schema": "https://opencode.ai/config.json",
  "provider": {
    "ollama": {
      "npm": "@ai-sdk/openai-compatible",
      "name": "Ollama (local)",
      "options": { "baseURL": "http://localhost:11434/v1" },
      "models": { "qwen3-coder": { "name": "qwen3-coder" } }
    },
    "openrouter": {
      "models": { "qwen/qwen3-coder": {} }
    }
  }
}
```

```markdown
# .opencode/agents/worker-fast.md   (tương tự worker-mid.md, worker-strong.md)
---
description: Thợ bậc nhanh
mode: subagent
model: ollama/qwen3-coder
---
```

Với Ollama Cloud, đừng viết tay cấu hình; chạy `opencode auth login` hoặc `ollama launch opencode`, rồi lấy mã chính xác bằng `opencode models` (**[KIỂM]** mã provider thực tế).

**Quy tắc sử dụng:**

1. **Đổi nguồn do Codex hoặc Opus quyết định**, không do thợ (mục 2, quy tắc đổi). Khi nguồn bị giới hạn tốc độ hoặc lỗi, Codex đổi sang nguồn khác cùng bậc, giữ nguyên worktree, và ghi vào nhật ký.
2. **Dữ liệu nhạy cảm:** đầu việc chạm bảo mật hoặc dữ liệu (mục 5, quy tắc 2) chỉ dùng model cục bộ, hoặc phải được người dùng đồng ý trước khi gửi mã lên nguồn cloud.
3. **Khóa API:** chỉ lưu trong kho đăng nhập của OpenCode (`opencode auth login`) hoặc biến môi trường (`OLLAMA_API_KEY`, `OPENROUTER_API_KEY`) của dịch vụ chạy nền; không đưa vào repo, tệp tác vụ hay nhật ký.
4. **Chi phí:** người dùng quyết định ngân sách (mục 3.1); `agm` giới hạn `max_workers` và ngân sách token/hạn mức qua `~/.agmag/providers.toml` và `agm budget`.
5. **Chọn model theo năng lực gọi công cụ:** thợ phải gọi công cụ ổn định (sửa tệp, chạy lệnh). Trước khi dùng model mới cho thợ, chạy thử một đầu việc mẫu: sửa một tệp trong worktree, chạy test, ghi nhật ký, commit. Model không qua thử nghiệm thì không được giao việc thật.
6. Tệp tác vụ có thể ghi trường `tier: nhanh | vừa | mạnh`; `dispatch.sh` chọn template tương ứng (`worker-fast`, `worker-mid`, `worker-strong`) khi gọi `agm spawn`.

### 2.2. Antigravity CLI `agy`: tìm kiếm, đồng trao đổi, docs, cố vấn & tối ưu harness

Người dùng dùng Antigravity CLI (`agy`) cho bốn vai trò riêng biệt (không còn dùng phần tạo ảnh 2D/3D và video dùng chung; dự án nào cần media sẽ tự tối ưu harness riêng):

| Vai trò | Model qua `agy` (kiểm danh sách bằng `agy models`) | Chế độ chạy | Ghi chú |
|---|---|---|---|
| **Tìm kiếm / research** | `gemini-3.8-flash-{low,medium,high}` (nhanh), `gemini-4-argon-{low,medium,high}` (sâu) | Chỉ đọc: `agm ask` (cho câu hỏi web độc lập 1 lượt) hoặc `agy --mode plan --sandbox` | Tìm mã trong repo không cần web dùng OpenCode `explore` với model miễn phí |
| **Đồng Kiến trúc sư & Reviewer cấp cao** | **Opus High qua `agy`** (`claude-opus-5-5-high`) | Phiên tương tác `agy --mode plan --sandbox` mở tại repo (hoặc nối `--conversation <id>`); **không** dùng `agm ask` one-shot | Tận dụng `cache_read_tokens` và tự đọc tệp trong repo để bàn thiết kế/chia việc thay Claude Code, hoặc phân xử review ca khó (bỏ Pre-review trên nhánh thường) |
| **Cập nhật tài liệu (Docs) & Báo cáo** | **`gemini-4-hydrogen-medium`** | Phiên `agy` trong worktree nhánh đang làm | Nhận lệnh cập nhật docs + tổng hợp `.agents/reports/<ID>.md` **sau khi Reviewer đã chỉnh sửa/duyệt xong và trước khi gọi GitHub Copilot**; commit trên cùng nhánh |
| **Cố vấn kỹ thuật & Tối ưu Harnessing** | **`gemini-4-argon-medium`** / **`gemini-4-argon-high`** | Chỉ đọc qua `agm ask` (hoặc script kiểu `argon_review.py` của `agmag`) để cố vấn/review; phiên `agy` trong worktree riêng khi sửa tệp harness | Đóng vai cố vấn kỹ thuật giống Codex, nhưng đặc thù phụ trách **cập nhật và tối ưu hóa harnessing** (`.agents/`, `AGENTS.md`, `CLAUDE.md`, template, skill, cổng) dựa trên các vấn đề lớn của phiên trước |

**Mẫu lệnh một lần (chỉ đọc, tự ghi token vào `agm`):**

```bash
# Cách ưu tiên: qua agm ask (tự chạy agy --mode plan --sandbox trong thư mục tạm ngoài repo và ghi token vào agm.db)
agm ask --model gemini-4-argon-high --timeout 300 "<câu hỏi hoặc đoạn diff/mã cần cố vấn>"

# Hoặc gọi agy trực tiếp ngoài repo (khi cần xuất JSON thô)
cd <thư-mục-tạm-ngoài-repo>
agy --model gemini-4-argon-medium --mode plan --sandbox --print-timeout 600s --output-format json \
  -p "<câu hỏi>. Mỗi kết luận kèm URL nguồn. Không sửa tệp." > research.json
```

**Quy tắc riêng cho các vai chạy qua `agy`:**

1. **Research là gợi ý, không phải sự thật.** Lần thử 2026-11-18, Gemini báo sai phiên bản một gói npm dù kèm đúng URL. Codex/Kiến trúc sư đối chiếu mọi con số, phiên bản, giá với nguồn gốc (lệnh `npm view`, trang chính thức) trước khi đưa vào tệp tác vụ.
2. `agy` có quyền riêng, **không** đọc quyền của agent OpenCode. Các vai chỉ đọc (Tìm kiếm, Đồng trao đổi, Cố vấn chỉ đọc) luôn chạy `--mode plan --sandbox` ở thư mục tạm ngoài repo (hoặc qua `agm ask`); không bao giờ `--dangerously-skip-permissions`.
3. **Vai sửa tệp qua `agy` (Hydrogen Medium sửa docs, Argon sửa harness):** chỉ mở phiên tương tác trong worktree đúng phạm vi và **chỉ giao đúng phạm vi được sửa** (docs với Hydrogen Medium; tệp harness/điều phối với Argon), không sửa code nghiệp vụ của thợ.
4. **Đo token trong `agm`:** `agm` tự quét token của phiên `agy` tương tác từ `transcript.jsonl` và ghi nhận `agm ask` riêng (khử trùng lặp); riêng phiên IDE không ghi `transcript.jsonl` chỉ đo được qua `agm ask`.
5. Dữ liệu gửi đi theo đúng quy tắc mục 2.1 quy tắc 2: không bí mật, không dữ liệu khách.

## 3. Điều phối và giao tiếp

### 3.1. Trách nhiệm

- **Người dùng:** quyết định về nghiệp vụ, quyền, dữ liệu, chi phí; phê duyệt phát hành; chỉ người dùng được dừng/ngắt phiên Codex hoặc Claude Code. Khi trao đổi nhiều lượt với Kiến trúc sư / BOT-CEO, người dùng mở thẳng phiên trong HERDR và chat trực tiếp (kèm `agm manage`), còn theo dõi/trả lời hộp thư và hỏi 1 lượt thì dùng `agm tui` / `agm ask`.
- **Kiến trúc sư (hoặc từng BOT-CEO theo lane trong `BOT-CEO.md`):** làm chủ mục tiêu cuối (`you own the goal, not the steps`), hiểu yêu cầu theo khung **Lauren Tan (`Goal first · Process second · Then routines`)** — hỏi từng câu một cho rõ quyết định ẩn, gom quy trình, đi qua 3 ví dụ thực tế (gồm cả ca lỗi), diễn đạt lại và **tạm dừng (`hold`) chờ người dùng xác nhận** rồi mới giao việc ([BOT-CEO.md §5.0](BOT-CEO.md)). Với mô hình đa mảng, **1 lane = BOT-CEO kiêm luôn Kiến trúc sư của lane đó**; chỉ mở Kiến trúc sư tổng khi thiết kế xương sống liên mảng.
- **Đồng Kiến trúc sư & Reviewer cấp cao (`agy` Opus High):** giữ phiên tương tác trong repo để bàn thiết kế/chia việc thay Claude Code khi cần tiết kiệm hạn mức Claude, và duyệt/phân xử các nhánh khó.
- **Reviewer (mặc định qua OpenCode non-OpenAI hoặc `agy`):** duyệt độc lập theo bằng chứng diff/test (1 tầng cho nhánh thường); ca khó dùng `agy` Opus High / Codex, chỉ leo thang về Claude Code Opus High với ca rủi ro cao (bảo mật/dữ liệu/migration/2 lần `CHƯA ĐẠT`).
- **Codex:** chia phương án thành đầu việc, giao việc một lần cho đủ (mục tiêu, phạm vi được sửa, ràng buộc, tiêu chí đạt, mẫu báo cáo), theo dõi tiến độ, gỡ vướng cho thợ.
- **Gemini Argon:** cố vấn kỹ thuật song song với Codex, đồng thời phụ trách tổng kết vấn đề lớn sau mỗi phiên để cập nhật, tối ưu hóa harnessing cho các phiên kế tiếp.
- **Gemini 4 Hydrogen Medium:** nhận lệnh cập nhật docs khi có thay đổi sau bước Reviewer (trước khi gọi Copilot) và commit docs trên nhánh.
- **GitHub Copilot:** quản lý phiên bản Git; nhận nhánh + SHA + kết quả Audit/Review (+ commit docs nếu có) từ Codex, tích hợp tuần tự và kiểm lại sau merge. Không tự sửa lỗi nghiệp vụ; trả về quản lý khi xung đột.
- **Mỗi tệp/phần chỉ một phiên sửa tại một thời điểm.** Codex chịu trách nhiệm phân chia để các worktree không chồng phạm vi.
- Công cụ không tạo được phiên riêng thì một AI tự đóng lần lượt từng vai, đúng thứ tự.

### 3.2. Giao tiếp bằng `agm` (trên `herdr-core`) và `herdr-mesh`

**Kênh mặc định trong `herdr-core` là `agm send` / `agm inbox`** (từ dự án `agmag`, mục 5–6 của `README_vi.md`). `agm` bọc `hc send` nhưng **ghi lịch sử tin nhắn vào `~/.agmag/agm.db`**, tự nhận diện người gửi qua biến `HC_TERMINAL_ID`, và định tuyến tin gửi `user` vào **hộp thư người dùng** (`agm inbox` / `agm tui`).

1. **Cho agent dùng `agm`:** thêm đoạn sau vào `AGENTS.md` và `CLAUDE.md` của dự án:
   - Gửi tin cho agent khác: `agm send <tên-terminal> --type GIAO|BÁO CÁO|LỖI|HỎI|CHẶN|ĐẠT|CHƯA ĐẠT --task <ID> "<nội dung ngắn>"`
   - Cần người dùng quyết định: `agm send user --type HỎI --task <ID> "<câu hỏi>"` (vào hộp thư người dùng)
   - Xem hộp thư (Codex quản lý / người dùng): `agm inbox --json`
   - Không dùng `agm kill` với phiên Codex/Claude — chỉ người dùng được làm việc đó.
2. **Tên agent / terminal:** chỉ gồm 1–32 ký tự chữ thường, số, `-`, `_`. Quy ước: `arch`, `sparring` (`agy` Opus High), `review`, `codex-mgr`, `argon-adv` (Gemini Argon), `docs-hyd` (Hydrogen Medium), `copilot-git` (quản lý Git), `gitnexus-sync` (cập nhật chỉ mục GitNexus), `w-<feature>-<số>` (thợ), `agm-daemon`. Cả `hc run --name` và `agm spawn` đều áp đúng giới hạn này, nên **mã việc `<ID>` phải viết thường**.
3. **Trước khi gửi / đọc kết quả:** `agm send` gọi `hc send --if idle` nên an toàn khi đích đang làm việc; mỗi lần chỉ gửi một tin ngắn một đến ba dòng, một việc. Khi cần đọc đầu ra màn hình, dùng `hc read <t> --lines N` (chỉ lấy phần mới, không đọc cả pane để tiết kiệm ngữ cảnh).
4. **Thông tin bền vững ghi vào tệp, không chỉ nằm trong tin nhắn.** Tin nhắn chỉ là tín hiệu và con trỏ. Đặc tả, quyết định, kết quả kiểm, tóm tắt bàn giao ghi vào `.agents/tasks/<ID>.md` (hoặc `log/`, `decisions/`, `reviews/`) trong worktree/nhánh và tin nhắn chỉ trỏ tới tệp đó.
5. **Nếu chạy trong HERDR TUI độc lập (không qua `agm`):** có thể dùng `herdr-mesh` (MCP bọc CLI `herdr`: `herdr install`, `claude mcp add -s user herdr-mesh herdr-mesh`, gửi bằng `herdr_relay`; bản `herdr-mesh-safe` có lease cho controller/reviewer/writer). Tuy nhiên **trên `herdr-core` đã có `agm` + `hc`**, không bắt buộc phải cài `herdr-mesh`.

Mẫu tin nhắn bắt buộc (ngắn, một tin một việc — khớp định dạng `agm send`):

```
agm send <đích> --type GIAO|BÁO CÁO|LỖI|HỎI|CHẶN|ĐẠT|CHƯA ĐẠT --task <ID> "<tóm tắt 1-2 câu; xem .agents/tasks/<ID>.md>"
```

Nội dung chuẩn khi hiển thị:

```
[LOẠI] [ID <ID>] từ <người-gửi>: <tóm tắt ngắn> (kèm kết quả kiểm số đếm nếu là BÁO CÁO)
```

Lỗi do Audit/Reviewer (hoặc `agy` Opus High khi phản biện trước) tìm ra được chuyển **nguyên văn** (qua tệp tác vụ/review) cho đúng thợ đã làm, thông qua Codex.

### 3.3. Sổ ghi chép trên nhánh và kiểm thay đổi kiểu git

Nguyên tắc: **git là nguồn sự thật về "đã đổi gì"; tệp `.md` ghi "vì sao" và "đã kiểm gì"**. Cả hai nằm trên cùng nhánh, đi cùng nhau.

**Cấu trúc thư mục (mỗi tệp theo mã việc `<ID>` nên không xung đột khi merge):**

```
.agents/
  tasks/<ID>.md         # đặc tả: mục tiêu, phạm vi sửa, ràng buộc, tiêu chí đạt   (Codex viết)
  log/<ID>.md           # nhật ký làm việc, chỉ thêm, không sửa cũ                 (thợ / Audit / Hydrogen docs viết)
  reviews/<ID>-<n>.md   # kết quả duyệt lần n: ĐẠT/CHƯA ĐẠT, lỗi, vị trí, cách sửa   (Reviewer / sparring viết)
  decisions/<ID>.md     # quyết định thiết kế, rủi ro, tối ưu harnessing           (Kiến trúc sư / Codex / Argon viết)
  reports/<ID>.md       # báo cáo cuối của nhánh (mục 8)                           (Codex viết)
```

*(Ghi chú: `.agents/handoff/` được `agm` tự thêm vào `git info/exclude` khi xoay vòng phiên Claude/Codex, không commit vào nhánh.)*

**Quy tắc ghi:**

1. **Mỗi tệp đúng người ghi** theo vai trò ở trên, đúng nguyên tắc "một phần, một người sửa". Thợ không sửa `tasks/`; muốn đổi đặc tả thì ghi đề xuất vào `log/` và báo Codex.
2. **Nhật ký chỉ thêm.** Sai thì thêm mục đính chính bên dưới, không xóa hay viết lại mục cũ.
3. **Commit sớm và nhỏ.** Mỗi bước có ý nghĩa (xong một phần, trước khi thử hướng rủi ro, trước khi bàn giao, sau khi Hydrogen Medium cập nhật docs) là một commit, **gồm cả code/docs và mục nhật ký tương ứng**. Thông điệp commit bắt đầu bằng `<ID>:`.
4. **Cấp quản lý & cố vấn cũng ghi:** Kiến trúc sư ghi `decisions/`, Codex ghi `tasks/`, `reports/` và các lần đổi model/phiên vào nhật ký điều phối, **Gemini Argon ghi đánh giá & thay đổi tối ưu harnessing** vào `decisions/` (hoặc nhánh tối ưu harness riêng), Reviewer ghi `reviews/`. Tất cả commit trên nhánh mà họ đang đứng.
5. Không ghi bí mật (khóa, mật khẩu, dữ liệu cá nhân) vào các tệp này.

**Mẫu một mục nhật ký:**

```
## 2026-11-19 21:05 · w-login-1 · bước 2/4
- Đã làm: ...
- Tệp đổi: src/a.ts, src/b.ts
- Kiểm: `npm test` → 42 đạt / 0 lỗi
- Quyết định: ... (lý do)
- Vướng/Hỏi: ...
- Commit trước đó: <mã ngắn>
```

**Kiểm thay đổi kiểu git (bắt buộc tại mỗi điểm bàn giao):**

1. Người bàn giao chạy `git status` (không còn tệp chưa commit, kể cả `.md`), `git diff --stat <nhánh-gốc>...HEAD` và `git log --oneline <nhánh-gốc>..HEAD`.
2. Đối chiếu danh sách "Tệp đổi" trong nhật ký với diff thực. Tệp đổi nằm ngoài phạm vi trong `tasks/<ID>.md`, hoặc đổi mà nhật ký không nhắc tới, là lỗi **Lớn** (CHƯA ĐẠT).
3. **Audit** kiểm thêm: nhật ký có mục cho mỗi commit, `.md` đã commit, lệnh kiểm ghi trong nhật ký chạy lại ra cùng kết quả.
4. **Reviewer** đọc `git diff <nhánh-gốc>...HEAD` kèm `tasks/` và `log/`; không đọc lập luận ngoài các tệp này (giữ tính độc lập).
5. **Cập nhật docs (nếu có thay đổi):** sau khi nhánh đã qua Reviewer / đã chỉnh sửa xong, **Gemini 4 Hydrogen Medium** cập nhật tài liệu liên quan trên đúng worktree nhánh đó, ghi một mục vào `log/<ID>.md` và commit để tạo SHA cuối.
6. **Codex** kiểm lại các bước trên, bàn giao nhánh + **SHA cuối** + kết quả Audit/Review (+ xác nhận cập nhật docs) cho **GitHub Copilot**. Copilot xác minh đúng SHA, tích hợp rồi chạy lại cổng kiểm tra. Khi có lỗi, chuyển nguyên văn qua `reviews/` hoặc nhật ký, rồi quay lại thợ.
7. Muốn xem lịch sử một việc, dùng `git log -- .agents/log/<ID>.md` hoặc `git log --oneline <nhánh>`. Muốn hoàn tác trên nhánh chung, giao **GitHub Copilot** thực hiện `git revert` và ghi mục đính chính; thợ chỉ commit trên nhánh riêng.

Khi tích hợp vào nhánh chung, giữ thư mục `.agents/` trong repo theo mặc định; Codex có thể quyết định loại khỏi bản phát hành nếu không muốn đưa ra ngoài.

### 3.3.1. Bàn giao quản lý Git cho GitHub Copilot

- Worker vẫn tạo commit nhỏ trên nhánh/worktree riêng; Hydrogen Medium commit phần cập nhật docs sau bước Reviewer. Codex quản lý việc và chuẩn bị gói bàn giao; Copilot phụ trách tích hợp và quản lý lịch sử trên nhánh chung.
- Gói bàn giao gồm: tên nhánh, SHA đầy đủ (sau commit docs nếu có), phạm vi diff, lệnh kiểm + kết quả thật, báo cáo Review/Audit, phụ thuộc và quyền push/phát hành đã có. Với MATHY-AI, CEO đăng ký vào `BRANCHES.md`; Copilot đọc hàng đợi theo `docs/harness/GIT_BOT.md` (hướng dẫn chung xem [docs/COPILOT-GIT.md](docs/COPILOT-GIT.md)).
- Mở `copilot` tại checkout tích hợp, yêu cầu đọc quy tắc dự án và playbook Git. Chỉ một phiên `copilot-git` xử lý hàng đợi, merge đúng SHA rồi kiểm lại. Xung đột nghiệp vụ hoặc test lỗi thì trả về quản lý, không tự sửa code để vượt cổng.
- Push, tag, phát hành và thao tác hủy lịch sử chỉ thực hiện khi người dùng đã cho phép. Dọn worktree/nhánh phải kiểm tra đã tích hợp, sạch và không còn phiên đang ghi.
- **Đã chạy thử Git** bằng Copilot CLI 1.0.113 trên fixture độc lập: 3/3 ca đạt, gồm dừng khi checkout bẩn, từ chối SHA sai và merge đúng SHA (xem báo cáo `docs/harness/COPILOT_GIT_SMOKE.md` trong repo `MATHY-AI`). Chưa nghiệm thu tích hợp toàn ứng dụng/deploy.
- **Chưa xác minh** Copilot CLI chạy trong HERDR/herdr-mesh trên máy này. Có thể mở terminal riêng và bàn giao bằng tệp; không mặc định dùng chung cấu hình MCP hoặc quyền của OpenCode.

### 3.4. Vòng đời của thợ: ngắn hạn theo đầu việc, ngữ cảnh nằm trong tệp

Vì trạng thái đã nằm trong `.agents/` và git, thợ là **thay thế được**; mặc định sống ngắn.

| Vai | Vòng đời |
|---|---|
| Tìm kiếm, Audit, Cập nhật GitNexus (`gitnexus-sync`), Cập nhật docs (`agy` Hydrogen Medium) | Sinh rồi diệt (hoặc chạy qua `agm ask` một lần cho tra cứu web) theo từng yêu cầu, không giữ trạng thái dài |
| Thợ cơ khí | Sống trong phạm vi một đầu việc; chỉ giữ qua nhiều đầu việc khi cùng feature và cùng vùng mã |
| Reviewer | Phiên mới cho mỗi lần duyệt (giữ độc lập); khi duyệt lại, đưa kết quả duyệt trước bằng tệp |
| Daemon (`agm daemon`) | Sống lâu nhưng không dùng LLM; chỉ lắng nghe `hc events`, đo token, kiểm ngân sách và xoay vòng |
| Kiến trúc sư / BOT-CEO, Đồng Kiến trúc sư (`agy` Opus High), Codex, Gemini Argon (cố vấn & harness) | Sống theo feature/đợt làm việc (với `agy` Opus High giữ `conversation_id` để ăn prompt cache), định kỳ nén hoặc xoay vòng ngữ cảnh, quyết định ghi vào tệp |
| GitHub Copilot quản lý Git | Một phiên riêng cho hàng đợi tích hợp; bàn giao bằng nhánh, SHA và log |

**Xoay vòng & thay phiên (do `agm daemon` theo dõi tự động hoặc gọi `agm rotate <t>`):**

- **Thợ OpenCode (`--role worker`):** khi đạt `ctx_warn` (mặc định 50%), `agm` cảnh báo hộp thư; khi đạt `ctx_rotate` (mặc định 70%) và rảnh, `agm` yêu cầu ghi bàn giao vào `.agents/log/<ID>.md` + commit, kiểm tra worktree sạch và có commit mới chạm tệp log đó, rồi dừng phiên cũ và mở phiên mới cùng template/cwd. Cũng thay phiên khi thợ sửa 2 lần chưa đạt (kèm nâng bậc model).
- **Phiên Claude Code & Codex đã bật `agm manage <t>`** *(lưu ý: `agm manage` chỉ hỗ trợ `claude-code` và `codex`, không áp dụng cho `agy` hay `copilot`)*: khi đạt `ctx_rotate`, `agm` yêu cầu ghi bàn giao vào `.agents/handoff/<id>-<ts>.md` (kết thúc bằng `<!-- agm:handoff-done -->`) rồi mở phiên mới đọc tệp đó; **`agm` không bao giờ tự kill phiên Claude/Codex cũ** (người dùng tự kiểm tra và dọn bằng `agm cleanup`).
- **Diệt/ngắt thủ công:** chỉ người dùng được dừng/ngắt Codex hoặc Claude Code qua `agm kill`/`agm interrupt` (phải xác nhận `y`).

### 3.5. Quy tắc sinh agent phụ

1. **Chỉ Kiến trúc sư và Codex được sinh agent** (qua `agm spawn`). Thợ, Reviewer, Audit, Hydrogen Medium và script nền không tự sinh. Độ sâu tối đa một cấp.
2. **Hai loại:** *subagent / hỏi một lần* (`agm ask` hoặc subagent chỉ đọc trong phiên) và *phiên riêng trong `herdr-core`* (`agm spawn`, có worktree riêng cho việc sửa mã hoặc cập nhật docs/harness).
3. **Giới hạn trao đổi:** thợ chỉ nhắn cho `codex-mgr` (qua `agm send`), không nhắn nhau; việc cần phối hợp do Codex làm trung gian. Mỗi agent phụ nhận một đầu việc, bộ công cụ tối thiểu và mẫu báo cáo định sẵn.
4. **Hạn mức:** số thợ đồng thời tối đa (`[policy].max_workers` trong `providers.toml`) và ngân sách token/hạn mức gói (`agm budget set`) được **`agm` kiểm tra cứng trước khi `agm spawn`**.

### 3.6. Lớp ngữ cảnh mã nguồn: GitNexus (đã chọn)

GitNexus **không phải công cụ quản lý tác vụ**. Nó lập chỉ mục mã nguồn thành đồ thị phụ thuộc, chuỗi gọi hàm và luồng thực thi rồi đưa cho agent qua MCP. Việc quản lý tác vụ vẫn do `.agents/` và git đảm nhiệm.

**Giấy phép:** GitNexus dùng PolyForm Noncommercial. Dự án hiện tại không thương mại nên được dùng. Nếu sau này dự án chuyển sang thương mại, phải xem lại giấy phép hoặc chuyển sang phương án MIT như codebase-memory-mcp.

**1. Agent đọc được**

| Agent | Mức hỗ trợ |
|---|---|
| Claude Code | MCP, skill, hook |
| Codex | MCP, skill, hook (cài hook bằng `gitnexus setup -c codex`) |
| OpenCode | MCP và skill, không hook |

**2. Cài đặt (làm một lần, trên nhánh riêng, do Codex thực hiện)**

1. Cài toàn cục để MCP khởi động nhanh và tránh quá thời gian chờ khi dùng `npx` lần đầu: `npm install -g gitnexus`.
2. Từ gốc repo: `gitnexus analyze --index-only` rồi `gitnexus setup` (tự dò Claude Code, Codex, OpenCode và ghi cấu hình MCP).
3. Với OpenCode, nếu cần cấu hình tay, khai báo trong `~/.config/opencode/config.json` mục `mcp`, máy chủ `gitnexus` kiểu `local`, lệnh `["gitnexus", "mcp"]`, hoặc nối qua một máy chủ HTTP dùng chung chỉ đọc (`GITNEXUS_MCP_READ_ONLY=1 gitnexus mcp --http --port 3111`).
4. **Lưu ý:** chạy `gitnexus analyze` **không** có `--index-only` sẽ tự cài skill, đăng ký hook của Claude Code và ghi đè `AGENTS.md`, `CLAUDE.md`. Vì vậy agent cập nhật chỉ mục **luôn dùng `gitnexus analyze --index-only`** (lần `setup` đầu tiên chạy trên nhánh riêng, xem `git diff`, giữ nguyên quy tắc dự án rồi mới commit).
5. Khi chạy một máy chủ MCP dùng chung cho nhiều client (`gitnexus mcp --http`), luôn bật `GITNEXUS_MCP_READ_ONLY=1` để công cụ `rename` không sửa nhầm lên checkout chính.

**3. Phân công dùng**

| Vai | Công cụ GitNexus nên dùng |
|---|---|
| Kiến trúc sư / BOT-CEO | `impact`, `context`, `generate_map` để đánh giá phương án và rủi ro |
| Reviewer (OpenCode) | `detect_changes`, `impact` qua MCP để kiểm phạm vi ảnh hưởng của diff (do OpenCode không có hook tự nhắc) |
| Thợ cơ khí | `impact` trước khi sửa; `rename` trong worktree khi đổi tên; `trace` khi tìm lỗi |
| Tìm kiếm | `query`, `context` |
| Cập nhật GitNexus (`gitnexus-sync`) | Chạy `gitnexus status`, `gitnexus analyze --index-only`, `gitnexus clean` (không gọi LLM cho phần lập chỉ mục) |
| Audit | Không cần |

**4. Giữ chỉ mục luôn mới qua agent `gitnexus-sync`**

1. OpenCode không có hook nên không tự nhắc làm mới chỉ mục. Thay vì để Copilot tự kiêm thêm, giao cho **agent `gitnexus-sync`** (chạy OpenCode model miễn phí, sinh ngắn hạn qua `agm spawn gitnexus-sync --task <ID> --role index --cwd <checkout>`) chạy **`gitnexus analyze --index-only`** tại checkout chính **ngay sau mỗi lần GitHub Copilot tích hợp vào nhánh chung**, hoặc khi `gitnexus status` / `gitnexus://repo/<tên>/context` báo `staleness.status` là `behind`/`diverged`.
2. Mỗi lần chỉ chạy **một** tiến trình `analyze` (dùng khóa `flock` hoặc thư mục khóa `.agents/locks/gitnexus`) để tránh xung đột kho chỉ mục hoặc ngốn RAM trên repo lớn.
3. Với worktree: mặc định (`GITNEXUS_SYNC=none`/`main`) thợ chỉ tra chỉ mục của checkout chính (gitnexus 1.6.12 **không** có biến `GITNEXUS_SHARED_STORE`; **[KIỂM]** cách nó xử lý linked worktree trước khi dựa vào); nếu dự án cấu hình `GITNEXUS_SYNC=worktree` (chỉ mục riêng theo worktree), `gitnexus-sync` chạy `gitnexus analyze --index-only` khi tách worktree / đổi cấu trúc và `gitnexus clean` khi dọn worktree.
4. Nếu có máy chủ `gitnexus mcp --http` chạy nền, `gitnexus-sync` kiểm tra server còn phản hồi (và chạy `opencode reload` / kiểm `opencode mcp list` nếu cần), rồi báo kết quả (`BÁO CÁO` hoặc `LỖI`) qua `agm send`.

**5. Kiểm tra sau khi cài (làm trước khi giao việc thật):** thử ba việc trên chính repo: tìm nơi gọi một hàm, đánh giá ảnh hưởng của một thay đổi, đổi tên một ký hiệu; thực hiện từ cả Claude Code, Codex và OpenCode. Ghi kết quả vào `.agents/decisions/`.

**Phương án dự phòng:** nếu GitNexus không đạt (lệch chỉ mục giữa worktree, tốn bộ nhớ) thì thử codebase-memory-mcp (MIT) hoặc Serena (công cụ ngữ nghĩa dựa trên LSP).

## 4. HERDR, `hc` và `agm`: người điều khiển + nền tự động

### 4.1. HERDR (APP / TUI) và `agm tui`

- Dùng HERDR để mở các phiên Claude Code, Codex, OpenCode, `agy`, `copilot` cạnh nhau, xem trạng thái và can thiệp.
- Điều khiển từ xa: máy chủ chạy HERDR / `herdr-core`; từ điện thoại dùng **Moshi** (iOS/Android) kết nối vào máy chủ rồi gắn lại (attach) phiên (`hc attach <t>`).
- Mở **`agm tui`** trong một pane của herdr để quan sát tổng thể: hạn mức gói 5h/7d, trạng thái từng terminal + % ngữ cảnh, ngân sách đầu việc và **hộp thư người dùng** (câu hỏi từ agent, lỗi, cảnh báo xoay vòng).
- Phiên của Kiến trúc sư, Reviewer, Codex, Argon nên dễ quan sát ở đây để người dùng đọc và phản hồi được.

### 4.2. Tự động hóa nền với `herdr-core` (`hc`) và `agm`

- **`agm daemon`** chạy nền trong `herdr-core` (`hc run --bg --no-parent --name agm-daemon agm daemon`) để lắng nghe `hc events --json`, quét token/hạn mức, kiểm ngân sách, mở hộp thư khi agent bị `blocked` và tự động xoay vòng phiên khi đầy ngữ cảnh.
- **Lớp wrapper công việc (worktree + inbox):** khi có đầu việc mới trong `.agents/inbox/<ID>.md`, script `dispatch.sh` (xem [HERDR-CORE-BOT.md](HERDR-CORE-BOT.md)) thực hiện:
  1. Tạo git worktree + nhánh mới: `git worktree add .wt/<ID> -b feat/<ID>` (hoặc đường dẫn quy ước của dự án như `../MATHY-AI.wt/<slug>`).
  2. Khởi động phiên thợ bằng **`agm spawn <template> --task <ID> --role worker --cwd <worktree> -- "..."`** (tự gắn `hc run --bg --outlive-parent`, kiểm `max_workers` và ngân sách cứng, đặt tên `w-<ID>-<số>`).
  3. Khi thợ xong hoặc bị chặn, báo cho quản lý bằng **`agm send`** (hoặc `agm send user --type HỎI ...` nếu cần người dùng quyết); **không tự merge**.
- Giới hạn an toàn bắt buộc: `max_workers` và ngưỡng ngữ cảnh trong `~/.agmag/providers.toml`, ngân sách token qua `agm budget set`, quyền `edit`/`bash` của OpenCode đặt rõ, nút dừng khẩn **`agm policy off`**, và **chỉ người dùng mới được `agm kill` phiên Codex/Claude**.
- GitHub Copilot phụ trách dọn worktree/nhánh đã tích hợp và sạch; `agm cleanup` chỉ dọn các terminal cũ đã bàn giao trong `agm`/`hc` khi người dùng xác nhận.

### 4.2.1. `hc` trên máy thật (đã kiểm 2026-11-17, bổ sung 2026-11-18, hc 0.1.21, Windows 11)

CLI của herdr-core là `hc`. Thử nhanh bằng ba lệnh:

```powershell
hc run --bg --name demo ping -n 600 127.0.0.1   # chạy nền, trả JSON có "address":"t10", "state":"live"
hc ls                                           # liệt kê terminal đang sống
hc read demo --lines 10                         # đọc 10 dòng cuối, không cần attach
```

Những gì đã thấy trên máy thật (và xác minh bổ sung trong `.agents/decisions/p0-research.md` của `agmag` ngày 2026-11-18):

| Điểm | Kết quả thật | Hệ quả |
|---|---|---|
| Core | Đã chạy sẵn; `hc server start\|stop\|status\|restart` tồn tại. `hc server status` trả JSON có `live`, `finished`, `uptime_ms`, `version` | Dịch vụ khởi động cùng máy có thể dùng `hc server start`; còn phải thử sau khi khởi động lại máy |
| Đầu ra | Mặc định là JSON khi đầu ra đi qua đường ống; `run` và `read` in văn bản | `agm` đọc JSON trực tiếp từ `hc <lệnh> --json` |
| Dạng `hc ls` | Một **đối tượng** `{"hidden_finished": n, "terminals": [...]}`, **không phải mảng** | `agm` xử lý trực tiếp `.terminals`; script shell dùng `jq '.terminals \| length'` |
| Quan hệ cha–con | `hc run` gọi từ trong một terminal sẽ tự gán `parent` là terminal đó; `die_with_parent: true` theo mặc định | `agm spawn` luôn truyền `--outlive-parent` để thợ sống sót |
| Biến môi trường terminal | `hc` tự đặt `HC_TERMINAL_ID`, `HC_TERMINAL_SOCKET`, `HC_SCOPE` trong mỗi terminal | `agm send` tự nhận diện người gửi qua `HC_TERMINAL_ID`, và `agm` kiểm tra người gọi fail-closed |
| Báo trạng thái (OSC 7501) | Đã thấy các app tự báo 7501: `claude-code`, `codex`, `antigravity` (và `opencode` qua `hc ls`). Khi bị chặn có `state:"blocked"`, `kind:"question"\|"permission"`, `msg` | `agm daemon` bắt trực tiếp `status` `blocked` để đẩy nguyên văn câu hỏi vào hộp thư `agm inbox` |
| Tên terminal | `--name` chỉ nhận 1–32 ký tự `a-z 0-9 - _`, không chữ hoa | Mã việc phải viết thường (`test-1`, không phải `TEST-1`); cả `agm spawn` và `dispatch.sh` kiểm trước khi chạy |
| Vị trí & HERDR app | `labels` có `where` (vd `"MATHY-AI / Codex"`) và `herdr.space`/`herdr.tab` | **Terminal mở trong HERDR app hiện cả trong `hc` / `herdr-core`** |
| Sự kiện | `hc events --json`: dòng đầu là ảnh chụp `{"channel", "seq", "terminals"}`; các dòng sau dạng JSON-RPC (`created`, `finished`, `status`, `turn`, `title`, `labels`, `size`, `foreground`, `doc`) + `heartbeat` mỗi 15s | `agm daemon` tiêu thụ toàn bộ luồng sự kiện này |
| Template | `hc templates` ban đầu trả `{"templates": []}` | Phải tự tạo `worker-fast`, `worker-mid`, `worker-strong`, `auditor`, `researcher`, `docs-hydrogen`, `sparring-opus`, `argon-advisor` theo `HERDR-CORE-BOT.md` §1.4 |
| Dọn dẹp | `hc kill <t>`, `hc rm <t>`, `hc ls --all`, `hc run --rm` | Dùng `--rm` cho lệnh ngắn; không dùng cho thợ vì cần giữ để đọc log/ghép phiên |
| Báo khi con kết thúc | Khi terminal con thoát, `hc` tự gõ ghi chú `<hc>t10 demo exited...</hc>` vào agent cha (tắt bằng `--no-notify` hoặc `notes = false` trong `config.toml`) | Hữu ích khi cần biết nhanh con đã thoát; tắt nếu thấy rối prompt của agent cha |

Các lệnh khác của `hc`: `hc wait <t>` (dùng thay `sleep`), `hc interrupt <t>`, `hc keys <t> enter|esc|up|ctrl+c`, `hc put <t> <file>`, `hc exec`, `hc file read|write|edit`, `hc keep <t>`, `hc label <t> k=v`.

### 4.3. Phạm vi giao tiếp giữa HERDR app, TUI và `herdr-core`

- **Cập nhật đã xác minh (2026-11-18):** khi bắt sự kiện `hc events --json` trên Windows 11, các terminal mở trong **HERDR app** đều có mặt trong `hc` (với nhãn `herdr.space` / `herdr.tab` và `where`), nên **`agm` thấy, đo và giao tiếp (`agm send`) được với chúng**.
- Nếu người dùng chạy **HERDR TUI** ở phiên bản hoặc máy khác mà `hc ls` không thấy terminal của nhau, vẫn dùng phương án dự phòng: bàn giao qua tệp (`.agents/inbox/`, `.agents/tasks/`, `.agents/log/`) và git, hoặc chạy cả đội trong cùng một `herdr-core`.

## 5. Quy tắc giao việc

1. Tìm kiếm & tóm tắt log dài → agent Tìm kiếm (`agy` qua `agm ask` / OpenCode `explore` dùng model miễn phí). Kiểm tra & sàng lọc quy ước → Audit (`:free` / local). Sửa/soạn code → Thợ cơ khí (ưu tiên `worker-fast` miễn phí cho việc rõ, hẹp).
2. **Điều phối theo `agm quota`:** trước khi mở phiên nặng, xem `agm quota` trong `agm tui`. OpenCode dùng `openai/gpt-6.1-sol` **tiêu chung hạn mức ChatGPT 5h/7d với Codex CLI** — khi hạn mức tuần của `codex` chỉ **còn < ~20–30%** (`agm quota` hiển thị phần còn lại), chuyển `worker-strong` & `Reviewer` sang **OpenRouter / Ollama Cloud / `agy`**.
3. **Claude Code chỉ mở cho Kiến trúc sư / BOT-CEO** và khi leo thang ca rủi ro cao: đổi cấu trúc lớn; việc chạm bảo mật/dữ liệu/migration DB; lỗi sửa 2 lần chưa xong (kể cả sau khi đã nâng bậc model); trước phát hành lớn.
4. Dùng **Opus High qua `agy`** (phiên tương tác trong repo / `--conversation <id>`, **không** gọi `agm ask` one-shot) để làm **Đồng Kiến trúc sư** (bàn thiết kế, chia `.agents/tasks/<ID>.md` khi muốn tiết kiệm hạn mức Claude Code) hoặc làm **Reviewer cấp cao** phân xử ca khó (bỏ bước Pre-review trên các nhánh thường).
5. Gọi **Codex (`gpt-6.1-sol`)** cho: **engineering khó / refactor đa tệp / fix lỗi sau 2 lần fail**, review chéo ca logic phức tạp, và làm **quản lý điều phối ở dự án 1 luồng** (khi đã bật `BOT-CEO.md`, BOT-CEO gọi thẳng thợ qua `agm spawn`, bỏ tầng `codex-mgr` trung gian thường trực).
6. Gọi **Gemini Argon (`agy`)** khi: cần cố vấn kỹ thuật độc lập như Codex, và **luôn giao Argon rà soát các vấn đề lớn sau phiên/giai đoạn để cập nhật và tối ưu hóa harnessing** (bao gồm tinh chỉnh bảng phân loại `tier:` cho model miễn phí) cho các phiên sau.
7. Gọi **Gemini 4 Hydrogen Medium (`agy`)** khi có thay đổi cần cập nhật docs (và tổng hợp báo cáo cuối `.agents/reports/<ID>.md`), thực hiện **sau khi Reviewer đã duyệt/chỉnh sửa xong và trước khi gọi GitHub Copilot**.
8. Gọi **GitHub Copilot** khi: quản lý nhánh/lịch sử, tích hợp đúng SHA, revert/rollback và dọn nhánh đã tích hợp.
9. Gọi **Reviewer mặc định (OpenCode non-OpenAI hoặc `agy` Argon/Hydrogen High)** sau khi Audit & sàng lọc quy ước đạt, duyệt 1 tầng cho từng nhánh thường; ca khó dùng **`agy` Opus High** (hoặc Codex), và chỉ kéo **Claude Code Opus 5.5 High** vào duyệt lại với ca bảo mật/dữ liệu hoặc bị `CHƯA ĐẠT` 2 lần liên tiếp.
10. Sau chẩn đoán của Kiến trúc sư / BOT-CEO vẫn chưa xong thì dừng và hỏi người dùng (`agm send user --type HỎI ...`).

## 6. Cổng phát hành (làm đúng thứ tự)

**Lưu bản / commit trên nhánh worktree** → **Audit & sàng lọc quy ước đạt hết (model miễn phí)** → **Reviewer mặc định đạt / hoàn tất chỉnh sửa (OpenCode non-OpenAI / `agy`; ca khó dùng `agy` Opus High; ca bảo mật/dữ liệu/2 lần CHƯA ĐẠT: leo thang Claude Opus High)** → **Cập nhật docs & tổng hợp báo cáo nếu cần (Gemini 4 Hydrogen Medium nhận lệnh, sửa & commit trên nhánh; chạy lại kiểm nhanh nếu chạm phạm vi kiểm)** → **GitHub Copilot tích hợp đúng SHA cuối vào nhánh chung, chạy lại Audit** → **Cập nhật GitNexus (agent `gitnexus-sync` chạy `analyze --index-only`)** → **Xem thử (nếu có giao diện)** → **Kiến trúc sư / BOT-CEO SẴN SÀNG** → **Phát hành (người dùng phê duyệt)** → **Xác minh môi trường thật** → *(Sau phiên/đợt: Gemini Argon tổng kết vấn đề lớn & tối ưu harnessing cho phiên sau)*

- Có lỗi ở bước nào thì sửa, lưu bản mới, chạy lại từ Audit.
- Kiểm không chạy được thì ghi rõ lý do, không báo là đạt.
- Trước mỗi cổng, kiểm thay đổi kiểu git theo mục 3.3 (không còn tệp chưa commit, nhật ký khớp diff, không sửa ngoài phạm vi).
- Bot/script nền và thợ không được phát hành; chỉ GitHub Copilot tích hợp, chỉ người dùng phê duyệt phát hành. Với MATHY-AI, Copilot triển khai theo phạm vi được cho phép và các cổng trong `GIT_BOT.md`.

## 7. Việc không phải code

Giữ nguyên đội hình và cổng. Kiến trúc sư lập dàn ý; (có thể phản biện trước qua `agy` Opus High); Codex hoặc BOT-CEO chia phần; mỗi thợ soạn một phần riêng (một worktree/tệp riêng); Tìm kiếm lấy nguồn, số liệu; Audit kiểm số liệu, mẫu, chính tả; Reviewer duyệt nội dung; Hydrogen Medium cập nhật tài liệu phụ trợ/chỉ mục liên quan trước khi Copilot chốt bản. "Commit" là lưu bản có số phiên bản; "phát hành" là gửi bản cuối. Mẫu người dùng đưa luôn giữ nguyên bố cục.

## 8. Báo cáo cuối

Do **Gemini 4 Hydrogen Medium** (lúc cập nhật docs cuối nhánh) hoặc **model rẻ (`:free` / `gemini-3.8-flash`)** tổng hợp từ `.agents/log/`, `reviews/` và `git diff --stat` rồi lưu vào `.agents/reports/<ID>.md` trên nhánh (BOT-CEO/Codex chỉ duyệt lại 1 lần), ngắn gọn: đã đổi gì, ai làm (phiên nào, model nào), kết quả kiểm (số đếm), kết luận Reviewer/Kiến trúc sư, docs đã cập nhật gì, phần chưa xác minh, việc còn lại, và **danh sách vấn đề lớn cần chuyển cho Gemini Argon tối ưu harnessing ở phiên sau**.

> **Cách dùng:** đặt toàn bộ nội dung này (hoặc bản tóm lược kèm quy tắc `agm` ở mục 3.2) vào `AGENTS.md` (Codex, OpenCode) và `CLAUDE.md` (Claude Code) của dự án; mỗi phiên bot khi khởi động cũng đọc tệp này. Khi giao việc, ghi "Việc cần làm: ...".

## Phần chưa xác minh (cần thử trên máy thực tế)

1. Lệnh dịch vụ khởi động cùng máy (`Task Scheduler` trên Windows hoặc `systemd` trên Linux) có tự khởi động `hc server start`, `agm daemon` và khôi phục ổn định sau khi reboot không.
2. ~~Tên model slug của **Opus High trên `agy`**~~ — đã kiểm 2026-11-20: `claude-opus-5-5-high`.
3. `agy` chạy tương tác cho **Hydrogen Medium** (sửa docs) và **Gemini Argon** (sửa harness) trong `hc`: độ ổn định của `hc send` vào phiên `agy` đang mở, và cấu hình quyền ghi thư mục worktree cho `agy`.
4. `herdr-mesh` (chỉ cần nếu dùng HERDR TUI tách biệt với `herdr-core`; với đội chạy trong `herdr-core`, `agm` đã thay thế phần nhắn tin, theo dõi và điều phối).
5. Nguồn model OpenCode: mã provider thực tế của Ollama Cloud (`opencode models`), model nào gọi công cụ ổn định trong worktree, giới hạn đồng thời và giá hiện hành của Ollama Cloud.
6. GitNexus: chỉ mục đồng bộ giữa nhiều worktree, mức độ `analyze` ghi đè `AGENTS.md`/`CLAUDE.md`, thao tác làm mới thủ công cho OpenCode.
7. Các ngưỡng kinh nghiệm ở mục 1.1 (1–3 skill, thợ tối đa 2) và mục 3.4 / `providers.toml` (`ctx_warn = 50`, `ctx_rotate = 70`) — cần hiệu chỉnh theo số liệu thực tế từ `agm cost` và `agm ls`.
8. Thư mục agent của OpenCode: `.opencode/agents/` hay `.opencode/agent/` (như repo MATHY-AI và `agmag`) theo bản OpenCode đang cài.

## Nguồn tham khảo

- `agmag` (`agm` v0.2.0): https://github.com/wosonday/agmag/ (`README_vi.md`, `src/main.rs`, `src/actions.rs`, `src/policy.rs`, `src/rotation.rs`, `src/ask.rs`, `.agents/decisions/p0-research.md`, `tools/argon_review.py`)
- GitHub Copilot CLI (khởi động và prompt `-p`): https://docs.github.com/en/copilot/get-started/cli-quickstart ; https://docs.github.com/en/copilot/how-tos/copilot-cli/automate-copilot-cli/run-cli-programmatically
- herdr-core (`hc`): `hc --help`, `hc <lệnh> --help`, `hc --skill`; https://github.com/herdrdev/herdr-core
- HERDR (kiến trúc TUI/core/app, attach từ xa): https://herdr.dev/app ; https://github.com/yigitkonur/awesome-herdr/blob/main/README.md
- herdr-mesh & herdr-mesh-safe (nếu dùng với TUI độc lập): https://github.com/runchr-works/herdr-mesh ; https://glama.ai/mcp/servers/pfoe5pzsjj
- GitNexus: https://github.com/abhigyanpatwari/GitNexus ; https://unpkg.com/gitnexus@latest/README.md
- Nguồn model OpenCode (Ollama, Ollama Cloud, OpenRouter): https://opencode.ai/docs/agents/ ; https://opencode.ai/docs/config.md ; https://mintlify.com/ollama/ollama/integrations/opencode ; https://openrouter.ai/docs/cookbook/coding-agents/opencode-integration
- Antigravity CLI (`agy`): `agy --help`, `agy models` (agy 1.5.10, 2026-11-18)
