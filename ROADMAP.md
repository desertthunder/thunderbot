# Stateful Agent Bot Specification

This document outlines the engineering specification for a **Stateful AI Agent** on the AT Protocol (Bluesky), utilizing **GLM-5** for reasoning, **Rust** for infrastructure, and **HTMX + Pico CSS** for the management dashboard.

## ToC

- [Stateful Agent Bot Specification](#stateful-agent-bot-specification)
  - [ToC](#toc)
  - [System Architecture](#system-architecture)
  - [Overview](#overview)
  - [Milestone 1: The Foundation, Ingestion Layer, and CLI](#milestone-1-the-foundation-ingestion-layer-and-cli)
  - [Milestone 2: State Persistence and Context](#milestone-2-state-persistence-and-context)
  - [Milestone 3: Bluesky XRPC Client](#milestone-3-bluesky-xrpc-client)
  - [Milestone 4: The Cognitive Core (GLM-5)](#milestone-4-the-cognitive-core-glm-5)
  - [Milestone 5: Vector Memory and Semantic Search](#milestone-5-vector-memory-and-semantic-search)
  - [Milestone 6: The Control Deck (Web UI)](#milestone-6-the-control-deck-web-ui)
  - [Milestone 7: Observability \& Health](#milestone-7-observability--health)
  - [Milestone 8: Developer Experience](#milestone-8-developer-experience)
  - [Milestone 9: Multi-Model Support](#milestone-9-multi-model-support)
  - [Milestone 10: Dashboard Enhancements](#milestone-10-dashboard-enhancements)
  - [Milestone 11: Operational Controls](#milestone-11-operational-controls)
  - [Milestone 12: Voice Interface](#milestone-12-voice-interface)
  - [Milestone 13: Deployment/Self-Hosting](#milestone-13-deploymentself-hosting)
  - [Parking Lot](#parking-lot)

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

Part 2: Milestones 7 & 8

Part 3: Milestone 9

Part 4: Milestones 10 & 11

Part 5: Milestone 12

Part 6: Milestone 13

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

1. `EmbeddingGemma` runs locally via Ollama and generates embeddings.
2. `memories` table stores `F32_BLOB(768)` vectors in libSQL native vector columns.
3. KNN semantic search retrieves relevant past context across threads.
4. Hybrid retrieval (FTS5 keyword + vector cosine via RRF) is functional.
5. PromptBuilder injects retrieved memories as context for RAG.

**Tasks**:

1. **Vector Store Schema**
    - **Requirements**:
        - Add `002_vector_memory.sql` migration.
        - `memories` table: `id`, `conversation_id` (FK), `root_uri`, `content`, `embedding F32_BLOB(768)`, `author_did`, `metadata` (JSON), `created_at`, `expires_at`.
        - Create `libsql_vector_idx` on `embedding` column for KNN.
        - `embedding_jobs` table: tracks pending/complete/failed embedding state per conversation.
        - FTS5 virtual table `memories_fts` for keyword search on `content`.

2. **Ollama Embedding Client**
    - **Requirements**:
        - Define `EmbeddingProvider` trait (`embed`, `embed_batch`, `dimensions`).
        - Implement `OllamaEmbeddingProvider` calling `POST http://localhost:11434/api/embed`.
        - Default model: `embeddinggemma` (768 dims, 308M params, <200MB RAM quantized).
        - Fallback model: `nomic-embed-text` (768 dims, 8K context for long summaries).
        - Batch support (configurable `batch_size`, default 32).
        - Retry on Ollama connection failure with backoff.

3. **Embedding Pipeline**
    - **Requirements**:
        - On new conversation insert → create `embedding_jobs` row (status: pending).
        - Background async worker picks up pending jobs, calls `EmbeddingProvider`, stores result.
        - Skip re-embedding if content hash matches existing memory (deduplication at distance < 0.05).
        - Retry up to 3× on failure, then mark as `failed`.
        - `vector backfill` CLI command for historical data.

4. **Semantic Retrieval**
    - **Requirements**:
        - KNN search via `vector_top_k()` + `vector_distance_cos()`.
        - Metadata filtering: by `author_did`, time range, `root_uri`.
        - Hybrid search: run FTS5 BM25 + vector cosine in parallel, merge with Reciprocal Rank Fusion (RRF, k=60).
        - Configurable `top_k` (default 5).
        - Expose `MemoryRetriever` service to PromptBuilder for RAG injection.

5. **Memory Management**
    - **Requirements**:
        - Consolidation: after 24h thread inactivity, summarize thread via GLM-5, embed summary, delete individual post embeddings.
        - TTL expiration: 90 days for post memories, 365 days for consolidated summaries.
        - Deduplication: skip insert if cosine distance to existing same-root memory < 0.05.
        - `vector consolidate` and `vector expire` CLI commands.

6. **CLI: Vector Commands**
    - **Requirements**:
        - `vector stats` — memory count, index size, embedding job status.
        - `vector search <QUERY>` — semantic search with `--top-k`, `--author` filters.
        - `vector embed <TEXT>` — generate and display raw embedding vector.
        - `vector backfill` — embed all un-embedded conversations.
        - `vector consolidate` — compress stale threads into summary memories.
        - `vector expire` — delete memories past TTL.

## Milestone 6: The Control Deck (Web UI)

**Goal**: A lightweight, fast dashboard to monitor the bot's "brain" and state, using Alpine.js, HTMX + Pico (written in maud macros in rust)

**Definition of Done**:

1. A web route `/dashboard` displays live stats of the bot.
2. Admins can view the raw "Conversation Tables" and "Identity Maps".
3. A "Manual Override" feature allowing the admin to post as the bot via the UI.
4. Auth protection (Basic Auth or session-based) is active.

**Tasks**:

1. **HTMX + Pico Setup**
    - **Requirements**:
        - Add `axum` for HTTP routing (with feature flags for different runtimes).
        - Serve Pico CSS from CDN or embedded assets.
        - Implement HTML templating using `maud` crate.
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

## Milestone 7: Observability & Health

**Goal**: Proper health checks, metrics, and diagnostic tooling for operators.

**Definition of Done**:

1. Health endpoint returns meaningful component status
2. CLI `status` and `config validate` commands work
3. Operators can diagnose issues without reading logs
4. Dashboard shows real-time service health

**Tasks**:

1. **Health Check Endpoint**
    - Implement `/health` returning JSON with component status
    - Check database connectivity (query latency)
    - Check Jetstream WebSocket connection state
    - Check Bluesky session validity (token expiration)
    - Check GLM-5 API reachability (ping)
    - Return HTTP 200 when healthy, 503 when degraded
    - Include version info and uptime in response

2. **CLI Completions**
    - Implement `status` command (query running server's `/health`)
    - Implement `config show` displaying current configuration
    - Implement `config validate` checking env vars and connectivity
    - Add `--json` flag for machine-readable output

3. **Observability Foundations**
    - Add request timing middleware (log request duration)
    - Track event processing metrics (events/second, queue depth)
    - Track GLM-5/Z.ai API latency and error rates
    - Custom `/metrics` endpoint in Prometheus text format (no external deps)
    - Atomic counters for thread-safe metric collection
    - Sliding window for latency quantiles (P50/P90/P99)

4. **Dashboard Health Panel**
    - Replace timestamp-only status with component health cards
    - Show Jetstream connection state (connected/reconnecting/disconnected)
    - Show last successful GLM-5 call timestamp
    - Show database stats (connection pool, query count)
    - Auto-refresh health status via HTMX polling

5. **Graceful Degradation**
    - Continue serving dashboard when Jetstream disconnects
    - Queue outbound posts when Bluesky is rate-limited
    - Surface degraded state in health endpoint and dashboard

## Milestone 8: Developer Experience

**Goal**: Make development and debugging faster with better CLI tools and error handling.

**Definition of Done**:

1. All commands support `--dry-run` for safe testing
2. Database can be backed up and restored from CLI
3. Errors include actionable suggestions
4. Graceful shutdown preserves state

**Tasks**:

1. **Dry-Run Mode**
    - Add `--dry-run` flag to `serve` (process events, skip posting)
    - Add `--dry-run` to `bsky post` and `bsky reply` (show what would post)
    - Add `--dry-run` to `ai prompt` (show formatted request)
    - Log dry-run actions clearly to distinguish from real operations

2. **Database Operations**
    - `db backup <path>` command (copy database file with WAL checkpoint)
    - `db restore <path>` command (validate and replace database)
    - `db vacuum` command (reclaim space, optimize)
    - Include backup procedures in error recovery docs

3. **Log Streaming**
    - `logs` command to tail structured logs from running server
    - Filter by level (`--level warn`), component (`--component jetstream`)
    - Follow mode (`--follow`) for real-time streaming
    - Output as JSON or human-readable

4. **Error Messages**
    - Include suggested fixes in common error scenarios
    - Link to relevant documentation sections
    - Show context (what was being attempted, what failed)
    - Distinguish transient vs permanent failures

5. **Graceful Shutdown**
    - Handle SIGTERM/SIGINT signals
    - Drain event processing queue before exit
    - Close WebSocket connections cleanly
    - Save pending state to database
    - Log shutdown progress

## Milestone 9: Multi-Model Support

**Goal**: Make the cognitive engine provider-agnostic, supporting Kimi K2.5 as the cloud primary and Gemma 3 via Ollama for local/offline inference.

**Definition of Done**:

1. A `ModelProvider` trait abstracts chat completion across backends.
2. Kimi K2.5 is callable via Moonshot's OpenAI-compatible API.
3. Gemma 3 4B runs locally via Ollama with the same trait interface.
4. Provider is selectable via configuration without code changes.
5. Existing GLM-5 client is refactored behind the trait (backwards compatible).
6. CLI commands work with any configured provider.

**Tasks**:

1. **ModelProvider Trait**
    - **Requirements**:
        - Define `ModelProvider` async trait with `chat_completion(&self, messages, config) -> Result<Response>`.
        - Shared types: `ChatMessage` (role, content), `CompletionConfig` (temperature, max_tokens, thinking).
        - Shared response type: `CompletionResponse` (text, usage, model, finish_reason).
        - Provider-level error types mapping to common errors (rate limit, auth, network, context overflow).
        - Factory function to instantiate provider from configuration enum.

2. **Kimi K2.5 Provider**
    - **Requirements**:
        - Implement `ModelProvider` for `KimiProvider` targeting `https://api.moonshot.ai/v1/chat/completions`.
        - Model ID: `kimi-k2.5` (MoE, 1T total params, 32B active per token).
        - 262K token context window support.
        - Configure with `TNBOT_AI__API_KEY` and `TNBOT_AI__BASE_URL` environment variables.
        - Support Thinking mode via Kimi's extended parameters.
        - Handle Moonshot-specific rate limits and error codes.
        - Pricing-aware logging: $0.60/M input, $3.00/M output tokens.

3. **Gemma 3 Provider (Local via Ollama)**
    - **Requirements**:
        - Implement `ModelProvider` for `OllamaProvider` targeting `http://localhost:11434/v1/chat/completions`.
        - Default model: `gemma3:4b` (4B dense params, ~2.5 GB quantized Q4_K_M, 4 GB RAM).
        - 32K token context window.
        - Configurable model tag via `TNBOT_AI__OLLAMA_MODEL` for swapping to `mistral:7b` or others.
        - Health check on startup: verify Ollama is running and model is pulled.
        - Graceful fallback: if Ollama is unreachable, log warning and disable local inference.

4. **GLM-5 Refactor**
    - **Requirements**:
        - Refactor existing `AiClient` to implement `ModelProvider` trait.
        - Preserve existing Z.ai API endpoint and authentication logic.
        - No behavioral changes to existing GLM-5 usage.

5. **Provider Configuration**
    - **Requirements**:
        - Add `[ai]` section to config: `provider` (kimi, ollama, glm5), `model`, `base_url`, `api_key`, `temperature`, `max_tokens`.
        - Support provider override per CLI command via `--provider` flag.
        - Validate provider configuration at startup (check API keys, connectivity).
        - Log active provider and model on startup.

6. **CLI: Model Commands**
    - **Requirements**:
        - `ai providers` — list configured providers and their status (reachable/unreachable).
        - `ai prompt <TEXT> --provider <PROVIDER>` — send prompt to a specific provider.
        - `ai benchmark <TEXT>` — run same prompt across all providers, compare latency and output.
        - Update existing `ai` subcommands to respect `--provider` flag.

## Milestone 10: Dashboard Enhancements

**Goal**: Make the dashboard more powerful for day-to-day bot management.

**Definition of Done**:

1. Users can search and filter conversations
2. Data can be exported for analysis
3. Keyboard navigation works throughout
4. Dark mode available

**Tasks**:

1. **Conversation Search**
    - Full-text search across message content
    - Filter by author DID or handle
    - Filter by date range
    - Search results with context highlighting

2. **Data Export**
    - Export conversations as JSON (full fidelity)
    - Export conversations as CSV (flattened for spreadsheets)
    - Export single thread or bulk selection
    - Include metadata (timestamps, URIs, author info)

3. **Bulk Actions**
    - Select multiple threads with checkboxes
    - Delete selected threads
    - Clear conversations older than N days
    - Confirmation dialogs for destructive actions

4. **Thread Filtering**
    - Mute specific authors (hide from default view)
    - Filter by conversation length
    - Show only threads with recent activity
    - Save filter presets

5. **Keyboard Shortcuts**
    - `j`/`k` for navigating thread list
    - `Enter` to open selected thread
    - `r` to start reply
    - `Escape` to close modals
    - `?` to show shortcut help overlay

6. **Dark Mode**
    - Toggle in navigation or settings
    - Persist preference in localStorage
    - Use Pico CSS built-in dark theme
    - Respect system preference by default

7. **Activity Timeline**
    - Recent bot actions (posts, replies, errors)
    - Timestamps and links to relevant threads
    - Filter by action type
    - Paginated history

## Milestone 11: Operational Controls

**Goal**: Give operators fine-grained control over bot behavior and visibility into limits.

**Definition of Done**:

1. Rate limits visible before hitting them
2. Bot behavior configurable without restart
3. Status communicable via Bluesky profile/posts
4. Failed events recoverable

**Tasks**:

1. **Rate Limit Dashboard**
    - Show current Bluesky rate limit usage and reset time
    - Show GLM-5 API quota consumption
    - Warn when approaching limits (80% threshold)
    - Historical rate limit graph

2. **Event Processing Visibility**
    - Show queue depth and processing lag
    - Display events/second throughput
    - Alert when backlog exceeds threshold
    - Pause/resume event processing from dashboard

3. **Session Management**
    - View active Bluesky session details
    - Show token expiration countdown
    - Force session refresh button
    - Automatic refresh before expiration (proactive)

4. **Response Preview Mode**
    - Queue responses for manual approval before posting
    - Show generated response with edit capability
    - Approve, edit, or discard pending responses
    - Bulk approve for trusted threads

5. **Quiet Hours**
    - Configure time windows when bot won't post
    - Queue responses during quiet hours, post when window ends
    - Timezone-aware scheduling
    - Override for urgent manual posts

6. **Reply Limits**
    - Maximum replies per thread (prevent runaway conversations)
    - Cooldown between replies to same thread
    - Per-author reply limits
    - Configurable from dashboard

7. **Blocklist Management**
    - Block DIDs from triggering bot responses
    - Import/export blocklist
    - Temporary blocks with expiration
    - View block reasons and history

8. **Status Broadcasting**
    - Update Bluesky bio with current status (online/maintenance/limited)
    - Post status updates for extended outages
    - Scheduled maintenance announcements
    - Status page integration

9. **Dead Letter Queue**
    - Store failed events for inspection
    - View failure reason and stack trace
    - Retry individual events or bulk retry
    - Purge old failures

## Milestone 12: Voice Interface

**Goal**: Add speech-to-text and text-to-speech capabilities using whisper.cpp and Piper, enabling voice interaction with the bot.

**Definition of Done**:

1. whisper.cpp transcribes audio input to text via embedded Rust bindings or HTTP sidecar.
2. Piper synthesizes bot responses into natural speech audio.
3. Audio pipeline integrates with the existing AI action pipeline (STT → AI → TTS).
4. Voice models are configurable and downloadable via CLI.
5. Dashboard exposes voice controls and playback.

**Tasks**:

1. **whisper.cpp Integration (Speech-to-Text)**
    - **Requirements**:
        - Add `whisper-rs` crate with Metal/CUDA feature flags for hardware acceleration.
        - Default model: `ggml-small` (244M params, 466 MB disk, ~1 GB RAM, 4% WER).
        - Support model tiers: `tiny` (fastest, 8% WER), `base`, `small` (recommended), `medium`, `large-v3-turbo` (best accuracy).
        - Accept audio formats: WAV, MP3, FLAC, OGG, AAC, M4A.
        - Implement `SttProvider` trait with `transcribe(&self, audio_data) -> Result<String>`.
        - Fallback: HTTP sidecar mode targeting `POST /v1/audio/transcriptions` (OpenAI-compatible whisper-server).
        - Performance target on Apple Silicon: ≤0.3× real-time factor with `small` model (3s processing for 10s audio).

2. **Piper Integration (Text-to-Speech)**
    - **Requirements**:
        - Add `piper-rs` crate for embedded VITS/ONNX inference.
        - Default voice: `en_US-ryan-high` (male, clear, assistant-appropriate, 22.05 kHz, ~60 MB).
        - Support quality tiers: `x_low` (16 kHz, ~15 MB), `low`, `medium`, `high` (recommended).
        - Alternative voices: `en_US-amy-medium` (female), `en_US-lessac-high` (expressive), `en_GB-alan-medium` (British).
        - Implement `TtsProvider` trait with `synthesize(&self, text) -> Result<AudioBuffer>`.
        - Fallback: HTTP sidecar mode targeting `POST /v1/audio/speech` (OpenAI-compatible TTS-API-Server).
        - Sentence-boundary streaming for low-latency playback on longer responses.

3. **Audio Pipeline**
    - **Requirements**:
        - Wire STT output into the existing `ActionService` as a text input source.
        - Wire AI response text into TTS for audio output generation.
        - Full flow: Microphone → `whisper-rs` → Text → AI Pipeline → Response Text → `piper-rs` → Speaker.
        - Support audio-only mode (voice in, voice out) and hybrid mode (voice in, text out).
        - Configurable pipeline via `[voice]` config section: `stt_enabled`, `tts_enabled`, `stt_model`, `tts_voice`.

4. **Model Management**
    - **Requirements**:
        - `voice models list` — show available whisper and piper models with size and quality info.
        - `voice models pull <MODEL>` — download model to local cache directory.
        - `voice models remove <MODEL>` — delete cached model.
        - Store models in `~/.thunderbot/models/` with separate `whisper/` and `piper/` subdirectories.
        - Verify model checksums after download.

5. **CLI: Voice Commands**
    - **Requirements**:
        - `voice transcribe <FILE>` — transcribe an audio file to text.
        - `voice speak <TEXT>` — synthesize text to audio and play or save to file.
        - `voice chat` — interactive voice conversation loop (record → transcribe → AI → speak).
        - `voice status` — show loaded models, hardware acceleration status, and latency benchmarks.

6. **Dashboard Voice Controls**
    - **Requirements**:
        - Audio player widget for listening to TTS output of bot responses.
        - Voice activity indicator showing STT processing state.
        - Model selection dropdown for switching whisper/piper models.
        - Latency display for STT and TTS processing times.

## Milestone 13: Deployment/Self-Hosting

**Goal**: Make Thunderbot easy to run locally or in containers for personal deployments.

**Definition of Done**:

1. Bot runs reliably as a local process with systemd/launchd
2. Docker container builds and runs correctly
3. Data persists across restarts
4. Logs are structured and actionable
5. Documentation covers all deployment scenarios

**Tasks**:

1. **Local Process Support**
    - Add `--address` and `--port` flags to `serve` command
    - Support `.env` file loading at startup
    - Document systemd unit file for Linux
    - Document launchd plist for macOS

2. **Containerization**
    - Create multi-stage `Dockerfile` for minimal image size
    - Create `docker-compose.yml` with volume mounts
    - Support both `x86_64` and `aarch64` architectures
    - Add `/health` endpoint for container orchestration

3. **Structured Logging**
    - Configure `tracing` for JSON output
    - Add request correlation IDs
    - Log Jetstream events, GLM-5 calls, Bluesky posts with context

4. **Error Recovery**
    - Wrap event loop with error handling and reconnection
    - Exponential backoff for transient failures
    - Health check returning connection status

5. **CLI Operations**
    - `serve --address 0.0.0.0` for container binding
    - `serve --dry-run` to process events without posting
    - `status` command showing service health

6. **Documentation**
    - Deployment guide: local, Docker, Fly.io
    - Backup and restore procedures
    - Troubleshooting section

## Parking Lot

- Whitelist for who can communicate with bots (mutuals?)
- Administrator accounts
- Use `icondata` crate for SVGs in dashboard
