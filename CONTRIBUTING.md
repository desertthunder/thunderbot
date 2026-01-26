# CONTRIBUTING

Contributing guidelines for ThunderBot, a Stateful AI agent for Bluesky (AT Protocol) built in Rust.

## Project Overview

Event-driven architecture consuming AT Protocol firehose via Jetstream, with libSQL persistence, Gemini 3 API reasoning, and LanceDB vector memory for RAG. Cargo workspace with `cli` (binary) and `core` (library) crates.

## Essential Commands

```bash
# Build
cargo build --release

# Code quality
cargo fmt && cargo clippy

# Run CLI
cargo run --bin thunderbot -- <command>

# Key commands
thunderbot jetstream listen [--filter-did <DID>]
thunderbot bsky login
thunderbot ai prompt "text"
thunderbot db migrate
thunderbot serve
```

## Web Dashboard

ThunderBot includes a web-based control deck for monitoring and managing the bot. See `docs/web-dashboard.md` for full documentation.

Quick start:

```bash
export DASHBOARD_TOKEN=your-secure-token
thunderbot serve


Dashboard available at `http://127.0.0.1:3000`

```

## Environment Variables (.env)

```bash
DATABASE_URL=file:bot.db
BSKY_HANDLE=your.bsky.social
BSKY_APP_PASSWORD=app-password
PDS_HOST=https://bsky.social
GEMINI_API_KEY=your-key
GEMINI_MODEL=gemini-3-pro-preview
DASHBOARD_TOKEN=changeme
```

## Project Structure

```sh
crates/cli/       # CLI entry point, command handlers
crates/core/      # Core logic
  src/agent.rs          # Orchestration (coordinates all modules)
  src/processor.rs      # Event processing pipeline
  src/jetstream/        # WebSocket firehose client
  src/bsky/             # XRPC client for Bluesky
  src/db/               # libSQL repository + thread context
  src/gemini/           # API client + prompt builder
  src/vector/           # LanceDB + embeddings + retrieval
  src/web/              # Axum web server + HTMX dashboard
  src/agent.rs          # Orchestration (coordinates all modules)
  src/processor.rs      # Event processing pipeline
  src/jetstream/        # WebSocket firehose client
  src/bsky/             # XRPC client for Bluesky
  src/db/               # libSQL repository + thread context
  src/gemini/           # API client + prompt builder
  src/vector/           # LanceDB + embeddings + retrieval
  migrations/           # SQL files (001_init.sql, 002_add_session.sql)
```

## Code Conventions

**Error Handling**: Use `anyhow::Result<T>`, chain with `.context()`:

```rust
use anyhow::{Context, Result};
client.get_post(uri).await.context("Failed to fetch post")?;
```

**Async**: `tokio` runtime, `#[tokio::main]` entry point, `tokio::time::sleep()`

**Shared State**: `Arc<T>` for immutable, `Arc<RwLock<T>>` for mutable (sessions)

**Database**: Repository pattern, `DatabaseRepository` trait, `Builder::new_local(url).build().await`

**Retries**: Exponential backoff for transient failures (3 attempts max)

**Logging**: `tracing` crate, levels via `-v`/-`vv`/`-vvv` flags

## Important Gotchas

**Sessions**: Cached in database, file (`.bsky_session.json`), and memory. Load order: DB → file → env. Uses `Arc<RwLock<Session>>`.

**Thread Context**: All replies share `thread_root_uri`. Post structure has `reply.root.*` (thread root) and `reply.parent.*` (direct parent). New posts are their own root.

**Loop Prevention**: Agent checks `author_did == own_did` before replying. Silent mode: `<SILENT_THOUGHT>` response means don't post.

**Rate Limiting**: Bluesky 429 errors retry with 2s/4s/8s backoff (max 3 retries).

**Jetstream**: Filter at source with `wantedCollections=app.bsky.feed.post`. Client-side DID filtering is CPU-heavy.

**Vector**: LanceDB stores embeddings with metadata. Backfill respects Gemini API rate limits.

## Database Schema

- `conversations`: Messages (id, thread_root_uri, post_uri, parent_uri, author_did, role, content, created_at)
- `identities`: DID → handle cache (did, handle, last_updated)
- `sessions`: Session tokens (did, handle, access_jwt, refresh_jwt, updated_at)

Indexes on: `thread_root_uri`, `author_did`, `created_at`, `last_updated`

## Key Modules

**agent.rs**: Coordinates all modules, processes mentions, posts replies with retry logic

**jetstream/client.rs**: WebSocket via `tokio-tungstenite`, handles binary/text/close messages

**bsky/client.rs**: XRPC with `reqwest`, session management, `create_post`, `reply_to_post`, `resolve_handle`

**db/repository.rs**: libSQL operations, migrations via `include_str!`, async trait methods

**gemini/client.rs**: HTTP client with 60s timeout, retries on 5xx, extracts text from response parts

**vector/retrieval.rs**: Semantic search with LanceDB, filters by author/role, returns scored results

**web/server.rs**: Axum HTTP server with routing, authentication middleware
**web/handlers.rs**: Request handlers for dashboard endpoints
**web/templates.rs**: Maud HTML templates with Pico CSS styling
**web/auth.rs**: Bearer token authentication middleware

## Dependencies

tokio, reqwest, libsql, lancedb, tokio-tungstenite, clap, serde, tracing, anyhow, chrono, async-compression (zstd), axum, maud, tower-http

## Workspace Config

Resolver v2, shared dependencies in root `Cargo.toml`, workspace Clippy lints configured. `rustfmt.toml`: max_width=120, compressed params, field init shorthand.
