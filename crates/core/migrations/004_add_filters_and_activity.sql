-- Muted authors table for filtering
CREATE TABLE IF NOT EXISTS muted_authors (
    did TEXT PRIMARY KEY,
    muted_at TEXT NOT NULL,
    muted_by TEXT NOT NULL
);

-- Filter presets for saving filter configurations
CREATE TABLE IF NOT EXISTS filter_presets (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    filters_json TEXT NOT NULL,
    created_at TEXT NOT NULL,
    created_by TEXT NOT NULL
);

-- Activity log for tracking bot actions
CREATE TABLE IF NOT EXISTS activity_log (
    id TEXT PRIMARY KEY,
    action_type TEXT NOT NULL,
    description TEXT NOT NULL,
    thread_uri TEXT,
    metadata_json TEXT,
    created_at TEXT NOT NULL
);

-- Indexes for performance
CREATE INDEX IF NOT EXISTS idx_activity_log_created_at ON activity_log(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_activity_log_type ON activity_log(action_type, created_at DESC);
CREATE INDEX IF NOT EXISTS idx_muted_authors_muted_at ON muted_authors(muted_at DESC);
