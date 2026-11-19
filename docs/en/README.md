# Model-Specific Usage Guides

[Tiếng Việt](../README.md) · **English**

This directory specifies startup commands, permission boundaries, `agm`/`hc` usage, and quota-saving practices for each model/CLI in [AGENTS-TUTORIAL.md](../../AGENTS-TUTORIAL.md), [BOT-CEO.md](../../BOT-CEO.md), and [HERDR-CORE-BOT.md](../../HERDR-CORE-BOT.md).

## Index

| Document | Model / Tool | Primary Role |
|---|---|---|
| [CLAUDE-ARCHITECT-CEO.md](CLAUDE-ARCHITECT-CEO.md) | Claude Code (`claude-opus-5-5`, `claude-fable-5-2`) | Architect / per-lane BOT-CEO, high-risk escalations |
| [CODEX-ENGINEERING.md](CODEX-ENGINEERING.md) | Codex CLI / `openai/gpt-6.1-sol` | Hard engineering, fixes after 2 failed attempts, complex cross-reviews, single-lane management |
| [AGY-GEMINI-OPUS.md](AGY-GEMINI-OPUS.md) | Antigravity CLI `agy` (3.8 Flash, Hydrogen Low/Medium, Argon, Opus High) | Fast research (`3.8-flash-low`), lightweight docs/reports (`3.8-flash-medium` / `hydrogen-low`), technical advisor & harness optimization (`argon`), Co-Architect & Senior Reviewer (`Opus High`) |
| [OPENCODE-WORKERS-REVIEWER.md](OPENCODE-WORKERS-REVIEWER.md) | OpenCode v2 (`@opencode/cli`: `:free`, Ollama, OpenRouter, Ollama Cloud) | Audit, `explore`/log summarization, `gitnexus-sync`, workers (`fast` expanded via ESLint + `mid/strong`), default Reviewer (non-OpenAI) |
| [COPILOT-GIT.md](COPILOT-GIT.md) | GitHub Copilot CLI (`copilot`) | Git version manager: reads `BRANCHES.md`, merges exact SHA, post-merge verification, reverts, cleans up worktrees/branches |
| [ESLINT-CI-GUARDRAILS.md](ESLINT-CI-GUARDRAILS.md) | ESLint (`--max-warnings 0`) + Git Diff Check + GitHub Actions CI | **Mandatory guardrails for all models:** blocks `any`, `@ts-ignore`, `eslint-disable`, bloated functions, missing `await`; enforced at Audit/CI so lightweight models can be used safely |

## Quick Quota Balancing via `agm`

Before dispatching a heavy batch, check current usage in `agm tui` or CLI:

```powershell
agm quota                     # check 5h / 7d quota % LEFT for Claude and Codex
agm cost --by source          # check token share across claude / codex / opencode / agy
```

| Situation | Load-balancing action |
|---|---|
| `codex` 7d < 20–30% left | Reserve `openai/gpt-6.1-sol` for genuinely hard engineering; route default Reviewer to OpenRouter/Ollama Cloud or `agy` (`Argon/Hydrogen High`), and `worker-strong` to OpenRouter |
| `claude` 5h/7d dropping fast | Open an interactive `agy` Opus High session inside the repo (`--mode plan --sandbox`, keeping `conversation_id` for prompt cache) to offload design/task splitting and hard-case review; never open Claude for routine reviews |
| `:free` endpoint hits `429` | Switch to a fallback `:free` slug or local Ollama (`num_ctx ≥ 64k`) |
