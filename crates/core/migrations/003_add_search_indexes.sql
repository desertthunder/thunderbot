-- Full-text search table for conversations
CREATE VIRTUAL TABLE IF NOT EXISTS conversations_fts
USING fts5(id, thread_root_uri, author_did, role, content, content='conversations', tokenize='porter');

-- Triggers for FTS sync on INSERT
CREATE TRIGGER IF NOT EXISTS conversations_ai AFTER INSERT ON conversations BEGIN
  INSERT INTO conversations_fts(id, thread_root_uri, author_did, role, content)
  VALUES (new.id, new.thread_root_uri, new.author_did, new.role, new.content);
END;

-- Triggers for FTS sync on DELETE
CREATE TRIGGER IF NOT EXISTS conversations_ad AFTER DELETE ON conversations BEGIN
  DELETE FROM conversations_fts WHERE id = old.id;
END;

-- Triggers for FTS sync on UPDATE
CREATE TRIGGER IF NOT EXISTS conversations_au AFTER UPDATE ON conversations BEGIN
  DELETE FROM conversations_fts WHERE id = old.id;
  INSERT INTO conversations_fts(id, thread_root_uri, author_did, role, content)
  VALUES (new.id, new.thread_root_uri, new.author_did, new.role, new.content);
END;
