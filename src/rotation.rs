//! Xoay vòng thợ OpenCode theo mức đầy ngữ cảnh (spec §5.9, người dùng chốt 2026-11-18).
//! pending → (agent rảnh) requested → (lượt mới + worktree sạch + commit chạm log) done | failed.
//! Chỉ tự động cho thợ (agm.role=worker) chạy OpenCode. Codex/Claude không bao giờ bị xoay tự động.

use anyhow::Result;
use rusqlite::params;

use crate::actions::{self, Caller, SpawnReq};
use crate::config::Config;
use crate::git::Git;
use crate::hc::Hc;
use crate::policy::{enabled, session_context_pct, POLICY_ACTOR};
use crate::store::{Store, TerminalRow};

struct Rotation {
    id: i64,
    task: Option<String>,
    old: String,
    state: String,
    head_before: Option<String>,
    turn_before: i64,
    requested_ms: Option<i64>,
}

pub fn handoff_text(task: &str) -> String {
    format!(
        "[GIAO] [ID {task}] từ agmag: ngữ cảnh của bạn đã đầy. Hãy dừng việc đang làm, ghi mục bàn giao vào .agents/log/{task}.md \
         (đã làm gì, còn gì, lỗi nguyên văn, quyết định đã chốt), commit tất cả (thông điệp bắt đầu bằng '{task}:'), rồi dừng. \
         Một phiên mới sẽ tiếp tục từ tệp đó."
    )
}

pub fn resume_text(task: &str) -> String {
    format!("Đọc .agents/tasks/{task}.md, .agents/log/{task}.md và git log của nhánh, rồi làm tiếp đầu việc {task}.")
}

fn rotatable(t: &TerminalRow) -> bool {
    t.role.as_deref() == Some("worker") && t.app.as_deref() == Some("opencode") && t.task.is_some()
}

fn idle(t: &TerminalRow) -> bool {
    matches!(t.status.as_deref(), Some("idle" | "done"))
}

fn active(store: &Store) -> Result<Vec<Rotation>> {
    let mut stmt = store.conn().prepare(
        "SELECT id, task, old_terminal, state, head_before, COALESCE(turn_before, 0), requested_ms FROM rotations
         WHERE state IN ('pending', 'requested') ORDER BY id",
    )?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Rotation {
                id: r.get(0)?,
                task: r.get(1)?,
                old: r.get(2)?,
                state: r.get(3)?,
                head_before: r.get(4)?,
                turn_before: r.get(5)?,
                requested_ms: r.get(6)?,
            })
        })?
        .collect::<rusqlite::Result<_>>()?;
    Ok(rows)
}

fn set_state(store: &Store, id: i64, state: &str, detail: Option<&str>, now: i64) -> Result<()> {
    store.conn().execute(
        "UPDATE rotations SET state = ?2, detail = COALESCE(?3, detail), updated_ms = ?4 WHERE id = ?1",
        params![id, state, detail, now],
    )?;
    Ok(())
}

fn fail(store: &Store, r: &Rotation, reason: &str, now: i64) -> Result<()> {
    fail_with(store, r, reason, "Phiên cũ được giữ nguyên.", now)
}

fn fail_with(store: &Store, r: &Rotation, reason: &str, after: &str, now: i64) -> Result<()> {
    set_state(store, r.id, "failed", Some(reason), now)?;
    let text = format!("Xoay vòng {} thất bại: {reason}. {after} Dùng `agm rotate {}` để thử lại.", r.old, r.old);
    store.open_inbox(Some(&r.old), "rotation", Some("failed"), &text, now)?;
    store.log_action(POLICY_ACTOR, "rotate", &r.old, reason, false, now)?;
    Ok(())
}

/// Bắt đầu xoay vòng một terminal. Bỏ qua nếu đã có vòng đang chạy; vòng **tự động** cũng bỏ qua
/// terminal đã có vòng thất bại (tránh nhắc bàn giao mãi) — chỉ `agm rotate` (manual) mới thử lại.
pub fn start(store: &Store, addr: &str, ctx_pct: Option<f64>, manual: bool, now: i64) -> Result<bool> {
    let states = if manual { "'pending', 'requested'" } else { "'pending', 'requested', 'failed'" };
    let busy: bool = store.conn().query_row(
        &format!("SELECT EXISTS(SELECT 1 FROM rotations WHERE old_terminal = ?1 AND state IN ({states}))"),
        [addr],
        |r| r.get(0),
    )?;
    if busy {
        return Ok(false);
    }
    let task = store.terminal(addr)?.and_then(|t| t.task);
    store.conn().execute(
        "INSERT INTO rotations (task, old_terminal, state, ctx_pct, updated_ms) VALUES (?1, ?2, 'pending', ?3, ?4)",
        params![task, addr, ctx_pct, now],
    )?;
    Ok(true)
}

pub fn tick(store: &Store, cfg: &Config, hc: &dyn Hc, git: &dyn Git, now: i64) -> Result<()> {
    if !enabled(store)? {
        return Ok(());
    }
    // 1. Phát hiện thợ OpenCode vượt ngưỡng xoay vòng.
    for t in store.terminals(true)?.iter().filter(|t| rotatable(t)) {
        let Some(sid) = t.session else { continue };
        if let Some(pct) = session_context_pct(store, cfg, sid)?
            && pct >= cfg.policy.ctx_rotate {
                start(store, &t.address, Some(pct), false, now)?;
            }
    }
    // 2. Tiến từng vòng đang chạy.
    let timeout = cfg.policy.handoff_timeout_min as i64 * 60_000;
    for r in active(store)? {
        let Some(t) = store.terminal(&r.old)? else {
            fail(store, &r, "không còn thấy terminal", now)?;
            continue;
        };
        if t.state != "live" {
            fail(store, &r, "terminal đã kết thúc trước khi bàn giao", now)?;
            continue;
        }
        if !rotatable(&t) {
            fail(store, &r, "chỉ xoay vòng được thợ OpenCode có mã việc", now)?;
            continue;
        }
        let task = r.task.clone().unwrap_or_default();
        let Some(cwd) = t.cwd.clone().filter(|c| !c.trim().is_empty()) else {
            fail(store, &r, "không biết thư mục làm việc của terminal", now)?;
            continue;
        };
        match r.state.as_str() {
            // Chỉ gửi khi đang idle (và kèm `--if idle`): `done` có thể vừa chuyển sang lượt mới.
            "pending" if t.status.as_deref() == Some("idle") => {
                let head = match git.head(&cwd) {
                    Ok(h) => h,
                    Err(e) => {
                        fail(store, &r, &format!("không đọc được git HEAD trong {cwd}: {e:#}"), now)?;
                        continue;
                    }
                };
                match hc.send(&t.address, &handoff_text(&task), true, false) {
                    Ok(_) => {
                        store.conn().execute(
                            "UPDATE rotations SET state = 'requested', head_before = ?2, turn_before = ?3, requested_ms = ?4, updated_ms = ?4
                             WHERE id = ?1",
                            params![r.id, head, t.turn, now],
                        )?;
                        store.log_action(POLICY_ACTOR, "rotate", &t.address, "yêu cầu bàn giao", true, now)?;
                    }
                    Err(e) => store.log_action(POLICY_ACTOR, "rotate", &t.address, &format!("gửi yêu cầu bàn giao lỗi: {e:#}"), false, now)?,
                }
            }
            "requested" => {
                let timed_out = now - r.requested_ms.unwrap_or(now) > timeout;
                if t.turn > r.turn_before && idle(&t) {
                    verify_and_respawn(store, cfg, hc, git, &r, &t, timed_out, now)?;
                } else if timed_out {
                    fail(store, &r, &format!("không bàn giao trong {} phút", cfg.policy.handoff_timeout_min), now)?;
                }
            }
            _ => {}
        }
    }
    Ok(())
}

/// Bàn giao chưa xong (worktree bẩn, chưa có commit) → chờ tiếp tới hết hạn rồi mới báo thất bại.
#[allow(clippy::too_many_arguments)]
fn verify_and_respawn(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    git: &dyn Git,
    r: &Rotation,
    t: &TerminalRow,
    timed_out: bool,
    now: i64,
) -> Result<()> {
    let task = r.task.clone().unwrap_or_default();
    let cwd = t.cwd.clone().unwrap_or_default();
    let Some(template) = t.template.clone() else { return fail(store, r, "không biết template để mở phiên mới", now) };
    let Some(base) = r.head_before.clone() else { return fail(store, r, "không đọc được HEAD lúc yêu cầu bàn giao", now) };
    let wait_or_fail = |reason: &str| if timed_out { fail(store, r, reason, now) } else { Ok(()) };
    if !git.is_clean(&cwd).unwrap_or(false) {
        return wait_or_fail("worktree còn thay đổi chưa commit");
    }
    let Some(commit) = git.touched_since(&cwd, &base, &format!(".agents/log/{task}.md")).unwrap_or(None) else {
        return wait_or_fail(&format!("không có commit mới chạm .agents/log/{task}.md"));
    };
    store.conn().execute("UPDATE rotations SET commit_sha = ?2 WHERE id = ?1", params![r.id, commit])?;
    let req = SpawnReq {
        template,
        task: task.clone(),
        role: t.role.clone().unwrap_or_else(|| "worker".into()),
        name: None,
        cwd: t.cwd.clone(),
        text: Some(resume_text(&task)),
    };
    // Kiểm mọi điều kiện mở phiên mới TRƯỚC khi kill (không để thợ chết mà không có người thay).
    if let Err(e) = actions::spawn_precheck(store, cfg, &req, Some(&t.address)) {
        return fail(store, r, &format!("không mở được phiên mới: {e:#}"), now);
    }
    if let Err(e) = actions::kill(store, hc, &Caller::Agent(Some(POLICY_ACTOR.into())), &t.address, now) {
        return fail(store, r, &format!("kill phiên cũ lỗi: {e:#}"), now);
    }
    // Đã kill: từ đây không được dừng giữa chừng vì lỗi CSDL — phải thử mở phiên mới.
    // Đánh dấu ngay (không chờ sự kiện hc) để max_workers và vòng sau không còn tính phiên cũ.
    if let Err(e) = store.finish_terminal(&t.address, None, now) {
        let _ = store.log_action(POLICY_ACTOR, "rotate", &t.address, &format!("đánh dấu kết thúc lỗi: {e:#}"), false, now);
    }
    match actions::spawn(store, hc, cfg, &req, now) {
        Ok(addr) => {
            store.conn().execute(
                "UPDATE rotations SET new_terminal = ?2, state = 'done', detail = ?3, updated_ms = ?4 WHERE id = ?1",
                params![r.id, addr, format!("{} → {addr} (commit {commit})", t.address), now],
            )?;
            store.log_action(POLICY_ACTOR, "rotate", &t.address, &format!("→ {addr}"), true, now)?;
            Ok(())
        }
        Err(e) => fail_with(
            store,
            r,
            &format!("mở phiên mới lỗi: {e:#}"),
            &format!("Phiên cũ ĐÃ bị dừng; bàn giao nằm ở commit {commit} — hãy `agm spawn` lại cho đầu việc {task}."),
            now,
        ),
    }
}
