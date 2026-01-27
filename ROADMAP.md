# Stateful Agent Bot Specification

This document outlines the remaining engineering work for a **Stateful AI Agent** on the AT Protocol (Bluesky), utilizing **Gemini 3** for reasoning, **Rust** for infrastructure, and **HTMX + Pico CSS** for the management dashboard.

For completed milestones, see CHANGELOG.md.

## System Architecture

- **Runtime**: Provider-agnostic Rust binary. Deployable locally, in Docker, or any container runtime.
- **Ingestion**: Manual WebSocket client consuming Jetstream via `tokio-tungstenite` with zstd decompression.
- **State Store**: libSQL/SQLite (embedded or Turso cloud). Portable, single-file database with replication support.
- **Vector Store**: SQLite with sqlite-vec extension for unified persistence.
- **Cognitive Engine**: Google Gemini 3 (Pro/Thinking) via REST API.
- **Bluesky Integration**: Manual XRPC client via `reqwest` for full control over authentication and posting.
- **Frontend**: Server-side rendered HTML (HTMX) with Pico CSS, served directly from the application.
- **CLI**: `clap`-based command-line interface for ad-hoc testing and system interaction.

## Milestone 10: Dashboard Enhancements

**Goal**: Make the dashboard more powerful for day-to-day bot management.

**Definition of Done**:

1. Users can search and filter conversations
2. Data can be exported for analysis
3. Keyboard navigation works throughout
4. Dark mode available

### Tasks

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

### Tasks

1. **Rate Limit Dashboard**
    - Show current Bluesky rate limit usage and reset time
    - Show Gemini API quota consumption
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

## Milestone 12: Self-Hosting

**Goal**: Make ThunderBot easy to run locally or in containers for personal deployments.

**Definition of Done**:

1. Bot runs reliably as a local process with systemd/launchd
2. Docker container builds and runs correctly
3. Data persists across restarts
4. Logs are structured and actionable
5. Documentation covers all deployment scenarios

See `docs/web-dashboard.md` for environment variable reference.

### Tasks

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
    - Log Jetstream events, Gemini calls, Bluesky posts with context

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

### Dashboard

1. **Real-time Updates:** Use WebSockets for live activity feed.
2. **Bulk Operations:** Add bulk export and archive functionality.
3. **Activity Coverage:** Extend logging to include Bluesky API calls, rate limit hits, and retry attempts.
4. **Export Formats:** XML, Excel formats.
5. **Pagination:** Add pagination to search and activity timeline for large result sets.
6. **Filter UX:** Sliders, toggles, and visual feedback.
7. **Search Relevance:** Better ranking algorithm with BM25.
