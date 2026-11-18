CREATE TABLE IF NOT EXISTS sessions (
    id INTEGER PRIMARY KEY,
    source TEXT NOT NULL,
    path TEXT NOT NULL UNIQUE,
    session_id TEXT,
    cwd TEXT,
    cli_version TEXT,
    started_ms INTEGER,
    last_ms INTEGER,
    model TEXT,
    ctx_ts_ms INTEGER,
    ctx_used INTEGER,
    ctx_window INTEGER,
    unknown_lines INTEGER NOT NULL DEFAULT 0,
    bad_lines INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS usage (
    session INTEGER NOT NULL REFERENCES sessions(id),
    dedupe_key TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    model TEXT,
    input_uncached INTEGER NOT NULL,
    cache_read INTEGER NOT NULL,
    cache_write INTEGER NOT NULL,
    output INTEGER NOT NULL,
    reasoning INTEGER NOT NULL,
    UNIQUE(session, dedupe_key)
);

CREATE TABLE IF NOT EXISTS quota (
    provider TEXT NOT NULL,
    ts_ms INTEGER NOT NULL,
    window_minutes INTEGER NOT NULL,
    used_percent REAL NOT NULL,
    resets_at_s INTEGER NOT NULL,
    UNIQUE(provider, ts_ms, window_minutes)
);

CREATE TABLE IF NOT EXISTS turns (
    session INTEGER NOT NULL REFERENCES sessions(id),
    byte_offset INTEGER NOT NULL,
    ts_ms INTEGER NOT NULL,
    role TEXT NOT NULL,
    summary TEXT NOT NULL,
    UNIQUE(session, byte_offset)
);

CREATE TABLE IF NOT EXISTS cursors (
    path TEXT PRIMARY KEY NOT NULL,
    offset INTEGER NOT NULL,
    size INTEGER NOT NULL,
    mtime_ms INTEGER NOT NULL,
    head_hash INTEGER NOT NULL
);

