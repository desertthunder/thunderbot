# Stateful Agent Bot Specification

This document outlines the remaining engineering work for a **Stateful AI Agent** on the AT Protocol (Bluesky), utilizing **Gemini 3** for reasoning, **Rust** for infrastructure, and **HTMX + Pico CSS** for the management dashboard.

For completed milestones, see CHANGELOG.md.

## System Architecture

- **Runtime**: Provider-agnostic Rust binary. Deployable to Fly.io (containers), Docker, or any container runtime.
- **Ingestion**: Manual WebSocket client consuming Jetstream via `tokio-tungstenite` with zstd decompression.
- **State Store**: libSQL/SQLite (embedded or Turso cloud). Portable, single-file database with replication support.
- **Vector Store**: SQLite with sqlite-vec extension for unified persistence.
- **Cognitive Engine**: Google Gemini 3 (Pro/Thinking) via REST API.
- **Bluesky Integration**: Manual XRPC client via `reqwest` for full control over authentication and posting.
- **Frontend**: Server-side rendered HTML (HTMX) with Pico CSS, served directly from the application.
- **CLI**: `clap`-based command-line interface for ad-hoc testing and system interaction.

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
