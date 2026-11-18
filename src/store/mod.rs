use std::path::Path;

use anyhow::Result;
use rusqlite::{Connection, OptionalExtension, params};

use crate::model::{
    ContextSnapshot, QuotaSnapshot, Role, SessionMeta, Source, TurnSummary, UsageRecord,
};

pub struct Store {
    conn: Connection,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Cursor {
    pub path: String,
    pub offset: u64,
    pub size: u64,
    pub mtime_ms: i64,
    pub head_hash: u64,
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SessionStats {
    pub unknown_lines: u64,
    pub bad_lines: u64,
}

impl Store {
    pub fn open(path: &Path) -> Result<Self> {
        if let Some(parent) = path.parent().filter(|p| !p.as_os_str().is_empty()) {
            std::fs::create_dir_all(parent)?;
        }
        Self::initialize(Connection::open(path)?)
    }

    pub fn open_in_memory() -> Result<Self> {
        Self::initialize(Connection::open_in_memory()?)
    }

    fn initialize(conn: Connection) -> Result<Self> {
        conn.execute_batch("PRAGMA journal_mode=WAL; PRAGMA foreign_keys=ON;")?;
        conn.execute_batch(include_str!("schema.sql"))?;
        Ok(Self { conn })
    }

    pub fn session_id_for_path(&self, source: Source, path: &str) -> Result<i64> {
        self.conn.execute(
            "INSERT INTO sessions(source, path) VALUES (?1, ?2) ON CONFLICT(path) DO NOTHING",
            params![source.as_str(), path],
        )?;
        Ok(self
            .conn
            .query_row("SELECT id FROM sessions WHERE path=?1", [path], |row| {
                row.get(0)
            })?)
    }

    pub fn set_meta(&self, sid: i64, meta: &SessionMeta) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET session_id=?2, cwd=?3, cli_version=?4, started_ms=?5 WHERE id=?1",
            params![
                sid,
                meta.session_id,
                meta.cwd,
                meta.cli_version,
                meta.started_ms
            ],
        )?;
        Ok(())
    }

    pub fn set_model(&self, sid: i64, model: &str) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET model=?2 WHERE id=?1",
            params![sid, model],
        )?;
        Ok(())
    }

    pub fn upsert_usage(&self, sid: i64, rec: &UsageRecord) -> Result<()> {
        self.with_savepoint(|tx| {
            tx.execute(
                "INSERT INTO usage(session, dedupe_key, ts_ms, model, input_uncached,
                               cache_read, cache_write, output, reasoning)
             VALUES (?1, ?2, ?3, ?4, ?5, ?6, ?7, ?8, ?9)
             ON CONFLICT(session, dedupe_key) DO UPDATE SET
                ts_ms=excluded.ts_ms, model=excluded.model,
                input_uncached=excluded.input_uncached, cache_read=excluded.cache_read,
                cache_write=excluded.cache_write, output=excluded.output,
                reasoning=excluded.reasoning",
                params![
                    sid,
                    rec.dedupe_key,
                    rec.ts_ms,
                    rec.model,
                    i64::try_from(rec.usage.input_uncached)?,
                    i64::try_from(rec.usage.cache_read)?,
                    i64::try_from(rec.usage.cache_write)?,
                    i64::try_from(rec.usage.output)?,
                    i64::try_from(rec.usage.reasoning)?
                ],
            )?;
            Self::refresh_last_ms(tx, sid)?;
            Ok(())
        })
    }

    pub fn insert_quota(&self, snap: &QuotaSnapshot) -> Result<()> {
        self.with_savepoint(|tx| {
            for window in &snap.windows {
                tx.execute(
                    "INSERT INTO quota(provider, ts_ms, window_minutes, used_percent, resets_at_s)
                 VALUES (?1, ?2, ?3, ?4, ?5)
                 ON CONFLICT(provider, ts_ms, window_minutes) DO NOTHING",
                    params![
                        snap.provider,
                        snap.ts_ms,
                        window.window_minutes,
                        window.used_percent,
                        window.resets_at_s
                    ],
                )?;
            }
            Ok(())
        })
    }

    pub fn set_context(&self, sid: i64, ctx: &ContextSnapshot) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET ctx_ts_ms=?2, ctx_used=?3, ctx_window=?4
             WHERE id=?1 AND (ctx_ts_ms IS NULL OR ctx_ts_ms <= ?2)",
            params![
                sid,
                ctx.ts_ms,
                i64::try_from(ctx.used_tokens)?,
                ctx.window_tokens.map(i64::try_from).transpose()?
            ],
        )?;
        Ok(())
    }

    pub fn insert_turn(&self, sid: i64, t: &TurnSummary) -> Result<()> {
        let role = match t.role {
            Role::User => "user",
            Role::Assistant => "assistant",
            Role::Tool => "tool",
        };
        let summary: String = t.summary.chars().take(200).collect();
        self.with_savepoint(|tx| {
            tx.execute(
                "INSERT INTO turns(session, byte_offset, ts_ms, role, summary)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(session, byte_offset) DO NOTHING",
                params![sid, i64::try_from(t.byte_offset)?, t.ts_ms, role, summary],
            )?;
            Self::refresh_last_ms(tx, sid)?;
            Ok(())
        })
    }

    fn with_savepoint(&self, write: impl FnOnce(&Connection) -> Result<()>) -> Result<()> {
        self.conn.execute_batch("SAVEPOINT agm_w")?;
        match write(&self.conn) {
            Ok(()) => {
                self.conn.execute_batch("RELEASE agm_w")?;
                Ok(())
            }
            Err(error) => {
                self.conn
                    .execute_batch("ROLLBACK TO agm_w; RELEASE agm_w")?;
                Err(error)
            }
        }
    }

    fn refresh_last_ms(conn: &Connection, sid: i64) -> Result<()> {
        conn.execute(
            "UPDATE sessions SET last_ms=(
                SELECT MAX(ts_ms) FROM (
                    SELECT ts_ms FROM usage WHERE session=?1
                    UNION ALL SELECT ts_ms FROM turns WHERE session=?1
                )
             ) WHERE id=?1",
            [sid],
        )?;
        Ok(())
    }

    pub fn add_line_stats(&self, sid: i64, unknown: u64, bad: u64) -> Result<()> {
        self.conn.execute(
            "UPDATE sessions SET unknown_lines=unknown_lines+?2, bad_lines=bad_lines+?3 WHERE id=?1",
            params![sid, i64::try_from(unknown)?, i64::try_from(bad)?],
        )?;
        Ok(())
    }

    pub fn cursor(&self, path: &str) -> Result<Option<Cursor>> {
        Ok(self
            .conn
            .query_row(
                "SELECT path, offset, size, mtime_ms, head_hash FROM cursors WHERE path=?1",
                [path],
                |row| {
                    Ok(Cursor {
                        path: row.get(0)?,
                        offset: row.get::<_, i64>(1)? as u64,
                        size: row.get::<_, i64>(2)? as u64,
                        mtime_ms: row.get(3)?,
                        head_hash: row.get::<_, i64>(4)? as u64,
                    })
                },
            )
            .optional()?)
    }

    pub fn save_cursor(&self, c: &Cursor) -> Result<()> {
        // SQLite integers are signed; preserve all 64 hash bits with a reversible cast.
        self.conn.execute(
            "INSERT INTO cursors(path, offset, size, mtime_ms, head_hash)
             VALUES (?1, ?2, ?3, ?4, ?5)
             ON CONFLICT(path) DO UPDATE SET offset=excluded.offset, size=excluded.size,
                mtime_ms=excluded.mtime_ms, head_hash=excluded.head_hash",
            params![
                c.path,
                i64::try_from(c.offset)?,
                i64::try_from(c.size)?,
                c.mtime_ms,
                c.head_hash as i64
            ],
        )?;
        Ok(())
    }

    pub fn conn(&self) -> &Connection {
        &self.conn
    }
}
