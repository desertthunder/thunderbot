# System Architecture

ThunderBot is an event-driven AI agent for Bluesky built in Rust.

## Overview

The system consumes the AT Protocol firehose via Jetstream, maintains conversation state in SQLite, generates responses using Gemini, and posts replies via XRPC.

```text
Jetstream Firehose
       |
       v
+------------------+
|  JetstreamClient |  WebSocket consumer with zstd decompression
+------------------+
       |
       v  (filtered mentions)
+------------------+
|  EventProcessor  |  Channel-based pipeline with mpsc
+------------------+
       |
       v
+------------------+
|  StateManager    |  libSQL: conversations, identities, sessions
+------------------+
       |
       v
+------------------+
|  PromptBuilder   |  Formats thread history for Gemini
+------------------+
       |
       v
+------------------+
|  GeminiClient    |  REST API with retry logic
+------------------+
       |
       v
+------------------+
|  BskyClient      |  XRPC posting with session management
+------------------+
```

## Functional Modules

### The Listener (Sensory Input)

Connects to Jetstream WebSocket and filters the firehose. Discards all events except `app.bsky.feed.post` creates that mention the bot's DID. Pushes relevant events to an async channel for processing.

Key characteristics:

- Lightweight: no database operations or API calls
- Decoupled: prevents WebSocket timeout during slow Gemini responses
- Compressed: zstd reduces bandwidth by ~56%

### The State Manager (Memory and Context)

Bridges raw events and the AI by managing conversation history in libSQL. Extracts `thread_root_uri` to identify threads, queries for all messages in a thread, and formats data for Gemini.

Key responsibilities:

- Thread reconstruction from root URI
- Identity resolution with caching
- Message persistence with idempotency

### The Actor (Cognitive Processing)

Interfaces with Gemini for reasoning and Bluesky XRPC for posting. Sends formatted history to Gemini, receives text response, constructs valid `ReplyRef` for threading, and posts via XRPC.

Key behaviors:

- Loop prevention: skip own posts
- Silent mode: respect `<SILENT_THOUGHT>` responses
- Rate limiting: exponential backoff on 429 errors

## Database Schema

### conversations

| Column          | Type | Purpose                              |
|-----------------|------|--------------------------------------|
| id              | TEXT | Primary key (UUID)                   |
| thread_root_uri | TEXT | Thread identifier (indexed)          |
| post_uri        | TEXT | Unique message identifier            |
| parent_uri      | TEXT | Direct parent reference              |
| author_did      | TEXT | Sender DID (indexed)                 |
| role            | TEXT | 'user' or 'model'                    |
| content         | TEXT | Message text                         |
| created_at      | TEXT | ISO 8601 timestamp (indexed)         |

### identities

| Column       | Type | Purpose                    |
|--------------|------|----------------------------|
| did          | TEXT | Primary key                |
| handle       | TEXT | Bluesky handle             |
| last_updated | TEXT | TTL tracking (24h default) |

### sessions

| Column      | Type | Purpose                |
|-------------|------|------------------------|
| did         | TEXT | Primary key            |
| handle      | TEXT | Bluesky handle         |
| access_jwt  | TEXT | Short-lived token      |
| refresh_jwt | TEXT | Long-lived token       |
| updated_at  | TEXT | Token freshness        |

## Vector Schema (sqlite-vec)

| Column          | Type        | Purpose                     |
|-----------------|-------------|-----------------------------|
| id              | TEXT        | Primary key                 |
| embedding       | VECTOR(768) | Dense vector                |
| conversation_id | TEXT        | Foreign key                 |
| content         | TEXT        | Embedded text               |
| metadata        | JSON        | User DID, topics, timestamps|
| created_at      | TEXT        | ISO 8601 timestamp          |

## Data Flow

1. Jetstream delivers mention event
2. EventProcessor saves to database, sends to channel
3. Agent fetches thread history from database
4. PromptBuilder formats history for Gemini
5. GeminiClient generates response
6. BskyClient posts reply with proper threading
7. Agent saves bot response to database

## Threading Model

Bluesky threads use a DAG structure with `root` and `parent` references:

- `root`: The first post in the thread (thread identifier)
- `parent`: The immediate post being replied to

When replying, both references must be correct for the reply to appear in the thread. The agent extracts these from incoming posts and constructs proper `ReplyRef` objects.

## Session Management

Sessions are cached in three layers:

1. **Database**: Primary persistent storage (Turso-compatible)
2. **File**: Fallback at `.bsky_session.json`
3. **Memory**: `Arc<RwLock<Session>>` for thread-safe access

Load order: database -> file -> environment credentials.
