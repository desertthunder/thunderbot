-- Session table for storing Bluesky authentication tokens
CREATE TABLE IF NOT EXISTS sessions (
    did TEXT PRIMARY KEY,
    handle TEXT NOT NULL,
    access_jwt TEXT NOT NULL,
    refresh_jwt TEXT NOT NULL,
    updated_at TEXT NOT NULL
);
