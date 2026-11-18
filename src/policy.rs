//! Policy: ngân sách (spec §5.7), cảnh báo ngữ cảnh và xoay vòng thợ OpenCode (§5.9).
//! Mọi hành động tự động tắt được bằng `agm policy off` (setting `policy = off`).

use anyhow::Result;
use rusqlite::{params, OptionalExtension};

use crate::actions::{self, Caller};
use crate::config::{context_pct, Config};
use crate::hc::Hc;
use crate::store::{BudgetRow, Store, TerminalRow};

pub const POLICY_ACTOR: &str = "agm-policy";

pub fn enabled(store: &Store) -> Result<bool> {
    Ok(store.setting("policy")?.as_deref() != Some("off"))
}

/// Tổng token (input + cache + output) của mọi phiên gắn với terminal thuộc đầu việc.
pub fn task_tokens(store: &Store, task: &str) -> Result<u64> {
    let n: i64 = store.conn().query_row(
        "SELECT COALESCE(SUM(u.input_uncached + u.cache_read + u.cache_write + u.output), 0) FROM usage u
         WHERE u.session IN (SELECT session FROM terminals WHERE task = ?1 AND session IS NOT NULL)",
        params![task],
        |r| r.get(0),
    )?;
    Ok(n.max(0) as u64)
}

fn provider_percent(store: &Store, provider: &str, window: u32) -> Result<Option<f64>> {
    Ok(store
        .conn()
        .query_row(
            "SELECT used_percent FROM quota WHERE provider = ?1 AND window_minutes = ?2 ORDER BY ts_ms DESC LIMIT 1",
            params![provider, window],
            |r| r.get(0),
        )
        .optional()?)
}

/// Giá trị hiện tại của một ngân sách; None nếu chưa có số liệu.
pub fn budget_value(store: &Store, b: &BudgetRow) -> Result<Option<f64>> {
    let parts: Vec<&str> = b.scope.split(':').collect();
    match parts.as_slice() {
        ["task", task] => Ok(Some(task_tokens(store, task)? as f64)),
        ["provider", p, w] => match w.parse() {
            Ok(w) => provider_percent(store, p, w),
            Err(_) => Ok(None),
        },
        _ => Ok(None),
    }
}

/// Lý do từ chối spawn nếu ngân sách cứng của đầu việc đã vượt.
pub fn spawn_blocked_by_budget(store: &Store, task: &str) -> Result<Option<String>> {
    let scope = format!("task:{task}");
    for b in store.budgets()?.into_iter().filter(|b| b.scope == scope) {
        if let Some(v) = budget_value(store, &b)? {
            if v >= b.hard {
                return Ok(Some(format!("ngân sách {scope} đã vượt ngưỡng cứng: {v:.0} / {:.0} {}", b.hard, b.unit)));
            }
        }
    }
    Ok(None)
}

/// % ngữ cảnh của phiên: cửa sổ ghi trong CSDL (log/hook) hoặc theo cấu hình model.
pub fn session_context_pct(store: &Store, cfg: &Config, sid: i64) -> Result<Option<f64>> {
    let row: Option<(Option<i64>, Option<i64>, Option<String>)> = store
        .conn()
        .query_row("SELECT ctx_used, ctx_window, model FROM sessions WHERE id = ?1", [sid], |r| Ok((r.get(0)?, r.get(1)?, r.get(2)?)))
        .optional()?;
    let Some((Some(used), window, model)) = row else { return Ok(None) };
    let window = window.map(|w| w as u64).or_else(|| model.as_deref().and_then(|m| cfg.model(m)).and_then(|m| m.context_window));
    Ok(context_pct(used.max(0) as u64, window))
}

fn notify(store: &Store, cfg: &Config, hc: &dyn Hc, level: &str, task: Option<&str>, text: &str, now: i64) -> Result<bool> {
    let opened = store.open_inbox(None, "budget", Some(level), text, now)?.is_some();
    if opened {
        if let Some(mgr) = &cfg.policy.manager {
            // Lỗi gửi đã được ghi trong bảng messages; không làm hỏng vòng policy.
            let _ = actions::send(store, hc, POLICY_ACTOR, mgr, "LỖI", task, text, now);
        }
    }
    Ok(opened)
}

fn task_terminals(store: &Store, task: &str) -> Result<Vec<TerminalRow>> {
    Ok(store.terminals(true)?.into_iter().filter(|t| t.task.as_deref() == Some(task)).collect())
}

fn budgets_tick(store: &Store, cfg: &Config, hc: &dyn Hc, now: i64) -> Result<()> {
    for b in store.budgets()? {
        let Some(v) = budget_value(store, &b)? else { continue };
        let task = b.scope.strip_prefix("task:");
        if v >= b.hard {
            let text = format!("Ngân sách {} đã vượt ngưỡng cứng ({:.0} {})", b.scope, b.hard, b.unit);
            // Chỉ hành động khi vừa vượt (mục hộp thư mới mở), không lặp mỗi vòng.
            if notify(store, cfg, hc, "hard", task, &text, now)? {
                if let (Some(task), "kill" | "interrupt") = (task, b.action.as_str()) {
                    let caller = Caller::Agent(Some(POLICY_ACTOR.into()));
                    for t in task_terminals(store, task)? {
                        // actions:: từ chối Codex/Claude và chuyển đề nghị cho người dùng.
                        let _ = if b.action == "kill" {
                            actions::kill(store, hc, &caller, &t.address, now)
                        } else {
                            actions::interrupt(store, hc, &caller, &t.address, now)
                        };
                    }
                }
            }
        } else if v >= b.soft {
            let text = format!("Ngân sách {} đã vượt ngưỡng mềm ({:.0} {})", b.scope, b.soft, b.unit);
            notify(store, cfg, hc, "soft", task, &text, now)?;
        }
    }
    Ok(())
}

fn context_tick(store: &Store, cfg: &Config, now: i64) -> Result<()> {
    for t in store.terminals(true)? {
        let Some(sid) = t.session else { continue };
        let Some(pct) = session_context_pct(store, cfg, sid)? else { continue };
        if pct >= cfg.policy.ctx_warn {
            let name = t.name.as_deref().unwrap_or(&t.address);
            let text = format!("{name} đã dùng từ {:.0}% ngữ cảnh trở lên", cfg.policy.ctx_warn);
            store.open_inbox(Some(&t.address), "context", Some("warn"), &text, now)?;
        }
    }
    Ok(())
}

/// Một vòng policy: ngân sách, cảnh báo ngữ cảnh, xoay vòng thợ.
pub fn tick(store: &Store, cfg: &Config, hc: &dyn Hc, now: i64) -> Result<()> {
    if !enabled(store)? {
        return Ok(());
    }
    budgets_tick(store, cfg, hc, now)?;
    context_tick(store, cfg, now)?;
    Ok(())
}
