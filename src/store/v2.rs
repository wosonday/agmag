//! API cho các bảng v2: terminals, messages, inbox, budgets, actions, settings.

use std::collections::BTreeMap;

use anyhow::Result;
use rusqlite::{params, OptionalExtension, Row};
use serde::Serialize;

use super::Store;

#[derive(Debug, Clone, Default, PartialEq, Serialize)]
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
    reported, parent, created_ms, finished_ms, exit_code, session, link_source";

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
                 cwd = COALESCE(excluded.cwd, cwd), labels = excluded.labels,
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
            params![addr, serde_json::to_string(labels)?, labels.get("agm.task"), labels.get("agm.role")],
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

    pub fn set_terminal_turn(&self, addr: &str, turn: i64, state: Option<&str>, now: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET turn = MAX(turn, ?2), status = COALESCE(?3, status), updated_ms = ?4 WHERE address = ?1",
            params![addr, turn, state, now],
        )?;
        Ok(())
    }

    pub fn set_terminal_app(&self, addr: &str, app: &str) -> Result<()> {
        self.conn.execute("UPDATE terminals SET app = ?2 WHERE address = ?1", params![addr, app])?;
        Ok(())
    }

    pub fn finish_terminal(&self, addr: &str, exit_code: Option<i64>, finished_ms: i64) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET state = 'finished', exit_code = COALESCE(?2, exit_code), finished_ms = ?3,
                 updated_ms = ?3 WHERE address = ?1",
            params![addr, exit_code, finished_ms],
        )?;
        Ok(())
    }

    pub fn remove_terminal(&self, addr: &str) -> Result<()> {
        self.conn.execute("UPDATE terminals SET state = 'removed' WHERE address = ?1", [addr])?;
        Ok(())
    }

    /// Kích thước cửa sổ ngữ cảnh là thuộc tính của model: ghi không phụ thuộc mốc thời gian.
    pub fn set_context_window(&self, sid: i64, window: u64) -> Result<()> {
        self.conn.execute("UPDATE sessions SET ctx_window = ?2 WHERE id = ?1", params![sid, window as i64])?;
        Ok(())
    }

    pub fn link_terminal(&self, addr: &str, session: i64, source: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE terminals SET session = ?2, link_source = ?3 WHERE address = ?1",
            params![addr, session, source],
        )?;
        Ok(())
    }

    pub fn terminal(&self, addr: &str) -> Result<Option<TerminalRow>> {
        Ok(self
            .conn
            .query_row(
                &format!("SELECT {TERM_COLS} FROM terminals WHERE address = ?1 OR name = ?1 ORDER BY state = 'live' DESC LIMIT 1"),
                [addr],
                term_from,
            )
            .optional()?)
    }

    pub fn terminals(&self, live_only: bool) -> Result<Vec<TerminalRow>> {
        let filter = if live_only { "WHERE state = 'live'" } else { "" };
        let mut stmt = self.conn.prepare(&format!("SELECT {TERM_COLS} FROM terminals {filter} ORDER BY created_ms"))?;
        let rows = stmt.query_map([], term_from)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    /// Mở một mục hộp thư; trả None nếu đã có mục đang mở y hệt (cùng terminal, nguồn, nội dung).
    pub fn open_inbox(&self, terminal: Option<&str>, source: &str, kind: Option<&str>, text: &str, now: i64) -> Result<Option<i64>> {
        let exists: bool = self.conn.query_row(
            "SELECT EXISTS(SELECT 1 FROM inbox WHERE resolved_ms IS NULL AND terminal IS ?1 AND source = ?2 AND text = ?3)",
            params![terminal, source, text],
            |r| r.get(0),
        )?;
        if exists {
            return Ok(None);
        }
        self.conn.execute(
            "INSERT INTO inbox (ts_ms, terminal, source, kind, text) VALUES (?1, ?2, ?3, ?4, ?5)",
            params![now, terminal, source, kind, text],
        )?;
        Ok(Some(self.conn.last_insert_rowid()))
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
        let filter = if open_only { "WHERE resolved_ms IS NULL" } else { "" };
        let mut stmt = self.conn.prepare(&format!(
            "SELECT id, ts_ms, terminal, source, kind, text, resolved_ms, reply FROM inbox {filter} ORDER BY id"
        ))?;
        let rows = stmt.query_map([], inbox_from)?.collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn add_message(&self, m: &MessageRow) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO messages (ts_ms, from_t, to_t, kind, task, text, status, detail)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8)",
            params![m.ts_ms, m.from, m.to, m.kind, m.task, m.text, m.status, m.detail],
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

    pub fn log_action(&self, actor: &str, kind: &str, target: &str, detail: &str, ok: bool, now: i64) -> Result<()> {
        self.conn.execute(
            "INSERT INTO actions (ts_ms, actor, kind, target, detail, ok) VALUES (?1, ?2, ?3, ?4, ?5, ?6)",
            params![now, actor, kind, target, detail, ok as i64],
        )?;
        Ok(())
    }

    pub fn setting(&self, key: &str) -> Result<Option<String>> {
        Ok(self
            .conn
            .query_row("SELECT value FROM settings WHERE key = ?1", [key], |r| r.get(0))
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
        let mut stmt = self.conn.prepare("SELECT scope, unit, soft, hard, action FROM budgets ORDER BY scope")?;
        let rows = stmt
            .query_map([], |r| {
                Ok(BudgetRow { scope: r.get(0)?, unit: r.get(1)?, soft: r.get(2)?, hard: r.get(3)?, action: r.get(4)? })
            })?
            .collect::<rusqlite::Result<_>>()?;
        Ok(rows)
    }

    pub fn rm_budget(&self, scope: &str) -> Result<bool> {
        Ok(self.conn.execute("DELETE FROM budgets WHERE scope = ?1", [scope])? > 0)
    }
}
