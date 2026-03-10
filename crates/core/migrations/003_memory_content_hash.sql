-- Migration 003: Memory content hash and indexing
-- 2026-03-10
--
-- Adds deterministic hash-based deduplication support for memories.

ALTER TABLE memories ADD COLUMN content_hash TEXT;

CREATE INDEX IF NOT EXISTS idx_memories_root_hash ON memories(root_uri, content_hash);
