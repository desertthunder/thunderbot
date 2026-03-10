-- Migration 001: Initial schema
-- 2026-03-10
--
-- Creates tables for conversations, identities, failed_events, and cursor_state

-- Conversations table: stores every post in a thread the bot participates in
CREATE TABLE IF NOT EXISTS conversations (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    root_uri    TEXT    NOT NULL,     -- AT URI of thread root
    post_uri    TEXT    NOT NULL UNIQUE, -- AT URI of this post (idempotency key)
    parent_uri  TEXT,                 -- AT URI of immediate parent (NULL if root)
    author_did  TEXT    NOT NULL,     -- DID of the post author
    role        TEXT    NOT NULL CHECK (role IN ('user', 'model')),
    content     TEXT    NOT NULL,     -- post text (UTF-8)
    cid         TEXT,                 -- CID of the record (for strong refs)
    created_at  TEXT    NOT NULL      -- ISO 8601 timestamp from the post
);

CREATE INDEX IF NOT EXISTS idx_conversations_root ON conversations(root_uri);
CREATE INDEX IF NOT EXISTS idx_conversations_author ON conversations(author_did);
CREATE INDEX IF NOT EXISTS idx_conversations_created_at ON conversations(created_at);

-- Identities table: DID -> Handle cache with TTL-based staleness
CREATE TABLE IF NOT EXISTS identities (
    did          TEXT PRIMARY KEY,
    handle       TEXT NOT NULL,
    display_name TEXT,
    last_updated TEXT NOT NULL  -- ISO 8601
);

CREATE INDEX IF NOT EXISTS idx_identities_last_updated ON identities(last_updated);

-- Failed events table: dead-letter table for events that fail processing
CREATE TABLE IF NOT EXISTS failed_events (
    id          INTEGER PRIMARY KEY AUTOINCREMENT,
    post_uri    TEXT    NOT NULL,
    event_json  TEXT    NOT NULL,
    error       TEXT    NOT NULL,
    attempts    INTEGER NOT NULL DEFAULT 1,
    created_at  TEXT    NOT NULL,
    last_tried  TEXT    NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_failed_events_post_uri ON failed_events(post_uri);
CREATE INDEX IF NOT EXISTS idx_failed_events_created_at ON failed_events(created_at);

-- Cursor state table: persists the last processed Jetstream cursor
CREATE TABLE IF NOT EXISTS cursor_state (
    id      INTEGER PRIMARY KEY CHECK (id = 1),  -- singleton row
    time_us INTEGER NOT NULL,
    updated TEXT    NOT NULL
);
