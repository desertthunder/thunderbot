# Stateful Agent Bot Specification

This document outlines the engineering specification for a **Stateful AI Agent** on the AT Protocol (Bluesky), utilizing **GLM-5** for reasoning, **Rust** for infrastructure, and **HTMX + Pico CSS** for the management dashboard.

## System Architecture

- **Runtime**: Provider-agnostic Rust binary. Deployable to Fly.io (containers), Docker, or any container runtime; Can be self-hosted.
- **Ingestion**: Manual WebSocket client consuming Jetstream via `tokio-tungstenite` with zstd decompression.
- **State Store**: SQLite (embedded). Portable, single-file database with replication support.
- **Vector Store**: SQLite. Provider-agnostic vector similarity search.
- **Cognitive Engine**: Z.ai's GLM5 via REST API using `reqwest` crate.
- **Bluesky Integration**: Manual XRPC client via `reqwest` for full control over authentication and posting.
- **Frontend**: Server-side rendered HTML (HTMX) with Pico CSS, served directly from the application.
- **CLI**: `clap`-based command-line interface for ad-hoc testing and system interaction.

## Overview

Part 1: Milestones 1-6

Part 2: Milestone 7

## Milestone 1: The Foundation, Ingestion Layer, and CLI

**Goal**: Establish the Rust application environment, CLI framework, and successfully consume the Bluesky Jetstream firehose.

**Definition of Done**:

1. A Rust binary is built and successfully runnable locally and in a container.
2. CLI framework is functional with global options and subcommand routing.
3. A persistent WebSocket connection to Jetstream is maintained via manual `tokio-tungstenite` client.
4. Incoming "Mentions" are filtered from the firehose and logged; 99% of noise is discarded.
5. The system handles WebSocket disconnections with automatic backoff and reconnection strategies.
6. Jetstream commands are testable via CLI.

**Tasks**:

1. **Project Initialization and Tooling**
    - **Requirements**:
        - Add core dependencies: `tokio` (async runtime), `serde` (serialization), `tracing` (logging).
        - Add `clap` crate with `derive` feature for CLI argument parsing.
        - Add `owo-colors` crate for colored console output.
        - Add `config` crate for layered configuration (defaults, file, env vars).
        - Support `.env` & `.toml` config files for local development via `dotenvy` & `toml` with serde
          feature.

2. **CLI Framework Setup**
    - **Requirements**:
        - Define top-level `Cli` struct with global options (config path, verbosity, output format).
        - Implement subcommand enum for each feature area (`jetstream`, `bsky`, `db`, `ai`, `serve`).
        - Support `--json` flag for machine-readable output across all commands.
        - Implement `serve` subcommand as the main daemon entry point.
        - Implement `config show` and `config validate` commands.

3. **Jetstream WebSocket Client**
    - **Requirements**:
        - Add `tokio-tungstenite` crate with `rustls-tls-native-roots` feature for WebSocket.
        - Add `async-compression` with `zstd` feature for frame decompression.
        - Define Rust structs (serde) for Jetstream JSON protocol: `JetstreamEvent`, `CommitData`, `IdentityData`, `AccountData`.
        - Connect to `wss://jetstream2.us-east.bsky.network/subscribe` with query params for filtering.
        - Implement reconnection loop with exponential backoff (1s initial, 60s max, 2x factor, jitter).
        - Handle zstd-compressed frames via `compress=true` query param or `Socket-Encoding: zstd` header.

4. **Jetstream Filtering Logic**
    - **Requirements**:
        - Pass `wantedCollections=app.bsky.feed.post` query parameter to filter at source.
        - Parse incoming JSON into typed `CommitData` structs with `operation`, `collection`, `rkey`, `record`, `cid`.
        - Implement "Facet Finding" logic: Access the `facets` array in the post record and match `app.bsky.richtext.facet#mention` against the Bot's DID.
        - Discard all events that do not match the Bot's DID to save CPU.
        - Log matched mentions with structured tracing including `time_us` for cursor tracking.

5. **Event Processing Pipeline**
    - **Requirements**:
        - Design async channel-based pipeline (`tokio::sync::mpsc`) to decouple ingestion from processing.
        - Implement a worker pool that receives filtered events for processing.
        - Ensure "At-least-once" processing semantics with acknowledgment tracking.
        - Support graceful shutdown with event drain.

6. **CLI: Jetstream Commands**
    - **Requirements**:
        - `jetstream|js listen` - Connect to Jetstream and print events in real-time.
        - `jetstream|js listen --filter-did <DID>` - Filter to mentions of a specific DID.
        - `jetstream|js listen --duration <SECONDS>` - Listen for a fixed duration then exit.
        - `jetstream|js replay --cursor <TIME_US>` - Replay events from a specific cursor.

## Milestone 2: State Persistence and Context

**Goal**: Implement the "Memory" of the agent using libSQL/SQLite, enabling conversation reconstruction.

**Definition of Done**:

1. libSQL database schema is applied and capable of storing threading relationships (root, parent, child).
2. Successfully parsing a "Reply" structure from Bluesky and mapping it to a `thread_root`.
3. Ability to query the full linear history of a conversation given a `root_uri`.
4. Identity resolution cache is working (mapping DIDs to Handles without external API calls for every request).
5. Database commands are testable via CLI.

**Tasks**:

1. **Database Schema Design and Migration**
    - **Requirements**:
        - Add `libsql` crate (Turso's SQLite fork) for embedded database access.
        - Design SQL schema: `conversations` (id, root_uri, post_uri, parent_uri, author_did, role, content, created_at).
        - Design SQL schema: `identities` (did, handle, last_updated).
        - Create `.sql` migration files and embed them using `include_str!`.
        - Implement a repository pattern with async trait methods for database operations.

2. **Thread Context Reconstruction**
    - **Requirements**:
        - Implement logic to determine `root_uri` from an incoming post (if it's a reply, use its `root`; if new, it is the `root`).
        - Implement query to fetch all messages for a `root_uri` ordered by `created_at`.
        - Test handling of "orphaned" replies (where the parent is missing from our DB) - decide on policy (fetch from Bsky API or ignore).
        - Ensure strict ordering by `created_at` to maintain causal coherence.

3. **Identity Resolution Module**
    - **Requirements**:
        - Implement a check: When a specific DID is encountered, check the `identities` table.
        - If missing or stale (>24h), query `com.atproto.identity.resolveHandle` via manual XRPC (`reqwest` GET to PDS).
        - Async update the `identities` table with TTL-based caching.
        - Expose a helper function `resolve_did_to_handle(did) -> String` for the UI and AI context.

4. **State Write Path**
    - **Requirements**:
        - Update the event processor to write incoming mentions to `conversations` table.
        - Map the incoming `text` to the `user` role.
        - Handle duplicates (idempotency) based on `post_uri` using `INSERT OR IGNORE`.
        - Verify that emojis and utf-8 characters are stored correctly.

5. **CLI: Database Commands**
    - **Requirements**:
        - `db migrate` - Run pending database migrations.
        - `db stats` - Show database statistics (row counts, size).
        - `db threads` - List recent conversation threads.
        - `db thread <ROOT_URI>` - Display full thread history.
        - `db identities` - List cached identity mappings.

## Milestone 3: Bluesky XRPC Client

**Goal**: Implement manual XRPC client for authentication, posting, and identity resolution.

**Definition of Done**:

1. Agent successfully authenticates with Bluesky PDS and manages session tokens.
2. Posts can be created and replies attached to threads with correct Root/Parent CIDs.
3. Handles can be resolved to DIDs and vice versa.
4. All XRPC operations are testable via CLI.

**Tasks**:

1. **XRPC Client Foundation**
    - **Requirements**:
        - Implement `BskyClient` struct with `reqwest` HTTP client.
        - Implement `createSession` for authentication with identifier/password
            - These are a handle + app password stored in `.env
        - Setup OAuth2 flow
        - Store `accessJwt` and `refreshJwt` with expiry tracking.
        - Implement `refreshSession` for automatic token renewal.
        - Handle common XRPC errors (rate limits, auth failures, network errors).

2. **Record Operations**
    - **Requirements**:
        - Implement `createRecord` for posting (`app.bsky.feed.post`).
        - Implement `getRecord` for fetching posts by URI.
        - Construct proper `ReplyRef` with Root URI/CID and Parent URI/CID.
        - Handle `createdAt` timestamp generation in ISO 8601 format.

3. **Identity Operations**
    - **Requirements**:
        - Implement `resolveHandle` to convert handle to DID.
        - Implement `getProfile` to fetch user profile information.
        - Cache resolved identities in the database (from Milestone 2).

4. **CLI: Bluesky Commands**
    - **Requirements**:
        - `bsky login` - Authenticate and cache session tokens.
        - `bsky whoami` - Display current session info (DID, handle, PDS).
        - `bsky post <TEXT>` - Create a new post.
        - `bsky reply <URI> <TEXT>` - Reply to an existing post.
        - `bsky resolve <HANDLE>` - Resolve a handle to DID.
        - `bsky get-post <URI>` - Fetch and display a post record.

## Milestone 4: The Cognitive Core (GLM-5)

**Goal**: Connect the stateful context to Z's GLM-5 model to generate intelligent responses.

**Definition of Done**:

1. Agent successfully authenticates with GLM-5 using Rust.
2. Context window is correctly formatted (Chats mapped to `Content` and `Part` types).
3. GLM-5 "Thinking" process is enabled and functioning.
4. Agent responses are posted back to Bluesky with valid Record keys (Reply/Root).
5. AI operations are testable via CLI.

**Tasks**:

1. **GLM-5 API Client (Rust)**
    - **Requirements**:
        - Configure with async HTTP client (`reqwest` with `rustls` for TLS).
        - Implement typed request/response structs for `GenerateContentRequest`, `Content`, `Part`, `ThinkingConfig`.
        - Implement error handling for API quotas and 5xx errors with retry logic.
        - Securely manage `GLM_5_API_KEY` using environment variables.

2. **Context Construction Logic**
    - **Requirements**:
        - Create a `PromptBuilder` that takes the `Vec<ConversationRow>` from Milestone 2.
        - Format the history
        - Prepend multi-user messages with handle context: `[@handle]: message`.
        - Inject a "System Instruction" that defines the persona (Stateful, Persistent, Helpful).
        - Serialize the payload to JSON.

3. **Action: Generating and Posting**
    - **Requirements**:
        - Call GLM-5 with `thinking_config: { include_thoughts: false }` (or true for debugging).
        - Extract the text response from model output.
        - Use `BskyClient` from Milestone 3 to post replies.
        - Construct the precise `reply` ref (Root URI/CID, Parent URI/CID) to ensure the post attaches to the thread.

4. **Self-Correction and Loop Prevention**
    - **Requirements**:
        - Implement logic: If `author_did` == `OWN_DID`, do not reply (Loop prevention).
        - Implement "Silent Mode": If GLM-5 generates `<SILENT_THOUGHT>`, output nothing (allow the bot to choose not to reply).
        - Handle "Rate Limit" errors from Bluesky (429) gracefully with exponential backoff.
        - Record the _Bot's_ reply into the database immediately after posting (Role: `model`).

5. **CLI: AI Commands**
    - **Requirements**:
        - `ai prompt <TEXT>` - Send a one-shot prompt to GLM-5.
        - `ai chat` - Interactive chat session with GLM-5.
        - `ai context <ROOT_URI>` - Build and display the prompt context for a thread.
        - `ai simulate <ROOT_URI>` - Simulate a response without posting.

## Milestone 5: Vector Memory and Semantic Search

**Goal**: Extend the agent's memory with vector embeddings for semantic retrieval beyond thread context.

**Definition of Done**:

1. Vector store is initialized and capable of storing/querying embeddings.
2. Conversations are embedded and indexed for semantic similarity search.
3. Agent can retrieve relevant past context from across different threads.
4. Hybrid retrieval (keyword + semantic) is functional.

**Tasks**:

1. **Vector Store Setup**
    - **Requirements**:
        - Use SQLite for vector storage
        - Design vector schema: `memories` (id, embedding, conversation_id, content, metadata, created_at).
        - Configure embedding dimension based on chosen model (768 for small, 1536 for large).
        - Implement connection pooling and error handling.

2. **Embedding Generation**
    - **Requirements**:
        - Implement embedding generation.
        - Create batch embedding pipeline for historical data backfill.
        - Implement caching layer to avoid re-embedding unchanged content.
        - Handle embedding failures gracefully with retry logic.

3. **Semantic Retrieval**
    - **Requirements**:
        - Implement similarity search function with configurable top-k results.
        - Add metadata filtering (by user, time range, topic).
        - Implement hybrid search combining BM25 (keyword) with vector similarity.
        - Expose retrieval function to the prompt builder for RAG context.

4. **Memory Management**
    - **Requirements**:
        - Implement memory consolidation to compress old conversations into summaries.
        - Add TTL-based memory expiration for privacy compliance.
        - Create admin API for memory inspection and deletion.
        - Implement memory deduplication to avoid redundant embeddings.

5. **CLI: Vector Commands**
    - **Requirements**:
        - `vector stats` - Show vector store statistics.
        - `vector search <QUERY>` - Perform similarity search and display results.
        - `vector embed <TEXT>` - Generate and display embedding for text.
        - `vector backfill` - Backfill embeddings for existing conversations.

## Milestone 6: The Control Deck (Web UI)

**Goal**: A lightweight, fast dashboard to monitor the bot's "brain" and state, using modern HTMX + Pico.

**Definition of Done**:

1. A web route `/dashboard` displays live stats of the bot.
2. Admins can view the raw "Conversation Tables" and "Identity Maps".
3. A "Manual Override" feature allowing the admin to post as the bot via the UI.
4. Auth protection (Basic Auth or session-based) is active.

**Tasks**:

1. **HTMX + Pico Setup**
    - **Requirements**:
        - Add `axum` or `actix-web` for HTTP routing (with feature flags for different runtimes).
        - Serve Pico CSS from CDN or embedded assets.
        - Implement HTML templating using `askama` or `maud` crate.
        - Create the main layout: Sidebar (Status, Logs, Chat, Config) + Content Area.
        - Set up HTMX for checking server health/status every 5s (`hx-trigger="every 5s"`).

2. **Live Status Dashboard**
    - **Requirements**:
        - Create a widget showing "Last Jetstream Event" timestamp.
        - Create a widget showing "Processing Queue Depth".
        - Create a widget showing "Monthly Token Usage" (estimated count).
        - Use HTMX to swap these numbers in real-time.

3. **Conversation Inspector**
    - **Requirements**:
        - Create a view that lists recent threads (grouped by `root_uri`).
        - Clicking a thread loads the full message history (User + Bot + Thoughts) into a detail view.
        - Style the chat to look like a messenger app using Pico's chat bubbles.
        - Indicate "Thinking Time" or latency metrics for each bot reply.

4. **Admin Controls**
    - **Requirements**:
        - Implement a "Pause Bot" toggle (stops event processing).
        - Implement a "Clear Context" button (clear history for a specific thread).
        - Implement a simple form to send a raw post (Broadcast) from the bot account.
        - Ensure all admin routes are behind authentication middleware.

## Milestone 7: Deployment and Reliability

**Goal**: Harden the system for continuous operation with provider-agnostic deployment.

**Definition of Done**:

1. System handles 24/7 operation with automatic recovery.
2. Secrets are managed via environment variables or secret managers.
3. Logging output is structured (JSON) and queryable.
4. Codebase is clean, formatted, and documented.

**Tasks**:

1. **Containerization**
    - **Requirements**:
        - Create multi-stage `Dockerfile` with cargo-chef for cached builds.
        - Support both `x86_64` and `aarch64` targets.
        - Minimize image size with `scratch` or `distroless` base.
        - Document deployment to Fly.io, Shuttle, and container registries.

2. **Structured Logging**
    - **Requirements**:
        - Implement `tracing` middleware that outputs JSON logs.
        - Include `trace_id` for every request/event to trace from Jetstream through GLM-5 to Bsky.
        - Log specific "Thinking" traces if available (for debugging).
        - Support log levels configurable via environment.

3. **Error Boundaries and Recovery**
    - **Requirements**:
        - Wrap main event loop with proper error handling and recovery.
        - Implement a "Dead Letter" table: If a message fails processing 3 times, move it to `failed_events`.
        - Implement health check endpoint for load balancer probes.
        - Add metrics endpoint for Prometheus scraping (optional).

4. **Final Polish**
    - **Requirements**:
        - Run `cargo fmt` and `cargo clippy` with strict settings.
        - Write `README.md` with "How to Deploy" instructions for multiple providers.
        - Perform a 24-hour soak test.
        - Document architecture decisions in ADR format.

5. **CLI: Operations Commands**
    - **Requirements**:
        - `status` - Show service health and connection status.
        - `serve` - Start the main daemon (Jetstream listener + event processor).
        - `serve --dry-run` - Process events without posting replies.

## Parking Lot

- Whitelist for who can communicate with bots (mutuals?)
- Administrator accounts
- Support other models
    1. [Gemma via Ollama](https://ollama.com/library/gemma3) (`gemma3:latest`)
    2. [Whisper.cpp](https://github.com/ggml-org/whisper.cpp) to talk to it
        - Small or Medium
