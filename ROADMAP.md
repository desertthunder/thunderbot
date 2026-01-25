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

## Milestone 8: Self-Hosting

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
