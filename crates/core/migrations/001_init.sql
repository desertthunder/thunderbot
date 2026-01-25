CREATE TABLE IF NOT EXISTS conversations (
    id TEXT PRIMARY KEY,
    thread_root_uri TEXT NOT NULL,
    post_uri TEXT NOT NULL UNIQUE,
    parent_uri TEXT,
    author_did TEXT NOT NULL,
    role TEXT NOT NULL CHECK(role IN ('user', 'model')),
    content TEXT NOT NULL,
    created_at TEXT NOT NULL
);

CREATE TABLE IF NOT EXISTS identities (
    did TEXT PRIMARY KEY,
    handle TEXT NOT NULL UNIQUE,
    last_updated TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_conversations_thread_root ON conversations(thread_root_uri);
CREATE INDEX IF NOT EXISTS idx_conversations_author_did ON conversations(author_did);
CREATE INDEX IF NOT EXISTS idx_conversations_created_at ON conversations(created_at);
CREATE INDEX IF NOT EXISTS idx_identities_last_updated ON identities(last_updated);
