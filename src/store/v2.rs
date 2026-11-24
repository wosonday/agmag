//! API cho các bảng v2: terminals, messages, inbox, budgets, actions, settings.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::{OptionalExtension, Row, params};
use serde::Serialize;

use super::Store;

fn approval_grant_from(row: &Row) -> rusqlite::Result<ApprovalGrant> {
    Ok(ApprovalGrant {
        id: row.get(0)?,
        lane: row.get(1)?,
        task: row.get(2)?,
        action: row.get(3)?,
        approved_by: row.get(4)?,
        approved_ms: row.get(5)?,
        expires_ms: row.get(6)?,
        revoked_ms: row.get(7)?,
        reason: row.get(8)?,
    })
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct ApprovalGrant {
    pub id: i64,
    pub lane: Option<String>,
    pub task: Option<String>,
    pub action: String,
    pub approved_by: String,
    pub approved_ms: i64,
    pub expires_ms: Option<i64>,
    pub revoked_ms: Option<i64>,
    pub reason: String,
}

#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct TerminalRow {
    pub address: String,
    pub name: Option<String>,
    pub app: Option<String>,
    pub cwd: Option<String>,
    pub labels: BTreeMap<String, String>,
    pub task: Option<String>,
    pub role: Option<String>,
    pub template: Option<String>,
    pub state: String,
    pub status: Option<String>,
    pub kind: Option<String>,
    pub msg: Option<String>,
    pub turn: i64,
    pub reported: bool,
    pub parent: Option<String>,
    pub created_ms: i64,
    pub finished_ms: Option<i64>,
    pub exit_code: Option<i64>,
    pub session: Option<i64>,
    pub link_source: Option<String>,
    pub handoff_state: String,
    pub managed: bool,
    pub launch_spec: Option<String>,
}

impl Default for TerminalRow {
    fn default() -> Self {
        Self {
            address: String::new(),
            name: None,
            app: None,
            cwd: None,
            labels: BTreeMap::new(),
            task: None,
            role: None,
            template: None,
            state: String::new(),
            status: None,
            kind: None,
            msg: None,
            turn: 0,
            reported: false,
            parent: None,
            created_ms: 0,
            finished_ms: None,
            exit_code: None,
            session: None,
            link_source: None,
            handoff_state: "active".into(),
            managed: false,
            launch_spec: None,
        }
    }
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct InboxRow {
    pub id: i64,
    pub ts_ms: i64,
    pub terminal: Option<String>,
    pub source: String,
    pub kind: Option<String>,
    pub text: String,
    pub resolved_ms: Option<i64>,
    pub reply: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct MessageRow {
    pub id: i64,
    pub ts_ms: i64,
    pub from: String,
    pub to: String,
    pub kind: String,
    pub task: Option<String>,
    pub text: String,
    pub status: String,
    pub detail: Option<String>,
}

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
pub struct BudgetRow {
    /// `task:<ID>` (đơn vị tokens) hoặc `provider:<tên>:<cửa sổ phút>` (đơn vị percent).
    pub scope: String,
    pub unit: String,
    pub soft: f64,
    pub hard: f64,
    /// notify | interrupt | kill (kill/interrupt không bao giờ áp cho Codex/Claude).
    pub action: String,
}

const TERM_COLS: &str = "address, name, app, cwd, labels, task, role, template, state, status, kind, msg, turn,
    reported, parent, created_ms, finished_ms, exit_code, session, link_source, handoff_state, managed, launch_spec";

fn term_from(r: &Row) -> rusqlite::Result<TerminalRow> {
    let labels: String = r.get(4)?;
    Ok(TerminalRow {
        address: r.get(0)?,
        name: r.get(1)?,
        app: r.get(2)?,
        cwd: r.get(3)?,
        labels: serde_json::from_str(&labels).unwrap_or_default(),
        task: r.get(5)?,
        role: r.get(6)?,
        template: r.get(7)?,
        state: r.get(8)?,
        status: r.get(9)?,
        kind: r.get(10)?,
        msg: r.get(11)?,
        turn: r.get(12)?,
        reported: r.get::<_, i64>(13)? != 0,
        parent: r.get(14)?,
        created_ms: r.get(15)?,
        finished_ms: r.get(16)?,
        exit_code: r.get(17)?,
        session: r.get(18)?,
        link_source: r.get(19)?,
        handoff_state: r.get(20)?,
        managed: r.get::<_, i64>(21)? != 0,
        launch_spec: r.get(22)?,
    })
}

fn inbox_from(r: &Row) -> rusqlite::Result<InboxRow> {
    Ok(InboxRow {
        id: r.get(0)?,
        ts_ms: r.get(1)?,
        terminal: r.get(2)?,
        source: r.get(3)?,
        kind: r.get(4)?,
        text: r.get(5)?,
        resolved_ms: r.get(6)?,
        reply: r.get(7)?,
    })
}

impl Store {
    /// Ghi/cập nhật thông tin terminal. Không đụng tới trạng thái 7501 và liên kết phiên đã có.
    pub fn upsert_terminal(&self, t: &TerminalRow) -> Result<()> {
        let task = t.task.clone().or_else(|| t.labels.get("agm.task").cloned());
        let role = t.role.clone().or_else(|| t.labels.get("agm.role").cloned());
        self.conn.execute(
            "INSERT INTO terminals (address, name, app, cwd, labels, task, role, template, state, reported, parent,
                 created_ms, finished_ms, exit_code, updated_ms)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9, ?10, ?11, ?12, ?13, ?14, ?12)
             ON CONFLICT(address) DO UPDATE SET
                 name = COALESCE(excluded.name, name), app = COALESCE(excluded.app, app),
                 cwd = COALESCE(excluded.cwd, cwd), labels = CASE WHEN excluded.labels = '{}' THEN labels ELSE excluded.labels END,
                 task = COALESCE(excluded.task, task), role = COALESCE(excluded.role, role),
                 template = COALESCE(excluded.template, template), state = excluded.state,
                 reported = MAX(reported, excluded.reported), parent = COALESCE(excluded.parent, parent),
                 created_ms = CASE WHEN created_ms = 0 THEN excluded.created_ms ELSE created_ms END,
                 finished_ms = COALESCE(excluded.finished_ms, finished_ms),
                 exit_code = COALESCE(excluded.exit_code, exit_code)",
            params![
                t.address,
                t.name,
                t.app,
                t.cwd,
                serde_json::to_string(&t.labels)?,
                task,
                role,
                t.template,
                t.state,
                t.reported as i64,
                t.parent,
                t.created_ms,
                t.finished_ms,
                t.exit_code
            ],
        )?;
        Ok(())
    }

    pub fn set_terminal_labels(&self, addr: &str, labels: &BTreeMap<String, String>) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET labels = ?2,
                 task = COALESCE(?3, task), role = COALESCE(?4, role) WHERE address = ?1",
            params![
                addr,
                serde_json::to_string(labels)?,
                labels.get("agm.task"),
                labels.get("agm.role")
            ],
        )?;
        Ok(())
    }

    pub fn set_terminal_status(
        &self,
        addr: &str,
        status: Option<&str>,
        kind: Option<&str>,
        msg: Option<&str>,
        turn: i64,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO terminals (address, status, kind, msg, turn, reported, updated_ms) VALUES (?1, ?2, ?3, ?4, ?5, 1, ?6)
             ON CONFLICT(address) DO UPDATE SET status = ?2, kind = ?3, msg = ?4,
                 turn = MAX(turn, ?5), reported = 1, updated_ms = ?6",
            params![addr, status, kind, msg, turn, now],
        )?;
        Ok(())
    }

    pub fn set_terminal_turn(
        &self,
        addr: &str,
        turn: i64,
        state: Option<&str>,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET turn = MAX(turn, ?2), status = COALESCE(?3, status), updated_ms = ?4 WHERE address = ?1",
            params![addr, turn, state, now],
        )?;
        Ok(())
    }

    pub fn set_terminal_app(&self, addr: &str, app: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET app = ?2 WHERE address = ?1",
            params![addr, app],
        )?;
        Ok(())
    }

    pub fn finish_terminal(
        &self,
        addr: &str,
        exit_code: Option<i64>,
        finished_ms: i64,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET state = 'finished', exit_code = COALESCE(?2, exit_code), finished_ms = ?3,
                 updated_ms = ?3 WHERE address = ?1",
            params![addr, exit_code, finished_ms],
        )?;
        Ok(())
    }

    pub fn remove_terminal(&self, addr: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET state = 'removed' WHERE address = ?1",
            [addr],
        )?;
        Ok(())
    }

    /// Kích thước cửa sổ ngữ cảnh là thuộc tính của model: ghi không phụ thuộc mốc thời gian.
    pub fn set_context_window(&self, sid: i64, window: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ctx_window = ?2 WHERE id = ?1",
            params![sid, window as i64],
        )?;
        Ok(())
    }

    pub fn link_terminal(&self, addr: &str, session: i64, source: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET session = ?2, link_source = ?3 WHERE address = ?1",
            params![addr, session, source],
        )?;
        Ok(())
    }

    /// Chỉnh `created_ms` của hàng đã có (giờ herdr thật, không phải giờ tick agm).
    pub fn set_terminal_created_ms(&self, addr: &str, created_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET created_ms = ?2 WHERE address = ?1",
            params![addr, created_ms],
        )?;
        Ok(())
    }

    pub fn set_terminal_launch(
        &self,
        addr: &str,
        managed: bool,
        launch_spec: Option<&str>,
    ) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET managed = ?2, launch_spec = COALESCE(?3, launch_spec) WHERE address = ?1",
            params![addr, managed as i64, launch_spec],
        )?;
        Ok(())
    }

    pub fn clear_terminal_managed(&self, addr: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET managed = 0 WHERE address = ?1",
            [addr],
        )?;
        Ok(())
    }

    pub fn set_terminal_handoff_state(&self, addr: &str, state: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET handoff_state = ?2 WHERE address = ?1",
            params![addr, state],
        )?;
        Ok(())
    }

    pub fn covered_since(&self, source: &str) -> Result<Option<i64>> {
        Ok(self
            .setting(&format!("covered_since:{source}"))?
            .and_then(|v| v.parse::<i64>().ok()))
    }

    pub fn update_covered_since(&self, source: &str, since_ms: i64) -> Result<()> {
        let key = format!("covered_since:{source}");
        let next = match self.covered_since(source)? {
            Some(prev) => prev.min(since_ms),
            None => since_ms,
        };
        self.set_setting(&key, &next.to_string())
    }

    pub fn terminal(&self, addr: &str) -> Result<Option<TerminalRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TERM_COLS} FROM terminals WHERE address = ?1 OR name = ?1 ORDER BY (address = ?1) DESC, (state = 'live') DESC, created_ms DESC LIMIT 1"),
                [addr],
                term_from,
            )
            .optional()?)
    }

    pub fn terminals(&self, live_only: bool) -> Result<Vec<TerminalRow>> {
        let filter = if live_only {
            "WHERE state = 'live'"
        } else {
            ""
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT {TERM_COLS} FROM terminals {filter} ORDER BY created_ms"
        ))?;
        let rows = stmt
            .query_map([], term_from)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Mở một mục hộp thư; trả None nếu đã có mục đang mở y hệt (cùng terminal, nguồn, nội dung).
    pub fn open_inbox(
        &self,
        terminal: Option<&str>,
        source: &str,
        kind: Option<&str>,
        text: &str,
        now: i64,
    ) -> Result<Option<i64>> {
        // Một câu lệnh nguyên tử: daemon và CLI cùng ghi không tạo được mục trùng.
        let n = self.conn.execute(
            "INSERT INTO inbox (ts_ms, terminal, source, kind, text)
             SELECT ?1, ?2, ?3, ?4, ?5
             WHERE NOT EXISTS (SELECT 1 FROM inbox WHERE resolved_ms IS NULL AND terminal IS ?2 AND source = ?3 AND text = ?5)",
            params![now, terminal, source, kind, text],
        )?;
        Ok((n > 0).then(|| self.conn.last_insert_rowid()))
    }

    /// Như `open_inbox` nhưng khử trùng theo (terminal, nguồn, kind) thay vì nội dung: nếu đã có mục
    /// đang mở cùng khóa thì chỉ cập nhật nội dung mới nhất (vd. % ngữ cảnh 86% → 87%) và trả None.
    pub fn open_inbox_keyed(
        &self,
        terminal: &str,
        source: &str,
        kind: &str,
        text: &str,
        now: i64,
    ) -> Result<Option<i64>> {
        let n = self.conn.execute(
            "INSERT INTO inbox (ts_ms, terminal, source, kind, text)
             SELECT ?1, ?2, ?3, ?4, ?5
             WHERE NOT EXISTS (SELECT 1 FROM inbox WHERE resolved_ms IS NULL AND terminal = ?2 AND source = ?3 AND kind = ?4)",
            params![now, terminal, source, kind, text],
        )?;
        if n > 0 {
            return Ok(Some(self.conn.last_insert_rowid()));
        }
        self.conn.execute(
            "UPDATE inbox SET text = ?4 WHERE resolved_ms IS NULL AND terminal = ?1 AND source = ?2 AND kind = ?3",
            params![terminal, source, kind, text],
        )?;
        Ok(None)
    }

    pub fn resolve_inbox_for_terminal(&self, terminal: &str, source: &str, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE inbox SET resolved_ms = ?3 WHERE terminal = ?1 AND source = ?2 AND resolved_ms IS NULL",
            params![terminal, source, now],
        )?;
        Ok(())
    }

    pub fn resolve_inbox(&self, id: i64, reply: Option<&str>, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE inbox SET resolved_ms = ?3, reply = COALESCE(?2, reply) WHERE id = ?1",
            params![id, reply, now],
        )?;
        Ok(())
    }

    /// Đóng nhiều mục hộp thư trong một transaction (không ghi đè `reply`).
    pub fn resolve_inbox_many(&self, ids: &[i64], now: i64) -> Result<()> {
        if ids.is_empty() {
            return Ok(());
        }
        self.conn.execute_batch("SAVEPOINT agm_done_bulk")?;
        let res: Result<()> = (|| {
            for &id in ids {
                self.conn.execute(
                    "UPDATE inbox SET resolved_ms = ?2 WHERE id = ?1 AND resolved_ms IS NULL",
                    params![id, now],
                )?;
            }
            Ok(())
        })();
        match res {
            Ok(()) => {
                self.conn.execute_batch("RELEASE agm_done_bulk")?;
                Ok(())
            }
            Err(e) => {
                let _ = self
                    .conn
                    .execute_batch("ROLLBACK TO agm_done_bulk; RELEASE agm_done_bulk");
                Err(e)
            }
        }
    }

    pub fn inbox_item(&self, id: i64) -> Result<Option<InboxRow>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, ts_ms, terminal, source, kind, text, resolved_ms, reply FROM inbox WHERE id = ?1",
                [id],
                inbox_from,
            )
            .optional()?)
    }

    pub fn inbox(&self, open_only: bool) -> Result<Vec<InboxRow>> {
        let filter = if open_only {
            "WHERE resolved_ms IS NULL"
        } else {
            ""
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, ts_ms, terminal, source, kind, text, resolved_ms, reply FROM inbox {filter} ORDER BY id"
        ))?;
        let rows = stmt
            .query_map([], inbox_from)?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn add_message(&self, m: &MessageRow) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO messages (ts_ms, from_t, to_t, kind, task, text, status, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![
                m.ts_ms, m.from, m.to, m.kind, m.task, m.text, m.status, m.detail
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn set_message_status(&self, id: i64, status: &str, detail: Option<&str>) -> Result<()> {
        self.conn.execute(
            "UPDATE messages SET status = ?2, detail = COALESCE(?3, detail) WHERE id = ?1",
            params![id, status, detail],
        )?;
        Ok(())
    }

    /// `limit` tin mới nhất, mới nhất trước.
    pub fn messages(&self, limit: usize) -> Result<Vec<MessageRow>> {
        let mut stmt = self.conn.prepare(
            "SELECT id, ts_ms, from_t, to_t, kind, task, text, status, detail FROM messages ORDER BY id DESC LIMIT ?1",
        )?;
        let rows = stmt
            .query_map([limit as i64], |r| {
                Ok(MessageRow {
                    id: r.get(0)?,
                    ts_ms: r.get(1)?,
                    from: r.get(2)?,
                    to: r.get(3)?,
                    kind: r.get(4)?,
                    task: r.get(5)?,
                    text: r.get(6)?,
                    status: r.get(7)?,
                    detail: r.get(8)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn log_action(
        &self,
        actor: &str,
        kind: &str,
        target: &str,
        detail: &str,
        ok: bool,
        now: i64,
    ) -> Result<()> {
        self.conn.execute(
            "INSERT INTO actions (ts_ms, actor, kind, target, detail, ok) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![now, actor, kind, target, detail, ok as i64],
        )?;
        Ok(())
    }

    pub fn upsert_approval_grant(&self, grant: &ApprovalGrant) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO approval_grants
             (lane, task, action, approved_by, approved_ms, expires_ms, revoked_ms, reason)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, NULL, ?7)",
            params![
                grant.lane,
                grant.task,
                grant.action,
                grant.approved_by,
                grant.approved_ms,
                grant.expires_ms,
                grant.reason,
            ],
        )?;
        Ok(self.conn.last_insert_rowid())
    }

    pub fn approval_grant(
        &self,
        lane: &str,
        task: &str,
        action: &str,
        now: i64,
    ) -> Result<Option<ApprovalGrant>> {
        Ok(self
            .conn
            .query_row(
                "SELECT id, lane, task, action, approved_by, approved_ms, expires_ms, revoked_ms, reason
                 FROM approval_grants
                 WHERE action = ?3 AND lane = ?1 AND task = ?2
                   AND revoked_ms IS NULL
                   AND (expires_ms IS NULL OR expires_ms > ?4)
                 ORDER BY approved_ms DESC, id DESC LIMIT 1",
                params![lane, task, action, now],
                approval_grant_from,
            )
            .optional()?)
    }

    pub fn approval_grants(&self, active_only: bool, now: i64) -> Result<Vec<ApprovalGrant>> {
        let filter = if active_only {
            " WHERE revoked_ms IS NULL AND (expires_ms IS NULL OR expires_ms > ?1)"
        } else {
            ""
        };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, lane, task, action, approved_by, approved_ms, expires_ms, revoked_ms, reason
             FROM approval_grants{filter}
             ORDER BY approved_ms DESC, id DESC"
        ))?;
        let params: [&dyn rusqlite::ToSql; 1] = [&now];
        let rows = if active_only {
            stmt.query_map(params, approval_grant_from)?
        } else {
            stmt.query_map([], approval_grant_from)?
        };
        Ok(rows.collect::<rusqlite::Result<_>>()?)
    }

    /// Thu hồi mọi grant còn hiệu lực khớp đúng lane/task/action; trả số dòng đã cập nhật.
    pub fn revoke_approval_grant(
        &self,
        lane: &str,
        task: &str,
        action: &str,
        now: i64,
    ) -> Result<usize> {
        Ok(self.conn.execute(
            "UPDATE approval_grants SET revoked_ms = ?4
             WHERE lane = ?1 AND task = ?2 AND action = ?3 AND revoked_ms IS NULL",
            params![lane, task, action, now],
        )?)
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| {
                r.get(0)
            })
            .optional()?)
    }

    pub fn set_setting(&self, key: &str, value: &str) -> Result<()> {
        self.conn.execute(
            "INSERT INTO settings (key, value) VALUES (?1, ?2) ON CONFLICT(key) DO UPDATE SET value = ?2",
            params![key, value],
        )?;
        Ok(())
    }

    pub fn set_budget(&self, b: &BudgetRow) -> Result<()> {
        self.conn.execute(
            "INSERT INTO budgets (scope, unit, soft, hard, action) VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(scope) DO UPDATE SET unit = ?2, soft = ?3, hard = ?4, action = ?5",
            params![b.scope, b.unit, b.soft, b.hard, b.action],
        )?;
        Ok(())
    }

    pub fn budgets(&self) -> Result<Vec<BudgetRow>> {
        let mut stmt = self
            .conn
            .prepare("SELECT scope, unit, soft, hard, action FROM budgets ORDER BY scope")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(BudgetRow {
                    scope: r.get(0)?,
                    unit: r.get(1)?,
                    soft: r.get(2)?,
                    hard: r.get(3)?,
                    action: r.get(4)?,
                })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn rm_budget(&self, scope: &str) -> Result<bool> {
        Ok(self
            .conn
            .execute("DELETE FROM budgets WHERE scope = ?1", [scope])?
            > 0)
    }
}
