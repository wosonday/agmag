# Handoff → copilot-git: `fix/sysmon-disk-identity` (task `sysmon-disk-identity`)

- **Branch:** `fix/sysmon-disk-identity` · worktree `D:/Project/agmag.wt/sysmon-disk-identity`
- **Base:** contains `main` @ `5c53a68`; fast-forward possible while `main` is unchanged.
- **Approved head:** the commit that adds this file (`git log -1 fix/sysmon-disk-identity`); reject if more commits appear.
- **depends_on:** none.

## What it changes
- `merge_disks` folds two mounts only when they are the same mount or report the same real volume id (Windows `GetVolumeNameForVolumeMountPointW` GUID path; Unix `st_dev`); no id → never folded. Drops the byte-count heuristic, which merged distinct disks (review `sysmon` 2.1) and merged/split three separate Google Drive accounts (G:, H:, I:) inconsistently.
- `Cargo.toml`: adds feature `Win32_Storage_FileSystem` to the existing `windows-sys`; `Cargo.lock` unchanged. `volume_id` is not serialized; volume labels are never read.

## Evidence
| Gate | Result |
|---|---|
| `cargo test --offline` | 309 passed / 0 failed (worker, CEO and reviewer runs) |
| `cargo clippy --offline --all-targets -- -D warnings` | clean |
| `cargo fmt --check` | clean |
| Live | `agm sys --json`: 5 separate disks `C:\ D:\ G:\ H:\ I:\`, all `also_mounted_at: []`, no label/GUID fields |
| Review | `.agents/reviews/sysmon-disk-identity-review.md` (opencode/hy4-flash-free; worker opencode/iceman-free): **ĐẠT**, 0/0/4 minor (hardening suggestions) |

## Post-merge
Re-run the three cargo gates on `main`. After merge the worktree and branch may be removed (`git worktree remove`, `git branch -d`) once `agm ps` shows no live session there.
