-- Migration 002: Vector Memory Schema
-- 2026-03-10
--
-- Creates tables for vector-based semantic memory using libSQL native vector support

-- Memories table: stores embeddings of conversation fragments for semantic search
CREATE TABLE IF NOT EXISTS memories (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL REFERENCES conversations(id) ON DELETE CASCADE,
    root_uri        TEXT    NOT NULL,
    content         TEXT    NOT NULL,          -- the text that was embedded
    embedding       F32_BLOB(768),            -- embedding vector (768 dims for embeddinggemma)
    author_did      TEXT    NOT NULL,
    metadata        TEXT,                      -- JSON: tags, topic, etc.
    created_at      TEXT    NOT NULL,          -- ISO 8601
    expires_at      TEXT                       -- NULL = no expiry
);

CREATE INDEX IF NOT EXISTS libsql_vector_idx ON memories (embedding);

CREATE INDEX IF NOT EXISTS idx_memories_root ON memories (root_uri);
CREATE INDEX IF NOT EXISTS idx_memories_author ON memories (author_did);
CREATE INDEX IF NOT EXISTS idx_memories_expires ON memories (expires_at);
CREATE INDEX IF NOT EXISTS idx_memories_created_at ON memories (created_at);

-- Embedding jobs table: tracks pending/complete/failed embedding generation
CREATE TABLE IF NOT EXISTS embedding_jobs (
    id              INTEGER PRIMARY KEY AUTOINCREMENT,
    conversation_id INTEGER NOT NULL UNIQUE REFERENCES conversations(id) ON DELETE CASCADE,
    status          TEXT    NOT NULL DEFAULT 'pending'
                    CHECK (status IN ('pending', 'complete', 'failed')),
    attempts        INTEGER NOT NULL DEFAULT 0,
    error           TEXT,
    created_at      TEXT    NOT NULL,          -- ISO 8601
    completed_at    TEXT
);

CREATE INDEX IF NOT EXISTS idx_embedding_jobs_status ON embedding_jobs (status);
CREATE INDEX IF NOT EXISTS idx_embedding_jobs_created_at ON embedding_jobs (created_at);

CREATE VIRTUAL TABLE IF NOT EXISTS memories_fts USING fts5(
    content,
    content=memories,
    content_rowid=id
);

CREATE TRIGGER IF NOT EXISTS memories_fts_insert AFTER INSERT ON memories BEGIN
    INSERT INTO memories_fts (rowid, content) VALUES (new.id, new.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_delete AFTER DELETE ON memories BEGIN
    INSERT INTO memories_fts (memories_fts, rowid, content) VALUES ('delete', old.id, old.content);
END;

CREATE TRIGGER IF NOT EXISTS memories_fts_update AFTER UPDATE ON memories BEGIN
    INSERT INTO memories_fts (memories_fts, rowid, content) VALUES ('delete', old.id, old.content);
    INSERT INTO memories_fts (rowid, content) VALUES (new.id, new.content);
END;
