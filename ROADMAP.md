# Stateful Agent Bot Specification

This document outlines the engineering specification for a **Stateful AI Agent** on the AT Protocol (Bluesky), utilizing **Gemini 3** for reasoning, **Rust** for infrastructure, and **HTMX + Pico CSS** for the management dashboard.

## System Architecture

- **Runtime**: Provider-agnostic Rust binary. Deployable to Fly.io (containers), Docker, or any container runtime.
- **Ingestion**: Manual WebSocket client consuming Jetstream via `tokio-tungstenite` with zstd decompression.
- **State Store**: libSQL/SQLite (embedded or Turso cloud). Portable, single-file database with replication support.
- **Vector Store**: LanceDB (embedded) or Qdrant (self-hosted/cloud). Provider-agnostic vector similarity search.
- **Cognitive Engine**: Google Gemini 3 (Pro/Thinking) via REST API using `gemini-rust` or `gmini` crate.
- **Bluesky Integration**: Manual XRPC client via `reqwest` for full control over authentication and posting.
- **Frontend**: Server-side rendered HTML (HTMX) with Pico CSS, served directly from the application.
- **CLI**: `clap`-based command-line interface for ad-hoc testing and system interaction.

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
        - Add `config` crate for layered configuration (defaults, file, env vars).
        - Support `.env` files for local development via `dotenvy`.
        - Configure feature flags for different deployment targets (native, wasm32).

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
        - `jetstream listen` - Connect to Jetstream and print events in real-time.
        - `jetstream listen --filter-did <DID>` - Filter to mentions of a specific DID.
        - `jetstream listen --duration <SECONDS>` - Listen for a fixed duration then exit.
        - `jetstream replay --cursor <TIME_US>` - Replay events from a specific cursor.

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

## Milestone 4: The Cognitive Core (Gemini 3)

**Goal**: Connect the stateful context to Google's Gemini 3 model to generate intelligent responses.

**Definition of Done**:

1. Agent successfully authenticates with Google Gemini API using Rust.
2. Context window is correctly formatted (Chats mapped to `Content` and `Part` types).
3. Gemini "Thinking" process is enabled and functioning.
4. Agent responses are posted back to Bluesky with valid Record keys (Reply/Root).
5. AI operations are testable via CLI.

**Tasks**:

1. **Gemini API Client (Rust)**
    - **Requirements**:
        - Add `gemini-rust` or `gmini` crate for Gemini API integration.
        - Configure with async HTTP client (`reqwest` with `rustls` for TLS).
        - Implement typed request/response structs for `GenerateContentRequest`, `Content`, `Part`, `ThinkingConfig`.
        - Implement error handling for API quotas and 5xx errors with retry logic.
        - Securely manage `GEMINI_API_KEY` using environment variables.

2. **Context Construction Logic**
    - **Requirements**:
        - Create a `PromptBuilder` that takes the `Vec<ConversationRow>` from Milestone 2.
        - Format the history into the strict `history` JSON array required by Gemini.
        - Prepend multi-user messages with handle context: `[@handle]: message`.
        - Inject a "System Instruction" that defines the persona (Stateful, Persistent, Helpful).
        - Serialize the payload to JSON.

3. **Action: Generating and Posting**
    - **Requirements**:
        - Call Gemini 3 with `thinking_config: { include_thoughts: false }` (or true for debugging).
        - Extract the text response from model output.
        - Use `BskyClient` from Milestone 3 to post replies.
        - Construct the precise `reply` ref (Root URI/CID, Parent URI/CID) to ensure the post attaches to the thread.

4. **Self-Correction and Loop Prevention**
    - **Requirements**:
        - Implement logic: If `author_did` == `OWN_DID`, do not reply (Loop prevention).
        - Implement "Silent Mode": If Gemini generates `<SILENT_THOUGHT>`, output nothing (allow the bot to choose not to reply).
        - Handle "Rate Limit" errors from Bluesky (429) gracefully with exponential backoff.
        - Record the _Bot's_ reply into the database immediately after posting (Role: `model`).

5. **CLI: AI Commands**
    - **Requirements**:
        - `ai prompt <TEXT>` - Send a one-shot prompt to Gemini.
        - `ai chat` - Interactive chat session with Gemini.
        - `ai context <ROOT_URI>` - Build and display the prompt context for a thread.
        - `ai simulate <ROOT_URI>` - Simulate a response without posting.

**Free Tier Rate Limits (as of January 2026)**:

| Model                    | API ID                   | RPM   | TPM     | RPD     | Context    |
| ------------------------ | ------------------------ | ----- | ------- | ------- | ---------- |
| Gemini 2.5 Pro           | `gemini-2.5-pro`         | 5     | 250,000 | 100     | 1M tokens  |
| Gemini 2.5 Flash         | `gemini-2.5-flash`       | 10-15 | 250,000 | 250-500 | 1M tokens  |
| Gemini 2.5 Flash-Lite    | `gemini-2.5-flash-lite`  | 15    | 250,000 | 1,000   | 1M tokens  |
| Gemini 3 Pro (Preview)   | `gemini-3-pro-preview`   | 10    | 250,000 | 100     | 1M+ tokens |
| Gemini 3 Flash (Preview) | `gemini-3-flash-preview` | -     | -       | -       | 1M+ tokens |

- RPM = Requests per minute, TPM = Tokens per minute, RPD = Requests per day
- Rate limits apply per Google Cloud Project, not per API key
- RPD quotas reset at midnight Pacific Time
- No credit card required for free tier
- Regional restrictions: Free tier unavailable for EU, UK, and Swiss users (paid tier required)
- December 2025 saw 50-80% reductions across most free tier models

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
        - Add `lancedb` crate for embedded vector storage (or `qdrant-client` for external).
        - Design vector schema: `memories` (id, embedding, conversation_id, content, metadata, created_at).
        - Configure embedding dimension based on chosen model (768 for small, 1536 for large).
        - Implement connection pooling and error handling.

2. **Embedding Generation**
    - **Requirements**:
        - Implement embedding generation using Gemini's embedding API or a local model.
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

**Embedding API Rate Limits**:

- Gemini Embedding (`gemini-embedding-001`) is available on the free tier
- Free tier: Embedding model shares the same rate limit structure as generation models
- Paid tier: $0.15 per 1M input tokens
- Consider local embedding models (e.g., via `fastembed-rs`) for high-volume backfill operations to avoid hitting rate limits
- Batch embedding requests where possible to maximize throughput within RPM constraints

## Milestone 6: Unified Persistence (SQLite + sqlite-vec)

**Goal**: Refactor the vector storage layer to use SQLite with the `sqlite-vec` extension, consolidating metadata and embeddings into a single unified database engine.

**Definition of Done**:

1. LanceDB dependency is removed from the project.
2. All memory persistence and retrieval (Vector + FTS) is handled by SQLite/libSQL.
3. Hybrid search (Semantic + Keyword) parity is achieved or improved.
4. Existing LanceDB data is successfully migrated to the new SQLite schema.
5. All vector-related CLI commands are updated and functional.

**Tasks**:

1. **Integrated Schema Design**
    - **Requirements**:
        - Extend the existing `bot.db` schema to include memory tables.
        - Implement `vec0` virtual table for high-performance vector storage and retrieval.
        - Implement FTS5 virtual table for keyword search on memory content.
        - Design a unified repository that manages both metadata and embeddings.

2. **sqlite-vec Integration**
    - **Requirements**:
        - Integrate `sqlite-vec` into the Rust build process (static linking or extension loading).
        - Implement `SqliteVecStore` to replace `LanceDBStore`, satisfying the `VectorStore` trait.
        - Optimize vector queries for performance (KNN search with `ORDER BY distance`).

3. **Hybrid Search & Ranking**
    - **Requirements**:
        - Implement hybrid search logic in SQL or Rust.
        - Use Reciprocal Rank Fusion (RRF) or similar ranking algorithms to blend FTS and Vector scores.
        - Ensure search relevance matches or exceeds the current LanceDB implementation.

4. **Data Migration Pipeline**
    - **Requirements**:
        - Create a one-time migration script/command to move data from `bot.lancedb` to `bot.db`.
        - Verify data integrity and embedding correctness after migration.
        - Clean up legacy LanceDB artifacts.

5. **Cli & Integration Update**
    - **Requirements**:
        - Update `vector` CLI commands to target the new SQLite implementation.
        - Ensure `MemoryConfig` is respected by the new store.
        - Verify `SemanticRetriever` works seamlessly with the new backend.

## Milestone 7: The Control Deck (Web UI)

**Goal**: A lightweight, fast dashboard to monitor the bot's "brain" and state, using modern HTMX + Pico.

**Definition of Done**:

1. Landing page `/` that explains the purpose and provides a link to the dashboard
2. A web route `/dashboard` displays live stats of the bot.
3. Admins can view the raw "Conversation Tables" and "Identity Maps".
4. A "Manual Override" feature allowing the admin to post as the bot via the UI.
5. Auth protection through BSky app password is active.

**Tasks**:

1. **HTMX + Pico Setup**
    - **Requirements**:
        - Add `axum` for HTTP routing (with feature flags for different runtimes).
        - Serve Pico CSS from CDN or embedded assets.
        - Implement HTML templating using `maud` crate.
        - Create the main layout: Sidebar (Status, Logs, Chat, Config) + Content Area.
        - Set up HTMX for checking server health/status every 5s (`hx-trigger="every 5s"`).

    ```html
    <link rel="stylesheet"
          href="https://cdn.jsdelivr.net/npm/@picocss/pico@2/css/pico.jade.min.css">
    <script src="https://cdn.jsdelivr.net/npm/htmx.org@2.0.8/dist/htmx.min.js"></script>
    ```

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

## Milestone 8: Deployment and Reliability

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
        - Include `trace_id` for every request/event to trace from Jetstream through Gemini to Bsky.
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
