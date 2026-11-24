-- v5: delegated user approvals for agent actions.
CREATE TABLE IF NOT EXISTS approval_grants (
    id INTEGER PRIMARY KEY,
    lane TEXT,
    task TEXT,
    action TEXT NOT NULL,
    approved_by TEXT NOT NULL,
    approved_ms INTEGER NOT NULL,
    expires_ms INTEGER,
    revoked_ms INTEGER,
    reason TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS approval_grants_lookup
    ON approval_grants(lane, task, action, revoked_ms, expires_ms);
