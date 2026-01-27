-- Response preview queue
CREATE TABLE IF NOT EXISTS response_queue (
    id TEXT PRIMARY KEY,
    thread_uri TEXT NOT NULL,
    parent_uri TEXT NOT NULL,
    parent_cid TEXT NOT NULL,
    root_uri TEXT NOT NULL,
    root_cid TEXT NOT NULL,
    content TEXT NOT NULL,
    status TEXT NOT NULL, -- 'pending', 'approved', 'edited', 'discarded'
    created_at TEXT NOT NULL,
    expires_at TEXT
);
CREATE INDEX IF NOT EXISTS idx_response_queue_status ON response_queue(status, created_at DESC);

-- Quiet hours windows
CREATE TABLE IF NOT EXISTS quiet_hours (
    id TEXT PRIMARY KEY,
    day_of_week INTEGER NOT NULL, -- 0-6 (Sunday-Saturday)
    start_time TEXT NOT NULL, -- HH:MM format
    end_time TEXT NOT NULL, -- HH:MM format
    timezone TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1
);

-- Reply limits configuration
CREATE TABLE IF NOT EXISTS reply_limits_config (
    id TEXT PRIMARY KEY,
    max_replies_per_thread INTEGER NOT NULL DEFAULT 10,
    cooldown_seconds INTEGER NOT NULL DEFAULT 60,
    max_replies_per_author_hour INTEGER NOT NULL DEFAULT 5,
    updated_at TEXT NOT NULL
);

-- Blocklist (enhanced mute)
CREATE TABLE IF NOT EXISTS blocklist (
    did TEXT PRIMARY KEY,
    blocked_at TEXT NOT NULL,
    blocked_by TEXT NOT NULL,
    reason TEXT,
    expires_at TEXT, -- NULL = permanent
    block_type TEXT NOT NULL -- 'author', 'domain'
);
CREATE INDEX IF NOT EXISTS idx_blocklist_expires ON blocklist(expires_at);

-- Dead letter queue
CREATE TABLE IF NOT EXISTS dead_letter_queue (
    id TEXT PRIMARY KEY,
    event_json TEXT NOT NULL,
    error_message TEXT NOT NULL,
    retry_count INTEGER NOT NULL DEFAULT 0,
    last_retry_at TEXT,
    created_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_dlq_created ON dead_letter_queue(created_at DESC);
CREATE INDEX IF NOT EXISTS idx_dlq_retries ON dead_letter_queue(retry_count, created_at);

-- Rate limit history (for graphs)
CREATE TABLE IF NOT EXISTS rate_limit_history (
    id TEXT PRIMARY KEY,
    endpoint TEXT NOT NULL,
    limit_remaining INTEGER NOT NULL,
    limit_reset TEXT NOT NULL,
    recorded_at TEXT NOT NULL
);
CREATE INDEX IF NOT EXISTS idx_rate_limit_endpoint ON rate_limit_history(endpoint, recorded_at DESC);

-- Session metadata for proactive refresh
CREATE TABLE IF NOT EXISTS session_metadata (
    did TEXT PRIMARY KEY,
    access_jwt_expires_at TEXT NOT NULL,
    refresh_jwt_expires_at TEXT NOT NULL,
    last_refresh_at TEXT,
    force_refresh_before TEXT
);
