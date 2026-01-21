CREATE TABLE IF NOT EXISTS meta_kv (
    mode_id TEXT NOT NULL,
    key TEXT NOT NULL,
    value TEXT NOT NULL CHECK (json_valid(value)),
    created_at INTEGER NOT NULL DEFAULT (unixepoch()),
    updated_at INTEGER NOT NULL DEFAULT (unixepoch()),
    PRIMARY KEY (mode_id, key)
);

CREATE INDEX idx_meta_kv_mode_latest ON meta_kv (mode_id, updated_at DESC);
