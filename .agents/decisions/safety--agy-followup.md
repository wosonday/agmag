# Agy consultation — safety follow-up

Date: 2026-11-19. Conversation: `ac968e24-dac9-4279-bba1-283cad292e95`.

A one-shot `agy --mode plan --sandbox` consultation ran outside the repository with bounded source excerpts in its prompt. It returned `SUCCESS`; it did not edit project files. This is discussion input, not independent Opus approval.

## Input
Three demonstrated failures: stale successor liveness, ignored failed old-identity lookup during cleanup, and cached `done` authorizing handoff after a failed fresh lookup.

## Agy recommendations
- Require explicit fresh live state for handoff; do not fall back to cached status.
- Require matching known app and creation identity for both old and successor terminals.
- Check the completed rotation's frozen old/new relationship; reject old == successor.
- Recheck successor, then old, immediately before killing; recognize get/kill TOCTOU cannot be fully eliminated without an identity-conditional hc API.
- Ensure `--yes` cannot skip protected-session typed confirmation, including cached-app mismatches.
- Restrict database-only removal to explicit finished state, close associated inbox items, retain history.
- Add negative coverage for stale state, errors, missing fields and identity mismatches.

## Engineering decisions
- Keep `Option<String>` for `fresh_idle_status`: its only caller already returns without sending on `None`, so a new enum would not change safety. Require fresh live/address/app evidence and never fall back.
- Make CLI cleanup require one typed confirmation for every batch. Retain `--yes` as a compatibility flag but explicitly document that it cannot bypass cleanup confirmation. This avoids a cached-app permission decision and cancels the entire batch if the user declines.
- Preserve the recorded successor address in cleanup candidates even when its terminal row is missing; otherwise exited-only cleanup cannot revalidate its completed rotation.
- No migration, no TUI changes, no direct actions on real agents. hc operations in tests are mocks.
