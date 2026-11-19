# OpenCode v2 Guide — Workers, Audit, `gitnexus-sync` & Default Reviewer

OpenCode v2 (`@opencode/cli`) is the primary execution engine for worktree-isolated automated roles: **Audit (`auditor`)**, **Repo Search (`explore`)**, **GitNexus Sync (`gitnexus-sync`)**, **Engineering Workers (`worker-fast/mid/strong`)**, and **Default Reviewer (`reviewer`)**.

## 1. Installation & Windows 11 Gotchas (verified on v2.3.2)

1. Install v2 via:
   ```powershell
   npm install -g @opencode/cli
   opencode --version                 # must be 2.x (npm package "opencode-ai" is legacy v1)
   ```
   Verify in **both PowerShell and Git Bash** so you don't accidentally invoke a legacy v1 binary in `~/.opencode/bin`.
2. Enable auto-update: set `"update": "auto"` in `~/.config/opencode/opencode.json`.
3. **Fully quit OpenCode before upgrading:** the background `opencode serve --service` locks `.exe`, causing `npm i -g` / `opencode upgrade` to hang or delete the `opencode` command.
4. **v2 stores credentials in SQLite (`~/.local/share/opencode/opencode.db`), not `auth.json`:** check authenticated providers with `opencode auth list`. `agm` opens `opencode.db` in `READ_ONLY` mode and reads only session/message tables—never `credential`/`account`.

---

## 2. Model Tiering in OpenCode (Lightweight/Free First, Avoid OpenAI Unless Needed)

| Role / Template | Recommended Model Source | Purpose & Limits |
|---|---|---|
| **`auditor.md`** (Audit & ESLint/CI gate) | **Lightweight / Free:** `opencode/deepseek-v4.1-flash-free`, `openrouter/thinkingmachines/inkling-2-small:free`, or **local Ollama** (`num_ctx ≥ 64k`) | Runs `eslint --max-warnings 0`, scans diff to block `eslint-disable`/`@ts-ignore`/lint config edits ([ESLINT-CI-GUARDRAILS.md](ESLINT-CI-GUARDRAILS.md)), runs tests/build, verifies `git diff` matches `.agents/log/<ID>.md`. Never edits code |
| **`explore`** (Repo search & long-log summary) | **Lightweight / Free:** `:free` or local Ollama | Scans files / condenses thousand-line test logs before passing to BOT-CEO / Claude |
| **`gitnexus-sync.md`** (GitNexus index updater) | **Lightweight / Free:** `:free` or local Ollama | Runs `gitnexus status`, `gitnexus analyze --index-only`, checks MCP HTTP server. Never edits repo files |
| **`worker-fast.md`** (Fast worker — **default**) | **Lightweight / Free / Cheap:** `opencode/deepseek-v4.1-flash-free`, OpenRouter `:free` (2–3 fallback slugs), local Ollama 7B–14B, or `gpt-6.1-terra-fast` | Backed by ESLint + typecheck + tests in worktree, handles **all well-specified tasks ≤ 2–3 files by default** (CRUD, UI components, API handlers, unit tests, fixtures, local refactors). Escalates to `worker-mid` only after failing lint/tests twice |
| **`worker-mid.md`** (Mid worker) | **Paid mid-tier (non-OpenAI):** OpenRouter / Ollama Cloud | Tasks spanning 3–5 files, multi-branch business logic, or escalated after 2 `worker-fast` failures |
| **`worker-strong.md`** (Strong worker) | **Strong OpenRouter model** or `openai/gpt-6.1-sol` *(only when `agm quota` for `codex` < 70–80%)* | Complex multi-file logic, rescues tasks failing `worker-mid` |
| **`reviewer.md`** (Default Reviewer) | **Strong non-OpenAI model** (OpenRouter / Ollama Cloud) | Independent review without consuming ChatGPT/Codex or Claude quotas; proactively calls GitNexus MCP (`detect_changes`, `impact`) |

### Tips for Running Lightweight / Free Models Reliably
- **Lock down with ESLint & CI guardrails ([ESLINT-CI-GUARDRAILS.md](ESLINT-CI-GUARDRAILS.md)):** every `worker-fast` must pass `npx eslint . --max-warnings 0 --report-unused-disable-directives` (or `ruff` / `clippy -D warnings`) with `0 errors, 0 warnings` before committing, and is strictly forbidden from inserting `eslint-disable` or `@ts-ignore`. The linter acts as a zero-cost supervisor so lightweight models self-correct in the worktree.
- **Tool-calling smoke test first:** before adding a new `:free` or local Ollama model to `worker-fast.md`, run one sample task (edit 1 file → run lint + tests → log to `.agents/log/` → commit). Reject models with unstable tool calls.
- **Local Ollama:** always set `num_ctx ≥ 64k`, otherwise OpenCode tool calls frequently fail.
- **`429` fallbacks:** OpenRouter `:free` endpoints enforce strict per-minute rate limits; keep 2–3 fallback slugs in `opencode.json`.
- **Measure real savings:** run `agm cost --by source` to track token share shifted to `opencode` / `:free` vs. `claude` and `codex`.

---

## 3. Permissions & Lifecycle in `.opencode/agent/` (or `.opencode/agents/`)

*(Verify whether your OpenCode build reads singular or plural; `MATHY-AI` and `agmag` use `.opencode/agent/`.)*

- **Strict per-role permissions:**
  - `worker-*`: `edit: allow` (scoped by `.agents/tasks/<ID>.md`), `bash` denies `git push`, `git merge`, `git rebase`, `git reset --hard`, `git worktree`, `rm -rf`, and reading `*.env*` / `*opencode.db*`.
  - `auditor` / `reviewer` / `gitnexus-sync`: forbidden from editing product source code.
- **Automatic rotation via `agm daemon`:**
  - When an OpenCode worker (`agm spawn ... --role worker`) hits **`ctx_rotate` (default 70%)**, `agm` prompts it to write a handoff entry to `.agents/log/<ID>.md` + commit, verifies a clean worktree, then kills the old session and spawns a fresh successor in the same worktree.
- **Concurrency:** hard-capped by `[policy].max_workers` in `~/.agmag/providers.toml` (keep within your Ollama Cloud plan's concurrent limit if applicable).
