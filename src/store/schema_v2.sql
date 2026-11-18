-- v2: terminal của herdr-core, tin nhắn giữa agent, hộp thư người dùng, ngân sách, nhật ký can thiệp, xoay vòng.
CREATE TABLE IF NOT EXISTS terminals (
    address TEXT PRIMARY KEY NOT NULL,
    name TEXT,
    app TEXT,
    cwd TEXT,
    labels TEXT NOT NULL DEFAULT '{}',
    task TEXT,
    role TEXT,
    template TEXT,
    state TEXT NOT NULL DEFAULT 'live',
    status TEXT,
    kind TEXT,
    msg TEXT,
    turn INTEGER NOT NULL DEFAULT 0,
    reported INTEGER NOT NULL DEFAULT 0,
    parent TEXT,
    created_ms INTEGER NOT NULL DEFAULT 0,
    finished_ms INTEGER,
    exit_code INTEGER,
    session INTEGER REFERENCES sessions(id),
    link_source TEXT,
    updated_ms INTEGER NOT NULL DEFAULT 0
);

CREATE TABLE IF NOT EXISTS messages (
    id INTEGER PRIMARY KEY,
    ts_ms INTEGER NOT NULL,
    from_t TEXT NOT NULL,
    to_t TEXT NOT NULL,
    kind TEXT NOT NULL,
    task TEXT,
    text TEXT NOT NULL,
    status TEXT NOT NULL,
    detail TEXT
);

CREATE TABLE IF NOT EXISTS inbox (
    id INTEGER PRIMARY KEY,
    ts_ms INTEGER NOT NULL,
    terminal TEXT,
    source TEXT NOT NULL,
    kind TEXT,
    text TEXT NOT NULL,
    resolved_ms INTEGER,
    reply TEXT
);

CREATE TABLE IF NOT EXISTS budgets (
    scope TEXT PRIMARY KEY NOT NULL,
    unit TEXT NOT NULL,
    soft REAL NOT NULL,
    hard REAL NOT NULL,
    action TEXT NOT NULL DEFAULT 'notify'
);

CREATE TABLE IF NOT EXISTS actions (
    id INTEGER PRIMARY KEY,
    ts_ms INTEGER NOT NULL,
    actor TEXT NOT NULL,
    kind TEXT NOT NULL,
    target TEXT NOT NULL,
    detail TEXT NOT NULL,
    ok INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS rotations (
    id INTEGER PRIMARY KEY,
    task TEXT,
    old_terminal TEXT NOT NULL,
    new_terminal TEXT,
    state TEXT NOT NULL,
    ctx_pct REAL,
    head_before TEXT,
    turn_before INTEGER,
    commit_sha TEXT,
    detail TEXT,
    requested_ms INTEGER,
    updated_ms INTEGER NOT NULL
);

CREATE TABLE IF NOT EXISTS settings (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL
);
