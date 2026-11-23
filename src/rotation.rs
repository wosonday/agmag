//! Xoay vòng phiên theo mức đầy ngữ cảnh:
//! - OpenCode worker: pending → requested → (commit .agents/log/<ID>.md) → kill cũ + spawn mới (`done`).
//! - Claude Code & Codex (managed=1): pending → requested → handoff_ready → spawning → awaiting_link → done
//!   (KHÔNG bao giờ gọi `kill`/`interrupt` lên phiên cũ; đánh dấu `handed_off` để người dùng dọn thủ công).

use std::collections::BTreeMap;
use std::path::Path;

use anyhow::Result;
use rusqlite::params;

use crate::actions::{self, Caller, SpawnReq};
use crate::config::Config;
use crate::correlate;
use crate::git::{self, Git};
use crate::hc::{Hc, RunSpec, terminal_row_from};
use crate::launch::{LaunchSpec, build_successor_argv};
use crate::policy::{POLICY_ACTOR, enabled, session_context_pct};
use crate::store::{Store, TerminalRow};

pub const HANDOFF_MARKER: &str = "<!-- agm:handoff-done -->";
const ACTIVE_STATES_SQL: &str =
    "'pending', 'requested', 'handoff_ready', 'spawning', 'awaiting_link'";

struct Rotation {
    id: i64,
    task: Option<String>,
    old: String,
    new_terminal: Option<String>,
    app: Option<String>,
    cwd: Option<String>,
    handoff_path: Option<String>,
    state: String,
    head_before: Option<String>,
    turn_before: i64,
    requested_ms: Option<i64>,
    updated_ms: i64,
    ctx_pct: Option<f64>,
}

pub fn handoff_text(task: &str) -> String {
    format!(
        "[GIAO] [ID {task}] từ agmag: ngữ cảnh của bạn đã đầy. Hãy dừng việc đang làm, ghi mục bàn giao vào .agents/log/{task}.md \
         (đã làm gì, còn gì, lỗi nguyên văn, quyết định đã chốt), commit tất cả (thông điệp bắt đầu bằng '{task}:'), rồi dừng. \
         Một phiên mới sẽ tiếp tục từ tệp đó."
    )
}

pub fn resume_text(task: &str) -> String {
    format!(
        "Đọc .agents/tasks/{task}.md, .agents/log/{task}.md và git log của nhánh, rồi làm tiếp đầu việc {task}."
    )
}

pub fn interactive_handoff_text(rot_id: i64, rel_path: &str) -> String {
    format!(
        "[GIAO] từ agmag (rotation {rot_id}): ngữ cảnh của bạn đã đầy. Hãy tạm dừng việc đang làm, \
         ghi đầy đủ trạng thái hiện tại, việc đã làm, việc còn lại, các tệp liên quan và bước tiếp theo vào tệp `{rel_path}`. \
         Trong tệp phải ghi rõ `rotation_id: {rot_id}` và dòng cuối cùng phải là đúng chuỗi `{HANDOFF_MARKER}` rồi dừng lại. \
         Một phiên mới sẽ đọc tệp đó để tiếp tục."
    )
}

pub fn build_successor_prompt(app: &str, rot_id: i64, rel_path: &str) -> String {
    if app == "codex" {
        format!(
            "[agm-rot:{rot_id}] Read {rel_path}, summarize your understanding briefly, then wait for user instructions."
        )
    } else {
        format!(
            "Read {rel_path}, summarize your understanding briefly, then wait for user instructions."
        )
    }
}

pub fn is_handoff_ready(path: &Path, rot_id: i64) -> bool {
    let Ok(raw) = std::fs::read_to_string(path) else {
        return false;
    };
    if raw.len() < 32 {
        return false;
    }
    let id_str = rot_id.to_string();
    let normalized = raw.replace("\r\n", "\n");
    let has_id = normalized.lines().any(|l| {
        let trimmed = l.trim().trim_matches('`').trim();
        trimmed
            .strip_prefix("rotation_id:")
            .is_some_and(|v| v.trim().trim_matches('`') == id_str)
    });
    if !has_id {
        return false;
    }
    let Some(last_line) = normalized.lines().rev().find(|l| !l.trim().is_empty()) else {
        return false;
    };
    last_line.trim() == HANDOFF_MARKER
}

fn rotatable_opencode(t: &TerminalRow) -> bool {
    t.role.as_deref() == Some("worker")
        && t.app.as_deref() == Some("opencode")
        && t.task.is_some()
        && t.handoff_state == "active"
}

fn rotatable_interactive(t: &TerminalRow) -> bool {
    t.managed
        && matches!(t.app.as_deref(), Some("claude-code" | "codex"))
        && t.handoff_state == "active"
}

fn rotatable(t: &TerminalRow) -> bool {
    rotatable_opencode(t) || rotatable_interactive(t)
}

fn idle(t: &TerminalRow) -> bool {
    matches!(t.status.as_deref(), Some("idle" | "done"))
}

fn active(store: &Store) -> Result<Vec<Rotation>> {
    let sql = format!(
        "SELECT id, task, old_terminal, new_terminal, app, cwd, handoff_path, state, head_before,
                COALESCE(turn_before, 0), requested_ms, updated_ms, ctx_pct
         FROM rotations WHERE state IN ({ACTIVE_STATES_SQL}) ORDER BY id"
    );
    let mut stmt = store.conn().prepare(&sql)?;
    let rows = stmt
        .query_map([], |r| {
            Ok(Rotation {
                id: r.get(0)?,
                task: r.get(1)?,
                old: r.get(2)?,
                new_terminal: r.get(3)?,
                app: r.get(4)?,
                cwd: r.get(5)?,
                handoff_path: r.get(6)?,
                state: r.get(7)?,
                head_before: r.get(8)?,
                turn_before: r.get(9)?,
                requested_ms: r.get(10)?,
                updated_ms: r.get(11)?,
                ctx_pct: r.get(12)?,
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
    let text = format!(
        "Xoay vòng {} thất bại: {reason}. {after} Dùng `agm rotate {}` để thử lại.",
        r.old, r.old
    );
    store.open_inbox(Some(&r.old), "rotation", Some("failed"), &text, now)?;
    store.log_action(POLICY_ACTOR, "rotate", &r.old, reason, false, now)?;
    Ok(())
}

fn needs_attention(store: &Store, r: &Rotation, reason: &str, now: i64) -> Result<()> {
    set_state(store, r.id, "needs_attention", Some(reason), now)?;
    let text = format!("Xoay vòng {} cần xử lý: {reason}.", r.old);
    store.open_inbox(
        Some(&r.old),
        "rotation",
        Some("needs_attention"),
        &text,
        now,
    )?;
    store.log_action(POLICY_ACTOR, "rotate", &r.old, reason, false, now)?;
    Ok(())
}

/// Bắt đầu xoay vòng một terminal. Bỏ qua nếu đã có vòng đang chạy hoặc terminal đã `handed_off`.
/// Vòng **tự động** cũng bỏ qua terminal đã có vòng thất bại/cần chú ý (tránh nhắc bàn giao mãi).
pub fn start(
    store: &Store,
    addr: &str,
    ctx_pct: Option<f64>,
    manual: bool,
    now: i64,
) -> Result<bool> {
    let Some(t) = store.terminal(addr)? else {
        return Ok(false);
    };
    if t.handoff_state == "handed_off" {
        return Ok(false);
    }
    let states = if manual {
        ACTIVE_STATES_SQL.to_string()
    } else {
        format!("{ACTIVE_STATES_SQL}, 'failed', 'needs_attention'")
    };
    let busy: bool = store.conn().query_row(
        &format!(
            "SELECT EXISTS(SELECT 1 FROM rotations WHERE old_terminal = ?1 AND state IN ({states}))"
        ),
        [&t.address],
        |r| r.get(0),
    )?;
    if busy {
        return Ok(false);
    }
    // Thư mục xoay vòng: terminal managed (Claude/Codex) → `launch_spec.cwd` mà người dùng đã
    // xác nhận ở `agm manage`; terminal do hc sinh (OpenCode, không có spec) → `terminals.cwd`.
    // Không đọc `foreground` ở đây: bước `requested`/`handoff_ready` phải dùng một thư mục ổn định.
    let rotation_cwd = t
        .launch_spec
        .as_deref()
        .and_then(|json| LaunchSpec::from_json(json).ok())
        .map(|spec| spec.cwd)
        .filter(|c| !c.trim().is_empty())
        .or_else(|| t.cwd.clone().filter(|c| !c.trim().is_empty()));
    store.conn().execute(
        "INSERT INTO rotations (task, old_terminal, app, cwd, state, ctx_pct, updated_ms)
         VALUES (?1, ?2, ?3, ?4, 'pending', ?5, ?6)",
        params![t.task, t.address, t.app, rotation_cwd, ctx_pct, now],
    )?;
    Ok(true)
}

pub fn tick(store: &Store, cfg: &Config, hc: &dyn Hc, git: &dyn Git, now: i64) -> Result<()> {
    if !enabled(store)? {
        return Ok(());
    }
    // 1. Phát hiện phiên đạt ngưỡng xoay vòng (hoặc cảnh báo phiên Claude/Codex mở tay).
    for t in store.terminals(true)? {
        let Some(sid) = t.session else { continue };
        let Some(pct) = session_context_pct(store, cfg, sid)? else {
            continue;
        };
        if pct < cfg.policy.ctx_rotate {
            continue;
        }
        if rotatable(&t) {
            start(store, &t.address, Some(pct), false, now)?;
        } else if !t.managed
            && matches!(t.app.as_deref(), Some("claude-code" | "codex"))
            && t.handoff_state == "active"
        {
            let name = t.name.as_deref().unwrap_or(&t.address);
            let app = t.app.as_deref().unwrap_or("?");
            let text = format!(
                "{name} ({app}) đã đạt {pct:.0}% ngữ cảnh (mở tay, chưa bật quản lý) — chạy `agm manage {}` để tự động bàn giao hoặc tự bàn giao sang phiên mới.",
                t.address
            );
            // Khử trùng theo khóa (không theo nội dung) để % đổi 86% → 87% không mở thêm mục mới.
            store.open_inbox_keyed(&t.address, "rotation", "unmanaged", &text, now)?;
        }
    }
    // 2. Tiến từng vòng đang chạy theo nhánh app tương ứng.
    for r in active(store)? {
        let app = r
            .app
            .clone()
            .or_else(|| store.terminal(&r.old).ok().flatten().and_then(|t| t.app));
        match app.as_deref() {
            Some("claude-code" | "codex") => {
                advance_interactive(store, cfg, hc, &r, app.as_deref().unwrap(), now)?
            }
            _ => advance_opencode(store, cfg, hc, git, &r, now)?,
        }
    }
    Ok(())
}

fn advance_opencode(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    git: &dyn Git,
    r: &Rotation,
    now: i64,
) -> Result<()> {
    let timeout = cfg.policy.handoff_timeout_min as i64 * 60_000;
    let Some(t) = store.terminal(&r.old)? else {
        return fail(store, r, "không còn thấy terminal", now);
    };
    if t.state != "live" {
        return fail(store, r, "terminal đã kết thúc trước khi bàn giao", now);
    }
    if !rotatable_opencode(&t) {
        return fail(store, r, "chỉ xoay vòng được thợ OpenCode có mã việc", now);
    }
    let task = r.task.clone().unwrap_or_default();
    let Some(cwd) = t.cwd.clone().filter(|c| !c.trim().is_empty()) else {
        return fail(store, r, "không biết thư mục làm việc của terminal", now);
    };
    match r.state.as_str() {
        // Giữ nguyên OpenCode: chỉ gửi khi `idle` (và kèm `--if idle`), vì `done` có thể vừa chuyển sang lượt mới.
        "pending" if t.status.as_deref() == Some("idle") => {
            let head = match git.head(&cwd) {
                Ok(h) => h,
                Err(e) => {
                    return fail(
                        store,
                        r,
                        &format!("không đọc được git HEAD trong {cwd}: {e:#}"),
                        now,
                    );
                }
            };
            match hc.send(&t.address, &handoff_text(&task), true, false) {
                Ok(_) => {
                    store.conn().execute(
                        "UPDATE rotations SET state = 'requested', head_before = ?2, turn_before = ?3, requested_ms = ?4, updated_ms = ?4
                         WHERE id = ?1",
                        params![r.id, head, t.turn, now],
                    )?;
                    store.log_action(
                        POLICY_ACTOR,
                        "rotate",
                        &t.address,
                        "yêu cầu bàn giao",
                        true,
                        now,
                    )?;
                }
                Err(e) => store.log_action(
                    POLICY_ACTOR,
                    "rotate",
                    &t.address,
                    &format!("gửi yêu cầu bàn giao lỗi: {e:#}"),
                    false,
                    now,
                )?,
            }
        }
        "requested" => {
            let timed_out = now - r.requested_ms.unwrap_or(now) > timeout;
            if t.turn > r.turn_before && idle(&t) {
                verify_and_respawn(store, cfg, hc, git, r, &t, timed_out, now)?;
            } else if timed_out {
                fail(
                    store,
                    r,
                    &format!(
                        "không bàn giao trong {} phút",
                        cfg.policy.handoff_timeout_min
                    ),
                    now,
                )?;
            }
        }
        _ => {}
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
    let Some(template) = t.template.clone() else {
        return fail(store, r, "không biết template để mở phiên mới", now);
    };
    let Some(base) = r.head_before.clone() else {
        return fail(store, r, "không đọc được HEAD lúc yêu cầu bàn giao", now);
    };
    let wait_or_fail = |reason: &str| {
        if timed_out {
            fail(store, r, reason, now)
        } else {
            Ok(())
        }
    };
    if !git.is_clean(&cwd).unwrap_or(false) {
        return wait_or_fail("worktree còn thay đổi chưa commit");
    }
    let Some(commit) = git
        .touched_since(&cwd, &base, &format!(".agents/log/{task}.md"))
        .unwrap_or(None)
    else {
        return wait_or_fail(&format!("không có commit mới chạm .agents/log/{task}.md"));
    };
    store.conn().execute(
        "UPDATE rotations SET commit_sha = ?2 WHERE id = ?1",
        params![r.id, commit],
    )?;
    let mut req = SpawnReq {
        template,
        task: task.clone(),
        role: t
            .role
            .clone()
            .filter(|r| !r.trim().is_empty())
            .unwrap_or_else(|| "worker".into()),
        name: None,
        cwd: t.cwd.clone(),
        text: vec![resume_text(&task)],
    };
    // Kiểm mọi điều kiện mở phiên mới TRƯỚC khi kill (không để thợ chết mà không có người thay).
    if let Err(e) = actions::spawn_precheck(store, cfg, &req, Some(&t.address)) {
        return fail(store, r, &format!("không mở được phiên mới: {e:#}"), now);
    }
    if let Err(e) = actions::prepare_rotation_spawn(hc, &mut req) {
        return fail(store, r, &format!("không mở được phiên mới: {e:#}"), now);
    }
    if let Err(e) = actions::kill(
        store,
        hc,
        &Caller::Agent(Some(POLICY_ACTOR.into())),
        &t.address,
        now,
    ) {
        return fail(store, r, &format!("kill phiên cũ lỗi: {e:#}"), now);
    }
    // Đã kill: từ đây không được dừng giữa chừng vì lỗi CSDL — phải thử mở phiên mới.
    // Đánh dấu ngay (không chờ sự kiện hc) để max_workers và vòng sau không còn tính phiên cũ.
    if let Err(e) = store.finish_terminal(&t.address, None, now) {
        let _ = store.log_action(
            POLICY_ACTOR,
            "rotate",
            &t.address,
            &format!("đánh dấu kết thúc lỗi: {e:#}"),
            false,
            now,
        );
    }
    match actions::spawn(store, hc, cfg, &req, now) {
        Ok(addr) => {
            store.conn().execute(
                "UPDATE rotations SET new_terminal = ?2, state = 'done', detail = ?3, updated_ms = ?4 WHERE id = ?1",
                params![r.id, addr, format!("{} → {addr} (commit {commit})", t.address), now],
            )?;
            store.log_action(
                POLICY_ACTOR,
                "rotate",
                &t.address,
                &format!("→ {addr}"),
                true,
                now,
            )?;
            Ok(())
        }
        Err(e) => match e.downcast::<actions::BriefUndelivered>() {
            // Phiên mới đã mở (chỉ thiếu brief): ghi lại để không ai spawn trùng.
            Ok(u) => {
                store.conn().execute(
                    "UPDATE rotations SET new_terminal = ?2 WHERE id = ?1",
                    params![r.id, u.address],
                )?;
                needs_attention(
                    store,
                    r,
                    &format!(
                        "{} → {}: bàn giao ở commit {commit}; {u}",
                        t.address, u.address
                    ),
                    now,
                )
            }
            Err(e) => fail_with(
                store,
                r,
                &format!("mở phiên mới lỗi: {e:#}"),
                &format!(
                    "Phiên cũ ĐÃ bị dừng; bàn giao nằm ở commit {commit} — hãy `agm spawn` lại cho đầu việc {task}."
                ),
                now,
            ),
        },
    }
}

fn fresh_idle_status(hc: &dyn Hc, t: &TerminalRow) -> Option<String> {
    let v = hc.get(&t.address).ok()?;
    let fresh = terminal_row_from(&v);
    if v["state"].as_str() != Some("live")
        || fresh.address != t.address
        || fresh.app.is_none()
        || fresh.app != t.app
    {
        return None;
    }
    fresh
        .status
        .filter(|s| matches!(s.as_str(), "idle" | "done"))
}

fn advance_interactive(
    store: &Store,
    cfg: &Config,
    hc: &dyn Hc,
    r: &Rotation,
    app: &str,
    now: i64,
) -> Result<()> {
    let handoff_timeout = cfg.policy.handoff_timeout_min as i64 * 60_000;
    let link_timeout = cfg.policy.link_timeout_min as i64 * 60_000;
    let old_opt = store.terminal(&r.old)?;

    match r.state.as_str() {
        "pending" => {
            let Some(t) = old_opt else {
                return fail(store, r, "không còn thấy terminal", now);
            };
            if t.state != "live" {
                return fail(store, r, "terminal đã kết thúc trước khi bàn giao", now);
            }
            if !t.managed {
                return fail(store, r, "terminal chưa bật quản lý (`agm manage`)", now);
            }
            // Chỉ tự động hủy khi ngưỡng là tự động (`ctx_pct` khác NULL); giữ nguyên vòng gọi tay.
            if r.ctx_pct.is_some()
                && let Some(sid) = t.session
                && let Some(pct) = session_context_pct(store, cfg, sid)?
                && pct < cfg.policy.ctx_warn
            {
                return set_state(
                    store,
                    r.id,
                    "cancelled",
                    Some("ngữ cảnh đã giảm dưới ngưỡng cảnh báo"),
                    now,
                );
            }
            if !idle(&t) {
                return Ok(());
            }
            let Some(fresh) = fresh_idle_status(hc, &t) else {
                return Ok(());
            };
            let Some(cwd) = r
                .cwd
                .clone()
                .or_else(|| t.cwd.clone())
                .filter(|c| !c.trim().is_empty())
            else {
                return fail(store, r, "không biết thư mục làm việc của terminal", now);
            };
            if !Path::new(&cwd).is_dir() {
                return needs_attention(
                    store,
                    r,
                    &format!("thư mục '{cwd}' không còn tồn tại"),
                    now,
                );
            }
            let rel_path = format!(".agents/handoff/{}-{now}.md", r.id);
            let abs_path = Path::new(&cwd).join(&rel_path);
            if abs_path.exists() {
                return fail(
                    store,
                    r,
                    &format!("tệp bàn giao {rel_path} đã tồn tại"),
                    now,
                );
            }
            if let Some(parent) = abs_path.parent()
                && let Err(e) = std::fs::create_dir_all(parent)
            {
                return needs_attention(
                    store,
                    r,
                    &format!("không tạo được thư mục bàn giao: {e}"),
                    now,
                );
            }
            if let Err(e) = git::ensure_handoff_excluded(&cwd) {
                let _ = store.log_action(
                    POLICY_ACTOR,
                    "exclude",
                    &t.address,
                    &format!("cảnh báo exclude: {e:#}"),
                    false,
                    now,
                );
            }
            let if_idle = fresh == "idle";
            match hc.send(
                &t.address,
                &interactive_handoff_text(r.id, &rel_path),
                if_idle,
                false,
            ) {
                Ok(_) => {
                    store.conn().execute(
                        "UPDATE rotations SET state = 'requested', cwd = ?2, handoff_path = ?3, turn_before = ?4, requested_ms = ?5, updated_ms = ?5
                         WHERE id = ?1",
                        params![r.id, cwd, rel_path, t.turn, now],
                    )?;
                    store.log_action(
                        POLICY_ACTOR,
                        "rotate",
                        &t.address,
                        "yêu cầu bàn giao (không kill)",
                        true,
                        now,
                    )?;
                }
                Err(e) => store.log_action(
                    POLICY_ACTOR,
                    "rotate",
                    &t.address,
                    &format!("gửi yêu cầu bàn giao lỗi: {e:#}"),
                    false,
                    now,
                )?,
            }
        }
        "requested" => {
            let Some(cwd) = r.cwd.as_deref() else {
                return fail(store, r, "thiếu cwd của rotation", now);
            };
            let Some(rel) = r.handoff_path.as_deref() else {
                return fail(store, r, "thiếu đường dẫn bàn giao", now);
            };
            let abs = Path::new(cwd).join(rel);
            let handoff_file_ok = is_handoff_ready(&abs, r.id);
            let live_ready = match &old_opt {
                Some(t) if t.state == "live" => {
                    t.turn > r.turn_before && idle(t) && handoff_file_ok
                }
                _ => handoff_file_ok,
            };
            let timed_out = now - r.requested_ms.unwrap_or(now) > handoff_timeout;
            if live_ready {
                store.conn().execute(
                    "UPDATE rotations SET state = 'handoff_ready', handoff_ms = ?2, updated_ms = ?2 WHERE id = ?1",
                    params![r.id, now],
                )?;
                // Tiếp tục mở phiên mới ngay trong cùng tick nếu điều kiện sẵn sàng.
                let ready = Rotation {
                    id: r.id,
                    task: r.task.clone(),
                    old: r.old.clone(),
                    new_terminal: r.new_terminal.clone(),
                    app: r.app.clone(),
                    cwd: r.cwd.clone(),
                    handoff_path: r.handoff_path.clone(),
                    state: "handoff_ready".into(),
                    head_before: r.head_before.clone(),
                    turn_before: r.turn_before,
                    requested_ms: r.requested_ms,
                    updated_ms: now,
                    ctx_pct: r.ctx_pct,
                };
                return advance_interactive(store, cfg, hc, &ready, app, now);
            }
            match old_opt {
                None => return fail(store, r, "không còn thấy terminal", now),
                Some(t) if t.state != "live" => {
                    return fail(store, r, "terminal đã kết thúc trước khi bàn giao", now);
                }
                _ if timed_out => {
                    fail(
                        store,
                        r,
                        &format!(
                            "chưa ghi xong tệp bàn giao hợp lệ trong {} phút",
                            cfg.policy.handoff_timeout_min
                        ),
                        now,
                    )?;
                }
                _ => {}
            }
        }
        "handoff_ready" => {
            // Tệp bàn giao đã sẵn sàng → tiếp tục mở phiên kế nhiệm ngay cả khi phiên cũ đã thoát.
            let Some(t) = old_opt else {
                return needs_attention(store, r, "không tìm thấy bản ghi terminal nguồn", now);
            };
            let Some(rel) = r.handoff_path.as_deref() else {
                return needs_attention(store, r, "thiếu đường dẫn tệp bàn giao", now);
            };
            let Some(spec_json) = t.launch_spec.as_deref() else {
                return needs_attention(store, r, "thiếu launch_spec để mở phiên mới", now);
            };
            let mut spec = match LaunchSpec::from_json(spec_json) {
                Ok(s) => s,
                Err(e) => {
                    return needs_attention(store, r, &format!("launch_spec hỏng: {e:#}"), now);
                }
            };
            if let Some(cwd) = &r.cwd {
                spec.cwd = cwd.clone();
            }
            let new_name = match actions::spawn_successor_precheck(
                store,
                r.task.as_deref(),
                t.name.as_deref(),
                &t.address,
                r.id,
                &spec,
            ) {
                Ok(n) => n,
                Err(e) => {
                    return needs_attention(
                        store,
                        r,
                        &format!("precheck mở phiên mới lỗi: {e:#}"),
                        now,
                    );
                }
            };
            let prompt = build_successor_prompt(app, r.id, rel);
            let argv = build_successor_argv(&spec, &prompt);
            let mut labels = BTreeMap::new();
            labels.insert("agm.parent_rotation".into(), r.id.to_string());
            if let Some(task) = &r.task {
                labels.insert("agm.task".into(), task.clone());
            }
            if let Some(role) = &t.role {
                labels.insert("agm.role".into(), role.clone());
            }
            set_state(store, r.id, "spawning", Some("đang mở phiên kế nhiệm"), now)?;
            let run_spec = RunSpec {
                argv,
                name: Some(new_name),
                cwd: Some(spec.cwd.clone()),
                labels,
            };
            let res = hc.run(&run_spec);
            let spec_saved = spec.to_json()?;
            match res {
                Ok(v) => {
                    let mut row = terminal_row_from(&v);
                    if row.address.is_empty() {
                        return needs_attention(
                            store,
                            r,
                            "hc run không trả địa chỉ terminal mới",
                            now,
                        );
                    }
                    row.app.get_or_insert_with(|| app.to_string());
                    row.cwd.get_or_insert_with(|| spec.cwd.clone());
                    row.task = row.task.or_else(|| r.task.clone());
                    row.role = row.role.or_else(|| t.role.clone());
                    if row.created_ms == 0 {
                        row.created_ms = now;
                    }
                    store.upsert_terminal(&row)?;
                    store.set_terminal_launch(&row.address, true, Some(&spec_saved))?;
                    store.conn().execute(
                        "UPDATE rotations SET new_terminal = ?2, state = 'awaiting_link', detail = ?3, updated_ms = ?4 WHERE id = ?1",
                        params![r.id, row.address, format!("{} → {} (đang chờ ghép)", t.address, row.address), now],
                    )?;
                    store.log_action(
                        POLICY_ACTOR,
                        "spawn_successor",
                        &row.address,
                        &format!("rotation #{}", r.id),
                        true,
                        now,
                    )?;
                }
                Err(e) => {
                    return needs_attention(
                        store,
                        r,
                        &format!("hc run mở phiên kế nhiệm lỗi: {e:#}"),
                        now,
                    );
                }
            }
        }
        "spawning" => {
            // Phục hồi sau crash ngay lúc gọi hc.run: tìm terminal có nhãn `agm.parent_rotation=<id>`.
            let tag = r.id.to_string();
            let mut found: Vec<TerminalRow> = Vec::new();
            if let Ok(list) = hc.ls(false) {
                for v in list {
                    let row = terminal_row_from(&v);
                    if row.labels.get("agm.parent_rotation").map(String::as_str)
                        == Some(tag.as_str())
                    {
                        found.push(row);
                    }
                }
            }
            if found.is_empty() {
                found = store
                    .terminals(true)?
                    .into_iter()
                    .filter(|t| {
                        t.labels.get("agm.parent_rotation").map(String::as_str)
                            == Some(tag.as_str())
                    })
                    .collect();
            }
            if let [only] = found.as_slice() {
                let mut row = only.clone();
                if row.created_ms == 0 {
                    row.created_ms = now;
                }
                store.upsert_terminal(&row)?;
                if let Some(old_spec) = old_opt.as_ref().and_then(|o| o.launch_spec.as_deref()) {
                    store.set_terminal_launch(&row.address, true, Some(old_spec))?;
                }
                store.conn().execute(
                    "UPDATE rotations SET new_terminal = ?2, state = 'awaiting_link', updated_ms = ?3 WHERE id = ?1",
                    params![r.id, row.address, now],
                )?;
            } else {
                needs_attention(
                    store,
                    r,
                    "không xác định chắc chắn terminal kế nhiệm sau gián đoạn spawning (không tự mở lại)",
                    now,
                )?;
            }
        }
        "awaiting_link" => {
            let Some(new_addr) = r.new_terminal.as_deref() else {
                return needs_attention(
                    store,
                    r,
                    "thiếu new_terminal ở trạng thái awaiting_link",
                    now,
                );
            };
            let Some(mut nt) = store.terminal(new_addr)? else {
                return needs_attention(
                    store,
                    r,
                    &format!("không thấy terminal kế nhiệm {new_addr}"),
                    now,
                );
            };
            if nt.state != "live" {
                return needs_attention(
                    store,
                    r,
                    &format!("phiên kế nhiệm {new_addr} đã thoát trước khi ghép"),
                    now,
                );
            }
            if app == "codex" && nt.session.is_none() {
                let cwd = r.cwd.as_deref().or(nt.cwd.as_deref()).unwrap_or_default();
                let _ = correlate::link_codex_rotation(
                    store,
                    &nt.address,
                    r.id,
                    cwd,
                    r.requested_ms.unwrap_or(r.updated_ms),
                )?;
                nt = store.terminal(&nt.address)?.unwrap_or(nt);
            }
            let linked = if app == "claude-code" {
                nt.session.is_some() && matches!(nt.link_source.as_deref(), Some("hook" | "manual"))
            } else {
                nt.session.is_some()
            };
            if linked {
                let old_spec = old_opt.as_ref().and_then(|o| o.launch_spec.as_deref());
                store.set_terminal_launch(&nt.address, true, old_spec)?;
                store.set_terminal_handoff_state(&r.old, "handed_off")?;
                store.conn().execute(
                    "UPDATE rotations SET state = 'done', linked_ms = ?2, detail = ?3, updated_ms = ?2 WHERE id = ?1",
                    params![r.id, now, format!("{} → {} (đã bàn giao, chờ dọn thủ công)", r.old, nt.address)],
                )?;
                store.log_action(
                    POLICY_ACTOR,
                    "rotate",
                    &r.old,
                    &format!("→ {} (handed_off)", nt.address),
                    true,
                    now,
                )?;
            } else if now - r.updated_ms > link_timeout {
                needs_attention(
                    store,
                    r,
                    &format!(
                        "quá {} phút chưa ghép được phiên cho {new_addr}",
                        cfg.policy.link_timeout_min
                    ),
                    now,
                )?;
            }
        }
        _ => {}
    }
    Ok(())
}
